//! The third Rust route: this program's own adb (`protocol.rs`) over
//! Windows' own WinUSB (`win/winusb.rs`), the phone found and opened the way
//! Google's adb finds and opens it on Windows: by the Android USB interface
//! class, through SetupAPI, then `WinUsb_Initialize` on its path. No libusb,
//! no nusb, no adb.exe, no adb server.
//!
//! On the test phone, with Samsung's driver package binding WinUSB to the
//! debugging interface under Samsung's own composite driver, libusb and nusb
//! both refused the interface, and Google's adb reached it this way.
//!
//! Every call has a time limit of its own, inside the protocol, so no call
//! needs a thread to bound it.

#![forbid(unsafe_code)]

use std::path::Path;
use std::time::{Duration, Instant};

use crate::cable::{Answer, Cable, CableError, INSTALL_LIMIT, OpenProblem, PROMPT_WAIT, SHELL_LIMIT, Via};
use crate::key::{GOOGLE_SHOWN_AS, Key, KeySource};
use rsa::RsaPrivateKey;
use crate::phone::{Ask, PACKAGE};
use crate::protocol::{AdbError, ConnectError, Connection, Wire, public_key_message};
use crate::ui::Ui;

/// The protocol's error, as the steps say it.
pub fn cable_error(error: AdbError) -> CableError {
    match error {
        AdbError::Lost(why) => CableError::Lost(why),
        AdbError::Timeout(limit) => CableError::Timeout(limit),
        other => CableError::Failed(other.to_string()),
    }
}

/// What a failed connection means while opening.
pub fn open_problem(error: ConnectError) -> OpenProblem {
    match error {
        ConnectError::NotAllowed(limit) => OpenProblem::NotAllowed(format!(
            "the phone was shown this program's key and did not answer within {} seconds",
            limit.as_secs()
        )),
        ConnectError::NoAnswer(limit) => OpenProblem::Other(format!(
            "the phone's debugging interface opened, but adb on the phone did not answer within {} seconds",
            limit.as_secs()
        )),
        ConnectError::WantsTls => OpenProblem::Other("the phone asked for TLS, which only wireless debugging uses".to_string()),
        ConnectError::Lost(why) => OpenProblem::Other(format!("the link to the phone failed while connecting: {why}")),
        ConnectError::Protocol(why) => OpenProblem::Other(why),
    }
}

/// The command that removes the test app, and only it.
pub fn uninstall_command() -> String {
    format!("cmd package uninstall {PACKAGE}")
}

/// Connects over an open wire with `key`, whose source says how it is
/// named: the part of opening that the tests run against a fake phone.
pub fn connect_over(
    wire: Box<dyn Wire>,
    key: &RsaPrivateKey,
    source: KeySource,
    prompt_wait: Duration,
    ui: &mut dyn Ui,
) -> Result<Box<dyn Cable>, OpenProblem> {
    let public = public_key_message(&crate::key::public_key_base64(key), crate::key::KEY_NAME);
    let named = match source {
        KeySource::Own => "this program's key",
        KeySource::GoogleAdb => GOOGLE_SHOWN_AS,
    };
    ui.say(&format!("> connect (CNXN), and answer the phone's question with {named} (AUTH)"));
    let started = Instant::now();
    let mut prompted = false;
    let mut on_prompt = || {
        prompted = true;
        ui.say(&format!(
            "  The phone did not know {named}, so it has been shown it: answer \"Allow USB debugging?\" on the phone now (this program waits {} seconds).",
            prompt_wait.as_secs()
        ))
    };
    let connection = Connection::connect(wire, key, &public, prompt_wait, &mut on_prompt).map_err(open_problem)?;
    if !prompted {
        ui.say(match source {
            KeySource::Own => "  The phone asked nothing: it allowed this program's key before (Always allow).",
            KeySource::GoogleAdb => {
                "  The question on the phone was skipped: the adb key of this Windows account was allowed on this phone before."
            }
        });
    }
    ui.say(&format!(
        "Connected through WinUSB in {:.1} seconds; the phone {} the second shell protocol, and takes {} KiB a message.",
        started.elapsed().as_secs_f64(),
        if connection.shell_v2 { "speaks" } else { "does not speak" },
        connection.max_data() / 1024
    ));
    Ok(Box::new(WinusbCable { connection: Some(connection) }))
}

/// Opens the one phone through WinUSB with `key`.
#[cfg(windows)]
pub fn open(key: &Key, ui: &mut dyn Ui) -> Result<Box<dyn Cable>, OpenProblem> {
    use crate::win::winusb::{self, OpenError};
    let private = key.load().map_err(OpenProblem::Other)?;
    if crate::route_libusb::adb_server_running() {
        return Err(OpenProblem::AdbServerRunning);
    }
    ui.say("> list the interfaces present that offer the Android USB interface class (SetupAPI, as Google's adb looks)");
    let mut found = winusb::interfaces().map_err(OpenProblem::Other)?;
    let interface = match found.len() {
        0 => return Err(OpenProblem::NoPhone),
        1 => found.remove(0),
        n => return Err(OpenProblem::TwoPhones(n)),
    };
    ui.say(&format!(
        "> open \"{}\" (its driver: {}), hand it to WinUSB (WinUsb_Initialize), and find its two bulk pipes",
        interface.name.as_deref().unwrap_or("no name"),
        interface.service.as_deref().unwrap_or("none")
    ));
    let wire = winusb::open(&interface).map_err(|e| match e {
        OpenError::Busy => OpenProblem::Busy,
        OpenError::Gone => OpenProblem::NoPhone,
        OpenError::NotWinUsb(why) => OpenProblem::DriverNotUsable(why),
        OpenError::Other(why) => OpenProblem::Other(why),
    })?;
    ui.say(&format!(
        "  WinUSB has it: bulk pipe 0x{:02x} from the phone, 0x{:02x} to it, packets of {} bytes.",
        wire.pipe_in,
        wire.pipe_out,
        wire.packet()
    ));
    connect_over(Box::new(wire), &private, key.source, PROMPT_WAIT, ui)
}

/// Opens the one phone through WinUSB: on Windows only.
#[cfg(not(windows))]
pub fn open(_key: &Key, _ui: &mut dyn Ui) -> Result<Box<dyn Cable>, OpenProblem> {
    Err(OpenProblem::Other("the WinUSB route is for Windows only".to_string()))
}

/// The interfaces this route would open, one line each, never a path.
#[cfg(windows)]
pub fn seen() -> Result<Vec<String>, String> {
    Ok(crate::win::winusb::interfaces()?
        .into_iter()
        .map(|f| {
            format!(
                "\"{}\", offering the Android USB interface class, its driver {}",
                f.name.as_deref().unwrap_or("no name"),
                f.service.as_deref().unwrap_or("none")
            )
        })
        .collect())
}

/// The interfaces this route would open: on Windows only.
#[cfg(not(windows))]
pub fn seen() -> Result<Vec<String>, String> {
    Err("the WinUSB route is for Windows only".to_string())
}

/// The phone through WinUSB, held for one step.
pub struct WinusbCable {
    connection: Option<Connection>,
}

impl WinusbCable {
    fn connection(&self) -> Result<&Connection, CableError> {
        self.connection.as_ref().ok_or_else(|| CableError::Lost("closed".to_string()))
    }

    fn shell_answer(&self, ask: &Ask, input: Option<&[u8]>) -> Result<Answer, CableError> {
        let output = self.connection()?.shell(&ask.command(), input, SHELL_LIMIT).map_err(cable_error)?;
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        Ok(Answer { text, exit: output.exit })
    }
}

/// Android's answer to an install or a removal: `Success`, or the line that
/// holds the failure, or all of it.
fn android_words(text: &str) -> String {
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    if lines.contains(&"Success") {
        return "Success".to_string();
    }
    lines.iter().find(|l| l.contains("Failure")).map(|l| l.to_string()).unwrap_or_else(|| lines.join(" "))
}

impl Cable for WinusbCable {
    fn via(&self) -> Via {
        Via::Winusb
    }

    fn shell(&self, ask: &Ask) -> Result<Answer, CableError> {
        self.shell_answer(ask, None)
    }

    fn shell_with_input(&self, ask: &Ask, input: &[u8]) -> Result<Answer, CableError> {
        self.shell_answer(ask, Some(input))
    }

    fn install(&self, apk: &Path) -> Result<String, CableError> {
        self.connection()?.install(apk, INSTALL_LIMIT).map(|t| android_words(&t)).map_err(cable_error)
    }

    fn uninstall(&self) -> Result<String, CableError> {
        self.connection()?.exec(&uninstall_command(), SHELL_LIMIT).map(|t| android_words(&t)).map_err(cable_error)
    }

    fn reverse(&self, port: u16) -> Result<(), CableError> {
        self.connection()?.reverse(port, SHELL_LIMIT).map_err(cable_error)
    }

    fn reverse_remove(&self, port: u16) -> Result<(), CableError> {
        self.connection()?.reverse_remove(port, SHELL_LIMIT).map_err(cable_error)
    }

    fn close(mut self: Box<Self>) -> Vec<String> {
        if let Some(connection) = self.connection.take() {
            connection.close();
        }
        vec!["Let go of the phone's debugging interface: WinUSB's handle is closed, and nothing of this program keeps running.".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn androids_answers_are_read_as_the_other_routes_read_them() {
        assert_eq!(android_words("Success\n"), "Success");
        assert_eq!(android_words("Failure [INSTALL_FAILED_UPDATE_INCOMPATIBLE: x]\n"), "Failure [INSTALL_FAILED_UPDATE_INCOMPATIBLE: x]");
        assert_eq!(uninstall_command(), "cmd package uninstall ai.lucidloom.limo.spike07");
        let lost = cable_error(AdbError::Lost("gone".to_string()));
        assert!(matches!(lost, CableError::Lost(_)));
        assert!(matches!(open_problem(ConnectError::NotAllowed(Duration::from_secs(45))), OpenProblem::NotAllowed(why) if why.contains("45 seconds")));
    }
}
