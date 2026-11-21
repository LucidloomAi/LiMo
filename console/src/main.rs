//! `limo-console.exe`: LiMo's guardian console on the master PC.
//!
//! One window, drawn by WebView2, showing the guardian screens from
//! `phone/app` (built into `phone/app/dist/console` and embedded in this
//! exe). The page reaches Rust through one command, `request`, and Rust
//! passes each request through the door to LiMo's service, after checking
//! that the door is the service's. Nothing is kept here. The requests for
//! adding a phone (`phone.*`) are the console's own: it answers them itself,
//! with the cable (`limo_console::phone`).
//!
//! With `--self-check <log>` the window asks LiMo for Windows' own door
//! served in the builder's account by `examples/real_door.rs`, lets the page
//! drive its own screen, and writes what happened (README.md, "The self
//! check").

// No console window for the release build.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

mod self_check;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use limo_console::door::{Door, DoorProblem, DoorReply, ServerMustBe};
use limo_console::phone::{self, ConsoleCable, CountedCable, DoorService, LINK_PORT, Pace, PhoneDesk, PhoneSetup};
use limo_console::protocol::{PageRequest, wipe};
use limo_console::webview::{CONSOLE, Switches};
use limo_console::{this_process, window};
use serde_json::{Value, json};
use tauri::webview::{NewWindowResponse, PageLoadEvent};
use tauri::{AppHandle, Manager, RunEvent, State, WebviewUrl, WebviewWindowBuilder};
use zeroize::Zeroizing;

use self_check::SelfCheck;

/// The window's size and title: the console's drawings are laid out for
/// about 1100 by 760 (design rule 6). The self check uses a smaller one.
const SIZE: (f64, f64) = (1100.0, 760.0);
const SELF_CHECK_SIZE: (f64, f64) = (720.0, 640.0);
const TITLE: &str = "LiMo settings";

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
    door_base: Option<String>,
    door_server: Option<String>,
    password_file: Option<PathBuf>,
}

const USAGE: &str = "limo-console takes no arguments. For the self check (README.md):\n  \
limo-console --self-check <log> --door-base <pipe> --door-server <program> --password-file <file>\n  \
[--hold-ms N] [--switches default|no-background-networking|no-component-update|both] [--proxy PORT]\n  \
[--webview-data FOLDER] [--probe-keys] [--keys-on] [--no-page-guard] [--size WxH]";

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
        door_base: None,
        door_server: None,
        password_file: None,
    };
    let mut args = std::env::args().skip(1);
    let mut checking_only = Vec::new();
    while let Some(arg) = args.next() {
        let mut value = |what: &str| args.next().ok_or(format!("{arg} needs {what}"));
        match arg.as_str() {
            "--self-check" => {
                o.self_check = Some(value("a log file")?.into());
                continue;
            }
            "--hold-ms" => {
                let ms: u64 = value("a number")?.parse().map_err(|_| "--hold-ms needs a number")?;
                o.hold = Duration::from_millis(ms.min(55_000));
            }
            "--switches" => {
                let name = value("a name")?;
                o.switches = Switches::from_name(&name).ok_or(format!("no switches named {name}"))?;
            }
            "--proxy" => o.proxy = Some(value("a port")?.parse().map_err(|_| "--proxy needs a port")?),
            "--webview-data" => o.data = Some(value("a folder")?.into()),
            "--probe-keys" => o.probe = true,
            "--keys-on" => o.keys_on = true,
            "--no-page-guard" => o.page_guard = false,
            "--size" => {
                let text = value("a size such as 1100x760")?;
                let (w, h) = text.split_once('x').ok_or("--size needs a size such as 1100x760")?;
                let (w, h): (f64, f64) = (w.parse().map_err(|_| "--size needs numbers")?, h.parse().map_err(|_| "--size needs numbers")?);
                o.size = Some((w.clamp(360.0, 1600.0), h.clamp(560.0, 1000.0)));
            }
            "--door-base" => {
                let base = value("a pipe name")?;
                if !base.starts_with(r"\\.\pipe\limo.console.") {
                    return Err(r"--door-base must be \\.\pipe\limo.console.<something>, never LiMo's own door".to_string());
                }
                o.door_base = Some(base);
            }
            "--door-server" => o.door_server = Some(value("a program file")?),
            "--password-file" => o.password_file = Some(value("a file")?.into()),
            _ => return Err(USAGE.to_string()),
        }
        checking_only.push(arg);
    }
    if o.self_check.is_none() && !checking_only.is_empty() {
        return Err(format!("{} only work with --self-check", checking_only.join(", ")));
    }
    if o.self_check.is_some() && (o.door_base.is_none() || o.door_server.is_none() || o.password_file.is_none()) {
        return Err("--self-check needs --door-base, --door-server and --password-file (tools\\run-self-check.ps1 gives them)".to_string());
    }
    Ok(o)
}

/// The self check's doors: LiMo for Windows' own, served by
/// `examples/real_door.rs` at `<base>.guardian` and `<base>.child`, and the
/// rule they are checked by (this session, this account, that program).
struct Doors {
    base: String,
    rule: ServerMustBe,
    password: Zeroizing<String>,
}

struct Console {
    /// The door, shared with the desk for a phone, which asks on the same
    /// signed in connection.
    door: Arc<Mutex<Door>>,
    phone: PhoneDesk,
    check: Option<Arc<SelfCheck>>,
    doors: Option<Doors>,
    parent_window: AtomicIsize,
    probe_running: AtomicBool,
    control: bool,
}

fn problem(code: &str, words: &str) -> Value {
    json!({ "code": code, "words": words })
}

/// True for arguments that say nothing: none, or an empty object.
fn says_nothing(args: &Value) -> bool {
    args.is_null() || args.as_object().is_some_and(|m| m.is_empty())
}

/// The page's one command. `more_info`, the requests for a phone and the
/// self check's own requests are the console's; every other name is the
/// door's.
#[tauri::command(async)]
fn request(app: AppHandle, console: State<'_, Console>, name: String, args: Value) -> Result<Value, Value> {
    match name.as_str() {
        "self_check" => self_check_request(&app, &console, &args),
        "more_info" => more_info(&console, args),
        name if phone::is_phone_request(name) => phone_request(&console, name, args),
        _ => ask_door(&console, &name, args),
    }
}

/// Adding a phone: answered by the console's desk for a phone, at once,
/// whatever the phone is doing. The self check never reaches a phone: the
/// PC it runs on may have one plugged in.
fn phone_request(console: &Console, name: &str, mut args: Value) -> Result<Value, Value> {
    if console.check.is_some() {
        wipe(&mut args);
        return Err(problem("self_check", "The self check never reaches a phone."));
    }
    console.phone.request(name, args)
}

/// Forwards a request through the door. What the page gets: the door's
/// reply as it came (`{"outcome":"done",...}` or `{"outcome":"problem",...}`),
/// `{"outcome":"refused","message":...}` when the door does not open for
/// this account, or, when the door could not be asked, an error
/// `{code, words}` with the sentence the screen shows.
fn ask_door(console: &Console, name: &str, args: Value) -> Result<Value, Value> {
    let asked = PageRequest::from_page(name, args).map_err(|why| problem("unknown_request", &why))?;
    let shown = asked.name().to_string();
    let started = Instant::now();
    let (answer, notes) = {
        let mut door = console.door.lock().map_err(|_| problem("lost", DoorProblem::Lost(String::new()).words()))?;
        let answer = door.ask(asked);
        (answer, door.take_notes())
    };
    if let Some(check) = &console.check {
        for note in notes {
            check.log("door", &note);
        }
        let said = match &answer {
            Ok(DoorReply::Reply(r)) if r["outcome"] == "done" => "done".to_string(),
            Ok(DoorReply::Reply(r)) => format!("problem {}", r["problem"]["kind"]),
            Ok(DoorReply::Refused(_)) => "refused".to_string(),
            Err(p) => format!("not asked: {p}"),
        };
        check.log("door", &format!("{shown}: {said} in {} ms", started.elapsed().as_millis()));
    }
    match answer {
        Ok(DoorReply::Reply(reply)) => Ok(reply),
        Ok(DoorReply::Refused(message)) => Ok(json!({ "outcome": "refused", "message": message })),
        Err(p) => Err(problem(p.code(), p.words())),
    }
}

/// "More info": LucidLoom AI's page in the PC's own browser. It takes
/// nothing from the page; the address is fixed. The self check writes down
/// what it would open and opens nothing.
fn more_info(console: &Console, mut args: Value) -> Result<Value, Value> {
    if !says_nothing(&args) {
        wipe(&mut args);
        return Err(problem("unknown_request", "more_info takes nothing"));
    }
    if let Some(check) = &console.check {
        check.log("more_info", &format!("would open {} in the PC's own browser; the self check opens nothing", window::MORE_INFO));
        return Ok(json!({ "opened": false, "address": window::MORE_INFO }));
    }
    Ok(json!({ "opened": window::open_more_info(), "address": window::MORE_INFO }))
}

fn self_check_request(app: &AppHandle, console: &Console, args: &Value) -> Result<Value, Value> {
    let (Some(check), Some(doors)) = (&console.check, &console.doors) else {
        return Err(problem("unknown_request", "that is not the name of a request"));
    };
    match args.get("action").and_then(Value::as_str) {
        Some("state") => Ok(check.state(&doors.password)),
        Some("report") => {
            let key = args.get("key").and_then(Value::as_str).unwrap_or("");
            let text = args.get("text").and_then(Value::as_str).unwrap_or("");
            let starts_probe = check.report(app, key, text).map_err(|e| problem("self_check", &e))?;
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
                "guardian" => Door::new(&format!("{}.guardian", doors.base), doors.rule.clone()),
                "child" => Door::new(&format!("{}.child", doors.base), doors.rule.clone()),
                // The guardian's door, checked by LiMo's own rule: session
                // 0, LocalSystem, Program Files\LiMo\limo.exe.
                "impostor" => {
                    let folder = this_process::program_files().unwrap_or_default();
                    Door::new(&format!("{}.guardian", doors.base), ServerMustBe::limo_service(&folder))
                }
                // A door nobody serves: the service not running.
                "absent" => Door::new(&format!("{}.absent", doors.base), doors.rule.clone()),
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

/// Sends a letter, Ctrl+P, Ctrl+F, a right click and F5 to WebView2's own
/// window, one at a time, and writes down what showed and what reloaded
/// after each: the top level windows of the console's processes, and the
/// windows inside the console's own. A control run (keys left on) leaves out
/// the right click, whose menu would float above every window for a moment.
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

    // The door: LiMo's own, or in self check mode the real door's code
    // served in this account by examples/real_door.rs.
    let (door, doors) = match (&check, &options.door_base, &options.door_server, &options.password_file) {
        (Some(check), Some(base), Some(server), Some(password_file)) => {
            let rule = ServerMustBe {
                session: this_process::session().unwrap_or(u32::MAX),
                owner: this_process::account().unwrap_or_default(),
                image: server.clone(),
            };
            let Ok(password) = std::fs::read_to_string(password_file).map(Zeroizing::new) else {
                check.log("error", "the password file could not be read");
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
            check.log("door", "LiMo for Windows' own door, served in this account by examples/real_door.rs");
            let door = Door::new(&format!("{base}.guardian"), rule.clone());
            (door, Some(Doors { base: base.clone(), rule, password }))
        }
        _ => match Door::limo() {
            Some(d) => (d, None),
            None => {
                eprintln!("Windows would not say where its Program Files folder is.");
                std::process::exit(1);
            }
        },
    };

    // The desk for a phone: the real cable, counting what it sends, made only
    // when the Add a phone screens first look; the door the page uses; LiMo's
    // app for phones beside this program.
    let door = Arc::new(Mutex::new(door));
    let phone = PhoneDesk::new(PhoneSetup {
        cable: Arc::new(|| Box::new(CountedCable::default()) as Box<dyn ConsoleCable>),
        service: Arc::new(DoorService::new(Arc::clone(&door))),
        app: std::env::current_exe().ok().and_then(|program| phone::app_beside(&program)),
        key_folder: None,
        link_port: LINK_PORT,
        waits: limo_cable::phone::Waits::default(),
        pace: Pace::default(),
    });
    let console = Console {
        door,
        phone,
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
                .title(TITLE)
                .inner_size(w, h)
                .min_inner_size(360.0, 560.0)
                .resizable(true)
                .maximized(false)
                .fullscreen(false)
                .always_on_top(false)
                .focused(for_setup.is_none())
                .additional_browser_args(&switches.arguments(proxy))
                // The console's own pages only: any other address is refused,
                // and no page may open a window of its own.
                .on_navigation(|url| url.scheme() == "http" && url.host_str() == Some("tauri.localhost"))
                .on_new_window(|_, _| NewWindowResponse::Deny);
            if let Some(check) = &for_setup {
                // The self check's WebView2 data stays out of the user's
                // AppData: the folder it is given, or one beside the exe.
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
                check.log("window_created", &format!("{w} by {h}, titled {TITLE:?}, not focused, not on top"));
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
