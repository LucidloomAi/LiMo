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
    /// Say how to use it.
    Help,
}

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
        Some("helper") => Ok(Command::Helper { channel: value_of(rest, "--channel").and_then(parse_channel) }),
        Some("install") => match rest {
            [] => Ok(Command::Install { dev: false }),
            [flag] if flag == "--dev" => Ok(Command::Install { dev: true }),
            _ => Err("install takes nothing, or --dev for a test install with the test cap.".to_string()),
        },
        Some("uninstall") => Ok(Command::Uninstall),
        Some("status") => Ok(Command::Status),
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
        "                         data folder with the family's file (administrator).",
        "  limo status            the service's state, and the end of its log when",
        "                         this account may read it.",
        "  limo dev-setup --child NAME --account WINDOWS_ACCOUNT [--minutes N] [--block PROGRAM]...",
        "                         (administrator, with the service stopped)",
        "                         SCAFFOLDING until the guardian's console exists:",
        "                         makes a family with a guardian password and PIN",
        "                         typed at the prompt, one child, links the Windows",
        "                         account to the child, and gives the child N",
        "                         minutes a day (10 when not given, 1 to 240),",
        "                         blocking each PROGRAM given, such as notepad.exe.",
        "  limo service           the service control manager's entry only.",
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
    }

    #[test]
    fn a_helper_without_a_channel_says_so() {
        assert_eq!(parse(&args("helper")), Ok(Command::Helper { channel: None }));
        assert_eq!(parse(&args("helper --channel 524:612")), Ok(Command::Helper { channel: Some((524, 612)) }));
        for bad in ["helper --channel", "helper --channel 1", "helper --channel a:b", "helper --channel 0:5", "helper --channel 7:7"] {
            assert_eq!(parse(&args(bad)), Ok(Command::Helper { channel: None }), "{bad}");
        }
    }
}
