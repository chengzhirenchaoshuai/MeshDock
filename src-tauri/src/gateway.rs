//! 读取本机 IPv4 默认网关，用来自动识别在不在家。直接调用 Windows API，不启动进程
use std::net::Ipv4Addr;

/// 当前所有 IPv4 默认路由（0.0.0.0/0）的网关，去重排序。
/// 代理软件的虚拟网卡也可能有默认路由，所以返回全部，由调用方判断其中有没有家里的网关
#[cfg(windows)]
pub fn default_gateways() -> Vec<Ipv4Addr> {
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        FreeMibTable, GetIpForwardTable2, MIB_IPFORWARD_TABLE2,
    };
    use windows_sys::Win32::Networking::WinSock::AF_INET;

    let mut table: *mut MIB_IPFORWARD_TABLE2 = std::ptr::null_mut();
    // SAFETY: 调用成功后 table 指向系统分配的路由表，读取完用 FreeMibTable 释放
    let mut gateways = unsafe {
        if GetIpForwardTable2(AF_INET, &mut table) != 0 || table.is_null() {
            return Vec::new();
        }
        let rows =
            std::slice::from_raw_parts((*table).Table.as_ptr(), (*table).NumEntries as usize);
        let list: Vec<Ipv4Addr> = rows
            .iter()
            .filter(|r| r.DestinationPrefix.PrefixLength == 0)
            .map(|r| Ipv4Addr::from(r.NextHop.Ipv4.sin_addr.S_un.S_addr.to_ne_bytes()))
            .filter(|ip| !ip.is_unspecified()) // 0.0.0.0 表示直连，不是网关
            .collect();
        FreeMibTable(table as *const _);
        list
    };
    gateways.sort();
    gateways.dedup();
    gateways
}

#[cfg(not(windows))]
pub fn default_gateways() -> Vec<Ipv4Addr> {
    Vec::new()
}

/// 是否在家：读不到任何网关时返回 None（无法判断）
pub fn at_home(gateways: &[Ipv4Addr], home_gateway: &str) -> Option<bool> {
    if gateways.is_empty() {
        return None;
    }
    let home: Ipv4Addr = home_gateway.trim().parse().ok()?;
    Some(gateways.contains(&home))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 按网关判断在家() {
        let home = "192.168.66.66";
        let gw = |s: &[&str]| {
            s.iter()
                .map(|a| a.parse().unwrap())
                .collect::<Vec<Ipv4Addr>>()
        };
        assert_eq!(at_home(&gw(&["192.168.66.66"]), home), Some(true));
        // 代理软件的虚拟网卡也有默认路由时，仍能识别
        assert_eq!(
            at_home(&gw(&["198.18.0.1", "192.168.66.66"]), home),
            Some(true)
        );
        assert_eq!(at_home(&gw(&["192.168.43.1"]), home), Some(false));
        assert_eq!(at_home(&[], home), None);
        // 设置里填错了网关，无法判断
        assert_eq!(at_home(&gw(&["192.168.66.66"]), "不是地址"), None);
    }

    /// 读取本机真实路由表，需手动运行：cargo test -- --ignored
    #[test]
    #[ignore]
    fn 实机_读取默认网关() {
        let gws = default_gateways();
        println!("默认网关：{gws:?}");
        assert!(!gws.is_empty());
    }
}
