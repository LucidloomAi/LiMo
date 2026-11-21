//! The first Rust route: adb_client 4.0.0 straight to the phone over USB,
//! through its own USB layer, rusb 0.9.4 and libusb 1.0.27 (compiled into
//! this program).
//!
//! libusb on Windows reaches an interface only through a driver it knows:
//! WinUSB, libusbK or libusb0 on the interface, and on a phone with several
//! interfaces, Windows' own composite driver (`usbccgp`) or Samsung's
//! (`dg_ssudbus`) above it (libusb 1.0.27, `os/windows_winusb.c`). Whether
//! the phone's debugging interface has such a driver is what `drivers` shows
//! and this route proves.

#![forbid(unsafe_code)]

use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::Path;
use std::time::{Duration, Instant};

use adb_client::RustADBError;
use adb_client::usb::{ADBUSBDevice, find_all_connected_adb_devices};

use crate::cable::{Cable, OPEN_LIMIT, OpenProblem, Via, with_timeout};
use crate::route_rust::ClientCable;
use crate::ui::Ui;

/// True when Google's adb server listens on its port on this PC: while it
/// runs it holds the phone's debugging interface, so neither Rust route can.
pub fn adb_server_running() -> bool {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, 5037));
    TcpStream::connect_timeout(&address, Duration::from_millis(300)).is_ok()
}

/// What adb_client's error means while opening.
pub fn open_problem(error: RustADBError) -> OpenProblem {
    match &error {
        RustADBError::DeviceBusy => OpenProblem::Busy,
        RustADBError::UsbError(rusb::Error::NotSupported) => {
            OpenProblem::DriverNotUsable(format!("libusb says: {error}"))
        }
        RustADBError::UsbError(rusb::Error::Access) => OpenProblem::Other(format!(
            "Windows refused this program access to the phone's debugging interface ({error})"
        )),
        RustADBError::UsbError(rusb::Error::NoDevice | rusb::Error::NotFound) | RustADBError::DeviceNotFound(_) => {
            OpenProblem::NoPhone
        }
        RustADBError::UsbError(rusb::Error::Timeout) => OpenProblem::NotAllowed(
            "the phone did not answer within adb_client's 10 seconds after it was shown this program's key".to_string(),
        ),
        RustADBError::IOError(e) if e.kind() == std::io::ErrorKind::TimedOut => OpenProblem::NotAllowed(
            "the phone did not answer within adb_client's 10 seconds after it was shown this program's key".to_string(),
        ),
        RustADBError::USBNoDescriptorFound => {
            OpenProblem::Other("the device has no debugging interface with two bulk endpoints".to_string())
        }
        _ => OpenProblem::Other(error.to_string()),
    }
}

/// The phones libusb sees with a debugging interface: vendor id, product id
/// and the name the device gives itself. Reads descriptors only.
pub fn phones() -> Result<Vec<(u16, u16, String)>, String> {
    match with_timeout(Duration::from_secs(15), find_all_connected_adb_devices) {
        Err(limit) => Err(format!("libusb's list did not come within {} seconds", limit.as_secs())),
        Ok(Err(e)) => Err(format!("libusb's list could not be read: {e}")),
        Ok(Ok(found)) => Ok(found.into_iter().map(|d| (d.vendor_id, d.product_id, d.device_description)).collect()),
    }
}

/// Opens the one phone through libusb with the key at `key`.
pub fn open(key: &Path, ui: &mut dyn Ui) -> Result<Box<dyn Cable>, OpenProblem> {
    if adb_server_running() {
        return Err(OpenProblem::AdbServerRunning);
    }
    ui.say("> list the USB devices with a debugging interface (libusb, descriptors only)");
    let found = phones().map_err(OpenProblem::Other)?;
    let (vendor, product, name) = match found.as_slice() {
        [] => return Err(OpenProblem::NoPhone),
        [one] => one.clone(),
        more => return Err(OpenProblem::TwoPhones(more.len())),
    };
    ui.say(&format!(
        "> open the debugging interface of \"{name}\" ({vendor:04x}:{product:04x}), claim it, and connect with this program's key"
    ));
    let key = key.to_path_buf();
    let started = Instant::now();
    let device = match with_timeout(OPEN_LIMIT, move || ADBUSBDevice::new_with_custom_private_key(vendor, product, key)) {
        Err(limit) => return Err(OpenProblem::Timeout(limit)),
        Ok(Err(e)) => return Err(open_problem(e)),
        Ok(Ok(device)) => device,
    };
    ui.say(&format!("Connected through libusb in {:.1} seconds.", started.elapsed().as_secs_f64()));
    Ok(Box::new(ClientCable::new(Via::Libusb, device, None)))
}
