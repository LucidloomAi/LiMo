//! Several sessions: two children switching, a guardian beside a child, an
//! account that is not linked under both settings, and the PC device paired
//! once for each child.

use limo_agent::{LockWhy, SessionState, Show};
use limo_core::DeviceKind;
use limo_hub::{AccountLink, EventKind, UnlinkedAccounts};

use crate::common::*;

const ASHA: &str = "S-1-5-21-1001";
const RAVI: &str = "S-1-5-21-1002";
const MUM: &str = "S-1-5-21-1000";
const VISITOR: &str = "S-1-5-21-1009";

fn pcs(pc: &mut Pc, child: &str) -> usize {
    pc.hub().devices(child).unwrap().iter().filter(|d| d.kind == DeviceKind::OnMaster).count()
}

#[test]
fn two_children_switching_sessions_count_only_while_in_use() {
    let mut pc = Pc::new(at(16, 0));
    let asha = pc.add_child("Asha", ASHA);
    let ravi = pc.add_child("Ravi", RAVI);
    pc.login(1, ASHA, "asha");
    pc.front(1, made("Minecraft.exe", "Minecraft.exe", "Mojang"));
    pc.login(2, RAVI, "ravi");
    pc.front(2, made("Roblox.exe", "Roblox.exe", "Roblox"));
    pc.set_state(2, SessionState::Disconnected);
    pc.tick();
    pc.run(10 * MIN);
    assert_eq!((pc.counted(&asha), pc.counted(&ravi)), (600, 0));
    // The PC holds no time for Ravi while he is not using it, so his
    // session would show a wait; nobody sees it while it is not in use.
    let Show::Locked { screen } = pc.show(2) else { panic!("{:?}", pc.show(2)) };
    assert_eq!(screen.why, LockWhy::WaitingForTime);

    // Fast user switching: Ravi's session is in use now. He gets time at the
    // first call, with no lock in between.
    pc.set_state(1, SessionState::Disconnected);
    pc.set_state(2, SessionState::InUse);
    pc.run(1);
    assert!(matches!(pc.show(2), Show::TimeLeft { seconds_left: Some(7_200), .. }), "{:?}", pc.show(2));
    pc.run(10 * MIN - 1);
    // The step of the switch counts for neither.
    assert_eq!((pc.counted(&asha), pc.counted(&ravi)), (600, 599));
    // And back.
    pc.set_state(1, SessionState::InUse);
    pc.set_state(2, SessionState::Disconnected);
    pc.run(5 * MIN);
    assert_eq!((pc.counted(&asha), pc.counted(&ravi)), (899, 599));
    // Each child has one device on this PC.
    assert_eq!((pcs(&mut pc, &asha), pcs(&mut pc, &ravi)), (1, 1));
}

#[test]
fn a_guardians_session_beside_a_childs_changes_nothing_for_the_child() {
    let mut pc = Pc::new(at(16, 0));
    let asha = pc.add_child("Asha", ASHA);
    pc.hub().link_account(MUM, "Mum", AccountLink::NotLimited).unwrap();
    pc.login(1, ASHA, "asha");
    pc.front(1, made("Minecraft.exe", "Minecraft.exe", "Mojang"));
    pc.login(2, MUM, "Mum");
    pc.front(2, made("EXCEL.EXE", "Excel.exe", "Microsoft Corporation"));
    pc.set_state(2, SessionState::Disconnected);
    pc.tick();
    pc.run(5 * MIN);
    assert_eq!(pc.counted(&asha), 300);
    let mum = pc.order(2);
    assert_eq!((mum.show, mum.helper_needed, mum.child_id), (Show::Nothing, false, None));

    // Mum takes the PC: nothing counts for Asha, and Mum sees nothing.
    pc.set_state(1, SessionState::Disconnected);
    pc.set_state(2, SessionState::InUse);
    pc.run(30 * MIN);
    assert_eq!(pc.counted(&asha), 300);
    assert_eq!(pc.show(2), Show::Nothing);
    assert!(matches!(pc.show(1), Show::TimeLeft { counting: false, .. }));
    // Mum's account is never paired as a device or noted.
    let now = pc.now();
    assert_eq!(pc.hub().family_today(now).unwrap()[0].devices.len(), 1);
    assert!(!pc.events().iter().any(|e| matches!(e, EventKind::UnlinkedAccountUsed { .. })));
}

#[test]
fn an_account_that_is_not_linked_is_free_or_locked_as_the_guardian_chose() {
    let mut pc = Pc::new(at(16, 0));
    pc.add_child("Asha", ASHA);
    pc.login(5, VISITOR, "Visitor");
    pc.front(5, made("Minecraft.exe", "Minecraft.exe", "Mojang"));
    pc.tick();
    // Free, the default: nothing shown, and the guardian told once a day.
    pc.run(10 * MIN);
    let order = pc.order(5);
    assert_eq!((order.show, order.helper_needed, order.child_id), (Show::Nothing, false, None));
    let unlinked = |pc: &mut Pc| {
        pc.events().into_iter().filter(|e| matches!(e, EventKind::UnlinkedAccountUsed { .. })).count()
    };
    assert_eq!(unlinked(&mut pc), 1);
    assert!(pc.events().contains(&EventKind::UnlinkedAccountUsed {
        account_id: VISITOR.into(),
        account_name: "Visitor".into(),
        day: day_of(at(16, 0)),
    }));
    // Not counted for anyone.
    assert_eq!(pc.engine().children_on_pc(), Vec::<String>::new());

    // Locked: from the next call.
    pc.hub().set_unlinked_accounts(UnlinkedAccounts::Locked).unwrap();
    pc.run(1);
    let order = pc.order(5);
    assert!(order.helper_needed && order.close.is_empty());
    let Show::Locked { screen } = order.show else { panic!("not locked") };
    assert_eq!(screen.why, LockWhy::AccountNotLinked);
    assert!(!screen.can_ask_for_time && !screen.can_use_code && !screen.can_use_pin);
    assert_eq!(screen.message, "This account is not set up in LiMo yet. A guardian can set it up.");
    pc.run(5 * MIN);
    assert_eq!(unlinked(&mut pc), 1);

    // The next day it is noted again, once; a session that is not in use is
    // not noted.
    pc.advance(24 * HOUR as u64 * 1_000);
    pc.set_state(5, SessionState::Locked);
    pc.run(5);
    assert_eq!(unlinked(&mut pc), 1);
    pc.set_state(5, SessionState::InUse);
    pc.run(5);
    assert_eq!(unlinked(&mut pc), 2);

    // Linked by the guardian: from the next call it is Asha's.
    let asha = pc.hub().children().unwrap()[0].child_id.clone();
    pc.hub().link_account(VISITOR, "Visitor", AccountLink::Child { child_id: asha.clone() }).unwrap();
    pc.run(1);
    assert_eq!(pc.order(5).child_id, Some(asha));
}

#[test]
fn the_pc_device_is_paired_the_first_time_a_child_is_seen_and_never_twice() {
    let mut pc = Pc::new(at(16, 0));
    let asha = pc.add_child("Asha", ASHA);
    pc.hub().link_account("S-1-5-21-1011", "asha2", AccountLink::Child { child_id: asha.clone() }).unwrap();
    pc.tick();
    assert_eq!(pcs(&mut pc, &asha), 0);
    // Two accounts of the same child, both logged in: one device, one count.
    pc.login(1, ASHA, "asha");
    pc.login(2, "S-1-5-21-1011", "asha2");
    pc.set_state(2, SessionState::Disconnected);
    pc.tick();
    assert_eq!(pcs(&mut pc, &asha), 1);
    for _ in 0..5 {
        pc.run(30);
        pc.restart();
    }
    pc.run(1);
    let counted = pc.counted(&asha);
    assert_eq!(counted, 151);
    pc.logout(1);
    pc.logout(2);
    pc.run(10);
    pc.login(1, ASHA, "asha");
    pc.run(10);
    assert_eq!(pc.counted(&asha), counted + 9);
    assert_eq!(pcs(&mut pc, &asha), 1);
    let paired = pc.events().into_iter().filter(|e| matches!(e, EventKind::DevicePaired { .. })).count();
    assert_eq!(paired, 1);

    // The guardian removes the PC device: the next call pairs a new one, once.
    let old = pc.view(&asha).device_id;
    let now = pc.now();
    pc.hub().remove_device(&old, now).unwrap();
    pc.allow_problems = true;
    pc.run(3);
    pc.allow_problems = false;
    pc.run(3);
    assert_eq!(pcs(&mut pc, &asha), 1);
    assert_ne!(pc.view(&asha).device_id, old);
    // The time already used stays counted for the day.
    pc.run(MIN);
    let used = pc.master_used(&asha);
    assert!(used >= 100, "{used}");

    // The child removed: the account is not linked any more.
    pc.hub().remove_child(&asha).unwrap();
    pc.run(2);
    assert_eq!(pc.order(1).child_id, None);
    assert_eq!(pc.show(1), Show::Nothing);
}
