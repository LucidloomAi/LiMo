//! Who is at the other end of a loopback connection: the process that made
//! it, and the account it runs in. The link asks for a connection that came
//! from the loopback address, where every program on the PC can connect, so
//! that one from a program in a child's account is closed at once.
//!
//! Windows' table of TCP connections with their owning processes
//! (`GetExtendedTcpTable`, `TCP_TABLE_OWNER_PID_CONNECTIONS`, IPv4) holds the
//! other end's own row: its local address and port are the connection's
//! peer, its remote ones the connection's own. That row names the process;
//! its token (`OpenProcess` with `PROCESS_QUERY_LIMITED_INFORMATION`, then
//! `OpenProcessToken`) names the account. The service, as SYSTEM, may read
//! any process's token. Anything Windows will not say gives no account, and
//! the connection is let in, so that the cable is never shut out by a read
//! that failed: the guardian password, the offer's secret and the phone's
//! key still guard everything behind it.

use std::ffi::c_void;
use std::net::{Ipv4Addr, SocketAddr};

use windows::Win32::NetworkManagement::IpHelper::{GetExtendedTcpTable, MIB_TCPROW_OWNER_PID, MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_CONNECTIONS};
use windows::Win32::Security::TOKEN_QUERY;
use windows::Win32::System::Threading::{OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION};

use super::Owned;
use super::sessions::token_user_sid;

/// IPv4, as `GetExtendedTcpTable` takes it (`AF_INET`).
const AF_INET: u32 = 2;
/// `ERROR_INSUFFICIENT_BUFFER`: the table grew between two calls.
const INSUFFICIENT_BUFFER: u32 = 122;

/// A port as the table holds it: the low 16 bits, in network order.
fn port_of(field: u32) -> u16 {
    u16::from_be(field as u16)
}

/// An address as the table holds it: in network order.
fn address_of(field: u32) -> Ipv4Addr {
    Ipv4Addr::from(field.to_ne_bytes())
}

/// The process at the other end of the connection whose own address is
/// `local` and whose peer is `peer`, from the table's rows.
fn owner_in(rows: &[MIB_TCPROW_OWNER_PID], local: SocketAddr, peer: SocketAddr) -> Option<u32> {
    let (SocketAddr::V4(local), SocketAddr::V4(peer)) = (local, peer) else { return None };
    rows.iter()
        .find(|row| {
            address_of(row.dwLocalAddr) == *peer.ip()
                && port_of(row.dwLocalPort) == peer.port()
                && address_of(row.dwRemoteAddr) == *local.ip()
                && port_of(row.dwRemotePort) == local.port()
        })
        .map(|row| row.dwOwningPid)
}

/// Every IPv4 TCP connection with its owning process.
fn tcp_rows() -> Option<Vec<MIB_TCPROW_OWNER_PID>> {
    for _ in 0..4 {
        let mut size: u32 = 0;
        // SAFETY: only asks how big the table is.
        unsafe { GetExtendedTcpTable(None, &mut size, false, AF_INET, TCP_TABLE_OWNER_PID_CONNECTIONS, 0) };
        if size == 0 {
            return None;
        }
        // Room for some more rows, should the table grow meanwhile.
        let mut buffer = vec![0u64; (size as usize).div_ceil(8) + 64];
        let mut room = u32::try_from(buffer.len() * 8).ok()?;
        // SAFETY: writes at most `room` bytes into a buffer that big,
        // aligned for the table's structures.
        let status = unsafe {
            GetExtendedTcpTable(Some(buffer.as_mut_ptr() as *mut c_void), &mut room, false, AF_INET, TCP_TABLE_OWNER_PID_CONNECTIONS, 0)
        };
        if status == INSUFFICIENT_BUFFER {
            continue;
        }
        if status != 0 {
            return None;
        }
        // SAFETY: the buffer now holds a MIB_TCPTABLE_OWNER_PID; its rows
        // are read only within its count, which Windows wrote, and only as
        // far as the buffer goes.
        unsafe {
            let table = &*(buffer.as_ptr() as *const MIB_TCPTABLE_OWNER_PID);
            let count = table.dwNumEntries as usize;
            let most = (buffer.len() * 8).saturating_sub(4) / std::mem::size_of::<MIB_TCPROW_OWNER_PID>();
            let rows = std::slice::from_raw_parts(table.table.as_ptr(), count.min(most));
            return Some(rows.to_vec());
        }
    }
    None
}

/// The account a process runs in, as a security identifier.
fn process_account(pid: u32) -> Option<String> {
    // SAFETY: opens the process for reading what little it allows; the
    // handle is owned at once.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let process = Owned::new(process)?;
    let mut token = windows::Win32::Foundation::HANDLE::default();
    // SAFETY: opens the process's token for reading; owned at once.
    unsafe { OpenProcessToken(process.raw(), TOKEN_QUERY, &mut token) }.ok()?;
    let token = Owned::new(token)?;
    token_user_sid(token.raw())
}

/// The account of the process at the other end of a loopback connection
/// whose own address is `local` and whose peer is `peer`, when Windows says.
pub(crate) fn loopback_peer_account(local: SocketAddr, peer: SocketAddr) -> Option<String> {
    if !peer.ip().is_loopback() {
        return None;
    }
    let pid = owner_in(&tcp_rows()?, local, peer)?;
    process_account(pid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{TcpListener, TcpStream};

    fn row(local: SocketAddr, remote: SocketAddr, pid: u32) -> MIB_TCPROW_OWNER_PID {
        let (SocketAddr::V4(local), SocketAddr::V4(remote)) = (local, remote) else { panic!() };
        MIB_TCPROW_OWNER_PID {
            dwState: 5,
            dwLocalAddr: u32::from_ne_bytes(local.ip().octets()),
            dwLocalPort: u32::from(local.port().to_be()),
            dwRemoteAddr: u32::from_ne_bytes(remote.ip().octets()),
            dwRemotePort: u32::from(remote.port().to_be()),
            dwOwningPid: pid,
        }
    }

    #[test]
    fn the_other_ends_own_row_names_the_process() {
        let server: SocketAddr = "127.0.0.1:47810".parse().unwrap();
        let client: SocketAddr = "127.0.0.1:52000".parse().unwrap();
        let rows = [row(server, client, 4), row(client, server, 77), row("127.0.0.1:52001".parse().unwrap(), server, 88)];
        assert_eq!(owner_in(&rows, server, client), Some(77));
        assert_eq!(owner_in(&rows, server, "127.0.0.1:52002".parse().unwrap()), None);
        assert_eq!(port_of(u32::from(47810u16.to_be())), 47810);
    }

    /// A loopback connection inside this test process, which changes
    /// nothing: its other end is this process, in this account.
    #[test]
    fn a_loopback_connection_of_this_process_is_this_accounts() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let _client = TcpStream::connect(address).unwrap();
        let (accepted, peer) = listener.accept().unwrap();
        let local = accepted.local_addr().unwrap();
        assert_eq!(loopback_peer_account(local, peer), super::super::door::this_account());
        assert_eq!(loopback_peer_account(local, "192.168.1.20:5000".parse().unwrap()), None);
    }
}
