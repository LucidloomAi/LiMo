//! The real [`Os`]: Windows, as the service sees it.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, SyncSender, TrySendError, channel};

use windows::Win32::Foundation::WAIT_TIMEOUT;
use windows::Win32::System::Threading::{TerminateProcess, WaitForSingleObject};

use super::door::RealDoor;
use super::pipe::{Incoming, spawn_reader, spawn_writer};
use super::{Owned, accounts, clock, launch, logfile, processes, sessions};
use crate::os::{AccountSeen, DoorEvent, DoorId, HelperId, HelperStarted, Os, ProcessSeen, Received, SendProblem, SessionSeen, VersionNames};
use crate::protocol::{Frame, FromHelper};
use crate::settings::{Places, Settings, read_settings};

/// How often the settings file is read again, in milliseconds.
const SETTINGS_EVERY_MS: u64 = 10_000;

struct HelperProcess {
    process: Owned,
    writer: SyncSender<Frame>,
}

/// Windows, for the service.
pub(crate) struct WinOs {
    exe: PathBuf,
    places: Places,
    next_id: u64,
    helpers: HashMap<HelperId, HelperProcess>,
    events_tx: Sender<(HelperId, Received)>,
    events_rx: Receiver<(HelperId, Received)>,
    settings_read_at: Option<u64>,
    settings: Settings,
    door: Option<RealDoor>,
}

impl WinOs {
    /// Windows, for a service running from `exe`, with its data in
    /// `places`.
    pub(crate) fn new(exe: PathBuf, places: Places) -> WinOs {
        let (events_tx, events_rx) = channel();
        WinOs {
            exe,
            places,
            next_id: 0,
            helpers: HashMap::new(),
            events_tx,
            events_rx,
            settings_read_at: None,
            settings: Settings::default(),
            door: None,
        }
    }

    /// Opens the console's door at `name`. `wake` is called, on another
    /// thread, whenever something came to it.
    pub(crate) fn open_door(&mut self, name: &str, wake: impl Fn() + Send + Sync + 'static) {
        self.door = Some(RealDoor::open(name, wake));
    }

    /// The settings file, read again when the last read is older than
    /// [`SETTINGS_EVERY_MS`]. A file that cannot be read is no setting.
    fn settings(&mut self) -> Settings {
        let now = clock::steady_ms();
        if self.settings_read_at.is_none_or(|at| now.saturating_sub(at) >= SETTINGS_EVERY_MS) {
            self.settings_read_at = Some(now);
            self.settings = std::fs::read_to_string(self.places.settings_file()).map(|text| read_settings(&text)).unwrap_or_default();
        }
        self.settings
    }
}

impl Os for WinOs {
    fn wall_secs(&mut self) -> i64 {
        clock::wall_secs()
    }

    fn steady_ms(&mut self) -> u64 {
        clock::steady_ms()
    }

    fn utc_offset_secs(&mut self) -> i32 {
        clock::utc_offset_secs()
    }

    fn sessions(&mut self) -> Vec<SessionSeen> {
        sessions::sessions()
    }

    fn processes(&mut self) -> Vec<ProcessSeen> {
        processes::processes()
    }

    fn process(&mut self, pid: u32) -> Option<ProcessSeen> {
        processes::process(pid)
    }

    fn file_stamp(&mut self, path: &str) -> Option<u64> {
        processes::file_stamp(path)
    }

    fn version_names(&mut self, path: &str) -> VersionNames {
        processes::version_names(path)
    }

    fn start_helper(&mut self, session_id: u32) -> Result<HelperStarted, String> {
        let plain = self.settings().plain_display;
        let launched = launch::launch_helper(session_id, &self.exe, plain)?;
        self.next_id += 1;
        let id = HelperId(self.next_id);
        let events = self.events_tx.clone();
        let reading = spawn_reader::<FromHelper, _>("limo helper reader", launched.from_helper, move |incoming| {
            let received = match incoming {
                Incoming::Message(message) => Received::Message(message),
                Incoming::Broken(problem) => Received::Broken(problem),
                Incoming::Closed => Received::Closed,
            };
            events.send((id, received)).is_ok()
        });
        let writer = spawn_writer("limo helper writer", launched.to_helper, || {});
        let Some(writer) = writer.filter(|_| reading) else {
            // SAFETY: ends the helper just started, whose channel could not
            // be served.
            unsafe {
                let _ = TerminateProcess(launched.process.raw(), 0);
            }
            return Err("the threads that serve its channel could not be started".to_string());
        };
        self.helpers.insert(id, HelperProcess { process: launched.process, writer });
        Ok(HelperStarted { id, pid: launched.pid, detail: launched.detail, problems: launched.problems })
    }

    fn helper_running(&mut self, id: HelperId) -> bool {
        let Some(helper) = self.helpers.get(&id) else { return false };
        // SAFETY: waits not at all on a process handle owned here. Anything
        // but "still running" (it ended, or the wait failed) is taken as
        // ended, and the helper is started again.
        let state = unsafe { WaitForSingleObject(helper.process.raw(), 0) };
        state == WAIT_TIMEOUT
    }

    fn end_helper(&mut self, id: HelperId) {
        if let Some(helper) = self.helpers.remove(&id) {
            // SAFETY: ends LiMo's own helper, by the handle its launch gave.
            unsafe {
                let _ = TerminateProcess(helper.process.raw(), 0);
            }
            // Dropping the writer ends its thread and closes the service's
            // end of the channel; the reader ends when the helper's end
            // closes with the helper.
        }
    }

    fn send(&mut self, id: HelperId, frame: Frame) -> Result<(), SendProblem> {
        let Some(helper) = self.helpers.get(&id) else { return Err(SendProblem::Gone) };
        match helper.writer.try_send(frame) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(SendProblem::NotReading),
            Err(TrySendError::Disconnected(_)) => Err(SendProblem::Gone),
        }
    }

    fn received(&mut self) -> Vec<(HelperId, Received)> {
        self.events_rx.try_iter().collect()
    }

    fn end_process(&mut self, process: &ProcessSeen) -> Result<(), String> {
        processes::end_process(process)
    }

    fn disconnect_session(&mut self, session_id: u32) -> Result<(), String> {
        sessions::disconnect(session_id)
    }

    fn test_cap(&mut self) -> bool {
        self.settings().test_cap
    }

    fn log(&mut self, line: &str) {
        logfile::append(&self.places, line);
    }

    fn door_received(&mut self) -> Vec<(DoorId, DoorEvent)> {
        self.door.as_mut().map_or_else(Vec::new, RealDoor::received)
    }

    fn door_send(&mut self, id: DoorId, frame: Frame) -> Result<(), SendProblem> {
        match self.door.as_mut() {
            Some(door) => door.send(id, frame),
            None => Err(SendProblem::Gone),
        }
    }

    fn door_close(&mut self, id: DoorId) {
        if let Some(door) = self.door.as_mut() {
            door.close(id);
        }
    }

    fn accounts(&mut self) -> Result<Vec<AccountSeen>, String> {
        accounts::local_accounts()
    }
}
