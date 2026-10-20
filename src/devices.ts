// 设备页的列表逻辑：筛选、收藏置顶、按连接状态统计。纯函数，便于单元测试
import type { Device } from "./api";
import { connTone, matchDevice } from "./format";

/** 设备搜索与筛选条件 */
export interface PeerFilter {
  /** 搜索词，空格分隔多个词 */
  query: string;
  /** 只看在线 */
  onlineOnly: boolean;
}

/** 有没有在筛选（决定标题上是否显示“符合条件数 / 总数”） */
export const isFiltering = (f: PeerFilter): boolean => f.query.trim() !== "" || f.onlineOnly;

/** 本机给设备加的备注和收藏（存在设置里） */
export interface PeerMarks {
  /** 设备 id → 备注 */
  notes: Record<string, string>;
  /** 收藏的设备 id */
  favorites: string[];
}

/** 按搜索词（也匹配备注）和“只看在线”筛选，保持原有顺序 */
export function filterPeers(peers: Device[], f: PeerFilter, notes: Record<string, string> = {}): Device[] {
  return peers.filter((d) => matchDevice(d, f.query, notes[d.id]) && (!f.onlineOnly || d.conn.kind !== "offline"));
}

/** 设备列表的最终顺序：先筛选，再把收藏的设备提到最前；两组内部保持后端的顺序（在线在前、路由器在前、按名称） */
export function arrangePeers(peers: Device[], f: PeerFilter, marks: PeerMarks): Device[] {
  const fav = new Set(marks.favorites);
  const shown = filterPeers(peers, f, marks.notes);
  return [...shown.filter((d) => fav.has(d.id)), ...shown.filter((d) => !fav.has(d.id))];
}

/** 概览里的一项：状态色名称、中文、台数 */
export interface PeerStat {
  tone: string;
  label: string;
  n: number;
}

/** 按连接状态统计台数；直连、中继、离线总是显示，打洞中和空闲没有设备时不显示 */
export function peerStats(peers: Device[]): PeerStat[] {
  const count = (tone: string) => peers.filter((p) => connTone(p.conn.kind) === tone).length;
  return [
    { tone: "direct", label: "直连", n: count("direct"), always: true },
    { tone: "relay", label: "中继", n: count("relay"), always: true },
    { tone: "punching", label: "打洞中", n: count("punching"), always: false },
    { tone: "idle", label: "在线·空闲", n: count("idle"), always: false },
    { tone: "offline", label: "离线", n: count("offline"), always: true },
  ]
    .filter((s) => s.always || s.n > 0)
    .map(({ tone, label, n }) => ({ tone, label, n }));
}
