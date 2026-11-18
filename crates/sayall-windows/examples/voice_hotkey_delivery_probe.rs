//! 一次性诊断探针：本应用注入的「按住说话」右 Alt 是否真的进了 OS 键盘队列？
//!
//! 背景（2026-11-11 现场）：遥控器语音键按下后，应用侧 `chord_press result=ok`、
//! 音频也进了虚拟声卡，但用户电脑上的输入法完全没反应，看起来像"系统没收到
//! 右 Alt"。`chord_press=ok` 只证明 SendInput 接受了这批事件，不证明事件被
//! 系统翻译成了可见的按键——所以这里用**自己的 LL 键盘钩子**去看真实到达形态。
//!
//! 方法：装 WH_KEYBOARD_LL 记录每个事件的 vk / scan / flags（含 LLKHF_INJECTED
//! 位）→ 调应用同一条注入路径 `SendInputRuntime::press/release`（右 Alt 单键
//! 和弦）→ 按住期间读 GetAsyncKeyState(VK_RMENU) → 释放后再读。
//!
//! 判定：
//!   saw=2 且 vk=0xA5 scan=0x38 extended=1 injected=1 ⇒ 注入与物理键同形，系统确实收到
//!   saw=0 ⇒ 注入被吞/未入队（钩子链或完整性层拦截）

#[cfg(not(windows))]
fn main() {
    println!("{{\"kind\":\"voice_hotkey_delivery_probe\",\"supported\":false}}");
}

#[cfg(windows)]
fn main() {
    windows_probe::run();
}

#[cfg(windows)]
mod windows_probe {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{mpsc, Mutex};
    use std::time::Duration;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VIRTUAL_KEY, VK_RMENU};
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, PeekMessageW, SetWindowsHookExW,
        TranslateMessage, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, MSG, PM_NOREMOVE,
        WH_KEYBOARD_LL, WM_APP, WM_QUIT,
    };

    use sayall_windows::send_input::{KeyChord, KeyCode};
    use sayall_windows::send_input_windows::SendInputRuntime;

    const WM_QUIT_PROBE: u32 = WM_APP + 0x91;

    static PROBE_THREAD_ID: AtomicU64 = AtomicU64::new(0);
    static EVENTS: Mutex<Vec<(u32, u32, u32)>> = Mutex::new(Vec::new());

    unsafe extern "system" fn record_hook(
        code: i32,
        wparam: windows::Win32::Foundation::WPARAM,
        lparam: windows::Win32::Foundation::LPARAM,
    ) -> windows::Win32::Foundation::LRESULT {
        if code >= 0 {
            let message = wparam.0 as u32;
            if matches!(message, 0x0100 | 0x0104 | 0x0101 | 0x0105) {
                let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
                if let Ok(mut events) = EVENTS.lock() {
                    events.push((kb.vkCode, kb.scanCode, kb.flags.0));
                }
            }
        }
        CallNextHookEx(None, code, wparam, lparam)
    }

    fn post_quit() {
        let tid = PROBE_THREAD_ID.load(Ordering::Relaxed) as u32;
        if tid != 0 {
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW(
                    tid,
                    WM_QUIT_PROBE,
                    windows::Win32::Foundation::WPARAM(0),
                    windows::Win32::Foundation::LPARAM(0),
                );
            }
        }
    }

    fn right_alt_down() -> bool {
        unsafe { GetAsyncKeyState(i32::from(VK_RMENU.0)) < 0 }
    }

    pub fn run() {
        let (tx, rx) = mpsc::channel::<u32>();
        let worker = std::thread::spawn(move || unsafe {
            let mut msg = MSG::default();
            let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
            let hook: Option<HHOOK> =
                SetWindowsHookExW(WH_KEYBOARD_LL, Some(record_hook), None, 0).ok();
            let _ = tx.send(windows::Win32::System::Threading::GetCurrentThreadId());
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                if msg.message == WM_QUIT || msg.message == WM_QUIT_PROBE {
                    break;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            if let Some(hook) = hook {
                let _ = UnhookWindowsHookEx(hook);
            }
        });
        let thread_id = rx.recv().expect("probe 线程未启动");
        PROBE_THREAD_ID.store(thread_id as u64, Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(150));

        if let Ok(mut events) = EVENTS.lock() {
            events.clear();
        }

        let runtime = SendInputRuntime::new();
        let chord = KeyChord {
            keys: vec![KeyCode::RightAlt],
        };
        let press = runtime.press(&chord);
        std::thread::sleep(Duration::from_millis(120));
        let down_visible = right_alt_down();
        let release = runtime.release(&chord);
        std::thread::sleep(Duration::from_millis(120));
        let up_visible = !right_alt_down();

        let events = EVENTS.lock().map(|e| e.clone()).unwrap_or_default();
        println!(
            "[probe] press_ok={} release_ok={} events={}",
            press.is_ok(),
            release.is_ok(),
            events.len()
        );
        for (vk, scan, flags) in &events {
            println!(
                "[probe]   vk=0x{vk:02X} scan=0x{scan:02X} flags=0x{flags:02X} injected={} extended={} up={}",
                flags & 0x10 != 0,
                flags & 0x01 != 0,
                flags & 0x80 != 0,
            );
        }
        println!("[probe] os_key_state down_visible={down_visible} up_visible={up_visible}");

        let saw_right_alt = events
            .iter()
            .any(|(vk, scan, _)| *vk == 0xA5 && *scan == 0x38);
        let verdict = if events.is_empty() {
            "NO_EVENTS: 注入的事件没有到达键盘钩子链（被吞或未入队）"
        } else if saw_right_alt {
            "DELIVERED: 系统钩子收到右 Alt（vk=0xA5 scan=0x38），注入与物理键同形"
        } else {
            "MISMATCH: 收到事件但不是右 Alt（扫描码/虚拟键被翻译成别的键）"
        };
        println!("[probe] verdict: {verdict}");

        post_quit();
        let _ = worker.join();
    }
}
