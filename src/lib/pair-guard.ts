/**
 * 配对守卫（2026-11-17 补全）：自动侦测「用户删掉了已配对的遥控器」并进入封锁态。
 *
 * 现场定死的顺序：
 *   删除 → 重新添加 → 重启   不可用
 *   删除 → 重启 → 重新添加   可用
 * 即「卸载绑定」与「重新建立绑定」之间必须隔一次干净启动。判定在后端
 * （`crates/sayall-windows/src/pair_guard.rs`，用系统开机时刻比较），这里只负责：
 *
 *   1. 定时对比"上次看到的已配对设备"与"现在的已配对设备"，少了一支就落时间戳；
 *   2. 对外暴露一个 `pairGuardBlocked`，供连接页/自定义遥控页禁用入口。
 *
 * 为什么要在前端轮询而不是后端常驻：设备删除是 Windows 设置里的动作，没有事件可订阅；
 * 而设备清单本来就要给连接页用，复用同一个扫描接口即可，不额外加系统依赖。
 */
import { ref } from "vue";

import { pairGuardNoteRemoval, pairGuardPending, scanPairedRemotes } from "./bridge";

/** 待重启期间为 true：连接、扫描、设为当前、绑定设备一律禁用。 */
export const pairGuardBlocked = ref(false);

/** 上一次观察到的已配对设备 ID（用于判断"少了一支"）。 */
const KNOWN_KEY = "freesay.pairGuard.knownIds";

export async function refreshPairGuard(): Promise<boolean> {
  try {
    pairGuardBlocked.value = await pairGuardPending();
  } catch {
    // 读不到就不拦：宁可少拦一次，也不要因为一次读失败把用户锁在门外。
    pairGuardBlocked.value = false;
  }
  return pairGuardBlocked.value;
}

function readKnown(): string[] {
  try {
    const raw = localStorage.getItem(KNOWN_KEY);
    return raw ? (JSON.parse(raw) as string[]) : [];
  } catch {
    return [];
  }
}

function writeKnown(ids: string[]): void {
  try {
    localStorage.setItem(KNOWN_KEY, JSON.stringify(ids));
  } catch {
    // 存不下就算了：下一轮会以磁盘上的实际清单重建。
  }
}

/**
 * 跑一轮侦测。
 *
 * 判定条件刻意保守：只有「上次有、这次没有、且上次非空」才算删除。首次运行
 * （上次为空）只记录基线，不触发封锁——否则新用户装完就会看到"请重启电脑"。
 */
async function checkOnce(): Promise<void> {
  try {
    const devices = await scanPairedRemotes();
    const ids = devices.map((device) => device.id);
    const known = readKnown();
    const removed = known.filter((id) => !ids.includes(id));
    if (known.length > 0 && removed.length > 0) {
      // 已经处于封锁态就不重复写时间戳：重复写会把"删除时刻"往后推，
      // 用户在重启前后各触发一次就永远解锁不了。
      if (!pairGuardBlocked.value) {
        await pairGuardNoteRemoval();
      }
      writeKnown(ids);
      await refreshPairGuard();
      return;
    }
    writeKnown(ids);
  } catch {
    // 扫描失败（蓝牙关闭、适配器忙）不改变判定，等下一轮。
  }
}

/**
 * 启动侦测循环，返回停止函数。默认 15 秒一轮：足够快到"删完就提示"，
 * 又不会让蓝牙扫描一直占着适配器。
 */
export function startPairGuardWatch(intervalMs = 15000): () => void {
  void refreshPairGuard().then(() => checkOnce());
  const timer = window.setInterval(() => void checkOnce(), intervalMs);
  return () => window.clearInterval(timer);
}

/**
 * 把「这一次看到的已配对设备」交给守卫（2026-11-17 改法）。
 *
 * 之前是守卫自己在后台每 15 秒扫一次，实测没有触发（marker 文件始终不存在）——
 * 后台扫描在无交互时经常拿不到结果。现在改成**由连接页喂数据**：页面本来就会
 * 扫描设备列表，把结果交进来即可，不需要额外一次蓝牙扫描。
 *
 * 判据：上一次记下的某支设备这次不见了 → 说明用户把它删了 → 落时间戳 + 封锁。
 */
export async function observePairedDevices(ids: string[]): Promise<void> {
  if (ids.length === 0) {
    // 空列表通常是"扫描失败/适配器忙"，不能当成"全被删了"，否则会误报。
    return;
  }
  const known = readKnown();
  const removed = known.filter((id) => !ids.includes(id));
  writeKnown(ids);
  if (known.length === 0 || removed.length === 0) {
    return;
  }
  if (pairGuardBlocked.value) {
    return;
  }
  await pairGuardNoteRemoval();
  await refreshPairGuard();
}
