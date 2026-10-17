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

export interface SubnetRoute {
  device: string;
  routes: string[];
}

/** 在不在家：优先按默认网关自动识别，读不到网关时用手动标记 */
export interface Location {
  atHome: boolean | null;
  /** true 为自动识别，false 为手动标记 */
  auto: boolean;
  gateways: string[];
  homeGateway: string;
}

export interface Overview {
  backendState: string | null;
  controlUnreachable: boolean;
  version: string | null;
  selfDevice: Device | null;
  peers: Device[];
  /** 本机 accept-routes 实际值；读取失败为 null */
  acceptRoutes: boolean | null;
  subnetRoutes: SubnetRoute[];
  location: Location;
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

export interface PingResult {
  /** 收到回包时的实测路径 */
  conn: Conn | null;
  latencyMs: number | null;
  /** 没收到回包时的中文原因 */
  error: string | null;
  raw: string;
}

export const ping = (ip: string) => invoke<PingResult>("ping", { ip });

/** 前端保存的一次测试结果：成功时有 result，命令本身出错时有 error */
export interface PingRecord {
  result: PingResult | null;
  error: CliError | null;
  atMs: number;
}

export type Level = "good" | "warn" | "bad" | "info";

export interface Finding {
  level: Level;
  text: string;
}

export interface RegionLatency {
  id: string;
  name: string;
  ms: number;
  preferred: boolean;
  selfHosted: boolean;
}

/** 本机网络体检报告 */
export interface NetcheckReport {
  findings: Finding[];
  globalV4: string | null;
  globalV6: string | null;
  portMapping: string[];
  /** 按延迟从低到高 */
  regions: RegionLatency[];
  atMs: number;
}

export const runNetcheck = () => invoke<NetcheckReport>("run_netcheck");

export type ThemeMode = "system" | "light" | "dark";

export interface Settings {
  /** 手动标记的“我在家”：true 在家，false 在外，null 没标记过 */
  atHome: boolean | null;
  /** 家里路由器地址：默认网关等于它即为在家 */
  homeGateway: string;
  theme: ThemeMode;
  launchOnBoot: boolean;
}

/** 保存全部设置并立即生效（主题、开机自启） */
export const saveSettings = (settings: Settings) => invoke<Settings>("save_settings", { settings });

export interface AppInfo {
  version: string;
  settingsFile: string;
  /** 便携模式（程序旁有 data 目录） */
  portable: boolean;
  /** 开发版：开机自启不会真正写入 */
  debug: boolean;
}

export const getAppInfo = () => invoke<AppInfo>("get_app_info");

export const getSettings = () => invoke<Settings>("get_settings");

/** 切换在家（false）/ 出门（true）模式，返回切换后的快照 */
export const setAcceptRoutes = (on: boolean) => invoke<Snapshot>("set_accept_routes", { on });

/** 取走托盘发起的模式切换请求 */
export const takeModeRequest = () => invoke<boolean | null>("take_mode_request");
export const onModeRequest = (cb: () => void): Promise<UnlistenFn> => listen("mode-request", () => cb());
