//! This PC's IPv4 addresses, per interface, from if-addrs (the same crate
//! mdns-sd uses, so both see the same list).

use std::net::Ipv4Addr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V4Addr {
    /// The interface's friendly name, such as "Wi-Fi".
    pub interface: String,
    /// The interface index, which Windows also uses for network profiles.
    pub index: Option<u32>,
    pub ip: Ipv4Addr,
    pub prefix: u8,
    pub up: bool,
}

/// Every IPv4 address on this PC, loopback included, sorted by interface
/// index and then address.
pub fn ipv4_addresses() -> Result<Vec<V4Addr>, String> {
    let all = if_addrs::get_if_addrs().map_err(|e| e.to_string())?;
    let mut list: Vec<V4Addr> = all
        .into_iter()
        .filter_map(|i| match i.addr {
            if_addrs::IfAddr::V4(v4) => Some(V4Addr {
                interface: i.name.clone(),
                index: i.index,
                ip: v4.ip,
                prefix: v4.prefixlen,
                up: matches!(i.oper_status, if_addrs::IfOperStatus::Up),
            }),
            _ => None,
        })
        .collect();
    list.sort_by_key(|a| (a.index.unwrap_or(u32::MAX), u32::from(a.ip)));
    Ok(list)
}
