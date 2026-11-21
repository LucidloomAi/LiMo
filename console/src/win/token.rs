//! Who a process or a thread acts as, where a process runs from, and
//! Windows' own folders.

use std::ffi::c_void;

use windows::Win32::Foundation::{HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows::Win32::Security::{GetTokenInformation, PSID, TOKEN_ELEVATION, TOKEN_INFORMATION_CLASS, TOKEN_QUERY, TOKEN_USER, TokenElevation, TokenUser};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Shell::{FOLDERID_ProgramFiles, KF_FLAG_DEFAULT, SHGetKnownFolderPath};
use windows::core::PWSTR;

use super::Owned;

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
    if sid.0.is_null() {
        return None;
    }
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

/// The account a token belongs to, as a security identifier string.
fn token_account(token: HANDLE) -> Option<String> {
    let buffer = token_information(token, TokenUser)?;
    // SAFETY: the buffer holds a TOKEN_USER and the SID it points into.
    let user = unsafe { &*(buffer.as_ptr() as *const TOKEN_USER) };
    sid_text(user.User.Sid)
}

/// The account a process runs as. None when it cannot be opened, which
/// for another account's process is usual unless the caller is LocalSystem.
#[cfg(test)]
pub(crate) fn process_account(pid: u32) -> Option<String> {
    // SAFETY: opens for reading what may be read of a process, and its
    // token for reading; both closed by `Owned`.
    unsafe {
        let process = Owned::new(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?)?;
        let mut token = HANDLE::default();
        OpenProcessToken(process.raw(), TOKEN_QUERY, &mut token).ok()?;
        let token = Owned::new(token)?;
        token_account(token.raw())
    }
}

/// The account this process runs as.
pub(crate) fn own_account() -> Option<String> {
    let mut token = HANDLE::default();
    // SAFETY: opens this process's own token for reading; closed by `Owned`.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).ok()? };
    let token = Owned::new(token)?;
    token_account(token.raw())
}

/// True when this process runs elevated.
pub(crate) fn own_token_is_elevated() -> bool {
    let mut token = HANDLE::default();
    // SAFETY: opens this process's own token for reading; closed by `Owned`.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.is_err() {
        return false;
    }
    let Some(token) = Owned::new(token) else { return false };
    let Some(buffer) = token_information(token.raw(), TokenElevation) else { return false };
    // SAFETY: the buffer holds a TOKEN_ELEVATION.
    let elevation = unsafe { &*(buffer.as_ptr() as *const TOKEN_ELEVATION) };
    elevation.TokenIsElevated != 0
}

/// The session a process runs in.
pub(crate) fn session_of(pid: u32) -> Option<u32> {
    let mut session: u32 = 0;
    // SAFETY: writes the session id into the value handed to it.
    unsafe { ProcessIdToSessionId(pid, &mut session).ok()? };
    Some(session)
}

/// The full path of a process's program file.
pub(crate) fn image_of(pid: u32) -> Option<String> {
    // SAFETY: opens for reading what may be read of a process; closed by
    // `Owned`.
    let process = Owned::new(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?)?;
    for room in [1_024u32, 32_768] {
        let mut buffer = vec![0u16; room as usize];
        let mut size = room;
        // SAFETY: writes at most `size` units into the buffer and says how
        // many.
        if unsafe { QueryFullProcessImageNameW(process.raw(), PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut size) }.is_ok() {
            let size = (size as usize).min(buffer.len());
            return Some(String::from_utf16_lossy(&buffer[..size]));
        }
    }
    None
}

/// Windows' own Program Files folder for this program's kind (64 bit on a
/// 64 bit Windows), as Windows says it, not as an environment variable does.
pub(crate) fn program_files() -> Option<String> {
    // SAFETY: Windows allocates the path; it is copied, then freed with
    // CoTaskMemFree.
    unsafe {
        let path = SHGetKnownFolderPath(&FOLDERID_ProgramFiles, KF_FLAG_DEFAULT, None).ok()?;
        let text = path.to_string().ok();
        CoTaskMemFree(Some(path.0 as *const c_void));
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads this test process's own account, session and program file,
    /// and Windows' Program Files folder. Only reads.
    #[test]
    fn this_process_reads_itself() {
        let me = own_account().unwrap();
        assert!(me.starts_with("S-1-5-"), "{me}");
        assert_eq!(process_account(std::process::id()).as_deref(), Some(me.as_str()));
        let exe = std::env::current_exe().unwrap();
        let image = image_of(std::process::id()).unwrap();
        assert!(crate::door::same_path(&image, &exe.to_string_lossy()), "{image}");
        assert!(session_of(std::process::id()).is_some());
        let folder = program_files().unwrap();
        assert!(folder.len() > 3 && folder.chars().nth(1) == Some(':'), "{folder}");
    }
}
