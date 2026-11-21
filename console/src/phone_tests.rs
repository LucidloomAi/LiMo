//! Adding a phone as the Add a phone screens drive it, through the
//! console's own requests (`phone.look` once a second, and the rest), against
//! limo-cable's fake phone at the far end of a fake cable, a throwaway app,
//! and the service as the stand in door answers it over a real pipe, as an
//! in memory family, or as LiMo for Windows' own door (which has no link
//! for phones in a test, so says so). No real phone and no real service:
//! the link's port is one the test's own listener was given by Windows on
//! 127.0.0.1, never LiMo's own and never 8081.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use limo_cable::commands::{LINK_PORT, PACKAGE};
use limo_cable::fake::{CableKind, Driver, FakeLimo, FakePhone, FakeState, Person};
use limo_cable::key::AdbKey;
use limo_cable::open::{Cable, Look};
use limo_cable::phone::{Knock, OpenProblem, Phone, Waits};
use limo_cable::protocol::{AUTH_RSAPUBLICKEY, AUTH_SIGNATURE};
use serde_json::{Value, json};

use crate::door::{Door, DoorReply};
use crate::door_tests::pipe_name;
use crate::phone::{ConsoleCable, DoorService, Pace, PhoneDesk, PhoneSetup, Said, Service};
use crate::protocol::PageRequest;
use crate::real_door_tests::real_door::RealDoorHost;
use crate::stand_in::{Family, Script, StandIn};
use crate::test_apk::{VERSION, apk_in};
use crate::this_process;

const PASSWORD: &str = "Mira's long sentence";
/// What LiMo for Windows says to `make_offer` while its link for phones is
/// not running.
const NO_LINK: &str = "The link for phones is not running on this PC, so no phone can pair with it now. The service’s log says why.";

/// The fake phone's cable, held as the console holds the real one.
pub(crate) struct FakeCable(Arc<Mutex<FakePhone>>);

pub(crate) fn phone_of(phone: &Mutex<FakePhone>) -> MutexGuard<'_, FakePhone> {
    phone.lock().unwrap_or_else(|p| p.into_inner())
}

impl ConsoleCable for FakeCable {
    fn look(&mut self) -> Look {
        phone_of(&self.0).look()
    }
    fn open(&mut self, key: &AdbKey, waits: Waits, knock: Knock<'_>) -> Result<Phone, OpenProblem> {
        phone_of(&self.0).open(key, waits, knock)
    }
}

/// The service as an in memory family, answering as the stand in does.
pub(crate) struct InMemory(Mutex<Family>);

impl InMemory {
    pub(crate) fn new(port: u16) -> Arc<InMemory> {
        Arc::new(InMemory(Mutex::new(Family { port, ..Family::default() })))
    }
    pub(crate) fn family(&self) -> MutexGuard<'_, Family> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }
}

impl Service for InMemory {
    fn ask(&self, name: &str, args: Value) -> Result<Value, Said> {
        match self.family().answer(name, &args) {
            Some(reply) if reply["outcome"] == "done" => Ok(reply["value"].clone()),
            Some(reply) => Err(Said::new(reply["problem"]["kind"].as_str().unwrap(), reply["problem"]["message"].as_str().unwrap())),
            None => Err(Said::new("unknown_request", "LiMo does not know that request.")),
        }
    }
}

/// LiMo's key for the tests, made once in a folder of the test run's own.
pub(crate) fn key_folder() -> PathBuf {
    static FOLDER: OnceLock<PathBuf> = OnceLock::new();
    FOLDER
        .get_or_init(|| {
            let folder = std::env::temp_dir().join(format!("limo-console-phone-key-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&folder);
            AdbKey::load_or_make(&folder).unwrap();
            folder
        })
        .clone()
}

pub(crate) fn fingerprint() -> String {
    AdbKey::load_or_make(&key_folder()).unwrap().fingerprint()
}

/// A listener on a port Windows picks on the loopback, as the service's
/// link: never LiMo's own port, never 8081.
pub(crate) fn link_listener() -> (TcpListener, u16) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    assert!(port != LINK_PORT && port != 8081, "{port}");
    (listener, port)
}

pub(crate) fn quick() -> Pace {
    Pace {
        look_every: Duration::from_millis(20),
        retry_after: Duration::from_millis(200),
        forget_after: Duration::from_secs(60),
        ask_every: Duration::from_millis(20),
        watch_every: Duration::from_millis(40),
        longest_pairing: Duration::from_secs(60),
    }
}

/// The fake phone, its cable in a desk, and every answer the desk gave.
pub(crate) struct Rig {
    pub(crate) phone: Arc<Mutex<FakePhone>>,
    /// The fake phone's own state, which a test changes as a person would.
    fake: Arc<Mutex<FakeState>>,
    desk: PhoneDesk,
    app: PathBuf,
    /// The window's log, when the rig has one.
    log_path: Option<PathBuf>,
    answers: Mutex<Vec<String>>,
    _dir: tempfile::TempDir,
}

pub(crate) struct RigSetup {
    pub(crate) app: AppKind,
    pub(crate) pace: Pace,
    /// The phone allowed this PC before: a quiet connection connects.
    pub(crate) known: bool,
    /// How long an ask waits for Allow.
    pub(crate) prompt: Duration,
    /// A window's log of its own, in the rig's folder.
    pub(crate) log: bool,
}

impl Default for RigSetup {
    /// LiMo's app, the quick pace, a phone that knows this PC, an ask
    /// waiting 700 ms.
    fn default() -> RigSetup {
        RigSetup { app: AppKind::Limo, pace: quick(), known: true, prompt: Duration::from_millis(700), log: false }
    }
}

pub(crate) enum AppKind {
    Limo,
    Other,
    Missing,
}

impl Rig {
    pub(crate) fn new(change: impl FnOnce(&mut FakeState), service: Arc<dyn Service>, link_port: u16) -> Rig {
        Rig::with(change, service, link_port, RigSetup::default())
    }

    pub(crate) fn with(change: impl FnOnce(&mut FakeState), service: Arc<dyn Service>, link_port: u16, setup: RigSetup) -> Rig {
        let dir = tempfile::tempdir().unwrap();
        let app = match setup.app {
            AppKind::Limo => apk_in(dir.path(), PACKAGE, false),
            AppKind::Other => apk_in(dir.path(), "com.example.other", false),
            AppKind::Missing => dir.path().join(crate::phone::APP_FOLDER).join(crate::phone::APP_FILE),
        };
        let log_path = setup.log.then(|| dir.path().join(crate::console_log::LOG_FILE));
        let fake_phone = FakePhone::with(change);
        if setup.known {
            fake_phone.state().knows(&AdbKey::load_or_make(&key_folder()).unwrap());
        }
        let fake = Arc::clone(&fake_phone.state);
        let phone = Arc::new(Mutex::new(fake_phone));
        let theirs = Arc::clone(&phone);
        let desk = PhoneDesk::new(PhoneSetup {
            cable: Arc::new(move || Box::new(FakeCable(Arc::clone(&theirs))) as Box<dyn ConsoleCable>),
            service,
            app: Some(app.clone()),
            key_folder: Some(key_folder()),
            link_port,
            waits: Waits { hello: Duration::from_secs(5), prompt: setup.prompt, call: Duration::from_secs(5), install: Duration::from_secs(10) },
            pace: setup.pace,
            log: log_path.as_ref().map(|p| Arc::new(crate::console_log::ConsoleLog::at(p))),
        });
        Rig { phone, fake, desk, app, log_path, answers: Mutex::new(Vec::new()), _dir: dir }
    }

    /// The fake phone's state, locked: plug it in, unplug it, switch USB
    /// debugging on, allow or not.
    pub(crate) fn state(&self) -> MutexGuard<'_, FakeState> {
        self.fake.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub(crate) fn ask(&self, name: &str, args: Value) -> Result<Value, Value> {
        let answer = self.desk.request(name, args);
        self.answers.lock().unwrap().push(format!("{answer:?}"));
        answer
    }

    /// Asks `phone.look`, as the screens do, until `test` holds.
    pub(crate) fn until(&self, what: &str, test: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let look = self.ask("phone.look", Value::Null).unwrap();
            if test(&look) {
                return look;
            }
            assert!(Instant::now() < deadline, "{what}: {look:#}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// True when some answer so far held `text`.
    fn said(&self, text: &str) -> bool {
        self.answers.lock().unwrap().iter().any(|a| a.contains(text))
    }
}

/// Accepts the one connection the phone's app makes through the cable.
fn accept(listener: &TcpListener) -> TcpStream {
    listener.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                return stream;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            Err(e) => panic!("the link's listener took no connection: {e}"),
        }
    }
}

pub(crate) fn a_paired_phone() -> Value {
    json!({ "device_id": "d7", "child_id": "c1", "name": "SM-G988B", "kind": "portable", "platform": "android", "paired_at": 1_800_000_000, "last_heard_at": null })
}

pub(crate) fn a_session(way: &str) -> Value {
    json!({ "way": way, "opened_at": 1_800_000_000, "last_report_at": null })
}

#[test]
fn a_phone_is_added_over_the_cable_from_plugging_in_to_usb_debugging_off() {
    let (listener, port) = link_listener();
    let door_side = StandIn::start(&pipe_name(), Script::Door { password: PASSWORD.into() });
    door_side.family().port = port;
    let door = Arc::new(Mutex::new(Door::new(door_side.name(), this_process::as_server().unwrap())));
    // The guardian signed in on the page, on the console's one connection.
    let signed = door.lock().unwrap().ask(PageRequest::from_page("sign_in", json!({ "password": PASSWORD })).unwrap()).unwrap();
    assert_eq!(signed, DoorReply::Reply(json!({ "outcome": "done", "value": {} })));
    // A phone that has never seen this PC.
    let rig = Rig::with(
        |s| s.cable = CableKind::ChargeOnly,
        Arc::new(DoorService::new(Arc::clone(&door))),
        port,
        RigSetup { known: false, ..RigSetup::default() },
    );

    // A1 and A2: the cable's strip in each of its cases, as drawn.
    let look = rig.until("a cable that only charges", |v| v["cable"]["case"] == "none");
    assert_eq!(
        look["cable"],
        json!({ "case": "none", "line": "No phone on a USB port.", "more": "If it is plugged in, the cable may only charge it. Try the one that came with the phone." })
    );
    assert_eq!((look["phone"].clone(), look["held"].clone(), look["busy"].clone()), (Value::Null, json!(false), Value::Null));
    {
        let mut s = rig.state();
        s.cable = CableKind::Data;
        s.debugging = false;
    }
    let look = rig.until("USB debugging off", |v| v["cable"]["case"] == "off");
    assert_eq!(look["cable"], json!({ "case": "off", "line": "A phone is plugged in.", "more": "USB debugging is off on it." }));
    {
        let mut s = rig.state();
        s.debugging = true;
        s.driver = Driver::None;
    }
    let look = rig.until("no driver", |v| v["cable"]["case"] == "driver");
    assert_eq!(
        look["cable"],
        json!({ "case": "driver", "line": "No working driver for this phone.", "more": "Install its maker’s USB driver for Windows, such as Samsung’s Android USB Driver." })
    );
    rig.state().driver = Driver::MakersOwn;
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(rig.until("a driver LiMo cannot use", |v| v["cable"]["case"] == "driver")["cable"]["line"], "No working driver for this phone.");
    assert!(rig.state().seen.clone().services.is_empty(), "nothing was opened");

    // A3: the phone is connected quietly and does not know this PC: nothing
    // is shown to it, however often the screens look. Asked, once, it asks
    // the person, with the key's fingerprint to compare; nobody allows in
    // time, which is said, and it is not asked again by itself; asked again,
    // the person allows.
    {
        let mut s = rig.state();
        s.driver = Driver::SamsungWinUsb;
        s.person = Person::Away;
    }
    let look = rig.until("the phone does not know this PC", |v| v["cable"]["case"] == "unknown");
    assert_eq!(look["cable"], json!({ "case": "unknown", "line": "A phone is plugged in.", "more": "It does not know this PC yet." }));
    assert_eq!(look["fingerprint"], fingerprint());
    assert_eq!(look["asked_secs"], Value::Null);
    std::thread::sleep(Duration::from_millis(400));
    rig.until("still not asked", |v| v["cable"]["case"] == "unknown");
    let seen = rig.state().seen.clone();
    assert!(!seen.auth.contains(&AUTH_RSAPUBLICKEY) && seen.prompts == 0, "looks never show the key: {:?}", seen.auth);
    assert_eq!(seen.auth.iter().filter(|a| **a == AUTH_SIGNATURE).count(), 1, "one quiet connection, not tried again: {:?}", seen.auth);
    assert_eq!(rig.ask("phone.ask", Value::Null), Ok(json!({ "started": true })));
    let look = rig.until("the phone asks", |v| v["cable"]["case"] == "allow");
    assert_eq!(look["cable"], json!({ "case": "allow", "line": "The phone is asking.", "more": "Waiting for Allow on the phone." }));
    assert_eq!(look["fingerprint"], fingerprint());
    assert_eq!(look["busy"], "asking");
    assert!(look["asked_secs"].is_u64(), "{look}");
    assert_eq!(rig.ask("phone.ask", Value::Null).unwrap_err()["code"], "busy");
    let look = rig.until("not allowed in time", |v| v["cable"]["case"] == "trouble");
    assert_eq!(look["cable"]["line"], "The phone did not allow this PC in time.");
    assert_eq!(rig.state().seen.clone().prompts, 1, "the key shown once");
    std::thread::sleep(Duration::from_millis(500));
    rig.until("not asked again by itself", |v| v["cable"]["case"] == "trouble");
    assert_eq!(rig.state().seen.clone().auth.iter().filter(|a| **a == AUTH_RSAPUBLICKEY).count(), 1, "never asked twice by itself");
    rig.state().person = Person::Allows;
    assert_eq!(rig.ask("phone.ask", Value::Null), Ok(json!({ "started": true })));

    // A4: connected, the checks, then LiMo put on the phone.
    let look = rig.until("connected and checked", |v| v["cable"]["case"] == "ok" && v["phone"]["limo"]["looked"] == true);
    assert_eq!(look["cable"], json!({ "case": "ok", "line": "Connected: Galaxy S20 Ultra 5G.", "more": "Android 13." }));
    assert_eq!(
        look["phone"],
        json!({ "name": "Galaxy S20 Ultra 5G", "model": "SM-G988B", "maker": "samsung", "android": "13", "recent_enough": true,
                "limo": { "looked": true, "on_it": false, "version": null }, "this_pc_carries": VERSION })
    );
    assert_eq!(
        look["checks"],
        json!([{ "ok": true, "words": "One phone plugged in" }, { "ok": true, "words": "Android 13, recent enough for LiMo" }, { "ok": true, "words": "LiMo is not on it yet" }])
    );
    assert_eq!(look["install"], json!({ "stage": "not_asked", "sent": null, "line": null, "advice": null }));
    assert_eq!(look["held"], true);
    assert_eq!(rig.ask("phone.pair", json!({ "child": "c1" })).unwrap_err()["code"], "not_yet");
    assert_eq!(rig.ask("phone.install", Value::Null), Ok(json!({ "started": true })));
    let look = rig.until("installed", |v| v["install"]["stage"] == "done");
    assert_eq!(look["install"]["line"], "LiMo is on the phone.");
    assert_eq!(look["checks"][2]["words"], format!("LiMo {VERSION} is on it already"));
    let seen = rig.state().seen.clone();
    assert_eq!(seen.installs, 1);
    assert_eq!(seen.installed[0], std::fs::read(&rig.app).unwrap(), "the bytes checked are the bytes sent");
    assert_eq!(rig.ask("phone.install", Value::Null), Ok(json!({ "started": false })));

    // A5: paired over the cable. The offer goes from the service to the
    // phone, and is never in an answer.
    assert_eq!(rig.ask("phone.pair", json!({})).unwrap_err()["code"], "unknown_request");
    assert_eq!(rig.ask("phone.pair", json!({ "child": "c1" })), Ok(json!({ "started": true })));
    let look = rig.until("handed, and waiting", |v| v["pairing"]["stage"] == "waiting");
    assert_eq!(look["pairing"]["line"], "Waiting for LiMo on the phone to pair with this PC.");
    let offers = door_side.family().offers.clone();
    assert_eq!(offers.len(), 1);
    {
        let s = rig.state();
        assert_eq!(s.offers, vec![offers[0].as_bytes().to_vec()], "LiMo on the phone has the service's offer");
        assert_eq!(s.reverses, vec![port], "the phone's port is carried to the link's");
        assert!(s.screen_open, "LiMo was started on the phone");
    }
    let names: Vec<String> = door_side.heard()[0].messages.iter().filter_map(|m| m["request"]["name"].as_str().map(str::to_string)).collect();
    assert_eq!(names[..3], ["sign_in", "devices", "make_offer"]);
    // The phone's app reaches the link through the cable, both ways.
    let mut app = phone_of(&rig.phone).app_connects(port).unwrap();
    let mut link = accept(&listener);
    app.write(b"hello from the phone").unwrap();
    let mut got = [0u8; 20];
    link.read_exact(&mut got).unwrap();
    assert_eq!(&got, b"hello from the phone");
    link.write_all(b"hello from the PC").unwrap();
    assert_eq!(app.read_exact(17, Duration::from_secs(5)).unwrap(), b"hello from the PC");
    // The service pairs it, and says so in `devices` and `device_link`.
    {
        let mut family = door_side.family();
        family.devices.push(a_paired_phone());
        family.sessions.push(a_session("cable"));
    }
    let look = rig.until("paired", |v| v["pairing"]["stage"] == "paired");
    assert_eq!(
        look["pairing"],
        json!({ "stage": "paired", "device": { "device_id": "d7", "name": "SM-G988B" }, "ways": ["cable"], "line": "Over the cable, with this PC.", "advice": null })
    );
    // A7: the phone reaches the PC over the home Wi-Fi too.
    door_side.family().sessions.push(a_session("home_network"));
    rig.until("over the home Wi-Fi", |v| v["pairing"]["ways"] == json!(["cable", "home_network"]));
    app.close();

    // A8 and A9: USB debugging off, and the strip says so.
    assert_eq!(rig.ask("phone.debugging_off", Value::Null), Ok(json!({ "started": true })));
    let look = rig.until("USB debugging off", |v| v["debugging"]["stage"] == "off" && v["cable"]["case"] == "off");
    assert_eq!(look["debugging"], json!({ "stage": "off", "line": "USB debugging is off.", "advice": null }));
    assert_eq!(look["held"], false);
    assert!(!rig.state().debugging);
    assert_eq!(rig.ask("phone.debugging_off", Value::Null), Ok(json!({ "started": false })));
    assert_eq!(rig.ask("phone.done", Value::Null), Ok(json!({ "done": true })));
    assert!(!rig.said(&offers[0]) && !rig.said("SECRET"), "the offer's secret is never in an answer");
}

#[test]
fn the_screens_never_wait_for_the_phone() {
    let (_listener, port) = link_listener();
    let rig = Rig::with(|s| s.person = Person::Away, InMemory::new(port), port, RigSetup { known: false, ..RigSetup::default() });
    rig.until("the phone does not know this PC", |v| v["cable"]["case"] == "unknown");
    rig.ask("phone.ask", Value::Null).unwrap();
    rig.until("the phone asks", |v| v["cable"]["case"] == "allow");
    // The console's thread waits for the person; the requests do not.
    let mut slowest = Duration::ZERO;
    for _ in 0..10 {
        let started = Instant::now();
        let look = rig.ask("phone.look", Value::Null).unwrap();
        slowest = slowest.max(started.elapsed());
        if look["cable"]["case"] != "allow" {
            break;
        }
    }
    assert!(slowest < Duration::from_millis(150), "{slowest:?}");
    // A step asked while the phone is busy is refused at once, with words.
    let started = Instant::now();
    assert_eq!(rig.ask("phone.install", Value::Null).unwrap_err()["code"], "no_phone");
    assert!(started.elapsed() < Duration::from_millis(150));
}

#[test]
fn limo_on_the_phone_at_this_pc_s_version_is_not_put_on_again() {
    let (_listener, port) = link_listener();
    let service = InMemory::new(port);
    let rig = Rig::new(
        |s| s.limo = Some(FakeLimo { version_code: 3, version_name: VERSION.to_string(), other_key: false }),
        Arc::clone(&service) as Arc<dyn Service>,
        port,
    );
    let look = rig.until("checked", |v| v["install"]["stage"] == "not_needed");
    assert_eq!(look["install"]["line"], format!("LiMo {VERSION} is on the phone already."));
    assert_eq!(look["checks"][2], json!({ "ok": true, "words": format!("LiMo {VERSION} is on it already") }));
    assert_eq!(look["phone"]["limo"], json!({ "looked": true, "on_it": true, "version": VERSION }));
    assert_eq!(rig.ask("phone.install", Value::Null), Ok(json!({ "started": false })));
    assert_eq!(rig.state().seen.clone().installs, 0);

    // The service cannot make an offer: its own sentence, and nothing is
    // handed to the phone.
    service.family().offer_problem = Some(("failed".into(), NO_LINK.into()));
    assert_eq!(rig.ask("phone.pair", json!({ "child": "c1" })), Ok(json!({ "started": true })));
    let look = rig.until("refused by the service", |v| v["pairing"]["stage"] == "failed");
    assert_eq!(look["pairing"]["line"], NO_LINK);
    assert!(rig.state().offers.is_empty() && rig.state().reverses.is_empty());
    // Asked again once it can: paired.
    service.family().offer_problem = None;
    assert_eq!(rig.ask("phone.pair", json!({ "child": "c1" })), Ok(json!({ "started": true })));
    rig.until("waiting", |v| v["pairing"]["stage"] == "waiting");
    service.family().devices.push(a_paired_phone());
    let look = rig.until("paired", |v| v["pairing"]["stage"] == "paired");
    assert_eq!(look["pairing"]["line"], "With this PC.", "paired, with no session open yet");
    assert_eq!(rig.ask("phone.pair", json!({ "child": "c1" })), Ok(json!({ "started": false })));
}

#[test]
fn an_older_limo_is_replaced_and_one_of_another_maker_is_left_as_it_is() {
    let (_listener, port) = link_listener();
    let older = Rig::new(|s| s.limo = Some(FakeLimo { version_code: 2, version_name: "0.2.0".into(), other_key: false }), InMemory::new(port), port);
    let look = older.until("checked", |v| v["phone"]["limo"]["looked"] == true);
    assert_eq!(look["checks"][2]["words"], format!("LiMo 0.2.0 is on it, and this PC carries {VERSION}"));
    assert_eq!(older.ask("phone.install", Value::Null), Ok(json!({ "started": true })));
    older.until("replaced", |v| v["install"]["stage"] == "done");

    let other = Rig::new(|s| s.limo = Some(FakeLimo { version_code: 2, version_name: "0.2.0".into(), other_key: true }), InMemory::new(port), port);
    other.until("checked", |v| v["phone"]["limo"]["looked"] == true);
    assert_eq!(other.ask("phone.install", Value::Null), Ok(json!({ "started": true })));
    let look = other.until("refused", |v| v["install"]["stage"] == "refused");
    assert_eq!(look["install"]["line"], "The phone did not take LiMo.");
    assert!(look["install"]["advice"].as_str().unwrap().contains("leaves it as it is"), "{look}");
    assert_eq!(other.ask("phone.pair", json!({ "child": "c1" })).unwrap_err()["code"], "not_yet");
    // It can be asked again.
    assert_eq!(other.ask("phone.install", Value::Null), Ok(json!({ "started": true })));
}

#[test]
fn what_stands_in_the_way_is_said_plainly() {
    let (_listener, port) = link_listener();
    // No phone yet: nothing to act on.
    let rig = Rig::new(|s| s.plugged = false, InMemory::new(port), port);
    assert_eq!(rig.ask("phone.install", Value::Null).unwrap_err()["code"], "no_phone");
    assert_eq!(rig.ask("phone.debugging_off", Value::Null).unwrap_err()["code"], "no_phone");
    assert_eq!(rig.ask("phone.reboot", Value::Null).unwrap_err()["code"], "unknown_request");
    assert_eq!(rig.ask("phone.look", json!({ "x": 1 })).unwrap_err()["code"], "unknown_request");
    rig.until("nothing plugged in", |v| v["cable"]["case"] == "none");
    assert_eq!(
        rig.ask("phone.install", Value::Null).unwrap_err(),
        json!({ "code": "no_phone", "words": "No phone is connected yet. Plug it in, unlocked, with USB debugging on." })
    );

    // This PC's app is missing, or is not LiMo's.
    for (kind, line) in [(AppKind::Missing, "LiMo’s app for phones is missing from this PC."), (AppKind::Other, "The app for phones beside this program is not LiMo’s own.")] {
        let rig = Rig::with(|_| {}, InMemory::new(port), port, RigSetup { app: kind, ..RigSetup::default() });
        let look = rig.until("checked", |v| v["phone"]["limo"]["looked"] == true);
        assert_eq!(look["install"]["stage"], "no_app");
        assert_eq!(look["install"]["line"], line);
        assert_eq!(rig.ask("phone.install", Value::Null).unwrap_err(), json!({ "code": "no_app", "words": line }));
    }

    // An Android too old for LiMo: said, and nothing done on it.
    let old = Rig::new(
        |s| {
            s.sdk = 25;
            s.android = "7.1".into();
        },
        InMemory::new(port),
        port,
    );
    let look = old.until("too old", |v| v["cable"]["case"] == "trouble");
    assert_eq!(look["cable"], json!({ "case": "trouble", "line": "This phone has Android 7.1. LiMo needs Android 8.0 or later.", "more": "Unplug the phone." }));
    assert_eq!(look["phone"]["recent_enough"], false);
    assert_eq!(look["checks"][1], json!({ "ok": false, "words": "Android 7.1, older than the Android 8.0 LiMo needs" }));
    assert_eq!(old.ask("phone.install", Value::Null).unwrap_err()["code"], "too_old");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(old.state().seen.clone().auth.len(), 1, "connected once, not again while it stays plugged in");
    assert_eq!(old.state().seen.clone().prompts, 0, "and never asked");

    // Another program holds the phone.
    let held = Rig::new(|s| s.held_by_another = true, InMemory::new(port), port);
    let look = held.until("held by another", |v| v["cable"]["case"] == "trouble");
    assert_eq!(look["cable"]["line"], "Another program on this PC holds the phone’s connection (Android Studio, or Google’s adb).");
    assert_eq!(look["cable"]["more"], "Close it, then unplug the phone and plug it in again.");
}

#[test]
fn a_phone_unplugged_while_it_pairs_has_the_link_carried_again_when_it_is_back() {
    let (_listener, port) = link_listener();
    let service = InMemory::new(port);
    let rig = Rig::new(|_| {}, Arc::clone(&service) as Arc<dyn Service>, port);
    rig.until("connected", |v| v["phone"]["limo"]["looked"] == true);
    assert_eq!(rig.ask("phone.install", Value::Null), Ok(json!({ "started": true })));
    rig.until("installed", |v| v["install"]["stage"] == "done");
    assert_eq!(rig.ask("phone.pair", json!({ "child": "c1" })), Ok(json!({ "started": true })));
    rig.until("waiting", |v| v["pairing"]["stage"] == "waiting");
    let carried = || rig.state().seen.clone().services.iter().filter(|s| s.starts_with(&format!("reverse:forward:tcp:{port}"))).count();
    assert_eq!(carried(), 1);

    rig.state().plugged = false;
    let look = rig.until("unplugged", |v| v["held"] == false);
    assert_eq!(look["pairing"]["stage"], "waiting", "the phone may still pair over the home Wi-Fi");
    rig.state().plugged = true;
    rig.until("back", |v| v["cable"]["case"] == "ok" && v["held"] == true && v["busy"].is_null());
    let deadline = Instant::now() + Duration::from_secs(5);
    while carried() < 2 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(carried(), 2, "the link's port carried again");
    assert_eq!(rig.state().offers.len(), 1, "the offer was handed once");
    assert_eq!(rig.state().seen.clone().installs, 1, "and LiMo put on once");
    service.family().devices.push(a_paired_phone());
    service.family().sessions.push(a_session("cable"));
    rig.until("paired", |v| v["pairing"]["stage"] == "paired" && v["pairing"]["ways"] == json!(["cable"]));
}

#[test]
fn a_pairing_offer_that_runs_out_is_said() {
    let (_listener, port) = link_listener();
    let pace = Pace { longest_pairing: Duration::from_millis(300), ..quick() };
    let rig = Rig::with(
        |s| s.limo = Some(FakeLimo { version_code: 3, version_name: VERSION.into(), other_key: false }),
        InMemory::new(port),
        port,
        RigSetup { pace, ..RigSetup::default() },
    );
    rig.until("checked", |v| v["install"]["stage"] == "not_needed");
    assert_eq!(rig.ask("phone.pair", json!({ "child": "c1" })), Ok(json!({ "started": true })));
    let look = rig.until("ran out", |v| v["pairing"]["stage"] == "failed");
    assert_eq!(look["pairing"]["line"], "The pairing offer ran out before LiMo on the phone paired.");
    assert_eq!(look["pairing"]["advice"], "Try again.");
    // Tried again: a new offer, handed, and waited for.
    assert_eq!(rig.ask("phone.pair", json!({ "child": "c1" })), Ok(json!({ "started": true })));
    rig.until("a new offer handed", |_| rig.state().offers.len() == 2);
    let offers = rig.state().offers.clone();
    assert_ne!(offers[0], offers[1]);
}

#[test]
fn the_real_door_is_asked_for_the_offer_and_its_own_words_are_said() {
    let host = RealDoorHost::start(&pipe_name(), PASSWORD, None);
    let door = Arc::new(Mutex::new(Door::new(host.name(), this_process::as_server().unwrap())));
    let service = Arc::new(DoorService::new(Arc::clone(&door)));
    let (_listener, port) = link_listener();
    let rig = Rig::new(
        |s| s.limo = Some(FakeLimo { version_code: 3, version_name: VERSION.into(), other_key: false }),
        Arc::clone(&service) as Arc<dyn Service>,
        port,
    );
    rig.until("checked", |v| v["install"]["stage"] == "not_needed");

    // Not signed in yet: the door's own words.
    let signed_out = service.ask("devices", json!({ "child": "c1" })).unwrap_err();
    assert_eq!(signed_out.code, "signed_out", "{signed_out:?}");
    assert_eq!(rig.ask("phone.pair", json!({ "child": "c1" })), Ok(json!({ "started": true })));
    let look = rig.until("signed out", |v| v["pairing"]["stage"] == "failed");
    assert_eq!(look["pairing"]["line"], signed_out.words);

    // Signed in, with a child: the real door makes no offer while its link
    // for phones is not running, and says so; nothing reaches the phone.
    let signed = door.lock().unwrap().ask(PageRequest::from_page("sign_in", json!({ "password": PASSWORD })).unwrap()).unwrap();
    assert!(matches!(&signed, DoorReply::Reply(r) if r["outcome"] == "done"), "{signed:?}");
    let child = service.ask("add_child", json!({ "name": "Mira", "age_band": "six_to_nine" })).unwrap();
    let child = child["child_id"].as_str().unwrap().to_string();
    assert_eq!(service.ask("devices", json!({ "child": child })), Ok(json!([])));
    assert_eq!(rig.ask("phone.pair", json!({ "child": child })), Ok(json!({ "started": true })));
    let look = rig.until("no link", |v| v["pairing"]["stage"] == "failed" && v["pairing"]["line"] != signed_out.words.as_str());
    assert_eq!(look["pairing"]["line"], NO_LINK);
    assert!(rig.state().offers.is_empty() && rig.state().reverses.is_empty());
    // And a page may not ask for an offer at all.
    assert!(PageRequest::from_page("make_offer", json!({ "child": child })).is_err());
}

#[test]
fn an_ask_waits_long_for_a_late_allow_and_looks_meanwhile_send_nothing() {
    let (_listener, port) = link_listener();
    let setup = RigSetup { known: false, prompt: Duration::from_secs(8), log: true, ..RigSetup::default() };
    let rig = Rig::with(|s| s.person = Person::Away, InMemory::new(port), port, setup);
    rig.until("the phone does not know this PC", |v| v["cable"]["case"] == "unknown");
    assert_eq!(rig.ask("phone.ask", Value::Null), Ok(json!({ "started": true })));
    rig.until("the phone asks", |v| v["cable"]["case"] == "allow");
    let shown = rig.state().seen.clone().auth;
    assert_eq!(shown.iter().filter(|a| **a == AUTH_RSAPUBLICKEY).count(), 1, "{shown:?}");
    // The screens keep looking while the person finds the phone.
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(2_500) {
        let look = rig.ask("phone.look", Value::Null).unwrap();
        assert_eq!(look["cable"]["case"], "allow", "{look}");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(rig.state().seen.clone().auth, shown, "nothing more was sent while it waited");
    let look = rig.until("a second", |v| v["asked_secs"].as_u64().is_some_and(|s| s >= 2));
    assert!(look["asked_secs"].as_u64().unwrap() < 8, "{look}");
    // A late Allow, well into the wait, still connects.
    rig.state().person = Person::Allows;
    rig.until("connected", |v| v["cable"]["case"] == "ok" && v["busy"].is_null());
    let seen = rig.state().seen.clone();
    assert_eq!((seen.prompts, seen.auth.iter().filter(|a| **a == AUTH_RSAPUBLICKEY).count()), (1, 1));
    assert_eq!(rig.ask("phone.ask", Value::Null).unwrap_err()["code"], "already_connected");
    // The window's log: a line for the phone that did not know this PC, and
    // one for the ask, with no key and no fingerprint.
    let log = std::fs::read_to_string(rig.log_path.as_ref().unwrap()).unwrap();
    assert!(log.contains("a phone is plugged in that does not know this PC; it was not asked"), "{log}");
    assert!(log.contains("the guardian asked: the phone is asked, once, to allow this PC"), "{log}");
    assert!(log.contains("the phone allowed this PC"), "{log}");
    assert!(!log.contains(&fingerprint()), "{log}");
}

#[test]
fn an_ask_ends_when_the_phone_goes_and_at_done() {
    let (_listener, port) = link_listener();
    let long = RigSetup { known: false, prompt: Duration::from_secs(60), ..RigSetup::default() };
    // The phone unplugged while it asks: the wait ends with it.
    let rig = Rig::with(|s| s.person = Person::Away, InMemory::new(port), port, long);
    rig.until("unknown", |v| v["cable"]["case"] == "unknown");
    rig.ask("phone.ask", Value::Null).unwrap();
    rig.until("asking", |v| v["cable"]["case"] == "allow");
    let started = Instant::now();
    rig.state().plugged = false;
    rig.until("the wait ended", |v| v["cable"]["case"] != "allow" && v["busy"].is_null());
    assert!(started.elapsed() < Duration::from_secs(5), "{:?}", started.elapsed());
    drop(rig);
    // Add a phone closed while it asks: given up at once.
    let long = RigSetup { known: false, prompt: Duration::from_secs(60), ..RigSetup::default() };
    let rig = Rig::with(|s| s.person = Person::Away, InMemory::new(port), port, long);
    rig.until("unknown", |v| v["cable"]["case"] == "unknown");
    rig.ask("phone.ask", Value::Null).unwrap();
    rig.until("asking", |v| v["cable"]["case"] == "allow");
    assert_eq!(rig.ask("phone.done", Value::Null), Ok(json!({ "done": true })));
    let started = Instant::now();
    drop(rig);
    assert!(started.elapsed() < Duration::from_secs(5), "the ask was given up: {:?}", started.elapsed());
}

#[test]
fn an_ask_is_refused_when_it_cannot_be_made() {
    let (_listener, port) = link_listener();
    let rig = Rig::with(|s| s.plugged = false, InMemory::new(port), port, RigSetup { known: false, ..RigSetup::default() });
    assert_eq!(rig.ask("phone.ask", Value::Null).unwrap_err()["code"], "no_phone");
    rig.until("nothing plugged in", |v| v["cable"]["case"] == "none");
    assert_eq!(rig.ask("phone.ask", Value::Null).unwrap_err()["code"], "no_phone");
    assert_eq!(rig.ask("phone.ask", json!({ "x": 1 })).unwrap_err()["code"], "unknown_request");
    drop(rig);
    let rig = Rig::new(|_| {}, InMemory::new(port), port);
    rig.until("connected", |v| v["cable"]["case"] == "ok" && v["busy"].is_null());
    assert_eq!(
        rig.ask("phone.ask", Value::Null).unwrap_err(),
        json!({ "code": "already_connected", "words": "The phone is connected already: it knows this PC." })
    );
    assert_eq!(rig.state().seen.clone().prompts, 0, "a phone that knows this PC is never asked");
}