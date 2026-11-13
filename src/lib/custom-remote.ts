/**
 * 自定义遥控器：类型、持久化与照片读取。
 *
 * 与 Rust 侧的约定见 `src-tauri/src/custom_remote.rs`：
 * - 全部数据落 `<app_config_dir>/custom-remotes.json`；
 * - 照片以 data URL 内联，前端上传时先缩到 ≤1024px 的 JPEG，避免几 MB 原图塞进配置；
 * - 坐标用 0..1 相对值，界面按图片自身宽高换算，窗口缩放不偏移。
 */

import { invoke } from "@tauri-apps/api/core";

import { isTauriRuntime, type KeyCode } from "./bridge";

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

/** 新建一只空白遥控器（名称可后改，坐标默认落在照片正中）。 */
export function createCustomRemote(name = "我的遥控器"): CustomRemote {
  return { id: makeId("remote"), name, image: null, imageWidth: null, imageHeight: null, buttons: [] };
}

/** 新增一个按键条目：默认落在照片正中，用户随后点名并标点。 */
export function createCustomButton(name = "", slot: string | null = null): CustomRemoteButton {
  return {
    id: makeId("button"),
    name,
    slot,
    side: null,
    keys: [],
    x: 0.5,
    y: 0.5,
  };
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
