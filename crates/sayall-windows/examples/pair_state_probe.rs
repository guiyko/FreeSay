//! 配对状态只读探针（2026-11-14「重配对后 STILL 返回状态 3」现场用）。
//!
//! 回答三个问题，全部只读、不写 CCCD、不改配对：
//!   1) 系统认为这支遥控器**已配对**吗？（`IsPaired` / `CanPair`）
//!   2) 系统对它的**保护级别**要求是什么？（`ProtectionLevel`：None / Encryption / EncryptionAndAuthentication）
//!   3) 现在是否有**其他客户端持有 GATT 会话**？（`GattSession.SessionStatus`）
//!
//! 判定逻辑（配合 atvv_char_probe 的 AccessDenied）：
//!   - `IsPaired=false`            → 配对根本没成，重配动作本身失败；
//!   - `IsPaired=true` + 特征被拒  → 绑定了，但链路没加密/密钥对不上（残留记录或设备端旧密钥）；
//!   - `ProtectionLevel` 要求 Authentication → Just-Works 配对无法满足，需要 MITM 配对。
//!
//! 用法：cargo run --release -p sayall-windows --example pair_state_probe

use std::future::IntoFuture;

use windows::core::HSTRING;
use windows::Devices::Bluetooth::{BluetoothConnectionStatus, BluetoothLEDevice};
use windows::Devices::Enumeration::DeviceInformation;
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

fn main() -> windows::core::Result<()> {
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
    }

    let selector = BluetoothLEDevice::GetDeviceSelectorFromPairingState(true)?;
    let devices = futures::executor::block_on(
        DeviceInformation::FindAllAsyncAqsFilter(&selector)?.into_future(),
    )?;
    println!("已配对 BLE 设备数 = {}", devices.Size()?);

    for item in &devices {
        let name = item.Name().map(|n| n.to_string()).unwrap_or_default();
        let id = item.Id().map(|v| v.to_string()).unwrap_or_default();
        // 只关心遥控器，耳机等跳过噪音（但仍打印一行便于对照）。
        println!("\n=== {name}");

        match item.Pairing() {
            Ok(pairing) => {
                let is_paired = pairing.IsPaired().unwrap_or(false);
                let can_pair = pairing.CanPair().unwrap_or(false);
                let protection = pairing
                    .ProtectionLevel()
                    .map(|level| format!("{level:?}"))
                    .unwrap_or_else(|error| format!("读取失败({error})"));
                println!("    IsPaired={is_paired}  CanPair={can_pair}");
                println!("    ProtectionLevel={protection}");
            }
            Err(error) => println!("    Pairing 读取失败：{error}"),
        }

        let device = match futures::executor::block_on(
            BluetoothLEDevice::FromIdAsync(&HSTRING::from(id.clone()))?.into_future(),
        ) {
            Ok(device) => device,
            Err(error) => {
                println!("    FromIdAsync 失败：{error}");
                continue;
            }
        };

        let connection = device
            .ConnectionStatus()
            .map(|status| match status {
                BluetoothConnectionStatus::Connected => "Connected(1)".to_owned(),
                BluetoothConnectionStatus::Disconnected => "Disconnected(0)".to_owned(),
                other => format!("{other:?}"),
            })
            .unwrap_or_else(|error| format!("读取失败({error})"));
        println!("    ConnectionStatus={connection}");
    }

    Ok(())
}
