//! What LiMo knows of a child's apps by name, for the guardian's screens:
//! each phone's list, as the phone last sent it over the link (kept in the
//! family's file), and the PC's programs as the service saw them running in
//! each account's sessions since it started (kept in memory only). Names are
//! only as the phone or Windows gave them: a phone's label, a program's
//! description from its own file, or its file name.
//!
//! What it does not know: a program installed on the PC that has not run
//! since the service started, or that ran only before a restart; and an app
//! on a phone that has sent no list yet.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use serde::Serialize;

use crate::programs::Known;

/// The most programs kept for one Windows account. When it is full, the one
/// seen longest ago makes room.
pub const MAX_SEEN_PROGRAMS: usize = 500;

/// The most apps one answer lists; a longer list is cut, and says so. A
/// phone sends at most the family file's 2,000.
pub const MAX_APPS_ANSWERED: usize = limo_hub::MAX_PHONE_APPS;

/// A program the service saw running in an account's session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SeenProgram {
    /// The id the rules and the day's use name it by: its original file
    /// name when its file records one, otherwise its file name, in lower
    /// case.
    pub app_id: String,
    /// The name a child knows it by: its file's description, or its file
    /// name.
    pub name: String,
    /// When it was last seen running, by the master's clock.
    pub last_seen_at: i64,
}

/// The programs seen running, by Windows account.
#[derive(Debug, Clone, Default)]
pub struct SeenPrograms {
    by_account: BTreeMap<String, BTreeMap<String, SeenProgram>>,
}

impl SeenPrograms {
    /// Nothing seen yet.
    pub fn new() -> SeenPrograms {
        SeenPrograms::default()
    }

    /// A program seen running in a session of `account_id` now.
    pub fn note(&mut self, account_id: &str, known: &Known, now: i64) {
        let app_id = known.program.app_id();
        if app_id.is_empty() || account_id.is_empty() {
            return;
        }
        let kept = self.by_account.entry(account_id.to_string()).or_default();
        if !kept.contains_key(&app_id) && kept.len() >= MAX_SEEN_PROGRAMS {
            let oldest = kept.values().min_by_key(|p| (p.last_seen_at, p.app_id.clone())).map(|p| p.app_id.clone());
            if let Some(oldest) = oldest {
                kept.remove(&oldest);
            }
        }
        kept.insert(app_id.clone(), SeenProgram { app_id, name: known.shown_name(), last_seen_at: now });
    }

    /// The programs seen in any of these accounts, each once (the newest
    /// sighting), by name.
    pub fn of_accounts(&self, accounts: &[String]) -> Vec<SeenProgram> {
        let mut merged: BTreeMap<String, SeenProgram> = BTreeMap::new();
        for account in accounts {
            for program in self.by_account.get(account).into_iter().flat_map(BTreeMap::values) {
                let newer = merged.get(&program.app_id).is_none_or(|kept| kept.last_seen_at < program.last_seen_at);
                if newer {
                    merged.insert(program.app_id.clone(), program.clone());
                }
            }
        }
        let mut programs: Vec<SeenProgram> = merged.into_values().collect();
        programs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.app_id.cmp(&b.app_id)));
        programs
    }
}

/// One app as a list names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NamedApp {
    /// The id the rules and the day's use name it by.
    pub app_id: String,
    /// Its name, as the phone or Windows gave it.
    pub name: String,
}

/// Where an app's name was found: on which device, under which name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sighting {
    /// The device.
    pub device_id: String,
    /// The device's name.
    pub device_name: String,
    /// The app on it.
    pub app: NamedApp,
}

/// An app a child's rules or day name, with its name and the devices it is
/// on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppName {
    /// The id as the rules or the day's use have it.
    pub app_id: String,
    /// Its name as a device gave it; None when no device's list has it.
    pub name: Option<String>,
    /// The devices whose list has it, by id and name.
    pub devices: Vec<DeviceRef>,
}

/// A device, by id and name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeviceRef {
    /// Its id.
    pub device_id: String,
    /// Its name.
    pub name: String,
}

/// Each id the rules or the day's use name, with its name and devices, by
/// id, each once. Ids are matched without regard to case, as the rules
/// match programs; the first device that names an app gives its name.
pub fn app_names(ids: &[String], sightings: &[Sighting]) -> Vec<AppName> {
    let mut seen: BTreeMap<String, AppName> = BTreeMap::new();
    for id in ids {
        let key = id.trim().to_lowercase();
        if key.is_empty() || seen.contains_key(&key) {
            continue;
        }
        let found: Vec<&Sighting> = sightings.iter().filter(|s| s.app.app_id.to_lowercase() == key).collect();
        let mut devices: Vec<DeviceRef> = Vec::new();
        for sighting in &found {
            if !devices.iter().any(|d| d.device_id == sighting.device_id) {
                devices.push(DeviceRef { device_id: sighting.device_id.clone(), name: sighting.device_name.clone() });
            }
        }
        let name = found.first().map(|s| s.app.name.clone());
        seen.insert(key, AppName { app_id: id.trim().to_string(), name, devices });
    }
    seen.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::os::VersionNames;

    fn known(file: &str, original: Option<&str>, description: Option<&str>) -> Known {
        Known::from_names(
            file,
            &VersionNames {
                original_name: original.map(String::from),
                description: description.map(String::from),
                ..VersionNames::default()
            },
        )
    }

    #[test]
    fn a_program_is_kept_by_its_app_id_with_its_description_or_its_file_name() {
        let mut seen = SeenPrograms::new();
        seen.note("S-1-5-21-1001", &known(r"C:\Games\gl.exe", Some("GameLauncher.exe"), Some("Game Launcher")), 10);
        seen.note("S-1-5-21-1001", &known(r"C:\Tools\notes.exe", None, None), 11);
        // The same program again, later: once, with its newest time.
        seen.note("S-1-5-21-1001", &known(r"C:\Games\gl.exe", Some("GameLauncher.exe"), Some("Game Launcher")), 20);
        let programs = seen.of_accounts(&["S-1-5-21-1001".to_string()]);
        assert_eq!(
            programs,
            vec![
                SeenProgram { app_id: "gamelauncher.exe".into(), name: "Game Launcher".into(), last_seen_at: 20 },
                SeenProgram { app_id: "notes.exe".into(), name: "notes.exe".into(), last_seen_at: 11 },
            ]
        );
        // Another account's programs are its own.
        assert!(seen.of_accounts(&["S-1-5-21-1002".to_string()]).is_empty());
    }

    #[test]
    fn a_full_account_makes_room_by_forgetting_the_program_seen_longest_ago() {
        let mut seen = SeenPrograms::new();
        for n in 0..MAX_SEEN_PROGRAMS {
            seen.note("a", &known(&format!("p{n}.exe"), None, None), 100 + n as i64);
        }
        seen.note("a", &known("new.exe", None, None), 10_000);
        let programs = seen.of_accounts(&["a".to_string()]);
        assert_eq!(programs.len(), MAX_SEEN_PROGRAMS);
        assert!(programs.iter().any(|p| p.app_id == "new.exe"));
        assert!(!programs.iter().any(|p| p.app_id == "p0.exe"), "the oldest went");
    }

    #[test]
    fn the_names_of_a_childs_apps_come_from_the_devices_that_list_them() {
        let phone = |app_id: &str, name: &str| Sighting {
            device_id: "device-2".into(),
            device_name: "Asha\u{2019}s phone".into(),
            app: NamedApp { app_id: app_id.into(), name: name.into() },
        };
        let pc = |app_id: &str, name: &str| Sighting {
            device_id: "device-1".into(),
            device_name: "Family PC".into(),
            app: NamedApp { app_id: app_id.into(), name: name.into() },
        };
        let sightings = vec![phone("com.example.game", "Game"), pc("notes.exe", "Notes"), phone("com.example.maps", "Maps")];
        let ids: Vec<String> = ["notes.exe", "com.example.game", "Notes.EXE", "unknown.exe", " "].iter().map(|s| s.to_string()).collect();
        let names = app_names(&ids, &sightings);
        assert_eq!(names.len(), 3, "each once, case aside, and nothing for an empty id");
        let game = names.iter().find(|n| n.app_id == "com.example.game").unwrap();
        assert_eq!(game.name.as_deref(), Some("Game"));
        assert_eq!(game.devices, vec![DeviceRef { device_id: "device-2".into(), name: "Asha\u{2019}s phone".into() }]);
        let notes = names.iter().find(|n| n.app_id == "notes.exe").unwrap();
        assert_eq!((notes.name.as_deref(), notes.devices.len()), (Some("Notes"), 1));
        let unknown = names.iter().find(|n| n.app_id == "unknown.exe").unwrap();
        assert_eq!((unknown.name.clone(), unknown.devices.clone()), (None, vec![]), "said plainly: no device names it");
    }
}
