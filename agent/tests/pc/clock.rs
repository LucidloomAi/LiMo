//! The clock: the wall clock set back and forward, a time zone change, a PC
//! asleep for hours, and a service held up. Counted time comes from the
//! steady count alone; the guardian is told of each jump.

use limo_agent::{LockWhy, SessionState, Show};
use limo_core::{BlockedWindow, Weekday};
use limo_hub::EventKind;

use crate::common::*;

const ASHA: &str = "S-1-5-21-1001";

fn asha_at(start: i64) -> (Pc, String) {
    let mut pc = Pc::new(start);
    let asha = pc.add_child("Asha", ASHA);
    pc.login(1, ASHA, "asha");
    pc.front(1, made("Minecraft.exe", "Minecraft.exe", "Mojang"));
    pc.tick();
    (pc, asha)
}

fn clock_events(pc: &mut Pc) -> Vec<EventKind> {
    pc.events()
        .into_iter()
        .filter(|e| matches!(e, EventKind::ClockJumped { .. } | EventKind::TimeZoneChanged { .. }))
        .collect()
}

#[test]
fn the_wall_clock_set_back_and_forward_neither_adds_nor_removes_counted_time() {
    let (mut pc, asha) = asha_at(at(14, 0));
    pc.run(10 * MIN);
    assert_eq!(pc.counted(&asha), 600);

    // Back an hour: ten more minutes of use count as ten minutes.
    pc.skew -= HOUR;
    pc.run(10 * MIN);
    assert_eq!(pc.counted(&asha), 1_200);
    // Forward two hours: the same.
    pc.skew += 2 * HOUR;
    pc.run(10 * MIN);
    assert_eq!(pc.counted(&asha), 1_800);
    assert_eq!(clock_events(&mut pc), vec![EventKind::ClockJumped { by_secs: 7_200 }, EventKind::ClockJumped {
        by_secs: -3_600
    }]);
    // The time left followed the use only.
    assert!(matches!(pc.show(1), Show::TimeLeft { seconds_left: Some(5_400), .. }));
    pc.logout(1);
    pc.run(1);
    assert_eq!(pc.master_used(&asha), 1_800);
}

#[test]
fn a_clock_set_forward_over_midnight_starts_the_day_the_clock_says_and_loses_nothing() {
    let (mut pc, asha) = asha_at(at(23, 40));
    pc.run(10 * MIN);
    // At 23:50 the clock is set forward twenty minutes: it is 00:10 on
    // Thursday by the wall clock, which says what day it is.
    pc.skew += 20 * MIN;
    pc.run(10 * MIN);
    assert_eq!(pc.counted(&asha), 10 * 60);
    assert_eq!(pc.view(&asha).day, day_of(at(12, 0)) + 1);
    pc.logout(1);
    pc.run(1);
    let wednesday = day_of(at(12, 0));
    let wed = pc.hub().usage(&asha, wednesday, wednesday).unwrap().counted_secs;
    let thu = pc.hub().usage(&asha, wednesday + 1, wednesday + 1).unwrap().counted_secs;
    // Twenty minutes of use in all, by the steady count; none gained or lost.
    assert_eq!((wed, thu), (600, 600));
}

#[test]
fn a_time_zone_change_is_noted_and_the_home_zone_still_rules() {
    let (mut pc, asha) = asha_at(at(20, 0));
    pc.change_rules(&asha, |r| {
        r.blocked_hours = vec![BlockedWindow { days: Weekday::ALL.to_vec(), start_minute: 21 * 60, end_minute: 7 * 60 }]
    });
    pc.run(MIN);
    // The PC's zone is changed to UTC (a child's account can do that on
    // Windows): nothing counted changes, and bedtime is still 21:00 at home.
    pc.offset = 0;
    pc.run(MIN);
    assert_eq!(clock_events(&mut pc), vec![EventKind::TimeZoneChanged { from_offset_secs: IST, to_offset_secs: 0 }]);
    assert_eq!(pc.counted(&asha), 120);
    pc.run_until(at(21, 0));
    let Show::Locked { screen } = pc.show(1) else { panic!("{:?}", pc.show(1)) };
    assert_eq!(screen.why, LockWhy::BlockedHours);
    assert_eq!(pc.counted(&asha), 3_600);
}

#[test]
fn a_pc_asleep_for_hours_and_a_service_held_up_count_nothing_for_the_gap() {
    let (mut pc, asha) = asha_at(at(9, 0));
    pc.run(10 * MIN);
    assert_eq!(pc.counted(&asha), 600);

    // Asleep three hours, with a steady count that goes on through sleep:
    // a long gap, not counted, and not a clock jump.
    pc.sleep(3 * HOUR, true);
    pc.tick();
    assert_eq!(pc.counted(&asha), 600);
    pc.set_state(1, SessionState::InUse);
    pc.typing(1, true);
    pc.run(MIN);
    assert_eq!(pc.counted(&asha), 600 + 59);
    assert!(clock_events(&mut pc).is_empty());

    // Asleep two hours with a steady count that stops during sleep: the
    // wall clock seems to jump forward. Not counted either; noted.
    pc.sleep(2 * HOUR, false);
    pc.tick();
    pc.set_state(1, SessionState::InUse);
    pc.run(MIN);
    assert_eq!(pc.counted(&asha), 600 + 59 + 59);
    assert_eq!(clock_events(&mut pc), vec![EventKind::ClockJumped { by_secs: 2 * HOUR }]);

    // The service is held up for a minute with Asha at the keyboard: the
    // long step is not counted, and counting goes on after it.
    let before = pc.counted(&asha);
    pc.advance(60_000);
    pc.tick();
    assert_eq!(pc.counted(&asha), before);
    pc.run(10);
    assert_eq!(pc.counted(&asha), before + 10);
    // A step just within the limit counts in full.
    pc.advance(9_000);
    pc.tick();
    assert_eq!(pc.counted(&asha), before + 19);
}
