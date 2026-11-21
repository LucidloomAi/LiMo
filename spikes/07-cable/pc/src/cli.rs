//! The command line: one step a run.

#![forbid(unsafe_code)]

use std::path::PathBuf;

use crate::cable::Via;
use crate::plan::StepName;

/// What was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// The whole run, for reading.
    Plan,
    /// The help.
    Help,
    /// One step.
    Step {
        /// Which.
        step: StepName,
        /// By which route; only `list` may go without one.
        via: Option<Via>,
        /// The APK to install, for `install`.
        apk: Option<PathBuf>,
    },
}

/// Which key the WinUSB route connects with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyChoice {
    /// This program's own, which makes the phone ask (the default).
    Own,
    /// The adb key of this Windows account, Google's adb's own, read only:
    /// a phone that allowed it before asks nothing. For this proof only.
    Adb,
}

/// The command line, read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    /// What was asked.
    pub command: Command,
    /// Google's adb.exe, if given.
    pub adb: Option<PathBuf>,
    /// Print what adb_client and nusb log.
    pub verbose: bool,
    /// Which key the WinUSB route connects with.
    pub key: KeyChoice,
}

/// Reads the command line (without the program's own name).
pub fn parse(args: &[String]) -> Result<Args, String> {
    let mut words = args.iter();
    let first = words.next().map(String::as_str).unwrap_or("help");
    let mut via = None;
    let mut apk = None;
    let mut adb = None;
    let mut verbose = false;
    let mut key = None;
    while let Some(word) = words.next() {
        match word.as_str() {
            "--via" => {
                let value = words.next().ok_or("--via needs a route: winusb, libusb, nusb or adb.")?;
                via = Some(Via::from_word(value).ok_or_else(|| format!("--via {value}: the routes are winusb, libusb, nusb and adb."))?);
            }
            "--apk" => apk = Some(PathBuf::from(words.next().ok_or("--apk needs a path.")?)),
            "--adb" => adb = Some(PathBuf::from(words.next().ok_or("--adb needs the path of adb.exe.")?)),
            "--verbose" => verbose = true,
            "--key" => {
                key = Some(match words.next().map(String::as_str) {
                    Some("own") => KeyChoice::Own,
                    Some("adb") => KeyChoice::Adb,
                    _ => return Err("--key takes own (this program's own key, the default) or adb (the adb key of this Windows account).".to_string()),
                })
            }
            other => return Err(format!("{other}: not understood. Run limo-cable-spike help.")),
        }
    }
    let command = match first {
        "plan" => Command::Plan,
        "help" | "--help" | "-h" => Command::Help,
        word => {
            let step = StepName::from_word(word).ok_or_else(|| format!("{word}: no such step. Run limo-cable-spike help."))?;
            if step.needs_a_route() && via.is_none() {
                return Err(format!("{word} needs a route: --via winusb, --via libusb, --via nusb or --via adb."));
            }
            if step == StepName::Drivers && via.is_some() {
                return Err("drivers reads Windows' lists only, and takes no route.".to_string());
            }
            if apk.is_some() && step != StepName::Install {
                return Err("--apk is for install only.".to_string());
            }
            Command::Step { step, via, apk }
        }
    };
    if adb.is_some() && !matches!(command, Command::Step { via: Some(Via::Adb), .. }) {
        return Err("--adb is for --via adb only.".to_string());
    }
    if key.is_some() && !matches!(command, Command::Step { via: Some(Via::Winusb), step, .. } if step.needs_a_route()) {
        return Err("--key is for the steps that reach the phone by --via winusb only.".to_string());
    }
    Ok(Args { command, adb, verbose, key: key.unwrap_or(KeyChoice::Own) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(line: &str) -> Result<Args, String> {
        parse(&line.split_whitespace().map(str::to_string).collect::<Vec<_>>())
    }

    #[test]
    fn each_step_and_its_route() {
        assert_eq!(args("").unwrap().command, Command::Help);
        assert_eq!(args("plan").unwrap().command, Command::Plan);
        assert_eq!(args("drivers").unwrap().command, Command::Step { step: StepName::Drivers, via: None, apk: None });
        assert_eq!(args("list").unwrap().command, Command::Step { step: StepName::List, via: None, apk: None });
        assert_eq!(
            args("connect --via nusb").unwrap().command,
            Command::Step { step: StepName::Connect, via: Some(Via::Nusb), apk: None }
        );
        let install = args("install --via adb --apk x.apk --adb C:/tools/adb.exe --verbose").unwrap();
        assert_eq!(install.command, Command::Step { step: StepName::Install, via: Some(Via::Adb), apk: Some(PathBuf::from("x.apk")) });
        assert!(install.verbose);
        assert_eq!(install.adb, Some(PathBuf::from("C:/tools/adb.exe")));
        assert_eq!(install.key, KeyChoice::Own, "this program's own key unless told");
        assert_eq!(args("connect --via winusb --key adb").unwrap().key, KeyChoice::Adb);
        assert_eq!(args("reverse --via winusb --key own").unwrap().key, KeyChoice::Own);
        for step in StepName::every() {
            let line = if step.needs_a_route() { format!("{} --via libusb", step.word()) } else { step.word().to_string() };
            assert!(args(&line).is_ok(), "{line}");
        }
    }

    #[test]
    fn what_is_refused_before_anything_is_done() {
        assert!(args("connect").unwrap_err().contains("needs a route"));
        assert!(args("debugging-off").unwrap_err().contains("needs a route"));
        assert!(args("connect --via usb").unwrap_err().contains("routes are"));
        assert!(args("connect --via").unwrap_err().contains("needs a route"));
        assert!(args("drivers --via libusb").unwrap_err().contains("takes no route"));
        assert!(args("start --via libusb --apk x.apk").unwrap_err().contains("install only"));
        assert!(args("connect --via libusb --adb adb.exe").unwrap_err().contains("--via adb only"));
        assert!(args("reboot --via adb").unwrap_err().contains("no such step"));
        assert!(args("connect --via adb --force").unwrap_err().contains("not understood"));
        assert!(args("connect --via libusb --key adb").unwrap_err().contains("--via winusb only"));
        assert!(args("list --via winusb --key adb").unwrap_err().contains("--via winusb only"));
        assert!(args("connect --via winusb --key google").unwrap_err().contains("takes own"));
        assert!(args("connect --via winusb --key").unwrap_err().contains("takes own"));
    }
}
