//! LiMo spike 07, the cable proof: can LiMo's PC program reach a phone over
//! the USB cable by itself, and do the things that adding a phone needs?
//! See README.md for what is asked, the run, and what each outcome means.
//!
//! One step a run (`steps.rs`), each by one of four routes (`cable.rs`):
//! adb_client inside this program over libusb (`route_libusb.rs`) or over
//! nusb (`route_nusb.rs`), Google's adb.exe (`route_adb.rs`), or this
//! program's own adb (`protocol.rs`) over Windows' own WinUSB
//! (`route_winusb.rs`). Everything the phone is asked is in `phone.rs`.
//! Windows' lists of drivers, and WinUSB, are in `win/`, the only unsafe
//! code here.

pub mod apk;
pub mod cable;
pub mod cli;
pub mod drivers;
pub mod echo;
pub mod key;
pub mod phone;
pub mod plan;
pub mod protocol;
pub mod route_adb;
pub mod route_libusb;
pub mod route_nusb;
pub mod route_rust;
pub mod route_winusb;
pub mod secret;
pub mod steps;
pub mod ui;
#[cfg(windows)]
pub mod win;

use std::path::{Path, PathBuf};

use crate::cable::{Cable, Connector, OpenProblem, Via};
use crate::cli::{Args, Command, KeyChoice};
use crate::key::Key;
use crate::phone::masked_serial;
use crate::plan::StepName;
use crate::route_adb::AdbExe;
use crate::steps::Exit;
use crate::ui::Ui;

/// The real routes.
pub struct RealConnector {
    /// This program's adb key, for the two Rust routes.
    pub key: Option<Key>,
    /// Google's adb.exe, if given with --adb.
    pub adb: Option<PathBuf>,
}

impl Connector for RealConnector {
    fn open(&self, via: Via, ui: &mut dyn Ui) -> Result<Box<dyn Cable>, OpenProblem> {
        match via {
            Via::Libusb | Via::Nusb => {
                let key = self.key.as_ref().ok_or_else(|| OpenProblem::Other("this program's adb key could not be made".to_string()))?;
                if via == Via::Libusb { route_libusb::open(&key.path, ui) } else { route_nusb::open(&key.path, ui) }
            }
            Via::Adb => route_adb::open(AdbExe::find(self.adb.as_deref()), ui),
            Via::Winusb => {
                let key = self.key.as_ref().ok_or_else(|| OpenProblem::Other("this program's adb key could not be made".to_string()))?;
                route_winusb::open(key, ui)
            }
        }
    }

    fn seen(&self, via: Via) -> Result<Vec<String>, String> {
        match via {
            Via::Libusb => Ok(route_libusb::phones()?
                .into_iter()
                .map(|(vendor, product, name)| format!("\"{name}\" ({vendor:04x}:{product:04x}), with a debugging interface"))
                .collect()),
            Via::Nusb => Ok(route_nusb::phones()?
                .into_iter()
                .map(|p| {
                    format!(
                        "\"{}\" ({:04x}:{:04x}), debugging interface {}, the whole device driven by {}",
                        p.name,
                        p.vendor,
                        p.product,
                        p.interface,
                        p.driver.as_deref().unwrap_or("none")
                    )
                })
                .collect()),
            Via::Winusb => route_winusb::seen(),
            Via::Adb => {
                let exe = AdbExe::find(self.adb.as_deref()).ok_or_else(|| OpenProblem::AdbMissing.to_string())?;
                exe.start_server().map_err(|e| format!("adb start-server: {e}"))?;
                let listed = exe.devices();
                let _ = exe.kill_server();
                Ok(listed
                    .map_err(|e| format!("adb devices: {e}"))?
                    .into_iter()
                    .map(|(serial, state)| format!("{} as {state} (adb's server stopped again)", masked_serial(&serial)))
                    .collect())
            }
        }
    }
}

/// The log of adb_client and nusb, for --verbose, with every serial number
/// it was told of cut to its last four characters.
struct Log {
    hide: Vec<String>,
}

impl log::Log for Log {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Debug
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let mut text = format!("[{} {}] {}", record.level(), record.target(), record.args());
        for serial in &self.hide {
            text = hide(&text, serial);
        }
        eprintln!("{text}");
    }

    fn flush(&self) {}
}

/// `text` with every `serial` in it, in any case, cut to its last four
/// characters.
pub fn hide(text: &str, serial: &str) -> String {
    if serial.len() < 5 || !serial.is_ascii() {
        return text.to_string();
    }
    let lower = text.to_ascii_lowercase();
    let wanted = serial.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(found) = lower[at..].find(&wanted) {
        out.push_str(&text[at..at + found]);
        out.push_str(&masked_serial(serial));
        at += found + wanted.len();
    }
    out.push_str(&text[at..]);
    out
}

/// Where the test app's APK is looked for when --apk is not given: in this
/// proof's folder, from the folder the program is run in (the proof's own,
/// or its `pc` folder).
fn default_apk() -> PathBuf {
    let here = Path::new(steps::APK_PATH);
    if here.is_file() {
        return here.to_path_buf();
    }
    Path::new("..").join(steps::APK_PATH)
}

/// Runs the program; its exit code.
pub fn run(args: &[String], ui: &mut dyn Ui) -> i32 {
    let args: Args = match cli::parse(args) {
        Ok(args) => args,
        Err(why) => {
            ui.say(&why);
            return Exit::Refused.code();
        }
    };
    if args.verbose {
        let hide = route_nusb::phones().map(|p| p.iter().filter_map(|p| p.serial_for_hiding()).collect()).unwrap_or_default();
        if log::set_boxed_logger(Box::new(Log { hide })).is_ok() {
            log::set_max_level(log::LevelFilter::Debug);
        }
    }
    let (step, via, apk) = match args.command {
        Command::Plan => {
            ui.say(&plan::plan());
            return Exit::Done.code();
        }
        Command::Help => {
            ui.say(&plan::help());
            return Exit::Done.code();
        }
        Command::Step { step, via, apk } => (step, via, apk),
    };
    let rust_route = matches!(via, Some(Via::Libusb | Via::Nusb | Via::Winusb));
    let key = if args.key == KeyChoice::Adb {
        match key::google_adb() {
            Ok(key) => Some(key),
            Err(why) => {
                ui.say(&why);
                return Exit::Refused.code();
            }
        }
    } else if rust_route && step.needs_a_route() {
        match key::folder() {
            Some(folder) => match key::load_or_make(&folder) {
                Ok(key) => Some(key),
                Err(why) => {
                    ui.say(&why);
                    return Exit::Refused.code();
                }
            },
            None => {
                ui.say("No folder for this program's adb key: LOCALAPPDATA is not set.");
                return Exit::Refused.code();
            }
        }
    } else {
        None
    };
    let connector = RealConnector { key, adb: args.adb };
    let exit = match (step, via) {
        (StepName::Drivers, _) => steps::drivers(windows_report(), ui),
        (StepName::List, via) => steps::list(&connector, via, ui),
        (StepName::Connect, Some(via)) => steps::connect(&connector, via, connector.key.as_ref(), ui),
        (StepName::Install, Some(via)) => {
            let apk = apk.unwrap_or_else(default_apk);
            steps::install(&connector, via, &apk, ui)
        }
        (StepName::WriteSecret, Some(via)) => steps::write_secret(&connector, via, ui),
        (StepName::Start, Some(via)) => steps::start(&connector, via, ui),
        (StepName::Reverse, Some(via)) => steps::reverse(&connector, via, phone::LOOPBACK_PORT, ui),
        (StepName::Uninstall, Some(via)) => steps::uninstall(&connector, via, ui),
        (StepName::DebuggingOff, Some(via)) => steps::debugging_off(&connector, via, ui),
        (_, None) => Exit::Refused,
    };
    exit.code()
}

#[cfg(windows)]
fn windows_report() -> Result<drivers::Report, String> {
    win::drivers::read()
}

#[cfg(not(windows))]
fn windows_report() -> Result<drivers::Report, String> {
    Err("this reads Windows' own lists, on Windows only".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_serial_in_a_log_line_is_cut_in_any_case() {
        assert_eq!(
            hide("opening \\\\?\\usb#vid_04e8&pid_6860#r5cn20abcd1a2b#{guid}", "R5CN20ABCD1A2B"),
            "opening \\\\?\\usb#vid_04e8&pid_6860#...1A2B#{guid}"
        );
        assert_eq!(hide("R5CN20ABCD1A2B and R5CN20ABCD1A2B", "R5CN20ABCD1A2B"), "...1A2B and ...1A2B");
        assert_eq!(hide("nothing here", "R5CN20ABCD1A2B"), "nothing here");
    }
}
