import { beforeEach, describe, expect, it, vi } from "vitest";

// 守卫依赖 bridge 的两个 IPC；单测里全部替换，不碰真实文件。
const noteRemoval = vi.fn().mockResolvedValue(true);
const pending = vi.fn().mockResolvedValue(false);
vi.mock("./bridge", () => ({
  pairGuardNoteRemoval: (...args: unknown[]) => noteRemoval(...(args as [])),
  pairGuardPending: () => pending(),
  scanPairedRemotes: vi.fn().mockResolvedValue([]),
}));

import { observePairedDevices, pairGuardBlocked, refreshPairGuard } from "./pair-guard";

function setKnown(ids: string[]): void {
  localStorage.setItem("freesay.pairGuard.knownIds", JSON.stringify(ids));
}

beforeEach(() => {
  localStorage.clear();
  noteRemoval.mockClear();
  pending.mockReset().mockResolvedValue(false);
  pairGuardBlocked.value = false;
});

describe("配对守卫：扫描结果驱动的删除侦测", () => {
  it("基线阶段不封锁：第一次上报只记录，不写时间戳", async () => {
    await observePairedDevices(["a", "b"]);
    expect(noteRemoval).not.toHaveBeenCalled();
    expect(pairGuardBlocked.value).toBe(false);
    expect(JSON.parse(localStorage.getItem("freesay.pairGuard.knownIds") ?? "[]")).toEqual([
      "a",
      "b",
    ]);
  });

  it("少了一支即判定删除：写时间戳并进入封锁", async () => {
    setKnown(["a", "b"]);
    // 落完时间戳后端应报 pending=true（模拟真实后端行为）。
    pending.mockResolvedValue(true);
    await observePairedDevices(["a"]);
    expect(noteRemoval).toHaveBeenCalledTimes(1);
    expect(pairGuardBlocked.value).toBe(true);
  });

  it("空列表视为扫描失败，不判定、不覆盖基线", async () => {
    setKnown(["a", "b"]);
    await observePairedDevices([]);
    expect(noteRemoval).not.toHaveBeenCalled();
    // 基线不被空结果冲掉，否则一次失败就永远进不了删除判定。
    expect(JSON.parse(localStorage.getItem("freesay.pairGuard.knownIds") ?? "[]")).toEqual([
      "a",
      "b",
    ]);
  });

  it("已封锁时不重复写时间戳（否则删除时刻被推后，永远解锁不了）", async () => {
    setKnown(["a", "b"]);
    pairGuardBlocked.value = true;
    await observePairedDevices(["a"]);
    expect(noteRemoval).not.toHaveBeenCalled();
  });

  it("refreshPairGuard 读到 pending=true 时置封锁", async () => {
    pending.mockResolvedValue(true);
    await refreshPairGuard();
    expect(pairGuardBlocked.value).toBe(true);
  });
});
