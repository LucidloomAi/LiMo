//! The seam between what the service decides and what Windows does: the
//! [`Os`] trait. The real implementation is in the Windows modules; the
//! tests use a fake one. Everything the service decides is written against
//! this trait alone, so it is tested without touching the PC.

#![forbid(unsafe_code)]

use limo_agent::SessionState;

use crate::protocol::{ChannelProblem, Frame, FromHelper};

/// One Windows session with a signed in user, as Windows tells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSeen {
    /// The session's id. Never 0: session 0 is where services run, and no
    /// one signs in there.
    pub session_id: u32,
    /// The account's security identifier, as a string ("S-1-5-21-...").
    pub account_id: String,
    /// The account's user name.
    pub account_name: String,
    /// In use, locked by Windows, or disconnected.
    pub state: SessionState,
    /// True when the account is in the Administrators group, also when it is
    /// not running elevated.
    pub is_administrator: bool,
}

/// One running process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessSeen {
    /// Its process id.
    pub pid: u32,
    /// The session it runs in.
    pub session_id: u32,
    /// When it started, in Windows' own count (FILETIME); 0 when it could
    /// not be read. With the pid, it tells this process from a later one
    /// that is given the same pid.
    pub started: u64,
    /// Its file name as the process list gives it, with no folder.
    pub name: String,
    /// The full path of its program file, or empty when it could not be
    /// read.
    pub path: String,
}

/// What a program file's version resource says about it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VersionNames {
    /// The original file name its maker recorded (`OriginalFilename`).
    pub original_name: Option<String>,
    /// Its maker (`CompanyName`).
    pub maker: Option<String>,
}

/// A helper the service started, as the service names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HelperId(pub u64);

/// A helper started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelperStarted {
    /// Its name for the service.
    pub id: HelperId,
    /// Its process id.
    pub pid: u32,
    /// What the guarded launch did, for the log, such as
    /// "uiaccess=1 integrity=set dacl=admin-owner".
    pub detail: String,
    /// Each step of the guarded launch that failed on the way, in plain
    /// words. The helper was started all the same: a weaker lock is better
    /// than none.
    pub problems: Vec<String>,
}

/// What came from a helper's channel.
#[derive(Debug, Clone, PartialEq)]
pub enum Received {
    /// A whole message.
    Message(FromHelper),
    /// Something that is not a message: the channel is closed.
    Broken(ChannelProblem),
    /// The helper closed its end: it has ended.
    Closed,
}

/// Why a message could not be handed to a helper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendProblem {
    /// The helper is not reading what it is sent.
    NotReading,
    /// The helper is gone.
    Gone,
}

/// What the operating system can tell the service, and do for it.
///
/// Every call is made from the service's one thread. None of them may
/// panic; a call that fails says so in its result, or gives what it could.
pub trait Os {
    /// The wall clock, unix seconds.
    fn wall_secs(&mut self) -> i64;
    /// A steady count of milliseconds that keeps counting while the PC
    /// sleeps and cannot be set.
    fn steady_ms(&mut self) -> u64;
    /// The PC's time zone now, as seconds from UTC, with daylight time when
    /// it is in force.
    fn utc_offset_secs(&mut self) -> i32;
    /// Every session with a signed in user.
    fn sessions(&mut self) -> Vec<SessionSeen>;
    /// Every process in every session but session 0.
    fn processes(&mut self) -> Vec<ProcessSeen>;
    /// One process, by its id, if it is running.
    fn process(&mut self, pid: u32) -> Option<ProcessSeen>;
    /// When a file was last written, as a number that changes when it is
    /// written again; None when that cannot be read.
    fn file_stamp(&mut self, path: &str) -> Option<u64>;
    /// What a program file's version resource says.
    fn version_names(&mut self, path: &str) -> VersionNames;
    /// Starts a helper in a session, the guarded way, born with its
    /// channel.
    fn start_helper(&mut self, session_id: u32) -> Result<HelperStarted, String>;
    /// True while a helper's process runs.
    fn helper_running(&mut self, id: HelperId) -> bool;
    /// Ends a helper's process and closes its channel. Nothing happens for
    /// a helper that is gone.
    fn end_helper(&mut self, id: HelperId);
    /// Hands a frame to a helper's channel, without waiting for it to be
    /// read.
    fn send(&mut self, id: HelperId, frame: Frame) -> Result<(), SendProblem>;
    /// Everything that came from the helpers' channels since the last call.
    fn received(&mut self) -> Vec<(HelperId, Received)>;
    /// Ends a process, after checking again that it is the same process
    /// (its session, its start and its path) that the list showed.
    fn end_process(&mut self, process: &ProcessSeen) -> Result<(), String>;
    /// Disconnects a session, so that the Windows sign in screen covers it.
    /// Its programs stay open.
    fn disconnect_session(&mut self, session_id: u32) -> Result<(), String>;
    /// True when the test cap is switched on in the data folder's settings.
    fn test_cap(&mut self) -> bool;
    /// Writes a line to the service's log.
    fn log(&mut self, line: &str);
}
