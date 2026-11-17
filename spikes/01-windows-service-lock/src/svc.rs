//! The Windows service. It runs as LocalSystem, reads the control file every
//! 100 ms tick, and keeps exactly one helper overlay alive in the active
//! console session while the lock holds.

use std::collections::HashSet;
use std::ffi::{c_void, OsString};
use std::path::Path;
use std::ptr::null_mut;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use windows::core::{Result as WinResult, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, LocalFree, HANDLE, HLOCAL, LUID, WAIT_OBJECT_0};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
    ConvertStringSidToSidW, SDDL_REVISION_1,
};
use windows::Win32::Security::{
    AdjustTokenPrivileges, DuplicateTokenEx, GetLengthSid, GetTokenInformation,
    LookupPrivilegeValueW, SecurityImpersonation, SetTokenInformation, TokenIntegrityLevel,
    TokenPrimary, TokenSessionId, TokenUIAccess, TokenUser, LUID_AND_ATTRIBUTES, PSECURITY_DESCRIPTOR,
    PSID, SECURITY_ATTRIBUTES, SE_ASSIGNPRIMARYTOKEN_NAME, SE_INCREASE_QUOTA_NAME,
    SE_PRIVILEGE_ENABLED, SE_TCB_NAME, SID_AND_ATTRIBUTES, TOKEN_ADJUST_DEFAULT,
    TOKEN_ADJUST_PRIVILEGES, TOKEN_ADJUST_SESSIONID, TOKEN_ALL_ACCESS, TOKEN_ASSIGN_PRIMARY,
    TOKEN_DUPLICATE, TOKEN_MANDATORY_LABEL, TOKEN_PRIVILEGES, TOKEN_QUERY, TOKEN_USER,
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

use crate::band::{IL_MEDIUM, IL_MEDIUM_PLUS};
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

/// The attribute that marks a SID as an integrity label (winnt.h's
/// SE_GROUP_INTEGRITY). Defined here rather than pulling in the large
/// SystemServices part of the windows crate for one number.
const SE_GROUP_INTEGRITY: u32 = 0x20;

/// A helper process we launched and are keeping an eye on.
struct Helper {
    handle: HANDLE,
    pid: u32,
    session: u32,
}

/// What the user may do to the guarded helper's process object: SYNCHRONIZE
/// (wait for it) and PROCESS_QUERY_LIMITED_INFORMATION (see it). Nothing
/// else, so not end it, and not rewrite its access list.
const USER_PROCESS_RIGHTS: u32 = 0x0010_1000;

/// What a launch did to the helper token's integrity level, as the launch
/// line shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GuardStep {
    /// The mode leaves the integrity level as the token had it.
    Unchanged,
    /// Raised to medium plus.
    Set,
    /// Raising it failed, and the helper was launched anyway.
    Failed,
}

impl GuardStep {
    fn as_str(self) -> &'static str {
        match self {
            GuardStep::Unchanged => "unchanged",
            GuardStep::Set => "set",
            GuardStep::Failed => "failed",
        }
    }

    /// The step for the result of raising the level, or None when the mode
    /// did not try.
    fn from_result<E>(result: Option<&Result<(), E>>) -> GuardStep {
        match result {
            None => GuardStep::Unchanged,
            Some(Ok(())) => GuardStep::Set,
            Some(Err(_)) => GuardStep::Failed,
        }
    }
}

/// The two protective access lists for the guarded helper's process object,
/// tried in this order; the first that launches is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DaclVariant {
    /// Owner Administrators, so the user is not the owner (admin_owner_sddl).
    AdminOwner,
    /// No explicit owner; an OWNER RIGHTS entry caps whatever owner Windows
    /// picks to the same rights as the user (owner_rights_sddl).
    OwnerRights,
}

impl DaclVariant {
    /// The order the variants are tried in.
    const ORDER: [DaclVariant; 2] = [DaclVariant::AdminOwner, DaclVariant::OwnerRights];

    /// The name the log and the launch line use.
    fn name(self) -> &'static str {
        match self {
            DaclVariant::AdminOwner => "admin-owner",
            DaclVariant::OwnerRights => "owner-rights",
        }
    }

    /// The variant tried after this one, if any.
    fn next(self) -> Option<DaclVariant> {
        let at = Self::ORDER.iter().position(|&v| v == self)?;
        Self::ORDER.get(at + 1).copied()
    }

    /// This variant's SDDL for the given user SID and label level.
    fn sddl(self, user_sid: &str, level: u32) -> String {
        match self {
            DaclVariant::AdminOwner => admin_owner_sddl(user_sid, level),
            DaclVariant::OwnerRights => owner_rights_sddl(user_sid, level),
        }
    }
}

/// Where trying one variant went wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DaclStage {
    /// Its security descriptor could not be built.
    Build,
    /// CreateProcessAsUserW failed with it.
    Launch,
}

/// One variant that did not launch, and why.
struct DaclFailure {
    variant: DaclVariant,
    stage: DaclStage,
    error: windows::core::Error,
}

impl DaclFailure {
    /// The log message, once per lock. It names the variant, so one hardware
    /// run shows which variants Windows accepts.
    fn message(&self) -> String {
        let v = self.variant.name();
        let err = win_error_text(&self.error);
        let what = match self.stage {
            DaclStage::Build => {
                format!("could not build the helper's protective access list, variant {v}: {err}")
            }
            DaclStage::Launch => format!(
                "launching the helper with its protective access list, variant {v}, failed: {err}"
            ),
        };
        let then = match self.variant.next() {
            Some(n) => format!("Trying variant {} next.", n.name()),
            None => "Launching it without one, so the user's ordinary programs may be able to \
                     end it."
                .to_string(),
        };
        format!("{what}. {then} (repeats of this error are not logged)")
    }
}

/// What the guarded launch did about the process access list.
struct DaclResult {
    /// The variant the helper was launched with, or None when every variant
    /// failed and it was launched without one.
    applied: Option<DaclVariant>,
    /// Every variant that failed on the way, in the order tried.
    failures: Vec<DaclFailure>,
}

/// The launch line's `dacl=` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DaclField {
    /// The mode leaves the process access list as Windows makes it.
    Unchanged,
    /// The helper was launched with this variant.
    Applied(DaclVariant),
    /// Every variant failed, and the helper was launched without one.
    Failed,
}

impl DaclField {
    fn as_str(self) -> &'static str {
        match self {
            DaclField::Unchanged => "unchanged",
            DaclField::Applied(v) => v.name(),
            DaclField::Failed => "failed",
        }
    }

    fn from_result(dacl: Option<&DaclResult>) -> DaclField {
        match dacl {
            None => DaclField::Unchanged,
            Some(r) => match r.applied {
                Some(v) => DaclField::Applied(v),
                None => DaclField::Failed,
            },
        }
    }
}

/// Try each variant in order and keep the first that launches. `attempt`
/// builds one variant's descriptor and launches with it. Returns what
/// launched and with which variant (None when every variant failed), and every
/// failure on the way, in order.
fn first_variant_that_launches<T, E>(
    mut attempt: impl FnMut(DaclVariant) -> Result<T, (DaclStage, E)>,
) -> (Option<(T, DaclVariant)>, Vec<(DaclVariant, DaclStage, E)>) {
    let mut failures = Vec::new();
    for variant in DaclVariant::ORDER {
        match attempt(variant) {
            Ok(launched) => return (Some((launched, variant)), failures),
            Err((stage, e)) => failures.push((variant, stage, e)),
        }
    }
    (None, failures)
}

/// What a launch did besides starting the helper. A failure in any step does
/// not stop the launch: a weaker lock is better than no lock.
struct LaunchSteps {
    uiaccess: WinResult<()>,
    /// None when the mode leaves the integrity level unchanged.
    integrity: Option<WinResult<()>>,
    /// None when the mode leaves the process access list as Windows makes it.
    dacl: Option<DaclResult>,
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
    let mut last_integrity_error: Option<String> = None;
    // Several access list variants can fail in one launch, so every message
    // already logged in this lock is kept, not just the last one.
    let mut logged_dacl_errors: HashSet<String> = HashSet::new();
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
                    Ok((h, steps)) => {
                        let gap_ms = died_at.take().map(|t| t.elapsed().as_millis());
                        log(&launch_line(
                            h.pid,
                            session,
                            mode.as_str(),
                            steps.uiaccess.is_ok(),
                            GuardStep::from_result(steps.integrity.as_ref()),
                            DaclField::from_result(steps.dacl.as_ref()),
                            gap_ms,
                        ));
                        match steps.uiaccess {
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
                        // Only the guarded mode tries these two; the others
                        // leave them None.
                        match steps.integrity {
                            Some(Err(e)) => {
                                let msg = win_error_text(&e);
                                if last_integrity_error.as_deref() != Some(msg.as_str()) {
                                    log(&format!(
                                        "could not raise the helper token to medium plus \
                                         integrity: {msg}. The helper was launched without \
                                         it, at the user's own level, so the user's ordinary \
                                         programs can inject into it or post messages to it \
                                         (repeats of this error are not logged)"
                                    ));
                                    last_integrity_error = Some(msg);
                                }
                            }
                            _ => last_integrity_error = None,
                        }
                        match steps.dacl {
                            Some(result) if !result.failures.is_empty() => {
                                for failure in &result.failures {
                                    let msg = failure.message();
                                    if !logged_dacl_errors.contains(&msg) {
                                        log(&msg);
                                        logged_dacl_errors.insert(msg);
                                    }
                                }
                            }
                            _ => logged_dacl_errors.clear(),
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
            last_integrity_error = None;
            logged_dacl_errors.clear();
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
/// UIAccess flag was set on the helper's token, `integrity` what was done to
/// its integrity level, and `dacl` which process access list it got.
fn launch_line(
    pid: u32,
    session: u32,
    mode: &str,
    uiaccess_set: bool,
    integrity: GuardStep,
    dacl: DaclField,
    gap_ms: Option<u128>,
) -> String {
    let ui = if uiaccess_set { 1 } else { 0 };
    let il = integrity.as_str();
    let acl = dacl.as_str();
    match gap_ms {
        Some(gap) => format!(
            "relaunched helper pid={pid} session={session} mode={mode} uiaccess={ui} \
             integrity={il} dacl={acl} gap_ms={gap} (death noticed within one {TICK_MS} ms tick)"
        ),
        None => format!(
            "helper launch pid={pid} session={session} mode={mode} uiaccess={ui} \
             integrity={il} dacl={acl}"
        ),
    }
}

/// The mandatory label SID for an integrity level, for example S-1-16-8448
/// for medium plus (0x2100).
fn integrity_sid_text(level: u32) -> String {
    format!("S-1-16-{level}")
}

/// The access list both variants share: SYSTEM and Administrators get full
/// access, the user only USER_PROCESS_RIGHTS. Protected (P), so nothing is
/// inherited into it.
fn guarded_dacl(user_sid: &str) -> String {
    format!("D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;{USER_PROCESS_RIGHTS:#x};;;{user_sid})")
}

/// The label both variants share: no write up and no read up at `level`.
fn guarded_label(level: u32) -> String {
    format!("S:(ML;;NWNR;;;{})", integrity_sid_text(level))
}

/// Variant admin-owner, tried first. Owner Administrators and group SYSTEM,
/// so the user is not the owner and gets no implied right to rewrite the
/// access list.
fn admin_owner_sddl(user_sid: &str, level: u32) -> String {
    format!("O:BAG:SY{}{}", guarded_dacl(user_sid), guarded_label(level))
}

/// Variant owner-rights, tried second. No explicit owner or group, so Windows
/// picks them (it may make the user the owner); an OWNER RIGHTS (OW) entry
/// then caps the owner's implied rights to the same as the user's, so being
/// the owner gives no right to rewrite the access list.
fn owner_rights_sddl(user_sid: &str, level: u32) -> String {
    let dacl = guarded_dacl(user_sid);
    format!(
        "{dacl}(A;;{USER_PROCESS_RIGHTS:#x};;;OW){}",
        guarded_label(level)
    )
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

/// Start a helper. Alongside it come the results of the other steps (UIAccess,
/// and in guarded mode the integrity level and the process access list): a
/// failure there does not stop the launch, because a weaker lock is better
/// than no lock.
unsafe fn launch_helper(
    mode: HelperMode,
    session: u32,
    exe: &Path,
) -> WinResult<(Helper, LaunchSteps)> {
    let (pi, steps) = match mode {
        HelperMode::User => {
            let (pi, uiaccess) = create_as_user(session, exe)?;
            let steps = LaunchSteps {
                uiaccess,
                integrity: None,
                dacl: None,
            };
            (pi, steps)
        }
        HelperMode::System => {
            let (pi, uiaccess) = create_as_system(session, exe)?;
            let steps = LaunchSteps {
                uiaccess,
                integrity: None,
                dacl: None,
            };
            (pi, steps)
        }
        HelperMode::Guarded => create_guarded(session, exe)?,
    };
    // We only need to watch the process; the thread handle can go.
    let _ = CloseHandle(pi.hThread);
    let helper = Helper {
        handle: pi.hProcess,
        pid: pi.dwProcessId,
        session,
    };
    Ok((helper, steps))
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

/// A string as a NUL terminated UTF 16 buffer for a PCWSTR argument.
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Raise a primary token's integrity level to medium plus (S-1-16-8448), one
/// step above the user's ordinary programs. Raising a level needs
/// SeTcbPrivilege, which the service enables at start.
unsafe fn set_medium_plus(token: HANDLE) -> WinResult<()> {
    let text = wide(&integrity_sid_text(IL_MEDIUM_PLUS));
    let mut sid = PSID::default();
    ConvertStringSidToSidW(PCWSTR(text.as_ptr()), &mut sid)?;

    let label = TOKEN_MANDATORY_LABEL {
        Label: SID_AND_ATTRIBUTES {
            Sid: sid,
            Attributes: SE_GROUP_INTEGRITY,
        },
    };
    let result = SetTokenInformation(
        token,
        TokenIntegrityLevel,
        &label as *const TOKEN_MANDATORY_LABEL as *const c_void,
        std::mem::size_of::<TOKEN_MANDATORY_LABEL>() as u32 + GetLengthSid(sid),
    );
    // ConvertStringSidToSidW allocated the SID with LocalAlloc.
    let _ = LocalFree(Some(HLOCAL(sid.0)));
    result
}

/// The string form (S-1-5-21-...) of the user a token belongs to.
unsafe fn token_user_sid_text(token: HANDLE) -> WinResult<String> {
    // Room for TOKEN_USER and the largest possible SID (68 bytes). The u64
    // storage keeps it aligned for the TOKEN_USER read below.
    let mut buf = [0u64; 16];
    let mut returned: u32 = 0;
    GetTokenInformation(
        token,
        TokenUser,
        Some(buf.as_mut_ptr() as *mut c_void),
        std::mem::size_of_val(&buf) as u32,
        &mut returned,
    )?;
    let user = &*(buf.as_ptr() as *const TOKEN_USER);
    let mut text = PWSTR::null();
    ConvertSidToStringSidW(user.User.Sid, &mut text)?;
    let sid = String::from_utf16_lossy(text.as_wide());
    // ConvertSidToStringSidW allocated the text with LocalAlloc.
    let _ = LocalFree(Some(HLOCAL(text.0 as *mut c_void)));
    Ok(sid)
}

/// Build one variant of the guarded helper's process security descriptor for
/// the user the token belongs to. The caller frees it with LocalFree.
unsafe fn guarded_descriptor(
    token: HANDLE,
    variant: DaclVariant,
    level: u32,
) -> WinResult<PSECURITY_DESCRIPTOR> {
    let user = token_user_sid_text(token)?;
    let text = wide(&variant.sddl(&user, level));
    let mut sd = PSECURITY_DESCRIPTOR::default();
    ConvertStringSecurityDescriptorToSecurityDescriptorW(
        PCWSTR(text.as_ptr()),
        SDDL_REVISION_1,
        &mut sd,
        None,
    )?;
    Ok(sd)
}

/// The logged in user's token for the session, duplicated to a primary token.
/// The caller closes it.
unsafe fn user_primary_token(session: u32) -> WinResult<HANDLE> {
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
    Ok(primary)
}

/// Launch the helper as the logged in user.
unsafe fn create_as_user(
    session: u32,
    exe: &Path,
) -> WinResult<(PROCESS_INFORMATION, WinResult<()>)> {
    let primary = user_primary_token(session)?;

    let uiaccess = set_uiaccess(primary);
    let pi = spawn_with_token(primary, exe, None);
    let _ = CloseHandle(primary);
    Ok((pi?, uiaccess))
}

/// Launch the helper as the logged in user, with UIAccess, at medium plus
/// integrity, and with a protective access list on its process object. It
/// keeps the user's own rights, so a bug in it yields nothing more. The two
/// protections divide the work: the integrity level stops the user's ordinary
/// medium integrity programs from injecting into it, posting messages to it,
/// tampering with its threads or rewriting its access list; the access list is
/// what takes away the user's right to end it. The access list variants are
/// tried in order (DaclVariant::ORDER) and the first that launches is kept. If
/// raising the level fails, or every variant fails, the helper is launched
/// anyway, without that protection.
unsafe fn create_guarded(
    session: u32,
    exe: &Path,
) -> WinResult<(PROCESS_INFORMATION, LaunchSteps)> {
    let primary = user_primary_token(session)?;

    let uiaccess = set_uiaccess(primary);
    let integrity = set_medium_plus(primary);
    // The process label matches the level the token really has: medium plus,
    // or the user's own medium if raising it failed. A label above the
    // token's own level could make the launch fail.
    let label_level = if integrity.is_ok() {
        IL_MEDIUM_PLUS
    } else {
        IL_MEDIUM
    };

    // Try each access list variant in order. The descriptor is set at
    // creation, so there is no moment when the helper runs unprotected. Any
    // failure, to build a variant or to launch with it, moves on to the next.
    let (launched, failed) = first_variant_that_launches(|variant| {
        let sd = guarded_descriptor(primary, variant, label_level)
            .map_err(|e| (DaclStage::Build, e))?;
        let pi = spawn_with_token(primary, exe, Some(sd));
        let _ = LocalFree(Some(HLOCAL(sd.0)));
        pi.map_err(|e| (DaclStage::Launch, e))
    });
    let failures: Vec<DaclFailure> = failed
        .into_iter()
        .map(|(variant, stage, error)| DaclFailure {
            variant,
            stage,
            error,
        })
        .collect();
    // A mistake in every descriptor must never mean no lock: launch without
    // one as the last resort.
    let (pi, applied) = match launched {
        Some((pi, variant)) => (Ok(pi), Some(variant)),
        None => (spawn_with_token(primary, exe, None), None),
    };
    let _ = CloseHandle(primary);

    let steps = LaunchSteps {
        uiaccess,
        integrity: Some(integrity),
        dacl: Some(DaclResult { applied, failures }),
    };
    Ok((pi?, steps))
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
    let pi = spawn_with_token(primary, exe, None);
    let _ = CloseHandle(primary);
    Ok((pi?, uiaccess))
}

/// Common tail of every launch path: build the environment and start the
/// helper on the interactive desktop with the given primary token. With a
/// security descriptor, the new process object gets it from the moment it is
/// created; without one (the `user` and `system` modes) it gets the default,
/// as before.
unsafe fn spawn_with_token(
    token: HANDLE,
    exe: &Path,
    process_sd: Option<PSECURITY_DESCRIPTOR>,
) -> WinResult<PROCESS_INFORMATION> {
    let process_attrs = process_sd.map(|sd| SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd.0,
        bInheritHandle: false.into(),
    });

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
        process_attrs
            .as_ref()
            .map(|a| a as *const SECURITY_ATTRIBUTES),
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

    use windows::Win32::Security::Authorization::ConvertSecurityDescriptorToStringSecurityDescriptorW;
    use windows::Win32::Security::{
        DACL_SECURITY_INFORMATION, GROUP_SECURITY_INFORMATION, LABEL_SECURITY_INFORMATION,
        OWNER_SECURITY_INFORMATION,
    };
    use windows::Win32::System::Threading::{
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
        PROCESS_WRITE_DAC,
    };

    fn denied() -> windows::core::Error {
        windows::core::Error::from_hresult(windows::core::HRESULT(0x8007_0005_u32 as i32))
    }

    fn invalid_owner() -> windows::core::Error {
        windows::core::Error::from_hresult(windows::core::HRESULT(0x8007_051B_u32 as i32))
    }

    const NONE: DaclField = DaclField::Unchanged;

    #[test]
    fn launch_lines_say_whether_uiaccess_was_set() {
        assert_eq!(
            launch_line(4242, 1, "user", true, GuardStep::Unchanged, NONE, None),
            "helper launch pid=4242 session=1 mode=user uiaccess=1 integrity=unchanged \
             dacl=unchanged"
        );
        assert_eq!(
            launch_line(4242, 1, "system", false, GuardStep::Unchanged, NONE, None),
            "helper launch pid=4242 session=1 mode=system uiaccess=0 integrity=unchanged \
             dacl=unchanged"
        );
    }

    #[test]
    fn relaunch_lines_keep_the_gap_and_say_uiaccess() {
        assert_eq!(
            launch_line(77, 2, "user", true, GuardStep::Unchanged, NONE, Some(35)),
            "relaunched helper pid=77 session=2 mode=user uiaccess=1 integrity=unchanged \
             dacl=unchanged gap_ms=35 (death noticed within one 100 ms tick)"
        );
        assert_eq!(
            launch_line(77, 2, "user", false, GuardStep::Unchanged, NONE, Some(0)),
            "relaunched helper pid=77 session=2 mode=user uiaccess=0 integrity=unchanged \
             dacl=unchanged gap_ms=0 (death noticed within one 100 ms tick)"
        );
    }

    #[test]
    fn guarded_launch_lines_name_the_variant_that_launched() {
        let admin = DaclField::Applied(DaclVariant::AdminOwner);
        let owner = DaclField::Applied(DaclVariant::OwnerRights);
        assert_eq!(
            launch_line(4242, 1, "guarded", true, GuardStep::Set, admin, None),
            "helper launch pid=4242 session=1 mode=guarded uiaccess=1 integrity=set \
             dacl=admin-owner"
        );
        assert_eq!(
            launch_line(4242, 1, "guarded", true, GuardStep::Set, owner, None),
            "helper launch pid=4242 session=1 mode=guarded uiaccess=1 integrity=set \
             dacl=owner-rights"
        );
        assert_eq!(
            launch_line(4242, 1, "guarded", true, GuardStep::Failed, DaclField::Failed, None),
            "helper launch pid=4242 session=1 mode=guarded uiaccess=1 integrity=failed \
             dacl=failed"
        );
        // dacl sits after integrity, and both before gap_ms
        assert_eq!(
            launch_line(77, 2, "guarded", false, GuardStep::Set, owner, Some(40)),
            "relaunched helper pid=77 session=2 mode=guarded uiaccess=0 integrity=set \
             dacl=owner-rights gap_ms=40 (death noticed within one 100 ms tick)"
        );
    }

    #[test]
    fn guard_step_follows_the_result() {
        assert_eq!(GuardStep::from_result::<()>(None), GuardStep::Unchanged);
        assert_eq!(GuardStep::from_result::<()>(Some(&Ok(()))), GuardStep::Set);
        assert_eq!(GuardStep::from_result(Some(&Err(denied()))), GuardStep::Failed);
    }

    #[test]
    fn dacl_field_follows_the_result() {
        assert_eq!(DaclField::from_result(None), DaclField::Unchanged);
        let applied = DaclResult {
            applied: Some(DaclVariant::OwnerRights),
            failures: vec![DaclFailure {
                variant: DaclVariant::AdminOwner,
                stage: DaclStage::Launch,
                error: invalid_owner(),
            }],
        };
        assert_eq!(
            DaclField::from_result(Some(&applied)),
            DaclField::Applied(DaclVariant::OwnerRights)
        );
        let none = DaclResult {
            applied: None,
            failures: Vec::new(),
        };
        assert_eq!(DaclField::from_result(Some(&none)), DaclField::Failed);
    }

    #[test]
    fn variants_are_tried_admin_owner_first_then_owner_rights() {
        assert_eq!(
            DaclVariant::ORDER,
            [DaclVariant::AdminOwner, DaclVariant::OwnerRights]
        );
        assert_eq!(DaclVariant::AdminOwner.next(), Some(DaclVariant::OwnerRights));
        assert_eq!(DaclVariant::OwnerRights.next(), None);
        assert_eq!(DaclVariant::AdminOwner.name(), "admin-owner");
        assert_eq!(DaclVariant::OwnerRights.name(), "owner-rights");
    }

    #[test]
    fn the_first_variant_that_launches_is_kept() {
        // admin-owner launches: owner-rights is never tried
        let mut tried = Vec::new();
        let (got, failed) = first_variant_that_launches::<u32, u32>(|v| {
            tried.push(v);
            Ok(1)
        });
        assert_eq!(got, Some((1, DaclVariant::AdminOwner)));
        assert!(failed.is_empty());
        assert_eq!(tried, vec![DaclVariant::AdminOwner]);

        // any launch failure on admin-owner, not only 1307, moves on
        let mut tried = Vec::new();
        let (got, failed) = first_variant_that_launches::<u32, u32>(|v| {
            tried.push(v);
            match v {
                DaclVariant::AdminOwner => Err((DaclStage::Launch, 87)),
                DaclVariant::OwnerRights => Ok(2),
            }
        });
        assert_eq!(got, Some((2, DaclVariant::OwnerRights)));
        assert_eq!(failed, vec![(DaclVariant::AdminOwner, DaclStage::Launch, 87)]);
        assert_eq!(tried, vec![DaclVariant::AdminOwner, DaclVariant::OwnerRights]);

        // so does a failure to build a variant
        let (got, failed) = first_variant_that_launches::<u32, u32>(|v| match v {
            DaclVariant::AdminOwner => Err((DaclStage::Build, 1337)),
            DaclVariant::OwnerRights => Ok(3),
        });
        assert_eq!(got, Some((3, DaclVariant::OwnerRights)));
        assert_eq!(failed, vec![(DaclVariant::AdminOwner, DaclStage::Build, 1337)]);

        // both fail: nothing launched, both failures kept in order
        let (got, failed) =
            first_variant_that_launches::<u32, u32>(|_| Err((DaclStage::Launch, 1307)));
        assert_eq!(got, None);
        assert_eq!(
            failed,
            vec![
                (DaclVariant::AdminOwner, DaclStage::Launch, 1307),
                (DaclVariant::OwnerRights, DaclStage::Launch, 1307),
            ]
        );
    }

    #[test]
    fn dacl_failures_name_the_variant_and_what_happens_next() {
        let first = DaclFailure {
            variant: DaclVariant::AdminOwner,
            stage: DaclStage::Launch,
            error: invalid_owner(),
        }
        .message();
        assert!(
            first.starts_with(
                "launching the helper with its protective access list, variant admin-owner, \
                 failed: Win32 error 1307:"
            ),
            "{first}"
        );
        assert!(first.contains("Trying variant owner-rights next."), "{first}");

        let last = DaclFailure {
            variant: DaclVariant::OwnerRights,
            stage: DaclStage::Build,
            error: denied(),
        }
        .message();
        assert!(
            last.starts_with(
                "could not build the helper's protective access list, variant owner-rights: \
                 Win32 error 5:"
            ),
            "{last}"
        );
        assert!(last.contains("Launching it without one"), "{last}");
        assert!(last.ends_with("(repeats of this error are not logged)"), "{last}");
    }

    #[test]
    fn medium_plus_label_sid_is_s_1_16_8448() {
        assert_eq!(IL_MEDIUM_PLUS, 0x2100);
        assert_eq!(integrity_sid_text(IL_MEDIUM_PLUS), "S-1-16-8448");
    }

    #[test]
    fn the_user_may_only_wait_for_and_see_the_guarded_helper() {
        assert_eq!(
            USER_PROCESS_RIGHTS,
            PROCESS_SYNCHRONIZE.0 | PROCESS_QUERY_LIMITED_INFORMATION.0
        );
        assert_eq!(USER_PROCESS_RIGHTS & PROCESS_TERMINATE.0, 0);
        assert_eq!(USER_PROCESS_RIGHTS & PROCESS_WRITE_DAC.0, 0);
    }

    #[test]
    fn admin_owner_sddl_is_the_agreed_descriptor() {
        assert_eq!(
            admin_owner_sddl("S-1-5-21-1-2-3-1001", IL_MEDIUM_PLUS),
            "O:BAG:SYD:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x101000;;;S-1-5-21-1-2-3-1001)\
             S:(ML;;NWNR;;;S-1-16-8448)"
        );
        // the label follows the level it is given
        assert_eq!(
            admin_owner_sddl("S-1-5-21-9-9-9-500", IL_MEDIUM),
            "O:BAG:SYD:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x101000;;;S-1-5-21-9-9-9-500)\
             S:(ML;;NWNR;;;S-1-16-8192)"
        );
    }

    #[test]
    fn owner_rights_sddl_has_no_owner_and_caps_the_owner() {
        assert_eq!(
            owner_rights_sddl("S-1-5-21-1-2-3-1001", IL_MEDIUM_PLUS),
            "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x101000;;;S-1-5-21-1-2-3-1001)\
             (A;;0x101000;;;OW)S:(ML;;NWNR;;;S-1-16-8448)"
        );
        assert_eq!(
            owner_rights_sddl("S-1-5-21-9-9-9-500", IL_MEDIUM),
            "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x101000;;;S-1-5-21-9-9-9-500)\
             (A;;0x101000;;;OW)S:(ML;;NWNR;;;S-1-16-8192)"
        );
        // each variant builds its own text
        assert_eq!(
            DaclVariant::AdminOwner.sddl("S-1-5-21-1-2-3-1001", IL_MEDIUM_PLUS),
            admin_owner_sddl("S-1-5-21-1-2-3-1001", IL_MEDIUM_PLUS)
        );
        assert_eq!(
            DaclVariant::OwnerRights.sddl("S-1-5-21-1-2-3-1001", IL_MEDIUM_PLUS),
            owner_rights_sddl("S-1-5-21-1-2-3-1001", IL_MEDIUM_PLUS)
        );
    }

    /// Builds the real descriptor of each variant for this test process's own
    /// user and reads it back. Only parses text into memory: no object is
    /// created or changed.
    #[test]
    fn both_guarded_descriptors_parse_for_this_user() {
        unsafe {
            let mut token = HANDLE::default();
            OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).unwrap();
            let user = token_user_sid_text(token).unwrap();
            assert!(user.starts_with("S-1-5-"), "unexpected user SID {user}");

            for variant in DaclVariant::ORDER {
                let sd = guarded_descriptor(token, variant, IL_MEDIUM_PLUS)
                    .unwrap_or_else(|e| panic!("variant {} must parse: {e}", variant.name()));
                let info = OWNER_SECURITY_INFORMATION
                    | GROUP_SECURITY_INFORMATION
                    | DACL_SECURITY_INFORMATION
                    | LABEL_SECURITY_INFORMATION;
                let mut text = PWSTR::null();
                let back = ConvertSecurityDescriptorToStringSecurityDescriptorW(
                    sd,
                    SDDL_REVISION_1,
                    info,
                    &mut text,
                    None,
                );
                let _ = LocalFree(Some(HLOCAL(sd.0)));
                back.unwrap();
                let sddl = String::from_utf16_lossy(text.as_wide());
                let _ = LocalFree(Some(HLOCAL(text.0 as *mut c_void)));
                // shown with: cargo test guarded_descriptors -- --nocapture
                println!("{} reads back as {sddl}", variant.name());

                assert!(sddl.contains("D:P(A;;GA;;;SY)(A;;GA;;;BA)"), "{sddl}");
                assert!(sddl.contains(&format!("(A;;0x101000;;;{user})")), "{sddl}");
                // the label may come back as the SID or as its SDDL alias
                assert!(
                    sddl.contains("S:(ML;;NWNR;;;S-1-16-8448)")
                        || sddl.contains("S:(ML;;NWNR;;;MP)"),
                    "{sddl}"
                );
                match variant {
                    DaclVariant::AdminOwner => {
                        assert!(sddl.starts_with("O:BAG:SYD:P"), "{sddl}");
                    }
                    DaclVariant::OwnerRights => {
                        assert!(sddl.starts_with("D:P"), "no owner or group: {sddl}");
                        assert!(sddl.contains("(A;;0x101000;;;OW)"), "{sddl}");
                    }
                }
            }
            let _ = CloseHandle(token);
        }
    }
}
