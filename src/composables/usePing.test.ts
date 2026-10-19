import { beforeEach, describe, expect, it, vi } from "vitest";
import { ref } from "vue";
import type { ConnKind, Device, PingResult } from "../api";
import { deviceIp, usePing } from "./usePing";

// 替换真正调用后端的 ping（vi.mock 会被提升到导入之前执行）
const ping = vi.hoisted(() => vi.fn<(ip: string) => Promise<PingResult>>());
vi.mock("../api", () => ({ ping }));

function device(id: string, ips: string[], kind: ConnKind = "lan"): Device {
  return {
    id,
    name: id,
    dnsName: null,
    ips,
    os: null,
    online: kind !== "offline",
    conn: { kind, label: "", hint: null },
    rxBytes: null,
    txBytes: null,
    lastSeenMs: null,
    lastHandshakeMs: null,
    sshAlias: null,
    exitOption: false,
    exitActive: false,
  };
}

const ok: PingResult = { conn: null, latencyMs: 1.2, error: null, raw: "" };

describe("deviceIp", () => {
  it("优先 IPv4，没有时退回第一个地址", () => {
    expect(deviceIp(device("a", ["fd7a::3", "100.64.0.3"]))).toBe("100.64.0.3");
    expect(deviceIp(device("a", ["fd7a::3"]))).toBe("fd7a::3");
    expect(deviceIp(device("a", []))).toBeUndefined();
  });
});

describe("usePing", () => {
  beforeEach(() => ping.mockReset());

  it("记录成功和失败的结果，测试中标记正在测试", async () => {
    const a = device("a", ["100.64.0.1"]);
    const b = device("b", ["100.64.0.2"]);
    const onDone = vi.fn();
    const p = usePing(ref([a, b]), onDone);
    ping.mockResolvedValueOnce(ok);
    ping.mockRejectedValueOnce({ kind: "timeout", message: "超时", raw: "" });

    const running = p.testOne(a);
    expect(p.testing.value.has("a")).toBe(true);
    await running;
    await p.testOne(b);

    expect(p.pings.value.a.result).toEqual(ok);
    expect(p.pings.value.b.error?.kind).toBe("timeout");
    expect(p.testing.value.size).toBe(0);
    expect(onDone).toHaveBeenCalledTimes(2);
  });

  it("同一台设备测试中时不重复发起", async () => {
    const a = device("a", ["100.64.0.1"]);
    const p = usePing(ref([a]), () => {});
    ping.mockResolvedValue(ok);
    await Promise.all([p.testOne(a), p.testOne(a)]);
    expect(ping).toHaveBeenCalledTimes(1);
  });

  it("全部测试跳过离线设备", async () => {
    const peers = ref([device("a", ["100.64.0.1"]), device("b", ["100.64.0.2"], "offline")]);
    const p = usePing(peers, () => {});
    ping.mockResolvedValue(ok);
    await p.testAll();
    expect(ping).toHaveBeenCalledWith("100.64.0.1");
    expect(ping).toHaveBeenCalledTimes(1);
    expect(p.testablePeers.value.map((d) => d.id)).toEqual(["a"]);
  });
});
