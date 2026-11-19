//! The drawn display's decisions, tested without a window.

#![forbid(unsafe_code)]

use std::sync::mpsc;

use limo_agent::{LockWhy, Show, WarningShown};
use limo_core::{PinAction, TryOutcome, WarningCause};
use limo_screens::{Key, Notice, Overlay, Panel, Size, Target, Way, samples};

use super::*;
use crate::helper::{Outbox, ShowNow};
use crate::protocol::{Answer, FromHelper};

fn at(secs: i64) -> Moment {
    Moment { wall: samples::NOW + secs, steady_ms: 1_000_000 + u64::try_from(secs).unwrap_or(0) * 1_000 }
}

fn core() -> (DrawnCore, mpsc::Receiver<FromHelper>) {
    let (tx, rx) = mpsc::channel();
    (DrawnCore::new(Outbox::new(tx)), rx)
}

fn locked(why: LockWhy) -> Show {
    Show::Locked { screen: samples::screen(why) }
}

fn show(core: &mut DrawnCore, seq: u64, show: &Show, secs: i64) -> Mode {
    core.show(&ShowNow { seq, child_name: Some("Asha"), show }, at(secs))
}

fn time_left(warning: Option<WarningShown>) -> Show {
    Show::TimeLeft { seconds_left: Some(14 * 60), counting: true, warning, unlocked_until: None }
}

/// Opens a way out with a click on its row, wherever the layout put it.
fn open(core: &mut DrawnCore, way: Way) {
    let found = (0..768)
        .step_by(4)
        .flat_map(|y| (0..1366).step_by(8).map(move |x| (x, y)))
        .find(|&(x, y)| core.lock().target_at(x as f32, y as f32) == Some(Target::Way(way)));
    let (x, y) = found.expect("the row of the way out");
    core.click(x as f32, y as f32);
    assert_ne!(core.lock().panel(), Panel::Ways, "{way:?} did not open");
}

/// Presses and lets go of a key, as the hook sees it.
fn press(core: &mut DrawnCore, vk: u32, text: Option<char>) {
    core.key(vk, true, text);
    core.key(vk, false, None);
}

/// Types digits, the odd ones on the top row and the even ones on the
/// number pad.
fn digits(core: &mut DrawnCore, typed: &str) {
    for c in typed.chars() {
        let d = c.to_digit(10).unwrap_or(0);
        let key = if d % 2 == 1 { vk::DIGIT_0 + d } else { vk::NUMPAD_0 + d };
        press(core, key, None);
    }
}

fn sent(rx: &mpsc::Receiver<FromHelper>) -> Vec<FromHelper> {
    rx.try_iter().collect()
}

fn wrong() -> Answer {
    Answer::Tried { outcome: TryOutcome::Wrong { wrong_guesses: 1, wait_until: None } }
}

// ---------------------------------------------------------------------------
// Keys
// ---------------------------------------------------------------------------

#[test]
fn windows_keys_become_the_locks_keys() {
    for wanted in [false, true] {
        assert_eq!(lock_key(vk::BACK, None, wanted), Some(Key::Backspace));
        assert_eq!(lock_key(vk::RETURN, Some('\r'), wanted), Some(Key::Enter));
        assert_eq!(lock_key(vk::ESCAPE, Some('\u{1b}'), wanted), Some(Key::Escape));
    }
    // Digits from the top row and the number pad, whatever the layout makes
    // of them: on a French layout the top row's 1 types "&" without Shift.
    assert_eq!(lock_key(vk::DIGIT_0 + 1, Some('&'), false), Some(Key::Char('1')));
    assert_eq!(lock_key(vk::DIGIT_9, None, false), Some(Key::Char('9')));
    assert_eq!(lock_key(vk::NUMPAD_0, None, false), Some(Key::Char('0')));
    assert_eq!(lock_key(vk::NUMPAD_9, Some('9'), false), Some(Key::Char('9')));
    // Letters and everything else: nothing, unless text is wanted.
    assert_eq!(lock_key(0x41, Some('a'), false), None);
    assert_eq!(lock_key(0x70, None, false), None);
    // While text is wanted, the layout's text, whatever key gave it.
    assert_eq!(lock_key(0x41, Some('A'), true), Some(Key::Char('A')));
    assert_eq!(lock_key(vk::DIGIT_0 + 1, Some('!'), true), Some(Key::Char('!')));
    assert_eq!(lock_key(vk::DIGIT_0 + 1, Some('&'), true), Some(Key::Char('&')));
    assert_eq!(lock_key(0xDE, Some('\u{e9}'), true), Some(Key::Char('\u{e9}')));
    assert_eq!(lock_key(0x20, Some(' '), true), Some(Key::Char(' ')));
    // A key that gives no text, or a control character, types nothing.
    assert_eq!(lock_key(vk::DIGIT_0 + 1, None, true), None);
    assert_eq!(lock_key(0x09, Some('\t'), true), None);
    assert_eq!(lock_key(0x41, Some('\u{1}'), true), None);
}

#[test]
fn the_modifiers_are_kept_by_side_and_make_the_keyboard_state() {
    let mut mods = Mods::default();
    assert!(mods.types_text());
    assert_eq!(mods.keyboard_state(false), [0u8; 256]);
    mods.note(vk::RSHIFT, true);
    assert!(mods.shift());
    let state = mods.keyboard_state(true);
    assert_eq!((state[vk::SHIFT as usize], state[vk::RSHIFT as usize], state[vk::LSHIFT as usize]), (0x80, 0x80, 0));
    assert_eq!(state[vk::CAPITAL as usize], 0x01);
    mods.note(vk::RSHIFT, false);
    assert!(!mods.shift());
    // Ctrl or Alt alone is a shortcut; both, as AltGr sends them, type.
    mods.note(vk::LCONTROL, true);
    assert!(!mods.types_text());
    mods.note(vk::RMENU, true);
    assert!(mods.types_text());
    let state = mods.keyboard_state(false);
    assert_eq!((state[vk::CONTROL as usize], state[vk::MENU as usize], state[vk::RMENU as usize]), (0x80, 0x80, 0x80));
    mods.note(vk::LCONTROL, false);
    assert!(!mods.types_text());
    mods.note(vk::RMENU, false);
    mods.note(vk::LWIN, true);
    assert!(!mods.types_text());
    mods.note(vk::LWIN, false);
    // Any other key changes nothing.
    mods.note(0x41, true);
    assert_eq!(mods, Mods::default());
}

#[test]
fn text_from_windows_and_points_from_mouse_messages_are_read_with_care() {
    assert_eq!(first_char(&[0x61]), Some('a'));
    assert_eq!(first_char(&[0xE9, 0x61]), Some('\u{e9}'));
    assert_eq!(first_char(&[0xD83D, 0xDE00]), Some('\u{1F600}'));
    assert_eq!(first_char(&[0xD83D]), None);
    assert_eq!(first_char(&[]), None);
    assert_eq!(point_from(0x0040_0020), (32, 64));
    // A window's point can be left of or above it while the mouse is held.
    assert_eq!(point_from(0xFFFF_FFFE_u32 as isize), (-2, -1));
    assert_eq!(point_from(-1), (-1, -1));
}

#[test]
fn the_pcs_clock_follows_the_users_own_time_format() {
    assert_eq!(clock_from_format("h:mm:ss tt"), Clock::Hours12);
    assert_eq!(clock_from_format("HH:mm:ss"), Clock::Hours24);
    assert_eq!(clock_from_format("H.mm"), Clock::Hours24);
    assert_eq!(clock_from_format("tt hh:mm"), Clock::Hours12);
    // Text in quotes is not the hour.
    assert_eq!(clock_from_format("'h'HH:mm"), Clock::Hours24);
    assert_eq!(clock_from_format(""), Clock::Hours12);
    assert_eq!(pc_clock(19_800, "HH:mm"), PcClock::new(330, Clock::Hours24));
    assert_eq!(pc_clock(-12_600, "h:mm tt"), PcClock::new(-210, Clock::Hours12));
}

// ---------------------------------------------------------------------------
// Places
// ---------------------------------------------------------------------------

const MAIN: Area = Area { left: 0, top: 0, right: 1920, bottom: 1080 };
const MAIN_WORK: Area = Area { left: 0, top: 0, right: 1920, bottom: 1032 };
/// A second monitor to the right, shorter, with no taskbar.
const SECOND_WORK: Area = Area { left: 1920, top: 0, right: 3200, bottom: 720 };

#[test]
fn the_time_left_display_starts_above_the_clock_by_the_right_edge() {
    let work = Area { left: 0, top: 0, right: 1366, bottom: 720 };
    let anchor = start_anchor(work, 1.0);
    // The card's corner: 16 in from the right, 14 above the taskbar.
    assert_eq!(anchor, Anchor { right: 1350, bottom: 706 });
    // Its picture reaches past the card by its shadow, 3 and the edge's 1.
    let shadow = shadow_px(PILL_SHADOW, 1.0);
    assert_eq!(shadow, 4);
    let size = Size::new(180, 38);
    assert_eq!(rect_at(anchor, size, shadow), Area { left: 1174, top: 672, right: 1354, bottom: 710 });
    // The real picture is the card's 34 design pixels high and the shadow's.
    let left = TimeLeft { seconds_left: Some(600), counting: true, unlocked_until: None, now: samples::NOW, pc: PcClock::default() };
    let picture_height = Overlay::new().time_left(&left, 1.0).height();
    assert_eq!(i64::from(picture_height) - 34, i64::from(shadow));
    assert_eq!(shadow_px(NOTICE_SHADOW, 1.4), 7);
    // At the scale of 1920 by 1080: 16 and 14 design pixels grow with it.
    let scale = limo_screens::scale_for(MAIN.width(), MAIN.height());
    assert_eq!(start_anchor(MAIN_WORK, scale), Anchor { right: 1920 - 22, bottom: 1032 - 20 });
    // A taskbar at the top, or on the left: the work area says where.
    let top_bar = Area { left: 0, top: 48, right: 1920, bottom: 1080 };
    assert_eq!(start_anchor(top_bar, 1.0), Anchor { right: 1904, bottom: 1066 });
    assert_eq!(MAIN.middle(), (960, 540));
    assert_eq!(Area { left: 10, top: 10, right: 5, bottom: 5 }.size(), Size::new(1, 1));
}

#[test]
fn a_drag_moves_the_display_with_the_pointer_and_keeps_it_on_a_screen() {
    let works = [MAIN_WORK, SECOND_WORK];
    let size = Size::new(180, 38);
    let shadow = 4;
    let from = Anchor { right: 1900, bottom: 1000 };
    let drag = |to: (i32, i32)| dragged(from, (1850, 990), to, size, shadow, &works);
    // Up and to the left by the pointer's own move.
    assert_eq!(drag((1350, 590)), Anchor { right: 1400, bottom: 600 });
    // Past the bottom right corner: kept inside the work area, its shadow
    // too, never under the taskbar.
    assert_eq!(drag((1910, 1075)), Anchor { right: 1916, bottom: 1028 });
    // Past the top left corner.
    assert_eq!(drag((-500, -500)), Anchor { right: 176, bottom: 34 });
    // Onto the second monitor: kept inside it.
    assert_eq!(drag((2600, 400)), Anchor { right: 2650, bottom: 410 });
    // Below the second monitor, where no monitor is: onto the nearest.
    assert_eq!(drag((2600, 1000)), Anchor { right: 2650, bottom: 716 });
    // Far off any number: no overflow, and still on a screen.
    let far = dragged(from, (i32::MIN, i32::MIN), (i32::MAX, i32::MAX), size, shadow, &works);
    let window = rect_at(far, size, shadow);
    assert!(SECOND_WORK.contains(window.left, window.top) && SECOND_WORK.contains(window.right - 1, window.bottom - 1), "{far:?}");
    // No work area known: left where it is.
    assert_eq!(kept_on_screen(from, size, shadow, &[]), from);
    // A display wider than its screen: from its top left corner.
    let tiny = Area { left: 0, top: 0, right: 100, bottom: 30 };
    assert_eq!(rect_at(kept_on_screen(from, size, shadow, &[tiny]), size, shadow).left, 0);
    assert_eq!(area_for(rect_at(from, size, shadow), &works), Some(MAIN_WORK));
}

#[test]
fn a_notice_sits_above_the_display_or_below_it_when_there_is_no_room() {
    // The display's picture, reaching 4 past its card; the notice's, 5.
    let (pill_shadow, shadow) = (4, 5);
    let pill = Area { left: 1718, top: 974, right: 1898, bottom: 1012 };
    let size = Size::new(344, 150);
    // Above: the cards' right edges in line, 14 apart at scale 1.
    let (left, top) = notice_at(pill, pill_shadow, size, shadow, MAIN_WORK, 1.0);
    assert_eq!((left, top), (1555, 815));
    assert_eq!(left + 344 - shadow, pill.right - pill_shadow);
    assert_eq!(top + 150 - shadow, pill.top - 14);
    // At a larger scale the gap grows.
    assert_eq!(notice_at(pill, pill_shadow, size, shadow, MAIN_WORK, 2.0), (1555, 974 - 28 - 145));
    // The display dragged to the top: the notice goes below its card.
    let high = Area { left: 400, top: 10, right: 580, bottom: 48 };
    assert_eq!(notice_at(high, pill_shadow, size, shadow, MAIN_WORK, 1.0), (237, 44 + 14));
    // By the left edge: kept inside the work area.
    let by_edge = Area { left: 0, top: 500, right: 180, bottom: 538 };
    assert_eq!(notice_at(by_edge, pill_shadow, size, shadow, MAIN_WORK, 1.0).0, 0);
    // A work area smaller than the notice: from its top left corner.
    let tiny = Area { left: 0, top: 0, right: 200, bottom: 100 };
    let small = Area { left: 0, top: 50, right: 100, bottom: 80 };
    assert_eq!(notice_at(small, pill_shadow, size, shadow, tiny, 1.0), (0, 0));
}

// ---------------------------------------------------------------------------
// Pixels
// ---------------------------------------------------------------------------

#[test]
fn pixels_go_into_a_windows_bitmap_and_only_a_change_says_so() {
    let size = Size::new(2, 2);
    let picture = [10, 20, 30, 255, 1, 2, 3, 4, 40, 50, 60, 255, 7, 8, 9, 128];
    let mut bitmap = vec![0u8; 16];
    assert!(copy_to_bgra(&picture, size, &mut bitmap, 2, 2));
    assert_eq!(bitmap, vec![30, 20, 10, 255, 3, 2, 1, 4, 60, 50, 40, 255, 9, 8, 7, 128]);
    // The same picture again changes nothing.
    assert!(!copy_to_bgra(&picture, size, &mut bitmap, 2, 2));
    // A bitmap smaller than the picture takes what it holds; a larger one
    // keeps the rest as it was.
    let mut small = vec![0u8; 4];
    assert!(copy_to_bgra(&picture, size, &mut small, 1, 1));
    assert_eq!(small, vec![30, 20, 10, 255]);
    let mut large = vec![7u8; 36];
    assert!(copy_to_bgra(&picture, size, &mut large, 3, 3));
    assert_eq!(&large[..12], &[30, 20, 10, 255, 3, 2, 1, 4, 7, 7, 7, 7]);
    assert_eq!(&large[24..], &[7u8; 12]);
    // Buffers of the wrong length change nothing, and never panic.
    assert!(!copy_to_bgra(&picture[..15], size, &mut bitmap, 2, 2));
    assert!(!copy_to_bgra(&picture, size, &mut bitmap, 3, 2));
    assert!(!copy_to_bgra(&picture, Size::new(u32::MAX, u32::MAX), &mut bitmap, 2, 2));
}

// ---------------------------------------------------------------------------
// Notices
// ---------------------------------------------------------------------------

fn warning(minutes: u32) -> WarningShown {
    WarningShown {
        cause: WarningCause::TimeRunsOut,
        minutes,
        lock_at: samples::NOW + i64::from(minutes) * 60,
        message: format!("{minutes} minutes of screen time left today."),
    }
}

#[test]
fn notices_take_their_turn_go_with_ok_and_go_by_themselves() {
    let mut shelf = Shelf::default();
    assert_eq!(shelf.top(0), None);
    shelf.warn(&warning(5), 1_000);
    // The same warning in every show after: shelved once.
    shelf.warn(&warning(5), 2_000);
    shelf.closed("Game.exe is blocked by your family's rules, so LiMo closed it.", Some(" Game.exe "), 3_000);
    let closed = Shelved::Closed {
        text: "Game.exe is blocked by your family's rules, so LiMo closed it.".into(),
        program: Some("Game.exe".into()),
    };
    // The newest first; OK puts it away, and the warning is back.
    assert_eq!(shelf.top(3_000), Some(&closed));
    shelf.dismiss();
    assert_eq!(shelf.top(3_000), Some(&Shelved::Warning(warning(5))));
    // A notice goes after 10 seconds, a warning after 30.
    shelf.closed("Again.", None, 4_000);
    assert_eq!(shelf.top(4_000 + NOTICE_SHOWN_MS - 1), Some(&Shelved::Closed { text: "Again.".into(), program: None }));
    assert_eq!(shelf.top(4_000 + NOTICE_SHOWN_MS), Some(&Shelved::Warning(warning(5))));
    assert_eq!(shelf.top(1_000 + WARNING_SHOWN_MS), None);
    // Dismissed, it does not come back with the next show.
    shelf.warn(&warning(5), 60_000);
    assert_eq!(shelf.top(60_000), None);
    // At most a few wait.
    for n in 0..10 {
        shelf.closed(&format!("Closed {n}."), None, 70_000);
    }
    assert_eq!(shelf.items.len(), MAX_SHELVED);
    // An empty name is no name.
    shelf.closed("Closed.", Some("  "), 80_000);
    assert_eq!(shelf.top(80_000), Some(&Shelved::Closed { text: "Closed.".into(), program: None }));
}

// ---------------------------------------------------------------------------
// The machine
// ---------------------------------------------------------------------------

#[test]
fn a_way_out_goes_out_once_and_only_its_own_answer_comes_back() {
    let (mut core, rx) = core();
    assert_eq!(show(&mut core, 1, &locked(LockWhy::TimeUsedUp), 0), Mode::Locked);
    open(&mut core, Way::Code);
    digits(&mut core, "13420715");
    press(&mut core, vk::RETURN, None);
    // Enter again, and a click where nothing is: nothing more goes.
    press(&mut core, vk::RETURN, None);
    core.click(2.0, 2.0);
    let out = sent(&rx);
    assert_eq!(out.len(), 1, "{out:?}");
    let FromHelper::TryCode { ask, code } = &out[0] else { panic!("not a code: {out:?}") };
    assert_eq!((*ask, code.expose()), (1, "13420715"));
    assert_eq!(core.waiting(), Some(1));
    assert_eq!(core.lock().typed_digits(), 0);
    // An answer to another way out does not reach this lock.
    assert!(!core.answer(2, &wrong()));
    assert_eq!(core.lock().waiting_for(), Some(Way::Code));
    // Its own does, once.
    assert!(core.answer(1, &wrong()));
    assert_eq!(core.waiting(), None);
    assert!(core.lock().message().is_some_and(|m| m.contains("not right")), "{:?}", core.lock().message());
    assert!(!core.answer(1, &Answer::Refused { message: "Late.".into() }));
    assert_ne!(core.lock().message(), Some("Late."));
}

#[test]
fn an_answer_for_a_lock_taken_down_never_reaches_the_next_one() {
    let (mut core, rx) = core();
    show(&mut core, 1, &locked(LockWhy::TimeUsedUp), 0);
    open(&mut core, Way::Pin);
    digits(&mut core, "482915");
    press(&mut core, vk::RETURN, None);
    assert_eq!(sent(&rx).len(), 1);
    // The lock goes, and another comes.
    assert_eq!(show(&mut core, 2, &time_left(None), 1), Mode::TimeLeft);
    assert_eq!(core.waiting(), None);
    show(&mut core, 3, &locked(LockWhy::GuardianLock), 2);
    assert_eq!(core.lock().panel(), Panel::Ways);
    assert!(!core.answer(1, &Answer::Refused { message: "Late.".into() }));
    assert_eq!(core.lock().message(), None);
}

#[test]
fn a_lock_that_waits_too_long_for_its_answer_forgets_it() {
    let (mut core, rx) = core();
    show(&mut core, 1, &locked(LockWhy::TimeUsedUp), 0);
    open(&mut core, Way::Code);
    digits(&mut core, "13420715");
    press(&mut core, vk::RETURN, None);
    assert_eq!(core.waiting(), Some(1));
    core.tick(at(limo_screens::ANSWER_WAIT_SECS + 1));
    assert_eq!(core.waiting(), None);
    assert!(!core.answer(1, &wrong()));
    // The child can try again, and that goes with a new number.
    digits(&mut core, "13420716");
    press(&mut core, vk::RETURN, None);
    let out = sent(&rx);
    assert!(matches!(&out[..], [FromHelper::TryCode { ask: 1, .. }, FromHelper::TryCode { ask: 2, .. }]), "{out:?}");
    assert_eq!(core.waiting(), Some(2));
}

#[test]
fn a_pin_moves_into_the_secret_and_nothing_of_it_stays_behind() {
    let (mut core, rx) = core();
    show(&mut core, 1, &locked(LockWhy::GuardianLock), 0);
    open(&mut core, Way::Pin);
    digits(&mut core, "482915");
    assert_eq!(core.lock().typed_digits(), 6);
    let before = format!("{core:?}");
    assert!(!before.contains("4829") && !before.contains("2915"), "{before}");
    press(&mut core, vk::RETURN, None);
    let out = sent(&rx);
    let [FromHelper::TryPin { ask: 1, pin, action }] = &out[..] else { panic!("not one PIN: {out:?}") };
    assert_eq!(pin.expose(), "482915");
    assert_eq!(*action, PinAction::AddMinutes { minutes: 30 });
    assert!(!format!("{out:?}").contains("4829"));
    assert_eq!(core.lock().typed_digits(), 0);
    let after = format!("{core:?}");
    assert!(!after.contains("4829") && !after.contains("2915"), "{after}");
    // Escape, while it is checked, keeps nothing typed either.
    press(&mut core, vk::ESCAPE, None);
    assert_eq!(core.lock().panel(), Panel::Ways);
}

#[test]
fn a_reason_is_typed_with_the_layouts_text_and_shortcuts_type_nothing() {
    let (mut core, rx) = core();
    show(&mut core, 1, &locked(LockWhy::TimeUsedUp), 0);
    // No panel open: no text is wanted, and keys type nothing.
    assert!(!core.text_wanted());
    press(&mut core, 0x48, Some('h'));
    open(&mut core, Way::Ask);
    assert!(core.text_wanted());
    // Shift and H, i, then Shift and the top row's 1: the layout's text.
    core.key(vk::LSHIFT, true, None);
    press(&mut core, 0x48, Some('H'));
    core.key(vk::LSHIFT, false, None);
    press(&mut core, 0x49, Some('i'));
    core.key(vk::RSHIFT, true, None);
    press(&mut core, vk::DIGIT_0 + 1, Some('!'));
    core.key(vk::RSHIFT, false, None);
    // The top row's digit without its text types nothing here.
    press(&mut core, vk::DIGIT_0 + 2, None);
    // Ctrl and V: a shortcut, not text.
    core.key(vk::LCONTROL, true, None);
    assert!(!core.text_wanted());
    press(&mut core, 0x56, Some('\u{16}'));
    press(&mut core, 0x56, Some('v'));
    core.key(vk::LCONTROL, false, None);
    // AltGr and E, as a layout with the euro sign on it gives it.
    core.key(vk::LCONTROL, true, None);
    core.key(vk::RMENU, true, None);
    assert!(core.text_wanted());
    press(&mut core, 0x45, Some('\u{20ac}'));
    core.key(vk::RMENU, false, None);
    core.key(vk::LCONTROL, false, None);
    assert_eq!(core.lock().reason(), "Hi!\u{20ac}");
    press(&mut core, vk::BACK, None);
    press(&mut core, vk::RETURN, None);
    let out = sent(&rx);
    assert_eq!(out, vec![FromHelper::AskForTime { ask: 1, minutes: 30, reason: "Hi!".into() }]);
}

#[test]
fn keys_reach_nothing_but_a_lock_and_a_new_lock_starts_with_no_key_held() {
    let (mut core, rx) = core();
    show(&mut core, 1, &time_left(None), 0);
    digits(&mut core, "13420715");
    press(&mut core, vk::RETURN, None);
    core.click(700.0, 400.0);
    assert!(sent(&rx).is_empty());
    // A Shift seen going down before the lock is not taken as held: the
    // keyboard hook comes with the lock, and knows nothing from before it.
    core.key(vk::LSHIFT, true, None);
    show(&mut core, 2, &locked(LockWhy::TimeUsedUp), 1);
    assert_eq!(core.keyboard_state(false)[vk::SHIFT as usize], 0);
    open(&mut core, Way::Ask);
    press(&mut core, 0x41, Some('a'));
    assert_eq!(core.lock().reason(), "a");
}

#[test]
fn the_same_lock_again_keeps_what_the_child_is_doing() {
    let (mut core, _rx) = core();
    let lock = locked(LockWhy::TimeUsedUp);
    show(&mut core, 1, &lock, 0);
    open(&mut core, Way::Code);
    digits(&mut core, "134");
    for seq in [1, 1, 2] {
        show(&mut core, seq, &lock, 1);
    }
    assert_eq!((core.lock().panel(), core.lock().typed_digits()), (Panel::Code, 3));
    // A lock that offers no code closes the panel and wipes the digits.
    show(&mut core, 3, &locked(LockWhy::GuardianLock), 2);
    assert_eq!((core.lock().panel(), core.lock().typed_digits()), (Panel::Ways, 0));
}

#[test]
fn the_time_left_and_its_notices_show_only_while_the_pc_may_be_used() {
    let (mut core, _rx) = core();
    assert!(core.pill(1.0).is_none() && core.notice_picture(1.0).is_none());
    show(&mut core, 1, &time_left(Some(warning(15))), 0);
    assert!(core.pill(1.0).is_some_and(|p| p.width() > 20));
    let expected = |notice: &Notice| Overlay::new().notice(notice, 1.0).rgba().to_vec();
    let warned = expected(&Notice::Warning { warning: warning(15), now: at(0).wall, pc: PcClock::default() });
    let (first, ok) = core.notice_picture(1.0).map(|(p, ok)| (p.rgba().to_vec(), ok)).expect("the warning");
    assert_eq!(first, warned);
    // A program closed: its notice, by name, above the warning.
    core.notice("Game.exe is blocked by your family's rules, so LiMo closed it.", Some("Game.exe"), at(1));
    let named = core.notice_picture(1.0).map(|(p, _)| p.rgba().to_vec());
    assert_eq!(named, Some(expected(&Notice::Blocked { program: "Game.exe".into() })));
    // A click beside OK does nothing; on OK, it goes and the warning is back.
    assert!(!core.notice_click(ok, 1.0, 1.0));
    let middle = (ok.x as f32 + ok.width as f32 / 2.0, ok.y as f32 + ok.height as f32 / 2.0);
    assert!(core.notice_click(ok, middle.0, middle.1));
    assert_eq!(core.notice_picture(1.0).map(|(p, _)| p.rgba().to_vec()), Some(warned));
    // Locked: no time left display and no notice, though one waits.
    core.notice("Again.", None, at(2));
    show(&mut core, 2, &locked(LockWhy::TimeUsedUp), 2);
    assert!(core.pill(1.0).is_none() && core.notice_picture(1.0).is_none());
    assert!(!core.notice_click(ok, middle.0, middle.1));
    // Back to the time left within its ten seconds: it shows.
    show(&mut core, 3, &time_left(None), 5);
    assert_eq!(
        core.notice_picture(1.0).map(|(p, _)| p.rgba().to_vec()),
        Some(expected(&Notice::Closed { text: "Again.".into() }))
    );
    // Nothing: nothing.
    show(&mut core, 4, &Show::Nothing, 6);
    assert!(core.pill(1.0).is_none() && core.notice_picture(1.0).is_none());
}

#[test]
fn the_lock_draws_on_the_main_monitor_and_the_others() {
    let (mut core, _rx) = core();
    core.set_clock(PcClock::new(330, Clock::Hours24));
    show(&mut core, 1, &locked(LockWhy::BlockedHours), 0);
    let main = core.lock_picture(Size::new(1920, 1080)).rgba().to_vec();
    let other = core.other_picture(Size::new(1280, 1024)).rgba().to_vec();
    assert_eq!(main.len(), 1920 * 1080 * 4);
    assert_eq!(other.len(), 1280 * 1024 * 4);
    // Opaque everywhere, as a lock must be.
    assert!(main.as_chunks::<4>().0.iter().all(|px| px[3] == 255));
}

#[test]
fn the_services_answers_become_the_locks() {
    let request = limo_core::TimeRequest {
        id: "pc-asha/ask/1".into(),
        child_id: "asha".into(),
        device_id: "pc-asha".into(),
        day: 20_775,
        minutes: 30,
        reason: "Homework".into(),
        asked_at: samples::NOW,
    };
    assert_eq!(lock_answer(&Answer::Asked { request: request.clone() }), LockAnswer::Asked { request });
    assert_eq!(
        lock_answer(&Answer::Tried { outcome: TryOutcome::Malformed }),
        LockAnswer::Tried { outcome: TryOutcome::Malformed }
    );
    assert_eq!(lock_answer(&Answer::Refused { message: "No.".into() }), LockAnswer::Refused { message: "No.".into() });
}
