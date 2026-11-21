//! The state machine driven as the console will drive it, against the fake
//! phone: plug in, connect quietly (the phone does not know this PC, and
//! nothing is shown), ask (the question on the phone, once), LiMo looked
//! for and installed, the offer handed, the link's port reversed, LiMo started,
//! USB debugging off, unplug; and each thing that can stand in the way.

mod common;

use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use limo_cable::commands::{Installed, PACKAGE};
use limo_cable::fake::{CableKind, Driver, FakePhone};
use limo_cable::flow::{Event, Next, State, next, step, tell};
use limo_cable::key::AdbKey;
use limo_cable::open::Cable;
use limo_cable::phone::{DebuggingOff, Knock, Phone, Waits};
use limo_cable::usb::Case;

use common::apk::apk_file;

fn key() -> AdbKey {
    static KEY: OnceLock<AdbKey> = OnceLock::new();
    KEY.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("limo-cable-flow-key-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        AdbKey::load_or_make(&dir).unwrap()
    })
    .clone()
}

/// The console's loop, as far as the cable goes: what it does for each
/// `Next`, and every line it would show.
struct Console {
    state: State,
    phone: Option<Phone>,
    said: Vec<String>,
}

impl Console {
    fn new() -> Console {
        Console { state: State::NotReady(Case::NoPhone), phone: None, said: Vec::new() }
    }

    fn hand(&mut self, event: Event) {
        self.state = step(self.state.clone(), event);
        let words = tell(&self.state);
        self.said.push(words.line);
    }

    /// One turn: does the first thing `next` offers, and asks the phone
    /// when it offers that too (the guardian's tap, in the test at once).
    fn turn(&mut self, cable: &mut FakePhone, apk: &Path) -> Next {
        let offered = next(&self.state);
        let todo = if offered.contains(&Next::Ask) { Next::Ask } else { offered[0] };
        match todo {
            Next::LookAgain => self.hand(Event::Looked(cable.look())),
            Next::Connect | Next::Ask => {
                self.hand(Event::Connecting);
                let mut asked = false;
                let waits = Waits { prompt: Duration::from_secs(2), ..Waits::default() };
                let result = if todo == Next::Ask {
                    cable.open(&key(), waits, Knock::asking(&mut || asked = true))
                } else {
                    cable.open(&key(), waits, Knock::Quiet)
                };
                if asked {
                    self.hand(Event::Asking);
                }
                match result {
                    Ok(phone) => {
                        let facts = phone.facts().clone();
                        self.phone = Some(phone);
                        self.hand(Event::Connected(facts));
                    }
                    Err(problem) => self.hand(Event::OpenFailed(problem)),
                }
            }
            Next::CheckLimo => {
                let limo = self.phone.as_ref().unwrap().limo().unwrap();
                self.hand(Event::LimoChecked(limo));
            }
            Next::InstallLimo => {
                let answer = self.phone.as_ref().unwrap().install_limo(apk).unwrap();
                self.hand(Event::Installed(match answer {
                    Installed::Done => Ok(()),
                    Installed::Refused { code, .. } => Err(code),
                }));
            }
            Next::HandOffer => {
                let handed = self.phone.as_ref().unwrap().hand_offer(b"limo1:SAMPLEOFFER\n").unwrap();
                self.hand(Event::OfferHanded(if handed.ok() { Ok(()) } else { Err(handed.said) }));
            }
            Next::ReverseLink => {
                self.phone.as_ref().unwrap().reverse_link().unwrap();
                self.hand(Event::Reversed);
            }
            Next::StartLimo => {
                let started = self.phone.as_ref().unwrap().start_limo().unwrap();
                self.hand(Event::Started(started));
            }
            Next::SwitchDebuggingOff => {
                let off = self.phone.take().unwrap().debugging_off();
                self.hand(Event::DebuggingOff(matches!(off, DebuggingOff::Answered | DebuggingOff::LinkEnded)));
            }
            Next::Unplug => {}
        }
        todo
    }
}

#[test]
fn a_phone_is_added_over_the_cable_from_plugging_in_to_unplugging() {
    // First a cable that only charges, then USB debugging off, then all well.
    let mut cable = FakePhone::with(|s| s.cable = CableKind::ChargeOnly);
    let apk = apk_file("limo-child-release.apk", PACKAGE, Some(false));
    let mut console = Console::new();
    console.turn(&mut cable, &apk);
    assert_eq!(console.state, State::NotReady(Case::NoPhone));
    assert!(tell(&console.state).advice.unwrap().contains("only charge"));
    {
        let mut s = cable.state();
        s.cable = CableKind::Data;
        s.debugging = false;
    }
    console.turn(&mut cable, &apk);
    assert!(matches!(console.state, State::NotReady(Case::PhoneWithoutDebugging { .. })));
    assert!(tell(&console.state).advice.unwrap().contains("Build number seven times"));
    cable.state().debugging = true;
    let mut done = Vec::new();
    for _ in 0..20 {
        let todo = console.turn(&mut cable, &apk);
        done.push(todo);
        if todo == Next::Unplug {
            break;
        }
    }
    assert_eq!(
        done,
        vec![
            Next::LookAgain,
            Next::Connect,
            Next::Ask,
            Next::CheckLimo,
            Next::InstallLimo,
            Next::HandOffer,
            Next::ReverseLink,
            Next::StartLimo,
            Next::SwitchDebuggingOff,
            Next::Unplug
        ]
    );
    assert_eq!(console.state, State::Done);
    assert!(console.said.contains(&"The phone asks: Allow USB debugging?".to_string()), "{:?}", console.said);
    assert!(console.said.contains(&"Pairing SM-G988B with this PC.".to_string()));
    assert_eq!(console.said.last().unwrap(), "USB debugging is off again on the phone.");
    let s = cable.state();
    assert_eq!(s.offers, vec![b"limo1:SAMPLEOFFER\n".to_vec()]);
    assert!(s.limo.is_some());
    assert!(!s.debugging);
}

#[test]
fn a_phone_with_no_driver_or_a_driver_limo_cannot_use_is_said_and_never_opened() {
    let apk = apk_file("limo-child-release.apk", PACKAGE, Some(false));
    let mut none = FakePhone::with(|s| s.driver = Driver::None);
    let mut console = Console::new();
    console.turn(&mut none, &apk);
    assert!(matches!(console.state, State::NotReady(Case::NoDriver { .. })));
    assert!(tell(&console.state).advice.unwrap().contains("LiMo installs no driver"));
    let mut makers = FakePhone::with(|s| s.driver = Driver::MakersOwn);
    let mut console = Console::new();
    console.turn(&mut makers, &apk);
    assert!(matches!(console.state, State::NotReady(Case::DebuggingNotListed { .. })));
    assert_eq!(next(&console.state), vec![Next::LookAgain]);
    assert!(makers.seen().services.is_empty() && none.seen().services.is_empty(), "nothing was opened");
}

#[test]
fn unplugging_while_connected_is_lost_and_plugging_in_again_goes_on() {
    let apk = apk_file("limo-child-release.apk", PACKAGE, Some(false));
    let mut cable = FakePhone::new();
    let mut console = Console::new();
    console.turn(&mut cable, &apk);
    console.turn(&mut cable, &apk);
    assert_eq!(console.state, State::Unknown, "a quiet connection: the phone does not know this PC");
    assert_eq!(cable.seen().prompts, 0);
    console.turn(&mut cable, &apk);
    assert!(matches!(console.state, State::Connected(_)));
    cable.state().plugged = false;
    console.hand(Event::Looked(cable.look()));
    assert!(matches!(console.state, State::Stopped(limo_cable::flow::Problem::Lost)));
    assert_eq!(tell(&console.state).advice.as_deref(), Some("Plug the phone in again, unlocked."));
    cable.state().plugged = true;
    console.phone = None;
    console.turn(&mut cable, &apk);
    assert!(matches!(console.state, State::Ready { .. }));
    console.turn(&mut cable, &apk);
    assert!(matches!(console.state, State::Connected(_)));
    assert_eq!(cable.seen().prompts, 1, "the key was allowed the first time");
}
