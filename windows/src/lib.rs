//! # LiMo for Windows
//!
//! The Windows service on the master PC, around the agent's engine, and the
//! helper it starts in each signed in session that needs one. One program,
//! `limo.exe`, does both, and installs and removes itself for testing.
//!
//! - The **service** runs as LocalSystem. About once a second it gathers the
//!   facts (the clocks, the sessions, the programs running in each, and what
//!   each session's helper reports), calls the engine
//!   ([`limo_agent::Engine::tick`]) and carries out the orders: a helper in
//!   each session that needs one, what it shows, programs closed, and a
//!   session disconnected when its lock cannot be confirmed. Ten times a
//!   second it looks after its helpers. See [`service`].
//! - The **helper** runs as the signed in user, guarded: UIAccess, one
//!   integrity step above the user's own programs, and a process access
//!   list that does not let the user end it. It is born with its channel to
//!   the service, reports what only a program inside the session can see,
//!   and shows what it is told through a [`helper::Display`]. See
//!   [`helper`] and [`protocol`].
//!
//! Everything the service decides is written against the [`os::Os`] trait,
//! and tested against a fake PC. The calls into Windows are thin and live in
//! the `win` modules alone; no other module has unsafe code, and each says
//! so with `#![forbid(unsafe_code)]`.
//!
//! What LiMo records stays as the engine allows: which program, and for how
//! long. Nothing here reads, keeps, sends or logs a window title, a file
//! being edited or a web address.

#![warn(missing_docs)]

pub mod about;
pub mod cli;
pub mod close;
pub mod helper;
pub mod os;
pub mod programs;
pub mod protocol;
pub mod quiet;
pub mod service;
pub mod settings;
pub mod setup;
pub mod text;

#[cfg(windows)]
mod win;

/// Runs `limo.exe` with its command line (without the program's own name)
/// and returns the exit code: 0 done, 1 failed, 2 refused.
pub fn run(args: &[String]) -> i32 {
    #[cfg(windows)]
    {
        win::run(args)
    }
    #[cfg(not(windows))]
    {
        let _ = args;
        eprintln!("limo runs only on Windows.");
        2
    }
}

/// The version of this crate, for example "0.1.0".
pub fn windows_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
