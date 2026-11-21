//! The real PC behind the installer's seam (`crate::installer::Machine`):
//! the service control manager, the folders and their protection, the
//! processes, Windows' firewall, the shortcut, the console's door, the
//! family's file and the terminal. Only `limo install`, `limo
//! stop-for-upgrade` and `limo uninstall` use it, elevated; the tests use a
//! fake PC instead.

use std::ffi::{OsString, c_void};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};

use limo_hub::{Hub, PasswordOutcome};
use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{
    ConvertSecurityDescriptorToStringSecurityDescriptorW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetNamedSecurityInfoW,
    SDDL_REVISION_1, SE_FILE_OBJECT, SetNamedSecurityInfoW,
};
use windows::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl, OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR,
};
use windows::core::{BOOL, PCWSTR, PWSTR};
use windows_service::service::{
    ServiceAccess, ServiceAction, ServiceActionType, ServiceErrorControl, ServiceFailureActions, ServiceFailureResetPeriod, ServiceInfo,
    ServiceStartType, ServiceState, ServiceType,
};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

use super::clock::{timestamp, wall_secs};
use super::console::read_hidden;
use super::door::DoorLine;
use super::install::{is_elevated, make_data_folder};
use super::{error_text, wide};
use crate::console::{CONSOLE_PIPE_NAME, Reply, Request};
use crate::installer::{DoorOpen, FirewallRule, FolderSeen, Machine, ServiceRun, ServiceSeen};
use crate::os::ProcessSeen;
use crate::protocol::Secret;
use crate::settings::{DATA_FOLDER_SDDL, Places, SERVICE_DESCRIPTION, SERVICE_DISPLAY, SERVICE_NAME};

const ERROR_SERVICE_DOES_NOT_EXIST: i32 = 1060;
/// How long the service may take to stop, or to be gone once deleted.
const SERVICE_WAIT: Duration = Duration::from_secs(30);

/// This PC.
pub(crate) struct WinMachine {
    places: Places,
    door: Option<DoorLine>,
}

impl WinMachine {
    pub(crate) fn new() -> WinMachine {
        WinMachine { places: Places::here(), door: None }
    }
}

fn manager(access: ServiceManagerAccess) -> Result<ServiceManager, String> {
    ServiceManager::local_computer(None::<&str>, access).map_err(|e| e.to_string())
}

fn not_installed(error: &windows_service::Error) -> bool {
    matches!(error, windows_service::Error::Winapi(e) if e.raw_os_error() == Some(ERROR_SERVICE_DOES_NOT_EXIST))
}

/// The service as `limo install` makes it.
fn service_info(exe: &Path) -> ServiceInfo {
    ServiceInfo {
        name: OsString::from(SERVICE_NAME),
        display_name: OsString::from(SERVICE_DISPLAY),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: exe.to_path_buf(),
        launch_arguments: vec![OsString::from("service")],
        dependencies: vec![],
        account_name: None,
        account_password: None,
    }
}

/// The description, and the restarts after a failure: 1, 1 and then 10
/// seconds, the count reset after a day.
fn finish_service(service: &windows_service::service::Service) {
    let _ = service.set_description(SERVICE_DESCRIPTION);
    let restart = |secs| ServiceAction { action_type: ServiceActionType::Restart, delay: Duration::from_secs(secs) };
    let failure = ServiceFailureActions {
        reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(86_400)),
        reboot_msg: None,
        command: None,
        actions: Some(vec![restart(1), restart(1), restart(10)]),
    };
    let _ = service.update_failure_actions(failure);
    let _ = service.set_failure_actions_on_non_crash_failures(true);
}

impl Machine for WinMachine {
    fn elevated(&mut self) -> bool {
        is_elevated()
    }

    fn this_exe(&mut self) -> PathBuf {
        std::env::current_exe().unwrap_or_default()
    }

    fn places(&mut self) -> Places {
        self.places.clone()
    }

    fn shortcut_folder(&mut self) -> PathBuf {
        super::shortcut::every_accounts_programs().unwrap_or_else(|| {
            let data = std::env::var_os("ProgramData").map_or_else(|| PathBuf::from(r"C:\ProgramData"), PathBuf::from);
            data.join(r"Microsoft\Windows\Start Menu\Programs")
        })
    }

    fn temp_folder(&mut self) -> PathBuf {
        std::env::temp_dir()
    }

    fn timestamp(&mut self) -> String {
        timestamp()
    }

    fn wall_secs(&mut self) -> i64 {
        wall_secs()
    }

    fn exists(&mut self, path: &Path) -> bool {
        path.exists()
    }

    fn read_text(&mut self, path: &Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }

    fn write_text(&mut self, path: &Path, text: &str) -> Result<(), String> {
        std::fs::write(path, text).map_err(|e| e.to_string())
    }

    fn append_text(&mut self, path: &Path, text: &str) -> Result<(), String> {
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path).map_err(|e| e.to_string())?;
        file.write_all(text.as_bytes()).map_err(|e| e.to_string())
    }

    fn copy_file(&mut self, from: &Path, to: &Path) -> Result<(), String> {
        std::fs::copy(from, to).map(drop).map_err(|e| e.to_string())
    }

    fn make_folder(&mut self, path: &Path) -> Result<(), String> {
        std::fs::create_dir_all(path).map_err(|e| e.to_string())
    }

    fn remove_file(&mut self, path: &Path) -> Result<(), String> {
        std::fs::remove_file(path).map_err(|e| e.to_string())
    }

    fn remove_folder(&mut self, path: &Path) -> Result<(), String> {
        let give_up = Instant::now() + Duration::from_secs(10);
        loop {
            match std::fs::remove_dir_all(path) {
                Ok(()) => return Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(e) if Instant::now() >= give_up => return Err(e.to_string()),
                Err(_) => sleep(Duration::from_millis(500)),
            }
        }
    }

    fn make_protected_folder(&mut self, path: &Path) -> Result<(), String> {
        make_data_folder(path)
    }

    fn protect_folder(&mut self, path: &Path) -> Result<(), String> {
        protect_folder(path)
    }

    fn folder_seen(&mut self, path: &Path) -> Option<FolderSeen> {
        folder_seen(path)
    }

    fn service(&mut self) -> Result<Option<ServiceSeen>, String> {
        let manager = manager(ServiceManagerAccess::CONNECT)?;
        let service = match manager.open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS | ServiceAccess::QUERY_CONFIG) {
            Ok(service) => service,
            Err(e) if not_installed(&e) => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        let state = match service.query_status().map_err(|e| e.to_string())?.current_state {
            ServiceState::Stopped => ServiceRun::Stopped,
            ServiceState::Running => ServiceRun::Running,
            other => ServiceRun::Other(format!("{other:?}")),
        };
        let exe = service.query_config().ok().map(|config| config.executable_path);
        Ok(Some(ServiceSeen { state, exe }))
    }

    fn create_service(&mut self, exe: &Path) -> Result<(), String> {
        let manager = manager(ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE)?;
        let access = ServiceAccess::QUERY_STATUS | ServiceAccess::START | ServiceAccess::CHANGE_CONFIG;
        let service = manager.create_service(&service_info(exe), access).map_err(|e| e.to_string())?;
        finish_service(&service);
        Ok(())
    }

    fn configure_service(&mut self, exe: &Path) -> Result<(), String> {
        let manager = manager(ServiceManagerAccess::CONNECT)?;
        let service = manager.open_service(SERVICE_NAME, ServiceAccess::CHANGE_CONFIG | ServiceAccess::QUERY_CONFIG).map_err(|e| e.to_string())?;
        service.change_config(&service_info(exe)).map_err(|e| e.to_string())?;
        finish_service(&service);
        Ok(())
    }

    fn start_service(&mut self) -> Result<(), String> {
        let manager = manager(ServiceManagerAccess::CONNECT)?;
        let service = manager.open_service(SERVICE_NAME, ServiceAccess::START | ServiceAccess::QUERY_STATUS).map_err(|e| e.to_string())?;
        if service.query_status().is_ok_and(|s| s.current_state == ServiceState::Running) {
            return Ok(());
        }
        let no_arguments: [OsString; 0] = [];
        service.start(&no_arguments).map_err(|e| e.to_string())
    }

    fn stop_service(&mut self) -> Result<(), String> {
        let manager = manager(ServiceManagerAccess::CONNECT)?;
        let service = manager.open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS | ServiceAccess::STOP).map_err(|e| e.to_string())?;
        if service.query_status().map_err(|e| e.to_string())?.current_state == ServiceState::Stopped {
            return Ok(());
        }
        let _ = service.stop();
        let give_up = Instant::now() + SERVICE_WAIT;
        while Instant::now() < give_up {
            if service.query_status().is_ok_and(|s| s.current_state == ServiceState::Stopped) {
                return Ok(());
            }
            sleep(Duration::from_millis(250));
        }
        Err(format!("it did not stop within {} seconds", SERVICE_WAIT.as_secs()))
    }

    fn delete_service(&mut self) -> Result<(), String> {
        let manager = manager(ServiceManagerAccess::CONNECT)?;
        let service = match manager.open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS | ServiceAccess::DELETE) {
            Ok(service) => service,
            Err(e) if not_installed(&e) => return Ok(()),
            Err(e) => return Err(e.to_string()),
        };
        service.delete().map_err(|e| e.to_string())?;
        drop(service);
        let give_up = Instant::now() + SERVICE_WAIT;
        while Instant::now() < give_up {
            match manager.open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS) {
                Err(e) if not_installed(&e) => return Ok(()),
                _ => sleep(Duration::from_millis(250)),
            }
        }
        Err("it is marked for deletion, and goes when nothing holds it any more (a restart at the latest)".to_string())
    }

    fn running_from(&mut self, folder: &Path) -> Vec<ProcessSeen> {
        let me = std::process::id();
        let folder = folder.to_string_lossy().to_string();
        super::processes::processes()
            .into_iter()
            .filter(|p| p.pid != me && !p.path.is_empty() && crate::close::path_is_inside(&p.path, &folder))
            .collect()
    }

    fn end_process(&mut self, process: &ProcessSeen) -> Result<(), String> {
        super::processes::end_process(process)
    }

    fn pause(&mut self, ms: u64) {
        sleep(Duration::from_millis(ms));
    }

    fn firewall_has_rule(&mut self, name: &str) -> Result<bool, String> {
        super::firewall::has_rule(name)
    }

    fn add_firewall_rule(&mut self, rule: &FirewallRule) -> Result<(), String> {
        super::firewall::add_rule(rule)
    }

    fn remove_firewall_rule(&mut self, name: &str) -> Result<(), String> {
        super::firewall::remove_rule(name)
    }

    fn make_shortcut(&mut self, at: &Path, target: &Path, description: &str) -> Result<(), String> {
        super::shortcut::make(at, target, description)
    }

    fn door_open(&mut self) -> DoorOpen {
        match DoorLine::open(CONSOLE_PIPE_NAME, true) {
            Ok((line, version)) => {
                self.door = Some(line);
                DoorOpen::Open { version }
            }
            Err(found) => found,
        }
    }

    fn door_ask(&mut self, request: Request) -> Result<Reply, String> {
        match self.door.as_mut() {
            Some(line) => line.ask(request),
            None => Err("the door is not open".to_string()),
        }
    }

    fn door_close(&mut self) {
        self.door = None;
    }

    fn file_has_family(&mut self) -> Result<bool, String> {
        let hub = Hub::open(&self.places.family_file()).map_err(|e| e.to_string())?;
        let has = hub.has_family().map_err(|e| e.to_string());
        let _ = hub.close();
        has
    }

    fn file_check_password(&mut self, password: &Secret, now: i64) -> Result<PasswordOutcome, String> {
        let mut hub = Hub::open(&self.places.family_file()).map_err(|e| e.to_string())?;
        let outcome = hub.check_password(password.expose(), now).map_err(|e| e.to_string());
        let _ = hub.close();
        outcome
    }

    fn ask_secret(&mut self, prompt: &str) -> Option<Secret> {
        read_hidden(&crate::text::typographic(prompt)).map(|typed| Secret::new(typed.to_string()))
    }

    fn ask_line(&mut self, prompt: &str) -> Option<String> {
        print!("{}", crate::text::typographic(prompt));
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        match std::io::stdin().lock().read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(line.trim().to_string()),
        }
    }

    fn say(&mut self, text: &str) {
        say!("{text}");
    }
}

/// A folder's owner and access list, as SDDL.
pub(crate) fn folder_seen(path: &Path) -> Option<FolderSeen> {
    let folder = wide(&path.to_string_lossy());
    let what = OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    // SAFETY: both buffers come from Windows and are freed with LocalFree
    // once read.
    let sddl = unsafe {
        let found = GetNamedSecurityInfoW(PCWSTR(folder.as_ptr()), SE_FILE_OBJECT, what, None, None, None, None, &mut descriptor);
        if found.is_err() {
            return None;
        }
        let mut text = PWSTR::null();
        let converted = ConvertSecurityDescriptorToStringSecurityDescriptorW(descriptor, SDDL_REVISION_1, what, &mut text, None);
        let _ = LocalFree(Some(HLOCAL(descriptor.0)));
        converted.ok()?;
        let sddl = text.to_string().ok();
        let _ = LocalFree(Some(HLOCAL(text.0 as *mut c_void)));
        sddl?
    };
    split_owner_and_dacl(&sddl)
}

/// `O:BAD:P(...)` as its owner (`BA`) and access list (`D:P(...)`).
pub(crate) fn split_owner_and_dacl(sddl: &str) -> Option<FolderSeen> {
    let rest = sddl.strip_prefix("O:")?;
    let at = rest.find("D:")?;
    let (owner, dacl) = rest.split_at(at);
    // A group or a system list after the access list is not part of it.
    let dacl = dacl.split("G:").next().unwrap_or(dacl).split("S:").next().unwrap_or(dacl);
    Some(FolderSeen { owner: owner.to_string(), dacl: dacl.to_string() })
}

/// Sets LiMo's protection on a folder: SYSTEM and Administrators only,
/// nothing inherited.
fn protect_folder(path: &Path) -> Result<(), String> {
    set_folder_access(path, DATA_FOLDER_SDDL)
}

/// Sets a folder's access list, protected, from SDDL.
fn set_folder_access(path: &Path, access: &str) -> Result<(), String> {
    let sddl = wide(access);
    let folder = wide(&path.to_string_lossy());
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    // SAFETY: the descriptor is made by Windows and freed with LocalFree
    // after its access list was set on the folder; the list lives inside it.
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(PCWSTR(sddl.as_ptr()), SDDL_REVISION_1, &mut descriptor, None)
            .map_err(|e| error_text(&e))?;
        let mut present = BOOL::default();
        let mut defaulted = BOOL::default();
        let mut dacl: *mut ACL = std::ptr::null_mut();
        let read = GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted);
        let set = read.map_err(|e| error_text(&e)).and_then(|()| {
            let done = SetNamedSecurityInfoW(
                PCWSTR(folder.as_ptr()),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                None,
                None,
                Some(dacl as *const ACL),
                None,
            );
            if done.is_ok() { Ok(()) } else { Err(error_text(&windows::core::Error::from(done.to_hresult()))) }
        });
        let _ = LocalFree(Some(HLOCAL(descriptor.0)));
        set
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_owner_and_access_list_are_read_apart() {
        let seen = split_owner_and_dacl("O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)").unwrap();
        assert_eq!((seen.owner.as_str(), seen.dacl.as_str()), ("BA", "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)"));
        let user = split_owner_and_dacl("O:S-1-5-21-1-2-3-1001D:PAI(A;OICI;FA;;;SY)").unwrap();
        assert_eq!((user.owner.as_str(), user.dacl.as_str()), ("S-1-5-21-1-2-3-1001", "D:PAI(A;OICI;FA;;;SY)"));
        assert!(split_owner_and_dacl("D:P(A;;FA;;;SY)").is_none());
    }

    #[test]
    fn a_folder_made_with_limos_protection_by_a_standard_token_is_not_limos() {
        // In the test's own temporary folder, not elevated: the access list
        // is LiMo's, but the owner is the test's own account, so an install
        // would never use it. This is how a folder made by a child's account
        // in ProgramData is turned away.
        let parent = tempfile::tempdir().unwrap();
        let folder = parent.path().join("LiMo");
        make_data_folder(&folder).unwrap();
        // However the test ends, the folder's owner gives itself the folder
        // back and removes it: nobody but SYSTEM and Administrators could.
        struct GiveBack(PathBuf);
        impl Drop for GiveBack {
            fn drop(&mut self) {
                if let Some(me) = super::super::door::this_account() {
                    let _ = set_folder_access(&self.0, &format!("D:P(A;OICI;FA;;;{me})"));
                }
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let back = GiveBack(folder.clone());
        let seen = folder_seen(&folder).unwrap();
        assert!(seen.dacl.starts_with("D:P") && seen.dacl.contains("(A;OICI;FA;;;SY)") && seen.dacl.contains("(A;OICI;FA;;;BA)"), "{seen:?}");
        if !is_elevated() {
            assert!(seen.owner.starts_with("S-1-5-21-"), "{seen:?}");
            assert!(matches!(crate::installer::judge_folder(Some(&seen)), crate::installer::FolderVerdict::NotLimos(_)));
        }
        drop(back);
        assert!(!folder.exists(), "the test's folder is gone");
    }
}
