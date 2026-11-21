//! The fake PC: a fake [`Os`] with sessions, processes, program files,
//! helpers and their channels, and a clock of its own; and a harness around
//! the real service, with a real family file in a temporary folder.

use std::collections::{BTreeMap, BTreeSet};
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use limo_agent::{Session, SessionState, Show};
use limo_core::{CivilDate, DailyBudget, RuleSet, WeekBudgets};
use limo_hub::{AccountLink, AgeBand, Hub};
use limo_link::{MasterId, Now};
use limo_windows::RealDoor;
use limo_windows::announce::Announce;
use limo_windows::phones::{LinkClock, LinkServer};
use limo_windows::console::{
    CONSOLE_PROTOCOL_VERSION, ConsoleIncoming, ConsoleReader, FromConsole, Reply, Request, ToConsole, answer_decoder, encode_from_console,
};
use limo_windows::os::{
    AccountSeen, ClientSeen, DoorEvent, DoorId, HelperId, HelperStarted, Os, ProcessSeen, Received, SendProblem, SessionSeen, VersionNames,
};
use limo_windows::protocol::{Decoder, Frame, FromHelper, PROTOCOL_VERSION, Secret, ToHelper, encode};
use limo_windows::service::{STEP_MS, Service, ServiceConfig};
use tempfile::TempDir;

/// India, the home zone of every story.
pub const IST: i32 = 19_800;
/// The guardian's password in every story.
pub const PASSWORD: &str = "correct horse battery";
/// The guardian's PIN in every story.
pub const PIN: &str = "482915";
/// LiMo's own program, as installed.
pub const OWN_EXE: &str = r"C:\Program Files\LiMo\limo.exe";
/// The service's own process id.
pub const OWN_PID: u32 = 400;
/// The Windows folder.
pub const WINDOWS: &str = r"C:\Windows";

/// Wednesday 18 November 2026 at `hour:minute`, home time.
pub fn at(hour: u8, minute: u8) -> i64 {
    RuleSet::new(1, IST).calendar().at(CivilDate::new(2026, 11, 18).unwrap(), hour, minute, 0)
}

/// A helper on the fake PC.
#[derive(Debug)]
pub struct FakeHelper {
    pub id: HelperId,
    pub session: u32,
    pub pid: u32,
    pub running: bool,
    /// Everything the service sent it, read from its frames.
    pub got: Vec<ToHelper>,
    /// True to make it stop reading what it is sent.
    pub not_reading: bool,
    decoder: Decoder,
}

/// The fake Windows.
pub struct FakeOs {
    pub wall_ms: i64,
    pub steady_ms: u64,
    pub offset: i32,
    pub sessions: Vec<SessionSeen>,
    pub processes: Vec<ProcessSeen>,
    /// Processes that started after the list was taken: found by their id
    /// alone.
    pub not_listed: BTreeSet<u32>,
    pub versions: BTreeMap<String, VersionNames>,
    pub stamps: BTreeMap<String, u64>,
    pub version_reads: Vec<String>,
    pub helpers: Vec<FakeHelper>,
    /// Bytes the helpers wrote, not yet read by the service.
    pub from_helpers: Vec<(HelperId, Vec<u8>)>,
    /// Channels that closed, not yet seen by the service.
    pub closed: Vec<HelperId>,
    decoders: BTreeMap<HelperId, Option<Decoder>>,
    pub launch_fails: Option<String>,
    pub launches: u32,
    pub ended: Vec<ProcessSeen>,
    pub refuse_end: BTreeSet<u32>,
    pub disconnected: Vec<u32>,
    pub test_cap: bool,
    pub log: Vec<String>,
    next_id: u64,
    next_pid: u32,
    /// What came to the console's door, not yet seen by the service.
    pub door_in: Vec<(DoorId, DoorEvent)>,
    /// Everything the service sent each console, read from its frames.
    pub door_got: BTreeMap<DoorId, Vec<ToConsole>>,
    /// Connections the service closed.
    pub door_closed: Vec<DoorId>,
    /// Consoles that stopped reading what they are sent.
    pub door_not_reading: BTreeSet<DoorId>,
    door_readers: BTreeMap<DoorId, Option<ConsoleReader>>,
    door_decoders: BTreeMap<DoorId, Decoder>,
    next_door: u64,
    next_ask: u32,
    /// The accounts of this PC.
    pub accounts: Vec<AccountSeen>,
    /// When set, listing the accounts fails with this.
    pub accounts_fail: Option<String>,
    /// A real door on a named pipe, in place of the fake one.
    pub real_door: Option<RealDoor>,
    /// The PC's home addresses, for a pairing offer.
    pub home: Vec<Ipv4Addr>,
}

impl FakeOs {
    pub fn new(start: i64) -> FakeOs {
        FakeOs {
            wall_ms: start * 1_000,
            steady_ms: 9_000_000,
            offset: IST,
            sessions: Vec::new(),
            processes: Vec::new(),
            not_listed: BTreeSet::new(),
            versions: BTreeMap::new(),
            stamps: BTreeMap::new(),
            version_reads: Vec::new(),
            helpers: Vec::new(),
            from_helpers: Vec::new(),
            closed: Vec::new(),
            decoders: BTreeMap::new(),
            launch_fails: None,
            launches: 0,
            ended: Vec::new(),
            refuse_end: BTreeSet::new(),
            disconnected: Vec::new(),
            test_cap: false,
            log: Vec::new(),
            next_id: 0,
            next_pid: 5_000,
            door_in: Vec::new(),
            door_got: BTreeMap::new(),
            door_closed: Vec::new(),
            door_not_reading: BTreeSet::new(),
            door_readers: BTreeMap::new(),
            door_decoders: BTreeMap::new(),
            next_door: 0,
            next_ask: 0,
            accounts: Vec::new(),
            accounts_fail: None,
            real_door: None,
            home: vec![Ipv4Addr::new(192, 168, 1, 20)],
        }
    }

    pub fn new_pid(&mut self) -> u32 {
        self.next_pid += 4;
        self.next_pid
    }
}

impl Os for FakeOs {
    fn wall_secs(&mut self) -> i64 {
        self.wall_ms.div_euclid(1_000)
    }

    fn steady_ms(&mut self) -> u64 {
        self.steady_ms
    }

    fn utc_offset_secs(&mut self) -> i32 {
        self.offset
    }

    fn sessions(&mut self) -> Vec<SessionSeen> {
        self.sessions.clone()
    }

    fn processes(&mut self) -> Vec<ProcessSeen> {
        self.processes.iter().filter(|p| !self.not_listed.contains(&p.pid)).cloned().collect()
    }

    fn process(&mut self, pid: u32) -> Option<ProcessSeen> {
        self.processes.iter().find(|p| p.pid == pid).cloned()
    }

    fn file_stamp(&mut self, path: &str) -> Option<u64> {
        self.stamps.get(path).copied()
    }

    fn version_names(&mut self, path: &str) -> VersionNames {
        self.version_reads.push(path.to_string());
        self.versions.get(path).cloned().unwrap_or_default()
    }

    fn start_helper(&mut self, session_id: u32) -> Result<HelperStarted, String> {
        self.launches += 1;
        if let Some(problem) = &self.launch_fails {
            return Err(problem.clone());
        }
        self.next_id += 1;
        let id = HelperId(self.next_id);
        let pid = self.new_pid();
        self.helpers.push(FakeHelper { id, session: session_id, pid, running: true, got: Vec::new(), not_reading: false, decoder: Decoder::new() });
        self.processes.push(ProcessSeen { pid, session_id, started: 77, name: "limo.exe".into(), path: OWN_EXE.into() });
        Ok(HelperStarted { id, pid, detail: "uiaccess=1 integrity=set dacl=admin-owner channel=inherited".into(), problems: Vec::new() })
    }

    fn helper_running(&mut self, id: HelperId) -> bool {
        self.helpers.iter().any(|h| h.id == id && h.running)
    }

    fn end_helper(&mut self, id: HelperId) {
        if let Some(helper) = self.helpers.iter_mut().find(|h| h.id == id) {
            helper.running = false;
            let pid = helper.pid;
            self.processes.retain(|p| p.pid != pid);
        }
    }

    fn send(&mut self, id: HelperId, frame: Frame) -> Result<(), SendProblem> {
        let Some(helper) = self.helpers.iter_mut().find(|h| h.id == id && h.running) else { return Err(SendProblem::Gone) };
        if helper.not_reading {
            return Err(SendProblem::NotReading);
        }
        helper.decoder.push(&frame).unwrap();
        while let Some(message) = helper.decoder.next_message::<ToHelper>().unwrap() {
            helper.got.push(message);
        }
        Ok(())
    }

    fn received(&mut self) -> Vec<(HelperId, Received)> {
        let mut out = Vec::new();
        for (id, bytes) in std::mem::take(&mut self.from_helpers) {
            let slot = self.decoders.entry(id).or_insert_with(|| Some(Decoder::new()));
            // The real reader stops at the first problem: so does this one.
            let Some(decoder) = slot.as_mut() else { continue };
            if let Err(problem) = decoder.push(&bytes) {
                out.push((id, Received::Broken(problem)));
                *slot = None;
                continue;
            }
            loop {
                match decoder.next_message::<FromHelper>() {
                    Ok(Some(message)) => out.push((id, Received::Message(message))),
                    Ok(None) => break,
                    Err(problem) => {
                        out.push((id, Received::Broken(problem)));
                        *slot = None;
                        break;
                    }
                }
            }
        }
        for id in std::mem::take(&mut self.closed) {
            out.push((id, Received::Closed));
        }
        out
    }

    fn end_process(&mut self, process: &ProcessSeen) -> Result<(), String> {
        if self.refuse_end.contains(&process.pid) {
            // As the real one gives it: Windows' text with no full stop.
            return Err("Win32 error 5: Access is denied".into());
        }
        self.ended.push(process.clone());
        self.processes.retain(|p| p.pid != process.pid);
        Ok(())
    }

    fn disconnect_session(&mut self, session_id: u32) -> Result<(), String> {
        self.disconnected.push(session_id);
        if let Some(session) = self.sessions.iter_mut().find(|s| s.session_id == session_id) {
            session.state = SessionState::Disconnected;
        }
        Ok(())
    }

    fn test_cap(&mut self) -> bool {
        self.test_cap
    }

    fn log(&mut self, line: &str) {
        self.log.push(line.to_string());
    }

    fn door_received(&mut self) -> Vec<(DoorId, DoorEvent)> {
        let mut out = self.real_door.as_mut().map_or_else(Vec::new, RealDoor::received);
        out.extend(std::mem::take(&mut self.door_in));
        out
    }

    fn door_send(&mut self, id: DoorId, frame: Frame) -> Result<(), SendProblem> {
        if let Some(door) = self.real_door.as_mut() {
            return door.send(id, frame);
        }
        if self.door_closed.contains(&id) || !self.door_readers.contains_key(&id) {
            return Err(SendProblem::Gone);
        }
        if self.door_not_reading.contains(&id) {
            return Err(SendProblem::NotReading);
        }
        let decoder = self.door_decoders.entry(id).or_insert_with(answer_decoder);
        decoder.push(&frame).unwrap();
        while let Some(message) = decoder.next_message::<ToConsole>().unwrap() {
            self.door_got.entry(id).or_default().push(message);
        }
        Ok(())
    }

    fn door_close(&mut self, id: DoorId) {
        if let Some(door) = self.real_door.as_mut() {
            door.close(id);
        }
        self.door_closed.push(id);
    }

    fn accounts(&mut self) -> Result<Vec<AccountSeen>, String> {
        match &self.accounts_fail {
            Some(problem) => Err(problem.clone()),
            None => Ok(self.accounts.clone()),
        }
    }

    fn home_addresses(&mut self) -> Vec<Ipv4Addr> {
        self.home.clone()
    }
}

/// The fake PC's clocks, shared with the link's threads: they move only
/// when the story steps.
#[derive(Debug, Clone, Default)]
pub struct SharedClock {
    ms: Arc<AtomicU64>,
    unix: Arc<AtomicI64>,
}

impl SharedClock {
    pub fn set(&self, ms: u64, unix: i64) {
        self.ms.store(ms, Ordering::SeqCst);
        self.unix.store(unix, Ordering::SeqCst);
    }
}

impl LinkClock for SharedClock {
    fn now(&self) -> Now {
        Now { ms: self.ms.load(Ordering::SeqCst), unix: self.unix.load(Ordering::SeqCst) }
    }
}

/// An announcer that only notes what it was asked.
#[derive(Debug, Clone, Default)]
pub struct NotedAnnouncer {
    pub said: Arc<Mutex<Vec<String>>>,
}

impl Announce for NotedAnnouncer {
    fn announce(&mut self, id: &MasterId, port: u16) -> Result<(), String> {
        self.said.lock().unwrap().push(format!("announce {} {port}", id.to_hex()));
        Ok(())
    }

    fn withdraw(&mut self) {
        self.said.lock().unwrap().push("withdraw".into());
    }
}

impl FakeOs {
    /// A console connects from `account` in `session` and sends its first
    /// bytes: Windows tells the service who it is.
    pub fn connect(&mut self, account: Option<&str>, session: u32) -> DoorId {
        self.next_door += 1;
        let id = DoorId(self.next_door);
        self.door_readers.insert(id, Some(ConsoleReader::new()));
        let pid = self.new_pid();
        let client = ClientSeen { account_id: account.map(String::from), session_id: Some(session), pid: Some(pid) };
        self.door_in.push((id, DoorEvent::Opened(client)));
        id
    }

    /// Bytes a console writes, read as the real connection's thread reads
    /// them: it stops at the first problem.
    pub fn console_raw(&mut self, id: DoorId, bytes: &[u8]) {
        let Some(slot) = self.door_readers.get_mut(&id) else { return };
        let Some(reader) = slot.as_mut() else { return };
        if let Err(problem) = reader.push(bytes) {
            *slot = None;
            self.door_in.push((id, DoorEvent::Broken(problem)));
            return;
        }
        loop {
            match reader.next_frame() {
                Ok(Some(ConsoleIncoming::Message(message))) => self.door_in.push((id, DoorEvent::Message(message))),
                Ok(Some(ConsoleIncoming::BadRequest { id: ask })) => self.door_in.push((id, DoorEvent::BadRequest { id: ask })),
                Ok(None) => break,
                Err(problem) => {
                    *slot = None;
                    self.door_in.push((id, DoorEvent::Broken(problem)));
                    break;
                }
            }
        }
    }

    /// The console closes its end.
    pub fn disconnect(&mut self, id: DoorId) {
        self.door_readers.remove(&id);
        self.door_in.push((id, DoorEvent::Closed));
    }
}

/// The fake PC with the real service on it.
pub struct Pc {
    _dir: TempDir,
    pub service: Service<FakeOs>,
    /// The clocks the link's threads read.
    pub clock: SharedClock,
    /// What the announcer was asked.
    pub announced: Arc<Mutex<Vec<String>>>,
}

impl Pc {
    /// A PC with a family set up, at `start`.
    pub fn new(start: i64) -> Pc {
        Pc::with_family(start, true)
    }

    /// A PC whose family's file is empty: LiMo just installed, no family.
    pub fn empty(start: i64) -> Pc {
        Pc::with_family(start, false)
    }

    fn with_family(start: i64, family: bool) -> Pc {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = Hub::open(&dir.path().join("family.limo")).unwrap();
        if family {
            hub.create_family(PASSWORD, start).unwrap();
        }
        let config = ServiceConfig {
            pc_name: "Family PC".into(),
            own_pid: OWN_PID,
            own_exe: OWN_EXE.into(),
            windows_dir: WINDOWS.into(),
        };
        let mut os = FakeOs::new(start);
        os.processes.push(ProcessSeen { pid: OWN_PID, session_id: 0, started: 1, name: "limo.exe".into(), path: OWN_EXE.into() });
        let clock = SharedClock::default();
        clock.set(os.steady_ms, os.wall_ms.div_euclid(1_000));
        Pc { _dir: dir, service: Service::new(os, hub, config), clock, announced: Arc::new(Mutex::new(Vec::new())) }
    }

    /// Starts the link on the loopback address, at a port the system
    /// chooses; its address. Its threads read the fake PC's clocks.
    pub fn with_link(&mut self) -> SocketAddr {
        let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let server = LinkServer::start(bind, Arc::new(self.clock.clone()), Arc::new(|| {})).unwrap();
        let address = server.local_addr();
        let announcer = NotedAnnouncer { said: self.announced.clone() };
        self.service.open_link(server, Box::new(announcer));
        address
    }

    /// The clocks as the link's threads will read them now.
    pub fn link_now(&mut self) -> Now {
        let os = self.os();
        Now { ms: os.steady_ms, unix: os.wall_ms.div_euclid(1_000) }
    }

    pub fn os(&mut self) -> &mut FakeOs {
        self.service.os()
    }

    pub fn hub(&mut self) -> &mut Hub {
        self.service.hub()
    }

    pub fn now(&mut self) -> i64 {
        self.os().wall_ms.div_euclid(1_000)
    }

    /// A child with `minutes` a day, linked to a Windows account.
    pub fn add_child(&mut self, name: &str, account: &str, minutes: u32) -> String {
        let now = self.now();
        let child = self.hub().add_child(name, AgeBand::TenToTwelve, IST, now).unwrap().child_id;
        self.hub().link_account(account, name, AccountLink::Child { child_id: child.clone() }).unwrap();
        self.change_rules(&child, |rules| rules.budgets = WeekBudgets::every_day(DailyBudget::Minutes(minutes)));
        child
    }

    pub fn change_rules(&mut self, child: &str, change: impl FnOnce(&mut RuleSet)) {
        let now = self.now();
        let mut rules = self.hub().rules(child).unwrap();
        change(&mut rules);
        self.hub().replace_rules(child, rules, now).unwrap();
    }

    /// Someone signs in to `account` in `session`, and it is the one in use.
    pub fn login(&mut self, session: u32, account: &str, name: &str) {
        self.os().sessions.push(SessionSeen {
            session_id: session,
            account_id: account.into(),
            account_name: name.into(),
            state: SessionState::InUse,
            is_administrator: false,
        });
    }

    pub fn set_state(&mut self, session: u32, state: SessionState) {
        self.os().sessions.iter_mut().find(|s| s.session_id == session).unwrap().state = state;
    }

    /// A program file, with what its version resource says, and no
    /// description.
    pub fn file(&mut self, path: &str, original: Option<&str>, maker: Option<&str>) {
        let os = self.os();
        let names = VersionNames { original_name: original.map(Into::into), maker: maker.map(Into::into), description: None };
        os.versions.insert(path.into(), names);
        os.stamps.insert(path.into(), 1);
    }

    /// A program file, with what its version resource says, its description
    /// among it.
    pub fn described_file(&mut self, path: &str, original: Option<&str>, maker: Option<&str>, description: &str) {
        self.file(path, original, maker);
        if let Some(names) = self.os().versions.get_mut(path) {
            names.description = Some(description.into());
        }
    }

    /// Starts a program from `path` in `session`; its process id.
    pub fn start(&mut self, session: u32, path: &str) -> u32 {
        let os = self.os();
        let pid = os.new_pid();
        let name = path.rsplit('\\').next().unwrap().to_string();
        os.processes.push(ProcessSeen { pid, session_id: session, started: 1_000 + u64::from(pid), name, path: path.into() });
        pid
    }

    /// One step, 100 ms later, and the door's turn after it, as the
    /// service's loop does.
    pub fn step(&mut self) {
        let os = self.os();
        os.steady_ms += STEP_MS;
        os.wall_ms += STEP_MS as i64;
        let (ms, unix) = (os.steady_ms, os.wall_ms.div_euclid(1_000));
        self.clock.set(ms, unix);
        self.service.step();
        self.service.door();
        self.service.link();
    }

    /// What the link's threads asked meanwhile, answered with no step: the
    /// pump of a played phone between two steps.
    pub fn answer_link(&mut self) -> Now {
        self.service.link();
        self.link_now()
    }

    /// Steps for `ms` milliseconds.
    pub fn run_ms(&mut self, ms: u64) {
        for _ in 0..ms / STEP_MS {
            self.step();
        }
    }

    /// Steps for `secs` seconds.
    pub fn run(&mut self, secs: u64) {
        self.run_ms(secs * 1_000);
    }

    /// The running helper of a session.
    pub fn helper(&mut self, session: u32) -> Option<&mut FakeHelper> {
        self.os().helpers.iter_mut().rev().find(|h| h.session == session && h.running)
    }

    pub fn helper_id(&mut self, session: u32) -> HelperId {
        self.helper(session).unwrap().id
    }

    /// How many helpers were ever started in a session.
    pub fn helpers_started(&mut self, session: u32) -> usize {
        self.os().helpers.iter().filter(|h| h.session == session).count()
    }

    /// How many helpers run in a session now.
    pub fn helpers_running(&mut self, session: u32) -> usize {
        self.os().helpers.iter().filter(|h| h.session == session && h.running).count()
    }

    /// Bytes the session's helper writes to its channel.
    pub fn write_raw(&mut self, session: u32, bytes: Vec<u8>) {
        let id = self.helper_id(session);
        self.os().from_helpers.push((id, bytes));
    }

    /// A message the session's helper sends.
    pub fn say(&mut self, session: u32, message: FromHelper) {
        let frame = encode(&message).unwrap();
        self.write_raw(session, frame.to_vec());
    }

    /// The session's helper says hello.
    pub fn greet(&mut self, session: u32) {
        self.say(session, FromHelper::Hello { protocol: PROTOCOL_VERSION });
    }

    /// The session's helper reports, with no sound and nothing keeping the
    /// screen awake.
    pub fn report(&mut self, session: u32, idle_ms: u64, front_pid: Option<u32>, full_screen: bool) {
        self.report_playing(session, idle_ms, front_pid, full_screen, false, false);
    }

    /// The session's helper reports, saying whether a program of the
    /// session plays sound and whether a program keeps the screen awake.
    pub fn report_playing(
        &mut self,
        session: u32,
        idle_ms: u64,
        front_pid: Option<u32>,
        full_screen: bool,
        playing_sound: bool,
        screen_kept_awake: bool,
    ) {
        self.say(session, FromHelper::Report { idle_ms, front_pid, full_screen, playing_sound, screen_kept_awake });
    }

    /// The last show the session's running helper got.
    pub fn last_show(&mut self, session: u32) -> Option<(u64, Option<String>, Show)> {
        self.helper(session)?.got.iter().rev().find_map(|m| match m {
            ToHelper::Show { seq, child_name, show } => Some((*seq, child_name.clone(), show.clone())),
            _ => None,
        })
    }

    /// The facts the engine was given for a session at the last call.
    pub fn facts_of(&self, session: u32) -> Option<Session> {
        self.service.last_facts()?.sessions.iter().find(|s| s.session_id == session).cloned()
    }

    /// Seconds counted today on this PC for a child.
    pub fn counted(&self, child: &str) -> u64 {
        self.service.engine().child_on_pc(child).map_or(0, |c| c.counted_secs)
    }

    /// The log lines containing `text`.
    pub fn logged(&mut self, text: &str) -> Vec<String> {
        self.os().log.iter().filter(|l| l.contains(text)).cloned().collect()
    }

    /// The helper confirms the lock it was last sent.
    pub fn confirm_lock(&mut self, session: u32) {
        let (seq, _, show) = self.last_show(session).unwrap();
        assert!(matches!(show, Show::Locked { .. }), "{show:?}");
        self.say(session, FromHelper::LockOnScreen { seq });
    }
}

// --------------------------------------------------------------------------
// A console at the door
// --------------------------------------------------------------------------

/// A Windows account of the PC.
pub fn account(account_id: &str, name: &str, is_administrator: bool) -> AccountSeen {
    AccountSeen { account_id: account_id.into(), name: name.into(), full_name: None, is_administrator }
}

impl Pc {
    /// A console connects from `account` in `session`.
    pub fn console(&mut self, account: &str, session: u32) -> DoorId {
        self.os().connect(Some(account), session)
    }

    /// A message from a console, as a frame.
    pub fn console_say(&mut self, id: DoorId, message: &FromConsole) {
        let frame = encode_from_console(message).unwrap();
        self.os().console_raw(id, &frame);
    }

    /// Bytes from a console, whatever they are.
    pub fn console_raw(&mut self, id: DoorId, bytes: &[u8]) {
        self.os().console_raw(id, bytes);
    }

    /// The console says hello; the service's last word to it.
    pub fn hello(&mut self, id: DoorId) -> Option<ToConsole> {
        self.console_say(id, &FromConsole::Hello { protocol: CONSOLE_PROTOCOL_VERSION });
        self.step();
        self.console_got(id).last().cloned()
    }

    /// Everything a console got.
    pub fn console_got(&mut self, id: DoorId) -> Vec<ToConsole> {
        self.os().door_got.get(&id).cloned().unwrap_or_default()
    }

    /// True when the service closed the connection.
    pub fn console_closed(&mut self, id: DoorId) -> bool {
        self.os().door_closed.contains(&id)
    }

    /// A request, with a number of its own; one step; its answer, if it
    /// came.
    pub fn ask(&mut self, id: DoorId, request: Request) -> Option<Reply> {
        let ask = self.next_ask();
        self.console_say(id, &FromConsole::Request { id: ask, request });
        self.step();
        self.answer_to(id, ask)
    }

    /// The answer to the request numbered `ask`, if it came.
    pub fn answer_to(&mut self, id: DoorId, ask: u32) -> Option<Reply> {
        self.console_got(id).into_iter().find_map(|m| match m {
            ToConsole::Answer { id, reply } if id == ask => Some(reply),
            _ => None,
        })
    }

    /// A number for a request, never the same twice in a story.
    pub fn next_ask(&mut self) -> u32 {
        let os = self.os();
        os.next_ask += 1;
        os.next_ask
    }

    /// A request that must be done; what it gave back.
    pub fn done(&mut self, id: DoorId, request: Request) -> serde_json::Value {
        let name = request.name();
        match self.ask(id, request) {
            Some(Reply::Done { value }) => value,
            other => panic!("{name}: {other:?}"),
        }
    }

    /// A console signs in with the right password, a second after the
    /// last secret it sent.
    pub fn sign_in(&mut self, id: DoorId) {
        self.run(1);
        self.done(id, Request::SignIn { password: Secret::new(PASSWORD.into()) });
    }
}
