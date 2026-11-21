//! The rules this program keeps, read from its own source.

use std::path::{Path, PathBuf};

use limo_cable_spike::phone::{Ask, PACKAGE};

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn source() -> Vec<(PathBuf, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut paths = Vec::new();
    files(&root.join("src"), &mut paths);
    paths.into_iter().map(|p| (p.clone(), std::fs::read_to_string(&p).unwrap())).collect()
}

fn in_win(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == "win")
}

#[test]
fn unsafe_code_is_only_in_win_and_says_why_each_time() {
    for (path, text) in source() {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if in_win(&path) {
            let lines: Vec<&str> = text.lines().collect();
            for (n, line) in lines.iter().enumerate() {
                if line.contains("unsafe {") || line.contains("unsafe{") {
                    let above = &lines[n.saturating_sub(4)..n];
                    assert!(above.iter().any(|l| l.contains("// SAFETY:")), "{name}:{}: an unsafe block without SAFETY", n + 1);
                }
            }
        } else if name != "lib.rs" {
            assert!(text.contains("#![forbid(unsafe_code)]"), "{name} forbids unsafe code");
            assert!(!text.contains("unsafe {"), "{name} has no unsafe block");
        } else {
            assert!(!text.contains("unsafe {") && !text.contains("unsafe fn"), "lib.rs has no unsafe code");
        }
        assert!(!text.contains("#[allow(") && !text.contains("#![allow("), "{name}: no allow attributes");
    }
}

#[test]
fn every_command_to_the_phone_touches_only_the_test_app_and_one_setting() {
    let allowed_starts = ["getprop ro.", "pm list packages ", "settings get global adb_enabled", "content write --uri content://ai.lucidloom.limo.spike07.cable/", "content query --uri content://ai.lucidloom.limo.spike07.cable/", "am start -n ai.lucidloom.limo.spike07/", "settings put global adb_enabled 0"];
    let never = ["dpm", "account", "uninstall", "reboot", "pm clear", "pm disable", "pm grant", "pm revoke", "cmd ", "rm ", " su", "run-as", "device_owner", "appops", "wm ", "input ", "svc "];
    for ask in Ask::every() {
        let command = ask.command();
        assert!(allowed_starts.iter().any(|s| command.starts_with(s)), "{command}");
        for word in never {
            assert!(!command.contains(word), "{command} holds {word}");
        }
        if command.starts_with("pm list packages ") {
            assert_eq!(command, format!("pm list packages {PACKAGE}"));
        }
        if command.starts_with("settings put") {
            assert!(ask.changes_a_setting());
            assert_eq!(command, "settings put global adb_enabled 0", "the one setting written, and only to switch off");
        }
    }
}

#[test]
fn the_routes_call_only_what_the_steps_need() {
    for (path, text) in source() {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if name == "route_rust.rs" {
            let calls: Vec<&str> = text
                .match_indices("device.")
                .filter_map(|(i, _)| {
                    let rest = &text[i + "device.".len()..];
                    let end = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))?;
                    (end > 0 && rest[end..].starts_with('(')).then(|| &rest[..end])
                })
                .collect();
            for call in calls {
                assert!(
                    ["clone", "shell_command", "exec", "install", "uninstall", "reverse", "reverse_remove"].contains(&call),
                    "route_rust.rs calls adb_client's {call}"
                );
            }
            assert!(text.contains("device.uninstall(PACKAGE, None)"), "adb_client removes only the test app");
        }
        if name == "route_adb.rs" {
            for word in ["\"push\"", "\"pull\"", "\"root\"", "\"reboot\"", "\"tcpip\"", "\"forward\"", "\"sideload\"", "\"remount\"", "\"emu\"", "\"exec-out\"", "\"disable-verity\""] {
                assert!(!text.contains(word), "route_adb.rs runs adb {word}");
            }
            assert!(text.contains("\"uninstall\", PACKAGE"), "adb removes only the test app");
        }
    }
}

#[test]
fn a_serial_number_is_never_printed_whole() {
    for (path, text) in source() {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        for (n, line) in text.lines().enumerate() {
            let prints = line.contains("format!(") || line.contains("say(") || line.contains("println!(") || line.contains("eprintln!(");
            if prints && line.contains("serial") && !line.contains("masked_serial") && !line.trim_start().starts_with("//") {
                panic!("{name}:{}: a line that prints a serial without masking it: {line}", n + 1);
            }
        }
        assert!(!text.contains("serial_number()") || name == "route_nusb.rs", "{name} reads a serial number");
        assert!(!text.contains("{info:?}") && !text.contains("{device:?}"), "{name}: a device's Debug form holds its serial");
    }
}

/// Dash punctuation in a line a person reads: a dash with spaces around
/// it, or an en or em dash. A list's bullet at the start of a line is not
/// punctuation, nor a command line quoted as code (between backticks).
fn has_dash_punctuation(line: &str) -> bool {
    let words: String = line.split('`').step_by(2).collect::<Vec<_>>().join(" ");
    let mut rest = words.trim_start();
    for prefix in ["//!", "///", "//", "#"] {
        if let Some(r) = rest.strip_prefix(prefix) {
            rest = r.trim_start();
            break;
        }
    }
    if let Some(r) = rest.strip_prefix("- ") {
        rest = r;
    }
    rest.contains(" - ") || rest.contains(" -- ") || rest.contains('\u{2013}') || rest.contains('\u{2014}')
}

#[test]
fn no_dash_punctuation_in_anything_a_person_reads() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut texts: Vec<(PathBuf, String)> = source();
    let mut tests = Vec::new();
    files(&root.join("tests"), &mut tests);
    texts.extend(tests.into_iter().map(|p| (p.clone(), std::fs::read_to_string(&p).unwrap())));
    for extra in ["../README.md", "Cargo.toml", ".cargo/config.toml"] {
        let path = root.join(extra);
        if let Ok(text) = std::fs::read_to_string(&path) {
            texts.push((path, text));
        }
    }
    for (path, text) in texts {
        let rust = path.extension().is_some_and(|e| e == "rs");
        let mut fenced = false;
        for (n, line) in text.lines().enumerate() {
            // A block of commands in a README is code, not words.
            if !rust && line.trim_start().starts_with("```") {
                fenced = !fenced;
                continue;
            }
            if fenced {
                continue;
            }
            // In Rust, the words are the comments and the string literals;
            // the code between them is not read by a person.
            let read: Vec<&str> = if !rust || line.trim_start().starts_with("//") {
                vec![line]
            } else {
                line.split('"').skip(1).step_by(2).collect()
            };
            for part in read {
                assert!(!has_dash_punctuation(part), "{}:{}: {line}", path.display(), n + 1);
            }
        }
    }
    // The sample is put together so that this file passes its own rule.
    assert!(has_dash_punctuation(&format!("a word {} another", '-')));
    assert!(!has_dash_punctuation("- a bullet, then --via adb"));
    assert!(!has_dash_punctuation("run `cargo test x -- --ignored` to see"));
}

#[test]
fn no_path_or_name_of_this_pc_in_the_source() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut texts: Vec<(PathBuf, String)> = source();
    for extra in ["../README.md", "Cargo.toml"] {
        if let Ok(text) = std::fs::read_to_string(root.join(extra)) {
            texts.push((root.join(extra), text));
        }
    }
    for (path, text) in texts {
        assert!(!text.contains("C:\\Users\\") && !text.contains("C:/Users/"), "{}: a user's folder", path.display());
        for var in ["USERNAME", "COMPUTERNAME"] {
            if let Ok(name) = std::env::var(var)
                && name.len() > 2
            {
                assert!(!text.to_lowercase().contains(&name.to_lowercase()), "{}: this PC's {var}", path.display());
            }
        }
    }
}
