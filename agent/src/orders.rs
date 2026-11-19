//! The orders: what the engine asks the Windows service to do, as plain
//! data.
//!
//! Orders are levels, not changes: "this session shows this", "this program
//! must not run in this session". Carrying out the same order twice does no
//! harm, and an order missed is given again at the next call. The one thing
//! given once is a warning; it carries what it is about, so a helper that
//! is handed the same warning twice can tell.

use limo_core::{LockReason, RequestView, WarningCause};
use serde::{Deserialize, Serialize};

use crate::error::EngineError;
use crate::facts::Program;

/// Everything to be done after one call.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Orders {
    /// One order for every session in the facts, in the same order.
    pub sessions: Vec<SessionOrder>,
    /// Devices of the family (phones) that the hub would like to hear from
    /// now: ask each one that can be reached to report. The PC's own devices
    /// are never here; the engine has already exchanged for them.
    pub prod_devices: Vec<String>,
    /// What went wrong on the way, for the service's log. The orders are
    /// still sound: the engine works from what it holds in memory until the
    /// family's file can be written again.
    pub problems: Vec<EngineError>,
}

/// What must happen in one session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionOrder {
    /// The session.
    pub session_id: u32,
    /// The child whose session it is, when it is a child's.
    pub child_id: Option<String>,
    /// True when LiMo's helper must run in this session: start it if it is
    /// not running.
    pub helper_needed: bool,
    /// What the helper shows.
    pub show: Show,
    /// Programs that must be closed in this session, each with the reason.
    /// Close every running copy of each.
    pub close: Vec<CloseOrder>,
}

/// What LiMo's helper shows in a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "show")]
pub enum Show {
    /// Nothing: the account is not limited.
    Nothing,
    /// The session may be used: show the time left.
    TimeLeft {
        /// The best known time left today, in seconds. None when today has
        /// no time limit.
        seconds_left: Option<u64>,
        /// True when time is being counted now: false while nothing has been
        /// typed or moved for longer than the idle minutes and nothing plays
        /// (in full screen, with sound, or keeping the screen awake), while
        /// a program on the always allowed list is in front, and while the
        /// session is not the one in use.
        counting: bool,
        /// A warning to show now. It comes in one call only.
        warning: Option<WarningShown>,
        /// When a guardian's unlock keeps the PC open, the moment it ends,
        /// unix seconds. Time used meanwhile still counts.
        unlocked_until: Option<i64>,
    },
    /// The session is locked: cover the whole screen.
    Locked {
        /// What the lock screen shows.
        screen: LockScreen,
    },
}

/// What a lock screen shows, and the ways out it offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockScreen {
    /// Why it is locked.
    pub why: LockWhy,
    /// Why, in a few words, for the top of the lock screen.
    pub title: String,
    /// Why, in plain words.
    pub message: String,
    /// When use is possible again if nothing changes, unix seconds, when it
    /// is known.
    pub until: Option<i64>,
    /// True when the child may ask for more time.
    pub can_ask_for_time: bool,
    /// True when a one time code can be typed.
    pub can_use_code: bool,
    /// True when a guardian can type the PIN.
    pub can_use_pin: bool,
    /// When wrong guesses have set a wait before the next code or PIN try:
    /// the moment it ends, unix seconds.
    pub try_wait_until: Option<i64>,
    /// The child's newest request for more time today and its answer, so
    /// the lock screen can say what became of it.
    pub latest_request: Option<RequestView>,
}

/// Why a session is locked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockWhy {
    /// It is blocked hours, such as bedtime.
    BlockedHours,
    /// Today has no screen time at all.
    NoTimeToday,
    /// Today's time is used up.
    TimeUsedUp,
    /// The PC is getting more time from the family's book. This passes at
    /// once while the book can be read.
    WaitingForTime,
    /// The rest of today's time is on another device, such as a phone out of
    /// the house. It comes here when that device reports from home.
    HeldByAnotherDevice,
    /// A guardian locked the child's devices.
    GuardianLock,
    /// The account is linked to no child and not marked as not limited, and
    /// the guardian chose to lock such accounts.
    AccountNotLinked,
    /// LiMo cannot read the child's record in the family's file, so it
    /// cannot tell how much time is left.
    CannotCheck,
    /// A blocked program. The rules core gives this for the program in
    /// front only, never for the whole PC, and the engine closes such a
    /// program instead; it is here so that every reason has a name.
    ProgramBlocked,
}

impl LockWhy {
    /// The lock for a reason the rules core gives.
    pub(crate) fn from_core(reason: LockReason) -> LockWhy {
        match reason {
            LockReason::BlockedHours => LockWhy::BlockedHours,
            LockReason::NoTimeToday => LockWhy::NoTimeToday,
            LockReason::TimeUsedUp => LockWhy::TimeUsedUp,
            LockReason::NeedsMaster => LockWhy::WaitingForTime,
            LockReason::HeldByAnotherDevice => LockWhy::HeldByAnotherDevice,
            LockReason::GuardianLock => LockWhy::GuardianLock,
            LockReason::AppBlocked => LockWhy::ProgramBlocked,
        }
    }

    /// Why, in a few words, for the top of the lock screen.
    pub fn title(self) -> &'static str {
        match self {
            LockWhy::BlockedHours => "Screen time is off for now.",
            LockWhy::NoTimeToday => "No screen time today.",
            LockWhy::TimeUsedUp => "Today's time is used up.",
            LockWhy::WaitingForTime => "Getting more time.",
            LockWhy::HeldByAnotherDevice => "The rest is on another device.",
            LockWhy::GuardianLock => "Locked by a guardian.",
            LockWhy::AccountNotLinked => "This account is not set up yet.",
            LockWhy::CannotCheck => "LiMo cannot check the time.",
            LockWhy::ProgramBlocked => "This program is blocked.",
        }
    }

    /// Why, in plain words, for the lock screen.
    pub fn message(self) -> &'static str {
        match self {
            LockWhy::BlockedHours => "This is a time when the PC is not used.",
            LockWhy::NoTimeToday => "Today has no screen time.",
            LockWhy::TimeUsedUp => "Today's screen time is used up.",
            LockWhy::WaitingForTime => "LiMo is getting more time. This takes a moment.",
            LockWhy::HeldByAnotherDevice => {
                "The rest of today's time is on another device. It comes here when that device is back home."
            }
            LockWhy::GuardianLock => "A guardian has locked the PC for now.",
            LockWhy::AccountNotLinked => "This account is not set up in LiMo yet. A guardian can set it up.",
            LockWhy::CannotCheck => "LiMo cannot check the time left just now. Ask a guardian.",
            LockWhy::ProgramBlocked => "This program is blocked by your family's rules.",
        }
    }
}

/// A warning that a lock is coming.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WarningShown {
    /// What the lock will be for.
    pub cause: WarningCause,
    /// The warning point reached, in minutes. It tells one warning from
    /// another; the words say the time actually left, which can be less.
    pub minutes: u32,
    /// When the lock is expected, unix seconds, if use goes on.
    pub lock_at: i64,
    /// The warning in plain words.
    pub message: String,
}

impl WarningShown {
    /// The warning, with its words, given at `now`, unix seconds.
    ///
    /// The words say the time left until the lock, never more than there
    /// is: with 6 minutes a day, the first point reached is 15 minutes, and
    /// the warning says "6 minutes of screen time left today.". See
    /// `minutes_to_say`.
    pub(crate) fn new(cause: WarningCause, minutes: u32, lock_at: i64, now: i64) -> WarningShown {
        let said = minutes_to_say(minutes, lock_at.saturating_sub(now));
        let span = if said == 1 { "1 minute".to_string() } else { format!("{said} minutes") };
        let message = match cause {
            WarningCause::TimeRunsOut => format!("{span} of screen time left today."),
            WarningCause::BlockedHoursStart => format!("The PC locks in {span}, for the hours it is not used."),
            WarningCause::UnlockEnds => format!("The unlock ends in {span}, and the PC locks then."),
        };
        WarningShown { cause, minutes, lock_at, message }
    }
}

/// The minutes a warning says, for the warning point reached (`point`) with
/// `secs_left` seconds left until the lock: the time left in whole minutes,
/// rounded up, never less than 1 and never more than the point. When the
/// time left is the point, as it is on most days, that is the point itself.
fn minutes_to_say(point: u32, secs_left: i64) -> u32 {
    let secs = u64::try_from(secs_left).unwrap_or(0);
    let left = u32::try_from(secs.div_ceil(60)).unwrap_or(u32::MAX);
    left.min(point).max(1)
}

/// A program to close.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloseOrder {
    /// The program, as the facts gave it.
    pub program: Program,
    /// Why, in plain words, to show the child.
    pub reason: String,
}

impl CloseOrder {
    /// A blocked program to close.
    pub(crate) fn blocked(program: &Program) -> CloseOrder {
        CloseOrder {
            program: program.clone(),
            reason: format!("{} is blocked by your family's rules, so LiMo closed it.", program.shown_name()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVERY_WHY: [LockWhy; 9] = [
        LockWhy::BlockedHours,
        LockWhy::NoTimeToday,
        LockWhy::TimeUsedUp,
        LockWhy::WaitingForTime,
        LockWhy::HeldByAnotherDevice,
        LockWhy::GuardianLock,
        LockWhy::AccountNotLinked,
        LockWhy::CannotCheck,
        LockWhy::ProgramBlocked,
    ];

    #[test]
    fn every_lock_has_a_short_title_beside_its_message() {
        assert_eq!(LockWhy::BlockedHours.title(), "Screen time is off for now.");
        assert_eq!(LockWhy::NoTimeToday.title(), "No screen time today.");
        assert_eq!(LockWhy::TimeUsedUp.title(), "Today's time is used up.");
        assert_eq!(LockWhy::WaitingForTime.title(), "Getting more time.");
        assert_eq!(LockWhy::HeldByAnotherDevice.title(), "The rest is on another device.");
        assert_eq!(LockWhy::GuardianLock.title(), "Locked by a guardian.");
        assert_eq!(LockWhy::AccountNotLinked.title(), "This account is not set up yet.");
        assert_eq!(LockWhy::CannotCheck.title(), "LiMo cannot check the time.");
        assert_eq!(LockWhy::ProgramBlocked.title(), "This program is blocked.");
        for why in EVERY_WHY {
            let title = why.title();
            assert!(title.ends_with('.') && title.chars().count() <= 32, "{title}");
            assert_ne!(title, why.message());
        }
    }

    const NOW: i64 = 1_800_000_000;

    fn words(cause: WarningCause, point: u32, secs_left: i64) -> String {
        let warning = WarningShown::new(cause, point, NOW + secs_left, NOW);
        assert_eq!((warning.minutes, warning.lock_at), (point, NOW + secs_left), "the point and the lock stay as given");
        warning.message
    }

    #[test]
    fn a_warning_at_its_point_says_the_point() {
        assert_eq!(words(WarningCause::TimeRunsOut, 15, 15 * 60), "15 minutes of screen time left today.");
        assert_eq!(words(WarningCause::TimeRunsOut, 5, 5 * 60), "5 minutes of screen time left today.");
        assert_eq!(words(WarningCause::TimeRunsOut, 1, 60), "1 minute of screen time left today.");
        assert_eq!(words(WarningCause::BlockedHoursStart, 15, 15 * 60), "The PC locks in 15 minutes, for the hours it is not used.");
        assert_eq!(words(WarningCause::UnlockEnds, 1, 60), "The unlock ends in 1 minute, and the PC locks then.");
        // A part of a minute short of the point still rounds up to it.
        assert_eq!(words(WarningCause::TimeRunsOut, 15, 15 * 60 - 59), "15 minutes of screen time left today.");
        // More than the point (a clock set back meanwhile, say): the point.
        assert_eq!(words(WarningCause::TimeRunsOut, 5, 9 * 60), "5 minutes of screen time left today.");
    }

    #[test]
    fn a_warning_short_of_its_point_says_the_time_actually_left() {
        // Each of the three causes, at the 15 minute point with 6 left.
        assert_eq!(words(WarningCause::TimeRunsOut, 15, 6 * 60), "6 minutes of screen time left today.");
        assert_eq!(words(WarningCause::BlockedHoursStart, 15, 6 * 60), "The PC locks in 6 minutes, for the hours it is not used.");
        assert_eq!(words(WarningCause::UnlockEnds, 15, 6 * 60), "The unlock ends in 6 minutes, and the PC locks then.");
        // Whole minutes, rounded up: 5 minutes and a second says 6.
        assert_eq!(words(WarningCause::TimeRunsOut, 15, 5 * 60 + 1), "6 minutes of screen time left today.");
        assert_eq!(words(WarningCause::TimeRunsOut, 15, 10 * 60), "10 minutes of screen time left today.");
        // One minute or less is "1 minute", never "0 minutes".
        assert_eq!(words(WarningCause::TimeRunsOut, 5, 42), "1 minute of screen time left today.");
        assert_eq!(words(WarningCause::UnlockEnds, 15, 1), "The unlock ends in 1 minute, and the PC locks then.");
        assert_eq!(words(WarningCause::BlockedHoursStart, 5, 0), "The PC locks in 1 minute, for the hours it is not used.");
        assert_eq!(words(WarningCause::TimeRunsOut, 5, -30), "1 minute of screen time left today.");
    }

    #[test]
    fn any_numbers_give_sound_words_and_no_panic() {
        for (point, lock_at, now) in [
            (0, NOW, NOW),
            (u32::MAX, i64::MAX, i64::MIN),
            (u32::MAX, i64::MIN, i64::MAX),
            (15, i64::MAX, 0),
            (1, i64::MIN, 0),
        ] {
            let warning = WarningShown::new(WarningCause::TimeRunsOut, point, lock_at, now);
            assert!(warning.message.ends_with(" of screen time left today."), "{}", warning.message);
            assert!(!warning.message.starts_with("0 "), "{}", warning.message);
        }
        assert_eq!(minutes_to_say(u32::MAX, i64::MAX), u32::MAX);
        assert_eq!(minutes_to_say(0, 600), 1);
    }
}
