//! The crate's own rules, checked on its own files: no unsafe code, no
//! allow attributes, no dash punctuation in anything a person reads, and
//! nothing printed by its Debug form; never the service's folder; and
//! nothing sent to the phone that could change anything but LiMo's own app
//! and files.

use std::path::{Path, PathBuf};

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files_under(&path, out);
        } else {
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
    files.retain(|p| p.extension().is_some_and(|e| e == "rs"));
    files
}

fn sources() -> Vec<PathBuf> {
    let mut files = rust_files("src");
    files.extend(rust_files("tests"));
    files
}

/// The code of a source file, without its unit tests.
fn code_of(file: &Path) -> String {
    let text = std::fs::read_to_string(file).unwrap();
    text.split("#[cfg(test)]").next().unwrap_or_default().to_string()
}

#[test]
fn there_is_no_unsafe_code() {
    for root in ["lib.rs", "main.rs"] {
        let text = std::fs::read_to_string(crate_dir().join("src").join(root)).unwrap();
        assert!(text.contains("#![forbid(unsafe_code)]"), "{root} does not forbid unsafe code");
    }
    let unsafe_word = ["unsafe", " {"].concat();
    let unsafe_fn = ["unsafe", " fn"].concat();
    for file in sources() {
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(!text.contains(&unsafe_word) && !text.contains(&unsafe_fn), "{} has unsafe code", file.display());
    }
}

#[test]
fn nothing_silences_a_lint() {
    let allow = ["#[", "allow("].concat();
    let allow_inner = ["#![", "allow("].concat();
    for file in sources() {
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(!text.contains(&allow) && !text.contains(&allow_inner), "{} has an allow attribute", file.display());
    }
}

#[test]
fn the_service_family_is_never_named() {
    // The Windows service keeps the real family in its own data folder; this
    // tool has its own, named for testing.
    for file in rust_files("src") {
        let text = code_of(&file).to_ascii_lowercase();
        for name in ["programdata", "\"family.limo\"", "program files"] {
            assert!(!text.contains(name), "{} names {name}", file.display());
        }
    }
    let places = std::fs::read_to_string(crate_dir().join("src").join("places.rs")).unwrap();
    assert!(places.contains("pub const FOLDER_NAME: &str = \"LiMo-phone-dev-TEST-ONLY\";"));
}

#[test]
fn nothing_sent_to_the_phone_could_change_anything_but_limo() {
    use limo_phone_dev::phone::*;
    // Every shell command the tool has: the reads, and what touches LiMo.
    let reads = [props_command(), listed_command(), dump_command(), clock_command()];
    let limo = [probe_command(), look_command(), read_state_command(), write_pairing_command(), remove_pairing_command()];
    let broadcasts = [broadcast_command(PAIR_ACTION), broadcast_command(FORGET_ACTION)];
    // What the promise at the top of the help rules out: settings, accounts,
    // a Device Owner or a device admin, other apps, permissions, the
    // developer options, rebooting.
    let forbidden = [
        "settings", "dpm", "grant", "revoke", "disable", "enable", "hide", "clear", "appops", "cmd ", "svc ", "setprop",
        "reboot", "account", "su ", "content ",
    ];
    for command in reads.iter().chain(&limo).chain(&broadcasts) {
        for word in forbidden {
            assert!(!command.contains(word), "{command} has {word}");
        }
    }
    for command in &reads {
        let first = command.split_whitespace().next().unwrap();
        assert!(["getprop", "pm", "dumpsys", "date"].contains(&first), "{command}");
        if first == "pm" {
            assert!(command.starts_with("pm list packages "), "{command}");
        }
    }
    for command in &limo {
        assert!(command.starts_with("run-as ai.lucidloom.limo.child "), "{command}");
    }
    for command in &broadcasts {
        assert!(command.starts_with("am broadcast -a ai.lucidloom.limo.child.dev."), "{command}");
        assert!(command.contains(" -n ai.lucidloom.limo.child/.dev.DevReceiver "), "{command}");
    }
    // The only other calls: listing phones, installing and removing LiMo's
    // child form, and its log (followed, or read once for LiMo's tag).
    let phone = code_of(&crate_dir().join("src").join("phone.rs"));
    let runs: Vec<&str> = phone.match_indices("self.run(&[\"").map(|(at, _)| &phone[at + 12..]).collect();
    assert!(!runs.is_empty());
    for run in runs {
        let first = run.split('"').next().unwrap();
        assert!(["shell", "install", "uninstall", "logcat"].contains(&first), "self.run with {first}");
        if first == "uninstall" {
            assert!(run.starts_with("uninstall\", PACKAGE]"), "{run}");
        }
        if first == "logcat" {
            // Read once, never cleared or changed.
            assert!(run.starts_with("logcat\", \"-d\""), "{run}");
            assert!(!run.contains("\"-c\"") && !run.contains("\"-G\"") && !run.contains("\"-P\""), "{run}");
        }
    }
}

#[test]
fn no_dash_punctuation_in_anything_a_person_reads() {
    let mut files = sources();
    for name in ["README.md", "Cargo.toml"] {
        files.push(crate_dir().join(name));
    }
    files.push(crate_dir().join("..").join("TESTING.md"));
    let em = char::from_u32(0x2014).unwrap();
    let en = char::from_u32(0x2013).unwrap();
    let double = [" -", "- "].concat();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        let mut fenced = false;
        for (number, line) in text.lines().enumerate() {
            assert!(!line.contains(em) && !line.contains(en), "{}:{} has a long dash", file.display(), number + 1);
            // Commands and code, in backticks or in a fenced block, keep
            // their own hyphens.
            if line.trim_start().starts_with("```") {
                fenced = !fenced;
                continue;
            }
            if fenced {
                continue;
            }
            let line: String = line.split('`').step_by(2).collect::<Vec<_>>().join("");
            let line = line.as_str();
            assert!(!line.contains(&double), "{}:{} has a double hyphen as a dash", file.display(), number + 1);
            let comment = line.trim_start().starts_with("//");
            let prose = file.extension().is_some_and(|e| e == "md") && !line.starts_with("    ") && !line.trim_start().starts_with('|');
            let mut body = line.trim_start();
            for marker in ["//!", "///", "//"] {
                if let Some(rest) = body.strip_prefix(marker) {
                    body = rest.trim_start();
                    break;
                }
            }
            let body = body.strip_prefix("- ").unwrap_or(body);
            if comment || prose {
                assert!(!body.contains(" - "), "{}:{} has a hyphen used as a dash", file.display(), number + 1);
            }
        }
    }
}

#[test]
fn nothing_a_person_reads_is_printed_by_its_debug_form() {
    // A value printed with `{:?}` shows Rust's own names ("UsageAccess"),
    // not words. Only the unit tests may use it, in their own messages.
    let debug = [":", "?}"].concat();
    let pretty = [":#", "?}"].concat();
    for file in rust_files("src") {
        let code = code_of(&file);
        for (number, line) in code.lines().enumerate() {
            assert!(!line.contains(&debug) && !line.contains(&pretty), "{}:{} prints a Debug form", file.display(), number + 1);
        }
    }
}

#[test]
fn every_sentence_the_tool_prints_is_free_of_dashes() {
    for file in rust_files("src") {
        let text = code_of(&file);
        for (number, line) in text.lines().enumerate() {
            for literal in line.split('"').skip(1).step_by(2) {
                assert!(!literal.contains(" - "), "{}:{} prints a hyphen as a dash: {literal}", file.display(), number + 1);
            }
        }
    }
}
