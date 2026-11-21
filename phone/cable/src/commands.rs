//! Everything LiMo asks of a phone over the cable, in one place, and how
//! each answer is read.
//!
//! The phone is asked for its maker, model and Android version, whether USB
//! debugging is on, whether LiMo's child form is installed and which
//! version; LiMo's child form is installed (with replace), handed its
//! pairing offer through its guarded door, and started; and, as its own
//! call, USB debugging is switched off. No command here names any other
//! app, an account, a device owner or any other setting, and none removes an
//! app: a phone holding a LiMo signed with another key is told so and left
//! as it is. A test (`tests/house_rules.rs`) reads every command to check it.

#![forbid(unsafe_code)]

/// LiMo's child form, the only app LiMo installs on a phone.
pub const PACKAGE: &str = "ai.lucidloom.limo.child";
/// Its screen, which it opens when started over the cable.
pub const ACTIVITY: &str = "ai.lucidloom.limo.child/.MainActivity";
/// Its cable door for the pairing offer: a content provider that only the
/// adb shell may write to (`android.permission.DUMP`), written from standard
/// input, so that the offer is never in a command line.
pub const OFFER_DOOR: &str = "content://ai.lucidloom.limo.child.cable/offer";
/// The link's port: the phone's own 127.0.0.1 at this port is carried to
/// the PC's by `reverse` (the link's `DEFAULT_PORT`).
pub const LINK_PORT: u16 = 47810;
/// The oldest Android LiMo runs on: 8.0, API 26.
pub const OLDEST_SDK: u32 = 26;
/// The most an offer may be: the door takes at most this many bytes.
pub const MAX_OFFER_BYTES: usize = 1024;

/// One thing asked of the phone with a shell command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    /// Its maker, as Android names it.
    Manufacturer,
    /// Its model.
    Model,
    /// Its Android version, such as 13.
    AndroidVersion,
    /// Its API level, such as 33.
    Sdk,
    /// Whether USB debugging is on: 1 or 0.
    UsbDebugging,
    /// Whether LiMo's child form is installed.
    LimoListed,
    /// LiMo's child form's version, from the package manager.
    LimoVersion,
    /// The pairing offer into the door, on standard input.
    OfferIntoDoor,
    /// LiMo's screen, started.
    StartLimo,
    /// USB debugging switched off: the last thing asked; the link ends.
    DebuggingOff,
}

impl Ask {
    /// The command, exactly as it crosses the cable.
    pub fn command(&self) -> String {
        match self {
            Ask::Manufacturer => "getprop ro.product.manufacturer".to_string(),
            Ask::Model => "getprop ro.product.model".to_string(),
            Ask::AndroidVersion => "getprop ro.build.version.release".to_string(),
            Ask::Sdk => "getprop ro.build.version.sdk".to_string(),
            Ask::UsbDebugging => "settings get global adb_enabled".to_string(),
            Ask::LimoListed => format!("pm list packages {PACKAGE}"),
            Ask::LimoVersion => format!("dumpsys package {PACKAGE}"),
            Ask::OfferIntoDoor => format!("content write --uri {OFFER_DOOR}"),
            Ask::StartLimo => format!("am start -n {ACTIVITY}"),
            Ask::DebuggingOff => "settings put global adb_enabled 0".to_string(),
        }
    }

    /// Every command, for the house rules' test.
    pub fn every() -> [Ask; 10] {
        [
            Ask::Manufacturer,
            Ask::Model,
            Ask::AndroidVersion,
            Ask::Sdk,
            Ask::UsbDebugging,
            Ask::LimoListed,
            Ask::LimoVersion,
            Ask::OfferIntoDoor,
            Ask::StartLimo,
            Ask::DebuggingOff,
        ]
    }
}

/// The install service: the APK streamed to the package manager, replacing
/// an app that is there, as Google's adb installs.
pub fn install_command(size: u64) -> String {
    format!("cmd package install -r -S {size}")
}

/// The text of an answer, each line trimmed, empty lines left out.
pub fn lines(text: &str) -> Vec<String> {
    text.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()
}

/// The first line of an answer, trimmed; empty when there is none.
pub fn first_line(text: &str) -> String {
    lines(text).into_iter().next().unwrap_or_default()
}

/// True when `pm list packages` lists LiMo's child form itself, not merely a
/// package whose name starts with it.
pub fn lists_limo(text: &str) -> bool {
    lines(text).iter().any(|l| l == &format!("package:{PACKAGE}"))
}

/// The version code and the version name `dumpsys package` gives, the first
/// of each.
pub fn parse_version(text: &str) -> (Option<u64>, Option<String>) {
    let mut code = None;
    let mut name = None;
    for line in text.lines() {
        for word in line.split_whitespace() {
            if code.is_none() && let Some(value) = word.strip_prefix("versionCode=") {
                code = value.chars().take_while(char::is_ascii_digit).collect::<String>().parse().ok();
            }
        }
        if name.is_none() && let Some(at) = line.find("versionName=") {
            let value = line[at + "versionName=".len()..].trim();
            if !value.is_empty() {
                name = Some(value.to_string());
            }
        }
    }
    (code, name)
}

/// What `am start` said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Started {
    /// The screen was started.
    Started,
    /// The screen was open already, and was handed the start: an app that
    /// acts only when its screen is made would do nothing.
    HandedToOpenScreen,
    /// Android did not start it; its words.
    Refused(String),
}

/// Reads `am start`'s answer.
pub fn parse_start(text: &str) -> Started {
    let all = lines(text);
    if let Some(error) = all.iter().find(|l| l.starts_with("Error")) {
        return Started::Refused(error.clone());
    }
    if all.iter().any(|l| l.contains("delivered to currently running top-most instance")) {
        return Started::HandedToOpenScreen;
    }
    if all.iter().any(|l| l.starts_with("Starting:")) {
        Started::Started
    } else {
        Started::Refused(all.join(" "))
    }
}

/// What the package manager said to an install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Installed {
    /// Installed (or replaced).
    Done,
    /// Refused, with Android's code (`INSTALL_FAILED_...`), when it gave one,
    /// and its words.
    Refused {
        /// Android's code, or empty.
        code: String,
        /// What Android said.
        said: String,
    },
}

/// Reads the package manager's answer to an install.
pub fn parse_install(text: &str) -> Installed {
    let all = lines(text);
    if all.iter().any(|l| l == "Success") {
        return Installed::Done;
    }
    let said = all.iter().find(|l| l.contains("Failure")).cloned().unwrap_or_else(|| all.join(" "));
    let code = said
        .split('[')
        .nth(1)
        .map(|rest| rest.split([':', ']', ' ']).next().unwrap_or_default().to_string())
        .unwrap_or_default();
    Installed::Refused { code, said }
}

/// What to tell a guardian when Android refuses to install LiMo, by its
/// code. LiMo never removes an app from the phone to make an install pass.
pub fn install_advice(code: &str) -> &'static str {
    match code {
        "INSTALL_FAILED_UPDATE_INCOMPATIBLE" => {
            "This phone holds a LiMo that was not made by the same maker as the LiMo on this PC (it was signed with another \
             key). LiMo on this PC leaves it as it is: whoever put it there can remove it on the phone, and then LiMo can be \
             put on again."
        }
        "INSTALL_FAILED_VERSION_DOWNGRADE" => {
            "This phone holds a newer LiMo than the one on this PC. Update LiMo on this PC first, then add the phone again."
        }
        "INSTALL_FAILED_OLDER_SDK" => "This phone's Android is older than 8.0, the oldest LiMo runs on.",
        "INSTALL_FAILED_INSUFFICIENT_STORAGE" => "The phone has no room for LiMo. Make room on the phone, then try again.",
        "INSTALL_FAILED_USER_RESTRICTED" => {
            "The phone refused an install over the cable. Some phones want it allowed on the phone itself (\"Install via USB\" \
             in Developer options), and newer Samsung phones block it while Auto Blocker is on: look at the phone, then try again."
        }
        "INSTALL_FAILED_ABORTED" | "INSTALL_FAILED_VERIFICATION_FAILURE" => {
            "The phone stopped the install, or asked about it on its screen. Look at the phone, then try again."
        }
        _ => "Look at the phone's screen for a question, then try again.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_as_it_crosses_the_cable() {
        assert_eq!(Ask::LimoListed.command(), "pm list packages ai.lucidloom.limo.child");
        assert_eq!(Ask::LimoVersion.command(), "dumpsys package ai.lucidloom.limo.child");
        assert_eq!(Ask::OfferIntoDoor.command(), "content write --uri content://ai.lucidloom.limo.child.cable/offer");
        assert_eq!(Ask::StartLimo.command(), "am start -n ai.lucidloom.limo.child/.MainActivity");
        assert_eq!(Ask::DebuggingOff.command(), "settings put global adb_enabled 0");
        assert_eq!(install_command(644_888), "cmd package install -r -S 644888");
    }

    #[test]
    fn answers_are_read_the_same_with_carriage_returns() {
        assert!(lists_limo("package:ai.lucidloom.limo.child\r\n"));
        assert!(!lists_limo("package:ai.lucidloom.limo.childx\n"));
        assert_eq!(first_line("\r\n13\r\n"), "13");
        let dump = "Packages:\n  Package [ai.lucidloom.limo.child] (1a2b):\n    versionCode=12 minSdk=26 targetSdk=36\n    versionName=0.3.0\n    versionCode=9\n";
        assert_eq!(parse_version(dump), (Some(12), Some("0.3.0".to_string())));
        assert_eq!(parse_version("Unable to find package: x\n"), (None, None));
    }

    #[test]
    fn am_start_and_install_answers_are_told_apart() {
        assert_eq!(parse_start("Starting: Intent { cmp=ai.lucidloom.limo.child/.MainActivity }\n"), Started::Started);
        assert_eq!(
            parse_start(
                "Starting: Intent { cmp=ai.lucidloom.limo.child/.MainActivity }\nWarning: Activity not started, intent has been delivered to currently running top-most instance.\n"
            ),
            Started::HandedToOpenScreen
        );
        assert!(matches!(parse_start("Starting: Intent\nError type 3\nError: Activity class does not exist.\n"), Started::Refused(_)));
        assert_eq!(parse_install("Success\n"), Installed::Done);
        let refused = parse_install("Failure [INSTALL_FAILED_UPDATE_INCOMPATIBLE: signatures do not match]\n");
        assert_eq!(refused, Installed::Refused {
            code: "INSTALL_FAILED_UPDATE_INCOMPATIBLE".to_string(),
            said: "Failure [INSTALL_FAILED_UPDATE_INCOMPATIBLE: signatures do not match]".to_string()
        });
        assert!(install_advice("INSTALL_FAILED_UPDATE_INCOMPATIBLE").contains("leaves it as it is"));
        assert!(install_advice("anything").starts_with("Look at the phone"));
    }
}
