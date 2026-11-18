# 项目现状

> 最后更新：2026-11-18 · 版本 **1.1.7**

## 一句话状态

**保持现状，短期无更新计划。** 代码、文档与发布资产均已定稿；本仓库不再排期新功能，仅保留现有实现与问题记录。

## 发布

| 项 | 值 |
|---|---|
| 仓库 | https://github.com/guiyko/FreeSay |
| 最新版本 | 1.1.7（tag `v1.1.7`） |
| 安装包 | `FreeSay_1.1.7_x64-setup.exe`（NSIS，Windows x64，约 21.6 MiB） |
| 下载 | https://github.com/guiyko/FreeSay/releases/latest |
| 更新说明 | [release-notes-1.1.7.md](release-notes-1.1.7.md) |

安装包不纳入版本库（`.gitignore` 排除 `*.exe`），一律作为 Release 附件分发。

## 支持范围

- Windows 10 1809（17763）及以上，64 位；ARM64 上基础语音与按键可用。
- 内置适配：小米蓝牙遥控器 2（RC001）、2 Pro（RC003）。
- 其它蓝牙语音遥控器：走 ATVV 协议，可在「自定义遥控器」上传照片自行标定（**只维护一只**）。

## 本地构建

```bash
pnpm install                  # 依赖
node scripts/stage-bundle-inputs.cjs   # 出包前必须：构建助手 + 取回 Gadget
pnpm tauri:dev                # 开发
pnpm tauri:build              # 出包（产物在 target/release/bundle/nsis）
```

出包与发布的完整不变量（资源暂存、前端必须内嵌、双载荷 x64/arm64、发布授权）见 [RELEASING.md](../RELEASING.md)。

## 文档索引

| 文档 | 用途 |
|---|---|
| [README.md](../README.md) | 项目介绍、能力、快速开始、常见问题 |
| [ui-guide/README.md](ui-guide/README.md) | 界面逐页说明（配当前版本截图） |
| [installation-and-configuration.md](installation-and-configuration.md) | 安装与配置细节 |
| [TECHNICAL.md](../TECHNICAL.md) | 技术架构与关键链路 |
| [architecture/](architecture/) | 组件边界与路线图 |
| [decisions/](decisions/) | 架构决策记录（ADR） |
| [investigations/](investigations/) | 历史调查与实验结论（只读，按日期） |
| [Bugs/](../Bugs/) · [Testing/](../Testing/) | 缺陷档案与真机验收记录 |
| [TODO.md](../TODO.md) | 未完成项（冻结后仅记录，不再排期） |
| [archive/](archive/) | 归档：完整历史 TODO、已废弃文档 |

## 已知边界（不打算在冻结期解决）

- 安装包未做 Windows 代码签名，首次运行可能触发 SmartScreen。
- 蓝牙加密记录可能被系统再次写坏（`KeyLength=0`），届时到「设置 → 诊断与日志」点一次修复。
- 返回 / 音量± / 电源的快捷键录入需先「设为当前」。
- 语音依赖 VB-CABLE 虚拟声卡。

## 依赖的持久标识（不要改）

改名或迁移时以下名字必须保持原样，改动会导致静默失败（任务找不到、管道连不上、旧实例认不出）：

| 类别 | 固定值 |
|---|---|
| 计划任务 | `Free SayAll RC003 Helper` |
| 运行时目录 | `%ProgramData%\SayAll\rc003-helper` |
| 本地数据目录 | `%LOCALAPPDATA%\SayAll` |
| 命名管道 | `\\.\pipe\SayAll.Rc003Bridge` |
| 单实例互斥体 | `SayAll.RC003Helper.SingleInstance` |
