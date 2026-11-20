//! Every conversion between the engine's and the link's own values and the
//! shapes that cross to Kotlin, and the few checks a value needs before it
//! can be handed on (a number below zero where none can be, a field missing
//! for its kind). Nothing here decides a rule: each value is carried as it
//! is, field by field.
//!
//! No UniFFI macro expands in this module, so it forbids unsafe code.

#![forbid(unsafe_code)]

use limo_core::{Grant, Report};
use limo_link::{LinkError as Link, PhoneEvent, Refusal};
use limo_phone as engine;
use serde::Serialize;

use crate::shapes::{
    AppEntry, BlockCover, CallState, Cover, CoverKind, EngineError, Front, FrontKind, GrantOutcome, GrantTaken,
    LinkError, LinkEvent, LinkEventKind, LoadProblem, LoadProblemKind, LockCover, LockWhy, NothingToDo, PhoneFacts,
    PhoneOrders, PhoneView, PinAction, PinActionKind, RequestAnswer, RequestLine, RequestShown, TimeLeft,
    TimeRequest, TryAnswer, TryKind, WarningCause, WarningShown, Way, WayRow,
};

// ---------------------------------------------------------------------------
// Numbers. Kotlin has no unsigned numbers in daily use, so they cross as
// `Long` and `Int`. Going in, a number below zero where none can be is
// refused; coming out, a number too big for its Kotlin type (none is, in
// practice: they are seconds and minutes) is held at the largest one.

/// A `u64` as a Kotlin `Long`.
pub(crate) fn long(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// A `u32` as a Kotlin `Int`.
pub(crate) fn int(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

/// A count from Kotlin, never below zero.
pub(crate) fn count_in(value: i32, what: &'static str) -> Result<u32, &'static str> {
    u32::try_from(value).map_err(|_| what)
}

/// A count of milliseconds from Kotlin, never below zero.
pub(crate) fn ms_in(value: i64, what: &'static str) -> Result<u64, &'static str> {
    u64::try_from(value).map_err(|_| what)
}

/// Bytes typed by a person (a code or a PIN) as text, borrowed: no copy is
/// made. Kotlin hands them over as UTF-8.
pub(crate) fn typed<'a>(bytes: &'a [u8], what: &'static str) -> Result<&'a str, EngineError> {
    std::str::from_utf8(bytes).map_err(|_| EngineError::OutOfRange { what })
}

// ---------------------------------------------------------------------------
// JSON: the report and the grant cross as the bytes the other crates write,
// so Kotlin carries them without reading them.

/// A value as JSON bytes.
pub(crate) fn to_json<T: Serialize>(value: &T) -> Option<Vec<u8>> {
    serde_json::to_vec(value).ok()
}

/// A value as JSON text.
pub(crate) fn to_json_text<T: Serialize>(value: &T) -> Option<String> {
    serde_json::to_string(value).ok()
}

/// A grant from JSON bytes.
pub(crate) fn grant_from(bytes: &[u8]) -> Option<Grant> {
    serde_json::from_slice(bytes).ok()
}

/// A report from JSON bytes.
pub(crate) fn report_from(bytes: &[u8]) -> Option<Report> {
    serde_json::from_slice(bytes).ok()
}

// ---------------------------------------------------------------------------
// The facts, in.

/// The facts as the engine takes them.
pub(crate) fn facts_in(facts: PhoneFacts) -> Result<engine::PhoneFacts, EngineError> {
    let out_of_range = |what| EngineError::OutOfRange { what };
    Ok(engine::PhoneFacts {
        wall_secs: facts.wall_secs,
        elapsed_ms: ms_in(facts.elapsed_ms, "the elapsed count is below zero").map_err(out_of_range)?,
        boot_count: count_in(facts.boot_count, "the boot count is below zero").map_err(out_of_range)?,
        utc_offset_secs: facts.utc_offset_secs,
        hours_24: facts.hours_24,
        auto_time: facts.auto_time,
        interactive: facts.interactive,
        keyguard_locked: facts.keyguard_locked,
        call: match facts.call {
            CallState::Idle => engine::CallState::Idle,
            CallState::Ringing => engine::CallState::Ringing,
            CallState::Ongoing => engine::CallState::Ongoing,
        },
        front: front_in(facts.front)?,
        in_reach: facts.in_reach,
        prodded: facts.prodded,
        permissions: engine::PhonePermissions {
            notifications: facts.permissions.notifications,
            phone: facts.permissions.phone,
            usage_access: facts.permissions.usage_access,
            accessibility: facts.permissions.accessibility,
            battery: facts.permissions.battery,
            protection: facts.permissions.protection,
        },
    })
}

/// What is in front, as the engine takes it. An app needs its package; the
/// other kinds ignore the two fields.
pub(crate) fn front_in(front: Front) -> Result<engine::Front, EngineError> {
    Ok(match front.kind {
        FrontKind::Unknown => engine::Front::Unknown,
        FrontKind::Home => engine::Front::Home,
        FrontKind::Limo => engine::Front::LiMo,
        FrontKind::Dialler => engine::Front::Dialler,
        FrontKind::EmergencyDialler => engine::Front::EmergencyDialler,
        FrontKind::App => engine::Front::App {
            package: front.package_name.ok_or(EngineError::OutOfRange { what: "an app in front has no package name" })?,
            label: front.label,
        },
    })
}

/// A guardian's wish with the PIN, as the core takes it.
pub(crate) fn pin_action_in(action: PinAction) -> Result<limo_core::PinAction, EngineError> {
    Ok(match action.kind {
        PinActionKind::AddMinutes => {
            let minutes = action.minutes.ok_or(EngineError::OutOfRange { what: "minutes to add are missing" })?;
            limo_core::PinAction::AddMinutes {
                minutes: count_in(minutes, "the minutes to add are below zero")
                    .map_err(|what| EngineError::OutOfRange { what })?,
            }
        }
        PinActionKind::UnlockUntil => limo_core::PinAction::UnlockUntil {
            until: action.until.ok_or(EngineError::OutOfRange { what: "the end of the unlock is missing" })?,
        },
        PinActionKind::ConfirmRelease => limo_core::PinAction::ConfirmRelease,
    })
}

// ---------------------------------------------------------------------------
// The orders, out.

/// The orders as they cross.
pub(crate) fn orders_out(orders: engine::PhoneOrders) -> Result<PhoneOrders, EngineError> {
    let notes = orders
        .notes
        .iter()
        .map(|note| to_json_text(note).ok_or(EngineError::Internal { what: "write a note as JSON" }))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PhoneOrders {
        cover: cover_out(orders.cover),
        time_left: time_left_out(orders.time_left),
        warning: orders.warning.map(warning_out),
        send_report: orders.send_report,
        save_state: orders.save_state,
        notes,
        next_check_in_secs: long(orders.next_check_in_secs),
    })
}

fn cover_out(cover: engine::Cover) -> Cover {
    match cover {
        engine::Cover::Nothing => Cover { kind: CoverKind::Nothing, block: None, lock: None },
        engine::Cover::Block { package, title, message, go_home, call_on, back_to_call } => Cover {
            kind: CoverKind::Block,
            block: Some(BlockCover { package_name: package, title, message, go_home, call_on, back_to_call }),
            lock: None,
        },
        engine::Cover::Lock(screen) => Cover { kind: CoverKind::Lock, block: None, lock: Some(lock_out(*screen)) },
    }
}

fn lock_out(screen: engine::LockScreen) -> LockCover {
    LockCover {
        why: why_out(screen.why),
        stamp: screen.stamp,
        title: screen.title,
        message: screen.message,
        until: screen.until,
        until_words: screen.until_words,
        allowed_packages: screen.allowed_packages,
        can_ask_for_time: screen.can_ask_for_time,
        can_use_code: screen.can_use_code,
        can_use_pin: screen.can_use_pin,
        try_wait_until: screen.try_wait_until,
        wait_words: screen.wait_words,
        latest_request: screen.latest_request.map(request_view_out),
        request_line: screen.request_line.map(|line| RequestLine { text: line.text, stamp: line.stamp }),
        ways: screen.ways.into_iter().map(way_row_out).collect(),
        nothing_to_do: screen.nothing_to_do.map(|(bold, line)| NothingToDo { bold, line }),
        call_on: screen.call_on,
        back_to_call: screen.back_to_call,
    }
}

fn why_out(why: engine::LockWhy) -> LockWhy {
    match why {
        engine::LockWhy::BlockedHours => LockWhy::BlockedHours,
        engine::LockWhy::NoTimeToday => LockWhy::NoTimeToday,
        engine::LockWhy::TimeUsedUp => LockWhy::TimeUsedUp,
        engine::LockWhy::WaitingForTime => LockWhy::WaitingForTime,
        engine::LockWhy::AwayTimeUsedUp => LockWhy::AwayTimeUsedUp,
        engine::LockWhy::HeldByAnotherDevice => LockWhy::HeldByAnotherDevice,
        engine::LockWhy::GuardianLock => LockWhy::GuardianLock,
        engine::LockWhy::CannotCheck => LockWhy::CannotCheck,
        engine::LockWhy::AppBlocked => LockWhy::AppBlocked,
    }
}

fn way_row_out(row: engine::WayRow) -> WayRow {
    WayRow {
        way: match row.way {
            engine::Way::Ask => Way::Ask,
            engine::Way::Code => Way::Code,
            engine::Way::Pin => Way::Pin,
        },
        title: row.title,
        sub: row.sub,
        off: row.off,
    }
}

fn request_view_out(view: limo_core::RequestView) -> RequestShown {
    let (answer, granted_minutes) = match view.answer {
        limo_core::RequestAnswer::Waiting => (RequestAnswer::Waiting, None),
        limo_core::RequestAnswer::Granted { minutes } => (RequestAnswer::Granted, Some(int(minutes))),
        limo_core::RequestAnswer::Refused => (RequestAnswer::Refused, None),
    };
    RequestShown {
        request: request_out(view.request),
        answer,
        granted_minutes,
        answered_at: view.answered_at,
        reached_master: view.reached_master,
    }
}

/// A request for more time as it crosses.
pub(crate) fn request_out(request: limo_core::TimeRequest) -> TimeRequest {
    TimeRequest {
        id: request.id,
        child_id: request.child_id,
        device_id: request.device_id,
        day: request.day,
        minutes: int(request.minutes),
        reason: request.reason,
        asked_at: request.asked_at,
    }
}

fn time_left_out(left: engine::TimeLeft) -> TimeLeft {
    TimeLeft {
        seconds_left: left.seconds_left.map(long),
        counting: left.counting,
        unlocked_until: left.unlocked_until,
        away: left.away,
        locked: left.locked,
        words: left.words,
    }
}

fn warning_out(warning: engine::WarningShown) -> WarningShown {
    WarningShown {
        cause: match warning.cause {
            limo_core::WarningCause::TimeRunsOut => WarningCause::TimeRunsOut,
            limo_core::WarningCause::BlockedHoursStart => WarningCause::BlockedHoursStart,
            limo_core::WarningCause::UnlockEnds => WarningCause::UnlockEnds,
        },
        minutes: int(warning.minutes),
        lock_at: warning.lock_at,
        away: warning.away,
        message: warning.message,
        hint: warning.hint,
        when_words: warning.when,
    }
}

// ---------------------------------------------------------------------------
// The other answers, out.

/// The answer to a code or a PIN as it crosses.
pub(crate) fn try_out(answer: engine::TryAnswer) -> TryAnswer {
    let mut out = TryAnswer {
        kind: TryKind::Malformed,
        message: answer.message,
        opens_now: answer.opens_now,
        added_secs: None,
        unlocked_until: None,
        wrong_guesses: None,
        wait_until: None,
    };
    out.kind = match answer.outcome {
        limo_core::TryOutcome::Added { added } => {
            out.added_secs = Some(long(added.secs));
            TryKind::Added
        }
        limo_core::TryOutcome::Unlocked { until } => {
            out.unlocked_until = Some(until);
            TryKind::Unlocked
        }
        limo_core::TryOutcome::ReleaseConfirmed => TryKind::ReleaseConfirmed,
        limo_core::TryOutcome::Wrong { wrong_guesses, wait_until } => {
            out.wrong_guesses = Some(int(wrong_guesses));
            out.wait_until = wait_until;
            TryKind::Wrong
        }
        limo_core::TryOutcome::MustWait { until } => {
            out.wait_until = Some(until);
            TryKind::MustWait
        }
        limo_core::TryOutcome::Malformed => TryKind::Malformed,
        limo_core::TryOutcome::AlreadyUsed => TryKind::AlreadyUsed,
        limo_core::TryOutcome::NotSet => TryKind::NotSet,
    };
    out
}

/// What became of a grant, as it crosses.
pub(crate) fn grant_taken_out(taken: engine::GrantTaken) -> GrantTaken {
    GrantTaken {
        outcome: match taken.outcome {
            limo_core::GrantOutcome::Taken => GrantOutcome::Taken,
            limo_core::GrantOutcome::Stale => GrantOutcome::Stale,
            limo_core::GrantOutcome::NotForThisDevice => GrantOutcome::NotForThisDevice,
        },
        report_again: taken.report_again,
    }
}

/// Why a kept state could not be loaded, as it crosses.
pub(crate) fn load_problem_out(problem: engine::LoadProblem) -> LoadProblem {
    let (kind, format) = match problem {
        engine::LoadProblem::Empty => (LoadProblemKind::Empty, None),
        engine::LoadProblem::NotLiMo => (LoadProblemKind::NotLimo, None),
        engine::LoadProblem::Newer { format } => (LoadProblemKind::Newer, Some(int(format))),
        engine::LoadProblem::Damaged => (LoadProblemKind::Damaged, None),
    };
    LoadProblem { kind, format, message: problem.to_string() }
}

/// The view as it crosses: a few numbers, and the whole of it as JSON.
pub(crate) fn view_out(view: engine::PhoneView) -> Result<PhoneView, EngineError> {
    let json = to_json_text(&view).ok_or(EngineError::Internal { what: "write the view as JSON" })?;
    Ok(PhoneView {
        child_id: view.child_id,
        device_id: view.device_id,
        day: view.day,
        counted_secs: long(view.counted_secs),
        over_secs: long(view.over_secs),
        open_secs: long(view.open_secs),
        unreported_slices: i32::try_from(view.unreported.len()).unwrap_or(i32::MAX),
        notes_kept: i32::try_from(view.notes.len()).unwrap_or(i32::MAX),
        clock_correction_secs: view.clock_correction_secs,
        json,
    })
}

/// The engine's refusals, each to its own variant, with its words.
impl From<engine::PhoneError> for EngineError {
    fn from(error: engine::PhoneError) -> EngineError {
        let message = error.to_string();
        match error {
            engine::PhoneError::CannotCheck => EngineError::CannotCheck { message },
            engine::PhoneError::Refused { problem } => match problem {
                limo_core::DeviceError::SecretWrongLength { .. } => EngineError::SecretWrongLength { message },
                limo_core::DeviceError::MinutesOutOfRange { .. } => EngineError::MinutesOutOfRange { message },
                limo_core::DeviceError::ReasonTooLong { .. } => EngineError::ReasonTooLong { message },
                limo_core::DeviceError::NoLimitToday => EngineError::NoLimitToday { message },
                limo_core::DeviceError::RequestWaiting => EngineError::RequestWaiting { message },
                limo_core::DeviceError::TooManyRequests => EngineError::TooManyRequests { message },
                limo_core::DeviceError::UnlockOutOfRange => EngineError::UnlockOutOfRange { message },
            },
        }
    }
}

// ---------------------------------------------------------------------------
// The link.

/// What came from the master, as it crosses: a grant as JSON bytes.
pub(crate) fn event_out(event: PhoneEvent) -> Result<LinkEvent, LinkError> {
    Ok(match event {
        PhoneEvent::Opened => LinkEvent { kind: LinkEventKind::Opened, grant: None, master_now: None },
        PhoneEvent::Grant { grant, master_now } => LinkEvent {
            kind: LinkEventKind::Grant,
            grant: Some(
                to_json(&*grant)
                    .ok_or_else(|| LinkError::Internal { message: "LiMo could not write a grant as JSON.".into() })?,
            ),
            master_now: Some(master_now),
        },
        PhoneEvent::ReportNow => LinkEvent { kind: LinkEventKind::ReportNow, grant: None, master_now: None },
        PhoneEvent::Pong { master_now } => {
            LinkEvent { kind: LinkEventKind::Pong, grant: None, master_now: Some(master_now) }
        }
    })
}

/// An app on the phone, as the link takes it.
pub(crate) fn app_in(app: AppEntry) -> limo_link::AppEntry {
    limo_link::AppEntry { package: app.package_name, label: app.label }
}

/// The link's errors, each to its own variant, with its words.
impl From<Link> for LinkError {
    fn from(error: Link) -> LinkError {
        let message = error.to_string();
        match error {
            Link::FrameTooLong { .. } => LinkError::FrameTooLong { message },
            Link::WrongSize { .. } => LinkError::WrongSize { message },
            Link::BadLength { .. } => LinkError::BadLength { message },
            Link::NotLiMo => LinkError::NotLimo { message },
            Link::BadVersion => LinkError::BadVersion { message },
            Link::Garbled => LinkError::Garbled { message },
            Link::UnknownOffer => LinkError::UnknownOffer { message },
            Link::UnknownPhone => LinkError::UnknownPhone { message },
            Link::WrongMaster => LinkError::WrongMaster { message },
            Link::Malformed { .. } => LinkError::Malformed { message },
            Link::Unexpected { .. } => LinkError::Unexpected { message },
            Link::OutOfBounds { .. } => LinkError::OutOfBounds { message },
            Link::OtherDevice => LinkError::OtherDevice { message },
            Link::Refused(why) => match why {
                Refusal::Removed => LinkError::RefusedRemoved { message },
                Refusal::Unknown => LinkError::RefusedUnknown { message },
                Refusal::OtherDevice => LinkError::RefusedOtherDevice { message },
                Refusal::Version => LinkError::RefusedVersion { message },
                Refusal::OfferUsed => LinkError::RefusedOfferUsed { message },
                Refusal::OfferExpired => LinkError::RefusedOfferExpired { message },
                Refusal::NotSupported => LinkError::RefusedNotSupported { message },
            },
            Link::HandshakeTooSlow => LinkError::HandshakeTooSlow { message },
            Link::NoAnswer { report_waiting: true } => LinkError::NoAnswerToReport { message },
            Link::NoAnswer { report_waiting: false } => LinkError::NoAnswer { message },
            Link::Silent => LinkError::Silent { message },
            Link::TooManyWaiting => LinkError::TooManyWaiting { message },
            Link::MessageTooLong { .. } => LinkError::MessageTooLong { message },
            Link::NotOpen => LinkError::NotOpen { message },
            Link::Closed => LinkError::Closed { message },
            Link::NoRandomness => LinkError::NoRandomness { message },
            Link::BadOffer(_) => LinkError::BadOffer { message },
            Link::Internal { .. } => LinkError::Internal { message },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::PhonePermissions;

    fn facts(front: Front) -> PhoneFacts {
        PhoneFacts {
            wall_secs: 1_800_000_000,
            elapsed_ms: 3_600_000,
            boot_count: 7,
            utc_offset_secs: 19_800,
            hours_24: true,
            auto_time: false,
            interactive: true,
            keyguard_locked: true,
            call: CallState::Ringing,
            front,
            in_reach: true,
            prodded: true,
            permissions: PhonePermissions {
                notifications: true,
                phone: false,
                usage_access: false,
                accessibility: true,
                battery: false,
                protection: true,
            },
        }
    }

    fn front(kind: FrontKind, package_name: Option<&str>, label: Option<&str>) -> Front {
        Front { kind, package_name: package_name.map(str::to_string), label: label.map(str::to_string) }
    }

    #[test]
    fn the_facts_cross_field_by_field() {
        let crossed = facts_in(facts(front(FrontKind::App, Some("org.example.game"), Some("Game")))).unwrap();
        assert_eq!(
            crossed,
            engine::PhoneFacts {
                wall_secs: 1_800_000_000,
                elapsed_ms: 3_600_000,
                boot_count: 7,
                utc_offset_secs: 19_800,
                hours_24: true,
                auto_time: false,
                interactive: true,
                keyguard_locked: true,
                call: engine::CallState::Ringing,
                front: engine::Front::labelled("org.example.game", "Game"),
                in_reach: true,
                prodded: true,
                permissions: engine::PhonePermissions {
                    notifications: true,
                    phone: false,
                    usage_access: false,
                    accessibility: true,
                    battery: false,
                    protection: true,
                },
            }
        );
        // Each permission crosses to its own field, and only to it.
        for permission in engine::Permission::ALL {
            let mut f = facts(front(FrontKind::Home, None, None));
            let p = &mut f.permissions;
            *p = PhonePermissions {
                notifications: true,
                phone: true,
                usage_access: true,
                accessibility: true,
                battery: true,
                protection: true,
            };
            match permission {
                engine::Permission::Notifications => p.notifications = false,
                engine::Permission::Phone => p.phone = false,
                engine::Permission::UsageAccess => p.usage_access = false,
                engine::Permission::Accessibility => p.accessibility = false,
                engine::Permission::Battery => p.battery = false,
                engine::Permission::Protection => p.protection = false,
            }
            assert_eq!(facts_in(f).unwrap().permissions.off(), vec![permission]);
        }
        for (call, expected) in [
            (CallState::Idle, engine::CallState::Idle),
            (CallState::Ringing, engine::CallState::Ringing),
            (CallState::Ongoing, engine::CallState::Ongoing),
        ] {
            let mut f = facts(front(FrontKind::Home, None, None));
            f.call = call;
            assert_eq!(facts_in(f).unwrap().call, expected);
        }
    }

    #[test]
    fn every_kind_of_front_crosses_and_an_app_needs_its_package() {
        let cases = [
            (FrontKind::Unknown, engine::Front::Unknown),
            (FrontKind::Home, engine::Front::Home),
            (FrontKind::Limo, engine::Front::LiMo),
            (FrontKind::Dialler, engine::Front::Dialler),
            (FrontKind::EmergencyDialler, engine::Front::EmergencyDialler),
        ];
        for (kind, expected) in cases {
            // The two fields mean nothing for these kinds, and are left.
            assert_eq!(front_in(front(kind, Some("org.example.game"), Some("Game"))).unwrap(), expected);
            assert_eq!(front_in(front(kind, None, None)).unwrap(), expected);
        }
        assert_eq!(
            front_in(front(FrontKind::App, Some("org.example.game"), None)).unwrap(),
            engine::Front::app("org.example.game")
        );
        assert!(matches!(front_in(front(FrontKind::App, None, Some("Game"))), Err(EngineError::OutOfRange { .. })));
    }

    #[test]
    fn numbers_below_zero_are_refused_going_in_and_held_at_the_largest_coming_out() {
        let mut f = facts(front(FrontKind::Home, None, None));
        f.elapsed_ms = -1;
        assert!(matches!(facts_in(f), Err(EngineError::OutOfRange { .. })));
        let mut f = facts(front(FrontKind::Home, None, None));
        f.boot_count = -1;
        assert!(matches!(facts_in(f), Err(EngineError::OutOfRange { .. })));
        assert_eq!(long(u64::MAX), i64::MAX);
        assert_eq!(long(42), 42);
        assert_eq!(int(u32::MAX), i32::MAX);
        assert_eq!(count_in(0, "x"), Ok(0));
        assert_eq!(count_in(-1, "x"), Err("x"));
        assert_eq!(ms_in(i64::MAX, "x"), Ok(i64::MAX.unsigned_abs()));
    }

    #[test]
    fn a_pin_action_crosses_with_what_its_kind_needs() {
        let action = |kind, minutes, until| PinAction { kind, minutes, until };
        assert_eq!(
            pin_action_in(action(PinActionKind::AddMinutes, Some(20), None)).unwrap(),
            limo_core::PinAction::AddMinutes { minutes: 20 }
        );
        assert_eq!(
            pin_action_in(action(PinActionKind::UnlockUntil, None, Some(1_800_003_600))).unwrap(),
            limo_core::PinAction::UnlockUntil { until: 1_800_003_600 }
        );
        assert_eq!(
            pin_action_in(action(PinActionKind::ConfirmRelease, Some(5), Some(5))).unwrap(),
            limo_core::PinAction::ConfirmRelease
        );
        for bad in [
            action(PinActionKind::AddMinutes, None, Some(1)),
            action(PinActionKind::AddMinutes, Some(-5), None),
            action(PinActionKind::UnlockUntil, Some(20), None),
        ] {
            assert!(matches!(pin_action_in(bad), Err(EngineError::OutOfRange { .. })));
        }
    }

    #[test]
    fn every_refusal_of_the_engine_has_its_own_variant_and_its_words() {
        let refusals = [
            limo_core::DeviceError::SecretWrongLength { given: 3 },
            limo_core::DeviceError::MinutesOutOfRange { minutes: 0, most: 120 },
            limo_core::DeviceError::ReasonTooLong { chars: 500 },
            limo_core::DeviceError::NoLimitToday,
            limo_core::DeviceError::RequestWaiting,
            limo_core::DeviceError::TooManyRequests,
            limo_core::DeviceError::UnlockOutOfRange,
        ];
        let mut seen = Vec::new();
        for problem in refusals {
            let words = problem.to_string();
            let crossed = EngineError::from(engine::PhoneError::Refused { problem });
            assert_eq!(crossed.to_string(), words);
            seen.push(format!("{:?}", std::mem::discriminant(&crossed)));
        }
        let cannot = EngineError::from(engine::PhoneError::CannotCheck);
        assert_eq!(cannot.to_string(), engine::PhoneError::CannotCheck.to_string());
        seen.push(format!("{:?}", std::mem::discriminant(&cannot)));
        let mut unique = seen.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), seen.len(), "two refusals share a variant");
    }

    #[test]
    fn every_error_of_the_link_has_its_own_variant_and_its_words() {
        let errors = [
            Link::FrameTooLong { said: 9, most: 8 },
            Link::WrongSize { step: "pairing reply", said: 3, expected: 96 },
            Link::BadLength { said: 20 },
            Link::NotLiMo,
            Link::BadVersion,
            Link::Garbled,
            Link::UnknownOffer,
            Link::UnknownPhone,
            Link::WrongMaster,
            Link::Malformed { line: 1, column: 2 },
            Link::Unexpected { what: "a pong nobody asked for" },
            Link::OutOfBounds { what: "an empty name" },
            Link::OtherDevice,
            Link::Refused(Refusal::Removed),
            Link::Refused(Refusal::Unknown),
            Link::Refused(Refusal::OtherDevice),
            Link::Refused(Refusal::Version),
            Link::Refused(Refusal::OfferUsed),
            Link::Refused(Refusal::OfferExpired),
            Link::Refused(Refusal::NotSupported),
            Link::HandshakeTooSlow,
            Link::NoAnswer { report_waiting: true },
            Link::NoAnswer { report_waiting: false },
            Link::Silent,
            Link::TooManyWaiting,
            Link::MessageTooLong { bytes: 2_000_000 },
            Link::NotOpen,
            Link::Closed,
            Link::NoRandomness,
            Link::BadOffer(limo_link::OfferProblem::WrongLength),
            Link::Internal { what: "finish a handshake" },
        ];
        let mut seen = Vec::new();
        for error in errors {
            let words = error.to_string();
            let crossed = LinkError::from(error);
            assert_eq!(crossed.to_string(), words);
            seen.push(format!("{:?}", std::mem::discriminant(&crossed)));
        }
        let mut unique = seen.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), seen.len(), "two errors of the link share a variant");
        assert!(matches!(LinkError::from(Link::NoAnswer { report_waiting: true }), LinkError::NoAnswerToReport { .. }));
    }

    #[test]
    fn json_that_is_not_a_grant_or_a_report_is_not_taken_for_one() {
        assert!(grant_from(b"").is_none());
        assert!(grant_from(b"{}").is_none());
        assert!(grant_from(b"not json").is_none());
        assert!(report_from(b"{\"child_id\":\"c\"}").is_none());
    }

    #[test]
    fn typed_bytes_must_be_text() {
        assert_eq!(typed(b"1342 0715", "x").unwrap(), "1342 0715");
        assert!(matches!(typed(&[0xff, 0xfe], "x"), Err(EngineError::OutOfRange { what: "x" })));
    }
}
