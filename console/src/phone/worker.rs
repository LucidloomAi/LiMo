//! The console's thread for a phone: it owns the cable and, once connected,
//! the phone, and does each step of adding the phone in turn, writing where
//! it stands into what the requests share (`look.rs`). Every call to the
//! phone and every question to the service happens here, never on a
//! request's own thread, so a request never waits for the phone.
//!
//! Once a second it looks at the USB ports and hands the look to limo-cable's
//! state machine; what the machine offers next it does when that step is
//! one it does by itself (connect, look whether LiMo is on the phone, carry
//! the link's port again after the phone came back) or one a request asked
//! for (install, pair, switch USB debugging off). While a pairing waits, it
//! asks the service once a second whether the phone has paired.

#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use limo_cable::apk::{ApkProblem, read_apk};
use limo_cable::commands::{Installed, PACKAGE, Started, install_advice};
use limo_cable::flow::{self, Event, Next, Problem, State};
use limo_cable::key::{AdbKey, KeyError};
use limo_cable::phone::{CallError, DebuggingOff, InstallError, Knock, LimoOnPhone, OpenProblem, Phone};
use serde_json::{Value, json};
use zeroize::Zeroizing;

use super::look::{self, DebugStage, Device, InstallStage, PairStage, Shared, Words, typeset};
use super::{ConsoleCable, PhoneSetup, Said};
use crate::protocol::wipe;

/// What a request asks of the thread.
pub(crate) enum Order {
    /// The screen looked: keep looking.
    Look,
    /// Ask the phone, once, to allow this PC.
    Ask,
    /// Put LiMo on the phone.
    Install,
    /// Pair the phone with this child.
    Pair { child: String },
    /// Switch USB debugging off.
    DebuggingOff,
    /// Let go of the phone and end.
    Done,
}

/// The running thread: its way in, what it shares, and the thread itself.
pub(crate) struct Handle {
    pub orders: Sender<Order>,
    pub shared: Arc<Mutex<Shared>>,
    pub thread: JoinHandle<()>,
    /// Set to end an ask that waits for the phone, at once.
    pub stop: Arc<AtomicBool>,
}

/// Starts the thread. It first waits for `before`, a thread that was told
/// to end and may still be ending a call, so that two never hold the cable.
pub(crate) fn start(setup: Arc<PhoneSetup>, before: Option<JoinHandle<()>>) -> Option<Handle> {
    let (orders, inbox) = mpsc::channel();
    let shared = Arc::new(Mutex::new(Shared::default()));
    let theirs = Arc::clone(&shared);
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = Arc::clone(&stop);
    let thread = std::thread::Builder::new()
        .name("limo phone".to_string())
        .spawn(move || {
            if let Some(before) = before {
                let _ = before.join();
            }
            let cable = (setup.cable)();
            Worker::new(setup, cable, theirs, inbox, stopping).run();
        })
        .ok()?;
    Some(Handle { orders, shared, thread, stop })
}

/// A pairing handed to the phone, waited for.
struct Watch {
    child: String,
    /// The child's devices before the offer was made.
    before: Vec<String>,
    /// When the offer runs out.
    until: Instant,
    next_ask: Instant,
    /// The device the service paired, once it has.
    device: Option<String>,
}

/// Why a step failed: the words, and whether the phone is gone.
struct Failure {
    words: Words,
    lost: bool,
}

impl Failure {
    fn said(said: Said) -> Failure {
        Failure { words: Words { line: said.words, advice: None }, lost: false }
    }

    fn call(error: CallError) -> Failure {
        match error {
            CallError::Lost(_) => Failure { words: Words::new(LOST, Some(PLUG_IN_AGAIN)), lost: true },
            CallError::Timeout(_) => Failure { words: Words::new("The phone did not answer in time.", Some(TRY_AGAIN)), lost: false },
            CallError::Failed(_) => Failure { words: Words::new("The phone did not answer as expected.", Some(TRY_AGAIN)), lost: false },
        }
    }
}

const LOST: &str = "The phone\u{2019}s connection ended.";
pub(crate) const PLUG_IN_AGAIN: &str = "Plug the phone in again, unlocked, then try again.";
/// Said when LiMo was being put on the phone and its connection ended.
pub(crate) const INSTALL_LOST: &str = "The phone\u{2019}s connection ended while LiMo was being put on it.";
const TRY_AGAIN: &str = "Try again; if it happens again, unplug the phone and plug it in again.";
const APP_MISSING: &str = "LiMo\u{2019}s app for phones is missing from this PC.";
const APP_NOT_OURS: &str = "The app for phones beside this program is not LiMo\u{2019}s own.";
const INSTALL_AGAIN: &str = "Install LiMo on this PC again: it puts the app for phones beside the console.";

fn wall_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// The ids of the devices in a `devices` answer.
fn device_ids(list: &Value) -> Vec<String> {
    list.as_array().into_iter().flatten().filter_map(|d| d.get("device_id").and_then(Value::as_str)).map(str::to_string).collect()
}

/// The device in a `devices` answer that was not there before: an Android
/// one first, the newest paired first.
fn new_device(list: &Value, before: &[String]) -> Option<Device> {
    let mut fresh: Vec<&Value> = list
        .as_array()?
        .iter()
        .filter(|d| d.get("device_id").and_then(Value::as_str).is_some_and(|id| !before.iter().any(|b| b == id)))
        .collect();
    fresh.sort_by_key(|d| {
        let android = d.get("platform").and_then(Value::as_str) == Some("android");
        (android, d.get("paired_at").and_then(Value::as_i64).unwrap_or(0))
    });
    let chosen = fresh.last()?;
    Some(Device {
        device_id: chosen["device_id"].as_str().unwrap_or_default().to_string(),
        name: chosen.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
    })
}

/// The ways a `device_link` answer shows the phone reaching the PC.
fn ways_of(link: &Value) -> Vec<String> {
    let mut ways: Vec<String> =
        link.get("sessions").and_then(Value::as_array).into_iter().flatten().filter_map(|s| s.get("way").and_then(Value::as_str)).map(str::to_string).collect();
    if let Some(way) = link.get("way").and_then(Value::as_str) {
        ways.push(way.to_string());
    }
    ways
}

/// True for what the door says when it is asked again too soon: the next
/// question may be answered.
fn passing(said: &Said) -> bool {
    matches!(said.code.as_str(), "too_fast" | "busy")
}

struct Worker {
    setup: Arc<PhoneSetup>,
    cable: Box<dyn ConsoleCable>,
    shared: Arc<Mutex<Shared>>,
    inbox: Receiver<Order>,
    key: Option<AdbKey>,
    key_failed: bool,
    phone: Option<Phone>,
    /// The link's port is carried on the phone held now.
    reversed: bool,
    asked_at: Instant,
    next_look: Instant,
    stopped_at: Instant,
    want_install: bool,
    want_pair: Option<String>,
    /// The guardian asked for the phone to be asked.
    want_ask: bool,
    /// Set by the desk to end an ask at once.
    stop: Arc<AtomicBool>,
    watch: Option<Watch>,
}

impl Worker {
    fn new(setup: Arc<PhoneSetup>, cable: Box<dyn ConsoleCable>, shared: Arc<Mutex<Shared>>, inbox: Receiver<Order>, stop: Arc<AtomicBool>) -> Worker {
        let now = Instant::now();
        Worker {
            setup,
            cable,
            shared,
            inbox,
            key: None,
            key_failed: false,
            phone: None,
            reversed: false,
            asked_at: now,
            next_look: now,
            stopped_at: now,
            want_install: false,
            want_pair: None,
            want_ask: false,
            stop,
            watch: None,
        }
    }

    /// A line in the window's own log, when it has one: never a key, a
    /// fingerprint or a serial number.
    fn log(&self, what: &str) {
        if let Some(log) = &self.setup.log {
            log.write(&format!("phone: {what}"));
        }
    }

    fn shared(&self) -> MutexGuard<'_, Shared> {
        look::lock(&self.shared)
    }

    /// Hands an event to the state machine.
    fn hand(&self, event: Event) {
        let mut s = self.shared();
        s.state = flow::step(s.state.clone(), event);
    }

    fn run(&mut self) {
        self.read_app();
        let counter = self.cable.sent();
        self.shared().counter = counter;
        loop {
            let wait = self.next_look.saturating_duration_since(Instant::now());
            match self.inbox.recv_timeout(wait) {
                Ok(Order::Done) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(order) => self.take(order),
                Err(RecvTimeoutError::Timeout) => {}
            }
            if self.asked_at.elapsed() >= self.setup.pace.forget_after {
                break;
            }
            if Instant::now() >= self.next_look {
                self.next_look = Instant::now() + self.setup.pace.look_every;
                self.tick();
            }
        }
        self.let_go();
    }

    fn take(&mut self, order: Order) {
        self.asked_at = Instant::now();
        match order {
            Order::Look | Order::Done => {}
            Order::Ask => {
                self.want_ask = true;
                self.advance();
                self.settle();
            }
            Order::Install => {
                self.want_install = true;
                self.advance();
                self.settle();
            }
            Order::Pair { child } => {
                self.want_pair = Some(child);
                self.advance();
                self.settle();
            }
            Order::DebuggingOff => self.debugging_off(),
        }
    }

    /// Reads what the app beside the program is, once.
    fn read_app(&mut self) {
        let mut s = self.shared();
        let problem = match &self.setup.app {
            None => Some(Words::new(APP_MISSING, Some(INSTALL_AGAIN))),
            Some(path) => match read_apk(path) {
                Ok(facts) if facts.package != PACKAGE || facts.debuggable => Some(Words::new(APP_NOT_OURS, Some(INSTALL_AGAIN))),
                Ok(facts) => {
                    s.app_version = facts.version_name;
                    s.install.bytes = std::fs::metadata(path).ok().map(|m| m.len());
                    None
                }
                Err(ApkProblem::Read(_)) => Some(Words::new(APP_MISSING, Some(INSTALL_AGAIN))),
                Err(_) => Some(Words::new(APP_NOT_OURS, Some(INSTALL_AGAIN))),
            },
        };
        if let Some(words) = problem {
            s.app_problem = Some(words.clone());
            s.install.stage = InstallStage::NoApp;
            s.install.words = Some(words);
        }
    }

    /// A look at the USB ports, then whatever is due.
    fn tick(&mut self) {
        let look = self.cable.look();
        let lost = {
            let mut s = self.shared();
            let hold = match &s.state {
                // An Android too old stays said until the phone is unplugged.
                State::Stopped(Problem::TooOld(_)) => !matches!(flow::seen(&look), State::NotReady(_)),
                // What went wrong stays said a moment before trying again.
                State::Stopped(_) => self.stopped_at.elapsed() < self.setup.pace.retry_after,
                _ => false,
            };
            let mut lost = false;
            if !hold {
                let was = matches!(s.state, State::Connected(_));
                s.state = flow::step(s.state.clone(), Event::Looked(look.clone()));
                lost = was && !matches!(s.state, State::Connected(_));
                if lost {
                    // Said in the same moment as the state, so that no look
                    // sees the phone gone and still held.
                    s.held = false;
                    s.busy = None;
                }
            }
            s.look = Some(look);
            lost
        };
        if lost {
            self.lost_now();
        }
        self.advance();
        self.watch();
    }

    /// Does each step that is due now, in turn.
    fn advance(&mut self) {
        let mut done: Vec<Next> = Vec::new();
        for _ in 0..10 {
            // An ask the guardian asked for, while the phone is there and not
            // connected: before anything else, and once.
            let state = self.shared().state.clone();
            let askable = matches!(state, State::Unknown | State::Ready { .. } | State::Stopped(Problem::Open(_) | Problem::Lost));
            if self.want_ask && askable && !self.key_failed {
                self.ask();
                continue;
            }
            let next = flow::next(&state)[0];
            if done.contains(&next) {
                break;
            }
            done.push(next);
            let held = self.phone.is_some();
            match next {
                Next::Connect if !self.key_failed => self.connect(),
                Next::CheckLimo if held => self.check_limo(),
                Next::InstallLimo if held && self.want_install => self.install(),
                Next::HandOffer | Next::ReverseLink | Next::StartLimo if held && self.watch.is_some() => self.carry_again(),
                // A pairing asked again after one failed part way: the state
                // machine has the old offer handed, so it no longer offers
                // to hand one; the whole of it is done again, with a new one.
                Next::HandOffer | Next::ReverseLink | Next::StartLimo | Next::SwitchDebuggingOff if held && self.want_pair.is_some() => self.pair(),
                _ => break,
            }
        }
    }

    /// A step asked for that could not start, because the phone went in the
    /// meantime: said, not left waiting.
    fn settle(&mut self) {
        if std::mem::take(&mut self.want_ask) {
            let mut s = self.shared();
            if s.busy == Some("asking") {
                s.busy = None;
            }
        }
        if self.phone.is_some() {
            return;
        }
        let install = std::mem::take(&mut self.want_install);
        let pair = self.want_pair.take().is_some();
        let mut s = self.shared();
        if install && s.install.stage == InstallStage::Sending {
            s.install.stage = InstallStage::Failed;
            s.install.words = Some(Words::new(LOST, Some(PLUG_IN_AGAIN)));
        }
        if pair && s.pairing.stage == PairStage::AskingService {
            s.pairing.stage = PairStage::Failed;
            s.pairing.words = Some(Words::new(LOST, Some(PLUG_IN_AGAIN)));
        }
    }

    fn key(&mut self) -> Result<AdbKey, Words> {
        if let Some(key) = &self.key {
            return Ok(key.clone());
        }
        let said = |e: KeyError| Words { line: "LiMo\u{2019}s key for phones could not be had on this PC.".to_string(), advice: Some(typeset(&e.to_string())) };
        let folder = match &self.setup.key_folder {
            Some(folder) => folder.clone(),
            None => limo_cable::key::default_folder().map_err(said)?,
        };
        let key = match AdbKey::load_or_make(&folder) {
            // A damaged key cannot be used by anyone: a new one is made, kept
            // beside the old, and a phone that knew the old asks again.
            Err(KeyError::Damaged) => AdbKey::replace(&folder).map_err(said)?,
            other => other.map_err(said)?,
        };
        self.key = Some(key.clone());
        Ok(key)
    }

    fn connect(&mut self) {
        let key = match self.key() {
            Ok(key) => key,
            Err(words) => {
                self.key_failed = true;
                self.shared().cable_trouble = Some(words);
                return;
            }
        };
        let name = {
            let mut s = self.shared();
            s.state = flow::step(s.state.clone(), Event::Connecting);
            s.busy = Some("connecting");
            s.fingerprint = Some(key.fingerprint());
            s.look.as_ref().and_then(look::phone_name)
        };
        // Quiet: the phone is never shown the key here.
        let opened = self.cable.open(&key, self.setup.waits, Knock::Quiet);
        if opened.as_ref().err() == Some(&OpenProblem::NotKnown) {
            self.log("a phone is plugged in that does not know this PC; it was not asked");
        }
        self.opened(opened, name);
    }

    /// Asks the phone, once, to allow this PC: shows it the key, and waits
    /// for Allow as long as `waits.prompt` (ten minutes), or until the phone
    /// goes, `phone.done`, or a minute with no request from the screens.
    fn ask(&mut self) {
        self.want_ask = false;
        let key = match self.key() {
            Ok(key) => key,
            Err(words) => {
                self.key_failed = true;
                self.shared().cable_trouble = Some(words);
                return;
            }
        };
        let name = {
            let mut s = self.shared();
            s.state = flow::step(s.state.clone(), Event::Connecting);
            s.busy = Some("asking");
            s.fingerprint = Some(key.fingerprint());
            s.look.as_ref().and_then(look::phone_name)
        };
        self.log("the guardian asked: the phone is asked, once, to allow this PC");
        let shared = Arc::clone(&self.shared);
        let mut on_prompt = move || {
            let mut s = look::lock(&shared);
            s.state = flow::step(s.state.clone(), Event::Asking);
            s.asked_at = Some(Instant::now());
        };
        let (stop, watched, forget) = (Arc::clone(&self.stop), Arc::clone(&self.shared), self.setup.pace.forget_after);
        let give_up = move || stop.load(Ordering::SeqCst) || look::lock(&watched).last_request.elapsed() >= forget;
        let opened = self.cable.open(&key, self.setup.waits, Knock::Asking { on_prompt: &mut on_prompt, give_up: &give_up });
        self.log(match &opened {
            Ok(_) => "the phone allowed this PC",
            Err(OpenProblem::NotAllowed) => "the phone was asked, and not allowed in time; it is not asked again by itself",
            Err(OpenProblem::GivenUp) => "the ask was given up",
            Err(_) => "the ask ended: the phone could not be reached",
        });
        self.shared().asked_at = None;
        self.opened(opened, name);
    }

    /// What a connection, quiet or asking, came to.
    fn opened(&mut self, opened: Result<Phone, OpenProblem>, name: Option<String>) {
        let mut s = self.shared();
        s.busy = None;
        match opened {
            Ok(phone) => {
                let facts = phone.facts().clone();
                s.state = flow::step(s.state.clone(), Event::Connected(facts.clone()));
                s.facts = Some(facts);
                s.limo = None;
                s.phone_name = name;
                if matches!(s.state, State::Connected(_)) {
                    s.held = true;
                    s.debugging = DebugStage::On;
                    drop(s);
                    self.phone = Some(phone);
                    self.reversed = false;
                } else {
                    // Too old: nothing is done on it.
                    drop(s);
                    phone.close();
                    self.stopped_at = Instant::now();
                }
            }
            Err(problem) => {
                s.state = flow::step(s.state.clone(), Event::OpenFailed(problem));
                drop(s);
                self.stopped_at = Instant::now();
            }
        }
    }

    fn check_limo(&mut self) {
        let Some(phone) = &self.phone else { return };
        self.shared().busy = Some("checking");
        let checked = phone.limo();
        self.shared().busy = None;
        match checked {
            Ok(limo) => {
                let mut s = self.shared();
                s.state = flow::step(s.state.clone(), Event::LimoChecked(limo.clone()));
                let same = match (&limo, &s.app_version) {
                    (Some(on), Some(carried)) => on.version_name.as_deref() == Some(carried.as_str()),
                    _ => false,
                };
                let ours = s.install.stage == InstallStage::Done;
                if limo.is_some() && (same || ours) {
                    // The phone holds the version this PC carries (or the one
                    // put on it a moment ago): nothing to install.
                    if !ours {
                        s.install.stage = InstallStage::NotNeeded;
                        s.install.words = None;
                    }
                    s.state = flow::step(s.state.clone(), Event::Installed(Ok(())));
                }
                s.limo = Some(limo);
            }
            Err(error) => self.call_failed(error),
        }
    }

    fn install(&mut self) {
        self.want_install = false;
        let (Some(phone), Some(app)) = (&self.phone, self.setup.app.clone()) else { return };
        {
            let mut s = self.shared();
            s.install.stage = InstallStage::Sending;
            s.install.base = s.counter.as_ref().map(|c| c.load(std::sync::atomic::Ordering::Relaxed));
            s.install.words = None;
            s.busy = Some("installing");
        }
        let result = phone.install_limo(&app);
        let mut lost = false;
        {
            let mut s = self.shared();
            s.busy = None;
            let failed = |line: &str, advice: &str| Some(Words::new(line, Some(advice)));
            match result {
                Ok(Installed::Done) => {
                    s.install.stage = InstallStage::Done;
                    s.state = flow::step(s.state.clone(), Event::Installed(Ok(())));
                    s.limo = Some(Some(LimoOnPhone { version_code: None, version_name: s.app_version.clone() }));
                }
                Ok(Installed::Refused { code, .. }) => {
                    s.install.stage = InstallStage::Refused;
                    s.install.words = Some(Words { line: "The phone did not take LiMo.".to_string(), advice: Some(typeset(install_advice(&code))) });
                    s.state = flow::step(s.state.clone(), Event::Installed(Err(code)));
                }
                Err(InstallError::NotAnApk(ApkProblem::Read(_))) => {
                    s.install.stage = InstallStage::NoApp;
                    s.install.words = failed(APP_MISSING, INSTALL_AGAIN);
                }
                Err(InstallError::NotAnApk(_) | InstallError::NotLimo(_) | InstallError::DebugBuild) => {
                    s.install.stage = InstallStage::NoApp;
                    s.install.words = failed(APP_NOT_OURS, INSTALL_AGAIN);
                }
                Err(InstallError::Call(CallError::Lost(_))) => {
                    s.install.stage = InstallStage::Failed;
                    s.install.words = failed(INSTALL_LOST, PLUG_IN_AGAIN);
                    lost = true;
                }
                Err(InstallError::Call(CallError::Timeout(_))) => {
                    s.install.stage = InstallStage::Failed;
                    s.install.words = failed("The phone did not finish installing LiMo in time.", "Look at the phone\u{2019}s screen for a question, then try again.");
                }
                Err(InstallError::Call(CallError::Failed(_))) => {
                    s.install.stage = InstallStage::Failed;
                    s.install.words = failed("The phone did not answer as expected.", TRY_AGAIN);
                }
            }
        }
        if lost {
            self.lost_now();
        }
    }

    fn pair(&mut self) {
        let Some(child) = self.want_pair.take() else { return };
        {
            let mut s = self.shared();
            s.pairing = look::Pairing { stage: PairStage::AskingService, ..look::Pairing::default() };
            s.busy = Some("pairing");
        }
        let outcome = self.hand_pairing(&child);
        self.shared().busy = None;
        match outcome {
            Ok(watch) => {
                self.watch = Some(watch);
                self.shared().pairing.stage = PairStage::Waiting;
            }
            Err(failure) => {
                {
                    let mut s = self.shared();
                    s.pairing.stage = PairStage::Failed;
                    s.pairing.words = Some(failure.words);
                }
                if failure.lost {
                    self.lost_now();
                }
            }
        }
    }

    /// Asks the service for an offer, hands it to LiMo on the phone, carries
    /// the link's port and starts LiMo: the pairing to wait for.
    fn hand_pairing(&mut self, child: &str) -> Result<Watch, Failure> {
        let service = Arc::clone(&self.setup.service);
        let before = device_ids(&service.ask("devices", json!({ "child": child })).map_err(Failure::said)?);
        let mut made = service.ask("make_offer", json!({ "child": child })).map_err(Failure::said)?;
        let offer = Zeroizing::new(made.get("offer").and_then(Value::as_str).unwrap_or_default().as_bytes().to_vec());
        let expires_at = made.get("expires_at").and_then(Value::as_i64);
        let port = made.get("port").and_then(Value::as_u64);
        wipe(&mut made);
        if offer.is_empty() {
            return Err(Failure { words: Words::new("LiMo\u{2019}s service gave no pairing offer.", Some("Try again.")), lost: false });
        }
        let link_port = self.setup.link_port;
        if port != Some(u64::from(link_port)) {
            return Err(Failure {
                words: Words::new(
                    &format!("LiMo\u{2019}s service waits for phones on another port than the {link_port} LiMo on the phone reaches over the cable."),
                    Some("Restart this PC, then try again."),
                ),
                lost: false,
            });
        }
        self.shared().pairing.stage = PairStage::Handing;
        let phone = self.phone.as_ref().ok_or(Failure { words: Words::new(LOST, Some(PLUG_IN_AGAIN)), lost: true })?;
        let handed = phone.hand_offer(&offer).map_err(Failure::call)?;
        drop(offer);
        if handed.ok() {
            self.hand(Event::OfferHanded(Ok(())));
        } else {
            self.hand(Event::OfferHanded(Err(handed.said)));
            return Err(self.told());
        }
        phone.reverse(link_port).map_err(Failure::call)?;
        self.reversed = true;
        self.hand(Event::Reversed);
        let started = phone.start_limo().map_err(Failure::call)?;
        let refused = matches!(started, Started::Refused(_));
        self.hand(Event::Started(started));
        if refused {
            return Err(self.told());
        }
        let left = expires_at.map_or(self.setup.pace.longest_pairing, |at| Duration::from_secs(u64::try_from(at - wall_secs()).unwrap_or(0)));
        let now = Instant::now();
        Ok(Watch { child: child.to_string(), before, until: now + left.min(self.setup.pace.longest_pairing), next_ask: now, device: None })
    }

    /// limo-cable's words for what went wrong on the phone.
    fn told(&self) -> Failure {
        let told = flow::tell(&self.shared().state);
        Failure { words: Words { line: typeset(&told.line), advice: told.advice.as_deref().map(typeset) }, lost: false }
    }

    /// The phone came back while a pairing waits or after it: the link's
    /// port is carried again, and nothing handed twice.
    fn carry_again(&mut self) {
        let Some(phone) = &self.phone else { return };
        let reversed = phone.reverse(self.setup.link_port);
        self.hand(Event::OfferHanded(Ok(())));
        match reversed {
            Ok(()) => {
                self.reversed = true;
                self.hand(Event::Reversed);
                self.hand(Event::Started(Started::Started));
            }
            Err(error) => self.call_failed(error),
        }
    }

    /// While a pairing waits: has the phone paired? Once paired: how does it
    /// reach the PC?
    fn watch(&mut self) {
        let Some(watch) = self.watch.as_mut() else { return };
        let now = Instant::now();
        if now < watch.next_ask {
            return;
        }
        let service = Arc::clone(&self.setup.service);
        let pace = self.setup.pace;
        if watch.device.is_none() {
            if now >= watch.until {
                self.watch = None;
                let mut s = self.shared();
                s.pairing.stage = PairStage::Failed;
                s.pairing.words = Some(Words::new("The pairing offer ran out before LiMo on the phone paired.", Some("Try again.")));
                return;
            }
            watch.next_ask = now + pace.ask_every;
            let found = match service.ask("devices", json!({ "child": watch.child })) {
                Ok(list) => new_device(&list, &watch.before),
                Err(said) if passing(&said) => None,
                Err(said) => {
                    self.watch = None;
                    let mut s = self.shared();
                    s.pairing.stage = PairStage::Failed;
                    s.pairing.words = Some(Words { line: said.words, advice: None });
                    return;
                }
            };
            let Some(device) = found else { return };
            // Paired once the service gives the device's link.
            let Ok(link) = service.ask("device_link", json!({ "device": device.device_id })) else { return };
            watch.device = Some(device.device_id.clone());
            watch.next_ask = now + pace.watch_every;
            let mut s = self.shared();
            s.pairing.stage = PairStage::Paired;
            s.pairing.device = Some(device);
            s.pairing.words = None;
            for way in ways_of(&link) {
                if !s.pairing.ways.contains(&way) {
                    s.pairing.ways.push(way);
                }
            }
        } else {
            watch.next_ask = now + pace.watch_every;
            let device = watch.device.clone().unwrap_or_default();
            if let Ok(link) = service.ask("device_link", json!({ "device": device })) {
                let mut s = self.shared();
                for way in ways_of(&link) {
                    if !s.pairing.ways.contains(&way) {
                        s.pairing.ways.push(way);
                    }
                }
            }
        }
    }

    fn debugging_off(&mut self) {
        let Some(phone) = self.phone.take() else {
            self.shared().debugging = DebugStage::On;
            return;
        };
        {
            let mut s = self.shared();
            s.debugging = DebugStage::SwitchingOff;
            s.busy = Some("switching_off");
        }
        let answered = phone.debugging_off();
        self.reversed = false;
        let off = match answered {
            DebuggingOff::Answered | DebuggingOff::LinkEnded => true,
            // No answer: whether it changed is for the USB ports to say.
            DebuggingOff::NoAnswer(_) => {
                std::thread::sleep(self.setup.pace.look_every);
                let look = self.cable.look();
                let seen = flow::seen(&look);
                self.shared().look = Some(look);
                !matches!(seen, State::Ready { .. } | State::TwoPhones(_))
            }
        };
        let mut s = self.shared();
        s.busy = None;
        s.held = false;
        if off {
            s.debugging = DebugStage::Off;
            s.state = flow::step(s.state.clone(), Event::DebuggingOff(true));
        } else {
            // Still on, and the phone let go: it is connected again at the
            // next look, and can be asked again.
            s.debugging = DebugStage::StillOn;
            s.state = State::NotReady(limo_cable::usb::Case::NoPhone);
        }
    }

    /// A call failed: lost, or said.
    fn call_failed(&mut self, error: CallError) {
        if matches!(error, CallError::Lost(_)) {
            self.lost_now();
        } else {
            self.hand(Event::CallFailed(error.to_string()));
        }
    }

    /// The phone is gone: let go of it.
    fn lost_now(&mut self) {
        if let Some(phone) = self.phone.take() {
            phone.close();
        }
        self.reversed = false;
        self.stopped_at = Instant::now();
        let mut s = self.shared();
        s.held = false;
        s.busy = None;
        if !matches!(s.state, State::Stopped(_)) {
            s.state = flow::step(s.state.clone(), Event::Lost);
        }
    }

    /// The end: the link's port is no longer carried, and the phone is let
    /// go.
    fn let_go(&mut self) {
        if let Some(phone) = self.phone.take() {
            if self.reversed {
                let _ = phone.remove_reverse(self.setup.link_port);
            }
            phone.close();
        }
        self.reversed = false;
        self.shared().held = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_new_device_is_the_one_that_was_not_there_an_android_one_first() {
        let list = json!([
            { "device_id": "pc1", "name": "This PC", "platform": "windows", "paired_at": 10 },
            { "device_id": "ph2", "name": "SM-G988B", "platform": "android", "paired_at": 20 },
            { "device_id": "pc3", "name": "Other", "platform": "windows", "paired_at": 30 },
        ]);
        assert_eq!(device_ids(&list), vec!["pc1", "ph2", "pc3"]);
        let before = vec!["pc1".to_string()];
        assert_eq!(new_device(&list, &before), Some(Device { device_id: "ph2".into(), name: "SM-G988B".into() }));
        assert_eq!(new_device(&list, &device_ids(&list)), None);
        assert_eq!(new_device(&json!({ "not": "a list" }), &before), None);
    }

    #[test]
    fn the_ways_are_read_from_the_sessions_and_from_a_single_way() {
        assert_eq!(ways_of(&json!({ "device_id": "ph2", "sessions": [{ "way": "cable" }, { "way": "home_network" }] })), vec!["cable", "home_network"]);
        assert_eq!(ways_of(&json!({ "way": "cable", "opened_at": 1 })), vec!["cable"]);
        assert!(ways_of(&json!({ "device_id": "ph2", "sessions": [] })).is_empty());
    }
}
