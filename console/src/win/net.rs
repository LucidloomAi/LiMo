//! What a measurement reads of the PC: which processes descend from one,
//! and which TCP connections each has. Both are read as any account may
//! read them; nothing is changed and nothing is sent.

use std::ffi::c_void;
use std::net::{Ipv4Addr, Ipv6Addr};

use windows::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_OWNER_PID, TCP_TABLE_OWNER_PID_ALL,
};
use windows::Win32::Networking::WinSock::{AF_INET, AF_INET6};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};

use super::{Owned, from_wide};
use crate::measure::Connection;

/// Every running process: its id, its parent's id and its program's name.
pub(crate) fn processes() -> Vec<(u32, u32, String)> {
    let mut all = Vec::new();
    // SAFETY: a snapshot owned by `Owned`; each entry is written into a
    // structure whose size is set first, as Windows asks.
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return all };
        let Some(snapshot) = Owned::new(snapshot) else { return all };
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        if Process32FirstW(snapshot.raw(), &mut entry).is_err() {
            return all;
        }
        loop {
            all.push((entry.th32ProcessID, entry.th32ParentProcessID, from_wide(&entry.szExeFile)));
            if Process32NextW(snapshot.raw(), &mut entry).is_err() {
                break;
            }
        }
    }
    all
}

/// Reads one of Windows' TCP tables into a buffer aligned for its rows.
fn table(family: u16) -> Option<Vec<u64>> {
    for _ in 0..4 {
        let mut size: u32 = 0;
        // SAFETY: the first call only asks for the size; the second writes
        // at most that many bytes into a buffer at least that big.
        unsafe {
            let _ = GetExtendedTcpTable(None, &mut size, false, u32::from(family), TCP_TABLE_OWNER_PID_ALL, 0);
            if size == 0 || size > 64 << 20 {
                return None;
            }
            // Room for connections opened between the two calls.
            size += 4096;
            let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
            let code = GetExtendedTcpTable(
                Some(buffer.as_mut_ptr() as *mut c_void),
                &mut size,
                false,
                u32::from(family),
                TCP_TABLE_OWNER_PID_ALL,
                0,
            );
            if code == 0 {
                return Some(buffer);
            }
        }
    }
    None
}

/// The rows of a table read by `table`, as the type `R`.
fn rows<R: Copy>(buffer: &[u64]) -> Vec<R> {
    let bytes = std::mem::size_of_val(buffer);
    // SAFETY: the table starts with its row count, then the rows, aligned
    // as Windows lays them out; no row is read past the buffer's end.
    unsafe {
        let count = *(buffer.as_ptr() as *const u32) as usize;
        let first = (buffer.as_ptr() as *const u8).add(std::mem::align_of::<R>().max(4)) as *const R;
        let room = (bytes.saturating_sub(std::mem::align_of::<R>().max(4))) / std::mem::size_of::<R>();
        std::slice::from_raw_parts(first, count.min(room)).to_vec()
    }
}

/// A port as Windows keeps it in a TCP row: network order, in the low two
/// bytes.
fn port(raw: u32) -> u16 {
    u16::from_be((raw & 0xFFFF) as u16)
}

/// Every TCP connection on the PC whose owning process is in `pids`, with a
/// remote end (listening sockets have none).
pub(crate) fn connections_of(pids: &[u32], names: &dyn Fn(u32) -> String) -> Vec<Connection> {
    let mut found = Vec::new();
    if let Some(buffer) = table(AF_INET.0) {
        for row in rows::<MIB_TCPROW_OWNER_PID>(&buffer) {
            if !pids.contains(&row.dwOwningPid) || row.dwRemoteAddr == 0 {
                continue;
            }
            found.push(Connection {
                pid: row.dwOwningPid,
                process: names(row.dwOwningPid),
                remote: Ipv4Addr::from(u32::from_be(row.dwRemoteAddr)).to_string(),
                remote_port: port(row.dwRemotePort),
                local_port: port(row.dwLocalPort),
                state: row.dwState,
            });
        }
    }
    if let Some(buffer) = table(AF_INET6.0) {
        for row in rows::<MIB_TCP6ROW_OWNER_PID>(&buffer) {
            if !pids.contains(&row.dwOwningPid) || row.ucRemoteAddr == [0; 16] {
                continue;
            }
            found.push(Connection {
                pid: row.dwOwningPid,
                process: names(row.dwOwningPid),
                remote: Ipv6Addr::from(row.ucRemoteAddr).to_string(),
                remote_port: port(row.dwRemotePort),
                local_port: port(row.dwLocalPort),
                state: row.dwState,
            });
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_port_reads_from_network_order() {
        assert_eq!(port(u32::from(443u16.to_be())), 443);
        assert_eq!(port(u32::from(47810u16.to_be())), 47810);
    }

    /// This test process is in the list, with its parent; a loopback
    /// connection it opens to itself shows with the right ports.
    #[test]
    fn this_process_and_its_own_loopback_connection_are_seen() {
        let me = std::process::id();
        let all = processes();
        assert!(all.iter().any(|(pid, _, _)| *pid == me));

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let there = listener.local_addr().unwrap().port();
        let _client = std::net::TcpStream::connect(("127.0.0.1", there)).unwrap();
        let _server = listener.accept().unwrap();
        let seen = connections_of(&[me], &|_| "test".to_string());
        assert!(seen.iter().any(|c| c.remote == "127.0.0.1" && c.remote_port == there && c.pid == me), "{seen:?}");
    }
}
