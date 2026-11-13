// @vitest-environment jsdom

import { mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import { ref } from "vue";

/**
 * 推荐页在 2026-11-13 从「子 WebView 加载远端页面」改成「随包打包的静态内容」，
 * 因此测试重点从"加载/失败/重试状态机"变成"内容是否完整、资源是否本地化"。
 */

const language = vi.hoisted(() => ({ current: "zh-Hans" as "zh-Hans" | "en" }));

vi.mock("../lib/language", () => ({
  useLanguage: () => ({ effectiveLanguage: ref(language.current) }),
}));
vi.mock("../lib/i18n", () => ({ t: (text: string) => text }));

import RecommendationsPage from "./RecommendationsPage.vue";

describe("推荐页（离线固定内容）", () => {
  it("渲染两个分组和全部条目", () => {
    const wrapper = mount(RecommendationsPage);

    expect(wrapper.findAll(".rec-group")).toHaveLength(2);
    // 4 个硬件 + 3 个软件
    expect(wrapper.findAll(".rec-card")).toHaveLength(7);
    expect(wrapper.text()).toContain("小米蓝牙遥控器 2");
    expect(wrapper.text()).toContain("微信输入法");
  });

  it("图片全部指向本地资源，不再引用远端地址", () => {
    const wrapper = mount(RecommendationsPage);
    const sources = wrapper.findAll("img").map((img) => img.attributes("src"));

    expect(sources.length).toBeGreaterThan(0);
    for (const source of sources) {
      expect(source?.startsWith("/recommendations/")).toBe(true);
    }
  });

  it("页面不发起任何网络请求（离线可用）", () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch");
    mount(RecommendationsPage);
    expect(fetchSpy).not.toHaveBeenCalled();
    fetchSpy.mockRestore();
  });

  it("英文界面下标题切换为英文", () => {
    language.current = "en";
    const wrapper = mount(RecommendationsPage);
    expect(wrapper.text()).toContain("Xiaomi Bluetooth Remote 2");
    language.current = "zh-Hans";
  });
});
