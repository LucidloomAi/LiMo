//! One budget: the PC and a phone of the same child, driven with its own
//! device state against the same family file, never pass the day's budget
//! plus extra time, with the phone in reach and away.

use limo_agent::{LockWhy, Show};

use crate::common::*;

const ASHA: &str = "S-1-5-21-1001";

fn locked_why(pc: &Pc) -> Option<LockWhy> {
    match pc.show(1) {
        Show::Locked { screen } => Some(screen.why),
        _ => None,
    }
}

/// The worked day of the core's README, with the real engine on the PC:
/// two hours, an away share of half, ten minute leases.
#[test]
fn the_worked_day_with_the_phone_away_then_home() {
    let mut pc = Pc::new(at(7, 0));
    let asha = pc.add_child("Asha", ASHA);
    let phone = pc.pair_phone(&asha);
    pc.tick();

    // 07:30 the phone, at home, takes a lease; 08:00 it leaves for school.
    pc.advance(30 * MIN as u64 * 1_000);
    let now = pc.now();
    let mut p = std::mem::take(&mut pc.phones);
    p[phone].exchange(pc.hub(), now, true);
    pc.phones = p;
    assert_eq!(pc.phones[phone].state.holding_at(now).lease_total_secs, 10 * 60);
    pc.advance(30 * MIN as u64 * 1_000);
    pc.phone_home(phone, false);
    // 10:00 to 10:45 at school, out of reach: its lease, then 35 minutes of
    // its away allowance.
    pc.phones[phone].state.record_use("com.google.android.youtube", at(10, 0), at(10, 45));
    assert_eq!(pc.phones[phone].used_today(at(10, 45)), 45 * 60);

    // 16:00 the PC: it takes leases from the fifty free minutes, then waits
    // for the rest, which the phone at school holds.
    pc.advance(u64::try_from(at(16, 0) - pc.now()).unwrap() * 1_000);
    pc.login(1, ASHA, "asha");
    pc.front(1, made("Minecraft.exe", "Minecraft.exe", "Mojang"));
    pc.tick();
    pc.run_until(at(16, 50));
    assert_eq!(locked_why(&pc), Some(LockWhy::HeldByAnotherDevice));
    assert_eq!(pc.counted(&asha), 50 * 60);
    let Show::Locked { screen } = pc.show(1) else { panic!() };
    assert_eq!(
        screen.message,
        "The rest of today's time is on another device. It comes here when that device is back home."
    );
    // The book wants the phone to report: the service would prod it.
    pc.run(MIN);
    assert!(pc.orders.prod_devices.contains(&pc.phones[phone].id));
    pc.run_until(at(17, 0));
    assert_eq!(pc.counted(&asha), 50 * 60);

    // 17:00 the phone comes home and reports its 45 minutes. The PC hears of
    // it at its next call, and goes on; the phone at home gives back a
    // little more each time the PC runs low, until the day is used up.
    pc.phone_home(phone, true);
    pc.run(1);
    assert_eq!(locked_why(&pc), None);
    pc.run_until(at(17, 40));
    assert_eq!(locked_why(&pc), Some(LockWhy::TimeUsedUp));
    assert_eq!(pc.counted(&asha), 75 * 60);
    assert_eq!(pc.master_used(&asha), 120 * 60);
    let now = pc.now();
    assert_eq!(pc.phones[phone].used_today(now), 45 * 60);
    // The phone has nothing left: the PC used it at home.
    assert_eq!(pc.phones[phone].state.holding_at(now).in_hand_secs(), 0);
}

#[test]
fn the_pc_and_a_phone_at_home_used_at_once_share_one_budget() {
    let mut pc = Pc::new(at(15, 0));
    let asha = pc.add_child("Asha", ASHA);
    let phone = pc.pair_phone(&asha);
    pc.login(1, ASHA, "asha");
    pc.front(1, made("Minecraft.exe", "Minecraft.exe", "Mojang"));
    pc.tick();
    pc.phones[phone].using = Some("com.whatsapp".into());
    // Both in use until the day runs out; every call checks the bound.
    pc.run_until(at(16, 15));
    assert_eq!(locked_why(&pc), Some(LockWhy::TimeUsedUp));
    pc.phones[phone].using = None;
    pc.run(2 * MIN);
    let now = pc.now();
    let pc_used = pc.counted(&asha);
    let phone_used = pc.phones[phone].used_today(now);
    assert_eq!(pc_used + phone_used, 120 * 60, "PC {pc_used}, phone {phone_used}");
    assert_eq!(pc.master_used(&asha), 120 * 60);
    // Extra time given at the master reaches the PC at its next call.
    pc.hub().give_extra(&asha, 10, "tap", now).unwrap();
    pc.run(1);
    assert_eq!(locked_why(&pc), None);
}
