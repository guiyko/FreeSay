import { describe, expect, it } from "vitest";

import { columnRowCenter, planColumns } from "./mapping-columns";

/** 与 ButtonsPage 一致的几何常量（改这里等于改画布，测试同时失效——故意的）。 */
const OPTIONS = {
  capacity: 7,
  baseHeight: 570,
  padding: 16,
  cardHeight: 72,
  gap: 4,
};

describe("按键页两列排法（2026-11-12 用户定稿）", () => {
  it("按键少于容量时全部排左列，画布保持 570", () => {
    // 用户的遥控器当时只有「电源 / 语音」两个键，两个都被排在右列最上面。
    const plan = planColumns(2, OPTIONS);
    expect(plan.leftCount).toBe(2);
    expect(plan.rightCount).toBe(0);
    expect(plan.rows).toBe(7);
    expect(plan.canvasHeight).toBe(570);
  });

  it("左列排满后排右列，右列同样从第一行开始", () => {
    const full = planColumns(7, OPTIONS);
    expect([full.leftCount, full.rightCount]).toEqual([7, 0]);

    const eight = planColumns(8, OPTIONS);
    expect([eight.leftCount, eight.rightCount]).toEqual([7, 1]);
    // 右列只有一张 → 落在第一行（中心 = 上留白 + 半张卡），不会凭空居中。
    expect(columnRowCenter(0, eight.rows, eight.canvasHeight, 16, 72)).toBe(52);
  });

  it("两列都满后才长高，而且左右各加一行", () => {
    const fourteen = planColumns(14, OPTIONS);
    expect([fourteen.leftCount, fourteen.rightCount]).toEqual([7, 7]);
    expect(fourteen.canvasHeight).toBe(570);

    // 第 15 个按键：左列 8、右列 7 —— 两列只差 1，画面不会一边倒。
    const fifteen = planColumns(15, OPTIONS);
    expect([fifteen.leftCount, fifteen.rightCount]).toEqual([8, 7]);
    expect(fifteen.rows).toBe(8);
    expect(fifteen.canvasHeight).toBe(636);

    // 第 16 个按键：两边各 8，重新齐平。
    const sixteen = planColumns(16, OPTIONS);
    expect([sixteen.leftCount, sixteen.rightCount]).toEqual([8, 8]);
    expect(sixteen.canvasHeight).toBe(636);
  });

  it("按键再多也只长高，不重叠：行距不小于卡片高度", () => {
    for (const count of [17, 24, 40]) {
      const plan = planColumns(count, OPTIONS);
      expect(Math.abs(plan.leftCount - plan.rightCount)).toBeLessThanOrEqual(1);
      const first = columnRowCenter(0, plan.rows, plan.canvasHeight, 16, 72);
      const second = columnRowCenter(1, plan.rows, plan.canvasHeight, 16, 72);
      expect(second - first).toBeGreaterThanOrEqual(72);
      // 末行必须留在画布内。
      const last = columnRowCenter(plan.rows - 1, plan.rows, plan.canvasHeight, 16, 72);
      expect(last + 72 / 2).toBeLessThanOrEqual(plan.canvasHeight);
    }
  });

  it("空遥控器不缩放画布（没有按键也保持 570）", () => {
    const plan = planColumns(0, OPTIONS);
    expect(plan.leftCount).toBe(0);
    expect(plan.rightCount).toBe(0);
    expect(plan.canvasHeight).toBe(570);
  });
});
