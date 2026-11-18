//! 切换压力探针（2026-11-16）：判定「切换遥控器后被拒」的脏源在应用还是在系统。
//!
//! 背景：应用内切换设备、系统里删除设备、甚至重启应用之后，目标设备都可能
//! 返回 `gatt_access_denied`，而重启电脑能恢复。到底是**我们应用的收尾**把宿主
//! 搞脏了，还是**系统/设备**本身如此，用一个不依赖产品的独立进程就能分开：
//!
//!   连接 A → 读一个需要加密的特征 → Close → 等 500ms → 连接 B → …（默认 20 轮）
//!
//! - 20 轮全部成功 ⇒ 脏是我们应用造成的（切换时的句柄/回调没清干净），可改代码修；
//! - 数轮内被拒    ⇒ 脏在系统/设备侧，产品侧只能做"快速复位"，改代码无用。
//!
//! 用法：
//!   cargo run --release -p sayall-windows --example switch_stress_probe -- <A地址> <B地址> [轮数]
//! 地址为 12 位十六进制（不带冒号），例如 `c05d39f86ed6 14befc9cd7d9 20`。
//!
//! 只读：不做任何写入、不改配置；失败只记录并继续下一轮。

use std::future::IntoFuture;
use std::time::Duration;

use windows::Devices::Bluetooth::GenericAttributeProfile::{
    GattCharacteristic, GattDeviceService, GattSession,
};
use windows::Devices::Bluetooth::{BluetoothCacheMode, BluetoothLEDevice};
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
use windows::core::GUID;

const ATVV_SERVICE: GUID = GUID::from_u128(0xAB5E0001_5A21_4F05_BC7D_AF01F617B664);
const ATVV_CONTROL: GUID = GUID::from_u128(0xAB5E0003_5A21_4F05_BC7D_AF01F617B664);
/// 每次切换之间的静默窗口：给 Windows 关掉上一条 ACL 留时间。
const SWITCH_GAP: Duration = Duration::from_millis(500);

fn wait<T, F>(future: F) -> windows::core::Result<T>
where
    F: IntoFuture<Output = windows::core::Result<T>>,
{
    futures::executor::block_on(future.into_future())
}

fn parse(hex: &str) -> Option<u64> {
    let digits: String = hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if digits.len() != 12 {
        return None;
    }
    u64::from_str_radix(&digits, 16).ok()
}

/// 一轮：连上 → 取 ATVV 服务与控制特征（这一步在加密链路上） → 全部关掉。
fn one_round(address: u64) -> Result<String, String> {
    let open = BluetoothLEDevice::FromBluetoothAddressAsync(address).map_err(to_err)?;
    let device = wait(open).map_err(to_err)?;
    let device_id = device.BluetoothDeviceId().map_err(to_err)?;
    let session_op = GattSession::FromDeviceIdAsync(&device_id).map_err(to_err)?;
    let session = wait(session_op).map_err(to_err)?;
    let _ = session.SetMaintainConnection(true);
    let mtu = session.MaxPduSize().unwrap_or(0);

    let services_op = device
        .GetGattServicesForUuidWithCacheModeAsync(ATVV_SERVICE, BluetoothCacheMode::Uncached)
        .map_err(to_err)?;
    let services = wait(services_op).map_err(to_err)?;
    let list = services.Services().map_err(to_err)?;
    if list.Size().map_err(to_err)? == 0 {
        let _ = device.Close();
        return Err("atvv_service_missing".to_owned());
    }
    let service: GattDeviceService = list.GetAt(0).map_err(to_err)?;

    let chars_op = service
        .GetCharacteristicsForUuidWithCacheModeAsync(ATVV_CONTROL, BluetoothCacheMode::Uncached)
        .map_err(to_err)?;
    let chars = wait(chars_op).map_err(to_err)?;
    let char_list = chars.Characteristics().map_err(to_err)?;
    if char_list.Size().map_err(to_err)? == 0 {
        let _ = service.Close();
        let _ = device.Close();
        return Err("atvv_control_missing".to_owned());
    }
    let characteristic: GattCharacteristic = char_list.GetAt(0).map_err(to_err)?;
    let protected = characteristic.ProtectionLevel().map_err(to_err)?;

    let _ = service.Close();
    let _ = session.SetMaintainConnection(false);
    let _ = device.Close();
    Ok(format!("ok mtu={mtu} protection={protected:?}"))
}

fn to_err<E: std::fmt::Debug>(error: E) -> String {
    format!("{error:?}")
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        println!("用法: switch_stress_probe <A地址> <B地址> [轮数]");
        return;
    }
    let (Some(a), Some(b)) = (parse(&args[0]), parse(&args[1])) else {
        println!("地址必须是 12 位十六进制");
        return;
    };
    let rounds: usize = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(20);

    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
    }

    let mut failures = 0usize;
    for round in 1..=rounds {
        let (target, label) = if round % 2 == 1 { (a, "A") } else { (b, "B") };
        let started = std::time::Instant::now();
        let outcome = one_round(target);
        let elapsed = started.elapsed().as_millis();
        match outcome {
            Ok(detail) => println!("round={round} device={label} result=ok {detail} elapsed_ms={elapsed}"),
            Err(error) => {
                failures += 1;
                println!("round={round} device={label} result=failed error={error} elapsed_ms={elapsed}");
            }
        }
        std::thread::sleep(SWITCH_GAP);
    }

    println!("summary rounds={rounds} failures={failures}");
    if failures == 0 {
        println!("判定: 切换到第 {rounds} 轮仍全部成功 ⇒ 脏不在本进程路径之外，问题更可能在应用侧的收尾/并发");
    } else {
        println!("判定: {failures}/{rounds} 轮失败 ⇒ 脏在系统或设备侧，改应用代码无效，需要快速复位手段");
    }
}
