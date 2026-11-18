// No console window for the release build.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod family;
mod self_check;

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use family::{Family, FamilyView};
use self_check::{SelfCheck, SelfCheckSlot, SelfCheckState};
use tauri::{webview::PageLoadEvent, AppHandle, Manager, RunEvent, State, WebviewUrl, WebviewWindowBuilder};

// ---------------------------------------------------------------------------
// The console's commands. The page reaches them only through host.request in
// src/host.ts. Arguments arrive in camelCase (childId) and Tauri maps them to
// these snake_case names.

#[tauri::command]
fn family_view(family: State<'_, Mutex<Family>>) -> Result<FamilyView, String> {
    Ok(family.lock().map_err(|_| "family state is poisoned")?.view())
}

#[tauri::command]
fn give_minutes(family: State<'_, Mutex<Family>>, child_id: String, minutes: u32) -> Result<FamilyView, String> {
    family.lock().map_err(|_| "family state is poisoned")?.give_minutes(&child_id, minutes)
}

#[tauri::command]
fn lock_child(family: State<'_, Mutex<Family>>, child_id: String) -> Result<FamilyView, String> {
    family.lock().map_err(|_| "family state is poisoned")?.lock_child(&child_id)
}

// The two self check commands. Their permissions are granted only in self
// check mode (see `setup` below), and they refuse without a SelfCheck anyway.

#[tauri::command]
fn self_check_state(slot: State<'_, SelfCheckSlot>) -> Result<SelfCheckState, String> {
    slot.0.as_ref().map(|sc| sc.state()).ok_or_else(|| "not in self check mode".into())
}

#[tauri::command]
fn self_check_report(app: AppHandle, slot: State<'_, SelfCheckSlot>, key: String, text: String) -> Result<(), String> {
    let sc = slot.0.as_ref().ok_or("not in self check mode")?;
    sc.report(&app, &key, &text)
}

// ---------------------------------------------------------------------------

struct Options {
    self_check: Option<PathBuf>,
    hold: Duration,
}

fn parse_args() -> Result<Options, String> {
    let mut options = Options { self_check: None, hold: Duration::ZERO };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--self-check" => {
                options.self_check = Some(args.next().ok_or("--self-check needs a log file path")?.into());
            }
            "--hold-ms" => {
                let ms: u64 = args.next().and_then(|v| v.parse().ok()).ok_or("--hold-ms needs a number")?;
                options.hold = Duration::from_millis(ms.min(60_000));
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    if options.hold > Duration::ZERO && options.self_check.is_none() {
        return Err("--hold-ms only works with --self-check".into());
    }
    Ok(options)
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
    let self_check = match &options.self_check {
        None => None,
        Some(path) => match SelfCheck::create(path, start, options.hold) {
            Ok(sc) => Some(Arc::new(sc)),
            Err(e) => {
                eprintln!("cannot write the self check log: {e}");
                std::process::exit(2);
            }
        },
    };
    if let Some(sc) = &self_check {
        sc.log(
            "start",
            &format!(
                "self check mode, limo-console-proof {}, webview2 runtime {}",
                env!("CARGO_PKG_VERSION"),
                tauri::webview_version().unwrap_or_else(|e| format!("not found ({e})"))
            ),
        );
    }

    let for_setup = self_check.clone();
    let app = tauri::Builder::default()
        .manage(Mutex::new(Family::canned()))
        .manage(SelfCheckSlot(self_check.clone()))
        .invoke_handler(tauri::generate_handler![
            family_view,
            give_minutes,
            lock_child,
            self_check_state,
            self_check_report
        ])
        .on_page_load(|webview, payload| {
            if let Some(sc) = &webview.state::<SelfCheckSlot>().0 {
                let what = match payload.event() {
                    PageLoadEvent::Started => "page_load_started",
                    PageLoadEvent::Finished => "page_load_finished",
                };
                sc.log(what, payload.url().as_str());
            }
        })
        .setup(move |app| {
            // The WebView2 data folder sits next to the exe in this proof, so
            // nothing is written under the user's AppData. The product would
            // keep Tauri's default (%LOCALAPPDATA%\<identifier>).
            let data_dir = std::env::current_exe()?
                .parent()
                .map(|p| p.join("webview-data"))
                .ok_or("the exe has no folder")?;

            let mut window = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("LiMo console proof")
                .inner_size(420.0, 760.0)
                .resizable(true)
                .maximized(false)
                .fullscreen(false)
                .always_on_top(false)
                .focused(false)
                .data_directory(data_dir);

            if let Some(sc) = &for_setup {
                app.add_capability(include_str!("../runtime-capabilities/self-check.json"))?;
                window = window
                    .initialization_script("Object.defineProperty(window, '__LIMO_SELF_CHECK__', { value: true });");
                sc.start_deadline(app.handle().clone());
            }

            window.build()?;
            if let Some(sc) = &for_setup {
                sc.log("window_created", "420 by 760, not focused");
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building the console proof");

    let closed_early = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&closed_early);
    let code = app.run_return(move |_app, event| {
        if let (RunEvent::Exit, Some(sc)) = (&event, &self_check) {
            flag.store(!sc.finish_at_exit(), Ordering::SeqCst);
            sc.log("event_loop_exit", "");
        }
    });
    std::process::exit(if closed_early.load(Ordering::SeqCst) { 1 } else { code });
}
