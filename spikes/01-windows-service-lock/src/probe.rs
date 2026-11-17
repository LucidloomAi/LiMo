//! `probe --after N`: an unattended check of what the current account can do
//! against the installed spike. It exists because while the overlay is up the
//! keyboard is swallowed, so a tester cannot type these attempts by hand.
//!
//! It only ever touches the spike itself: the LiMoSpike service, processes
//! whose image path is exactly the installed exe (never its own process, never
//! any other program), the spike's control file, and the spike's Program Files
//! folder. Every attempt is recorded as succeeded, refused (with the Win32
//! error), or not applicable (nothing there to attack).
//!
//! For each spike process it was refused permission to end, it also tries to
//! open that process with WRITE_DAC, the right a determined user would need to
//! hand themselves terminate back. It only opens and closes the handle; it
//! never changes an access list.

use std::ffi::c_void;
use std::fs::OpenOptions;
use std::io::Write;
use std::thread::sleep;
use std::time::Duration;

use windows::core::{HRESULT, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{
    GetTokenInformation, LookupAccountSidW, TokenUser, PSID, SID_NAME_USE, TOKEN_QUERY, TOKEN_USER,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken,
    QueryFullProcessImageNameW, TerminateProcess, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE, PROCESS_WRITE_DAC,
};

use windows_service::service::ServiceAccess;
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

use crate::config::{
    control_path, installed_exe, program_files_dir, same_path, win32_code_from_hresult,
    SERVICE_NAME,
};

const EXE_NAME: &str = "limo-spike.exe";
const REPORT_NAME: &str = "limo-spike-probe.txt";

// ---------------------------------------------------------------------------
// Pure logic (unit tested)
// ---------------------------------------------------------------------------

/// Clamp the requested wait to 0..=60 seconds.
pub fn clamp_probe_seconds(n: u64) -> u64 {
    n.min(60)
}

/// The result of one attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The attempt worked. The text says what happened.
    Succeeded(String),
    /// The system said no.
    Refused { code: u32, text: String },
    /// There was nothing to attack (not installed, not running, not found).
    NotApplicable(String),
}

/// Sort a Win32 error into refused or not applicable. File or path not found,
/// service does not exist, and service not running mean the target was not
/// there, which is not a refusal.
pub fn outcome_from_code(code: u32, text: &str) -> Outcome {
    match code {
        2 | 3 | 1060 | 1062 => Outcome::NotApplicable(format!("Win32 error {code}: {text}")),
        _ => Outcome::Refused {
            code,
            text: text.to_string(),
        },
    }
}

/// One report line for an attempt.
pub fn format_line(label: &str, outcome: &Outcome) -> String {
    match outcome {
        Outcome::Succeeded(detail) if detail.is_empty() => format!("{label}: SUCCEEDED"),
        Outcome::Succeeded(detail) => format!("{label}: SUCCEEDED ({detail})"),
        Outcome::Refused { code, text } => {
            format!("{label}: REFUSED (Win32 error {code}: {text})")
        }
        Outcome::NotApplicable(detail) => format!("{label}: NOT APPLICABLE ({detail})"),
    }
}

/// Count refused, succeeded and not applicable outcomes, in that order.
pub fn tally(outcomes: &[Outcome]) -> (usize, usize, usize) {
    outcomes.iter().fold((0, 0, 0), |(r, s, n), o| match o {
        Outcome::Refused { .. } => (r + 1, s, n),
        Outcome::Succeeded(_) => (r, s + 1, n),
        Outcome::NotApplicable(_) => (r, s, n + 1),
    })
}

/// The closing summary line.
pub fn summary_line(refused: usize, succeeded: usize, not_applicable: usize) -> String {
    format!(
        "summary: {refused} refused, {succeeded} succeeded, {not_applicable} not applicable"
    )
}

/// The attempts recorded for one spike process: the attempt to end it, then,
/// only when that was refused, the attempt to open it to rewrite its access
/// list. A process that was ended has nothing left to attack.
pub fn process_attempts<F>(
    pid: u32,
    end_label: String,
    end: Outcome,
    try_write_dac: F,
) -> Vec<(String, Outcome)>
where
    F: FnOnce(u32) -> Outcome,
{
    let refused = matches!(end, Outcome::Refused { .. });
    let mut attempts = vec![(end_label, end)];
    if refused {
        attempts.push((
            format!("b. open pid {pid} to rewrite its access list"),
            try_write_dac(pid),
        ));
    }
    attempts
}

// ---------------------------------------------------------------------------
// The probe itself
// ---------------------------------------------------------------------------

/// Run the probe. Always exits 0: the report is the result.
pub fn run(after_secs: u64) -> i32 {
    let wait = clamp_probe_seconds(after_secs);
    println!("probe: waiting {wait} seconds before attempting. The report will also be saved to %TEMP%\\{REPORT_NAME}.");
    let _ = std::io::stdout().flush();
    sleep(Duration::from_secs(wait));

    let installed = installed_exe();
    let installed_s = installed.to_string_lossy().to_string();
    let me = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    let elevated = crate::is_elevated();
    let who = unsafe { account_of(GetCurrentProcess()) }.unwrap_or_else(|c| format!("unknown ({c})"));

    let mut lines: Vec<String> = vec![
        "LiMo spike probe report".to_string(),
        format!("time: {}", crate::logging::timestamp()),
        format!("account: {who} (elevated: {})", if elevated { "yes" } else { "no" }),
        format!("probe binary: {me}"),
        format!("installed exe: {installed_s}"),
        format!("waited {wait} seconds before attempting"),
    ];
    if elevated {
        lines.push(
            "note: this probe is running elevated, so its attempts may really succeed and \
             change the spike install. It is meant to run as a standard account."
                .to_string(),
        );
    }
    if same_path(&me, &installed_s) {
        lines.push(
            "note: this probe is running from the installed exe itself, so attempt e is \
             confounded (a running exe cannot be deleted by anyone). Copy it to %TEMP% and run \
             the copy for a clean result."
                .to_string(),
        );
    }
    lines.push(String::new());

    let mut outcomes: Vec<Outcome> = Vec::new();
    let mut record = |lines: &mut Vec<String>, label: String, outcome: Outcome| {
        lines.push(format_line(&label, &outcome));
        outcomes.push(outcome);
    };

    // a. stop the service through the service control manager
    record(
        &mut lines,
        format!("a. stop service {SERVICE_NAME}"),
        attempt_stop_service(),
    );

    // b. end every other process running the installed exe
    let (proc_results, notes) = unsafe { attempt_end_processes(&installed_s) };
    for (label, outcome) in proc_results {
        record(&mut lines, label, outcome);
    }
    lines.extend(notes);

    // c. overwrite the control file with 0
    let control = control_path();
    record(
        &mut lines,
        format!("c. overwrite {} with 0", control.display()),
        attempt_overwrite_control(&control),
    );

    // d. create a file inside the install folder
    let folder = program_files_dir();
    record(
        &mut lines,
        format!("d. create a file in {}", folder.display()),
        attempt_create_in(&folder),
    );

    // e. delete the installed exe
    record(
        &mut lines,
        format!("e. delete {installed_s}"),
        match std::fs::remove_file(&installed) {
            Ok(()) => Outcome::Succeeded("the installed exe was deleted".to_string()),
            Err(e) => outcome_from_io(&e),
        },
    );

    let (refused, succeeded, na) = tally(&outcomes);
    lines.push(String::new());
    lines.push(summary_line(refused, succeeded, na));

    for line in &lines {
        println!("{line}");
    }

    let report = std::env::temp_dir().join(REPORT_NAME);
    let mut text = lines.join("\r\n");
    text.push_str("\r\n");
    match std::fs::write(&report, text) {
        Ok(()) => println!("report written to {}", report.display()),
        Err(e) => println!("could not write the report to {}: {e}", report.display()),
    }
    0
}

fn win32_text(code: u32) -> String {
    HRESULT::from_win32(code).message().trim().to_string()
}

fn outcome_from_io(e: &std::io::Error) -> Outcome {
    match e.raw_os_error() {
        Some(c) => outcome_from_code(c as u32, &win32_text(c as u32)),
        None => Outcome::Refused {
            code: 0,
            text: e.to_string(),
        },
    }
}

fn outcome_from_service_error(e: &windows_service::Error) -> Outcome {
    match e {
        windows_service::Error::Winapi(io) => outcome_from_io(io),
        other => Outcome::Refused {
            code: 0,
            text: other.to_string(),
        },
    }
}

fn outcome_from_win(e: &windows::core::Error) -> Outcome {
    let code = win32_code_from_hresult(e.code().0);
    outcome_from_code(code, e.message().trim())
}

fn attempt_stop_service() -> Outcome {
    let manager = match ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
    {
        Ok(m) => m,
        Err(e) => return outcome_from_service_error(&e),
    };
    let service = match manager.open_service(
        SERVICE_NAME,
        ServiceAccess::STOP | ServiceAccess::QUERY_STATUS,
    ) {
        Ok(s) => s,
        Err(e) => return outcome_from_service_error(&e),
    };
    match service.stop() {
        Ok(_) => Outcome::Succeeded("the stop control was accepted".to_string()),
        Err(e) => outcome_from_service_error(&e),
    }
}

fn attempt_overwrite_control(path: &std::path::Path) -> Outcome {
    // No create flag: a missing file stays missing.
    match OpenOptions::new().write(true).truncate(true).open(path) {
        Ok(mut f) => match f.write_all(b"0") {
            Ok(()) => Outcome::Succeeded("the file now holds 0, so the lock is cleared".to_string()),
            Err(e) => outcome_from_io(&e),
        },
        Err(e) => outcome_from_io(&e),
    }
}

fn attempt_create_in(folder: &std::path::Path) -> Outcome {
    let path = folder.join("probe-write-test.txt");
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(f) => {
            drop(f);
            if std::fs::remove_file(&path).is_ok() {
                Outcome::Succeeded("a file was created there, then removed again".to_string())
            } else {
                Outcome::Succeeded(format!(
                    "a file was created there and could not be removed: {}",
                    path.display()
                ))
            }
        }
        Err(e) => outcome_from_io(&e),
    }
}

/// Find and try to end every other process whose image path is exactly the
/// installed exe. Returns the attempt results and any notes about processes
/// that were looked at but deliberately left alone.
unsafe fn attempt_end_processes(installed: &str) -> (Vec<(String, Outcome)>, Vec<String>) {
    let mut results: Vec<(String, Outcome)> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let own_pid = GetCurrentProcessId();

    let candidates = match same_name_pids() {
        Ok(p) => p,
        Err(e) => {
            results.push((
                "b. list running processes".to_string(),
                outcome_from_win(&e),
            ));
            return (results, notes);
        }
    };

    for pid in candidates.into_iter().filter(|&p| p != own_pid) {
        let session = {
            let mut sid: u32 = 0;
            match ProcessIdToSessionId(pid, &mut sid) {
                Ok(()) => sid.to_string(),
                Err(_) => "unknown".to_string(),
            }
        };

        // Read the image path and account with query rights only.
        let (path, account) = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) => {
                let p = image_path(h);
                let a = account_of(h);
                let _ = CloseHandle(h);
                (p, a)
            }
            Err(e) => {
                let c = win32_code_from_hresult(e.code().0);
                (Err(c), Err(format!("Win32 error {c}")))
            }
        };
        let account: String = account.unwrap_or_else(|why| format!("unknown, {why}"));
        let label = format!("b. end pid {pid} (session {session}, account {account})");

        let (label, end) = match path {
            Ok(p) if !same_path(&p, installed) => {
                notes.push(format!(
                    "   skipped pid {pid}: its image path {p} is not the installed exe, so it was not touched"
                ));
                continue;
            }
            Ok(_) => (label, terminate(pid)),
            Err(c) => {
                // The path could not be confirmed. Ask only for terminate
                // rights, to learn whether we would be refused, and never end a
                // process we could not confirm.
                let label = format!("{label}, image path unreadable (Win32 error {c})");
                let end = match OpenProcess(PROCESS_TERMINATE, false, pid) {
                    Err(e) => outcome_from_win(&e),
                    Ok(h) => {
                        let _ = CloseHandle(h);
                        Outcome::Succeeded(
                            "terminate access was granted; not used because the image \
                             path could not be confirmed"
                                .to_string(),
                        )
                    }
                };
                (label, end)
            }
        };
        results.extend(process_attempts(pid, label, end, |p| open_for_write_dac(p)));
    }

    if results.is_empty() {
        results.push((
            "b. end other limo-spike.exe processes running from the installed path".to_string(),
            Outcome::NotApplicable("no such process is running".to_string()),
        ));
    }
    (results, notes)
}

unsafe fn terminate(pid: u32) -> Outcome {
    match OpenProcess(PROCESS_TERMINATE, false, pid) {
        Err(e) => outcome_from_win(&e),
        Ok(h) => {
            // Exit code 1, the same code Task Manager's End task leaves.
            let r = TerminateProcess(h, 1);
            let _ = CloseHandle(h);
            match r {
                Ok(()) => Outcome::Succeeded("the process was ended".to_string()),
                Err(e) => outcome_from_win(&e),
            }
        }
    }
}

/// Try to open a process with WRITE_DAC, the right needed to rewrite its
/// access list and so hand yourself terminate back. Nothing is changed: a
/// handle that is granted is closed at once.
unsafe fn open_for_write_dac(pid: u32) -> Outcome {
    match OpenProcess(PROCESS_WRITE_DAC, false, pid) {
        Err(e) => outcome_from_win(&e),
        Ok(h) => {
            let _ = CloseHandle(h);
            Outcome::Succeeded(
                "write access to its access list was granted; the handle was closed and \
                 nothing was changed"
                    .to_string(),
            )
        }
    }
}

/// Process ids of every running process named limo-spike.exe. Only these are
/// ever opened, so no other program is touched.
unsafe fn same_name_pids() -> windows::core::Result<Vec<u32>> {
    let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)?;
    let mut pids = Vec::new();
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    if Process32FirstW(snap, &mut entry).is_ok() {
        loop {
            let len = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
            if name.eq_ignore_ascii_case(EXE_NAME) {
                pids.push(entry.th32ProcessID);
            }
            if Process32NextW(snap, &mut entry).is_err() {
                break;
            }
        }
    }
    let _ = CloseHandle(snap);
    Ok(pids)
}

unsafe fn image_path(h: HANDLE) -> Result<String, u32> {
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len)
        .map_err(|e| win32_code_from_hresult(e.code().0))?;
    Ok(String::from_utf16_lossy(&buf[..len as usize]))
}

/// "DOMAIN\name" of the account a process runs as.
unsafe fn account_of(process: HANDLE) -> Result<String, String> {
    let code = |e: windows::core::Error| format!("Win32 error {}", win32_code_from_hresult(e.code().0));

    let mut token = HANDLE::default();
    OpenProcessToken(process, TOKEN_QUERY, &mut token).map_err(code)?;

    let mut needed: u32 = 0;
    let _ = GetTokenInformation(token, TokenUser, None, 0, &mut needed);
    if needed == 0 {
        let _ = CloseHandle(token);
        return Err("token user size unavailable".to_string());
    }
    // u64 storage keeps the buffer aligned for the TOKEN_USER read below.
    let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
    let r = GetTokenInformation(
        token,
        TokenUser,
        Some(buf.as_mut_ptr() as *mut c_void),
        needed,
        &mut needed,
    );
    let _ = CloseHandle(token);
    r.map_err(code)?;

    let user = &*(buf.as_ptr() as *const TOKEN_USER);
    lookup_sid(user.User.Sid).map_err(code)
}

unsafe fn lookup_sid(sid: PSID) -> windows::core::Result<String> {
    let mut name = [0u16; 256];
    let mut name_len = name.len() as u32;
    let mut domain = [0u16; 256];
    let mut domain_len = domain.len() as u32;
    let mut kind = SID_NAME_USE(0);
    LookupAccountSidW(
        PCWSTR::null(),
        sid,
        Some(PWSTR(name.as_mut_ptr())),
        &mut name_len,
        Some(PWSTR(domain.as_mut_ptr())),
        &mut domain_len,
        &mut kind,
    )?;
    let n = String::from_utf16_lossy(&name[..name_len as usize]);
    let d = String::from_utf16_lossy(&domain[..domain_len as usize]);
    Ok(if d.is_empty() { n } else { format!("{d}\\{n}") })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_wait_is_clamped_to_0_60() {
        assert_eq!(clamp_probe_seconds(0), 0);
        assert_eq!(clamp_probe_seconds(20), 20);
        assert_eq!(clamp_probe_seconds(60), 60);
        assert_eq!(clamp_probe_seconds(61), 60);
        assert_eq!(clamp_probe_seconds(u64::MAX), 60);
    }

    #[test]
    fn missing_targets_are_not_applicable_not_refused() {
        for code in [2u32, 3, 1060, 1062] {
            assert!(matches!(
                outcome_from_code(code, "x"),
                Outcome::NotApplicable(_)
            ));
        }
    }

    #[test]
    fn access_denied_is_refused() {
        assert_eq!(
            outcome_from_code(5, "Access is denied."),
            Outcome::Refused {
                code: 5,
                text: "Access is denied.".to_string()
            }
        );
    }

    #[test]
    fn lines_are_formatted_plainly() {
        assert_eq!(
            format_line("a. stop service LiMoSpike", &outcome_from_code(5, "Access is denied.")),
            "a. stop service LiMoSpike: REFUSED (Win32 error 5: Access is denied.)"
        );
        assert_eq!(
            format_line("b. end pid 7", &Outcome::Succeeded("the process was ended".into())),
            "b. end pid 7: SUCCEEDED (the process was ended)"
        );
        assert_eq!(
            format_line("x", &Outcome::Succeeded(String::new())),
            "x: SUCCEEDED"
        );
        assert_eq!(
            format_line(
                "e. delete it",
                &outcome_from_code(2, "The system cannot find the file specified.")
            ),
            "e. delete it: NOT APPLICABLE (Win32 error 2: The system cannot find the file specified.)"
        );
    }

    #[test]
    fn tally_and_summary_count_each_kind() {
        let outcomes = vec![
            outcome_from_code(5, "denied"),
            outcome_from_code(5, "denied"),
            Outcome::Succeeded("ended".into()),
            outcome_from_code(1060, "no service"),
        ];
        assert_eq!(tally(&outcomes), (2, 1, 1));
        assert_eq!(
            summary_line(2, 1, 1),
            "summary: 2 refused, 1 succeeded, 1 not applicable"
        );
        assert_eq!(tally(&[]), (0, 0, 0));
    }

    fn denied() -> Outcome {
        outcome_from_code(5, "Access is denied.")
    }

    #[test]
    fn a_refused_end_is_followed_by_an_access_list_attempt() {
        let mut asked = Vec::new();
        let got = process_attempts(42, "b. end pid 42".to_string(), denied(), |pid| {
            asked.push(pid);
            denied()
        });
        assert_eq!(asked, vec![42]);
        assert_eq!(got.len(), 2);
        assert_eq!(got[1].0, "b. open pid 42 to rewrite its access list");
        assert_eq!(
            format_line(&got[1].0, &got[1].1),
            "b. open pid 42 to rewrite its access list: REFUSED (Win32 error 5: Access is denied.)"
        );
    }

    #[test]
    fn an_ended_process_gets_no_second_attempt() {
        let ended = Outcome::Succeeded("the process was ended".into());
        let got = process_attempts(7, "b. end pid 7".to_string(), ended, |_| {
            panic!("must not be tried on a process that was ended")
        });
        assert_eq!(got.len(), 1);
        // nor when terminate was granted but not used, or nothing was there
        let granted = Outcome::Succeeded("terminate access was granted; not used".into());
        assert_eq!(process_attempts(7, "x".into(), granted, |_| denied()).len(), 1);
        let gone = outcome_from_code(2, "gone");
        assert_eq!(process_attempts(7, "x".into(), gone, |_| denied()).len(), 1);
    }

    /// The summaries TESTING.md quotes, built the way run() builds them: a,
    /// then b for each spike process (the service, then the helper if a lock is
    /// up), then c, d and e.
    #[test]
    fn summaries_match_the_testing_checklist() {
        let summary = |helper_end: Option<Outcome>| {
            let mut outcomes = vec![denied()]; // a
            let service = process_attempts(1, "svc".into(), denied(), |_| denied());
            outcomes.extend(service.into_iter().map(|(_, o)| o));
            if let Some(end) = helper_end {
                let helper = process_attempts(2, "helper".into(), end, |_| denied());
                outcomes.extend(helper.into_iter().map(|(_, o)| o));
            }
            outcomes.extend([denied(), denied(), denied()]); // c, d, e
            let (r, s, n) = tally(&outcomes);
            summary_line(r, s, n)
        };
        // no lock (C1), any mode
        assert_eq!(summary(None), "summary: 6 refused, 0 succeeded, 0 not applicable");
        // lock up, user mode: the helper is ended, so it gets no second attempt
        assert_eq!(
            summary(Some(Outcome::Succeeded("the process was ended".into()))),
            "summary: 6 refused, 1 succeeded, 0 not applicable"
        );
        // lock up, system or guarded mode: everything refused
        assert_eq!(
            summary(Some(denied())),
            "summary: 8 refused, 0 succeeded, 0 not applicable"
        );
    }
}
