//! A program, as LiMo knows it, for each running process: its file name,
//! and the original file name and maker from the file's version resource.
//!
//! Reading a version resource opens the file, so doing it every second for
//! every process would be too slow. The [`ProgramCache`] keeps the answer
//! for each file by its path and its last write time, so a file is read once
//! and again only after it is written; and for each running process, by its
//! id and its start, so a process is looked at once in its life.

#![forbid(unsafe_code)]

use std::collections::HashMap;

use limo_agent::Program;

use crate::os::{Os, ProcessSeen, VersionNames};

/// The most files the cache remembers before it starts again empty.
pub const MAX_FILES_KEPT: usize = 4_096;

/// The longest name kept from a version resource, in characters. A file can
/// say anything there; LiMo keeps no more than a path could hold.
pub const MAX_NAME_CHARS: usize = 260;

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

/// The name a child knows a program by, for a notice that it was closed:
/// its file name as it is now ("Game.exe"), as the engine's own sentence
/// names it; the name LiMo records it under when the file name is empty.
pub fn shown_name(program: &Program) -> String {
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

/// Remembers the program of each file and of each running process.
#[derive(Debug, Default)]
pub struct ProgramCache {
    by_file: HashMap<(String, u64), Program>,
    by_process: HashMap<(u32, u64), Program>,
    files_read: u64,
}

impl ProgramCache {
    /// An empty cache.
    pub fn new() -> ProgramCache {
        ProgramCache::default()
    }

    /// The program a process runs.
    pub fn program<O: Os + ?Sized>(&mut self, os: &mut O, process: &ProcessSeen) -> Program {
        // A process that could not be opened has no start to tell it from a
        // later one with the same id, so it is not remembered by process.
        let by_process = process.started != 0;
        if by_process && let Some(program) = self.by_process.get(&(process.pid, process.started)) {
            return program.clone();
        }
        let program = if process.path.is_empty() {
            program_from(&process.name, &VersionNames::default())
        } else {
            match os.file_stamp(&process.path) {
                Some(stamp) => {
                    let key = (process.path.to_lowercase(), stamp);
                    match self.by_file.get(&key) {
                        Some(program) => program.clone(),
                        None => {
                            let program = program_from(&process.path, &os.version_names(&process.path));
                            self.files_read += 1;
                            if self.by_file.len() >= MAX_FILES_KEPT {
                                self.by_file.clear();
                            }
                            self.by_file.insert(key, program.clone());
                            program
                        }
                    }
                }
                None => {
                    self.files_read += 1;
                    program_from(&process.path, &os.version_names(&process.path))
                }
            }
        };
        if by_process {
            self.by_process.insert((process.pid, process.started), program.clone());
        }
        program
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

    #[test]
    fn names_are_cleaned_and_bounded() {
        assert_eq!(file_name_of(r"C:\Games\Roblox\RobloxPlayerBeta.exe"), "RobloxPlayerBeta.exe");
        assert_eq!(file_name_of("game.exe"), "game.exe");
        assert_eq!(clean_name("  Mojang\u{0}\n "), Some("Mojang".into()));
        assert_eq!(clean_name(" \u{7} "), None);
        assert_eq!(clean_name(&"x".repeat(10_000)).unwrap().chars().count(), MAX_NAME_CHARS);
        let names = VersionNames { original_name: Some("Minecraft.exe\u{0}".into()), maker: Some("  ".into()) };
        let program = program_from(r"C:\Users\Asha\Downloads\homework.exe", &names);
        assert_eq!(program.file_name, "homework.exe");
        assert_eq!(program.original_name.as_deref(), Some("Minecraft.exe"));
        assert_eq!(program.maker, None);
        assert_eq!(program.app_id(), "minecraft.exe");
    }

    #[test]
    fn a_program_is_shown_by_its_file_name_as_it_is_now() {
        let names = VersionNames { original_name: Some("Minecraft.exe".into()), maker: None };
        assert_eq!(shown_name(&program_from(r"C:\Users\Asha\Downloads\Homework.exe", &names)), "Homework.exe");
        assert_eq!(shown_name(&program_from("Game Launcher.exe", &VersionNames::default())), "Game Launcher.exe");
        let nameless = Program { file_name: "  ".into(), original_name: Some("Minecraft.exe".into()), maker: None };
        assert_eq!(shown_name(&nameless), "minecraft.exe");
    }
}
