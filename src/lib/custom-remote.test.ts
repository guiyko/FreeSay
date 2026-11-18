import { describe, expect, it } from "vitest";

import { createCustomButton, createCustomRemote } from "./custom-remote";

/**
 * 2026-11-15 现场回归锁。
 *
 * 事故：新建的自定义遥控器是空按钮列表，用户逐个手工添加并选槽位；只要漏掉一个，
 * 那个键就永远不响应（用户看到的正是"电源键识别不到，其他键都正常"）。更隐蔽的是
 * 槽位留「（未指定）」的按钮只做展示、不参与映射，界面上很难自查。
 *
 * 现在新建即预置与内置 RC003 一致的 12 个键位，槽位全部填好。
 */
describe("custom remote defaults", () => {
  it("新建遥控器自带 12 个已指定槽位的按键", () => {
    const remote = createCustomRemote();
    expect(remote.buttons).toHaveLength(12);
    for (const button of remote.buttons) {
      expect(button.slot, `${button.name} 缺少槽位`).toBeTruthy();
      expect(button.name.length).toBeGreaterThan(0);
    }
  });

  it("包含电源键，且槽位是 power", () => {
    const remote = createCustomRemote();
    const power = remote.buttons.find((button) => button.slot === "power");
    expect(power, "必须预置电源键").toBeDefined();
    expect(power?.name).toBe("电源");
  });

  it("槽位不重复（重复会导致两个卡片抢同一个动作）", () => {
    const remote = createCustomRemote();
    const slots = remote.buttons.map((button) => button.slot);
    expect(new Set(slots).size).toBe(slots.length);
  });

  it("每个按键的坐标都落在照片范围内", () => {
    const remote = createCustomRemote();
    for (const button of remote.buttons) {
      expect(button.x).toBeGreaterThan(0);
      expect(button.x).toBeLessThan(1);
      expect(button.y).toBeGreaterThan(0);
      expect(button.y).toBeLessThan(1);
    }
  });

  it("手工新增的按键默认不带槽位（需要用户显式选择）", () => {
    const button = createCustomButton("自定义键");
    expect(button.slot).toBeNull();
    expect(button.x).toBe(0.5);
    expect(button.y).toBe(0.5);
  });

  it("createCustomButton 支持指定列与坐标（预置模板依赖它）", () => {
    const button = createCustomButton("电源", "power", "left", 0.3, 0.1);
    expect(button.slot).toBe("power");
    expect(button.side).toBe("left");
    expect(button.x).toBe(0.3);
    expect(button.y).toBe(0.1);
  });
});
