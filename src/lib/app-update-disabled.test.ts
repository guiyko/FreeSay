// @vitest-environment jsdom
/**
 * 更新检查**关闭态**（2026-11-12 用户指定）。
 *
 * 现场原因：改名分叉后 1.0.0 仍会去官方 Releases 找 latest.json，"检查更新"会把
 * 用户推回上游版本；自建更新源尚未落地。用户要求"机制保留、关联取消"——所以
 * 这里锁定两件事：
 *  1. 总开关线上默认是 false；
 *  2. 关闭态下 `check()` 既不调用 `checkAppUpdate`（不发任何网络请求），也不写
 *     错误——启动静默检查与手动点击都安静地留在 idle。
 *
 * 本文件**故意不 mock feature-flags**，读的就是线上默认值。要验证流水线本身的
 * 用例在 `app-update.test.ts`（那里把开关 mock 成 true）。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const checkAppUpdate = vi.fn<() => Promise<never>>();
const installAppUpdate = vi.fn<() => Promise<void>>();
const getAppUpdatePreferences = vi.fn();
const setAppUpdatePreferences = vi.fn();

vi.mock("./bridge", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./bridge")>();
  return {
    ...actual,
    checkAppUpdate: () => checkAppUpdate(),
    installAppUpdate: () => installAppUpdate(),
    getAppUpdatePreferences: () => getAppUpdatePreferences(),
    setAppUpdatePreferences: (includePrereleases: boolean) =>
      setAppUpdatePreferences(includePrereleases),
  };
});

import { APP_UPDATE_CHECK_ENABLED } from "./feature-flags";
import { resetAppUpdateForTests, useAppUpdate } from "./app-update";

describe("更新检查关闭态（切断与官方服务器的关联）", () => {
  beforeEach(() => {
    resetAppUpdateForTests();
    checkAppUpdate.mockReset();
    installAppUpdate.mockReset();
  });

  it("总开关默认关闭", () => {
    expect(APP_UPDATE_CHECK_ENABLED).toBe(false);
  });

  it("手动检查不发请求：留在 idle 且不显示错误", async () => {
    const update = useAppUpdate();
    await update.check(true);
    expect(checkAppUpdate).not.toHaveBeenCalled();
    expect(update.phase.value).toBe("idle");
    expect(update.errorMessage.value).toBe("");
    expect(update.info.value).toBeNull();
  });

  it("启动静默检查同样不发请求", async () => {
    const update = useAppUpdate();
    await update.runStartupSilentCheck();
    expect(checkAppUpdate).not.toHaveBeenCalled();
    expect(update.phase.value).toBe("idle");
    expect(update.bannerVisible.value).toBe(false);
  });
});
