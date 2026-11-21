//! The real cable, counting what it sends: limo-cable's `WindowsCable`,
//! opened the same way (Windows' own WinUSB, the one phone on it), with the
//! wire wrapped in a [`super::CountingWire`] so that the install's progress
//! is what really went down the cable. limo-cable gives no progress of its
//! own; were it to, this copy of its opening would go.

#![forbid(unsafe_code)]

use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use limo_cable::key::AdbKey;
use limo_cable::open::{Cable, Look, WindowsCable, adb_server_running};
use limo_cable::phone::{Knock, OpenProblem, Phone, Waits};
use limo_cable::win::winusb::{self, OpenError};

use super::{ConsoleCable, CountingWire};

/// The PC's USB ports and the phone on them, as the product reaches it.
#[derive(Debug, Default)]
pub struct CountedCable {
    sent: Arc<AtomicU64>,
}

impl ConsoleCable for CountedCable {
    fn look(&mut self) -> Look {
        WindowsCable.look()
    }

    /// As `WindowsCable::open`: Google's adb server holds the phone while it
    /// runs, and is never stopped; the one interface on WinUSB is opened.
    fn open(&mut self, key: &AdbKey, waits: Waits, knock: Knock<'_>) -> Result<Phone, OpenProblem> {
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
        Phone::connect(Box::new(CountingWire::new(wire, Arc::clone(&self.sent))), key, waits, knock)
    }

    fn sent(&self) -> Option<Arc<AtomicU64>> {
        Some(Arc::clone(&self.sent))
    }
}
