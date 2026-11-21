//! # LiMo's guardian console
//!
//! The window a guardian opens on the master PC to see the family's day and
//! change its rules. It is the guardian form's own React Native screens,
//! from `phone/app`, exported for the web and drawn by WebView2 in a Tauri
//! window (`limo-console.exe`). It holds no state of its own: every action
//! goes to LiMo's service through the door, a named pipe that only the
//! service makes, and the service alone holds the family's file.
//!
//! - [`protocol`]: what crosses the door, and how (the helper channel's
//!   frames, with a password carried as a wiped [`protocol::Secret`]).
//! - [`door`]: the console's half. Before it says anything it checks that
//!   the pipe is served by LiMo's service: session 0, owned by
//!   LocalSystem, LiMo's own program file.
//! - [`stand_in`]: the service's half in miniature, for the tests and the
//!   self check: the door's access list, its check of who is on the other
//!   end, and the refusal of a child's account.
//! - [`webview`]: the switches WebView2 starts with, and what each was
//!   measured to leave of its contact with Microsoft.
//! - [`measure`]: what the measurement of that contact reads (a process's
//!   connections, a loopback proxy that writes down host names and sends
//!   nothing on).
//!
//! The calls into Windows are thin and live in the `win` modules alone; no
//! other module has unsafe code, and each says so with
//! `#![forbid(unsafe_code)]`.

#![warn(missing_docs)]

pub mod door;
pub mod measure;
pub mod protocol;
pub mod stand_in;
pub mod webview;

#[cfg(windows)]
mod win;

#[cfg(all(test, windows))]
mod door_tests;

/// The version of this crate, for example "0.1.0".
pub fn console_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// What the console's window needs from Windows, kept here so that the
/// window's own code holds no unsafe code: WebView2's settings, and the
/// self check's key probe.
#[cfg(all(windows, feature = "window"))]
pub mod window {
    pub use crate::win::keys::{Probe, WindowSeen, children, input_window, send, visible_windows_of};
    pub use crate::win::webview2::{SettingsSeen, lock_down, parent_window, read_settings};
}

/// Who this process runs as, and where Windows keeps programs: for the
/// self check, which plays the service's part in the builder's own account.
#[cfg(windows)]
pub mod this_process {
    /// This process's account, as a security identifier.
    pub fn account() -> Option<String> {
        crate::win::token::own_account()
    }

    /// The session this process runs in.
    pub fn session() -> Option<u32> {
        crate::win::token::session_of(std::process::id())
    }

    /// True when this process runs elevated.
    pub fn elevated() -> bool {
        crate::win::token::own_token_is_elevated()
    }

    /// Windows' own Program Files folder.
    pub fn program_files() -> Option<String> {
        crate::win::token::program_files()
    }

    /// What the console's server check must see for a door served by this
    /// very process: its session, its account, its program file.
    pub fn as_server() -> Option<crate::door::ServerMustBe> {
        Some(crate::door::ServerMustBe {
            session: session()?,
            owner: account()?,
            image: std::env::current_exe().ok()?.to_string_lossy().into_owned(),
        })
    }
}
