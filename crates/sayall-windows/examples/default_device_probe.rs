//! 默认设备探针：系统默认播放 / 录音端点分别是谁。
//!
//! 背景（2026-11-11 现场）：空闲时 CABLE Output 上持续有 -19.6 dBFS 的信号，
//! `cable_writer_probe` 查到 MusicFree 在往 "CABLE Input" 渲染（峰值 0.94）。
//! 需要确认这是"系统默认播放被设成了虚拟声卡"还是"播放器自己选了虚拟声卡"。

use wasapi::{DeviceEnumerator, Direction};

fn main() {
    let _ = wasapi::initialize_mta();
    let enumerator = match DeviceEnumerator::new() {
        Ok(value) => value,
        Err(error) => {
            println!("[default] 创建枚举器失败：{error}");
            return;
        }
    };
    for (label, direction) in [
        ("render", Direction::Render),
        ("capture", Direction::Capture),
    ] {
        match enumerator.get_default_device(&direction) {
            Ok(device) => {
                let name = device.get_friendlyname().unwrap_or_default();
                let id = device.get_id().unwrap_or_default();
                println!("[default] {label} = {name}");
                println!("[default]   id = {id}");
            }
            Err(error) => println!("[default] {label} 取默认设备失败：{error}"),
        }
    }
}
