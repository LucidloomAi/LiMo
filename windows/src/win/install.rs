//! `limo install`, `uninstall`, `status`, `dev-setup`, `dev-code` and
//! `reset-password`: for testing with the owner at the PC, and for a family
//! that forgot the guardian password. Ported from the first spike, with the
//! data folder closed to everyone but SYSTEM and Administrators.

use std::ffi::{OsString, c_void};
use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use limo_hub::Hub;
use windows::Win32::Foundation::{HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{
    ConvertSecurityDescriptorToStringSecurityDescriptorW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetNamedSecurityInfoW,
    SDDL_REVISION_1, SE_FILE_OBJECT,
};
use windows::Win32::Security::{
    DACL_SECURITY_INFORMATION, GetTokenInformation, LookupAccountNameW, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES, SID_NAME_USE,
    SidTypeUser, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
};
use windows::Win32::Storage::FileSystem::CreateDirectoryW;
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::core::{PCWSTR, PWSTR};
use windows_service::service::{
    ServiceAccess, ServiceAction, ServiceActionType, ServiceErrorControl, ServiceFailureActions, ServiceFailureResetPeriod, ServiceInfo,
    ServiceStartType, ServiceState, ServiceType,
};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

use super::clock::{utc_offset_secs, wall_secs};
use super::console::read_hidden;
use super::logfile::{append, tail};
use super::{Owned, error_text, wide};
use crate::close::path_is_inside;
use crate::settings::{
    DATA_FOLDER_SDDL, Places, SERVICE_DESCRIPTION, SERVICE_DISPLAY, SERVICE_NAME, Settings, read_settings, settings_text,
};
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
fn make_data_folder(path: &Path) -> Result<(), String> {
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

/// A folder's access list, as SDDL, to show what was applied.
fn folder_access(path: &Path) -> Option<String> {
    let folder = wide(&path.to_string_lossy());
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    // SAFETY: both buffers come from Windows and are freed with LocalFree
    // once read.
    unsafe {
        let found = GetNamedSecurityInfoW(PCWSTR(folder.as_ptr()), SE_FILE_OBJECT, DACL_SECURITY_INFORMATION, None, None, None, None, &mut descriptor);
        if found.is_err() {
            return None;
        }
        let mut text = PWSTR::null();
        let converted =
            ConvertSecurityDescriptorToStringSecurityDescriptorW(descriptor, SDDL_REVISION_1, DACL_SECURITY_INFORMATION, &mut text, None);
        let _ = LocalFree(Some(HLOCAL(descriptor.0)));
        converted.ok()?;
        let sddl = text.to_string().ok();
        let _ = LocalFree(Some(HLOCAL(text.0 as *mut c_void)));
        sddl
    }
}

/// `limo install [--dev]`.
pub(crate) fn install(dev: bool) -> i32 {
    if !is_elevated() {
        println!("install must be run from an administrator terminal. Nothing was changed.");
        return 2;
    }
    match service_state() {
        Ok(None) => {}
        Ok(Some(_)) => {
            println!("LiMo is already installed. Run limo uninstall first. Nothing was changed.");
            return 2;
        }
        Err(problem) => {
            println!("The service control manager could not be asked whether LiMo is installed: {problem}. Nothing was changed.");
            return 1;
        }
    }
    let places = Places::here();
    if places.data_dir.exists() {
        println!(
            "The data folder {} already exists. LiMo makes it itself, so that nobody else can have made it first. \
             If it is left from an earlier test, run limo uninstall, which removes it. Nothing was changed.",
            places.data_dir.display()
        );
        return 2;
    }
    match do_install(&places, dev) {
        Ok(()) => 0,
        Err(problem) => {
            println!("install failed: {problem}.");
            println!("Run limo uninstall to remove what was done.");
            1
        }
    }
}

fn do_install(places: &Places, dev: bool) -> Result<(), String> {
    let source = std::env::current_exe().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&places.install_dir).map_err(|e| format!("making {}: {e}", places.install_dir.display()))?;
    std::fs::copy(&source, places.installed_exe()).map_err(|e| format!("copying the program: {e}"))?;
    println!("Copied the program to {}.", places.installed_exe().display());

    make_data_folder(&places.data_dir).map_err(|e| format!("making the data folder {}: {e}", places.data_dir.display()))?;
    println!("Made the data folder {}, for SYSTEM and Administrators only.", places.data_dir.display());
    if let Some(access) = folder_access(&places.data_dir) {
        println!("  Its access list, as Windows applied it: {access}");
    }
    if dev {
        std::fs::write(places.settings_file(), settings_text(&Settings { test_cap: true, ..Settings::default() }))
            .map_err(|e| format!("writing the settings: {e}"))?;
        println!("TEST CAP ON: this is a test install. Any single lock is lifted after 120 seconds, and the log says so each time.");
    }

    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE)
        .map_err(|e| e.to_string())?;
    let info = ServiceInfo {
        name: OsString::from(SERVICE_NAME),
        display_name: OsString::from(SERVICE_DISPLAY),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: places.installed_exe(),
        launch_arguments: vec![OsString::from("service")],
        dependencies: vec![],
        account_name: None,
        account_password: None,
    };
    let access = ServiceAccess::QUERY_STATUS | ServiceAccess::START | ServiceAccess::CHANGE_CONFIG;
    let service = manager.create_service(&info, access).map_err(|e| format!("creating the service: {e}"))?;
    let _ = service.set_description(SERVICE_DESCRIPTION);
    let restart = |secs| ServiceAction { action_type: ServiceActionType::Restart, delay: Duration::from_secs(secs) };
    let failure = ServiceFailureActions {
        reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(86_400)),
        reboot_msg: None,
        command: None,
        actions: Some(vec![restart(1), restart(1), restart(10)]),
    };
    if let Err(e) = service.update_failure_actions(failure) {
        println!("The service's restart on failure could not be set: {e}.");
    }
    let _ = service.set_failure_actions_on_non_crash_failures(true);
    let no_arguments: [OsString; 0] = [];
    service.start(&no_arguments).map_err(|e| format!("starting the service: {e}"))?;
    println!("Created the service {SERVICE_NAME} (LocalSystem, automatic, restarted on failure) and started it.");
    append(places, &format!("Installed{}.", if dev { " for testing, with the test cap on" } else { "" }));
    Ok(())
}

/// True when this program runs from the installed folder, which uninstall
/// could not then delete.
fn running_installed(places: &Places) -> bool {
    std::env::current_exe().is_ok_and(|me| {
        path_is_inside(&me.to_string_lossy(), &places.install_dir.to_string_lossy())
            || std::fs::canonicalize(&me)
                .ok()
                .zip(std::fs::canonicalize(&places.install_dir).ok())
                .is_some_and(|(me, dir)| path_is_inside(&me.to_string_lossy(), &dir.to_string_lossy()))
    })
}

/// Removes a folder, trying for ten seconds while a stopping process lets
/// go of its files.
fn remove_folder(path: &Path) {
    let give_up = Instant::now() + Duration::from_secs(10);
    loop {
        match std::fs::remove_dir_all(path) {
            Ok(()) => {
                println!("Removed {}.", path.display());
                return;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(e) if Instant::now() >= give_up => {
                println!("Could not remove {}: {e}. A LiMo process may still be closing; run uninstall again in a moment.", path.display());
                return;
            }
            Err(_) => sleep(Duration::from_millis(500)),
        }
    }
}

/// `limo uninstall`.
pub(crate) fn uninstall() -> i32 {
    let places = Places::here();
    if running_installed(&places) {
        println!(
            "uninstall cannot run from the installed copy in {}, which cannot delete itself. Run it from the build folder's copy. Nothing was changed.",
            places.install_dir.display()
        );
        return 2;
    }
    if !is_elevated() {
        println!("uninstall must be run from an administrator terminal. Nothing was changed.");
        return 2;
    }
    let manager = match ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT) {
        Ok(manager) => manager,
        Err(e) => {
            println!("The service control manager could not be reached: {e}.");
            return 1;
        }
    };
    match manager.open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE) {
        Ok(service) => {
            if service.query_status().is_ok_and(|s| s.current_state != ServiceState::Stopped) {
                let _ = service.stop();
            }
            let _ = service.delete();
            drop(service);
            let give_up = Instant::now() + Duration::from_secs(10);
            while Instant::now() < give_up {
                match manager.open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS) {
                    Err(windows_service::Error::Winapi(e)) if e.raw_os_error() == Some(ERROR_SERVICE_DOES_NOT_EXIST) => break,
                    _ => sleep(Duration::from_millis(500)),
                }
            }
            println!("Stopped and removed the service {SERVICE_NAME}.");
        }
        Err(windows_service::Error::Winapi(e)) if e.raw_os_error() == Some(ERROR_SERVICE_DOES_NOT_EXIST) => {
            println!("The service {SERVICE_NAME} was not installed.");
        }
        Err(e) => {
            println!("The service could not be opened: {e}.");
            return 1;
        }
    }
    remove_folder(&places.install_dir);
    remove_folder(&places.data_dir);
    println!("Uninstall complete.");
    0
}

/// `limo status`.
pub(crate) fn status() -> i32 {
    match service_state() {
        Ok(Some(state)) => println!("Service {SERVICE_NAME}: {state:?}."),
        Ok(None) => println!("Service {SERVICE_NAME}: not installed."),
        Err(problem) => println!("Service {SERVICE_NAME}: could not be asked ({problem})."),
    }
    println!("This terminal is {}.", if is_elevated() { "elevated" } else { "not elevated" });
    let places = Places::here();
    match std::fs::read_to_string(places.settings_file()) {
        Ok(text) => {
            let settings = read_settings(&text);
            if settings.test_cap {
                println!("TEST CAP ON: any single lock is lifted after 120 seconds.");
            } else {
                println!("Test cap: off.");
            }
            if settings.plain_display {
                println!("PLAIN DISPLAY: each helper started from now on shows the plain display, not the drawn one.");
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && places.data_dir.exists() => println!("Test cap: off."),
        Err(_) => {}
    }
    match tail(&places.log_file(), 20) {
        Ok(lines) if lines.is_empty() => println!("The log is empty."),
        Ok(lines) => {
            println!("The last lines of the log:");
            for line in lines {
                println!("  {line}");
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => println!(
            "The log is in a folder only SYSTEM and Administrators may read. From a standard account, or a terminal that is not \
             elevated, only the service's state shows. Run limo status from an elevated terminal to see the log."
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => println!("There is no log yet."),
        Err(e) => println!("The log could not be read: {e}."),
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
            return Err(format!("{name} is not a user's account."));
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
        println!("{command} must be run from an administrator terminal. Nothing was changed.");
        return Err(2);
    }
    let places = Places::here();
    if !places.data_dir.exists() {
        println!("LiMo is not installed: there is no data folder. Run limo install first. Nothing was changed.");
        return Err(2);
    }
    match service_state() {
        Ok(Some(ServiceState::Stopped)) | Ok(None) => Ok(places),
        Ok(Some(state)) => {
            println!(
                "The service is {state:?}, and holds the family's file while it runs. Stop it first (sc.exe stop {SERVICE_NAME}), then run {command} again. Nothing was changed."
            );
            Err(2)
        }
        Err(problem) => {
            println!("The service control manager could not be asked about the service: {problem}. Nothing was changed.");
            Err(1)
        }
    }
}

/// `limo dev-code`: scaffolding until the guardian's phone app exists. The
/// code is printed here only, never logged.
pub(crate) fn dev_code(child: Option<&str>, minutes: u32) -> i32 {
    println!("dev-code is scaffolding until the guardian's phone app exists.");
    let places = match ready_for_family_file("dev-code") {
        Ok(places) => places,
        Err(code) => return code,
    };
    let mut hub = match Hub::open(&places.family_file()) {
        Ok(hub) => hub,
        Err(problem) => {
            println!("The family's file could not be opened: {problem}");
            return 1;
        }
    };
    let made = make_dev_code(&mut hub, child, minutes, &super::pc_name(), wall_secs());
    let closed = hub.close();
    match made {
        Ok(code) => {
            println!("A one time code for {}'s device on this PC, worth {} minutes, for today only:", code.child, code.minutes);
            println!();
            println!("    {}", code.shown);
            println!();
            println!("It works once. On the lock screen, choose \"Type a one time code\" and type its 8 digits.");
            if let Err(problem) = closed {
                println!("The family's file did not close cleanly: {problem}");
            }
            println!("Start the service again (sc.exe start {SERVICE_NAME}) to use it.");
            append(&places, &format!("dev-code made a one time code of {} minutes.", code.minutes));
            0
        }
        Err(problem) => {
            println!("{problem}");
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
    println!("reset-password sets a new guardian password without the old one. Everything else is kept.");
    if !is_elevated() {
        println!("reset-password must be run from an administrator terminal. Nothing was changed.");
        return 2;
    }
    let places = Places::here();
    if !places.data_dir.exists() {
        println!("LiMo is not installed: there is no data folder. Nothing was changed.");
        return 2;
    }
    let Some(password) = read_hidden("New guardian password (8 or more characters, not shown): ") else { return 2 };
    let Some(again) = read_hidden("The same password again: ") else { return 2 };
    if *password != *again {
        println!("The two passwords differ. Nothing was changed.");
        return 2;
    }
    if let Some(problem) = crate::setup::new_password_problem(&password) {
        println!("{problem} Nothing was changed.");
        return 2;
    }
    let was_running = match stop_for_a_moment() {
        Ok(was_running) => was_running,
        Err(problem) => {
            println!("The service could not be stopped to reach the family's file: {problem}. Nothing was changed.");
            return 1;
        }
    };
    let done = match Hub::open(&places.family_file()) {
        Ok(mut hub) => {
            let done = crate::setup::reset_family_password(&mut hub, &password, wall_secs());
            if let Err(problem) = hub.close() {
                println!("The family's file did not close cleanly: {problem}");
            }
            done
        }
        Err(problem) => Err(format!("The family's file could not be opened: {problem}")),
    };
    let code = match &done {
        Ok(()) => {
            println!("The guardian password is set. Sign in to LiMo's settings with the new one.");
            append(&places, "reset-password set a new guardian password.");
            0
        }
        Err(problem) => {
            println!("{problem}");
            append(&places, "reset-password changed nothing.");
            1
        }
    };
    if was_running {
        match start_again() {
            Ok(()) => println!("The service is started again."),
            Err(problem) => println!("The service could not be started again: {problem}. Start it with sc.exe start {SERVICE_NAME}."),
        }
    }
    code
}

/// `limo dev-setup`: scaffolding until the guardian's console exists.
pub(crate) fn dev_setup(child: &str, account: &str, minutes: u32, block: &[String]) -> i32 {
    println!("dev-setup is scaffolding until the guardian's console exists.");
    let places = match ready_for_family_file("dev-setup") {
        Ok(places) => places,
        Err(code) => return code,
    };
    let account_id = match account_sid(account) {
        Ok(sid) => sid,
        Err(problem) => {
            println!("{problem} Nothing was changed.");
            return 2;
        }
    };
    let Some(password) = read_hidden("Guardian password (8 or more characters, not shown): ") else { return 2 };
    let Some(again) = read_hidden("The same password again: ") else { return 2 };
    if *password != *again {
        println!("The two passwords differ. Nothing was changed.");
        return 2;
    }
    let Some(pin) = read_hidden("Guardian PIN (6 to 8 digits, not shown): ") else { return 2 };
    let Some(pin_again) = read_hidden("The same PIN again: ") else { return 2 };
    if *pin != *pin_again {
        println!("The two PINs differ. Nothing was changed.");
        return 2;
    }
    let mut hub = match Hub::open(&places.family_file()) {
        Ok(hub) => hub,
        Err(problem) => {
            println!("The family's file could not be opened: {problem}");
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
            println!("Made the family: {child}, linked to the Windows account {account}, with {minutes} minutes a day, and the PIN set.");
            if !block.is_empty() {
                println!("Blocked for {child}: {}.", block.join(", "));
            }
            if let Err(problem) = closed {
                println!("The family's file did not close cleanly: {problem}");
            }
            println!("Start the service again (sc.exe start {SERVICE_NAME}) to have it count.");
            append(&places, &format!("dev-setup made a family with one child and {minutes} minutes a day."));
            0
        }
        Err(problem) => {
            println!("{problem}");
            1
        }
    }
}
