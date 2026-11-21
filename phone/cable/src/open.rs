//! The PC's side of the cable: what is plugged in ([`Cable::look`]), and
//! opening the phone ([`Cable::open`]).
//!
//! The real one is [`WindowsCable`]: it reads Windows' list of USB devices
//! (read only, `win/usb.rs`) and SetupAPI's list of the interfaces that
//! offer the Android USB interface class (`win/winusb.rs`), and opens the
//! one phone through WinUSB, as Google's adb finds and opens a phone on
//! Windows. The tests' is the fake phone's cable (`fake.rs`, feature
//! `fake`).

#![forbid(unsafe_code)]

use crate::key::AdbKey;
use crate::phone::{Knock, OpenProblem, Phone, Waits};
use crate::usb::UsbDevice;

/// An interface that offers the Android USB interface class, as Windows
/// names it. Never its path, which holds the phone's serial number.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Interface {
    /// Its name ("SAMSUNG Android ADB Interface").
    pub name: Option<String>,
    /// Its driver's service: `WINUSB` (in any case) is Windows' own WinUSB,
    /// which LiMo opens.
    pub driver: Option<String>,
    /// The driver of the device above it: `usbccgp` is Windows' own
    /// composite driver, `dg_ssudbus` Samsung's. LiMo opens the interface
    /// whatever is above it.
    pub above: Option<String>,
}

impl Interface {
    /// True when its driver is WinUSB, which LiMo can open.
    pub fn on_winusb(&self) -> bool {
        self.driver.as_deref().is_some_and(|d| d.trim().eq_ignore_ascii_case("winusb"))
    }
}

/// What is plugged in, as Windows sees it now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Look {
    /// The devices that look like a phone (`usb.rs`), or why the list could
    /// not be read.
    pub windows: Result<Vec<UsbDevice>, String>,
    /// The interfaces that offer the Android USB interface class, or why
    /// that list could not be read.
    pub interfaces: Result<Vec<Interface>, String>,
}

/// The PC's side of the cable.
pub trait Cable {
    /// What is plugged in now. Reads only; opens nothing.
    fn look(&mut self) -> Look;
    /// Opens the one phone LiMo can open, and connects with `key`, quietly
    /// or asking as `knock` says (`Phone::connect`).
    fn open(&mut self, key: &AdbKey, waits: Waits, knock: Knock<'_>) -> Result<Phone, OpenProblem>;
}

/// True when Google's adb server listens on its port on this PC: while it
/// runs it holds the phone's debugging interface, so LiMo cannot open it.
/// LiMo never stops it: it is someone else's (Android Studio's, often).
pub fn adb_server_running() -> bool {
    use std::net::{Ipv4Addr, SocketAddr, TcpStream};
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, 5037));
    TcpStream::connect_timeout(&address, std::time::Duration::from_millis(300)).is_ok()
}

/// The real cable, on Windows.
#[cfg(windows)]
#[derive(Debug, Default)]
pub struct WindowsCable;

#[cfg(windows)]
impl Cable for WindowsCable {
    fn look(&mut self) -> Look {
        use crate::usb::UsbLook;
        Look {
            windows: crate::win::usb::WindowsUsb.phones_seen(),
            interfaces: crate::win::winusb::interfaces().map(|found| found.into_iter().map(|f| f.interface).collect()),
        }
    }

    fn open(&mut self, key: &AdbKey, waits: Waits, knock: Knock<'_>) -> Result<Phone, OpenProblem> {
        use crate::win::winusb::{self, OpenError};
        if adb_server_running() {
            return Err(OpenProblem::AdbServerRunning);
        }
        let mut found = winusb::interfaces().map_err(OpenProblem::Other)?;
        let usable = found.iter().filter(|f| f.interface.on_winusb()).count();
        let one = match (found.len(), usable) {
            (0, _) => return Err(OpenProblem::NoPhone),
            (_, 0) => {
                let driver = found[0].interface.driver.clone().unwrap_or_else(|| "none".to_string());
                return Err(OpenProblem::DriverNotUsable(format!("its driver is {driver}, not WinUSB")));
            }
            (_, 1) => found.remove(found.iter().position(|f| f.interface.on_winusb()).unwrap_or(0)),
            (_, n) => return Err(OpenProblem::TwoPhones(n)),
        };
        let wire = winusb::open(&one).map_err(|e| match e {
            OpenError::Busy => OpenProblem::Busy,
            OpenError::Gone => OpenProblem::NoPhone,
            OpenError::NotWinUsb(why) => OpenProblem::DriverNotUsable(why),
            OpenError::Other(why) => OpenProblem::Other(why),
        })?;
        Phone::connect(Box::new(wire), key, waits, knock)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn winusb_is_known_in_any_case() {
        let on = |driver: &str| Interface { driver: Some(driver.to_string()), ..Interface::default() }.on_winusb();
        assert!(on("WINUSB") && on("WinUSB") && on("winusb"));
        assert!(!on("ssudadb") && !Interface::default().on_winusb());
    }
}
