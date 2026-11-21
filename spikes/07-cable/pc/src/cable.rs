//! The cable, whichever way it is reached: the three routes this proof
//! compares, the few things each must do, and how each failure is said.
//!
//! The steps (`steps.rs`) only ever talk to a [`Cable`], which a
//! [`Connector`] opens; the real ones are in `route_libusb.rs`,
//! `route_nusb.rs` and `route_adb.rs`, the tests' fake in `tests/`.

#![forbid(unsafe_code)]

use std::fmt;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use crate::phone::Ask;
use crate::ui::Ui;

/// How long an ordinary command may take.
pub const SHELL_LIMIT: Duration = Duration::from_secs(30);
/// How long an install may take.
pub const INSTALL_LIMIT: Duration = Duration::from_secs(180);
/// How long opening the phone may take, the prompt included: adb_client
/// itself waits only 10 seconds once it has shown the phone its key.
pub const OPEN_LIMIT: Duration = Duration::from_secs(40);

/// The three routes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
    /// adb_client 4.0.0 in this program, over USB through libusb (rusb).
    Libusb,
    /// adb_client 4.0.0 in this program, over USB through nusb.
    Nusb,
    /// Google's own adb.exe, run as a separate program.
    Adb,
}

impl Via {
    /// The word on the command line.
    pub fn word(&self) -> &'static str {
        match self {
            Via::Libusb => "libusb",
            Via::Nusb => "nusb",
            Via::Adb => "adb",
        }
    }

    /// The route, in words.
    pub fn words(&self) -> &'static str {
        match self {
            Via::Libusb => "adb_client 4.0.0 in this program, over USB through libusb (no adb.exe, no adb server)",
            Via::Nusb => "adb_client 4.0.0 in this program, over USB through nusb (no adb.exe, no adb server, no C)",
            Via::Adb => "Google's own adb.exe (its server is stopped again at the end of the step)",
        }
    }

    /// From the command line's word.
    pub fn from_word(word: &str) -> Option<Via> {
        match word {
            "libusb" => Some(Via::Libusb),
            "nusb" => Some(Via::Nusb),
            "adb" => Some(Via::Adb),
            _ => None,
        }
    }
}

/// A text answer from the phone.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Answer {
    /// Everything the command printed, as it came.
    pub text: String,
    /// Its exit code, when the route can know it: Google's adb can (its
    /// shell protocol carries it), adb_client 4.0.0 cannot (it speaks the
    /// first shell protocol, which carries none).
    pub exit: Option<i32>,
}

/// Why a call to the phone did not give an answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CableError {
    /// It did not end in time.
    Timeout(Duration),
    /// The connection to the phone is gone (unplugged, or debugging off).
    Lost(String),
    /// It failed for another reason, in the route's own words.
    Failed(String),
}

impl fmt::Display for CableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CableError::Timeout(limit) => write!(f, "it did not end within {} seconds and was given up", limit.as_secs()),
            CableError::Lost(why) => write!(f, "the connection to the phone is gone ({why})"),
            CableError::Failed(why) => write!(f, "it failed: {why}"),
        }
    }
}

/// An open way to the phone.
pub trait Cable {
    /// Which route it is.
    fn via(&self) -> Via;
    /// Runs one command on the phone.
    fn shell(&self, ask: &Ask) -> Result<Answer, CableError>;
    /// Runs one command on the phone with `input` on its standard input,
    /// which then ends.
    fn shell_with_input(&self, ask: &Ask, input: &[u8]) -> Result<Answer, CableError>;
    /// Installs the APK at `apk`, replacing the app if it is there; Android's
    /// own answer (`Success`, or its failure's words).
    fn install(&self, apk: &Path) -> Result<String, CableError>;
    /// Removes the test app, and only it; Android's own answer.
    fn uninstall(&self) -> Result<String, CableError>;
    /// Carries connections to `port` on the phone's own loopback to `port`
    /// on this PC's.
    fn reverse(&self, port: u16) -> Result<(), CableError>;
    /// Removes that again.
    fn reverse_remove(&self, port: u16) -> Result<(), CableError>;
    /// Lets go of the phone; what it did, in lines (Google's adb: its server
    /// stopped).
    fn close(self: Box<Self>) -> Vec<String>;
}

/// Why the phone could not be opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenProblem {
    /// Nothing on this PC's USB ports looks like a phone with USB debugging on.
    NoPhone,
    /// More than one does.
    TwoPhones(usize),
    /// Google's adb server runs on this PC, and holds the phone.
    AdbServerRunning,
    /// Another program holds the phone's debugging interface.
    Busy,
    /// This route cannot use the driver Windows bound to the debugging
    /// interface; the route's own words.
    DriverNotUsable(String),
    /// The phone has not allowed this PC (yet).
    NotAllowed(String),
    /// Google's adb.exe was not found.
    AdbMissing,
    /// Opening did not end in time.
    Timeout(Duration),
    /// Anything else, in the route's own words.
    Other(String),
}

impl fmt::Display for OpenProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpenProblem::NoPhone => f.write_str(
                "No phone with USB debugging on is on this PC's USB ports, as this route sees them. Unlock the phone, \
                 check that USB debugging is on (Settings, Developer options), plug it in again, and run `drivers` \
                 to see what Windows sees.",
            ),
            OpenProblem::TwoPhones(n) => {
                write!(f, "{n} phones with USB debugging on are attached. Unplug all but the test phone, so that this cannot pick the wrong one.")
            }
            OpenProblem::AdbServerRunning => f.write_str(
                "Google's adb server is running on this PC (it listens on 127.0.0.1:5037), and while it runs it holds \
                 the phone's debugging interface. Stop it with `adb kill-server`, and close Android Studio, then run \
                 this again.",
            ),
            OpenProblem::Busy => f.write_str(
                "Another program holds the phone's debugging interface: most often Google's adb server or Android \
                 Studio. Stop it (`adb kill-server`), then run this again.",
            ),
            OpenProblem::DriverNotUsable(why) => write!(
                f,
                "This route cannot use the driver Windows bound to the phone's debugging interface ({why}). Run \
                 `drivers` to see which driver that is, and write it down: that is a result."
            ),
            OpenProblem::NotAllowed(why) => write!(
                f,
                "The phone has not allowed this PC ({why}). Unlock it; if it shows \"Allow USB debugging?\", tick \
                 \"Always allow from this computer\" and tap Allow; then run this again."
            ),
            OpenProblem::AdbMissing => f.write_str(
                "Google's adb.exe was not found: not given with --adb, not on the PATH, nor in platform-tools under \
                 ANDROID_HOME, ANDROID_SDK_ROOT or %LOCALAPPDATA%\\Android\\Sdk.",
            ),
            OpenProblem::Timeout(limit) => write!(
                f,
                "Opening the phone did not end within {} seconds. Unplug it, plug it in again unlocked, and run this \
                 again.",
                limit.as_secs()
            ),
            OpenProblem::Other(why) => write!(f, "The phone could not be opened: {why}."),
        }
    }
}

/// Opens the phone by a route, and lists what a route sees.
pub trait Connector {
    /// Opens the phone by `via`, saying on `ui` each thing it does.
    fn open(&self, via: Via, ui: &mut dyn Ui) -> Result<Box<dyn Cable>, OpenProblem>;
    /// The phones with USB debugging on that `via` sees now, one line each,
    /// never with a serial number in full. Opens nothing for use.
    fn seen(&self, via: Via) -> Result<Vec<String>, String>;
}

/// Runs `work` on its own thread and waits at most `limit` for it. A call
/// that does not end is given up and its thread left behind; the program
/// ends soon after, and the system then lets go of the device.
pub fn with_timeout<T: Send + 'static>(limit: Duration, work: impl FnOnce() -> T + Send + 'static) -> Result<T, Duration> {
    let (tx, rx) = mpsc::channel();
    let spawned = std::thread::Builder::new().name("limo-cable-call".to_string()).spawn(move || {
        let _ = tx.send(work());
    });
    if spawned.is_err() {
        return Err(limit);
    }
    rx.recv_timeout(limit).map_err(|_| limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_that_does_not_end_is_given_up() {
        assert_eq!(with_timeout(Duration::from_secs(5), || 7), Ok(7));
        let limit = Duration::from_millis(100);
        assert_eq!(with_timeout(limit, move || std::thread::sleep(Duration::from_secs(3))), Err(limit));
    }

    #[test]
    fn each_route_has_its_word() {
        for via in [Via::Libusb, Via::Nusb, Via::Adb] {
            assert_eq!(Via::from_word(via.word()), Some(via));
        }
        assert_eq!(Via::from_word("usb"), None);
    }
}
