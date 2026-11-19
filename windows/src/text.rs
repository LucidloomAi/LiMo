//! The words the plain display shows, worked out from what the service
//! hands over. The real lock screen and time left display, drawn later to
//! the agreed design, may use these or their own.

#![forbid(unsafe_code)]

/// A moment on the PC's own clock, for showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalMoment {
    /// The year.
    pub year: u16,
    /// The month, 1 to 12.
    pub month: u16,
    /// The day of the month, 1 to 31.
    pub day: u16,
    /// The hour, 0 to 23.
    pub hour: u16,
    /// The minute, 0 to 59.
    pub minute: u16,
}

const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/// "18:30".
pub fn clock(moment: LocalMoment) -> String {
    format!("{:02}:{:02}", moment.hour, moment.minute)
}

/// True when `later` is the calendar day after `now`.
fn is_next_day(now: LocalMoment, later: LocalMoment) -> bool {
    let days_in = |year: u16, month: u16| -> u16 {
        match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            _ if (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400) => 29,
            _ => 28,
        }
    };
    if now.day < days_in(now.year, now.month) {
        later.year == now.year && later.month == now.month && later.day == now.day + 1
    } else if now.month < 12 {
        later.year == now.year && later.month == now.month + 1 && later.day == 1
    } else {
        later.year == now.year + 1 && later.month == 1 && later.day == 1
    }
}

/// When something happens, said from now: "at 18:30", "tomorrow at 07:00",
/// or "on 21 November at 07:00".
pub fn when(now: LocalMoment, at: LocalMoment) -> String {
    if (at.year, at.month, at.day) == (now.year, now.month, now.day) {
        format!("at {}", clock(at))
    } else if is_next_day(now, at) {
        format!("tomorrow at {}", clock(at))
    } else {
        let month = MONTHS.get(usize::from(at.month.saturating_sub(1))).copied().unwrap_or("");
        format!("on {} {month} at {}", at.day, clock(at))
    }
}

/// "Opens again at 18:30." for a lock that ends at a known moment.
pub fn opens_again(now: LocalMoment, at: LocalMoment) -> String {
    format!("Opens again {}.", when(now, at))
}

/// The time left, in words: "1 h 05 min left today.", "12 min left
/// today.", "Less than a minute left today.", or "No time limit today.",
/// with "Paused." after it while time is not being counted.
pub fn time_left(seconds_left: Option<u64>, counting: bool) -> String {
    let left = match seconds_left {
        None => "No time limit today.".to_string(),
        Some(secs) if secs < 60 => "Less than a minute left today.".to_string(),
        Some(secs) => {
            let minutes = secs.div_ceil(60);
            if minutes < 60 {
                format!("{minutes} min left today.")
            } else {
                format!("{} h {:02} min left today.", minutes / 60, minutes % 60)
            }
        }
    };
    if counting || seconds_left.is_none() { left } else { format!("{left} Paused.") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(year: u16, month: u16, day: u16, hour: u16, minute: u16) -> LocalMoment {
        LocalMoment { year, month, day, hour, minute }
    }

    #[test]
    fn times_are_said_from_now() {
        let now = at(2026, 11, 19, 16, 5);
        assert_eq!(opens_again(now, at(2026, 11, 19, 18, 30)), "Opens again at 18:30.");
        assert_eq!(opens_again(now, at(2026, 11, 20, 7, 0)), "Opens again tomorrow at 07:00.");
        assert_eq!(opens_again(now, at(2026, 11, 21, 7, 0)), "Opens again on 21 November at 07:00.");
        assert_eq!(when(at(2026, 11, 30, 21, 0), at(2026, 12, 1, 7, 0)), "tomorrow at 07:00");
        assert_eq!(when(at(2026, 12, 31, 21, 0), at(2027, 1, 1, 7, 0)), "tomorrow at 07:00");
        assert_eq!(when(at(2028, 2, 28, 21, 0), at(2028, 2, 29, 7, 0)), "tomorrow at 07:00");
        assert_eq!(when(at(2026, 2, 28, 21, 0), at(2026, 3, 1, 7, 0)), "tomorrow at 07:00");
    }

    #[test]
    fn the_time_left_is_said_in_minutes_rounded_up() {
        assert_eq!(time_left(Some(7_200), true), "2 h 00 min left today.");
        assert_eq!(time_left(Some(3_901), true), "1 h 06 min left today.");
        assert_eq!(time_left(Some(600), true), "10 min left today.");
        assert_eq!(time_left(Some(61), true), "2 min left today.");
        assert_eq!(time_left(Some(59), true), "Less than a minute left today.");
        assert_eq!(time_left(Some(600), false), "10 min left today. Paused.");
        assert_eq!(time_left(None, false), "No time limit today.");
    }
}
