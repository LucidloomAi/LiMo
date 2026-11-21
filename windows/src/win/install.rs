//! `limo install`, `stop-for-upgrade` and `uninstall` (their steps are
//! decided in `crate::installer`, over `machine.rs`), `status`, `dev-setup`,
//! `dev-code` and `reset-password`: for the installer, for testing with the
//! owner at the PC, and for a family that forgot the guardian password.

use std::ffi::{OsString, c_void};
use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use limo_hub::Hub;
use windows::Win32::Foundation::{HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1};
use windows::Win32::Security::{
    GetTokenInformation, LookupAccountNameW, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES, SID_NAME_USE, SidTypeUser, TOKEN_ELEVATION,
    TOKEN_QUERY, TokenElevation,
};
use windows::Win32::Storage::FileSystem::CreateDirectoryW;
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::core::{PCWSTR, PWSTR};
use windows_service::service::{ServiceAccess, ServiceState};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

use super::clock::{utc_offset_secs, wall_secs};
use super::console::read_hidden;
use super::logfile::{append, tail};
use super::machine::WinMachine;
use super::{Owned, error_text, wide};
use crate::installer::{FamilyFile, InstallRecord, RECORD_FILE};
use crate::settings::{DATA_FOLDER_SDDL, Places, SERVICE_NAME, read_settings};
use crate::setup::{DevFamily, make_dev_code, make_dev_family};

const ERROR_SERVICE_DOES_NOT_EXIST: i32 = 1060;

/// True when this process runs elevated.
pub fn is_elevated() -> bool {
    let mut token = HANDLE::default();
    // SAFETY: opens this process's own token for reading; closed by
    // `Owned`; writes into the structure handed to it.
    unsafe {
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let Some(token) = Owned::new(token) else { return false };
        let mut elevation = TOKEN_ELEVATION::default();
        let mut returned: u32 = 0;
        GetTokenInformation(
            token.raw(),
            TokenElevation,
            Some(&mut elevation as *mut TOKEN_ELEVATION as *mut c_void),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
        .is_ok()
            && elevation.TokenIsElevated != 0
    }
}

/// The service's state, None when it is not installed, or why it could not
/// be read.
fn service_state() -> Result<Option<ServiceState>, String> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT).map_err(|e| e.to_string())?;
    match manager.open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS) {
        Ok(service) => service.query_status().map(|s| Some(s.current_state)).map_err(|e| e.to_string()),
        Err(windows_service::Error::Winapi(e)) if e.raw_os_error() == Some(ERROR_SERVICE_DOES_NOT_EXIST) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// Makes the data folder with its protection from the first moment: SYSTEM
/// and Administrators only, nothing inherited.
pub(crate) fn make_data_folder(path: &Path) -> Result<(), String> {
    let sddl = wide(DATA_FOLDER_SDDL);
    let folder = wide(&path.to_string_lossy());
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    // SAFETY: the descriptor is made by Windows and freed with LocalFree
    // after the folder is made with it.
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(PCWSTR(sddl.as_ptr()), SDDL_REVISION_1, &mut descriptor, None)
            .map_err(|e| error_text(&e))?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: false.into(),
        };
        let made = CreateDirectoryW(PCWSTR(folder.as_ptr()), Some(&attributes));
        let _ = LocalFree(Some(HLOCAL(descriptor.0)));
        made.map_err(|e| error_text(&e))
    }
}

/// `limo install [--dev]`: see `crate::installer`.
pub(crate) fn install(dev: bool) -> i32 {
    crate::installer::install(&mut WinMachine::new(), dev)
}

/// `limo stop-for-upgrade`: see `crate::installer`.
pub(crate) fn stop_for_upgrade() -> i32 {
    crate::installer::stop_for_upgrade(&mut WinMachine::new())
}

/// `limo uninstall`: see `crate::installer`.
pub(crate) fn uninstall(choice: Option<FamilyFile>) -> i32 {
    crate::installer::uninstall(&mut WinMachine::new(), choice)
}

/// `limo status`.
pub(crate) fn status() -> i32 {
    match service_state() {
        Ok(Some(state)) => say!("Service {SERVICE_NAME}: {state:?}."),
        Ok(None) => say!("Service {SERVICE_NAME}: not installed."),
        Err(problem) => say!("Service {SERVICE_NAME}: could not be asked ({problem})."),
    }
    say!("This terminal is {}.", if is_elevated() { "elevated" } else { "not elevated" });
    let places = Places::here();
    if let Some(record) = std::fs::read_to_string(places.data_dir.join(RECORD_FILE))
        .ok()
        .and_then(|text| serde_json::from_str::<InstallRecord>(&text).ok())
    {
        say!("Installed: LiMo {}.", record.version);
    }
    match std::fs::read_to_string(places.settings_file()) {
        Ok(text) => {
            let settings = read_settings(&text);
            if settings.test_cap {
                say!("TEST CAP ON: any single lock is lifted after 120 seconds.");
            } else {
                say!("Test cap: off.");
            }
            if settings.plain_display {
                say!("PLAIN DISPLAY: each helper started from now on shows the plain display, not the drawn one.");
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && places.data_dir.exists() => say!("Test cap: off."),
        Err(_) => {}
    }
    match tail(&places.log_file(), 20) {
        Ok(lines) if lines.is_empty() => say!("The log is empty."),
        Ok(lines) => {
            say!("The last lines of the log:");
            for line in lines {
                say!("  {line}");
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => say!(
            "The log is in a folder only SYSTEM and Administrators may read. From a standard account, or a terminal that is not \
             elevated, only the service's state shows. Run limo status from an elevated terminal to see the log."
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => say!("There is no log yet."),
        Err(e) => say!("The log could not be read: {e}."),
    }
    0
}

/// A Windows account's security identifier, by its name, when it is a
/// user's account.
fn account_sid(name: &str) -> Result<String, String> {
    let account = wide(name);
    let mut sid_bytes: u32 = 0;
    let mut domain_units: u32 = 0;
    let mut kind = SID_NAME_USE::default();
    // SAFETY: the first call asks for the sizes; the second writes at most
    // those into buffers that big. The SID is read before its buffer goes.
    unsafe {
        let _ = LookupAccountNameW(PCWSTR::null(), PCWSTR(account.as_ptr()), None, &mut sid_bytes, None, &mut domain_units, &mut kind);
        if sid_bytes == 0 {
            return Err(format!("Windows knows no account named {name}."));
        }
        let mut sid = vec![0u64; (sid_bytes as usize).div_ceil(8)];
        let mut domain = vec![0u16; domain_units.max(1) as usize];
        LookupAccountNameW(
            PCWSTR::null(),
            PCWSTR(account.as_ptr()),
            Some(PSID(sid.as_mut_ptr() as *mut c_void)),
            &mut sid_bytes,
            Some(PWSTR(domain.as_mut_ptr())),
            &mut domain_units,
            &mut kind,
        )
        .map_err(|e| format!("Windows could not look up the account {name}: {}.", error_text(&e)))?;
        if kind != SidTypeUser {
            return Err(format!("{name} is not a user’s account."));
        }
        let mut text = PWSTR::null();
        windows::Win32::Security::Authorization::ConvertSidToStringSidW(PSID(sid.as_mut_ptr() as *mut c_void), &mut text)
            .map_err(|e| format!("The security identifier of the account {name} could not be written out: {}.", error_text(&e)))?;
        let found = text.to_string().map_err(|e| format!("The security identifier of the account {name} could not be read: {e}."));
        let _ = LocalFree(Some(HLOCAL(text.0 as *mut c_void)));
        found
    }
}

/// The checks `dev-setup` and `dev-code` make before they touch the
/// family's file: an elevated terminal, LiMo installed, and the service
/// stopped, since it holds the file while it runs. The data folder's places
/// when they pass; the exit code when they do not, with the reason printed.
fn ready_for_family_file(command: &str) -> Result<Places, i32> {
    if !is_elevated() {
        say!("{command} must be run from an administrator terminal. Nothing was changed.");
        return Err(2);
    }
    let places = Places::here();
    if !places.data_dir.exists() {
        say!("LiMo is not installed: there is no data folder. Run limo install first. Nothing was changed.");
        return Err(2);
    }
    match service_state() {
        Ok(Some(ServiceState::Stopped)) | Ok(None) => Ok(places),
        Ok(Some(state)) => {
            say!(
                "The service is {state:?}, and holds the family’s file while it runs. Stop it first (sc.exe stop {SERVICE_NAME}), then run {command} again. Nothing was changed."
            );
            Err(2)
        }
        Err(problem) => {
            say!("The service control manager could not be asked about the service: {problem}. Nothing was changed.");
            Err(1)
        }
    }
}

/// `limo dev-code`: scaffolding until the guardian's phone app exists. The
/// code is printed here only, never logged.
pub(crate) fn dev_code(child: Option<&str>, minutes: u32) -> i32 {
    say!("dev-code is scaffolding until the guardian’s phone app exists.");
    let places = match ready_for_family_file("dev-code") {
        Ok(places) => places,
        Err(code) => return code,
    };
    let mut hub = match Hub::open(&places.family_file()) {
        Ok(hub) => hub,
        Err(problem) => {
            say!("The family’s file could not be opened: {problem}");
            return 1;
        }
    };
    let made = make_dev_code(&mut hub, child, minutes, &super::pc_name(), wall_secs());
    let closed = hub.close();
    match made {
        Ok(code) => {
            say!("A one time code for {}'s device on this PC, worth {} minutes, for today only:", code.child, code.minutes);
            say!();
            say!("    {}", code.shown);
            say!();
            say!("It works once. On the lock screen, choose \"Type a one time code\" and type its 8 digits.");
            if let Err(problem) = closed {
                say!("The family’s file did not close cleanly: {problem}");
            }
            say!("Start the service again (sc.exe start {SERVICE_NAME}) to use it.");
            append(&places, &format!("dev-code made a one time code of {} minutes.", code.minutes));
            0
        }
        Err(problem) => {
            say!("{problem}");
            1
        }
    }
}

/// Stops the service for a moment, when it runs, since it holds the
/// family's file; true when it was running, so that it is started again.
fn stop_for_a_moment() -> Result<bool, String> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT).map_err(|e| e.to_string())?;
    let service = match manager.open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS | ServiceAccess::STOP) {
        Ok(service) => service,
        Err(windows_service::Error::Winapi(e)) if e.raw_os_error() == Some(ERROR_SERVICE_DOES_NOT_EXIST) => return Ok(false),
        Err(e) => return Err(e.to_string()),
    };
    let state = service.query_status().map_err(|e| e.to_string())?.current_state;
    if state == ServiceState::Stopped {
        return Ok(false);
    }
    let _ = service.stop();
    let give_up = Instant::now() + Duration::from_secs(20);
    while Instant::now() < give_up {
        if service.query_status().is_ok_and(|s| s.current_state == ServiceState::Stopped) {
            return Ok(true);
        }
        sleep(Duration::from_millis(250));
    }
    Err("the service did not stop within 20 seconds".to_string())
}

/// Starts the service again.
fn start_again() -> Result<(), String> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT).map_err(|e| e.to_string())?;
    let service = manager.open_service(SERVICE_NAME, ServiceAccess::START).map_err(|e| e.to_string())?;
    let no_arguments: [OsString; 0] = [];
    service.start(&no_arguments).map_err(|e| e.to_string())
}

/// `limo reset-password`: a new guardian password without the old one,
/// for an administrator of this PC whose family forgot it. The new password
/// is typed at the prompt and never shown; everything else is kept. The
/// service holds the family's file, so it is stopped for a moment and
/// started again.
pub(crate) fn reset_password() -> i32 {
    say!("reset-password sets a new guardian password without the old one. Everything else is kept.");
    if !is_elevated() {
        say!("reset-password must be run from an administrator terminal. Nothing was changed.");
        return 2;
    }
    let places = Places::here();
    if !places.data_dir.exists() {
        say!("LiMo is not installed: there is no data folder. Nothing was changed.");
        return 2;
    }
    let Some(password) = read_hidden("New guardian password (8 or more characters, not shown): ") else { return 2 };
    let Some(again) = read_hidden("The same password again: ") else { return 2 };
    if *password != *again {
        say!("The two passwords differ. Nothing was changed.");
        return 2;
    }
    if let Some(problem) = crate::setup::new_password_problem(&password) {
        say!("{problem} Nothing was changed.");
        return 2;
    }
    let was_running = match stop_for_a_moment() {
        Ok(was_running) => was_running,
        Err(problem) => {
            say!("The service could not be stopped to reach the family’s file: {problem}. Nothing was changed.");
            return 1;
        }
    };
    let done = match Hub::open(&places.family_file()) {
        Ok(mut hub) => {
            let done = crate::setup::reset_family_password(&mut hub, &password, wall_secs());
            if let Err(problem) = hub.close() {
                say!("The family’s file did not close cleanly: {problem}");
            }
            done
        }
        Err(problem) => Err(format!("The family’s file could not be opened: {problem}")),
    };
    let code = match &done {
        Ok(()) => {
            say!("The guardian password is set. Sign in to LiMo’s settings with the new one.");
            append(&places, "reset-password set a new guardian password.");
            0
        }
        Err(problem) => {
            say!("{problem}");
            append(&places, "reset-password changed nothing.");
            1
        }
    };
    if was_running {
        match start_again() {
            Ok(()) => say!("The service is started again."),
            Err(problem) => say!("The service could not be started again: {problem}. Start it with sc.exe start {SERVICE_NAME}."),
        }
    }
    code
}

/// `limo dev-setup`: scaffolding until the guardian's console exists.
pub(crate) fn dev_setup(child: &str, account: &str, minutes: u32, block: &[String]) -> i32 {
    say!("dev-setup is scaffolding until the guardian’s console exists.");
    let places = match ready_for_family_file("dev-setup") {
        Ok(places) => places,
        Err(code) => return code,
    };
    let account_id = match account_sid(account) {
        Ok(sid) => sid,
        Err(problem) => {
            say!("{problem} Nothing was changed.");
            return 2;
        }
    };
    let Some(password) = read_hidden("Guardian password (8 or more characters, not shown): ") else { return 2 };
    let Some(again) = read_hidden("The same password again: ") else { return 2 };
    if *password != *again {
        say!("The two passwords differ. Nothing was changed.");
        return 2;
    }
    let Some(pin) = read_hidden("Guardian PIN (6 to 8 digits, not shown): ") else { return 2 };
    let Some(pin_again) = read_hidden("The same PIN again: ") else { return 2 };
    if *pin != *pin_again {
        say!("The two PINs differ. Nothing was changed.");
        return 2;
    }
    let mut hub = match Hub::open(&places.family_file()) {
        Ok(hub) => hub,
        Err(problem) => {
            say!("The family’s file could not be opened: {problem}");
            return 1;
        }
    };
    let family = DevFamily {
        password: &password,
        pin: &pin,
        child,
        account_id: &account_id,
        account_name: account,
        minutes,
        block,
        utc_offset_secs: utc_offset_secs(),
    };
    let made = make_dev_family(&mut hub, &family, wall_secs());
    let closed = hub.close();
    match made {
        Ok(_) => {
            say!("Made the family: {child}, linked to the Windows account {account}, with {minutes} minutes a day, and the PIN set.");
            if !block.is_empty() {
                say!("Blocked for {child}: {}.", block.join(", "));
            }
            if let Err(problem) = closed {
                say!("The family’s file did not close cleanly: {problem}");
            }
            say!("Start the service again (sc.exe start {SERVICE_NAME}) to have it count.");
            append(&places, &format!("dev-setup made a family with one child and {minutes} minutes a day."));
            0
        }
        Err(problem) => {
            say!("{problem}");
            1
        }
    }
}
