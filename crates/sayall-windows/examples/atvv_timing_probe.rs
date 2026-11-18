//! ATVV 音频通知时序探针（2026-11-16「只有满速 1/4」现场用）。
//!
//! 只读：订阅 ATVV 音频特征，记录每次通知的到达时间与负载长度，输出间隔直方图。
//! 用途是把三种成因分开：
//!   - 间隔几乎等距、集中在 ~12.5 ms → 每个连接事件只送 1 包（链路/控制器限流）；
//!   - 间隔是几毫秒的簇、簇内有多个包 → 远端成簇发送，链路没限；
//!   - 间隔杂乱并带大空洞 → 远端产生不足或重传。
//!
//! 不改设备状态、不写配置。用法：
//!   cargo run --release -p sayall-windows --example atvv_timing_probe [HEX地址]

use std::future::IntoFuture;
use std::time::{Duration, Instant};

use windows::Devices::Bluetooth::BluetoothLEDevice;
use windows::Devices::Bluetooth::GenericAttributeProfile::{
    GattCharacteristic, GattClientCharacteristicConfigurationDescriptorValue,
    GattValueChangedEventArgs, GattSession,
};
use windows::Foundation::TypedEventHandler;
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
use windows::core::GUID;

fn wait<T, F>(future: F) -> windows::core::Result<T>
where
    F: IntoFuture<Output = windows::core::Result<T>>,
{
    futures::executor::block_on(future.into_future())
}

const ATVV_SERVICE: GUID = GUID::from_u128(0xAB5E0001_5A21_4F05_BC7D_AF01F617B664);
const ATVV_AUDIO: GUID = GUID::from_u128(0xAB5E0002_5A21_4F05_BC7D_AF01F617B664);

const RUN: Duration = Duration::from_secs(12);

fn target_address() -> Option<u64> {
    if let Some(raw) = std::env::args().nth(1) {
        let hex: String = raw.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        if hex.len() == 12 {
            return u64::from_str_radix(&hex, 16).ok();
        }
    }
    let base = std::env::var("LOCALAPPDATA").ok()?;
    let path = std::path::Path::new(&base)
        .join("SayAll")
        .join("rc003-target.ini");
    let text = std::fs::read_to_string(path).ok()?;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("address=") {
            let hex: String = value.chars().filter(|c| c.is_ascii_hexdigit()).collect();
            if hex.len() == 12 {
                return u64::from_str_radix(&hex, 16).ok();
            }
        }
    }
    None
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let address = target_address().ok_or("没有可用的目标地址（rc003-target.ini 或命令行参数）")?;
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
    }

    let device = wait(BluetoothLEDevice::FromBluetoothAddressAsync(address)?)?;
    let services = wait(device.GetGattServicesForUuidAsync(ATVV_SERVICE)?)?;
    if services.Services()?.Size()? == 0 {
        println!("atvv_service_missing");
        return Ok(());
    }
    let service = services.Services()?.GetAt(0)?;
    let audio = wait(service.GetCharacteristicsForUuidAsync(ATVV_AUDIO)?)?
        .Characteristics()?
        .GetAt(0)?;

    let session = wait(GattSession::FromDeviceIdAsync(&device.BluetoothDeviceId()?)?)?;
    session.SetMaintainConnection(true)?;
    let mtu = session.MaxPduSize().unwrap_or(0);

    let cccd = wait(audio
        .WriteClientCharacteristicConfigurationDescriptorAsync(
            GattClientCharacteristicConfigurationDescriptorValue::Notify,
        )?)?;
    println!("cccd_status={cccd:?} max_pdu={mtu}");

    let stamps = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u32>::new()));
    let sizes = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u32>::new()));
    let started = Instant::now();

    let stamps_cb = stamps.clone();
    let sizes_cb = sizes.clone();
    let handler = TypedEventHandler::<GattCharacteristic, GattValueChangedEventArgs>::new(
        move |_, args| {
            let elapsed = started.elapsed().as_millis() as u32;
            if let Some(args) = args.as_ref() {
                if let Ok(buffer) = args.CharacteristicValue() {
                    let len = buffer.Length().unwrap_or(0);
                    if let Ok(mut guard) = sizes_cb.lock() {
                        guard.push(len);
                    }
                }
            }
            if let Ok(mut guard) = stamps_cb.lock() {
                guard.push(elapsed);
            }
            Ok(())
        },
    );
    let token = audio.ValueChanged(&handler)?;

    println!("listening {RUN:?} —— 请现在按住遥控器语音键，连续说 8 秒以上");

    std::thread::sleep(RUN);

    let samples = stamps.lock().map(|guard| guard.clone()).unwrap_or_default();
    let payloads = sizes.lock().map(|guard| guard.clone()).unwrap_or_default();
    let mut gaps: Vec<u32> = samples
        .windows(2)
        .filter(|pair| pair[1] >= pair[0])
        .map(|pair| pair[1] - pair[0])
        .collect();
    gaps.sort_unstable();

    let window_ms = samples.last().copied().unwrap_or(0).max(1);
    println!(
        "notifications={} window_ms={window_ms} frames_per_sec={:.1}",
        samples.len(),
        samples.len() as f64 * 1000.0 / window_ms as f64
    );
    if !gaps.is_empty() {
        let pick = |q: f64| gaps[((gaps.len() as f64 - 1.0) * q) as usize];
        println!(
            "gap_ms p10={} p50={} p90={} max={}",
            pick(0.10),
            pick(0.50),
            pick(0.90),
            gaps[gaps.len() - 1]
        );
        let mut bucket = [0usize; 6];
        for gap in &gaps {
            let index = match *gap {
                0..=2 => 0,
                3..=7 => 1,
                8..=15 => 2,
                16..=30 => 3,
                31..=99 => 4,
                _ => 5,
            };
            bucket[index] += 1;
        }
        println!(
            "hist <=2ms:{} 3-7ms:{} 8-15ms:{} 16-30ms:{} 31-99ms:{} >=100ms:{}",
            bucket[0], bucket[1], bucket[2], bucket[3], bucket[4], bucket[5]
        );
    }
    if !payloads.is_empty() {
        let total: u32 = payloads.iter().sum();
        println!(
            "payload avg={:.1}B min={} max={}",
            total as f64 / payloads.len() as f64,
            payloads.iter().min().copied().unwrap_or(0),
            payloads.iter().max().copied().unwrap_or(0)
        );
    }

    let _ = audio.RemoveValueChanged(token);
    let _ = audio.WriteClientCharacteristicConfigurationDescriptorAsync(
        GattClientCharacteristicConfigurationDescriptorValue::None,
    );
    let _ = session.SetMaintainConnection(false);
    Ok(())
}
