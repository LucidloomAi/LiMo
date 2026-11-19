//! The orders carried out: a warning held until it is shown, programs
//! closed in their own session only and never the parts of Windows or
//! LiMo's own, a part of Windows the rules block said once a session, a
//! closed program named by its description, phones ignored and said once,
//! problems logged once in a while.

use limo_agent::{Program, SessionState, Show, WarningShown};
use limo_core::DeviceKind;
use limo_hub::{AccountLink, EventKind};
use limo_windows::os::ProcessSeen;
use limo_windows::protocol::{FromHelper, ToHelper};
use limo_windows::service::QUIET_MS;

use crate::common::*;

const ASHA: &str = "S-1-5-21-1001";
const MIRA: &str = "S-1-5-21-1002";
const MUM: &str = "S-1-5-21-1000";
const GAME: &str = r"C:\Games\Blocks\blocks.exe";

/// Every show a session's helpers got, in order, with its warning.
fn warnings_shown(pc: &mut Pc, session: u32) -> Vec<(u64, Option<WarningShown>)> {
    let mut out = Vec::new();
    for helper in pc.os().helpers.iter().filter(|h| h.session == session) {
        for message in &helper.got {
            if let ToHelper::Show { seq, show: Show::TimeLeft { warning, .. }, .. } = message {
                out.push((*seq, warning.clone()));
            }
        }
    }
    out
}

/// Runs until the session's helper has been shown a warning; its show's
/// number and the warning.
fn run_to_warning(pc: &mut Pc, session: u32) -> (u64, WarningShown) {
    for _ in 0..1_200 {
        pc.step();
        if let Some((seq, Some(warning))) = warnings_shown(pc, session).into_iter().find(|(_, w)| w.is_some()) {
            return (seq, warning);
        }
    }
    panic!("no warning came");
}

#[test]
fn a_warning_is_held_until_the_helper_says_it_showed_it() {
    let mut pc = Pc::new(at(16, 0));
    pc.add_child("Asha", ASHA, 6);
    pc.login(1, ASHA, "asha");
    pc.step();
    pc.greet(1);
    let (seq, warning) = run_to_warning(&mut pc, 1);
    // With 6 minutes a day, the first warning point reached is 15 minutes,
    // and its words say the 6 minutes there are.
    assert_eq!((warning.minutes, warning.message.as_str()), (15, "6 minutes of screen time left today."));
    // The engine gave it in one call; every show after it carries it still.
    pc.run(10);
    let after: Vec<_> = warnings_shown(&mut pc, 1).into_iter().filter(|(s, _)| *s > seq).collect();
    assert!(after.len() >= 9, "{after:?}");
    assert!(after.iter().all(|(_, w)| w.as_ref() == Some(&warning)), "{after:?}");
    assert_eq!(pc.service.session_view(1).unwrap().held_warning, Some(warning.clone()));
    // The helper says it showed it: it is held no more.
    pc.say(1, FromHelper::WarningShown { cause: warning.cause, minutes: warning.minutes, lock_at: warning.lock_at });
    pc.step();
    let last = warnings_shown(&mut pc, 1).last().cloned().unwrap();
    assert_eq!(last.1, None);
    assert_eq!(pc.service.session_view(1).unwrap().held_warning, None);
    pc.run(5);
    assert!(warnings_shown(&mut pc, 1).iter().rev().take(5).all(|(_, w)| w.is_none()));
}

#[test]
fn a_warning_never_confirmed_is_held_until_its_lock_time_passes() {
    let mut pc = Pc::new(at(16, 0));
    let asha = pc.add_child("Asha", ASHA, 6);
    pc.login(1, ASHA, "asha");
    pc.step();
    pc.greet(1);
    let (_, warning) = run_to_warning(&mut pc, 1);
    // A guardian gives more time: the PC stays open past the lock time the
    // warning named, so only that time ends the hold.
    let now = pc.now();
    pc.hub().give_extra(&asha, 30, "tap-1", now).unwrap();
    while pc.now() < warning.lock_at - 2 {
        pc.step();
    }
    assert_eq!(pc.service.session_view(1).unwrap().held_warning, Some(warning.clone()));
    pc.run(4);
    assert!(matches!(pc.service.session_view(1).unwrap().show, Show::TimeLeft { warning: None, .. }));
    assert_eq!(pc.service.session_view(1).unwrap().held_warning, None);
}

#[test]
fn a_blocked_program_is_closed_in_its_own_session_only_every_copy_and_logged_once() {
    let mut pc = Pc::new(at(16, 0));
    let asha = pc.add_child("Asha", ASHA, 60);
    pc.add_child("Mira", MIRA, 60);
    pc.hub().link_account(MUM, "Mum", AccountLink::NotLimited).unwrap();
    pc.change_rules(&asha, |rules| rules.blocked_apps = vec!["Blocks.exe".into()]);
    pc.login(1, ASHA, "asha");
    pc.login(2, MIRA, "mira");
    pc.set_state(2, SessionState::Disconnected);
    pc.login(3, MUM, "mum");
    pc.set_state(3, SessionState::Disconnected);
    pc.file(GAME, Some("Blocks.exe"), Some("Blocks Ltd"));
    let ashas = [pc.start(1, GAME), pc.start(1, GAME)];
    let miras = pc.start(2, GAME);
    let mums = pc.start(3, GAME);
    let pid = pc.os().new_pid();
    pc.os().processes.push(ProcessSeen { pid, session_id: 0, started: 3, name: "blocks.exe".into(), path: GAME.into() });
    pc.step();
    pc.greet(1);
    pc.run(2);

    let ended: Vec<u32> = pc.os().ended.iter().map(|p| p.pid).collect();
    assert_eq!(ended, ashas.to_vec());
    for other in [miras, mums, pid] {
        assert!(pc.os().processes.iter().any(|p| p.pid == other), "{other} was closed");
    }
    let closed = pc.logged("Closed blocks.exe in session 1 (2 running copies): blocks.exe is blocked by your family's rules");
    assert_eq!(closed.len(), 1, "{:?}", pc.os().log);
    // The sentence, with the program's name beside it for the drawn notice.
    assert!(pc.helper(1).unwrap().got.iter().any(|m| matches!(
        m,
        ToHelper::Notice { text, program: Some(program) }
            if text == "blocks.exe is blocked by your family's rules, so LiMo closed it." && program == "blocks.exe"
    )));

    // Started again and again: closed each time, logged once.
    for _ in 0..10 {
        pc.start(1, GAME);
        pc.run(1);
    }
    assert_eq!(pc.os().ended.len(), 12);
    assert_eq!(pc.logged("Closed blocks.exe in session 1").len(), 1);
}

/// Every notice the helpers of a session got, in order: the sentence and
/// the name shown.
fn notices(pc: &mut Pc, session: u32) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    for helper in pc.os().helpers.iter().filter(|h| h.session == session) {
        for message in &helper.got {
            if let ToHelper::Notice { text, program } = message {
                out.push((text.clone(), program.clone()));
            }
        }
    }
    out
}

/// Starts the program at `path` in session 1, where it is blocked: the
/// notice of its close.
fn closed_as(pc: &mut Pc, path: &str) -> (String, Option<String>) {
    let before = notices(pc, 1).len();
    let ended = pc.os().ended.len();
    pc.start(1, path);
    pc.run(1);
    assert_eq!(pc.os().ended.len(), ended + 1, "{path} was not closed");
    let after = notices(pc, 1);
    assert_eq!(after.len(), before + 1, "{after:?}");
    after.last().cloned().unwrap_or_default()
}

#[test]
fn a_closed_program_is_named_by_its_files_description_when_it_has_one() {
    const LAUNCHER: &str = r"C:\Games\Launcher\gamelauncher.exe";
    const MAZE: &str = r"C:\Games\Maze\maze.exe";
    const NOISY: &str = r"C:\Games\Noisy\noisy.exe";
    const BLANK: &str = r"C:\Games\Blank\blank.exe";
    const PAINT: &str = r"C:\Apps\Paint\paint.exe";
    const NOTEPAD: &str = r"C:\Program Files\WindowsApps\Notepad\Notepad.exe";
    let mut pc = Pc::new(at(16, 0));
    let asha = pc.add_child("Asha", ASHA, 60);
    pc.change_rules(&asha, |rules| {
        rules.blocked_apps =
            ["GameLauncher.exe", "blocks.exe", "maze.exe", "noisy.exe", "blank.exe", "notepad.exe"].map(String::from).to_vec();
    });
    pc.login(1, ASHA, "asha");
    pc.described_file(LAUNCHER, Some("GameLauncher.exe"), Some("Some Studio"), "Game Launcher");
    // Windows 11's Notepad describes itself by its file name.
    pc.described_file(NOTEPAD, None, Some("Microsoft Corporation"), "Notepad.exe");
    pc.file(GAME, Some("Blocks.exe"), Some("Blocks Ltd"));
    let long = format!("The Amazing Maze of {}and More", "Endless ".repeat(20));
    pc.described_file(MAZE, None, None, &long);
    pc.described_file(NOISY, None, None, "\u{7}Noisy\u{0}\r\n\tGame\u{1b}  Pro ");
    pc.described_file(BLANK, None, None, " \u{0}\t\u{7} ");
    // A program that is not blocked, with a description, runs all along.
    pc.described_file(PAINT, Some("Paint.exe"), Some("Paint Ltd"), "Paint Studio");
    pc.start(1, PAINT);
    pc.step();
    pc.greet(1);
    pc.run(2);

    // With a description: the notice shows it. The engine's sentence, and
    // everything else, still name the file.
    let reason = |file: &str| format!("{file} is blocked by your family's rules, so LiMo closed it.");
    assert_eq!(closed_as(&mut pc, LAUNCHER), (reason("gamelauncher.exe"), Some("Game Launcher".to_string())));
    let line = "Closed gamelauncher.exe (\"Game Launcher\") in session 1 (1 running copy): gamelauncher.exe is blocked";
    assert_eq!(pc.logged(line).len(), 1, "{:?}", pc.os().log);
    // Without one: the file name, as before, and the log line as before.
    assert_eq!(closed_as(&mut pc, GAME), (reason("blocks.exe"), Some("blocks.exe".to_string())));
    assert_eq!(pc.logged("Closed blocks.exe in session 1 (1 running copy): blocks.exe is blocked").len(), 1);
    // A long one: cut to sixty characters, the last an ellipsis.
    let (_, shown) = closed_as(&mut pc, MAZE);
    let shown = shown.unwrap_or_default();
    assert_eq!(shown, format!("The Amazing Maze of {}\u{2026}", ["Endless"; 5].join(" ")));
    assert_eq!(shown.chars().count(), 60);
    // Control characters dropped, whitespace trimmed and made single spaces.
    assert_eq!(closed_as(&mut pc, NOISY).1.as_deref(), Some("Noisy Game Pro"));
    // Nothing left of it: the file name.
    assert_eq!(closed_as(&mut pc, BLANK).1.as_deref(), Some("blank.exe"));
    // A description that is the file name again: the notice says
    // "Notepad", and the log line has no bracket, since it would say
    // nothing more.
    assert_eq!(closed_as(&mut pc, NOTEPAD), (reason("Notepad.exe"), Some("Notepad".to_string())));
    let line = "Closed Notepad.exe in session 1 (1 running copy): Notepad.exe is blocked";
    assert_eq!(pc.logged(line).len(), 1, "{:?}", pc.os().log);
    assert!(pc.logged("Closed Notepad.exe (").is_empty(), "{:?}", pc.os().log);

    // The file is read once, however often the program is started and
    // closed; written again (an update), it is read again, and its new
    // description shown.
    for _ in 0..3 {
        assert_eq!(closed_as(&mut pc, LAUNCHER).1.as_deref(), Some("Game Launcher"));
    }
    let reads = |pc: &mut Pc, path: &str| pc.os().version_reads.iter().filter(|p| p.as_str() == path).count();
    assert_eq!(reads(&mut pc, LAUNCHER), 1);
    assert_eq!(reads(&mut pc, PAINT), 1);
    pc.os().stamps.insert(LAUNCHER.into(), 2);
    pc.os().versions.get_mut(LAUNCHER).unwrap().description = Some("Game Launcher 2".into());
    assert_eq!(closed_as(&mut pc, LAUNCHER).1.as_deref(), Some("Game Launcher 2"));
    assert_eq!(reads(&mut pc, LAUNCHER), 2);

    // The engine's facts are the program alone, as before: no description
    // reaches them, nor the family's records.
    pc.run(2);
    let paint = Program { file_name: "paint.exe".into(), original_name: Some("Paint.exe".into()), maker: Some("Paint Ltd".into()) };
    assert_eq!(pc.facts_of(1).unwrap().running, vec![paint]);
    let facts = format!("{:?}", pc.service.last_facts());
    assert!(!facts.contains("Paint Studio") && !facts.contains("Game Launcher"), "{facts}");
    let events = pc.hub().events(500, None).unwrap();
    assert!(events.iter().any(|e| matches!(&e.what, EventKind::BlockedProgramClosed { program, .. } if program == "gamelauncher.exe")));
    let events = format!("{events:?}");
    assert!(!events.contains("Game Launcher") && !events.contains("Amazing") && !events.contains("Noisy Game"), "{events}");
}

#[test]
fn limos_own_processes_and_the_parts_of_windows_are_never_closed() {
    let mut pc = Pc::new(at(16, 0));
    let asha = pc.add_child("Asha", ASHA, 60);
    pc.change_rules(&asha, |rules| {
        rules.blocked_apps = vec!["explorer.exe".into(), "limo.exe".into(), "RobloxPlayerBeta.exe".into(), "mystery.exe".into()];
    });
    pc.login(1, ASHA, "asha");
    let shell = r"C:\Windows\explorer.exe";
    let renamed = r"C:\Users\Asha\Downloads\explorer.exe";
    pc.file(shell, Some("EXPLORER.EXE.MUI"), Some("Microsoft Corporation"));
    pc.file(renamed, Some("RobloxPlayerBeta.exe"), Some("Roblox Corporation"));
    let shell_pid = pc.start(1, shell);
    let renamed_pid = pc.start(1, renamed);
    // A stray copy of LiMo's own program, and a process whose file cannot be
    // read.
    let stray = pc.start(1, OWN_EXE);
    let mystery = pc.os().new_pid();
    pc.os().processes.push(ProcessSeen { pid: mystery, session_id: 1, started: 0, name: "mystery.exe".into(), path: String::new() });
    pc.step();
    pc.greet(1);
    pc.run(3);

    let ended: Vec<u32> = pc.os().ended.iter().map(|p| p.pid).collect();
    assert_eq!(ended, vec![renamed_pid]);
    let helper = pc.helper(1).unwrap().pid;
    for kept in [shell_pid, stray, mystery, helper] {
        assert!(pc.os().processes.iter().any(|p| p.pid == kept), "{kept} was closed");
    }
    assert_eq!(pc.logged("it is part of Windows and LiMo never closes it").len(), 1);
    assert_eq!(pc.logged("cannot read where its file is").len(), 1);
    // LiMo's own program is not even among the programs running.
    assert!(pc.facts_of(1).unwrap().running.iter().all(|p| p.file_name != "limo.exe"));

    // A process that refuses to end: said, once in a while.
    let stubborn = pc.start(1, renamed);
    pc.os().refuse_end.insert(stubborn);
    pc.run(10);
    let refused = pc.logged("LiMo could not close explorer.exe");
    assert_eq!(refused.len(), 1);
    assert!(refused[0].ends_with(": Win32 error 5: Access is denied."), "{refused:?}");
}

#[test]
fn a_part_of_windows_the_rules_block_is_said_once_for_each_session_and_never_again_while_it_lasts() {
    let mut pc = Pc::new(at(16, 0));
    let asha = pc.add_child("Asha", ASHA, 120);
    let mira = pc.add_child("Mira", MIRA, 120);
    for child in [&asha, &mira] {
        pc.change_rules(child, |rules| rules.blocked_apps = vec!["explorer.exe".into()]);
    }
    let shell = r"C:\Windows\explorer.exe";
    pc.file(shell, Some("EXPLORER.EXE.MUI"), Some("Microsoft Corporation"));
    pc.login(1, ASHA, "asha");
    let shell_pid = pc.start(1, shell);
    pc.step();
    pc.greet(1);
    let spared = "The family's rules block explorer.exe in session 1, but it is part of Windows and LiMo never closes it";

    // The engine orders the close every second, all afternoon: one line,
    // with no count, past the ten minutes after which a quiet line comes
    // back.
    pc.run(QUIET_MS / 1_000 + 60);
    assert_eq!(pc.logged(spared).len(), 1, "{:?}", pc.os().log);
    assert!(pc.logged("since it was last logged").is_empty(), "{:?}", pc.os().log);
    assert!(pc.os().processes.iter().any(|p| p.pid == shell_pid));

    // Asha signs out and in again, in the same session id: said once more.
    pc.os().sessions.retain(|s| s.session_id != 1);
    pc.run(2);
    pc.login(1, ASHA, "asha");
    pc.run(2);
    pc.greet(1);
    pc.run(30);
    assert_eq!(pc.logged(spared).len(), 2, "{:?}", pc.os().log);

    // Mira takes the same session id at once, with no look at the PC
    // between that misses it: once more.
    let session = pc.os().sessions.iter_mut().find(|s| s.session_id == 1).unwrap();
    session.account_id = MIRA.into();
    session.account_name = "mira".into();
    pc.run(2);
    pc.greet(1);
    pc.run(30);
    assert_eq!(pc.logged(spared).len(), 3, "{:?}", pc.os().log);
    assert!(pc.logged("since it was last logged").is_empty(), "{:?}", pc.os().log);
}

#[test]
fn phones_the_books_want_to_hear_from_are_ignored_and_said_once() {
    let mut pc = Pc::new(at(16, 0));
    let asha = pc.add_child("Asha", ASHA, 60);
    let now = pc.now();
    pc.hub().pair_device(&asha, "Phone", DeviceKind::Portable, "android", now).unwrap();
    pc.login(1, ASHA, "asha");
    pc.step();
    pc.greet(1);
    pc.run(20);
    pc.change_rules(&asha, |rules| rules.warning_minutes = vec![10]);
    pc.run(20);
    assert_eq!(pc.logged("no network yet").len(), 1);
}

#[test]
fn the_engines_problems_are_logged_without_repeating_every_second() {
    let mut pc = Pc::new(at(16, 0));
    let asha = pc.add_child("Asha", ASHA, 60);
    pc.login(1, ASHA, "asha");
    pc.step();
    pc.greet(1);
    pc.run(5);
    for _ in 0..3 {
        let device = pc.service.engine().child_on_pc(&asha).unwrap().device_id;
        let now = pc.now();
        pc.hub().remove_device(&device, now).unwrap();
        pc.run(10);
    }
    let problems = pc.logged("The engine met a problem");
    assert_eq!(problems.len(), 1, "{problems:?}");
}
