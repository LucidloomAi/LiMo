//! The crate's own rules, checked on its own files: no unsafe code, no
//! allow attributes, no makers' package names in the engine, and no dash
//! punctuation in anything a person reads.

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

#[test]
fn there_is_no_unsafe_code() {
    let lib = std::fs::read_to_string(crate_dir().join("src").join("lib.rs")).unwrap();
    assert!(lib.contains("#![forbid(unsafe_code)]"), "lib.rs does not forbid unsafe code");
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
fn the_engine_holds_no_makers_package_names() {
    // Which app is the home screen, the dialler or the emergency dialler
    // differs from maker to maker: the host says so in the facts.
    for file in rust_files("src") {
        let text = std::fs::read_to_string(&file).unwrap();
        // Unit tests may use sample names; the engine's own code may not.
        let code = text.split("#[cfg(test)]").next().unwrap_or_default();
        for start in ["\"com.", "\"android", "\"org.", "\"ai.", "\"net."] {
            assert!(!code.contains(start), "{} names a package ({start})", file.display());
        }
    }
}

#[test]
fn no_dash_punctuation_in_anything_a_person_reads() {
    let mut files = sources();
    for name in ["README.md", "Cargo.toml"] {
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

#[test]
fn every_sentence_the_phone_shows_is_free_of_dashes() {
    // The words module holds every sentence; string literals there must not
    // use a hyphen as a dash either.
    let words = std::fs::read_to_string(crate_dir().join("src").join("words.rs")).unwrap();
    for (number, line) in words.lines().enumerate() {
        for literal in line.split('"').skip(1).step_by(2) {
            assert!(!literal.contains(" - ") && !literal.contains("--"), "words.rs:{} {literal}", number + 1);
        }
    }
}
