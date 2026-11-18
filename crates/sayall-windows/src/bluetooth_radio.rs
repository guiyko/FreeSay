//! 蓝牙栈僵死态的错误码分类（原"无线电自动恢复"模块）。
//!
//! 历史：本模块曾实现"BLE 僵死链路的终极恢复手段"——自动重连连续失败达
//! 阈值时关开整个蓝牙无线电（Off/On），并在 WinRT 栈不可用时回退到提权
//! PnP 重启。依据是 2026-09-05 真机取证：应用被强杀后 Windows 侧可能残留
//! 僵死的 GATT/HID 链路或服务缓存，普通重试循环永远无法恢复。
//!
//! **2026-10-14 产品决策：移除自动关开蓝牙无线电与 PnP 重启。** 原因：
//! - 该动作会断开用户**全部**蓝牙设备（现场报障："开启软件时蓝牙设备
//!   周期性中断、整个蓝牙断（图标消失）"，约 1 小时/次；关闭软件后恢复）；
//! - 收益从未被证明：2026-09-16 A/B 对照显示僵死态下 Off/On 与不开关
//!   统计上不可区分（0.61% vs 0.62%），且"执行成功"多命中启动预热缓存、
//!   未触达真实蓝牙栈；非僵死态的收益也从未测出（本机 6 天内仅执行 2 次：
//!   1 次与恢复无关，1 次无法与"时间自然恢复"区分）。
//!
//! 失败路径现在与其它失败一致：只按既有退避持续重连；僵死态的用户文案在
//! `ble::user_facing_connection_error` 内分流，不在本模块发生任何系统级动作。
//!
//! 历史证据与复算入口（保留供追溯）：`ATTRIBUTION.md`「2026-09-16 A/B 对照」、
//! `Testing/WindowsBleResourceRecovery.md`、`scripts/analyze-radio-recovery-ab.py`。
//!
//! 本模块现在只回答一个问题：**该错误码是否属于"蓝牙栈僵死态"**——用于失败
//! 分类日志与用户文案分流。

/// **系统蓝牙栈僵死**的错误码（`ble::ble_error_code` 的输出）。
///
/// 这些码表示 OS 侧 BLE 栈已耗尽/链路僵死，而不是"遥控器暂时不可达"。
/// 2026-09-16 实测：此态下无线电 Off/On 与提权 PnP 重启均无效（Off/On 报
/// "执行成功"143 次后连接只恢复 3 次 / 2.10%，与同事件内不开关的对照组
/// 0.62% 统计上不可区分）；Off/On 与 PnP 机制已于 2026-10-14 整体移除，
/// 此分类只用于文案与日志分流。
///
/// 证据与复算：`ATTRIBUTION.md`「2026-09-16 A/B 对照」、
/// `Testing/WindowsBleResourceRecovery.md`、`scripts/analyze-radio-recovery-ab.py`。
pub const STACK_EXHAUSTED_ERROR_CODES: [&str; 2] =
    ["windows_resource_exhausted", "winrt_operation_aborted"];

/// 该错误码是否属于"蓝牙栈僵死态"（单元测试覆盖）。
///
/// 只认常量里的码。**不要**在这里另写一份 `matches!` 字面量——两处定义会各自
/// 漂移：往 `STACK_EXHAUSTED_ERROR_CODES` 加码却忘了同步函数，测试照样通过，
/// 而分流会静默漏掉新码（2026-09-16 复查发现过这个隐患）。
pub fn is_stack_exhausted(error_code: &str) -> bool {
    STACK_EXHAUSTED_ERROR_CODES.contains(&error_code)
}

/// ---- 2026-11-16 修订：只对「音频吞吐塌陷」恢复关开无线电 ----
///
/// 上面 2026-10-14 的移除是对的：当时的触发条件是**连接失败次数**，而连接失败
/// 的成因太多（设备睡眠、配对失效、栈僵死），关开无线电对其中绝大多数无效，
/// 换来的是用户每小时被断一次全部蓝牙设备。
///
/// 本次恢复的触发条件是**另一个可量化的事实**：语音会话结束后算出的样本率。
/// 2026-11-16 现场：`voice_audio_rate` 连续 8 次落在 3.2–4.4 kHz（满速 16 kHz，
/// 只有 26%），遥控器申请的是 12.5 ms 快间隔但每个连接事件只上来 1 帧音频；
/// 关开蓝牙总开关后立刻回到 13.0–13.5 kHz。这条链路上「症状—动作—效果」
/// 三者都实测过，与 2026-09-16 A/B 里「连接失败后关开」不可区分的情形不同。
///
/// 仍然保留当初的全部顾虑，因此加了四道闸：连续 2 次、样本率 < 6000、
/// 单次会话 ≥ 1200 ms、10 分钟冷却，且每个进程生命周期最多 2 次。
pub const RADIO_CYCLE_MIN_STREAK: u32 = 2;
pub const RADIO_CYCLE_MIN_RATE_HZ: u32 = 6_000;
pub const RADIO_CYCLE_MIN_DURATION_MS: u64 = 1_200;
pub const RADIO_CYCLE_COOLDOWN_SECS: u64 = 600;
pub const RADIO_CYCLE_MAX_ATTEMPTS: u32 = 2;

/// 该不该自动关开一次蓝牙无线电（纯判定，单测覆盖）。
///
/// `since_last_cycle_secs`：距上次自动复位经过的秒数；`None` 表示本进程还没做过。
pub fn should_auto_cycle_radio(
    streak: u32,
    rate_hz: u32,
    duration_ms: u64,
    since_last_cycle_secs: Option<u64>,
    attempts: u32,
) -> bool {
    if attempts >= RADIO_CYCLE_MAX_ATTEMPTS {
        return false;
    }
    if streak < RADIO_CYCLE_MIN_STREAK
        || rate_hz >= RADIO_CYCLE_MIN_RATE_HZ
        || duration_ms < RADIO_CYCLE_MIN_DURATION_MS
    {
        return false;
    }
    match since_last_cycle_secs {
        None => true,
        Some(elapsed) => elapsed >= RADIO_CYCLE_COOLDOWN_SECS,
    }
}

/// 一次复位的结论，直接进日志与界面。
pub struct RadioCycleOutcome {
    pub result: &'static str,
    pub detail: String,
}

fn find_bluetooth_radio() -> Option<windows::Devices::Radios::Radio> {
    use std::future::IntoFuture;

    use windows::Devices::Radios::Radio;

    let operation = Radio::GetRadiosAsync().ok()?;
    let radios = futures::executor::block_on(operation.into_future()).ok()?;
    let count = radios.Size().ok()?;
    for index in 0..count {
        let radio = radios.GetAt(index).ok()?;
        if radio.Kind().ok()? == windows::Devices::Radios::RadioKind::Bluetooth {
            return Some(radio);
        }
    }
    None
}

fn set_radio_state(
    radio: &windows::Devices::Radios::Radio,
    state: windows::Devices::Radios::RadioState,
) -> windows::core::Result<()> {
    use std::future::IntoFuture;

    use windows::Devices::Radios::RadioAccessStatus;

    let operation = radio.SetStateAsync(state)?;
    let status = futures::executor::block_on(operation.into_future())?;
    if status != RadioAccessStatus::Allowed {
        return Err(windows::core::Error::from(windows::core::HRESULT(-2)));
    }
    Ok(())
}

/// 关 → 等 1.2 秒 → 开。**必须在独立线程调用**：内部阻塞等待，单次约 3–4 秒。
///
/// 走 `Windows.Devices.Radios` 的原因：提权助手的 `CM_Disable_DevNode` 在本机
/// 对 USB 蓝牙适配器返回 40（CR_ACCESS_DENIED，疑似终端安全软件拦 USB 设备禁用），
/// 而 `Radio.SetStateAsync` 正是系统设置里那个开关的路径，本机实测普通权限进程
/// 能拿到 `Allowed` 并真正切换状态。
pub fn cycle_bluetooth_radio() -> RadioCycleOutcome {
    use std::time::Duration;

    use windows::Devices::Radios::RadioState;
    use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};

    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
    }

    let Some(radio) = find_bluetooth_radio() else {
        return RadioCycleOutcome {
            result: "radio_not_found",
            detail: "未找到蓝牙无线电".to_owned(),
        };
    };
    let name = radio
        .Name()
        .map(|value| value.to_string())
        .unwrap_or_default();
    let before = radio.State().ok();

    if let Err(error) = set_radio_state(&radio, RadioState::Off) {
        // 失败也要确保射频没有被留在关闭状态。
        let _ = set_radio_state(&radio, RadioState::On);
        return RadioCycleOutcome {
            result: "off_failed",
            detail: format!("{name} off: {error:?}"),
        };
    }
    std::thread::sleep(Duration::from_millis(1_200));
    if let Err(error) = set_radio_state(&radio, RadioState::On) {
        return RadioCycleOutcome {
            result: "on_failed",
            detail: format!("{name} on: {error:?}"),
        };
    }
    // 给驱动与设备回连留一点时间，避免调用方立刻读到中间态。
    std::thread::sleep(Duration::from_millis(1_500));
    let after = radio.State().ok();
    RadioCycleOutcome {
        result: "cycled",
        detail: format!("{name} {before:?} -> {after:?}"),
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    /// 分流的唯一判据。这里**故意**只断言常量里的两个码 + 边界样本，不再抄
    /// 一份字面量：`is_stack_exhausted` 与 `STACK_EXHAUSTED_ERROR_CODES` 必须
    /// 同源，否则往常量加码会静默漏掉分流。
    #[test]
    fn exhausted_stack_is_recognised_and_everything_else_is_not() {
        for code in STACK_EXHAUSTED_ERROR_CODES {
            assert!(is_stack_exhausted(code), "{code} 应被判为僵死态");
        }

        // 近似但不相同的码不能误命中——它们是可恢复的正常失败。
        for code in [
            "windows_resource_exhausted_",
            "WINRT_OPERATION_ABORTED",
            "operation_aborted",
            "",
            "timeout",
        ] {
            assert!(!is_stack_exhausted(code), "{code} 不该被判为僵死态");
        }

        assert_eq!(STACK_EXHAUSTED_ERROR_CODES.len(), 2);
    }

    /// 自动复位的四道闸：连续 2 次、样本率 < 6000、会话 ≥ 1200 ms、10 分钟冷却、
    /// 每进程最多 2 次。这里逐条钉住，避免以后调参把闸门改没了。
    #[test]
    fn auto_cycle_needs_two_severe_degraded_sessions() {
        // 第一次降级不动手。
        assert!(!should_auto_cycle_radio(1, 4_000, 3_000, None, 0));
        // 连续第二次且足够严重 → 触发。
        assert!(should_auto_cycle_radio(2, 4_000, 3_000, None, 0));
        // 样本率不算太差（8 kHz）→ 不触发，避免正常抖动误伤。
        assert!(!should_auto_cycle_radio(2, 8_000, 3_000, None, 0));
        // 会话太短，样本率不可信。
        assert!(!should_auto_cycle_radio(2, 4_000, 900, None, 0));
        // 冷却期内不重复。
        assert!(!should_auto_cycle_radio(2, 4_000, 3_000, Some(60), 0));
        assert!(should_auto_cycle_radio(
            2,
            4_000,
            3_000,
            Some(RADIO_CYCLE_COOLDOWN_SECS),
            0
        ));
        // 每进程最多 2 次，之后只提示不再断设备。
        assert!(should_auto_cycle_radio(2, 4_000, 3_000, None, 1));
        assert!(!should_auto_cycle_radio(2, 4_000, 3_000, None, 2));
    }
}
