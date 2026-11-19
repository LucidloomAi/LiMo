//! The channel: anything malformed, oversized or out of turn closes it and
//! restarts the helper, and never reaches the engine; the ways out belong to
//! the helper's own session; a PIN or a code is never logged or kept.

use limo_core::{PinAction, TryOutcome};
use limo_hub::UnlinkedAccounts;
use limo_windows::protocol::{Answer, FromHelper, MAX_MESSAGE_BYTES, PROTOCOL_VERSION, Secret, ToHelper, encode};

use crate::common::*;

const ASHA: &str = "S-1-5-21-1001";
const MIRA: &str = "S-1-5-21-1002";
const VISITOR: &str = "S-1-5-21-1009";

fn frame_of(version: u16, json: &str) -> Vec<u8> {
    let mut bytes = version.to_le_bytes().to_vec();
    bytes.extend_from_slice(&(json.len() as u32).to_le_bytes());
    bytes.extend_from_slice(json.as_bytes());
    bytes
}

fn answers(pc: &mut Pc, session: u32) -> Vec<(u32, Answer)> {
    pc.helper(session)
        .unwrap()
        .got
        .iter()
        .filter_map(|m| match m {
            ToHelper::Answer { ask, answer } => Some((*ask, answer.clone())),
            _ => None,
        })
        .collect()
}

/// What a helper may send that closes its channel, and whether it says
/// hello first.
fn bad_inputs() -> Vec<(&'static str, bool, Vec<u8>)> {
    let mut huge = PROTOCOL_VERSION.to_le_bytes().to_vec();
    huge.extend_from_slice(&(10 * 1024 * 1024u32).to_le_bytes());
    let mut flood = PROTOCOL_VERSION.to_le_bytes().to_vec();
    flood.extend_from_slice(&((MAX_MESSAGE_BYTES as u32) - 1).to_le_bytes());
    flood.extend(std::iter::repeat_n(b'x', 3 * MAX_MESSAGE_BYTES));
    vec![
        ("garbage", true, frame_of(PROTOCOL_VERSION, "not json at all")),
        ("another version", true, frame_of(7, r#"{"say":"hello","protocol":7}"#)),
        ("a frame too long", true, huge),
        ("a flood", true, flood),
        ("a message nobody knows", true, frame_of(PROTOCOL_VERSION, r#"{"say":"take_over_the_pc"}"#)),
        (
            "a window title",
            true,
            frame_of(
                PROTOCOL_VERSION,
                r#"{"say":"report","idle_ms":900000,"front_pid":null,"full_screen":true,"playing_sound":true,"screen_kept_awake":true,"window_title":"secret diary"}"#,
            ),
        ),
        (
            "a report that says nothing of sound or the screen kept awake",
            true,
            frame_of(PROTOCOL_VERSION, r#"{"say":"report","idle_ms":900000,"front_pid":null,"full_screen":true}"#),
        ),
        (
            "a report before hello",
            false,
            encode(&FromHelper::Report {
                idle_ms: 900_000,
                front_pid: None,
                full_screen: true,
                playing_sound: true,
                screen_kept_awake: true,
            })
            .unwrap()
            .to_vec(),
        ),
        ("hello twice", true, encode(&FromHelper::Hello { protocol: PROTOCOL_VERSION }).unwrap().to_vec()),
        ("hello in another version", false, encode(&FromHelper::Hello { protocol: PROTOCOL_VERSION - 1 }).unwrap().to_vec()),
        ("a lock it was never sent", true, encode(&FromHelper::LockOnScreen { seq: 999_999 }).unwrap().to_vec()),
        (
            "a PIN as a number",
            true,
            frame_of(PROTOCOL_VERSION, r#"{"say":"try_pin","ask":1,"pin":482915,"action":"confirm_release"}"#),
        ),
    ]
}

#[test]
fn anything_malformed_oversized_or_out_of_turn_closes_the_channel_and_restarts_the_helper() {
    for (what, greet, bytes) in bad_inputs() {
        let mut pc = Pc::new(at(16, 0));
        let asha = pc.add_child("Asha", ASHA, 60);
        pc.login(1, ASHA, "asha");
        pc.step();
        if greet {
            pc.greet(1);
            pc.step();
        }
        let first = pc.helper(1).unwrap().pid;
        let counted = pc.counted(&asha);
        pc.write_raw(1, bytes);
        pc.step();
        assert_eq!(pc.helpers_running(1), 0, "{what}: the helper still runs");
        let closed = pc.logged(&format!("(pid {first}) was closed, and is started again, because"));
        assert_eq!(closed.len(), 1, "{what}: {:?}", pc.os().log);
        for secret in ["take_over", "secret diary", "window_title", "482915", "4829", "not json"] {
            assert!(!closed[0].contains(secret), "{what}: {}", closed[0]);
        }
        // Nothing of it reached the engine: no idle time, no full screen,
        // nothing playing, and time went on counting.
        pc.run(3);
        let facts = pc.facts_of(1).unwrap();
        assert_eq!((facts.idle_secs, facts.full_screen, facts.playing), (0, false, false), "{what}");
        assert!(pc.counted(&asha) >= counted + 2, "{what}");
        // A new helper runs.
        assert_eq!((pc.helpers_started(1), pc.helpers_running(1)), (2, 1), "{what}");
    }
}

#[test]
fn a_helper_that_stops_reading_is_ended_and_started_again() {
    let mut pc = Pc::new(at(16, 0));
    pc.add_child("Asha", ASHA, 60);
    pc.login(1, ASHA, "asha");
    pc.run(1);
    pc.greet(1);
    pc.run(3);
    pc.helper(1).unwrap().not_reading = true;
    pc.run(5);
    assert_eq!(pc.helpers_started(1), 2);
    assert_eq!(pc.helpers_running(1), 1);
    assert_eq!(pc.logged("it stopped reading what it is sent").len(), 1);
}

#[test]
fn a_way_out_belongs_to_the_session_its_helper_was_started_in() {
    let mut pc = Pc::new(at(16, 0));
    let asha = pc.add_child("Asha", ASHA, 60);
    let mira = pc.add_child("Mira", MIRA, 60);
    pc.login(1, ASHA, "asha");
    pc.login(2, MIRA, "mira");
    pc.set_state(2, limo_agent::SessionState::Disconnected);
    pc.run(1);
    pc.greet(1);
    pc.greet(2);
    pc.run(2);
    // Mira's helper asks. Nothing in the message says whose it is: it is
    // the session the helper was started in.
    pc.say(2, FromHelper::AskForTime { ask: 7, minutes: 20, reason: "For the school project".into() });
    pc.run(1);
    let now = pc.now();
    let waiting = pc.hub().waiting_requests(now).unwrap();
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0].child_id, mira);
    assert_eq!(waiting[0].request.request.minutes, 20);
    let got = answers(&mut pc, 2);
    assert!(matches!(&got[..], [(7, Answer::Asked { request })] if request.child_id == mira), "{got:?}");
    assert!(answers(&mut pc, 1).is_empty());
    assert!(pc.hub().waiting_requests(now).unwrap().iter().all(|w| w.child_id != asha));
    // The reason the child typed is not logged.
    assert!(pc.logged("school project").is_empty());
    assert_eq!(pc.logged("Session 2: a request for more time was tried").len(), 1);
}

#[test]
fn a_pin_or_a_code_is_never_logged_kept_or_shown() {
    let mut pc = Pc::new(at(16, 0));
    let asha = pc.add_child("Asha", ASHA, 60);
    let now = pc.now();
    pc.hub().set_pin(PIN, now).unwrap();
    pc.hub().lock_now(&asha, None, now).unwrap();
    pc.login(1, ASHA, "asha");
    pc.run(1);
    pc.greet(1);
    pc.run(3);
    pc.say(1, FromHelper::TryPin { ask: 1, pin: Secret::new(PIN.into()), action: PinAction::AddMinutes { minutes: 10 } });
    pc.run(2);
    pc.say(1, FromHelper::TryPin { ask: 2, pin: Secret::new("111111".into()), action: PinAction::AddMinutes { minutes: 10 } });
    pc.run(2);
    pc.say(1, FromHelper::TryCode { ask: 3, code: Secret::new("13420715".into()) });
    pc.run(2);
    let got = answers(&mut pc, 1);
    assert!(matches!(got[0], (1, Answer::Tried { outcome: TryOutcome::Added { .. } })), "{got:?}");
    assert!(matches!(got[1], (2, Answer::Tried { outcome: TryOutcome::Wrong { .. } })), "{got:?}");
    assert!(matches!(got[2], (3, Answer::Tried { outcome: TryOutcome::Wrong { .. } })), "{got:?}");
    assert_eq!(pc.logged("the guardian PIN was tried from the lock screen: right; 10 minutes added").len(), 1);
    assert_eq!(pc.logged("the guardian PIN was tried from the lock screen: wrong").len(), 1);
    assert_eq!(pc.logged("a one time code was tried from the lock screen: wrong").len(), 1);
    for secret in [PIN, "111111", "13420715", "1342"] {
        assert!(pc.logged(secret).is_empty(), "{secret} logged");
        assert!(!format!("{:?}", pc.service).contains(secret));
        assert!(!format!("{:?}", pc.service.session_view(1)).contains(secret));
        for message in &pc.helper(1).unwrap().got {
            assert!(!format!("{message:?}").contains(secret));
        }
    }
}

#[test]
fn ways_out_are_taken_at_most_one_a_second() {
    let mut pc = Pc::new(at(16, 0));
    let asha = pc.add_child("Asha", ASHA, 60);
    let now = pc.now();
    pc.hub().lock_now(&asha, None, now).unwrap();
    pc.login(1, ASHA, "asha");
    pc.run(1);
    pc.greet(1);
    pc.run(2);
    pc.say(1, FromHelper::TryCode { ask: 1, code: Secret::new("11111111".into()) });
    pc.say(1, FromHelper::TryCode { ask: 2, code: Secret::new("22222222".into()) });
    pc.step();
    let got = answers(&mut pc, 1);
    assert!(matches!(got[0], (1, Answer::Tried { .. })), "{got:?}");
    assert!(matches!(&got[1], (2, Answer::Refused { message }) if message.contains("One thing at a time")), "{got:?}");
}

#[test]
fn a_helper_that_shows_the_plain_display_is_logged_in_plain_words_and_nothing_else_changes() {
    let mut pc = Pc::new(at(16, 0));
    pc.add_child("Asha", ASHA, 60);
    pc.login(1, ASHA, "asha");
    pc.run(1);
    pc.greet(1);
    pc.run(1);
    let pid = pc.helper(1).unwrap().pid;
    // A reason with a line break in it, and far too long: one line, cut.
    let why = format!("the drawn display could not start: {}\nforged line", "x".repeat(1_000));
    pc.say(1, FromHelper::ShowsPlain { why });
    pc.run(1);
    let lines = pc.logged("shows the plain display");
    assert_eq!(lines.len(), 1, "{:?}", pc.os().log);
    let start = format!("The helper in session 1 (pid {pid}) shows the plain display, not the drawn one, because the drawn display");
    assert!(lines[0].starts_with(&start), "{}", lines[0]);
    assert!(!lines[0].contains('\n') && !lines[0].contains("forged") && lines[0].len() < 500, "{}", lines[0]);
    // Said again, it is no harm, and logged at most once in a while.
    pc.say(1, FromHelper::ShowsPlain { why: "again".into() });
    pc.run(1);
    assert_eq!((pc.helpers_started(1), pc.helpers_running(1)), (1, 1));
    assert_eq!(pc.logged("shows the plain display").len(), 1);
}

#[test]
fn a_way_out_from_a_session_that_is_not_a_childs_is_refused() {
    let mut pc = Pc::new(at(16, 0));
    pc.add_child("Asha", ASHA, 60);
    pc.hub().set_unlinked_accounts(UnlinkedAccounts::Locked).unwrap();
    pc.login(5, VISITOR, "visitor");
    pc.run(1);
    pc.greet(5);
    pc.run(2);
    pc.say(5, FromHelper::TryPin { ask: 1, pin: Secret::new(PIN.into()), action: PinAction::ConfirmRelease });
    pc.run(1);
    let got = answers(&mut pc, 5);
    assert!(matches!(&got[..], [(1, Answer::Refused { message })] if message.contains("not a child's session")), "{got:?}");
    // The engine's sentence ends the log line with one full stop, not two.
    let line = "Session 5: the guardian PIN was tried from the lock screen: refused: Session 5 is not a child's session \
                that LiMo limits on this PC.";
    let logged = pc.logged("tried from the lock screen");
    assert_eq!(logged, vec![line.to_string()], "{:?}", pc.os().log);
}
