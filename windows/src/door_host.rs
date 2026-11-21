//! A small host of the console's door, for other crates' tests: the real
//! service and its real door, on a named pipe of the test's own, over a real
//! family file in a folder the test gives, with nothing else of a PC (no
//! sessions, no programs, no helpers, no link). A test of the console's
//! side knocks on it with its own pipe client, as on a family's PC; it never
//! implements LiMo's `Os` itself, so it does not break when that trait
//! gains a call. Only with the `door-host` feature; never part of the
//! service.
//!
//! ```toml
//! [dev-dependencies]
//! limo-windows = { path = "../windows", features = ["door-host"] }
//! ```
//!
//! ```no_run
//! # fn main() -> Result<(), String> {
//! use limo_windows::door_host::{DoorHost, DoorHostSetup};
//! let folder = std::env::temp_dir().join("a-test-of-mine");
//! let host = DoorHost::start(DoorHostSetup::new(r"\\.\pipe\limo.console.test.mine", &folder))?;
//! // connect to host.pipe_name(), say hello, sign in with the setup's
//! // password, ask; the answers are the door's own.
//! drop(host); // the door closes, the family's file with it
//! # Ok(())
//! # }
//! ```
//!
//! The pipe's access list is the door's own, with the test's account added
//! (making the pipe's second instance takes it; see
//! `limo_windows::RealDoor::open_with`). The service's log is kept in memory
//! ([`DoorHost::log`]). Its clock is the PC's own.

#![forbid(unsafe_code)]

use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use limo_hub::Hub;

use crate::console::HELLO_WAIT_MS;
use crate::os::{AccountSeen, DoorEvent, DoorId, HelperId, HelperStarted, Os, ProcessSeen, Received, SendProblem, SessionSeen, VersionNames};
use crate::protocol::Frame;
use crate::service::{STEP_MS, Service, ServiceConfig};
use crate::{DOOR_SDDL, RealDoor, this_account};

/// The password a family made by the host has, unless the setup says
/// another.
pub const HOST_PASSWORD: &str = "a family password for tests";

/// What the host starts with.
#[derive(Debug, Clone)]
pub struct DoorHostSetup {
    /// The pipe's name: the test's own, never `\\.\pipe\limo.console`.
    pub pipe_name: String,
    /// The folder the family's file is made in (a temporary one).
    pub folder: PathBuf,
    /// The Windows accounts the door lists (`accounts`).
    pub accounts: Vec<AccountSeen>,
    /// The PC's time zone, seconds from UTC.
    pub utc_offset_secs: i32,
    /// A family made first, with this password; none when the test makes
    /// it through the door.
    pub family_password: Option<String>,
}

impl DoorHostSetup {
    /// A host on `pipe_name` with its family's file in `folder`, a family
    /// made with [`HOST_PASSWORD`], no accounts, and the zone of UTC.
    pub fn new(pipe_name: &str, folder: &Path) -> DoorHostSetup {
        DoorHostSetup {
            pipe_name: pipe_name.to_string(),
            folder: folder.to_path_buf(),
            accounts: Vec::new(),
            utc_offset_secs: 0,
            family_password: Some(HOST_PASSWORD.to_string()),
        }
    }
}

/// The PC, as the host has it: the real door, and nothing else.
struct HostOs {
    door: RealDoor,
    accounts: Vec<AccountSeen>,
    offset: i32,
    started: Instant,
    log: Arc<Mutex<Vec<String>>>,
}

impl Os for HostOs {
    fn wall_secs(&mut self) -> i64 {
        SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
    }
    fn steady_ms(&mut self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
    fn utc_offset_secs(&mut self) -> i32 {
        self.offset
    }
    fn sessions(&mut self) -> Vec<SessionSeen> {
        Vec::new()
    }
    fn processes(&mut self) -> Vec<ProcessSeen> {
        Vec::new()
    }
    fn process(&mut self, _pid: u32) -> Option<ProcessSeen> {
        None
    }
    fn file_stamp(&mut self, _path: &str) -> Option<u64> {
        None
    }
    fn version_names(&mut self, _path: &str) -> VersionNames {
        VersionNames::default()
    }
    fn start_helper(&mut self, _session_id: u32) -> Result<HelperStarted, String> {
        Err("the door’s test host starts no helper".to_string())
    }
    fn helper_running(&mut self, _id: HelperId) -> bool {
        false
    }
    fn end_helper(&mut self, _id: HelperId) {}
    fn send(&mut self, _id: HelperId, _frame: Frame) -> Result<(), SendProblem> {
        Err(SendProblem::Gone)
    }
    fn received(&mut self) -> Vec<(HelperId, Received)> {
        Vec::new()
    }
    fn end_process(&mut self, _process: &ProcessSeen) -> Result<(), String> {
        Err("the door’s test host ends no process".to_string())
    }
    fn disconnect_session(&mut self, _session_id: u32) -> Result<(), String> {
        Ok(())
    }
    fn test_cap(&mut self) -> bool {
        false
    }
    fn log(&mut self, line: &str) {
        if let Ok(mut log) = self.log.lock() {
            log.push(line.to_string());
        }
    }
    fn door_received(&mut self) -> Vec<(DoorId, DoorEvent)> {
        self.door.received()
    }
    fn door_send(&mut self, id: DoorId, frame: Frame) -> Result<(), SendProblem> {
        self.door.send(id, frame)
    }
    fn door_close(&mut self, id: DoorId) {
        self.door.close(id);
    }
    fn accounts(&mut self) -> Result<Vec<AccountSeen>, String> {
        Ok(self.accounts.clone())
    }
    fn home_addresses(&mut self) -> Vec<Ipv4Addr> {
        Vec::new()
    }
}

/// The host: the service on a thread of its own, stepping as the service
/// does. Dropping it stops the service and closes the door.
pub struct DoorHost {
    pipe_name: String,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    log: Arc<Mutex<Vec<String>>>,
}

impl std::fmt::Debug for DoorHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DoorHost").field("pipe_name", &self.pipe_name).finish_non_exhaustive()
    }
}

impl DoorHost {
    /// Starts the host, and returns once its door is open (or says why it
    /// could not be).
    pub fn start(setup: DoorHostSetup) -> Result<DoorHost, String> {
        if setup.pipe_name.eq_ignore_ascii_case(crate::console::CONSOLE_PIPE_NAME) {
            return Err("the door’s test host never takes the real door’s name".to_string());
        }
        std::fs::create_dir_all(&setup.folder).map_err(|e| e.to_string())?;
        let mut hub = Hub::open(&setup.folder.join("family.limo")).map_err(|e| e.to_string())?;
        if let Some(password) = &setup.family_password
            && !hub.has_family().map_err(|e| e.to_string())?
        {
            hub.create_family(password, SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64))
                .map_err(|e| e.to_string())?;
        }
        let me = this_account().ok_or("this process’s own account could not be read")?;
        let stop = Arc::new(AtomicBool::new(false));
        let log = Arc::new(Mutex::new(Vec::new()));
        let (ready_tx, ready) = std::sync::mpsc::channel();
        let thread = {
            let (stop, log, pipe) = (stop.clone(), log.clone(), setup.pipe_name.clone());
            std::thread::Builder::new()
                .name("limo door host".to_string())
                .spawn(move || {
                    let door =
                        RealDoor::open_with(&pipe, || {}, Duration::from_millis(HELLO_WAIT_MS), &format!("{DOOR_SDDL}(A;;GA;;;{me})"));
                    let os = HostOs { door, accounts: setup.accounts, offset: setup.utc_offset_secs, started: Instant::now(), log: log.clone() };
                    let config = ServiceConfig {
                        pc_name: "Test PC".to_string(),
                        own_pid: std::process::id(),
                        own_exe: String::new(),
                        windows_dir: r"C:\Windows".to_string(),
                    };
                    let mut service = Service::new(os, hub, config);
                    let mut told = false;
                    let mut stepped: Option<Instant> = None;
                    while !stop.load(Ordering::SeqCst) {
                        // A step every 100 ms, and the door's turn between,
                        // as the service's own loop has them.
                        if stepped.is_none_or(|at| at.elapsed() >= Duration::from_millis(STEP_MS)) {
                            stepped = Some(Instant::now());
                            service.step();
                        }
                        service.door();
                        if !told && log.lock().is_ok_and(|l| l.iter().any(|line| line.contains("could not be opened") || line.contains("is open"))) {
                            told = true;
                            let _ = ready_tx.send(());
                        }
                        std::thread::sleep(Duration::from_millis(STEP_MS / 5));
                    }
                    service.stop();
                    service.finish();
                })
                .map_err(|e| e.to_string())?
        };
        let host = DoorHost { pipe_name: setup.pipe_name, stop, thread: Some(thread), log };
        if ready.recv_timeout(Duration::from_secs(10)).is_err() {
            return Err("the door’s test host did not open its door in time".to_string());
        }
        if let Some(problem) = host.log().into_iter().find(|l| l.contains("could not be opened")) {
            return Err(problem);
        }
        Ok(host)
    }

    /// The pipe's name.
    pub fn pipe_name(&self) -> &str {
        &self.pipe_name
    }

    /// The service's log so far.
    pub fn log(&self) -> Vec<String> {
        self.log.lock().map(|l| l.clone()).unwrap_or_default()
    }
}

impl Drop for DoorHost {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
