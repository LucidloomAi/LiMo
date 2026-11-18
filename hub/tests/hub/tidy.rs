//! Keeping the file small: folding usage, trimming books, codes and events.

use limo_core::{DeviceKind, Slice};
use limo_hub::{DEFAULT_DETAIL_DAYS, Hub, HubError, MAX_EVENTS, UsageView};

use crate::common::*;

const DAY: i64 = 86_400;

#[test]
fn folding_keeps_daily_totals_equal_to_what_the_slices_summed_to() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let mut rules = hub.rules(&asha).unwrap();
    rules.always_allowed_apps = vec!["clock".into()];
    hub.replace_rules(&asha, rules, at(7, 0)).unwrap();
    let mut pc = pair(&mut hub, &asha, "PC", DeviceKind::OnMaster, at(7, 0));
    let mut phone = pair(&mut hub, &asha, "Phone", DeviceKind::Portable, at(7, 0));

    // Forty days of use: the PC plays two apps, the phone two, one of them
    // always allowed, so that it does not count.
    let day0 = day_of(at(8, 0));
    for d in 0..40 {
        let base = at(8, 0) + d * DAY;
        use_app(&mut hub, &mut pc, "game", base, base + (5 + d % 7) * MIN);
        use_app(&mut hub, &mut pc, "browser", base + 15 * MIN, base + 18 * MIN);
        use_app(&mut hub, &mut phone, "video", base + 20 * MIN, base + (21 + d % 5) * MIN);
        use_app(&mut hub, &mut phone, "clock", base + 30 * MIN, base + 31 * MIN);
    }
    let days = |hub: &Hub| -> Vec<UsageView> { (0..40).map(|d| hub.usage(&asha, day0 + d, day0 + d).unwrap()).collect() };
    let whole = |hub: &Hub| hub.usage(&asha, day0, day0 + 39).unwrap();
    let per_day = days(&hub);
    let all = whole(&hub);
    assert_eq!(all.secs - all.counted_secs, 40 * MIN as u64, "the clock never counts");

    // On day 39, with 35 days in detail, days 0 to 4 fold.
    let now = at(9, 0) + 39 * DAY;
    let report = hub.tidy(now).unwrap();
    assert_eq!(report.slices_folded, 5 * 4);
    assert_eq!(days(&hub), per_day);
    assert_eq!(whole(&hub), all);
    // Nothing is due a second time.
    assert_eq!(hub.tidy(now).unwrap().slices_folded, 0);

    // A shorter window folds more; a longer one unfolds nothing.
    hub.set_detail_days(7).unwrap();
    assert_eq!(hub.tidy(now).unwrap().slices_folded, 28 * 4);
    hub.set_detail_days(400).unwrap();
    assert_eq!(hub.tidy(now).unwrap().slices_folded, 0);
    assert_eq!(days(&hub), per_day);
    assert_eq!(whole(&hub), all);

    // A slice that turns up late for a folded day is not added again: it may
    // be one already folded.
    let mut report = phone.state.report(now, false);
    report.slices.push(Slice {
        id: format!("{}/late", phone.id),
        child_id: asha.clone(),
        device_id: phone.id.clone(),
        app_id: "video".into(),
        day: day0 + 30,
        start: at(8, 20) + 30 * DAY,
        end: at(8, 50) + 30 * DAY,
        counts: true,
    });
    hub.handle_report(&report, now).unwrap();
    assert_eq!(hub.usage(&asha, day0 + 30, day0 + 30).unwrap(), per_day[30]);
    hub.close().unwrap();

    // Inside the file: no slice before the window, one total per day,
    // device and app.
    let conn = raw(&place.path);
    let cutoff = day0 + 39 - 6;
    assert_eq!(count(&conn, "slice", &format!("day < {cutoff}")), 0);
    assert_eq!(count(&conn, "slice", "1"), 7 * 4);
    assert_eq!(count(&conn, "day_total", "1"), 33 * 4);
}

#[test]
fn tidy_trims_the_books_codes_and_events() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let mut phone = pair(&mut hub, &asha, "Phone", DeviceKind::Portable, at(7, 0));
    let day0 = day_of(at(8, 0));
    for d in 0..4 {
        let base = at(8, 0) + d * DAY;
        use_app(&mut hub, &mut phone, "video", base, base + 10 * MIN);
    }
    hub.make_code(&phone.id, 15, at(9, 0) + 3 * DAY).unwrap();
    hub.make_code(&phone.id, 30, at(9, 1) + 3 * DAY).unwrap();
    let now = at(10, 0) + 3 * DAY;
    let before = hub.child_today(&asha, now).unwrap();
    let report = hub.tidy(now).unwrap();
    assert_eq!(report.codes_dropped, 0, "today's codes are kept");
    assert_eq!(hub.child_today(&asha, now).unwrap(), before, "today is untouched");
    // Two days on, the codes of day 3 are of no more use.
    assert_eq!(hub.tidy(now + 2 * DAY).unwrap().codes_dropped, 2);
    hub.close().unwrap();

    // The book keeps only today and yesterday.
    let conn = raw(&place.path);
    let json: String = conn.query_row("SELECT json FROM book", [], |r| r.get(0)).unwrap();
    let book: serde_json::Value = serde_json::from_str(&json).unwrap();
    let kept: Vec<i64> =
        book["days"].as_array().unwrap().iter().map(|d| d["day"].as_i64().unwrap()).collect();
    assert!(kept.iter().all(|d| *d >= day0 + 4), "{kept:?}");

    // Old events and too many events go.
    let old = now - 100 * DAY;
    conn.execute_batch("BEGIN").unwrap();
    for i in 0..2_100 {
        let at = if i < 50 { old } else { now };
        conn.execute("INSERT INTO event (at, child_id, kind, detail) VALUES (?1, NULL, 'lifted', '{\"kind\":\"lifted\"}')", [at])
            .unwrap();
    }
    conn.execute_batch("COMMIT").unwrap();
    drop(conn);
    let mut hub = Hub::open(&place.path).unwrap();
    let report = hub.tidy(now).unwrap();
    assert!(report.events_dropped >= 100, "{report:?}");
    hub.close().unwrap();
    let conn = raw(&place.path);
    assert_eq!(count(&conn, "event", "1"), MAX_EVENTS as i64);
    assert_eq!(count(&conn, "event", &format!("at < {}", now - 90 * DAY)), 0);
}

#[test]
fn a_removed_device_leaves_the_book_once_its_days_are_forgotten() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let mut phone = pair(&mut hub, &asha, "Phone", DeviceKind::Portable, at(7, 0));
    use_app(&mut hub, &mut phone, "video", at(8, 0), at(8, 30));
    hub.remove_device(&phone.id, at(9, 0)).unwrap();
    assert_eq!(hub.tidy(at(10, 0)).unwrap().retired_devices_dropped, 0, "today's pool still counts its use");
    assert_eq!(hub.child_today(&asha, at(10, 0)).unwrap().used_secs, 30 * MIN as u64);
    assert_eq!(hub.tidy(at(10, 0) + 2 * DAY).unwrap().retired_devices_dropped, 1);
    // The child's history still shows it.
    let day0 = day_of(at(8, 0));
    let usage = hub.usage(&asha, day0, day0).unwrap();
    assert_eq!((usage.secs, usage.devices[0].removed), (30 * MIN as u64, true));
    assert!(hub.child_today(&asha, at(10, 0) + 2 * DAY).unwrap().devices.is_empty());
}

#[test]
fn the_detail_window_is_a_setting_kept_in_the_file() {
    let place = place();
    let mut hub = new_family(&place.path);
    assert_eq!(hub.detail_days().unwrap(), DEFAULT_DETAIL_DAYS);
    for bad in [0, 6, 401, u32::MAX] {
        let err = hub.set_detail_days(bad).unwrap_err();
        assert_eq!(err, HubError::SettingOutOfRange { min: 7, max: 400 });
        plain(&err.to_string());
    }
    hub.set_detail_days(10).unwrap();
    hub.close().unwrap();
    assert_eq!(Hub::open(&place.path).unwrap().detail_days().unwrap(), 10);
}
