//! A whole day in the life of a family, with the guardian's views checked
//! at every stage. Asha has the family PC and a phone; Ravi has nothing yet.
//! Two hours a day, half of it as the phone's away share, ten minute leases,
//! bedtime from 21:00 to 07:00.

use std::collections::BTreeSet;

use limo_core::{
    BlockedWindow, DeviceKind, ExtraSource, GuardianMode, LockReason, PinAction, RequestAnswer, RequestReply,
    TryOutcome, Weekday,
};
use limo_hub::{AccountLink, AccountStanding, AgeBand, ChildToday, EventKind, Hub};

use crate::common::*;

const M: u64 = 60;

fn view(hub: &Hub, child_id: &str, now: i64) -> ChildToday {
    hub.child_today(child_id, now).unwrap()
}

fn set(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|s| s.to_string()).collect()
}

#[test]
fn a_whole_day_in_the_life_of_a_family() {
    let place = place();
    let mut hub = Hub::open(&place.path).unwrap();
    assert!(!hub.has_family().unwrap());
    hub.create_family(PASSWORD, at(7, 0)).unwrap();
    let asha = hub.add_child("Asha", AgeBand::TenToTwelve, IST, at(7, 0)).unwrap();
    let ravi = hub.add_child("Ravi", AgeBand::SixToNine, IST, at(7, 0)).unwrap();
    assert_eq!(hub.children().unwrap(), vec![asha.clone(), ravi.clone()]);
    let a = asha.child_id.as_str();

    // 07:05 Bedtime, an always allowed app and a blocked one.
    let mut rules = hub.rules(a).unwrap();
    assert_eq!(rules.version, 1);
    rules.blocked_hours = vec![BlockedWindow { days: Weekday::ALL.to_vec(), start_minute: 21 * 60, end_minute: 7 * 60 }];
    rules.always_allowed_apps = vec!["phone-calls".into()];
    rules.blocked_apps = vec!["casino".into()];
    let taken = hub.replace_rules(a, rules, at(7, 5)).unwrap();
    assert_eq!(taken.version, 2);
    assert_eq!(hub.rules(a).unwrap(), taken);

    // 07:10 The PC and the phone are paired.
    let mut pc = pair(&mut hub, a, "Family PC", DeviceKind::OnMaster, at(7, 10));
    let mut phone = pair(&mut hub, a, "Asha's phone", DeviceKind::Portable, at(7, 10));
    assert_eq!(pc.state.rules_version(), 2);
    let v = view(&hub, a, at(7, 10));
    assert_eq!((v.budget_secs, v.extra_secs, v.used_secs, v.left_secs), (Some(120 * M), 0, 0, Some(120 * M)));
    assert_eq!(v.guardian, GuardianMode::Nothing);
    assert_eq!(v.devices.len(), 2);
    assert_eq!((v.devices[0].name.as_str(), v.devices[0].holds_secs), ("Family PC", Some(0)));
    // The phone holds its away allowance from the start: half the day.
    assert_eq!((v.devices[1].name.as_str(), v.devices[1].holds_secs), ("Asha's phone", Some(60 * M)));
    assert!(v.devices.iter().all(|d| d.last_heard_at == Some(at(7, 10)) && !d.in_use));
    assert_eq!(hub.wanted_reports(at(7, 10)).unwrap(), Vec::<String>::new());

    // 08:00 to 08:10 The PC plays a game.
    exchange(&mut hub, &mut pc, at(8, 0), true);
    use_app(&mut hub, &mut pc, "game", at(8, 0), at(8, 10));
    let v = view(&hub, a, at(8, 10));
    assert_eq!((v.used_secs, v.left_secs), (10 * M, Some(110 * M)));
    assert!(v.devices[0].in_use && !v.devices[1].in_use);
    assert_eq!(v.devices[0].used_secs, 10 * M);
    assert_eq!(v.devices[0].holds_secs, Some(10 * M));

    // 09:00 to 09:20 The phone, at home: its lease, then its away time.
    exchange(&mut hub, &mut phone, at(9, 0), true);
    use_app(&mut hub, &mut phone, "video", at(9, 0), at(9, 20));
    let v = view(&hub, a, at(9, 20));
    assert_eq!((v.used_secs, v.left_secs), (30 * M, Some(90 * M)));
    assert!(!v.devices[0].in_use, "the PC was last heard from at 08:10");
    assert!(v.devices[1].in_use);
    assert_eq!(v.devices[1].holds_secs, Some(60 * M));

    // 10:00 Fifteen minutes from the guardian. The same tap twice gives once.
    let given = hub.give_extra(a, 15, "tap-1", at(10, 0)).unwrap();
    assert!(given.given_now);
    assert!(!hub.give_extra(a, 15, "tap-1", at(10, 0)).unwrap().given_now);
    let extra = hub.extra_time_today(a, at(10, 0)).unwrap();
    assert_eq!(extra.len(), 1);
    assert_eq!((extra[0].id.as_str(), extra[0].secs, &extra[0].source), ("tap/tap-1", 15 * M, &ExtraSource::Guardian));
    let v = view(&hub, a, at(10, 0));
    assert_eq!((v.extra_secs, v.left_secs), (15 * M, Some(105 * M)));

    // 11:00 Asha asks for twenty minutes for a project; the guardian gives 25.
    phone.state.ask_for_time(20, "Science project", at(11, 0)).unwrap();
    exchange(&mut hub, &mut phone, at(11, 0), false);
    let waiting = hub.waiting_requests(at(11, 0)).unwrap();
    assert_eq!(waiting.len(), 1);
    assert_eq!((waiting[0].child_name.as_str(), waiting[0].device_name.as_str()), ("Asha", "Asha's phone"));
    assert_eq!((waiting[0].request.request.minutes, waiting[0].request.request.reason.as_str()), (20, "Science project"));
    assert_eq!(view(&hub, a, at(11, 0)).waiting_requests, 1);
    let request_id = waiting[0].request.request.id.clone();
    let answered = hub.answer_request(a, &request_id, RequestReply::Grant { minutes: 25 }, at(11, 5)).unwrap();
    assert_eq!(answered.answer, RequestAnswer::Granted { minutes: 25 });
    let v = view(&hub, a, at(11, 5));
    assert_eq!((v.extra_secs, v.waiting_requests), (40 * M, 0));
    assert!(hub.waiting_requests(at(11, 5)).unwrap().is_empty());
    // Both devices are asked to report, to hear the answer.
    assert_eq!(hub.wanted_reports(at(11, 5)).unwrap().into_iter().collect::<BTreeSet<_>>(), set(&[&pc.id, &phone.id]));
    exchange(&mut hub, &mut phone, at(11, 6), false);
    let seen = phone.state.requests_today(at(11, 6));
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].answer, RequestAnswer::Granted { minutes: 25 });

    // 12:00 A one time code for the phone, read out over a call; then another.
    let code = hub.make_code(&phone.id, 30, at(12, 0)).unwrap();
    assert_eq!((code.minutes, code.counter, code.day), (30, 0, day_of(at(12, 0))));
    assert!(!format!("{code:?}").contains(&code.digits));
    assert_eq!(hub.make_code(&phone.id, 15, at(12, 1)).unwrap().counter, 1);
    // The phone, away from home, takes it with no network.
    assert!(matches!(phone.state.try_code(&code.shown, at(12, 5)), TryOutcome::Added { .. }));
    exchange(&mut hub, &mut phone, at(13, 0), false);
    let extra = hub.extra_time_today(a, at(13, 0)).unwrap();
    let sources: Vec<ExtraSource> = extra.iter().map(|e| e.source.clone()).collect();
    assert_eq!(sources, vec![
        ExtraSource::Guardian,
        ExtraSource::Request { request_id: request_id.clone() },
        ExtraSource::Code { device_id: phone.id.clone(), selector: 10 },
    ]);
    let v = view(&hub, a, at(13, 0));
    assert_eq!((v.extra_secs, v.devices[1].added_secs), (70 * M, 30 * M));
    assert!(!v.devices[1].in_use, "nothing was used since 09:20");

    // 14:00 The guardian PIN, for every child at once.
    hub.set_pin("482915", at(14, 0)).unwrap();
    assert!(hub.has_pin().unwrap());
    assert_eq!(hub.wanted_reports(at(14, 0)).unwrap().into_iter().collect::<BTreeSet<_>>(), set(&[&pc.id, &phone.id]));
    exchange(&mut hub, &mut pc, at(14, 1), false);
    exchange(&mut hub, &mut phone, at(14, 2), false);
    assert!(pc.state.has_pin() && phone.state.has_pin());
    // The master learns that a device took the news from its next report.
    assert_eq!(hub.wanted_reports(at(14, 2)).unwrap().len(), 2);
    exchange(&mut hub, &mut pc, at(14, 3), false);
    exchange(&mut hub, &mut phone, at(14, 3), false);
    assert_eq!(hub.wanted_reports(at(14, 3)).unwrap(), Vec::<String>::new());
    // Two wrong guesses on the phone, seen by the guardian.
    for n in 1..=2 {
        let outcome = phone.state.try_pin("000000", PinAction::AddMinutes { minutes: 10 }, at(14, 10)).unwrap();
        assert_eq!(outcome, TryOutcome::Wrong { wrong_guesses: n, wait_until: None });
    }
    exchange(&mut hub, &mut phone, at(14, 11), false);
    assert_eq!(view(&hub, a, at(14, 11)).devices[1].wrong_guesses, 2);
    // Then the right PIN adds ten minutes on the phone.
    let outcome = phone.state.try_pin("482915", PinAction::AddMinutes { minutes: 10 }, at(14, 20)).unwrap();
    assert!(matches!(outcome, TryOutcome::Added { .. }));
    exchange(&mut hub, &mut phone, at(14, 21), false);
    let v = view(&hub, a, at(14, 21));
    assert_eq!((v.devices[1].wrong_guesses, v.devices[1].added_secs, v.extra_secs), (0, 40 * M, 80 * M));
    assert_eq!(v.left_secs, Some((120 + 80 - 30) * M));

    // 15:00 Locked from the master until lifted; 15:30 lifted.
    hub.lock_now(a, None, at(15, 0)).unwrap();
    assert_eq!(view(&hub, a, at(15, 0)).guardian, GuardianMode::Lock { until: None });
    exchange(&mut hub, &mut pc, at(15, 1), false);
    let d = pc.state.decide(at(15, 1), Some("game"));
    assert_eq!(d.lock.map(|l| l.reason), Some(LockReason::GuardianLock));
    assert!(pc.state.decide(at(15, 1), Some("phone-calls")).allowed);
    hub.lift(a, at(15, 30)).unwrap();
    assert_eq!(view(&hub, a, at(15, 30)).guardian, GuardianMode::Nothing);
    exchange(&mut hub, &mut pc, at(15, 31), true);
    assert!(pc.state.decide(at(15, 31), Some("game")).allowed);
    assert!(!pc.state.decide(at(15, 31), Some("casino")).allowed);

    // 20:30 A film night: unlocked until 22:00, past bedtime.
    hub.unlock_until(a, at(22, 0), at(20, 30)).unwrap();
    exchange(&mut hub, &mut pc, at(20, 31), false);
    assert_eq!(view(&hub, a, at(21, 30)).guardian, GuardianMode::Unlock { until: at(22, 0) });
    let d = pc.state.decide(at(21, 30), Some("game"));
    assert!(d.allowed);
    assert_eq!(d.unlocked_until, Some(at(22, 0)));
    assert_eq!(view(&hub, a, at(22, 1)).guardian, GuardianMode::Nothing);
    let d = pc.state.decide(at(22, 1), Some("game"));
    assert_eq!(d.lock.map(|l| l.reason), Some(LockReason::BlockedHours));

    // Ravi has nothing paired and nothing used.
    let r = view(&hub, &ravi.child_id, at(22, 1));
    assert_eq!((r.name.as_str(), r.age_band, r.used_secs, r.left_secs), ("Ravi", AgeBand::SixToNine, 0, Some(120 * M)));
    assert!(r.devices.is_empty());
    assert_eq!(hub.family_today(at(22, 1)).unwrap().len(), 2);

    // The day's usage: which app, for how long, per device and in total.
    let today = day_of(at(12, 0));
    let usage = hub.usage(a, today, today).unwrap();
    assert_eq!((usage.secs, usage.counted_secs), (30 * M, 30 * M));
    let apps: Vec<(&str, u64)> = usage.apps.iter().map(|x| (x.app_id.as_str(), x.secs)).collect();
    assert_eq!(apps, vec![("video", 20 * M), ("game", 10 * M)]);
    let devices: Vec<(&str, u64, bool)> = usage.devices.iter().map(|x| (x.name.as_str(), x.secs, x.removed)).collect();
    assert_eq!(devices, vec![("Asha's phone", 20 * M, false), ("Family PC", 10 * M, false)]);

    // The Windows accounts on the PC.
    hub.link_account("S-1-5-21-1001", "Asha", AccountLink::Child { child_id: a.to_string() }).unwrap();
    hub.link_account("S-1-5-21-1000", "Mum", AccountLink::NotLimited).unwrap();
    assert_eq!(hub.account_standing("S-1-5-21-1001").unwrap(), AccountStanding::Child { child_id: a.to_string() });
    assert_eq!(hub.account_standing("S-1-5-21-1000").unwrap(), AccountStanding::NotLimited);
    assert_eq!(hub.account_standing("S-1-5-21-9999").unwrap(), AccountStanding::NotLinked);
    let names: Vec<String> = hub.accounts().unwrap().into_iter().map(|x| x.display_name).collect();
    assert_eq!(names, vec!["Asha".to_string(), "Mum".to_string()]);

    // What the guardian can look back on, oldest first.
    let mut events = hub.events(100, None).unwrap();
    events.reverse();
    let what: Vec<EventKind> = events.iter().map(|e| e.what.clone()).collect();
    assert_eq!(what, vec![
        EventKind::RulesChanged { version: 2 },
        EventKind::DevicePaired { device_id: pc.id.clone(), name: "Family PC".into() },
        EventKind::DevicePaired { device_id: phone.id.clone(), name: "Asha's phone".into() },
        EventKind::RequestArrived { device_id: phone.id.clone(), request_id: request_id.clone(), minutes: 20 },
        EventKind::PinSet,
        EventKind::WrongGuesses { device_id: phone.id.clone(), count: 2 },
        EventKind::Locked { until: None },
        EventKind::Lifted,
        EventKind::Unlocked { until: at(22, 0) },
    ]);
    assert!(events.iter().filter(|e| e.what != EventKind::PinSet).all(|e| e.child_id.as_deref() == Some(a)));
    // Paging: the two newest, then the rest.
    let first = hub.events(2, None).unwrap();
    let rest = hub.events(100, Some(first[1].id)).unwrap();
    assert_eq!(first.len() + rest.len(), events.len());
}
