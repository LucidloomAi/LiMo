//! The clocks: the wall clock, the steady count, the time zone, and local
//! times for showing and for the log.

use std::time::{SystemTime, UNIX_EPOCH};

use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
use windows::Win32::Globalization::{GetLocaleInfoEx, LOCALE_STIMEFORMAT};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::System::Time::{
    FileTimeToSystemTime, GetTimeZoneInformation, SystemTimeToTzSpecificLocalTime, TIME_ZONE_INFORMATION,
};
use windows::Win32::System::WindowsProgramming::QueryInterruptTime;
use windows::core::PCWSTR;

use super::from_wide;
use crate::text::LocalMoment;

/// `GetTimeZoneInformation`: standard time is in force.
const TIME_ZONE_ID_STANDARD: u32 = 1;
/// `GetTimeZoneInformation`: daylight time is in force.
const TIME_ZONE_ID_DAYLIGHT: u32 = 2;

/// Seconds from 1601 (Windows' start) to 1970 (unix's).
const SECS_1601_TO_1970: i64 = 11_644_473_600;

/// The wall clock, unix seconds.
pub(crate) fn wall_secs() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(since) => i64::try_from(since.as_secs()).unwrap_or(i64::MAX),
        Err(before) => -i64::try_from(before.duration().as_secs()).unwrap_or(i64::MAX),
    }
}

/// The interrupt time in milliseconds: counted from start up, it goes on
/// through sleep and hibernation (`QueryInterruptTime`, not the unbiased
/// one), and cannot be set.
pub(crate) fn steady_ms() -> u64 {
    // SAFETY: takes no arguments and only reads a counter.
    let hundreds_of_ns = unsafe { QueryInterruptTime() };
    hundreds_of_ns / 10_000
}

/// The PC's offset from UTC, in seconds, from what
/// `GetTimeZoneInformation` returned: the zone's bias, and the daylight or
/// standard bias when that is in force. Windows says UTC = local + bias, in
/// minutes.
pub(crate) fn offset_from(zone_id: u32, bias: i32, standard_bias: i32, daylight_bias: i32) -> i32 {
    let extra = match zone_id {
        TIME_ZONE_ID_DAYLIGHT => daylight_bias,
        TIME_ZONE_ID_STANDARD => standard_bias,
        _ => 0,
    };
    bias.saturating_add(extra).saturating_mul(-60)
}

/// The PC's offset from UTC now, in seconds, with daylight time when it is
/// in force.
pub(crate) fn utc_offset_secs() -> i32 {
    let mut zone = TIME_ZONE_INFORMATION::default();
    // SAFETY: writes into the structure handed to it.
    let id = unsafe { GetTimeZoneInformation(&mut zone) };
    offset_from(id, zone.Bias, zone.StandardBias, zone.DaylightBias)
}

/// The signed in user's time format, as their own Windows settings give it,
/// such as "h:mm:ss tt" or "HH:mm:ss"; empty when it cannot be read.
pub(crate) fn time_format() -> String {
    let mut buffer = [0u16; 80];
    // SAFETY: asks for the user's own locale (a null name) and writes at
    // most the buffer's length into it; the count it returns includes the
    // ending NUL, which `from_wide` drops.
    let written = unsafe { GetLocaleInfoEx(PCWSTR::null(), LOCALE_STIMEFORMAT, Some(&mut buffer)) };
    let count = usize::try_from(written).unwrap_or(0).min(buffer.len());
    from_wide(&buffer[..count])
}

/// A unix moment on the PC's own clock, for showing.
pub(crate) fn local_moment(unix_secs: i64) -> Option<LocalMoment> {
    let ticks = unix_secs.checked_add(SECS_1601_TO_1970)?.checked_mul(10_000_000)?;
    let ticks = u64::try_from(ticks).ok()?;
    let file_time = FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 };
    let mut utc = SYSTEMTIME::default();
    let mut local = SYSTEMTIME::default();
    // SAFETY: both calls read and write only the structures handed to them.
    unsafe {
        FileTimeToSystemTime(&file_time, &mut utc).ok()?;
        SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).ok()?;
    }
    Some(LocalMoment { year: local.wYear, month: local.wMonth, day: local.wDay, hour: local.wHour, minute: local.wMinute })
}

/// The local time now as "YYYY-MM-DD HH:MM:SS", for the log.
pub(crate) fn timestamp() -> String {
    // SAFETY: returns a structure, and reads nothing given.
    let t = unsafe { GetLocalTime() };
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_offset_follows_daylight_time_only_while_it_is_in_force() {
        // India: no daylight time.
        assert_eq!(offset_from(0, -330, 0, 0), 19_800);
        // London: standard, then daylight.
        assert_eq!(offset_from(TIME_ZONE_ID_STANDARD, 0, 0, -60), 0);
        assert_eq!(offset_from(TIME_ZONE_ID_DAYLIGHT, 0, 0, -60), 3_600);
        // New York.
        assert_eq!(offset_from(TIME_ZONE_ID_STANDARD, 300, 0, -60), -18_000);
        assert_eq!(offset_from(TIME_ZONE_ID_DAYLIGHT, 300, 0, -60), -14_400);
        // Unknown or failed: the bias alone.
        assert_eq!(offset_from(0xFFFF_FFFF, 300, 0, -60), -18_000);
    }

    #[test]
    fn the_clocks_read_sensibly_on_this_pc() {
        let a = steady_ms();
        let b = steady_ms();
        assert!(b >= a);
        assert!(wall_secs() > 1_700_000_000);
        let offset = utc_offset_secs();
        assert!((-14 * 3_600..=14 * 3_600).contains(&offset), "{offset}");
        let moment = local_moment(wall_secs()).unwrap();
        assert!((1..=12).contains(&moment.month) && moment.hour < 24 && moment.minute < 60);
        assert_eq!(timestamp().len(), 19);
        // This account's own time format names an hour, 12 or 24.
        let format = time_format();
        assert!(format.contains('h') || format.contains('H'), "{format}");
    }
}
