//! The cable's part of adding a phone, as a small state machine the console
//! drives: what is plugged in, which case it is, what to tell the person,
//! and what can be done next. Pure: no calls, no clock; the console makes
//! the calls (a [`crate::open::Cable`] and the [`crate::phone::Phone`] it
//! opens), and hands each outcome in as an [`Event`].
//!
//! The console's loop, in short:
//!
//! 1. While no phone is connected, once a second: [`Event::Looked`] with
//!    what [`crate::open::Cable::look`] gives; show [`tell`].
//! 2. When [`next`] offers [`Next::Connect`]: [`Event::Connecting`], then
//!    open quietly ([`crate::phone::Knock::Quiet`]): [`Event::Connected`],
//!    or [`Event::OpenFailed`], [`crate::phone::OpenProblem::NotKnown`]
//!    for a phone that does not know this PC, which makes the state
//!    [`State::Unknown`]: nothing more is tried while the phone stays.
//! 3. Only when a person asks ([`Next::Ask`]): [`Event::Connecting`], then
//!    open asking ([`crate::phone::Knock::Asking`]); when the cable calls
//!    back that the phone is asking, [`Event::Asking`]; then
//!    [`Event::Connected`] or [`Event::OpenFailed`]. A phone that did not
//!    allow in time stays so while it stays plugged in.
//! 4. Then each step [`next`] offers, in turn, each outcome handed in:
//!    LiMo checked, installed, the offer handed, the link's port reversed,
//!    LiMo started; pairing and the phone's own setup happen meanwhile over
//!    the link, which is not this crate's.
//! 5. At the end, USB debugging switched off, and the phone unplugged.
//!
//! [`tell`] holds the words a guardian reads for every state; none of them
//! names adb.

#![forbid(unsafe_code)]

use crate::commands::{Started, install_advice};
use crate::open::Look;
use crate::phone::{LimoOnPhone, OpenProblem, PhoneFacts};
use crate::usb::{Case, decide};

/// Where adding a phone stands, on the cable's side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// Nothing LiMo can open is plugged in; the case says why.
    NotReady(Case),
    /// More than one phone LiMo could open is plugged in.
    TwoPhones(usize),
    /// A phone LiMo can open is plugged in, not connected yet.
    Ready {
        /// Its name, as Windows shows its interface.
        name: Option<String>,
    },
    /// A phone LiMo can open is plugged in, and does not know this PC yet:
    /// a quiet connection found so, and nothing was asked. Nothing more is
    /// tried until the phone goes, or a person asks.
    Unknown,
    /// Connecting.
    Connecting,
    /// The phone asks the person "Allow USB debugging?" now.
    Asking,
    /// Connected, and what has been done.
    Connected(Progress),
    /// Connecting failed, or the link was lost; what to do is in [`tell`].
    Stopped(Problem),
    /// USB debugging is off again: the cable's part is over.
    Done,
}

/// What has been done on a connected phone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    /// What the phone is.
    pub facts: PhoneFacts,
    /// LiMo on the phone: not looked at yet (None), not there (Some(None)),
    /// or there (Some(Some)).
    pub limo: Option<Option<LimoOnPhone>>,
    /// LiMo installed in this connection.
    pub installed: bool,
    /// The pairing offer handed through LiMo's door.
    pub offer_handed: bool,
    /// The link's port reversed.
    pub reversed: bool,
    /// LiMo's screen started.
    pub started: bool,
    /// The last thing that went wrong, which can be tried again.
    pub trouble: Option<Trouble>,
}

impl Progress {
    fn new(facts: PhoneFacts) -> Progress {
        Progress { facts, limo: None, installed: false, offer_handed: false, reversed: false, started: false, trouble: None }
    }
}

/// Something that went wrong on a connected phone, and can be tried again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trouble {
    /// Android refused to install LiMo, with its code.
    InstallRefused(String),
    /// The offer did not go through the door; what it said.
    OfferRefused(String),
    /// A call failed; its words.
    Call(String),
    /// USB debugging did not switch off.
    DebuggingStillOn,
}

/// Why the cable stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// Opening failed.
    Open(OpenProblem),
    /// The phone's Android is too old for LiMo.
    TooOld(String),
    /// The link was lost (unplugged, or USB debugging switched off on the
    /// phone).
    Lost,
}

/// Something that happened, handed in by the console.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// What is plugged in now.
    Looked(Look),
    /// Opening has begun.
    Connecting,
    /// The phone asks the person now.
    Asking,
    /// Connected; what the phone is.
    Connected(PhoneFacts),
    /// Opening failed.
    OpenFailed(OpenProblem),
    /// LiMo on the phone, looked at.
    LimoChecked(Option<LimoOnPhone>),
    /// LiMo installed; or refused, with Android's code.
    Installed(Result<(), String>),
    /// The offer handed; or not, with what was said.
    OfferHanded(Result<(), String>),
    /// The link's port reversed.
    Reversed,
    /// LiMo started, as `am start` said.
    Started(Started),
    /// A call failed; its words.
    CallFailed(String),
    /// USB debugging switched off (true), or not (false).
    DebuggingOff(bool),
    /// The link was lost.
    Lost,
}

/// What can be done next, in the order of adding a phone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    /// Nothing to do but look again in a second.
    LookAgain,
    /// Open the phone and connect, quietly.
    Connect,
    /// Ask the phone to allow this PC: shows it the key, once (a person's
    /// tap, never done by itself).
    Ask,
    /// Look whether LiMo is on the phone, and which version.
    CheckLimo,
    /// Install LiMo (the console skips it when the phone holds the same
    /// version as the one it carries).
    InstallLimo,
    /// Hand LiMo the pairing offer the service made.
    HandOffer,
    /// Reverse the link's port.
    ReverseLink,
    /// Start LiMo's screen.
    StartLimo,
    /// Switch USB debugging off (the guardian's tap).
    SwitchDebuggingOff,
    /// Tell the person to unplug the phone.
    Unplug,
}

/// The state that what is plugged in gives, when not connected.
pub fn seen(look: &Look) -> State {
    if let Ok(interfaces) = &look.interfaces {
        let usable: Vec<_> = interfaces.iter().filter(|i| i.on_winusb()).collect();
        match usable.len() {
            0 => {}
            1 => return State::Ready { name: usable[0].name.clone() },
            n => return State::TwoPhones(n),
        }
    }
    let windows = look.windows.as_deref().map_err(String::as_str);
    State::NotReady(decide(false, windows).unwrap_or(Case::NoPhone))
}

/// The next state.
pub fn step(state: State, event: Event) -> State {
    match (state, event) {
        // Lost, from anywhere: look again.
        (_, Event::Lost) => State::Stopped(Problem::Lost),
        // While connected or connecting, a look changes nothing, unless it
        // shows the phone gone.
        (State::Connected(progress), Event::Looked(look)) => match seen(&look) {
            State::NotReady(_) => State::Stopped(Problem::Lost),
            _ => State::Connected(progress),
        },
        (State::Done, Event::Looked(_)) => State::Done,
        // A phone that does not know this PC, or did not allow it, stays so
        // while it stays plugged in: nothing is tried by itself.
        (state @ (State::Unknown | State::Stopped(Problem::Open(OpenProblem::NotAllowed | OpenProblem::GivenUp))), Event::Looked(look)) => {
            match seen(&look) {
                State::Ready { .. } => state,
                other => other,
            }
        }
        (State::Connecting | State::Asking, Event::Looked(look)) => match seen(&look) {
            State::NotReady(_) => State::Stopped(Problem::Lost),
            _ => State::Connecting,
        },
        (_, Event::Looked(look)) => seen(&look),
        (State::Ready { .. } | State::Unknown | State::Stopped(_), Event::Connecting) => State::Connecting,
        (State::Connecting, Event::Asking) => State::Asking,
        (State::Connecting | State::Asking, Event::Connected(facts)) => {
            if facts.too_old() {
                State::Stopped(Problem::TooOld(facts.android))
            } else {
                State::Connected(Progress::new(facts))
            }
        }
        (State::Connecting, Event::OpenFailed(OpenProblem::NotKnown)) => State::Unknown,
        (State::Connecting | State::Asking, Event::OpenFailed(problem)) => State::Stopped(Problem::Open(problem)),
        (State::Connected(mut p), event) => {
            p.trouble = None;
            match event {
                Event::LimoChecked(limo) => p.limo = Some(limo),
                Event::Installed(Ok(())) => {
                    p.installed = true;
                    p.limo = Some(Some(LimoOnPhone::default()));
                }
                Event::Installed(Err(code)) => p.trouble = Some(Trouble::InstallRefused(code)),
                Event::OfferHanded(Ok(())) => p.offer_handed = true,
                Event::OfferHanded(Err(said)) => p.trouble = Some(Trouble::OfferRefused(said)),
                Event::Reversed => p.reversed = true,
                Event::Started(Started::Started | Started::HandedToOpenScreen) => p.started = true,
                Event::Started(Started::Refused(said)) => p.trouble = Some(Trouble::Call(said)),
                Event::CallFailed(said) => p.trouble = Some(Trouble::Call(said)),
                Event::DebuggingOff(true) => return State::Done,
                Event::DebuggingOff(false) => p.trouble = Some(Trouble::DebuggingStillOn),
                _ => {}
            }
            State::Connected(p)
        }
        (state, _) => state,
    }
}

/// What can be done next.
pub fn next(state: &State) -> Vec<Next> {
    match state {
        State::NotReady(_) | State::TwoPhones(_) | State::Connecting | State::Asking => vec![Next::LookAgain],
        State::Ready { .. } => vec![Next::Connect],
        State::Stopped(Problem::TooOld(_)) => vec![Next::Unplug],
        State::Unknown | State::Stopped(Problem::Open(OpenProblem::NotAllowed | OpenProblem::GivenUp)) => vec![Next::LookAgain, Next::Ask],
        State::Stopped(_) => vec![Next::LookAgain],
        State::Done => vec![Next::Unplug],
        State::Connected(p) => {
            let first = if p.limo.is_none() {
                Next::CheckLimo
            } else if !p.installed {
                Next::InstallLimo
            } else if !p.offer_handed {
                Next::HandOffer
            } else if !p.reversed {
                Next::ReverseLink
            } else if !p.started {
                Next::StartLimo
            } else {
                return vec![Next::SwitchDebuggingOff];
            };
            vec![first, Next::SwitchDebuggingOff]
        }
    }
}

/// What to tell the person: one line of what is so, and what to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tell {
    /// What is so.
    pub line: String,
    /// What to do, if anything.
    pub advice: Option<String>,
}

fn named(names: &[String]) -> String {
    if names.is_empty() { String::new() } else { format!(" ({})", names.join(", ")) }
}

fn tell_case(case: &Case) -> Tell {
    let (line, advice) = match case {
        Case::NoPhone => (
            "This PC sees no phone on its USB ports.".to_string(),
            "The cable may only charge: many cables carry no data. Use the cable that came with the phone, or one known to \
             move files, in a port on the PC itself, not a hub; and unlock the phone."
                .to_string(),
        ),
        Case::PhoneWithoutDebugging { names } => (
            format!("This PC sees what looks like a phone{}, but USB debugging is off on it.", named(names)),
            "Switch USB debugging on, on the phone: Settings, About phone, tap Build number seven times; then Settings, \
             Developer options, USB debugging. If it is on already, unplug the phone and plug it in again, unlocked."
                .to_string(),
        ),
        Case::NoDriver { names } => (
            format!("This PC sees the phone's debugging connection{}, but has no driver for it.", named(names)),
            "Install the phone maker's USB driver on this PC (for a Samsung phone, Samsung's Android USB Driver for \
             Windows), then unplug the phone and plug it in again. LiMo installs no driver."
                .to_string(),
        ),
        Case::DebuggingNotListed { names } => (
            format!("This PC sees the phone's debugging connection{}, but its driver is not one LiMo can use.", named(names)),
            "Install the phone maker's current USB driver on this PC (for a Samsung phone, Samsung's Android USB Driver \
             for Windows), then unplug the phone and plug it in again."
                .to_string(),
        ),
        Case::CannotLook(_) => (
            "This PC's list of USB devices could not be read.".to_string(),
            "Plug the phone in with its own cable, unlock it, and check that USB debugging is on (Settings, Developer options)."
                .to_string(),
        ),
    };
    Tell { line, advice: Some(advice) }
}

/// The words for the state.
pub fn tell(state: &State) -> Tell {
    let say = |line: &str, advice: Option<&str>| Tell { line: line.to_string(), advice: advice.map(str::to_string) };
    match state {
        State::NotReady(case) => tell_case(case),
        State::TwoPhones(n) => Tell {
            line: format!("{n} phones are plugged in."),
            advice: Some("Unplug all but the phone to add.".to_string()),
        },
        State::Ready { name } => Tell {
            line: match name {
                Some(name) => format!("The phone is plugged in ({name}), with USB debugging on."),
                None => "The phone is plugged in, with USB debugging on.".to_string(),
            },
            advice: Some("Unlock the phone.".to_string()),
        },
        State::Unknown => say(
            "The phone is plugged in, with USB debugging on, and does not know this PC yet.",
            Some("Unlock the phone, then ask it to allow this PC: it asks once, on its own screen."),
        ),
        State::Connecting => say("Connecting to the phone.", Some("Unlock the phone.")),
        State::Asking => say(
            "The phone asks: Allow USB debugging?",
            Some("Tick \"Always allow from this computer\", then tap Allow."),
        ),
        State::Stopped(Problem::Open(problem)) => match problem {
            OpenProblem::NotAllowed => say(
                "The phone did not allow this PC in time.",
                Some("Unlock the phone and ask again. If the phone shows no question, switch USB debugging off and on again on the phone first."),
            ),
            OpenProblem::GivenUp => say("The phone was not waited for any longer.", Some("Unlock the phone and ask again.")),
            OpenProblem::NotKnown => say(
                "The phone does not know this PC yet.",
                Some("Unlock the phone, then ask it to allow this PC: it asks once, on its own screen."),
            ),
            OpenProblem::AdbServerRunning | OpenProblem::Busy => say(
                "Another program on this PC holds the phone's connection (Android Studio, or Google's adb).",
                Some("Close it, then unplug the phone and plug it in again."),
            ),
            OpenProblem::TwoPhones(n) => Tell { line: format!("{n} phones are plugged in."), advice: Some("Unplug all but the phone to add.".to_string()) },
            OpenProblem::DriverNotUsable(_) => tell_case(&Case::DebuggingNotListed { names: Vec::new() }),
            OpenProblem::NoPhone | OpenProblem::NoAnswer | OpenProblem::Lost(_) | OpenProblem::Other(_) => say(
                "The phone could not be reached over the cable.",
                Some("Unplug the phone, plug it in again unlocked, and try again."),
            ),
        },
        State::Stopped(Problem::TooOld(android)) => Tell {
            line: format!("This phone has Android {android}. LiMo needs Android 8.0 or later."),
            advice: Some("Unplug the phone.".to_string()),
        },
        State::Stopped(Problem::Lost) => say(
            "The phone's connection ended.",
            Some("Plug the phone in again, unlocked."),
        ),
        State::Connected(p) => {
            let phone = if p.facts.model.is_empty() { "the phone".to_string() } else { p.facts.model.clone() };
            if let Some(trouble) = &p.trouble {
                return match trouble {
                    Trouble::InstallRefused(code) => Tell {
                        line: "The phone did not take LiMo.".to_string(),
                        advice: Some(install_advice(code).to_string()),
                    },
                    Trouble::OfferRefused(_) => say(
                        "LiMo on the phone did not take this PC's pairing.",
                        Some("Open LiMo on the phone; if it says it belongs to a family already, it was set up before."),
                    ),
                    Trouble::Call(_) => say("The phone did not answer as expected.", Some("Try again; if it happens again, unplug the phone and plug it in again.")),
                    Trouble::DebuggingStillOn => say(
                        "USB debugging is still on.",
                        Some("Switch it off on the phone: Settings, Developer options, USB debugging."),
                    ),
                };
            }
            let line = if p.started {
                format!("LiMo is starting on {phone}.")
            } else if p.offer_handed {
                format!("Pairing {phone} with this PC.")
            } else if p.installed {
                format!("LiMo is on {phone}.")
            } else if p.limo.is_some() {
                format!("Putting LiMo on {phone}.")
            } else {
                format!("Connected to {phone}, Android {}.", p.facts.android)
            };
            Tell { line, advice: None }
        }
        State::Done => say("USB debugging is off again on the phone.", Some("Unplug the phone.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open::Interface;
    use crate::usb::{Kind, UsbDevice};

    fn galaxy_look() -> Look {
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
        PhoneFacts { manufacturer: "samsung".to_string(), model: "SM-G988B".to_string(), android: "13".to_string(), sdk: Some(33), usb_debugging: Some(true) }
    }

    #[test]
    fn what_is_plugged_in_gives_each_state() {
        assert_eq!(seen(&galaxy_look()), State::Ready { name: Some("SAMSUNG Android ADB Interface".to_string()) });
        let nothing = Look { windows: Ok(vec![]), interfaces: Ok(vec![]) };
        assert_eq!(seen(&nothing), State::NotReady(Case::NoPhone));
        let mut off = galaxy_look();
        off.interfaces = Ok(vec![]);
        off.windows.as_mut().unwrap().truncate(1);
        assert!(matches!(seen(&off), State::NotReady(Case::PhoneWithoutDebugging { .. })));
        // The interface there, but on a driver that is not WinUSB.
        let mut other = galaxy_look();
        other.interfaces.as_mut().unwrap()[0].driver = Some("ssudadb".to_string());
        assert!(matches!(seen(&other), State::NotReady(Case::DebuggingNotListed { .. })));
        let mut two = galaxy_look();
        let again = two.interfaces.as_ref().unwrap()[0].clone();
        two.interfaces.as_mut().unwrap().push(again);
        assert_eq!(seen(&two), State::TwoPhones(2));
        let unreadable = Look { windows: Err("no list".to_string()), interfaces: Err("no list".to_string()) };
        assert!(matches!(seen(&unreadable), State::NotReady(Case::CannotLook(_))));
    }

    #[test]
    fn adding_a_phone_runs_through_each_next_step_in_order() {
        let mut state = step(State::NotReady(Case::NoPhone), Event::Looked(galaxy_look()));
        assert_eq!(next(&state), vec![Next::Connect]);
        state = step(state, Event::Connecting);
        state = step(state, Event::Asking);
        assert_eq!(tell(&state).line, "The phone asks: Allow USB debugging?");
        state = step(state, Event::Connected(facts()));
        assert_eq!(next(&state), vec![Next::CheckLimo, Next::SwitchDebuggingOff]);
        state = step(state, Event::LimoChecked(None));
        assert_eq!(next(&state)[0], Next::InstallLimo);
        state = step(state, Event::Installed(Ok(())));
        assert_eq!(next(&state)[0], Next::HandOffer);
        state = step(state, Event::OfferHanded(Ok(())));
        assert_eq!(next(&state)[0], Next::ReverseLink);
        state = step(state, Event::Reversed);
        assert_eq!(next(&state)[0], Next::StartLimo);
        state = step(state, Event::Started(Started::HandedToOpenScreen));
        assert_eq!(next(&state), vec![Next::SwitchDebuggingOff]);
        // A look while connected changes nothing.
        state = step(state, Event::Looked(galaxy_look()));
        assert!(matches!(state, State::Connected(_)));
        state = step(state, Event::DebuggingOff(true));
        assert_eq!(state, State::Done);
        assert_eq!(next(&state), vec![Next::Unplug]);
        assert_eq!(tell(&state).advice.as_deref(), Some("Unplug the phone."));
    }

    #[test]
    fn what_goes_wrong_is_said_and_can_be_tried_again() {
        let connected = step(step(State::Ready { name: None }, Event::Connecting), Event::Connected(facts()));
        let refused = step(connected.clone(), Event::Installed(Err("INSTALL_FAILED_UPDATE_INCOMPATIBLE".to_string())));
        assert!(tell(&refused).advice.unwrap().contains("leaves it as it is"));
        assert_eq!(next(&refused)[0], Next::CheckLimo, "tried again");
        let gone = step(connected, Event::Looked(Look { windows: Ok(vec![]), interfaces: Ok(vec![]) }));
        assert_eq!(gone, State::Stopped(Problem::Lost));
        assert_eq!(next(&gone), vec![Next::LookAgain]);
        let not_allowed = step(State::Connecting, Event::OpenFailed(OpenProblem::NotAllowed));
        assert!(tell(&not_allowed).advice.unwrap().contains("ask again"));
        // Asked and not allowed, or not known: nothing tried by itself while
        // the phone stays plugged in; the person may ask.
        assert_eq!(step(not_allowed.clone(), Event::Looked(galaxy_look())), not_allowed);
        assert_eq!(next(&not_allowed), vec![Next::LookAgain, Next::Ask]);
        let unknown = step(State::Connecting, Event::OpenFailed(OpenProblem::NotKnown));
        assert_eq!(unknown, State::Unknown);
        assert_eq!(step(State::Unknown, Event::Looked(galaxy_look())), State::Unknown);
        assert_eq!(next(&State::Unknown), vec![Next::LookAgain, Next::Ask]);
        // The phone gone and back: looked at afresh, and connected quietly.
        let gone = step(State::Unknown, Event::Looked(Look { windows: Ok(vec![]), interfaces: Ok(vec![]) }));
        assert_eq!(gone, State::NotReady(Case::NoPhone));
        assert_eq!(next(&step(gone, Event::Looked(galaxy_look()))), vec![Next::Connect]);
        assert_eq!(step(State::Unknown, Event::Connecting), State::Connecting);
        let busy = step(State::Connecting, Event::OpenFailed(OpenProblem::AdbServerRunning));
        assert!(tell(&busy).line.contains("Android Studio"));
        let old = PhoneFacts { sdk: Some(25), android: "7.1".to_string(), ..facts() };
        let too_old = step(State::Connecting, Event::Connected(old));
        assert_eq!(tell(&too_old).line, "This phone has Android 7.1. LiMo needs Android 8.0 or later.");
        assert_eq!(next(&too_old), vec![Next::Unplug]);
        // Words a guardian reads never name adb, but where they name another program.
        for state in [
            State::NotReady(Case::NoPhone),
            State::NotReady(Case::PhoneWithoutDebugging { names: vec![] }),
            State::NotReady(Case::NoDriver { names: vec![] }),
            State::NotReady(Case::DebuggingNotListed { names: vec![] }),
            State::Ready { name: None },
            State::Unknown,
            State::Asking,
            State::Stopped(Problem::Lost),
            State::Done,
        ] {
            let words = tell(&state);
            assert!(!words.line.contains("adb") && !words.advice.unwrap_or_default().contains("adb"), "{state:?}");
        }
    }
}
