//! LiMo spike 05: set up strict mode (Device Owner) on an Android phone from
//! the PC, check it, and above all release it. Throwaway code; see README.md
//! and TESTING.md.
//!
//! Exit codes: 0 success, 1 refused or failed, 2 bad arguments.

mod adb;
mod args;
mod commands;
mod parse;
mod rules;

use adb::Adb;
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let cmd = match args::parse(&argv) {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("error: {msg}");
            eprintln!();
            eprint!("{}", args::USAGE);
            return ExitCode::from(2);
        }
    };
    if cmd == args::Command::Help {
        print!("{}", args::USAGE);
        return ExitCode::from(0);
    }

    // Everything that needs no adb is checked before adb is touched.
    if let args::Command::Setup { apk, .. } = &cmd {
        if !Path::new(apk).is_file() {
            eprintln!("error: the APK \"{apk}\" does not exist. Build it first: in phone\\, .\\gradlew.bat assembleDebug");
            return ExitCode::from(2);
        }
    }

    let mut real = match adb::RealAdb::find() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(1);
        }
    };
    println!("adb: {}", real.path().display());
    if let Err(e) = real.start_server() {
        eprintln!("error: {e}");
        return ExitCode::from(1);
    }
    if let Ok(o) = real.run(&["version"], Duration::from_secs(15)) {
        if let Some(v) = parse::adb_version(&o.text()) {
            println!("adb version {v}");
        }
    }

    let code = match cmd {
        args::Command::Check => commands::check(&mut real),
        args::Command::Setup { apk, deadline_minutes } => commands::setup(&mut real, &apk, deadline_minutes),
        args::Command::Status => commands::status(&mut real),
        args::Command::Release { pin } => commands::release(&mut real, &pin),
        args::Command::ForceRelease => commands::force_release(&mut real),
        args::Command::Uninstall => commands::uninstall(&mut real),
        args::Command::Help => 0,
    };
    ExitCode::from(code.clamp(0, 255) as u8)
}
