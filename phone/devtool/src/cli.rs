//! The command line, and the help. Everything that needs no phone and no
//! file is checked here, before adb is touched.

use limo_core::BlockedWindow;

use crate::family::{is_package_name, parse_bedtime};
use crate::phone::PACKAGE;

/// The longest `log` follows the phone's log before it stops by itself, in
/// minutes: 12 hours.
pub const MAX_LOG_MINUTES: u32 = 720;
/// How long `log` follows when not told: an hour.
pub const DEFAULT_LOG_MINUTES: u32 = 60;
/// The most minutes a day `setup` takes: the whole day.
pub const MAX_MINUTES: u32 = 1_440;

/// One command, as the command line gave it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `help`, or nothing.
    Help,
    /// `check`.
    Check,
    /// `install --apk PATH`.
    Install {
        /// The APK's path.
        apk: String,
    },
    /// `setup --child NAME --minutes N [--block PACKAGE]... [--allow PACKAGE]... [--bedtime HH:MM-HH:MM]`.
    Setup {
        /// The child's name.
        child: String,
        /// Minutes a day.
        minutes: u32,
        /// Apps blocked.
        block: Vec<String>,
        /// Apps always allowed.
        allow: Vec<String>,
        /// Hours screens are off, every day.
        bedtime: Option<BlockedWindow>,
    },
    /// `code [--minutes 15|30|60]`.
    Code {
        /// What the code is worth.
        minutes: u32,
    },
    /// `status`.
    Status,
    /// `log [--minutes N]`.
    Log {
        /// How long to follow.
        minutes: u32,
    },
    /// `forget [--this-pc-only]`.
    Forget {
        /// Only this PC's side: for a phone that is gone.
        this_pc_only: bool,
    },
    /// `uninstall`.
    Uninstall,
}

/// The help, with where the test family lives.
pub fn usage(folder: &str) -> String {
    format!(
        "limo-phone-dev: puts a test family on a child's phone over the USB cable, so that LiMo can be tried
on a real phone before pairing over the network exists. It is scaffolding, like limo dev-setup and
limo dev-code on the PC.

What it never does: it changes nothing on the phone but LiMo's own app and LiMo's own files. It
never turns on developer options or USB debugging (you do that yourself, on the phone), and it never
touches the phone's accounts, its Device Owner or its settings.

Commands:
  check
      Reads only. Is adb there, is exactly one phone attached and allowed, which Android, is LiMo's
      child form installed, is it a debug build (only those can take a test family), is a test
      family on it.
  install --apk PATH
      Installs that APK with adb install, keeping LiMo's files if it is there already. Refuses
      anything that is not a debug build of LiMo's child form ({PACKAGE}).
  setup --child NAME --minutes N [--block PACKAGE]... [--allow PACKAGE]... [--bedtime HH:MM-HH:MM]
      Asks for a guardian PIN twice, at a prompt that does not show it; makes the test family on
      this PC (N minutes a day, 0 to {MAX_MINUTES}, every day; the away share at 100 percent, so that a
      phone with no master has its whole day); pairs the phone; hands LiMo the pairing file over
      the cable; and reads back that LiMo took it. Once only: forget starts again.
  code [--minutes 15|30|60]
      Prints a one time code for the test phone, worth 15 minutes unless told, for today. Needs no
      phone. Each run makes a new one; a phone takes ten a day.
  status
      What LiMo on the phone holds, as its own engine reads its kept state: the child, the time
      left, the time used and each app's minutes, whether it is locked and why, wrong tries, a
      request that waits. Never a PIN or a code.
  log [--minutes N]
      Follows the phone's log for LiMo's tags (LiMo and LiMoWatch) until Ctrl+C, or N minutes
      ({DEFAULT_LOG_MINUTES} unless told, at most {MAX_LOG_MINUTES}).
  forget [--this-pc-only]
      Asks LiMo on the phone to wipe the test family, checks that it did, and deletes the test
      family on this PC. With --this-pc-only, for a phone that is gone: this PC's side alone.
  uninstall
      Removes LiMo from the phone, with everything it kept. The test family on this PC stays until
      forget.
  help
      This.

The test family lives in this folder, apart from the family of the LiMo service, which this tool
never opens:
  {folder}
Exit codes: 0 done, 1 failed, 2 refused (nothing was changed).
"
    )
}

/// The value after an option, from `--name VALUE` or `--name=VALUE`.
fn value(args: &[String], i: &mut usize, name: &str) -> Result<String, String> {
    let arg = &args[*i];
    if let Some(v) = arg.strip_prefix(&format!("{name}=")) {
        return Ok(v.to_string());
    }
    *i += 1;
    args.get(*i).cloned().ok_or_else(|| format!("{name} needs a value."))
}

fn number(text: &str, name: &str) -> Result<u32, String> {
    text.trim().parse::<u32>().map_err(|_| format!("{name} takes a whole number, not \"{text}\"."))
}

/// Reads the command line (the arguments after the program's name).
pub fn parse(argv: &[String]) -> Result<Command, String> {
    let Some(first) = argv.first() else { return Ok(Command::Help) };
    let rest = &argv[1..];
    let mut i = 0;
    let unknown = |arg: &str| format!("{first} does not take {arg}. See limo-phone-dev help.");
    match first.as_str() {
        "help" | "--help" | "-h" | "/?" => Ok(Command::Help),
        "check" | "status" | "uninstall" => {
            if let Some(arg) = rest.first() {
                return Err(unknown(arg));
            }
            Ok(match first.as_str() {
                "check" => Command::Check,
                "status" => Command::Status,
                _ => Command::Uninstall,
            })
        }
        "install" => {
            let mut apk = None;
            while i < rest.len() {
                match rest[i].as_str() {
                    a if a == "--apk" || a.starts_with("--apk=") => apk = Some(value(rest, &mut i, "--apk")?),
                    other => return Err(unknown(other)),
                }
                i += 1;
            }
            let apk = apk.filter(|a| !a.trim().is_empty()).ok_or("install needs --apk PATH: the APK of LiMo's child form, a debug build.")?;
            Ok(Command::Install { apk })
        }
        "setup" => {
            let (mut child, mut minutes, mut block, mut allow, mut bedtime) = (None, None, Vec::new(), Vec::new(), None);
            while i < rest.len() {
                match rest[i].as_str() {
                    a if a == "--child" || a.starts_with("--child=") => child = Some(value(rest, &mut i, "--child")?),
                    a if a == "--minutes" || a.starts_with("--minutes=") => {
                        minutes = Some(number(&value(rest, &mut i, "--minutes")?, "--minutes")?)
                    }
                    a if a == "--block" || a.starts_with("--block=") => block.push(value(rest, &mut i, "--block")?),
                    a if a == "--allow" || a.starts_with("--allow=") => allow.push(value(rest, &mut i, "--allow")?),
                    a if a == "--bedtime" || a.starts_with("--bedtime=") => {
                        bedtime = Some(parse_bedtime(&value(rest, &mut i, "--bedtime")?)?)
                    }
                    other => return Err(unknown(other)),
                }
                i += 1;
            }
            let child = child.map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).ok_or("setup needs --child NAME: the test child's name, such as Asha.")?;
            let minutes = minutes.ok_or("setup needs --minutes N: the minutes a day, such as 20.")?;
            if minutes > MAX_MINUTES {
                return Err(format!("--minutes is {minutes}, more than a whole day. Use 0 to {MAX_MINUTES}."));
            }
            for (list, name) in [(&block, "--block"), (&allow, "--allow")] {
                for package in list {
                    if !is_package_name(package) {
                        return Err(format!("{name} takes an app's package name, such as com.example.game, not \"{package}\"."));
                    }
                    if package == PACKAGE {
                        return Err(format!("{name} names LiMo itself, which is never counted or covered."));
                    }
                    if list.iter().filter(|p| *p == package).count() > 1 {
                        return Err(format!("{package} is given twice to {name}."));
                    }
                }
            }
            if let Some(both) = block.iter().find(|b| allow.contains(b)) {
                return Err(format!("{both} is given both to --block and to --allow. Choose one."));
            }
            Ok(Command::Setup { child, minutes, block, allow, bedtime })
        }
        "code" => {
            let mut minutes = 15;
            while i < rest.len() {
                match rest[i].as_str() {
                    a if a == "--minutes" || a.starts_with("--minutes=") => {
                        minutes = number(&value(rest, &mut i, "--minutes")?, "--minutes")?
                    }
                    other => return Err(unknown(other)),
                }
                i += 1;
            }
            if ![15, 30, 60].contains(&minutes) {
                return Err(format!("A one time code is worth 15, 30 or 60 minutes, not {minutes}."));
            }
            Ok(Command::Code { minutes })
        }
        "log" => {
            let mut minutes = DEFAULT_LOG_MINUTES;
            while i < rest.len() {
                match rest[i].as_str() {
                    a if a == "--minutes" || a.starts_with("--minutes=") => {
                        minutes = number(&value(rest, &mut i, "--minutes")?, "--minutes")?
                    }
                    other => return Err(unknown(other)),
                }
                i += 1;
            }
            if minutes == 0 || minutes > MAX_LOG_MINUTES {
                return Err(format!("log follows for 1 to {MAX_LOG_MINUTES} minutes, not {minutes}."));
            }
            Ok(Command::Log { minutes })
        }
        "forget" => {
            let mut this_pc_only = false;
            for arg in rest {
                match arg.as_str() {
                    "--this-pc-only" => this_pc_only = true,
                    other => return Err(unknown(other)),
                }
            }
            Ok(Command::Forget { this_pc_only })
        }
        other => Err(format!("There is no command \"{other}\". See limo-phone-dev help.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn each_command_is_read() {
        assert_eq!(parse(&[]), Ok(Command::Help));
        assert_eq!(parse(&args("help")), Ok(Command::Help));
        assert_eq!(parse(&args("check")), Ok(Command::Check));
        assert_eq!(parse(&args("status")), Ok(Command::Status));
        assert_eq!(parse(&args("uninstall")), Ok(Command::Uninstall));
        assert_eq!(parse(&args("install --apk a.apk")), Ok(Command::Install { apk: "a.apk".to_string() }));
        assert_eq!(parse(&args("install --apk=a.apk")), Ok(Command::Install { apk: "a.apk".to_string() }));
        assert_eq!(parse(&args("code")), Ok(Command::Code { minutes: 15 }));
        assert_eq!(parse(&args("code --minutes 60")), Ok(Command::Code { minutes: 60 }));
        assert_eq!(parse(&args("log")), Ok(Command::Log { minutes: 60 }));
        assert_eq!(parse(&args("log --minutes 5")), Ok(Command::Log { minutes: 5 }));
        assert_eq!(parse(&args("forget")), Ok(Command::Forget { this_pc_only: false }));
        assert_eq!(parse(&args("forget --this-pc-only")), Ok(Command::Forget { this_pc_only: true }));
        let setup = parse(&args("setup --child Asha --minutes 20 --block com.example.game --allow com.example.clock --bedtime 21:00-07:00")).unwrap();
        let Command::Setup { child, minutes, block, allow, bedtime } = setup else { panic!("not setup") };
        assert_eq!((child.as_str(), minutes), ("Asha", 20));
        assert_eq!(block, vec!["com.example.game".to_string()]);
        assert_eq!(allow, vec!["com.example.clock".to_string()]);
        assert_eq!(bedtime.map(|b| (b.start_minute, b.end_minute)), Some((1260, 420)));
    }

    #[test]
    fn anything_wrong_is_refused_with_what_to_do() {
        for (line, says) in [
            ("install", "--apk PATH"),
            ("setup --minutes 20", "--child NAME"),
            ("setup --child Asha", "--minutes N"),
            ("setup --child Asha --minutes 2000", "more than a whole day"),
            ("setup --child Asha --minutes many", "whole number"),
            ("setup --child Asha --minutes 20 --block notepad", "package name"),
            ("setup --child Asha --minutes 20 --allow com.game!", "package name"),
            ("setup --child Asha --minutes 20 --block ai.lucidloom.limo.child", "LiMo itself"),
            ("setup --child Asha --minutes 20 --block com.a.b --block com.a.b", "twice"),
            ("setup --child Asha --minutes 20 --block com.a.b --allow com.a.b", "Choose one"),
            ("setup --child Asha --minutes 20 --bedtime 9pm", "--bedtime"),
            ("code --minutes 20", "15, 30 or 60"),
            ("log --minutes 0", "1 to 720"),
            ("check --now", "does not take"),
            ("pair", "no command"),
        ] {
            let problem = parse(&args(line)).unwrap_err();
            assert!(problem.contains(says), "{line}: {problem}");
        }
    }

    #[test]
    fn the_help_says_what_the_tool_never_does_first() {
        let help = usage("C:\\example\\LiMo-phone-dev-TEST-ONLY");
        let never = help.find("What it never does").unwrap();
        assert!(never < help.find("Commands:").unwrap());
        for promise in ["LiMo's own app and LiMo's own files", "developer options or USB debugging", "accounts, its Device Owner or its settings"] {
            assert!(help.contains(promise), "{promise}");
        }
    }
}
