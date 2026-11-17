//! limo-spike: a throwaway proof for the LiMo screen time limiter.
//!
//! It proves three things, so the real build can rely on them:
//!   1. a LocalSystem service can start a helper inside the logged in user's
//!      desktop session,
//!   2. that helper can cover every monitor and swallow keyboard input,
//!   3. a standard account cannot stop the service, and a killed helper comes
//!      back within about a second.
//!
//! Safety limits that the code enforces on itself:
//!   a lock can never last longer than 120 seconds, the overlay never hides or
//!   ends other programs, and uninstall always removes everything.

#![cfg(windows)]

mod config;
mod logging;
mod monitors;
mod overlay;
mod probe;
mod svc;

use std::ffi::OsString;
use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};

use windows_service::service::{
    ServiceAccess, ServiceAction, ServiceActionType, ServiceErrorControl, ServiceFailureActions,
    ServiceFailureResetPeriod, ServiceInfo, ServiceStartType, ServiceState, ServiceType,
};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

use config::{
    clamp_lock_seconds, clamp_overlay_seconds, config_path, control_path, deadline_too_far,
    installed_exe, now_unix, parse_deadline, path_is_inside, program_data_dir, program_files_dir,
    remaining_secs, HelperMode, HELPER_CAP_SECS, SERVICE_DISPLAY, SERVICE_NAME,
};

type CmdResult = Result<(), Box<dyn std::error::Error>>;

const ERR_SERVICE_DOES_NOT_EXIST: i32 = 1060;
const ERR_FAILED_SERVICE_CONTROLLER_CONNECT: i32 = 1063;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(dispatch(&args));
}

fn dispatch(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        Some("monitors") => cmd_monitors(),
        Some("overlay") => cmd_overlay(&args[1..]),
        Some("install") => cmd_install(&args[1..]),
        Some("uninstall") => cmd_uninstall(),
        Some("lock") => cmd_lock(&args[1..]),
        Some("unlock") => cmd_unlock(),
        Some("status") => cmd_status(),
        Some("probe") => cmd_probe(&args[1..]),
        Some("service") => cmd_service(),
        _ => {
            print_usage();
            2
        }
    }
}

fn print_usage() {
    println!("limo-spike: LiMo service and lock proof. Safe to remove.");
    println!();
    println!("Usage:");
    println!("  limo-spike monitors                 list the attached monitors");
    println!("  limo-spike overlay --seconds N      developer lock for N seconds (1..30)");
    println!("  limo-spike overlay --managed        the mode the service launches");
    println!("  limo-spike install [--helper-as user|system]   (needs administrator)");
    println!("  limo-spike uninstall                (needs administrator)");
    println!("  limo-spike lock --seconds N         set the lock for N seconds (5..120, admin)");
    println!("  limo-spike unlock                   clear the lock (admin)");
    println!("  limo-spike status                   show service, mode, lock and recent log");
    println!("  limo-spike probe --after N          wait N s (0..60), then try to stop or damage the");
    println!("                                      spike as this account; report to %TEMP%");
    println!("  limo-spike service                  service control manager entry point only");
}

fn cmd_probe(args: &[String]) -> i32 {
    let after = match flag_value(args, "--after") {
        None => 0,
        Some(v) => match v.parse::<u64>() {
            Ok(n) => n,
            Err(_) => {
                println!("probe --after needs a whole number of seconds, for example: probe --after 20");
                return 2;
            }
        },
    };
    probe::run(after)
}

/// True when the LiMoSpike service is registered with the service control
/// manager. Needs no elevation.
fn service_exists() -> bool {
    let Ok(manager) = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
    else {
        return false;
    };
    manager
        .open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS)
        .is_ok()
}

/// True when this running binary lives inside the install folder, which
/// uninstall could then not delete.
fn running_from_install_folder() -> bool {
    let Ok(me) = std::env::current_exe() else {
        return false;
    };
    let folder = program_files_dir();
    let raw = path_is_inside(&me.to_string_lossy(), &folder.to_string_lossy());
    let canonical = match (std::fs::canonicalize(&me), std::fs::canonicalize(&folder)) {
        (Ok(m), Ok(f)) => path_is_inside(&m.to_string_lossy(), &f.to_string_lossy()),
        _ => false,
    };
    raw || canonical
}

/// Return the value that follows a flag, if present.
fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

fn is_elevated() -> bool {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut returned: u32 = 0;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut core::ffi::c_void),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
        .is_ok();
        let _ = CloseHandle(token);
        ok && elevation.TokenIsElevated != 0
    }
}

fn cmd_monitors() -> i32 {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let found = monitors::list_monitors();
    for (i, m) in found.iter().enumerate() {
        println!(
            "monitor {}: left={} top={} right={} bottom={} primary={}",
            i + 1,
            m.rect.left,
            m.rect.top,
            m.rect.right,
            m.rect.bottom,
            m.primary
        );
    }
    println!("count: {}", found.len());
    0
}

fn cmd_overlay(args: &[String]) -> i32 {
    if has_flag(args, "--managed") {
        return overlay::run(true, 0);
    }
    if let Some(v) = flag_value(args, "--seconds") {
        match v.parse::<u64>() {
            Ok(n) => {
                let seconds = clamp_overlay_seconds(n);
                return overlay::run(false, seconds);
            }
            Err(_) => {
                println!("overlay --seconds needs a whole number, for example: overlay --seconds 8");
                return 2;
            }
        }
    }
    println!("overlay needs either --managed or --seconds N (1..30).");
    2
}

fn cmd_lock(args: &[String]) -> i32 {
    if !is_elevated() {
        println!("lock must be run from an administrator terminal. Nothing was changed.");
        return 2;
    }
    let seconds = match flag_value(args, "--seconds").and_then(|v| v.parse::<u64>().ok()) {
        Some(n) => clamp_lock_seconds(n),
        None => {
            println!("lock needs --seconds N (5..120), for example: lock --seconds 60");
            return 2;
        }
    };
    match write_lock(seconds) {
        Ok(deadline) => {
            println!(
                "lock set for {} seconds. Deadline unix time {}.",
                seconds, deadline
            );
            logging::log(&format!("lock command: {} seconds, deadline {}", seconds, deadline));
            0
        }
        Err(e) => {
            eprintln!("could not write the control file: {e}");
            1
        }
    }
}

fn write_lock(seconds: u64) -> Result<i64, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(program_data_dir())?;
    let deadline = now_unix() + seconds as i64;
    std::fs::write(control_path(), deadline.to_string())?;
    Ok(deadline)
}

fn cmd_unlock() -> i32 {
    if !is_elevated() {
        println!("unlock must be run from an administrator terminal. Nothing was changed.");
        return 2;
    }
    match std::fs::create_dir_all(program_data_dir()).and_then(|_| std::fs::write(control_path(), "0")) {
        Ok(_) => {
            println!("lock cleared.");
            logging::log("unlock command: lock cleared");
            0
        }
        Err(e) => {
            eprintln!("could not write the control file: {e}");
            1
        }
    }
}

fn cmd_status() -> i32 {
    // Service state.
    match ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT) {
        Ok(manager) => match manager.open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS) {
            Ok(service) => match service.query_status() {
                Ok(st) => println!("service {}: {:?}", SERVICE_NAME, st.current_state),
                Err(e) => println!("service {}: present, status unavailable ({e})", SERVICE_NAME),
            },
            Err(windows_service::Error::Winapi(e))
                if e.raw_os_error() == Some(ERR_SERVICE_DOES_NOT_EXIST) =>
            {
                println!("service {}: not installed", SERVICE_NAME)
            }
            Err(e) => println!("service {}: query failed ({e})", SERVICE_NAME),
        },
        Err(e) => println!("service control manager unavailable ({e})"),
    }

    // Helper mode.
    match std::fs::read_to_string(config_path()) {
        Ok(s) => println!("helper mode: {}", HelperMode::parse(&s).as_str()),
        Err(_) => println!("helper mode: unknown (config.txt not found)"),
    }

    // Lock deadline, judged by the same rule the service and overlay use.
    let deadline = std::fs::read_to_string(control_path())
        .map(|s| parse_deadline(&s))
        .unwrap_or(0);
    let now = now_unix();
    if deadline == 0 {
        println!("lock: cleared");
    } else if deadline_too_far(deadline, now) {
        println!(
            "lock: deadline unix {} IGNORED. It is {} seconds ahead, more than the {} second cap. \
             The lock command never writes a deadline that far ahead, so this one came from a \
             hand edit or a clock change. The service and overlay treat it as no lock.",
            deadline,
            deadline.saturating_sub(now),
            HELPER_CAP_SECS
        );
    } else {
        let remaining = remaining_secs(deadline, now);
        if remaining > 0 {
            println!("lock: holds, deadline unix {}, {} seconds remaining", deadline, remaining);
        } else {
            println!("lock: expired, deadline unix {} (now {})", deadline, now);
        }
    }

    // Recent log.
    println!("recent log (last 15 lines):");
    let lines = logging::tail(15);
    if lines.is_empty() {
        println!("  (log is empty or not found)");
    } else {
        for line in lines {
            println!("  {line}");
        }
    }
    0
}

fn cmd_service() -> i32 {
    match svc::run() {
        Ok(()) => 0,
        Err(windows_service::Error::Winapi(e))
            if e.raw_os_error() == Some(ERR_FAILED_SERVICE_CONTROLLER_CONNECT) =>
        {
            println!(
                "This is the service control manager entry point. It is not meant to be run \
                 from a terminal. Use install, then the Services console or the lock command."
            );
            0
        }
        Err(e) => {
            eprintln!("service dispatcher error: {e}");
            1
        }
    }
}

fn cmd_install(args: &[String]) -> i32 {
    let mode = flag_value(args, "--helper-as")
        .map(|v| HelperMode::parse(&v))
        .unwrap_or(HelperMode::User);

    if !is_elevated() {
        println!("install must be run from an administrator terminal. Nothing was changed.");
        return 2;
    }

    // An existing install would make the copy fail halfway (file in use).
    if service_exists() {
        println!("{SERVICE_NAME} is already installed. Run uninstall first. Nothing was changed.");
        return 2;
    }

    match do_install(mode) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("install failed: {e}");
            1
        }
    }
}

fn do_install(mode: HelperMode) -> CmdResult {
    let install_dir = program_files_dir();
    let data_dir = program_data_dir();
    let dst = installed_exe();

    std::fs::create_dir_all(&install_dir)?;
    let src = std::env::current_exe()?;
    std::fs::copy(&src, &dst)?;
    println!("copied binary to {}", dst.display());

    std::fs::create_dir_all(&data_dir)?;
    set_data_dir_acls(&data_dir);
    std::fs::write(config_path(), mode.as_str())?;
    std::fs::write(control_path(), "0")?;
    println!("prepared data folder {}", data_dir.display());
    println!("helper mode set to {}", mode.as_str());

    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )?;

    let info = ServiceInfo {
        name: OsString::from(SERVICE_NAME),
        display_name: OsString::from(SERVICE_DISPLAY),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: dst.clone(),
        launch_arguments: vec![OsString::from("service")],
        dependencies: vec![],
        account_name: None, // LocalSystem
        account_password: None,
    };

    let service = manager.create_service(
        &info,
        ServiceAccess::QUERY_STATUS
            | ServiceAccess::START
            | ServiceAccess::STOP
            | ServiceAccess::CHANGE_CONFIG
            | ServiceAccess::DELETE,
    )?;
    let _ = service
        .set_description("LiMo screen time limiter spike. Safe to remove with: limo-spike uninstall");

    let failure = ServiceFailureActions {
        reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(86_400)),
        reboot_msg: None,
        command: None,
        actions: Some(vec![
            ServiceAction {
                action_type: ServiceActionType::Restart,
                delay: Duration::from_secs(1),
            },
            ServiceAction {
                action_type: ServiceActionType::Restart,
                delay: Duration::from_secs(1),
            },
            ServiceAction {
                action_type: ServiceActionType::None,
                delay: Duration::ZERO,
            },
        ]),
    };
    let _ = service.update_failure_actions(failure);

    let no_args: [OsString; 0] = [];
    service.start(&no_args)?;

    println!("service {} created, set to automatic, and started.", SERVICE_NAME);
    logging::log(&format!("installed and started, helper mode {}", mode.as_str()));
    Ok(())
}

/// Lock the data folder down: SYSTEM and Administrators get full control, and
/// Users may only read. Well known SIDs are used so this works on any language.
fn set_data_dir_acls(dir: &Path) {
    let target = dir.to_string_lossy().to_string();
    let result = std::process::Command::new("icacls")
        .arg(&target)
        .arg("/inheritance:r")
        .arg("/grant:r")
        .arg("*S-1-5-18:(OI)(CI)F") // SYSTEM
        .arg("/grant:r")
        .arg("*S-1-5-32-544:(OI)(CI)F") // Administrators
        .arg("/grant:r")
        .arg("*S-1-5-32-545:(OI)(CI)R") // Users, read only
        .output();
    match result {
        Ok(out) if out.status.success() => println!("set permissions on {}", target),
        Ok(out) => println!(
            "icacls returned a non zero status for {}: {}",
            target,
            String::from_utf8_lossy(&out.stderr).trim()
        ),
        Err(e) => println!("could not run icacls for {}: {e}", target),
    }
}

fn cmd_uninstall() -> i32 {
    // A running exe cannot delete itself, so the installed copy cannot remove
    // its own folder. Check this first: it is the useful message either way.
    if running_from_install_folder() {
        println!(
            "uninstall cannot run from the installed copy in {}. Run it from the build folder \
             copy (target\\release\\limo-spike.exe) instead. Nothing was changed.",
            program_files_dir().display()
        );
        return 2;
    }
    if !is_elevated() {
        println!("uninstall must be run from an administrator terminal. Nothing was changed.");
        return 2;
    }
    match do_uninstall() {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("uninstall failed: {e}");
            1
        }
    }
}

fn do_uninstall() -> CmdResult {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;

    match manager.open_service(
        SERVICE_NAME,
        ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE,
    ) {
        Ok(service) => {
            if let Ok(st) = service.query_status() {
                if st.current_state != ServiceState::Stopped {
                    let _ = service.stop();
                }
            }
            let _ = service.delete();
            drop(service);

            // Wait for the database entry to disappear.
            let start = Instant::now();
            let timeout = Duration::from_secs(10);
            while start.elapsed() < timeout {
                match manager.open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS) {
                    Err(windows_service::Error::Winapi(e))
                        if e.raw_os_error() == Some(ERR_SERVICE_DOES_NOT_EXIST) =>
                    {
                        break
                    }
                    _ => sleep(Duration::from_millis(500)),
                }
            }
            println!("service {} stopped and removed.", SERVICE_NAME);
        }
        Err(windows_service::Error::Winapi(e))
            if e.raw_os_error() == Some(ERR_SERVICE_DOES_NOT_EXIST) =>
        {
            println!("service {} was not installed.", SERVICE_NAME);
        }
        Err(e) => return Err(Box::new(e)),
    }

    // Remove both folders. Missing folders are fine, so this is safe twice.
    remove_dir_with_retry(&program_files_dir());
    remove_dir_with_retry(&program_data_dir());
    println!("uninstall complete.");
    Ok(())
}

/// Remove a folder, retrying for a few seconds: the stopped service process,
/// or its helper, can hold the exe open for a moment after the stop.
fn remove_dir_with_retry(dir: &Path) {
    let give_up = Instant::now() + Duration::from_secs(10);
    loop {
        match std::fs::remove_dir_all(dir) {
            Ok(()) => {
                println!("removed {}", dir.display());
                return;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(e) => {
                if Instant::now() >= give_up {
                    println!(
                        "could not remove {}: {e}. A spike process may still be closing; \
                         run uninstall again in a few seconds.",
                        dir.display()
                    );
                    return;
                }
                sleep(Duration::from_millis(500));
            }
        }
    }
}
