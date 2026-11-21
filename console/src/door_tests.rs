//! The console's half of the door over real named pipes, against a stand in
//! that speaks the door's protocol and misbehaves on purpose: a door that
//! refuses, closes, answers nonsense or says nothing, and a pipe that is not
//! LiMo's service. Each test makes pipes under a name of its own
//! (`limo.console.test.<process>.<n>`), never LiMo's own door. What the real
//! door decides is in `real_door_tests.rs`.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use serde_json::{Value, json};

use crate::door::{Door, DoorProblem, DoorReply, NotTheService, ServerMustBe};
use crate::protocol::PageRequest;
use crate::stand_in::{Script, StandIn};
use crate::this_process;
use crate::win::pipe::{CLIENT_ACCESS, OpenProblem, open_client};

const PASSWORD: &str = "Mira's long sentence";

pub(crate) fn pipe_name() -> String {
    static N: AtomicU32 = AtomicU32::new(0);
    format!(r"\\.\pipe\limo.console.test.{}.{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst))
}

fn ask(door: &mut Door, name: &str, args: Value) -> Result<DoorReply, DoorProblem> {
    door.ask(PageRequest::from_page(name, args).unwrap())
}

fn mine(name: &str) -> Door {
    Door::new(name, this_process::as_server().unwrap())
}

/// Waits a moment for the stand in to write down what a connection did.
fn settle() {
    std::thread::sleep(Duration::from_millis(50));
}

#[test]
fn the_console_says_hello_then_numbers_each_request_on_one_connection() {
    let door_side = StandIn::start(&pipe_name(), Script::Door { password: PASSWORD.into() });
    let mut door = mine(door_side.name());
    let about = ask(&mut door, "about", Value::Null).unwrap();
    assert_eq!(about, DoorReply::Reply(json!({ "outcome": "done", "value": { "version": "0.1.0", "protocol": 1, "pc_name": "Family PC" } })));
    let wrong = ask(&mut door, "sign_in", json!({ "password": "not it" })).unwrap();
    let DoorReply::Reply(wrong) = wrong else { panic!() };
    assert_eq!(wrong["problem"]["kind"], "wrong_password");
    assert_eq!(wrong["problem"]["tries_before_wait"], 3);
    assert_eq!(ask(&mut door, "sign_in", json!({ "password": PASSWORD })).unwrap(), DoorReply::Reply(json!({ "outcome": "done", "value": {} })));
    drop(door);
    settle();
    let heard = door_side.heard();
    assert_eq!(heard.len(), 1, "one connection for all: {heard:?}");
    let said: Vec<Value> = heard[0].messages.clone();
    assert_eq!(said[0], json!({ "say": "hello", "protocol": 1 }));
    assert_eq!(said[1], json!({ "say": "request", "id": 1, "request": { "name": "about" } }));
    assert_eq!(said[2]["id"], 2);
    assert_eq!(said[3]["id"], 3);
}

#[test]
fn a_refusal_at_hello_is_the_door_s_own_sentence_and_nothing_follows_it() {
    let words = "LiMo’s settings cannot be opened from Asha’s account. A guardian opens them from their own.";
    let door_side = StandIn::start(&pipe_name(), Script::Refuse { message: words.into() });
    let mut door = mine(door_side.name());
    assert_eq!(ask(&mut door, "sign_in", json!({ "password": PASSWORD })).unwrap(), DoorReply::Refused(words.into()));
    assert!(door.server().is_none());
    drop(door);
    settle();
    // Only the hello was sent: the password never left the console.
    let heard = door_side.heard();
    assert_eq!(heard[0].messages, vec![json!({ "say": "hello", "protocol": 1 })]);
}

#[test]
fn nothing_is_sent_to_a_pipe_that_is_not_limo_s_service() {
    let door_side = StandIn::start(&pipe_name(), Script::Door { password: PASSWORD.into() });
    let program_files = this_process::program_files().unwrap();
    let mut door = Door::new(door_side.name(), ServerMustBe::limo_service(&program_files));
    let refused = ask(&mut door, "sign_in", json!({ "password": PASSWORD })).unwrap_err();
    assert_eq!(refused, DoorProblem::NotTheService(NotTheService::Session { seen: this_process::session() }));
    assert!(refused.words().contains("password was not sent"));
    assert!(door.take_notes().iter().any(|n| n.starts_with("nothing was sent to the pipe")));
    drop(door);
    settle();
    assert!(door_side.heard().is_empty(), "not one byte: {:?}", door_side.heard());
    // Checked as what it is, the same pipe answers.
    assert!(ask(&mut mine(door_side.name()), "about", Value::Null).is_ok());
}

#[test]
fn a_door_that_closes_is_lost_and_the_next_request_connects_afresh() {
    let door_side = StandIn::start(&pipe_name(), Script::CloseAfterHello);
    let mut door = mine(door_side.name());
    let lost = ask(&mut door, "family_today", Value::Null).unwrap_err();
    assert!(matches!(lost, DoorProblem::Lost(_)), "{lost:?}");
    assert_eq!(lost.words(), "LiMo’s service stopped answering. Try again: the console asks it afresh, and you sign in again.");
    assert!(door.server().is_none());
    assert!(matches!(ask(&mut door, "family_today", Value::Null), Err(DoorProblem::Lost(_))));
    drop(door);
    settle();
    assert_eq!(door_side.heard().len(), 2, "a new connection, with a new hello");
}

#[test]
fn nonsense_a_wrong_number_or_silence_from_the_door_is_a_lost_door() {
    for script in [Script::Garbage, Script::WrongNumber, Script::Silent] {
        let door_side = StandIn::start(&pipe_name(), script.clone());
        let mut door = mine(door_side.name()).answer_wait(Duration::from_millis(400));
        let got = ask(&mut door, "about", Value::Null);
        assert!(matches!(got, Err(DoorProblem::Lost(_))), "{script:?}: {got:?}");
        assert!(door.server().is_none(), "{script:?}");
    }
}

#[test]
fn nothing_serving_the_door_is_said_plainly() {
    let mut door = mine(&pipe_name());
    assert_eq!(ask(&mut door, "about", Value::Null).unwrap_err(), DoorProblem::NotRunning);
    assert_eq!(DoorProblem::NotRunning.words(), "LiMo’s service is not running on this PC, so its settings cannot open.");
}

#[test]
fn a_page_that_asks_too_often_is_told_so_and_nothing_more_is_sent() {
    let door_side = StandIn::start(&pipe_name(), Script::Door { password: PASSWORD.into() });
    let mut door = mine(door_side.name());
    let mut too_fast = 0;
    for _ in 0..30 {
        match ask(&mut door, "about", Value::Null) {
            Ok(_) => {}
            Err(DoorProblem::TooFast) => too_fast += 1,
            Err(other) => panic!("{other:?}"),
        }
    }
    assert!(too_fast >= 15, "{too_fast}");
    drop(door);
    settle();
    // The hello and at most 14 requests in that second: under the door's 20.
    let sent = door_side.heard()[0].messages.len();
    assert!(sent <= crate::door::MAX_SENT_PER_SECOND, "{sent}");
}

#[test]
fn the_console_asks_for_no_more_than_it_needs_and_the_door_s_list_grants_it() {
    // Read, write the data, write the attributes: not the right to make an
    // instance (FILE_APPEND_DATA, 0x4), not the extended attributes (0x10).
    assert_eq!(CLIENT_ACCESS & 0x4, 0);
    assert_eq!(CLIENT_ACCESS & 0x10, 0);
    let door_side = StandIn::start(&pipe_name(), Script::Door { password: PASSWORD.into() });
    let sddl = door_side.sddl().unwrap().to_string();
    assert!(sddl.contains("(A;;0x12019b;;;IU)"), "{sddl}");
    assert!(open_client(door_side.name(), CLIENT_ACCESS, Duration::from_secs(1)).is_ok());
    // A pipe nobody serves is not there, whatever is asked.
    assert_eq!(open_client(&pipe_name(), CLIENT_ACCESS, Duration::from_millis(100)).err(), Some(OpenProblem::NotThere));
}
