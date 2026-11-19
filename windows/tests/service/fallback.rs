//! The fallback: a session ordered locked and in use whose lock no helper
//! confirms within five seconds is disconnected; nothing else ever is. And
//! the test cap.

use limo_agent::{SessionState, Show};
use limo_hub::UnlinkedAccounts;

use crate::common::*;

const ASHA: &str = "S-1-5-21-1001";
const VISITOR: &str = "S-1-5-21-1009";

/// A PC with Asha locked by a guardian in session 1, in use, her helper
/// greeted.
fn locked_pc() -> Pc {
    let mut pc = Pc::new(at(16, 0));
    let asha = pc.add_child("Asha", ASHA, 60);
    let now = pc.now();
    pc.hub().lock_now(&asha, None, now).unwrap();
    pc.login(1, ASHA, "asha");
    pc.step();
    pc.greet(1);
    pc
}

#[test]
fn a_lock_no_helper_confirms_within_five_seconds_disconnects_the_session() {
    let mut pc = locked_pc();
    assert!(matches!(pc.last_show(1), Some((_, _, Show::Locked { .. }))));
    pc.run(4);
    assert!(pc.os().disconnected.is_empty());
    pc.run(2);
    assert_eq!(pc.os().disconnected, vec![1]);
    assert_eq!(pc.logged("so LiMo disconnected the session").len(), 1);
    // Its programs stay open: nothing was ended.
    assert!(pc.os().ended.is_empty());
    // Disconnected, it is not disconnected again...
    pc.run(30);
    assert_eq!(pc.os().disconnected, vec![1]);
    // ...until the child is back on it, and still no lock is confirmed.
    pc.set_state(1, SessionState::InUse);
    pc.run(4);
    assert_eq!(pc.os().disconnected, vec![1]);
    pc.run(3);
    assert_eq!(pc.os().disconnected, vec![1, 1]);
}

#[test]
fn a_lock_confirmed_in_time_is_left_alone() {
    let mut pc = locked_pc();
    pc.run(1);
    pc.confirm_lock(1);
    pc.run(60);
    assert!(pc.os().disconnected.is_empty());
    assert!(pc.service.session_view(1).unwrap().lock_confirmed);
}

#[test]
fn a_helper_that_dies_after_confirming_must_confirm_again() {
    let mut pc = locked_pc();
    pc.run(1);
    pc.confirm_lock(1);
    pc.run(10);
    pc.helper(1).unwrap().running = false;
    pc.step();
    assert_eq!(pc.helpers_running(1), 1);
    assert!(!pc.service.session_view(1).unwrap().lock_confirmed);
    pc.greet(1);
    pc.run(4);
    assert!(pc.os().disconnected.is_empty());
    pc.run(2);
    assert_eq!(pc.os().disconnected, vec![1]);
}

#[test]
fn only_a_lock_in_a_session_on_the_screen_is_ever_disconnected() {
    // Time left, from a helper that never says a word.
    let mut pc = Pc::new(at(16, 0));
    pc.add_child("Asha", ASHA, 60);
    pc.login(1, ASHA, "asha");
    pc.run(60);
    assert!(pc.os().disconnected.is_empty());

    // A lock in a session locked by Windows, or not on the screen.
    let mut pc = locked_pc();
    pc.set_state(1, SessionState::Locked);
    pc.run(30);
    pc.set_state(1, SessionState::Disconnected);
    pc.run(30);
    assert!(pc.os().disconnected.is_empty());

    // An account that is not linked, free.
    let mut pc = Pc::new(at(16, 0));
    pc.hub().set_unlinked_accounts(UnlinkedAccounts::Free).unwrap();
    pc.login(5, VISITOR, "visitor");
    pc.run(30);
    assert!(pc.os().disconnected.is_empty());
    assert_eq!(pc.helpers_started(5), 0);
}

#[test]
fn the_test_cap_lifts_a_lock_after_120_seconds_and_says_so_loudly() {
    let mut pc = locked_pc();
    pc.os().test_cap = true;
    pc.run(1);
    pc.confirm_lock(1);
    pc.run(117);
    assert!(matches!(pc.service.session_view(1).unwrap().show, Show::Locked { .. }));
    pc.run(3);
    let view = pc.service.session_view(1).unwrap();
    assert!(view.lifted && view.show == Show::Nothing, "{view:?}");
    assert!(matches!(pc.last_show(1), Some((_, _, Show::Nothing))));
    let loud = pc.logged("TEST CAP:");
    assert_eq!(loud.len(), 1);
    assert!(loud[0].contains("the test cap lifted it") && loud[0].contains("A real install has no test cap."));
    pc.run(60);
    assert!(pc.os().disconnected.is_empty());
    assert_eq!(pc.logged("TEST CAP:").len(), 1);
    // The cap switched off: the lock is back.
    pc.os().test_cap = false;
    pc.run(1);
    assert!(matches!(pc.service.session_view(1).unwrap().show, Show::Locked { .. }));
    assert_eq!(pc.logged("The test cap was switched off").len(), 1);
}

#[test]
fn without_the_test_cap_a_lock_stays() {
    let mut pc = locked_pc();
    pc.run(1);
    pc.confirm_lock(1);
    pc.run(300);
    assert!(matches!(pc.service.session_view(1).unwrap().show, Show::Locked { .. }));
    assert!(pc.logged("TEST CAP").is_empty());
}
