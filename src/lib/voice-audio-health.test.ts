import { describe, expect, it } from "vitest";
import {
  normalizeVoiceAudioHealth,
  shouldShowVoiceAudioDegraded,
  voiceAudioDegradedText,
} from "./voice-audio-health";

describe("voice audio health（2026-10-24 蓝牙共存降级提示）", () => {
  it("只认三个稳定 token；缺失或未知值回落 unknown（不提示）", () => {
    expect(normalizeVoiceAudioHealth("unknown")).toBe("unknown");
    expect(normalizeVoiceAudioHealth("full")).toBe("full");
    expect(normalizeVoiceAudioHealth("degraded")).toBe("degraded");
    expect(normalizeVoiceAudioHealth(undefined)).toBe("unknown");
    expect(normalizeVoiceAudioHealth(null)).toBe("unknown");
    expect(normalizeVoiceAudioHealth("")).toBe("unknown");
    expect(normalizeVoiceAudioHealth("Degraded")).toBe("unknown");
    expect(normalizeVoiceAudioHealth("nonsense")).toBe("unknown");
  });

  it("只有 degraded 显示提示；full / unknown 不显示", () => {
    expect(shouldShowVoiceAudioDegraded("degraded")).toBe(true);
    expect(shouldShowVoiceAudioDegraded("full")).toBe(false);
    expect(shouldShowVoiceAudioDegraded("unknown")).toBe(false);
  });

  it("提示说清原因与下一步，不出现内部术语", () => {
    const text = voiceAudioDegradedText();
    // 2026-11-16 现场修订：降级也可能"出了字但识别成别的词"，文案必须两种现象都覆盖。
    expect(text).toContain("只有一部分声音送到了电脑");
    expect(text).toContain("被识别成完全不相干的词");
    expect(text).toContain("蓝牙总开关");
    expect(text).toContain("关掉同时连接的其他蓝牙设备");
    expect(text).toContain("耳机、键盘、鼠标");
    expect(text).not.toMatch(/样本率|MTU|BLE|ATVV|16\s*kHz/);
  });
});
