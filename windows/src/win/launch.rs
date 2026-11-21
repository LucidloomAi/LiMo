//! Starting a helper the guarded way, born with its channel. Ported from the
//! first spike's `guarded` mode, which held on real hardware on 17 Nov 2026,
//! with one thing added: the channel.
//!
//! The helper runs as the signed in user, with that user's own token, so a
//! bug in it yields nothing more than the user's own rights. Before it
//! starts, the service:
//!
//! 1. makes two anonymous pipes, one each way, and marks only the helper's
//!    two ends as inheritable;
//! 2. sets the UIAccess flag on the user's token, so the helper's topmost
//!    windows sit in the UIAccess band above every ordinary always on top
//!    window;
//! 3. raises the token to medium plus integrity, one step above the user's
//!    own programs, which then cannot inject into it, post messages to it,
//!    tamper with its threads or rewrite its access list;
//! 4. gives the process object an access list that lets the user only see
//!    it and wait for it (not end it), trying three variants in order;
//! 5. starts it on the user's desktop with `CreateProcessAsUserW`, handles
//!    inherited, and the list of handles it may inherit limited to those two
//!    pipe ends (`PROC_THREAD_ATTRIBUTE_HANDLE_LIST`), so nothing else of the
//!    service's reaches it. Their values are on its command line.
//!
//! A step that fails never stops the launch: a weaker lock is better than
//! no lock. Every variant of the access list refused: it starts with none.
//! The one thing it is never started without is its channel; with no
//! channel it could show nothing, and the service's fallback (disconnecting
//! a session whose lock is not confirmed) covers the session instead.

use std::ffi::c_void;
use std::path::Path;

use windows::Win32::Foundation::{CloseHandle, HANDLE, HLOCAL, LUID, LocalFree};
use windows::Win32::Security::Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, ConvertStringSidToSidW, SDDL_REVISION_1};
use windows::Win32::Security::{
    AdjustTokenPrivileges, DuplicateTokenEx, GetLengthSid, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW, PSECURITY_DESCRIPTOR, PSID,
    SE_ASSIGNPRIMARYTOKEN_NAME, SE_INCREASE_QUOTA_NAME, SE_PRIVILEGE_ENABLED, SE_TCB_NAME, SECURITY_ATTRIBUTES, SID_AND_ATTRIBUTES,
    SecurityImpersonation, SetTokenInformation, TOKEN_ADJUST_PRIVILEGES, TOKEN_ALL_ACCESS, TOKEN_MANDATORY_LABEL, TOKEN_PRIVILEGES,
    TOKEN_QUERY, TokenIntegrityLevel, TokenPrimary, TokenUIAccess,
};
use windows::Win32::System::Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock};
use windows::Win32::System::RemoteDesktop::WTSQueryUserToken;
use windows::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_UNICODE_ENVIRONMENT, CreateProcessAsUserW, DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT,
    GetCurrentProcess, InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST, OpenProcessToken, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
    PROCESS_INFORMATION, STARTUPINFOEXW, STARTUPINFOW, UpdateProcThreadAttribute,
};
use windows::core::{PCWSTR, PWSTR};

use super::pipe::{pipe, set_inheritable};
use super::sessions::token_user_sid;
use super::{Owned, error_text, wide};
use crate::about::{IL_MEDIUM, IL_MEDIUM_PLUS};

/// winnt.h's `SE_GROUP_INTEGRITY`: marks a SID as an integrity label.
const SE_GROUP_INTEGRITY: u32 = 0x20;

/// What the user may do to the guarded helper's process object: wait for
/// it (SYNCHRONIZE) and see it (PROCESS_QUERY_LIMITED_INFORMATION). Not end
/// it, and not rewrite its access list.
pub(crate) const USER_PROCESS_RIGHTS: u32 = 0x0010_1000;

/// The protective access lists for the helper's process object, tried in
/// this order; the first that launches is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DaclVariant {
    /// Owner Administrators, so the user is not the owner.
    AdminOwner,
    /// No owner named; an OWNER RIGHTS entry caps whatever owner Windows
    /// picks to the user's own rights.
    OwnerRights,
    /// The same without the label, so Windows labels the process itself.
    OwnerRightsNoLabel,
}

impl DaclVariant {
    /// The order the variants are tried in.
    pub(crate) const ORDER: [DaclVariant; 3] = [DaclVariant::AdminOwner, DaclVariant::OwnerRights, DaclVariant::OwnerRightsNoLabel];

    /// The name the log uses.
    pub(crate) fn name(self) -> &'static str {
        match self {
            DaclVariant::AdminOwner => "admin-owner",
            DaclVariant::OwnerRights => "owner-rights",
            DaclVariant::OwnerRightsNoLabel => "owner-rights-no-label",
        }
    }

    /// The variant tried after this one.
    pub(crate) fn next(self) -> Option<DaclVariant> {
        let at = Self::ORDER.iter().position(|&v| v == self)?;
        Self::ORDER.get(at + 1).copied()
    }

    /// This variant's SDDL for the user and the label level.
    pub(crate) fn sddl(self, user_sid: &str, level: u32) -> String {
        let dacl = format!("D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;{USER_PROCESS_RIGHTS:#x};;;{user_sid})");
        let label = format!("S:(ML;;NWNR;;;S-1-16-{level})");
        let owner_rights = format!("(A;;{USER_PROCESS_RIGHTS:#x};;;OW)");
        match self {
            DaclVariant::AdminOwner => format!("O:BAG:SY{dacl}{label}"),
            DaclVariant::OwnerRights => format!("{dacl}{owner_rights}{label}"),
            DaclVariant::OwnerRightsNoLabel => format!("{dacl}{owner_rights}"),
        }
    }
}

/// Where trying one variant failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DaclStage {
    /// Its security descriptor could not be built.
    Build,
    /// The launch with it failed.
    Launch,
}

/// The log line for one variant that did not launch.
pub(crate) fn dacl_failure_text(variant: DaclVariant, stage: DaclStage, error: &str) -> String {
    let name = variant.name();
    let what = match stage {
        DaclStage::Build => format!("The helper’s protective access list, variant {name}, could not be built: {error}."),
        DaclStage::Launch => format!("Starting the helper with its protective access list, variant {name}, failed: {error}."),
    };
    match variant.next() {
        Some(next) => format!("{what} Variant {} is tried next.", next.name()),
        None => format!("{what} It is started without one, so the user’s own programs may be able to end it."),
    }
}

/// What launched, with the variant it launched with.
pub(crate) type Kept<T> = Option<(T, DaclVariant)>;

/// Each variant that failed, where, and why.
pub(crate) type Failures<E> = Vec<(DaclVariant, DaclStage, E)>;

/// Tries each variant in order and keeps the first that launches, with
/// every failure on the way.
pub(crate) fn first_variant_that_launches<T, E>(
    mut attempt: impl FnMut(DaclVariant) -> Result<T, (DaclStage, E)>,
) -> (Kept<T>, Failures<E>) {
    let mut failures = Vec::new();
    for variant in DaclVariant::ORDER {
        match attempt(variant) {
            Ok(launched) => return (Some((launched, variant)), failures),
            Err((stage, e)) => failures.push((variant, stage, e)),
        }
    }
    (None, failures)
}

/// The command line a helper is started with; with `--plain` when the
/// settings file asks for the plain display.
pub(crate) fn helper_command_line(exe: &Path, read: usize, write: usize, plain: bool) -> String {
    let plain = if plain { " --plain" } else { "" };
    format!("\"{}\" helper --channel {read}:{write}{plain}", exe.display())
}

/// A helper started, with the service's ends of its channel.
pub(crate) struct Launched {
    /// The helper's process.
    pub process: Owned,
    /// Its process id.
    pub pid: u32,
    /// The end the service writes to.
    pub to_helper: Owned,
    /// The end the service reads from.
    pub from_helper: Owned,
    /// What the guarded launch did, for the log.
    pub detail: String,
    /// Each step that failed on the way, in plain words.
    pub problems: Vec<String>,
}

/// Enables a privilege on the service's own token, if it holds it.
fn enable_privilege(name: PCWSTR) {
    let mut token = HANDLE::default();
    // SAFETY: opens this process's own token; closed by `Owned`.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut token) }.is_err() {
        return;
    }
    let Some(token) = Owned::new(token) else { return };
    let mut luid = LUID::default();
    // SAFETY: reads the privilege's name and writes its id; then enables
    // it on the token owned here.
    unsafe {
        if LookupPrivilegeValueW(PCWSTR::null(), name, &mut luid).is_ok() {
            let privileges = TOKEN_PRIVILEGES {
                PrivilegeCount: 1,
                Privileges: [LUID_AND_ATTRIBUTES { Luid: luid, Attributes: SE_PRIVILEGE_ENABLED }],
            };
            let _ = AdjustTokenPrivileges(token.raw(), false, Some(&privileges), 0, None, None);
        }
    }
}

/// Enables the privileges the launch needs. LocalSystem holds them; this
/// makes sure they are on.
pub(crate) fn enable_launch_privileges() {
    enable_privilege(SE_TCB_NAME);
    enable_privilege(SE_ASSIGNPRIMARYTOKEN_NAME);
    enable_privilege(SE_INCREASE_QUOTA_NAME);
}

/// The signed in user's token for a session, as a primary token.
fn user_primary_token(session: u32) -> windows::core::Result<Owned> {
    let mut user = HANDLE::default();
    let mut primary = HANDLE::default();
    // SAFETY: both handles are owned by `Owned` at once.
    unsafe {
        WTSQueryUserToken(session, &mut user)?;
        let user = Owned::new(user).ok_or_else(windows::core::Error::from_thread)?;
        DuplicateTokenEx(user.raw(), TOKEN_ALL_ACCESS, None, SecurityImpersonation, TokenPrimary, &mut primary)?;
    }
    Owned::new(primary).ok_or_else(windows::core::Error::from_thread)
}

/// Sets the UIAccess flag on a primary token.
fn set_uiaccess(token: HANDLE) -> windows::core::Result<()> {
    let on: u32 = 1;
    // SAFETY: hands a 4 byte value of its stated size.
    unsafe { SetTokenInformation(token, TokenUIAccess, &on as *const u32 as *const c_void, 4) }
}

/// Raises a primary token's integrity level to medium plus (S-1-16-8448).
fn set_medium_plus(token: HANDLE) -> windows::core::Result<()> {
    let text = wide(&format!("S-1-16-{IL_MEDIUM_PLUS}"));
    let mut sid = PSID::default();
    // SAFETY: the SID is allocated by Windows and freed with LocalFree after
    // the call that reads it.
    unsafe {
        ConvertStringSidToSidW(PCWSTR(text.as_ptr()), &mut sid)?;
        let label = TOKEN_MANDATORY_LABEL { Label: SID_AND_ATTRIBUTES { Sid: sid, Attributes: SE_GROUP_INTEGRITY } };
        let size = std::mem::size_of::<TOKEN_MANDATORY_LABEL>() as u32 + GetLengthSid(sid);
        let result = SetTokenInformation(token, TokenIntegrityLevel, &label as *const TOKEN_MANDATORY_LABEL as *const c_void, size);
        let _ = LocalFree(Some(HLOCAL(sid.0)));
        result
    }
}

/// One variant's security descriptor for the token's user. Freed with
/// LocalFree by the caller.
fn guarded_descriptor(token: HANDLE, variant: DaclVariant, level: u32) -> windows::core::Result<PSECURITY_DESCRIPTOR> {
    let user = token_user_sid(token).ok_or_else(windows::core::Error::from_thread)?;
    let text = wide(&variant.sddl(&user, level));
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    // SAFETY: reads the text and writes a descriptor allocated by Windows.
    unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(PCWSTR(text.as_ptr()), SDDL_REVISION_1, &mut descriptor, None)? };
    Ok(descriptor)
}

/// Starts the helper with the token, on the user's desktop, inheriting
/// only `handles`, with the process descriptor when given.
fn spawn(
    token: HANDLE,
    exe: &Path,
    command_line: &str,
    descriptor: Option<PSECURITY_DESCRIPTOR>,
    handles: &[HANDLE; 2],
) -> windows::core::Result<PROCESS_INFORMATION> {
    let process_attributes = descriptor.map(|d| SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: d.0,
        bInheritHandle: false.into(),
    });
    let application = wide(&exe.to_string_lossy());
    let mut command = wide(command_line);
    let mut desktop = wide("winsta0\\default");
    let mut environment: *mut c_void = std::ptr::null_mut();
    // SAFETY: the attribute list lives in `storage` from its first use to
    // its deletion; the handle list it points to, the strings and the
    // environment live until CreateProcessAsUserW has returned.
    unsafe {
        let have_environment = CreateEnvironmentBlock(&mut environment, Some(token), false).is_ok();
        let mut size: usize = 0;
        let _ = InitializeProcThreadAttributeList(None, 1, None, &mut size);
        let mut storage = vec![0u64; size.div_ceil(8).max(1)];
        let list = LPPROC_THREAD_ATTRIBUTE_LIST(storage.as_mut_ptr() as *mut c_void);
        let result = match InitializeProcThreadAttributeList(Some(list), 1, None, &mut size) {
            Err(e) => Err(e),
            Ok(()) => {
                let result = UpdateProcThreadAttribute(
                    list,
                    0,
                    PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                    Some(handles.as_ptr() as *const c_void),
                    std::mem::size_of_val(handles),
                    None,
                    None,
                )
                .and_then(|()| {
                    let mut startup = STARTUPINFOEXW::default();
                    startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
                    startup.StartupInfo.lpDesktop = PWSTR(desktop.as_mut_ptr());
                    startup.lpAttributeList = list;
                    let mut info = PROCESS_INFORMATION::default();
                    CreateProcessAsUserW(
                        Some(token),
                        PCWSTR(application.as_ptr()),
                        Some(PWSTR(command.as_mut_ptr())),
                        process_attributes.as_ref().map(|a| a as *const SECURITY_ATTRIBUTES),
                        None,
                        true,
                        CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT | CREATE_NO_WINDOW,
                        if have_environment { Some(environment as *const c_void) } else { None },
                        PCWSTR::null(),
                        &startup as *const STARTUPINFOEXW as *const STARTUPINFOW,
                        &mut info,
                    )
                    .map(|()| info)
                });
                DeleteProcThreadAttributeList(list);
                result
            }
        };
        if have_environment {
            let _ = DestroyEnvironmentBlock(environment);
        }
        drop(storage);
        result
    }
}

/// Starts a helper in a session, the guarded way, born with its channel;
/// told to show the plain display when `plain` is set.
pub(crate) fn launch_helper(session: u32, exe: &Path, plain: bool) -> Result<Launched, String> {
    let channel = |e: windows::core::Error| format!("its channel could not be made: {}", error_text(&e));
    let (helper_reads, service_writes) = pipe().map_err(channel)?;
    let (service_reads, helper_writes) = pipe().map_err(channel)?;
    set_inheritable(&helper_reads, true).map_err(channel)?;
    set_inheritable(&helper_writes, true).map_err(channel)?;

    let token = user_primary_token(session).map_err(|e| format!("the user’s token could not be had: {}", error_text(&e)))?;
    let mut problems = Vec::new();
    let uiaccess = set_uiaccess(token.raw());
    if let Err(e) = &uiaccess {
        problems.push(format!(
            "UIAccess could not be set on the helper’s token: {}. It was started without it, so other always on top windows can show over the lock.",
            error_text(e)
        ));
    }
    let integrity = set_medium_plus(token.raw());
    if let Err(e) = &integrity {
        problems.push(format!(
            "The helper’s token could not be raised to medium plus integrity: {}. It was started at the user’s own level, so the user’s own programs can reach into it.",
            error_text(e)
        ));
    }
    let level = if integrity.is_ok() { IL_MEDIUM_PLUS } else { IL_MEDIUM };
    let handles = [helper_reads.raw(), helper_writes.raw()];
    let command_line = helper_command_line(exe, helper_reads.raw().0 as usize, helper_writes.raw().0 as usize, plain);

    let (launched, failed) = first_variant_that_launches(|variant| {
        let descriptor = guarded_descriptor(token.raw(), variant, level).map_err(|e| (DaclStage::Build, e))?;
        let started = spawn(token.raw(), exe, &command_line, Some(descriptor), &handles);
        // SAFETY: frees the descriptor made by `guarded_descriptor`.
        unsafe {
            let _ = LocalFree(Some(HLOCAL(descriptor.0)));
        }
        started.map_err(|e| (DaclStage::Launch, e))
    });
    for (variant, stage, error) in &failed {
        problems.push(dacl_failure_text(*variant, *stage, &error_text(error)));
    }
    let (started, dacl) = match launched {
        Some((info, variant)) => (Ok(info), variant.name()),
        None => (spawn(token.raw(), exe, &command_line, None, &handles), "failed"),
    };
    // The helper has its copies of its two ends now; the service's go.
    drop(helper_reads);
    drop(helper_writes);
    let info = started.map_err(|e| format!("CreateProcessAsUserW failed: {}", error_text(&e)))?;
    // SAFETY: the thread handle is not needed; the process handle is owned
    // from here.
    unsafe {
        let _ = CloseHandle(info.hThread);
    }
    let process = Owned::new(info.hProcess).ok_or_else(|| "the helper’s process handle was not given".to_string())?;
    let detail = format!(
        "uiaccess={} integrity={} dacl={dacl} channel=inherited display={}",
        if uiaccess.is_ok() { 1 } else { 0 },
        if integrity.is_ok() { "set" } else { "failed" },
        if plain { "plain" } else { "drawn" },
    );
    Ok(Launched { process, pid: info.dwProcessId, to_helper: service_writes, from_helper: service_reads, detail, problems })
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Security::Authorization::ConvertSecurityDescriptorToStringSecurityDescriptorW;
    use windows::Win32::Security::{DACL_SECURITY_INFORMATION, GROUP_SECURITY_INFORMATION, LABEL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION};
    use windows::Win32::System::Threading::{PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, PROCESS_WRITE_DAC};

    #[test]
    fn the_user_may_only_wait_for_and_see_the_helper() {
        assert_eq!(USER_PROCESS_RIGHTS, PROCESS_SYNCHRONIZE.0 | PROCESS_QUERY_LIMITED_INFORMATION.0);
        assert_eq!(USER_PROCESS_RIGHTS & PROCESS_TERMINATE.0, 0);
        assert_eq!(USER_PROCESS_RIGHTS & PROCESS_WRITE_DAC.0, 0);
    }

    #[test]
    fn the_three_descriptors_are_the_spikes_own() {
        let user = "S-1-5-21-1-2-3-1001";
        assert_eq!(
            DaclVariant::AdminOwner.sddl(user, IL_MEDIUM_PLUS),
            "O:BAG:SYD:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x101000;;;S-1-5-21-1-2-3-1001)S:(ML;;NWNR;;;S-1-16-8448)"
        );
        assert_eq!(
            DaclVariant::OwnerRights.sddl(user, IL_MEDIUM_PLUS),
            "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x101000;;;S-1-5-21-1-2-3-1001)(A;;0x101000;;;OW)S:(ML;;NWNR;;;S-1-16-8448)"
        );
        assert_eq!(
            DaclVariant::OwnerRightsNoLabel.sddl(user, IL_MEDIUM),
            "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x101000;;;S-1-5-21-1-2-3-1001)(A;;0x101000;;;OW)"
        );
        assert_eq!(DaclVariant::AdminOwner.sddl(user, IL_MEDIUM), DaclVariant::AdminOwner.sddl(user, IL_MEDIUM_PLUS).replace("8448", "8192"));
    }

    #[test]
    fn the_variants_are_tried_in_order_and_the_first_that_launches_is_kept() {
        assert_eq!(DaclVariant::AdminOwner.next(), Some(DaclVariant::OwnerRights));
        assert_eq!(DaclVariant::OwnerRightsNoLabel.next(), None);
        let (got, failed) = first_variant_that_launches::<u32, u32>(|v| match v {
            DaclVariant::AdminOwner => Err((DaclStage::Launch, 1307)),
            _ => Ok(2),
        });
        assert_eq!(got, Some((2, DaclVariant::OwnerRights)));
        assert_eq!(failed, vec![(DaclVariant::AdminOwner, DaclStage::Launch, 1307)]);
        let (got, failed) = first_variant_that_launches::<u32, u32>(|_| Err((DaclStage::Build, 5)));
        assert_eq!(got, None);
        assert_eq!(failed.len(), 3);
        // As error_text gives it: with no closing full stop of its own.
        let text = dacl_failure_text(DaclVariant::OwnerRightsNoLabel, DaclStage::Launch, "Win32 error 5: Access is denied");
        assert!(text.contains("started without one"), "{text}");
        assert!(text.contains("Access is denied. It is started") && !text.contains(".."), "{text}");
        assert!(dacl_failure_text(DaclVariant::AdminOwner, DaclStage::Build, "x").contains("Variant owner-rights is tried next."));
    }

    #[test]
    fn the_command_line_carries_the_two_handles() {
        let line = helper_command_line(Path::new(r"C:\Program Files\LiMo\limo.exe"), 524, 612, false);
        assert_eq!(line, r#""C:\Program Files\LiMo\limo.exe" helper --channel 524:612"#);
        let args: Vec<String> = ["helper", "--channel", "524:612"].iter().map(|s| s.to_string()).collect();
        assert_eq!(crate::cli::parse(&args), Ok(crate::cli::Command::Helper { channel: Some((524, 612)), plain: false }));
    }

    #[test]
    fn the_command_line_asks_for_the_plain_display_when_the_settings_do() {
        let line = helper_command_line(Path::new(r"C:\Program Files\LiMo\limo.exe"), 524, 612, true);
        assert_eq!(line, r#""C:\Program Files\LiMo\limo.exe" helper --channel 524:612 --plain"#);
        let args: Vec<String> = ["helper", "--channel", "524:612", "--plain"].iter().map(|s| s.to_string()).collect();
        assert_eq!(crate::cli::parse(&args), Ok(crate::cli::Command::Helper { channel: Some((524, 612)), plain: true }));
    }

    /// Builds each variant's descriptor for this test process's own user and
    /// reads it back. Only parses text into memory: no object is made or
    /// changed.
    #[test]
    fn every_descriptor_parses_for_this_user() {
        let mut token = HANDLE::default();
        // SAFETY: opens this process's own token for reading; closed by
        // `Owned`. The descriptors are freed with LocalFree.
        unsafe {
            OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).unwrap();
            let token = Owned::new(token).unwrap();
            let user = token_user_sid(token.raw()).unwrap();
            for variant in DaclVariant::ORDER {
                let descriptor = guarded_descriptor(token.raw(), variant, IL_MEDIUM_PLUS).unwrap();
                let parts = OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION | LABEL_SECURITY_INFORMATION;
                let mut text = PWSTR::null();
                let back = ConvertSecurityDescriptorToStringSecurityDescriptorW(descriptor, SDDL_REVISION_1, parts, &mut text, None);
                let _ = LocalFree(Some(HLOCAL(descriptor.0)));
                back.unwrap();
                let sddl = text.to_string().unwrap();
                let _ = LocalFree(Some(HLOCAL(text.0 as *mut c_void)));
                assert!(sddl.contains("D:P(A;;GA;;;SY)(A;;GA;;;BA)"), "{sddl}");
                assert!(sddl.contains(&format!("(A;;0x101000;;;{user})")), "{sddl}");
            }
        }
    }
}
