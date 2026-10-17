// 与 Rust 端的接口：类型定义与调用封装
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type ConnKind =
  | "self-node"
  | "offline"
  | "lan"
  | "ipv6"
  | "public"
  | "peer-relay"
  | "punching"
  | "relay"
  | "idle"
  | "unknown";

export interface Conn {
  kind: ConnKind;
  label: string;
  hint: string | null;
}

export interface Device {
  id: string;
  name: string;
  dnsName: string | null;
  ips: string[];
  os: string | null;
  online: boolean | null;
  conn: Conn;
  rxBytes: number | null;
  txBytes: number | null;
  lastSeenMs: number | null;
  lastHandshakeMs: number | null;
}

export interface Overview {
  backendState: string | null;
  controlUnreachable: boolean;
  version: string | null;
  selfDevice: Device | null;
  peers: Device[];
}

export type ErrorKind = "not-installed" | "service-down" | "timeout" | "access-denied" | "failed";

export interface CliError {
  kind: ErrorKind;
  message: string;
  raw: string;
}

export interface Snapshot {
  overview: Overview | null;
  error: CliError | null;
  updatedAtMs: number;
}

export const getSnapshot = () => invoke<Snapshot>("get_snapshot");
export const refresh = () => invoke<Snapshot>("refresh");

/** 后台每次刷新都会推送快照 */
export const onSnapshot = (cb: (s: Snapshot) => void): Promise<UnlistenFn> =>
  listen<Snapshot>("snapshot", (e) => cb(e.payload));
