//! The crate's own rules, checked on its own files: no unsafe code, no
//! allow attributes, and no dash punctuation in anything a person reads.

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
    for dir in ["src", "tests", "examples"] {
        files_under(&crate_dir().join(dir), &mut files);
    }
    files.retain(|p| p.extension().is_some_and(|e| e == "rs"));
    files
}

#[test]
fn the_crate_forbids_unsafe_code_and_has_none() {
    let lib = std::fs::read_to_string(crate_dir().join("src").join("lib.rs")).unwrap();
    assert!(lib.contains("#![forbid(unsafe_code)]"));
    let unsafe_word = ["unsafe", " {"].concat();
    let unsafe_fn = ["unsafe", " fn"].concat();
    let unsafe_impl = ["unsafe", " impl"].concat();
    for file in sources() {
        let text = std::fs::read_to_string(&file).unwrap();
        for word in [&unsafe_word, &unsafe_fn, &unsafe_impl] {
            assert!(!text.contains(word.as_str()), "{} has unsafe code", file.display());
        }
    }
}

#[test]
fn nothing_silences_a_lint() {
    let allow = ["#[", "allow("].concat();
    let allow_inner = ["#![", "allow("].concat();
    let expect = ["#[", "expect("].concat();
    for file in sources() {
        let text = std::fs::read_to_string(&file).unwrap();
        for word in [&allow, &allow_inner, &expect] {
            assert!(!text.contains(word.as_str()), "{} has an attribute that silences a lint", file.display());
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
            // Commands and code, in backticks or in a fenced block (in a
            // README, or in a doc comment), keep their own hyphens.
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
            let prose = file.extension().is_some_and(|e| e == "md") && !line.starts_with("    ") && !line.trim_start().starts_with('|');
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
fn no_machine_paths_or_real_names() {
    // The repository is public: sample names only, and no paths of the
    // machine the crate was written on.
    let mut files = sources();
    files.push(crate_dir().join("README.md"));
    let home = ["C:", "\\Users\\"].concat();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(!text.contains(&home), "{} names a home folder", file.display());
    }
}
