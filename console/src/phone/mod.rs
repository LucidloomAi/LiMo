//! Adding a phone over the USB cable: the console's own requests for it,
//! answered here and never sent to the service.
//!
//! | Request | Arguments | What it does |
//! |---|---|---|
//! | `phone.look` | none | where adding the phone stands, in the words the Add a phone screens show; starts the looking |
//! | `phone.install` | none | starts putting LiMo on the connected phone |
//! | `phone.pair` | `child` | starts pairing the phone with that child |
//! | `phone.debugging_off` | none | starts switching USB debugging off on the phone |
//! | `phone.done` | none | lets go of the phone and stops looking |
//!
//! They drive limo-cable's state machine (`limo_cable::flow`) on a thread
//! of their own (`worker.rs`), so the window never waits for the phone: a
//! request starts a step and answers at once, and `phone.look`, which the
//! screens ask once a second, says where each step stands (`look.rs`).
//! README.md, "Adding a phone", has every answer.
//!
//! Pairing asks the service, through the door, for a pairing offer
//! (`make_offer`, a request only the console asks: see
//! [`crate::protocol::CONSOLE_ONLY`]), hands it to LiMo on the phone over
//! the cable and wipes it, carries the phone's 127.0.0.1:47810 to the
//! PC's, where the service's link listens, starts LiMo on the phone, and
//! holds the phone open until the service says the phone is paired
//! (`devices`, then `device_link`), and after, until USB debugging is off or
//! the guardian is done.

#![forbid(unsafe_code)]

mod look;
mod worker;

#[cfg(windows)]
mod counted;
#[cfg(windows)]
mod door_service;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

use limo_cable::key::AdbKey;
use limo_cable::open::Look;
use limo_cable::phone::{OpenProblem, Phone, Waits};
use limo_cable::protocol::Wire;
use serde_json::{Value, json};

use crate::protocol::wipe;
use look::{DebugStage, InstallStage, PairStage, Shared};
use worker::{Handle, Order};

#[cfg(windows)]
pub use counted::CountedCable;
#[cfg(windows)]
pub use door_service::DoorService;
pub use limo_cable::commands::LINK_PORT;

/// The console's own requests for a phone, by name.
pub const REQUESTS: [&str; 5] = ["phone.look", "phone.install", "phone.pair", "phone.debugging_off", "phone.done"];

/// True for a name the console answers itself, for a phone: never sent to
/// the service.
pub fn is_phone_request(name: &str) -> bool {
    name.starts_with("phone.")
}

/// The folder beside the program that holds LiMo's app for phones.
pub const APP_FOLDER: &str = "phone";
/// LiMo's app for phones (its child form, a release build), in that folder.
pub const APP_FILE: &str = "limo-child.apk";

/// Where LiMo's app for phones is kept for the program at `program`:
/// `phone\limo-child.apk` beside it.
pub fn app_beside(program: &Path) -> Option<PathBuf> {
    program.parent().map(|folder| folder.join(APP_FOLDER).join(APP_FILE))
}

/// The cable, as the console holds it: limo-cable's `Cable`, which the
/// console's thread owns, and a count of what was sent to the phone, which
/// the install's progress is read from.
pub trait ConsoleCable: Send {
    /// What is plugged in now. Reads only; opens nothing.
    fn look(&mut self) -> Look;
    /// Opens the one phone LiMo can open, and connects with `key`;
    /// `on_asking` is called when the phone asks the person to allow it.
    fn open(&mut self, key: &AdbKey, waits: Waits, on_asking: &mut dyn FnMut()) -> Result<Phone, OpenProblem>;
    /// The bytes sent to the phone so far, when this cable counts them.
    fn sent(&self) -> Option<Arc<AtomicU64>> {
        None
    }
}

/// A wire that counts the bytes it sends: what LiMo's install sends is the
/// APK, so the count is how much of it is on the phone.
pub struct CountingWire<W> {
    inner: W,
    sent: Arc<AtomicU64>,
}

impl<W: Wire> CountingWire<W> {
    /// `inner`, counting into `sent`.
    pub fn new(inner: W, sent: Arc<AtomicU64>) -> CountingWire<W> {
        CountingWire { inner, sent }
    }
}

impl<W: Wire> Wire for CountingWire<W> {
    fn packet(&self) -> usize {
        self.inner.packet()
    }

    fn read(&self, want: usize, wait: Duration) -> Result<Option<Vec<u8>>, String> {
        self.inner.read(want, wait)
    }

    fn write(&self, bytes: &[u8]) -> Result<(), String> {
        self.inner.write(bytes)?;
        self.sent.fetch_add(bytes.len() as u64, Ordering::Relaxed);
        Ok(())
    }
}

/// What the console asks of LiMo's service for a phone: through the door
/// ([`DoorService`]), or in tests a stand in.
pub trait Service: Send + Sync {
    /// Asks `name` with `args`: the answer's value, or what the service (or
    /// the door) said instead.
    fn ask(&self, name: &str, args: Value) -> Result<Value, Said>;
}

/// What the service said instead of an answer, or why it could not be
/// asked: its kind (`failed`, `signed_out`, `lost`...) and its sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Said {
    /// The kind, as the door names it.
    pub code: String,
    /// The sentence for the screen.
    pub words: String,
}

impl Said {
    /// A kind and its sentence.
    pub fn new(code: &str, words: &str) -> Said {
        Said { code: code.to_string(), words: words.to_string() }
    }
}

/// How often the console's thread does what it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pace {
    /// How often the cable is looked at: once a second, as the screens read it.
    pub look_every: Duration,
    /// How long a failed connection's words stay before the phone is tried
    /// again.
    pub retry_after: Duration,
    /// How long after the last request the console lets go of the phone and
    /// stops looking: the Add a phone screens ask every second while they
    /// are open.
    pub forget_after: Duration,
    /// How often the service is asked whether the phone has paired.
    pub ask_every: Duration,
    /// How often, once paired, the service is asked how the phone reaches
    /// the PC.
    pub watch_every: Duration,
    /// The longest a pairing is waited for, whatever the offer says.
    pub longest_pairing: Duration,
}

impl Default for Pace {
    /// A look a second, five seconds before trying again, a minute without
    /// a request before letting go, the service asked once a second while
    /// pairing and every two seconds after, and a pairing waited for at
    /// most as long as its offer lasts (ten minutes) and never over fifteen.
    fn default() -> Pace {
        Pace {
            look_every: Duration::from_secs(1),
            retry_after: Duration::from_secs(5),
            forget_after: Duration::from_secs(60),
            ask_every: Duration::from_secs(1),
            watch_every: Duration::from_secs(2),
            longest_pairing: Duration::from_secs(15 * 60),
        }
    }
}

/// What adding a phone is done with.
pub struct PhoneSetup {
    /// Makes the cable, on the console's thread, when the looking starts.
    pub cable: Arc<dyn Fn() -> Box<dyn ConsoleCable> + Send + Sync>,
    /// The service, through the door.
    pub service: Arc<dyn Service>,
    /// LiMo's app for phones ([`app_beside`]); None when the program's own
    /// place is not known.
    pub app: Option<PathBuf>,
    /// The folder of LiMo's adb key; None for its own folder in this
    /// account's local application data.
    pub key_folder: Option<PathBuf>,
    /// The port the phone reaches the service's link on, over the cable:
    /// [`LINK_PORT`]. Tests use a port of their own.
    pub link_port: u16,
    /// How long each call to the phone may take.
    pub waits: Waits,
    /// How often things are done.
    pub pace: Pace,
}

/// The console's desk for a phone: the five requests, and the thread that
/// holds the cable while they are asked.
pub struct PhoneDesk {
    setup: Arc<PhoneSetup>,
    inner: Mutex<DeskInner>,
}

#[derive(Default)]
struct DeskInner {
    worker: Option<Handle>,
    /// A thread told to stop that may still be ending a call: the next one
    /// waits for it, so two never hold the cable at once.
    ending: Option<JoinHandle<()>>,
}

fn problem(code: &str, words: &str) -> Value {
    json!({ "code": code, "words": words })
}

/// The child of `phone.pair`'s arguments: `{"child": "<its id>"}` and
/// nothing else.
fn child_of(args: &Value) -> Option<String> {
    let map = args.as_object()?;
    let child = map.get("child")?.as_str()?;
    let shaped = map.len() == 1 && (1..=100).contains(&child.chars().count()) && !child.chars().any(char::is_control);
    shaped.then(|| child.to_string())
}

impl PhoneDesk {
    /// A desk that starts looking at the first `phone.look`.
    pub fn new(setup: PhoneSetup) -> PhoneDesk {
        PhoneDesk { setup: Arc::new(setup), inner: Mutex::new(DeskInner::default()) }
    }

    fn inner(&self) -> MutexGuard<'_, DeskInner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// The running thread's way in and what it shares; with `start`, one
    /// is started when none runs.
    fn running(&self, start: bool) -> Option<(Sender<Order>, Arc<Mutex<Shared>>)> {
        let mut inner = self.inner();
        if let Some(handle) = &inner.worker
            && !handle.thread.is_finished()
        {
            return Some((handle.orders.clone(), Arc::clone(&handle.shared)));
        }
        if let Some(old) = inner.worker.take() {
            inner.ending = Some(old.thread);
        }
        if !start {
            return None;
        }
        let before = inner.ending.take();
        let handle = worker::start(Arc::clone(&self.setup), before)?;
        let out = (handle.orders.clone(), Arc::clone(&handle.shared));
        inner.worker = Some(handle);
        Some(out)
    }

    /// Answers one of [`REQUESTS`]: its value, or `{code, words}` with the
    /// sentence the screen shows. Arguments are wiped once read.
    pub fn request(&self, name: &str, mut args: Value) -> Result<Value, Value> {
        let child = if name == "phone.pair" { child_of(&args) } else { None };
        let takes_nothing = args.is_null() || args.as_object().is_some_and(|m| m.is_empty());
        wipe(&mut args);
        match name {
            "phone.look" if takes_nothing => self.look(),
            "phone.install" if takes_nothing => self.install(),
            "phone.pair" => match child {
                Some(child) => self.pair(child),
                None => Err(problem("unknown_request", "phone.pair takes the child, as {\"child\": \"<its id>\"}, and nothing else")),
            },
            "phone.debugging_off" if takes_nothing => self.debugging_off(),
            "phone.done" if takes_nothing => Ok(self.done()),
            known if REQUESTS.contains(&known) => Err(problem("unknown_request", &format!("{known} takes nothing"))),
            _ => Err(problem("unknown_request", "that is not the name of a request")),
        }
    }

    fn look(&self) -> Result<Value, Value> {
        let Some((orders, shared)) = self.running(true) else {
            return Err(problem("failed", "The console could not start looking at the USB ports. Close it and open it again."));
        };
        let _ = orders.send(Order::Look);
        Ok(look::lock(&shared).answer())
    }

    /// The thread's way in and what it shares, or the words for "no phone".
    fn connected(&self) -> Result<(Sender<Order>, Arc<Mutex<Shared>>), Value> {
        self.running(false).ok_or_else(|| problem("no_phone", look::NO_PHONE))
    }

    fn install(&self) -> Result<Value, Value> {
        let (orders, shared) = self.connected()?;
        let mut s = look::lock(&shared);
        if let Some(words) = &s.app_problem {
            return Err(problem("no_app", &words.line));
        }
        s.may_act()?;
        if s.limo.is_none() || s.busy.is_some() {
            return Err(problem("busy", look::BUSY));
        }
        match s.install.stage {
            InstallStage::Done | InstallStage::NotNeeded => return Ok(json!({ "started": false })),
            InstallStage::Sending => return Err(problem("busy", look::BUSY)),
            _ => {}
        }
        s.install.stage = InstallStage::Sending;
        s.install.base = None;
        s.install.words = None;
        drop(s);
        let _ = orders.send(Order::Install);
        Ok(json!({ "started": true }))
    }

    fn pair(&self, child: String) -> Result<Value, Value> {
        let (orders, shared) = self.connected()?;
        let mut s = look::lock(&shared);
        s.may_act()?;
        match s.pairing.stage {
            PairStage::Paired => return Ok(json!({ "started": false })),
            PairStage::AskingService | PairStage::Handing | PairStage::Waiting => return Err(problem("busy", look::BUSY)),
            PairStage::NotAsked | PairStage::Failed => {}
        }
        if !matches!(s.install.stage, InstallStage::Done | InstallStage::NotNeeded) {
            return Err(problem("not_yet", look::NOT_INSTALLED));
        }
        if s.busy.is_some() {
            return Err(problem("busy", look::BUSY));
        }
        s.pairing = look::Pairing { stage: PairStage::AskingService, ..look::Pairing::default() };
        drop(s);
        let _ = orders.send(Order::Pair { child });
        Ok(json!({ "started": true }))
    }

    fn debugging_off(&self) -> Result<Value, Value> {
        let (orders, shared) = self.connected()?;
        let mut s = look::lock(&shared);
        if s.debugging == DebugStage::Off {
            return Ok(json!({ "started": false }));
        }
        s.may_act()?;
        if s.busy.is_some() || s.debugging == DebugStage::SwitchingOff {
            return Err(problem("busy", look::BUSY));
        }
        s.debugging = DebugStage::SwitchingOff;
        drop(s);
        let _ = orders.send(Order::DebuggingOff);
        Ok(json!({ "started": true }))
    }

    fn done(&self) -> Value {
        let mut inner = self.inner();
        if let Some(handle) = inner.worker.take() {
            let _ = handle.orders.send(Order::Done);
            inner.ending = Some(handle.thread);
        }
        json!({ "done": true })
    }
}

impl Drop for PhoneDesk {
    /// Lets go of the phone, and waits for the thread to end.
    fn drop(&mut self) {
        self.done();
        if let Some(thread) = self.inner().ending.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dummy(Mutex<Vec<u8>>);

    impl Wire for Dummy {
        fn packet(&self) -> usize {
            512
        }
        fn read(&self, _want: usize, _wait: Duration) -> Result<Option<Vec<u8>>, String> {
            Ok(None)
        }
        fn write(&self, bytes: &[u8]) -> Result<(), String> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(())
        }
    }

    #[test]
    fn the_counting_wire_counts_what_it_sends_and_passes_the_rest_on() {
        let sent = Arc::new(AtomicU64::new(0));
        let wire = CountingWire::new(Dummy(Mutex::new(Vec::new())), Arc::clone(&sent));
        wire.write(&[1; 24]).unwrap();
        wire.write(&[2; 1000]).unwrap();
        assert_eq!(sent.load(Ordering::Relaxed), 1024);
        assert_eq!(wire.packet(), 512);
        assert_eq!(wire.read(512, Duration::ZERO), Ok(None));
        assert_eq!(wire.inner.0.lock().unwrap().len(), 1024);
    }

    #[test]
    fn the_phone_requests_are_the_console_s_own_and_take_what_they_say() {
        for name in REQUESTS {
            assert!(is_phone_request(name), "{name}");
        }
        assert!(!is_phone_request("make_offer") && !is_phone_request("devices"));
        assert_eq!(child_of(&json!({ "child": "c1" })), Some("c1".to_string()));
        for bad in [json!(null), json!({}), json!({ "child": "" }), json!({ "child": 4 }), json!({ "child": "c1", "more": 1 }), json!({ "child": "a\nb" })] {
            assert_eq!(child_of(&bad), None, "{bad}");
        }
        assert_eq!(app_beside(Path::new(r"C:\Program Files\LiMo\limo-console.exe")), Some(PathBuf::from(r"C:\Program Files\LiMo\phone\limo-child.apk")));
    }
}
