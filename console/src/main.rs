//! `limo-console.exe`: LiMo's guardian console on the master PC.
//!
//! One window, drawn by WebView2, showing the guardian screens from
//! `phone/app` (built into `phone/app/dist/console` and embedded in this
//! exe). The page reaches Rust through one command, `request`, and Rust
//! passes each request through the door to LiMo's service, after checking
//! that the door is the service's. Nothing is kept here.
//!
//! With `--self-check <log>` it plays the service's part itself with two
//! stand in doors in this account, lets the page drive its own screen, and
//! writes what happened (README.md, "The self check").

// No console window for the release build.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

mod self_check;

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use limo_console::door::{Door, DoorProblem, ServerMustBe};
use limo_console::protocol::{Request, Secret};
use limo_console::stand_in::{DOOR_SDDL, StandIn, StandInServer};
use limo_console::webview::{CONSOLE, Switches};
use limo_console::{this_process, window};
use serde_json::{Value, json};
use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Manager, RunEvent, State, WebviewUrl, WebviewWindowBuilder};

use self_check::SelfCheck;

/// The window's size: the console's drawings are laid out for about 1100
/// by 760 (design rule 6). The self check uses a smaller one.
const SIZE: (f64, f64) = (1100.0, 760.0);
const SELF_CHECK_SIZE: (f64, f64) = (720.0, 640.0);

struct Options {
    self_check: Option<PathBuf>,
    hold: Duration,
    switches: Switches,
    proxy: Option<u16>,
    data: Option<PathBuf>,
    keys_on: bool,
    page_guard: bool,
    probe: bool,
    size: Option<(f64, f64)>,
}

const USAGE: &str = "limo-console takes no arguments. For the self check (README.md):\n  \
limo-console --self-check <log> [--hold-ms N] [--switches default|no-background-networking|no-component-update|both]\n  \
[--proxy PORT] [--webview-data FOLDER] [--probe-keys] [--keys-on] [--no-page-guard] [--size WxH]";

fn parse_args() -> Result<Options, String> {
    let mut o = Options {
        self_check: None,
        hold: Duration::ZERO,
        switches: CONSOLE,
        proxy: None,
        data: None,
        keys_on: false,
        page_guard: true,
        probe: false,
        size: None,
    };
    let mut args = std::env::args().skip(1);
    let mut checking_only = Vec::new();
    while let Some(arg) = args.next() {
        let mut value = |what: &str| args.next().ok_or(format!("{arg} needs {what}"));
        match arg.as_str() {
            "--self-check" => o.self_check = Some(value("a log file")?.into()),
            "--hold-ms" => {
                let ms: u64 = value("a number")?.parse().map_err(|_| "--hold-ms needs a number")?;
                o.hold = Duration::from_millis(ms.min(55_000));
                checking_only.push("--hold-ms");
            }
            "--switches" => {
                let name = value("a name")?;
                o.switches = Switches::from_name(&name).ok_or(format!("no switches named {name}"))?;
                checking_only.push("--switches");
            }
            "--proxy" => {
                o.proxy = Some(value("a port")?.parse().map_err(|_| "--proxy needs a port")?);
                checking_only.push("--proxy");
            }
            "--webview-data" => {
                o.data = Some(value("a folder")?.into());
                checking_only.push("--webview-data");
            }
            "--probe-keys" => {
                o.probe = true;
                checking_only.push("--probe-keys");
            }
            "--keys-on" => {
                o.keys_on = true;
                checking_only.push("--keys-on");
            }
            "--size" => {
                let text = value("a size such as 1100x760")?;
                let (w, h) = text.split_once('x').ok_or("--size needs a size such as 1100x760")?;
                let (w, h): (f64, f64) = (w.parse().map_err(|_| "--size needs numbers")?, h.parse().map_err(|_| "--size needs numbers")?);
                o.size = Some((w.clamp(360.0, 1600.0), h.clamp(560.0, 1000.0)));
                checking_only.push("--size");
            }
            "--no-page-guard" => {
                o.page_guard = false;
                checking_only.push("--no-page-guard");
            }
            _ => return Err(USAGE.to_string()),
        }
    }
    if o.self_check.is_none() && !checking_only.is_empty() {
        return Err(format!("{} only work with --self-check", checking_only.join(", ")));
    }
    Ok(o)
}

/// The self check's doors: two stand ins in this account, and the names
/// the page may switch between.
struct Doors {
    password: String,
    guardian: StandInServer,
    _child: StandInServer,
    must_mine: ServerMustBe,
    child_name: String,
}

struct Console {
    door: Mutex<Door>,
    check: Option<Arc<SelfCheck>>,
    doors: Option<Doors>,
    parent_window: AtomicIsize,
    probe_running: AtomicBool,
    control: bool,
}

fn problem(code: &str, words: &str) -> Value {
    json!({ "code": code, "words": words })
}

/// The page's one command: a guardian's request to the door, or in self
/// check mode the self check's own requests.
#[tauri::command(async)]
fn request(app: AppHandle, console: State<'_, Console>, name: String, args: Value) -> Result<Value, Value> {
    if name == "self_check" {
        return self_check_request(&app, &console, &args);
    }
    let asked = Request::from_page(&name, &args).map_err(|why| problem("unknown_request", &why))?;
    let started = Instant::now();
    let answer = {
        let mut door = console.door.lock().map_err(|_| problem("broken", DoorProblem::Broken(String::new()).words()))?;
        door.ask(&asked)
    };
    let took = started.elapsed().as_millis();
    if let Some(check) = &console.check {
        let said = match &answer {
            Ok(a) => serde_json::to_value(a).ok().and_then(|v| v.get("answer").cloned()).unwrap_or_default().to_string(),
            Err(p) => format!("refused before anything was sent: {p}"),
        };
        check.log("door", &format!("{} answered {said} in {took} ms", asked.name()));
    }
    drop(asked);
    match answer {
        Ok(a) => serde_json::to_value(a).map_err(|_| problem("broken", "The console could not read the answer.")),
        Err(p) => Err(problem(p.code(), p.words())),
    }
}

fn self_check_request(app: &AppHandle, console: &Console, args: &Value) -> Result<Value, Value> {
    let (Some(check), Some(doors)) = (&console.check, &console.doors) else {
        return Err(problem("unknown_request", "the console has no request named self_check"));
    };
    match args.get("action").and_then(Value::as_str) {
        Some("state") => Ok(check.state(&doors.password)),
        Some("report") => {
            let key = args.get("key").and_then(Value::as_str).unwrap_or("");
            let text = args.get("text").and_then(Value::as_str).unwrap_or("");
            let starts_probe = check.report(app, key, text).map_err(|e| problem("self_check", &e))?;
            if key == "not_the_service" {
                let served = doors.guardian.served();
                check.log(
                    "stand_in",
                    &format!(
                        "the guardian's stand in served {} connections in all, the last with requests {:?}; the impostor door's connection wrote nothing, so it is not among them",
                        served.len(),
                        served.last().map(|s| s.requests.clone()).unwrap_or_default()
                    ),
                );
            }
            if starts_probe && !console.probe_running.swap(true, Ordering::SeqCst) {
                let parent = console.parent_window.load(Ordering::SeqCst);
                let (app2, check2, control) = (app.clone(), Arc::clone(check), console.control);
                std::thread::spawn(move || {
                    run_key_probe(&check2, parent, control);
                    check2.probe_finished(&app2);
                });
            }
            Ok(Value::Null)
        }
        Some("use") => {
            let which = args.get("door").and_then(Value::as_str).unwrap_or("");
            let door = match which {
                "child" => Door::new(&doors.child_name, doors.must_mine.clone()),
                "impostor" => {
                    let folder = this_process::program_files().unwrap_or_default();
                    Door::new(doors.guardian.name(), ServerMustBe::limo_service(&folder))
                }
                _ => return Err(problem("self_check", "no such door")),
            };
            check.log("use_door", which);
            if let Ok(mut d) = console.door.lock() {
                *d = door;
            }
            Ok(Value::Null)
        }
        _ => Err(problem("self_check", "no such self check action")),
    }
}

/// Sends Ctrl+P, Ctrl+F, a right click and F5 to WebView2's own window,
/// one at a time, and writes down what showed and what reloaded after each:
/// the top level windows of the console's processes, and the windows inside
/// the console's own. A control run (keys left on) leaves out the right
/// click, whose menu would float above every window for a moment.
fn run_key_probe(check: &SelfCheck, parent: isize, control: bool) {
    let tree: Vec<u32> = limo_console::measure::process_tree(std::process::id()).into_iter().map(|(p, _)| p).collect();
    let Some(input) = window::input_window(parent) else {
        check.log("keys", "WebView2's own window was not found; nothing was sent");
        return;
    };
    check.log("keys", &format!("sending to WebView2's window of class {} in process {}", input.class, input.pid));
    let shown = |label: &str| {
        let mut w: Vec<String> = window::visible_windows_of(&tree).into_iter().map(|w| format!("{}:{}", w.class, w.handle)).collect();
        w.extend(window::children(parent).into_iter().map(|w| format!("inside {}:{}", w.class, w.handle)));
        w.sort();
        check.log("keys", &format!("{label}: windows showing [{}], page loads {}", w.join(" "), check.page_loads()));
        w
    };
    let before = shown("before");
    for (probe, label, wait) in [
        (window::Probe::PlainX, "a plain X", 400),
        (window::Probe::CtrlP, "Ctrl+P", 1200),
        (window::Probe::CtrlF, "Ctrl+F", 1000),
        (window::Probe::RightClick, "right click", 1000),
        (window::Probe::F5, "F5", 1500),
    ] {
        if control && probe == window::Probe::RightClick {
            check.log("keys", "right click: not sent in a control run");
            continue;
        }
        let loads = check.page_loads();
        window::send(input.handle, probe);
        std::thread::sleep(Duration::from_millis(wait));
        let now = shown(&format!("after {label}"));
        let new: Vec<&String> = now.iter().filter(|w| !before.contains(w)).collect();
        check.log(
            "keys",
            &format!(
                "{label}: new windows {}, page reloaded {}",
                if new.is_empty() { "none".to_string() } else { format!("{new:?}") },
                if check.page_loads() > loads { "YES" } else { "no" }
            ),
        );
    }
}

/// A password for the stand in, new each run, from Windows' own random
/// seed for hash maps.
fn throwaway_password() -> String {
    let mut h = RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    let a = h.finish();
    let mut h = RandomState::new().build_hasher();
    h.write_u64(a);
    format!("{a:016x}{:016x}", h.finish())
}

fn main() {
    let start = Instant::now();
    let options = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };

    let check = match &options.self_check {
        None => None,
        Some(path) => match SelfCheck::create(path, start, options.hold, options.keys_on, options.probe) {
            Ok(c) => Some(Arc::new(c)),
            Err(e) => {
                eprintln!("cannot write the self check log: {e}");
                std::process::exit(2);
            }
        },
    };

    // The door: LiMo's own, or in self check mode the stand ins.
    let (door, doors) = match &check {
        None => match Door::limo() {
            Some(d) => (d, None),
            None => {
                eprintln!("Windows would not say where its Program Files folder is.");
                std::process::exit(1);
            }
        },
        Some(check) => {
            let Some(must_mine) = this_process::as_server() else {
                check.log("error", "this process's own account could not be read");
                std::process::exit(1);
            };
            let me = this_process::account().unwrap_or_default();
            let password = throwaway_password();
            let base = format!(r"\\.\pipe\limo.console.self-check.{}", std::process::id());
            let guardian = StandInServer::start(&format!("{base}.guardian"), DOOR_SDDL, StandIn::new(Secret::new(password.clone())));
            let child = StandInServer::start(
                &format!("{base}.child"),
                DOOR_SDDL,
                StandIn::new(Secret::new(password.clone())).with_child_account(&me, "Asha"),
            );
            let (Ok(guardian), Ok(child)) = (guardian, child) else {
                check.log("error", "the stand in doors could not be made");
                std::process::exit(1);
            };
            check.log(
                "start",
                &format!(
                    "self check, limo-console {}, webview2 runtime {}, switches {}, proxy {}, keys {}, page guard {}, probe {}, arguments {}",
                    env!("CARGO_PKG_VERSION"),
                    tauri::webview_version().unwrap_or_else(|e| format!("not found ({e})")),
                    options.switches.name(),
                    options.proxy.map_or("none".to_string(), |p| format!("127.0.0.1:{p}")),
                    if options.keys_on { "left on (control)" } else { "switched off" },
                    if options.page_guard { "on" } else { "off" },
                    if options.probe { "yes" } else { "no" },
                    options.switches.arguments(options.proxy),
                ),
            );
            check.log("stand_in", &format!("door sddl {}", guardian.sddl().unwrap_or("(unread)")));
            let door = Door::new(guardian.name(), must_mine.clone());
            let child_name = child.name().to_string();
            (door, Some(Doors { password, guardian, _child: child, must_mine, child_name }))
        }
    };

    let console = Console {
        door: Mutex::new(door),
        check: check.clone(),
        doors,
        parent_window: AtomicIsize::new(0),
        probe_running: AtomicBool::new(false),
        control: options.keys_on,
    };

    let for_setup = check.clone();
    let switches = options.switches;
    let proxy = options.proxy;
    let data = options.data.clone();
    let keys_on = options.keys_on;
    let page_guard = options.page_guard;
    let size = options.size;
    let app = tauri::Builder::default()
        .manage(console)
        .invoke_handler(tauri::generate_handler![request])
        .on_page_load(|webview, payload| {
            if let (Some(check), PageLoadEvent::Finished) = (&webview.state::<Console>().check, payload.event()) {
                check.page_loaded(payload.url().as_str());
            }
        })
        .setup(move |app| {
            let (w, h) = if for_setup.is_some() { size.unwrap_or(SELF_CHECK_SIZE) } else { SIZE };
            let mut builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("LiMo settings")
                .inner_size(w, h)
                .min_inner_size(360.0, 560.0)
                .resizable(true)
                .maximized(false)
                .fullscreen(false)
                .always_on_top(false)
                .focused(for_setup.is_none())
                .additional_browser_args(&switches.arguments(proxy))
                // The console's own pages only: any other address is refused.
                .on_navigation(|url| url.scheme() == "http" && url.host_str() == Some("tauri.localhost"));
            if let Some(check) = &for_setup {
                // The proof's WebView2 data stays out of the user's AppData:
                // the folder it is given, or one beside the exe.
                let folder = match &data {
                    Some(d) => d.clone(),
                    None => std::env::current_exe()?.parent().map(|p| p.join("webview-data")).ok_or("the exe has no folder")?,
                };
                builder = builder.data_directory(folder);
                let mut marks = "Object.defineProperty(window, '__LIMO_SELF_CHECK__', { value: true });".to_string();
                if !page_guard {
                    marks.push_str("Object.defineProperty(window, '__LIMO_SELF_CHECK_NO_PAGE_GUARD__', { value: true });");
                }
                builder = builder.initialization_script(&marks);
                check.start_deadline(app.handle().clone());
            }
            let window = builder.build()?;
            if let Some(check) = &for_setup {
                check.log("window_created", &format!("{w} by {h}, not focused, not on top"));
            }
            let handle = app.handle().clone();
            window.with_webview(move |webview| {
                let controller = webview.controller();
                let locked = window::lock_down(&controller, keys_on);
                let console = handle.state::<Console>();
                if let Some(parent) = window::parent_window(&controller) {
                    console.parent_window.store(parent, Ordering::SeqCst);
                }
                if let Some(check) = &console.check {
                    let seen = window::read_settings(&controller);
                    check.log(
                        "webview2_settings",
                        &format!(
                            "lock down {}; read back: accelerator keys {:?}, context menus {:?}, devtools {:?}, zoom {:?}, status bar {:?}",
                            match locked {
                                Ok(()) if keys_on => "skipped (control)".to_string(),
                                Ok(()) => "done".to_string(),
                                Err(e) => format!("failed: {e}"),
                            },
                            seen.accelerator_keys,
                            seen.context_menus,
                            seen.devtools,
                            seen.zoom,
                            seen.status_bar
                        ),
                    );
                }
            })?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building the console");

    let closed_early = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&closed_early);
    let code = app.run_return(move |_app, event| {
        if let (RunEvent::Exit, Some(check)) = (&event, &check) {
            flag.store(!check.finish_at_exit(), Ordering::SeqCst);
            check.log("event_loop_exit", "");
        }
    });
    std::process::exit(if closed_early.load(Ordering::SeqCst) { 1 } else { code });
}
