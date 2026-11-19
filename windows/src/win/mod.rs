//! Everything that calls Windows. All of LiMo for Windows' unsafe code is
//! here, and each call is as thin as it can be: what to do is decided in the
//! modules above, which are tested against a fake PC. The tests in these
//! modules are limited to pure functions and harmless reads of the test's
//! own process.

mod about;
mod clock;
mod console;
mod display;
mod helper;
mod install;
mod launch;
mod logfile;
mod os;
mod pipe;
mod processes;
mod service;
mod sessions;

use windows::Win32::Foundation::{CloseHandle, HANDLE};

use crate::cli::{self, Command};

/// Runs one command and returns its exit code.
pub(crate) fn run(args: &[String]) -> i32 {
    match cli::parse(args) {
        Ok(Command::Help) => {
            println!("{}", cli::usage());
            0
        }
        Ok(Command::Service) => service::run(),
        Ok(Command::Helper { channel }) => helper::run(channel),
        Ok(Command::Install { dev }) => install::install(dev),
        Ok(Command::Uninstall) => install::uninstall(),
        Ok(Command::Status) => install::status(),
        Ok(Command::DevSetup { child, account, minutes, block }) => install::dev_setup(&child, &account, minutes, &block),
        Err(problem) => {
            println!("{problem}");
            2
        }
    }
}

/// A kernel handle, closed when dropped.
pub(crate) struct Owned(HANDLE);

// SAFETY: a kernel handle is a number the kernel looks up; it may be used
// and closed from any thread. `Owned` closes it once, when dropped.
unsafe impl Send for Owned {}

impl Owned {
    /// Takes ownership of a handle. None for a null or invalid one.
    pub(crate) fn new(handle: HANDLE) -> Option<Owned> {
        if handle.is_invalid() { None } else { Some(Owned(handle)) }
    }

    /// The handle, still owned here.
    pub(crate) fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY: the handle is owned here and closed once.
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

/// A string as UTF-16 ending in a NUL, for a Windows call.
pub(crate) fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// UTF-16 from Windows, up to its first NUL.
pub(crate) fn from_wide(units: &[u16]) -> String {
    let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
    String::from_utf16_lossy(&units[..end])
}

/// The plain Win32 error number in an error from the `windows` crate.
pub(crate) fn win32_code(error: &windows::core::Error) -> u32 {
    let bits = error.code().0 as u32;
    if bits & 0xFFFF_0000 == 0x8007_0000 { bits & 0xFFFF } else { bits }
}

/// "Win32 error N: text", for the log.
pub(crate) fn error_text(error: &windows::core::Error) -> String {
    format!("Win32 error {}: {}", win32_code(error), error.message().trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_and_strings_cross_to_windows_and_back() {
        let denied = windows::core::Error::from_hresult(windows::core::HRESULT(0x8007_0005_u32 as i32));
        assert_eq!(win32_code(&denied), 5);
        assert!(error_text(&denied).starts_with("Win32 error 5:"));
        let other = windows::core::Error::from_hresult(windows::core::HRESULT(0x8000_4005_u32 as i32));
        assert_eq!(win32_code(&other), 0x8000_4005);
        assert_eq!(from_wide(&wide("Asha")), "Asha");
        assert_eq!(from_wide(&[0x41, 0, 0x42]), "A");
        assert!(Owned::new(HANDLE(std::ptr::null_mut())).is_none());
    }
}
