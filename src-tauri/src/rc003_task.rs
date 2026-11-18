//! RC003 三键助手：计划任务的检测 / 授权 / 触发 / 停止。
//!
//! 产品流程（2026-11-12 用户定稿：**只授权一次**）：
//! * 首次打开：走一次 UAC 注册计划任务（用户点了「是」才算授权），然后触发助手；
//! * 之后每次打开：任务已注册且指向当前这份助手 → **直接触发，不再弹 UAC**。
//!   触发计划任务本身不需要提权，任务以授权时的最高权限运行由调度器负责。
//! * 需要重新授权的情况只有：任务不存在（首次 / 被系统清理）、任务指向别的助手
//!   （换安装位置、dev 与安装版切换、助手改名）、或判断不了（读不到任务定义）。
//! * 关闭：结束当前助手进程；任务保留在系统里，授权继续有效。
//! * 主程序每次启动：若开关仍开着（设置里 enabled），自动触发一次。
//!
//! 2026-10-03 ~ 2026-11-12 期间这里是"每次开启都重新授权"（`AUTHORIZATION_REQUIRED_
//! ON_EVERY_ENABLE = true`）。改回全量重授权只需把 `FORCE_REAUTHORIZE_ON_EVERY_ENABLE`
//! 置 true，但必须同步前端确认弹窗的文案与 `capture-switch.test.ts`。
//!
//! 任务名必须与助手侧一致：`hardware/RC003/helper/src/main.rs` 的
//! `SCHEDULED_TASK_NAME`。两侧各有一份常量，靠 `--selftest` 与本文件的
//! 测试分别钉住字面值；改任何一边都要同步另一边。2026-11-12 全仓改名时
//! **只改了源码没重编助手**，旧二进制按旧名注册成功、主程序按新名找不到，
//! 表现为「打开开关 → 授权通过 → 却提示拒绝访问」；助手侧已加遗留任务清理。

/// 与助手侧 `SCHEDULED_TASK_NAME` 必须逐字符一致。
pub const SCHEDULED_TASK_NAME: &str = "Free SayAll RC003 Helper";

/// 开关状态。`installed` = 任务在系统里（**不等于**授权有效：任务可能在，
/// 但指向的是别的助手，那种情况仍需重新授权）；
/// `enabled` = 用户意图（持久化在设置里，默认关闭）。
/// 两者是**不同的状态**：关闭开关只结束助手、任务保留，
/// 所以开关显示读 `enabled`，绝不能读 `installed`。
///
/// `authorization_required` = **这次打开开关会触发系统授权（UAC）**。
/// 2026-11-12 起不再恒为 true：任务可复用时为 false，前端据此跳过确认弹窗
/// 与 UAC 说明（见 [`FORCE_REAUTHORIZE_ON_EVERY_ENABLE`]）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStatus {
    pub installed: bool,
    pub authorization_required: bool,
    pub enabled: bool,
    pub helper_path: Option<String>,
    pub last_error: Option<String>,
}

/// 逃生阀：置 true 即退回 2026-10-03 的「每次开启都重新授权」。
///
/// 默认 false（2026-11-12 用户定稿「只授权一次」）。改这个值**必须**同步
/// 前端 `EnhancedCaptureConfirmDialog` 的文案与 `capture-switch.test.ts`
/// 里"每次开启都先弹确认"的断言，否则界面会承诺一次永远不发生的 UAC。
const FORCE_REAUTHORIZE_ON_EVERY_ENABLE: bool = false;

/// 定位助手 exe。按序尝试：
/// 1. 环境变量覆盖（验收 / 非标准安装位置）；
/// 2. 与主程序同目录（产品化布局：安装器把两者放在一起）——文件名按**本机原生架构**取
///    （x64 = `sayall-helper.exe`，arm64 = `sayall-helper-arm64.exe`，见 ADR 0003）；
/// 3. 开发布局：`target/debug/sayall-windows-app.exe` 向上找到仓库根，再进
///    `hardware/RC003/helper/target/[<triple>/]release/sayall-helper.exe`
///    （cargo 产物名恒为包名，arm64 只多一层 triple 目录）。
///
/// 架构由 [`sayall_windows::os_arch::native_arch`] 判定，**不**用编译期常量或
/// `PROCESSOR_ARCHITECTURE`——x64 主程序在 ARM64 上运行时那两者都报 x64（Issue #206）。
pub fn locate_helper_exe() -> Option<std::path::PathBuf> {
    locate_helper_exe_for(sayall_windows::os_arch::native_arch())
}

pub fn locate_helper_exe_for(
    arch: sayall_windows::os_arch::NativeArch,
) -> Option<std::path::PathBuf> {
    if let Ok(path) = std::env::var("SAYALL_RC003_HELPER") {
        let path = std::path::PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    let exe = std::env::current_exe().ok()?;
    if let Some(dir) = exe.parent() {
        let candidate = dir.join(arch.helper_file_name());
        if candidate.is_file() {
            return Some(candidate);
        }
        // 开发布局：沿着父目录向上找仓库根。
        let mut dir = dir.to_path_buf();
        for _ in 0..4 {
            dir = dir.parent()?.to_path_buf();
            let mut candidate = dir
                .join("hardware")
                .join("RC003")
                .join("helper")
                .join("target");
            if let Some(triple) = dev_target_triple(arch) {
                candidate = candidate.join(triple);
            }
            let candidate = candidate.join("release").join("sayall-helper.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// 该架构的 cargo `--target` 目录名。x64 用默认 target 目录（无 triple），
/// arm64 必须显式 `--target aarch64-pc-windows-msvc`（交叉编译），因此多一层。
fn dev_target_triple(arch: sayall_windows::os_arch::NativeArch) -> Option<&'static str> {
    match arch {
        sayall_windows::os_arch::NativeArch::Arm64 => Some("aarch64-pc-windows-msvc"),
        _ => None,
    }
}

/// 增强捕获在本机的可用性（IPC `get_capture_support` 的返回体）。
///
/// 为什么要有它：ARM64 上"打开开关 → 永远停在正在启动"曾经没有任何解释（Issue #206），
/// 用户与排查者都只能看到 25 秒后的一条失败日志。这里把结论前移：本机架构有没有对应的
/// 助手 / Gadget，界面据此置灰并说明原因。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureSupport {
    /// 本机**原生**架构：`x64` / `arm64` / `unknown`（探测失败或不受支持的原生架构）。
    pub native_arch: String,
    /// 本架构应当存在的助手文件名（诊断用；界面不显示）。
    pub helper_expected: String,
    pub available: bool,
    /// `arch_unsupported` = 本机原生架构不属于 x64 / arm64；
    /// `helper_missing` = 架构对得上，但安装包里没有对应的助手（载荷缺失）。
    pub reason: Option<String>,
}

/// 可用性判定（纯函数，便于单测覆盖两种不可用原因）。
pub(crate) fn capture_support_decision(
    arch: sayall_windows::os_arch::NativeArch,
    helper_found: bool,
) -> (bool, Option<&'static str>) {
    if arch.is_explicitly_unsupported() {
        return (false, Some("arch_unsupported"));
    }
    if !helper_found {
        return (false, Some("helper_missing"));
    }
    (true, None)
}

/// 本机增强捕获的可用性。探测失败（`unknown`）时沿用 x64 路径与既有行为：
/// 一次探测失败不足以把功能判死。
pub fn capture_support() -> CaptureSupport {
    let arch = sayall_windows::os_arch::native_arch();
    let helper_found = locate_helper_exe_for(arch).is_some();
    let (available, reason) = capture_support_decision(arch, helper_found);
    CaptureSupport {
        native_arch: arch.as_str().to_string(),
        helper_expected: arch.helper_file_name().to_string(),
        available,
        reason: reason.map(|value| value.to_string()),
    }
}

#[cfg(test)]
thread_local! {
    /// 测试专用覆盖：只替换**本线程**读到的任务文件路径，不碰 SystemRoot
    /// 或其他进程级环境变量。生产构建编译期就删掉。
    static TASK_FILE_OVERRIDE: std::cell::RefCell<Option<std::path::PathBuf>> =
        std::cell::RefCell::new(None);
}

/// 计划任务 XML 的路径。任务文件是否存在与 `schtasks /query` 的成功/失败
/// 是同一件事，但前者不需要启动子进程。
fn task_file_path() -> std::path::PathBuf {
    #[cfg(test)]
    {
        if let Some(path) = TASK_FILE_OVERRIDE.with(|slot| slot.borrow().clone()) {
            return path;
        }
    }
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    std::path::Path::new(&root)
        .join("System32")
        .join("Tasks")
        .join(SCHEDULED_TASK_NAME)
}

/// 任务文件是否存在。`None` = 判断不了（权限 / IO 错误），不是"不存在"。
///
/// 为什么不用 `Path::is_file()`：它把"无权限"和"不存在"都压成 `false`，
/// 让调用方无法区分「明确不存在」与「需要回退查询」。
fn task_file_present_at(path: &std::path::Path) -> Option<bool> {
    match std::fs::metadata(path) {
        Ok(metadata) => Some(metadata.is_file()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Some(false),
        Err(_) => None,
    }
}

fn task_file_present() -> Option<bool> {
    task_file_present_at(&task_file_path())
}

/// 任务是否已注册（= 用户已授权）。
///
/// 三级判据，从便宜到贵：
/// 1. 任务文件存在性——`Some(_)` 时直接采信（只有管理员权限才读得到）；
/// 2. COM 读任务定义——普通权限下唯一可用的通道（2026-11-12 本机实测
///    `C:\Windows\System32\Tasks` 对普通用户 `Permission denied`）；
/// 3. `schtasks /query` 退出码——只在 COM 明确"这次问不出来"（服务不可用、
///    关机中）时兜底，不能据它判"任务缺失"。
///
/// **每秒轮询的 `status()` 不调用本函数**：关机时新进程初始化会失败
/// （`0xc0000142`），`schtasks.exe` 会弹系统错误框并阻塞关机——2026-11-10
/// 用户反馈的关机弹窗即由此产生。
pub fn task_installed() -> bool {
    if task_file_present() == Some(true) {
        return true;
    }
    match probe_task_cached(false) {
        TaskProbe::Defined(_) => true,
        TaskProbe::Absent => false,
        TaskProbe::Unavailable => {
            schtasks(&["/query", "/tn", SCHEDULED_TASK_NAME]).unwrap_or(false)
        }
    }
}

/// COM 探测计划任务定义的结果。
///
/// `Absent` 与 `Unavailable` **必须分开**：前者是"系统里确实没有这个任务"
/// （据此回落开关是正确的），后者只是"这次没问出来"（服务不可用、关机中），
/// 不能当成任务缺失，否则一次探测抖动就会把用户的开关静默拉低。
#[derive(Clone)]
enum TaskProbe {
    Defined(String),
    Absent,
    Unavailable,
}

/// 通过 COM 读取计划任务的完整 XML 定义。
///
/// **为什么必须走 COM**：`C:\Windows\System32\Tasks` 对普通用户不可列举、也不可读
/// （2026-11-12 本机实测 `Permission denied`），所以 `task_file_present()` 在普通
/// 权限下恒为 `None` —— `status().installed` 长期只能当诊断值、"任务重装生效了吗"
/// 长期靠退出码兜底、对账里"任务指向别的助手"永远判不出来，根子都在这里。COM
/// 查询不需要管理员权限、不创建子进程，也不受"拦截非提权进程启动系统命令行工具"
/// 的影响。
#[cfg(windows)]
fn probe_task_via_com() -> TaskProbe {
    use windows::core::{BSTR, GUID};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use windows::Win32::System::TaskScheduler::ITaskService;
    use windows::Win32::System::Variant::VARIANT;

    const CLSID_TASK_SCHEDULER: GUID = GUID::from_u128(0x0F87369F_A4E5_4CFC_BD3E_73E6154572DD);
    // HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND)：任务文件夹里没有这个名字。
    const HRESULT_NOT_FOUND: i32 = 0x8007_0002u32 as i32;

    // 可能已被对账线程初始化过；MTA 允许重复初始化。
    let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    let outcome = (|| -> TaskProbe {
        let Ok(service) =
            (unsafe { CoCreateInstance(&CLSID_TASK_SCHEDULER, None, CLSCTX_INPROC_SERVER) })
        else {
            return TaskProbe::Unavailable;
        };
        let service: ITaskService = service;
        let empty = VARIANT::default();
        if unsafe { service.Connect(&empty, &empty, &empty, &empty) }.is_err() {
            return TaskProbe::Unavailable;
        }
        let Ok(folder) = (unsafe { service.GetFolder(&BSTR::from("\\")) }) else {
            return TaskProbe::Unavailable;
        };
        let task = match unsafe { folder.GetTask(&BSTR::from(SCHEDULED_TASK_NAME)) } {
            Ok(task) => task,
            Err(error) if error.code().0 == HRESULT_NOT_FOUND => return TaskProbe::Absent,
            Err(_) => return TaskProbe::Unavailable,
        };
        match unsafe { task.Xml() } {
            Ok(xml) => TaskProbe::Defined(xml.to_string()),
            Err(_) => TaskProbe::Unavailable,
        }
    })();
    if initialized {
        unsafe { CoUninitialize() };
    }
    outcome
}

#[cfg(not(windows))]
fn probe_task_via_com() -> TaskProbe {
    TaskProbe::Unavailable
}

/// 任务 `<Command>` 的**完整路径**（与 [`extract_task_command`] 的区别：那个只取
/// 文件名，因为文件名可以进日志、完整路径是个人路径不进日志）。这里只用于内存
/// 比对，绝不写日志。
fn extract_task_command_full(xml: &str) -> Option<String> {
    let start = xml.find("<Command>")? + "<Command>".len();
    let end = xml[start..].find("</Command>")? + start;
    let raw = xml[start..end].trim().trim_matches('"').trim();
    if raw.is_empty() {
        None
    } else {
        Some(raw.to_string())
    }
}

/// 任务定义是否**可以直接复用**（这次打开开关不必重新授权）。纯函数，便于单测。
///
/// 判据只有一条：任务指向的可执行文件就是 `helper` 这一份。路径一致意味着
/// "上次授权注册的就是它"，触发它不需要提权；路径不一致（换安装目录、dev 与
/// 安装版切换、助手改名）一律按"需要授权"处理——宁可多弹一次 UAC，也不跳过
/// 授权步骤。
fn task_xml_reusable_for(xml: &str, helper: &std::path::Path) -> bool {
    let Some(target) = extract_task_command_full(xml) else {
        return false;
    };
    std::path::Path::new(&target)
        .to_string_lossy()
        .eq_ignore_ascii_case(&helper.to_string_lossy())
}

/// 上一次 COM 探测的结果。
///
/// `status()` 由按键页 / 连接页每秒调用一次，不能每次都跨进程问
/// `Schedule.Service`；任务只在「授权安装 / 被删除」时变化，那两处都会主动
/// 以 `reprobe = true` 重探。
static TASK_PROBE_CACHE: std::sync::Mutex<Option<TaskProbe>> = std::sync::Mutex::new(None);

fn probe_task_cached(reprobe: bool) -> TaskProbe {
    let mut guard = TASK_PROBE_CACHE
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if reprobe || guard.is_none() {
        *guard = Some(probe_task_via_com());
    }
    guard.clone().unwrap_or(TaskProbe::Unavailable)
}

/// 重新探测"任务可复用"（顺带刷新任务定义缓存）。启动、授权成功、任务被清理后调用。
pub fn refresh_task_reusable() -> bool {
    let TaskProbe::Defined(xml) = probe_task_cached(true) else {
        return false;
    };
    locate_helper_exe()
        .map(|helper| task_xml_reusable_for(&xml, &helper))
        .unwrap_or(false)
}

/// 这次打开开关会不会弹系统授权（UAC）——**界面提示用**，允许读缓存。
///
/// 判定权威在 `enable_capture()`（那里强制重探）。这里只负责让界面提前知道
/// "要不要先弹确认框"，因此首次调用会做一次 COM 探测（毫秒级），之后全走缓存。
pub fn authorization_required_now() -> bool {
    if FORCE_REAUTHORIZE_ON_EVERY_ENABLE {
        return true;
    }
    let TaskProbe::Defined(xml) = probe_task_cached(false) else {
        // 任务不存在，或这次问不出来：都需要走一次授权（后者重装也无害）。
        return true;
    };
    locate_helper_exe()
        .map(|helper| !task_xml_reusable_for(&xml, &helper))
        .unwrap_or(true)
}

/// 进程内触发计划任务（Task Scheduler COM）。
///
/// 2026-11-12 现场：`schtasks.exe` 子进程在应用里 3~5ms 内创建失败（同一命令行
/// 从 shell 可成功），"开启全按键支持"因此永远失败。COM 走进程内 RPC，不创建
/// 子进程，也不受"拦截非提权进程启动系统命令行工具"这类策略影响；主程序仍是
/// 普通用户权限（任务以授权时的最高权限运行由调度器负责，不需要我们提权）。
#[cfg(windows)]
fn task_trigger_com() -> Result<(), String> {
    use windows::core::{BSTR, GUID};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use windows::Win32::System::TaskScheduler::{ITaskService, TASK_STATE_RUNNING};
    use windows::Win32::System::Variant::VARIANT;

    // 公开文档值：TaskScheduler CoClass / ITaskService 接口。
    const CLSID_TASK_SCHEDULER: GUID = GUID::from_u128(0x0F87369F_A4E5_4CFC_BD3E_73E6154572DD);

    // spawn_blocking / 对账线程未必初始化过 COM；MTA 允许重复初始化。
    let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    let outcome = (|| -> Result<(), String> {
        let service: ITaskService =
            unsafe { CoCreateInstance(&CLSID_TASK_SCHEDULER, None, CLSCTX_INPROC_SERVER) }
                .map_err(|error| format!("创建任务计划服务失败：{error}"))?;
        let empty = VARIANT::default();
        unsafe { service.Connect(&empty, &empty, &empty, &empty) }
            .map_err(|error| format!("连接任务计划服务失败：{error}"))?;
        let folder = unsafe { service.GetFolder(&BSTR::from("\\")) }
            .map_err(|error| format!("打开任务文件夹失败：{error}"))?;
        let task = unsafe { folder.GetTask(&BSTR::from(SCHEDULED_TASK_NAME)) }
            .map_err(|error| format!("找不到计划任务 {SCHEDULED_TASK_NAME}：{error}"))?;
        unsafe { task.Run(&empty) }.map_err(|error| format!("运行计划任务失败：{error}"))?;
        // 成功判据用**状态读回**，而不是"调用没报错"（AGENTS.md 外部调用判据）。
        match unsafe { task.State() } {
            Ok(state) => sayall_windows::gatt_note(format!(
                "rc003 feature=enhanced-capture action=task_trigger phase=state_read \
                 state={} state_code={}",
                if state == TASK_STATE_RUNNING {
                    "running"
                } else {
                    "not_running"
                },
                state.0
            )),
            Err(error) => sayall_windows::gatt_note(format!(
                "rc003 feature=enhanced-capture action=task_trigger phase=state_read \
                 terminal_result=failed detail={error}"
            )),
        }
        Ok(())
    })();
    if initialized {
        unsafe { CoUninitialize() };
    }
    outcome
}

/// 触发助手（免提权）。助手带 `--follow-app`，会在主程序关闭后自行退出。
///
/// 优先进程内 COM；失败才回退 `schtasks.exe`（老环境/COM 不可用时保底）。
pub fn task_trigger() -> Result<(), String> {
    #[cfg(windows)]
    {
        match task_trigger_com() {
            Ok(()) => return Ok(()),
            Err(error) => {
                sayall_windows::gatt_note(format!(
                    "rc003 feature=enhanced-capture action=task_trigger phase=com_failed \
                     fallback=schtasks detail={error}"
                ));
            }
        }
    }
    run_schtasks(&["/run", "/tn", SCHEDULED_TASK_NAME], "触发")
}

/// 结束助手（免提权尝试；被拒时由调用方走提权路径）。
/// **不移除任务**——授权保留，这是"只弹一次 UAC"的一部分。
///
/// 同 [`task_trigger`]：优先进程内 COM，失败回退 `schtasks.exe`。
pub fn task_stop() -> Result<(), String> {
    #[cfg(windows)]
    {
        match task_stop_com() {
            Ok(()) => return Ok(()),
            Err(error) => {
                sayall_windows::gatt_note(format!(
                    "rc003 feature=enhanced-capture action=task_stop phase=com_failed \
                     fallback=schtasks detail={error}"
                ));
            }
        }
    }
    run_schtasks(&["/end", "/tn", SCHEDULED_TASK_NAME], "停止")
}

#[cfg(windows)]
fn task_stop_com() -> Result<(), String> {
    use windows::core::{BSTR, GUID};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use windows::Win32::System::TaskScheduler::ITaskService;

    const CLSID_TASK_SCHEDULER: GUID = GUID::from_u128(0x0F87369F_A4E5_4CFC_BD3E_73E6154572DD);

    let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    let outcome = (|| -> Result<(), String> {
        let service: ITaskService =
            unsafe { CoCreateInstance(&CLSID_TASK_SCHEDULER, None, CLSCTX_INPROC_SERVER) }
                .map_err(|error| format!("创建任务计划服务失败：{error}"))?;
        let empty = windows::Win32::System::Variant::VARIANT::default();
        unsafe { service.Connect(&empty, &empty, &empty, &empty) }
            .map_err(|error| format!("连接任务计划服务失败：{error}"))?;
        let folder = unsafe { service.GetFolder(&BSTR::from("\\")) }
            .map_err(|error| format!("打开任务文件夹失败：{error}"))?;
        let task = unsafe { folder.GetTask(&BSTR::from(SCHEDULED_TASK_NAME)) }
            .map_err(|error| format!("找不到计划任务 {SCHEDULED_TASK_NAME}：{error}"))?;
        unsafe { task.Stop(0) }.map_err(|error| format!("停止计划任务失败：{error}"))?;
        sayall_windows::gatt_note(
            "rc003 feature=enhanced-capture action=task_stop phase=completed \
             terminal_result=passed via=com"
                .to_owned(),
        );
        Ok(())
    })();
    if initialized {
        unsafe { CoUninitialize() };
    }
    outcome
}

/// 注册任务：弹**一次** UAC（PowerShell `Start-Process -Verb RunAs` 等到装完），
/// GUI 子系统的主程序没有自己的控制台，而 `schtasks` / `powershell` 都是
/// 控制台程序——不加这个标志，每次调用都会分配一个新控制台窗口并立刻
/// 消失（按键页每秒轮询任务状态 = **每秒闪一次黑窗**，2026-09-24 真机复现）。
/// dev 模式看不出来：那时主程序带着父控制台，子进程直接继承。
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn silent_command(program: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    let mut command = std::process::Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

/// 弹一次 UAC 完成任务注册：**直接 ShellExecuteExW(runas) 助手本体**，
/// 不再经由 PowerShell。
///
/// 为什么必须换掉 PowerShell（2026-09-24 两轮真机证伪）：PowerShell 5.1
/// 对「UAC 被取消」的报错是**非终止性错误**，`-Command` 退出码 0——
/// 连 `-ErrorAction Stop` 都可能不产生预期效果（两次 passed 且任务
/// Date 未变）。「用户拒绝」与「用户允许」从此无法区分。
/// ShellExecuteExW 则给出操作系统级的确定信号：
///   用户点否 / 叉掉 UAC 窗口 → 返回 FALSE + GetLastError = ERROR_CANCELLED
///   用户允许 → hProcess 有效，等助手退出码（0 = 安装成功）
pub fn task_install_elevated(helper: &std::path::Path) -> Result<(), String> {
    if system_is_shutting_down() {
        return Err("系统正在关机，已取消计划任务授权。".to_string());
    }
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{ERROR_CANCELLED, HANDLE};
    use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
    use windows::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };

    let file: Vec<u16> = std::ffi::OsStr::new(helper)
        .encode_wide()
        .chain(Some(0))
        .collect();
    // lpParameters 是单个字符串；助手自 2026-10-04 起是 **GUI 子系统**程序：
    // 提权 / 计划任务拉起时不创建任何控制台窗口。`--hide-window` 保留为兼容
    // 参数（旧版助手用它隐藏自己的黑框；新版接受但无窗口可隐藏）。
    let parameters: Vec<u16> = "--install-task --hide-window"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let verb: Vec<u16> = "runas\0".encode_utf16().collect();

    let mut sei = SHELLEXECUTEINFOW::default();
    sei.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    // NOCLOSEPROCESS：要 hProcess 等助手退出码；FLAG_NO_UI：失败不要
    // Shell 自己弹 UI，错误归我们表达。
    sei.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_FLAG_NO_UI;
    sei.lpVerb = PCWSTR(verb.as_ptr());
    sei.lpFile = PCWSTR(file.as_ptr());
    sei.lpParameters = PCWSTR(parameters.as_ptr());
    // SW_HIDE 双保险：新版助手本就没有窗口；万一拉起的是尚未升级的旧版
    // 控制台助手，也让它隐藏启动，而不是闪一下黑框。
    sei.nShow = 0;

    if let Err(error) = unsafe { ShellExecuteExW(&mut sei) } {
        // 用户点「否」或叉掉 UAC 窗口都走这里：ERROR_CANCELLED (1223)。
        let cancelled = error.code() == windows::core::HRESULT::from_win32(ERROR_CANCELLED.0);
        sayall_windows::gatt_note(format!(
            "rc003 feature=enhanced-capture action=elevated_install outcome={} hresult={:?}",
            if cancelled {
                "cancelled_by_user"
            } else {
                "shell_execute_failed"
            },
            error.code()
        ));
        return Err(if cancelled {
            "授权未完成（UAC 被取消）。三键捕获保持关闭，可再次打开重试。".to_string()
        } else {
            format!("提权安装失败：{error}")
        });
    }
    let process = sei.hProcess;
    if process.is_invalid() || process == HANDLE::default() {
        return Err("提权流程没有返回助手进程句柄（安装未执行）".to_string());
    }
    unsafe {
        let _ = WaitForSingleObject(process, INFINITE);
    }
    let mut exit_code = 0u32;
    let _ = unsafe { GetExitCodeProcess(process, &mut exit_code) };
    // 退出码分流（2026-10-13，issue #206 第三轮报障人建议）：
    // 14 = 助手在`/rl highest` 任务路径上仍拿不到 SeDebugPrivilege（见助手
    // `enable_se_debug` 的退出码 14）——"再试一次"通常无效，日志里必须能一步分流。
    // 注意边界：捕获运行的退出码走计划任务，应用看不到；这里能覆盖的是应用直接
    // 等到的那些次数（--install-task 等）。
    if exit_code != 0 {
        let reason = if exit_code == 14 {
            "se_debug_unavailable"
        } else {
            "task_install_failed"
        };
        sayall_windows::gatt_note(format!(
            "rc003 feature=enhanced-capture action=elevated_install outcome=helper_exit exit_code={exit_code} reason={reason} retryable={}",
            exit_code != 14
        ));
    } else {
        sayall_windows::gatt_note(
            "rc003 feature=enhanced-capture action=elevated_install outcome=helper_exit exit_code=0"
                .to_owned(),
        );
    }
    if exit_code != 0 {
        return Err(if exit_code == 14 {
            // 原文只进诊断日志；界面把含"授权"的句子映射成用户语言。
            "系统授权没有生效（助手没有获得所需权限）。".to_string()
        } else {
            format!("授权未完成（助手注册计划任务失败，退出码 {exit_code}）。可再次打开重试。")
        });
    }
    Ok(())
}

/// 开关打开。
///
/// 语义（2026-11-12 用户定稿）：**只在需要时授权一次**。任务已注册且指向当前这份
/// 助手时直接触发，不弹 UAC；首次开启、助手换了位置/版本、任务是旧形态或读不到
/// 任务定义时才重走一次提权安装，成功之后一直有效。
/// 失败原样返回（调用方保持开关原状态）。
pub fn enable_capture() -> Result<(), String> {
    // 本机没有对应架构的载荷时直接拒绝（界面已按 `get_capture_support` 置灰，这里是兜底）：
    // 放它走下去只会白弹一次系统授权窗口，然后必然连不上（Issue #206）。
    let support = capture_support();
    if !support.available {
        sayall_windows::gatt_note(format!(
            "rc003 feature=enhanced-capture action=enable phase=completed terminal_result=failed \
             reason={} native_arch={} helper_expected={} retryable=false",
            support.reason.as_deref().unwrap_or("unavailable"),
            support.native_arch,
            support.helper_expected
        ));
        return Err(match support.reason.as_deref() {
            Some("arch_unsupported") => "这台电脑暂不支持全按键支持。".to_string(),
            _ => "安装包缺少本机需要的组件，全按键支持暂不可用；请重新安装本版本。".to_string(),
        });
    }
    // 清掉可能残留的停用信号：否则"关开关（没助手在跑）→ 立刻再开"时，
    // 新助手一启动就见到信号当场退出，开关开着却永远连不上。
    let _ = std::fs::remove_file(stop_signal_path());
    let helper = locate_helper_exe().ok_or_else(|| {
        format!(
            "找不到 {}（检查安装布局，或设置 SAYALL_RC003_HELPER 指向它）",
            support.helper_expected
        )
    })?;
    // 2026-11-12 用户定稿「只授权一次」：任务已注册、且指向的正是当前这份助手时，
    // 直接触发即可（触发计划任务本身不需要提权，最高权限由调度器按授权时的
    // 注册信息负责）。只有首次开启、助手换了位置/版本、任务是旧形态，或读不到
    // 任务定义时才重走 UAC。
    //
    // 这里**强制重探**（`refresh_task_reusable`）而不是读 `authorization_required_now()`
    // 的缓存：用户已经点了开关，多花一次毫秒级 COM 往返换取"这次判定一定新鲜"，
    // 免得启动时一次失败的探测让以后每次开启都白弹一次 UAC。
    let needs_authorization = FORCE_REAUTHORIZE_ON_EVERY_ENABLE || !refresh_task_reusable();
    if needs_authorization {
        task_install_elevated(&helper)?;
        // 重授权成功的判据**不能是退出码**：助手退出 0 只说明它自己没报错，
        // 不代表任务真的建成了、更不代表它指向这份助手。
        //
        // 2026-11-12 教训：原先的硬判据是"任务 XML 的 <Date> 变了"，但
        // `C:\Windows\System32\Tasks` 对普通用户不可读，`task_registered_at()`
        // 在本机**恒为 None**，于是那个 match 一律落到"信任退出码"。当天旧助手
        // （改名前的二进制）按旧任务名 `SayAll RC003 Helper` 注册成功、退出 0，
        // 主程序按新名找不到任务 → 用户看到的是"拒绝访问"，排查方向被彻底带偏。
        // 现在改成读**任务定义本身**（COM，普通权限可读）：任务存在且指向当前
        // 这份助手，才算授权真的完成。
        if !refresh_task_reusable() {
            // 只记文件名，不记完整路径（个人路径不进日志）。
            let probe = probe_task_cached(false);
            let (stage, actual) = match &probe {
                TaskProbe::Defined(xml) => (
                    "verify_task_target",
                    extract_task_command_full(xml)
                        .and_then(|path| {
                            std::path::Path::new(&path)
                                .file_name()
                                .map(|name| name.to_string_lossy().to_string())
                        })
                        .unwrap_or_else(|| "none".to_string()),
                ),
                TaskProbe::Absent => ("verify_task_present", "none".to_string()),
                TaskProbe::Unavailable => ("verify_task_readable", "unreadable".to_string()),
            };
            sayall_windows::gatt_note(format!(
                "rc003 feature=enhanced-capture action=enable phase=completed terminal_result=failed \
                 stage={stage} expected={} actual={actual}",
                support.helper_expected
            ));
            // 三种失败给三种说法：系统里没有任务 / 任务指向别处 / 读不出来。
            // 原文一律进日志，界面只需要一句能指导下一步动作的话。
            return Err(match stage {
                "verify_task_present" => {
                    "授权没有完成：系统里没有这个计划任务。三键捕获保持关闭，可再次打开重试。"
                        .to_string()
                }
                "verify_task_target" => {
                    "授权没有完成：计划任务指向的不是当前这份组件（可能刚升级过）。三键捕获保持关闭，可再次打开重试。"
                        .to_string()
                }
                _ => {
                    "授权已通过，但读不到计划任务定义，无法确认。三键捕获保持关闭，可再次打开重试。"
                        .to_string()
                }
            });
        }
        // 授权刚做完、校验已通过：缓存已由 refresh_task_reusable 写好，
        // 用户下次拨开关时不会再弹 UAC。
    }
    let result = task_trigger();
    if result.is_ok() {
        clear_reauth_marker();
        // 触发成功也是"任务可用"的实证（尤其是重装路径没走到的情况）。
        refresh_task_reusable();
    } else if let Err(error) = &result {
        // 触发失败的原文必须进日志：界面只显示通用句，现场无法一步定位。
        sayall_windows::gatt_note(format!(
            "rc003 feature=enhanced-capture action=enable phase=completed terminal_result=failed \
             stage=task_trigger detail={error}"
        ));
    }
    result
}

/// 开关关闭：结束助手，**不移除任务**——授权保留，下次开启不再弹 UAC
/// （2026-11-12 起的「只授权一次」，见 [`FORCE_REAUTHORIZE_ON_EVERY_ENABLE`]）。
///
/// 主程序是普通权限，**杀不掉提权助手**；`schtasks /end` 只能停"当前任务
/// 实例"——若实例已换（任务重装、改名后旧进程挂着），它完全落空，
/// 旧助手就继续映射（2026-09-24 真机复现）。所以真正的停止通道是**文件
/// 信号**：写一个助手可见的标记，助手轮询到就自行退出（它有权限删自己）。
/// task_stop 保留为兜底（对恰好还是当前实例的任务有效）。
pub fn disable_capture() -> Result<(), String> {
    if let Some(parent) = stop_signal_path().parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(stop_signal_path(), b"stop")
        .map_err(|error| format!("写停用信号失败：{error}"))?;
    task_stop()
}

/// 停用信号文件：`%LOCALAPPDATA%\SayAll\rc003-capture-stop`。
/// 与助手侧 `app_stop_signal_path()` 的路径约定**必须逐字符一致**
/// （两侧各有一份常量，靠测试钉住——见 rc003_task 测试与助手自检）。
fn stop_signal_path() -> std::path::PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| {
        let home = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\".to_string());
        format!("{home}\\AppData\\Local")
    });
    std::path::PathBuf::from(base)
        .join("SayAll")
        .join("rc003-capture-stop")
}

/// 重授权标记：`%LOCALAPPDATA%\SayAll\rc003-reauth-required`。
///
/// **为什么需要**：授权（计划任务）由提权进程创建，普通权限的卸载器
/// **删不掉它**（真机实测：schtasks /delete 静默失败）——卸载时
/// 只能写这个标记作为「授权已应撤销」的凭证。应用启动见到标记就把
/// 开关回落为关闭；用户重新打开时 enable 会按「任务还能不能用」重新授权
/// （2026-11-12 起不是无条件弹 UAC：任务仍在且指向当前助手就直接复用），
/// 成功后清除标记。
///
/// 内容与写入时机（2026-09-28 修订）：**只有真卸载**（NSIS 把卸载器
/// 拷进临时目录跑，`$EXEDIR != $INSTDIR`）才写，内容为
/// `uninstalled=<卸载时刻 GetTickCount>`；升级安装原位调用旧卸载器
/// 不写。带 `uninstalled=` 前缀的新格式**任何安装都不删**（重装凭证）；
/// 旧格式（裸 tick / 字面 `reauth`）是历史版本卸载器产物，安装器侧
/// 只做过渡清理。与安装器侧写入的路径**必须逐字符一致**。
pub fn reauth_marker_path() -> std::path::PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| {
        let home = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\".to_string());
        format!("{home}\\AppData\\Local")
    });
    std::path::PathBuf::from(base)
        .join("SayAll")
        .join("rc003-reauth-required")
}

/// 是否处于「需重新授权」状态（安装/升级后的首次开启前）。
pub fn reauth_required() -> bool {
    reauth_marker_path().exists()
}

fn clear_reauth_marker() {
    let _ = std::fs::remove_file(reauth_marker_path());
}

/// 任务注册的可执行文件名（`<Exec><Command>` 的最后一段路径）。
/// **只取文件名**：完整路径是个人路径，不进日志。
fn extract_task_command(xml: &str) -> Option<String> {
    let start = xml.find("<Command>")? + "<Command>".len();
    let end = xml[start..].find("</Command>")? + start;
    let raw = xml[start..end].trim().trim_matches('"');
    let name = raw.rsplit(|c| c == '\\' || c == '/').next()?.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// 计划任务指向的助手是不是**本架构**那一份（纯函数，便于单测）。
/// `None` = 拿不到命令：那属于"任务缺失"另一条判据，这里不表态。
pub(crate) fn task_target_verdict(command: Option<&str>, expected: &str) -> Option<bool> {
    Some(command?.trim().eq_ignore_ascii_case(expected))
}

/// 计划任务当前指向的助手是否与本机架构相符。
///
/// **为什么需要**（2026-10-07 报障人 ARM64 实测发现）：覆盖安装会保留旧版注册的计划任务，
/// 任务目标写的是**当时那个助手**的路径。从"只有 x64"的版本升到带 arm64 载荷的版本后，
/// ARM64 机器上的任务仍指向 `sayall-helper.exe`：启动自动拉起会去跑 x64 助手，架构闸门
/// 会拦下它（文案清楚），但界面要等约 25 秒重试完才失败，用户会以为功能坏了。
/// 所以启动对账先比一次：不一致就回落开关，等用户拨一次开关重建任务——重装任务要提权，
/// 不在启动时擅自弹 UAC。
pub fn task_target_matches(helper_file_name: &str) -> Option<bool> {
    // 任务不存在 / 读不到时 `task_installed()` 那条判据已经会处理，这里不重复表态。
    let TaskProbe::Defined(xml) = probe_task_cached(false) else {
        return None;
    };
    task_target_verdict(extract_task_command(&xml).as_deref(), helper_file_name)
}

/// 读取任务 XML 里的布尔设置项（纯函数）。`None` = 元素缺失或不是 true/false。
fn extract_task_bool(xml: &str, tag: &str) -> Option<bool> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    match xml[start..end].trim().to_ascii_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// 任务电源策略判据（纯函数）：`DisallowStartIfOnBatteries` 与
/// `StopIfGoingOnBatteries` 两项都为 false 才算通过。任一项缺失 → `None`
/// （不表态，交给别的判据）。
///
/// **为什么需要**（2026-10-13，issue #206 第三轮报障人实测）：旧版助手用
/// `schtasks /create` 的命令行形态注册任务，它会**固定**写出这两项为 true，
/// 而命令行没有任何开关能改。电池供电时调度器**静默拦截**任务动作：
/// LastRunTime 照更新、上次结果 0、进程不出现——助手从此起不来，应用只能看到
/// "助手没连上"并无限重试（报障人用对照实验定位：同一命令手写、不带电源策略的
/// 任务立刻能跑）。新版本改用 XML 注册、两项显式置 false（见助手 `task_xml`）；
/// 本判据供启动对账发现"旧形态任务"，回落开关等用户拨一次重建。
pub(crate) fn task_power_policy_verdict(xml: &str) -> Option<bool> {
    let disallow = extract_task_bool(xml, "DisallowStartIfOnBatteries")?;
    let stop = extract_task_bool(xml, "StopIfGoingOnBatteries")?;
    Some(!disallow && !stop)
}

/// 计划任务当前的电源策略是否与本版本注册的形态一致（供启动对账）。
/// 任务缺失/读不到 → `None`（由 `task_installed` 那条判据处理，这里不重复表态）。
pub fn task_power_policy_matches() -> Option<bool> {
    let TaskProbe::Defined(xml) = probe_task_cached(false) else {
        return None;
    };
    task_power_policy_verdict(&xml)
}

/// `enabled` 来自持久化的用户意图（AppSettings.rc003_capture_enabled），
/// **不是**"任务是否存在"——两者是不同的状态（见 TaskStatus 注释）。
pub fn status(enabled: bool) -> TaskStatus {
    // 本函数由按键页 / 连接页每秒调用一次（`get_rc003_task_status`）。
    // **绝不能启动 schtasks.exe**：系统关机时子进程初始化会失败
    // （`0xc0000142`），每次失败都弹一个系统错误框，关机被拖住 8–11 秒
    // （2026-11-10 用户反馈）。所以这里只读缓存，不调用会兜底跑子进程的
    // `task_installed()`；首次探测由下面那次 `authorization_required_now()` 触发。
    let installed = task_file_present().unwrap_or(false)
        || matches!(probe_task_cached(false), TaskProbe::Defined(_));
    TaskStatus {
        installed,
        // 2026-11-12 起不再恒为 true：任务可复用（已授权且指向当前助手）就是 false。
        // 首次调用会做一次 COM 探测，之后走缓存（本函数每秒被调用一次）。
        authorization_required: authorization_required_now(),
        enabled,
        helper_path: locate_helper_exe().map(|p| p.display().to_string()),
        last_error: None,
    }
}

/// 当前会话是否正在关机 / 重启。
///
/// 关机阶段系统会拒绝新进程初始化，`schtasks.exe` 起不来时由系统弹出
/// 「应用程序无法正常启动(0xc0000142)」错误框；每个框都会拖住关机。任何
/// 后台线程（如常驻对账）都必须先问这里，不能再尝试拉起子进程。
fn system_is_shutting_down() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_SHUTTINGDOWN};
    // SAFETY: GetSystemMetrics 只读取当前会话的系统指标，无副作用。
    unsafe { GetSystemMetrics(SM_SHUTTINGDOWN) != 0 }
}

/// 以 **ShellExecuteExW** 启动 `schtasks`（不捕获输出，因此没有管道）。
///
/// 2026-11-12 现场：`Command::new("schtasks").output()` 在应用里 3~4ms 内直接
/// 失败（"开启全按键支持"永远失败的直接原因），而同一命令行从 shell 能成功
/// ——失败发生在**子进程创建 + 管道**这一层，不是 schtasks 本身。这里改走与
/// `task_install_elevated`（UAC 拉起助手，现场已验证可用）同一条外壳通道：
/// 只需要退出码，不需要 stdout/stderr 文本，因此不再建管道。
fn run_schtasks(args: &[&str], label: &str) -> Result<(), String> {
    if system_is_shutting_down() {
        return Err("系统正在关机，已跳过计划任务操作。".to_string());
    }
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
    use windows::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };

    let file: Vec<u16> = std::ffi::OsStr::new("schtasks.exe")
        .encode_wide()
        .chain(Some(0))
        .collect();
    // 参数含空格的（任务名）必须加引号；/tn 与任务名保持两个 argv 元素。
    let quoted: Vec<String> = args
        .iter()
        .map(|arg| {
            if arg.contains(' ') {
                format!("\"{arg}\"")
            } else {
                (*arg).to_string()
            }
        })
        .collect();
    let parameters: Vec<u16> = quoted.join(" ").encode_utf16().chain(Some(0)).collect();

    let mut sei = SHELLEXECUTEINFOW::default();
    sei.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    sei.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_FLAG_NO_UI;
    sei.lpFile = PCWSTR(file.as_ptr());
    sei.lpParameters = PCWSTR(parameters.as_ptr());
    // SW_HIDE：schtasks 是控制台程序；GUI 主程序里不允许它闪窗。
    sei.nShow = 0;
    if let Err(error) = unsafe { ShellExecuteExW(&mut sei) } {
        sayall_windows::gatt_note(format!(
            "rc003 feature=enhanced-capture action=schtasks phase=spawn_failed label={label} \
             hresult={:?} detail={error}",
            error.code()
        ));
        return Err(format!("无法执行 schtasks（{label}）：{error}"));
    }
    let process = sei.hProcess;
    if process.is_invalid() || process == HANDLE::default() {
        sayall_windows::gatt_note(format!(
            "rc003 feature=enhanced-capture action=schtasks phase=spawn_failed label={label} \
             reason=no_process_handle"
        ));
        return Err(format!("无法执行 schtasks（{label}）：没有返回进程句柄"));
    }
    unsafe {
        let _ = WaitForSingleObject(process, INFINITE);
    }
    let mut exit_code = 0u32;
    let _ = unsafe { GetExitCodeProcess(process, &mut exit_code) };
    // 必须关句柄：每次触发/停止都会拿到新句柄，不关就是内核句柄泄漏。
    let _ = unsafe { CloseHandle(process) };
    if exit_code == 0 {
        return Ok(());
    }
    sayall_windows::gatt_note(format!(
        "rc003 feature=enhanced-capture action=schtasks phase=non_zero_exit label={label} \
         exit_code={exit_code}"
    ));
    Err(format!("schtasks {label}失败：退出码 {exit_code}"))
}

/// `schtasks /query` 的成功/失败就是"存在/不存在"，不需要碰输出编码。
fn schtasks(args: &[&str]) -> Option<bool> {
    if system_is_shutting_down() {
        return None;
    }
    silent_command("schtasks")
        .args(args)
        .output()
        .ok()
        .map(|output| output.status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_name_matches_helper_side() {
        // 与助手侧的常量逐字符一致。助手 --selftest 钉住它那一边，
        // 这里钉住这一边；两边任何一个改名而另一个没跟上，这条就会红。
        assert_eq!(SCHEDULED_TASK_NAME, "Free SayAll RC003 Helper");
    }

    #[test]
    fn trigger_args_use_task_name_verbatim() {
        // schtasks 的 /tn 值含空格，靠 Command 参数数组传递（不拼字符串），
        // 因此这里断言"一个参数就是完整任务名"，防止有人改成拼接后再踩编码坑。
        let name = SCHEDULED_TASK_NAME;
        assert!(name.contains(' '));
        assert!(!name.starts_with('"') && !name.ends_with('"'));
    }

    #[test]
    fn stop_signal_path_matches_helper_side() {
        // 停用信号路径与助手侧 app_stop_signal_path() 逐字符一致。
        // 这条路径错一个字符，停用就静默失效（旧助手继续映射）——
        // 2026-09-24 的"开关关了还能映射"正是停止通道不可靠的结果。
        let path = stop_signal_path();
        assert!(path.ends_with(std::path::Path::new("SayAll").join("rc003-capture-stop")));
    }

    #[test]
    fn reauth_marker_path_matches_installer_side() {
        // 重授权标记路径与安装器钩子写入的路径逐字符一致
        // （installer-hooks.nsh：$LOCALAPPDATA\SayAll\rc003-reauth-required）。
        // 错一个字符，"升级后需重新授权"就静默失效。
        let path = reauth_marker_path();
        assert!(path.ends_with(std::path::Path::new("SayAll").join("rc003-reauth-required")));
    }

    #[test]
    fn task_reuse_judged_by_full_command_path() {
        // 复用判据 = 任务指向的**完整路径**就是当前这份助手。它决定"这次开启会不会
        // 弹 UAC"，判错两个方向都难受：判成"可复用"而实际指向别处 → 触发到一个
        // 不存在的路径，开关开着却永远连不上；判成"需授权" → 每次开启都白弹一次
        // UAC，正是用户 2026-11-12 反馈的原话。
        let helper = std::path::Path::new(r"C:\Program Files\FreeSay\sayall-helper.exe");
        let same = r#"<Task><Actions><Exec>
            <Command>"C:\Program Files\FreeSay\sayall-helper.exe"</Command>
            </Exec></Actions></Task>"#;
        assert!(task_xml_reusable_for(same, helper));
        // Windows 路径大小写不敏感：安装器与调度器回读的大小写可能不一致
        assert!(task_xml_reusable_for(
            r#"<Command>c:\program files\free sayall\SAYALL-HELPER.EXE</Command>"#,
            helper
        ));
        // 换安装目录（dev 产物 vs 安装版）→ 必须重授权
        assert!(!task_xml_reusable_for(
            r#"<Command>D:\sayall-target\debug\sayall-helper.exe</Command>"#,
            helper
        ));
        // 同名不同目录也是"别处"，只看文件名会把这两者混为一谈
        assert!(!task_xml_reusable_for(
            r#"<Command>C:\Other\sayall-helper.exe</Command>"#,
            helper
        ));
        // 没有 <Command> → 判不出来，按"需授权"处理
        assert!(!task_xml_reusable_for("<Task></Task>", helper));
        assert!(!task_xml_reusable_for("<Command>  </Command>", helper));
    }

    #[test]
    fn extracts_task_command_file_name_without_paths() {
        // 真实任务 XML 的形态：<Command> 是完整路径（可能带引号），参数在 <Arguments> 里。
        // 这里只允许取文件名——完整路径是个人路径，不能进日志。
        let xml = r#"<?xml version="1.0"?><Task><Actions><Exec>
            <Command>"C:\Users\someone\AppData\Local\自由麦 FreeSay\sayall-helper.exe"</Command>
            <Arguments>--follow-app</Arguments></Exec></Actions></Task>"#;
        assert_eq!(
            extract_task_command(xml).as_deref(),
            Some("sayall-helper.exe")
        );
        // 不带引号 / 正斜杠 / 尾随空白都要能吃下
        assert_eq!(
            extract_task_command("<Command>C:/x/sayall-helper-arm64.exe </Command>").as_deref(),
            Some("sayall-helper-arm64.exe")
        );
        // 没有 <Command> 或值为空 → None（交给"任务缺失"那条判据）
        assert_eq!(extract_task_command("<Task></Task>"), None);
        assert_eq!(extract_task_command("<Command>   </Command>"), None);
    }

    #[test]
    fn task_target_verdict_flags_the_other_arch_helper() {
        // 2026-10-07 报障人现场：ARM64 机器上任务仍指向 x64 助手（覆盖安装保留了旧任务）
        assert_eq!(
            task_target_verdict(Some("sayall-helper.exe"), "sayall-helper-arm64.exe"),
            Some(false)
        );
        assert_eq!(
            task_target_verdict(Some("sayall-helper-arm64.exe"), "sayall-helper-arm64.exe"),
            Some(true)
        );
        // Windows 路径大小写不敏感
        assert_eq!(
            task_target_verdict(Some("SAYALL-HELPER-ARM64.EXE"), "sayall-helper-arm64.exe"),
            Some(true)
        );
        // 拿不到命令 → 不表态
        assert_eq!(task_target_verdict(None, "sayall-helper.exe"), None);
    }

    #[test]
    fn power_policy_verdict_reads_both_battery_flags() {
        // 2026-10-13 报障人现场形态：`schtasks /create` 命令行注册的任务
        // 两个电池策略都是 true → 电池供电下助手被静默拦下。
        let cli_style = r#"<?xml version="1.0"?><Task><Settings>
            <DisallowStartIfOnBatteries>true</DisallowStartIfOnBatteries>
            <StopIfGoingOnBatteries>true</StopIfGoingOnBatteries>
            </Settings></Task>"#;
        assert_eq!(task_power_policy_verdict(cli_style), Some(false));

        // 本版本 XML 注册的形态：两项都为 false。
        let xml_style = r#"<?xml version="1.0"?><Task><Settings>
            <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
            <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
            </Settings></Task>"#;
        assert_eq!(task_power_policy_verdict(xml_style), Some(true));

        // 半为真也算不通过（保守：宁可回落一次重建，也不让静默趴窝存在）。
        let half = r#"<Task><Settings>
            <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
            <StopIfGoingOnBatteries>true</StopIfGoingOnBatteries>
            </Settings></Task>"#;
        assert_eq!(task_power_policy_verdict(half), Some(false));

        // 任一项读不到 → 不表态（任务缺失/格式变化由其它判据处理）。
        assert_eq!(task_power_policy_verdict("<Task></Task>"), None);
        assert_eq!(
            task_power_policy_verdict(
                "<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>"
            ),
            None
        );
    }

    #[test]
    fn authorization_is_reused_unless_forced() {
        // 2026-11-12 用户定稿：只授权一次。任务已注册且指向当前助手时，
        // 再打开开关不再弹 UAC。逃生阀 `FORCE_REAUTHORIZE_ON_EVERY_ENABLE`
        // 置 true 才退回旧语义——改它必须同步：
        // ① 前端弹窗（EnhancedCaptureConfirmDialog）文案与
        // ② ButtonsPage / ConnectionPage 的确认弹窗判据测试。
        assert!(!FORCE_REAUTHORIZE_ON_EVERY_ENABLE);
    }

    #[test]
    fn mismatched_task_is_not_reused() {
        // 读不到任务定义（任务不存在 / COM 用不了）时必须按"需要授权"处理——
        // 宁可多弹一次 UAC，也不能跳过授权步骤去触发一个来路不明的任务。
        // 这里钉住纯函数那一半：任务存在、但指向别处 → 不可复用。
        let ghost = std::path::Path::new(r"C:\__no_such_dir__\sayall-helper.exe");
        assert!(!task_xml_reusable_for(
            r#"<Command>D:\sayall-target\debug\sayall-helper.exe</Command>"#,
            ghost
        ));
        assert!(!task_xml_reusable_for("<Task></Task>", ghost));
    }

    #[test]
    fn capture_support_reasons_are_stable_strings() {
        use sayall_windows::os_arch::NativeArch;
        // 不受支持的原生架构（如 32 位 ARM）：无论助手在不在，都判不可用且原因固定
        assert_eq!(
            capture_support_decision(NativeArch::Other, true),
            (false, Some("arch_unsupported"))
        );
        assert_eq!(
            capture_support_decision(NativeArch::Other, false),
            (false, Some("arch_unsupported"))
        );
        // 架构对得上但载荷缺失：只有这一种情况算 helper_missing
        assert_eq!(
            capture_support_decision(NativeArch::X64, false),
            (false, Some("helper_missing"))
        );
        assert_eq!(
            capture_support_decision(NativeArch::Arm64, false),
            (false, Some("helper_missing"))
        );
        // 正常与"探测失败"都必须可用：探测失败不改既有行为
        assert_eq!(
            capture_support_decision(NativeArch::X64, true),
            (true, None)
        );
        assert_eq!(
            capture_support_decision(NativeArch::Arm64, true),
            (true, None)
        );
        assert_eq!(
            capture_support_decision(NativeArch::Unknown, true),
            (true, None)
        );
        // 这两条字符串是前端契约（`reason` 字段），改名即破坏界面判据
        assert_eq!(
            capture_support_decision(NativeArch::Other, false).1,
            Some("arch_unsupported")
        );
    }

    #[test]
    fn helper_file_name_follows_the_native_arch() {
        use sayall_windows::os_arch::NativeArch;
        // 安装目录里的文件名按架构分（ADR 0003）；开发布局里 cargo 产物名不变，
        // 只多一层 triple 目录。
        assert_eq!(NativeArch::X64.helper_file_name(), "sayall-helper.exe");
        assert_eq!(
            NativeArch::Arm64.helper_file_name(),
            "sayall-helper-arm64.exe"
        );
        assert_eq!(
            dev_target_triple(NativeArch::Arm64),
            Some("aarch64-pc-windows-msvc")
        );
        assert_eq!(dev_target_triple(NativeArch::X64), None);
        assert_eq!(dev_target_triple(NativeArch::Unknown), None);
    }

    #[test]
    fn capture_support_reports_a_consistent_shape() {
        // 本机是 x64 且仓库里就有助手产物（开发布局）——这依赖环境，故只断言形状不变量：
        // 字段命名是前端契约，available 与 reason 必须自洽。
        let support = capture_support();
        assert!(matches!(
            support.native_arch.as_str(),
            "x64" | "arm64" | "unknown"
        ));
        assert!(support.helper_expected.ends_with(".exe"));
        assert_eq!(support.available, support.reason.is_none());
    }

    struct TaskFileGuard {
        previous: Option<std::path::PathBuf>,
        dir: std::path::PathBuf,
        path: std::path::PathBuf,
    }

    impl TaskFileGuard {
        fn new(label: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "sayall-rc003-status-{}-{label}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("创建临时目录");
            let path = dir.join(SCHEDULED_TASK_NAME);
            let previous = TASK_FILE_OVERRIDE.with(|slot| slot.replace(Some(path.clone())));
            Self {
                previous,
                dir,
                path,
            }
        }
    }

    impl Drop for TaskFileGuard {
        fn drop(&mut self) {
            TASK_FILE_OVERRIDE.with(|slot| *slot.borrow_mut() = self.previous.take());
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn status_reads_task_file_when_present() {
        // 2026-11-10 关机弹窗回归：`status()` 每秒被界面调用，绝不能启动
        // `schtasks.exe`。任务文件存在时应直接判 installed=true。
        //
        // 本测试与下一条合起来是"先红后绿"的回归：
        // * 机器上有真实任务时，缺文件那条在旧实现下会返回 true；
        // * 机器上没有真实任务时，有文件那条在旧实现下会返回 false。
        let guard = TaskFileGuard::new("present");
        std::fs::write(&guard.path, "<Task/>").expect("写临时任务文件");
        assert!(status(false).installed);
    }

    #[test]
    fn status_reports_not_installed_when_task_file_is_missing() {
        let _guard = TaskFileGuard::new("missing");
        assert!(!status(false).installed);
    }
}

// 2026-11-12: 配置修正后触发 dev 热重启（验证用，保留注释）。
