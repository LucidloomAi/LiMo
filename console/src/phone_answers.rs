//! The answers of the console's requests for a phone, as files, for the Add
//! a phone screens' own tests: limo-cable's fake phone played through the
//! console's real desk (`phone_tests.rs`'s rig), each answer written as the
//! page gets it, one JSON file per request and case, into
//! `phone/app/tests/window-answers/`: the answers the window itself gives,
//! as against the door's in `console-answers/`. A few moments pass too fast
//! for the fake phone to hold (looking, connecting, sending, installing, an
//! install the phone's going ends, asking the service, handing, switching
//! off); their files come from the same answer code (`phone::look`), given
//! the state of that moment.
//!
//! What would change from run to run is a fixed sample, the door's own
//! where it has one: the key's fingerprint (the drawing's), the child
//! `child-0000000000000001` and the phone `device-0000000000000002`, "Asha's
//! phone". No offer, key or password is in any file, nor anything of this
//! PC, and the test checks so. A file is written only when it changed, and
//! a `.json` file in the folder that this run did not write is removed; its
//! README.md is left as it is.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use limo_cable::fake::{CableKind, Driver, FakeLimo, Person};
use limo_cable::flow::{Event, State, step};
use limo_cable::open::{Interface, Look};
use limo_cable::phone::PhoneFacts;
use limo_cable::usb::{Kind, UsbDevice};
use serde_json::{Value, json};

use crate::phone::look::{DebugStage, Device, Install, InstallStage, PairStage, Pairing, Shared, Words, phone_name};
use crate::phone::Service;
use crate::phone_tests::{AppKind, InMemory, Rig, RigSetup, fingerprint, link_listener, quick};
use crate::test_apk::VERSION;

/// The drawing's fingerprint, written for the test key's.
const SAMPLE_FINGERPRINT: &str = "3C:91:5A:07:E2:4B:D8:16:AF:62:0C:B9:75:18:4E:D3";
const CHILD: &str = "child-0000000000000001";
const PHONE: &str = "device-0000000000000002";
/// What the door says while its link for phones is not running.
const NO_LINK: &str = "The link for phones is not running on this PC, so no phone can pair with it now. The service’s log says why.";

fn answers_folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("phone").join("app").join("tests").join("window-answers")
}

/// The phone the door pairs, as its `devices` lists it.
fn paired_phone() -> Value {
    json!({ "device_id": PHONE, "child_id": CHILD, "name": "Asha’s phone", "kind": "portable", "platform": "android",
            "paired_at": 1_794_998_143, "last_heard_at": 1_794_998_143 })
}

fn session(way: &str) -> Value {
    json!({ "device_id": PHONE, "way": way, "opened_at": 1_794_998_143, "last_report_at": 1_794_998_143 })
}

/// The files of one run.
struct Files {
    fingerprint: String,
    files: BTreeMap<String, String>,
}

impl Files {
    fn new() -> Files {
        Files { fingerprint: fingerprint(), files: BTreeMap::new() }
    }

    /// Keeps one file: the request as the page asked it, and the answer as
    /// the page got it (`answer`), or the error it was rejected with
    /// (`error`).
    fn keep(&mut self, file: &str, request: &str, case: &str, args: &Value, got: &Result<Value, Value>) {
        let file = file.to_string();
        assert!(!self.files.contains_key(&file), "{file} twice");
        let mut asked = json!({ "name": request });
        if !args.is_null() {
            asked["args"] = args.clone();
        }
        let pretty = |v: &Value| serde_json::to_string_pretty(v).unwrap().replace('\n', "\n  ");
        let (key, value) = match got {
            Ok(v) => ("answer", v),
            Err(e) => ("error", e),
        };
        let mut text = String::from("{\n");
        text.push_str(&format!("  \"request\": {},\n", json!(request)));
        text.push_str(&format!("  \"case\": {},\n", json!(case.replace('\'', "\u{2019}"))));
        text.push_str(&format!("  \"asked\": {},\n", pretty(&asked)));
        text.push_str(&format!("  \"{key}\": {}\n}}\n", pretty(value)));
        let text = text.replace(&self.fingerprint, SAMPLE_FINGERPRINT);
        self.files.insert(file, text);
    }

    /// Asks the rig and keeps what it answered.
    fn ask(&mut self, rig: &Rig, file: &str, request: &str, case: &str, args: Value) -> Result<Value, Value> {
        let got = rig.ask(request, args.clone());
        self.keep(file, request, case, &args, &got);
        got
    }

    /// `phone.look` until `test` holds, and keeps that answer.
    fn look_until(&mut self, rig: &Rig, file: &str, case: &str, test: impl Fn(&Value) -> bool) -> Value {
        let look = rig.until(case, test);
        self.keep(file, "phone.look", case, &Value::Null, &Ok(look.clone()));
        look
    }

    /// A moment's answer, from the answer code itself.
    fn moment(&mut self, file: &str, case: &str, shared: &Shared) {
        self.keep(file, "phone.look", case, &Value::Null, &Ok(shared.answer()));
    }

    fn write(&self) {
        let pc = std::env::var("COMPUTERNAME").unwrap_or_default().to_lowercase();
        let user = std::env::var("USERNAME").unwrap_or_default().to_lowercase();
        for (file, text) in &self.files {
            let lower = text.to_lowercase();
            for never in ["limo1:", "secret", r"\\users\\", "/users/"] {
                assert!(!lower.contains(never), "{file} holds {never}");
            }
            for mine in [&pc, &user].into_iter().filter(|n| n.len() >= 3) {
                assert!(!lower.contains(mine.as_str()), "{file} names this PC or its user");
            }
            assert!(!text.contains(&self.fingerprint) || self.fingerprint == SAMPLE_FINGERPRINT, "{file} holds the test key's fingerprint");
        }
        let folder = answers_folder();
        // The screens' tests live beside the console; without them there is
        // nowhere to write.
        if !folder.parent().is_some_and(|tests| tests.is_dir()) {
            return;
        }
        std::fs::create_dir_all(&folder).unwrap();
        for (file, text) in &self.files {
            let path = folder.join(format!("{file}.json"));
            let there = std::fs::read_to_string(&path).ok().map(|t| t.replace("\r\n", "\n"));
            if there.as_deref() != Some(text.as_str()) {
                std::fs::write(&path, text).unwrap();
            }
        }
        for entry in std::fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            if path.extension().is_some_and(|e| e == "json") && !self.files.contains_key(&stem) {
                std::fs::remove_file(&path).unwrap();
            }
        }
    }
}

/// The test phone as Windows lists it: a Galaxy on Samsung's driver.
fn galaxy(interfaces: usize) -> Look {
    let interface = Interface {
        name: Some("SAMSUNG Android ADB Interface".to_string()),
        driver: Some("WINUSB".to_string()),
        above: Some("dg_ssudbus".to_string()),
    };
    Look {
        windows: Ok(vec![
            UsbDevice { kind: Kind::PortableDevice, name: Some("Galaxy S20 Ultra 5G".to_string()), bus: "USB".to_string(), working: true },
            UsbDevice { kind: Kind::DebugInterface, name: Some("SAMSUNG Android ADB Interface".to_string()), bus: "USB".to_string(), working: true },
        ]),
        interfaces: Ok(vec![interface; interfaces]),
    }
}

/// The test phone connected and looked at, LiMo not on it yet.
fn connected() -> Shared {
    let facts = PhoneFacts { manufacturer: "samsung".into(), model: "SM-G988B".into(), android: "13".into(), sdk: Some(33), usb_debugging: Some(true) };
    let look = galaxy(1);
    let mut s = Shared::default();
    s.phone_name = phone_name(&look);
    s.look = Some(look);
    s.state = step(step(State::Ready { name: None }, Event::Connecting), Event::Connected(facts.clone()));
    s.state = step(s.state.clone(), Event::LimoChecked(None));
    s.facts = Some(facts);
    s.limo = Some(None);
    s.app_version = Some(VERSION.to_string());
    s.held = true;
    s
}

/// The same, LiMo put on it in this run.
fn installed() -> Shared {
    let mut s = connected();
    s.state = step(s.state.clone(), Event::Installed(Ok(())));
    s.limo = Some(Some(limo_cable::phone::LimoOnPhone { version_code: None, version_name: Some(VERSION.to_string()) }));
    s.install = Install { stage: InstallStage::Done, base: None, bytes: Some(4_000_000), words: None };
    s
}

#[test]
fn the_phone_requests_answers_are_written_for_the_screens_in_every_state() {
    let mut f = Files::new();
    let (_listener, port) = link_listener();

    // The moments too short for the fake phone to hold.
    f.moment("look-looking", "before the first look at the USB ports", &Shared::default());
    let mut s = Shared { look: Some(galaxy(1)), state: State::Connecting, busy: Some("connecting"), ..Shared::default() };
    f.moment("look-connecting", "a phone with USB debugging on, being connected quietly", &s);
    s = Shared {
        look: Some(galaxy(1)),
        state: State::Asking,
        busy: Some("asking"),
        fingerprint: Some(fingerprint()),
        asked_at: Some(Instant::now() - Duration::from_secs(12)),
        ..Shared::default()
    };
    f.moment("look-allow", "the phone asked twelve seconds ago, waiting for Allow, with the key's fingerprint", &s);
    s = Shared { look: Some(galaxy(2)), state: State::TwoPhones(2), ..Shared::default() };
    f.moment("look-two-phones", "two phones plugged in", &s);
    let counter = Arc::new(AtomicU64::new(1_000 + 2_480_000));
    s = connected();
    s.counter = Some(Arc::clone(&counter));
    s.install = Install { stage: InstallStage::Sending, base: Some(1_000), bytes: Some(4_000_000), words: None };
    s.busy = Some("installing");
    f.moment("look-sending", "LiMo going on the phone: 62 in a hundred of the app sent", &s);
    counter.store(1_000 + 4_000_000, Ordering::Relaxed);
    f.moment("look-installing", "LiMo going on the phone: all of the app sent, the phone installing it", &s);
    // The phone's connection ends while LiMo goes on it: the install failed,
    // and the phone is let go, as the console's thread does it.
    s.install = Install {
        stage: InstallStage::Failed,
        base: None,
        bytes: Some(4_000_000),
        words: Some(Words::new(crate::phone::worker::INSTALL_LOST, Some(crate::phone::worker::PLUG_IN_AGAIN))),
    };
    s.busy = None;
    s.held = false;
    s.state = step(s.state.clone(), Event::Lost);
    f.moment("look-install-failed", "the phone's connection ended while LiMo went on it", &s);
    s = installed();
    s.pairing = Pairing { stage: PairStage::AskingService, ..Pairing::default() };
    s.busy = Some("pairing");
    f.moment("look-asking-the-service", "pairing: asking LiMo's service for an offer", &s);
    s.pairing.stage = PairStage::Handing;
    f.moment("look-handing", "pairing: handing the offer to LiMo on the phone", &s);
    s = installed();
    s.pairing = Pairing {
        stage: PairStage::Paired,
        device: Some(Device { device_id: PHONE.into(), name: "Asha’s phone".into() }),
        ways: vec!["cable".into(), "home_network".into()],
        words: None,
    };
    s.debugging = DebugStage::SwitchingOff;
    s.busy = Some("switching_off");
    f.moment("look-debugging-switching-off", "paired, USB debugging being switched off", &s);
    s.debugging = DebugStage::StillOn;
    s.busy = None;
    f.moment("look-debugging-still-on", "paired, USB debugging did not switch off", &s);

    // No phone yet.
    let service = InMemory::new(port);
    // A phone that has never seen this PC.
    let rig = Rig::with(
        |s| s.cable = CableKind::ChargeOnly,
        Arc::clone(&service) as Arc<dyn Service>,
        port,
        RigSetup { known: false, ..RigSetup::default() },
    );
    f.ask(&rig, "ask-no-phone", "phone.ask", "no phone is plugged in", Value::Null).unwrap_err();
    f.ask(&rig, "install-no-phone", "phone.install", "no phone is connected", Value::Null).unwrap_err();
    f.ask(&rig, "debugging-off-no-phone", "phone.debugging_off", "no phone is connected", Value::Null).unwrap_err();
    f.ask(&rig, "look-with-arguments", "phone.look", "asked with arguments it does not take", json!({ "child": CHILD })).unwrap_err();

    // The whole of it: the cable's cases, the question, LiMo put on, paired,
    // USB debugging off.
    f.look_until(&rig, "look-none", "no phone on a USB port (a cable that only charges)", |v| v["cable"]["case"] == "none");
    {
        let mut st = rig.state();
        st.cable = CableKind::Data;
        st.debugging = false;
    }
    f.look_until(&rig, "look-off", "a phone plugged in, with USB debugging off", |v| v["cable"]["case"] == "off");
    {
        let mut st = rig.state();
        st.debugging = true;
        st.driver = Driver::None;
    }
    f.look_until(&rig, "look-driver", "a phone whose debugging side has no working driver", |v| v["cable"]["case"] == "driver");
    {
        let mut st = rig.state();
        st.driver = Driver::SamsungWinUsb;
        st.person = Person::Away;
    }
    f.look_until(&rig, "look-unknown", "a phone that does not know this PC yet: nothing was shown to it", |v| v["cable"]["case"] == "unknown");
    assert_eq!(f.ask(&rig, "ask-started", "phone.ask", "the phone asked, once, to allow this PC", Value::Null), Ok(json!({ "started": true })));
    // Played here; its file is the moment above, with a fixed asked_secs.
    rig.until("the phone asks", |v| v["cable"]["case"] == "allow");
    f.ask(&rig, "ask-busy", "phone.ask", "asked again while the phone asks", Value::Null).unwrap_err();
    f.look_until(&rig, "look-not-allowed", "the phone was asked, and nobody allowed this PC in time", |v| v["cable"]["case"] == "trouble");
    rig.state().person = Person::Allows;
    rig.ask("phone.ask", Value::Null).unwrap();
    f.look_until(&rig, "look-ok", "connected and looked at: LiMo not on the phone yet", |v| {
        v["cable"]["case"] == "ok" && v["phone"]["limo"]["looked"] == true && v["busy"].is_null()
    });
    f.ask(&rig, "ask-already-connected", "phone.ask", "asked once the phone is connected", Value::Null).unwrap_err();
    f.ask(&rig, "pair-not-yet", "phone.pair", "pairing asked before LiMo is on the phone", json!({ "child": CHILD })).unwrap_err();
    assert_eq!(f.ask(&rig, "install-started", "phone.install", "LiMo put on the phone", Value::Null), Ok(json!({ "started": true })));
    f.ask(&rig, "install-busy", "phone.install", "asked again while LiMo goes on the phone", Value::Null).unwrap_err();
    f.look_until(&rig, "look-installed", "LiMo put on the phone in this run", |v| v["install"]["stage"] == "done" && v["busy"].is_null());
    f.ask(&rig, "install-not-needed", "phone.install", "asked again once LiMo is on the phone", Value::Null).unwrap();
    f.ask(&rig, "pair-no-child", "phone.pair", "pairing asked without the child", json!({})).unwrap_err();
    assert_eq!(f.ask(&rig, "pair-started", "phone.pair", "pairing the phone with Asha", json!({ "child": CHILD })), Ok(json!({ "started": true })));
    f.ask(&rig, "pair-busy", "phone.pair", "asked again while the pairing goes on", json!({ "child": CHILD })).unwrap_err();
    f.look_until(&rig, "look-pairing", "the offer handed, waiting for LiMo on the phone to pair", |v| {
        v["pairing"]["stage"] == "waiting" && v["busy"].is_null()
    });
    {
        let mut family = service.family();
        family.devices.push(paired_phone());
        family.sessions.push(session("cable"));
    }
    f.look_until(&rig, "look-paired-over-the-cable", "paired, the phone reaching the PC over the cable", |v| v["pairing"]["stage"] == "paired");
    f.ask(&rig, "pair-already-paired", "phone.pair", "asked again once paired", json!({ "child": CHILD })).unwrap();
    service.family().sessions.push(session("home_network"));
    f.look_until(&rig, "look-paired-home-wifi", "paired, the phone reaching the PC over the home Wi-Fi too", |v| {
        v["pairing"]["ways"] == json!(["cable", "home_network"])
    });
    assert_eq!(f.ask(&rig, "debugging-off-started", "phone.debugging_off", "USB debugging switched off", Value::Null), Ok(json!({ "started": true })));
    f.look_until(&rig, "look-debugging-off", "USB debugging switched off, the phone still plugged in", |v| {
        v["debugging"]["stage"] == "off" && v["cable"]["case"] == "off"
    });
    f.ask(&rig, "debugging-off-already-off", "phone.debugging_off", "asked again once it is off", Value::Null).unwrap();
    f.ask(&rig, "done", "phone.done", "Add a phone closed: the phone let go", Value::Null).unwrap();
    drop(rig);

    // LiMo there already, at this PC's version, and older.
    let service = InMemory::new(port);
    let rig = Rig::new(
        |s| s.limo = Some(FakeLimo { version_code: 3, version_name: VERSION.into(), other_key: false }),
        Arc::clone(&service) as Arc<dyn Service>,
        port,
    );
    f.look_until(&rig, "look-limo-already-on", "LiMo on the phone already, at this PC's version", |v| {
        v["install"]["stage"] == "not_needed" && v["busy"].is_null()
    });
    // The service cannot make an offer, then an offer runs out.
    service.family().offer_problem = Some(("failed".into(), NO_LINK.into()));
    rig.ask("phone.pair", json!({ "child": CHILD })).unwrap();
    f.look_until(&rig, "look-pairing-refused-by-the-service", "the service made no offer, in its own words", |v| v["pairing"]["stage"] == "failed");
    drop(rig);
    let pace = crate::phone::Pace { longest_pairing: Duration::from_millis(200), ..quick() };
    let rig = Rig::with(
        |s| s.limo = Some(FakeLimo { version_code: 3, version_name: VERSION.into(), other_key: false }),
        InMemory::new(port),
        port,
        RigSetup { pace, ..RigSetup::default() },
    );
    rig.until("checked", |v| v["install"]["stage"] == "not_needed");
    rig.ask("phone.pair", json!({ "child": CHILD })).unwrap();
    f.look_until(&rig, "look-pairing-ran-out", "the offer ran out before LiMo on the phone paired", |v| v["pairing"]["stage"] == "failed");
    drop(rig);
    let rig = Rig::new(|s| s.limo = Some(FakeLimo { version_code: 2, version_name: "0.2.0".into(), other_key: false }), InMemory::new(port), port);
    f.look_until(&rig, "look-limo-older", "an older LiMo on the phone", |v| v["phone"]["limo"]["looked"] == true && v["busy"].is_null());
    drop(rig);
    let rig = Rig::new(|s| s.limo = Some(FakeLimo { version_code: 2, version_name: "0.2.0".into(), other_key: true }), InMemory::new(port), port);
    rig.until("checked", |v| v["phone"]["limo"]["looked"] == true && v["busy"].is_null());
    rig.ask("phone.install", Value::Null).unwrap();
    f.look_until(&rig, "look-install-refused", "the phone refused LiMo: one of another maker is on it", |v| v["install"]["stage"] == "refused");
    drop(rig);

    // What stands in the way.
    let rig = Rig::with(|_| {}, InMemory::new(port), port, RigSetup { app: AppKind::Missing, ..RigSetup::default() });
    f.look_until(&rig, "look-no-app", "this PC's app for phones is missing", |v| v["phone"]["limo"]["looked"] == true && v["busy"].is_null());
    f.ask(&rig, "install-no-app", "phone.install", "this PC's app for phones is missing", Value::Null).unwrap_err();
    drop(rig);
    let rig = Rig::new(
        |s| {
            s.sdk = 25;
            s.android = "7.1".into();
        },
        InMemory::new(port),
        port,
    );
    f.look_until(&rig, "look-android-too-old", "a phone with an Android older than LiMo needs", |v| v["cable"]["case"] == "trouble" && v["busy"].is_null());
    f.ask(&rig, "install-too-old", "phone.install", "the phone's Android is too old", Value::Null).unwrap_err();
    drop(rig);
    let rig = Rig::new(|s| s.held_by_another = true, InMemory::new(port), port);
    f.look_until(&rig, "look-held-by-another-program", "another program on the PC holds the phone", |v| v["cable"]["case"] == "trouble");
    drop(rig);
    let rig = Rig::new(|_| {}, InMemory::new(port), port);
    rig.until("connected", |v| v["phone"]["limo"]["looked"] == true && v["busy"].is_null());
    rig.state().plugged = false;
    f.look_until(&rig, "look-connection-ended", "the phone unplugged while connected", |v| v["cable"]["case"] == "trouble");
    drop(rig);

    f.write();
    assert!(f.files.len() >= 40, "{}", f.files.len());
}
