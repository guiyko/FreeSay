/**
 * 语音音频完整度的用户提示（2026-10-24 现场）。
 *
 * 现场事实：遥控器与另一支 BLE 键盘同时连接时，ATVV 音频只剩满速的约 1/3；
 * 豆包语音条能出现，但转写不出文字。应用不自动断开用户的其它蓝牙设备，
 * 只在**最近一次会话确实降级**时给一句可执行的提示（文案规范见
 * docs/product-copy.md：说清发生了什么 + 用户该做什么，不贴内部错误）。
 */
import type { VoiceAudioHealth } from "./bridge";
import { t } from "./i18n";

/** 载荷规范化：缺失/未知值按"读不到"处理，不提示。 */
export function normalizeVoiceAudioHealth(raw: unknown): VoiceAudioHealth {
  return raw === "unknown" || raw === "full" || raw === "degraded"
    ? raw
    : "unknown";
}

/** 只有明确降级才提示；满速与未知都不显示。 */
export function shouldShowVoiceAudioDegraded(health: VoiceAudioHealth): boolean {
  return health === "degraded";
}

/**
 * 一句用户语言：不出现样本率、MTU、BLE 调度等内部词。
 * 连接页与向导第⑥步共用，保证同一事实同一说法。
 *
 * 2026-11-16 修订（现场）：降级**不一定**是"没出字"——只剩约 1/4 音频时，
 * 输入法照样出字，只是按音猜成了别的词，用户会以为"识别不准"。原文案只写
 * "文字可能没有出现"，对不上这个现象，也没给出当天真正有效的动作
 * （关闭再打开 Windows 蓝牙总开关，重建适配器的 BLE 会话）。
 */
export function voiceAudioDegradedText(): string {
  return t(
    "最近一次语音只有一部分声音送到了电脑：可能缺字，也可能被识别成完全不相干的词。先在 Windows 设置里关掉再打开蓝牙总开关，等遥控器重连后再说一句；仍然不对就断开重连，或关掉同时连接的其他蓝牙设备（耳机、键盘、鼠标）。",
  );
}
