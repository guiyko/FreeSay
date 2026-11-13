//! 修饰键卡死探针：读 `GetAsyncKeyState` / `GetKeyState` 判断有没有修饰键
//! 在 OS 层被钉在按下态。
//!
//! 背景（2026-11-11 现场）：用户报告「物理按 Ctrl+M 能激活语音，遥控器注入
//! Ctrl+M 却把输入窗口最小化」。两者唯一的系统性差异是注入路径。若此前的
//! `Ctrl+Win`（向导 revert_voice_hotkey）有 DOWN 沿泄漏而未配对 UP，OS 会
//! 认为 Win 一直按住——此后任何注入的 `M` 在系统看来都是 `Win+M`（最小化
//! 全部窗口），而物理按下的 Ctrl+M 因为真实 Win 键抬起过，不受影响。
//!
//! 用法：
//!   cargo run --release -p sayall-windows --example modifier_state_probe
//!   ... --watch 10      # 连续采样 10 秒，抓「按下的那一刻」

#[cfg(not(windows))]
fn main() {
    println!("{{\"kind\":\"modifier_state_probe\",\"supported\":false}}");
}

#[cfg(windows)]
fn main() {
    windows_probe::run();
}

#[cfg(windows)]
mod windows_probe {
    use std::time::Duration;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, GetKeyState, VK_CAPITAL, VK_LALT, VK_LCONTROL, VK_LMENU, VK_LSHIFT,
        VK_LWIN, VK_NUMLOCK, VK_RALT, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SCROLL,
    };

    struct Probe {
        label: &'static str,
        async_vk: i32,
        toggle_vk: i32,
    }

    const MODIFIERS: &[Probe] = &[
        Probe { label: "LControl", async_vk: VK_LCONTROL.0 as i32, toggle_vk: 0 },
        Probe { label: "RControl", async_vk: VK_RCONTROL.0 as i32, toggle_vk: 0 },
        Probe { label: "LShift", async_vk: VK_LSHIFT.0 as i32, toggle_vk: 0 },
        Probe { label: "RShift", async_vk: VK_RSHIFT.0 as i32, toggle_vk: 0 },
        Probe { label: "LAlt", async_vk: VK_LMENU.0 as i32, toggle_vk: 0 },
        Probe { label: "RAlt", async_vk: VK_RMENU.0 as i32, toggle_vk: 0 },
        // 别名：VK_MENU / VK_CONTROL 是通用键，也要看，注入可能只动了通用键
        Probe { label: "LAlt(alias)", async_vk: VK_LALT.0 as i32, toggle_vk: 0 },
        Probe { label: "RAlt(alias)", async_vk: VK_RALT.0 as i32, toggle_vk: 0 },
        Probe { label: "LWin", async_vk: VK_LWIN.0 as i32, toggle_vk: 0 },
        Probe { label: "RWin", async_vk: VK_RWIN.0 as i32, toggle_vk: 0 },
        // 锁定键：卡住会让后续按键改变语义，一并记录
        Probe { label: "CapsLock", async_vk: VK_CAPITAL.0 as i32, toggle_vk: VK_CAPITAL.0 as i32 },
        Probe { label: "NumLock", async_vk: VK_NUMLOCK.0 as i32, toggle_vk: VK_NUMLOCK.0 as i32 },
        Probe { label: "ScrollLock", async_vk: VK_SCROLL.0 as i32, toggle_vk: VK_SCROLL.0 as i32 },
    ];

    fn sample(label: &str) {
        println!("[probe] ---- {label} ----");
        let mut stuck = Vec::new();
        for entry in MODIFIERS {
            let async_state = unsafe { GetAsyncKeyState(entry.async_vk) };
            let down = (async_state as u16 & 0x8000) != 0;
            let recently = (async_state as u16 & 0x0001) != 0;
            let toggled = if entry.toggle_vk != 0 {
                (unsafe { GetKeyState(entry.toggle_vk) } as u16 & 0x0001) != 0
            } else {
                false
            };
            println!(
                "[probe]   {:<12} down={:<5} pressed_since_last={:<5} toggled={:<5} raw=0x{:04X}",
                entry.label, down, recently, toggled, async_state as u16
            );
            if down && entry.toggle_vk == 0 {
                stuck.push(entry.label);
            }
        }
        if stuck.is_empty() {
            println!("[probe] verdict: no modifier stuck down");
        } else {
            println!("[probe] verdict: STUCK DOWN -> {}", stuck.join(", "));
            if stuck.iter().any(|name| name.contains("Win")) {
                println!(
                    "[probe] note: Win 卡住时，任何后续注入的普通键都会被系统当作 Win+<key>；\
                     M=最小化全部窗口、D=显示桌面、L=锁屏"
                );
            }
        }
    }

    pub fn run() {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let watch_secs: u64 = args
            .iter()
            .skip_while(|value| *value != "--watch")
            .nth(1)
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);

        println!("[probe] modifier_state_probe watch_secs={watch_secs}");
        sample("now");
        if watch_secs == 0 {
            return;
        }
        // 连续采样：用户在这段时间里按一次遥控器语音键，就能看到按下前后
        // （以及松手后）有没有修饰键被留在按下态。
        let mut elapsed = 0_u64;
        let mut previous: Vec<bool> = Vec::new();
        while elapsed < watch_secs * 1000 {
            std::thread::sleep(Duration::from_millis(100));
            elapsed += 100;
            let current: Vec<bool> = MODIFIERS
                .iter()
                .map(|entry| (unsafe { GetAsyncKeyState(entry.async_vk) } as u16 & 0x8000) != 0)
                .collect();
            if current != previous {
                if !previous.is_empty() {
                    let changed: Vec<String> = MODIFIERS
                        .iter()
                        .zip(current.iter())
                        .zip(previous.iter())
                        .filter(|((_, now), before)| now != before)
                        .map(|((entry, now), _)| format!("{}={}", entry.label, now))
                        .collect();
                    println!("[probe] t={elapsed}ms change {}", changed.join(" "));
                }
                previous = current;
            }
        }
        sample("after watch");
    }
}
