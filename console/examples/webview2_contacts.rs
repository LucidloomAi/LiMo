//! Measures what WebView2 contacts while the console is open, for each set
//! of switches (`limo_console::webview::Switches`), without elevation.
//!
//!   cargo run --example webview2_contacts --no-default-features -- [--hold-ms 50000] [--sets default,both] [--modes host-log,open]
//!
//! Run from the console folder, after tools\build-release.ps1. For each set
//! and each mode it starts `target\release\limo-console.exe --self-check`
//! with a new WebView2 data folder (a first start on a new PC), holds the
//! window open `--hold-ms` (at most 55 s, so the window is gone within a
//! minute), and meanwhile reads, every 50 ms, every TCP connection with a
//! remote end owned by the console or any process it started. Modes:
//!
//! - `host-log`: WebView2 reaches the network through the host log, a proxy
//!   on a loopback port picked at run time that writes down the host names
//!   it is asked for and refuses them all. What goes through it never
//!   leaves the PC, and is known by name; a connection that goes around it
//!   shows in the connections. A second run reuses that set's data folder
//!   (a later start).
//! - `open`: no proxy, nothing else changed: what WebView2 really opens, by
//!   address, on a first start and on a later one. (Windows' DNS cache,
//!   which would name the addresses, can be read only elevated, so it is not
//!   read.)
//!
//! Each connection's process is named by its part in WebView2 (browser,
//! network service, renderer, ...), read from the process's command line;
//! nothing else of it is kept. It writes target\measure\contacts-<modes>.json and
//! prints a summary. It sends nothing anywhere itself.

#[cfg(windows)]
fn main() {
    measure::run();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("the measurement runs on Windows only");
}

#[cfg(windows)]
mod measure {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::{Duration, Instant};

    use limo_console::measure::{HostLog, connections_of, process_tree};
    use limo_console::webview::Switches;
    use serde_json::{Value, json};

    struct Run {
        set: Switches,
        mode: &'static str,
        start: &'static str,
    }

    /// Each running WebView2 process's part, from its command line's
    /// --type (and --utility-sub-type): browser, renderer, gpu-process,
    /// network service, and so on. Nothing else of the command line is kept.
    fn parts() -> BTreeMap<u32, String> {
        let out = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Get-CimInstance Win32_Process -Filter \"Name = 'msedgewebview2.exe'\" | Select-Object ProcessId, CommandLine | ConvertTo-Json -Compress",
            ])
            .output();
        let mut found = BTreeMap::new();
        let Ok(out) = out else { return found };
        let rows = match serde_json::from_slice::<Value>(&out.stdout) {
            Ok(Value::Array(a)) => a,
            Ok(one) => vec![one],
            Err(_) => return found,
        };
        for row in rows {
            let (Some(pid), Some(line)) = (row["ProcessId"].as_u64(), row["CommandLine"].as_str()) else { continue };
            let value = |name: &str| line.split_whitespace().find_map(|w| w.strip_prefix(name)).map(|v| v.trim_matches('"').to_string());
            let part = match (value("--type="), value("--utility-sub-type=")) {
                (None, _) => "browser".to_string(),
                (Some(t), Some(s)) if t == "utility" => s.replace("network.mojom.NetworkService", "network service"),
                (Some(t), _) => t,
            };
            found.insert(pid as u32, part);
        }
        found
    }

    fn arg(name: &str) -> Option<String> {
        let args: Vec<String> = std::env::args().collect();
        args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
    }

    fn one(console: &Path, out: &Path, run: &Run, hold_ms: u64) -> Value {
        let data = out.join(format!("webview-data-{}-{}", run.set.name(), run.mode));
        if run.start == "first" && data.exists() {
            let _ = std::fs::remove_dir_all(&data);
        }
        let log_file = out.join(format!("{}-{}-{}.log", run.set.name(), run.mode, run.start));
        let host_log = (run.mode == "host-log").then(|| HostLog::start().expect("the host log starts"));
        let mut command = Command::new(console.join(r"target\release\limo-console.exe"));
        command.current_dir(console).arg("--self-check").arg(&log_file);
        command.args(["--hold-ms", &hold_ms.to_string(), "--switches", run.set.name(), "--webview-data"]).arg(&data);
        if let Some(log) = &host_log {
            command.args(["--proxy", &log.port().to_string()]);
        }
        let started = Instant::now();
        let mut child = command.spawn().expect("the console starts");
        let root = child.id();
        let mut known: BTreeMap<u32, String> = BTreeMap::new();
        let mut seen: BTreeMap<(String, u16, u32), u128> = BTreeMap::new();
        let mut roles: BTreeMap<u32, String> = BTreeMap::new();
        let mut roles_read = false;
        let mut exited_at: Option<Instant> = None;
        let mut code = None;
        loop {
            if exited_at.is_none() {
                for (pid, name) in process_tree(root) {
                    known.insert(pid, name);
                }
            }
            let tree: Vec<(u32, String)> = known.iter().map(|(p, n)| (*p, n.clone())).collect();
            for c in connections_of(&tree) {
                if c.remote == "127.0.0.1" || c.remote == "::1" {
                    continue;
                }
                seen.entry((c.remote.clone(), c.remote_port, c.pid)).or_insert(started.elapsed().as_millis());
            }
            if !roles_read && started.elapsed() > Duration::from_secs(4) {
                roles = parts();
                roles_read = true;
            }
            if exited_at.is_none() {
                if let Ok(Some(status)) = child.try_wait() {
                    code = status.code();
                    exited_at = Some(Instant::now());
                }
                if started.elapsed() > Duration::from_secs(60) {
                    let _ = child.kill();
                }
            } else if exited_at.is_some_and(|t| t.elapsed() > Duration::from_secs(3)) {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        // Already ended; this only reaps it.
        let _ = child.wait();
        let asked: Vec<Value> = host_log
            .as_ref()
            .map(|l| l.asked().into_iter().map(|a| json!({ "ms": a.ms as u64, "method": a.method, "host": a.host, "port": a.port })).collect())
            .unwrap_or_default();
        let connections: Vec<Value> = seen
            .iter()
            .map(|((remote, port, pid), ms)| {
                let process = known.get(pid).cloned().unwrap_or_default();
                let part = roles.get(pid).cloned().unwrap_or_else(|| if process.starts_with("limo") { "the console".to_string() } else { "?".to_string() });
                json!({ "first_ms": *ms as u64, "remote": remote, "port": port, "process": process, "part": part })
            })
            .collect();
        let log = std::fs::read_to_string(&log_file).unwrap_or_default();
        let drawn = log.lines().find(|l| l.contains("page sign_in_drawn")).and_then(|l| l.split_whitespace().next()).unwrap_or("?").to_string();
        let result = json!({
            "set": run.set.name(),
            "arguments": run.set.arguments(host_log.as_ref().map(|l| l.port())),
            "mode": run.mode,
            "start": run.start,
            "hold_ms": hold_ms,
            "exit_code": code,
            "ran_ms": exited_at.map(|t| t.duration_since(started).as_millis() as u64),
            "sign_in_drawn_ms": drawn,
            "processes": known.values().cloned().collect::<BTreeSet<_>>(),
            "connections": connections,
            "asked_through_host_log": asked,
        });
        println!(
            "{:<26} {:<8} {:<6} exit {:?}, sign in drawn at {} ms, {} connections, {} asked through the host log",
            run.set.name(),
            run.mode,
            run.start,
            code,
            drawn,
            seen.len(),
            result["asked_through_host_log"].as_array().map_or(0, Vec::len)
        );
        for c in &connections {
            println!("    open: {}:{} from {} ({}) at {} ms", c["remote"], c["port"], c["process"], c["part"], c["first_ms"]);
        }
        for a in &asked {
            println!("    asked: {} {}:{} at {} ms", a["method"], a["host"], a["port"], a["ms"]);
        }
        result
    }

    pub fn run() {
        let console = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let out = console.join(r"target\measure");
        std::fs::create_dir_all(&out).expect("the measure folder");
        let hold_ms: u64 = arg("--hold-ms").and_then(|v| v.parse().ok()).unwrap_or(50_000).min(55_000);
        let sets: Vec<Switches> = match arg("--sets") {
            Some(list) => list.split(',').map(|s| Switches::from_name(s).expect("a set's name")).collect(),
            None => Switches::ALL.to_vec(),
        };
        let modes: Vec<&'static str> = match arg("--modes") {
            Some(list) => list.split(',').map(|m| if m == "open" { "open" } else { "host-log" }).collect(),
            None => vec!["host-log", "open"],
        };
        let mut runs = Vec::new();
        for set in &sets {
            for mode in &modes {
                runs.push(Run { set: *set, mode, start: "first" });
                runs.push(Run { set: *set, mode, start: "later" });
            }
        }
        println!("{} runs of about {} s each", runs.len(), hold_ms / 1000 + 4);
        let results: Vec<Value> = runs.iter().map(|r| one(&console, &out, r, hold_ms)).collect();
        let file = out.join(format!("contacts-{}.json", modes.join("-")));
        std::fs::write(&file, serde_json::to_string_pretty(&results).expect("JSON")).expect("the results file");
        println!("written: {}", file.strip_prefix(&console).unwrap_or(&file).display());
    }
}
