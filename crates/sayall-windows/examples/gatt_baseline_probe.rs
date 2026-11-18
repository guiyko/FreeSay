//! GATT 基线探针（2026-11-14「重配对后 ATVV 仍状态 3」现场用）。
//!
//! 目的：把「整台设备的 GATT 都不可访问」与「只有 ATVV 服务被拒」区分开。
//!   - 若 GAP(0x1800) / Battery(0x180F) 的特征也 AccessDenied ⇒ 设备级/系统级权限问题；
//!   - 若它们 Success、只有 ATVV 被拒 ⇒ 服务级问题（该服务要求更强的安全等级，如 MITM）。
//!
//! 只读：不订阅、不写 CCCD、不配对。

use std::future::IntoFuture;

use windows::core::{GUID, HSTRING};
use windows::Devices::Bluetooth::BluetoothLEDevice;
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

const ADDR: u64 = 0x14BEFC9CD7D9;

const GAP_SERVICE: GUID = GUID::from_u128(0x00001800_0000_1000_8000_00805f9b34fb);
const DEVICE_NAME: GUID = GUID::from_u128(0x00002a00_0000_1000_8000_00805f9b34fb);
const BATTERY_SERVICE: GUID = GUID::from_u128(0x0000180f_0000_1000_8000_00805f9b34fb);
const BATTERY_LEVEL: GUID = GUID::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);
const ATVV_SERVICE: GUID = GUID::from_u128(0xab5e0001_5a21_4f05_bc7d_af01f617b664);

fn main() -> windows::core::Result<()> {
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
    }

    let device = futures::executor::block_on(
        BluetoothLEDevice::FromBluetoothAddressAsync(ADDR)?.into_future(),
    )?;
    println!(
        "设备: {:?}  status={:?}",
        device.Name(),
        device.ConnectionStatus()
    );

    // 1) 全表枚举服务：这一步若失败，说明连服务列表都拿不到（设备级）。
    match futures::executor::block_on(device.GetGattServicesAsync()?.into_future()) {
        Ok(result) => {
            println!(
                "\n[1] GetGattServicesAsync status={:?} count={}",
                result.Status(),
                result.Services()?.Size()?
            );
            if let Ok(services) = result.Services() {
                for service in &services {
                    let uuid = service.Uuid().map(|u| format!("{u:?}")).unwrap_or_default();
                    println!("    service {uuid}");
                }
            }
        }
        Err(error) => println!("\n[1] GetGattServicesAsync 失败：{error}"),
    }

    // 2) 逐个服务查特征：GAP / Battery / ATVV 三条对照。
    for (label, uuid, char_uuid) in [
        ("GAP(0x1800)", GAP_SERVICE, Some(DEVICE_NAME)),
        ("Battery(0x180F)", BATTERY_SERVICE, Some(BATTERY_LEVEL)),
        ("ATVV(ab5e0001)", ATVV_SERVICE, None),
    ] {
        println!("\n[2] {label}");
        let services = match futures::executor::block_on(
            device.GetGattServicesForUuidAsync(uuid)?.into_future(),
        ) {
            Ok(result) => {
                println!("    服务发现 status={:?}", result.Status());
                result.Services()
            }
            Err(error) => {
                println!("    服务发现失败：{error}");
                continue;
            }
        };
        let Ok(services) = services else {
            println!("    Services 读取失败");
            continue;
        };
        if services.Size().unwrap_or(0) == 0 {
            println!("    服务数 0（设备没有这个服务）");
            continue;
        }
        for service in &services {
            // 2026-11-15：计时是区分「本机内核直接拒」与「空中 ATT/SMP 交互后被拒」的关键判据
            // （gemini-4-carbon 建议）：<2ms 说明没走空口；30ms~500ms 说明真的协商过。
            let started = std::time::Instant::now();
            match futures::executor::block_on(service.GetCharacteristicsAsync()?.into_future()) {
                Ok(result) => {
                    println!(
                        "    整表枚举 status={:?} count={} elapsed={}ms",
                        result.Status(),
                        result
                            .Characteristics()
                            .map(|c| c.Size().unwrap_or(0))
                            .unwrap_or(0),
                        started.elapsed().as_millis()
                    );
                }
                Err(error) => println!(
                    "    整表枚举失败：{error} elapsed={}ms",
                    started.elapsed().as_millis()
                ),
            }
            if let Some(char_uuid) = char_uuid {
                match futures::executor::block_on(
                    service
                        .GetCharacteristicsForUuidAsync(char_uuid)?
                        .into_future(),
                ) {
                    Ok(result) => {
                        println!(
                            "    按 UUID 查 status={:?} count={}",
                            result.Status(),
                            result
                                .Characteristics()
                                .map(|c| c.Size().unwrap_or(0))
                                .unwrap_or(0)
                        );
                    }
                    Err(error) => println!("    按 UUID 查失败：{error}"),
                }
            }
        }
    }

    // 2026-11-15：探针用完必须显式关闭设备对象。否则 GATT 连接会被留在这里，
    // 应用随后连接就会拿到 gatt_unreachable / operation_timed_out——反过来干扰被测对象。
    let _ = device.Close();
    let _ = HSTRING::new();
    Ok(())
}
