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

/// The string literals on one line of Rust, roughly: enough for words.
fn literals(line: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut chars = line.chars().peekable();
    let mut inside: Option<String> = None;
    let mut before = ' ';
    while let Some(c) = chars.next() {
        match &mut inside {
            None if c == '"' && before != '\'' => inside = Some(String::new()),
            None => {}
            Some(text) if c == '\\' => {
                text.push(c);
                if let Some(next) = chars.next() {
                    text.push(next);
                }
            }
            Some(text) if c == '"' => {
                found.push(std::mem::take(text));
                inside = None;
            }
            Some(text) => text.push(c),
        }
        before = c;
    }
    // A literal that goes on to the next line counts as far as it goes.
    if let Some(text) = inside {
        found.push(text);
    }
    found
}

/// LiMo's words have the typographic apostrophe ("family’s"), as the
/// screens draw them: the crate's own sentences say so in their literals,
/// and words from other crates are turned where they leave the crate (the
/// console's answers, the log, the plain display, the terminal). Tests may
/// hold either, to check both.
#[test]
fn every_apostrophe_in_the_crates_own_words_is_the_typographic_one() {
    let mut files = Vec::new();
    files_under(&crate_dir().join("src"), &mut files);
    files.retain(|p| p.extension().is_some_and(|e| e == "rs") && p.file_name().is_some_and(|n| n != "tests.rs"));
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        let body = text.split("#[cfg(test)]").next().unwrap_or("");
        for (number, line) in body.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            for literal in literals(line) {
                let chars: Vec<char> = literal.chars().collect();
                let straight = chars
                    .windows(3)
                    .any(|w| w[1] == '\'' && w[0].is_alphanumeric() && w[2].is_alphanumeric());
                assert!(!straight, "{}:{} has a straight apostrophe in words: {literal}", file.display(), number + 1);
            }
        }
    }
    // The rule itself, as the crate applies it to words from elsewhere.
    assert_eq!(limo_windows::text::typographic("the family's file"), "the family\u{2019}s file");
    assert_eq!(limo_windows::text::typographic("'quoted' and it's"), "'quoted' and it\u{2019}s");
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
