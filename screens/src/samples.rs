//! Sample states of every screen, as the design's review sheet shows them:
//! for the preview pictures (`cargo run --example preview`) and the tests.
//! The child is Mira; it is Wednesday 18 November 2026, 9:42 pm.

use limo_agent::{LockScreen, LockWhy, WarningShown};
use limo_core::{AddedBy, AddedTime, RequestAnswer, RequestView, TimeRequest, TryOutcome, WarningCause};

use crate::lock::{Answer, Input, Key, Lock, Target, Way};
use crate::overlay::{Notice, TimeLeft};
use crate::words::{Clock, PcClock};

/// Wednesday 18 November 2026, 9:42 pm, on a PC at UTC.
pub const NOW: i64 = 1_794_960_000 + 21 * 3600 + 42 * 60;
/// Tomorrow, 7:00 am.
pub const TOMORROW_SEVEN: i64 = 1_794_960_000 + 86_400 + 7 * 3600;
/// The sample PC's clock: UTC, 12 hours.
pub const PC: PcClock = PcClock::new(0, Clock::Hours12);
/// The sample child.
pub const CHILD: &str = "Mira";

/// The engine's lock screen for a reason, with the ways out the design's
/// sample offers for it.
pub fn screen(why: LockWhy) -> LockScreen {
    let (until, ask, code, pin) = match why {
        LockWhy::TimeUsedUp | LockWhy::NoTimeToday => (Some(TOMORROW_SEVEN), true, true, true),
        LockWhy::BlockedHours => (Some(TOMORROW_SEVEN), false, false, true),
        LockWhy::GuardianLock => (None, false, false, true),
        LockWhy::HeldByAnotherDevice => (None, true, true, true),
        LockWhy::ProgramBlocked => (None, false, false, true),
        LockWhy::WaitingForTime | LockWhy::AccountNotLinked | LockWhy::CannotCheck => (None, false, false, false),
    };
    LockScreen {
        why,
        title: why.title().to_string(),
        message: why.message().to_string(),
        until,
        can_ask_for_time: ask,
        can_use_code: code,
        can_use_pin: pin,
        try_wait_until: None,
        latest_request: None,
    }
}

/// A request Mira made at 9:32 pm for 30 minutes, and its answer.
pub fn request(answer: RequestAnswer) -> RequestView {
    RequestView {
        request: TimeRequest {
            id: "pc-mira/ask/1".to_string(),
            child_id: "mira".to_string(),
            device_id: "pc-mira".to_string(),
            day: 20_775,
            minutes: 30,
            reason: "To finish my science project".to_string(),
            asked_at: NOW - 600,
        },
        answer,
        answered_at: None,
        reached_master: true,
    }
}

/// A lock screen told this screen, for Mira, now.
pub fn lock_for(screen: &LockScreen) -> Lock {
    let mut lock = Lock::new();
    lock.show(screen, Some(CHILD), NOW, PC);
    lock
}

fn typed(lock: &mut Lock, digits: &str) {
    for c in digits.chars() {
        lock.input(Input::Key(Key::Char(c)));
    }
}

/// Every lock screen state the design's sheet shows, and a few more, by
/// name: the name is the one the preview pictures are saved under.
pub fn lock_states() -> Vec<(&'static str, Lock)> {
    let mut out = Vec::new();
    for (name, why) in [
        ("time_used_up", LockWhy::TimeUsedUp),
        ("blocked_hours", LockWhy::BlockedHours),
        ("no_time_today", LockWhy::NoTimeToday),
        ("guardian_lock", LockWhy::GuardianLock),
        ("held_by_another_device", LockWhy::HeldByAnotherDevice),
        ("waiting_for_time", LockWhy::WaitingForTime),
        ("account_not_linked", LockWhy::AccountNotLinked),
        ("cannot_check", LockWhy::CannotCheck),
        ("program_blocked", LockWhy::ProgramBlocked),
    ] {
        out.push((name, lock_for(&screen(why))));
    }
    let used_up = screen(LockWhy::TimeUsedUp);

    let mut ask = lock_for(&used_up);
    open(&mut ask, Way::Ask);
    for c in "To finish my science project".chars() {
        ask.input(Input::Key(Key::Char(c)));
    }
    out.push(("panel-ask", ask));

    let mut empty = lock_for(&used_up);
    open(&mut empty, Way::Ask);
    out.push(("panel-ask-empty", empty));

    let mut long = lock_for(&used_up);
    open(&mut long, Way::Ask);
    let words = "I need more time because my science project is due tomorrow morning and I still have to finish the poster about the water cycle, print it, and check the spelling with my brother before bed, please. ";
    for c in words.chars().cycle().take(220) {
        long.input(Input::Key(Key::Char(c)));
    }
    out.push(("panel-ask-200", long));

    let mut sending = lock_for(&used_up);
    open(&mut sending, Way::Ask);
    typed(&mut sending, "Homework");
    sending.input(Input::Key(Key::Enter));
    out.push(("panel-ask-sending", sending));

    let mut asked = used_up.clone();
    asked.latest_request = Some(request(RequestAnswer::Waiting));
    out.push(("asked", lock_for(&asked)));

    let mut refused = used_up.clone();
    refused.latest_request = Some(request(RequestAnswer::Refused));
    out.push(("answered", lock_for(&refused)));

    let mut granted = used_up.clone();
    granted.latest_request = Some(request(RequestAnswer::Granted { minutes: 20 }));
    out.push(("granted", lock_for(&granted)));

    let mut code = lock_for(&used_up);
    open(&mut code, Way::Code);
    typed(&mut code, "13420");
    out.push(("panel-code", code));

    let mut pin = lock_for(&used_up);
    open(&mut pin, Way::Pin);
    typed(&mut pin, "2468");
    out.push(("panel-pin", pin));

    let mut pin_empty = lock_for(&used_up);
    open(&mut pin_empty, Way::Pin);
    out.push(("panel-pin-empty", pin_empty));

    let mut wrong = lock_for(&used_up);
    open(&mut wrong, Way::Code);
    typed(&mut wrong, "13420716");
    wrong.input(Input::Key(Key::Enter));
    wrong.input(Input::Answer(Answer::Tried { outcome: TryOutcome::Wrong { wrong_guesses: 2, wait_until: None } }));
    out.push(("wrong", wrong));

    let mut checking = lock_for(&used_up);
    open(&mut checking, Way::Code);
    typed(&mut checking, "13420715");
    checking.input(Input::Key(Key::Enter));
    out.push(("panel-code-checking", checking));

    let mut wait = used_up.clone();
    wait.try_wait_until = Some(NOW + 5 * 60);
    out.push(("wait", lock_for(&wait)));

    let mut both = wait.clone();
    both.latest_request = Some(request(RequestAnswer::Refused));
    out.push(("wait-and-answered", lock_for(&both)));

    let mut opened = lock_for(&used_up);
    open(&mut opened, Way::Code);
    typed(&mut opened, "13420715");
    opened.input(Input::Key(Key::Enter));
    let added = AddedTime {
        id: "pc-mira/code/20775/11".to_string(),
        child_id: "mira".to_string(),
        device_id: "pc-mira".to_string(),
        day: 20_775,
        secs: 1800,
        by: AddedBy::Code { selector: 11 },
        at: NOW,
    };
    opened.input(Input::Answer(Answer::Tried { outcome: TryOutcome::Added { added } }));
    out.push(("opened", opened));

    let mut clock24 = Lock::new();
    clock24.show(&used_up, Some(CHILD), NOW, PcClock::new(0, Clock::Hours24));
    out.push(("clock_24h", clock24));

    let mut long_name = Lock::new();
    let mut long_words = screen(LockWhy::HeldByAnotherDevice);
    long_words.message = "The rest of today's time is on another device that is out of the house just now. It comes here when that device is back home and has spoken to this PC, which can take a little while after it is on the home Wi-Fi again. Until then, ask a guardian.".to_string();
    long_name.show(&long_words, Some("Asha Mirabelle Konstantinopoulou the Second"), NOW, PC);
    out.push(("long_words", long_name));

    out
}

/// Opens a way out as a click on its row does.
fn open(lock: &mut Lock, way: Way) {
    let found = (0..768).step_by(4).flat_map(|y| (0..1366).step_by(8).map(move |x| (x, y))).find(|&(x, y)| {
        lock.target_at(x as f32, y as f32) == Some(Target::Way(way))
    });
    if let Some((x, y)) = found {
        lock.input(Input::Click { x: x as f32, y: y as f32 });
    }
}

/// Every state of the time left display, by name.
pub fn time_left_states() -> Vec<(&'static str, TimeLeft)> {
    let base = TimeLeft { seconds_left: Some(4320), counting: true, unlocked_until: None, now: NOW, pc: PC };
    vec![
        ("counting", base),
        ("paused", TimeLeft { counting: false, ..base }),
        ("soon", TimeLeft { seconds_left: Some(14 * 60), ..base }),
        ("last", TimeLeft { seconds_left: Some(42), ..base }),
        ("nolimit", TimeLeft { seconds_left: None, ..base }),
        // Open until 10:30 pm.
        ("unlocked", TimeLeft { unlocked_until: Some(NOW - 12 * 60 + 3600), ..base }),
    ]
}

/// Every notice, by name.
pub fn notice_states() -> Vec<(&'static str, Notice)> {
    let warning = |cause: WarningCause, minutes: u32, lock_at: i64| {
        let span = if minutes == 1 { "1 minute".to_string() } else { format!("{minutes} minutes") };
        let message = match cause {
            WarningCause::TimeRunsOut => format!("{span} of screen time left today."),
            WarningCause::BlockedHoursStart => format!("The PC locks in {span}, for the hours it is not used."),
            WarningCause::UnlockEnds => format!("The unlock ends in {span}, and the PC locks then."),
        };
        Notice::Warning { warning: WarningShown { cause, minutes, lock_at, message }, now: NOW, pc: PC }
    };
    vec![
        ("warn15", warning(WarningCause::TimeRunsOut, 15, NOW + 15 * 60)),
        ("warn5", warning(WarningCause::TimeRunsOut, 5, NOW + 5 * 60)),
        ("warn1", warning(WarningCause::TimeRunsOut, 1, NOW + 42)),
        ("bed15", warning(WarningCause::BlockedHoursStart, 15, NOW + 18 * 60)),
        ("unlock5", warning(WarningCause::UnlockEnds, 5, NOW + 5 * 60)),
        (
            "closed",
            Notice::Closed { text: "Game Launcher is blocked by your family's rules, so LiMo closed it.".to_string() },
        ),
    ]
}
