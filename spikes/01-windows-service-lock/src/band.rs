//! What a lock window actually got: the process's UIAccess flag, its integrity
//! level, and each window's z order band.
//!
//! Windows keeps windows in z order bands. Topmost windows of a process whose
//! token carries the UIAccess flag go into the UIAccess band, which sits above
//! every ordinary always on top window. The service sets that flag on the
//! helper's token; this module reads back what the helper really got, so a
//! test can see it instead of guessing.
//!
//! The band is read with user32's GetWindowBand, which is undocumented. It is
//! looked up at run time, so a missing export is reported as "unknown" and is
//! never a crash or a link error. Band numbers are undocumented too: the raw
//! number is always printed, and only 1 (the normal desktop band) and 2 (the
//! UIAccess band) are labelled.
//!
//! The integrity level is the last sub authority of the token's mandatory
//! label SID. It is printed as hex, labelled only for the known values.
//!
//! A second report line, `process_sddl=`, gives the process object's own
//! security descriptor (owner, access list and, when it can be read, the
//! integrity label) as SDDL, so a test can see who may end the helper.

use std::ffi::c_void;
use std::path::{Path, PathBuf};

use windows::core::{s, w, BOOL, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, ERROR_SUCCESS, HANDLE, HINSTANCE, HLOCAL, HWND,
};
use windows::Win32::Security::Authorization::{
    ConvertSecurityDescriptorToStringSecurityDescriptorW, GetSecurityInfo, SDDL_REVISION_1,
    SE_KERNEL_OBJECT,
};
use windows::Win32::Security::{
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TokenIntegrityLevel,
    TokenUIAccess, DACL_SECURITY_INFORMATION, LABEL_SECURITY_INFORMATION,
    OBJECT_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
    TOKEN_MANDATORY_LABEL, TOKEN_QUERY,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use crate::config::{program_data_dir, same_path, win32_code_from_hresult};

/// File name of the helper report, overwritten on each helper start.
pub const REPORT_NAME: &str = "helper-report.txt";

/// Believed band numbers. Undocumented, so only these two get a label.
pub const BAND_DESKTOP: u32 = 1;
pub const BAND_UIACCESS: u32 = 2;

/// Integrity levels, as the last sub authority of a mandatory label SID
/// (S-1-16-N). Only these get a label in the report.
pub const IL_LOW: u32 = 0x1000;
pub const IL_MEDIUM: u32 = 0x2000;
/// Medium plus, winnt.h's SECURITY_MANDATORY_MEDIUM_PLUS_RID (medium plus
/// 0x100), a level above the user's ordinary programs. The guarded helper is
/// raised to this.
pub const IL_MEDIUM_PLUS: u32 = 0x2100;
pub const IL_HIGH: u32 = 0x3000;
pub const IL_SYSTEM: u32 = 0x4000;

/// Mode names used in the report line.
pub const MODE_MANAGED: &str = "managed";
pub const MODE_DEVELOPER: &str = "developer";
pub const MODE_BANDCHECK: &str = "bandcheck";

// ---------------------------------------------------------------------------
// Pure logic (unit tested)
// ---------------------------------------------------------------------------

/// The result of reading one window's band.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Band {
    /// GetWindowBand answered with this raw band number.
    Known(u32),
    /// user32.dll has no GetWindowBand export on this system.
    NoExport,
    /// GetWindowBand was found but failed with this Win32 error.
    Failed(u32),
}

/// Text for one band: the raw number, labelled only for the two known bands.
pub fn band_label(band: Band) -> String {
    match band {
        Band::Known(BAND_DESKTOP) => format!("{BAND_DESKTOP} (normal desktop band)"),
        Band::Known(BAND_UIACCESS) => format!("{BAND_UIACCESS} (UIAccess band)"),
        Band::Known(n) => n.to_string(),
        Band::NoExport => "unknown (GetWindowBand not found in user32.dll)".to_string(),
        Band::Failed(code) => format!("unknown (GetWindowBand failed, Win32 error {code})"),
    }
}

/// Text for the UIAccess flag: 1, 0, or unknown with the Win32 error.
pub fn uiaccess_text(flag: Result<bool, u32>) -> String {
    match flag {
        Ok(true) => "1".to_string(),
        Ok(false) => "0".to_string(),
        Err(code) => format!("unknown (Win32 error {code})"),
    }
}

/// Text for an integrity level: the raw value as hex, with a name only for the
/// known levels.
pub fn integrity_label(level: u32) -> String {
    let name = match level {
        IL_LOW => Some("low"),
        IL_MEDIUM => Some("medium"),
        IL_MEDIUM_PLUS => Some("medium plus"),
        IL_HIGH => Some("high"),
        IL_SYSTEM => Some("system"),
        _ => None,
    };
    match name {
        Some(n) => format!("0x{level:04x} ({n})"),
        None => format!("0x{level:04x}"),
    }
}

/// Text for the integrity level as read back: the label, or unknown with the
/// Win32 error.
pub fn integrity_text(level: Result<u32, u32>) -> String {
    match level {
        Ok(l) => integrity_label(l),
        Err(code) => format!("unknown (Win32 error {code})"),
    }
}

/// The one line report: time, pid, mode, UIAccess flag, integrity level, and
/// each window's band.
pub fn report_line(
    time: &str,
    pid: u32,
    mode: &str,
    uiaccess: Result<bool, u32>,
    integrity: Result<u32, u32>,
    bands: &[Band],
) -> String {
    let mut line = format!(
        "time={time} pid={pid} mode={mode} uiaccess={} integrity={} windows={}",
        uiaccess_text(uiaccess),
        integrity_text(integrity),
        bands.len()
    );
    for (i, band) in bands.iter().enumerate() {
        line.push_str(&format!(" band[{}]={}", i + 1, band_label(*band)));
    }
    line
}

/// The second report line: the process object's security descriptor as SDDL,
/// or unknown with the Win32 error.
pub fn sddl_line(sddl: Result<String, u32>) -> String {
    match sddl {
        Ok(s) => format!("process_sddl={s}"),
        Err(code) => format!("process_sddl=unknown (Win32 error {code})"),
    }
}

/// The report file's text: every line, each ended with CRLF.
pub fn report_text(lines: &[String]) -> String {
    lines.iter().map(|l| format!("{l}\r\n")).collect()
}

/// The time a report says it was written, taken from its `time=` field.
pub fn report_time(text: &str) -> Option<&str> {
    let start = text.find("time=")? + "time=".len();
    // "YYYY-MM-DD HH:MM:SS" is 19 characters.
    text.get(start..start + 19)
}

/// Where the report may go, in order of preference: the spike's data folder
/// first, then the temp folder. The same folder is listed once.
pub fn report_candidates(data_dir: &Path, temp_dir: &Path) -> Vec<PathBuf> {
    let first = data_dir.join(REPORT_NAME);
    let second = temp_dir.join(REPORT_NAME);
    if same_path(&first.to_string_lossy(), &second.to_string_lossy()) {
        vec![first]
    } else {
        vec![first, second]
    }
}

/// Write `text` to the first candidate that accepts it, and return that path.
/// None when every candidate refused.
pub fn write_first<F>(candidates: &[PathBuf], text: &str, mut write: F) -> Option<PathBuf>
where
    F: FnMut(&Path, &str) -> std::io::Result<()>,
{
    candidates.iter().find(|p| write(p, text).is_ok()).cloned()
}

// ---------------------------------------------------------------------------
// Reading what the process and its windows got
// ---------------------------------------------------------------------------

/// This process's own UIAccess flag, read from its token.
pub fn own_uiaccess() -> Result<bool, u32> {
    let code = |e: windows::core::Error| win32_code_from_hresult(e.code().0);
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).map_err(code)?;
        let mut value: u32 = 0;
        let mut returned: u32 = 0;
        let r = GetTokenInformation(
            token,
            TokenUIAccess,
            Some(&mut value as *mut u32 as *mut c_void),
            std::mem::size_of::<u32>() as u32,
            &mut returned,
        );
        let _ = CloseHandle(token);
        r.map_err(code)?;
        Ok(value != 0)
    }
}

/// This process's own integrity level, read from its token: the last sub
/// authority of the mandatory label SID.
pub fn own_integrity() -> Result<u32, u32> {
    /// ERROR_INVALID_SID, for a label SID with no sub authority at all.
    const ERROR_INVALID_SID: u32 = 1337;
    let code = |e: windows::core::Error| win32_code_from_hresult(e.code().0);
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).map_err(code)?;
        // Room for the label and the largest possible SID (68 bytes). The u64
        // storage keeps it aligned for the TOKEN_MANDATORY_LABEL read below.
        let mut buf = [0u64; 16];
        let mut returned: u32 = 0;
        let r = GetTokenInformation(
            token,
            TokenIntegrityLevel,
            Some(buf.as_mut_ptr() as *mut c_void),
            std::mem::size_of_val(&buf) as u32,
            &mut returned,
        );
        let _ = CloseHandle(token);
        r.map_err(code)?;

        let label = &*(buf.as_ptr() as *const TOKEN_MANDATORY_LABEL);
        let sid = label.Label.Sid;
        let count = *GetSidSubAuthorityCount(sid);
        if count == 0 {
            return Err(ERROR_INVALID_SID);
        }
        Ok(*GetSidSubAuthority(sid, u32::from(count) - 1))
    }
}

/// This process object's own security descriptor as SDDL: the owner, the
/// access list and the integrity label. If the label cannot be read, it is
/// left out rather than failing the whole line.
pub fn own_process_sddl() -> Result<String, u32> {
    let full = OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION | LABEL_SECURITY_INFORMATION;
    let no_label = OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
    unsafe { process_sddl(full).or_else(|_| process_sddl(no_label)) }
}

/// Read the parts of this process's security descriptor named by `info` and
/// convert them to SDDL. Needs only READ_CONTROL on the process, which a
/// process always has on itself.
unsafe fn process_sddl(info: OBJECT_SECURITY_INFORMATION) -> Result<String, u32> {
    let mut sd = PSECURITY_DESCRIPTOR::default();
    let err = GetSecurityInfo(
        GetCurrentProcess(),
        SE_KERNEL_OBJECT,
        info,
        None,
        None,
        None,
        None,
        Some(&mut sd),
    );
    if err != ERROR_SUCCESS {
        return Err(err.0);
    }
    let mut text = PWSTR::null();
    let converted = ConvertSecurityDescriptorToStringSecurityDescriptorW(
        sd,
        SDDL_REVISION_1,
        info,
        &mut text,
        None,
    );
    // Both buffers come from LocalAlloc.
    let _ = LocalFree(Some(HLOCAL(sd.0)));
    converted.map_err(|e| win32_code_from_hresult(e.code().0))?;
    let sddl = String::from_utf16_lossy(text.as_wide());
    let _ = LocalFree(Some(HLOCAL(text.0 as *mut c_void)));
    Ok(sddl)
}

/// BOOL GetWindowBand(HWND hwnd, DWORD* pdwBand), undocumented.
type GetWindowBandFn = unsafe extern "system" fn(HWND, *mut u32) -> BOOL;

fn get_window_band_fn() -> Option<GetWindowBandFn> {
    unsafe {
        let user32 = GetModuleHandleW(w!("user32.dll")).ok()?;
        let found = GetProcAddress(user32, s!("GetWindowBand"))?;
        // SAFETY: both are plain "system" function pointers of the same size;
        // the signature is the one GetWindowBand is believed to have.
        Some(std::mem::transmute::<
            unsafe extern "system" fn() -> isize,
            GetWindowBandFn,
        >(found))
    }
}

/// The z order band of one of our own windows.
pub fn window_band(hwnd: HWND) -> Band {
    let Some(get_band) = get_window_band_fn() else {
        return Band::NoExport;
    };
    let mut band: u32 = 0;
    unsafe {
        if get_band(hwnd, &mut band).as_bool() {
            Band::Known(band)
        } else {
            Band::Failed(GetLastError().0)
        }
    }
}

/// Build the report for this process and the given windows: the main line,
/// then the process_sddl line.
pub fn collect_report(mode: &str, windows: &[HWND]) -> Vec<String> {
    let bands: Vec<Band> = windows.iter().map(|&h| window_band(h)).collect();
    let main = report_line(
        &crate::logging::timestamp(),
        std::process::id(),
        mode,
        own_uiaccess(),
        own_integrity(),
        &bands,
    );
    vec![main, sddl_line(own_process_sddl())]
}

// ---------------------------------------------------------------------------
// The report file
// ---------------------------------------------------------------------------

/// The two places a report may be: the data folder, then the temp folder.
pub fn report_paths() -> Vec<PathBuf> {
    report_candidates(&program_data_dir(), &std::env::temp_dir())
}

/// Save the report lines, overwriting any older report. The data folder is
/// tried first (the SYSTEM helper can write there); a helper that runs as the
/// user cannot, by design, so it falls back to the temp folder. The data
/// folder is never created here. Failures are swallowed: the report must never
/// stop or delay the lock.
pub fn write_report(lines: &[String]) -> Option<PathBuf> {
    let text = report_text(lines);
    write_first(&report_paths(), &text, |p, t| std::fs::write(p, t))
}

/// Every report that exists, with its path, data folder first.
pub fn read_reports() -> Vec<(PathBuf, String)> {
    report_paths()
        .into_iter()
        .filter_map(|p| std::fs::read_to_string(&p).ok().map(|t| (p, t)))
        .collect()
}

// ---------------------------------------------------------------------------
// bandcheck
// ---------------------------------------------------------------------------

/// Create one topmost tool window that is never shown (no WS_VISIBLE, never
/// passed to ShowWindow, 1 by 1 pixel, off screen), read this process's
/// UIAccess flag, its integrity level, its process security descriptor and
/// that window's band, destroy the window, and return the two report lines.
/// Err carries a message when the window could not be created.
pub fn bandcheck() -> Result<Vec<String>, String> {
    unsafe {
        let hinst: Option<HINSTANCE> = GetModuleHandleW(None).ok().map(|h| h.into());
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            w!("STATIC"),
            w!("LiMo spike bandcheck"),
            WS_POPUP,
            -32000,
            -32000,
            1,
            1,
            None,
            None,
            hinst,
            None,
        )
        .map_err(|e| {
            format!(
                "bandcheck: could not create the check window: Win32 error {}",
                win32_code_from_hresult(e.code().0)
            )
        })?;
        let lines = collect_report(MODE_BANDCHECK, &[hwnd]);
        let _ = DestroyWindow(hwnd);
        Ok(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_two_known_bands_get_a_label() {
        assert_eq!(band_label(Band::Known(1)), "1 (normal desktop band)");
        assert_eq!(band_label(Band::Known(2)), "2 (UIAccess band)");
        assert_eq!(band_label(Band::Known(0)), "0");
        assert_eq!(band_label(Band::Known(7)), "7");
        assert_eq!(
            band_label(Band::NoExport),
            "unknown (GetWindowBand not found in user32.dll)"
        );
        assert_eq!(
            band_label(Band::Failed(1400)),
            "unknown (GetWindowBand failed, Win32 error 1400)"
        );
    }

    #[test]
    fn uiaccess_flag_is_1_0_or_unknown() {
        assert_eq!(uiaccess_text(Ok(true)), "1");
        assert_eq!(uiaccess_text(Ok(false)), "0");
        assert_eq!(uiaccess_text(Err(5)), "unknown (Win32 error 5)");
    }

    #[test]
    fn report_line_lists_every_window_band() {
        assert_eq!(
            report_line(
                "2026-11-17 10:00:00",
                4242,
                MODE_MANAGED,
                Ok(true),
                Ok(IL_MEDIUM),
                &[Band::Known(2), Band::Known(2)]
            ),
            "time=2026-11-17 10:00:00 pid=4242 mode=managed uiaccess=1 \
             integrity=0x2000 (medium) windows=2 \
             band[1]=2 (UIAccess band) band[2]=2 (UIAccess band)"
        );
        assert_eq!(
            report_line(
                "2026-11-17 10:00:00",
                7,
                MODE_DEVELOPER,
                Ok(false),
                Ok(IL_MEDIUM),
                &[Band::Known(1), Band::NoExport]
            ),
            "time=2026-11-17 10:00:00 pid=7 mode=developer uiaccess=0 \
             integrity=0x2000 (medium) windows=2 \
             band[1]=1 (normal desktop band) \
             band[2]=unknown (GetWindowBand not found in user32.dll)"
        );
        assert_eq!(
            report_line("2026-11-17 10:00:00", 1, MODE_BANDCHECK, Err(5), Err(5), &[]),
            "time=2026-11-17 10:00:00 pid=1 mode=bandcheck uiaccess=unknown (Win32 error 5) \
             integrity=unknown (Win32 error 5) windows=0"
        );
    }

    #[test]
    fn integrity_is_hex_with_a_name_only_for_known_levels() {
        assert_eq!(integrity_label(0x1000), "0x1000 (low)");
        assert_eq!(integrity_label(0x2000), "0x2000 (medium)");
        assert_eq!(integrity_label(0x2100), "0x2100 (medium plus)");
        assert_eq!(integrity_label(0x3000), "0x3000 (high)");
        assert_eq!(integrity_label(0x4000), "0x4000 (system)");
        // anything else is the raw hex only
        assert_eq!(integrity_label(0x2010), "0x2010");
        assert_eq!(integrity_label(0x3010), "0x3010");
        assert_eq!(integrity_label(0), "0x0000");
        assert_eq!(integrity_label(0x5000), "0x5000");
    }

    #[test]
    fn integrity_text_says_unknown_with_the_error() {
        assert_eq!(IL_MEDIUM_PLUS, 0x2100);
        assert_eq!(integrity_text(Ok(IL_MEDIUM_PLUS)), "0x2100 (medium plus)");
        assert_eq!(integrity_text(Err(5)), "unknown (Win32 error 5)");
        assert_eq!(integrity_text(Err(122)), "unknown (Win32 error 122)");
    }

    #[test]
    fn guarded_report_line_puts_integrity_after_uiaccess() {
        assert_eq!(
            report_line(
                "2026-11-17 10:00:00",
                4242,
                MODE_MANAGED,
                Ok(true),
                Ok(IL_MEDIUM_PLUS),
                &[Band::Known(2), Band::Known(2)]
            ),
            "time=2026-11-17 10:00:00 pid=4242 mode=managed uiaccess=1 \
             integrity=0x2100 (medium plus) windows=2 \
             band[1]=2 (UIAccess band) band[2]=2 (UIAccess band)"
        );
    }

    #[test]
    fn report_time_is_read_back_from_the_line() {
        let line = report_line(
            "2026-11-17 10:00:00",
            1,
            MODE_MANAGED,
            Ok(true),
            Ok(IL_MEDIUM_PLUS),
            &[],
        );
        assert_eq!(report_time(&line), Some("2026-11-17 10:00:00"));
        assert_eq!(report_time(&format!("{line}\r\n")), Some("2026-11-17 10:00:00"));
        assert_eq!(report_time("no time here"), None);
        assert_eq!(report_time("time=2026"), None);
    }

    #[test]
    fn sddl_line_says_unknown_with_the_error() {
        assert_eq!(
            sddl_line(Ok("O:BAD:P(A;;0x101000;;;S-1-5-21-1-2-3-1001)".to_string())),
            "process_sddl=O:BAD:P(A;;0x101000;;;S-1-5-21-1-2-3-1001)"
        );
        assert_eq!(sddl_line(Err(5)), "process_sddl=unknown (Win32 error 5)");
    }

    #[test]
    fn a_two_line_report_keeps_its_time_and_both_lines() {
        let lines = vec![
            report_line(
                "2026-11-17 10:00:00",
                4242,
                MODE_MANAGED,
                Ok(true),
                Ok(IL_MEDIUM_PLUS),
                &[Band::Known(2)],
            ),
            sddl_line(Ok("O:BAD:P(A;;GA;;;SY)S:(ML;;NWNR;;;MP)".to_string())),
        ];
        let text = report_text(&lines);
        assert_eq!(
            text,
            "time=2026-11-17 10:00:00 pid=4242 mode=managed uiaccess=1 \
             integrity=0x2100 (medium plus) windows=1 band[1]=2 (UIAccess band)\r\n\
             process_sddl=O:BAD:P(A;;GA;;;SY)S:(ML;;NWNR;;;MP)\r\n"
        );
        // status reads the time from the first line and prints every line
        assert_eq!(report_time(&text), Some("2026-11-17 10:00:00"));
        assert_eq!(text.lines().collect::<Vec<_>>(), lines);
        // an empty report is empty text, not a stray line ending
        assert_eq!(report_text(&[]), "");
    }

    #[test]
    fn the_data_folder_is_tried_before_temp() {
        let data = Path::new(r"C:\ProgramData\LiMoSpike");
        let temp = Path::new(r"C:\Users\kid\AppData\Local\Temp");
        assert_eq!(
            report_candidates(data, temp),
            vec![data.join(REPORT_NAME), temp.join(REPORT_NAME)]
        );
        // the same folder twice is listed once, whatever the case or slashes
        assert_eq!(
            report_candidates(Path::new(r"C:\X"), Path::new("c:/x/")),
            vec![Path::new(r"C:\X").join(REPORT_NAME)]
        );
    }

    #[test]
    fn the_report_goes_to_the_first_place_that_accepts_it() {
        let data = PathBuf::from(r"C:\ProgramData\LiMoSpike\helper-report.txt");
        let temp = PathBuf::from(r"C:\Temp\helper-report.txt");
        let places = vec![data.clone(), temp.clone()];
        let denied = || std::io::Error::from(std::io::ErrorKind::PermissionDenied);

        // the SYSTEM helper: the data folder accepts it, temp is never tried
        let mut tried = Vec::new();
        let got = write_first(&places, "x", |p, _| {
            tried.push(p.to_path_buf());
            Ok(())
        });
        assert_eq!(got, Some(data.clone()));
        assert_eq!(tried, vec![data.clone()]);

        // the user mode helper: the data folder refuses, temp accepts
        let mut tried = Vec::new();
        let got = write_first(&places, "x", |p, _| {
            tried.push(p.to_path_buf());
            if p == data.as_path() {
                Err(denied())
            } else {
                Ok(())
            }
        });
        assert_eq!(got, Some(temp.clone()));
        assert_eq!(tried, vec![data.clone(), temp.clone()]);

        // nowhere writable: no path, and no panic
        assert_eq!(write_first(&places, "x", |_, _| Err(denied())), None);
    }
}
