import { describe, expect, it } from "vitest";
import type { Device, PingRecord, Sample } from "./api";
import {
  backendLabel,
  clock,
  connTone,
  formatBytes,
  formatLatency,
  matchDevice,
  formatUptime,
  kindLabel,
  latencyStats,
  measureLine,
  timeAgo,
  timeTicks,
  usageTone,
} from "./format";

describe("formatBytes", () => {
  it("未知、整数和小数", () => {
    expect(formatBytes(null)).toBe("未知");
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1023)).toBe("1023 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(10 * 1024 * 1024)).toBe("10 MB");
  });

  it("超过 TB 时不越界", () => {
    expect(formatBytes(2048 * 1024 ** 4)).toBe("2048 TB");
  });
});

describe("formatUptime", () => {
  it("按天、小时、分钟取最大的两级", () => {
    expect(formatUptime(2_380_130)).toBe("27 天 13 小时");
    expect(formatUptime(86400 * 3)).toBe("3 天");
    expect(formatUptime(5 * 3600 + 3 * 60)).toBe("5 小时 3 分");
    expect(formatUptime(7200)).toBe("2 小时");
    expect(formatUptime(720)).toBe("12 分钟");
    expect(formatUptime(10)).toBe("1 分钟");
  });
});

describe("usageTone", () => {
  it("80% 提醒、90% 报警", () => {
    expect(usageTone(79.9)).toBe("accent");
    expect(usageTone(80)).toBe("punching");
    expect(usageTone(90)).toBe("error");
  });
});

describe("timeAgo", () => {
  const now = 1_000_000_000;
  it("按分钟、小时、天", () => {
    expect(timeAgo(null, now)).toBe("未知");
    expect(timeAgo(now - 30_000, now)).toBe("刚刚");
    expect(timeAgo(now - 5 * 60_000, now)).toBe("5 分钟前");
    expect(timeAgo(now - 3 * 3600_000, now)).toBe("3 小时前");
    expect(timeAgo(now - 2 * 86400_000, now)).toBe("2 天前");
  });

  it("时间在未来（本机时钟偏差）时显示刚刚", () => {
    expect(timeAgo(now + 60_000, now)).toBe("刚刚");
  });
});

describe("clock", () => {
  it("24 小时制时分秒", () => {
    expect(clock(Date.now())).toMatch(/^\d{1,2}:\d{2}:\d{2}$/);
  });
});

describe("backendLabel", () => {
  it("已知状态翻译成中文，未知状态保留原文", () => {
    expect(backendLabel("Running")).toBe("已连接");
    expect(backendLabel("Stopped")).toBe("已断开");
    expect(backendLabel("NeedsLogin")).toBe("需要登录");
    expect(backendLabel("Weird")).toBe("未知状态（Weird）");
    expect(backendLabel(null)).toBe("未知");
  });
});

describe("connTone", () => {
  it("直连、中继、其他", () => {
    expect(connTone("lan")).toBe("direct");
    expect(connTone("ipv6")).toBe("direct");
    expect(connTone("peer-relay")).toBe("relay");
    expect(connTone("punching")).toBe("punching");
    expect(connTone("self-node")).toBe("accent");
    expect(connTone("unknown")).toBe("idle");
  });
});

describe("formatLatency", () => {
  it("10 ms 以下保留一位小数，去掉多余的 0", () => {
    expect(formatLatency(0.43)).toBe("0.4 ms");
    expect(formatLatency(1)).toBe("1 ms");
    expect(formatLatency(12.6)).toBe("13 ms");
  });
});

describe("kindLabel", () => {
  it("直连、中继和未知", () => {
    expect(kindLabel("lan")).toBe("局域网直连");
    expect(kindLabel("peer-relay")).toBe("中继");
    expect(kindLabel(null)).toBe("");
  });
});

describe("measureLine", () => {
  const now = 1_000_000_000;
  const sample = (agoMs: number, ms: number | null, kind: Sample["kind"] = "lan"): Sample => ({
    atMs: now - agoMs,
    ms,
    kind: ms == null ? null : kind,
  });
  const manual = (agoMs: number): PingRecord => ({
    result: { conn: { kind: "relay", label: "中继（经 广州）", hint: null }, latencyMs: 35, error: null, raw: "pong" },
    error: null,
    atMs: now - agoMs,
  });

  it("都没有时显示延迟未测", () => {
    expect(measureLine(undefined, undefined, now).text).toBe("延迟未测");
    expect(measureLine(undefined, [], now).text).toBe("延迟未测");
  });

  it("没有手动测试时用延迟历史的最近一次", () => {
    const line = measureLine(undefined, [sample(600_000, 9), sample(180_000, 2.3)], now);
    expect(line).toEqual({ text: "延迟 2.3 ms · 局域网直连 · 3 分钟前测得", failed: false, raw: "" });
  });

  it("最近一次没有回应时标为失败", () => {
    const line = measureLine(undefined, [sample(120_000, null)], now);
    expect(line.text).toBe("2 分钟前测速没有回应");
    expect(line.failed).toBe(true);
  });

  it("手动测试与历史中同一次时用手动结果", () => {
    const line = measureLine(manual(60_000), [sample(61_000, 35, "relay")], now);
    expect(line.text).toMatch(/^延迟 35 ms · 中继（经 广州） · .* 实测$/);
    expect(line.raw).toBe("pong");
  });

  it("后台测速比手动测试新时用后台结果", () => {
    const line = measureLine(manual(600_000), [sample(30_000, 3)], now);
    expect(line.text).toBe("延迟 3 ms · 局域网直连 · 刚刚测得");
  });
});

describe("matchDevice", () => {
  const d = {
    name: "ming-pc",
    dnsName: "ming-pc.mesh.example",
    os: "windows",
    ips: ["100.64.0.3", "fd7a:115c:a1e0::3"],
  } as Device;
  it("空查询全部命中", () => expect(matchDevice(d, "  ")).toBe(true));
  it("按名称、IP、系统匹配，不分大小写", () => {
    expect(matchDevice(d, "MING")).toBe(true);
    expect(matchDevice(d, "100.64.0.3")).toBe(true);
    expect(matchDevice(d, "win")).toBe(true);
    expect(matchDevice(d, "linux")).toBe(false);
  });
  it("多个词需全部命中", () => {
    expect(matchDevice(d, "ming windows")).toBe(true);
    expect(matchDevice(d, "ming linux")).toBe(false);
  });
});

describe("latencyStats", () => {
  const s = (ms: number | null, kind: Sample["kind"] = "lan"): Sample => ({ atMs: 0, ms, kind: ms == null ? null : kind });
  it("没有测量时各项为空", () => {
    expect(latencyStats([])).toEqual({
      count: 0,
      timeouts: 0,
      min: null,
      median: null,
      p95: null,
      max: null,
      relayRatio: null,
    });
  });
  it("没有回应只计入次数，中继比例只看有回应的", () => {
    const list = [s(5), s(1), s(null), s(40, "relay"), ...Array.from({ length: 16 }, () => s(2))];
    const r = latencyStats(list);
    expect(r.count).toBe(20);
    expect(r.timeouts).toBe(1);
    expect([r.min, r.median, r.max]).toEqual([1, 2, 40]);
    // 19 个有回应的测量：95% 位置是第 19 个，即最大值
    expect(r.p95).toBe(40);
    expect(r.relayRatio).toBeCloseTo(1 / 19);
  });
});

describe("timeTicks", () => {
  const at = (d: number, h: number, m = 0) => new Date(2026, 9, d, h, m).getTime();

  it("24 小时：从起点之后的第一个 6 小时整点开始，不画到起点之前", () => {
    const ticks = timeTicks(at(19, 12, 47), at(20, 12, 47));
    expect(ticks.map((t) => t.label)).toEqual(["18:00", "00:00", "06:00", "12:00"]);
    expect(ticks[0].at).toBe(at(19, 18));
  });

  it("起点正好在整点时包含起点", () => {
    expect(timeTicks(at(19, 12), at(20, 12)).map((t) => t.label)).toEqual(["12:00", "18:00", "00:00", "06:00", "12:00"]);
  });

  it("7 天：每天零点一格", () => {
    const ticks = timeTicks(at(13, 12, 47), at(20, 12, 47));
    expect(ticks.map((t) => t.label)).toEqual(["10/14", "10/15", "10/16", "10/17", "10/18", "10/19", "10/20"]);
  });
});
