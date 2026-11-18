/**
 * 蓝牙连接失败 → 用户能照做的中文说明（2026-11-14 现场）。
 *
 * 背景：`connectRemote` 抛出的错误是后端 `PlatformError` 的 Display 原文，
 * 形如「Xiaomi voice remote GATT operation failed: 发现 ATVV 特征返回状态 3」。
 * 这句英文 + 数字对用户没有任何可执行含义，界面直接打印就等于把故障丢回给用户。
 *
 * `GattCommunicationStatus` 的三个失败值本来就是三种不同的故障层，处置方式也
 * 完全不同，所以这里按状态值分流，再给出对应动作：
 *
 *   1 = Unreachable     遥控器不在无线电上（休眠 / 走远了）→ 拿近、按一下任意键
 *   2 = ProtocolError   链路在但协议错 → 断开重连一次
 *   3 = AccessDenied    系统层面拒绝访问（配对记录坏了）→ 删掉重新配对
 *
 * 判据只认后端已经稳定输出的文本：`发现 … 返回状态 N`（见 `ble.rs::gatt_status_error`）
 * 以及错误码里的 `AccessDenied` / `Unreachable` / `ProtocolError`。
 */
export type ConnectionFailureKind =
  | "access_denied"
  | "unreachable"
  | "protocol"
  | "service_missing"
  | "characteristic_missing"
  | "other";

const PATTERNS: Array<[RegExp, ConnectionFailureKind]> = [
  // 状态 3：Windows 拒绝访问。实测（2026-11-14）同一支遥控器服务发现能过、
  // 特征查询每次都秒回 3，换进程重试 11 次全部一致 —— 不是瞬态，重试无用。
  [/返回状态\s*3\b/, "access_denied"],
  [/AccessDenied/i, "access_denied"],
  // 状态 1：设备不在无线电上，属于可自愈场景。
  [/返回状态\s*1\b/, "unreachable"],
  [/Unreachable/i, "unreachable"],
  [/返回状态\s*2\b/, "protocol"],
  [/ProtocolError/i, "protocol"],
  [/GATT characteristic .* is missing/i, "characteristic_missing"],
  [/GATT service is missing/i, "service_missing"],
];

function errorText(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  if (error && typeof error === "object" && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return String(error ?? "");
}

export function classifyConnectionFailure(error: unknown): ConnectionFailureKind {
  const text = errorText(error);
  for (const [pattern, kind] of PATTERNS) {
    if (pattern.test(text)) return kind;
  }
  return "other";
}

/** 每种故障一句「原因 + 下一步」。故意不出现 GATT / ATVV / 状态码这类内部词。 */
export function describeConnectionFailure(kind: ConnectionFailureKind): string {
  switch (kind) {
    case "access_denied":
      // 2026-11-14 现场追问："Windows 里明明显示已连接、快捷键也能按，凭什么说拒绝访问？"
      // 因为被拒的是**语音服务**这条 GATT 链路，按键走的是 HID 报告，两者互不影响。
      // 措辞必须说清这一点，否则用户会以为设备没连上。
      return "这台电脑打不开这支遥控器的语音服务（系统返回「拒绝访问」）。按键功能不受影响，但语音转写会用不了。最有效的修复是在「设置 → 蓝牙和其他设备」里把它「删除设备」再重新配对。";
    case "unreachable":
      return "暂时找不到这支遥控器，它可能睡着了或离得远。把它拿近、按一下任意键唤醒，再重试。";
    case "protocol":
      return "蓝牙连上了但通信出错。先在「连接」页把它断开，再连一次；仍然失败就重新配对。";
    case "service_missing":
    case "characteristic_missing":
      return "这支遥控器没有提供可用的语音服务，可能不是支持语音的型号，或者固件里没有这段服务。";
    default:
      return "连接没有成功。请确认遥控器就在电脑旁边、电池有电，然后重试。";
  }
}

/** 只有这一种失败需要用户去系统设置里动手。 */
export function needsRepairHint(kind: ConnectionFailureKind): boolean {
  return kind === "access_denied";
}
