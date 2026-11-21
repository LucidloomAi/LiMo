//! # LiMo for Windows
//!
//! The Windows service on the master PC, around the agent's engine, and the
//! helper it starts in each signed in session that needs one. One program,
//! `limo.exe`, does both, and does the real work of installing, upgrading
//! and removing LiMo, for an installer that only copies files (see
//! [`installer`]).
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
//!   and shows what it is told through a [`helper::Display`]: the lock
//!   screen and time left display drawn to the agreed design (see
//!   [`drawn`]), or the plain display in their place. See [`helper`] and
//!   [`protocol`].
//! - The **console's door** is a named pipe the service serves, through
//!   which the guardian's console, a window in a guardian's own session,
//!   asks for everything it shows and changes: the service alone holds the
//!   family's file. A child's account is refused; everything else needs the
//!   guardian password. Requests are answered on the service's one thread,
//!   between two of its steps. See [`console`] and [`service::door`].
//! - The **link with the family's phones** listens on TCP port 47810 (see
//!   [`phones`]): one thread for each connection runs the link's master
//!   side, and everything that needs the family's file is answered on the
//!   service's one thread, between two steps ([`service::link`]), so a phone
//!   and the PC spend one budget and the PC's counting never waits for a
//!   phone. While the family has a phone, the PC announces itself on its
//!   home networks ([`announce`]).
//!
//! Everything the service decides is written against the [`os::Os`] trait,
//! and tested against a fake PC. The calls into Windows are thin and live in
//! the `win` modules alone; no other module has unsafe code, and each says
//! so with `#![forbid(unsafe_code)]`.
//!
//! What LiMo records stays as the engine allows: which program, and for how
//! long. Nothing here reads, keeps, sends or logs a window title, a file
//! being edited or a web address. A program's description, from its own
//! file's version resource, names it in the notice that it was closed, and
//! goes nowhere else but the log.

#![warn(missing_docs)]

pub mod about;
pub mod announce;
pub mod apps;
pub mod cli;
pub mod close;
pub mod console;
pub mod devconsole;
#[cfg(all(windows, feature = "door-host"))]
pub mod door_host;
pub mod drawn;
pub mod helper;
pub mod installer;
pub mod os;
#[cfg(feature = "played-phone")]
pub mod played_phone;
pub mod phones;
pub mod programs;
pub mod protocol;
pub mod quiet;
pub mod service;
pub mod settings;
pub mod standing;
pub mod setup;
pub mod text;

#[cfg(windows)]
mod win;

/// The console's door as Windows serves it, a client of it, and this
/// process's own account: for the tests over a real named pipe, and for
/// `limo dev-console`.
#[cfg(windows)]
pub use win::door::{CLIENT_ACCESS, DOOR_OWNER, DOOR_SDDL, DoorClient, DoorLine, RealDoor, ServerSeen, door_descriptor, this_account};

/// True when this process runs elevated: for the tests of the door, whose
/// access list an elevated administrator passes.
#[cfg(windows)]
pub use win::install::is_elevated;

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
