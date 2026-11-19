//! `limo service`: the service control manager's entry, and the service's
//! one thread, where the engine and the family's file live. The handler the
//! service control manager calls runs on another thread, and only tells the
//! service's thread to stop, through a channel.

use std::ffi::OsString;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::Duration;

use limo_hub::Hub;
use windows::Win32::System::SystemInformation::GetSystemWindowsDirectoryW;
use windows_service::service::{ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::service_dispatcher;

use super::launch::enable_launch_privileges;
use super::logfile::append;
use super::os::WinOs;
use crate::quiet::Quiet;
use crate::service::{STEP_MS, Service, ServiceConfig};
use crate::settings::{Places, SERVICE_NAME};

/// Win32's "the service process could not connect to the service
/// controller": it was run from a terminal.
const ERROR_FAILED_SERVICE_CONTROLLER_CONNECT: i32 = 1063;

windows_service::define_windows_service!(ffi_service_main, service_main);

/// Hands this process to the service control manager, until the service
/// stops.
pub(crate) fn run() -> i32 {
    match service_dispatcher::start(SERVICE_NAME, ffi_service_main) {
        Ok(()) => 0,
        Err(windows_service::Error::Winapi(e)) if e.raw_os_error() == Some(ERROR_FAILED_SERVICE_CONTROLLER_CONNECT) => {
            println!("limo service is the service control manager's entry, and does nothing run from a terminal. Use limo install.");
            2
        }
        Err(e) => {
            println!("The service could not start: {e}.");
            1
        }
    }
}

fn status(state: ServiceState, accepts: ServiceControlAccept, wait_secs: u64) -> ServiceStatus {
    ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: accepts,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::from_secs(wait_secs),
        process_id: None,
    }
}

/// The Windows folder, such as `C:\Windows`.
fn windows_dir() -> String {
    let mut buffer = [0u16; 512];
    // SAFETY: writes at most the buffer's length into it.
    let length = unsafe { GetSystemWindowsDirectoryW(Some(&mut buffer)) } as usize;
    if length == 0 || length >= buffer.len() {
        std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string())
    } else {
        String::from_utf16_lossy(&buffer[..length])
    }
}

fn service_main(_arguments: Vec<OsString>) {
    let places = Places::here();
    if let Err(problem) = run_service(&places) {
        append(&places, &format!("The service stopped with an error: {problem}."));
    }
}

fn run_service(places: &Places) -> windows_service::Result<()> {
    let (stop_tx, stop_rx) = channel::<()>();
    let handler = move |control| -> ServiceControlHandlerResult {
        match control {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                let _ = stop_tx.send(());
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };
    let handle = service_control_handler::register(SERVICE_NAME, handler)?;
    handle.set_service_status(status(ServiceState::Running, ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN, 0))?;
    enable_launch_privileges();
    let exe = std::env::current_exe().unwrap_or_else(|_| places.installed_exe());
    append(places, &format!("The service started (LiMo for Windows {}).", crate::windows_version()));

    // The family's file, held by this thread alone. Tried again every five
    // seconds while it cannot be opened, such as while dev-setup has it.
    let mut quiet = Quiet::new(crate::service::QUIET_MS);
    let mut waited: u64 = 0;
    let hub = loop {
        match Hub::open(&places.family_file()) {
            Ok(hub) => break Some(hub),
            Err(problem) => {
                // The family's file says why in whole sentences; this one
                // goes on after it.
                let problem = problem.to_string();
                let why = problem.trim_end().trim_end_matches('.');
                if let Some(line) = quiet.line("hub", &format!("The family's file could not be opened: {why}. LiMo tries again every five seconds; until then it limits nothing."), waited) {
                    append(places, &line);
                }
                waited += 5_000;
                match stop_rx.recv_timeout(Duration::from_secs(5)) {
                    Err(RecvTimeoutError::Timeout) => {}
                    _ => break None,
                }
            }
        }
    };

    if let Some(hub) = hub {
        let config = ServiceConfig {
            pc_name: super::pc_name(),
            own_pid: std::process::id(),
            own_exe: exe.to_string_lossy().to_string(),
            windows_dir: windows_dir(),
        };
        let mut service = Service::new(WinOs::new(exe, places.clone()), hub, config);
        loop {
            service.step();
            match stop_rx.recv_timeout(Duration::from_millis(STEP_MS)) {
                Err(RecvTimeoutError::Timeout) => {}
                _ => break,
            }
        }
        handle.set_service_status(status(ServiceState::StopPending, ServiceControlAccept::empty(), 5))?;
        service.stop();
        std::thread::sleep(Duration::from_millis(500));
        service.finish();
        drop(service);
    } else {
        handle.set_service_status(status(ServiceState::StopPending, ServiceControlAccept::empty(), 2))?;
    }
    append(places, "The service stopped.");
    handle.set_service_status(status(ServiceState::Stopped, ServiceControlAccept::empty(), 0))?;
    Ok(())
}
