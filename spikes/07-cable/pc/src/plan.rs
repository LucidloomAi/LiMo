//! What each step does to the phone, said before it does it; the whole run
//! (`plan`), for a person to read before starting; and the help.

#![forbid(unsafe_code)]

use crate::cable::Via;
use crate::phone::{LOOPBACK_PORT, PACKAGE};

/// The steps, as the command line names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepName {
    /// Windows' lists of devices and drivers.
    Drivers,
    /// The USB lists of the routes.
    List,
    /// The prompt, and three properties read.
    Connect,
    /// The test app installed.
    Install,
    /// The secret into the test app's door.
    WriteSecret,
    /// The test app's screen opened.
    Start,
    /// The phone's loopback carried to this PC's echo server.
    Reverse,
    /// The test app removed.
    Uninstall,
    /// USB debugging switched off: the last step.
    DebuggingOff,
}

impl StepName {
    /// The word on the command line.
    pub fn word(&self) -> &'static str {
        match self {
            StepName::Drivers => "drivers",
            StepName::List => "list",
            StepName::Connect => "connect",
            StepName::Install => "install",
            StepName::WriteSecret => "write-secret",
            StepName::Start => "start",
            StepName::Reverse => "reverse",
            StepName::Uninstall => "uninstall",
            StepName::DebuggingOff => "debugging-off",
        }
    }

    /// Every step, in the order of the run.
    pub fn every() -> [StepName; 9] {
        [
            StepName::Drivers,
            StepName::List,
            StepName::Connect,
            StepName::Install,
            StepName::WriteSecret,
            StepName::Start,
            StepName::Reverse,
            StepName::Uninstall,
            StepName::DebuggingOff,
        ]
    }

    /// From the command line's word.
    pub fn from_word(word: &str) -> Option<StepName> {
        StepName::every().into_iter().find(|s| s.word() == word)
    }

    /// True for the steps that reach the phone, which need `--via`.
    pub fn needs_a_route(&self) -> bool {
        !matches!(self, StepName::Drivers | StepName::List)
    }
}

/// What a step does to the phone, said first, before anything is done.
pub fn does(step: StepName, via: Option<Via>) -> String {
    match step {
        StepName::Drivers => "Nothing reaches the phone. This reads Windows' own lists of devices and drivers, and \
             changes nothing on this PC."
            .to_string(),
        StepName::List => match via {
            Some(Via::Adb) => "This asks Google's adb which phones it sees (adb devices), then stops adb's server \
                 again. Nothing is asked of the phone but adb's own connection."
                .to_string(),
            _ => "Nothing reaches the phone. This reads the USB lists of libusb and of nusb, the devices' own \
                 descriptors only: nothing is opened for use or claimed."
                .to_string(),
        },
        StepName::Connect => {
            let key = match via {
                Some(Via::Adb) => "Google's adb's own key",
                _ => "this program's own adb key",
            };
            format!(
                "This opens the phone's debugging interface and connects with {key}. If the phone has not allowed \
                 that key, it shows \"Allow USB debugging?\". Then it reads the model, the Android version, the \
                 API level and whether USB debugging is on, and whether the test app is installed. It changes \
                 nothing on the phone; with \"Always allow\" ticked, the phone remembers the key."
            )
        }
        StepName::Install => format!(
            "This installs the test app \"LiMo spike 07\" ({PACKAGE}), a release build signed with a throwaway \
             key, and replaces it if it is installed already. It installs nothing else, and never LiMo itself."
        ),
        StepName::WriteSecret => "This writes a fresh random sample secret, never shown, into the test app's \
             guarded door, through the standard input of `content write`, and then reads what the door saw: its \
             length and its check, never the secret. It changes nothing but what the test app holds in memory."
            .to_string(),
        StepName::Start => "This opens the test app's screen (am start). Nothing else.".to_string(),
        StepName::Reverse => format!(
            "This listens on 127.0.0.1:{LOOPBACK_PORT} on this PC with an echo server, asks the phone to carry \
             its own 127.0.0.1:{LOOPBACK_PORT} to it (reverse), starts the test app afresh (am start -S stops \
             it first, if it runs) and asks it to send 256 KiB there and check that every byte comes back, then \
             removes the reverse again."
        ),
        StepName::Uninstall => format!(
            "This removes the test app ({PACKAGE}) and everything it kept. Nothing else."
        ),
        StepName::DebuggingOff => "THIS IS THE LAST STEP. It asks Android to switch USB debugging off (settings \
             put global adb_enabled 0). The cable link ends there: nothing reaches the phone over the cable again, \
             from this program or any other, until a person switches USB debugging on again on the phone \
             (Settings, Developer options, USB debugging). It changes no other setting."
            .to_string(),
    }
}

/// The whole run, for a person to read first.
pub fn plan() -> String {
    let mut out = vec![
        "The cable proof: the whole run, at the PC with the phone (about 45 minutes).".to_string(),
        String::new(),
        "Before: the phone unlocked, its screen timeout long (Settings, Display, Screen timeout), USB debugging on, the \
         cable in; Android Studio closed; Google's adb server stopped (adb kill-server). Every step prints, before \
         anything else, exactly what it will do to the phone."
            .to_string(),
        String::new(),
    ];
    let mut n = 0;
    let mut line = |out: &mut Vec<String>, command: &str, what: &str| {
        n += 1;
        out.push(format!("{n:>2}. limo-cable-spike {command}"));
        out.push(format!("    {what}"));
    };
    line(&mut out, "drivers", "Reads Windows' lists: which driver Windows bound to the phone's debugging interface. Nothing reaches the phone.");
    line(&mut out, "list", "Reads what the WinUSB route, libusb and nusb each see. Nothing reaches the phone.");
    for via in [Via::Winusb, Via::Libusb, Via::Nusb, Via::Adb] {
        let v = via.word();
        let first = match via {
            Via::Winusb => "The phone asks \"Allow USB debugging?\" for this program's key (unless it was allowed before): tick Always allow, tap Allow within 45 seconds. Reads properties.",
            Via::Libusb => "The same key. On a phone under Samsung's own composite driver libusb refuses the interface: then skip the rest of this route.",
            Via::Nusb => "The same key. nusb needs Windows' own composite driver: where it refuses, skip the rest of this route.",
            Via::Adb => "Google's adb with its own key (allowed before on this PC, so most likely no question). Reads properties, then stops adb's server, as every step by this route does.",
        };
        line(&mut out, &format!("connect --via {v}"), first);
        line(&mut out, &format!("install --via {v}"), "Installs the test app LiMo spike 07, a release build signed with a throwaway key.");
        line(&mut out, &format!("write-secret --via {v}"), "Writes a random sample secret into the test app's guarded door from standard input; compares what arrived.");
        line(&mut out, &format!("start --via {v}"), "Opens the test app's screen.");
        line(&mut out, &format!("reverse --via {v}"), "Starts the test app afresh and carries its connection to 127.0.0.1:47810 on the phone to an echo server on this PC, 256 KiB each way.");
        line(&mut out, &format!("uninstall --via {v}"), "Removes the test app.");
    }
    line(
        &mut out,
        "debugging-off --via <a route that worked>",
        "LAST. Switches USB debugging off on the phone; the cable link ends there. It asks you to type OFF first.",
    );
    out.push(String::new());
    out.push(
        "Then, on a Windows PC that never had Android tools (about 15 minutes, reading only): copy this one program \
         there, switch USB debugging on again on the phone, plug it in unlocked, and run drivers, list, connect --via \
         winusb, connect --via libusb and connect --via nusb. On that PC leave Always allow unticked, so the phone \
         keeps nothing of it."
            .to_string(),
    );
    out.join("\n")
}

/// The help.
pub fn help() -> String {
    let mut out = vec![
        "limo-cable-spike: LiMo's cable proof. Can the PC reach a phone over USB by itself?".to_string(),
        String::new(),
        "What it never does: it installs and removes only its own test app (ai.lucidloom.limo.spike07), never LiMo \
         and never anything else; it never touches the phone's accounts, a device owner, or any setting but the one \
         the last step switches off (USB debugging), and only when that step is run and OFF is typed; it never \
         shows or keeps a serial number (only its last four characters are shown); it installs nothing on this PC."
            .to_string(),
        String::new(),
        "Usage: limo-cable-spike <step> [--via winusb|libusb|nusb|adb] [--key own|adb] [--apk PATH] [--adb PATH] [--verbose]".to_string(),
        String::new(),
        "  plan                 the whole run, for reading first".to_string(),
    ];
    for step in StepName::every() {
        let route = if step.needs_a_route() { " --via R" } else { "" };
        let short = match step {
            StepName::Drivers => "which driver Windows bound to the phone's debugging interface (reads only)",
            StepName::List => "the phones each route sees (reads only); --via adb asks Google's adb",
            StepName::Connect => "the \"Allow USB debugging?\" prompt, then three properties read",
            StepName::Install => "installs the test app (--apk PATH, else the release build in phone\\)",
            StepName::WriteSecret => "a sample secret into the test app's guarded door, from standard input",
            StepName::Start => "opens the test app's screen",
            StepName::Reverse => "the phone's 127.0.0.1:47810 carried to an echo server on this PC",
            StepName::Uninstall => "removes the test app",
            StepName::DebuggingOff => "LAST: switches USB debugging off; the cable link ends there",
        };
        out.push(format!("  {:<20} {short}", format!("{}{route}", step.word())));
    }
    out.push("  help                 this".to_string());
    out.push(String::new());
    out.push(
        "Routes: winusb is this program's own adb over Windows' own WinUSB, the phone found as Google's adb finds \
         it; libusb and nusb are adb_client 4.0.0 inside this program; none of these runs adb.exe. adb is Google's \
         adb.exe, found on the PATH, in ANDROID_HOME, or given with --adb. --verbose prints what adb_client and nusb log, \
         with any serial number cut to its last four characters."
            .to_string(),
    );
    out.push(
        "--key adb, with --via winusb only, connects with the adb key of this Windows account (Google's adb's own, \
         read only, never copied or shown) instead of this program's own: a phone that allowed it before asks \
         nothing. For this proof only; the real product makes its own key, and the guardian allows it once."
            .to_string(),
    );
    out.push("Exit codes: 0 done, 1 failed, 2 refused (nothing was changed on the phone).".to_string());
    out.join("\n")
}
