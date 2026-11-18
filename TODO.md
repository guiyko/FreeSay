# TODO（未完成项）

> 项目处于**维护冻结**状态：以下只记录已知未完成项，不再排期。
> 每条的完整证据链、日志与真机记录见 `docs/archive/TODO-full-2026-10.md` 对应章节。

## 语音识别"能出字但对不上"的定位链（2026-10-27 用户报障，已定位为配置问题）

- [ ] 现象：遥控器语音键正常、豆包能出字，但识别内容与实际说的出入较大；链路日志全绿（详见 `Bugs/2026-10-27-voice-accuracy-input-device-auto-detect.md`）

## v1 决策（2026-09-04）

## 后续产品功能（开发前对照 Mac App）

- [ ] **遥控器操作 Windows 应用切换器（Alt+Tab，对齐 Mac Command+Tab 会话（详见 `Testing/WindowsAppSwitcher.md`）
- [ ] 个人中心（2026-10-24 用户要求，对齐 Mac 设置页 `statisticsPage`（详见 `Testing/WindowsProfileStatistics.md`）
- [ ] 回眸文字版（2026-10-25，用户要求参照 Mac 复刻（详见 `Testing/WindowsReflections.md`）
- [ ] 推荐页（2026-10-18，用户指定）：参考 Mac 设置侧栏「推荐」入口，Windows 侧直接加载官网 `https://freesay.app/recommendations/`，不复制推荐内容或自建推荐 UI（详见 `Bugs/2026-10-24-recommendations-stuck-loading.md`）
- [ ] 增加首次使用 Onboarding 流程页面：参考 Mac App 的步骤顺序和完成条件，覆盖遥控器连接、语音输出设备、目标输入法选择、按住说话验证与失败恢复；支持中断后继续、完成后重新进入，并为关键状态、分支和外部调用补齐结构化日志。（详见 `Testing/WindowsOnboardingWizard.md`）
- [ ] 组合动作（参考 Mac `macos-combination-actions` 包）：把多个本机步骤按顺序组合成一个动作，绑定到遥控器按键的单击 / 双击 / 长按（详见 `Bugs/2026-10-15-combo-edit-shows-two-and-format-version-zero.md`）
- [ ] 增强捕获：换遥控器后**自动重选宿主**（2026-10-13 现场提出，助手侧已实现）：助手新增目标（详见 `Bugs/2026-10-13-helper-does-not-follow-remote-switch.md`）
- [ ] 增强捕获：固定端口被系统级保留时回退自动分配（issue #296；根因定位与首版修复 @jihtsan，（详见 `Testing/WindowsRC003EnhancedCapture.md`）
- [ ] 增强捕获：renew 写失败 / agent 心跳停顿的快速自愈（2026-10-21 现场，日志（详见 `Bugs/2026-10-21-rc003-renew-stall.md`）
- [ ] 增强捕获：共享宿主里的旧 tap 自动恢复（2026-10-21 现场）：旧宿主（详见 `Bugs/2026-10-21-rc003-stale-tap-shared-host-helper-recovery.md`）
- [ ] 向导⑤步的前置状态守卫（2026-10-13 现场：⑤步在"输入法还没选"时就进去了）：重启后落在⑤步
- [ ] 支持 Typeless：参考 Mac App 的选择、配置、触发、状态反馈和恢复流程，调研 Windows 公开能力后实现按住说话生命周期、音频路由与失败关闭；分别完成 RC001/RC003、冷态首用、快速连按、断连和睡眠恢复真机验收。
- [ ] 支持 Vokie（第三方语音助手，官网 vokie.com）：连接页「选择你在用的输入工具」新增 Vokie 卡片（默认快捷键 右 Alt；说明“流式显示，智能整理，不占用输入法”），选卡即自动落 右 Alt；面板按状态提示——未安装显示官网入口、已安装但没运行提示先启动并提供「打开 Vokie」一键启动（
- [ ] 支持豆包输入法：参考 Mac App 的产品行为与配置引导，在不读取或修改豆包私有配置、内部数据库、内存或私有协议的前提下设计 Windows 支持路径
- [ ] 语音增益（对齐 Mac 设置页「增益」滑块，2026-10-04）：连接页「语音设备」卡新增 0–24 dB 滑块（步进 1，0 dB = 原始音量，说明文案与 Mac `audio.gain.help` 同义），`AppSetting…
- [ ] 完善聚焦输入框处理：参考 Mac App 对目标输入框的识别、焦点保持、恢复和无可编辑目标时的用户提示
- [ ] 按键页「打开应用」预设收敛与自动聚焦（2026-10-23 用户要求）：默认预设固定为 10 个——自由麦 FreeSay、微信、网易云音乐、Chrome 浏览器、ChatGPT、WorkBuddy、DimAgent、Vokie、记事本…
- [ ] 常用语（会员能力，对齐 Mac「常用语」）：把常用句绑定到遥控器五个位置（上 / 左 / OK / 右 / 下），（详见 `Testing/WindowsCommonPhrases.md`）
- [ ] 快速启动（hiberboot）蓝牙断连取证（2026-10-15 用户现场反馈，2026-10-20 落地取证）：Windows 10 开启「启用快速启动」时，已配对遥控器每隔几十秒掉线、几秒后自连

## Windows RC001 / RC003

- [ ] 设置页「启动时自动打开主面板」（2026-10-19 用户需求，照 Mac `openMainWindowAtLaunch`，默认开
- [ ] 鼠标动作映射：支持左/右/中键单击、左键双击、滚轮格数与指针移动像素（2026-10-22 定稿：两者统一为 1–100 的整数、默认 20，使用普通文本框，Enter 或失焦即保存）
- [ ] Windows 注册应用扫描与应用库：支持搜索、多选/全选、配置保存及导入导出
- [ ] 遥控器电量显示：连接建立后在既有 GATT 会话内订阅标准 Battery Service（0x180F/0x2A19）notify——设备上报即更新，并读一次初始值
- [ ] 设置页改版 + 应用图标切换（2026-10-02 用户定稿
- [ ] 关于页模块重组（2026-10-01 用户定稿）：顶部模块 = 应用标识 + “官网”（https://freesay.app/）与“GitHub”（https://github.com/guiyko/FreeSay）入口，检查更新为紧…
- [ ] 设置页吸收权限页 + 侧栏紧凑化（2026-10-31 用户指定，对照 Mac PR [#803](https://github.com/HD838A/remote-mic-app/pull/803)）：侧栏删除独立「权限」入口，旧持久…
- [ ] 主窗口 Ctrl+W 关闭快捷键：Ctrl+W 隐藏主窗口并由托盘驻留，语义与点标题栏"X"完全一致（不动 BLE/语音链路、不退出进程，真正退出仍走托盘菜单"退出"）
- [ ] 使用真实 RC001/RC003 和第三方语音程序（微信输入法、Win+H 等）验证按住说话快捷键：DOWN/UP 严格成对、无粘键、无重复音频，且断连和睡眠恢复后不残留按住的快捷键
- [ ] 使用真实 RC001 验证型号识别、BLE 配对、连接、断开、重连和首次语音。
- [ ] 使用真实 RC003 验证型号识别、BLE 配对、连接、断开、重连和首次语音。
- [ ] 验证 `STREAM_START → AUDIO → STREAM_STOP` 首次会话完整可用。
- [ ] 在 Windows 真机验证 WASAPI 端点初始化、VB-CABLE 回环、欠载恢复与完整尾音。
- [ ] WASAPI 客户端在 WTS 会话断开/重连后自动释放并重开同一端点（issue #276）：自动化与本机 VB-CABLE 真实端点重开 passed
- [ ] 定位 WASAPI PCM `queue_overflow`：已加失效早检与 1 秒 `audio_pump` 进度日志（`available_frames` / `queued_samples` / `pump_calls` / `z…
- [ ] 如未来需要捆绑或自动执行 VB-CABLE 驱动包，先取得与 Pack45 内附许可一致的作者书面授权，并实现来源校验、显式 UAC、结果检测和重启流程。
- [ ] 实现按键映射保存、热加载和 SendInput：独立映射文件、显式热加载、批量 SendInput、部分提交回滚和界面测试已完成
- [ ] 完成 Windows 10 1809 / Windows 11 安装、升级和卸载验证。
- [ ] 建立自签 Authenticode、证书指纹和 SHA-256 发布流程。（详见 `Testing/WindowsRC001ThreeKeyMapping.md`）
- [ ] 为返回键、音量加、音量减完成按设备源头捕获：按 2026-09-22 路线文档实现独立 Helper/捕获协议，先修复完成态与握手状态机，再分别完成 RC001/RC003 的 E1–E7 真机验收（详见 `Testing/WindowsRC003EnhancedCapture.md`）
- [ ] **笔记本功耗：后台节流豁免改为条件化**（bf03f0e 当前全局豁免——它是首按修复的组成部分，全局豁免代价是闲置功耗略高）。方向：仅在遥控器连接期间豁免，断连后恢复参与节流；重连可靠性已由 WakeReconnect + 无线电自愈兜底。台式机无影响；上笔记本场景前处理。
- [ ] **失败恢复加速（可选）**：wetype_check 检测判据从 ConsentStore 注册表（700ms 检查窗）换 LL 钩子 0xFC 标记观察（毫秒级，kb-live 已验证与开麦 100% 交叉一致）
- [ ] **WeType 2.1.4.6 探测时好时坏导致重放和弦拆掉进行中的会话（issue #118，0.3.0 未修）**：本机 2026-09-30 复现（16 次 `reacted=false`、2 次 `chord_retry`，示…
- [ ] **冷态管道 ~120ms（低优先级）**：闲置后首按应用内部链路（GATT 回调→武装→工作线程→IME 查询→和弦）实测可拖 ~120ms（不失败但慢）。节流豁免已生效仍有首次线程调度延迟；武装已内联到 GATT 回调。进一步压缩收益 ~100ms 冷态延迟，风险中（动的是刚修好的链路），无用户报障不动。
- [ ] **语音键 F5 抑制器的"链头 bump"去留 —— 待 F5 实测判定（关联 issue #111）**：现状（`main` / 0.3.0）：`crates/sayall-windows/src/key_suppressor.rs…
- [ ] **全按键接管名单：方向 / 确定 / Enter 等高频键是否排除（产品决策，待 Andy 拍板）**：0.3.0 起 RC003「全按键支持」在报告层按 usage 清空报告，无法区分"遥控器"与"物理键盘"——配置了动作的键，其物…（详见 `Bugs/2026-10-07-issue206-arm64-unsupported.md`）
- [ ] 13 键全量走查与边界矩阵：本次实测覆盖三键（音量± 连按 3/3、返回 2/2，均命中配置动作）与对照组按键，**仍缺**冷态首用、连续会话、快速连按、断连与睡眠恢复
- [ ] 不可用场景（人为移除 arm64 载荷）验证提示与置灰，确认不再空转。
