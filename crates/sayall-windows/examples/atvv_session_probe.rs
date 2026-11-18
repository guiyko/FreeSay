//! 节点 1 探针（2026-11-15，Gemini 决策树第 1 步）：不动配对，靠「会话 + 无缓存 + 声明加密等级」
//! 让内核在发 ATT 写请求之前先把链路加密起来。
//!
//! 三个关键动作，缺一不可：
//!   1. 持有 `GattSession` 并 `SetMaintainConnection(true)` —— 让 Windows 保持连接与安全上下文；
//!   2. 服务与特征都用 `Uncached` 拉取 —— 绕过 19045 上已知的 GATT 缓存/权限错位；
//!   3. 写 CCCD 之前 `SetProtectionLevel(EncryptionRequired)`（注意是 2，**不是** 3）——
//!      指示 bthleenum 在发送写请求前检查 ACL 加密状态，未加密则先跑 LL_ENC_REQ。
//!
//! **本探针不调用任何 Pair / Unpair**（那正是把绑定搞成半残的原因）。
//!
//! 用法：`atvv_session_probe [HEX地址]`，默认 c05d39f86ed6。

use std::future::IntoFuture;
use std::time::Instant;

use windows::core::GUID;
use windows::Devices::Bluetooth::GenericAttributeProfile::{
    GattCharacteristicProperties, GattClientCharacteristicConfigurationDescriptorValue,
    GattProtectionLevel, GattSession,
};
use windows::Devices::Bluetooth::{BluetoothCacheMode, BluetoothLEDevice};
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

const SERVICE_UUID: GUID = GUID::from_u128(0xab5e0001_5a21_4f05_bc7d_af01f617b664);

fn has(properties: GattCharacteristicProperties, flag: GattCharacteristicProperties) -> bool {
    (properties.0 & flag.0) == flag.0
}

fn main() -> windows::core::Result<()> {
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
    }

    let address = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "c05d39f86ed6".to_owned());
    let value = u64::from_str_radix(&address.replace(':', ""), 16).unwrap_or(0);
    println!("目标地址={address}");

    let device = futures::executor::block_on(
        BluetoothLEDevice::FromBluetoothAddressAsync(value)?.into_future(),
    )?;
    println!(
        "设备 name={:?} status={:?}",
        device.Name(),
        device.ConnectionStatus()
    );

    // 1) 持有会话并维持连接（不能提前 drop）
    let device_id = device.BluetoothDeviceId()?;
    let session =
        futures::executor::block_on(GattSession::FromDeviceIdAsync(&device_id)?.into_future())?;
    session.SetMaintainConnection(true)?;
    // MaintainConnection 只是"许愿"，链路建立是异步的：给内核最多 5 秒把 ACL 拉起来，
    // 期间轮询 SessionStatus，避免刚设置完就查服务拿到 Unreachable。
    let mut connected_at: Option<u32> = None;
    for tick in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let status = session.SessionStatus();
        println!("  wait {tick} SessionStatus={status:?}");
        if let Ok(value) = status {
            if value.0 != 0 {
                connected_at = Some(tick);
                break;
            }
        }
    }
    match connected_at {
        Some(tick) => println!("会话在约 {}ms 后建立", (tick + 1) * 500),
        None => println!("会话 5 秒内未建立（设备可能睡眠，需要按一下遥控器）"),
    }

    // 2) 服务与特征全部 Uncached
    let services = futures::executor::block_on(
        device
            .GetGattServicesForUuidWithCacheModeAsync(SERVICE_UUID, BluetoothCacheMode::Uncached)?
            .into_future(),
    )?;
    println!("服务发现 status={:?}", services.Status());
    let list = services.Services()?;
    if list.Size().unwrap_or(0) == 0 {
        println!("没有 ATVV 服务");
        let _ = session.SetMaintainConnection(false);
        let _ = device.Close();
        return Ok(());
    }

    for service in &list {
        let characteristics = futures::executor::block_on(
            service
                .GetCharacteristicsWithCacheModeAsync(BluetoothCacheMode::Uncached)?
                .into_future(),
        )?;
        println!(
            "\n特征枚举 status={:?} count={}",
            characteristics.Status(),
            characteristics.Characteristics()?.Size().unwrap_or(0)
        );

        for characteristic in &characteristics.Characteristics()? {
            let uuid = characteristic
                .Uuid()
                .map(|u| format!("{u:?}"))
                .unwrap_or_default();
            let properties = characteristic.CharacteristicProperties()?;
            let notify = has(properties, GattCharacteristicProperties::Notify);
            println!("  {uuid}  notify={notify}");
            if !notify {
                continue;
            }

            // 3) 关键：先声明需要加密，再写 CCCD
            match characteristic.SetProtectionLevel(GattProtectionLevel::EncryptionRequired) {
                Ok(()) => println!("    SetProtectionLevel(EncryptionRequired) ok"),
                Err(error) => println!("    SetProtectionLevel 失败 {error:?}"),
            }

            let started = Instant::now();
            match characteristic.WriteClientCharacteristicConfigurationDescriptorAsync(
                GattClientCharacteristicConfigurationDescriptorValue::Notify,
            ) {
                Ok(operation) => match futures::executor::block_on(operation.into_future()) {
                    Ok(status) => println!(
                        "    CCCD 写入 status={:?} elapsed={}ms",
                        status,
                        started.elapsed().as_millis()
                    ),
                    Err(error) => println!(
                        "    CCCD 异常 {error:?} elapsed={}ms",
                        started.elapsed().as_millis()
                    ),
                },
                Err(error) => println!("    CCCD 调用失败 {error:?}"),
            }
        }
    }

    let _ = session.SetMaintainConnection(false);
    let _ = device.Close();
    Ok(())
}
