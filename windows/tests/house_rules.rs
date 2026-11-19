//! The crate's own rules, checked on its own files: unsafe code only in the
//! Windows modules, no allow attributes, and no dash punctuation in anything
//! a person reads.

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

fn sources() -> Vec<PathBuf> {
    let mut files = Vec::new();
    files_under(&crate_dir().join("src"), &mut files);
    files_under(&crate_dir().join("tests"), &mut files);
    files.retain(|p| p.extension().is_some_and(|e| e == "rs"));
    files
}

#[test]
fn unsafe_code_is_only_in_the_windows_modules() {
    let win = crate_dir().join("src").join("win");
    for file in sources() {
        let text = std::fs::read_to_string(&file).unwrap();
        if file.starts_with(&win) {
            continue;
        }
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        let in_src = file.starts_with(crate_dir().join("src"));
        if in_src && name != "lib.rs" {
            assert!(text.contains("#![forbid(unsafe_code)]"), "{} does not forbid unsafe code", file.display());
        }
        let unsafe_word = ["unsafe", " {"].concat();
        let unsafe_fn = ["unsafe", " fn"].concat();
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
fn no_dash_punctuation_in_anything_a_person_reads() {
    let mut files = sources();
    for name in ["README.md", "TESTING.md", "Cargo.toml"] {
        files.push(crate_dir().join(name));
    }
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
