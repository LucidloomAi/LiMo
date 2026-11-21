//! The crate's own rules, checked on its own files: unsafe code only in
//! `win/`, each block saying why; which Windows calls each file there makes
//! (the look at Windows' lists only reads); no allow attributes; nothing
//! printed by its Debug form; every command and service sent to a phone
//! touching only LiMo, none removing an app; no dash punctuation in anything
//! a person reads.

use std::path::{Path, PathBuf};

use limo_cable::commands::{Ask, PACKAGE, install_command};

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files_under(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn rust_files(dir: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    files_under(&crate_dir().join(dir), &mut files);
    files
}

/// The code of a source file, without its unit tests.
fn code_of(file: &Path) -> String {
    let text = std::fs::read_to_string(file).unwrap();
    text.split("#[cfg(test)]").next().unwrap_or_default().to_string()
}

fn name_of(file: &Path) -> String {
    file.file_name().unwrap().to_string_lossy().to_string()
}

#[test]
fn unsafe_code_only_in_win_each_block_saying_why() {
    let win = crate_dir().join("src").join("win");
    let unsafe_word = ["unsafe", " "].concat();
    let mut files = rust_files("src");
    files.extend(rust_files("tests"));
    files.extend(rust_files("examples"));
    for file in files {
        if name_of(&file) == "house_rules.rs" {
            continue;
        }
        let text = std::fs::read_to_string(&file).unwrap();
        if file.starts_with(&win) {
            let lines: Vec<&str> = text.lines().collect();
            for (number, line) in lines.iter().enumerate() {
                assert!(!line.contains(&format!("{unsafe_word}fn")) && !line.contains(&format!("{unsafe_word}extern")), "{}:{}", file.display(), number + 1);
                if line.contains(&format!("{unsafe_word}{{")) || line.contains(&format!("{unsafe_word}impl")) {
                    let above = &lines[number.saturating_sub(4)..number];
                    assert!(above.iter().any(|l| l.trim_start().starts_with("// SAFETY:")), "{}:{} says not why it is sound", file.display(), number + 1);
                }
            }
            continue;
        }
        let code: Vec<&str> = text.lines().filter(|l| !l.trim_start().starts_with("//")).collect();
        assert!(!code.iter().any(|l| l.contains(&unsafe_word)), "{} has unsafe code", file.display());
        if file.parent() == Some(crate_dir().join("src").as_path()) && name_of(&file) != "lib.rs" {
            assert!(text.contains("#![forbid(unsafe_code)]"), "{} does not forbid unsafe code", file.display());
        }
    }
    let lib = std::fs::read_to_string(crate_dir().join("src").join("lib.rs")).unwrap();
    assert!(lib.contains("#[cfg(windows)]\npub mod win;"), "win is built on Windows only");
}

/// The Windows calls a file makes: every name that starts as Windows'
/// calls do.
fn windows_calls(file: &Path) -> Vec<String> {
    let mut calls = Vec::new();
    for line in code_of(file).lines() {
        if line.trim_start().starts_with("//") || line.trim_start().starts_with("use ") {
            continue;
        }
        for word in line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
            let windows = ["SetupDi", "CM_", "WinUsb_", "Create", "Close", "WaitFor", "GetLastError", "Reg", "DeviceIo"]
                .iter()
                .any(|p| word.starts_with(p));
            if windows && word.chars().any(|c| c.is_ascii_lowercase()) && line.contains(&format!("{word}(")) {
                calls.push(word.to_string());
            }
        }
    }
    calls
}

#[test]
fn the_look_only_reads_and_winusb_only_opens_the_debugging_interface() {
    let win = crate_dir().join("src").join("win");
    let reads = [
        "SetupDiGetClassDevsW",
        "SetupDiEnumDeviceInfo",
        "SetupDiGetDeviceRegistryPropertyW",
        "SetupDiDestroyDeviceInfoList",
        "CM_Get_DevNode_Status",
        "GetLastError",
    ];
    for name in ["mod.rs", "usb.rs"] {
        for call in windows_calls(&win.join(name)) {
            assert!(reads.contains(&call.as_str()), "win/{name} calls {call}");
        }
    }
    let winusb = [
        "SetupDiEnumDeviceInterfaces",
        "SetupDiGetDeviceInterfaceDetailW",
        "CM_Get_Parent",
        "CreateFileW",
        "CloseHandle",
        "CreateEventW",
        "WaitForSingleObject",
        "WinUsb_Initialize",
        "WinUsb_Free",
        "WinUsb_QueryInterfaceSettings",
        "WinUsb_QueryPipe",
        "WinUsb_SetPipePolicy",
        "WinUsb_ReadPipe",
        "WinUsb_WritePipe",
        "WinUsb_GetOverlappedResult",
        "WinUsb_AbortPipe",
    ];
    let calls = windows_calls(&win.join("winusb.rs"));
    for call in &calls {
        assert!(winusb.contains(&call.as_str()) || reads.contains(&call.as_str()), "win/winusb.rs calls {call}");
    }
    assert!(calls.iter().any(|c| c == "WinUsb_Initialize") && calls.iter().any(|c| c == "WinUsb_ReadPipe"));
    for file in rust_files("src") {
        let text = std::fs::read_to_string(&file).unwrap();
        for change in [
            "SetupDiSet", "SetupDiCall", "SetupDiChange", "SetupDiInstall", "SetupDiRemove", "CM_Set", "CM_Disable", "CM_Enable",
            "CM_Reenumerate", "CM_Query_And_Remove", "WinUsb_ControlTransfer", "WinUsb_ResetPipe", "CreateProcess", "Command::new",
            "DeviceIoControl",
        ] {
            assert!(!text.contains(change), "{} has {change}", file.display());
        }
    }
}

#[test]
fn every_command_and_service_touches_only_limo_and_none_removes_an_app() {
    let starts = [
        "getprop ro.",
        "settings get global adb_enabled",
        "settings put global adb_enabled 0",
        "pm list packages ai.lucidloom.limo.child",
        "dumpsys package ai.lucidloom.limo.child",
        "content write --uri content://ai.lucidloom.limo.child.cable/",
        "am start -n ai.lucidloom.limo.child/",
    ];
    let never = ["uninstall", "dpm", "account", "pm clear", "pm disable", "pm grant", "pm revoke", "appops", "reboot", "rm ", " su", "run-as", "svc ", "wm ", "input ", "setprop"];
    for ask in Ask::every() {
        let command = ask.command();
        assert!(starts.iter().any(|s| command.starts_with(s)), "{command}");
        for word in never {
            assert!(!command.contains(word), "{command} has {word}");
        }
        if command.starts_with("settings put") {
            assert_eq!(command, "settings put global adb_enabled 0", "the one setting written, and only to switch off");
        }
    }
    assert_eq!(install_command(1), "cmd package install -r -S 1");
    // The services the protocol opens: the shell, the install, the reverse.
    let protocol = code_of(&crate_dir().join("src").join("protocol.rs"));
    let opened: Vec<&str> = protocol.match_indices("self.open(&format!(\"").map(|(at, _)| &protocol[at + 20..]).collect();
    assert!(!opened.is_empty());
    for service in opened {
        let prefix = service.split('{').next().unwrap();
        assert!(["shell,v2,raw:", "shell:", "exec:", "reverse:forward:", "reverse:killforward:"].contains(&prefix), "{prefix}");
        if prefix == "exec:" {
            assert!(service.starts_with("exec:{}\", crate::commands::install_command(apk.len() as u64))"), "exec only to install");
        }
    }
    for file in rust_files("src") {
        if name_of(&file) == "fake.rs" {
            continue;
        }
        let code = code_of(&file);
        assert!(!code.contains("uninstall"), "{} names uninstall", file.display());
    }
    assert!(PACKAGE == "ai.lucidloom.limo.child");
}

#[test]
fn nothing_silences_a_lint_and_nothing_is_printed_by_its_debug_form() {
    let allow = ["#[", "allow("].concat();
    let allow_inner = ["#![", "allow("].concat();
    let debug = [":", "?}"].concat();
    let mut files = rust_files("src");
    files.extend(rust_files("tests"));
    files.extend(rust_files("examples"));
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(!text.contains(&allow) && !text.contains(&allow_inner), "{} has an allow attribute", file.display());
        if file.starts_with(crate_dir().join("src")) || file.starts_with(crate_dir().join("examples")) {
            for (number, line) in code_of(&file).lines().enumerate() {
                assert!(!line.contains(&debug), "{}:{} prints a Debug form", file.display(), number + 1);
            }
        }
    }
}

#[test]
fn a_device_path_and_a_key_are_never_printed() {
    let winusb = code_of(&crate_dir().join("src").join("win").join("winusb.rs"));
    for line in winusb.lines() {
        let prints = line.contains("format!(") || line.contains("write!(") || line.contains("println!(");
        assert!(!(prints && (line.contains("path.display()") || line.contains("{path") || line.contains("self.path"))), "winusb.rs prints a path: {line}");
    }
    let key = code_of(&crate_dir().join("src").join("key.rs"));
    for line in key.lines() {
        let prints = line.contains("format!(") || line.contains("write!(") || line.contains("println!(");
        let shows = line.contains("{pem") || line.contains("{text") || line.contains("path.display()") || line.contains("{path") || line.contains("{folder");
        assert!(!(prints && shows), "key.rs prints its key or its place: {line}");
    }
}

#[test]
fn no_dash_punctuation_in_anything_a_person_reads() {
    let mut files = rust_files("src");
    files.extend(rust_files("tests"));
    files.extend(rust_files("examples"));
    for name in ["README.md", "Cargo.toml"] {
        files.push(crate_dir().join(name));
    }
    let em = char::from_u32(0x2014).unwrap();
    let en = char::from_u32(0x2013).unwrap();
    let double = [" -", "- "].concat();
    let single = [" ", "-", " "].concat();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        let rust = file.extension().is_some_and(|e| e == "rs");
        let mut fenced = false;
        for (number, line) in text.lines().enumerate() {
            assert!(!line.contains(em) && !line.contains(en), "{}:{} has a long dash", file.display(), number + 1);
            // A fenced block of commands, in a README or a doc comment, is
            // code, not words.
            let bare = line.trim_start().trim_start_matches("//!").trim_start_matches("///").trim_start();
            if bare.starts_with("```") && (!rust || line.trim_start().starts_with("//")) {
                fenced = !fenced;
                continue;
            }
            if fenced {
                continue;
            }
            let words: String = line.split('`').step_by(2).collect::<Vec<_>>().join("");
            let comment = words.trim_start().starts_with("//") || words.trim_start().starts_with('#');
            let read: Vec<String> = if !rust || comment {
                vec![words.clone()]
            } else {
                words.split('"').skip(1).step_by(2).map(str::to_string).collect()
            };
            for part in read {
                let mut body = part.trim_start();
                for marker in ["//!", "///", "//", "#"] {
                    if let Some(rest) = body.strip_prefix(marker) {
                        body = rest.trim_start();
                        break;
                    }
                }
                let body = body.strip_prefix("- ").unwrap_or(body);
                assert!(!body.contains(&double) && !body.contains(&single), "{}:{} has a hyphen used as a dash", file.display(), number + 1);
            }
        }
    }
}
