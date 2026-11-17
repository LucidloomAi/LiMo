//! The Windows service. It runs as LocalSystem, reads the control file every
//! 100 ms tick, and keeps exactly one helper overlay alive in the active
//! console session while the lock holds.

use std::ffi::{c_void, OsString};
use std::path::Path;
use std::ptr::null_mut;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use windows::core::{Result as WinResult, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, LUID, WAIT_OBJECT_0};
use windows::Win32::Security::{
    AdjustTokenPrivileges, DuplicateTokenEx, LookupPrivilegeValueW, SecurityImpersonation,
    SetTokenInformation, TokenPrimary, TokenSessionId, TokenUIAccess, LUID_AND_ATTRIBUTES,
    SE_ASSIGNPRIMARYTOKEN_NAME, SE_INCREASE_QUOTA_NAME, SE_PRIVILEGE_ENABLED, SE_TCB_NAME,
    TOKEN_ADJUST_DEFAULT, TOKEN_ADJUST_PRIVILEGES, TOKEN_ADJUST_SESSIONID, TOKEN_ALL_ACCESS,
    TOKEN_ASSIGN_PRIMARY, TOKEN_DUPLICATE, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows::Win32::System::Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock};
use windows::Win32::System::RemoteDesktop::{WTSGetActiveConsoleSessionId, WTSQueryUserToken};
use windows::Win32::System::Threading::{
    CreateProcessAsUserW, GetCurrentProcess, GetExitCodeProcess, OpenProcessToken, TerminateProcess,
    WaitForSingleObject, CREATE_UNICODE_ENVIRONMENT, PROCESS_INFORMATION, STARTUPINFOW,
};

use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};

use crate::config::{
    config_path, control_path, deadline_too_far, installed_exe, lock_holds, now_unix,
    parse_deadline, sane_deadline, win32_code_from_hresult, HelperMode, HELPER_CAP_SECS,
    SERVICE_NAME,
};
use crate::logging::log;

/// Session id value that WTSGetActiveConsoleSessionId returns when no session
/// is attached to the physical console.
const NO_SESSION: u32 = 0xFFFF_FFFF;

/// How often the loop wakes. A dead helper is noticed within one tick, so the
/// relaunch gap the log reports is honest to within this much.
const TICK_MS: u64 = 100;

/// After a failed launch, wait this long before trying again.
const RETRY_AFTER: Duration = Duration::from_secs(1);

/// The managed overlay ends itself within a quarter second of the lock ending.
/// If it is still running this long after, the service closes it.
const STRAGGLER_GRACE: Duration = Duration::from_secs(3);

/// A helper process we launched and are keeping an eye on.
struct Helper {
    handle: HANDLE,
    pid: u32,
    session: u32,
}

windows_service::define_windows_service!(ffi_service_main, my_service_main);

/// Hand control to the service control manager. This blocks until the service
/// stops. If the program was started from a terminal rather than by the SCM,
/// the dispatcher fails and the error is returned to the caller.
pub fn run() -> windows_service::Result<()> {
    windows_service::service_dispatcher::start(SERVICE_NAME, ffi_service_main)
}

fn my_service_main(_arguments: Vec<OsString>) {
    if let Err(e) = run_service() {
        log(&format!("service error: {e}"));
    }
}

fn status(state: ServiceState, accepts: ServiceControlAccept) -> ServiceStatus {
    ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: accepts,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::from_secs(2),
        process_id: None,
    }
}

fn run_service() -> windows_service::Result<()> {
    let (shutdown_tx, shutdown_rx) = mpsc::channel();

    let event_handler = move |control| -> ServiceControlHandlerResult {
        match control {
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            ServiceControl::Stop => {
                let _ = shutdown_tx.send(());
                ServiceControlHandlerResult::NoError
            }
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)?;
    status_handle.set_service_status(status(ServiceState::Running, ServiceControlAccept::STOP))?;
    log("service started");

    // As LocalSystem these privileges are normally present already. Enabling
    // them defensively makes the token launch paths more likely to succeed.
    unsafe {
        enable_privilege(SE_TCB_NAME);
        enable_privilege(SE_ASSIGNPRIMARYTOKEN_NAME);
        enable_privilege(SE_INCREASE_QUOTA_NAME);
    }

    let exe = std::env::current_exe().unwrap_or_else(|_| installed_exe());
    let tick = Duration::from_millis(TICK_MS);

    let mut helper: Option<Helper> = None;
    // When we noticed the helper had died while the lock still held.
    let mut died_at: Option<Instant> = None;
    // When the lock stopped holding while a helper was still up.
    let mut lock_ended_at: Option<Instant> = None;
    // Earliest moment to try again after a failed launch.
    let mut retry_at: Option<Instant> = None;
    // Last values logged, so a steady state is logged once, not every tick.
    let mut last_session_logged: Option<u32> = None;
    let mut last_launch_error: Option<String> = None;
    let mut last_uiaccess_error: Option<String> = None;
    let mut last_ignored_deadline: Option<i64> = None;

    loop {
        let now = now_unix();
        let raw = read_raw_deadline();

        if deadline_too_far(raw, now) {
            if last_ignored_deadline != Some(raw) {
                log(&format!(
                    "ignoring deadline {}: it is {} s ahead, more than the {} s cap, so no lock",
                    raw,
                    raw.saturating_sub(now),
                    HELPER_CAP_SECS
                ));
                last_ignored_deadline = Some(raw);
            }
        } else {
            last_ignored_deadline = None;
        }
        let holds = lock_holds(sane_deadline(raw, now), now);

        // Reap the helper if it has exited.
        if let Some(h) = helper.as_ref() {
            if unsafe { process_exited(h.handle) } {
                let code = unsafe { exit_code(h.handle) };
                log(&format!("helper exited pid={} code={}", h.pid, code));
                unsafe {
                    let _ = CloseHandle(h.handle);
                }
                if holds {
                    died_at = Some(Instant::now());
                }
                helper = None;
            }
        }

        if holds {
            lock_ended_at = None;

            let session = unsafe { WTSGetActiveConsoleSessionId() };
            if last_session_logged != Some(session) {
                if session == NO_SESSION {
                    log("lock holds but no session is attached to the console; waiting");
                } else {
                    log(&format!("lock holds; active console session is {}", session));
                }
                last_session_logged = Some(session);
            }

            // The helper lives in a session that is no longer the active one
            // (for example after a user switch): close it and start a new one.
            if let Some(h) = helper.as_ref() {
                if session != NO_SESSION && h.session != session {
                    log(&format!(
                        "active session changed from {} to {}; closing helper pid={}",
                        h.session, session, h.pid
                    ));
                    unsafe {
                        let _ = TerminateProcess(h.handle, 0);
                        let _ = CloseHandle(h.handle);
                    }
                    helper = None;
                }
            }

            let may_try = retry_at.is_none_or(|t| Instant::now() >= t);
            if session != NO_SESSION && helper.is_none() && may_try {
                let mode = read_mode();
                match unsafe { launch_helper(mode, session, &exe) } {
                    Ok((h, uiaccess)) => {
                        let gap_ms = died_at.take().map(|t| t.elapsed().as_millis());
                        log(&launch_line(
                            h.pid,
                            session,
                            mode.as_str(),
                            uiaccess.is_ok(),
                            gap_ms,
                        ));
                        match uiaccess {
                            Ok(()) => last_uiaccess_error = None,
                            Err(e) => {
                                let msg = win_error_text(&e);
                                if last_uiaccess_error.as_deref() != Some(msg.as_str()) {
                                    log(&format!(
                                        "could not set UIAccess on the helper token: {msg}. \
                                         The helper was launched without it, so other always \
                                         on top windows can show over the lock (repeats of \
                                         this error are not logged)"
                                    ));
                                    last_uiaccess_error = Some(msg);
                                }
                            }
                        }
                        helper = Some(h);
                        retry_at = None;
                        last_launch_error = None;
                    }
                    Err(e) => {
                        let msg = win_error_text(&e);
                        if last_launch_error.as_deref() != Some(msg.as_str()) {
                            log(&format!(
                                "helper launch failed: {msg} (retrying every second, \
                                 repeats of this error are not logged)"
                            ));
                            last_launch_error = Some(msg);
                        }
                        retry_at = Some(Instant::now() + RETRY_AFTER);
                    }
                }
            }
        } else {
            died_at = None;
            retry_at = None;
            last_launch_error = None;
            last_uiaccess_error = None;
            last_session_logged = None;

            // Normally the managed overlay has already ended itself. If it is
            // still up after the grace period, close it so a stuck helper can
            // never outlive the lock.
            if let Some(h) = helper.as_ref() {
                let since = *lock_ended_at.get_or_insert_with(Instant::now);
                if since.elapsed() >= STRAGGLER_GRACE {
                    log(&format!(
                        "helper pid={} still running {} s after the lock ended; closing it",
                        h.pid,
                        STRAGGLER_GRACE.as_secs()
                    ));
                    unsafe {
                        let _ = TerminateProcess(h.handle, 0);
                        let _ = CloseHandle(h.handle);
                    }
                    helper = None;
                    lock_ended_at = None;
                }
            } else {
                lock_ended_at = None;
            }
        }

        match shutdown_rx.recv_timeout(tick) {
            Ok(_) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }

    status_handle.set_service_status(status(
        ServiceState::StopPending,
        ServiceControlAccept::empty(),
    ))?;

    if let Some(h) = helper.take() {
        unsafe {
            let _ = TerminateProcess(h.handle, 0);
            let _ = CloseHandle(h.handle);
        }
        log(&format!("stop: closed helper pid={}", h.pid));
    }

    log("service stopped");
    status_handle.set_service_status(status(
        ServiceState::Stopped,
        ServiceControlAccept::empty(),
    ))?;
    Ok(())
}

fn read_raw_deadline() -> i64 {
    std::fs::read_to_string(control_path())
        .map(|s| parse_deadline(&s))
        .unwrap_or(0)
}

fn read_mode() -> HelperMode {
    std::fs::read_to_string(config_path())
        .map(|s| HelperMode::parse(&s))
        .unwrap_or(HelperMode::User)
}

/// "Win32 error N: text" for a failed call, as the log shows it.
fn win_error_text(e: &windows::core::Error) -> String {
    format!(
        "Win32 error {}: {}",
        win32_code_from_hresult(e.code().0),
        e.message().trim()
    )
}

/// The log line for a helper start. `gap_ms` is set when this start replaces a
/// helper that died while the lock held. `uiaccess_set` says whether the
/// UIAccess flag was set on the helper's token.
fn launch_line(
    pid: u32,
    session: u32,
    mode: &str,
    uiaccess_set: bool,
    gap_ms: Option<u128>,
) -> String {
    let ui = if uiaccess_set { 1 } else { 0 };
    match gap_ms {
        Some(gap) => format!(
            "relaunched helper pid={pid} session={session} mode={mode} uiaccess={ui} \
             gap_ms={gap} (death noticed within one {TICK_MS} ms tick)"
        ),
        None => format!("helper launch pid={pid} session={session} mode={mode} uiaccess={ui}"),
    }
}

fn build_cmdline(exe: &Path) -> Vec<u16> {
    let line = format!("\"{}\" overlay --managed", exe.display());
    line.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn process_exited(handle: HANDLE) -> bool {
    WaitForSingleObject(handle, 0) == WAIT_OBJECT_0
}

unsafe fn exit_code(handle: HANDLE) -> u32 {
    let mut code: u32 = 0;
    let _ = GetExitCodeProcess(handle, &mut code);
    code
}

/// Start a helper. Alongside it comes the result of setting UIAccess on its
/// token: a failure there does not stop the launch, because a lock without
/// UIAccess is better than no lock.
unsafe fn launch_helper(
    mode: HelperMode,
    session: u32,
    exe: &Path,
) -> WinResult<(Helper, WinResult<()>)> {
    let (pi, uiaccess) = match mode {
        HelperMode::User => create_as_user(session, exe)?,
        HelperMode::System => create_as_system(session, exe)?,
    };
    // We only need to watch the process; the thread handle can go.
    let _ = CloseHandle(pi.hThread);
    let helper = Helper {
        handle: pi.hProcess,
        pid: pi.dwProcessId,
        session,
    };
    Ok((helper, uiaccess))
}

/// Set the UIAccess flag on a primary token. Topmost windows of a process with
/// this flag go into the UIAccess z order band, above every ordinary always on
/// top window. Needs SeTcbPrivilege, which the service enables at start. No
/// code signing or manifest is needed when the flag is set this way.
unsafe fn set_uiaccess(token: HANDLE) -> WinResult<()> {
    let on: u32 = 1;
    SetTokenInformation(
        token,
        TokenUIAccess,
        &on as *const u32 as *const c_void,
        std::mem::size_of::<u32>() as u32,
    )
}

/// Launch the helper as the logged in user.
unsafe fn create_as_user(
    session: u32,
    exe: &Path,
) -> WinResult<(PROCESS_INFORMATION, WinResult<()>)> {
    let mut user_token = HANDLE::default();
    WTSQueryUserToken(session, &mut user_token)?;

    let mut primary = HANDLE::default();
    let dup = DuplicateTokenEx(
        user_token,
        TOKEN_ALL_ACCESS,
        None,
        SecurityImpersonation,
        TokenPrimary,
        &mut primary,
    );
    let _ = CloseHandle(user_token);
    dup?;

    let uiaccess = set_uiaccess(primary);
    let pi = spawn_with_token(primary, exe);
    let _ = CloseHandle(primary);
    Ok((pi?, uiaccess))
}

/// Launch the helper as SYSTEM inside the user's session, so a standard user
/// cannot end it.
unsafe fn create_as_system(
    session: u32,
    exe: &Path,
) -> WinResult<(PROCESS_INFORMATION, WinResult<()>)> {
    let mut proc_token = HANDLE::default();
    OpenProcessToken(
        GetCurrentProcess(),
        TOKEN_DUPLICATE
            | TOKEN_ASSIGN_PRIMARY
            | TOKEN_QUERY
            | TOKEN_ADJUST_DEFAULT
            | TOKEN_ADJUST_SESSIONID,
        &mut proc_token,
    )?;

    let mut primary = HANDLE::default();
    let dup = DuplicateTokenEx(
        proc_token,
        TOKEN_ALL_ACCESS,
        None,
        SecurityImpersonation,
        TokenPrimary,
        &mut primary,
    );
    let _ = CloseHandle(proc_token);
    dup?;

    let sid: u32 = session;
    if let Err(e) = SetTokenInformation(
        primary,
        TokenSessionId,
        &sid as *const u32 as *const c_void,
        std::mem::size_of::<u32>() as u32,
    ) {
        let _ = CloseHandle(primary);
        return Err(e);
    }

    let uiaccess = set_uiaccess(primary);
    let pi = spawn_with_token(primary, exe);
    let _ = CloseHandle(primary);
    Ok((pi?, uiaccess))
}

/// Common tail of both launch paths: build the environment and start the
/// helper on the interactive desktop with the given primary token.
unsafe fn spawn_with_token(token: HANDLE, exe: &Path) -> WinResult<PROCESS_INFORMATION> {
    let mut env: *mut c_void = null_mut();
    let have_env = CreateEnvironmentBlock(&mut env, Some(token), false).is_ok();

    let mut desktop: Vec<u16> = "winsta0\\default"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // Built fresh for every launch: CreateProcessAsUserW may write into it.
    let mut cmdline = build_cmdline(exe);

    let mut si = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        ..Default::default()
    };
    si.lpDesktop = PWSTR(desktop.as_mut_ptr());

    let mut pi = PROCESS_INFORMATION::default();
    let res = CreateProcessAsUserW(
        Some(token),
        PCWSTR::null(),
        Some(PWSTR(cmdline.as_mut_ptr())),
        None,
        None,
        false,
        CREATE_UNICODE_ENVIRONMENT,
        if have_env {
            Some(env as *const c_void)
        } else {
            None
        },
        PCWSTR::null(),
        &si,
        &mut pi,
    );

    if have_env {
        let _ = DestroyEnvironmentBlock(env);
    }
    res?;
    Ok(pi)
}

unsafe fn enable_privilege(name: PCWSTR) {
    let mut token = HANDLE::default();
    if OpenProcessToken(
        GetCurrentProcess(),
        TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
        &mut token,
    )
    .is_err()
    {
        return;
    }

    let mut luid = LUID::default();
    if LookupPrivilegeValueW(PCWSTR::null(), name, &mut luid).is_ok() {
        let tp = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };
        let _ = AdjustTokenPrivileges(token, false, Some(&tp), 0, None, None);
    }
    let _ = CloseHandle(token);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_lines_say_whether_uiaccess_was_set() {
        assert_eq!(
            launch_line(4242, 1, "user", true, None),
            "helper launch pid=4242 session=1 mode=user uiaccess=1"
        );
        assert_eq!(
            launch_line(4242, 1, "system", false, None),
            "helper launch pid=4242 session=1 mode=system uiaccess=0"
        );
    }

    #[test]
    fn relaunch_lines_keep_the_gap_and_say_uiaccess() {
        assert_eq!(
            launch_line(77, 2, "user", true, Some(35)),
            "relaunched helper pid=77 session=2 mode=user uiaccess=1 gap_ms=35 \
             (death noticed within one 100 ms tick)"
        );
        assert_eq!(
            launch_line(77, 2, "user", false, Some(0)),
            "relaunched helper pid=77 session=2 mode=user uiaccess=0 gap_ms=0 \
             (death noticed within one 100 ms tick)"
        );
    }
}
