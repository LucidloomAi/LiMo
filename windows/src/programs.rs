//! A program, as LiMo knows it, for each running process: its file name,
//! and the original file name, maker and description from the file's
//! version resource.
//!
//! Reading a version resource opens the file, so doing it every second for
//! every process would be too slow. The [`ProgramCache`] keeps the answer
//! for each file by its path and its last write time, so a file is read once
//! and again only after it is written; and for each running process, by its
//! id and its start, so a process is looked at once in its life.
//!
//! The description ("Game Launcher") is kept beside the program, never in
//! it: the engine's facts and the family's records are the program alone.
//! Only the notice of a program closed shows it, as the name a child knows
//! the program by. It is a property of the program's file, the same for
//! every copy of it. It is not a window title, a document's name or a web
//! address; LiMo reads none of those, and sends none.

#![forbid(unsafe_code)]

use std::collections::HashMap;

use limo_agent::Program;

use crate::os::{Os, ProcessSeen, VersionNames};

/// The most files the cache remembers before it starts again empty.
pub const MAX_FILES_KEPT: usize = 4_096;

/// The longest name kept from a version resource, in characters. A file can
/// say anything there; LiMo keeps no more than a path could hold.
pub const MAX_NAME_CHARS: usize = 260;

/// The longest description a notice shows, in characters. A longer one is
/// cut to end with an ellipsis, which is one of these.
pub const MAX_SHOWN_CHARS: usize = 60;

/// The characters that change the direction text is laid out in (Unicode's
/// `Bidi_Control`). A description could use them to make its words read as
/// something else, so they go with the control characters.
const BIDI_CONTROLS: [char; 12] = [
    '\u{061C}', '\u{200E}', '\u{200F}', '\u{202A}', '\u{202B}', '\u{202C}', '\u{202D}', '\u{202E}', '\u{2066}',
    '\u{2067}', '\u{2068}', '\u{2069}',
];

/// The last part of a path: the file name, with no folder.
pub fn file_name_of(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

/// A name from a version resource or a process list, made safe to keep:
/// control characters dropped, spaces trimmed, at most [`MAX_NAME_CHARS`]
/// characters. None when nothing is left.
pub fn clean_name(raw: &str) -> Option<String> {
    let kept: String = raw.chars().filter(|c| !c.is_control()).take(MAX_NAME_CHARS).collect();
    let trimmed = kept.trim();
    if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
}

/// A program's description made fit to show: whitespace trimmed from both
/// ends and each run of it inside made one space, control characters
/// dropped, and at most [`MAX_SHOWN_CHARS`] characters, a longer one cut to
/// end with an ellipsis. A tab or a line break is whitespace, so it parts
/// two words rather than joining them. None when nothing is left. Doing it
/// twice gives the same as doing it once.
pub fn shown_description(raw: &str) -> Option<String> {
    let mut kept: Vec<char> = Vec::with_capacity(MAX_SHOWN_CHARS + 1);
    let mut space = false;
    for c in raw.chars() {
        if c.is_whitespace() {
            space = !kept.is_empty();
            continue;
        }
        if c.is_control() || BIDI_CONTROLS.contains(&c) {
            continue;
        }
        if space {
            kept.push(' ');
            space = false;
        }
        kept.push(c);
        // One more than fits says there is more: no need to read on.
        if kept.len() > MAX_SHOWN_CHARS {
            break;
        }
    }
    if kept.len() > MAX_SHOWN_CHARS {
        kept.truncate(MAX_SHOWN_CHARS - 1);
        while kept.last() == Some(&' ') {
            kept.pop();
        }
        kept.push('\u{2026}');
    }
    if kept.is_empty() { None } else { Some(kept.into_iter().collect()) }
}

/// The name a child knows a program by, for a notice that it was closed:
/// its description, when its file has one ("Game Launcher"), made fit to
/// show by [`shown_description`]; otherwise its file name as it is now
/// ("Game.exe"), as the engine's own sentence names it; and the name LiMo
/// records it under when the file name is empty.
pub fn shown_name(program: &Program, description: Option<&str>) -> String {
    if let Some(description) = description.and_then(shown_description) {
        return description;
    }
    let base = file_name_of(&program.file_name).trim();
    if base.is_empty() { program.app_id() } else { base.to_string() }
}

/// The program for a file name and what its version resource says.
pub fn program_from(file_name: &str, names: &VersionNames) -> Program {
    Program {
        file_name: clean_name(file_name_of(file_name)).unwrap_or_default(),
        original_name: names.original_name.as_deref().and_then(clean_name),
        maker: names.maker.as_deref().and_then(clean_name),
    }
}

/// A program, and its description beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Known {
    /// The program, as the engine's facts give it.
    pub program: Program,
    /// Its description from its file's version resource, made fit to show
    /// by [`shown_description`]; None when the file has none.
    pub description: Option<String>,
}

impl Known {
    /// The program and its description, for a file name and what its
    /// version resource says.
    pub fn from_names(file_name: &str, names: &VersionNames) -> Known {
        Known {
            program: program_from(file_name, names),
            description: names.description.as_deref().and_then(shown_description),
        }
    }

    /// The name a notice that it was closed shows: see [`shown_name`].
    pub fn shown_name(&self) -> String {
        shown_name(&self.program, self.description.as_deref())
    }
}

/// Remembers the program, and its description, of each file and of each
/// running process.
#[derive(Debug, Default)]
pub struct ProgramCache {
    by_file: HashMap<(String, u64), Known>,
    by_process: HashMap<(u32, u64), Known>,
    files_read: u64,
}

impl ProgramCache {
    /// An empty cache.
    pub fn new() -> ProgramCache {
        ProgramCache::default()
    }

    /// The program a process runs.
    pub fn program<O: Os + ?Sized>(&mut self, os: &mut O, process: &ProcessSeen) -> Program {
        self.known(os, process).program
    }

    /// The program a process runs, with its description.
    pub fn known<O: Os + ?Sized>(&mut self, os: &mut O, process: &ProcessSeen) -> Known {
        // A process that could not be opened has no start to tell it from a
        // later one with the same id, so it is not remembered by process.
        let by_process = process.started != 0;
        if by_process && let Some(known) = self.by_process.get(&(process.pid, process.started)) {
            return known.clone();
        }
        let known = if process.path.is_empty() {
            Known::from_names(&process.name, &VersionNames::default())
        } else {
            match os.file_stamp(&process.path) {
                Some(stamp) => {
                    let key = (process.path.to_lowercase(), stamp);
                    match self.by_file.get(&key) {
                        Some(known) => known.clone(),
                        None => {
                            let known = Known::from_names(&process.path, &os.version_names(&process.path));
                            self.files_read += 1;
                            if self.by_file.len() >= MAX_FILES_KEPT {
                                self.by_file.clear();
                            }
                            self.by_file.insert(key, known.clone());
                            known
                        }
                    }
                }
                None => {
                    self.files_read += 1;
                    Known::from_names(&process.path, &os.version_names(&process.path))
                }
            }
        };
        if by_process {
            self.by_process.insert((process.pid, process.started), known.clone());
        }
        known
    }

    /// Forgets the processes that are no longer running.
    pub fn keep_only(&mut self, running: &[ProcessSeen]) {
        let live: std::collections::HashSet<(u32, u64)> = running.iter().map(|p| (p.pid, p.started)).collect();
        self.by_process.retain(|key, _| live.contains(key));
    }

    /// How many times a version resource was read: for the tests.
    pub fn files_read(&self) -> u64 {
        self.files_read
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(original: Option<&str>, description: Option<&str>) -> VersionNames {
        VersionNames { original_name: original.map(Into::into), maker: None, description: description.map(Into::into) }
    }

    #[test]
    fn names_are_cleaned_and_bounded() {
        assert_eq!(file_name_of(r"C:\Games\Roblox\RobloxPlayerBeta.exe"), "RobloxPlayerBeta.exe");
        assert_eq!(file_name_of("game.exe"), "game.exe");
        assert_eq!(clean_name("  Mojang\u{0}\n "), Some("Mojang".into()));
        assert_eq!(clean_name(" \u{7} "), None);
        assert_eq!(clean_name(&"x".repeat(10_000)).unwrap().chars().count(), MAX_NAME_CHARS);
        let names = VersionNames { original_name: Some("Minecraft.exe\u{0}".into()), maker: Some("  ".into()), description: None };
        let program = program_from(r"C:\Users\Asha\Downloads\homework.exe", &names);
        assert_eq!(program.file_name, "homework.exe");
        assert_eq!(program.original_name.as_deref(), Some("Minecraft.exe"));
        assert_eq!(program.maker, None);
        assert_eq!(program.app_id(), "minecraft.exe");
    }

    #[test]
    fn a_description_is_trimmed_collapsed_cleaned_and_cut() {
        assert_eq!(shown_description("Game Launcher"), Some("Game Launcher".into()));
        assert_eq!(shown_description("  Game \t\t Launcher \r\n"), Some("Game Launcher".into()));
        // A line break parts two words; other control characters go.
        assert_eq!(shown_description("Game\nLauncher"), Some("Game Launcher".into()));
        assert_eq!(shown_description("Ga\u{0}me\u{7} Laun\u{1b}cher\u{7f}"), Some("Game Launcher".into()));
        assert_eq!(shown_description("\u{202E}rehcnuaL emaG"), Some("rehcnuaL emaG".into()));
        // Nothing left: no description.
        assert_eq!(shown_description(""), None);
        assert_eq!(shown_description(" \u{0}\t\u{7}\u{a0} "), None);
        // Sixty characters fit; one more is cut to fifty nine and an
        // ellipsis.
        let sixty = "x".repeat(MAX_SHOWN_CHARS);
        assert_eq!(shown_description(&format!("{sixty}   ")), Some(sixty.clone()));
        let cut = shown_description(&format!("{sixty}y")).unwrap();
        assert_eq!(cut.chars().count(), MAX_SHOWN_CHARS);
        assert_eq!(cut, format!("{}\u{2026}", "x".repeat(MAX_SHOWN_CHARS - 1)));
        // A cut at a space leaves no space before the ellipsis.
        let words = format!("{} {}", "a".repeat(58), "b".repeat(20));
        assert_eq!(shown_description(&words), Some(format!("{}\u{2026}", "a".repeat(58))));
        // Characters, not bytes: a long name in another script is cut the
        // same way, and never inside a character.
        let long = "\u{0905}".repeat(1_000);
        let cut = shown_description(&long).unwrap();
        assert_eq!(cut.chars().count(), MAX_SHOWN_CHARS);
        assert!(cut.ends_with('\u{2026}'));
        // Twice is the same as once.
        for raw in ["Game Launcher", words.as_str(), long.as_str(), "  a \n b  "] {
            let once = shown_description(raw);
            assert_eq!(once.as_deref().and_then(shown_description), once);
        }
    }

    #[test]
    fn a_program_is_shown_by_its_description_or_else_its_file_name_as_it_is_now() {
        let launcher = Known::from_names(r"C:\Games\gamelauncher.exe", &names(Some("GameLauncher.exe"), Some(" Game  Launcher ")));
        assert_eq!(launcher.description.as_deref(), Some("Game Launcher"));
        assert_eq!(launcher.shown_name(), "Game Launcher");
        // The program itself is what it was: the description is beside it.
        assert_eq!(launcher.program, program_from(r"C:\Games\gamelauncher.exe", &names(Some("GameLauncher.exe"), None)));

        let renamed = Known::from_names(r"C:\Users\Asha\Downloads\Homework.exe", &names(Some("Minecraft.exe"), None));
        assert_eq!(renamed.description, None);
        assert_eq!(renamed.shown_name(), "Homework.exe");
        assert_eq!(Known::from_names("Game Launcher.exe", &VersionNames::default()).shown_name(), "Game Launcher.exe");
        // A description of nothing but spaces and control characters is no
        // description.
        let blank = Known::from_names("game.exe", &names(None, Some(" \u{0} \t")));
        assert_eq!((blank.description.as_deref(), blank.shown_name().as_str()), (None, "game.exe"));

        let nameless = Program { file_name: "  ".into(), original_name: Some("Minecraft.exe".into()), maker: None };
        assert_eq!(shown_name(&nameless, None), "minecraft.exe");
        assert_eq!(shown_name(&nameless, Some("Minecraft")), "Minecraft");
        assert_eq!(shown_name(&nameless, Some("\u{7}")), "minecraft.exe");
    }
}
