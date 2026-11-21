//! The signed in sessions: who is in each, whether they are an
//! administrator, and whether the session is in use, locked or
//! disconnected; and disconnecting one.

use std::ffi::c_void;

use limo_agent::SessionState;
use windows::Win32::Foundation::{HANDLE, LocalFree, HLOCAL};
use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows::Win32::Security::{
    GetTokenInformation, PSID, TOKEN_GROUPS, TOKEN_INFORMATION_CLASS, TOKEN_LINKED_TOKEN, TOKEN_USER, TokenGroups,
    TokenLinkedToken, TokenUser,
};
use windows::Win32::System::RemoteDesktop::{
    WTS_SESSION_INFOW, WTSActive, WTSDisconnectSession, WTSEnumerateSessionsW, WTSFreeMemory, WTSINFOEXW,
    WTSQuerySessionInformationW, WTSQueryUserToken, WTSSessionInfoEx, WTSUserName,
};
use windows::core::PWSTR;

use super::{Owned, error_text, from_wide};
use crate::os::SessionSeen;

/// The Administrators group's security identifier.
pub(crate) const ADMINISTRATORS: &str = "S-1-5-32-544";

/// `WTSINFOEX_LEVEL1_W::SessionFlags`: the Windows lock screen is up.
const WTS_SESSIONSTATE_LOCK: i32 = 0;

/// How a session stands, from its connection state and its lock flags. Only
/// an active session is on the screen; it is locked when Windows says so,
/// and taken as in use when Windows cannot say, so that time counts.
pub(crate) fn state_of(active: bool, lock_flags: Option<i32>) -> SessionState {
    match (active, lock_flags) {
        (false, _) => SessionState::Disconnected,
        (true, Some(WTS_SESSIONSTATE_LOCK)) => SessionState::Locked,
        (true, _) => SessionState::InUse,
    }
}

/// True when the Administrators group is among a token's groups, enabled or
/// not: a UAC filtered token keeps it, for denying only.
pub(crate) fn admin_in(groups: &[String]) -> bool {
    groups.iter().any(|sid| sid == ADMINISTRATORS)
}

/// Every session with a signed in user, but session 0.
pub(crate) fn sessions() -> Vec<SessionSeen> {
    let mut list: *mut WTS_SESSION_INFOW = std::ptr::null_mut();
    let mut count: u32 = 0;
    // SAFETY: the list is freed below with WTSFreeMemory, and read only
    // within its count.
    unsafe {
        if WTSEnumerateSessionsW(None, 0, 1, &mut list, &mut count).is_err() || list.is_null() {
            return Vec::new();
        }
        let infos = std::slice::from_raw_parts(list, count as usize);
        let mut seen = Vec::new();
        for info in infos {
            let id = info.SessionId;
            if id == 0 {
                continue;
            }
            let Some(name) = user_name(id) else { continue };
            let Some((account_id, is_administrator)) = account_of(id) else { continue };
            let active = info.State == WTSActive;
            let state = state_of(active, if active { lock_flags(id) } else { None });
            seen.push(SessionSeen { session_id: id, account_id, account_name: name, state, is_administrator });
        }
        WTSFreeMemory(list as *mut c_void);
        seen
    }
}

/// The user name signed in to a session, if any.
fn user_name(session: u32) -> Option<String> {
    let mut buffer = PWSTR::null();
    let mut bytes: u32 = 0;
    // SAFETY: the buffer comes from WTSQuerySessionInformationW and is
    // freed with WTSFreeMemory once copied.
    unsafe {
        WTSQuerySessionInformationW(None, session, WTSUserName, &mut buffer, &mut bytes).ok()?;
        if buffer.is_null() {
            return None;
        }
        let units = std::slice::from_raw_parts(buffer.0, (bytes as usize) / 2);
        let name = from_wide(units);
        WTSFreeMemory(buffer.0 as *mut c_void);
        let name = name.trim().to_string();
        if name.is_empty() { None } else { Some(name) }
    }
}

/// The session's lock flags, if Windows says.
fn lock_flags(session: u32) -> Option<i32> {
    let mut buffer = PWSTR::null();
    let mut bytes: u32 = 0;
    // SAFETY: the buffer is read as WTSINFOEXW only when it is at least that
    // big, and freed with WTSFreeMemory.
    unsafe {
        WTSQuerySessionInformationW(None, session, WTSSessionInfoEx, &mut buffer, &mut bytes).ok()?;
        if buffer.is_null() {
            return None;
        }
        let flags = if (bytes as usize) >= std::mem::size_of::<WTSINFOEXW>() {
            let info = &*(buffer.0 as *const WTSINFOEXW);
            (info.Level == 1).then_some(info.Data.WTSInfoExLevel1.SessionFlags)
        } else {
            None
        };
        WTSFreeMemory(buffer.0 as *mut c_void);
        flags
    }
}

/// The account in a session: its security identifier, and whether it is
/// an administrator.
fn account_of(session: u32) -> Option<(String, bool)> {
    let mut token = HANDLE::default();
    // SAFETY: needs SeTcbPrivilege, which LocalSystem has; the token is
    // closed when `owned` drops.
    unsafe { WTSQueryUserToken(session, &mut token).ok()? };
    let owned = Owned::new(token)?;
    let sid = token_user_sid(owned.raw())?;
    Some((sid, is_administrator(owned.raw())))
}

/// Reads one token information class into a buffer aligned for any of the
/// structures it holds.
fn token_information(token: HANDLE, class: TOKEN_INFORMATION_CLASS) -> Option<Vec<u64>> {
    let mut needed: u32 = 0;
    // SAFETY: the first call only asks for the size; the second writes at
    // most that many bytes into a buffer at least that big.
    unsafe {
        let _ = GetTokenInformation(token, class, None, 0, &mut needed);
        if needed == 0 || needed > 1 << 20 {
            return None;
        }
        let mut buffer = vec![0u64; (needed as usize).div_ceil(8)];
        GetTokenInformation(token, class, Some(buffer.as_mut_ptr() as *mut c_void), needed, &mut needed).ok()?;
        Some(buffer)
    }
}

/// A security identifier as a string, "S-1-5-21-...".
pub(crate) fn sid_text(sid: PSID) -> Option<String> {
    let mut text = PWSTR::null();
    // SAFETY: the text is allocated by Windows and freed with LocalFree once
    // copied.
    unsafe {
        ConvertSidToStringSidW(sid, &mut text).ok()?;
        let copied = text.to_string().ok();
        let _ = LocalFree(Some(HLOCAL(text.0 as *mut c_void)));
        copied
    }
}

/// The user a token belongs to, as a security identifier string.
pub(crate) fn token_user_sid(token: HANDLE) -> Option<String> {
    let buffer = token_information(token, TokenUser)?;
    // SAFETY: the buffer holds a TOKEN_USER and the SID it points into.
    let user = unsafe { &*(buffer.as_ptr() as *const TOKEN_USER) };
    sid_text(user.User.Sid)
}

/// The groups of a token, as security identifier strings.
fn token_groups(token: HANDLE) -> Vec<String> {
    let Some(buffer) = token_information(token, TokenGroups) else { return Vec::new() };
    // SAFETY: the buffer holds a TOKEN_GROUPS with GroupCount entries, and
    // the SIDs they point into.
    unsafe {
        let groups = &*(buffer.as_ptr() as *const TOKEN_GROUPS);
        let entries = std::slice::from_raw_parts(groups.Groups.as_ptr(), groups.GroupCount as usize);
        entries.iter().filter_map(|entry| sid_text(entry.Sid)).collect()
    }
}

/// True when the token's account is in the Administrators group: in the
/// token's own groups, or in those of its linked token (the elevated one,
/// for an administrator running without elevation).
pub(crate) fn is_administrator(token: HANDLE) -> bool {
    if admin_in(&token_groups(token)) {
        return true;
    }
    let Some(buffer) = token_information(token, TokenLinkedToken) else { return false };
    // SAFETY: the buffer holds a TOKEN_LINKED_TOKEN; its handle is ours to
    // close, which `Owned` does.
    let linked = unsafe { (*(buffer.as_ptr() as *const TOKEN_LINKED_TOKEN)).LinkedToken };
    match Owned::new(linked) {
        Some(linked) => admin_in(&token_groups(linked.raw())),
        None => false,
    }
}

/// Disconnects a session: the Windows sign in screen covers it, and its
/// programs stay open.
pub(crate) fn disconnect(session: u32) -> Result<(), String> {
    // SAFETY: takes plain values.
    unsafe { WTSDisconnectSession(None, session, false) }.map_err(|e| error_text(&e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Security::TOKEN_QUERY;
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    #[test]
    fn a_session_is_in_use_locked_or_disconnected() {
        assert_eq!(state_of(true, Some(1)), SessionState::InUse);
        assert_eq!(state_of(true, Some(0)), SessionState::Locked);
        assert_eq!(state_of(true, Some(-1)), SessionState::InUse);
        assert_eq!(state_of(true, None), SessionState::InUse);
        assert_eq!(state_of(false, Some(1)), SessionState::Disconnected);
        assert_eq!(state_of(false, None), SessionState::Disconnected);
    }

    #[test]
    fn an_administrator_is_found_by_the_group_enabled_or_not() {
        assert!(admin_in(&["S-1-1-0".into(), ADMINISTRATORS.into()]));
        assert!(!admin_in(&["S-1-1-0".into(), "S-1-5-32-545".into()]));
        assert!(!admin_in(&[]));
    }

    /// Reads this test process's own token: its user, and whether that
    /// account is an administrator. Only reads.
    #[test]
    fn this_processs_own_account_reads() {
        let mut token = HANDLE::default();
        // SAFETY: opens this process's own token for reading; closed by
        // `Owned`.
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).unwrap() };
        let token = Owned::new(token).unwrap();
        let sid = token_user_sid(token.raw()).unwrap();
        assert!(sid.starts_with("S-1-5-"), "{sid}");
        // The answer depends on the account running the tests; it must only
        // come back.
        let _ = is_administrator(token.raw());
        assert!(!token_groups(token.raw()).is_empty());
    }
}
