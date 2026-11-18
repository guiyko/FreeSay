/**
 * 自定义遥控器：类型、持久化与照片读取。
 *
 * 与 Rust 侧的约定见 `src-tauri/src/custom_remote.rs`：
 * - 全部数据落 `<app_config_dir>/custom-remotes.json`；
 * - 照片以 data URL 内联，前端上传时先缩到 ≤1024px 的 JPEG，避免几 MB 原图塞进配置；
 * - 坐标用 0..1 相对值，界面按图片自身宽高换算，窗口缩放不偏移。
 */

import { invoke } from "@tauri-apps/api/core";

import {
  getActiveMappingProfile,
  getConnectionSnapshot,
  isTauriRuntime,
  setActiveMappingProfile,
  type ButtonMappings,
  type KeyCode,
} from "./bridge";

/**
 * 单个按键：名称、逻辑键位、照片相对坐标。
 *
 * 2026-11-12 修订：`slot` 是**逻辑键位**（`power` / `up` / `voice` …）。自定义遥控器
 * 的按键要能和内置遥控器一样设置单击/双击/长按，动作就必须落在既有的
 * `mappings.actions[slot]` 上——这也是「按键」页能直接编辑同一个格子的前提。
 * 三档动作本身不再存在这里（早期版本的 `single/double/longPress` 只剩兼容读取）。
 */
export interface CustomRemoteButton {
  id: string;
  name: string;
  /** 逻辑键位；空表示只展示、不参与动作映射。 */
  slot?: string | null;
  /**
   * 卡片在哪一列。2026-11-13 起真正启用：拖动卡片可以跨列，拖到哪一侧就记在这里；
   * 缺省 = 按"先排满左列再排右列"的自动规则。
   *
   * **列内顺序不用额外字段**：就是 `buttons` 数组里的先后。拖动是"移动到某个行槽位"，
   * 松手后重排数组即可 —— 这样间距永远由布局均分，不会出现卡片叠在一起。
   */
  side?: "left" | "right" | null;
  /** 绑定的按键序列（仅用于界面回显，动作归属由 `slot` 决定）。 */
  keys?: KeyCode[];
  x: number;
  y: number;
}

/** 一只自定义遥控器。 */
export interface CustomRemote {
  id: string;
  name: string;
  /** 照片 data URL；null 表示还没上传。 */
  image?: string | null;
  /** 照片原始像素尺寸，用来算画布上的等比容器（缺省按 250×508 处理）。 */
  imageWidth?: number | null;
  imageHeight?: number | null;
  /**
   * 绑定的物理遥控器（设备令牌，与 `PairedRemote.token` 同源）。
   *
   * 2026-11-14：自定义遥控器原本只是"外观 + 按键布局"，与「连接」页选中的设备
   * 毫无关系，两支同型号遥控器会互相串。绑上令牌后，「设为当前」才能做到
   * "连它 + 用它这套布局"，并且换设备时不会串到另一支。
   */
  boundDeviceToken?: string | null;
  buttons: CustomRemoteButton[];
}

export interface CustomRemoteStoreSnapshot {
  remotes: CustomRemote[];
  activeId: string | null;
}

function makeId(prefix: string): string {
  const random = Math.random().toString(36).slice(2, 8);
  return `${prefix}-${Date.now().toString(36)}-${random}`;
}

/**
 * 新建遥控器时预置的键位模板：与内置 RC003 的 12 个键一致。
 *
 * 2026-11-15 用户现场：新建的自定义遥控器是**空按钮列表**，用户逐个人工添加并选择
 * 槽位；漏掉哪一个，那个键就永远不响应——表现为"电源键识别不到，其他键正常"。
 * 更糟的是槽位留空（「未指定」）的按钮只做展示，不会有任何动作，且很难自查。
 *
 * 所以新建即预置一整套：槽位已填好，坐标取内置遥控器照片上的相对位置，
 * 用户只需上传自己的照片（照片比例不同时可再拖动微调），不会缺键。
 */
const DEFAULT_BUTTONS: Array<{
  slot: string;
  name: string;
  side: "left" | "right";
  x: number;
  y: number;
}> = [
  { slot: "power", name: "电源", side: "left", x: 0.386, y: 0.099 },
  { slot: "up", name: "方向上", side: "left", x: 0.502, y: 0.179 },
  { slot: "left", name: "方向左", side: "left", x: 0.362, y: 0.246 },
  { slot: "back", name: "返回", side: "left", x: 0.406, y: 0.389 },
  { slot: "home", name: "主页", side: "left", x: 0.406, y: 0.479 },
  { slot: "menu", name: "菜单", side: "left", x: 0.406, y: 0.569 },
  { slot: "right", name: "方向右", side: "right", x: 0.638, y: 0.246 },
  { slot: "ok", name: "确定", side: "right", x: 0.502, y: 0.246 },
  { slot: "down", name: "方向下", side: "right", x: 0.502, y: 0.317 },
  { slot: "volume_up", name: "音量+", side: "right", x: 0.604, y: 0.39 },
  { slot: "volume_down", name: "音量−", side: "right", x: 0.604, y: 0.48 },
  { slot: "tv", name: "电视", side: "right", x: 0.604, y: 0.569 },
];

/** 新建一只遥控器：预置 12 个键位（含电源），名称可后改。 */
export function createCustomRemote(name = "我的遥控器"): CustomRemote {
  return {
    id: makeId("remote"),
    name,
    image: null,
    imageWidth: null,
    imageHeight: null,
    buttons: DEFAULT_BUTTONS.map((template) =>
      createCustomButton(template.name, template.slot, template.side, template.x, template.y),
    ),
  };
}

/** 新增一个按键条目：默认落在照片正中，用户随后点名并标点。 */
export function createCustomButton(
  name = "",
  slot: string | null = null,
  side: "left" | "right" | null = null,
  x = 0.5,
  y = 0.5,
): CustomRemoteButton {
  return {
    id: makeId("button"),
    name,
    slot,
    side,
    keys: [],
    x,
    y,
  };
}

/**
 * 让「布局 + 按键档案」与**当前实际连接的设备**保持一致（2026-11-15）。
 *
 * 为什么必须有这个函数：切换设备有两个入口——自定义遥控器页的「设为当前」，
 * 以及**连接页直接点另一支设备**。后者原来只改 BLE 连接与 `settings.selected_remote_id`，
 * 完全没动 `custom-remotes.active_id` 和 `mapping-profiles/active.txt`。后果是：
 * 用户从自定义遥控器切回小米遥控器后，物理连着 A，按键页却仍渲染自定义照片与布局，
 * 映射引擎也仍跑那套档案 —— 内置遥控器的"直接可用"（S1）被破坏。
 *
 * 现在把判据收敛为一条：**由当前连接设备反查应使用哪套布局与档案**。
 *   - 找到绑定该设备的自定义遥控器 → 用它（active_id = 该 id，档案 = 该 id）；
 *   - 没有 → 回到内置（active_id = null，档案 = builtin）。
 *
 * 返回 `mappings` 只在档案真的换了时才非空，调用方据此刷新界面状态。
 */
export async function syncLayoutWithConnectedDevice(): Promise<{
  activeId: string | null;
  mappings: ButtonMappings | null;
}> {
  if (!isTauriRuntime()) {
    return { activeId: null, mappings: null };
  }
  const [snapshot, connection] = await Promise.all([
    loadCustomRemotes(),
    getConnectionSnapshot().catch(() => null),
  ]);
  const token = connection?.remoteToken?.trim() ?? "";
  // 2026-11-17 现场：刚「设为当前」后跳到本页，连接快照的 remoteToken 可能还没就绪。
  // 原逻辑在 token 为空时把 activeId 判成 null（=内置），于是刚设好的自定义布局被
  // 立刻重置，用户看到的就是"照片还是没跟过来"。拿不到 token 时不做判定。
  if (!token) {
    return { activeId: snapshot.activeId, mappings: null };
  }
  const matched = snapshot.remotes.find((remote) => remote.boundDeviceToken === token) ?? null;
  const nextActiveId = matched?.id ?? null;

  if (snapshot.activeId !== nextActiveId) {
    await persistCustomRemotes(snapshot.remotes, nextActiveId);
  }

  const profileId = nextActiveId ?? "builtin";
  const currentProfile = await getActiveMappingProfile();
  if (currentProfile === profileId) {
    return { activeId: nextActiveId, mappings: null };
  }
  const mappings = await setActiveMappingProfile(profileId);
  return { activeId: nextActiveId, mappings };
}

/** 读取全部自定义遥控器与当前生效项。非 Tauri 环境返回空集。 */
export async function loadCustomRemotes(): Promise<CustomRemoteStoreSnapshot> {
  if (!isTauriRuntime()) {
    return { remotes: [], activeId: null };
  }
  const [remotes, activeId] = await invoke<[CustomRemote[], string | null]>("get_custom_remotes");
  return { remotes: remotes ?? [], activeId: activeId ?? null };
}

/** 整体保存（编辑器每次提交后调用）。 */
export async function persistCustomRemotes(
  remotes: CustomRemote[],
  activeId: string | null,
): Promise<void> {
  if (!isTauriRuntime()) {
    return;
  }
  await invoke("save_custom_remotes", { remotes, activeId });
}

/**
 * 把用户选的照片读成 data URL。
 *
 * 走 `createImageBitmap` + canvas 而不是 `FileReader`：前者能按 EXIF 方向摆正并
 * 按最长边等比缩放到 `maxEdge`，常见 4 MB 手机照能压到 150 KB 上下，远小于直接
 * 内联原图的体积。
 */
export interface RemotePhoto {
  dataUrl: string;
  width: number;
  height: number;
}

/**
 * 把用户选的照片读成 data URL，并带回缩放后的像素尺寸。
 *
 * 尺寸必须一起返回：画布上照片容器要按真实宽高比等比收缩，否则锚点的归一化坐标
 * 会落在错位的矩形里，箭头指向就会偏（2026-11-12 现场：上传后连线乱指）。
 */
export async function readRemotePhoto(file: File, maxEdge = 1024): Promise<RemotePhoto> {
  const bitmap = await createImageBitmap(file);
  const scale = Math.min(1, maxEdge / Math.max(bitmap.width, bitmap.height));
  const width = Math.max(1, Math.round(bitmap.width * scale));
  const height = Math.max(1, Math.round(bitmap.height * scale));
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d");
  if (!context) {
    throw new Error("无法创建画布，照片读取失败");
  }
  context.drawImage(bitmap, 0, 0, width, height);
  bitmap.close?.();
  return { dataUrl: canvas.toDataURL("image/jpeg", 0.85), width, height };
}

/** 按键序列的可读文本：`left_control + m` 这类形态，与快捷键录入结果同源。 */
export function formatKeys(keys: KeyCode[] | undefined | null): string {
  if (!keys || keys.length === 0) {
    return "";
  }
  return keys
    .map((key) => KEY_LABELS[key] ?? key.replace(/_/g, " "))
    .join(" + ");
}

/** 录入结果里可能出现的键名。未收录的一律原样显示（下划线换空格）。 */
const KEY_LABELS: Record<string, string> = {
  left_control: "左 Ctrl",
  right_control: "右 Ctrl",
  left_shift: "左 Shift",
  right_shift: "右 Shift",
  left_alt: "左 Alt",
  right_alt: "右 Alt",
  left_windows: "左 Win",
  right_windows: "右 Win",
  space: "空格",
  enter: "回车",
  escape: "Esc",
  tab: "Tab",
  backspace: "退格",
  // 遥控器专有键（2026-11-13）：返回 / 音量± / 主页 / 菜单等**在 Windows 侧
  // 没有键盘事件**，只能从报告层边沿拿到，`[EDGE]` 里记的就是这些 usage 名。
  // 核对时必须能显示成人话，否则用户看到 "volume up" 会以为是乱码。
  back: "返回",
  ok: "确定",
  tv: "电视",
  home: "主页",
  menu: "菜单",
  power: "电源",
  up: "方向↑",
  down: "方向↓",
  left: "方向←",
  right: "方向→",
  volume_mute: "静音",
  volume_up: "音量+",
  volume_down: "音量−",
};
