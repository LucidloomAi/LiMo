//! Announcing the master PC on the home network, so that a paired phone
//! finds it there (multicast DNS service discovery, with `mdns-sd`), and the
//! PC's own addresses on that network, for a pairing offer.
//!
//! What counts as the home network: the PC's IPv4 addresses in the private
//! ranges home routers give out (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16),
//! never loopback, never the self assigned 169.254.0.0/16 of a network with
//! no router, never the 100.64.0.0/10 that carrier and virtual private
//! networks use, never IPv6, never a public address. The announcement goes
//! out on those interfaces only, and holds nothing of the family: see
//! [`what_is_announced`]. mDNS is multicast to 224.0.0.251, which routers
//! never pass on beyond the local network.
//!
//! The service announces only while the family has a phone paired or an
//! offer for one open; a PC with no phone says nothing on the network.

#![forbid(unsafe_code)]

use std::net::{IpAddr, Ipv4Addr};

use limo_link::MasterId;
use limo_link::discovery::{SERVICE_TYPE, host_name, instance_name, txt_records};
use mdns_sd::{IfKind, IfPredicate, ServiceDaemon, ServiceInfo};

/// The most addresses an offer carries: the link's own limit.
pub const MAX_HOME_ADDRESSES: usize = limo_link::MAX_OFFER_ADDRESSES;

/// True for an address of the home network, as this module says it.
pub fn is_home_address(address: Ipv4Addr) -> bool {
    let [a, b, ..] = address.octets();
    let private = a == 10 || (a == 172 && (16..=31).contains(&b)) || (a == 192 && b == 168);
    private && !address.is_loopback() && !address.is_link_local() && !address.is_unspecified()
}

/// The home addresses among `addresses`, each once, in order, at most
/// [`MAX_HOME_ADDRESSES`].
pub fn home_addresses_of(addresses: impl IntoIterator<Item = IpAddr>) -> Vec<Ipv4Addr> {
    let mut home = Vec::new();
    for address in addresses {
        if let IpAddr::V4(v4) = address
            && is_home_address(v4)
            && !home.contains(&v4)
            && home.len() < MAX_HOME_ADDRESSES
        {
            home.push(v4);
        }
    }
    home
}

/// The PC's own home addresses, as its network interfaces say now.
pub fn home_addresses() -> Vec<Ipv4Addr> {
    match if_addrs::get_if_addrs() {
        Ok(interfaces) => home_addresses_of(interfaces.iter().map(|i| i.ip())),
        Err(_) => Vec::new(),
    }
}

/// Everything the announcement holds, in words, for the log and the
/// README: the service type, the instance, the host, the port, and the TXT
/// records. The PC's name, the family, a child or a device is never in it.
pub fn what_is_announced(id: &MasterId, port: u16) -> String {
    let txt: Vec<String> = txt_records(id).into_iter().map(|(k, v)| format!("{k}={v}")).collect();
    format!(
        "service {SERVICE_TYPE}, instance \"{}\", host {}, port {port}, TXT {}, and the PC’s address on each home network",
        instance_name(id),
        host_name(id),
        txt.join(" ")
    )
}

/// Announcing, as the service asks it.
pub trait Announce: Send {
    /// Announces the master with this id at this port, on the home
    /// networks; again when called again with the same, it changes nothing.
    fn announce(&mut self, id: &MasterId, port: u16) -> Result<(), String>;
    /// Withdraws the announcement, with a goodbye on the network.
    fn withdraw(&mut self);
}

/// Announces nothing: for a service with no network, and the tests.
#[derive(Debug, Default)]
pub struct NoAnnouncement;

impl Announce for NoAnnouncement {
    fn announce(&mut self, _id: &MasterId, _port: u16) -> Result<(), String> {
        Ok(())
    }

    fn withdraw(&mut self) {}
}

/// The announcement with `mdns-sd`.
pub struct MdnsAnnouncer {
    daemon: Option<ServiceDaemon>,
    registered: Option<(String, MasterId, u16)>,
}

impl std::fmt::Debug for MdnsAnnouncer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MdnsAnnouncer").field("announced", &self.registered.as_ref().map(|r| &r.0)).finish()
    }
}

impl MdnsAnnouncer {
    /// An announcer that has not started its daemon yet.
    pub fn new() -> MdnsAnnouncer {
        MdnsAnnouncer { daemon: None, registered: None }
    }

    fn daemon(&mut self) -> Result<&ServiceDaemon, String> {
        if self.daemon.is_none() {
            let daemon = ServiceDaemon::new().map_err(|e| format!("mDNS could not start: {e}"))?;
            // Every interface off, then the home networks' IPv4 ones on:
            // the later choice wins for an interface both name.
            daemon.disable_interface(IfKind::All).map_err(|e| e.to_string())?;
            let home = IfPredicate::new(|interface| match interface.ip() {
                IpAddr::V4(v4) => is_home_address(v4),
                IpAddr::V6(_) => false,
            });
            daemon.enable_interface(IfKind::Predicate(home)).map_err(|e| e.to_string())?;
            self.daemon = Some(daemon);
        }
        self.daemon.as_ref().ok_or_else(|| "mDNS did not start".to_string())
    }
}

impl Default for MdnsAnnouncer {
    fn default() -> MdnsAnnouncer {
        MdnsAnnouncer::new()
    }
}

impl Announce for MdnsAnnouncer {
    fn announce(&mut self, id: &MasterId, port: u16) -> Result<(), String> {
        if self.registered.as_ref().is_some_and(|(_, held, held_port)| held == id && *held_port == port) {
            return Ok(());
        }
        self.withdraw();
        let txt = txt_records(id);
        let properties: Vec<(&str, &str)> = txt.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        // No addresses given: each interface announces its own address.
        let info = ServiceInfo::new(SERVICE_TYPE, &instance_name(id), &host_name(id), "", port, &properties[..])
            .map_err(|e| format!("the announcement could not be made: {e}"))?
            .enable_addr_auto();
        let fullname = info.get_fullname().to_string();
        self.daemon()?.register(info).map_err(|e| format!("the announcement could not be made: {e}"))?;
        self.registered = Some((fullname, *id, port));
        Ok(())
    }

    fn withdraw(&mut self) {
        if let (Some((fullname, ..)), Some(daemon)) = (self.registered.take(), self.daemon.as_ref())
            && let Ok(done) = daemon.unregister(&fullname)
        {
            let _ = done.recv_timeout(std::time::Duration::from_secs(1));
        }
    }
}

impl Drop for MdnsAnnouncer {
    fn drop(&mut self) {
        self.withdraw();
        if let Some(daemon) = self.daemon.take() {
            let _ = daemon.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_private_ranges_of_home_routers_are_home() {
        for home in ["192.168.1.20", "10.0.0.5", "172.16.4.1", "172.31.255.254"] {
            assert!(is_home_address(home.parse().unwrap()), "{home}");
        }
        for not_home in ["127.0.0.1", "169.254.10.2", "100.64.0.1", "100.100.1.1", "172.32.0.1", "8.8.8.8", "0.0.0.0", "192.169.1.1"]
        {
            assert!(!is_home_address(not_home.parse().unwrap()), "{not_home}");
        }
        let mixed: Vec<IpAddr> = ["127.0.0.1", "192.168.1.20", "::1", "fe80::1", "192.168.1.20", "10.0.0.5", "100.64.0.1"]
            .iter()
            .map(|a| a.parse().unwrap())
            .collect();
        assert_eq!(home_addresses_of(mixed), vec![Ipv4Addr::new(192, 168, 1, 20), Ipv4Addr::new(10, 0, 0, 5)]);
        let many = (1..=20u8).map(|n| IpAddr::V4(Ipv4Addr::new(10, 0, 0, n)));
        assert_eq!(home_addresses_of(many).len(), MAX_HOME_ADDRESSES);
    }

    #[test]
    fn the_announcement_holds_the_id_and_the_port_and_nothing_of_the_family() {
        let id = MasterId::from_hex("1a2b3c4d5e6f7a8b").unwrap();
        let said = what_is_announced(&id, 47810);
        assert_eq!(
            said,
            "service _limo._tcp.local., instance \"LiMo 1a2b3c4d\", host limo-1a2b3c4d.local., port 47810, \
             TXT v=1 id=1a2b3c4d5e6f7a8b, and the PC’s address on each home network"
        );
    }

    /// Reads this PC's own addresses, which changes nothing, and passes
    /// whatever they are.
    #[test]
    fn this_pcs_home_addresses_read() {
        for address in home_addresses() {
            assert!(is_home_address(address));
        }
    }
}
