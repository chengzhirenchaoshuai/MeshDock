import { describe, expect, it } from "vitest";
import type { ConnKind, Device } from "./api";
import { filterPeers, isFiltering, peerStats } from "./devices";

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
