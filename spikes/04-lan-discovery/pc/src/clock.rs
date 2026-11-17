//! Time: the safety stop arithmetic (pure, unit tested), time stamps for the
//! event lines, and the unix time for the protocol.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum StopReason {
    CtrlC,
    SafetyStop,
}

impl StopReason {
    pub fn word(self) -> &'static str {
        match self {
            StopReason::CtrlC => "ctrl_c",
            StopReason::SafetyStop => "safety_stop",
        }
    }
}

/// The moment the safety stop ends serve.
pub fn stop_deadline(start: Instant, seconds: u64) -> Instant {
    start + Duration::from_secs(seconds)
}

/// Time left until `deadline`, never negative.
pub fn remaining(deadline: Instant, now: Instant) -> Duration {
    deadline.saturating_duration_since(now)
}

/// Whether serve should stop now, and why. Ctrl+C wins over the deadline.
pub fn stop_reason(now: Instant, deadline: Instant, ctrl_c: bool) -> Option<StopReason> {
    if ctrl_c {
        Some(StopReason::CtrlC)
    } else if now >= deadline {
        Some(StopReason::SafetyStop)
    } else {
        None
    }
}

/// The clock time (hours, minutes, seconds) `plus` seconds after the given
/// one, wrapping at midnight.
pub fn clock_after(h: u32, m: u32, s: u32, plus: u64) -> (u32, u32, u32) {
    let day = 24 * 3600u64;
    let start = (h as u64 * 3600 + m as u64 * 60 + s as u64) % day;
    let t = (start + plus % day) % day;
    ((t / 3600) as u32, ((t % 3600) / 60) as u32, (t % 60) as u32)
}

/// Seconds since 1970, as the protocol sends it.
pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Milliseconds with one decimal, from a duration. Pure.
pub fn ms_one_decimal(d: Duration) -> String {
    let tenths = (d.as_nanos() + 50_000) / 100_000;
    format!("{}.{}", tenths / 10, tenths % 10)
}

/// Local time now: hours, minutes, seconds, milliseconds.
pub fn local_now() -> (u32, u32, u32, u32) {
    #[cfg(windows)]
    {
        let mut st = win::SystemTime::default();
        // SAFETY: GetLocalTime only writes the struct it is given.
        unsafe { win::GetLocalTime(&mut st) };
        (
            st.hour as u32,
            st.minute as u32,
            st.second as u32,
            st.milliseconds as u32,
        )
    }
    #[cfg(not(windows))]
    {
        // Elsewhere: UTC, which is good enough for a spike built for Windows.
        let d = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let s = d.as_secs() % 86_400;
        (
            (s / 3600) as u32,
            ((s % 3600) / 60) as u32,
            (s % 60) as u32,
            d.subsec_millis(),
        )
    }
}

/// "HH:MM:SS.mmm" in local time, for the start of each event line.
pub fn stamp() -> String {
    let (h, m, s, ms) = local_now();
    format!("{h:02}:{m:02}:{s:02}.{ms:03}")
}

/// "HH:MM:SS" local time, `plus` seconds from now.
pub fn local_clock_after(plus: u64) -> String {
    let (h, m, s, _) = local_now();
    let (h, m, s) = clock_after(h, m, s, plus);
    format!("{h:02}:{m:02}:{s:02}")
}

#[cfg(windows)]
mod win {
    /// SYSTEMTIME from the Windows SDK: eight 16 bit fields.
    #[repr(C)]
    #[derive(Default)]
    pub struct SystemTime {
        pub year: u16,
        pub month: u16,
        pub day_of_week: u16,
        pub day: u16,
        pub hour: u16,
        pub minute: u16,
        pub second: u16,
        pub milliseconds: u16,
    }

    #[link(name = "kernel32")]
    extern "system" {
        pub fn GetLocalTime(system_time: *mut SystemTime);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadline_and_remaining() {
        let start = Instant::now();
        let deadline = stop_deadline(start, 600);
        assert_eq!(deadline - start, Duration::from_secs(600));
        assert_eq!(remaining(deadline, start), Duration::from_secs(600));
        assert_eq!(
            remaining(deadline, start + Duration::from_secs(599)),
            Duration::from_secs(1)
        );
        assert_eq!(remaining(deadline, deadline), Duration::ZERO);
        // Past the deadline it stays at zero, it never goes negative.
        assert_eq!(
            remaining(deadline, deadline + Duration::from_secs(5)),
            Duration::ZERO
        );
    }

    #[test]
    fn stop_reasons() {
        let start = Instant::now();
        let deadline = stop_deadline(start, 3600);
        assert_eq!(stop_reason(start, deadline, false), None);
        assert_eq!(
            stop_reason(deadline - Duration::from_millis(1), deadline, false),
            None
        );
        assert_eq!(
            stop_reason(deadline, deadline, false),
            Some(StopReason::SafetyStop)
        );
        assert_eq!(
            stop_reason(deadline + Duration::from_secs(1), deadline, false),
            Some(StopReason::SafetyStop)
        );
        assert_eq!(stop_reason(start, deadline, true), Some(StopReason::CtrlC));
        // Both at once: Ctrl+C is reported.
        assert_eq!(stop_reason(deadline, deadline, true), Some(StopReason::CtrlC));
    }

    #[test]
    fn the_longest_serve_is_one_hour() {
        let start = Instant::now();
        let deadline = stop_deadline(start, crate::args::MAX_SERVE_SECONDS);
        assert_eq!(deadline - start, Duration::from_secs(3600));
    }

    #[test]
    fn clock_after_wraps_at_midnight() {
        assert_eq!(clock_after(10, 0, 0, 600), (10, 10, 0));
        assert_eq!(clock_after(23, 55, 0, 600), (0, 5, 0));
        assert_eq!(clock_after(23, 59, 59, 1), (0, 0, 0));
        assert_eq!(clock_after(12, 0, 0, 3600), (13, 0, 0));
        assert_eq!(clock_after(1, 2, 3, 0), (1, 2, 3));
        assert_eq!(clock_after(1, 0, 0, 86_400 * 3 + 60), (1, 1, 0));
    }

    #[test]
    fn milliseconds_with_one_decimal() {
        assert_eq!(ms_one_decimal(Duration::from_micros(0)), "0.0");
        assert_eq!(ms_one_decimal(Duration::from_micros(440)), "0.4");
        assert_eq!(ms_one_decimal(Duration::from_micros(450)), "0.5");
        assert_eq!(ms_one_decimal(Duration::from_micros(7_260)), "7.3");
        assert_eq!(ms_one_decimal(Duration::from_millis(19)), "19.0");
        assert_eq!(ms_one_decimal(Duration::from_micros(1_999_960)), "2000.0");
    }

    #[test]
    fn stamp_shape() {
        let s = stamp();
        assert_eq!(s.len(), 12);
        assert_eq!(&s[2..3], ":");
        assert_eq!(&s[5..6], ":");
        assert_eq!(&s[8..9], ".");
    }
}
