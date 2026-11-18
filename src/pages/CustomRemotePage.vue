<!--
  自定义遥控器（2026-11-12 用户指定）。

  为什么单独一页：内置四款遥控器是编译期白名单，用户手上的遥控器型号无穷多。
  这一页让用户自带外形与命名——上传照片、给按键起名、点一下照片标出位置、
  再选它的**逻辑键位**；「按键」页用同一套画布渲染（同一张照片、同款卡片与箭头）。
  单击/双击/长按的动作只在「按键」页编辑，两页共享同一份 `mappings.actions`。

  动作编辑**不再在本页重复实现**（2026-11-12 用户现场：本页只有 5 个选项，按键页有
  几十个）。本页只负责"这只遥控器长什么样、有哪些键、每个键是哪个逻辑键位"，
  动作统一在「按键」页设置——同一个编辑器，选项自然完全一致，也不会出现两套配置打架。

  交互约定：
  - 绑定复用现有快捷键录入通道（`start_shortcut_capture` + 边沿订阅），只作核对回显；
  - 标点是"先选条目、再点照片"两步，避免照片点击被两种语义抢；
  - 所有改动经 400ms 防抖后整体落盘，不需要用户记得按保存。
-->
<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";

import {
  createCustomButton,
  createCustomRemote,
  formatKeys,
  loadCustomRemotes,
  persistCustomRemotes,
  readRemotePhoto,
  type CustomRemote,
  type CustomRemoteButton,
} from "../lib/custom-remote";
import {
  bondHealth,
  connectRemote,
  getConnectionSnapshot,
  getVoiceHoldHotkey,
  loadDeviceAliases,
  copyMappingProfile,
  deleteMappingProfile,
  disconnectRemote,
  openWindowsSettings,
  setActiveMappingProfile,
  scanPairedRemotes,
  startShortcutCapture,
  stopShortcutCapture,
  subscribeButtonEdges,
  subscribeShortcutCaptureEdges,
  type KeyCode,
  type PairedRemote,
  type RemoteButton,
} from "../lib/bridge";
import { pairGuardBlocked, refreshPairGuard } from "../lib/pair-guard";
import {
  classifyConnectionFailure,
  describeConnectionFailure,
  needsRepairHint,
  type ConnectionFailureKind,
} from "../lib/connect-error";
/**
 * 逻辑键位：决定动作归属与卡片图标，必须与 Rust 侧 `RemoteButton` 一一对应。
 * `voice` 是展示用的语音键（不参与 `mappings.actions`）。
 */
const SLOT_OPTIONS: Array<{ value: string; label: string }> = [
  { value: "", label: "（未指定）" },
  { value: "power", label: "电源" },
  { value: "up", label: "方向上" },
  { value: "down", label: "方向下" },
  { value: "left", label: "方向左" },
  { value: "right", label: "方向右" },
  { value: "ok", label: "确定" },
  { value: "back", label: "返回" },
  { value: "home", label: "主页" },
  { value: "menu", label: "菜单" },
  { value: "tv", label: "电视" },
  { value: "volume_up", label: "音量+" },
  { value: "volume_down", label: "音量−" },
  { value: "volume_mute", label: "静音" },
  { value: "voice", label: "语音" },
];

/**
 * 连接页设置的语音快捷键（2026-11-16）。
 *
 * 语音不走"发键"这条路：它靠 ATVV 音频链路 + 连接页的按住说话快捷键。
 * 本页语音行的"核对"因此没有任何作用，只做只读展示，避免用户以为改了能生效。
 */
const voiceHotkeyLabel = ref("连接页的按住说话快捷键");

const remotes = ref<CustomRemote[]>([]);
/** 设为当前之后要跳到「按键」页，所以本页需要发 navigate 事件（App.vue 已绑定）。 */
const emit = defineEmits<{ navigate: [request: string] }>();

const activeId = ref<string | null>(null);
const selectedId = ref<string | null>(null);
const statusText = ref("");
/** 已配对的物理遥控器（用于给自定义遥控器绑定设备）。 */
const pairedDevices = ref<PairedRemote[]>([]);
/** 设备别名（键 = 设备令牌），与「连接」页共用同一份数据。 */
const deviceAliases = ref<Record<string, string>>({});
const switchingRemote = ref(false);
/** 上一次连接失败的分类：决定状态栏文案，以及是否给「打开蓝牙设置」入口。 */
const connectFailure = ref<ConnectionFailureKind | null>(null);
const bindingButtonId = ref<string | null>(null);
const capturedKeys = ref<KeyCode[]>([]);
const markingButtonId = ref<string | null>(null);
/** 名称输入框：失焦后变灰只读，点击再编辑（2026-11-17 用户要求）。 */
const editingName = ref(false);
const photoInput = ref<HTMLInputElement | null>(null);

let unsubscribeEdges: (() => void) | null = null;
/** 报告层边沿的订阅：返回 / 音量± 这类键在 Windows 侧**没有键盘事件**，
 *  只走 `button-edge`（`[EDGE]` 那条通道），键盘钩子永远看不到。 */
let unsubscribeButtons: (() => void) | null = null;
let saveTimer: number | null = null;

/**
 * 一次按压结束后自动收尾的静默窗口。
 *
 * 2026-11-13 现场：点「按一下」→ 按遥控器键 → 界面毫无变化（结果要等用户再点一次
 * 按钮才回填）→ 用户以为"没检测到"，再点一次时捕获列表已被重置。遥控器一次按压
 * 会连发修饰键 + 主键（如 Ctrl、M 两条边沿），彼此间隔几十毫秒，因此以**最后一次
 * 边沿之后** 600ms 无新键作为"这一组录完了"，自动结束并回填。
 */
const BINDING_IDLE_MS = 600;
let bindingIdleTimer: number | null = null;

/**
 * 录入态的绝对上限。进入录入后一按键都没按（用户点了按钮就去干别的），静默
 * 计时器永远不会启动，而拦截是全局的——回来按键盘会被无声吞掉。这里兜一层硬超时。
 */
const BINDING_TIMEOUT_MS = 15_000;
let bindingTimeoutTimer: number | null = null;
/** 录入 4 秒仍无按键时，把"可能不是当前遥控器"的原因直接说出来。 */
let bindingHintTimer: number | null = null;

function clearBindingIdleTimer(): void {
  if (bindingIdleTimer !== null) {
    window.clearTimeout(bindingIdleTimer);
    bindingIdleTimer = null;
  }
  if (bindingTimeoutTimer !== null) {
    window.clearTimeout(bindingTimeoutTimer);
    bindingTimeoutTimer = null;
  }
}

function scheduleBindingIdleFinish(): void {
  clearBindingIdleTimer();
  bindingIdleTimer = window.setTimeout(() => {
    bindingIdleTimer = null;
    void finishBinding();
  }, BINDING_IDLE_MS);
}

/**
 * 录入态必须在窗口失焦时立即结束。
 *
 * 拦截通道是**全系统**生效的低级键盘钩子；用户切到别的程序后若还留着录入态，
 * 那边所有按键都会先被本应用裁决（现场："切到其他程序也没办法输入"）。所以失焦
 * 即收尾，宁可这一组没录完，也不能让别人的键盘停摆。
 */
function handleWindowBlur(): void {
  if (bindingButtonId.value) void finishBinding();
}

const selected = computed<CustomRemote | null>(
  () => remotes.value.find((remote) => remote.id === selectedId.value) ?? null,
);

const markingButton = computed<CustomRemoteButton | null>(() => {
  const remote = selected.value;
  if (!remote || !markingButtonId.value) return null;
  return remote.buttons.find((button) => button.id === markingButtonId.value) ?? null;
});

onMounted(async () => {
  const snapshot = await loadCustomRemotes();
  remotes.value = snapshot.remotes;
  activeId.value = snapshot.activeId;
  selectedId.value = snapshot.activeId ?? snapshot.remotes[0]?.id ?? null;
  window.addEventListener("blur", handleWindowBlur);
  // 绑定选择器需要设备列表与别名；失败不阻塞页面（只是选择器为空）。
  void refreshDevices();
  void refreshPairGuard();
  void refreshVoiceHotkey();
});

async function refreshVoiceHotkey() {
  try {
    const chord = await getVoiceHoldHotkey();
    if (chord && chord.keys.length > 0) {
      voiceHotkeyLabel.value = formatKeys(chord.keys);
    }
  } catch {
    // 读不到就保持默认文案，不影响页面其它功能。
  }
}

onBeforeUnmount(() => {
  unsubscribeEdges?.();
  unsubscribeButtons?.();
  clearBindingIdleTimer();
  if (bindingHintTimer !== null) {
    window.clearTimeout(bindingHintTimer);
    bindingHintTimer = null;
  }
  window.removeEventListener("blur", handleWindowBlur);
  void stopShortcutCapture().catch(() => undefined);
  // 离场必须把待写的改动发出去：绑定 / 设为当前 / 最后 400ms 内的编辑都在这一笔里。
  void saveNow();
});

/**
 * 立即落盘。
 *
 * 2026-11-14 现场：绑定/设为当前之后用户马上切到「连接」页确认，回来后**绑定丢了**，
 * 「设为当前」按钮变灰再也点不动。原因是改动统一走 400ms 防抖，而 `onBeforeUnmount`
 * 只是 `clearTimeout`——没等它跑，最后一笔改动（尤其是 `boundDeviceToken` 与
 * `activeId`）就整条丢了，下一次任何改动再把这个"未绑定"的快照写回磁盘坐实。
 */
async function saveNow(): Promise<void> {
  if (saveTimer !== null) {
    window.clearTimeout(saveTimer);
    saveTimer = null;
  }
  try {
    await persistCustomRemotes(remotes.value, activeId.value);
    statusText.value = "已保存";
  } catch (error) {
    statusText.value = `保存失败：${error instanceof Error ? error.message : String(error)}`;
  }
}

/** 连续改动（拖动坐标、逐字改名）走这里：防抖后整体落盘，避免写爆磁盘。 */
function scheduleSave(): void {
  if (saveTimer !== null) window.clearTimeout(saveTimer);
  saveTimer = window.setTimeout(() => {
    saveTimer = null;
    void saveNow();
  }, 400);
}

watch(remotes, scheduleSave, { deep: true });
watch(activeId, scheduleSave);

/** 自定义遥控器数量上限（与后端 `MAX_CUSTOM_REMOTES` 一致）。 */
const MAX_CUSTOM_REMOTES = 1;

/**
 * 「新建遥控器」入口。
 *
 * 2026-11-15 修正：**删掉了「必须先把系统里已配对的遥控器删掉」这道门禁**。
 *
 * 那道门禁建立在「两支同型号遥控器不能同时配对、语音会互相顶掉」的结论上，而该结论
 * 已被证伪：真实原因是 Windows 绑定记录里 `KeyLength = 0` 导致链路无法加密（同机正常
 * 工作的耳机是 16），与"同时配了几支"毫无关系。据此设的"只能有一个自定义遥控器"也
 * 只是因为当时无法解释故障而采取的保守限制。
 *
 * 现在：系统里保留已配对遥控器也能新建自定义遥控器——自定义遥控器只是「外观 + 按键
 * 布局」，实际连哪一支由「绑定的遥控器」下拉决定。数量上限暂时仍为 1（多只留到下一版）。
 */
async function requestAddRemote(): Promise<void> {
  if (remotes.value.length >= MAX_CUSTOM_REMOTES) return;
  // 注意：**不要在这里扫设备**。`scanPairedRemotes()` 会做一次蓝牙设备枚举，
  // 与正在跑的 ATVV 音频会话抢 BLE 栈——2026-11-15 现场表现为"新建遥控器后
  // 用原遥控器说话，识别内容完全不对、像信号很弱"。
  // 下拉的刷新由展开时触发（`refreshDevicesOnOpen`，带 2.5 秒节流）负责，足够。
  addRemote();
}

function addRemote(): void {
  if (remotes.value.length >= MAX_CUSTOM_REMOTES) return;
  const remote = createCustomRemote("我的遥控器");
  remotes.value = [remote];
  selectedId.value = remote.id;
  statusText.value = "已新建。请上传照片、给按键起名，再在「按键」页设置动作";
}

/**
 * 判断一支设备是不是内置型号（小米蓝牙遥控器 2 / 2 Pro）。
 *
 * 口径与后端 `remote_model_from_name` 保持一致：中文系统上报名形如
 * 「小米蓝牙语音遥控器 2 Pro」，广告名形如 "Xiaomi Bluetooth Remote 2 Pro" / "ARN9"。
 * 只用于"删除布局后该连回谁"这一处决策，不参与任何语音能力判断
 * （语音能力始终由连接阶段是否发现 ATVV 服务决定）。
 */
function isBuiltinRemote(device: PairedRemote): boolean {
  const name = (device.name ?? "").toLowerCase();
  if (name.includes("小米") && name.includes("遥控器")) return true;
  if (/xiaomi\s*bluetooth\s*remote/.test(name)) return true;
  if (/\barn9\b/.test(name)) return true;
  // 2026-11-15 实测：系统重启后，这两支设备的名称会变成广播名「MI RC」
  // （不再上报「小米蓝牙语音遥控器 2 Pro」）。只按中文名判断会漏判，
  // 导致"删除自定义遥控器后把内置遥控器当成第三方设备断开"。
  if (/^mi\s*rc$/.test(name.replace(/\s+/g, " ").trim())) return true;
  if (name.includes("mi rc") || name.includes("mirc")) return true;
  // 设备 id 里带小米的 VID（0x2717）也视为内置型号的设备节点
  return /vid&0*2717/i.test(device.id ?? "");
}

/**
 * 删除一只自定义遥控器。
 *
 * 删除的是**布局配置**，不是"这台电脑与设备的配对关系"——所以这里只做断开/切换连接，
 * **绝不解配对**（解配对会让设备从系统里消失，下次要用得重新配对，而重配正是把
 * `KeyLength` 写坏的那条路）。
 *
 * 连接归属按下面的顺序决定（2026-11-15 用户现场要求）：
 *   1. 被删布局绑定的设备本身就是内置型号（小米 2 / 2 Pro）→ 它本来就是"默认遥控器"，
 *      **连接保持不动**，只回内置布局 —— 用户几乎无感；
 *   2. 否则（绑定的是第三方遥控器），在已配对设备里找另一支内置型号 → **切回它**；
 *   3. 一支内置型号都没有 → **断开**，连接页回到"待选择"，绝不停在一个已无布局的设备上。
 */
async function removeRemote(remote: CustomRemote): Promise<void> {
  const removedToken = remote.boundDeviceToken?.trim() ?? "";
  const wasActive = activeId.value === remote.id;

  remotes.value = remotes.value.filter((item) => item.id !== remote.id);
  if (wasActive) activeId.value = null;
  if (selectedId.value === remote.id) selectedId.value = remotes.value[0]?.id ?? null;

  // 档案一并清理：否则 mapping-profiles/ 里留下孤儿文件，当前档案也可能指向它。
  let profileFellBack = false;
  try {
    const fallback = await deleteMappingProfile(remote.id);
    profileFellBack = Boolean(fallback);
  } catch (error) {
    console.warn("[custom-remote] 删除按键配置档案失败", error);
  }

  if (!wasActive) return;

  try {
    const devices = await scanPairedRemotes();
    const removedDevice = devices.find((item) => item.token === removedToken) ?? null;
    const otherBuiltin = devices.find(
      (item) => item.token !== removedToken && isBuiltinRemote(item),
    );

    if (removedDevice && isBuiltinRemote(removedDevice)) {
      statusText.value = "已删除布局，继续使用内置遥控器的预设布局";
      return;
    }
    if (otherBuiltin) {
      await connectRemote(otherBuiltin.id);
      statusText.value = `已删除，切回「${deviceDisplayName(otherBuiltin)}」`;
      return;
    }
    await disconnectRemote();
    statusText.value = removedToken
      ? "已删除并断开这支遥控器；系统里没有其他小米遥控器，请到「连接」页选择设备"
      : "已删除，按键配置已回到「内置遥控器」那一套";
  } catch (error) {
    console.warn("[custom-remote] 删除后恢复连接失败", error);
    statusText.value = `已删除；但恢复连接失败：${error instanceof Error ? error.message : String(error)}`;
  }
  if (profileFellBack) {
    statusText.value += "（按键配置已回到内置那一套）";
  }
}

/**
 * 设为当前 = 连接它绑定的那支物理遥控器 + 切到这套布局。
 *
 * 2026-11-14：以前这里只改 `activeId`，跟「连接」页选的设备毫无关系，
 * 于是出现"布局切了、连的还是另一支"。现在两步要么都成立，要么都不做。
 */
async function useRemote(remote: CustomRemote): Promise<void> {
  // 2026-11-17：删过遥控器未重启时禁止切换——此时建立的新绑定是脏的。
  if (pairGuardBlocked.value) {
    statusText.value = "已删除遥控器，请先重启电脑，再回来切换或绑定遥控器。";
    return;
  }
  if (switchingRemote.value) return;
  switchingRemote.value = true;
  connectFailure.value = null;
  statusText.value = "正在连接绑定的遥控器…";
  // 提到 try 之外：catch 里的自动修复也要用它（设备 id 与显示名）。
  let device: PairedRemote | null = null;
  try {
    let token = remote.boundDeviceToken?.trim() ?? "";
    // 未绑定时的回落（2026-11-14 现场）：绑定字段曾经根本没落盘，用户看到的就是
    // 按钮永远变灰点不动。现在允许"没绑定也能设为当前"——用「连接」页此刻真正
    // 连着的那支遥控器，并把它顺手绑到这只自定义遥控器上（下次就不用再选）。
    // 只有"既没绑定、当前也没连着任何设备"时才要求用户先连一支。
    if (!token) {
      const snapshot = await getConnectionSnapshot().catch(() => null);
      token = snapshot?.remoteToken?.trim() ?? "";
      if (!token) {
        statusText.value =
          "还没有选中的遥控器。请先到「连接」页连上一支，或者在右侧「绑定的遥控器」里指定一支。";
        return;
      }
      remote.boundDeviceToken = token;
    }
    if (pairedDevices.value.length === 0) await refreshDevices();
    device = pairedDevices.value.find((item) => item.token === token) ?? null;
    if (!device) {
      // 列表可能是页面挂载时缓存的：设备刚配对完 / 刚从系统里删掉都还没同步。
      // 这里强刷一次再找，避免"明明在系统里，却提示不在已配对列表"。
      await refreshDevices();
      device = pairedDevices.value.find((item) => item.token === token) ?? null;
    }
    if (!device) {
      statusText.value =
        "绑定的那支遥控器不在已配对列表里（可能已取消配对或不在范围内）。请重新绑定后再试。";
      return;
    }
    // 连接失败（例如语音服务被系统拒绝）时不切布局：宁可保持原样，也不能
    // 出现"界面说切了、实际连的是另一支"。
    await connectRemote(device.id);
    await finishSwitch(remote, device);
  } catch (error) {
    // 不把后端的英文 + 状态码原文丢给用户（2026-11-14 现场：界面显示
    // "Xiaomi voice remote GATT operation failed: 发现 ATVV 特征返回状态 3"，
    // 用户既看不懂也不知道该做什么）。分类后给一句可照做的中文。
    console.error("[custom-remote] 连接绑定的遥控器失败", error);
    const kind = classifyConnectionFailure(error);
    connectFailure.value = kind;
    // 2026-11-15：`access_denied` 的本体是绑定记录里 `KeyLength=0`（每次配对都会
    // 被写成 0），与"设备没连上"无关。这里自动修一次再重连，用户不该为这件事
    // 去开管理员窗口跑脚本。每台设备每会话只试一次，避免失败时反复重启蓝牙服务。
    if (kind === "access_denied" && device && !autoRepairTried.has(device.id)) {
      autoRepairTried.add(device.id);
      statusText.value = "检测到这台设备的蓝牙加密记录异常，正在自动修复…";
      const health = await bondHealth(device.id, true).catch(() => null);
      if (health?.result === "repaired") {
        statusText.value = "已修复蓝牙加密记录（蓝牙服务已重启），正在重新连接…";
        try {
          await connectRemote(device.id);
          await finishSwitch(remote, device);
          return;
        } catch (retryError) {
          console.warn("[custom-remote] 修复后重连仍失败", retryError);
          connectFailure.value = classifyConnectionFailure(retryError);
        }
      } else if (health?.result === "restart_failed") {
        // 记录已改成 16，但蓝牙服务没重启成功：数据是对的，只是内核还没重载。
        statusText.value =
          "已修复蓝牙加密记录，但蓝牙服务没能重启。请重启电脑（或重新配对一次）后再试语音。";
      } else if (health?.result === "ltk_invalid") {
        statusText.value =
          "这台设备的蓝牙绑定已损坏（密钥为空），自动修复无法恢复。请在「设置 → 蓝牙和其他设备」里删除它并重新配对。";
      }
    }
    // 2026-11-14 用户现场追问："Windows 里两支都显示已连接、快捷键也能识别，
    // 为什么说拒绝访问？" —— 因为失败的是**语音链路**（ATVV over GATT），
    // 而布局与按键映射完全走 HID 报告，不需要这条链路。
    // 所以布局照切，语音链路单独报：否则用户点一次「设为当前」就卡在原地，
    // 明明能按的按键也设置不了。
    activeId.value = remote.id;
    void saveNow();
    await applyMappingProfile(remote.id);
    statusText.value = `已切到「${remote.name || "这只遥控器"}」的布局；语音链路这次没连上。${describeConnectionFailure(kind)}`;
  } finally {
    switchingRemote.value = false;
  }
}

/** 状态栏里的「打开蓝牙设置」：配对记录坏掉时唯一需要用户动手的地方。 */
async function openBluetoothSettings(): Promise<void> {
  try {
    await openWindowsSettings("bluetooth");
  } catch (error) {
    console.error("[custom-remote] 打开蓝牙设置失败", error);
  }
}

/**
 * 切到这只遥控器自己的按键配置档案（2026-11-14 用户现场）：
 * 原来动作表按"槽位"全局存一份，A 遥控器配的 `up = Alt+W` 会串到 B 上。
 * 现在每只自定义遥控器一个档案，切换即生效；失败只记日志，不影响布局切换。
 */
/**
 * 把内置遥控器那套配置复制到这只自定义遥控器（2026-11-14）。
 *
 * 新档案默认是空白的（独立性优先），但很多用户的按键用法与内置遥控器一致，
 * 从空白开始一个个配太麻烦——这个按钮就是"显式复用"的出口。
 */
async function copyBuiltinInto(remote: CustomRemote): Promise<void> {
  try {
    await copyMappingProfile("builtin", remote.id);
    statusText.value =
      remote.id === activeId.value
        ? `已把内置配置复制到「${remote.name || "这只遥控器"}」，并已生效`
        : `已把内置配置复制到「${remote.name || "这只遥控器"}」，切到它时生效`;
  } catch (error) {
    console.error("[custom-remote] 复制内置配置失败", error);
    statusText.value = `复制失败：${error instanceof Error ? error.message : String(error)}`;
  }
}

/** 已尝试过自动修复的设备（每会话每设备一次，避免失败时反复重启蓝牙服务）。 */
const autoRepairTried = new Set<string>();

/** 连接成功后的统一收尾：切布局、落盘、切按键档案。 */
async function finishSwitch(remote: CustomRemote, device: PairedRemote): Promise<void> {
  activeId.value = remote.id;
  statusText.value = `已连接「${deviceDisplayName(device)}」，这套布局已生效`;
  // `activeId` 必须马上落盘，否则切页就回退成"没有当前遥控器"。
  // 2026-11-17：原来是 `void saveNow()`，与下面的 applyMappingProfile 竞态——
  // 后端读不到刚落盘的档案就切不了配置，表现就是"按键页没跟着切"。
  await saveNow();
  // 按键配置也跟着切档案：这只遥控器改过的快捷键只对它自己生效。
  const actionCount = await applyMappingProfile(remote.id);
  if (actionCount === 0) {
    // 新遥控器是**空白配置**（独立性优先）。不说清楚就会被当成"按键全失效"。
    statusText.value += "。这只遥控器还没有任何按键配置——要沿用内置那套，点顶部的「复制内置配置」。";
  }
  // 切完直接进「按键」页：这套布局的按键就在那一页，不用再自己找入口。
  emit("navigate", "buttons");
}

async function applyMappingProfile(profileId: string): Promise<number | null> {
  try {
    const mappings = await setActiveMappingProfile(profileId);
    return mappings.actions ? Object.keys(mappings.actions).length : 0;
  } catch (error) {
    console.error("[custom-remote] 切换按键配置失败", error);
    return null;
  }
}

function deviceDisplayName(device: PairedRemote): string {
  const alias = device.token ? deviceAliases.value[device.token]?.trim() : "";
  return alias ? alias : device.name;
}

/** 列表副标题：这只自定义遥控器绑了哪支物理设备。 */
function boundDeviceLabel(remote: CustomRemote): string {
  const token = remote.boundDeviceToken?.trim();
  if (!token) return "未绑定设备";
  const device = pairedDevices.value.find((item) => item.token === token);
  return device ? `已绑定 ${deviceDisplayName(device)}` : `已绑定 #${token.slice(0, 6)}（未在线）`;
}

/**
 * 展开「绑定的遥控器」下拉前重新扫一次。
 *
 * 2026-11-14 现场：用户在 Windows 设置里删掉一支遥控器后回到本页，下拉仍显示旧设备，
 * 必须手点「刷新设备」才消失——而新建流程恰恰以"系统里已没有遥控器"为前提，
 * 留着旧项会让人以为删失败了。这里在下拉获得焦点时静默刷新，并做 2.5 秒节流
 * （枚举蓝牙设备本身要几百毫秒，不能每次点都全量扫）。
 */
let lastDeviceRefreshAt = 0;

function refreshDevicesOnOpen(): void {
  const now = Date.now();
  if (now - lastDeviceRefreshAt < 2500) return;
  lastDeviceRefreshAt = now;
  void refreshDevices();
}

async function refreshDevices(): Promise<void> {
  lastDeviceRefreshAt = Date.now();
  try {
    const [devices, aliases] = await Promise.all([
      scanPairedRemotes(),
      loadDeviceAliases().catch(() => ({})),
    ]);
    pairedDevices.value = devices;
    deviceAliases.value = aliases ?? {};
    // 绑定失效清理（2026-11-14 现场）：用户在系统里把原来那支删掉后，自定义遥控器
    // 的 `boundDeviceToken` 仍指着旧设备令牌，于是"设为当前"一直去连一个已经不存在的
    // 设备，表现为"连上了新遥控器，但界面还在用上一个遥控器的配置"。
    for (const remote of remotes.value) {
      const token = remote.boundDeviceToken?.trim();
      if (!token) continue;
      if (!devices.some((device) => device.token === token)) {
        remote.boundDeviceToken = null;
        statusText.value =
          "原来绑定的那支遥控器已经不在系统里了，已解除绑定。请重新选择「绑定的遥控器」。";
      }
    }
  } catch (error) {
    pairedDevices.value = [];
    statusText.value = `读取已配对设备失败：${error instanceof Error ? error.message : String(error)}`;
  }
}

/** 绑定/解绑当前编辑的这只自定义遥控器。 */
function bindDevice(token: string): void {
  const remote = selected.value;
  if (!remote) return;
  remote.boundDeviceToken = token ? token : null;
  statusText.value = token ? "已绑定，点右上角圆钮即可「设为当前」并连接它" : "已解除绑定";
  // 绑定是离散动作，不给防抖留窗口（用户很可能立刻切页去看连接状态）。
  void saveNow();
}

async function pickPhoto(event: Event): Promise<void> {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  const remote = selected.value;
  input.value = "";
  if (!file || !remote) return;
  try {
    const photo = await readRemotePhoto(file);
    remote.image = photo.dataUrl;
    remote.imageWidth = photo.width;
    remote.imageHeight = photo.height;
    statusText.value = `照片已载入（${photo.width}×${photo.height}），现在可以标点`;
  } catch (error) {
    statusText.value = `照片读取失败：${error instanceof Error ? error.message : String(error)}`;
  }
}

/** 选中键位时顺手把空名称填上，减少重复输入。 */
function setSlot(button: CustomRemoteButton, value: string): void {
  button.slot = value === "" ? null : value;
  if (!button.name.trim()) {
    const option = SLOT_OPTIONS.find((item) => item.value === value);
    if (option && option.value) button.name = option.label;
  }
}

function addButton(): void {
  const remote = selected.value;
  if (!remote) return;
  remote.buttons = [...remote.buttons, createCustomButton("")];
  statusText.value = "已加一个按键：填名称、选键位，再点「标点」在照片上定位";
}

function removeButton(button: CustomRemoteButton): void {
  const remote = selected.value;
  if (!remote) return;
  remote.buttons = remote.buttons.filter((item) => item.id !== button.id);
  if (markingButtonId.value === button.id) markingButtonId.value = null;
}

/** 进入绑定态：录到的第一个按下沿序列即为该按键（只作核对回显）。 */
async function beginBinding(button: CustomRemoteButton): Promise<void> {
  if (bindingButtonId.value === button.id) {
    await finishBinding();
    return;
  }
  // 换绑到别的键：先把上一组收尾，避免两个按钮同时挂着一个录入订阅。
  if (bindingButtonId.value) await finishBinding();
  bindingButtonId.value = button.id;
  capturedKeys.value = [];
  markingButtonId.value = null;
  try {
    await startShortcutCapture();
    unsubscribeEdges = await subscribeShortcutCaptureEdges((edge) => {
      if (!edge.isPressed) return;
      // 2026-11-13：**注入边沿必须收**。RC003 走报告层时，遥控器按键是以
      // `LLKHF_INJECTED` 合成进系统的（`synth=0x01` 路径），后端已经在录入期
      // 明确接受这类副本（见 `key_gate::injected_edge_is_passthrough`），前端
      // 若再按 `source === "injected"` 丢弃，用户按遥控器就会"完全没反应"。
      // 自家映射注入不会出现在这里：核对期间没有任何语音会话在跑。
      if (capturedKeys.value.includes(edge.key)) return;
      capturedKeys.value = [...capturedKeys.value, edge.key];
      statusText.value = `已捕获 ${formatKeys(capturedKeys.value)}，继续按或稍候自动完成`;
      // 每条边沿都重置静默计时：遥控器一次按压会连发修饰键 + 主键。
      scheduleBindingIdleFinish();
    });
    // 报告层边沿（2026-11-13 现场）：返回 / 音量± / 主页 / 菜单这些键
    // **在 Windows 侧不产生任何键盘事件**（kbdhid 没有对应 VK 映射，直接被丢弃），
    // 所以上面的键盘钩子永远看不到它们 —— 用户点「核对」按这三个键会一直显示
    // "请按一下这个遥控器上的该键"，然后超时退出，看起来就是"捕捉不到"。
    // 它们只存在于 `button-edge`（助手在 HID 报告层识别出的 usage）。
    unsubscribeButtons = await subscribeButtonEdges((edge) => {
      if (!edge.isPressed) return;
      // 同一次按压可能两条通道都报（例如电源键在 OS 侧有键位映射）。
      // 以报告层为准直接替换，避免录成 "电源 + 回车" 这种双份。
      const key = edge.button as KeyCode;
      capturedKeys.value = [key];
      statusText.value = `已捕获 ${formatKeys(capturedKeys.value)}，松手稍候自动完成`;
      scheduleBindingIdleFinish();
    });
    statusText.value = "请按一下这个遥控器上的该键";
    // 2026-11-14：返回 / 音量± / 电源这类键**只存在于 HID 报告层**，而报告层
    // 只覆盖"当前使用的那支遥控器"。给另一支遥控器配键时，按这些键永远没有反应，
    // 现场看到的就是"捕捉不到"。4 秒还没收到按键时直接把原因说出来，省得反复试。
    if (bindingHintTimer !== null) window.clearTimeout(bindingHintTimer);
    bindingHintTimer = window.setTimeout(() => {
      bindingHintTimer = null;
      if (bindingButtonId.value === button.id && capturedKeys.value.length === 0) {
        statusText.value =
          "还没收到按键。如果一直没反应，很可能这支遥控器不是「当前使用」的那支：返回 / 音量± / 电源这类键只有当前遥控器才能被识别。请先到「连接」页把它设为当前，再回来录入。";
      }
    }, 4000);
    // 硬超时：一按键都没按也要收尾，否则拦截会一直挂着。
    clearBindingIdleTimer();
    bindingTimeoutTimer = window.setTimeout(() => {
      bindingTimeoutTimer = null;
      void finishBinding();
    }, BINDING_TIMEOUT_MS);
  } catch (error) {
    bindingButtonId.value = null;
    clearBindingIdleTimer();
    statusText.value = `无法开始录入：${error instanceof Error ? error.message : String(error)}`;
  }
}

async function finishBinding(): Promise<void> {
  // 幂等：idle 计时器、按钮二次点击、窗口失焦三条路径都可能触发收尾。
  if (!bindingButtonId.value) return;
  const remote = selected.value;
  const target = remote?.buttons.find((button) => button.id === bindingButtonId.value);
  clearBindingIdleTimer();
  if (bindingHintTimer !== null) {
    window.clearTimeout(bindingHintTimer);
    bindingHintTimer = null;
  }
  unsubscribeEdges?.();
  unsubscribeEdges = null;
  unsubscribeButtons?.();
  unsubscribeButtons = null;
  bindingButtonId.value = null;
  await stopShortcutCapture().catch(() => undefined);
  if (!target) return;
  const keys = capturedKeys.value;
  if (keys.length === 0) {
    statusText.value = "没有捕获到按键，请重试";
    return;
  }
  target.keys = keys;
  statusText.value = `「${target.name || "未命名按键"}」已绑定：${formatKeys(keys)}`;
}

/** 进入标点态：下一次点击照片即写入该条目的相对坐标。 */
function beginMarking(button: CustomRemoteButton): void {
  markingButtonId.value = markingButtonId.value === button.id ? null : button.id;
  statusText.value = markingButtonId.value ? "请在照片上点一下这个按键的位置" : "";
}

function markAt(event: MouseEvent): void {
  const button = markingButton.value;
  if (!button) return;
  const target = event.currentTarget as HTMLElement;
  const rect = target.getBoundingClientRect();
  button.x = Math.min(1, Math.max(0, (event.clientX - rect.left) / rect.width));
  button.y = Math.min(1, Math.max(0, (event.clientY - rect.top) / rect.height));
  // `side` 自 2026-11-12 起只作历史字段保留：按键页改成「按添加顺序先排满左列、
  // 再排右列」（见 `lib/mapping-columns`），卡片在照片哪一侧不再由这个横坐标决定，
  // 照片上的橙点仍标在真实位置。这里照旧写入，是为了旧数据格式与回滚都能读。
  button.side = button.x < 0.5 ? "left" : "right";
  statusText.value = `坐标已记录：${(button.x * 100).toFixed(1)}% / ${(button.y * 100).toFixed(1)}%`;
  markingButtonId.value = null;
}

</script>

<template>
  <section class="custom-remote-page">
    <header class="page-header">
      <div>
        <h1>自定义遥控器</h1>
        <p class="subtitle">
          上传你在用的遥控器照片，给每个按键起名、选它的键位，再在照片上标出位置。
          绑定后的按键会和内置遥控器一样出现在「按键」页，动作也都在那一页设置。
        </p>
        <p class="subtitle-note">
          新建自定义遥控器不需要删除系统里已配对的遥控器：这里定义的只是「外观 + 按键布局」，
          实际连接哪一支由下面的「绑定的遥控器」决定。
          没有自定义遥控器时，「按键」页使用内置遥控器（小米 2 / 2 Pro）的预设布局。
        </p>
      </div>
      <button
        type="button"
        class="primary"
        :disabled="remotes.length >= MAX_CUSTOM_REMOTES"
        :title="
          remotes.length >= MAX_CUSTOM_REMOTES
            ? '最多只能有一个自定义遥控器；要新建请先删除现有的那只'
            : '添加自定义遥控器'
        "
        @click="requestAddRemote"
      >
        {{ remotes.length >= MAX_CUSTOM_REMOTES ? "已有自定义遥控器" : "新建遥控器" }}
      </button>
    </header>

    <div class="layout">
            <!-- 顶部横向区（2026-11-17 用户定稿）：遥控器条目横向排列（不要缩略图），
           最右侧是大图 + 更换照片按钮（按钮在图区右上角）。 -->
      

      <div v-if="selected" class="editor-grid">
        <section class="card meta-card">
          <div class="meta-row meta-title-row">
            <input
              v-model="selected.name"
              class="name-input"
              :class="{ idle: !editingName }"
              :readonly="!editingName"
              :title="editingName ? '' : '点击可编辑'"
              placeholder="遥控器名称"
              @focus="editingName = true"
              @blur="editingName = false"
            />
            <span class="meta-tags">
              {{ selected.buttons.length }} 个按键 · {{ boundDeviceLabel(selected) }}
              <span v-if="selected.id === activeId" class="state-chip">当前使用中</span>
            </span>
            <button
              v-if="selected.id !== activeId"
              type="button"
              class="btn"
              :disabled="switchingRemote"
              @click="useRemote(selected)"
            >
              {{ switchingRemote ? "切换中…" : "设为当前" }}
            </button>
            <button type="button" class="btn" @click="copyBuiltinInto(selected)">复制内置配置</button>
            <button type="button" class="btn danger" @click="removeRemote(selected)">删除</button>
          </div>
          <div class="meta-row">
            <select
              class="bind-select"
              :value="selected.boundDeviceToken ?? ''"
              :disabled="pairGuardBlocked"
              @focus="refreshDevicesOnOpen"
              @change="bindDevice(($event.target as HTMLSelectElement).value)"
            >
              <option value="">未绑定设备</option>
              <option v-for="device in pairedDevices" :key="device.id" :value="device.token">
                {{ deviceDisplayName(device) }} · #{{ device.token?.slice(0, 6) }}
              </option>
            </select>
            <button type="button" class="btn refresh-btn" title="刷新设备" @click="refreshDevices">↻</button>
          </div>
        </section>

        <section class="card key-table">
          <div class="key-head-row">
            <h3 class="key-title">按键</h3>
            <button type="button" class="btn primary" @click="addButton">添加按键</button>
          </div>

                  <div class="table-wrap">
                <table class="button-table">
                  <colgroup>
                  <col style="width: 22%" />
                  <col style="width: 20%" />
                  <col style="width: 25%" />
                  <col style="width: 24%" />
                  <col style="width: 9%" />
                  </colgroup>
                  <thead>
                    <tr>
                      <th>名称</th>
                      <th>键位</th>
                      <th>核对</th>
                      <th>位置</th>
                      <th></th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr v-for="button in selected.buttons" :key="button.id">
                      <td><input v-model="button.name" type="text" placeholder="电源 / 上 / 语音…" /></td>
                      <td>
                        <!-- 语音键：键位固定为「语音」，不给改（2026-11-16） -->
                        <span v-if="button.slot === 'voice'" class="voice-locked">语音</span>
                        <select v-else :value="button.slot ?? ''" @change="setSlot(button, ($event.target as HTMLSelectElement).value)">
                          <option v-for="option in SLOT_OPTIONS" :key="option.value" :value="option.value">
                            {{ option.label }}
                          </option>
                        </select>
                      </td>
                      <td>
                        <!-- 语音键不吃"发键"这条路：实际触发的快捷键在「连接」页设置，
                             这里只做只读展示，避免"改了这里却没生效"的误导（2026-11-16）。 -->
                        <span v-if="button.slot === 'voice'" class="voice-locked" :title="'在「连接」页修改'">
                          {{ voiceHotkeyLabel }}
                        </span>
                        <button
                          v-else
                          type="button"
                          class="ghost key-bind"
                          :class="{ recording: bindingButtonId === button.id }"
                          :title="formatKeys(button.keys) || '点击后按键录入'"
                          @click="beginBinding(button)"
                        >
                          {{ bindingButtonId === button.id ? "…" : formatKeys(button.keys) || "录入" }}
                        </button>
                      </td>
                      <td>
                        <button type="button" class="ghost" :class="{ recording: markingButtonId === button.id }" @click="beginMarking(button)">
                          {{ markingButtonId === button.id ? "…" : "◎" }}
                        </button>
                      </td>
                      <!-- 2026-11-13 用户要求删除：单击/双击/长按三列在本页只是「按键」页的
                           只读镜像，点了还要跳页，占地方又没有可操作性。动作统一在「按键」页
                           设置，本页只负责外形、命名与键位归属。 -->
                      <td>
                        <button
                          type="button"
                          class="icon-button danger"
                          title="删除这个按键"
                          aria-label="删除这个按键"
                          @click="removeButton(button)"
                        >
                          <svg viewBox="0 0 24 24" fill="none" aria-hidden="true">
                            <path d="M4.5 7h15M9.5 7V4.8h5V7M6.8 7l.8 12.2h8.8L17.2 7" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
                          </svg>
                        </button>
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>
          <p v-if="selected && selected.buttons.length === 0" class="empty">还没有按键，点「添加按键」逐个加。</p>
        </section>

        <section class="card photo-card">
          <button type="button" class="btn upload-btn" @click="photoInput?.click()">
            {{ selected?.image ? "更换" : "上传" }}
          </button>
          <div class="photo-frame" :class="{ empty: !selected?.image }">
            <div v-if="selected?.image" class="photo-wrap" :class="{ marking: !!markingButton }" @click="markAt">
              <img :src="selected.image ?? ''" :alt="selected.name ?? '未命名'" draggable="false" />
              <span
                v-for="button in selected.buttons"
                :key="button.id"
                class="dot"
                :class="{ armed: markingButtonId === button.id }"
                :style="{ left: `${button.x * 100}%`, top: `${button.y * 100}%` }"
                :title="button.name"
              />
            </div>
            <p v-else class="photo-placeholder">上传照片后<br />在图上点选按键位置</p>
          </div>
          <input ref="photoInput" type="file" accept="image/*" hidden @change="pickPhoto" />
        </section>

        <p class="hint hint-footer" :title="`键位只决定图标与统计；实际按下发送什么键由「核对」决定。语音键触发的是「连接」页的按住说话快捷键。${selected.boundDeviceToken ? ' 返回/音量±/电源需先设为当前才能录入。' : ''}`">
          键位只决定图标与统计；实际按下的键由「核对」决定；语音键走「连接」页的快捷键。
        </p>
      </div>
<main v-else class="editor empty-editor">点右上角「新建遥控器」开始：上传照片、加按键、绑定设备。</main>
    </div>

    <footer v-if="statusText" class="status">
      <span>{{ statusText }}</span>
      <button
        v-if="connectFailure && needsRepairHint(connectFailure)"
        type="button"
        class="ghost"
        @click="openBluetoothSettings"
      >
        打开蓝牙设置
      </button>
    </footer>
  </section>
</template>

<style scoped>
.custom-remote-page {
  display: flex;
  flex-direction: column;
  gap: 18px;
  padding: 24px 28px 32px;
  color: var(--text-primary);
  overflow-y: auto;
}

.page-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
}

.page-header > button,
.page-header .primary {
  flex: 0 0 auto;
  white-space: nowrap;
  height: 32px;
  padding: 0 14px;
}

.page-header h1 {
  margin: 0 0 6px;
  font-size: 22px;
  font-weight: 650;
}

.subtitle {
  margin: 0;
  /* 640 会把这段说明挤成三行；页宽约 950，放开到整行宽度后稳定两行 */
  max-width: none;
  color: var(--text-secondary);
  font-size: 13px;
  line-height: 1.6;
}

.layout {
  display: grid;
  /* 2026-11-14 用户反馈：卡片太挤，一屏放不下几条信息、缩略图也小。列表列加宽，
     缩略图改成竖版（遥控器是竖长的），名字/按键数/绑定设备分三行显示。 */
  grid-template-columns: minmax(0, 1fr);
  gap: 18px;
  align-items: start;
}

.list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

/**
 * 列表条目：一行三列（缩略图 / 名称+按键数 / 操作），全部 flex + align-items:center。
 *
 * 2026-11-12 用户现场："左侧图标、右侧文字没对齐"——旧版用 grid 双行区域
 * （`"thumb meta" "thumb actions"`），缩略图固定 44px 却跨两行，行高由右侧
 * 文字+按钮撑到 66px 以上，图标就顶在上沿、文字沉在下面，看起来永远错位。
 * 改成单行 flex 后，三列共用一条中线对齐，高度只由最高的那一格决定。
 */
.list-item {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 12px 14px;
  border: 1px solid var(--border);
  border-radius: 12px;
  background: var(--card);
  cursor: pointer;
  transition: border-color 0.15s ease, background 0.15s ease;
}

/* 上半段：缩略图 + 文字。整行宽度都给文字，名字不再被按钮挤成省略号。 */
.item-main {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  min-width: 0;
}

.list-item:hover {
  border-color: var(--border-strong);
}

.list-item.selected {
  border-color: var(--accent-border);
  background: var(--accent-surface);
}

.list-item.active {
  box-shadow: inset 0 0 0 1px var(--accent-border);
}

.thumb {
  flex: 0 0 auto;
  /* 2026-11-14 第三轮：实测用户的照片是 173×700（长宽比 1:4），之前 64×99 的框
     contain 出来只有 24px 宽的一条细线，看起来就像"没显示完"。
     框给到 72×146，长条遥控器按 contain 会是 36×146 —— 整支都看得到，
     卡片高度也随之够排三行文字。 */
  width: 72px;
  height: 146px;
  border-radius: 10px;
  overflow: hidden;
  border: 1px solid var(--border);
  background: var(--surface-subtle);
  display: grid;
  place-items: center;
}

/* contain 而不是 cover：遥控器照片是竖长的，cover 在 40px 方块里只剩中间一截。 */
.thumb img {
  width: 100%;
  height: 100%;
  object-fit: contain;
}

.thumb-empty {
  font-size: 9.5px;
  letter-spacing: 0.2px;
  color: var(--text-secondary);
}

.list-meta {
  flex: 1 1 auto;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
  align-self: center;
}

.list-meta strong {
  font-size: 13.5px;
  line-height: 1.4;
  /* 名字允许两行：320px 的卡片里"小米蓝牙遥控器 2 Pro"一行放不下，
     截成"小米蓝牙遥控器…"反而认不出是哪一支。 */
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  overflow-wrap: anywhere;
}

.list-meta span {
  font-size: 11.5px;
  line-height: 1.45;
  color: var(--text-secondary);
}

.list-bound {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 「设为当前」与「当前使用中」用同一套形态（同高、同内距、同圆角、同字号），
   只是配色区分可用/已生效——否则一个描边按钮 + 一个胶囊标签并排会显得很别扭。 */
.switch-button {
  flex: 0 0 auto;
  height: 28px;
  padding: 0 12px;
  border: 1px solid var(--accent-border);
  border-radius: 8px;
  background: var(--accent-surface);
  color: var(--accent-text);
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  transition: background 0.15s ease, border-color 0.15s ease;
}

.switch-button:hover:not(:disabled) {
  background: var(--accent-surface-strong);
}

/* 当前生效那只：实心强调色，形态与「设为当前」完全一致。 */
.switch-button.is-current {
  border-color: transparent;
  background: var(--accent);
  color: var(--text-on-accent);
  cursor: default;
  opacity: 1;
}

.switch-button:disabled {
  cursor: default;
  opacity: 0.6;
}

.switch-button.is-current:disabled {
  opacity: 1;
}

.action-left {
  display: flex;
  align-items: center;
  gap: 8px;
}

/* 「复制内置配置」：次要动作，描边比主按钮弱一档。 */
.ghost-copy {
  flex: 0 0 auto;
  height: 28px;
  padding: 0 10px;
  border: 1px solid var(--border-strong);
  border-radius: 8px;
  background: var(--surface-control);
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition: border-color 0.15s ease, color 0.15s ease;
}

.ghost-copy:hover {
  border-color: var(--accent-border);
  color: var(--text-primary);
}

/* 下半段操作栏：独立一行，左右分开，按钮形态统一。 */
.list-actions {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding-top: 9px;
  border-top: 1px dashed var(--border);
}

/* 操作按钮收成图标：260px 的列表列里放不下两个中文按钮，图标能保证整行不换行。 */
.icon-button {
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  padding: 0;
  border: 1px solid transparent;
  border-radius: 7px;
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  transition: background 0.15s ease, border-color 0.15s ease, color 0.15s ease;
}

.icon-button svg {
  width: 14px;
  height: 14px;
}

.icon-button:hover {
  background: var(--surface-subtle);
  border-color: var(--border);
  color: var(--text-primary);
}

.icon-button.danger:hover {
  color: #b4322d;
}

.badge {
  display: inline-flex;
  align-items: center;
  height: 20px;
  padding: 0 7px;
  border-radius: 999px;
  background: var(--accent-surface-strong);
  color: var(--accent-text);
  font-size: 10.5px;
  font-weight: 600;
}

.editor {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 16px;
  border: 1px solid var(--border);
  border-radius: 14px;
  background: var(--card);
}

.empty-editor {
  align-items: center;
  justify-content: center;
  min-height: 200px;
  color: var(--text-secondary);
}

.row {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-end;
  gap: 12px;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 4px;
  flex: 1 1 200px;
  min-width: 180px;
  font-size: 12px;
  color: var(--text-secondary);
}

/* 语音键的只读展示（2026-11-16）：让它看起来"不可改"，而不是"改了没反应"。 */
.voice-locked {
  display: inline-flex;
  align-items: center;
  height: 32px;
  padding: 0 10px;
  border-radius: 8px;
  background: var(--surface-muted, rgba(0, 0, 0, 0.04));
  color: var(--text-muted, rgba(0, 0, 0, 0.45));
  font-size: 13px;
  white-space: nowrap;
  cursor: not-allowed;
}

.slot-hint {
  margin: 0 0 8px;
  font-size: 12px;
  line-height: 1.6;
  color: var(--text-muted, rgba(0, 0, 0, 0.45));
}

/* 绑定那一格比名称宽一档：它要放下「名称 · #令牌」，太窄会截成省略号。 */
.field-bind {
  flex: 1.3 1 250px;
}

.field input,
.button-table input,
.field select,
.button-table select {
  height: 32px;
  padding: 0 10px;
  border: 1px solid var(--border-strong);
  border-radius: 8px;
  background: var(--surface-control);
  color: var(--text-primary);
  font-size: 13px;
}

/* 下拉框（2026-11-14 用户现场："这个下拉框太丑了"）：
   之前 <select> 没有任何样式，Windows WebView 会画成带立体边框的系统控件，
   和旁边的圆角输入框完全两个体系。这里统一成输入框外观，再自己画一个
   跟随主题的箭头——`appearance: none` 去掉系统箭头后必须补一个。 */
.field select,
.button-table select {
  appearance: none;
  -webkit-appearance: none;
  width: 100%;
  padding-right: 30px;
  cursor: pointer;
  text-overflow: ellipsis;
  background-image: url("data:image/svg+xml;charset=utf-8,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 12 12'%3E%3Cpath d='M2.5 4.5 6 8l3.5-3.5' fill='none' stroke='%236b7280' stroke-width='1.6' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E");
  background-repeat: no-repeat;
  background-position: right 9px center;
  background-size: 12px 12px;
  transition: border-color 0.15s ease, background-color 0.15s ease;
}

html[data-theme="dark"] .field select,
html[data-theme="dark"] .button-table select {
  background-image: url("data:image/svg+xml;charset=utf-8,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 12 12'%3E%3Cpath d='M2.5 4.5 6 8l3.5-3.5' fill='none' stroke='%23a9aebb' stroke-width='1.6' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E");
}

.field select:hover,
.button-table select:hover {
  border-color: var(--accent-border);
}

.field select:focus-visible,
.button-table select:focus-visible {
  outline: 2px solid var(--accent-border);
  outline-offset: 1px;
}

/* 让 `<option>` 下拉列表跟主题走（Chromium 会沿用 select 的配色）。 */
.field select option,
.button-table select option {
  background: var(--surface-control);
  color: var(--text-primary);
}

.photo-area {
  border: 1px dashed var(--border-strong);
  border-radius: 12px;
  background: var(--surface-subtle);
  padding: 12px;
  display: grid;
  place-items: center;
  min-height: 220px;
}

.photo-wrap {
  position: relative;
  max-width: 420px;
  max-height: 420px;
}

.photo-wrap.marking {
  cursor: crosshair;
  outline: 2px solid var(--accent-border);
  outline-offset: 4px;
  border-radius: 8px;
}

.photo-wrap img {
  display: block;
  max-width: 100%;
  max-height: 420px;
  border-radius: 8px;
}

.dot {
  position: absolute;
  width: 12px;
  height: 12px;
  margin: -6px 0 0 -6px;
  border-radius: 50%;
  background: var(--accent);
  border: 2px solid #fff;
  box-shadow: 0 0 0 1px var(--accent-border);
}

.dot.armed {
  background: #e8730c;
}

.photo-placeholder {
  margin: 0;
  color: var(--text-secondary);
  font-size: 13px;
}

.buttons-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.buttons-head h2 {
  margin: 0;
  font-size: 15px;
}

.table-wrap {
  overflow-x: auto;
}

/* 8 列在窄窗口里会被挤成两行文字：给一个最小宽度，不够就横向滚动，不折行。 */
.button-table {
  width: 100%;
  min-width: 680px;
  border-collapse: separate;
  border-spacing: 0;
  font-size: 12px;
}

.button-table th {
  text-align: left;
  font-weight: 600;
  font-size: 11.5px;
  color: var(--text-secondary);
  padding: 8px 6px;
  border-bottom: 1px solid var(--border);
  white-space: nowrap;
}

.button-table td {
  padding: 6px;
  border-bottom: 1px solid var(--border);
  vertical-align: middle;
}

.button-table tbody tr:last-child td {
  border-bottom: 0;
}

.button-table tbody tr:hover td {
  background: var(--surface-subtle);
}

.button-table input {
  width: 100%;
}

.primary {
  height: 34px;
  padding: 0 14px;
  border: 0;
  border-radius: 9px;
  background: var(--accent);
  color: var(--text-on-accent);
  font-size: 13px;
  cursor: pointer;
}

.primary.small {
  height: 28px;
  padding: 0 10px;
  font-size: 12px;
}

.ghost {
  height: 28px;
  padding: 0 10px;
  border: 1px solid var(--border-strong);
  border-radius: 8px;
  background: var(--surface-control);
  color: var(--text-control);
  font-size: 12px;
  cursor: pointer;
}

.ghost.recording {
  border-color: var(--accent-border);
  background: var(--accent-surface);
  color: var(--accent-text);
}

.ghost.danger {
  color: #b4322d;
}

.empty {
  margin: 0;
  color: var(--text-secondary);
  font-size: 12px;
}

.hint {
  margin: 0;
  color: var(--text-secondary);
  font-size: 11px;
  line-height: 1.6;
}

.status {
  position: sticky;
  bottom: 0;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  border-radius: 10px;
  background: var(--accent-surface);
  color: var(--accent-text);
  font-size: 12px;
}

.status > span {
  flex: 1 1 auto;
  line-height: 1.5;
}

.status .ghost {
  flex: 0 0 auto;
}
/* ── 自定义遥控器卡片（2026-11-14 第四轮设计，按 1:4 实拍照片定稿） ──────────
   结构：产品图面板（左） + 信息（右） + 底部操作栏。
   订正过的三个坑：① 方框裁掉长条照片；② 文字被按钮挤成省略号；③ 按钮与标签形态不一。 */
.layout {
  grid-template-columns: minmax(0, 1fr);
}

.list-item {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 14px;
  border: 1px solid var(--border);
  border-radius: 14px;
  background: var(--card);
  cursor: pointer;
  transition: border-color 0.15s ease, background 0.15s ease, box-shadow 0.15s ease;
}

.list-item.active {
  border-color: var(--accent-border);
  box-shadow: inset 0 0 0 1px var(--accent-border);
}

.item-main {
  display: flex;
  gap: 14px;
  min-width: 0;
}

/* 产品图面板：照片按真实比例完整显示（典型 1:4），面板只负责给一圈浅底与描边。 */
.thumb {
  flex: 0 0 auto;
  width: 108px;
  height: 170px;
  padding: 10px 6px;
  border: 1px solid var(--border);
  border-radius: 12px;
  background: linear-gradient(180deg, #fbfbfd, #f3f4f8);
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
}

.thumb img {
  max-width: 100%;
  max-height: 100%;
  display: block;
  filter: drop-shadow(0 2px 4px rgba(33, 38, 56, 0.12));
}

.thumb-empty {
  font-size: 11px;
  color: var(--text-secondary);
}

.list-meta {
  flex: 1 1 auto;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding-top: 4px;
}

.list-meta strong {
  font-size: 14px;
  font-weight: 650;
  line-height: 1.45;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  overflow-wrap: anywhere;
}

/* 「当前使用中」是状态，不是操作：做成 chip，避免和「设为当前」按钮抢视线。 */
.state-chip {
  display: inline-flex;
  align-items: center;
  height: 22px;
  padding: 0 8px;
  border-radius: 6px;
  background: var(--accent-surface);
  color: var(--accent-text);
  font-size: 11px;
  font-weight: 600;
  width: max-content;
}

.list-count,
.list-bound {
  font-size: 12px;
  line-height: 1.55;
  color: var(--text-secondary);
}

.list-bound {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.list-actions {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding-top: 10px;
  border-top: 1px dashed var(--border);
}

.action-left {
  display: flex;
  align-items: center;
  gap: 8px;
}

/* 删除：危险动作靠颜色识别，不用红底大块，避免一张卡上两个高饱和块打架。 */
.danger-text {
  flex: 0 0 auto;
  height: 28px;
  padding: 0 10px;
  border: 1px solid transparent;
  border-radius: 8px;
  background: var(--delete-surface);
  color: var(--delete-text);
  font-size: 12px;
  cursor: pointer;
  transition: background 0.15s ease, color 0.15s ease;
}

.danger-text:hover {
  background: rgba(255, 59, 48, 0.16);
  color: var(--delete-text-hover);
}

.bind-hint {
  margin: 8px 0 0;
  padding: 7px 10px;
  border-radius: 8px;
  background: var(--surface-subtle);
  color: var(--text-secondary);
  font-size: 12px;
  line-height: 1.55;
}

.subtitle-note {
  margin: 6px 0 0;
  font-size: 12px;
  line-height: 1.6;
  color: var(--text-secondary);
}


/* 编辑区（2026-11-17 Kimi 620px 版定稿）：
   meta 通栏 → 左表格 + 右照片条 → 页脚说明。 */
.editor-grid {
  flex: 1 1 auto;
  min-width: 0;
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  grid-template-rows: auto minmax(0, 1fr) auto;
  grid-template-areas:
    "meta   meta"
    "table  photo"
    "footer footer";
  gap: 12px;
  height: 100%;
  min-height: 0;
  box-sizing: border-box;
}
.editor-grid .card {
  background: var(--surface-muted, rgba(0, 0, 0, 0.025));
  border: 1px solid var(--border-subtle, rgba(0, 0, 0, 0.08));
  border-radius: 8px;
  box-sizing: border-box;
  min-width: 0;
}
.meta-card { grid-area: meta; padding: 8px 12px; display: flex; flex-direction: column; gap: 8px; }
.key-table { grid-area: table; min-height: 0; display: flex; flex-direction: column; padding: 8px 8px 0; }
.editor-grid .photo-card {
  grid-area: photo;
  min-height: 0;
  min-width: 220px;
  max-width: 360px;
  position: relative;
  padding: 6px;
}
.hint-footer {
  grid-area: footer;
  margin: 0;
  font-size: 12px;
  line-height: 18px;
  height: 18px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  color: var(--text-muted, rgba(0, 0, 0, 0.5));
}

.meta-title-row { gap: 10px; }
.meta-title-row .name-input { flex: 1 1 auto; min-width: 160px; width: auto; }
.meta-tags {
  flex: 0 1 auto;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--text-muted, rgba(0, 0, 0, 0.5));
  white-space: nowrap;
}
.editor-grid .btn.danger { color: #c0392b; border-color: rgba(192, 57, 43, 0.3); }
.editor-grid .btn.danger:hover { background: rgba(192, 57, 43, 0.08); }
.name-input { width: 100%; }
.meta-row { display: flex; gap: 8px; align-items: center; }
.bind-select { flex: 1 1 auto; min-width: 0; }
.refresh-btn { flex: 0 0 32px; width: 32px; padding: 0; }

.name-input,
.bind-select,
.editor-grid .btn {
  height: 32px;
  font-size: 13px;
  border: 1px solid var(--border-subtle, rgba(0, 0, 0, 0.14));
  border-radius: 6px;
  padding: 0 10px;
  box-sizing: border-box;
  color: var(--text-primary);
  background: var(--surface, #fff);
  font-family: inherit;
}
.name-input.idle {
  color: var(--text-muted, rgba(0, 0, 0, 0.5));
  border-color: transparent;
  background: transparent;
  cursor: pointer;
}
.editor-grid .btn { cursor: pointer; white-space: nowrap; background: transparent; }
.editor-grid .btn.primary {
  background: var(--primary, #4c6ef5);
  border-color: var(--primary, #4c6ef5);
  color: #fff;
  height: 28px;
  font-size: 12px;
}

.key-head-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 4px 6px;
}
.key-title { margin: 0; font-size: 15px; font-weight: 600; color: var(--text-primary); }

.key-table .table-wrap { flex: 1 1 auto; min-height: 0; overflow: auto; }
.key-table .empty { margin: 8px 4px; font-size: 12px; color: var(--text-muted); }
.key-table table { width: 100%; min-width: 0; border-collapse: collapse; table-layout: fixed; }
.key-table th,
.key-table td {
  height: 36px;
  padding: 0 4px;
  font-size: 12px;
  text-align: left;
  border-bottom: 1px solid var(--border-subtle, rgba(0, 0, 0, 0.08));
}
.key-table th { font-size: 12px; font-weight: 500; color: var(--text-muted, rgba(0, 0, 0, 0.5)); }
.key-table th:nth-child(3),
.key-table td:nth-child(3) { text-align: right; }
.key-table th:nth-child(4),
.key-table td:nth-child(4),
.key-table th:nth-child(5),
.key-table td:nth-child(5) { text-align: center; }
.key-table td input,
.key-table td select { width: 100%; height: 30px; font-size: 12px; padding: 0 4px; min-width: 0; }
.key-table td .ghost,
.key-table td button {
  font-size: 12px;
  padding: 0 4px;
  max-width: 100%;
  height: 26px;
  border: 1px solid transparent;
  border-radius: 5px;
  background: transparent;
  cursor: pointer;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.key-table td .ghost:hover { background: var(--surface-muted, rgba(0, 0, 0, 0.05)); }
.key-table .voice-locked { font-size: 11px; padding: 0 2px; }

.upload-btn {
  position: absolute;
  top: 8px;
  right: 8px;
  z-index: 2;
  height: 24px;
  font-size: 12px;
  padding: 0 8px;
  background: var(--surface, #fff);
}
.photo-frame {
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 6px;
  background: var(--surface, #fff);
  overflow: hidden;
}
.photo-frame.empty { border: 1px dashed var(--border-subtle, rgba(0, 0, 0, 0.14)); }
.photo-frame .photo-wrap { position: relative; display: inline-block; line-height: 0; }
.photo-frame img {
  height: 100%;
  width: auto;
  max-width: 100%;
  object-fit: contain;
  display: block;
}
.photo-frame .photo-placeholder {
  margin: 0;
  padding: 8px;
  text-align: center;
  font-size: 12px;
  line-height: 1.7;
  color: var(--text-muted, rgba(0, 0, 0, 0.45));
}

@media (max-width: 1100px) {
  .editor-grid {
    grid-template-columns: 1fr;
    grid-template-rows: none;
    grid-template-areas: "meta" "table" "photo" "footer";
    height: auto;
  }
  .key-table { min-height: 320px; }
  .photo-card { height: 320px; }
}
</style>
