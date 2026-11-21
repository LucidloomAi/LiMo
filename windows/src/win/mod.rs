//! Everything that calls Windows. All of LiMo for Windows' unsafe code is
//! here, and each call is as thin as it can be: what to do is decided in the
//! modules above, which are tested against a fake PC. The tests in these
//! modules are limited to pure functions, harmless reads of the test's own
//! process, and harmless reads of what the PC says of itself (its clocks,
//! whether it plays sound or keeps the screen awake), which change nothing
//! and pass whatever the answer.

/// `println!` for words a person reads in the terminal, their apostrophes
/// as LiMo writes them, whatever crate the words came from.
macro_rules! say {
    () => {
        println!()
    };
    ($($arg:tt)*) => {
        println!("{}", $crate::text::typographic(&format!($($arg)*)))
    };
}

mod about;
mod accounts;
mod clock;
mod com;
mod console;
mod devconsole;
mod display;
pub(crate) mod door;
mod drawn;
mod firewall;
mod helper;
pub(crate) mod install;
mod launch;
mod logfile;
mod machine;
mod os;
mod peers;
mod pipe;
mod playing;
mod processes;
mod service;
mod sessions;
mod shortcut;

use windows::Win32::Foundation::{CloseHandle, HANDLE};

use crate::cli::{self, Command};

/// Runs one command and returns its exit code.
pub(crate) fn run(args: &[String]) -> i32 {
    match cli::parse(args) {
        Ok(Command::Help) => {
            say!("{}", cli::usage());
            0
        }
        Ok(Command::Service) => service::run(),
        Ok(Command::Helper { channel, plain }) => helper::run(channel, plain),
        Ok(Command::Install { dev }) => install::install(dev),
        Ok(Command::Uninstall { family_file }) => install::uninstall(family_file),
        Ok(Command::StopForUpgrade) => install::stop_for_upgrade(),
        Ok(Command::Status) => install::status(),
        Ok(Command::DevSetup { child, account, minutes, block }) => install::dev_setup(&child, &account, minutes, &block),
        Ok(Command::DevCode { child, minutes }) => install::dev_code(child.as_deref(), minutes),
        Ok(Command::ResetPassword) => install::reset_password(),
        Ok(Command::DevConsole) => devconsole::run(),
        Err(problem) => {
            say!("{problem}");
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

/// The name the guardian sees for each child's device on this PC: the PC's
/// own name.
pub(crate) fn pc_name() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "This PC".to_string())
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

/// "Win32 error N: text", for the log. Windows' own text comes without its
/// closing full stop, so that a sentence can go on after it: "The helper
/// could not be started in session 1: Win32 error 2: The system cannot find
/// the file specified. LiMo tries again every second." Just "Win32 error N"
/// when Windows has no text for it.
pub(crate) fn error_text(error: &windows::core::Error) -> String {
    let code = win32_code(error);
    match without_closing_stop(&error.message()) {
        "" => format!("Win32 error {code}"),
        text => format!("Win32 error {code}: {text}"),
    }
}

/// A message without the whitespace around it and its closing full stops.
fn without_closing_stop(message: &str) -> &str {
    message.trim().trim_end_matches('.').trim_end()
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

    #[test]
    fn an_error_reads_right_in_the_middle_of_a_sentence() {
        // ERROR_FILE_NOT_FOUND: Windows' own text ends in a full stop. It is
        // in the PC's own language, so only an English one is checked word
        // for word.
        let missing = windows::core::Error::from_hresult(windows::core::HRESULT(0x8007_0002_u32 as i32));
        let raw = missing.message();
        let text = error_text(&missing);
        assert_eq!(text, format!("Win32 error 2: {}", raw.trim().trim_end_matches('.')));
        assert!(text.len() > "Win32 error 2: ".len() && !text.ends_with('.') && text.trim_end() == text, "{text:?}");
        let sentence = format!("The helper could not be started in session 1: {text}. LiMo tries again every second.");
        assert!(!sentence.contains(".."), "{sentence}");
        // On an English Windows, the words themselves.
        if raw.starts_with("The system") {
            assert_eq!(text, "Win32 error 2: The system cannot find the file specified");
        }

        assert_eq!(without_closing_stop(" Access is denied.\r\n"), "Access is denied");
        assert_eq!(without_closing_stop("Access is denied"), "Access is denied");
        assert_eq!(without_closing_stop("Wait... "), "Wait");
        assert_eq!(without_closing_stop(" . \r\n"), "");
        assert_eq!(without_closing_stop("Version 1.2 is needed."), "Version 1.2 is needed");
    }
}
