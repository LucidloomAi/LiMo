//! The crate's own rules, checked on its own files. UniFFI's macros
//! generate unsafe code, so the crate cannot forbid it; these check that no
//! line written here is unsafe, that the conversions forbid it, that the
//! testkit is only ever behind its feature, that nothing silences a lint,
//! and that nothing a person reads has dash punctuation or a machine path.

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
    for dir in ["src", "tests"] {
        files_under(&crate_dir().join(dir), &mut files);
    }
    files.push(crate_dir().join("uniffi-bindgen.rs"));
    files.retain(|p| p.extension().is_some_and(|e| e == "rs"));
    files
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn nothing_written_here_is_unsafe_code() {
    let words = [["unsafe", " {"].concat(), ["unsafe", " fn"].concat(), ["unsafe", " impl"].concat(), ["unsafe", "("].concat()];
    for file in sources() {
        let text = read(&file);
        for word in &words {
            assert!(!text.contains(word.as_str()), "{} has unsafe code", file.display());
        }
    }
}

#[test]
fn the_conversions_forbid_unsafe_code_and_no_uniffi_macro_expands_there() {
    let convert = read(&crate_dir().join("src").join("convert.rs"));
    assert!(convert.contains("#![forbid(unsafe_code)]"));
    assert!(!convert.contains(&["uniffi", "::"].concat()));
    // The crate root says why it cannot forbid it.
    let lib = read(&crate_dir().join("src").join("lib.rs"));
    assert!(lib.contains("cannot forbid it"));
}

#[test]
fn the_testkit_is_only_behind_its_feature() {
    let lib = read(&crate_dir().join("src").join("lib.rs"));
    let declared: Vec<&str> = lib.lines().filter(|l| l.contains("mod testkit")).collect();
    assert_eq!(declared, vec!["pub mod testkit;"]);
    let lines: Vec<&str> = lib.lines().collect();
    let at = lines.iter().position(|l| *l == "pub mod testkit;").unwrap();
    assert_eq!(lines[at - 1], "#[cfg(feature = \"testkit\")]");
    // Nothing else in the library names it.
    for file in sources().iter().filter(|p| p.starts_with(crate_dir().join("src"))) {
        let name = file.file_name().unwrap().to_str().unwrap();
        if name == "testkit.rs" || name == "lib.rs" {
            continue;
        }
        assert!(!read(file).contains("testkit"), "{name} names the testkit");
    }
    let manifest = read(&crate_dir().join("Cargo.toml"));
    assert!(manifest.contains("testkit = [\"limo-link/dev\"]"));
    assert!(!manifest.contains("default ="), "no feature is on by default");
}

#[test]
fn nothing_silences_a_lint() {
    let allow = ["#[", "allow("].concat();
    let allow_inner = ["#![", "allow("].concat();
    let expect = ["#[", "expect("].concat();
    for file in sources() {
        let text = read(&file);
        for word in [&allow, &allow_inner, &expect] {
            assert!(!text.contains(word.as_str()), "{} has an attribute that silences a lint", file.display());
        }
    }
}

#[test]
fn no_dash_punctuation_in_anything_a_person_reads() {
    let mut files = sources();
    for name in ["README.md", "Cargo.toml", "uniffi.toml"] {
        files.push(crate_dir().join(name));
    }
    let em = char::from_u32(0x2014).unwrap();
    let en = char::from_u32(0x2013).unwrap();
    let double = [" -", "- "].concat();
    for file in files {
        let text = read(&file);
        let mut fenced = false;
        for (number, line) in text.lines().enumerate() {
            assert!(!line.contains(em) && !line.contains(en), "{}:{} has a long dash", file.display(), number + 1);
            let unmarked = ["//!", "///"]
                .iter()
                .find_map(|marker| line.trim_start().strip_prefix(marker))
                .unwrap_or(line)
                .trim_start();
            if unmarked.starts_with("```") {
                fenced = !fenced;
                continue;
            }
            if fenced {
                continue;
            }
            let line: String = line.split('`').step_by(2).collect::<Vec<_>>().join("");
            let line = line.as_str();
            assert!(!line.contains(&double), "{}:{} has a double hyphen as a dash", file.display(), number + 1);
            let comment = line.trim_start().starts_with("//") || line.trim_start().starts_with('#');
            let prose = file.extension().is_some_and(|e| e == "md")
                && !line.starts_with("    ")
                && !line.trim_start().starts_with('|');
            let mut body = line.trim_start();
            for marker in ["//!", "///", "//", "#"] {
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
fn no_machine_paths() {
    let mut files = sources();
    files.push(crate_dir().join("README.md"));
    let home = ["C:", "\\Users\\"].concat();
    for file in files {
        let text = read(&file);
        assert!(!text.contains(&home), "{} names a home folder", file.display());
    }
}
