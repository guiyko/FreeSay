//! 蓝牙绑定记录（`KeyLength`）健康检查与修复 —— 主程序侧的投递与等待。
//!
//! ## 为什么这件事不能在本进程里做
//!
//! `HKLM\SYSTEM\CurrentControlSet\Services\BTHPORT\Parameters\Keys\<adapter>\<device>`
//! 上 `BUILTIN\Users` 只有 `ReadKey`、`BUILTIN\Administrators` 才是 `FullControl`，
//! 而主程序是普通用户权限：能读、不能写。写完之后还必须让内核重新加载这条绑定
//! （本机实测：只改注册表、不重启 `bthserv`，ATVV 特征仍然 `AccessDenied`），
//! 那需要服务控制权限。两件事都在提权助手里做。
//!
//! ## 为什么用文件而不是桥接协议
//!
//! 桥接协议只表达"捕获目标 / 语音合成"这类高频绝对状态，没有请求-应答语义。
//! 为一件低频运维动作（只会在"重新配对之后"发生）给协议加请求 ID、超时、重试，
//! 成本与风险都不划算。文件信号是项目里已验证的普通权限投递通道（停止信号、
//! 卸载标记都用它）。助手每 500ms 轮询一次，典型往返 < 1s。
//!
//! 请求与结果都是纯文本 `key=value`，双方都能用记事本自查——出问题时不需要
//! 再加日志就能看清卡在哪一步。

use std::path::PathBuf;
use std::time::{Duration, Instant};

/// 一次检查/修复的结论。字段与助手写出的结果文件逐项对应。
#[derive(Debug, Clone, serde::Serialize)]
pub struct BondHealth {
    /// `healthy` / `needs_repair` / `repaired` / `ltk_invalid` / `missing` /
    /// `no_key_length` / `write_denied` / `bad_request` / `timeout`
    pub result: String,
    /// 记录的 `KeyLength`（读不到为 `None`）。
    pub key_length: Option<u32>,
    /// `LTK` 的字节数（正常 16）。
    pub ltk_len: u32,
    /// `LTK` 是否全零（全零说明绑定本身不可用，改长度无效，只能重配）。
    pub ltk_all_zero: bool,
    /// `skipped` / `restarted` / `restart_failed`。
    pub service: String,
}

impl BondHealth {
    fn failed(result: &str) -> Self {
        Self {
            result: result.to_owned(),
            key_length: None,
            ltk_len: 0,
            ltk_all_zero: true,
            service: "skipped".to_owned(),
        }
    }
}

fn sayall_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| {
        let home = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\".to_owned());
        format!("{home}\\AppData\\Local")
    });
    PathBuf::from(base).join("SayAll")
}

fn request_path() -> PathBuf {
    sayall_dir().join("bond-request.txt")
}

fn result_path() -> PathBuf {
    sayall_dir().join("bond-result.txt")
}

/// 从 WinRT 设备 id 里取 `(adapter, device)` 两段 12 位十六进制地址。
///
/// 形如 `BluetoothLE#BluetoothLEc0:bf:be:00:00:01-14:be:fc:00:00:02`：
/// 前半段是**本机适配器**，后半段是设备。适配器地址必须由调用方给 ——
/// `Keys` 这一层不允许普通用户枚举，助手没法自己找。
pub fn parse_addresses(device_id: &str) -> Option<(String, String)> {
    let mut found: Vec<String> = Vec::new();
    let bytes = device_id.as_bytes();
    let mut index = 0;
    while index + 17 <= bytes.len() {
        let window = &device_id[index..index + 17];
        let looks_like_mac = window
            .chars()
            .enumerate()
            .all(|(offset, ch)| match offset % 3 {
                2 => ch == ':',
                _ => ch.is_ascii_hexdigit(),
            });
        if looks_like_mac {
            found.push(window.replace(':', "").to_ascii_lowercase());
            index += 17;
            continue;
        }
        index += 1;
    }
    match found.len() {
        0 | 1 => None,
        _ => Some((found[0].clone(), found[1].clone())),
    }
}

/// 投递请求并等待结果。`repair=false` 只查不改。
///
/// 等待上限 12 秒：助手轮询节拍 500ms，修复路径还要重启 `bthserv`（本机约 2–4s），
/// 再长就是异常，宁可如实报 `timeout` 也不无限等。
pub fn request(device_id: &str, repair: bool) -> BondHealth {
    let Some((adapter, device)) = parse_addresses(device_id) else {
        return BondHealth::failed("bad_request");
    };
    let dir = sayall_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return BondHealth::failed("bad_request");
    }
    // 清掉上一轮的残留结果，避免把旧的 `healthy` 当成这次的结论。
    let _ = std::fs::remove_file(result_path());
    let body = format!(
        "adapter={adapter}\ndevice={device}\naction={}\n",
        if repair { "repair" } else { "check" }
    );
    if std::fs::write(request_path(), body).is_err() {
        return BondHealth::failed("bad_request");
    }

    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(12) {
        std::thread::sleep(Duration::from_millis(200));
        let Ok(text) = std::fs::read_to_string(result_path()) else {
            continue;
        };
        if let Some(health) = parse_result(&text) {
            let _ = std::fs::remove_file(result_path());
            sayall_windows::gatt_note(format!(
                "bond_health action={} result={} key_length={} ltk_len={} ltk_all_zero={} service={}",
                if repair { "repair" } else { "check" },
                health.result,
                health
                    .key_length
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_owned()),
                health.ltk_len,
                health.ltk_all_zero,
                health.service
            ));
            return health;
        }
    }
    sayall_windows::gatt_note(format!(
        "bond_health action={} result=timeout",
        if repair { "repair" } else { "check" }
    ));
    BondHealth::failed("timeout")
}

fn parse_result(text: &str) -> Option<BondHealth> {
    let mut health = BondHealth::failed("bad_request");
    let mut seen_result = false;
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "result" => {
                health.result = value.to_owned();
                seen_result = true;
            }
            "key_length" => {
                health.key_length = value.parse::<u32>().ok();
            }
            "ltk_len" => health.ltk_len = value.parse::<u32>().unwrap_or(0),
            "ltk_all_zero" => health.ltk_all_zero = value.eq_ignore_ascii_case("true"),
            "service" => health.service = value.to_owned(),
            _ => {}
        }
    }
    seen_result.then_some(health)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_adapter_and_device_from_winrt_id() {
        let id = "BluetoothLE#BluetoothLEc0:bf:be:00:00:01-14:be:fc:00:00:02";
        assert_eq!(
            parse_addresses(id),
            Some(("c0bfbe002875".to_owned(), "14befc9cd7d9".to_owned()))
        );
    }

    #[test]
    fn rejects_ids_without_two_addresses() {
        assert_eq!(parse_addresses(""), None);
        assert_eq!(
            parse_addresses("BluetoothLE#BluetoothLEc0:bf:be:00:00:01"),
            None
        );
    }

    #[test]
    fn parses_result_file() {
        let text =
            "result=repaired\nkey_length=16\nltk_len=16\nltk_all_zero=false\nservice=restarted\n";
        let health = parse_result(text).expect("应能解析");
        assert_eq!(health.result, "repaired");
        assert_eq!(health.key_length, Some(16));
        assert_eq!(health.ltk_len, 16);
        assert!(!health.ltk_all_zero);
        assert_eq!(health.service, "restarted");
    }

    #[test]
    fn result_without_result_line_is_rejected() {
        assert!(parse_result("key_length=16\n").is_none());
    }
}
