import { describe, expect, it } from "vitest";

import {
  classifyConnectionFailure,
  describeConnectionFailure,
  needsRepairHint,
} from "./connect-error";

describe("连接失败分类", () => {
  it("把后端的原始错误文本分到对应的故障层", () => {
    // 2026-11-14 现场原文：用户看到的就是这一句。
    expect(
      classifyConnectionFailure(
        new Error("Xiaomi voice remote GATT operation failed: 发现 ATVV 特征返回状态 3"),
      ),
    ).toBe("access_denied");
    expect(
      classifyConnectionFailure(
        "Xiaomi voice remote GATT operation failed: 发现 ATVV 服务返回状态 1",
      ),
    ).toBe("unreachable");
    expect(classifyConnectionFailure("发现 ATVV 服务返回状态 2")).toBe("protocol");
  });

  it("也认 WinRT 枚举名（英文界面与日志粘贴都可能用它）", () => {
    expect(classifyConnectionFailure("GattCommunicationStatus.AccessDenied")).toBe(
      "access_denied",
    );
    expect(classifyConnectionFailure("GattCommunicationStatus.Unreachable")).toBe(
      "unreachable",
    );
    expect(classifyConnectionFailure("GattCommunicationStatus.ProtocolError")).toBe(
      "protocol",
    );
  });

  it("服务/特征缺失单独分流，不误判成权限问题", () => {
    expect(
      classifyConnectionFailure(
        new Error("Xiaomi voice remote GATT service is missing"),
      ),
    ).toBe("service_missing");
    expect(
      classifyConnectionFailure(
        new Error('Xiaomi voice remote GATT characteristic "transmit" is missing'),
      ),
    ).toBe("characteristic_missing");
  });

  it("认不出来的错误退到 other，不猜故障层", () => {
    expect(classifyConnectionFailure(new Error("something else"))).toBe("other");
    expect(classifyConnectionFailure(undefined)).toBe("other");
    expect(classifyConnectionFailure(null)).toBe("other");
  });

  it("返回状态 30 不应被当成状态 3", () => {
    expect(classifyConnectionFailure("发现 ATVV 服务返回状态 30")).toBe("other");
  });
});

describe("连接失败文案", () => {
  it("每种失败都有一句可照做的中文，且不暴露内部术语", () => {
    const kinds = [
      "access_denied",
      "unreachable",
      "protocol",
      "service_missing",
      "characteristic_missing",
      "other",
    ] as const;
    for (const kind of kinds) {
      const text = describeConnectionFailure(kind);
      expect(text.length).toBeGreaterThan(8);
      expect(text).not.toMatch(/GATT|ATVV|状态码|AccessDenied|Unreachable/);
    }
  });

  it("只有权限类失败提示去系统设置重新配对", () => {
    expect(needsRepairHint("access_denied")).toBe(true);
    expect(needsRepairHint("unreachable")).toBe(false);
    expect(needsRepairHint("protocol")).toBe(false);
    expect(needsRepairHint("other")).toBe(false);
  });
});
