//! What `phone.look` answers: where adding the phone stands, shared by the
//! console's thread, which changes it, and the requests, which read it. The
//! words are the ones the Add a phone screens show (`design/screens/
//! console.html`, frames A1 to A9 and the cable's strips); where limo-cable
//! has the words already (`flow::tell`, `install_advice`), they are its own.

#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use limo_cable::flow::{self, Problem, State};
use limo_cable::open::Look;
use limo_cable::phone::{LimoOnPhone, PhoneFacts};
use limo_cable::usb::{Case, Kind};
use serde_json::{Value, json};

/// Said when a request needs a connected phone and there is none.
pub(crate) const NO_PHONE: &str = "No phone is connected yet. Plug it in, unlocked, with USB debugging on.";
/// Said when a request comes while another step runs.
pub(crate) const BUSY: &str = "The phone is busy with another step. Wait a moment, then try again.";
/// Said when pairing is asked before LiMo is on the phone.
pub(crate) const NOT_INSTALLED: &str = "LiMo is not on the phone yet.";

/// A line to show, and what to do, if anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Words {
    pub line: String,
    pub advice: Option<String>,
}

impl Words {
    pub(crate) fn new(line: &str, advice: Option<&str>) -> Words {
        Words { line: line.to_string(), advice: advice.map(str::to_string) }
    }
}

/// limo-cable's words, with the apostrophes the screens are set with.
pub(crate) fn typeset(text: &str) -> String {
    text.replace('\'', "\u{2019}")
}

/// Where putting LiMo on the phone stands. `Sending` is shown as
/// `installing` once all of the app is on the phone, or when the cable
/// does not count what it sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum InstallStage {
    #[default]
    NotAsked,
    NotNeeded,
    Sending,
    Done,
    Refused,
    Failed,
    NoApp,
}

/// Putting LiMo on the phone.
#[derive(Debug, Clone, Default)]
pub(crate) struct Install {
    pub stage: InstallStage,
    /// The count of bytes sent when the install began.
    pub base: Option<u64>,
    /// The app's size.
    pub bytes: Option<u64>,
    /// What went wrong.
    pub words: Option<Words>,
}

/// Where pairing stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum PairStage {
    #[default]
    NotAsked,
    AskingService,
    Handing,
    Waiting,
    Paired,
    Failed,
}

/// The device the service paired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Device {
    pub device_id: String,
    pub name: String,
}

/// Pairing the phone.
#[derive(Debug, Clone, Default)]
pub(crate) struct Pairing {
    pub stage: PairStage,
    pub device: Option<Device>,
    /// Every way the phone was seen to reach the PC since it paired
    /// (`cable`, `home_network`), in the order first seen.
    pub ways: Vec<String>,
    /// What went wrong.
    pub words: Option<Words>,
}

/// USB debugging, as far as LiMo switching it off goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum DebugStage {
    /// Not switched off by LiMo.
    #[default]
    On,
    SwitchingOff,
    Off,
    StillOn,
}

/// Everything `phone.look` reads.
#[derive(Debug)]
pub(crate) struct Shared {
    /// The cable's state machine.
    pub state: State,
    /// The last look at the USB ports; None before the first.
    pub look: Option<Look>,
    /// The phone's name, as Windows shows it, from when it connected.
    pub phone_name: Option<String>,
    /// What the phone is, once connected.
    pub facts: Option<PhoneFacts>,
    /// LiMo on the phone: not looked at (None), not there (Some(None)), or
    /// there (Some(Some)).
    pub limo: Option<Option<LimoOnPhone>>,
    /// The version of the app this PC carries.
    pub app_version: Option<String>,
    /// Why this PC's app cannot be put on a phone.
    pub app_problem: Option<Words>,
    /// The key's fingerprint, while the phone asks.
    pub fingerprint: Option<String>,
    /// A problem of the console's own with the cable (its key).
    pub cable_trouble: Option<Words>,
    pub install: Install,
    pub pairing: Pairing,
    pub debugging: DebugStage,
    /// The call running now.
    pub busy: Option<&'static str>,
    /// True while the console holds the phone open.
    pub held: bool,
    /// The cable's count of bytes sent.
    pub counter: Option<Arc<AtomicU64>>,
}

impl Default for Shared {
    /// Before the first look: no phone, nothing asked.
    fn default() -> Shared {
        Shared {
            state: State::NotReady(Case::NoPhone),
            look: None,
            phone_name: None,
            facts: None,
            limo: None,
            app_version: None,
            app_problem: None,
            fingerprint: None,
            cable_trouble: None,
            install: Install::default(),
            pairing: Pairing::default(),
            debugging: DebugStage::default(),
            busy: None,
            held: false,
            counter: None,
        }
    }
}

/// The shared state, locked.
pub(crate) fn lock(shared: &Mutex<Shared>) -> MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(|p| p.into_inner())
}

/// The name Windows shows for the phone itself ("Galaxy S20 Ultra 5G"):
/// its portable device's, when it has one.
pub(crate) fn phone_name(look: &Look) -> Option<String> {
    look.windows.as_ref().ok()?.iter().find(|d| d.kind == Kind::PortableDevice).and_then(|d| d.name.clone())
}

fn strip(case: &str, line: &str, more: &str) -> (&'static str, String, String) {
    let case = match case {
        "looking" => "looking",
        "none" => "none",
        "off" => "off",
        "driver" => "driver",
        "connecting" => "connecting",
        "allow" => "allow",
        "ok" => "ok",
        _ => "trouble",
    };
    (case, line.to_string(), more.to_string())
}

fn trouble(state: &State) -> (&'static str, String, String) {
    let told = flow::tell(state);
    ("trouble", typeset(&told.line), typeset(&told.advice.unwrap_or_default()))
}

/// The cable's strip for a case of no phone LiMo can open, as drawn.
fn case_strip(case: &Case, state: &State) -> (&'static str, String, String) {
    match case {
        Case::NoPhone => strip(
            "none",
            "No phone on a USB port.",
            "If it is plugged in, the cable may only charge it. Try the one that came with the phone.",
        ),
        Case::PhoneWithoutDebugging { .. } => strip("off", "A phone is plugged in.", "USB debugging is off on it."),
        Case::NoDriver { .. } | Case::DebuggingNotListed { .. } => strip(
            "driver",
            "No working driver for this phone.",
            "Install its maker\u{2019}s USB driver for Windows, such as Samsung\u{2019}s Android USB Driver.",
        ),
        Case::CannotLook(_) => trouble(state),
    }
}

impl Shared {
    /// May a step be started on the phone now: connected, and held.
    pub(crate) fn may_act(&self) -> Result<(), Value> {
        match &self.state {
            State::Connected(_) if self.held => Ok(()),
            State::Stopped(Problem::TooOld(_)) => Err(json!({ "code": "too_old", "words": typeset(&flow::tell(&self.state).line) })),
            _ => Err(json!({ "code": "no_phone", "words": NO_PHONE })),
        }
    }

    fn shown_name(&self, facts: &PhoneFacts) -> String {
        self.phone_name.clone().unwrap_or_else(|| if facts.model.is_empty() { "the phone".to_string() } else { facts.model.clone() })
    }

    /// The cable's strip: its case, its line and the line under it.
    pub(crate) fn cable(&self) -> (&'static str, String, String) {
        if let Some(words) = &self.cable_trouble {
            return ("trouble", words.line.clone(), words.advice.clone().unwrap_or_default());
        }
        let Some(look) = &self.look else {
            return strip("looking", "Looking at this PC\u{2019}s USB ports.", "");
        };
        match &self.state {
            State::NotReady(case) => case_strip(case, &self.state),
            State::Ready { .. } | State::Connecting => strip("connecting", "A phone is plugged in.", "Connecting to it."),
            State::Asking => strip("allow", "A phone is plugged in.", "Waiting for Allow on the phone."),
            State::Connected(progress) => {
                strip("ok", &format!("Connected: {}.", self.shown_name(&progress.facts)), &format!("Android {}.", progress.facts.android))
            }
            // After USB debugging is off, what is plugged in now.
            State::Done => match flow::seen(look) {
                State::NotReady(case) => case_strip(&case, &self.state),
                _ => strip("trouble", "USB debugging is on again on the phone.", "Unplug the phone."),
            },
            other => trouble(other),
        }
    }

    fn phone(&self) -> Value {
        let Some(facts) = &self.facts else { return Value::Null };
        let limo = match &self.limo {
            None => json!({ "looked": false, "on_it": null, "version": null }),
            Some(None) => json!({ "looked": true, "on_it": false, "version": null }),
            Some(Some(limo)) => json!({ "looked": true, "on_it": true, "version": limo.version_name }),
        };
        json!({
            "name": self.shown_name(facts),
            "model": facts.model,
            "maker": facts.manufacturer,
            "android": facts.android,
            "recent_enough": !facts.too_old(),
            "limo": limo,
            "this_pc_carries": self.app_version,
        })
    }

    /// The checks before installing (A4), once the phone is connected.
    fn checks(&self) -> Value {
        let Some(facts) = &self.facts else { return json!([]) };
        let check = |ok: bool, words: String| json!({ "ok": ok, "words": words });
        let mut list = vec![check(true, "One phone plugged in".to_string())];
        list.push(if facts.too_old() {
            check(false, format!("Android {}, older than the Android 8.0 LiMo needs", facts.android))
        } else {
            check(true, format!("Android {}, recent enough for LiMo", facts.android))
        });
        match &self.limo {
            None => {}
            Some(None) => list.push(check(true, "LiMo is not on it yet".to_string())),
            Some(Some(limo)) => list.push(check(
                true,
                match (&limo.version_name, &self.app_version) {
                    (Some(on), Some(carried)) if on == carried => format!("LiMo {on} is on it already"),
                    (Some(on), Some(carried)) => format!("LiMo {on} is on it, and this PC carries {carried}"),
                    _ => "LiMo is on it".to_string(),
                },
            )),
        }
        Value::Array(list)
    }

    fn install_answer(&self) -> Value {
        let install = &self.install;
        let fraction = |sent: u64, bytes: u64| ((sent as f64 / bytes as f64).min(1.0) * 100.0).floor() / 100.0;
        let (stage, sent) = match install.stage {
            InstallStage::NotAsked => ("not_asked", None),
            InstallStage::NotNeeded => ("not_needed", None),
            InstallStage::Sending => match (&self.counter, install.base, install.bytes) {
                (Some(counter), Some(base), Some(bytes)) if bytes > 0 => {
                    let sent = counter.load(Ordering::Relaxed).saturating_sub(base);
                    if sent >= bytes { ("installing", Some(1.0)) } else { ("sending", Some(fraction(sent, bytes))) }
                }
                (Some(_), None, Some(_)) => ("sending", Some(0.0)),
                _ => ("installing", None),
            },
            InstallStage::Done => ("done", None),
            InstallStage::Refused => ("refused", None),
            InstallStage::Failed => ("failed", None),
            InstallStage::NoApp => ("no_app", None),
        };
        let version = self.limo.as_ref().and_then(|l| l.as_ref()).and_then(|l| l.version_name.clone()).or_else(|| self.app_version.clone());
        let words = match (stage, &install.words) {
            (_, Some(words)) => Some(words.clone()),
            ("not_needed", None) => Some(Words {
                line: match version {
                    Some(v) => format!("LiMo {v} is on the phone already."),
                    None => "LiMo is on the phone already.".to_string(),
                },
                advice: None,
            }),
            ("sending", None) => Some(Words::new("Sending LiMo to the phone.", None)),
            ("installing", None) => Some(Words::new("The phone is installing LiMo.", None)),
            ("done", None) => Some(Words::new("LiMo is on the phone.", None)),
            _ => None,
        };
        json!({
            "stage": stage,
            "sent": sent,
            "line": words.as_ref().map(|w| w.line.clone()),
            "advice": words.and_then(|w| w.advice),
        })
    }

    fn pairing_answer(&self) -> Value {
        let p = &self.pairing;
        let stage = match p.stage {
            PairStage::NotAsked => "not_asked",
            PairStage::AskingService => "asking_the_service",
            PairStage::Handing => "handing",
            PairStage::Waiting => "waiting",
            PairStage::Paired => "paired",
            PairStage::Failed => "failed",
        };
        let words = match (&p.words, p.stage) {
            (Some(words), _) => Some(words.clone()),
            (None, PairStage::AskingService) => Some(Words::new("Asking LiMo\u{2019}s service for a pairing.", None)),
            (None, PairStage::Handing) => Some(Words::new("Handing the pairing to LiMo on the phone.", None)),
            (None, PairStage::Waiting) => Some(Words::new("Waiting for LiMo on the phone to pair with this PC.", None)),
            (None, PairStage::Paired) => Some(Words::new(
                if p.ways.iter().any(|w| w == "cable") {
                    "Over the cable, with this PC."
                } else if p.ways.iter().any(|w| w == "home_network") {
                    "Over the home Wi-Fi, with this PC."
                } else {
                    "With this PC."
                },
                None,
            )),
            _ => None,
        };
        json!({
            "stage": stage,
            "device": p.device.as_ref().map(|d| json!({ "device_id": d.device_id, "name": d.name })),
            "ways": p.ways,
            "line": words.as_ref().map(|w| w.line.clone()),
            "advice": words.and_then(|w| w.advice),
        })
    }

    fn debugging_answer(&self) -> Value {
        let (stage, words) = match self.debugging {
            DebugStage::On => ("on", None),
            DebugStage::SwitchingOff => ("switching_off", Some(Words::new("Switching USB debugging off.", None))),
            DebugStage::Off => ("off", Some(Words::new("USB debugging is off.", None))),
            DebugStage::StillOn => (
                "still_on",
                Some(Words::new("USB debugging is still on.", Some("Switch it off on the phone: Settings, Developer options, USB debugging."))),
            ),
        };
        json!({
            "stage": stage,
            "line": words.as_ref().map(|w| w.line.clone()),
            "advice": words.and_then(|w| w.advice),
        })
    }

    /// `phone.look`'s answer.
    pub(crate) fn answer(&self) -> Value {
        let (case, line, more) = self.cable();
        json!({
            "cable": { "case": case, "line": line, "more": more },
            "fingerprint": if case == "allow" { self.fingerprint.clone() } else { None },
            "phone": self.phone(),
            "checks": self.checks(),
            "install": self.install_answer(),
            "pairing": self.pairing_answer(),
            "debugging": self.debugging_answer(),
            "busy": self.busy,
            "held": self.held,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use limo_cable::flow::{Event, step};
    use limo_cable::open::Interface;
    use limo_cable::usb::UsbDevice;

    fn galaxy() -> Look {
        Look {
            windows: Ok(vec![
                UsbDevice { kind: Kind::PortableDevice, name: Some("Galaxy S20 Ultra 5G".to_string()), bus: "USB".to_string(), working: true },
                UsbDevice { kind: Kind::DebugInterface, name: Some("SAMSUNG Android ADB Interface".to_string()), bus: "USB".to_string(), working: true },
            ]),
            interfaces: Ok(vec![Interface {
                name: Some("SAMSUNG Android ADB Interface".to_string()),
                driver: Some("WINUSB".to_string()),
                above: Some("dg_ssudbus".to_string()),
            }]),
        }
    }

    fn facts() -> PhoneFacts {
        PhoneFacts { manufacturer: "samsung".into(), model: "SM-G988B".into(), android: "13".into(), sdk: Some(33), usb_debugging: Some(true) }
    }

    fn shown(s: &Shared) -> (String, String, String) {
        let (case, line, more) = s.cable();
        (case.to_string(), line, more)
    }

    #[test]
    fn each_of_the_cable_s_drawn_cases_has_its_drawn_words() {
        let mut s = Shared::default();
        assert_eq!(shown(&s).0, "looking");
        s.look = Some(Look { windows: Ok(vec![]), interfaces: Ok(vec![]) });
        s.state = flow::seen(s.look.as_ref().unwrap());
        assert_eq!(
            shown(&s),
            ("none".into(), "No phone on a USB port.".into(), "If it is plugged in, the cable may only charge it. Try the one that came with the phone.".into())
        );
        s.state = State::NotReady(Case::PhoneWithoutDebugging { names: vec![] });
        assert_eq!(shown(&s), ("off".into(), "A phone is plugged in.".into(), "USB debugging is off on it.".into()));
        s.state = State::NotReady(Case::NoDriver { names: vec![] });
        assert_eq!(
            shown(&s),
            ("driver".into(), "No working driver for this phone.".into(), "Install its maker’s USB driver for Windows, such as Samsung’s Android USB Driver.".into())
        );
        s.state = State::Asking;
        assert_eq!(shown(&s), ("allow".into(), "A phone is plugged in.".into(), "Waiting for Allow on the phone.".into()));
        s.look = Some(galaxy());
        s.phone_name = phone_name(&galaxy());
        s.state = step(step(State::Ready { name: None }, Event::Connecting), Event::Connected(facts()));
        assert_eq!(shown(&s), ("ok".into(), "Connected: Galaxy S20 Ultra 5G.".into(), "Android 13.".into()));
        // Anything else is trouble, in limo-cable's own words, set as the
        // screens are set.
        s.state = State::TwoPhones(2);
        assert_eq!(shown(&s), ("trouble".into(), "2 phones are plugged in.".into(), "Unplug all but the phone to add.".into()));
        s.state = State::Stopped(Problem::Lost);
        assert_eq!(shown(&s), ("trouble".into(), "The phone’s connection ended.".into(), "Plug the phone in again, unlocked.".into()));
    }

    #[test]
    fn the_fingerprint_is_given_only_while_the_phone_asks() {
        let mut s = Shared { look: Some(galaxy()), fingerprint: Some("3C:91".into()), state: State::Asking, ..Shared::default() };
        assert_eq!(s.answer()["fingerprint"], "3C:91");
        s.state = State::Connecting;
        assert_eq!(s.answer()["fingerprint"], Value::Null);
    }

    #[test]
    fn the_install_s_progress_is_what_the_cable_counted() {
        let counter = Arc::new(AtomicU64::new(1_000));
        let mut s = Shared { counter: Some(Arc::clone(&counter)), ..Shared::default() };
        s.install = Install { stage: InstallStage::Sending, base: Some(1_000), bytes: Some(4_000), words: None };
        counter.store(2_000, Ordering::Relaxed);
        let a = s.answer();
        assert_eq!((a["install"]["stage"].as_str(), a["install"]["sent"].as_f64()), (Some("sending"), Some(0.25)));
        assert_eq!(a["install"]["line"], "Sending LiMo to the phone.");
        counter.store(5_100, Ordering::Relaxed);
        let a = s.answer();
        assert_eq!((a["install"]["stage"].as_str(), a["install"]["sent"].as_f64()), (Some("installing"), Some(1.0)));
        assert_eq!(a["install"]["line"], "The phone is installing LiMo.");
        // A cable that does not count says installing, with nothing sent known.
        s.counter = None;
        let a = s.answer();
        assert_eq!((a["install"]["stage"].as_str(), &a["install"]["sent"]), (Some("installing"), &Value::Null));
    }
}
