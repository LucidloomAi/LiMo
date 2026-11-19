//! The lock screen's machine: every path, by clicks and by keys.

use limo_agent::LockWhy;
use limo_core::{AddedBy, AddedTime, PinAction, RequestAnswer, TryOutcome};

use super::*;
use crate::samples::{CHILD, NOW, PC, lock_for, request, screen};

/// Clicks the middle of a target, as laid out at the lock's size. Returns
/// None (and clicks nothing) when the target is not on the screen.
fn click(lock: &mut Lock, target: Target) -> Option<Option<WayOut>> {
    let size = lock.size;
    lock.lay_out(size, false);
    let k = lock_scale(size);
    let rect = lock.main.as_ref()?.scene.hits.iter().rev().find(|(_, t)| *t == target).map(|(r, _)| *r)?;
    let (x, y) = ((rect.x + rect.w / 2.0) * k, (rect.y + rect.h / 2.0) * k);
    Some(lock.input(Input::Click { x, y }))
}

fn on_screen(lock: &mut Lock, target: Target) -> bool {
    let size = lock.size;
    lock.lay_out(size, false);
    lock.main.as_ref().is_some_and(|d| d.scene.hits.iter().any(|(_, t)| *t == target))
}

fn key(lock: &mut Lock, k: Key) -> Option<WayOut> {
    lock.input(Input::Key(k))
}

fn type_text(lock: &mut Lock, text: &str) {
    for c in text.chars() {
        assert!(key(lock, Key::Char(c)).is_none());
    }
}

fn used_up() -> Lock {
    lock_for(&screen(LockWhy::TimeUsedUp))
}

fn added(minutes: u64) -> TryOutcome {
    TryOutcome::Added {
        added: AddedTime {
            id: "pc-mira/pin/1".into(),
            child_id: "mira".into(),
            device_id: "pc-mira".into(),
            day: 20_775,
            secs: minutes * 60,
            by: AddedBy::Pin,
            at: NOW,
        },
    }
}

#[test]
fn asking_for_time_by_clicks_sends_the_minutes_and_the_reason() {
    let mut lock = used_up();
    assert_eq!(click(&mut lock, Target::Way(Way::Ask)), Some(None));
    assert_eq!(lock.panel(), Panel::Ask);
    assert_eq!(lock.minutes(), 30, "30 minutes is picked at first");
    click(&mut lock, Target::Minutes(45));
    assert_eq!(lock.minutes(), 45);
    click(&mut lock, Target::Reason);
    type_text(&mut lock, "Science project");
    let sent = click(&mut lock, Target::Send).flatten();
    assert_eq!(sent, Some(WayOut::AskForTime { minutes: 45, reason: "Science project".into() }));
    assert_eq!(lock.waiting_for(), Some(Way::Ask));
}

#[test]
fn asking_by_keys_sends_with_enter_and_an_empty_reason_is_fine() {
    let mut lock = used_up();
    click(&mut lock, Target::Way(Way::Ask));
    assert_eq!(key(&mut lock, Key::Enter), Some(WayOut::AskForTime { minutes: 30, reason: String::new() }));
}

#[test]
fn only_one_way_out_waits_for_its_answer_and_its_button_says_so() {
    let mut lock = used_up();
    click(&mut lock, Target::Way(Way::Ask));
    assert!(key(&mut lock, Key::Enter).is_some());
    // The button no longer sends, by click or by key.
    assert!(!on_screen(&mut lock, Target::Send));
    assert_eq!(key(&mut lock, Key::Enter), None);
    let view = lock.view(false);
    assert!(matches!(view.right, Right::Ask { waiting: true, .. }));
    // The answer comes: the request shows, and asking waits for it.
    let made = request(RequestAnswer::Waiting).request;
    lock.input(Input::Answer(Answer::Asked { request: made }));
    assert_eq!(lock.panel(), Panel::Ways);
    assert_eq!(lock.waiting_for(), None);
    assert!(!on_screen(&mut lock, Target::Way(Way::Ask)), "asking comes back once the request is answered");
    assert!(on_screen(&mut lock, Target::Way(Way::Code)));
    match lock.view(false).right {
        Right::Ways { request: Some(r), .. } => {
            assert_eq!(r.text, "You asked for 30 min at 9:32 pm");
            assert_eq!(r.stamp, "Waiting");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_way_out_with_no_answer_can_be_tried_again_after_a_while() {
    let mut lock = used_up();
    click(&mut lock, Target::Way(Way::Ask));
    assert!(key(&mut lock, Key::Enter).is_some());
    lock.input(Input::Time { now: NOW + ANSWER_WAIT_SECS - 1 });
    assert_eq!(lock.waiting_for(), Some(Way::Ask));
    lock.input(Input::Time { now: NOW + ANSWER_WAIT_SECS });
    assert_eq!(lock.waiting_for(), None);
    assert_eq!(lock.message(), Some("LiMo did not answer. Try again."));
    assert!(key(&mut lock, Key::Enter).is_some());
}

#[test]
fn a_code_by_the_keys_on_the_screen() {
    let mut lock = used_up();
    click(&mut lock, Target::Way(Way::Code));
    assert_eq!(lock.panel(), Panel::Code);
    for d in [1u8, 3, 4, 2, 0, 7, 1, 5] {
        click(&mut lock, Target::Digit(d));
    }
    assert_eq!(lock.typed_digits(), 8);
    match click(&mut lock, Target::UseCode).flatten() {
        Some(WayOut::TryCode { code }) => assert_eq!(code.expose(), "13420715"),
        other => panic!("{other:?}"),
    }
    assert_eq!(lock.typed_digits(), 0, "the code is wiped when it is sent");
    assert_eq!(lock.waiting_for(), Some(Way::Code));
}

#[test]
fn a_code_by_the_keyboard_and_digits_past_the_limit_go_nowhere() {
    let mut lock = used_up();
    click(&mut lock, Target::Way(Way::Code));
    type_text(&mut lock, "1342x 0715!99");
    assert_eq!(lock.typed_digits(), 8, "letters are ignored and the ninth digit is not kept");
    key(&mut lock, Key::Backspace);
    assert_eq!(lock.typed_digits(), 7);
    assert_eq!(key(&mut lock, Key::Enter), None, "seven digits are not sent");
    assert_eq!(lock.message(), Some("A code has 8 digits."));
    click(&mut lock, Target::Digit(9));
    assert!(lock.message().is_none(), "a digit typed clears the message");
    match key(&mut lock, Key::Enter) {
        Some(WayOut::TryCode { code }) => assert_eq!(code.take(), "13420719"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_delete_key_removes_the_last_digit() {
    let mut lock = used_up();
    click(&mut lock, Target::Way(Way::Pin));
    type_text(&mut lock, "123");
    click(&mut lock, Target::Delete);
    assert_eq!(lock.typed_digits(), 2);
    click(&mut lock, Target::Delete);
    click(&mut lock, Target::Delete);
    click(&mut lock, Target::Delete);
    assert_eq!(lock.typed_digits(), 0);
}

#[test]
fn the_pin_sends_what_the_guardian_picked() {
    let mut lock = used_up();
    click(&mut lock, Target::Way(Way::Pin));
    assert_eq!(lock.pin_choice(), PinChoice::Add30);
    type_text(&mut lock, "246810");
    match key(&mut lock, Key::Enter) {
        Some(WayOut::TryPin { pin, action }) => {
            assert_eq!(pin.expose(), "246810");
            assert_eq!(action, PinAction::AddMinutes { minutes: 30 });
        }
        other => panic!("{other:?}"),
    }
    lock.input(Input::Answer(Answer::Tried { outcome: TryOutcome::Wrong { wrong_guesses: 1, wait_until: None } }));
    click(&mut lock, Target::Way(Way::Pin));
    for (choice, expected) in [
        (PinChoice::Add15, PinAction::AddMinutes { minutes: 15 }),
        (PinChoice::Add60, PinAction::AddMinutes { minutes: 60 }),
        (PinChoice::OpenHour, PinAction::UnlockUntil { until: NOW + 3600 }),
        (PinChoice::OpenTwoHours, PinAction::UnlockUntil { until: NOW + 7200 }),
        (PinChoice::OpenToday, PinAction::UnlockUntil { until: crate::words::end_of_day(NOW, PC) }),
    ] {
        click(&mut lock, Target::Choice(choice));
        assert_eq!(lock.pin_choice(), choice);
        type_text(&mut lock, "2468");
        match click(&mut lock, Target::Unlock).flatten() {
            Some(WayOut::TryPin { action, .. }) => assert_eq!(action, expected),
            other => panic!("{other:?}"),
        }
        lock.input(Input::Answer(Answer::Refused { message: "Not now.".into() }));
        assert_eq!(lock.message(), Some("Not now."));
    }
}

#[test]
fn a_pin_of_fewer_than_four_digits_is_not_sent() {
    let mut lock = used_up();
    click(&mut lock, Target::Way(Way::Pin));
    type_text(&mut lock, "246");
    assert_eq!(key(&mut lock, Key::Enter), None);
    assert_eq!(lock.message(), Some("A PIN has 6 to 8 digits."));
    // An old PIN of four digits is still tried: the rules core decides.
    type_text(&mut lock, "8");
    assert!(matches!(key(&mut lock, Key::Enter), Some(WayOut::TryPin { .. })));
}

#[test]
fn wrong_tries_count_down_then_the_wait_greys_the_code_and_the_pin() {
    let mut lock = used_up();
    click(&mut lock, Target::Way(Way::Code));
    for wrong in 1..=4u32 {
        type_text(&mut lock, "11112222");
        assert!(key(&mut lock, Key::Enter).is_some());
        lock.input(Input::Answer(Answer::Tried { outcome: TryOutcome::Wrong { wrong_guesses: wrong, wait_until: None } }));
        assert_eq!(lock.panel(), Panel::Code);
        assert_eq!(lock.typed_digits(), 0);
        let left = 5 - wrong;
        let tries = if left == 1 { "1 more try".to_string() } else { format!("{left} more tries") };
        assert_eq!(lock.message(), Some(format!("That code is not right. {tries} before a wait.").as_str()));
    }
    type_text(&mut lock, "11112222");
    assert!(key(&mut lock, Key::Enter).is_some());
    let wait = NOW + 60;
    lock.input(Input::Answer(Answer::Tried { outcome: TryOutcome::Wrong { wrong_guesses: 5, wait_until: Some(wait) } }));
    assert_eq!(lock.panel(), Panel::Ways);
    match lock.view(false).right {
        Right::Ways { wait: Some(words), rows, .. } => {
            assert_eq!(words, "Too many wrong tries. The next try is at 9:43 pm.");
            let off: Vec<(Way, bool)> = rows.iter().map(|r| (r.way, r.off)).collect();
            assert_eq!(off, vec![(Way::Ask, false), (Way::Code, true), (Way::Pin, true)]);
            assert_eq!(rows[1].sub, "Wait until 9:43 pm");
        }
        other => panic!("{other:?}"),
    }
    // The greyed rows cannot be opened, by a click or at all; asking still works.
    assert!(!on_screen(&mut lock, Target::Way(Way::Code)));
    lock.open(Way::Pin);
    assert_eq!(lock.panel(), Panel::Ways);
    click(&mut lock, Target::Way(Way::Ask));
    assert_eq!(lock.panel(), Panel::Ask);
    key(&mut lock, Key::Escape);
    // When the wait is over, they come back.
    lock.input(Input::Time { now: wait });
    assert!(on_screen(&mut lock, Target::Way(Way::Code)));
}

#[test]
fn a_wait_from_the_engine_closes_an_open_code_panel() {
    let mut lock = used_up();
    click(&mut lock, Target::Way(Way::Code));
    type_text(&mut lock, "1234");
    let mut waiting = screen(LockWhy::TimeUsedUp);
    waiting.try_wait_until = Some(NOW + 300);
    lock.show(&waiting, Some(CHILD), NOW, PC);
    assert_eq!(lock.panel(), Panel::Ways);
    assert_eq!(lock.typed_digits(), 0);
    lock.input(Input::Answer(Answer::Tried { outcome: TryOutcome::MustWait { until: NOW + 300 } }));
}

#[test]
fn must_wait_goes_back_to_the_ways_out() {
    let mut lock = used_up();
    click(&mut lock, Target::Way(Way::Pin));
    type_text(&mut lock, "123456");
    assert!(key(&mut lock, Key::Enter).is_some());
    lock.input(Input::Answer(Answer::Tried { outcome: TryOutcome::MustWait { until: NOW + 120 } }));
    assert_eq!(lock.panel(), Panel::Ways);
    assert!(!on_screen(&mut lock, Target::Way(Way::Pin)));
}

#[test]
fn escape_goes_back_from_every_panel_and_wipes_what_was_typed() {
    for way in [Way::Ask, Way::Code, Way::Pin] {
        let mut lock = used_up();
        click(&mut lock, Target::Way(way));
        assert_ne!(lock.panel(), Panel::Ways);
        type_text(&mut lock, "12345");
        assert_eq!(key(&mut lock, Key::Escape), None);
        assert_eq!(lock.panel(), Panel::Ways);
        assert_eq!(lock.typed_digits(), 0);
    }
    let mut lock = used_up();
    key(&mut lock, Key::Escape);
    assert_eq!(lock.panel(), Panel::Ways, "escape on the ways out does nothing");
    // Back does the same as Escape.
    click(&mut lock, Target::Way(Way::Code));
    type_text(&mut lock, "99");
    click(&mut lock, Target::Back);
    assert_eq!((lock.panel(), lock.typed_digits()), (Panel::Ways, 0));
}

#[test]
fn a_reason_is_at_most_two_hundred_characters() {
    let mut lock = used_up();
    click(&mut lock, Target::Way(Way::Ask));
    type_text(&mut lock, &"\u{e9}".repeat(150));
    type_text(&mut lock, &"ab".repeat(60));
    assert_eq!(lock.reason().chars().count(), 200);
    // Control characters do not go in.
    type_text(&mut lock, "\u{7}\n\t");
    assert_eq!(lock.reason().chars().count(), 200);
    match lock.view(false).right {
        Right::Ask { reason, .. } => assert_eq!(reason.chars().count(), 200),
        other => panic!("{other:?}"),
    }
    assert!(lock.overflows(Size::new(1366, 768)).is_empty());
    match key(&mut lock, Key::Enter) {
        Some(WayOut::AskForTime { reason, .. }) => assert_eq!(reason.chars().count(), 200),
        other => panic!("{other:?}"),
    }
    key(&mut lock, Key::Backspace);
}

#[test]
fn a_right_code_or_pin_says_so_until_the_lock_goes() {
    let mut lock = used_up();
    click(&mut lock, Target::Way(Way::Pin));
    click(&mut lock, Target::Choice(PinChoice::OpenTwoHours));
    type_text(&mut lock, "246810");
    assert!(key(&mut lock, Key::Enter).is_some());
    lock.input(Input::Answer(Answer::Tried { outcome: TryOutcome::Unlocked { until: NOW + 7200 } }));
    assert!(lock.opened());
    let view = lock.view(false);
    assert_eq!(view.title, "Open for 2 hours.");
    assert_eq!(view.stamp, "Open");
    assert_eq!(view.icon, Icon::Unlock);
    assert!(view.until.is_none());
    // Nothing more can be opened meanwhile.
    assert!(!on_screen(&mut lock, Target::Way(Way::Ask)));
    // A show that is still a lock leaves it said; time takes it away.
    lock.show(&screen(LockWhy::TimeUsedUp), Some(CHILD), NOW + 1, PC);
    assert!(lock.opened());
    lock.input(Input::Time { now: NOW + OPENED_SECS });
    assert!(!lock.opened());

    let mut lock = used_up();
    click(&mut lock, Target::Way(Way::Code));
    type_text(&mut lock, "13420715");
    assert!(key(&mut lock, Key::Enter).is_some());
    lock.input(Input::Answer(Answer::Tried { outcome: added(30) }));
    assert_eq!(lock.view(false).title, "30 minutes added.");
    let mut lock = used_up();
    click(&mut lock, Target::Way(Way::Pin));
    click(&mut lock, Target::Choice(PinChoice::Add60));
    type_text(&mut lock, "246810");
    assert!(key(&mut lock, Key::Enter).is_some());
    lock.input(Input::Answer(Answer::Tried { outcome: added(60) }));
    assert_eq!(lock.view(false).title, "1 hour added.");
}

#[test]
fn every_other_answer_has_its_words() {
    for (way, outcome, words) in [
        (Way::Code, TryOutcome::Malformed, "A code has 8 digits."),
        (Way::Pin, TryOutcome::Malformed, "A PIN has 6 to 8 digits."),
        (Way::Code, TryOutcome::AlreadyUsed, "That code was already used today."),
        (Way::Code, TryOutcome::NotSet, "Codes are not set up on this PC yet."),
        (Way::Pin, TryOutcome::NotSet, "A guardian PIN is not set up yet."),
        (Way::Pin, TryOutcome::Wrong { wrong_guesses: 4, wait_until: None }, "That PIN is not right. 1 more try before a wait."),
    ] {
        let mut lock = used_up();
        click(&mut lock, Target::Way(way));
        type_text(&mut lock, "13420715");
        assert!(key(&mut lock, Key::Enter).is_some());
        lock.input(Input::Answer(Answer::Tried { outcome }));
        assert_eq!(lock.message(), Some(words));
        assert!(lock.overflows(Size::new(1280, 720)).is_empty());
    }
}

#[test]
fn an_answer_nobody_waits_for_changes_nothing() {
    let mut lock = used_up();
    lock.input(Input::Answer(Answer::Tried { outcome: added(30) }));
    assert!(!lock.opened());
    lock.input(Input::Answer(Answer::Refused { message: "No.".into() }));
    assert!(lock.message().is_none());
}

#[test]
fn only_the_ways_out_offered_are_shown() {
    let mut lock = lock_for(&screen(LockWhy::BlockedHours));
    assert_eq!(lock.ways(), vec![Way::Pin]);
    assert!(!on_screen(&mut lock, Target::Way(Way::Ask)));
    assert!(!on_screen(&mut lock, Target::Way(Way::Code)));
    assert!(on_screen(&mut lock, Target::Way(Way::Pin)));
    lock.open(Way::Ask);
    assert_eq!(lock.panel(), Panel::Ways);
    // A way out taken away while it is open closes it.
    let mut lock = used_up();
    click(&mut lock, Target::Way(Way::Ask));
    type_text(&mut lock, "hello");
    let mut no_ask = screen(LockWhy::TimeUsedUp);
    no_ask.can_ask_for_time = false;
    lock.show(&no_ask, Some(CHILD), NOW, PC);
    assert_eq!(lock.panel(), Panel::Ways);
    // No way out at all: the words for the reason.
    let mut lock = lock_for(&screen(LockWhy::AccountNotLinked));
    assert!(lock.ways().is_empty());
    match lock.view(false).right {
        Right::Nothing { bold, .. } => assert_eq!(bold, "A guardian can set it up."),
        other => panic!("{other:?}"),
    }
    assert!(lock.overflows(Size::new(1366, 768)).is_empty());
}

#[test]
fn a_request_made_shows_until_the_engine_has_it_then_the_engine_s_word() {
    let mut lock = used_up();
    click(&mut lock, Target::Way(Way::Ask));
    assert!(key(&mut lock, Key::Enter).is_some());
    let made = request(RequestAnswer::Waiting).request;
    lock.input(Input::Answer(Answer::Asked { request: made }));
    assert!(lock.asked.is_some());
    let mut answered = screen(LockWhy::TimeUsedUp);
    answered.latest_request = Some(request(RequestAnswer::Granted { minutes: 20 }));
    lock.show(&answered, Some(CHILD), NOW, PC);
    assert!(lock.asked.is_none());
    match lock.view(false).right {
        Right::Ways { request: Some(r), rows, .. } => {
            assert_eq!(r.stamp, "Given 20 min");
            assert_eq!(rows.len(), 3, "asking is offered again once answered");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn every_reason_has_its_picture_stamp_and_words() {
    for why in [
        LockWhy::BlockedHours,
        LockWhy::NoTimeToday,
        LockWhy::TimeUsedUp,
        LockWhy::WaitingForTime,
        LockWhy::HeldByAnotherDevice,
        LockWhy::GuardianLock,
        LockWhy::AccountNotLinked,
        LockWhy::CannotCheck,
        LockWhy::ProgramBlocked,
    ] {
        let lock = lock_for(&screen(why));
        let view = lock.view(false);
        assert_eq!(view.icon, why_icon(why));
        assert_eq!(view.title, crate::words::typographic(why.title()));
        assert_eq!(view.eyebrow, "Mira \u{b7} this PC");
        let expected_stamp = if why == LockWhy::WaitingForTime { "One moment" } else { "Locked" };
        assert_eq!(view.stamp, expected_stamp);
        match why {
            LockWhy::TimeUsedUp | LockWhy::BlockedHours | LockWhy::NoTimeToday => {
                assert_eq!(view.until, Some(("Tomorrow, 7:00 am".to_string(), false)));
            }
            LockWhy::GuardianLock => assert_eq!(view.until, Some(("When a guardian unlocks it".to_string(), true))),
            LockWhy::HeldByAnotherDevice => {
                assert_eq!(view.until, Some(("When that device is back home".to_string(), true)));
            }
            _ => assert!(view.until.is_none()),
        }
    }
    let unknown = Lock::new().view(false);
    assert_eq!(unknown.icon, Icon::Alert, "a lock told nothing yet says LiMo cannot check the time");
}

#[test]
fn secrets_never_show_in_debug_and_are_wiped() {
    let mut lock = used_up();
    click(&mut lock, Target::Way(Way::Code));
    type_text(&mut lock, "97531864");
    let _ = lock.draw(Size::new(1366, 768));
    let debug = format!("{lock:?}");
    assert!(!debug.contains("97531864") && !debug.contains("9753"), "{debug}");
    let view_debug = format!("{:?}", lock.view(false));
    assert!(!view_debug.contains("[9, 7"), "{view_debug}");
    let sent = key(&mut lock, Key::Enter);
    let sent_debug = format!("{sent:?}");
    assert!(!sent_debug.contains("9753") && sent_debug.contains("hidden"), "{sent_debug}");
    assert_eq!(lock.typed_digits(), 0);
    assert!(lock.main.is_none(), "the picture with the digits on it is wiped");
    match sent {
        Some(WayOut::TryCode { code }) => assert_eq!(code.take(), "97531864"),
        other => panic!("{other:?}"),
    }
    // The PIN: the same, and leaving the panel wipes it too.
    click(&mut lock, Target::Way(Way::Pin));
    type_text(&mut lock, "864213");
    assert!(!format!("{lock:?}").contains("8642"));
    click(&mut lock, Target::Back);
    assert_eq!(lock.typed_digits(), 0);
    // Reset forgets everything.
    click(&mut lock, Target::Way(Way::Ask));
    type_text(&mut lock, "why");
    lock.reset();
    assert_eq!((lock.panel(), lock.reason(), lock.typed_digits()), (Panel::Ways, "", 0));
}

#[test]
fn the_pin_is_drawn_only_as_dots_and_the_code_as_digits() {
    let draw_with = |way: Way, digits: &str| {
        let mut lock = used_up();
        click(&mut lock, Target::Way(way));
        type_text(&mut lock, digits);
        lock.draw(Size::new(1366, 768)).clone()
    };
    assert_eq!(draw_with(Way::Pin, "1234"), draw_with(Way::Pin, "9876"), "the same number of dots, whatever the digits");
    assert_ne!(draw_with(Way::Pin, "1234"), draw_with(Way::Pin, "12345"));
    assert_ne!(draw_with(Way::Code, "1234"), draw_with(Way::Code, "9876"), "a code shows its digits");
}

#[test]
fn a_second_draw_of_the_same_state_is_the_same_picture() {
    let mut lock = used_up();
    let first = lock.draw(Size::new(1366, 768)).rgba().as_ptr();
    let second = lock.draw(Size::new(1366, 768)).rgba().as_ptr();
    assert_eq!(first, second, "not drawn again");
    // A tick within the same minute changes nothing on the screen.
    lock.input(Input::Time { now: NOW + 5 });
    assert_eq!(lock.draw(Size::new(1366, 768)).rgba().as_ptr(), first);
    // The next minute redraws.
    lock.input(Input::Time { now: NOW + 60 });
    let _ = lock.draw(Size::new(1366, 768));
    assert_eq!(lock.view(false).clock, "9:43 pm");
}

#[test]
fn clicks_land_on_the_same_layout_at_every_size() {
    for size in [Size::new(1920, 1080), Size::new(1280, 720), Size::new(640, 480), Size::new(1080, 1920)] {
        let mut lock = used_up();
        let _ = lock.draw(size);
        click(&mut lock, Target::Way(Way::Code)).expect("the code row is on the screen");
        assert_eq!(lock.panel(), Panel::Code, "{size:?}");
        click(&mut lock, Target::Digit(7));
        assert_eq!(lock.typed_digits(), 1);
    }
}

#[test]
fn clicks_and_keys_anywhere_never_panic() {
    for (_, mut lock) in crate::samples::lock_states() {
        let _ = lock.draw(Size::new(1366, 768));
        for y in (-20..800).step_by(37) {
            for x in (-20..1400).step_by(41) {
                let _ = lock.input(Input::Click { x: x as f32, y: y as f32 });
            }
        }
        for c in ['1', 'a', '\u{0}', '\u{1F600}', ' ', '\u{202e}', '9'] {
            let _ = lock.input(Input::Key(Key::Char(c)));
        }
        for k in [Key::Backspace, Key::Enter, Key::Escape, Key::Enter] {
            let _ = lock.input(Input::Key(k));
        }
        let _ = lock.input(Input::Click { x: f32::NAN, y: f32::INFINITY });
        let _ = lock.input(Input::Time { now: i64::MAX });
        let _ = lock.input(Input::Time { now: i64::MIN });
        let _ = lock.draw(Size::new(800, 600));
    }
}

#[test]
fn other_screens_of_different_sizes_are_each_kept() {
    let mut lock = used_up();
    let small = lock.draw_other(Size::new(1366, 768)).rgba().as_ptr();
    let large = lock.draw_other(Size::new(1920, 1080)).rgba().as_ptr();
    assert_ne!(small, large);
    assert_eq!(lock.draw_other(Size::new(1366, 768)).rgba().as_ptr(), small);
    assert_eq!(lock.draw_other(Size::new(1920, 1080)).rgba().as_ptr(), large);
    // The main screen beside them is kept too.
    let main = lock.draw(Size::new(1920, 1080)).rgba().as_ptr();
    let _ = lock.draw_other(Size::new(1366, 768));
    assert_eq!(lock.draw(Size::new(1920, 1080)).rgba().as_ptr(), main);
}
