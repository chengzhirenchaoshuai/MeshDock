// 展示用的格式化工具
import type { ConnKind } from "./api";

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
