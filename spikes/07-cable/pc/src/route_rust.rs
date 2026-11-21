//! What the two Rust routes share: adb_client's own device, over libusb or
//! over nusb, and every call to it with a time limit.
//!
//! What adb_client 4.0.0 does, read in its source for this proof:
//!
//! - a shell command goes as `shell:COMMAND`, adb's first shell protocol: no
//!   exit code and no separate error stream come back (`shell_command`
//!   always answers `None` for the code);
//! - a command with standard input goes as `exec:COMMAND` (as `adb
//!   exec-in` does): the input is sent, and its end is the end of the whole
//!   stream, so whatever the command prints after that is lost. The door's
//!   answer is therefore read by a second command (`content query`);
//! - an install goes as `exec:cmd package 'install' -S SIZE`, with no `-r`;
//!   its answer is Android's own (`Success`, or the failure's words);
//! - a reverse is relayed by adb_client itself, inside this program, for as
//!   long as the device is held: it ends with the step.

#![forbid(unsafe_code)]

use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use adb_client::{ADBDeviceExt, RustADBError};
use zeroize::Zeroizing;

use crate::cable::{Answer, Cable, CableError, INSTALL_LIMIT, SHELL_LIMIT, Via, with_timeout};
use crate::phone::{Ask, PACKAGE};

/// An adb_client device, held for one step.
pub struct ClientCable<D: ADBDeviceExt + Send + Sync + 'static> {
    via: Via,
    device: Arc<D>,
    /// Whatever must live as long as the device (nusb's relay).
    keep: Option<Box<dyn Send>>,
}

impl<D: ADBDeviceExt + Send + Sync + 'static> ClientCable<D> {
    /// Wraps an open device.
    pub fn new(via: Via, device: D, keep: Option<Box<dyn Send>>) -> Self {
        ClientCable { via, device: Arc::new(device), keep }
    }
}

/// adb_client's error, as this program says it.
pub fn cable_error(error: RustADBError) -> CableError {
    match &error {
        RustADBError::IOError(e)
            if matches!(
                e.kind(),
                ErrorKind::ConnectionAborted | ErrorKind::ConnectionReset | ErrorKind::BrokenPipe | ErrorKind::UnexpectedEof
            ) =>
        {
            CableError::Lost(e.to_string())
        }
        RustADBError::UsbError(rusb::Error::NoDevice | rusb::Error::Io | rusb::Error::Pipe) => CableError::Lost(error.to_string()),
        _ => CableError::Failed(error.to_string()),
    }
}

/// Bytes handed to the phone's standard input, wiped when dropped.
struct InputReader {
    bytes: Zeroizing<Vec<u8>>,
    at: usize,
}

impl Read for InputReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let rest = &self.bytes[self.at..];
        let n = rest.len().min(buf.len());
        buf[..n].copy_from_slice(&rest[..n]);
        self.at += n;
        Ok(n)
    }
}

/// What the phone prints, kept for after the call.
#[derive(Clone, Default)]
struct Collected(Arc<Mutex<Vec<u8>>>);

impl Write for Collected {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Ok(mut all) = self.0.lock() {
            all.extend_from_slice(buf);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Android's answer from adb_client's result for an install or a removal:
/// `Success`, or the failure's own words.
fn android_answer(result: Result<Result<(), RustADBError>, std::time::Duration>) -> Result<String, CableError> {
    match result {
        Err(limit) => Err(CableError::Timeout(limit)),
        Ok(Ok(())) => Ok("Success".to_string()),
        Ok(Err(RustADBError::ADBRequestFailed(words))) => Ok(words.trim().to_string()),
        Ok(Err(other)) => Err(cable_error(other)),
    }
}

impl<D: ADBDeviceExt + Send + Sync + 'static> Cable for ClientCable<D> {
    fn via(&self) -> Via {
        self.via
    }

    fn shell(&self, ask: &Ask) -> Result<Answer, CableError> {
        let device = self.device.clone();
        let command = ask.command();
        let result = with_timeout(SHELL_LIMIT, move || {
            let mut out = Vec::new();
            device.shell_command(&command, Some(&mut out), None).map(|_| out)
        });
        match result {
            Err(limit) => Err(CableError::Timeout(limit)),
            Ok(Err(e)) => Err(cable_error(e)),
            Ok(Ok(out)) => Ok(Answer { text: String::from_utf8_lossy(&out).into_owned(), exit: None }),
        }
    }

    fn shell_with_input(&self, ask: &Ask, input: &[u8]) -> Result<Answer, CableError> {
        let device = self.device.clone();
        let command = ask.command();
        let reader = InputReader { bytes: Zeroizing::new(input.to_vec()), at: 0 };
        let output = Collected::default();
        let kept = output.clone();
        let result = with_timeout(SHELL_LIMIT, move || device.exec(&command, Box::new(reader), Box::new(output)));
        match result {
            Err(limit) => Err(CableError::Timeout(limit)),
            Ok(Err(e)) => Err(cable_error(e)),
            Ok(Ok(())) => {
                let text = kept.0.lock().map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
                Ok(Answer { text, exit: None })
            }
        }
    }

    fn install(&self, apk: &Path) -> Result<String, CableError> {
        let device = self.device.clone();
        let apk: PathBuf = apk.to_path_buf();
        android_answer(with_timeout(INSTALL_LIMIT, move || device.install(&apk, None)))
    }

    fn uninstall(&self) -> Result<String, CableError> {
        let device = self.device.clone();
        android_answer(with_timeout(SHELL_LIMIT, move || device.uninstall(PACKAGE, None)))
    }

    fn reverse(&self, port: u16) -> Result<(), CableError> {
        let device = self.device.clone();
        match with_timeout(SHELL_LIMIT, move || device.reverse(&format!("tcp:{port}"), &format!("tcp:{port}"))) {
            Err(limit) => Err(CableError::Timeout(limit)),
            Ok(result) => result.map_err(cable_error),
        }
    }

    fn reverse_remove(&self, port: u16) -> Result<(), CableError> {
        let device = self.device.clone();
        match with_timeout(SHELL_LIMIT, move || device.reverse_remove(&format!("tcp:{port}"))) {
            Err(limit) => Err(CableError::Timeout(limit)),
            Ok(result) => result.map_err(cable_error),
        }
    }

    fn close(self: Box<Self>) -> Vec<String> {
        let ClientCable { device, keep, .. } = *self;
        drop(device);
        drop(keep);
        vec!["Let go of the phone's debugging interface: nothing of this program keeps running.".to_string()]
    }
}
