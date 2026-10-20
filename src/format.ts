// 展示用的格式化工具
import type { ConnKind, Device, PingRecord, Sample } from "./api";

/** 字节数 → 1.2 MB */
export function formatBytes(n: number | null): string {
  if (n == null) return "未知";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = n;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v < 10 && i > 0 ? v.toFixed(1) : Math.round(v)} ${units[i]}`;
}

/** 运行秒数 → “27 天 9 小时” / “5 小时 3 分” / “12 分钟” */
export function formatUptime(secs: number): string {
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  if (d > 0) return h > 0 ? `${d} 天 ${h} 小时` : `${d} 天`;
  if (h > 0) return m > 0 ? `${h} 小时 ${m} 分` : `${h} 小时`;
  return `${Math.max(m, 1)} 分钟`;
}

/** 用量百分比 → 状态色名称：80% 起提醒，90% 起报警 */
export function usageTone(pct: number): string {
  if (pct >= 90) return "error";
  if (pct >= 80) return "punching";
  return "accent";
}

/** 时间戳 → “3 分钟前” */
export function timeAgo(ms: number | null, now: number): string {
  if (ms == null) return "未知";
  const s = Math.max(0, Math.round((now - ms) / 1000));
  if (s < 60) return "刚刚";
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} 分钟前`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h} 小时前`;
  return `${Math.floor(h / 24)} 天前`;
}

/** 时间戳 → 14:05:09 */
export function clock(ms: number): string {
  return new Date(ms).toLocaleTimeString("zh-CN", { hour12: false });
}

/** 后端状态 → 中文 */
export function backendLabel(state: string | null): string {
  switch (state) {
    case "Running":
      return "已连接";
    case "Stopped":
      return "已断开";
    case "Starting":
      return "正在连接";
    case "NeedsLogin":
      return "需要登录";
    case "NeedsMachineAuth":
      return "等待控制端批准";
    default:
      return state ? `未知状态（${state}）` : "未知";
  }
}

/** 连接方式 → 状态色名称（对应 style.css 中的颜色变量） */
export function connTone(kind: ConnKind): string {
  switch (kind) {
    case "lan":
    case "ipv6":
    case "public":
      return "direct";
    case "relay":
    case "peer-relay":
      return "relay";
    case "punching":
      return "punching";
    case "offline":
      return "offline";
    case "self-node":
      return "accent";
    default:
      return "idle";
  }
}

/** 延迟毫秒 → “1 ms” / “0.4 ms” */
export function formatLatency(ms: number): string {
  return ms < 10 ? `${Number(ms.toFixed(1))} ms` : `${Math.round(ms)} ms`;
}

/** 实测路径 → 简短中文（延迟历史只记了路径类型，没有完整说明） */
export function kindLabel(kind: ConnKind | null): string {
  switch (kind) {
    case "lan":
      return "局域网直连";
    case "ipv6":
      return "IPv6 直连";
    case "public":
      return "公网直连";
    case "relay":
    case "peer-relay":
      return "中继";
    default:
      return "";
  }
}

/** 设备卡片的测速行 */
export interface MeasureLine {
  text: string;
  /** 失败时用错误色显示 */
  failed: boolean;
  /** 悬停显示的原始输出 */
  raw: string;
}

/**
 * 测速行：取手动测试结果和延迟历史中较新的一次。
 * 手动测试也会记入历史，时间相差几秒内视为同一次，优先用手动结果（信息更全，有失败原因）
 */
export function measureLine(ping: PingRecord | undefined, history: Sample[] | undefined, now: number): MeasureLine {
  const last = history && history.length ? history[history.length - 1] : undefined;
  if (ping && (!last || ping.atMs >= last.atMs - 5_000)) {
    const at = clock(ping.atMs);
    if (ping.error) return { text: `测试出错：${ping.error.message}（${at}）`, failed: true, raw: ping.error.raw };
    const r = ping.result!;
    if (!r.conn || r.latencyMs == null) return { text: `${r.error ?? "测试失败"}（${at}）`, failed: true, raw: r.raw };
    return { text: `延迟 ${formatLatency(r.latencyMs)} · ${r.conn.label} · ${at} 实测`, failed: false, raw: r.raw };
  }
  if (last) {
    const when = timeAgo(last.atMs, now);
    if (last.ms == null) return { text: `${when}测速没有回应`, failed: true, raw: "" };
    const parts = [`延迟 ${formatLatency(last.ms)}`, kindLabel(last.kind), `${when}测得`].filter(Boolean);
    return { text: parts.join(" · "), failed: false, raw: "" };
  }
  return { text: "延迟未测", failed: false, raw: "" };
}

/** 设备搜索：按名称、MagicDNS 名、组网 IP、系统和备注匹配，不分大小写；多个词用空格分开，需全部命中 */
export function matchDevice(d: Device, query: string, note = ""): boolean {
  const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return true;
  const text = [d.name, d.dnsName ?? "", d.os ?? "", note, ...d.ips].join(" ").toLowerCase();
  return words.every((w) => text.includes(w));
}

/** 一段延迟历史的统计；没有回应的测量只计入 timeouts */
export interface LatencyStats {
  /** 测量总次数（含没有回应的） */
  count: number;
  timeouts: number;
  /** 以下各项在没有任何回应时为 null */
  min: number | null;
  median: number | null;
  /** 95% 的测量不超过这个值 */
  p95: number | null;
  max: number | null;
  /** 有回应的测量中走中继的比例（0～1） */
  relayRatio: number | null;
}

export function latencyStats(samples: Sample[]): LatencyStats {
  const ok = samples.filter((s) => s.ms != null);
  const sorted = ok.map((s) => s.ms!).sort((a, b) => a - b);
  const n = sorted.length;
  const relay = ok.filter((s) => s.kind === "relay" || s.kind === "peer-relay").length;
  return {
    count: samples.length,
    timeouts: samples.length - n,
    min: n ? sorted[0] : null,
    median: n ? sorted[Math.floor(n / 2)] : null,
    p95: n ? sorted[Math.ceil(n * 0.95) - 1] : null,
    max: n ? sorted[n - 1] : null,
    relayRatio: n ? relay / n : null,
  };
}
