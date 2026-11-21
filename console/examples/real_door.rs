//! Serves LiMo for Windows' own door for the console's self check, in the
//! builder's own account: two doors, each with a real family's file in a
//! temporary folder, at pipe names of the caller's choosing, never LiMo's
//! own. Nothing is installed, nothing runs as a service, nothing elevated.
//!
//!   real_door.exe <pipe base> <password file> [seconds]
//!
//! - `<pipe base>.guardian`: a family whose guardian password is new each
//!   run, written to `<password file>` for the self check to read (never on
//!   a command line); this account is not linked to any child;
//! - `<pipe base>.child`: the same, with this account linked to a child
//!   called Asha, so the door refuses it in its own words.
//!
//! It prints `ready` once both pipes are made, and stops after `seconds`
//! (60 at most, 50 if not given), or as soon as a file named like the
//! password file with `.stop` after it appears. The password file is deleted
//! when it stops.

#[cfg(windows)]
#[path = "../support/real_door.rs"]
mod real_door;

#[cfg(windows)]
fn main() {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    use std::time::{Duration, Instant};

    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(base), Some(password_file)) = (args.first(), args.get(1)) else {
        eprintln!("real_door <pipe base> <password file> [seconds]");
        std::process::exit(2);
    };
    if !base.starts_with(r"\\.\pipe\limo.console.") || base == r"\\.\pipe\limo.console" {
        eprintln!(r"the pipe base must be \\.\pipe\limo.console.<something>, never LiMo's own door");
        std::process::exit(2);
    }
    let seconds: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(50).min(60);
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    let password = format!("self check {:016x}", hasher.finish());
    if std::fs::write(password_file, &password).is_err() {
        eprintln!("the password file could not be written");
        std::process::exit(1);
    }
    let me = limo_windows::this_account().expect("this account");
    let guardian = real_door::RealDoorHost::start(&format!("{base}.guardian"), &password, None);
    let child = real_door::RealDoorHost::start(&format!("{base}.child"), &password, Some(&me));
    println!("ready");

    // Ends when the stop file appears, or after the time given.
    let stop = std::path::PathBuf::from(format!("{password_file}.stop"));
    let until = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < until && !stop.exists() {
        std::thread::sleep(Duration::from_millis(50));
    }
    let said: Vec<String> = guardian.log().into_iter().chain(child.log()).collect();
    drop((guardian, child));
    let _ = std::fs::remove_file(password_file);
    let _ = std::fs::remove_file(&stop);
    for line in said {
        println!("door: {line}");
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("the real door runs on Windows only");
}
