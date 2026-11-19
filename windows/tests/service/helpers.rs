//! The helpers: exactly one in each session that needs one, back at once
//! when it dies, told to quit when it is not needed, and handed what to
//! show whenever it changes and every five seconds.

use limo_agent::{SessionState, Show};
use limo_hub::AccountLink;
use limo_windows::protocol::ToHelper;

use crate::common::*;

const ASHA: &str = "S-1-5-21-1001";
const MUM: &str = "S-1-5-21-1000";

#[test]
fn exactly_one_helper_runs_in_each_session_that_needs_one() {
    let mut pc = Pc::new(at(16, 0));
    pc.add_child("Asha", ASHA, 60);
    pc.hub().link_account(MUM, "Mum", AccountLink::NotLimited).unwrap();
    pc.login(1, ASHA, "asha");
    pc.login(2, MUM, "mum");
    pc.set_state(2, SessionState::Disconnected);
    pc.run(1);
    pc.greet(1);
    pc.run(30);
    assert_eq!((pc.helpers_started(1), pc.helpers_running(1)), (1, 1));
    // A guardian's session needs none.
    assert_eq!(pc.helpers_started(2), 0);
    assert_eq!(pc.logged("Started the helper in session 1").len(), 1);
    assert!(pc.logged("Started the helper in session 1")[0].contains("uiaccess=1 integrity=set dacl=admin-owner channel=inherited"));
}

#[test]
fn a_helper_that_dies_is_back_within_one_step_with_what_to_show() {
    let mut pc = Pc::new(at(16, 0));
    pc.add_child("Asha", ASHA, 60);
    pc.login(1, ASHA, "asha");
    pc.run(1);
    pc.greet(1);
    pc.run(15);
    let first = pc.helper(1).unwrap().pid;
    pc.helper(1).unwrap().running = false;
    pc.step();
    assert_eq!((pc.helpers_started(1), pc.helpers_running(1)), (2, 1));
    assert_ne!(pc.helper(1).unwrap().pid, first);
    // It has what to show at once, before it has said a word.
    assert!(matches!(pc.last_show(1), Some((_, Some(_), Show::TimeLeft { .. }))), "{:?}", pc.last_show(1));
    let started = pc.logged("Started the helper in session 1");
    assert!(started.last().unwrap().contains("gap_ms=0"), "{started:?}");
    assert_eq!(pc.logged(&format!("(pid {first}) ended")).len(), 1);
}

#[test]
fn a_helper_that_ends_at_once_is_started_again_after_a_second() {
    let mut pc = Pc::new(at(16, 0));
    pc.add_child("Asha", ASHA, 60);
    pc.login(1, ASHA, "asha");
    pc.step();
    assert_eq!(pc.helpers_started(1), 1);
    pc.helper(1).unwrap().running = false;
    pc.step();
    pc.run_ms(800);
    assert_eq!(pc.helpers_started(1), 1);
    pc.run_ms(300);
    assert_eq!(pc.helpers_started(1), 2);
}

#[test]
fn a_launch_that_fails_is_tried_every_second_and_logged_once() {
    let mut pc = Pc::new(at(16, 0));
    pc.add_child("Asha", ASHA, 60);
    pc.login(1, ASHA, "asha");
    pc.os().launch_fails = Some("CreateProcessAsUserW failed: Win32 error 87: The parameter is incorrect.".into());
    pc.run(10);
    let tries = pc.os().launches;
    assert!((9..=11).contains(&tries), "{tries}");
    assert_eq!(pc.logged("could not be started in session 1").len(), 1);
    pc.os().launch_fails = None;
    pc.run(2);
    assert_eq!(pc.helpers_running(1), 1);
}

#[test]
fn a_helper_that_never_says_hello_is_replaced_after_ten_seconds() {
    let mut pc = Pc::new(at(16, 0));
    pc.add_child("Asha", ASHA, 60);
    pc.login(1, ASHA, "asha");
    pc.run(9);
    assert_eq!(pc.helpers_started(1), 1);
    pc.run(2);
    assert_eq!((pc.helpers_started(1), pc.helpers_running(1)), (2, 1));
    assert_eq!(pc.logged("did not say hello within 10 seconds").len(), 1);
}

#[test]
fn a_helper_no_longer_needed_is_told_to_quit_and_ended_if_it_does_not() {
    let mut pc = Pc::new(at(16, 0));
    pc.add_child("Asha", ASHA, 60);
    pc.login(1, ASHA, "asha");
    pc.run(1);
    pc.greet(1);
    pc.run(2);
    // The guardian marks the account as not limited.
    pc.hub().link_account(ASHA, "asha", AccountLink::NotLimited).unwrap();
    pc.run(1);
    let helper = pc.os().helpers.last().unwrap();
    assert_eq!(helper.got.last(), Some(&ToHelper::Quit));
    assert!(helper.running);
    pc.run(3);
    assert_eq!(pc.helpers_running(1), 0);
    assert_eq!(pc.logged("did not quit when told").len(), 1);
    pc.run(10);
    assert_eq!(pc.helpers_started(1), 1);
}

#[test]
fn what_to_show_is_handed_over_at_once_when_it_changes_and_again_every_five_seconds() {
    let mut pc = Pc::new(at(16, 0));
    let asha = pc.add_child("Asha", ASHA, 60);
    pc.login(1, ASHA, "asha");
    pc.run(1);
    pc.greet(1);
    let now = pc.now();
    pc.hub().lock_now(&asha, None, now).unwrap();
    pc.run(2);
    let shows = |pc: &mut Pc| -> Vec<(u64, Show)> {
        pc.helper(1)
            .unwrap()
            .got
            .iter()
            .filter_map(|m| match m {
                ToHelper::Show { seq, show, .. } => Some((*seq, show.clone())),
                _ => None,
            })
            .collect()
    };
    let (seq, show) = shows(&mut pc).last().cloned().unwrap();
    assert!(matches!(show, Show::Locked { .. }));
    let before = shows(&mut pc).len();
    pc.run(20);
    let repeats: Vec<(u64, Show)> = shows(&mut pc).split_off(before);
    assert_eq!(repeats.len(), 4, "{repeats:?}");
    assert!(repeats.iter().all(|(s, sh)| *s == seq && *sh == show));
    // Lifted: handed over within the second, with a new number.
    let now = pc.now();
    pc.hub().lift(&asha, now).unwrap();
    pc.run(1);
    let (new_seq, new_show) = shows(&mut pc).last().cloned().unwrap();
    assert!(new_seq > seq && matches!(new_show, Show::TimeLeft { .. }), "{new_show:?}");
}

#[test]
fn the_childs_name_goes_with_what_to_show() {
    let mut pc = Pc::new(at(16, 0));
    pc.add_child("Asha", ASHA, 60);
    pc.login(1, ASHA, "asha");
    pc.run(1);
    assert_eq!(pc.last_show(1).unwrap().1.as_deref(), Some("Asha"));
}

#[test]
fn a_session_that_ends_takes_its_helper_with_it() {
    let mut pc = Pc::new(at(16, 0));
    pc.add_child("Asha", ASHA, 60);
    pc.login(1, ASHA, "asha");
    pc.run(1);
    pc.greet(1);
    pc.run(2);
    pc.os().sessions.clear();
    pc.run(1);
    assert_eq!(pc.helpers_running(1), 0);
    assert_eq!(pc.logged("Session 1 has ended").len(), 1);
    assert!(pc.service.session_view(1).is_none());
}

#[test]
fn a_clean_stop_keeps_what_was_counted_and_tells_every_helper_to_quit() {
    let mut pc = Pc::new(at(16, 0));
    let asha = pc.add_child("Asha", ASHA, 60);
    pc.login(1, ASHA, "asha");
    pc.run(1);
    pc.greet(1);
    pc.run(30);
    let counted = pc.counted(&asha);
    assert!(counted >= 28, "{counted}");
    pc.service.stop();
    assert_eq!(pc.helper(1).unwrap().got.last(), Some(&ToHelper::Quit));
    pc.service.finish();
    assert_eq!(pc.helpers_running(1), 0);
    assert!(pc.logged("At the stop").is_empty());
    // What the engine counted is in the family's file: a new engine on it,
    // as after a restart, has it all.
    let mut facts = pc.service.last_facts().unwrap().clone();
    facts.steady_ms += 1_000;
    facts.wall_secs += 1;
    let mut engine = limo_agent::Engine::new("Family PC");
    engine.tick(pc.hub(), &facts);
    assert!(engine.child_on_pc(&asha).unwrap().counted_secs >= counted);
}
