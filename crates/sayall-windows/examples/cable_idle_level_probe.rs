//! 虚拟声卡「空闲电平」探针：没有按住语音键时，CABLE Output 上到底有没有信号？
//!
//! 背景（2026-11-11 现场）：用户截图显示 Chatterfly 的麦克风检测条在**没按键、
//! 没说话**时也是满的（选中「自动检测 (CABLE Output)」）。应用日志里
//! `audio_stream phase=started` 反复出现却从未出现停止记录，怀疑渲染流结束后
//! 端点仍被占用、VB-CABLE 把最后一次缓冲反复播放（直流/残留），于是输入法
//! 侧看到的是持续满幅。
//!
//! 方法：只**采集** CABLE Output，不渲染任何东西，读 2 秒，按 200ms 窗口输出
//! peak / rms（dBFS）与样本 min/max。直流偏移会让 min/max 明显偏离 0 且
//! rms 与 peak 接近；噪声则 rms 远低于 peak。
//!
//! 判定：
//!   peak < -60 dBFS  ⇒ 空闲静音，输入法看到的满格不是这条链路的信号
//!   peak 高且 min/max 稳定偏离 0 ⇒ 直流/残留缓冲，渲染流没真正放干净
//!   peak 高且 rms 低 ⇒ 有噪声在持续写入

use std::time::{Duration, Instant};
use wasapi::{AudioClient, DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat};

const SAMPLE_RATE: usize = 16_000;
const CHANNELS: usize = 1;
const WINDOW_MS: u64 = 200;
const TOTAL_MS: u64 = 2000;

fn describe(error: &wasapi::WasapiError) -> String {
    match error {
        wasapi::WasapiError::Windows(inner) => {
            format!("{error} hresult=0x{:08X}", inner.code().0 as u32)
        }
        other => format!("{other}"),
    }
}

fn find_endpoint(enumerator: &DeviceEnumerator, needle: &str) -> Option<(String, String)> {
    let collection = enumerator.get_device_collection(&Direction::Capture).ok()?;
    for device in &collection {
        let Ok(device) = device else { continue };
        let (Ok(id), Ok(name)) = (device.get_id(), device.get_friendlyname()) else {
            continue;
        };
        if name.contains(needle) {
            return Some((id, name));
        }
    }
    None
}

fn open_capture(
    enumerator: &DeviceEnumerator,
    id: &str,
) -> Result<(AudioClient, wasapi::AudioCaptureClient), wasapi::WasapiError> {
    let device = enumerator.get_device(id)?;
    let mut client = device.get_iaudioclient()?;
    let (period, _) = client.get_device_period()?;
    let format = WaveFormat::new(16, 16, &SampleType::Int, SAMPLE_RATE, CHANNELS, None);
    client.initialize_client(
        &format,
        &Direction::Capture,
        &StreamMode::PollingShared {
            autoconvert: true,
            buffer_duration_hns: period,
        },
    )?;
    let capture = client.get_audiocaptureclient()?;
    Ok((client, capture))
}

fn dbfs(value: f64) -> f64 {
    if value <= 0.0 {
        f64::NEG_INFINITY
    } else {
        20.0 * (value / i16::MAX as f64).log10()
    }
}

fn report(label: &str, peak: i32, sum_sq: f64, count: usize, min: i32, max: i32) {
    let rms = if count == 0 {
        0.0
    } else {
        (sum_sq / count as f64).sqrt()
    };
    println!(
        "[idle] {label} peak={peak} ({:.1} dBFS) rms={:.1} dBFS min={min} max={max} samples={count}",
        dbfs(peak as f64),
        dbfs(rms),
    );
}

fn main() {
    let mta = wasapi::initialize_mta();
    if mta.is_err() {
        println!("initialize_mta failed: hr=0x{:08X}", mta.0 as u32);
        return;
    }
    let enumerator = match DeviceEnumerator::new() {
        Ok(value) => value,
        Err(error) => {
            println!("DeviceEnumerator::new failed: {}", describe(&error));
            return;
        }
    };

    let needle = std::env::args()
        .skip_while(|argument| argument != "--device")
        .nth(1)
        .unwrap_or_else(|| "CABLE Output".to_owned());

    let Some((id, name)) = find_endpoint(&enumerator, &needle) else {
        println!("[idle] 未找到名称含 {needle:?} 的录音端点");
        return;
    };
    println!("[idle] capture endpoint = {name}");

    let (client, capture) = match open_capture(&enumerator, &id) {
        Ok(pair) => pair,
        Err(error) => {
            println!("[idle] 录音端点打不开：{}", describe(&error));
            return;
        }
    };
    if let Err(error) = client.start_stream() {
        println!("[idle] 录音流启动失败：{}", describe(&error));
        return;
    }

    let started = Instant::now();
    let mut window_start = started;
    let mut window_peak = 0i32;
    let mut window_sum_sq = 0.0f64;
    let mut window_count = 0usize;
    let mut window_min = i32::MAX;
    let mut window_max = i32::MIN;
    let mut total_peak = 0i32;
    let mut total_sum_sq = 0.0f64;
    let mut total_count = 0usize;
    let mut total_min = i32::MAX;
    let mut total_max = i32::MIN;
    let mut packet_bytes = vec![0u8; 8192];

    while started.elapsed() < Duration::from_millis(TOTAL_MS) {
        match capture.get_next_packet_size() {
            Ok(Some(frames)) if frames > 0 => {
                if let Ok((read, _info)) = capture.read_from_device(&mut packet_bytes) {
                    let bytes = (read as usize) * 2;
                    for pair in packet_bytes[..bytes.min(packet_bytes.len())].chunks_exact(2) {
                        let sample = i16::from_le_bytes([pair[0], pair[1]]) as i32;
                        let magnitude = sample.abs();
                        window_peak = window_peak.max(magnitude);
                        total_peak = total_peak.max(magnitude);
                        window_sum_sq += (sample as f64) * (sample as f64);
                        total_sum_sq += (sample as f64) * (sample as f64);
                        window_count += 1;
                        total_count += 1;
                        window_min = window_min.min(sample);
                        window_max = window_max.max(sample);
                        total_min = total_min.min(sample);
                        total_max = total_max.max(sample);
                    }
                }
            }
            _ => std::thread::sleep(Duration::from_millis(2)),
        }
        if window_start.elapsed() >= Duration::from_millis(WINDOW_MS) {
            let offset = started.elapsed().as_millis();
            report(
                &format!("t={offset}ms"),
                window_peak,
                window_sum_sq,
                window_count,
                if window_count == 0 { 0 } else { window_min },
                if window_count == 0 { 0 } else { window_max },
            );
            window_start = Instant::now();
            window_peak = 0;
            window_sum_sq = 0.0;
            window_count = 0;
            window_min = i32::MAX;
            window_max = i32::MIN;
        }
    }
    let _ = client.stop_stream();
    report(
        "TOTAL",
        total_peak,
        total_sum_sq,
        total_count,
        if total_count == 0 { 0 } else { total_min },
        if total_count == 0 { 0 } else { total_max },
    );

    let peak_dbfs = dbfs(total_peak as f64);
    let verdict = if peak_dbfs < -60.0 {
        "SILENT: 空闲时 CABLE Output 是静音，输入法看到的满格不来自这条链路"
    } else if total_count > 0 && (total_min.abs().min(total_max.abs()) as f64) > 1000.0 {
        "DC_OFFSET: 采样整体偏离 0，端点残留了上一次的缓冲（渲染流未真正放干净）"
    } else {
        "LIVE: 空闲时仍有信号在持续写入 CABLE Input"
    };
    println!("[idle] verdict: {verdict}");
}
