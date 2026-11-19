//! The facts the service gathers: from Windows, and from each session's
//! helper only while its report is fresh.

use limo_agent::{LockWhy, Program, SessionState, Show};
use limo_hub::UnlinkedAccounts;
use limo_windows::os::{ProcessSeen, SessionSeen};

use crate::common::*;

const ASHA: &str = "S-1-5-21-1001";
const MIRA: &str = "S-1-5-21-1002";
const ADMIN: &str = "S-1-5-21-1010";
const VISITOR: &str = "S-1-5-21-1009";
const GAME: &str = r"C:\Games\Blocks\blocks.exe";

fn blocks() -> Program {
    Program { file_name: "blocks.exe".into(), original_name: Some("Blocks.exe".into()), maker: Some("Blocks Ltd".into()) }
}

#[test]
fn with_no_fresh_report_input_counts_as_just_now_so_a_missing_helper_gives_no_free_time() {
    let mut pc = Pc::new(at(16, 0));
    let asha = pc.add_child("Asha", ASHA, 60);
    pc.login(1, ASHA, "asha");
    pc.file(GAME, Some("Blocks.exe"), Some("Blocks Ltd"));
    pc.start(1, GAME);
    // The helper is started, and never says a word.
    pc.run(61);
    let facts = pc.facts_of(1).unwrap();
    assert_eq!((facts.idle_secs, facts.full_screen, facts.in_front), (0, false, None));
    assert!(pc.counted(&asha) >= 58, "{}", pc.counted(&asha));

    // A report that nothing was touched for ten minutes pauses the count,
    // but only while it is fresh.
    pc.greet(1);
    pc.report(1, 600_000, None, false);
    let before = pc.counted(&asha);
    pc.run(2);
    assert_eq!(pc.facts_of(1).unwrap().idle_secs, 600);
    let during = pc.counted(&asha);
    assert!(during <= before + 2, "{before} then {during}");
    pc.run(10);
    assert_eq!(pc.facts_of(1).unwrap().idle_secs, 0);
    assert!(pc.counted(&asha) >= during + 5, "{during} then {}", pc.counted(&asha));
}

#[test]
fn a_fresh_report_gives_the_idle_time_full_screen_and_the_program_in_front() {
    let mut pc = Pc::new(at(16, 0));
    pc.add_child("Asha", ASHA, 60);
    pc.login(1, ASHA, "asha");
    pc.file(GAME, Some("Blocks.exe"), Some("Blocks Ltd"));
    let game = pc.start(1, GAME);
    pc.run(1);
    pc.greet(1);
    pc.report(1, 4_200, Some(game), true);
    pc.run(1);
    let facts = pc.facts_of(1).unwrap();
    assert_eq!((facts.idle_secs, facts.full_screen), (4, true));
    assert_eq!(facts.in_front, Some(blocks()));
    assert_eq!(facts.running, vec![blocks()]);
    // LiMo's own helper runs in the session, and is not among its programs.
    assert_eq!(pc.helpers_running(1), 1);
    assert!(pc.os().processes.iter().any(|p| p.session_id == 1 && p.path == OWN_EXE));
}

#[test]
fn the_program_in_front_is_found_by_the_service_itself_and_only_in_that_session() {
    let mut pc = Pc::new(at(16, 0));
    pc.add_child("Asha", ASHA, 60);
    pc.add_child("Mira", MIRA, 60);
    pc.login(1, ASHA, "asha");
    pc.login(2, MIRA, "mira");
    pc.set_state(2, SessionState::Disconnected);
    pc.file(GAME, Some("Blocks.exe"), Some("Blocks Ltd"));
    let miras_game = pc.start(2, GAME);
    pc.run(1);
    pc.greet(1);

    // A process of another session: nothing in front, and said in the log.
    pc.report(1, 0, Some(miras_game), false);
    pc.run(1);
    assert_eq!(pc.facts_of(1).unwrap().in_front, None);
    assert_eq!(pc.logged("named a process of another session").len(), 1);

    // The helper's own window in front: nothing in front.
    let helper = pc.helper(1).unwrap().pid;
    pc.report(1, 0, Some(helper), false);
    pc.run(1);
    assert_eq!(pc.facts_of(1).unwrap().in_front, None);

    // A process that started after the list was taken is found by its id,
    // in the right session.
    let late = pc.start(1, GAME);
    pc.os().not_listed.insert(late);
    pc.report(1, 0, Some(late), false);
    pc.run(1);
    assert_eq!(pc.facts_of(1).unwrap().in_front, Some(blocks()));

    // A process id that is nothing: nothing in front.
    pc.report(1, 0, Some(99_999), false);
    pc.run(1);
    assert_eq!(pc.facts_of(1).unwrap().in_front, None);
}

#[test]
fn a_files_version_resource_is_read_once_until_the_file_is_written() {
    let mut pc = Pc::new(at(16, 0));
    pc.add_child("Asha", ASHA, 60);
    pc.login(1, ASHA, "asha");
    pc.file(GAME, Some("Blocks.exe"), Some("Blocks Ltd"));
    pc.start(1, GAME);
    pc.start(1, GAME);
    pc.run(30);
    let reads = |pc: &mut Pc| pc.os().version_reads.iter().filter(|p| p.as_str() == GAME).count();
    assert_eq!(reads(&mut pc), 1);
    assert_eq!(pc.facts_of(1).unwrap().running, vec![blocks()]);
    // The file is written (an update): the next process of it reads it again.
    pc.os().stamps.insert(GAME.into(), 2);
    pc.os().versions.get_mut(GAME).unwrap().maker = Some("Blocks Studio".into());
    pc.start(1, GAME);
    pc.run(5);
    assert_eq!(reads(&mut pc), 2);
    let running = pc.facts_of(1).unwrap().running;
    assert_eq!(running.len(), 2, "{running:?}");
    assert!(running.iter().any(|p| p.maker.as_deref() == Some("Blocks Studio")));
}

#[test]
fn session_zero_is_never_in_the_facts_and_an_administrator_is_marked() {
    let mut pc = Pc::new(at(16, 0));
    pc.add_child("Asha", ASHA, 60);
    pc.os().sessions.push(SessionSeen {
        session_id: 0,
        account_id: "S-1-5-18".into(),
        account_name: "SYSTEM".into(),
        state: SessionState::InUse,
        is_administrator: true,
    });
    pc.login(1, ASHA, "asha");
    pc.login(3, ADMIN, "admin");
    pc.os().sessions.last_mut().unwrap().is_administrator = true;
    pc.set_state(3, SessionState::Disconnected);
    pc.file(GAME, Some("Blocks.exe"), Some("Blocks Ltd"));
    let pid = pc.os().new_pid();
    pc.os().processes.push(ProcessSeen { pid, session_id: 0, started: 9, name: "blocks.exe".into(), path: GAME.into() });
    pc.run(2);
    let facts = pc.service.last_facts().unwrap().clone();
    assert_eq!(facts.sessions.iter().map(|s| s.session_id).collect::<Vec<_>>(), vec![1, 3]);
    assert!(!facts.sessions[0].is_administrator && facts.sessions[1].is_administrator);
    assert!(facts.sessions.iter().all(|s| s.running.is_empty()));
}

#[test]
fn an_administrators_account_that_is_not_linked_is_never_locked() {
    let mut pc = Pc::new(at(16, 0));
    pc.add_child("Asha", ASHA, 60);
    pc.hub().set_unlinked_accounts(UnlinkedAccounts::Locked).unwrap();
    pc.login(3, ADMIN, "admin");
    pc.os().sessions.last_mut().unwrap().is_administrator = true;
    pc.login(5, VISITOR, "visitor");
    pc.set_state(5, SessionState::Disconnected);
    pc.run(5);
    // The administrator: nothing shown, and no helper at all.
    assert_eq!(pc.service.session_view(3).unwrap().show, Show::Nothing);
    assert_eq!(pc.helpers_started(3), 0);
    // A standard account that is not linked: locked, with its helper.
    let Some((_, None, Show::Locked { screen })) = pc.last_show(5) else { panic!("{:?}", pc.last_show(5)) };
    assert_eq!(screen.why, LockWhy::AccountNotLinked);
    assert_eq!(screen.title, "This account is not set up yet.");
}
