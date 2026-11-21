//! LiMo for Windows' own door, served in this process: its pipe threads
//! (`RealDoor`), everything it decides (`Service::door`, on the service's
//! own loop), and a real family's file in a temporary folder, made with a
//! guardian password. It runs in whatever account runs it, at a pipe name
//! of the caller's choosing, never LiMo's own door.
//!
//! The PC under it is quiet: no one signed in, no programs, no helpers, so
//! the service counts nothing and does nothing but answer its door. Who a
//! client is still comes from Windows, through the door's own threads, so
//! an account linked to a child is refused as on a family's PC.
//!
//! Included by the console's tests (`src/real_door_tests.rs`) and by
//! `examples/real_door.rs`, which serves it for the self check. Never in the
//! console's exe.

#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use limo_hub::{AccountLink, AgeBand, Hub};
use limo_windows::os::{
    AccountSeen, DoorEvent, DoorId, HelperId, HelperStarted, Os, ProcessSeen, Received, SendProblem, SessionSeen, VersionNames,
};
use limo_windows::protocol::Frame;
use limo_windows::service::{Service, ServiceConfig};
use limo_windows::{DOOR_SDDL, RealDoor, this_account};

/// A PC with nothing on it but the door.
struct QuietPc {
    started: Instant,
    door: RealDoor,
    log: Arc<Mutex<Vec<String>>>,
}

impl Os for QuietPc {
    fn wall_secs(&mut self) -> i64 {
        now()
    }
    fn steady_ms(&mut self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
    fn utc_offset_secs(&mut self) -> i32 {
        0
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
        Err("no helpers on this PC".to_string())
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
        Err("nothing is ended on this PC".to_string())
    }
    fn disconnect_session(&mut self, _session_id: u32) -> Result<(), String> {
        Err("no sessions on this PC".to_string())
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
        Ok(Vec::new())
    }
    fn home_addresses(&mut self) -> Vec<std::net::Ipv4Addr> {
        Vec::new()
    }
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// The real door, served on a thread of its own until dropped.
pub struct RealDoorHost {
    name: String,
    stop: Arc<AtomicBool>,
    log: Arc<Mutex<Vec<String>>>,
    thread: Option<JoinHandle<()>>,
}

impl RealDoorHost {
    /// Serves the real door at `name`, for a family as it is after its first
    /// start: the guardian password `password`, a PIN, and a child called
    /// Asha (so the guardian screens open on Today after signing in). With
    /// `asha_account`, that Windows account (a security identifier) is
    /// linked to Asha. Returns once the door's pipe is made.
    pub fn start(name: &str, password: &str, asha_account: Option<&str>) -> RealDoorHost {
        let stop = Arc::new(AtomicBool::new(false));
        let log = Arc::new(Mutex::new(Vec::new()));
        let (ready_tx, ready_rx) = channel::<Result<(), String>>();
        let (name2, password, asha) = (name.to_string(), password.to_string(), asha_account.map(str::to_string));
        let (stop2, log2) = (Arc::clone(&stop), Arc::clone(&log));
        let thread = std::thread::spawn(move || {
            let made = (|| -> Result<(tempfile::TempDir, Service<QuietPc>), String> {
                let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
                let mut hub = Hub::open(&dir.path().join("family.limo")).map_err(|e| format!("{e:?}"))?;
                hub.create_family(&password, now()).map_err(|e| format!("{e:?}"))?;
                let pin: String = "4718203956".chars().take(limo_hub::MIN_PIN_DIGITS as usize).collect();
                hub.set_pin(&pin, now()).map_err(|e| format!("{e:?}"))?;
                let child = hub.add_child("Asha", AgeBand::TenToTwelve, 0, now()).map_err(|e| format!("{e:?}"))?.child_id;
                if let Some(account) = &asha {
                    hub.link_account(account, "Asha", AccountLink::Child { child_id: child }).map_err(|e| format!("{e:?}"))?;
                }
                // The door's own access list, and this account allowed to
                // make an instance too, as the service's own tests do: the
                // service, as LocalSystem, makes one for each console.
                let me = this_account().ok_or("this account could not be read")?;
                let door = RealDoor::open_with(&name2, || {}, Duration::from_secs(10), &format!("{DOOR_SDDL}(A;;GA;;;{me})"));
                let config = ServiceConfig {
                    pc_name: "Family PC".to_string(),
                    own_pid: std::process::id(),
                    own_exe: std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
                    windows_dir: r"C:\Windows".to_string(),
                };
                let os = QuietPc { started: Instant::now(), door, log: log2 };
                Ok((dir, Service::new(os, hub, config)))
            })();
            let (_dir, mut service) = match made {
                Ok(made) => made,
                Err(problem) => {
                    let _ = ready_tx.send(Err(problem));
                    return;
                }
            };
            // The door says in its log when its pipe is open ("The door for
            // LiMo’s settings is open.", its apostrophe either way).
            let mut said_ready = false;
            while !stop2.load(Ordering::SeqCst) {
                service.step();
                service.door();
                let open = |x: &String| x.starts_with("The door for LiMo") && x.contains("settings is open");
                if !said_ready && service.os().log.lock().map(|l| l.iter().any(open)).unwrap_or(false) {
                    said_ready = true;
                    let _ = ready_tx.send(Ok(()));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            service.stop();
        });
        match ready_rx.recv_timeout(Duration::from_secs(20)) {
            Ok(Ok(())) => {}
            Ok(Err(problem)) => panic!("the real door could not be served: {problem}"),
            Err(_) => panic!("the real door did not open in 20 seconds"),
        }
        RealDoorHost { name: name.to_string(), stop, log, thread: Some(thread) }
    }

    /// The pipe's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The service's log so far.
    pub fn log(&self) -> Vec<String> {
        self.log.lock().map(|l| l.clone()).unwrap_or_default()
    }
}

impl Drop for RealDoorHost {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
