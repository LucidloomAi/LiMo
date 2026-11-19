//! The running processes: each one's session, start and program file; what
//! a program file's version resource says; and ending a process after
//! checking it is still the one that was seen.

use std::ffi::c_void;
use std::time::UNIX_EPOCH;

use windows::Win32::Foundation::{FILETIME, HANDLE};
use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
use windows::Win32::System::RemoteDesktop::{
    ProcessIdToSessionId, WTS_PROCESS_INFOW, WTSEnumerateProcessesExW, WTSFreeMemoryExW, WTSTypeProcessInfoLevel0,
};
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
    QueryFullProcessImageNameW, TerminateProcess,
};
use windows::core::{PCWSTR, PWSTR};

use super::{Owned, error_text, from_wide, wide};
use crate::close::same_path;
use crate::os::{ProcessSeen, VersionNames};
use crate::programs::file_name_of;

/// `WTS_ANY_SESSION`: every session.
const WTS_ANY_SESSION: u32 = 0xFFFF_FFFE;

/// The room tried first for a path, in UTF-16 units.
const SHORT_PATH_UNITS: u32 = 1_024;

/// The longest path read, in UTF-16 units.
const MAX_PATH_UNITS: u32 = 32_768;

/// The largest version resource read, in bytes.
const MAX_VERSION_BYTES: u32 = 4 * 1024 * 1024;

/// The version resource's languages to try, in order: the ones the file
/// lists, then US English in Unicode, in Windows' Latin 1, in no code page,
/// and no language in Unicode, each once.
pub(crate) fn translation_order(listed: &[(u16, u16)]) -> Vec<(u16, u16)> {
    let mut order: Vec<(u16, u16)> = Vec::new();
    for pair in listed.iter().copied().chain([(0x0409, 0x04B0), (0x0409, 0x04E4), (0x0409, 0x0000), (0x0000, 0x04B0)]) {
        if !order.contains(&pair) {
            order.push(pair);
        }
    }
    order
}

/// The place of one string in a version resource.
pub(crate) fn string_query(language: u16, code_page: u16, name: &str) -> String {
    format!("\\StringFileInfo\\{language:04x}{code_page:04x}\\{name}")
}

/// Every process in every session but session 0.
pub(crate) fn processes() -> Vec<ProcessSeen> {
    let mut level: u32 = 0;
    let mut list = PWSTR::null();
    let mut count: u32 = 0;
    // SAFETY: the list is read within its count as level 0 entries, and
    // freed with WTSFreeMemoryExW.
    unsafe {
        if WTSEnumerateProcessesExW(None, &mut level, WTS_ANY_SESSION, &mut list, &mut count).is_err() || list.is_null() {
            return Vec::new();
        }
        let infos = std::slice::from_raw_parts(list.0 as *const WTS_PROCESS_INFOW, count as usize);
        let mut seen = Vec::with_capacity(infos.len());
        for info in infos {
            if info.SessionId == 0 || info.ProcessId == 0 {
                continue;
            }
            let name = if info.pProcessName.is_null() { String::new() } else { info.pProcessName.to_string().unwrap_or_default() };
            let (started, path) = details(info.ProcessId);
            seen.push(ProcessSeen { pid: info.ProcessId, session_id: info.SessionId, started, name, path });
        }
        let _ = WTSFreeMemoryExW(WTSTypeProcessInfoLevel0, list.0 as *const c_void, count);
        seen
    }
}

/// One process by its id, if it runs.
pub(crate) fn process(pid: u32) -> Option<ProcessSeen> {
    let mut session: u32 = 0;
    // SAFETY: writes the session id into the value handed to it.
    unsafe { ProcessIdToSessionId(pid, &mut session).ok()? };
    let (started, path) = details(pid);
    let name = file_name_of(&path).to_string();
    Some(ProcessSeen { pid, session_id: session, started, name, path })
}

/// When a process started, and its program file's path; 0 and empty when
/// it cannot be opened.
fn details(pid: u32) -> (u64, String) {
    // SAFETY: opens for reading what anyone may read of a process; closed by
    // `Owned`.
    let Ok(handle) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }) else {
        return (0, String::new());
    };
    let Some(handle) = Owned::new(handle) else { return (0, String::new()) };
    (started(handle.raw()).unwrap_or(0), image_path(handle.raw()).unwrap_or_default())
}

/// When the process started, as Windows counts it.
fn started(process: HANDLE) -> Option<u64> {
    let (mut created, mut exited, mut kernel, mut user) =
        (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
    // SAFETY: writes into the four structures handed to it.
    unsafe { GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user).ok()? };
    Some((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
}

/// The full path of the process's program file: read into a buffer for an
/// ordinary path first, and into one for the longest path only when that
/// is too small.
fn image_path(process: HANDLE) -> Option<String> {
    for room in [SHORT_PATH_UNITS, MAX_PATH_UNITS] {
        let mut buffer = vec![0u16; room as usize];
        let mut size = room;
        // SAFETY: writes at most `size` units into the buffer and says how
        // many.
        if unsafe { QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut size) }.is_ok() {
            let size = (size as usize).min(buffer.len());
            return Some(String::from_utf16_lossy(&buffer[..size]));
        }
    }
    None
}

/// When a file was last written, in nanoseconds since 1970.
pub(crate) fn file_stamp(path: &str) -> Option<u64> {
    let written = std::fs::metadata(path).and_then(|m| m.modified()).ok()?;
    let since = written.duration_since(UNIX_EPOCH).ok()?;
    u64::try_from(since.as_nanos()).ok()
}

/// One string from a version resource held in `data`.
fn query_string(data: &[u32], query: &str) -> Option<String> {
    let query = wide(query);
    let mut found: *mut c_void = std::ptr::null_mut();
    let mut units: u32 = 0;
    let base = data.as_ptr() as usize;
    let end = base + std::mem::size_of_val(data);
    // SAFETY: VerQueryValueW points into `data`; the string is read only
    // when it starts inside it, and never past its end.
    unsafe {
        if !VerQueryValueW(data.as_ptr() as *const c_void, PCWSTR(query.as_ptr()), &mut found, &mut units).as_bool() {
            return None;
        }
        let start = found as usize;
        if found.is_null() || units == 0 || start < base || start >= end || !start.is_multiple_of(2) {
            return None;
        }
        let room = (end - start) / 2;
        let text = std::slice::from_raw_parts(found as *const u16, (units as usize).min(room));
        Some(from_wide(text))
    }
}

/// The languages a version resource lists.
fn translations(data: &[u32]) -> Vec<(u16, u16)> {
    let query = wide("\\VarFileInfo\\Translation");
    let mut found: *mut c_void = std::ptr::null_mut();
    let mut bytes: u32 = 0;
    let base = data.as_ptr() as usize;
    let end = base + std::mem::size_of_val(data);
    // SAFETY: as in `query_string`: read only inside `data`, as pairs of
    // 16 bit values.
    unsafe {
        if !VerQueryValueW(data.as_ptr() as *const c_void, PCWSTR(query.as_ptr()), &mut found, &mut bytes).as_bool() {
            return Vec::new();
        }
        let start = found as usize;
        if found.is_null() || start < base || start >= end || !start.is_multiple_of(2) {
            return Vec::new();
        }
        let pairs = ((bytes as usize).min(end - start)) / 4;
        let values = std::slice::from_raw_parts(found as *const u16, pairs * 2);
        values.as_chunks::<2>().0.iter().map(|pair| (pair[0], pair[1])).take(16).collect()
    }
}

/// What a program file's version resource says: its original file name and
/// its maker. Nothing, when it has none.
pub(crate) fn version_names(path: &str) -> VersionNames {
    let file = wide(path);
    // SAFETY: the size call reads the path; the data call writes at most
    // `size` bytes into a buffer at least that big, aligned for the
    // resource's structures.
    unsafe {
        let size = GetFileVersionInfoSizeW(PCWSTR(file.as_ptr()), None);
        if size == 0 || size > MAX_VERSION_BYTES {
            return VersionNames::default();
        }
        let mut data = vec![0u32; (size as usize).div_ceil(4)];
        if GetFileVersionInfoW(PCWSTR(file.as_ptr()), None, size, data.as_mut_ptr() as *mut c_void).is_err() {
            return VersionNames::default();
        }
        for (language, code_page) in translation_order(&translations(&data)) {
            let original_name = query_string(&data, &string_query(language, code_page, "OriginalFilename"));
            let maker = query_string(&data, &string_query(language, code_page, "CompanyName"));
            if original_name.is_some() || maker.is_some() {
                return VersionNames { original_name, maker };
            }
        }
        VersionNames::default()
    }
}

/// Ends a process, after checking again, on the process itself, that it is
/// the one the list showed: in the same session (never session 0), started
/// at the same moment, from the same file.
pub(crate) fn end_process(seen: &ProcessSeen) -> Result<(), String> {
    if seen.session_id == 0 {
        return Err("it is in session 0, which LiMo never touches".to_string());
    }
    if seen.started == 0 || seen.path.is_empty() {
        return Err("what it is could not be read, so LiMo cannot be sure it is the same process".to_string());
    }
    // SAFETY: the handle is closed by `Owned`; holding it keeps its process
    // id from being given to another process while it is checked.
    let handle = unsafe { OpenProcess(PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION, false, seen.pid) }
        .map_err(|e| error_text(&e))?;
    let handle = Owned::new(handle).ok_or_else(|| "it could not be opened".to_string())?;
    let mut session: u32 = 0;
    // SAFETY: writes the session id into the value handed to it.
    unsafe { ProcessIdToSessionId(seen.pid, &mut session) }.map_err(|e| error_text(&e))?;
    if session != seen.session_id || session == 0 {
        return Err("it is no longer in that session".to_string());
    }
    if started(handle.raw()) != Some(seen.started) {
        return Err("it is not the process that was seen: it started at another moment".to_string());
    }
    if !image_path(handle.raw()).is_some_and(|path| same_path(&path, &seen.path)) {
        return Err("its file is not the one that was seen".to_string());
    }
    // SAFETY: ends the process the handle names, the one just checked.
    unsafe { TerminateProcess(handle.raw(), 1) }.map_err(|e| error_text(&e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_are_tried_listed_first_then_english_each_once() {
        assert_eq!(
            translation_order(&[(0x0407, 0x04B0), (0x0409, 0x04B0)]),
            vec![(0x0407, 0x04B0), (0x0409, 0x04B0), (0x0409, 0x04E4), (0x0409, 0x0000), (0x0000, 0x04B0)]
        );
        assert_eq!(translation_order(&[]).len(), 4);
        assert_eq!(string_query(0x0409, 0x04B0, "OriginalFilename"), "\\StringFileInfo\\040904b0\\OriginalFilename");
    }

    /// Reads this test process's own entry: its session, start, path and
    /// program file. Only reads.
    #[test]
    fn this_processs_own_entry_reads() {
        let me = process(std::process::id()).unwrap();
        assert_eq!(me.pid, std::process::id());
        assert_ne!(me.started, 0);
        let exe = std::env::current_exe().unwrap();
        assert!(same_path(&me.path, &exe.to_string_lossy()), "{} {}", me.path, exe.display());
        assert!(file_stamp(&me.path).is_some());
        // A test program has no version resource.
        assert_eq!(version_names(&me.path), VersionNames::default());
        // Nothing is ended for an entry that cannot be checked.
        let unknown = ProcessSeen { started: 0, ..me.clone() };
        assert!(end_process(&unknown).is_err());
        let session_zero = ProcessSeen { session_id: 0, ..me };
        assert!(end_process(&session_zero).is_err());
    }
}
