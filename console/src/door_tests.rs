//! The door's two halves over real named pipes, in the test's own account:
//! the stand in serves, the console's [`Door`] asks. Each test makes pipes
//! under a name of its own (`limo.console.test.<process>.<n>`), never
//! LiMo's own door.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use crate::door::{Door, DoorProblem, NotTheService, ServerMustBe};
use crate::protocol::{Answer, PROTOCOL_VERSION, Refusal, Request, Secret, encode};
use crate::stand_in::{CLIENT_ACCESS, DOOR_SDDL, LOOKS_RIGHT_SDDL, StandIn, StandInServer};
use crate::this_process;
use crate::win::pipe::{OpenProblem, create_server, open_client, read_some, write_all};

const PASSWORD: &str = "Mira's long sentence";

fn pipe_name() -> String {
    static N: AtomicU32 = AtomicU32::new(0);
    format!(r"\\.\pipe\limo.console.test.{}.{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst))
}

fn stand_in(sddl: &str) -> StandInServer {
    StandInServer::start(&pipe_name(), sddl, StandIn::new(Secret::new(PASSWORD.to_string()))).unwrap()
}

fn sign_in(words: &str) -> Request {
    Request::SignIn { password: Secret::new(words.to_string()) }
}

/// Waits a moment for the stand in to write down what a connection did.
fn settle() {
    std::thread::sleep(Duration::from_millis(50));
}

#[test]
fn a_guardian_s_console_signs_in_through_a_real_pipe() {
    let server = stand_in(DOOR_SDDL);
    let mut door = Door::new(server.name(), this_process::as_server().unwrap());
    assert!(matches!(door.ask(&Request::About).unwrap(), Answer::About { family: true, .. }));
    assert_eq!(door.ask(&sign_in("not it")).unwrap(), Answer::WrongPassword { tries_before_wait: 3 });
    assert_eq!(door.ask(&sign_in(PASSWORD)).unwrap(), Answer::SignedIn);
    assert_eq!(door.server().map(|s| s.pid), Some(std::process::id()));
    drop(door);
    settle();

    let served = server.served();
    assert_eq!(served.len(), 1, "one connection for all three: {served:?}");
    let s = &served[0];
    assert_eq!(s.requests, vec!["about", "sign_in", "sign_in"]);
    assert_eq!(s.bad_frames, 0);
    // The door knows who knocked: this process, this account, by both ways.
    assert_eq!(s.client.pid, std::process::id());
    assert_eq!(s.client.account(), this_process::account().as_deref());
    // The console lent its token for identification only (level 1): the
    // server may read who the guardian is, never act as the guardian.
    assert_eq!(s.lent_level, Some(1));
}

#[test]
fn an_account_marked_as_a_child_s_is_refused_and_told_why() {
    let me = this_process::account().unwrap();
    let door_state = StandIn::new(Secret::new(PASSWORD.to_string())).with_child_account(&me, "Asha");
    let server = StandInServer::start(&pipe_name(), DOOR_SDDL, door_state).unwrap();
    let mut door = Door::new(server.name(), this_process::as_server().unwrap());
    let refused = Answer::Refused { why: Refusal::ChildAccount { child: "Asha".to_string() } };
    assert_eq!(door.ask(&Request::About).unwrap(), refused);
    // Both ends closed the connection after the refusal: the next request
    // opens a new one, and is refused again.
    assert!(door.server().is_none());
    assert_eq!(door.ask(&sign_in(PASSWORD)).unwrap(), refused);
    drop(door);
    settle();
    let served = server.served();
    assert_eq!(served.len(), 2, "{served:?}");
    assert!(served.iter().all(|s| s.requests.len() == 1), "{served:?}");
}

#[test]
fn the_console_sends_nothing_to_a_door_that_is_not_limo_s_service() {
    let server = stand_in(DOOR_SDDL);
    let program_files = this_process::program_files().unwrap();
    let mine = this_process::as_server().unwrap();

    // LiMo's own rule against a pipe served in this session by a test.
    let mut door = Door::new(server.name(), ServerMustBe::limo_service(&program_files));
    let refused = door.ask(&sign_in(PASSWORD)).unwrap_err();
    assert_eq!(refused, DoorProblem::NotTheService(NotTheService::Session { seen: this_process::session() }));
    assert!(refused.words().contains("password was not sent"));

    // Each of the three checks refuses alone.
    let mut owner = Door::new(server.name(), ServerMustBe { owner: crate::door::LOCAL_SYSTEM.to_string(), ..mine.clone() });
    assert_eq!(owner.ask(&Request::About).unwrap_err(), DoorProblem::NotTheService(NotTheService::Owner));
    let mut image = Door::new(server.name(), ServerMustBe { image: format!(r"{program_files}\LiMo\limo.exe"), ..mine.clone() });
    assert_eq!(image.ask(&Request::About).unwrap_err(), DoorProblem::NotTheService(NotTheService::Image));
    drop((door, owner, image));
    settle();
    // Three connections opened and closed, and not one byte written.
    assert_eq!(server.served(), vec![]);

    // And the same pipe, checked as what it is, answers.
    let mut right = Door::new(server.name(), mine);
    assert!(matches!(right.ask(&Request::About).unwrap(), Answer::About { .. }));
}

#[test]
fn nothing_serving_the_door_is_said_plainly() {
    let mut door = Door::new(&pipe_name(), this_process::as_server().unwrap());
    assert_eq!(door.ask(&Request::About).unwrap_err(), DoorProblem::NotRunning);
    assert_eq!(DoorProblem::NotRunning.words(), "LiMo's service is not running on this PC, so its settings cannot open.");
}

#[test]
fn the_door_s_access_list_reads_back_as_written() {
    let server = stand_in(DOOR_SDDL);
    let sddl = server.sddl().unwrap().to_string();
    assert!(sddl.contains("D:P"), "{sddl}");
    assert!(sddl.contains(";;;SY)") && sddl.contains(";;;BA)"), "{sddl}");
    assert!(sddl.contains("(A;;0x12018b;;;IU)"), "{sddl}");
    // The owner is this test's account, as the service's would be
    // LocalSystem.
    assert!(sddl.starts_with(&format!("O:{}", this_process::account().unwrap())), "{sddl}");
}

#[test]
fn a_second_server_beside_the_door_is_refused_and_the_name_cannot_be_taken_first() {
    if this_process::elevated() {
        eprintln!("skipped: this test runs elevated, and Administrators may serve the door");
        return;
    }
    // With the door's own list, an interactive user cannot serve beside it.
    let server = stand_in(DOOR_SDDL);
    assert_eq!(create_server(server.name(), DOOR_SDDL, false).err(), Some(5), "access is denied");
    // Nor make it first: the name is taken.
    assert!(create_server(server.name(), DOOR_SDDL, true).is_err());

    // The list that looks right ("GW" for interactive users) carries the
    // right to make another instance, so a program in a child's account
    // could answer the next console that knocks.
    let leaky = stand_in(LOOKS_RIGHT_SDDL);
    let beside = create_server(leaky.name(), LOOKS_RIGHT_SDDL, false);
    assert!(beside.is_ok(), "the hole this list leaves: {:?}", beside.err());
}

#[test]
fn the_door_grants_a_client_what_it_needs_and_not_generic_write() {
    if this_process::elevated() {
        eprintln!("skipped: this test runs elevated, and Administrators may do anything");
        return;
    }
    let server = stand_in(DOOR_SDDL);
    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    let generic = open_client(server.name(), GENERIC_READ | GENERIC_WRITE, Duration::from_secs(1));
    assert_eq!(generic.err(), Some(OpenProblem::Failed(5)));
    assert!(open_client(server.name(), CLIENT_ACCESS, Duration::from_secs(1)).is_ok());
}

#[test]
fn hostile_frames_close_the_connection_and_say_nothing_back() {
    let server = stand_in(DOOR_SDDL);
    let mut bytes_each = Vec::new();
    for hostile in [
        // Another version of the protocol.
        { let mut b = 9u16.to_le_bytes().to_vec(); b.extend_from_slice(&2u32.to_le_bytes()); b.extend_from_slice(b"{}"); b },
        // A length past the longest message.
        { let mut b = PROTOCOL_VERSION.to_le_bytes().to_vec(); b.extend_from_slice(&(2u32 << 20).to_le_bytes()); b },
        // A request that does not exist.
        { let body = br#"{"request":"delete_family"}"#; let mut b = PROTOCOL_VERSION.to_le_bytes().to_vec(); b.extend_from_slice(&(body.len() as u32).to_le_bytes()); b.extend_from_slice(body); b },
        // Not JSON at all.
        { let mut b = PROTOCOL_VERSION.to_le_bytes().to_vec(); b.extend_from_slice(&4u32.to_le_bytes()); b.extend_from_slice(b"\x00\xff\x00\xff"); b },
    ] {
        let pipe = open_client(server.name(), CLIENT_ACCESS, Duration::from_secs(1)).unwrap();
        write_all(&pipe, &hostile).unwrap();
        let mut back = [0u8; 64];
        // The door closes without a word.
        assert_eq!(read_some(&pipe, &mut back), Ok(0));
        bytes_each.push(hostile.len());
    }
    settle();
    let served = server.served();
    assert_eq!(served.len(), 4, "{served:?}");
    assert!(served.iter().all(|s| s.bad_frames == 1 && s.requests.is_empty()), "{served:?}");

    // A good request still works after them.
    let mut door = Door::new(server.name(), this_process::as_server().unwrap());
    assert!(matches!(door.ask(&Request::About).unwrap(), Answer::About { .. }));
    // And a frame encoded by the console is what the stand in reads.
    assert!(encode(&Request::About).is_ok());
}
