//! 组合键注入探针：用应用同一条注入路径按下一个和弦，供「输入法到底认不认」实测。
//!
//! 背景（2026-11-11 现场）：向导里把快捷键设成 `left_control+m` 后，按下遥控器
//! 语音键，Chatterfly 完全没有反应。已证明注入事件进了 OS 键盘链（见
//! `voice_hotkey_delivery_probe`），剩下两种可能：①输入法过滤带 LLKHF_INJECTED
//! 标记的键；②和弦本身不对（Ctrl 左右、时序）。
//!
//! 用法（先让目标窗口拿到焦点，再运行）：
//!   cargo run --release -p sayall-windows --example chord_inject_probe -- left_control,m --hold 400
//!
//! 输出注入前后自己的钩子看到的事件；配合手动观察目标程序是否有反应。

#[cfg(not(windows))]
fn main() {
    println!("{{\"kind\":\"chord_inject_probe\",\"supported\":false}}");
}

#[cfg(windows)]
fn main() {
    windows_probe::run();
}

#[cfg(windows)]
mod windows_probe {
    use std::collections::BTreeSet;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{mpsc, Mutex};
    use std::time::Duration;
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, EnumWindows, GetClassNameW, GetMessageW, GetWindowTextW,
        GetWindowTextLengthW, GetWindowThreadProcessId, IsWindowVisible, PeekMessageW,
        SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, MSG,
        PM_NOREMOVE, WH_KEYBOARD_LL, WM_APP,
    };

    use sayall_windows::send_input::{KeyChord, KeyCode};
    use sayall_windows::send_input_windows::SendInputRuntime;

    const WM_QUIT_PROBE: u32 = WM_APP + 0x92;

    static PROBE_THREAD_ID: AtomicU64 = AtomicU64::new(0);
    static EVENTS: Mutex<Vec<(u32, u32, u32)>> = Mutex::new(Vec::new());
    static WINDOWS: Mutex<Vec<(u32, String, String)>> = Mutex::new(Vec::new());

    unsafe extern "system" fn enum_window(hwnd: HWND, _lparam: LPARAM) -> BOOL {
        if IsWindowVisible(hwnd).as_bool() {
            let mut pid = 0u32;
            let _ = GetWindowThreadProcessId(hwnd, Some(&mut pid));
            let mut title_buf = [0u16; 512];
            let title_len = if GetWindowTextLengthW(hwnd) > 0 {
                GetWindowTextW(hwnd, &mut title_buf) as usize
            } else {
                0
            };
            let mut class_buf = [0u16; 256];
            let class_len = GetClassNameW(hwnd, &mut class_buf) as usize;
            let title = String::from_utf16_lossy(&title_buf[..title_len]);
            let class = String::from_utf16_lossy(&class_buf[..class_len]);
            if let Ok(mut windows) = WINDOWS.lock() {
                windows.push((pid, class, title));
            }
        }
        BOOL(1)
    }

    fn snapshot() -> BTreeSet<String> {
        if let Ok(mut windows) = WINDOWS.lock() {
            windows.clear();
        }
        unsafe {
            let _ = EnumWindows(Some(enum_window), LPARAM(0));
        }
        let windows = WINDOWS.lock().map(|w| w.clone()).unwrap_or_default();
        windows
            .into_iter()
            .map(|(pid, class, title)| format!("{pid}|{class}|{title}"))
            .collect()
    }

    fn process_name(pid: u32) -> String {
        if pid == 0 {
            return "system".to_owned();
        }
        unsafe {
            let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
                return format!("pid={pid}");
            };
            let mut buffer = [0u16; 1024];
            let mut size = buffer.len() as u32;
            let ok = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_FORMAT(0),
                windows::core::PWSTR(buffer.as_mut_ptr()),
                &mut size,
            );
            let _ = windows::Win32::Foundation::CloseHandle(handle);
            if ok.is_err() {
                return format!("pid={pid}");
            }
            String::from_utf16_lossy(&buffer[..size as usize])
        }
    }

    fn report_new(baseline: &BTreeSet<String>, label: &str) {
        let current = snapshot();
        let mut added: Vec<&String> = current.difference(baseline).collect();
        added.sort();
        println!("[chord] {label}: 新增可见窗口 {}", added.len());
        for entry in added {
            let pid = entry.split('|').next().and_then(|value| value.parse::<u32>().ok());
            let tail: Vec<&str> = entry.splitn(3, '|').collect();
            let image = pid.map(process_name).unwrap_or_default();
            let class = tail.get(1).copied().unwrap_or("");
            let title = tail.get(2).copied().unwrap_or("");
            println!("[chord]   + class={class} title={title:?} image={image}");
        }
    }

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

    fn parse_key(token: &str) -> Option<KeyCode> {
        let normalized = token.trim().to_ascii_lowercase();
        let key = match normalized.as_str() {
            "left_control" | "leftcontrol" | "lctrl" => KeyCode::LeftControl,
            "right_control" | "rightcontrol" | "rctrl" => KeyCode::RightControl,
            "left_alt" | "leftalt" => KeyCode::LeftAlt,
            "right_alt" | "rightalt" => KeyCode::RightAlt,
            "left_shift" | "leftshift" => KeyCode::LeftShift,
            "right_shift" | "rightshift" => KeyCode::RightShift,
            "space" => KeyCode::Space,
            "enter" => KeyCode::Enter,
            "tab" => KeyCode::Tab,
            "a" => KeyCode::A,
            "b" => KeyCode::B,
            "c" => KeyCode::C,
            "d" => KeyCode::D,
            "e" => KeyCode::E,
            "f" => KeyCode::F,
            "g" => KeyCode::G,
            "h" => KeyCode::H,
            "i" => KeyCode::I,
            "j" => KeyCode::J,
            "k" => KeyCode::K,
            "l" => KeyCode::L,
            "m" => KeyCode::M,
            "n" => KeyCode::N,
            "o" => KeyCode::O,
            "p" => KeyCode::P,
            "q" => KeyCode::Q,
            "r" => KeyCode::R,
            "s" => KeyCode::S,
            "t" => KeyCode::T,
            "u" => KeyCode::U,
            "v" => KeyCode::V,
            "w" => KeyCode::W,
            "x" => KeyCode::X,
            "y" => KeyCode::Y,
            "z" => KeyCode::Z,
            _ => return None,
        };
        Some(key)
    }

    pub fn run() {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let spec = args
            .iter()
            .find(|value| !value.starts_with("--") && value.contains(','))
            .cloned()
            .unwrap_or_else(|| "left_control,m".to_owned());
        let hold_ms: u64 = args
            .iter()
            .skip_while(|value| *value != "--hold")
            .nth(1)
            .and_then(|value| value.parse().ok())
            .unwrap_or(400);

        let keys: Vec<KeyCode> = spec.split(',').filter_map(parse_key).collect();
        if keys.is_empty() {
            println!("[chord] 无法解析组合键：{spec}");
            return;
        }

        let (tx, rx) = mpsc::channel::<u32>();
        let worker = std::thread::spawn(move || unsafe {
            let mut msg = MSG::default();
            let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
            let hook: Option<HHOOK> =
                SetWindowsHookExW(WH_KEYBOARD_LL, Some(record_hook), None, 0).ok();
            let _ = tx.send(windows::Win32::System::Threading::GetCurrentThreadId());
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                if msg.message == WM_QUIT_PROBE {
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
        let chord = KeyChord { keys };
        let baseline = snapshot();
        let press = runtime.press(&chord);
        println!("[chord] spec={spec} hold_ms={hold_ms} press_ok={}", press.is_ok());
        std::thread::sleep(Duration::from_millis(180));
        report_new(&baseline, "按住 180ms");
        std::thread::sleep(Duration::from_millis(hold_ms.saturating_sub(180)));
        report_new(&baseline, "按住结束前");
        let release = runtime.release(&chord);
        std::thread::sleep(Duration::from_millis(400));
        println!("[chord] release_ok={}", release.is_ok());
        report_new(&baseline, "松开 400ms 后");

        let events = EVENTS.lock().map(|e| e.clone()).unwrap_or_default();
        println!("[chord] events={}", events.len());
        for (vk, scan, flags) in &events {
            println!(
                "[chord]   vk=0x{vk:02X} scan=0x{scan:02X} flags=0x{flags:02X} injected={} extended={} up={}",
                flags & 0x10 != 0,
                flags & 0x01 != 0,
                flags & 0x80 != 0,
            );
        }
        println!("[chord] done");

        post_quit();
        let _ = worker.join();
    }
}
