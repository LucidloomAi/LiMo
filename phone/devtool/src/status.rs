//! What the phone holds, read from LiMo's kept state by the phone's own
//! engine, here on the PC.
//!
//! **How `status` knows whether the phone is locked now.** A state that was
//! just loaded has made no decision yet: its view says the child, the rules,
//! the use and each app's minutes as they were kept. So `status` asks the
//! engine one more question, on a copy that stays on this PC: what it
//! decides at the phone's own clock now, with the screen off. The facts it
//! gives say a start of the phone that never was, so the step since the
//! kept moment counts nothing, and the screen is off, so nothing is
//! counted, covered or warned of. Nothing it works out goes back to the
//! phone. The phone's own answer can differ by what it counted since it
//! last kept its state, at most a few seconds while it is in use.

use limo_core::{HomeCalendar, LockReason, RequestAnswer, Weekday};
use limo_phone::{CallState, Front, LoadProblem, Permission, PhoneEngine, PhoneFacts, PhonePermissions, PhoneView};

use crate::family::Family;

/// Whose state the phone keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Whose {
    /// This PC's test family's phone.
    Ours,
    /// A family this PC did not make.
    Other {
        /// The device the state is for.
        device_id: String,
    },
    /// A state LiMo cannot read, and why.
    Unreadable(String),
}

/// Words for a state that cannot be read: the engine's own sentence.
pub fn problem_words(problem: LoadProblem) -> String {
    problem.to_string()
}

/// Whose state these bytes are.
pub fn whose(bytes: &[u8], family: Option<&Family>) -> Whose {
    let engine = PhoneEngine::load(bytes);
    if let Some(problem) = engine.load_problem() {
        return Whose::Unreadable(problem_words(problem));
    }
    let Some(view) = engine.view() else { return Whose::Unreadable("it gave no view".to_string()) };
    if family.and_then(Family::device_id) == Some(view.device_id.as_str()) {
        Whose::Ours
    } else {
        Whose::Other { device_id: view.device_id }
    }
}

/// "today at 21:00", "tomorrow at 07:00", "Fri 27 Nov at 07:00", on the
/// phone's clock.
pub fn when_text(now: i64, at: i64, utc_offset_secs: i32) -> String {
    let calendar = HomeCalendar::new(utc_offset_secs, 0);
    let local = calendar.local(at);
    let days = calendar.date_of(at) - calendar.date_of(now);
    let time = format!("{:02}:{:02}", local.hour, local.minute);
    match days {
        0 => format!("today at {time}"),
        1 => format!("tomorrow at {time}"),
        _ => {
            let months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
            let month = months.get(usize::from(local.date.month).saturating_sub(1)).copied().unwrap_or("?");
            format!("{} {} {month} at {time}", short_day(local.weekday), local.date.day)
        }
    }
}

fn short_day(day: Weekday) -> &'static str {
    match day {
        Weekday::Monday => "Mon",
        Weekday::Tuesday => "Tue",
        Weekday::Wednesday => "Wed",
        Weekday::Thursday => "Thu",
        Weekday::Friday => "Fri",
        Weekday::Saturday => "Sat",
        Weekday::Sunday => "Sun",
    }
}

/// Why the phone is locked, in a few words.
fn reason_words(reason: LockReason) -> &'static str {
    match reason {
        LockReason::AppBlocked => "an app is blocked",
        LockReason::BlockedHours => "the hours screens are off",
        LockReason::NoTimeToday => "today has no screen time",
        LockReason::TimeUsedUp => "today's time is used up",
        LockReason::NeedsMaster => "the away time is used up",
        LockReason::HeldByAnotherDevice => "the rest of the time is on another device",
        LockReason::GuardianLock => "a guardian locked it",
    }
}

/// A permission in the words a person reads, as the phone's setup names
/// it. Every permission is named here, with no catch all, so a permission
/// the engine gains does not build until it has its words.
pub fn permission_words(permission: Permission) -> &'static str {
    match permission {
        Permission::Notifications => "notifications",
        Permission::Phone => "phone",
        Permission::UsageAccess => "usage access",
        Permission::Accessibility => "accessibility",
        Permission::Battery => "battery",
        Permission::Protection => "protection",
    }
}

/// The line for what LiMo last saw switched off, in words, in the order
/// setup asks for them: "Switched off on the phone when LiMo last looked:
/// usage access, accessibility."; or that nothing was.
pub fn switched_off_line(off: &[Permission]) -> String {
    let names: Vec<&str> = Permission::ALL.into_iter().filter(|p| off.contains(p)).map(permission_words).collect();
    if names.is_empty() {
        "Nothing LiMo needs was switched off when it last looked.".to_string()
    } else {
        format!("Switched off on the phone when LiMo last looked: {}.", names.join(", "))
    }
}

/// The permissions a view says are switched off, the others on.
fn permissions_of(view: &PhoneView) -> PhonePermissions {
    let mut p = PhonePermissions::ALL;
    for off in view.today.guardian_needed.iter().flat_map(|g| g.off.iter()) {
        match off.permission {
            Permission::Notifications => p.notifications = false,
            Permission::Phone => p.phone = false,
            Permission::UsageAccess => p.usage_access = false,
            Permission::Accessibility => p.accessibility = false,
            Permission::Battery => p.battery = false,
            Permission::Protection => p.protection = false,
        }
    }
    p
}

/// What `status` prints of a kept state: never a PIN, a code or the code
/// secret, which the engine's view does not carry; never a reason a child
/// typed. `clock` is the phone's clock and zone, when they could be read;
/// `pc_now` stands in for the clock when they could not.
pub fn status_lines(bytes: &[u8], family: Option<&Family>, clock: Option<(i64, i32)>, pc_now: i64) -> Vec<String> {
    let mut lines = Vec::new();
    let mut engine = PhoneEngine::load(bytes);
    if let Some(problem) = engine.load_problem() {
        lines.push(format!(
            "LiMo's state on the phone cannot be read. {} LiMo then shows a lock that says it cannot check the time \
             left, with no way out on the phone, until it is paired again. forget cannot tell whose it is; uninstall \
             removes it with LiMo.",
            problem_words(problem)
        ));
        return lines;
    }
    let Some(kept) = engine.view() else { return lines };
    match family.filter(|f| f.device_id() == Some(kept.device_id.as_str())) {
        Some(family) => lines.push(format!("Child: {} (this PC's test family), on its {}.", family.child, crate::family::PHONE_NAME.to_lowercase())),
        None => lines.push(format!(
            "This phone holds a family this PC did not make: child {}, device {}. Only what its state says is shown.",
            kept.child_id, kept.device_id
        )),
    }

    let (now, offset) = clock.unwrap_or((pc_now, kept.rules.utc_offset_secs));
    if clock.is_none() {
        lines.push("The phone's clock could not be read, so this PC's clock stands in for it.".to_string());
    }
    let facts = PhoneFacts {
        wall_secs: now,
        elapsed_ms: 0,
        // A start of the phone that never was: the step since the kept
        // moment counts nothing.
        boot_count: u32::MAX,
        utc_offset_secs: offset,
        hours_24: true,
        // Off, so that a correction the phone keeps for a clock set by hand
        // is kept here too.
        auto_time: false,
        interactive: false,
        keyguard_locked: true,
        call: CallState::Idle,
        front: Front::Unknown,
        in_reach: false,
        prodded: false,
        permissions: permissions_of(&kept),
    };
    let orders = engine.tick(&facts);
    let wrong_tries = engine.outgoing_report(now).map(|r| r.wrong_guesses);
    let Some(view) = engine.view() else { return lines };
    let today = &view.today;

    let held = orders.time_left.away && !orders.time_left.locked && orders.time_left.seconds_left.is_some_and(|s| s > 0);
    lines.push(format!(
        "Time left today: {}{}",
        today.left.words,
        if held { " (held on the phone: there is no master to reach)." } else { "." }
    ));
    lines.push(format!("Used today: {}. {}.", today.used, today.where_used));
    if today.apps.is_empty() {
        lines.push("Apps today: none used for a minute or more.".to_string());
    } else {
        lines.push("Apps today:".to_string());
        for app in &today.apps {
            lines.push(format!("  {}: {}", app.package, app.minutes));
        }
    }
    if let Some(bedtime) = &today.bedtime {
        lines.push(bedtime.clone());
    }
    match view.decision.as_ref().and_then(|d| d.device_lock) {
        None => lines.push("Locked: no.".to_string()),
        Some(lock) => {
            let until = match lock.until {
                Some(at) => format!(", until {}", when_text(now, at, offset)),
                None => String::new(),
            };
            lines.push(format!("Locked: yes, {}{until}. The lock says: {}", reason_words(lock.reason), orders.time_left.words));
        }
    }
    if let Some(until) = view.decision.as_ref().and_then(|d| d.unlocked_until) {
        lines.push(format!("A guardian's unlock keeps it open until {}.", when_text(now, until, offset)));
    }
    match wrong_tries {
        Some(0) | None => lines.push("Wrong tries of a code or the PIN: none.".to_string()),
        Some(n) => lines.push(format!("Wrong tries of a code or the PIN since the last right PIN: {n}.")),
    }
    if let Some(wait) = view.decision.as_ref().and_then(|d| d.try_wait_until).filter(|w| *w > now) {
        lines.push(format!("After too many wrong tries, the next try is {}.", when_text(now, wait, offset)));
    }
    let waiting: Vec<_> = view.requests_today.iter().filter(|r| r.answer == RequestAnswer::Waiting).collect();
    for request in &waiting {
        lines.push(format!(
            "A request waits for an answer: {} minutes, asked {}.",
            request.request.minutes,
            when_text(now, request.request.asked_at, offset)
        ));
    }
    let answered = view.requests_today.len() - waiting.len();
    if answered > 0 {
        lines.push(format!("Requests answered today: {answered}."));
    }
    let off: Vec<Permission> = kept.today.guardian_needed.iter().flat_map(|g| g.off.iter().map(|o| o.permission)).collect();
    lines.push(switched_off_line(&off));
    if !view.unreported.is_empty() {
        lines.push(format!(
            "Use not yet sent to a master: {} pieces. They stay on the phone until the link to the master PC is built.",
            view.unreported.len()
        ));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permissions_are_named_in_words_in_the_order_setup_asks_for_them() {
        let words: Vec<&str> = Permission::ALL.into_iter().map(permission_words).collect();
        assert_eq!(words, ["notifications", "phone", "usage access", "accessibility", "battery", "protection"]);
        assert_eq!(
            switched_off_line(&Permission::ALL),
            "Switched off on the phone when LiMo last looked: notifications, phone, usage access, accessibility, battery, protection."
        );
        // In setup's order whatever order they come in.
        assert_eq!(
            switched_off_line(&[Permission::Accessibility, Permission::UsageAccess]),
            "Switched off on the phone when LiMo last looked: usage access, accessibility."
        );
        for one in Permission::ALL {
            assert_eq!(
                switched_off_line(&[one]),
                format!("Switched off on the phone when LiMo last looked: {}.", permission_words(one))
            );
        }
        assert_eq!(switched_off_line(&[]), "Nothing LiMo needs was switched off when it last looked.");
    }

    #[test]
    fn times_are_said_on_the_phones_clock() {
        // 2027-01-15 08:00:00 UTC is 13:30 in a zone 5 h 30 min ahead.
        let now = 1_800_000_000;
        assert_eq!(when_text(now, now, 19_800), "today at 13:30");
        assert_eq!(when_text(now, now + 12 * 3_600, 19_800), "tomorrow at 01:30");
        assert_eq!(when_text(now, now + 3 * 86_400, 0), "Mon 18 Jan at 08:00");
    }
}
