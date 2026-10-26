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
  /** ~/.ssh/config 中对应的别名；没有则不显示 SSH 按钮 */
  sshAlias: string | null;
  /** 这台设备可以当出口 */
  exitOption: boolean;
  /** 本机正在用这台设备当出口 */
  exitActive: boolean;
}

/** 本机的组网选项，各项为 null 表示读取不到 */
export interface LocalPrefs {
  acceptRoutes: boolean | null;
  /** 选用的出口设备 id；null 表示不用出口 */
  exitNodeId: string | null;
  /** 走出口时仍可访问本地局域网 */
  exitNodeAllowLan: boolean | null;
  /** 使用组网 DNS（accept-dns） */
  acceptDns: boolean | null;
  /** 挡住所有传入连接（shields-up） */
  shieldsUp: boolean | null;
  /** 无人登录时保持连接（unattended） */
  unattended: boolean | null;
}

/** 可在界面上开关的本机组网选项 */
export type PrefSwitch = "exit-node-allow-lan" | "accept-dns" | "shields-up" | "unattended";

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
  /** 本机的组网选项；读取失败为 null */
  prefs: LocalPrefs | null;
  subnetRoutes: SubnetRoute[];
  location: Location;
  /** 本机是否装了 RustDesk */
  rustdesk: boolean;
  /** 子网路由器的健康状态 */
  routers: RouterHealth[];
  /** 官方客户端有新版本时为新版本号 */
  clientUpdate: string | null;
  /** MeshDock 自身的新版本检查结果 */
  appUpdate: AppUpdateState;
}

/** MeshDock 自身的新版本检查结果（读取 GitHub 发布里的 latest.json） */
export type AppUpdateState =
  | { state: "unknown" }
  | { state: "latest" }
  /** installable：能否一键更新（免安装版要发布里有免安装版的下载项） */
  | { state: "available"; version: string; url: string; notes: string | null; installable: boolean }
  | { state: "unavailable"; reason: string };

/** 在浏览器中打开新版本的发布页 */
export const openAppRelease = () => invoke<void>("open_app_release");
/** 下载并安装新版本：成功时程序退出并自动重新打开，所以只有失败才会返回 */
export const installAppUpdate = () => invoke<void>("install_app_update");

/** 新版本的下载进度；total 为 null 表示服务器没给出总大小 */
export interface UpdateProgress {
  downloaded: number;
  total: number | null;
}
export const onUpdateProgress = (cb: (p: UpdateProgress) => void): Promise<UnlistenFn> =>
  listen<UpdateProgress>("app-update-progress", (e) => cb(e.payload));

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
  /** 家里路由器地址：默认网关等于它即为在家；空表示还没设置 */
  homeGateway: string;
  /** 自建中继的显示名（如“广州”），空时用 derp-map 里的名称或“自建” */
  relayName: string;
  theme: ThemeMode;
  /** 配色方案，见 palette.ts 的 PALETTES；与浅色 / 深色互相独立 */
  palette: string;
  launchOnBoot: boolean;
  /** 路径变化、模式开错时发 Windows 通知 */
  notify: boolean;
  /** 点关闭按钮时最小化到托盘；false 时直接退出程序 */
  closeToTray: boolean;
  /** 窗口打开时的状态刷新间隔（秒）：5 / 10 / 30 / 60 */
  refreshSecs: number;
  /** 后台测速间隔（分钟）：1 / 2 / 5 / 10 */
  sampleMinutes: number;
  /** 延迟持续偏高提醒的阈值（毫秒）：0（关闭）/ 100 / 200 / 500 */
  latencyAlertMs: number;
  /** 持续走中继多久后提醒（分钟）：0（关闭）/ 10 / 30 / 60 */
  relayAlertMinutes: number;
  /** 设备 id → 备注（只存本机） */
  deviceNotes: Record<string, string>;
  /** 收藏的设备 id，排在设备列表最前面 */
  favorites: string[];
  /** 按网关识别到位置变化、模式不对时自动切换在家 / 出门模式 */
  autoMode: boolean;
}

/** 保存全部设置并立即生效（主题、开机自启） */
export const saveSettings = (settings: Settings) => invoke<Settings>("save_settings", { settings });

export interface AppInfo {
  version: string;
  settingsFile: string;
  /** 便携模式（程序旁有 data 目录） */
  portable: boolean;
  /** 安装版（可自动更新）；免安装版只能到下载页下载 */
  installed: boolean;
  /** 开发版：开机自启不会真正写入 */
  debug: boolean;
}

export const getAppInfo = () => invoke<AppInfo>("get_app_info");

export const getSettings = () => invoke<Settings>("get_settings");

/** 切换在家（false）/ 出门（true）模式，返回切换后的快照 */
export const setAcceptRoutes = (on: boolean) => invoke<Snapshot>("set_accept_routes", { on });

/** 选用出口设备（传设备 id），null 表示停用，返回生效后的快照 */
export const setExitNode = (id: string | null) => invoke<Snapshot>("set_exit_node", { id });
/** 开关本机组网选项，返回生效后的快照 */
export const setPrefSwitch = (pref: PrefSwitch, on: boolean) =>
  invoke<Snapshot>("set_pref_switch", { switch: pref, on });

export interface SendResult {
  device: string;
  /** 发出的文件名 */
  files: string[];
}
/** 弹出选文件对话框并用 Taildrop 发给设备；取消选择时返回 null */
export const sendFiles = (id: string) => invoke<SendResult | null>("send_files", { id });
/** 把刚拖到设备卡片上的文件发给这台设备（文件路径由后端记下，不经过前端） */
export const sendDropped = (id: string) => invoke<SendResult>("send_dropped", { id });

/** 共享方式：网页或任意 TCP（HTTPS 需要证书，Headscale 不支持） */
export type ServeKind = "http" | "tcp";

/** 一条共享规则（tailscale serve） */
export interface ServeRule {
  /** "http"、"https" 或 "tcp" */
  kind: string;
  /** 组网里对外的端口 */
  port: number;
  /** 转发目标 */
  target: string;
}
export const getServeRules = () => invoke<ServeRule[]>("get_serve_rules");
/** 新增规则，返回新的规则列表 */
export const addServeRule = (kind: ServeKind, port: number, localPort: number) =>
  invoke<ServeRule[]>("add_serve_rule", { kind, port, localPort });
/** 删除规则，返回新的规则列表 */
export const removeServeRule = (kind: string, port: number) =>
  invoke<ServeRule[]>("remove_serve_rule", { kind, port });

/** 已登录的一个账户 */
export interface Profile {
  id: string;
  nickname: string;
  /** 组网名；Headscale 下是控制端地址 */
  tailnet: string;
  account: string;
  selected: boolean;
}
export const getProfiles = () => invoke<Profile[]>("get_profiles");
/** 切换到另一个已登录的账户，调用前需二次确认 */
export const switchProfile = (id: string) => invoke<Snapshot>("switch_profile", { id });

/** 取走托盘发起的模式切换请求 */
export const takeModeRequest = () => invoke<boolean | null>("take_mode_request");
export const onModeRequest = (cb: () => void): Promise<UnlistenFn> => listen("mode-request", () => cb());

/** 在新终端中执行 ssh <别名> */
export const openSsh = (alias: string) => invoke<void>("open_ssh", { alias });
/** 用 RustDesk 直连组网 IP */
export const openRustdesk = (ip: string) => invoke<void>("open_rustdesk", { ip });

/** 官方客户端的启动方式 */
export interface ClientSetup {
  /** 无人值守：没有用户登录、托盘退出时组网仍保持连接 */
  unattended: boolean;
  /** 启动文件夹里有官方托盘的快捷方式 */
  shortcut: boolean;
  /** 开机时启动官方托盘 */
  trayAutostart: boolean;
  /** 执行的账户是管理员（远端时即 ssh 登录的账户） */
  admin: boolean;
}
/** alias 为 null 读本机，否则经 ssh 读对方（Windows） */
export const getClientSetup = (alias: string | null) => invoke<ClientSetup>("get_client_setup", { alias });
/** 开关官方托盘的开机自启：本机会弹出 UAC 确认框，对方需 ssh 账户是管理员 */
export const setTrayAutostart = (alias: string | null, on: boolean) =>
  invoke<ClientSetup>("set_tray_autostart", { alias, on });
/** 经 ssh 开关对方的无人值守 */
export const setRemoteUnattended = (alias: string, on: boolean) =>
  invoke<ClientSetup>("set_remote_unattended", { alias, on });

/** 一个分区的用量 */
export interface Disk {
  mount: string;
  totalKb: number;
  usedKb: number;
}

/** 电池；tempC 为摄氏度，status 是内核原文（Charging、Discharging、Full、Not charging） */
export interface Battery {
  percent: number | null;
  tempC: number | null;
  status: string | null;
}

/** 经 ssh 采集的 Linux 设备系统信息，读不到的项为 null 或空数组 */
export interface SysInfo {
  uptimeSecs: number | null;
  /** 1、5、15 分钟平均负载 */
  load: [number, number, number] | null;
  cpus: number | null;
  memTotalKb: number | null;
  memAvailableKb: number | null;
  swapTotalKb: number | null;
  swapFreeKb: number | null;
  disks: Disk[];
  /** 全局 IPv6 地址；为空说明拿不到 IPv6 */
  ipv6: string[];
  battery: Battery | null;
  /** 对外监听的 TCP 端口 */
  ports: number[];
}
/** 经 ssh <别名> 只读采集系统信息 */
export const getSysinfo = (alias: string) => invoke<SysInfo>("device_sysinfo", { alias });

/** 一次延迟测量；没有回应时 ms 与 kind 为 null */
export interface Sample {
  atMs: number;
  ms: number | null;
  kind: ConnKind | null;
}

/** 设备 id → 最近 24 小时的测量（按时间排列） */
export type History = Record<string, Sample[]>;

export const getHistory = () => invoke<History>("get_history");
/** 一台设备最近 7 天的延迟历史（详情弹窗用） */
export const getDeviceHistory = (id: string) => invoke<Sample[]>("get_device_history", { id });
/** 导出一台设备最近 7 天的延迟历史为 CSV，保存到“下载”文件夹，返回文件路径 */
export const exportHistory = (id: string) => invoke<string>("export_history", { id });
export const onHistory = (cb: (h: History) => void): Promise<UnlistenFn> =>
  listen<History>("history", (e) => cb(e.payload));

/** 生成诊断报告（已打码），约 5 秒 */
export const buildReport = () => invoke<string>("build_report");
/** 保存到“下载”文件夹并在资源管理器中选中，返回文件路径 */
export const saveReport = (text: string) => invoke<string>("save_report", { text });

/** 子网路由器的健康状态 */
export interface RouterHealth {
  name: string;
  online: boolean;
  routes: string[];
  /** 最严重的结论级别 */
  level: Level;
  findings: Finding[];
  /** 到自建中继的延迟（毫秒） */
  relayMs: number | null;
  /** 自建中继的显示名 */
  relayName: string;
  version: string | null;
  osVersion: string | null;
  deviceModel: string | null;
  lastSeenMs: number | null;
}

/** 连接组网（tailscale up，不改动现有设置） */
export const connect = () => invoke<Snapshot>("connect");
/** 断开组网（tailscale down），调用前需二次确认 */
export const disconnect = () => invoke<Snapshot>("disconnect");
