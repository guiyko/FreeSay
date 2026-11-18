//! ATVV 订阅探针（2026-11-15「CCCD 写不进去」现场用）。
//!
//! 应用日志已经走到「拿到特征、但 `WriteClientCharacteristicConfigurationDescriptorAsync`
//! 返回 AsyncStatus(3)=Error」这一步，说明加密已通、卡在订阅。这个探针把该步单独测出来：
//!   1. 列出 ATVV 服务下所有特征及其属性（Notify / Indicate / Write …）；
//!   2. 对每个支持 Notify/Indicate 的特征，试写 CCCD，打印状态与耗时。
//!
//! 用法：`atvv_subscribe_probe [HEX地址]`，默认 c05d39f86ed6。

use std::future::IntoFuture;
use std::time::Instant;

use windows::core::GUID;
use windows::Devices::Bluetooth::BluetoothLEDevice;
use windows::Devices::Bluetooth::GenericAttributeProfile::{
    GattCharacteristicProperties, GattClientCharacteristicConfigurationDescriptorValue,
};
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

    let services = futures::executor::block_on(
        device
            .GetGattServicesForUuidAsync(SERVICE_UUID)?
            .into_future(),
    )?;
    println!("服务发现 status={:?}", services.Status());
    let list = services.Services()?;
    if list.Size().unwrap_or(0) == 0 {
        println!("没有 ATVV 服务");
        let _ = device.Close();
        return Ok(());
    }

    for service in &list {
        let characteristics =
            futures::executor::block_on(service.GetCharacteristicsAsync()?.into_future())?;
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
            let indicate = has(properties, GattCharacteristicProperties::Indicate);
            println!("  {uuid}  notify={notify} indicate={indicate}");

            if !notify && !indicate {
                continue;
            }
            let descriptor = if notify {
                GattClientCharacteristicConfigurationDescriptorValue::Notify
            } else {
                GattClientCharacteristicConfigurationDescriptorValue::Indicate
            };
            let started = Instant::now();
            match characteristic.WriteClientCharacteristicConfigurationDescriptorAsync(descriptor) {
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

    let _ = device.Close();
    Ok(())
}
