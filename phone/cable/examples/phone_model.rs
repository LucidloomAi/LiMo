//! Connects to the one phone on this PC's USB ports, the way LiMo will, and
//! prints what it is. For a person at the PC with the test phone:
//!
//! ```text
//! cargo run --example phone_model
//! cargo run --example phone_model -- --key adb
//! ```
//!
//! With no option it connects with LiMo's own key for this Windows account
//! (made the first time, in `%LOCALAPPDATA%\LiMo\adbkey`). It connects quietly
//! first, showing the phone nothing; a phone that does not know the key is
//! asked "Allow USB debugging?" only once the person at the PC presses Enter,
//! with the phone unlocked, and only once. With `--key adb` it connects with
//! the adb key of this Windows account instead, Google's adb's own, which a
//! phone may have allowed long ago, so that it can be run with nobody at the
//! phone. That key is only read: never written, copied or shown, and its
//! place is never said. It is for trying only: LiMo itself uses its own key,
//! and the guardian allows it once.
//!
//! It changes nothing on the phone: it reads the maker, the model, the
//! Android version and whether USB debugging is on, and whether LiMo is
//! installed, then lets go.

#![forbid(unsafe_code)]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let adb_key = match args.as_slice() {
        [] => false,
        [flag, value] if flag == "--key" && value == "adb" => true,
        _ => {
            println!("Usage: phone_model [--key adb]");
            std::process::exit(2);
        }
    };
    std::process::exit(run(adb_key));
}

#[cfg(windows)]
fn run(adb_key: bool) -> i32 {
    use limo_cable::flow::{State, seen, tell};
    use limo_cable::key::AdbKey;
    use limo_cable::open::{Cable, WindowsCable};
    use limo_cable::phone::{Knock, OpenProblem, Waits};

    println!("What this does to the phone: it opens the phone's debugging interface through Windows' own WinUSB and");
    println!("connects; then it reads the maker, the model, the Android version, whether USB debugging is on, and");
    println!("whether LiMo is installed. It changes nothing on the phone, installs nothing, and lets go at the end.");
    println!();
    let key = if adb_key {
        println!("The key: the adb key of this Windows account (Google's adb's own), read only. For trying only.");
        match google_adb_key() {
            Ok(key) => key,
            Err(why) => {
                println!("{why}");
                return 2;
            }
        }
    } else {
        println!("The key: LiMo's own key for this Windows account (%LOCALAPPDATA%\\LiMo\\adbkey).");
        match AdbKey::for_this_account() {
            Ok(key) => key,
            Err(why) => {
                println!("{why}");
                return 2;
            }
        }
    };
    println!("Its fingerprint, as the phone shows it: {}", key.fingerprint());
    println!();
    let mut cable = WindowsCable;
    let look = cable.look();
    let state = seen(&look);
    let words = tell(&state);
    println!("{}", words.line);
    if !matches!(state, State::Ready { .. }) {
        if let Some(advice) = words.advice {
            println!("{advice}");
        }
        return 1;
    }
    println!("> open the phone through WinUSB, and connect quietly (nothing is shown to the phone)");
    let mut asked = false;
    let waits = Waits { prompt: std::time::Duration::from_secs(600), ..Waits::default() };
    let opened = match cable.open(&key, waits, Knock::Quiet) {
        // The phone does not know this PC: it is asked only when the person
        // says so, with the phone unlocked, and once.
        Err(OpenProblem::NotKnown) => {
            println!("The phone does not know this PC yet. Unlock it, then press Enter here to ask it to allow this PC (it asks once).");
            // Only a line the person ended with Enter asks: an end of input
            // (a closed or empty input) is not a yes.
            let mut line = String::new();
            match std::io::stdin().read_line(&mut line) {
                Ok(_) if line.ends_with('\n') => {}
                _ => {
                    println!("Nothing was asked of the phone: the input ended before Enter was pressed.");
                    return 1;
                }
            }
            cable.open(
                &key,
                waits,
                Knock::asking(&mut || {
                    asked = true;
                    println!(
                        "The phone asks: Allow USB debugging? Tick \"Always allow from this computer\", then tap Allow (within {} minutes).",
                        waits.prompt.as_secs() / 60
                    );
                }),
            )
        }
        other => other,
    };
    let phone = match opened {
        Ok(phone) => phone,
        Err(problem) => {
            println!("Not connected: {problem}.");
            return 1;
        }
    };
    if !asked {
        let skipped = if adb_key {
            "The question on the phone was skipped: the adb key of this Windows account was allowed on this phone before."
        } else {
            "The phone asked nothing: it allowed LiMo's key before."
        };
        println!("{skipped}");
    }
    let facts = phone.facts();
    println!(
        "Connected: {} {}, Android {} (API {}); USB debugging: {}.",
        facts.manufacturer,
        facts.model,
        facts.android,
        facts.sdk.map(|s| s.to_string()).unwrap_or_else(|| "not read".to_string()),
        match facts.usb_debugging {
            Some(true) => "on",
            Some(false) => "off",
            None => "not read",
        }
    );
    match phone.limo() {
        Ok(None) => println!("LiMo is not on the phone."),
        Ok(Some(limo)) => println!("LiMo is on the phone, version {}.", limo.version_name.unwrap_or_else(|| "not read".to_string())),
        Err(why) => println!("Whether LiMo is on the phone could not be read: {why}."),
    }
    phone.close();
    println!("Let go of the phone.");
    0
}

/// The adb key of this Windows account, read only: in the folder
/// `ANDROID_USER_HOME` names, else in `.android` in the user's folder.
#[cfg(windows)]
fn google_adb_key() -> Result<limo_cable::key::AdbKey, String> {
    let folder = std::env::var_os("ANDROID_USER_HOME")
        .filter(|f| !f.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(|h| std::path::PathBuf::from(h).join(".android")))
        .ok_or_else(|| "This Windows account has no folder for an adb key.".to_string())?;
    let text = zeroize::Zeroizing::new(
        std::fs::read_to_string(folder.join("adbkey"))
            .map_err(|_| "This Windows account has no adb key that can be read (Google's adb makes one the first time it runs).".to_string())?,
    );
    limo_cable::key::AdbKey::from_pem(&text).ok_or_else(|| "The adb key of this Windows account cannot be read as a key.".to_string())
}

#[cfg(not(windows))]
fn run(_adb_key: bool) -> i32 {
    println!("This example reaches a phone through Windows' own WinUSB, on Windows only.");
    2
}
