//! ATVV 特征发现故障探针（2026-11-14「发现 ATVV 特征返回状态 3」现场用）。
//!
//! 目的：分清服务发现成功、特征发现被拒（`GattCommunicationStatus(3)`）到底是
//! 哪一层的问题，并验证三条候选路径是否有一条能拿到特征：
//!
//!   A. `BluetoothLEDevice` → `GetGattServicesForUuid(Uncached)`
//!      → `GetCharacteristicsForUuid(Uncached)`      ← 主应用当前走的路径
//!   A2. 同一个 service 对象 → `GetCharacteristics(Uncached)`（整表枚举）
//!   B. `GattDeviceService::GetDeviceSelectorFromUuid` + `DeviceInformation`
//!      → `GattDeviceService::FromIdAsync`（共享访问路径）
//!      → 逐 UUID 查询 + 整表枚举
//!
//! 用法：cargo run --release -p sayall-windows --example atvv_char_probe
//!
//! 只做查询，不写 CCCD、不改系统状态。

use std::future::IntoFuture;

use windows::core::GUID;
use windows::Devices::Bluetooth::BluetoothConnectionStatus;
use windows::Devices::Bluetooth::BluetoothLEDevice;
use windows::Devices::Bluetooth::GenericAttributeProfile::{
    GattCharacteristic, GattCommunicationStatus, GattDeviceService,
};
use windows::Devices::Enumeration::DeviceInformation;
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

use windows::Devices::Bluetooth::BluetoothCacheMode;

const SERVICE_UUID: GUID = GUID::from_u128(0xab5e00015a214f05bc7daf01f617b664);
const TRANSMIT_UUID: GUID = GUID::from_u128(0xab5e00025a214f05bc7daf01f617b664);
const AUDIO_UUID: GUID = GUID::from_u128(0xab5e00035a214f05bc7daf01f617b664);
const CONTROL_UUID: GUID = GUID::from_u128(0xab5e00045a214f05bc7daf01f617b664);

fn status_text(status: GattCommunicationStatus) -> String {
    match status {
        GattCommunicationStatus::Success => "Success(0)".to_owned(),
        GattCommunicationStatus::Unreachable => "Unreachable(1)".to_owned(),
        GattCommunicationStatus::ProtocolError => "ProtocolError(2)".to_owned(),
        GattCommunicationStatus::AccessDenied => "AccessDenied(3)".to_owned(),
        other => format!("Unknown({})", other.0),
    }
}

fn main() {
    unsafe {
        RoInitialize(RO_INIT_MULTITHREADED).expect("RoInitialize");
    }

    // 1) 已配对 BLE 设备清单
    let selector = BluetoothLEDevice::GetDeviceSelectorFromPairingState(true).expect("selector");
    let operation =
        DeviceInformation::FindAllAsyncAqsFilter(&selector).expect("FindAllAsyncAqsFilter");
    let devices = futures::executor::block_on(operation.into_future()).expect("设备枚举失败");
    let count = devices.Size().unwrap();
    println!("== 已配对 BLE 设备 {count} 台 ==");
    let mut ids = Vec::new();
    for index in 0..count {
        let info = devices.GetAt(index).unwrap();
        let name = info.Name().map(|n| n.to_string()).unwrap_or_default();
        let id = info.Id().map(|i| i.to_string()).unwrap_or_default();
        println!("[{index}] name={name} id={id}");
        ids.push((name, id));
    }
    if ids.is_empty() {
        println!("没有已配对设备，退出。");
        return;
    }

    for (index, (name, id)) in ids.iter().enumerate() {
        println!("\n===== [{index}] {name} =====");
        probe_device(id);
    }
}

fn probe_device(device_id: &str) {
    // 路径 A：BluetoothLEDevice 级查询（主应用当前路径）
    let device = {
        let operation = BluetoothLEDevice::FromIdAsync(&windows::core::HSTRING::from(device_id))
            .expect("FromIdAsync");
        match futures::executor::block_on(operation.into_future()) {
            Ok(device) => Some(device),
            Err(error) => {
                println!("A: FromIdAsync 失败: {error}");
                None
            }
        }
    };

    if let Some(device) = device.as_ref() {
        // WinRT 的枚举是 `Disconnected = 0, Connected = 1`；用 Debug 字符串里是否含
        // "Connected" 来判定会**把已连接误报成未连接**（`BluetoothConnectionStatus(1)`
        // 不含这个词）——2026-11-14 我自己被这行误导过，按值比较才可靠。
        let status = device.ConnectionStatus();
        let connected = matches!(status, Ok(s) if s == BluetoothConnectionStatus::Connected);
        println!(
            "A: ConnectionStatus={:?} (connected={connected})",
            status
                .map(|s| format!("{s:?}"))
                .unwrap_or_else(|e| format!("读取失败 {e}"))
        );

        let services = {
            let operation = match device.GetGattServicesForUuidWithCacheModeAsync(
                SERVICE_UUID,
                BluetoothCacheMode::Uncached,
            ) {
                Ok(operation) => operation,
                Err(error) => {
                    println!("A: 发起服务查询失败: {error}");
                    return;
                }
            };
            match futures::executor::block_on(operation.into_future()) {
                Ok(result) => {
                    let status = result.Status().unwrap();
                    println!("A: 服务查询 status={}", status_text(status));
                    if status != GattCommunicationStatus::Success {
                        return;
                    }
                    result.Services().unwrap()
                }
                Err(error) => {
                    println!("A: 服务查询异常: {error}");
                    return;
                }
            }
        };
        let service_count = services.Size().unwrap();
        println!("A: 服务数量={service_count}");
        if service_count == 0 {
            return;
        }
        let service = services.GetAt(0).unwrap();

        let result = service
            .GetCharacteristicsForUuidWithCacheModeAsync(
                TRANSMIT_UUID,
                BluetoothCacheMode::Uncached,
            )
            .expect("发起特征查询");
        match futures::executor::block_on(result.into_future()) {
            Ok(result) => {
                let status = result.Status().unwrap();
                let size = result
                    .Characteristics()
                    .map(|c| c.Size().unwrap())
                    .unwrap_or(0);
                println!(
                    "A: 按 UUID 查特征(Uncached) status={} count={size}",
                    status_text(status)
                );
            }
            Err(error) => println!("A: 按 UUID 查特征异常: {error}"),
        }

        let result = service
            .GetCharacteristicsForUuidWithCacheModeAsync(TRANSMIT_UUID, BluetoothCacheMode::Cached)
            .expect("发起特征查询");
        match futures::executor::block_on(result.into_future()) {
            Ok(result) => {
                let status = result.Status().unwrap();
                let size = result
                    .Characteristics()
                    .map(|c| c.Size().unwrap())
                    .unwrap_or(0);
                println!(
                    "A: 按 UUID 查特征(Cached)   status={} count={size}",
                    status_text(status)
                );
            }
            Err(error) => println!("A: 按 UUID 查特征(Cached) 异常: {error}"),
        }

        // A2：整表枚举（同一个 service 对象）
        let result = service
            .GetCharacteristicsWithCacheModeAsync(BluetoothCacheMode::Uncached)
            .expect("发起整表枚举");
        match futures::executor::block_on(result.into_future()) {
            Ok(result) => {
                let status = result.Status().unwrap();
                println!("A2: 整表枚举(Uncached) status={}", status_text(status));
                if status == GattCommunicationStatus::Success {
                    let list = result.Characteristics().unwrap();
                    let size = list.Size().unwrap();
                    println!("A2: 特征总数={size}");
                    for i in 0..size {
                        let characteristic: GattCharacteristic = list.GetAt(i).unwrap();
                        let uuid = characteristic
                            .Uuid()
                            .map(|u| format!("{u:?}"))
                            .unwrap_or_default();
                        println!("    [{i}] {uuid}");
                    }
                }
            }
            Err(error) => println!("A2: 整表枚举异常: {error}"),
        }
    }

    // 路径 B：DeviceInformation + GattDeviceService::FromIdAsync（共享访问）
    let selector = match GattDeviceService::GetDeviceSelectorFromUuid(SERVICE_UUID) {
        Ok(selector) => selector,
        Err(error) => {
            println!("B: 选择器失败: {error}");
            return;
        }
    };
    let operation = match DeviceInformation::FindAllAsyncAqsFilter(&selector) {
        Ok(operation) => operation,
        Err(error) => {
            println!("B: 枚举接口失败: {error}");
            return;
        }
    };
    let interfaces = match futures::executor::block_on(operation.into_future()) {
        Ok(collection) => collection,
        Err(error) => {
            println!("B: 枚举接口异常: {error}");
            return;
        }
    };
    let size = interfaces.Size().unwrap();
    println!("B: ATVV 服务接口候选 {size} 个");
    // 设备的蓝牙地址：取 id 最后一段里的冒号地址
    let address = device_id
        .rsplit('-')
        .next()
        .map(|tail| tail.replace(':', "").to_uppercase())
        .unwrap_or_default();
    for index in 0..size {
        let info = interfaces.GetAt(index).unwrap();
        let id = info.Id().map(|i| i.to_string()).unwrap_or_default();
        let name = info.Name().map(|n| n.to_string()).unwrap_or_default();
        let matches = id.to_uppercase().contains(&address);
        println!("B[{index}] name={name} 目标={matches} id={id}");
        if !matches {
            continue;
        }
        let operation = match GattDeviceService::FromIdAsync(&info.Id().unwrap()) {
            Ok(operation) => operation,
            Err(error) => {
                println!("B[{index}]: FromIdAsync 失败: {error}");
                continue;
            }
        };
        let service = match futures::executor::block_on(operation.into_future()) {
            Ok(service) => service,
            Err(error) => {
                println!("B[{index}]: FromIdAsync 异常: {error}");
                continue;
            }
        };
        for (label, uuid) in [
            ("transmit", TRANSMIT_UUID),
            ("audio", AUDIO_UUID),
            ("control", CONTROL_UUID),
        ] {
            let operation = match service
                .GetCharacteristicsForUuidWithCacheModeAsync(uuid, BluetoothCacheMode::Uncached)
            {
                Ok(operation) => operation,
                Err(error) => {
                    println!("B[{index}]: 查询 {label} 发起失败: {error}");
                    continue;
                }
            };
            match futures::executor::block_on(operation.into_future()) {
                Ok(result) => {
                    let status = result.Status().unwrap();
                    let count = result
                        .Characteristics()
                        .map(|c| c.Size().unwrap())
                        .unwrap_or(0);
                    println!(
                        "B[{index}]: 按 UUID 查 {label} status={} count={count}",
                        status_text(status)
                    );
                }
                Err(error) => println!("B[{index}]: 查询 {label} 异常: {error}"),
            }
        }
        let operation =
            match service.GetCharacteristicsWithCacheModeAsync(BluetoothCacheMode::Uncached) {
                Ok(operation) => operation,
                Err(error) => {
                    println!("B[{index}]: 整表枚举发起失败: {error}");
                    continue;
                }
            };
        match futures::executor::block_on(operation.into_future()) {
            Ok(result) => {
                let status = result.Status().unwrap();
                println!("B[{index}]: 整表枚举 status={}", status_text(status));
            }
            Err(error) => println!("B[{index}]: 整表枚举异常: {error}"),
        }
    }
}
