import { describe, expect, it } from "vitest";
import { backendLabel, clock, connTone, formatBytes, formatLatency, timeAgo } from "./format";

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
