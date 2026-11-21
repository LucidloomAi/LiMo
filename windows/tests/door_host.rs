//! The door's test host, as another crate's tests use it: the real service
//! and door on a pipe of the test's own, knocked on with a real client; and
//! the uninstaller's own line to the door, over the same real pipe.

#![cfg(windows)]

use std::time::{Duration, Instant};

use limo_hub::AgeBand;
use limo_windows::installer::DoorOpen;
use limo_windows::{DoorClient, DoorLine};
use limo_windows::console::{CONSOLE_PROTOCOL_VERSION, FromConsole, Reply, Request, ToConsole};
use limo_windows::door_host::{DoorHost, DoorHostSetup, HOST_PASSWORD};
use limo_windows::protocol::Secret;

fn pipe(name: &str) -> String {
    format!(r"\\.\pipe\limo.console.test.host.{}.{name}", std::process::id())
}

fn connect(name: &str) -> DoorClient {
    let give_up = Instant::now() + Duration::from_secs(5);
    loop {
        match DoorClient::connect(name, 1_000) {
            Ok(client) => return client,
            Err(problem) if Instant::now() > give_up => panic!("could not connect: {problem}"),
            Err(_) => std::thread::sleep(Duration::from_millis(10)),
        }
    }
}

fn next(client: &mut DoorClient) -> ToConsole {
    let give_up = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(message) = client.read(50).unwrap() {
            return message;
        }
        assert!(Instant::now() < give_up, "nothing came");
    }
}

fn ask(client: &mut DoorClient, id: u32, request: Request) -> Reply {
    client.send(&FromConsole::Request { id, request }).unwrap();
    match next(client) {
        ToConsole::Answer { id: got, reply } if got == id => reply,
        other => panic!("{other:?}"),
    }
}

fn hello(client: &mut DoorClient) {
    client.send(&FromConsole::Hello { protocol: CONSOLE_PROTOCOL_VERSION }).unwrap();
    assert!(matches!(next(client), ToConsole::Hello { protocol: CONSOLE_PROTOCOL_VERSION, .. }));
}

#[test]
fn another_crates_test_signs_in_at_the_hosts_door_and_is_answered() {
    let folder = tempfile::tempdir().unwrap();
    let name = pipe("family");
    let host = DoorHost::start(DoorHostSetup::new(&name, folder.path())).unwrap();
    assert_eq!(host.pipe_name(), name);
    let mut client = connect(host.pipe_name());
    hello(&mut client);
    assert!(matches!(ask(&mut client, 1, Request::SignIn { password: Secret::new(HOST_PASSWORD.into()) }), Reply::Done { .. }));
    let added = match ask(&mut client, 2, Request::AddChild { name: "Mira".into(), age_band: AgeBand::SixToNine }) {
        Reply::Done { value } => value,
        other => panic!("{other:?}"),
    };
    assert!(added["child_id"].as_str().is_some_and(|id| id.starts_with("child-")));
    assert!(matches!(ask(&mut client, 3, Request::FamilyToday), Reply::Done { value } if value.as_array().is_some_and(|d| d.len() == 1)));
    assert!(host.log().iter().any(|line| line.contains("add_child done")), "{:?}", host.log());
    drop(client);
    drop(host);
    // The door closed with the host.
    assert!(DoorClient::connect(&name, 200).is_err());
}

#[test]
fn a_host_with_no_family_starts_where_the_first_start_does_and_never_takes_the_real_doors_name() {
    let folder = tempfile::tempdir().unwrap();
    let mut setup = DoorHostSetup::new(&pipe("first"), folder.path());
    setup.family_password = None;
    let host = DoorHost::start(setup).unwrap();
    let mut client = connect(host.pipe_name());
    hello(&mut client);
    assert!(matches!(ask(&mut client, 1, Request::FamilyState), Reply::Done { value } if value["has_family"] == false));
    assert!(matches!(ask(&mut client, 2, Request::CreateFamily { password: Secret::new("a password of the test".into()) }), Reply::Done { .. }));
    assert!(matches!(ask(&mut client, 3, Request::FamilyState), Reply::Done { value } if value["has_family"] == true && value["signed_in"] == true));
    let real = DoorHostSetup::new(r"\\.\pipe\limo.console", folder.path());
    assert!(DoorHost::start(real).unwrap_err().contains("never takes the real door"));
}

#[test]
fn the_uninstallers_line_refuses_a_door_not_served_by_a_service_and_checks_the_password_through_a_real_one() {
    let folder = tempfile::tempdir().unwrap();
    let host = DoorHost::start(DoorHostSetup::new(&pipe("line"), folder.path())).unwrap();
    // The host runs in this test's own session, not session 0: as the
    // uninstaller opens the door, it is refused before anything is sent.
    match DoorLine::open(host.pipe_name(), true) {
        Err(DoorOpen::NotTheService(why)) => assert!(why.contains("not a service"), "{why}"),
        other => panic!("{other:?}"),
    }
    assert!(host.log().iter().all(|line| !line.contains("sign_in")));
    // Opened as a test may, the same line asks and is answered.
    let (mut line, version) = DoorLine::open(host.pipe_name(), false).unwrap();
    assert_eq!(version, limo_windows::windows_version());
    assert!(matches!(line.ask(Request::FamilyState).unwrap(), Reply::Done { value } if value["has_family"] == true && value["removal_allowed_until"].is_null()));
    let wrong = line.ask(Request::SignIn { password: Secret::new("not the password".into()) }).unwrap();
    assert!(matches!(&wrong, Reply::Problem { problem } if problem.message == "That password is not right. 4 more tries before a wait."), "{wrong:?}");
    // The next secret waits for the door's second.
    let started = Instant::now();
    assert!(matches!(line.ask(Request::SignIn { password: Secret::new(HOST_PASSWORD.into()) }).unwrap(), Reply::Done { .. }));
    assert!(started.elapsed() >= Duration::from_millis(1_000));
    // Allowed removal shows in the family's state for the next line.
    assert!(matches!(line.ask(Request::AllowRemoval { password: Secret::new(HOST_PASSWORD.into()) }).unwrap(), Reply::Done { .. }));
    drop(line);
    let (mut next, _) = DoorLine::open(host.pipe_name(), false).unwrap();
    assert!(matches!(next.ask(Request::FamilyState).unwrap(), Reply::Done { value } if value["removal_allowed_until"].is_i64()));
}
