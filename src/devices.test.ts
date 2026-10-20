import { describe, expect, it } from "vitest";
import type { ConnKind, Device } from "./api";
import { arrangePeers, filterPeers, isFiltering, peerStats } from "./devices";

const dev = (name: string, kind: ConnKind, ip = "100.64.0.1"): Device => ({
  id: name,
  name,
  dnsName: null,
  ips: [ip],
  os: "windows",
  online: kind !== "offline",
  conn: { kind, label: "", hint: null },
  rxBytes: null,
  txBytes: null,
  lastSeenMs: null,
  lastHandshakeMs: null,
  sshAlias: null,
  exitOption: false,
  exitActive: false,
});

const peers = [dev("ming-pc", "lan", "100.64.0.3"), dev("peng-work", "relay"), dev("box", "offline"), dev("tv", "idle")];

describe("filterPeers", () => {
  it("不筛选时原样返回", () => {
    expect(filterPeers(peers, { query: "", onlineOnly: false })).toEqual(peers);
    expect(isFiltering({ query: "  ", onlineOnly: false })).toBe(false);
  });

  it("搜索词和只看在线同时生效，保持原顺序", () => {
    expect(filterPeers(peers, { query: "", onlineOnly: true }).map((d) => d.name)).toEqual(["ming-pc", "peng-work", "tv"]);
    expect(filterPeers(peers, { query: "100.64.0.3", onlineOnly: true }).map((d) => d.name)).toEqual(["ming-pc"]);
    expect(filterPeers(peers, { query: "box", onlineOnly: true })).toEqual([]);
    expect(isFiltering({ query: "", onlineOnly: true })).toBe(true);
  });
});

describe("arrangePeers", () => {
  it("收藏的排在最前，两组内部保持原顺序", () => {
    const marks = { notes: {}, favorites: ["box", "peng-work"] };
    expect(arrangePeers(peers, { query: "", onlineOnly: false }, marks).map((d) => d.name)).toEqual([
      "peng-work",
      "box",
      "ming-pc",
      "tv",
    ]);
    // 筛选后再置顶：离线的收藏被“只看在线”滤掉
    expect(arrangePeers(peers, { query: "", onlineOnly: true }, marks).map((d) => d.name)).toEqual([
      "peng-work",
      "ming-pc",
      "tv",
    ]);
  });

  it("搜索也匹配备注", () => {
    const marks = { notes: { tv: "客厅电视" }, favorites: [] };
    expect(arrangePeers(peers, { query: "客厅", onlineOnly: false }, marks).map((d) => d.name)).toEqual(["tv"]);
  });
});

describe("peerStats", () => {
  it("直连、中继、离线总是显示，打洞中没有时不显示", () => {
    expect(peerStats(peers)).toEqual([
      { tone: "direct", label: "直连", n: 1 },
      { tone: "relay", label: "中继", n: 1 },
      { tone: "idle", label: "在线·空闲", n: 1 },
      { tone: "offline", label: "离线", n: 1 },
    ]);
    expect(peerStats([]).map((s) => s.n)).toEqual([0, 0, 0]);
  });
});
