//! The second Rust route: adb_client 4.0.0 over USB through nusb 0.2.9, a
//! USB layer in Rust alone, with no libusb.
//!
//! **Why a relay.** adb_client keeps its transport trait
//! (`ADBMessageTransport`) and its generic device (`ADBMessageDevice`) in
//! private modules: neither is exported in 4.0.0, so no other crate can
//! give it a USB layer of its own. What it does export is a device that
//! speaks the same adb messages over TCP (`ADBTcpDevice`). So this route
//! opens the phone's debugging interface with nusb and carries the bytes
//! between it and a socket on this PC's own loopback, which adb_client's TCP
//! device connects to: one listener on 127.0.0.1 at a port the system picks,
//! one connection taken, the listener closed at once. The messages are the
//! same ones adb_client sends over libusb, framed as adb frames them on USB:
//! each message's 24 byte header in one transfer, its data in the next,
//! and a zero length packet after data that fills its last packet exactly;
//! and they are read from the phone as adb's own host side reads them
//! (`protocol::next_message`, shared with the WinUSB route).
//!
//! This is the proof's way to ask whether nusb reaches the phone; LiMo itself
//! would not open a socket for it (it would give adb_client a nusb transport
//! of its own, which needs adb_client changed, by a fork or upstream).
//!
//! nusb on Windows reaches an interface only through WinUSB: the interface
//! bound to WinUSB under Windows' own composite driver (`usbccgp`), or the
//! whole device bound to WinUSB (nusb 0.2.9,
//! `platform/windows_winusb/enumeration.rs`).

#![forbid(unsafe_code)]

use std::io::{ErrorKind, Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use adb_client::tcp::ADBTcpDevice;
use nusb::MaybeFuture;
use nusb::descriptors::TransferType;
use nusb::transfer::{Buffer, Bulk, Direction, In, Out, TransferError};

use crate::cable::{Cable, OPEN_LIMIT, OpenProblem, Via, with_timeout};
use crate::protocol::next_message;
use crate::route_libusb::{adb_server_running, open_problem};
use crate::route_rust::ClientCable;
use crate::ui::Ui;

/// The debugging interface: vendor class 0xFF, subclass 0x42, protocol 1.
const ADB_CLASS: (u8, u8, u8) = (0xff, 0x42, 0x01);
/// The largest message adb_client sends or takes: a 24 byte header and 1 MiB.
const MAX_DATA: usize = 1024 * 1024;

/// A phone nusb sees with a debugging interface. Its `Debug` leaves out
/// nusb's own device record, which holds the serial number.
pub struct NusbPhone {
    /// The vendor id.
    pub vendor: u16,
    /// The product id.
    pub product: u16,
    /// The name the device gives itself, if any.
    pub name: String,
    /// The driver Windows bound to the whole device (its service), if any:
    /// `usbccgp` is Windows' own composite driver.
    pub driver: Option<String>,
    /// The debugging interface's number.
    pub interface: u8,
    info: nusb::DeviceInfo,
}

impl std::fmt::Debug for NusbPhone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NusbPhone({:04x}:{:04x} {:?} driver {:?} interface {})", self.vendor, self.product, self.name, self.driver, self.interface)
    }
}

impl NusbPhone {
    /// Its serial number, for hiding it in the log of `--verbose` only; never
    /// printed, never kept.
    pub fn serial_for_hiding(&self) -> Option<String> {
        self.info.serial_number().map(str::to_string)
    }
}

/// The phones nusb sees with a debugging interface. Reads Windows' own
/// lists and the descriptors only: nothing is opened.
pub fn phones() -> Result<Vec<NusbPhone>, String> {
    let devices = nusb::list_devices().wait().map_err(|e| format!("nusb's list could not be read: {e}"))?;
    Ok(devices
        .filter_map(|info| {
            let interface = info
                .interfaces()
                .find(|i| (i.class(), i.subclass(), i.protocol()) == ADB_CLASS)?
                .interface_number();
            Some(NusbPhone {
                vendor: info.vendor_id(),
                product: info.product_id(),
                name: format!(
                    "{} {}",
                    info.manufacturer_string().unwrap_or("Unknown"),
                    info.product_string().unwrap_or("Unknown")
                ),
                driver: info.driver().map(str::to_string),
                interface,
                info,
            })
        })
        .collect())
}

/// nusb's error while opening, as this program says it.
fn nusb_problem(what: &str, error: nusb::Error) -> OpenProblem {
    match error.kind() {
        nusb::ErrorKind::Busy => OpenProblem::Busy,
        nusb::ErrorKind::Unsupported => OpenProblem::DriverNotUsable(format!("nusb, {what}: {error}")),
        nusb::ErrorKind::NotFound | nusb::ErrorKind::Disconnected => OpenProblem::NoPhone,
        nusb::ErrorKind::PermissionDenied => {
            OpenProblem::Other(format!("Windows refused this program access to the device ({what}: {error})"))
        }
        _ => OpenProblem::Other(format!("nusb, {what}: {error}")),
    }
}

/// The relay between the phone's debugging interface and the loopback
/// socket. Stops when the socket closes; dropped, it stops and lets go of
/// the interface.
pub struct Relay {
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

impl Drop for Relay {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let deadline = Instant::now() + Duration::from_secs(3);
        for thread in self.threads.drain(..) {
            while !thread.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            if thread.is_finished() {
                let _ = thread.join();
            }
        }
    }
}

/// Reads one adb message from the socket: its header and its data.
fn read_message(tcp: &mut TcpStream) -> std::io::Result<([u8; 24], Vec<u8>)> {
    let mut header = [0u8; 24];
    tcp.read_exact(&mut header)?;
    let length = u32::from_le_bytes([header[12], header[13], header[14], header[15]]) as usize;
    if length > MAX_DATA {
        return Err(std::io::Error::new(ErrorKind::InvalidData, "a message larger than adb allows"));
    }
    let mut data = vec![0u8; length];
    tcp.read_exact(&mut data)?;
    Ok((header, data))
}

impl Relay {
    /// Starts relaying between the interface's two bulk endpoints and one
    /// connection to a new listener on 127.0.0.1; its address.
    fn start(interface: nusb::Interface) -> Result<(Relay, SocketAddr), OpenProblem> {
        let descriptor = interface
            .descriptor()
            .ok_or_else(|| OpenProblem::Other("the debugging interface has no descriptor".to_string()))?;
        let bulk = |direction: Direction| {
            descriptor
                .endpoints()
                .find(|e| e.transfer_type() == TransferType::Bulk && e.direction() == direction)
                .map(|e| e.address())
        };
        let (Some(in_address), Some(out_address)) = (bulk(Direction::In), bulk(Direction::Out)) else {
            return Err(OpenProblem::Other("the debugging interface lacks its two bulk endpoints".to_string()));
        };
        let to_phone = interface
            .endpoint::<Bulk, Out>(out_address)
            .map_err(|e| nusb_problem("opening the endpoint to the phone", e))?;
        let from_phone = interface
            .endpoint::<Bulk, In>(in_address)
            .map_err(|e| nusb_problem("opening the endpoint from the phone", e))?;
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .map_err(|e| OpenProblem::Other(format!("the relay's socket could not be opened: {e}")))?;
        let address = listener
            .local_addr()
            .map_err(|e| OpenProblem::Other(format!("the relay's socket has no address: {e}")))?;
        let stop = Arc::new(AtomicBool::new(false));
        let stop_here = stop.clone();
        let relay_thread = std::thread::Builder::new()
            .name("limo-nusb-relay".to_string())
            .spawn(move || {
                let _keep = interface;
                let Some(tcp) = accept_one(listener, &stop_here) else { return };
                let Ok(mut up_tcp) = tcp.try_clone() else { return };
                let stop_up = stop_here.clone();
                let up = std::thread::spawn(move || {
                    let mut writer = to_phone.writer(MAX_DATA).with_write_timeout(Duration::from_secs(10));
                    while !stop_up.load(Ordering::SeqCst) {
                        let Ok((header, data)) = read_message(&mut up_tcp) else { break };
                        let sent = writer
                            .write_all(&header)
                            .and_then(|()| writer.flush_end())
                            .and_then(|()| if data.is_empty() { Ok(()) } else { writer.write_all(&data).and_then(|()| writer.flush_end()) });
                        if sent.is_err() {
                            break;
                        }
                    }
                    let _ = up_tcp.shutdown(Shutdown::Both);
                    stop_up.store(true, Ordering::SeqCst);
                });
                let mut down_tcp = tcp;
                let mut from_phone = from_phone;
                let packet = from_phone.max_packet_size();
                let mut read = |want: usize, wait: Duration| -> Result<Option<Vec<u8>>, String> {
                    let done = from_phone.transfer_blocking(Buffer::new(want), wait);
                    let mut bytes = done.buffer.into_vec();
                    bytes.truncate(done.actual_len);
                    match done.status {
                        Ok(()) => Ok(Some(bytes)),
                        Err(TransferError::Cancelled) if bytes.is_empty() => Ok(None),
                        Err(TransferError::Cancelled) => Ok(Some(bytes)),
                        Err(e) => Err(e.to_string()),
                    }
                };
                while !stop_here.load(Ordering::SeqCst) {
                    match next_message(&mut read, packet) {
                        Ok(None) => continue,
                        Ok(Some(message)) => {
                            if down_tcp.write_all(&message).is_err() {
                                break;
                            }
                        }
                        Err(why) => {
                            log::debug!("relay from the phone stopped: {why}");
                            break;
                        }
                    }
                }
                stop_here.store(true, Ordering::SeqCst);
                let _ = down_tcp.shutdown(Shutdown::Both);
                let _ = up.join();
            })
            .map_err(|e| OpenProblem::Other(format!("the relay could not start: {e}")))?;
        Ok((Relay { stop, threads: vec![relay_thread] }, address))
    }
}

/// Takes one connection within 15 seconds, then closes the listener.
fn accept_one(listener: TcpListener, stop: &AtomicBool) -> Option<TcpStream> {
    listener.set_nonblocking(true).ok()?;
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline && !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, peer)) if peer.ip().is_loopback() => {
                stream.set_nonblocking(false).ok()?;
                let _ = stream.set_nodelay(true);
                return Some(stream);
            }
            Ok(_) => continue,
            Err(e) if e.kind() == ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => return None,
        }
    }
    None
}

/// Opens the one phone through nusb with the key at `key`.
pub fn open(key: &Path, ui: &mut dyn Ui) -> Result<Box<dyn Cable>, OpenProblem> {
    if adb_server_running() {
        return Err(OpenProblem::AdbServerRunning);
    }
    ui.say("> list the USB devices with a debugging interface (nusb, Windows' lists and descriptors only)");
    let mut found = phones().map_err(OpenProblem::Other)?;
    let phone = match found.len() {
        0 => return Err(OpenProblem::NoPhone),
        1 => found.remove(0),
        n => return Err(OpenProblem::TwoPhones(n)),
    };
    ui.say(&format!(
        "> open \"{}\" ({:04x}:{:04x}) through nusb (the driver Windows bound to the whole device: {}), and claim interface {}",
        phone.name,
        phone.vendor,
        phone.product,
        phone.driver.as_deref().unwrap_or("none"),
        phone.interface
    ));
    let device = phone.info.open().wait().map_err(|e| nusb_problem("opening the device", e))?;
    let interface = device
        .claim_interface(phone.interface)
        .wait()
        .map_err(|e| nusb_problem("claiming the debugging interface", e))?;
    let (relay, address) = Relay::start(interface)?;
    ui.say("> relay its bytes to adb_client through one socket on 127.0.0.1, and connect with this program's key");
    let key = key.to_path_buf();
    let started = Instant::now();
    let device = match with_timeout(OPEN_LIMIT, move || ADBTcpDevice::new_with_custom_private_key(address, key)) {
        Err(limit) => return Err(OpenProblem::Timeout(limit)),
        Ok(Err(e)) => return Err(open_problem(e)),
        Ok(Ok(device)) => device,
    };
    ui.say(&format!("Connected through nusb in {:.1} seconds.", started.elapsed().as_secs_f64()));
    Ok(Box::new(ClientCable::new(Via::Nusb, device, Some(Box::new(relay)))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_is_read_as_its_header_and_its_data() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let sender = std::thread::spawn(move || {
            let mut out = TcpStream::connect(address).unwrap();
            let mut header = [0u8; 24];
            header[12..16].copy_from_slice(&5u32.to_le_bytes());
            out.write_all(&header).unwrap();
            out.write_all(b"hello").unwrap();
            let mut too_big = [0u8; 24];
            too_big[12..16].copy_from_slice(&((MAX_DATA + 1) as u32).to_le_bytes());
            out.write_all(&too_big).unwrap();
        });
        let (mut tcp, _) = listener.accept().unwrap();
        let (header, data) = read_message(&mut tcp).unwrap();
        assert_eq!(&header[12..16], &5u32.to_le_bytes());
        assert_eq!(data, b"hello");
        assert_eq!(read_message(&mut tcp).unwrap_err().kind(), ErrorKind::InvalidData);
        sender.join().unwrap();
    }
}
