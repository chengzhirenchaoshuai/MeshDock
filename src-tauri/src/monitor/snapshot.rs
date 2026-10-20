//! 推给前端的快照数据：设备、组网概况、在不在家，以及由组网状态整理设备的规则
use crate::classify::{Conn, ConnKind};
use crate::cli::CliError;
use crate::model::{LocalPrefs, PeerStatus, non_empty, time_ms};
use crate::router::RouterHealth;
use serde::Serialize;

/// 一台设备，给前端展示用
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub name: String,
    pub dns_name: Option<String>,
    pub ips: Vec<String>,
    pub os: Option<String>,
    pub online: Option<bool>,
    pub conn: Conn,
    pub rx_bytes: Option<u64>,
    pub tx_bytes: Option<u64>,
    pub last_seen_ms: Option<i64>,
    pub last_handshake_ms: Option<i64>,
    /// 节点公钥，只在后端用来对应 netmap，不发给前端
    #[serde(skip)]
    pub key: Option<String>,
    /// ~/.ssh/config 中对应这台设备的别名，没有则不显示 SSH 按钮
    pub ssh_alias: Option<String>,
    /// 这台设备可以当出口
    pub exit_option: bool,
    /// 本机正在用这台设备当出口
    pub exit_active: bool,
}

/// 组网整体情况
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub backend_state: Option<String>,
    pub control_unreachable: bool,
    pub version: Option<String>,
    pub self_device: Option<Device>,
    pub peers: Vec<Device>,
    /// 本机 accept-routes 的实际值（读取 debug prefs）；读取失败为 None
    pub accept_routes: Option<bool>,
    /// 本机的组网选项（出口、DNS、传入连接等）；读取失败为 None
    pub prefs: Option<LocalPrefs>,
    /// 组网里发布了子网路由的设备
    pub subnet_routes: Vec<SubnetRoute>,
    /// 本机在不在家
    pub location: Location,
    /// 本机是否装了 RustDesk（决定是否显示 RustDesk 按钮）
    pub rustdesk: bool,
    /// 子网路由器的健康状态
    pub routers: Vec<RouterHealth>,
    /// 官方客户端有新版本时为新版本号
    pub client_update: Option<String>,
}

/// 在不在家：优先按默认网关自动识别，读不到网关时用手动标记
#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    /// None 表示无法判断（读不到网关且没有手动标记）
    pub at_home: Option<bool>,
    /// true 为按网关自动识别，false 为手动标记
    pub auto: bool,
    /// 当前所有默认网关
    pub gateways: Vec<String>,
    /// 设置中的家里网关
    pub home_gateway: String,
}

/// 某台设备发布的子网路由
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SubnetRoute {
    pub device: String,
    pub routes: Vec<String>,
}

/// 一次刷新的结果：成功时有 overview，失败时有 error
#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub overview: Option<Overview>,
    pub error: Option<CliError>,
    pub updated_at_ms: i64,
}

impl Device {
    /// 组网 IP，优先 IPv4
    pub(super) fn main_ip(&self) -> Option<String> {
        self.ips
            .iter()
            .find(|a| !a.contains(':'))
            .or(self.ips.first())
            .cloned()
    }
}

pub(super) fn device(p: &PeerStatus, conn: Conn) -> Device {
    Device {
        id: p.id.clone().unwrap_or_default(),
        name: p.display_name().unwrap_or("未知设备").to_string(),
        dns_name: non_empty(&p.dns_name).map(|s| s.trim_end_matches('.').to_string()),
        ips: p.tailscale_ips.clone().unwrap_or_default(),
        os: non_empty(&p.os).map(str::to_string),
        online: p.online,
        conn,
        rx_bytes: p.rx_bytes,
        tx_bytes: p.tx_bytes,
        last_seen_ms: time_ms(p.last_seen.as_deref()),
        last_handshake_ms: time_ms(p.last_handshake.as_deref()),
        key: p.public_key.clone(),
        ssh_alias: None,
        exit_option: p.exit_node_option == Some(true),
        exit_active: p.exit_node == Some(true),
    }
}

/// 选用的出口设备离线或已不在组网中时返回它的名字；没选出口返回 None
pub(super) fn exit_offline(prefs: Option<&LocalPrefs>, peers: &[Device]) -> Option<String> {
    let id = prefs?.exit_node_id.as_ref()?;
    match peers.iter().find(|d| &d.id == id) {
        Some(d) => (d.conn.kind == ConnKind::Offline).then(|| d.name.clone()),
        None => Some("（已不在组网中）".into()),
    }
}

/// 设备卡片的排序键：在线的在前、离线的在后；在线里子网路由器在前；同组按名称（不分大小写）。
/// 不按连接方式或延迟排，避免卡片随路径变化来回跳
pub(super) fn peer_order(offline: bool, router: bool, name: &str) -> (bool, bool, String) {
    (offline, !router, name.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(id: &str, kind: ConnKind) -> Device {
        Device {
            id: id.into(),
            name: format!("{id}-名"),
            dns_name: None,
            ips: vec![],
            os: None,
            online: None,
            conn: Conn { kind, label: String::new(), hint: None },
            rx_bytes: None,
            tx_bytes: None,
            last_seen_ms: None,
            last_handshake_ms: None,
            key: None,
            ssh_alias: None,
            exit_option: true,
            exit_active: false,
        }
    }

    #[test]
    fn 出口设备离线判断() {
        let peers = [dev("a", ConnKind::Lan), dev("b", ConnKind::Offline)];
        let pick = |id: Option<&str>| LocalPrefs {
            exit_node_id: id.map(str::to_string),
            ..Default::default()
        };
        assert_eq!(exit_offline(None, &peers), None);
        assert_eq!(exit_offline(Some(&pick(None)), &peers), None);
        assert_eq!(exit_offline(Some(&pick(Some("a"))), &peers), None);
        assert_eq!(exit_offline(Some(&pick(Some("b"))), &peers).as_deref(), Some("b-名"));
        assert_eq!(
            exit_offline(Some(&pick(Some("x"))), &peers).as_deref(),
            Some("（已不在组网中）")
        );
    }

    #[test]
    fn 设备排序_在线在前_路由器在前_再按名称() {
        let mut list = [
            ("ming-pc", true, false),
            ("peng-work", false, false),
            ("Alpha", false, false),
            ("phone-router", false, true),
            ("box", true, false),
        ];
        list.sort_by_key(|&(name, offline, router)| peer_order(offline, router, name));
        let names: Vec<_> = list.iter().map(|t| t.0).collect();
        assert_eq!(names, ["phone-router", "Alpha", "peng-work", "box", "ming-pc"]);
    }
}
