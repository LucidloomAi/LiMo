//! Command line arguments. Pure, so it is unit tested.

/// `setup --deadline-minutes` may only shorten the app's default 30 minutes.
pub const MIN_DEADLINE: u32 = 1;
pub const MAX_DEADLINE: u32 = 29;

pub const USAGE: &str = "\
limo-strict-spike: LiMo spike 05, strict mode (Device Owner) over adb

Every command prints each adb line before it runs it. check and status only
read; setup, release, force-release and uninstall change the phone, and say
so first.

Usage:
  limo-strict-spike check
      Read only. Whether set-device-owner can work on the connected phone,
      and if not, exactly what is in the way.
  limo-strict-spike setup --apk PATH [--deadline-minutes N]
      Runs check and refuses if anything is in the way. Then installs the
      APK (adb install -t -r), checks it is a test only build, makes LiMo
      spike 05 the Device Owner, confirms it, and opens the app so its dead
      man's timer starts. --deadline-minutes N (1 to 29) makes the timer
      shorter than its default 30 minutes. Never retries anything.
  limo-strict-spike status
      Read only. Owner, admin, restrictions in force, suspended packages and
      the app's last log lines.
  limo-strict-spike release --pin PIN
      Asks the app to release the phone (the PIN for the spike is 2468),
      waits up to 15 seconds and checks that no owner is left.
  limo-strict-spike force-release
      The escape hatch: adb shell dpm remove-active-admin. Works even if the
      app is broken, because the app is a test only build.
  limo-strict-spike uninstall
      Refuses while the app is owner or admin; otherwise adb uninstall.
  limo-strict-spike help

Exit codes: 0 success, 1 refused or failed, 2 bad arguments.
";

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Check,
    Setup {
        apk: String,
        deadline_minutes: Option<u32>,
    },
    Status,
    Release {
        pin: String,
    },
    ForceRelease,
    Uninstall,
    Help,
}

pub fn parse(args: &[String]) -> Result<Command, String> {
    let Some(sub) = args.first() else {
        return Ok(Command::Help);
    };
    let rest = &args[1..];
    match sub.as_str() {
        "check" => no_more(rest).map(|_| Command::Check),
        "status" => no_more(rest).map(|_| Command::Status),
        "force-release" => no_more(rest).map(|_| Command::ForceRelease),
        "uninstall" => no_more(rest).map(|_| Command::Uninstall),
        "setup" => parse_setup(rest),
        "release" => parse_release(rest),
        "help" | "--help" | "-h" | "/?" => Ok(Command::Help),
        other => Err(format!("unknown command \"{other}\"")),
    }
}

fn parse_setup(rest: &[String]) -> Result<Command, String> {
    let mut apk: Option<String> = None;
    let mut deadline: Option<u32> = None;
    let mut i = 0;
    while i < rest.len() {
        let flag = rest[i].as_str();
        match flag {
            "--apk" => {
                once(apk.is_some(), flag)?;
                apk = Some(check_apk(value(rest, i)?)?);
                i += 2;
            }
            "--deadline-minutes" => {
                once(deadline.is_some(), flag)?;
                deadline = Some(parse_deadline(value(rest, i)?)?);
                i += 2;
            }
            other => return Err(format!("setup: unknown option \"{other}\"")),
        }
    }
    let apk = apk.ok_or("setup: --apk is required")?;
    Ok(Command::Setup {
        apk,
        deadline_minutes: deadline,
    })
}

fn parse_release(rest: &[String]) -> Result<Command, String> {
    let mut pin: Option<String> = None;
    let mut i = 0;
    while i < rest.len() {
        let flag = rest[i].as_str();
        match flag {
            "--pin" => {
                once(pin.is_some(), flag)?;
                pin = Some(check_pin(value(rest, i)?)?);
                i += 2;
            }
            other => return Err(format!("release: unknown option \"{other}\"")),
        }
    }
    let pin = pin.ok_or("release: --pin is required")?;
    Ok(Command::Release { pin })
}

fn no_more(rest: &[String]) -> Result<(), String> {
    match rest.first() {
        None => Ok(()),
        Some(extra) => Err(format!("unexpected argument \"{extra}\"")),
    }
}

fn once(already: bool, flag: &str) -> Result<(), String> {
    if already {
        Err(format!("{flag} given more than once"))
    } else {
        Ok(())
    }
}

fn value(rest: &[String], i: usize) -> Result<&str, String> {
    match rest.get(i + 1) {
        Some(v) if !v.starts_with("--") => Ok(v.as_str()),
        _ => Err(format!("{} needs a value", rest[i])),
    }
}

/// A PIN is 4 to 12 digits. Digits only, so it can never change the meaning
/// of the shell line it is sent in.
pub fn check_pin(text: &str) -> Result<String, String> {
    let t = text.trim();
    if t.len() < 4 || t.len() > 12 || !t.chars().all(|c| c.is_ascii_digit()) {
        return Err("--pin must be 4 to 12 digits".to_string());
    }
    Ok(t.to_string())
}

pub fn check_apk(text: &str) -> Result<String, String> {
    let t = text.trim();
    if t.is_empty() {
        return Err("--apk must not be empty".to_string());
    }
    if !t.to_ascii_lowercase().ends_with(".apk") {
        return Err(format!("--apk \"{t}\" does not end in .apk"));
    }
    if t.chars().any(char::is_control) {
        return Err("--apk must not contain control characters".to_string());
    }
    Ok(t.to_string())
}

pub fn parse_deadline(text: &str) -> Result<u32, String> {
    let n: u32 = text
        .trim()
        .parse()
        .map_err(|_| format!("--deadline-minutes \"{text}\" is not a whole number"))?;
    if !(MIN_DEADLINE..=MAX_DEADLINE).contains(&n) {
        return Err(format!(
            "--deadline-minutes must be between {MIN_DEADLINE} and {MAX_DEADLINE}, not {n}: the timer can only be made shorter than its 30 minutes"
        ));
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_arguments_and_help() {
        assert_eq!(parse(&v(&[])), Ok(Command::Help));
        assert_eq!(parse(&v(&["help"])), Ok(Command::Help));
        assert_eq!(parse(&v(&["--help"])), Ok(Command::Help));
        assert!(parse(&v(&["launch"])).is_err());
    }

    #[test]
    fn simple_commands_take_nothing() {
        assert_eq!(parse(&v(&["check"])), Ok(Command::Check));
        assert_eq!(parse(&v(&["status"])), Ok(Command::Status));
        assert_eq!(parse(&v(&["force-release"])), Ok(Command::ForceRelease));
        assert_eq!(parse(&v(&["uninstall"])), Ok(Command::Uninstall));
        assert!(parse(&v(&["check", "--fix"])).is_err());
        assert!(parse(&v(&["uninstall", "--force"])).is_err());
    }

    #[test]
    fn setup_options() {
        assert_eq!(
            parse(&v(&["setup", "--apk", "app-debug.apk"])),
            Ok(Command::Setup {
                apk: "app-debug.apk".to_string(),
                deadline_minutes: None
            })
        );
        assert_eq!(
            parse(&v(&["setup", "--deadline-minutes", "2", "--apk", "C:\\a b\\x.APK"])),
            Ok(Command::Setup {
                apk: "C:\\a b\\x.APK".to_string(),
                deadline_minutes: Some(2)
            })
        );
        assert!(parse(&v(&["setup"])).is_err());
        assert!(parse(&v(&["setup", "--apk"])).is_err());
        assert!(parse(&v(&["setup", "--apk", "app.zip"])).is_err());
        assert!(parse(&v(&["setup", "--apk", ""])).is_err());
        assert!(parse(&v(&["setup", "--apk", "a.apk", "--apk", "b.apk"])).is_err());
        assert!(parse(&v(&["setup", "--apk", "a.apk", "--deadline-minutes", "0"])).is_err());
        assert!(parse(&v(&["setup", "--apk", "a.apk", "--deadline-minutes", "30"])).is_err());
        assert!(parse(&v(&["setup", "--apk", "a.apk", "--deadline-minutes", "ten"])).is_err());
        assert!(parse(&v(&["setup", "--apk", "a.apk", "--force"])).is_err());
    }

    #[test]
    fn deadline_limits_are_inclusive() {
        assert_eq!(parse_deadline("1"), Ok(1));
        assert_eq!(parse_deadline("29"), Ok(29));
        assert!(parse_deadline("-1").is_err());
    }

    #[test]
    fn release_needs_a_digit_pin() {
        assert_eq!(
            parse(&v(&["release", "--pin", "2468"])),
            Ok(Command::Release {
                pin: "2468".to_string()
            })
        );
        assert!(parse(&v(&["release"])).is_err());
        assert!(parse(&v(&["release", "--pin", "123"])).is_err());
        assert!(parse(&v(&["release", "--pin", "1234567890123"])).is_err());
        assert!(parse(&v(&["release", "--pin", "24a8"])).is_err());
        assert!(parse(&v(&["release", "--pin", "2468;reboot"])).is_err());
        assert!(parse(&v(&["release", "--pin", "2468", "--pin", "2468"])).is_err());
    }
}
