//! `limo service`: the service control manager's entry, and the service's
//! one thread, where the engine and the family's file live. The handler the
//! service control manager calls runs on another thread, and only tells the
//! service's thread to stop, through a channel. The console's door and the
//! link with the phones wake the same thread through the same channel when
//! something came, and the service answers it between two steps; the steps
//! keep their cadence whatever the door and the phones do.

use std::ffi::OsString;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::mpsc::{RecvTimeoutError, Sender, channel};
use std::time::{Duration, Instant};

use limo_hub::Hub;
use windows::Win32::System::SystemInformation::GetSystemWindowsDirectoryW;
use windows_service::service::{ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::service_dispatcher;

use super::launch::enable_launch_privileges;
use super::logfile::append;
use super::os::WinOs;
use crate::announce::MdnsAnnouncer;
use crate::console::CONSOLE_PIPE_NAME;
use crate::phones::{LinkServer, SystemClock};
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

/// What wakes the service's thread between two steps.
enum Wake {
    /// The service control manager asks it to stop.
    Stop,
    /// Something came to the console's door, or from a phone.
    Door,
}

/// How often the link is tried again while its port cannot be had, in
/// seconds.
const LINK_RETRY_SECS: u64 = 60;

/// Starts the link for the phones on every IPv4 address, loopback included
/// (the USB cable's tunnel arrives there), at the link's port.
fn start_link(wake_tx: &Sender<Wake>) -> std::io::Result<LinkServer> {
    let tx = wake_tx.clone();
    let wake = Arc::new(move || {
        let _ = tx.send(Wake::Door);
    });
    let bind = SocketAddr::from((Ipv4Addr::UNSPECIFIED, limo_link::DEFAULT_PORT));
    LinkServer::start(bind, Arc::new(SystemClock::new()), wake)
}

fn run_service(places: &Places) -> windows_service::Result<()> {
    let (wake_tx, stop_rx) = channel::<Wake>();
    let stop_tx = wake_tx.clone();
    let handler = move |control| -> ServiceControlHandlerResult {
        match control {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                let _ = stop_tx.send(Wake::Stop);
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
                    Err(RecvTimeoutError::Timeout) | Ok(Wake::Door) => {}
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
        let mut os = WinOs::new(exe, places.clone());
        let door_tx = wake_tx.clone();
        os.open_door(CONSOLE_PIPE_NAME, move || {
            let _ = door_tx.send(Wake::Door);
        });
        let mut service = Service::new(os, hub, config);
        let mut link_open = false;
        let mut link_tried: Option<Instant> = None;
        let step = Duration::from_millis(STEP_MS);
        let mut next_step = Instant::now();
        loop {
            if !link_open && link_tried.is_none_or(|at| at.elapsed() >= Duration::from_secs(LINK_RETRY_SECS)) {
                match start_link(&wake_tx) {
                    Ok(server) => {
                        service.open_link(server, Box::new(MdnsAnnouncer::new()));
                        link_open = true;
                    }
                    Err(problem) if link_tried.is_none() => {
                        append(places, &format!(
                            "The link for the family's phones could not listen on port {}: {problem}. Phones cannot reach this PC until it can; LiMo tries again every minute.",
                            limo_link::DEFAULT_PORT
                        ));
                        link_tried = Some(Instant::now());
                    }
                    Err(_) => link_tried = Some(Instant::now()),
                }
            }
            if Instant::now() >= next_step {
                service.step();
                service.door();
                service.link();
                // On the cadence, and never more than one step behind it.
                next_step = (next_step + step).max(Instant::now());
            }
            match stop_rx.recv_timeout(next_step.saturating_duration_since(Instant::now())) {
                Ok(Wake::Door) => {
                    service.door();
                    service.link();
                }
                Err(RecvTimeoutError::Timeout) => {}
                Ok(Wake::Stop) | Err(RecvTimeoutError::Disconnected) => break,
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
