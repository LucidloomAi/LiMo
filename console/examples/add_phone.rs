//! Adds a phone from a terminal, with the console's own desk for a phone:
//! the real cable (LiMo's own way through Windows' WinUSB, counting what it
//! sends), LiMo's key in this account (shown to the phone only when the
//! person presses Enter, with the phone unlocked, and once), and LiMo's
//! service through its real
//! door, checked as the service. The same requests the Add a phone screens
//! ask, one a second, with each change printed. For a first run on a real
//! phone before the screens exist (README.md, "A first run on a real
//! phone"). It reaches a real phone: never run by a test.
//!
//! ```text
//! cargo run --release --no-default-features --example add_phone -- --password-file <file> --app <limo-child.apk> [--child <id>] [--leave-debugging-on]
//! ```
//!
//! It signs in with the guardian password in the file (then wipes it from
//! memory; delete the file afterwards), picks the child (the only one, or
//! `--child`), waits for the phone, puts LiMo on it unless it holds this
//! version, pairs it, waits up to two minutes for it to reach the PC over
//! the home Wi-Fi, switches USB debugging off unless told to leave it on,
//! lets go and signs out. Stops at the first step that fails, with the
//! words the screen would show.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use limo_console::door::{Door, DoorReply};
use limo_console::phone::{ConsoleCable, CountedCable, DoorService, LINK_PORT, Pace, PhoneDesk, PhoneSetup, Service};
use limo_console::protocol::PageRequest;
use serde_json::{Value, json};
use zeroize::Zeroizing;

struct Options {
    password_file: PathBuf,
    app: PathBuf,
    child: Option<String>,
    leave_on: bool,
}

fn options() -> Result<Options, String> {
    let usage = "add_phone --password-file <file> --app <limo-child.apk> [--child <id>] [--leave-debugging-on]";
    let (mut password_file, mut app, mut child, mut leave_on) = (None, None, None, false);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--password-file" => password_file = args.next().map(PathBuf::from),
            "--app" => app = args.next().map(PathBuf::from),
            "--child" => child = args.next(),
            "--leave-debugging-on" => leave_on = true,
            _ => return Err(usage.to_string()),
        }
    }
    Ok(Options { password_file: password_file.ok_or(usage)?, app: app.ok_or(usage)?, child, leave_on })
}

/// Prints each part of the answer that changed since the last.
fn show(look: &Value, last: &mut BTreeMap<String, Value>) {
    let started = START.get_or_init(Instant::now);
    for part in ["cable", "fingerprint", "phone", "checks", "install", "pairing", "debugging", "busy"] {
        if last.get(part) != Some(&look[part]) {
            println!("{:>6.1} s  {part}: {}", started.elapsed().as_secs_f64(), look[part]);
            last.insert(part.to_string(), look[part].clone());
        }
    }
}

static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

fn main() {
    let o = match options() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let Ok(password) = std::fs::read_to_string(&o.password_file).map(|p| Zeroizing::new(p.trim_end().to_string())) else {
        eprintln!("the password file could not be read");
        std::process::exit(2);
    };
    let Some(door) = Door::limo() else {
        eprintln!("Windows would not say where its Program Files folder is.");
        std::process::exit(1);
    };
    let door = Arc::new(Mutex::new(door));
    let service = Arc::new(DoorService::new(Arc::clone(&door)));

    // Signing in, as the sign in screen does.
    let signed = door.lock().unwrap().ask(PageRequest::from_page("sign_in", json!({ "password": password.as_str() })).unwrap());
    drop(password);
    match signed {
        Ok(DoorReply::Reply(r)) if r["outcome"] == "done" => println!("signed in"),
        Ok(DoorReply::Reply(r)) => fail(&format!("not signed in: {}", r["problem"]["message"])),
        Ok(DoorReply::Refused(message)) => fail(&message),
        Err(problem) => fail(problem.words()),
    }

    // The child.
    let children = service.ask("family_today", Value::Null).unwrap_or_else(|said| fail(&said.words));
    let listed: Vec<(String, String)> = children
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| (c["child_id"].as_str().unwrap_or_default().to_string(), c["name"].as_str().unwrap_or_default().to_string()))
        .collect();
    let child = match (&o.child, listed.as_slice()) {
        (Some(id), _) => id.clone(),
        (None, [(id, name)]) => {
            println!("the child: {name}");
            id.clone()
        }
        (None, _) => {
            for (id, name) in &listed {
                println!("  {name}: --child {id}");
            }
            fail("say which child with --child");
        }
    };

    let desk = PhoneDesk::new(PhoneSetup {
        cable: Arc::new(|| Box::new(CountedCable::default()) as Box<dyn ConsoleCable>),
        service: Arc::clone(&service) as Arc<dyn Service>,
        app: Some(o.app.clone()),
        key_folder: None,
        link_port: LINK_PORT,
        waits: limo_cable::phone::Waits { prompt: limo_console::phone::ASK_WAIT, ..limo_cable::phone::Waits::default() },
        pace: Pace::default(),
        log: None,
    });
    let ask = |name: &str, args: Value| {
        let answer = desk.request(name, args);
        if !name.ends_with("look") {
            println!("         {name}: {answer:?}");
        }
        answer
    };

    let deadline = Instant::now() + Duration::from_secs(15 * 60);
    let mut last = BTreeMap::new();
    let mut wifi_from: Option<Instant> = None;
    let mut asked_off = false;
    // The phone is asked only once the person says so, unlocked, and once.
    let (mut ask_now, mut asked) = (false, false);
    let outcome = loop {
        let look = ask("phone.look", Value::Null).unwrap_or_else(|e| fail(&e.to_string()));
        show(&look, &mut last);
        if Instant::now() > deadline {
            break "fifteen minutes passed".to_string();
        }
        let connected = look["cable"]["case"] == "ok" && look["held"] == true && look["busy"].is_null();
        let install = look["install"]["stage"].as_str().unwrap_or_default();
        let pairing = look["pairing"]["stage"].as_str().unwrap_or_default();
        let debugging = look["debugging"]["stage"].as_str().unwrap_or_default();
        if ["refused", "failed", "no_app"].contains(&install) {
            break format!("install: {} {}", look["install"]["line"], look["install"]["advice"]);
        }
        if pairing == "failed" {
            break format!("pairing: {} {}", look["pairing"]["line"], look["pairing"]["advice"]);
        }
        if debugging == "off" {
            break "done: USB debugging is off; unplug the phone".to_string();
        }
        if debugging == "still_on" {
            break format!("{} {}", look["debugging"]["line"], look["debugging"]["advice"]);
        }
        if look["cable"]["case"] == "unknown" && !asked && !ask_now {
            println!("         the phone does not know this PC: unlock it, then press Enter here to ask it to allow this PC (it asks once)");
            // Only a line the person ended with Enter asks: an end of input
            // (a closed or empty input) is not a yes.
            let mut line = String::new();
            match std::io::stdin().read_line(&mut line) {
                Ok(_) if line.ends_with('\n') => ask_now = true,
                _ => break "nothing was asked of the phone: the input ended before Enter was pressed".to_string(),
            }
        } else if look["cable"]["case"] == "unknown" && ask_now {
            ask_now = false;
            asked = true;
            let _ = ask("phone.ask", Value::Null);
        } else if look["cable"]["case"] == "trouble" && asked && look["held"] == false && look["phone"].is_null() {
            break format!("{} {}", look["cable"]["line"], look["cable"]["more"]);
        } else if connected && look["phone"]["limo"]["looked"] == true && install == "not_asked" {
            let _ = ask("phone.install", Value::Null);
        } else if connected && ["done", "not_needed"].contains(&install) && pairing == "not_asked" {
            let _ = ask("phone.pair", json!({ "child": child }));
        } else if pairing == "paired" {
            let reached = look["pairing"]["ways"].as_array().is_some_and(|w| w.iter().any(|w| w == "home_network"));
            let from = *wifi_from.get_or_insert_with(Instant::now);
            if reached || from.elapsed() > Duration::from_secs(120) {
                if !reached {
                    println!("         the phone did not reach this PC over the home Wi-Fi in two minutes");
                }
                if o.leave_on {
                    break "done: paired; USB debugging left on".to_string();
                }
                if !asked_off && connected {
                    asked_off = true;
                    let _ = ask("phone.debugging_off", Value::Null);
                }
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    };
    println!("{outcome}");
    let _ = ask("phone.done", Value::Null);
    let _ = service.ask("sign_out", Value::Null);
}

fn fail(words: &str) -> ! {
    eprintln!("{words}");
    std::process::exit(1);
}
