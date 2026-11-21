//! The command line of `limo.exe`: which command, with what.

#![forbid(unsafe_code)]

/// The fewest minutes `dev-setup` gives a day.
pub const MIN_DEV_MINUTES: u32 = 1;
/// The most minutes `dev-setup` gives a day.
pub const MAX_DEV_MINUTES: u32 = 240;
/// The minutes `dev-setup` gives a day when not told: a small budget, to see
/// the warnings and the lock soon.
pub const DEFAULT_DEV_MINUTES: u32 = 10;

/// One command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// The service control manager's entry.
    Service,
    /// The helper, as the service starts it, with the values of its two
    /// channel handles (the one it reads, the one it writes), when given.
    Helper {
        /// The handles, when `--channel R:W` was given and reads as two
        /// numbers.
        channel: Option<(usize, usize)>,
        /// `--plain`: show the plain display, not the drawn one, because the
        /// settings file asks for it.
        plain: bool,
    },
    /// Install the service, with the test cap when `dev` is set.
    Install {
        /// `--dev`: switch the test cap on.
        dev: bool,
    },
    /// Remove the service and both folders.
    Uninstall,
    /// The service's state and the end of its log.
    Status,
    /// Make a family for testing, until the guardian's console exists.
    DevSetup {
        /// The child's name.
        child: String,
        /// The Windows account to link to the child.
        account: String,
        /// The child's daily budget, in minutes.
        minutes: u32,
        /// Programs to block for the child, by file name.
        block: Vec<String>,
    },
    /// Make a one time code for the test child's device on this PC, until
    /// the guardian's phone app exists.
    DevCode {
        /// The child's name, when the family has more than one child.
        child: Option<String>,
        /// The minutes the code adds: 15, 30 or 60.
        minutes: u32,
    },
    /// Set a new guardian password without the old one: for an
    /// administrator of this PC whose family forgot it.
    ResetPassword,
    /// A small client of the console's door, typed at by hand, until the
    /// guardian's console exists.
    DevConsole,
    /// Say how to use it.
    Help,
}

/// The minutes `dev-code` makes a code for when not told.
pub const DEFAULT_CODE_MINUTES: u32 = 15;

/// The value after a flag, if the flag is there.
fn value_of<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let at = args.iter().position(|a| a == flag)?;
    args.get(at + 1).map(String::as_str)
}

/// `R:W`, two handle values in decimal.
pub fn parse_channel(text: &str) -> Option<(usize, usize)> {
    let (read, write) = text.split_once(':')?;
    let read: usize = read.trim().parse().ok()?;
    let write: usize = write.trim().parse().ok()?;
    (read != 0 && write != 0 && read != write).then_some((read, write))
}

/// The command a command line asks for, or why it cannot be run, in plain
/// words.
pub fn parse(args: &[String]) -> Result<Command, String> {
    let rest = args.get(1..).unwrap_or(&[]);
    match args.first().map(String::as_str) {
        None | Some("help" | "--help" | "-h" | "/?") => Ok(Command::Help),
        Some("service") => Ok(Command::Service),
        Some("helper") => Ok(Command::Helper {
            channel: value_of(rest, "--channel").and_then(parse_channel),
            plain: rest.iter().any(|a| a == "--plain"),
        }),
        Some("install") => match rest {
            [] => Ok(Command::Install { dev: false }),
            [flag] if flag == "--dev" => Ok(Command::Install { dev: true }),
            _ => Err("install takes nothing, or --dev for a test install with the test cap.".to_string()),
        },
        Some("uninstall") => Ok(Command::Uninstall),
        Some("status") => Ok(Command::Status),
        Some("reset-password") => match rest {
            [] => Ok(Command::ResetPassword),
            _ => Err("reset-password takes nothing: the new password is asked for at the prompt, not shown.".to_string()),
        },
        Some("dev-console") => match rest {
            [] => Ok(Command::DevConsole),
            _ => Err("dev-console takes nothing; type help once it runs.".to_string()),
        },
        Some("dev-setup") => {
            let child = value_of(rest, "--child").map(str::trim).filter(|s| !s.is_empty());
            let account = value_of(rest, "--account").map(str::trim).filter(|s| !s.is_empty());
            let (Some(child), Some(account)) = (child, account) else {
                return Err("dev-setup needs --child and --account, for example: limo dev-setup --child Asha --account asha".to_string());
            };
            let minutes = match value_of(rest, "--minutes") {
                None => DEFAULT_DEV_MINUTES,
                Some(text) => match text.parse::<u32>() {
                    Ok(m) if (MIN_DEV_MINUTES..=MAX_DEV_MINUTES).contains(&m) => m,
                    _ => {
                        return Err(format!(
                            "--minutes takes a whole number from {MIN_DEV_MINUTES} to {MAX_DEV_MINUTES}."
                        ));
                    }
                },
            };
            let block: Vec<String> = rest
                .iter()
                .enumerate()
                .filter(|(_, a)| a.as_str() == "--block")
                .filter_map(|(at, _)| rest.get(at + 1))
                .map(|name| name.trim().to_string())
                .filter(|name| !name.is_empty())
                .collect();
            Ok(Command::DevSetup { child: child.to_string(), account: account.to_string(), minutes, block })
        }
        Some("dev-code") => {
            let child =
                value_of(rest, "--child").map(str::trim).filter(|s| !s.is_empty() && !s.starts_with("--")).map(String::from);
            if rest.iter().any(|a| a == "--child") && child.is_none() {
                return Err("--child takes the child’s name, for example: limo dev-code --child Asha".to_string());
            }
            let minutes = match value_of(rest, "--minutes") {
                None if rest.iter().any(|a| a == "--minutes") => {
                    return Err("--minutes takes 15, 30 or 60.".to_string());
                }
                None => DEFAULT_CODE_MINUTES,
                Some(text) => match text.parse::<u32>() {
                    Ok(m) if limo_core::CODE_MINUTES.contains(&m) => m,
                    _ => return Err("--minutes takes 15, 30 or 60.".to_string()),
                },
            };
            Ok(Command::DevCode { child, minutes })
        }
        Some(other) => Err(format!("limo does not know the command \"{other}\". Run limo help.")),
    }
}

/// What `limo help` prints.
pub fn usage() -> String {
    [
        "limo: LiMo for Windows, a free screen time limiter for children.",
        "",
        "  limo install [--dev]   install and start the service (administrator).",
        "                         --dev also switches on the test cap: any single",
        "                         lock is lifted after 120 seconds. For testing only.",
        "  limo uninstall         stop and remove the service, the program and the",
        "                         data folder with the family’s file (administrator).",
        "  limo status            the service’s state, and the end of its log when",
        "                         this account may read it.",
        "  limo reset-password    (administrator) sets a new guardian password when",
        "                         the family forgot it: asked for at the prompt, not",
        "                         shown. Everything else is kept. The service is",
        "                         stopped for a moment and started again.",
        "  limo dev-setup --child NAME --account WINDOWS_ACCOUNT [--minutes N] [--block PROGRAM]...",
        "                         (administrator, with the service stopped)",
        "                         SCAFFOLDING until the guardian’s console exists:",
        "                         makes a family with a guardian password and PIN",
        "                         typed at the prompt, one child, links the Windows",
        "                         account to the child, and gives the child N",
        "                         minutes a day (10 when not given, 1 to 240),",
        "                         blocking each PROGRAM given, such as notepad.exe.",
        "  limo dev-code [--minutes 15|30|60] [--child NAME]",
        "                         (administrator, with the service stopped)",
        "                         SCAFFOLDING until the guardian’s phone app exists:",
        "                         makes and prints a one time code for the test",
        "                         child’s device on this PC, for today only, worth",
        "                         the minutes given (15 when not given). Each run",
        "                         makes a new code; a device takes ten a day.",
        "  limo dev-console       SCAFFOLDING until the guardian’s console exists:",
        "                         knocks on the door of LiMo’s settings from this",
        "                         account, and sends what is typed (type help).",
        "  limo service           the service control manager’s entry only.",
        "  limo helper            started by the service only.",
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn each_command_reads_as_it_should() {
        assert_eq!(parse(&[]), Ok(Command::Help));
        assert_eq!(parse(&args("service")), Ok(Command::Service));
        assert_eq!(parse(&args("install")), Ok(Command::Install { dev: false }));
        assert_eq!(parse(&args("install --dev")), Ok(Command::Install { dev: true }));
        assert!(parse(&args("install --helper-as system")).is_err());
        assert_eq!(parse(&args("uninstall")), Ok(Command::Uninstall));
        assert_eq!(parse(&args("status")), Ok(Command::Status));
        assert_eq!(
            parse(&args("dev-setup --child Asha --account asha")),
            Ok(Command::DevSetup { child: "Asha".into(), account: "asha".into(), minutes: DEFAULT_DEV_MINUTES, block: vec![] })
        );
        assert_eq!(
            parse(&args("dev-setup --account mira --child Mira --minutes 3 --block notepad.exe --block explorer.exe")),
            Ok(Command::DevSetup {
                child: "Mira".into(),
                account: "mira".into(),
                minutes: 3,
                block: vec!["notepad.exe".into(), "explorer.exe".into()],
            })
        );
        assert!(parse(&args("dev-setup --child Asha")).is_err());
        assert!(parse(&args("dev-setup --child Asha --account asha --minutes 0")).is_err());
        assert!(parse(&args("dev-setup --child Asha --account asha --minutes 241")).is_err());
        assert!(parse(&args("lock --seconds 60")).is_err());
        assert_eq!(parse(&args("reset-password")), Ok(Command::ResetPassword));
        assert!(parse(&args("reset-password hunter22")).is_err(), "a password is never taken on the command line");
        assert_eq!(parse(&args("dev-console")), Ok(Command::DevConsole));
        assert!(parse(&args("dev-console --pipe x")).is_err());
        assert!(usage().contains("reset-password") && usage().contains("dev-console"));
    }

    #[test]
    fn a_helper_without_a_channel_says_so() {
        assert_eq!(parse(&args("helper")), Ok(Command::Helper { channel: None, plain: false }));
        assert_eq!(parse(&args("helper --channel 524:612")), Ok(Command::Helper { channel: Some((524, 612)), plain: false }));
        assert_eq!(parse(&args("helper --channel 524:612 --plain")), Ok(Command::Helper { channel: Some((524, 612)), plain: true }));
        for bad in ["helper --channel", "helper --channel 1", "helper --channel a:b", "helper --channel 0:5", "helper --channel 7:7"] {
            assert_eq!(parse(&args(bad)), Ok(Command::Helper { channel: None, plain: false }), "{bad}");
        }
    }

    #[test]
    fn dev_code_takes_the_minutes_of_a_code_and_a_child_if_needed() {
        assert_eq!(parse(&args("dev-code")), Ok(Command::DevCode { child: None, minutes: DEFAULT_CODE_MINUTES }));
        assert_eq!(parse(&args("dev-code --minutes 60")), Ok(Command::DevCode { child: None, minutes: 60 }));
        assert_eq!(
            parse(&args("dev-code --child Mira --minutes 30")),
            Ok(Command::DevCode { child: Some("Mira".into()), minutes: 30 })
        );
        for bad in ["dev-code --minutes 20", "dev-code --minutes", "dev-code --minutes x", "dev-code --child", "dev-code --child --minutes 30"] {
            assert!(parse(&args(bad)).is_err(), "{bad}");
        }
        assert!(usage().contains("dev-code") && usage().contains("SCAFFOLDING until the guardian’s phone app exists"));
    }
}
