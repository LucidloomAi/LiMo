//! The console's door over a real named pipe, in the test's own account: a
//! pipe of the test's own name, served by the door's real threads, with the
//! real service and a real family file behind it, and a real client. Who
//! the client is comes from Windows, through the same seam the service uses
//! on a family's PC; so the test's own account, linked to a child, shows the
//! refusal with no second Windows account.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use limo_windows::console::{CONSOLE_PROTOCOL_VERSION, FromConsole, ProblemKind, Reply, Request, ToConsole};
use limo_windows::protocol::Secret;
use limo_windows::{CLIENT_ACCESS, DOOR_SDDL, DoorClient, RealDoor, is_elevated, this_account};

use crate::common::*;

static NEXT: AtomicU32 = AtomicU32::new(0);

/// A pipe name of this test's own, never the real door's.
pub(crate) fn pipe_name() -> String {
    format!(r"\\.\pipe\limo.console.test.{}.{}", std::process::id(), NEXT.fetch_add(1, Ordering::SeqCst))
}

/// A PC whose door is a real pipe, with the door's own access list and this
/// test's account added to it: the service, as SYSTEM, may make as many
/// instances as it serves consoles; a test in its own account needs the
/// same right on its own pipe.
fn pc_with_a_real_door(first_bytes_wait: Duration) -> (Pc, String) {
    let me = this_account().unwrap();
    pc_with_a_door_of(first_bytes_wait, &format!("{DOOR_SDDL}(A;;GA;;;{me})"))
}

fn pc_with_a_door_of(first_bytes_wait: Duration, sddl: &str) -> (Pc, String) {
    let mut pc = Pc::new(at(16, 0));
    let name = pipe_name();
    pc.os().real_door = Some(RealDoor::open_with(&name, || {}, first_bytes_wait, sddl));
    (pc, name)
}

/// Lets what the door's threads sent reach the service.
fn settle(pc: &mut Pc) {
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(5));
        pc.step();
    }
}

/// Connects, waiting a moment for the door's thread to make the pipe.
pub(crate) fn connect(name: &str) -> DoorClient {
    let give_up = Instant::now() + Duration::from_secs(5);
    loop {
        match DoorClient::connect(name, 1_000) {
            Ok(client) => return client,
            Err(problem) if Instant::now() > give_up => panic!("could not connect: {problem}"),
            Err(_) => std::thread::sleep(Duration::from_millis(10)),
        }
    }
}

/// The next message for the client, while the service steps (its clock is
/// the fake one; the pipe and its threads are real).
fn next(pc: &mut Pc, client: &mut DoorClient) -> Result<ToConsole, String> {
    let give_up = Instant::now() + Duration::from_secs(10);
    loop {
        pc.step();
        if let Some(message) = client.read(10)? {
            return Ok(message);
        }
        if Instant::now() > give_up {
            return Err("nothing came".to_string());
        }
    }
}

/// The client's request and its answer.
fn ask(pc: &mut Pc, client: &mut DoorClient, id: u32, request: Request) -> Reply {
    client.send(&FromConsole::Request { id, request }).unwrap();
    match next(pc, client).unwrap() {
        ToConsole::Answer { id: got, reply } if got == id => reply,
        other => panic!("{other:?}"),
    }
}

/// Steps the service until the client's connection is closed: true when it
/// was within ten seconds.
fn closed_soon(pc: &mut Pc, client: &mut DoorClient) -> bool {
    let give_up = Instant::now() + Duration::from_secs(10);
    while Instant::now() < give_up {
        pc.step();
        if client.read(10).is_err() {
            settle(pc);
            return true;
        }
    }
    false
}

#[test]
fn a_console_over_a_real_named_pipe_is_answered_in_the_tests_own_account() {
    let (mut pc, name) = pc_with_a_real_door(Duration::from_secs(10));
    let mut client = connect(&name);
    // The pipe is served by this very process, as it will be by the service,
    // with the door's access list.
    let server = client.server();
    assert_eq!(server.pid, Some(std::process::id()));
    let sddl = client.access_list().unwrap();
    assert!(sddl.contains("IU") && sddl.contains("SY") && sddl.contains("BA"), "{sddl}");
    assert!(!sddl.contains(";;;WD)") && !sddl.contains(";;;AU)"), "{sddl}");

    client.send(&FromConsole::Hello { protocol: CONSOLE_PROTOCOL_VERSION }).unwrap();
    assert!(matches!(next(&mut pc, &mut client).unwrap(), ToConsole::Hello { protocol: CONSOLE_PROTOCOL_VERSION, .. }));
    assert!(matches!(ask(&mut pc, &mut client, 1, Request::FamilyState), Reply::Done { value } if value["has_family"] == true));
    assert!(matches!(ask(&mut pc, &mut client, 2, Request::FamilyToday), Reply::Problem { problem } if problem.kind == ProblemKind::SignedOut));
    assert!(matches!(ask(&mut pc, &mut client, 3, Request::SignIn { password: Secret::new(PASSWORD.into()) }), Reply::Done { .. }));
    assert!(matches!(ask(&mut pc, &mut client, 4, Request::FamilyToday), Reply::Done { value } if value.is_array()));
    // The service was told who the client is by Windows: this process.
    let pid = std::process::id();
    assert_eq!(pc.logged(&format!("(pid {pid})")).iter().filter(|l| l.contains("A console connected")).count(), 1, "{:?}", pc.os().log);
    assert_eq!(pc.service.door_sessions(), 1);
}

#[test]
fn the_tests_own_account_linked_to_a_child_is_refused_over_the_real_pipe() {
    let (mut pc, name) = pc_with_a_real_door(Duration::from_secs(10));
    let me = this_account().unwrap();
    pc.add_child("Asha", &me, 60);
    let mut client = connect(&name);
    client.send(&FromConsole::Hello { protocol: CONSOLE_PROTOCOL_VERSION }).unwrap();
    assert_eq!(
        next(&mut pc, &mut client).unwrap(),
        ToConsole::Refused {
            message: "LiMo’s settings cannot be opened from Asha’s account. A guardian opens them from their own.".into()
        }
    );
    // A password sent after is never answered: the connection is closed.
    let _ = client.send(&FromConsole::Request { id: 1, request: Request::SignIn { password: Secret::new(PASSWORD.into()) } });
    assert!(closed_soon(&mut pc, &mut client));
    assert_eq!(pc.service.door_sessions(), 0);
    assert_eq!(pc.logged("refused to a console").len(), 1);
}

#[test]
fn two_clients_at_once_over_the_real_pipe_each_have_their_own_session() {
    let (mut pc, name) = pc_with_a_real_door(Duration::from_secs(10));
    let mut first = connect(&name);
    let mut second = connect(&name);
    for client in [&mut first, &mut second] {
        client.send(&FromConsole::Hello { protocol: CONSOLE_PROTOCOL_VERSION }).unwrap();
        assert!(matches!(next(&mut pc, client).unwrap(), ToConsole::Hello { .. }));
    }
    assert!(matches!(ask(&mut pc, &mut first, 1, Request::SignIn { password: Secret::new(PASSWORD.into()) }), Reply::Done { .. }));
    assert!(matches!(ask(&mut pc, &mut second, 1, Request::PcSettings), Reply::Problem { problem } if problem.kind == ProblemKind::SignedOut));
    assert!(matches!(ask(&mut pc, &mut first, 2, Request::PcSettings), Reply::Done { .. }));
    drop(first);
    let give_up = Instant::now() + Duration::from_secs(5);
    while pc.service.door_sessions() > 0 && Instant::now() < give_up {
        pc.step();
    }
    assert_eq!(pc.service.door_sessions(), 0, "the closed connection’s session ended");
    assert!(matches!(ask(&mut pc, &mut second, 2, Request::About), Reply::Done { .. }));
}

#[test]
fn hostile_bytes_over_the_real_pipe_close_it_and_a_client_that_says_nothing_is_closed() {
    let (mut pc, name) = pc_with_a_real_door(Duration::from_millis(500));
    // Garbage, after a good hello.
    let mut client = connect(&name);
    client.send(&FromConsole::Hello { protocol: CONSOLE_PROTOCOL_VERSION }).unwrap();
    assert!(matches!(next(&mut pc, &mut client).unwrap(), ToConsole::Hello { .. }));
    let mut garbage = CONSOLE_PROTOCOL_VERSION.to_le_bytes().to_vec();
    garbage.extend_from_slice(&(15u32).to_le_bytes());
    garbage.extend_from_slice(b"not json at all");
    client.write_raw(&garbage).unwrap();
    assert!(closed_soon(&mut pc, &mut client));
    assert_eq!(pc.logged("was closed, because it sent a message LiMo cannot read").len(), 1, "{:?}", pc.os().log);
    // A frame that says it is ten megabytes long.
    let mut client = connect(&name);
    let mut huge = CONSOLE_PROTOCOL_VERSION.to_le_bytes().to_vec();
    huge.extend_from_slice(&(10u32 * 1024 * 1024).to_le_bytes());
    client.write_raw(&huge).unwrap();
    assert!(closed_soon(&mut pc, &mut client));
    // A flood of requests, faster than any screen asks.
    let mut client = connect(&name);
    client.send(&FromConsole::Hello { protocol: CONSOLE_PROTOCOL_VERSION }).unwrap();
    assert!(matches!(next(&mut pc, &mut client).unwrap(), ToConsole::Hello { .. }));
    let mut flood = Vec::new();
    for id in 0..60 {
        flood.extend_from_slice(&limo_windows::console::encode_from_console(&FromConsole::Request { id, request: Request::About }).unwrap());
    }
    let _ = client.write_raw(&flood);
    assert!(closed_soon(&mut pc, &mut client));
    // Connected, and nothing said: closed by the door's own thread.
    let mut silent = connect(&name);
    let started = Instant::now();
    assert!(closed_soon(&mut pc, &mut silent));
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(pc.logged("sent nothing for 0.5 seconds, so its connection was closed").len(), 1, "{:?}", pc.os().log);
    // The door is still open for the next.
    let mut after = connect(&name);
    after.send(&FromConsole::Hello { protocol: CONSOLE_PROTOCOL_VERSION }).unwrap();
    assert!(matches!(next(&mut pc, &mut after).unwrap(), ToConsole::Hello { .. }));
}

/// Opens the pipe with `access` and closes it, trying again while no
/// instance is ready (the door makes the next one when the last closes).
fn try_open(name: &str, access: u32) -> Result<(), u32> {
    let give_up = Instant::now() + Duration::from_secs(5);
    loop {
        match DoorClient::try_open(name, access) {
            Err(2 | 231) if Instant::now() < give_up => std::thread::sleep(Duration::from_millis(20)),
            other => return other,
        }
    }
}

#[test]
fn only_system_and_administrators_may_make_an_instance_of_the_door() {
    // The door's own access list, exactly as the service makes it, made by
    // this test's account, which is then the pipe's owner.
    let (mut pc, name) = pc_with_a_door_of(Duration::from_secs(10), DOOR_SDDL);
    assert_eq!(try_open(&name, CLIENT_ACCESS), Ok(()));
    let mut client = connect(&name);
    client.send(&FromConsole::Hello { protocol: CONSOLE_PROTOCOL_VERSION }).unwrap();
    assert!(matches!(next(&mut pc, &mut client).unwrap(), ToConsole::Hello { .. }));
    // With a console connected, the door makes the next instance before it
    // serves it. Windows checks the right to make an instance
    // (FILE_CREATE_PIPE_INSTANCE) there, against the access list, and only
    // SYSTEM and Administrators have it: so the next instance is refused to
    // a process that is not elevated, its owner included, as it is to any
    // program in a child's account that would serve the door in LiMo's
    // place. (A client's own open is not where Windows checks that right.)
    if is_elevated() {
        return;
    }
    let give_up = Instant::now() + Duration::from_secs(5);
    while pc.logged("could not be made ready for another console").is_empty() && Instant::now() < give_up {
        settle(&mut pc);
    }
    let said = pc.logged("could not be made ready for another console");
    assert_eq!(said.len(), 1, "{:?}", pc.os().log);
    assert!(said[0].contains("Win32 error 5"), "{}", said[0]);
    // When the console goes, the door is made again as the first of its
    // name, and serves the next.
    drop(client);
    settle(&mut pc);
    let mut next_one = connect(&name);
    next_one.send(&FromConsole::Hello { protocol: CONSOLE_PROTOCOL_VERSION }).unwrap();
    assert!(matches!(next(&mut pc, &mut next_one).unwrap(), ToConsole::Hello { .. }));
}

#[test]
fn a_door_whose_name_is_already_taken_is_not_opened_and_says_so() {
    let (mut pc, name) = pc_with_a_real_door(Duration::from_secs(10));
    let _first = connect(&name);
    // A second door of the same name, as a program that took the name
    // first would make it: it cannot make the first instance.
    let mut second = RealDoor::open(&name, || {});
    let give_up = Instant::now() + Duration::from_secs(5);
    let mut notes = Vec::new();
    while notes.is_empty() && Instant::now() < give_up {
        notes = second.received();
        std::thread::sleep(Duration::from_millis(10));
    }
    let said = format!("{notes:?}");
    assert!(said.contains("could not be opened") && said.contains("Another program may hold its name"), "{said}");
    drop(second);
    // The first door still serves.
    pc.step();
    let mut client = connect(&name);
    client.send(&FromConsole::Hello { protocol: CONSOLE_PROTOCOL_VERSION }).unwrap();
    assert!(matches!(next(&mut pc, &mut client).unwrap(), ToConsole::Hello { .. }));
}

#[test]
fn a_session_that_opens_connection_after_connection_is_capped_and_the_next_closed_at_once() {
    let (mut pc, name) = pc_with_a_real_door(Duration::from_secs(10));
    // Four silent connections from this session hold their places...
    let held: Vec<DoorClient> = (0..4).map(|_| connect(&name)).collect();
    // ...and the fifth is closed at once, before anything is read.
    let mut fifth = connect(&name);
    let started = Instant::now();
    assert!(closed_soon(&mut pc, &mut fifth));
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(pc.logged("at most 4 from one session").len(), 1, "{:?}", pc.os().log);
    // When they go, the door serves this session again.
    drop(held);
    settle(&mut pc);
    let mut after = connect(&name);
    after.send(&FromConsole::Hello { protocol: CONSOLE_PROTOCOL_VERSION }).unwrap();
    assert!(matches!(next(&mut pc, &mut after).unwrap(), ToConsole::Hello { .. }));
}
