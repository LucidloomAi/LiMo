//! The console against LiMo for Windows' own door: its pipe threads, its
//! decisions and a real family's file, served in the test's own process
//! (`../support/real_door.rs`) at a pipe name of the test's own. Nothing is
//! started as a service and nothing runs elevated. And the console's frames
//! against the service's own reader and writer, both ways.

use std::time::{Duration, Instant};

use serde_json::{Value, json};

#[path = "../support/real_door.rs"]
pub(crate) mod real_door;

use limo_windows::console::{
    CONSOLE_PROTOCOL_VERSION, ConsoleIncoming, ConsoleReader, FromConsole, Problem, ProblemKind, Reply, ToConsole, encode_to_console,
};
use real_door::RealDoorHost;

use crate::door::{Door, DoorProblem, DoorReply, NotTheService, ServerMustBe};
use crate::door_tests::pipe_name;
use crate::protocol::{Decoder, FromDoor, PageRequest, encode, hello};
use crate::this_process;

const PASSWORD: &str = "correct horse battery";

fn ask(door: &mut Door, name: &str, args: Value) -> Result<DoorReply, DoorProblem> {
    door.ask(PageRequest::from_page(name, args).unwrap())
}

fn reply(door: &mut Door, name: &str, args: Value) -> Value {
    match ask(door, name, args) {
        Ok(DoorReply::Reply(reply)) => reply,
        other => panic!("{name}: {other:?}"),
    }
}

/// The door served by this very process: its session, its account, its
/// program file (the test's own).
fn mine(name: &str) -> Door {
    Door::new(name, this_process::as_server().unwrap())
}

#[test]
fn the_console_signs_in_at_limo_s_own_door_with_a_real_family_file() {
    let host = RealDoorHost::start(&pipe_name(), PASSWORD, None);
    let mut door = mine(host.name());
    let about = reply(&mut door, "about", Value::Null);
    assert_eq!(about["outcome"], "done", "{about}");
    let state = reply(&mut door, "family_state", Value::Null);
    assert_eq!(state["value"]["has_family"], true, "{state}");
    let today = reply(&mut door, "family_today", Value::Null);
    assert_eq!(today["problem"]["kind"], "signed_out", "{today}");

    // The door's own count and words for a wrong password.
    let wrong = reply(&mut door, "sign_in", json!({ "password": "not it" }));
    assert_eq!(wrong["problem"]["kind"], "wrong_password", "{wrong}");
    assert_eq!(wrong["problem"]["tries_before_wait"], 4, "{wrong}");
    assert_eq!(wrong["problem"]["message"], "That password is not right. 4 more tries before a wait.", "{wrong}");
    // A second password within a second of the first is answered busy, in
    // the door's own words, and not checked.
    let busy = reply(&mut door, "sign_in", json!({ "password": PASSWORD }));
    assert_eq!(busy["problem"]["kind"], "busy", "{busy}");
    std::thread::sleep(Duration::from_millis(1_100));
    let right = reply(&mut door, "sign_in", json!({ "password": PASSWORD }));
    assert_eq!(right["outcome"], "done", "{right}");
    let today = reply(&mut door, "family_today", Value::Null);
    assert!(today["value"].is_array(), "{today}");

    // A request the door cannot read is answered as unknown, by its number,
    // and the connection stays signed in.
    let unknown = reply(&mut door, "take_over_the_pc", Value::Null);
    assert_eq!(unknown["problem"]["kind"], "unknown_request", "{unknown}");
    let odd = reply(&mut door, "lock_now", json!({ "child": "c1", "window_title": "x" }));
    assert_eq!(odd["problem"]["kind"], "unknown_request", "{odd}");
    assert_eq!(reply(&mut door, "family_today", Value::Null)["outcome"], "done");

    // The service was told who knocked by Windows: this process.
    let pid = std::process::id();
    let log = host.log();
    assert_eq!(log.iter().filter(|l| l.contains("A console connected") && l.contains(&format!("(pid {pid})"))).count(), 1, "{log:?}");
    assert!(!log.iter().any(|l| l.contains(PASSWORD)), "the password is never in the log");
}

#[test]
fn a_child_s_account_is_refused_by_the_real_door_in_its_own_words() {
    let me = this_process::account().unwrap();
    let host = RealDoorHost::start(&pipe_name(), PASSWORD, Some(&me));
    let mut door = mine(host.name());
    let refused = ask(&mut door, "sign_in", json!({ "password": PASSWORD })).unwrap();
    assert_eq!(
        refused,
        DoorReply::Refused("LiMo’s settings cannot be opened from Asha’s account. A guardian opens them from their own.".into())
    );
    assert!(door.server().is_none());
    // The password was never sent: the door refused at hello.
    assert!(!host.log().iter().any(|l| l.contains("sign_in")), "{:?}", host.log());
}

#[test]
fn nothing_is_sent_to_the_real_door_when_it_is_checked_as_limo_s_service() {
    let host = RealDoorHost::start(&pipe_name(), PASSWORD, None);
    let program_files = this_process::program_files().unwrap();
    let mut door = Door::new(host.name(), ServerMustBe::limo_service(&program_files));
    let refused = ask(&mut door, "sign_in", json!({ "password": PASSWORD })).unwrap_err();
    assert_eq!(refused, DoorProblem::NotTheService(NotTheService::Session { seen: this_process::session() }));
    std::thread::sleep(Duration::from_millis(300));
    // The door's thread tells the service of a console only once it has
    // read its first bytes: none came.
    assert!(!host.log().iter().any(|l| l.contains("A console connected")), "{:?}", host.log());
}

#[test]
fn a_lost_service_is_said_plainly_and_the_next_request_connects_afresh() {
    let name = pipe_name();
    let host = RealDoorHost::start(&name, PASSWORD, None);
    let mut door = mine(&name);
    assert_eq!(reply(&mut door, "sign_in", json!({ "password": PASSWORD }))["outcome"], "done");
    drop(host);
    let lost = ask(&mut door, "family_today", Value::Null).unwrap_err();
    assert!(matches!(lost, DoorProblem::Lost(_) | DoorProblem::NotRunning), "{lost:?}");
    // The service again: the console connects afresh, signed out.
    let host = RealDoorHost::start(&name, PASSWORD, None);
    let today = reply(&mut door, "family_today", Value::Null);
    assert_eq!(today["problem"]["kind"], "signed_out", "{today}");
    drop(host);
}

#[test]
fn the_console_never_trips_the_real_door_s_message_limit() {
    let host = RealDoorHost::start(&pipe_name(), PASSWORD, None);
    let mut door = mine(host.name());
    let started = Instant::now();
    let mut too_fast = 0;
    for _ in 0..40 {
        match ask(&mut door, "about", Value::Null) {
            Ok(DoorReply::Reply(r)) => assert_eq!(r["outcome"], "done"),
            Err(DoorProblem::TooFast) => too_fast += 1,
            other => panic!("{other:?}"),
        }
    }
    assert!(started.elapsed() < Duration::from_secs(1) || too_fast > 0);
    // The connection is still the door's: the door closes one that sends
    // more than 20 in a second, and this one is open.
    std::thread::sleep(Duration::from_millis(1_100));
    assert_eq!(reply(&mut door, "about", Value::Null)["outcome"], "done");
    assert!(door.server().is_some());
    assert!(!host.log().iter().any(|l| l.contains("messages in one second")), "{:?}", host.log());
}

#[test]
fn every_frame_the_console_writes_is_read_by_the_service_s_own_reader() {
    let read = |frame: &[u8]| {
        let mut reader = ConsoleReader::new();
        reader.push(frame).unwrap();
        reader.next_frame().unwrap().unwrap()
    };
    assert_eq!(read(&encode(&hello()).unwrap()), ConsoleIncoming::Message(FromConsole::Hello { protocol: CONSOLE_PROTOCOL_VERSION }));
    let cases = [
        ("about", Value::Null),
        ("family_today", json!({})),
        ("sign_in", json!({ "password": "correct horse battery" })),
        ("lock_now", json!({ "child": "c1" })),
        ("lock_now", json!({ "child": "c1", "until": 1_800_000_000 })),
        ("give_extra", json!({ "child": "c1", "minutes": 15, "tap_id": "t1" })),
        ("answer_request", json!({ "child": "c1", "request": "d/ask/1", "reply": { "grant": { "minutes": 30 } } })),
        ("change_password", json!({ "old": "a long old one", "new": "a long new one" })),
        ("events", json!({ "limit": 20 })),
    ];
    for (n, (name, args)) in cases.into_iter().enumerate() {
        let frame = PageRequest::from_page(name, args).unwrap().into_frame(n as u32 + 1).unwrap();
        match read(&frame) {
            ConsoleIncoming::Message(FromConsole::Request { id, request }) => {
                assert_eq!(id, n as u32 + 1);
                assert_eq!(request.name(), name);
            }
            other => panic!("{name}: {other:?}"),
        }
    }
    // A request the service cannot read still reaches it as one, by number.
    let frame = PageRequest::from_page("take_over_the_pc", Value::Null).unwrap().into_frame(77).unwrap();
    assert_eq!(read(&frame), ConsoleIncoming::BadRequest { id: 77 });
}

#[test]
fn every_message_the_service_writes_is_read_by_the_console() {
    let mut problem = Problem::new(ProblemKind::MustWait, "Too many wrong tries.");
    problem.wait_until = Some(1_800_000_000);
    let mut wrong = Problem::new(ProblemKind::WrongPassword, "That password is not right. 3 more tries before a wait.");
    wrong.tries_before_wait = Some(3);
    let messages = [
        ToConsole::Hello { protocol: CONSOLE_PROTOCOL_VERSION, version: "0.1.0".into() },
        ToConsole::Refused { message: "LiMo's settings cannot be opened from Asha's account. A guardian opens them from their own.".into() },
        ToConsole::Answer { id: 1, reply: Reply::Done { value: json!({ "has_family": true }) } },
        ToConsole::Answer { id: 2, reply: Reply::Problem { problem } },
        ToConsole::Answer { id: 3, reply: Reply::Problem { problem: wrong } },
    ];
    let mut decoder = Decoder::new();
    for m in &messages {
        decoder.push(&encode_to_console(m).unwrap()).unwrap();
    }
    assert!(matches!(decoder.next_message::<FromDoor>().unwrap(), Some(FromDoor::Hello { protocol: 1, .. })));
    assert!(matches!(decoder.next_message::<FromDoor>().unwrap(), Some(FromDoor::Refused { message }) if message.contains("Asha's account")));
    match decoder.next_message::<FromDoor>().unwrap() {
        Some(FromDoor::Answer { id: 1, reply }) => assert_eq!(reply, json!({ "outcome": "done", "value": { "has_family": true } })),
        other => panic!("{other:?}"),
    }
    match decoder.next_message::<FromDoor>().unwrap() {
        Some(FromDoor::Answer { id: 2, reply }) => {
            assert_eq!(reply["outcome"], "problem");
            assert_eq!(reply["problem"]["kind"], "must_wait");
            assert_eq!(reply["problem"]["wait_until"], 1_800_000_000);
        }
        other => panic!("{other:?}"),
    }
    match decoder.next_message::<FromDoor>().unwrap() {
        Some(FromDoor::Answer { id: 3, reply }) => assert_eq!(reply["problem"]["tries_before_wait"], 3),
        other => panic!("{other:?}"),
    }
}
