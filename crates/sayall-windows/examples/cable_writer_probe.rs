//! 虚拟声卡「谁在写」探针：空闲时 CABLE Output 上持续有 -19 dBFS 的声音，
//! 枚举 CABLE 渲染端点上的所有音频会话，把每个写入进程的 PID / 镜像路径 /
//! 会话状态 / 端点峰值全部打出来。
//!
//! 背景（2026-11-11 现场）：用户没按键也没说话，Chatterfly 的麦克风检测条却是满的。
//! `cable_idle_level_probe` 实测 CABLE Output 空闲时有 -19.6 dBFS RMS 的持续信号，
//! 说明有进程一直在往 CABLE Input 渲染（而非静音、也非残留直流）。
//!
//! 用法：
//!   cargo run --release -p sayall-windows --example cable_writer_probe

use windows::core::{Interface, HSTRING};
use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
use windows::Win32::Media::Audio::{
    IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
};
use wasapi::{DeviceEnumerator, Direction};

fn process_name(pid: u32) -> String {
    if pid == 0 {
        return "system".to_owned();
    }
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return format!("pid={pid} (打不开，可能已退出或权限不足)");
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
            return format!("pid={pid} (读不到路径)");
        }
        String::from_utf16_lossy(&buffer[..size as usize])
    }
}

fn main() {
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };

    let enumerator = match DeviceEnumerator::new() {
        Ok(value) => value,
        Err(error) => {
            println!("DeviceEnumerator::new failed: {error}");
            return;
        }
    };
    let mut targets: Vec<(String, String)> = Vec::new();
    if let Ok(collection) = enumerator.get_device_collection(&Direction::Render) {
        for device in &collection {
            let Ok(device) = device else { continue };
            let (Ok(id), Ok(name)) = (device.get_id(), device.get_friendlyname()) else {
                continue;
            };
            if name.contains("VB-Audio") || name.contains("CABLE") {
                targets.push((id, name));
            }
        }
    }
    if targets.is_empty() {
        println!("[writer] 没有找到 VB-Audio 渲染端点");
        return;
    }

    for (endpoint_id, name) in targets {
        println!("[writer] === render endpoint: {name}");
        unsafe {
            let com: IMMDeviceEnumerator =
                match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
                    Ok(value) => value,
                    Err(error) => {
                        println!("[writer] 创建枚举器失败：{error}");
                        continue;
                    }
                };
            let device = match com.GetDevice(&HSTRING::from(endpoint_id.clone())) {
                Ok(value) => value,
                Err(error) => {
                    println!("[writer] 取端点失败：{error}");
                    continue;
                }
            };
            if let Ok(meter) = device.Activate::<IAudioMeterInformation>(CLSCTX_ALL, None) {
                let peak = meter.GetPeakValue().unwrap_or(-1.0);
                println!("[writer] endpoint_peak={peak:.4}");
            }
            let manager: IAudioSessionManager2 = match device.Activate(CLSCTX_ALL, None) {
                Ok(value) => value,
                Err(error) => {
                    println!("[writer] 取会话管理器失败：{error}");
                    continue;
                }
            };
            let Ok(sessions) = manager.GetSessionEnumerator() else {
                println!("[writer] 取会话枚举失败");
                continue;
            };
            let count = sessions.GetCount().unwrap_or(0);
            println!("[writer] sessions={count}");
            for index in 0..count {
                let Ok(control) = sessions.GetSession(index) else {
                    continue;
                };
                let control2: IAudioSessionControl2 = match control.cast() {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                let pid = control2.GetProcessId().unwrap_or(0);
                let state = control.GetState().map(|value| format!("{value:?}")).unwrap_or_default();
                let peak = control
                    .cast::<IAudioMeterInformation>()
                    .and_then(|meter| meter.GetPeakValue())
                    .unwrap_or(-1.0);
                println!(
                    "[writer]   pid={pid} peak={peak:.4} state={state} image={}",
                    process_name(pid)
                );
            }
        }
    }
}
