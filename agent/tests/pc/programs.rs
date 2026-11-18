//! Blocked programs: closed whenever they run, renamed or not, noted once a
//! day; and what still gets around it.

use limo_agent::{CloseOrder, Show};
use limo_hub::EventKind;

use crate::common::*;

const ASHA: &str = "S-1-5-21-1001";
const RAVI: &str = "S-1-5-21-1002";

fn closes(pc: &Pc, session_id: u32) -> Vec<CloseOrder> {
    pc.order(session_id).close
}

fn noted(pc: &mut Pc) -> Vec<(String, i64)> {
    pc.events()
        .into_iter()
        .filter_map(|e| match e {
            EventKind::BlockedProgramClosed { program, day, .. } => Some((program, day)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_blocked_program_is_closed_whenever_it_runs_even_renamed() {
    let mut pc = Pc::new(at(8, 0));
    let asha = pc.add_child("Asha", ASHA);
    pc.change_rules(&asha, |r| r.blocked_apps = vec!["RobloxPlayerBeta.exe".into()]);
    pc.login(1, ASHA, "asha");
    pc.front(1, made("Minecraft.exe", "Minecraft.exe", "Mojang"));
    pc.tick();
    assert!(closes(&pc, 1).is_empty());

    // Roblox starts in the background, renamed. It is ordered closed at
    // every call it is seen, and the guardian is told once.
    let renamed = made("homework.exe", "RobloxPlayerBeta.exe", "Roblox Corporation");
    pc.session(1).facts.running.push(renamed.clone());
    for _ in 0..30 {
        pc.run(1);
        let close = closes(&pc, 1);
        assert_eq!(close.len(), 1);
        assert_eq!(close[0].program, renamed);
        assert_eq!(close[0].reason, "homework.exe is blocked by your family's rules, so LiMo closed it.");
    }
    // Closed; started again under another name, in front this time: closed
    // again, and its time does not count. The PC is not locked for it.
    pc.session(1).facts.running.clear();
    pc.run(1);
    assert!(closes(&pc, 1).is_empty());
    let before = pc.counted(&asha);
    let again = made("calc.exe", "RobloxPlayerBeta.exe", "Roblox Corporation");
    pc.front(1, again.clone());
    pc.run(60);
    assert_eq!(closes(&pc, 1).iter().map(|c| c.program.clone()).collect::<Vec<_>>(), vec![again]);
    assert!(matches!(pc.show(1), Show::TimeLeft { counting: false, .. }));
    // The step in which it came to the front was Minecraft's.
    assert_eq!(pc.counted(&asha), before + 1);
    let today = day_of(pc.now());
    assert_eq!(noted(&mut pc), vec![("robloxplayerbeta.exe".to_string(), today)]);

    // A program with no name recorded inside it is known only by its file
    // name: under its own name it is closed, renamed it is not. This is the
    // hole the README names.
    let bare = program("RobloxPlayerBeta.exe");
    let bare_renamed = program("notes.exe");
    pc.session(1).facts.running = vec![bare.clone(), bare_renamed];
    pc.front(1, made("Minecraft.exe", "Minecraft.exe", "Mojang"));
    pc.run(1);
    let close: Vec<_> = closes(&pc, 1).into_iter().map(|c| c.program).collect();
    assert_eq!(close, vec![bare]);
    // And a program that only has the blocked name as its file name is
    // closed too: either name is enough to close.
    pc.session(1).facts.running = vec![made("RobloxPlayerBeta.exe", "Notepad.exe", "Microsoft")];
    pc.run(1);
    assert_eq!(closes(&pc, 1).len(), 1);

    // The guardian is told it under the name the rules gave.
    assert_eq!(noted(&mut pc), vec![("robloxplayerbeta.exe".to_string(), today)]);

    // The next day it is noted once more.
    pc.session(1).facts.running = vec![renamed];
    pc.advance(24 * HOUR as u64 * 1_000);
    pc.tick();
    pc.run(5);
    assert_eq!(noted(&mut pc), vec![
        ("robloxplayerbeta.exe".to_string(), today + 1),
        ("robloxplayerbeta.exe".to_string(), today)
    ]);
}

#[test]
fn a_childs_rules_close_nothing_in_another_session() {
    let mut pc = Pc::new(at(8, 0));
    let asha = pc.add_child("Asha", ASHA);
    pc.add_child("Ravi", RAVI);
    pc.hub().link_account("S-1-5-21-1000", "Mum", limo_hub::AccountLink::NotLimited).unwrap();
    pc.change_rules(&asha, |r| r.blocked_apps = vec!["fortnite.exe".into()]);
    let fortnite = made("Fortnite.exe", "Fortnite.exe", "Epic Games");
    pc.login(1, ASHA, "asha");
    pc.login(2, RAVI, "ravi");
    pc.login(3, "S-1-5-21-1000", "Mum");
    for session in [1, 2, 3] {
        pc.session(session).facts.running.push(fortnite.clone());
    }
    pc.set_state(1, limo_agent::SessionState::Disconnected);
    pc.set_state(2, limo_agent::SessionState::Disconnected);
    pc.tick();
    // Asha's, even disconnected; not Ravi's, whose rules allow it; not Mum's.
    assert_eq!(closes(&pc, 1).len(), 1);
    assert!(closes(&pc, 2).is_empty() && closes(&pc, 3).is_empty());
    assert!(!pc.order(3).helper_needed);
}
