//! LiMo spike 03: a tiny but real piece of the time rules.
//!
//! Pure logic, no I/O. The same code is an ordinary Rust library on Windows
//! (see the tests at the end) and a shared library on Android, called from
//! Kotlin through bindings that UniFFI generates.
//!
//! What crosses the boundary: a string (`core_version`), a record in and a
//! record out (`BudgetInput`, `LeaseDecision`), a boolean (`locked`) and a
//! typed error (`CoreError`).

use std::fmt;

uniffi::setup_scaffolding!();

/// Seconds in one day. More use than this in one day is an input error.
pub const SECONDS_PER_DAY: u32 = 86_400;

/// What the caller knows when it asks for a lease of screen time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BudgetInput {
    /// The day's budget, in seconds.
    pub daily_budget_secs: u32,
    /// Seconds already used today.
    pub used_secs: u32,
    /// Seconds the caller would like to be granted now.
    pub lease_secs: u32,
}

/// The core's answer to one lease request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct LeaseDecision {
    /// Seconds granted now: the smaller of the lease asked for and the time left.
    pub granted_secs: u32,
    /// Seconds still left in the day after this grant.
    pub remaining_secs: u32,
    /// True when nothing could be granted.
    pub locked: bool,
}

/// Inputs the core refuses. The variant names reach Kotlin and JavaScript
/// unchanged, so callers can match on them. A "flat" error: Kotlin gets the
/// variant and the text from `Display` below as the exception message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Error)]
#[uniffi(flat_error)]
pub enum CoreError {
    /// `lease_secs` was 0. A lease of nothing is a caller bug.
    ZeroLease,
    /// `used_secs` was over 86400, more than a whole day.
    UsedMoreThanADay,
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoreError::ZeroLease => f.write_str("ZeroLease: lease_secs must be more than 0"),
            CoreError::UsedMoreThanADay => {
                f.write_str("UsedMoreThanADay: used_secs must be 86400 or less")
            }
        }
    }
}

impl std::error::Error for CoreError {}

/// The version of this crate, for example "0.1.0".
#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Decides one lease.
///
/// Time left is the budget minus what was used, never below zero. Granted is
/// the smaller of the lease asked for and the time left. Remaining is the
/// time left minus what was granted. Locked means nothing was granted.
///
/// Errors, checked in this order: `ZeroLease` when `lease_secs` is 0,
/// `UsedMoreThanADay` when `used_secs` is over 86400.
#[uniffi::export]
pub fn decide_lease(input: BudgetInput) -> Result<LeaseDecision, CoreError> {
    if input.lease_secs == 0 {
        return Err(CoreError::ZeroLease);
    }
    if input.used_secs > SECONDS_PER_DAY {
        return Err(CoreError::UsedMoreThanADay);
    }
    let left = input.daily_budget_secs.saturating_sub(input.used_secs);
    let granted = input.lease_secs.min(left);
    Ok(LeaseDecision {
        granted_secs: granted,
        remaining_secs: left - granted,
        locked: granted == 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(daily_budget_secs: u32, used_secs: u32, lease_secs: u32) -> Result<LeaseDecision, CoreError> {
        decide_lease(BudgetInput { daily_budget_secs, used_secs, lease_secs })
    }

    fn decision(granted_secs: u32, remaining_secs: u32, locked: bool) -> Result<LeaseDecision, CoreError> {
        Ok(LeaseDecision { granted_secs, remaining_secs, locked })
    }

    #[test]
    fn version_is_the_crate_version() {
        assert_eq!(core_version(), "0.1.0");
    }

    #[test]
    fn plenty_left_grants_the_whole_lease() {
        assert_eq!(ask(3600, 0, 600), decision(600, 3000, false));
    }

    #[test]
    fn little_left_grants_only_what_is_left() {
        // The app's starting values.
        assert_eq!(ask(3600, 3300, 600), decision(300, 0, false));
    }

    #[test]
    fn lease_exactly_equal_to_time_left() {
        assert_eq!(ask(3600, 3000, 600), decision(600, 0, false));
    }

    #[test]
    fn budget_used_up_locks() {
        assert_eq!(ask(3600, 3600, 600), decision(0, 0, true));
    }

    #[test]
    fn used_more_than_budget_never_goes_below_zero() {
        assert_eq!(ask(3600, 5000, 600), decision(0, 0, true));
    }

    #[test]
    fn zero_budget_locks() {
        assert_eq!(ask(0, 0, 600), decision(0, 0, true));
    }

    #[test]
    fn zero_lease_is_an_error() {
        assert_eq!(ask(3600, 0, 0), Err(CoreError::ZeroLease));
    }

    #[test]
    fn a_whole_day_used_is_allowed() {
        assert_eq!(ask(SECONDS_PER_DAY, SECONDS_PER_DAY, 600), decision(0, 0, true));
    }

    #[test]
    fn more_than_a_day_used_is_an_error() {
        assert_eq!(ask(3600, SECONDS_PER_DAY + 1, 600), Err(CoreError::UsedMoreThanADay));
    }

    #[test]
    fn zero_lease_is_checked_first() {
        assert_eq!(ask(3600, SECONDS_PER_DAY + 1, 0), Err(CoreError::ZeroLease));
    }

    #[test]
    fn largest_numbers_do_not_overflow() {
        assert_eq!(ask(u32::MAX, 0, u32::MAX), decision(u32::MAX, 0, false));
        assert_eq!(ask(u32::MAX, SECONDS_PER_DAY, 1), decision(1, u32::MAX - SECONDS_PER_DAY - 1, false));
    }

    #[test]
    fn error_text_starts_with_the_variant_name() {
        assert!(CoreError::ZeroLease.to_string().starts_with("ZeroLease"));
        assert!(CoreError::UsedMoreThanADay.to_string().starts_with("UsedMoreThanADay"));
    }
}
