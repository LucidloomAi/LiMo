//! Every state of every screen draws at every size, and no text leaves its
//! room, through the crate's public face only.

use limo_agent::{LockScreen, LockWhy};
use limo_core::{RequestAnswer, TryOutcome};
use limo_screens::samples::{self, CHILD, NOW, PC};
use limo_screens::{Answer, Clock, Input, Key, Lock, MAX_SIDE, Overlay, PcClock, Size, scale_for};

/// The three sizes the design is checked at.
const SIZES: [Size; 3] = [Size::new(1366, 768), Size::new(1920, 1080), Size::new(1280, 720)];

#[test]
fn no_text_leaves_its_room_in_any_state_at_the_three_sizes() {
    for (name, mut lock) in samples::lock_states() {
        for size in SIZES.into_iter().chain([Size::new(640, 480), Size::new(1024, 768)]) {
            let spilled = lock.overflows(size);
            assert!(spilled.is_empty(), "{name} at {size:?}: {spilled:#?}");
        }
    }
}

#[test]
fn every_state_draws_at_every_size() {
    let sizes = [
        Size::new(640, 480),
        Size::new(800, 600),
        Size::new(1024, 768),
        Size::new(1280, 720),
        Size::new(1366, 768),
        Size::new(1920, 1080),
        Size::new(1080, 1920),
        Size::new(2560, 1440),
        Size::new(3000, 500),
    ];
    for (name, mut lock) in samples::lock_states() {
        for size in sizes {
            let picture = lock.draw(size);
            assert_eq!(picture.size(), size, "{name}");
            // The lock screen is opaque, every pixel of it.
            assert!(picture.rgba().as_chunks::<4>().0.iter().all(|px| px[3] == 255), "{name} at {size:?}");
            let other = lock.draw_other(size);
            assert_eq!(other.size(), size);
        }
    }
}

#[test]
fn odd_sizes_never_panic() {
    let mut lock = samples::lock_for(&samples::screen(LockWhy::TimeUsedUp));
    for size in [Size::new(0, 0), Size::new(1, 1), Size::new(10, 10000), Size::new(u32::MAX, 3), Size::new(639, 479)] {
        let picture = lock.draw(size);
        assert!(picture.width() >= 1 && picture.width() <= MAX_SIDE && picture.height() <= MAX_SIDE);
        let _ = lock.draw_other(size);
        let _ = lock.input(Input::Click { x: 5.0, y: 5.0 });
    }
}

#[test]
fn a_large_monitor_draws() {
    let mut lock = samples::lock_for(&samples::screen(LockWhy::TimeUsedUp));
    let picture = lock.draw(Size::new(3840, 2160));
    assert_eq!(picture.size(), Size::new(3840, 2160));
    assert!(lock.overflows(Size::new(3840, 2160)).is_empty());
}

#[test]
fn words_of_any_length_from_the_engine_are_cut_with_care() {
    let mut screen = samples::screen(LockWhy::TimeUsedUp);
    screen.title = "A title far longer than any title the engine gives, and longer again, ".repeat(6);
    screen.message = "word ".repeat(2000);
    screen.latest_request = Some(samples::request(RequestAnswer::Granted { minutes: 1440 }));
    let mut lock = Lock::new();
    lock.show(&screen, Some(&"Konstantinopoulou ".repeat(40)), NOW, PC);
    for size in SIZES.into_iter().chain([Size::new(640, 480)]) {
        let spilled = lock.overflows(size);
        assert!(spilled.is_empty(), "{size:?}: {spilled:#?}");
    }
    // A long word with no space in it, and a refusal of any length.
    screen.message = "x".repeat(5000);
    lock.show(&screen, Some(""), NOW, PC);
    lock.input(Input::Key(Key::Escape));
    assert!(lock.overflows(Size::new(1366, 768)).is_empty());
    let mut lock = samples::lock_for(&samples::screen(LockWhy::TimeUsedUp));
    let _ = lock.draw(Size::new(1366, 768));
    // Open the code panel by keys alone is not possible; open it by a click
    // on its row, wherever it is.
    let row = (0..768)
        .step_by(4)
        .flat_map(|y| (0..1366).step_by(8).map(move |x| (x, y)))
        .find(|&(x, y)| lock.target_at(x as f32, y as f32) == Some(limo_screens::Target::Way(limo_screens::Way::Code)));
    let (x, y) = row.expect("the code row");
    lock.input(Input::Click { x: x as f32, y: y as f32 });
    for c in "13420715".chars() {
        lock.input(Input::Key(Key::Char(c)));
    }
    assert!(lock.input(Input::Key(Key::Enter)).is_some());
    lock.input(Input::Answer(Answer::Refused { message: "No. ".repeat(500) }));
    assert!(lock.overflows(Size::new(1280, 720)).is_empty());
    lock.input(Input::Answer(Answer::Tried { outcome: TryOutcome::NotSet }));
    assert!(lock.overflows(Size::new(1280, 720)).is_empty());
}

#[test]
fn every_reason_in_twenty_four_hours_and_any_offset() {
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
        for pc in [PcClock::new(0, Clock::Hours24), PcClock::new(330, Clock::Hours12), PcClock::new(-600, Clock::Hours24)] {
            let mut lock = Lock::new();
            let mut screen: LockScreen = samples::screen(why);
            screen.until = screen.until.map(|u| u + 9 * 86_400);
            lock.show(&screen, Some(CHILD), NOW, pc);
            for size in SIZES {
                assert!(lock.overflows(size).is_empty(), "{why:?} {pc:?} {size:?}");
            }
        }
    }
}

#[test]
fn the_scale_is_the_designs() {
    assert_eq!(scale_for(1366, 768), 1.0);
    let mut lock = samples::lock_for(&samples::screen(LockWhy::TimeUsedUp));
    // The card is in the middle at every size: its middle pixel is the
    // card's colour, the corner is the page's.
    for size in SIZES {
        let picture = lock.draw(size);
        let corner = picture.pixel(2, size.height - 2).expect("corner");
        assert_eq!(&corner[..3], &[0xf6, 0xf1, 0xe4], "{size:?}");
    }
}

#[test]
fn the_overlay_draws_every_state() {
    let mut overlay = Overlay::new();
    for (scale, size) in [(scale_for(1366, 768), 1.0f32), (scale_for(1920, 1080), 1.4)] {
        for (name, t) in samples::time_left_states() {
            let p = overlay.time_left(&t, scale);
            assert!(p.height() as f32 >= 36.0 * size, "{name}");
        }
        for (name, n) in samples::notice_states() {
            let p = overlay.notice(&n, scale);
            assert!(p.width() as f32 >= 343.0 * size, "{name}");
        }
    }
}
