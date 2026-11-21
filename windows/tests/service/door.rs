//! The console's door: a family made through it with no scaffolding; a
//! child's account refused in the guardian's words; the password's waits;
//! nothing answered before sign in; a session per connection that ends
//! after fifteen idle minutes; hostile input that closes a connection and
//! reaches nothing; the lock of an account not set up taking the guardian
//! PIN; and the service's step time with the door busy.

use std::time::{Duration, Instant};

use limo_agent::{LockWhy, Show};
use limo_core::{BlockedWindow, DailyBudget, PinAction, RuleSet, TryOutcome, WeekBudgets};
use limo_hub::{AccountLink, AccountStanding, AgeBand, UnlinkedAccounts};
use limo_windows::console::{CONSOLE_PROTOCOL_VERSION, FromConsole, MAX_CONSOLE_MESSAGE_BYTES, ProblemKind, Reply, Request, ToConsole};
use limo_windows::protocol::{Answer, FromHelper, Secret, ToHelper};
use limo_windows::service::door::{MAX_CONNECTIONS, SESSION_IDLE_MS};

use crate::common::*;

const GUARDIAN: &str = "S-1-5-21-1000";
const ASHA: &str = "S-1-5-21-1001";
const MIRA: &str = "S-1-5-21-1002";
const VISITOR: &str = "S-1-5-21-1009";

fn secret(text: &str) -> Secret {
    Secret::new(text.into())
}

/// The problem a request met, or a failure of the test.
fn problem(reply: Option<Reply>) -> limo_windows::console::Problem {
    match reply {
        Some(Reply::Problem { problem }) => problem,
        other => panic!("not a problem: {other:?}"),
    }
}

fn frame_of(version: u16, json: &str) -> Vec<u8> {
    let mut bytes = version.to_le_bytes().to_vec();
    bytes.extend_from_slice(&(json.len() as u32).to_le_bytes());
    bytes.extend_from_slice(json.as_bytes());
    bytes
}

/// The answers a helper got to its ways out.
fn way_out_answers(pc: &mut Pc, session: u32) -> Vec<(u32, Answer)> {
    pc.helper(session)
        .map(|h| {
            h.got
                .iter()
                .filter_map(|m| match m {
                    ToHelper::Answer { ask, answer } => Some((*ask, answer.clone())),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Moves both clocks on by `secs` at once, then one step.
fn leap(pc: &mut Pc, secs: u64) {
    let os = pc.os();
    os.steady_ms += secs * 1_000;
    os.wall_ms += (secs * 1_000) as i64;
    pc.step();
}

/// A PC with a family, Asha with an hour a day, a PIN, and the guardian's
/// own account in session 1, with a console from it that said hello.
fn family_with_a_console() -> (Pc, String, limo_windows::os::DoorId) {
    let mut pc = Pc::new(at(16, 0));
    let asha = pc.add_child("Asha", ASHA, 60);
    let now = pc.now();
    pc.hub().set_pin(PIN, now).unwrap();
    pc.os().accounts = vec![account(GUARDIAN, "guardian", true), account(ASHA, "asha", false), account(VISITOR, "visitor", false)];
    pc.login(1, GUARDIAN, "guardian");
    pc.os().sessions[0].is_administrator = true;
    pc.run(1);
    let door = pc.console(GUARDIAN, 1);
    assert!(matches!(pc.hello(door), Some(ToConsole::Hello { protocol: CONSOLE_PROTOCOL_VERSION, .. })));
    (pc, asha, door)
}

#[test]
fn a_family_is_made_through_the_door_with_no_dev_setup() {
    let mut pc = Pc::empty(at(16, 0));
    pc.os().accounts = vec![account(GUARDIAN, "guardian", true), account(ASHA, "asha", false), account(VISITOR, "visitor", false)];
    pc.login(1, GUARDIAN, "guardian");
    pc.os().sessions[0].is_administrator = true;
    pc.run(1);
    let door = pc.console(GUARDIAN, 1);
    assert!(matches!(pc.hello(door), Some(ToConsole::Hello { protocol: CONSOLE_PROTOCOL_VERSION, .. })));
    let state = pc.done(door, Request::FamilyState);
    assert_eq!((state["has_family"].as_bool(), state["signed_in"].as_bool()), (Some(false), Some(false)), "{state}");
    assert_eq!(problem(pc.ask(door, Request::FamilyToday)).kind, ProblemKind::SignedOut);

    // The password: making the family signs this connection in.
    pc.done(door, Request::CreateFamily { password: secret(PASSWORD) });
    pc.run(1);
    pc.done(door, Request::SetPin { pin: secret(PIN) });
    let child = pc.done(door, Request::AddChild { name: "Asha".into(), age_band: AgeBand::TenToTwelve });
    let asha = child["child_id"].as_str().unwrap().to_string();
    let mut rules: RuleSet = serde_json::from_value(pc.done(door, Request::Rules { child: asha.clone() })).unwrap();
    assert_eq!(rules.utc_offset_secs, IST, "the child's home zone is the PC's");
    rules.budgets = WeekBudgets::every_day(DailyBudget::Minutes(30));
    rules.blocked_apps = vec!["game.exe".into()];
    pc.done(door, Request::ReplaceRules { child: asha.clone(), rules });

    // Every account of the PC, with whose it is.
    let accounts = pc.done(door, Request::Accounts);
    let rows = accounts.as_array().unwrap();
    assert_eq!(rows.len(), 3, "{accounts}");
    let me = rows.iter().find(|r| r["account_id"] == GUARDIAN).unwrap();
    assert_eq!((me["this_window"].as_bool(), me["is_administrator"].as_bool(), me["signed_in"].as_bool()), (Some(true), Some(true), Some(true)));
    assert_eq!(me["link"]["kind"], "none");
    pc.done(door, Request::LinkAccount { account: ASHA.into(), link: AccountLink::Child { child_id: asha.clone() } });
    pc.done(door, Request::LinkAccount { account: GUARDIAN.into(), link: AccountLink::NotLimited });
    let settings = pc.done(door, Request::FinishFirstStart);
    assert_eq!(settings["unlinked_accounts"], "locked", "{settings}");

    // The PC follows: Asha is limited to her 30 minutes, an account nobody
    // set up is locked with the PIN offered, the guardian's shows nothing.
    pc.login(2, ASHA, "asha");
    pc.login(3, VISITOR, "visitor");
    pc.run(1);
    pc.greet(2);
    pc.greet(3);
    pc.run(2);
    match pc.last_show(2).unwrap().2 {
        Show::TimeLeft { seconds_left: Some(left), .. } => assert!((29 * 60..=30 * 60).contains(&left), "{left}"),
        other => panic!("{other:?}"),
    }
    match pc.last_show(3).unwrap().2 {
        Show::Locked { screen } => assert!(screen.why == LockWhy::AccountNotLinked && screen.can_use_pin, "{screen:?}"),
        other => panic!("{other:?}"),
    }
    assert!(pc.helper(1).is_none());
    let today = pc.done(door, Request::FamilyToday);
    assert_eq!(today[0]["name"], "Asha");

    // The log says what was done, with no name and no secret.
    for done in ["create_family done", "set_pin done", "add_child done", "replace_rules done", "finish_first_start done"] {
        assert_eq!(pc.logged(done).len(), 1, "{done}: {:?}", pc.os().log);
    }
    for secret in [PASSWORD, PIN, "Asha"] {
        assert!(pc.logged(secret).is_empty(), "{secret} in the log");
    }
}

#[test]
fn the_childs_account_is_refused_at_the_door_in_the_guardians_words() {
    let mut pc = Pc::new(at(16, 0));
    let asha = pc.add_child("Asha", ASHA, 60);
    let door = pc.console(ASHA, 2);
    let refused = ToConsole::Refused {
        message: "LiMo's settings cannot be opened from Asha's account. A guardian opens them from their own.".into(),
    };
    assert_eq!(pc.hello(door), Some(refused.clone()));
    // Nothing it says after is answered, and it is closed a moment later.
    pc.console_say(door, &FromConsole::Request { id: 1, request: Request::SignIn { password: secret(PASSWORD) } });
    pc.console_say(door, &FromConsole::Request { id: 2, request: Request::Lift { child: asha.clone() } });
    pc.run(2);
    assert!(pc.console_closed(door));
    assert_eq!(pc.console_got(door), vec![refused]);
    assert_eq!(pc.service.door_sessions(), 0);
    let lines = pc.logged("refused to a console in session 2");
    assert_eq!(lines.len(), 1, "{:?}", pc.os().log);
    assert!(lines[0].contains("linked to a child") && !lines[0].contains("Asha"), "{}", lines[0]);

    // An account not linked, and the guardian's own marked not limited,
    // come in.
    let visitor = pc.console(VISITOR, 3);
    assert!(matches!(pc.hello(visitor), Some(ToConsole::Hello { .. })));
    pc.hub().link_account(GUARDIAN, "guardian", AccountLink::NotLimited).unwrap();
    let guardian = pc.console(GUARDIAN, 1);
    assert!(matches!(pc.hello(guardian), Some(ToConsole::Hello { .. })));

    // An account linked to a child while a console is open in it is refused
    // at its next request, signed in or not.
    pc.sign_in(visitor);
    pc.hub().link_account(VISITOR, "visitor", AccountLink::Child { child_id: asha }).unwrap();
    assert!(pc.ask(visitor, Request::FamilyToday).is_none());
    assert!(matches!(pc.console_got(visitor).last(), Some(ToConsole::Refused { .. })));
    pc.run(2);
    assert!(pc.console_closed(visitor));
    assert!(!pc.console_closed(guardian));
}

#[test]
fn a_client_whose_account_windows_would_not_say_is_refused() {
    let mut pc = Pc::new(at(16, 0));
    let door = pc.os().connect(None, 4);
    match pc.hello(door) {
        Some(ToConsole::Refused { message }) => assert!(message.contains("could not tell which Windows account"), "{message}"),
        other => panic!("{other:?}"),
    }
    pc.run(2);
    assert!(pc.console_closed(door));
}

#[test]
fn a_wrong_password_says_the_tries_left_then_the_wait_and_the_wait_holds_for_every_console() {
    let (mut pc, _, door) = family_with_a_console();
    for left in [4, 3, 2, 1] {
        pc.run(1);
        let wrong = problem(pc.ask(door, Request::SignIn { password: secret("not the password") }));
        assert_eq!((wrong.kind, wrong.tries_before_wait, wrong.wait_until), (ProblemKind::WrongPassword, Some(left), None));
        let tries = if left == 1 { "1 more try".to_string() } else { format!("{left} more tries") };
        assert_eq!(wrong.message, format!("That password is not right. {tries} before a wait."));
    }
    pc.run(1);
    let fifth = problem(pc.ask(door, Request::SignIn { password: secret("not the password") }));
    let wait = fifth.wait_until.expect("a wait after the fifth");
    assert_eq!(fifth.kind, ProblemKind::WrongPassword);
    assert!(wait > pc.now() + 50 && wait <= pc.now() + 61, "a minute: {wait}");
    // During the wait even the right password is not checked.
    pc.run(1);
    let waiting = problem(pc.ask(door, Request::SignIn { password: secret(PASSWORD) }));
    assert_eq!((waiting.kind, waiting.wait_until), (ProblemKind::MustWait, Some(wait)));
    assert_eq!(pc.service.door_sessions(), 0);
    // The wait is the family file's own: another console waits too.
    let other = pc.console(GUARDIAN, 1);
    pc.hello(other);
    pc.run(1);
    assert_eq!(problem(pc.ask(other, Request::SignIn { password: secret(PASSWORD) })).kind, ProblemKind::MustWait);
    // When it has passed, the right password opens.
    pc.os().wall_ms = (wait + 1) * 1_000;
    pc.run(1);
    pc.done(door, Request::SignIn { password: secret(PASSWORD) });
    assert_eq!(pc.service.door_sessions(), 1);
    // Two in one second from one connection: the second is not tried.
    pc.run(1);
    let (a, b) = (pc.next_ask(), pc.next_ask());
    pc.console_say(door, &FromConsole::Request { id: a, request: Request::SignIn { password: secret("not the password") } });
    pc.console_say(door, &FromConsole::Request { id: b, request: Request::SignIn { password: secret("not the password") } });
    pc.run(1);
    assert_eq!(problem(pc.answer_to(door, a)).kind, ProblemKind::WrongPassword);
    assert_eq!(problem(pc.answer_to(door, b)).kind, ProblemKind::Busy);
    // Whether, never what.
    assert_eq!(pc.logged("a wrong guardian password was typed").len(), 1);
    assert!(pc.logged("not the password").is_empty() && pc.logged(PASSWORD).is_empty());
}

#[test]
fn nothing_is_answered_before_sign_in_but_what_the_first_screen_needs() {
    let (mut pc, asha, door) = family_with_a_console();
    for request in [
        Request::FamilyToday,
        Request::LockNow { child: asha.clone(), until: None },
        Request::GiveExtra { child: asha.clone(), minutes: 30, tap_id: "t1".into() },
        Request::Accounts,
        Request::LinkAccount { account: VISITOR.into(), link: AccountLink::NotLimited },
        Request::FinishFirstStart,
        Request::SetPin { pin: secret("111111") },
        Request::Rules { child: asha.clone() },
        Request::Events { limit: 10, before: None },
        Request::MakeCode { device: "d".into(), minutes: 15 },
        Request::DeleteFamily { password: secret(PASSWORD) },
        Request::MakeOffer { child: asha.clone() },
    ] {
        let name = request.name();
        pc.run(1);
        assert_eq!(problem(pc.ask(door, request)).kind, ProblemKind::SignedOut, "{name}");
    }
    // None of it changed anything.
    let now = pc.now();
    assert_eq!(pc.hub().child_today(&asha, now).unwrap().guardian, limo_core::GuardianMode::Nothing);
    assert_eq!(pc.hub().child_today(&asha, now).unwrap().extra_secs, 0);
    assert_eq!(pc.hub().pc_settings().unwrap().unlinked_accounts, UnlinkedAccounts::Free);
    assert!(pc.hub().has_family().unwrap());
    // What the first screen needs is answered.
    assert!(pc.done(door, Request::About)["version"].is_string());
    assert_eq!(pc.done(door, Request::FamilyState)["has_family"], true);
    // A family already made cannot be made again, and that signs nobody in.
    pc.run(1);
    assert_eq!(problem(pc.ask(door, Request::CreateFamily { password: secret("another password") })).kind, ProblemKind::Refused);
    assert_eq!(pc.service.door_sessions(), 0);
}

#[test]
fn a_session_ends_fifteen_minutes_after_the_last_request_that_is_not_a_repeated_look() {
    let (mut pc, asha, door) = family_with_a_console();
    pc.sign_in(door);
    // Today asked again and again does not keep the session open...
    for _ in 0..13 {
        leap(&mut pc, 60);
        pc.done(door, Request::FamilyToday);
    }
    // ...a request of the guardian's does.
    leap(&mut pc, 60);
    pc.done(door, Request::Rules { child: asha.clone() });
    for _ in 0..14 {
        leap(&mut pc, 60);
        pc.done(door, Request::FamilyToday);
    }
    leap(&mut pc, 61);
    assert_eq!(pc.service.door_sessions(), 0);
    assert_eq!(problem(pc.ask(door, Request::FamilyToday)).kind, ProblemKind::SignedOut);
    assert_eq!(pc.logged(&format!("ended after {} minutes with no request", SESSION_IDLE_MS / 60_000)).len(), 1);
    // The connection is still open: signing in again goes on.
    pc.sign_in(door);
    pc.done(door, Request::Rules { child: asha });
}

#[test]
fn each_connection_has_its_own_session_and_sign_out_or_closing_ends_it() {
    let (mut pc, _, first) = family_with_a_console();
    let second = pc.console(GUARDIAN, 1);
    pc.hello(second);
    pc.sign_in(first);
    pc.done(first, Request::FamilyToday);
    assert_eq!(problem(pc.ask(second, Request::FamilyToday)).kind, ProblemKind::SignedOut);
    pc.done(first, Request::SignOut);
    assert_eq!(problem(pc.ask(first, Request::FamilyToday)).kind, ProblemKind::SignedOut);
    pc.sign_in(second);
    assert_eq!(pc.service.door_sessions(), 1);
    pc.os().disconnect(second);
    pc.step();
    assert_eq!(pc.service.door_sessions(), 0);
    let third = pc.console(GUARDIAN, 1);
    pc.hello(third);
    assert_eq!(problem(pc.ask(third, Request::FamilyToday)).kind, ProblemKind::SignedOut);
}

/// What a console may send that closes its connection, and whether it says
/// hello first.
fn bad_inputs() -> Vec<(&'static str, bool, Vec<u8>)> {
    let v = CONSOLE_PROTOCOL_VERSION;
    let mut huge = v.to_le_bytes().to_vec();
    huge.extend_from_slice(&(10 * 1024 * 1024u32).to_le_bytes());
    let mut one_too_long = v.to_le_bytes().to_vec();
    one_too_long.extend_from_slice(&((MAX_CONSOLE_MESSAGE_BYTES as u32) + 1).to_le_bytes());
    let mut flood = v.to_le_bytes().to_vec();
    flood.extend_from_slice(&((MAX_CONSOLE_MESSAGE_BYTES as u32) - 1).to_le_bytes());
    flood.extend(std::iter::repeat_n(b'x', 3 * MAX_CONSOLE_MESSAGE_BYTES));
    let sign_in = format!(r#"{{"say":"request","id":1,"request":{{"name":"sign_in","args":{{"password":"{PASSWORD}"}}}}}}"#);
    let mut garbage_then_good = frame_of(v, "not json at all");
    garbage_then_good.extend(frame_of(v, &sign_in));
    vec![
        ("garbage", true, garbage_then_good),
        ("another version", true, frame_of(7, r#"{"say":"hello","protocol":7}"#)),
        ("a frame too long", true, huge),
        ("one byte longer than a message may be", true, one_too_long),
        ("a flood", true, flood),
        ("a message nobody knows", true, frame_of(v, r#"{"say":"take_over_the_pc"}"#)),
        (
            "a field it should not have",
            true,
            frame_of(v, r#"{"say":"request","id":1,"request":{"name":"family_today"},"window_title":"secret diary"}"#),
        ),
        ("a request before hello", false, frame_of(v, &sign_in)),
        ("hello twice", true, frame_of(v, r#"{"say":"hello","protocol":1}"#)),
        ("hello in another version", false, frame_of(v, r#"{"say":"hello","protocol":9}"#)),
        ("an empty frame", true, frame_of(v, "")),
    ]
}

#[test]
fn anything_malformed_oversized_or_out_of_turn_closes_the_connection_and_reaches_nothing() {
    for (what, greet, bytes) in bad_inputs() {
        let mut pc = Pc::new(at(16, 0));
        pc.add_child("Asha", ASHA, 60);
        pc.os().accounts = vec![account(GUARDIAN, "guardian", true)];
        let door = pc.console(GUARDIAN, 1);
        if greet {
            pc.hello(door);
        }
        let before = pc.os().log.len();
        pc.console_raw(door, &bytes);
        pc.step();
        assert!(pc.console_closed(door), "{what}: not closed");
        let closed: Vec<String> = pc.os().log[before..].iter().filter(|l| l.contains("was closed, because")).cloned().collect();
        assert_eq!(closed.len(), 1, "{what}: {:?}", pc.os().log);
        for secret in [PASSWORD, "horse", "secret diary", "window_title", "take_over", "not json"] {
            assert!(!closed[0].contains(secret), "{what}: {}", closed[0]);
        }
        // Nothing reached the family's file: nobody signed in, no answer.
        assert_eq!(pc.service.door_sessions(), 0, "{what}");
        assert!(!pc.console_got(door).iter().any(|m| matches!(m, ToConsole::Answer { .. })), "{what}");
        // The service goes on counting.
        pc.login(2, ASHA, "asha");
        pc.run(2);
        assert!(pc.facts_of(2).is_some(), "{what}");
    }
}

#[test]
fn a_request_this_limo_cannot_read_is_answered_as_unknown_and_the_connection_stays() {
    let (mut pc, _, door) = family_with_a_console();
    let v = CONSOLE_PROTOCOL_VERSION;
    for json in [
        r#"{"say":"request","id":901,"request":{"name":"take_over_the_pc"}}"#,
        r#"{"say":"request","id":902,"request":{"name":"lock_now","args":{"child":"c","until":"soon"}}}"#,
        r#"{"say":"request","id":903,"request":{"name":"sign_in","args":{"password":482915}}}"#,
        r#"{"say":"request","id":904,"request":{"name":"lift","args":{"child":"c","window_title":"x"}}}"#,
    ] {
        pc.console_raw(door, &frame_of(v, json));
    }
    pc.step();
    for id in [901, 902, 903, 904] {
        let unknown = problem(pc.answer_to(door, id));
        assert_eq!(unknown.kind, ProblemKind::UnknownRequest, "{id}");
        assert!(!unknown.message.contains("482915") && !unknown.message.contains("soon"), "{}", unknown.message);
    }
    assert!(!pc.console_closed(door));
    assert!(pc.done(door, Request::About)["version"].is_string());
}

#[test]
fn a_client_that_says_nothing_is_closed_after_ten_seconds() {
    let mut pc = Pc::new(at(16, 0));
    let door = pc.console(GUARDIAN, 1);
    pc.run(9);
    assert!(!pc.console_closed(door));
    pc.run(2);
    assert!(pc.console_closed(door));
    assert_eq!(pc.logged("it did not say hello within 10 seconds").len(), 1);
}

#[test]
fn a_flood_of_requests_closes_the_connection_before_any_reaches_the_file() {
    let (mut pc, asha, door) = family_with_a_console();
    pc.sign_in(door);
    let now = pc.now();
    let extra_before = pc.hub().child_today(&asha, now).unwrap().extra_secs;
    pc.run(1);
    for n in 0..40 {
        let request = Request::GiveExtra { child: asha.clone(), minutes: 1, tap_id: format!("tap{n}") };
        pc.console_say(door, &FromConsole::Request { id: 5_000 + n, request });
    }
    pc.step();
    assert!(pc.console_closed(door));
    assert_eq!(pc.logged("more than 20 messages in one second").len(), 1);
    let now = pc.now();
    assert_eq!(pc.hub().child_today(&asha, now).unwrap().extra_secs, extra_before);
}

#[test]
fn two_clients_at_once_are_each_answered_and_one_too_many_is_refused_with_words() {
    let (mut pc, asha, first) = family_with_a_console();
    let second = pc.console(GUARDIAN, 1);
    pc.hello(second);
    pc.sign_in(first);
    pc.sign_in(second);
    let (a, b) = (pc.next_ask(), pc.next_ask());
    pc.console_say(first, &FromConsole::Request { id: a, request: Request::Rules { child: asha.clone() } });
    pc.console_say(second, &FromConsole::Request { id: b, request: Request::FamilyToday });
    pc.step();
    assert!(matches!(pc.answer_to(first, a), Some(Reply::Done { value }) if value["budgets"].is_object()));
    assert!(matches!(pc.answer_to(second, b), Some(Reply::Done { value }) if value.is_array()));
    assert!(pc.answer_to(first, b).is_none() && pc.answer_to(second, a).is_none());
    // As many as the door serves, then one more.
    let more: Vec<_> = (2..MAX_CONNECTIONS).map(|_| pc.console(GUARDIAN, 1)).collect();
    pc.step();
    let extra = pc.console(GUARDIAN, 1);
    pc.step();
    match pc.console_got(extra).last() {
        Some(ToConsole::Refused { message }) => assert!(message.contains("already open in 4 windows"), "{message}"),
        other => panic!("{other:?}"),
    }
    pc.run(2);
    assert!(pc.console_closed(extra) && more.iter().all(|id| !pc.console_closed(*id)));
}

#[test]
fn requests_wait_their_turn_with_one_password_between_two_steps() {
    let (mut pc, _, first) = family_with_a_console();
    let doors = [first, pc.console(GUARDIAN, 1), pc.console(GUARDIAN, 1)];
    for door in &doors[1..] {
        pc.hello(*door);
    }
    pc.run(1);
    let asks: Vec<u32> = doors
        .iter()
        .map(|door| {
            let ask = pc.next_ask();
            pc.console_say(*door, &FromConsole::Request { id: ask, request: Request::SignIn { password: secret(PASSWORD) } });
            ask
        })
        .collect();
    pc.step();
    let answered = |pc: &mut Pc| doors.iter().zip(&asks).filter(|(d, a)| pc.answer_to(**d, **a).is_some()).count();
    assert_eq!(answered(&mut pc), 1, "one password between two steps");
    pc.step();
    assert_eq!(answered(&mut pc), 2);
    pc.step();
    assert_eq!(answered(&mut pc), 3);
    assert_eq!(pc.service.door_sessions(), 3);
}

#[test]
fn a_console_that_stops_reading_is_closed() {
    let (mut pc, _, door) = family_with_a_console();
    pc.os().door_not_reading.insert(door);
    assert!(pc.ask(door, Request::About).is_none());
    assert!(pc.console_closed(door));
    assert_eq!(pc.logged("it stopped reading what it is sent").len(), 1);
}

#[test]
fn a_change_from_the_console_reaches_the_pc_at_the_next_step() {
    let (mut pc, asha, door) = family_with_a_console();
    pc.login(2, ASHA, "asha");
    pc.run(1);
    pc.greet(2);
    pc.run(2);
    assert!(matches!(pc.last_show(2).unwrap().2, Show::TimeLeft { .. }));
    pc.sign_in(door);
    // Half way through a second, so the engine's own once a second call is
    // not what brings it.
    pc.run_ms(500);
    pc.done(door, Request::LockNow { child: asha, until: None });
    pc.step();
    match pc.last_show(2).unwrap().2 {
        Show::Locked { screen } => assert_eq!(screen.why, LockWhy::GuardianLock),
        other => panic!("{other:?}"),
    }
}

#[test]
fn accounts_lists_every_account_with_its_link_and_who_is_signed_in() {
    let (mut pc, asha, door) = family_with_a_console();
    pc.os().accounts[1].full_name = Some("Asha Rao".into());
    pc.hub().link_account(ASHA, "asha", AccountLink::Child { child_id: asha.clone() }).unwrap();
    pc.hub().link_account("S-1-5-21-1077", "gone", AccountLink::NotLimited).unwrap();
    pc.sign_in(door);
    let rows = pc.done(door, Request::Accounts);
    let rows = rows.as_array().unwrap();
    let find = |id: &str| rows.iter().find(|r| r["account_id"] == id).unwrap().clone();
    let asha_row = find(ASHA);
    assert_eq!(asha_row["display_name"], "Asha Rao");
    assert_eq!((asha_row["link"]["kind"].as_str(), asha_row["link"]["child_name"].as_str()), (Some("child"), Some("Asha")));
    assert_eq!(asha_row["signed_in"], false);
    assert_eq!(find(GUARDIAN)["signed_in"], true);
    assert_eq!(find(VISITOR)["link"]["kind"], "none");
    let gone = find("S-1-5-21-1077");
    assert_eq!((gone["on_this_pc"].as_bool(), gone["link"]["kind"].as_str()), (Some(false), Some("not_limited")));
    // An account Windows does not have is not linked; a failure to list is
    // said plainly.
    assert_eq!(
        problem(pc.ask(door, Request::LinkAccount { account: MIRA.into(), link: AccountLink::NotLimited })).kind,
        ProblemKind::NotFound
    );
    pc.os().accounts_fail = Some("NetUserEnum failed with error 5".into());
    assert_eq!(problem(pc.ask(door, Request::Accounts)).kind, ProblemKind::Failed);
    assert_eq!(pc.logged("The accounts of this PC could not be listed").len(), 1);
}

#[test]
fn the_consoles_own_account_is_never_linked_to_a_child_from_it() {
    let (mut pc, asha, door) = family_with_a_console();
    pc.sign_in(door);
    let refused = problem(pc.ask(door, Request::LinkAccount { account: GUARDIAN.into(), link: AccountLink::Child { child_id: asha } }));
    assert_eq!(refused.kind, ProblemKind::Refused);
    assert!(refused.message.contains("Link it from another account"), "{}", refused.message);
    assert_eq!(pc.hub().account_standing(GUARDIAN).unwrap(), AccountStanding::NotLinked);
    pc.done(door, Request::LinkAccount { account: GUARDIAN.into(), link: AccountLink::NotLimited });
}

#[test]
fn removing_a_child_or_deleting_the_family_takes_the_password_again() {
    let (mut pc, asha, door) = family_with_a_console();
    pc.sign_in(door);
    pc.run(1);
    let wrong = problem(pc.ask(door, Request::RemoveChild { child: asha.clone(), password: secret("not the password") }));
    assert_eq!(wrong.kind, ProblemKind::WrongPassword);
    assert_eq!(pc.hub().children().unwrap().len(), 1);
    pc.run(1);
    pc.done(door, Request::RemoveChild { child: asha, password: secret(PASSWORD) });
    assert!(pc.hub().children().unwrap().is_empty());
    pc.run(1);
    let until = pc.done(door, Request::AllowRemoval { password: secret(PASSWORD) })["until"].as_i64().unwrap();
    assert_eq!(until, pc.now() + 600);
    assert_eq!(pc.service.removal_allowed_until(), Some(until));
    pc.run(1);
    pc.done(door, Request::DeleteFamily { password: secret(PASSWORD) });
    assert!(!pc.hub().has_family().unwrap());
    assert_eq!(pc.service.door_sessions(), 0);
    assert_eq!(pc.service.removal_allowed_until(), None);
    assert_eq!(pc.done(door, Request::FamilyState)["has_family"], false);
}

#[test]
fn rules_that_do_not_fit_are_answered_with_their_own_problems() {
    let (mut pc, asha, door) = family_with_a_console();
    pc.sign_in(door);
    let mut rules: RuleSet = serde_json::from_value(pc.done(door, Request::Rules { child: asha.clone() })).unwrap();
    rules.blocked_hours = vec![BlockedWindow { days: Vec::new(), start_minute: 2_000, end_minute: 420 }];
    let refused = problem(pc.ask(door, Request::ReplaceRules { child: asha.clone(), rules }));
    assert_eq!(refused.kind, ProblemKind::Rules);
    assert!(refused.rule_problems.iter().any(|p| p.field.starts_with("blocked_hours")), "{refused:?}");
    assert_eq!(problem(pc.ask(door, Request::Rules { child: "no such child".into() })).kind, ProblemKind::NotFound);
}

#[test]
fn adding_a_phone_answers_not_yet() {
    let (mut pc, asha, door) = family_with_a_console();
    pc.sign_in(door);
    for request in [Request::MakeOffer { child: asha }, Request::DeviceLink { device: "d".into() }] {
        let not_yet = problem(pc.ask(door, request));
        assert_eq!(not_yet.kind, ProblemKind::NotYet);
        assert_eq!(not_yet.message, "Adding a phone is not built into LiMo for Windows yet.");
    }
}

#[test]
fn a_one_time_code_made_at_the_door_works_once_and_is_never_logged() {
    let (mut pc, asha, door) = family_with_a_console();
    pc.login(2, ASHA, "asha");
    pc.run(2);
    pc.sign_in(door);
    let devices = pc.done(door, Request::Devices { child: asha.clone() });
    let device = devices[0]["device_id"].as_str().unwrap().to_string();
    let code = pc.done(door, Request::MakeCode { device, minutes: 15 });
    let shown = code["shown"].as_str().unwrap().to_string();
    assert_eq!(shown.len(), 9, "two groups of four: {shown}");
    assert!(pc.logged(&shown).is_empty() && pc.logged(&shown[..4]).is_empty());
    assert_eq!(pc.logged("make_code done").len(), 1);
}

#[test]
fn a_pin_or_a_password_is_never_logged_kept_or_shown() {
    let (mut pc, _, door) = family_with_a_console();
    pc.sign_in(door);
    pc.run(1);
    pc.done(door, Request::SetPin { pin: secret("771144") });
    pc.run(1);
    pc.ask(door, Request::ChangePassword { old: secret("not the password"), new: secret("battery staple horse") });
    pc.run(1);
    pc.done(door, Request::ChangePassword { old: secret(PASSWORD), new: secret("battery staple horse") });
    for secret in [PASSWORD, "771144", "battery staple", "not the password"] {
        assert!(pc.logged(secret).is_empty(), "{secret} logged");
        assert!(!format!("{:?}", pc.service).contains(secret));
        for message in pc.console_got(door) {
            assert!(!format!("{message:?}").contains(secret));
        }
    }
}

#[test]
fn the_lock_of_an_account_not_set_up_takes_the_guardian_pin() {
    let mut pc = Pc::new(at(16, 0));
    pc.add_child("Asha", ASHA, 60);
    pc.hub().set_unlinked_accounts(UnlinkedAccounts::Locked).unwrap();
    pc.login(5, VISITOR, "visitor");
    pc.run(1);
    pc.greet(5);
    pc.run(2);
    // With no PIN, the lock offers none.
    match pc.last_show(5).unwrap().2 {
        Show::Locked { screen } => assert!(screen.why == LockWhy::AccountNotLinked && !screen.can_use_pin),
        other => panic!("{other:?}"),
    }
    let now = pc.now();
    pc.hub().set_pin(PIN, now).unwrap();
    // Asha is signed in too, not on the screen: her device on this PC counts
    // nothing of what is typed on the other account's lock.
    pc.login(2, ASHA, "asha");
    pc.set_state(2, limo_agent::SessionState::Disconnected);
    pc.run(2);
    match pc.last_show(5).unwrap().2 {
        Show::Locked { screen } => assert!(screen.can_use_pin, "{screen:?}"),
        other => panic!("{other:?}"),
    }
    pc.say(5, FromHelper::TryPin { ask: 1, pin: Secret::new("111111".into()), action: PinAction::AddMinutes { minutes: 30 } });
    pc.run(2);
    pc.say(5, FromHelper::TryPin { ask: 2, pin: Secret::new(PIN.into()), action: PinAction::ConfirmRelease });
    pc.run(2);
    let now = pc.now();
    pc.say(5, FromHelper::TryPin { ask: 3, pin: Secret::new(PIN.into()), action: PinAction::AddMinutes { minutes: 30 } });
    pc.step();
    let got = way_out_answers(&mut pc, 5);
    assert!(matches!(got[0], (1, Answer::Tried { outcome: TryOutcome::Wrong { wrong_guesses: 1, .. } })), "{got:?}");
    assert!(matches!(&got[1], (2, Answer::Refused { message }) if message.contains("nothing to release")), "{got:?}");
    match got[2] {
        (3, Answer::Tried { outcome: TryOutcome::Unlocked { until } }) => assert!((now + 1_790..=now + 1_810).contains(&until), "{until}"),
        ref other => panic!("{other:?}"),
    }
    // Open: nothing is shown, and the helper is told to quit.
    pc.run(5);
    assert_eq!(pc.helpers_running(5), 0);
    assert_eq!(pc.logged("on the lock of an account not set up in LiMo, was tried from the lock screen: right").len(), 1);
    // The family's file is as it was: the account is still not set up, and
    // Asha's device counts no wrong guess.
    assert_eq!(pc.hub().account_standing(VISITOR).unwrap(), AccountStanding::NotLinked);
    let now = pc.now();
    let today = pc.hub().family_today(now).unwrap();
    assert!(today[0].devices.iter().all(|d| d.wrong_guesses == 0), "{today:?}");
    // When the half hour is over, the lock is back.
    leap(&mut pc, 30 * 60);
    pc.run(1);
    pc.greet(5);
    pc.run(2);
    assert!(matches!(pc.last_show(5).unwrap().2, Show::Locked { .. }));
    // Five wrong tries set a wait, which the lock says, and the family's
    // file keeps, so a restart of the service changes nothing.
    for ask in 10..15 {
        pc.say(5, FromHelper::TryPin { ask, pin: Secret::new("111111".into()), action: PinAction::AddMinutes { minutes: 15 } });
        pc.run(2);
    }
    let wait = match pc.last_show(5).unwrap().2 {
        Show::Locked { screen } => screen.try_wait_until.filter(|w| *w > pc.now()).expect("a wait on the lock"),
        other => panic!("{other:?}"),
    };
    let now = pc.now();
    assert_eq!(pc.hub().check_pin(PIN, now).unwrap(), limo_hub::PinOutcome::MustWait { until: wait });
    assert!(pc.logged(PIN).is_empty());
}

/// The service's step with the door idle, then busy: two consoles signed in,
/// each asking for Today and a child's rules at every step, and one of them
/// signing in again with the password (hashed slowly on purpose) every
/// second, while Asha's session counts. Measured with the real clock and
/// printed (`cargo test -- --nocapture` shows it). The steps do not wait for
/// the door: the door is answered between them, at most one password a turn.
#[test]
fn the_step_time_with_the_door_busy() {
    let (mut pc, asha, first) = family_with_a_console();
    let second = pc.console(GUARDIAN, 1);
    pc.hello(second);
    pc.sign_in(first);
    pc.sign_in(second);
    pc.login(2, ASHA, "asha");
    pc.run(1);
    pc.greet(2);
    pc.run(2);

    let measure = |pc: &mut Pc, busy: bool| -> (Vec<Duration>, Vec<Duration>, u64) {
        let counted_before = pc.counted(&asha);
        let (mut steps, mut turns) = (Vec::new(), Vec::new());
        for n in 0..300u32 {
            pc.report(2, 0, None, false);
            if busy {
                // One request from each console at every step: ten a second
                // each, as fast as the door lets one console ask.
                for door in [first, second] {
                    let ask = pc.next_ask();
                    let request = if n % 2 == 0 { Request::FamilyToday } else { Request::Rules { child: asha.clone() } };
                    pc.console_say(door, &FromConsole::Request { id: ask, request });
                }
                if n % 10 == 0 {
                    let ask = pc.next_ask();
                    pc.console_say(first, &FromConsole::Request { id: ask, request: Request::SignIn { password: secret(PASSWORD) } });
                }
            }
            let os = pc.os();
            os.steady_ms += 100;
            os.wall_ms += 100;
            let started = Instant::now();
            pc.service.step();
            steps.push(started.elapsed());
            let started = Instant::now();
            pc.service.door();
            turns.push(started.elapsed());
        }
        (steps, turns, pc.counted(&asha) - counted_before)
    };
    let (idle_steps, _, idle_counted) = measure(&mut pc, false);
    let (busy_steps, busy_turns, busy_counted) = measure(&mut pc, true);
    let ms = |d: &Duration| d.as_secs_f64() * 1_000.0;
    let mean = |v: &[Duration]| v.iter().map(ms).sum::<f64>() / v.len() as f64;
    let most = |v: &[Duration]| v.iter().map(ms).fold(0.0, f64::max);
    eprintln!(
        "step with the door idle: mean {:.2} ms, longest {:.2} ms; with the door busy: mean {:.2} ms, longest {:.2} ms; \
         the door's turn between two steps: mean {:.2} ms, longest {:.2} ms ({:?}); counted in 30 s: idle {idle_counted} s, busy {busy_counted} s",
        mean(&idle_steps),
        most(&idle_steps),
        mean(&busy_steps),
        most(&busy_steps),
        mean(&busy_turns),
        most(&busy_turns),
        pc.service.door_times(),
    );
    // Counting is the same with the door busy: the engine goes by the
    // steady clock, and the door never runs inside a step.
    assert_eq!(idle_counted, busy_counted);
    assert!(busy_counted >= 29, "{busy_counted}");
    // Every request was answered, and nobody was closed for it.
    assert!(!pc.console_closed(first) && !pc.console_closed(second));
    assert!(mean(&busy_steps) < 50.0, "a step is slow: {:.2} ms", mean(&busy_steps));
}
