//! 配对守卫：强制「删除旧遥控器 → 重启 → 再添加新遥控器」（2026-11-16 现场）。
//!
//! 用户实测的顺序差异是本模块存在的唯一理由：
//!
//! | 顺序 | 结果 |
//! |---|---|
//! | 删除 → 重新添加 → 重启 | 仍不可用 |
//! | 删除 → **重启** → 重新添加 | 可用 |
//!
//! 也就是说，"卸载绑定"和"重新建立绑定"之间必须隔一次**干净启动**：删设备会把
//! 驱动栈弄脏，在脏状态里重新配对，新绑定本身就是坏的。此前我们一直是"删完立刻
//! 重配、不行才重启"，把重启放在了错误的位置。
//!
//! 判定不依赖用户自觉：删设备时落一个时间戳，每次启动用
//! `开机时刻 = 现在 - 系统运行时长` 反推本次开机时刻，**开机时刻晚于删除时刻**
//! 才算重启过。全程普通权限，不碰系统设置。

use std::path::PathBuf;

/// 守卫状态：`Idle` = 正常；`AwaitingReboot` = 已删除设备、必须重启后才能配对。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairGuard {
    Idle,
    AwaitingReboot,
}

impl PairGuard {
    /// 是否允许连接 / 扫描 / 设置当前设备 / 给自定义遥控器绑定设备。
    pub fn allows_pairing(self) -> bool {
        matches!(self, PairGuard::Idle)
    }
}

/// 把「本次开机时刻」从「现在」与「已运行时长」反推出来。
///
/// 拆成纯函数是为了能单测：真实运行时时长来自 `GetTickCount64`，
/// 而判据只关心两个时刻的先后。
pub fn boot_time_from_uptime(now_epoch: u64, uptime_secs: u64) -> u64 {
    now_epoch.saturating_sub(uptime_secs)
}

/// 依据「删除时刻」与「本次开机时刻」判定状态。
///
/// - 没有删除记录（`None`）→ `Idle`；
/// - 开机时刻 **晚于** 删除时刻 → 说明删除后重启过 → `Idle`；
/// - 否则 → `AwaitingReboot`。
///
/// 相等也按 `AwaitingReboot` 处理：宁可多要求一次重启，也不要放进一个可能脏的栈。
pub fn evaluate(removed_at: Option<u64>, boot_at: u64) -> PairGuard {
    match removed_at {
        None => PairGuard::Idle,
        Some(removed) if boot_at > removed => PairGuard::Idle,
        Some(_) => PairGuard::AwaitingReboot,
    }
}

fn marker_path() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| {
        let home = std::env::var("USERPROFILE").unwrap_or_else(|_| "C://".to_owned());
        format!("{home}\\AppData\\Local")
    });
    PathBuf::from(base).join("SayAll").join("pair-guard.txt")
}

/// 读取删除时刻（epoch 秒）。文件不存在或内容损坏都按"没有删除记录"处理。
pub fn read_removal() -> Option<u64> {
    let text = std::fs::read_to_string(marker_path()).ok()?;
    text.lines()
        .find_map(|line| line.strip_prefix("removed_at="))
        .and_then(|value| value.trim().parse::<u64>().ok())
}

/// 记录一次「用户删除了已配对的遥控器」。
pub fn note_removal(removed_at_epoch: u64) {
    let path = marker_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, format!("removed_at={removed_at_epoch}\n"));
}

/// 解除守卫（重启后判定通过时调用）。
pub fn clear() {
    let _ = std::fs::remove_file(marker_path());
}

/// 当前进程看到的守卫状态。
pub fn current(now_epoch: u64, uptime_secs: u64) -> PairGuard {
    evaluate(read_removal(), boot_time_from_uptime(now_epoch, uptime_secs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_removal_never_blocks() {
        assert_eq!(evaluate(None, 1_000), PairGuard::Idle);
    }

    #[test]
    fn blocks_until_a_boot_after_the_removal_happens() {
        // 删除发生在 2000；本次开机是 1500（删除之前就开机了）→ 必须重启。
        assert_eq!(evaluate(Some(2_000), 1_500), PairGuard::AwaitingReboot);
        // 开机时刻正好等于删除时刻，按必须重启处理（宁严不松）。
        assert_eq!(evaluate(Some(2_000), 2_000), PairGuard::AwaitingReboot);
        // 重启过（开机 2500 > 删除 2000）→ 放行。
        assert_eq!(evaluate(Some(2_000), 2_500), PairGuard::Idle);
    }

    #[test]
    fn boot_time_is_now_minus_uptime() {
        assert_eq!(boot_time_from_uptime(10_000, 400), 9_600);
        // 运行时长比当前时刻还大（时钟回拨）时不能下溢。
        assert_eq!(boot_time_from_uptime(100, 400), 0);
    }

    #[test]
    fn guard_gate_matches_state() {
        assert!(PairGuard::Idle.allows_pairing());
        assert!(!PairGuard::AwaitingReboot.allows_pairing());
    }
}
