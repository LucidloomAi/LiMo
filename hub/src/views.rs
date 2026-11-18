//! What a guardian sees: read only, shaped for a screen. Each child's view
//! reads that child's book once and its device rows once.

use std::collections::BTreeMap;

use limo_core::{GuardianMode, GuardianSetting};
use rusqlite::{Connection, params};

use crate::error::HubError;
use crate::hub::Hub;
use crate::store::{self, to_u64};
use crate::types::{AppUsage, Child, ChildToday, DeviceToday, DeviceUsage, UsageView};
use crate::{IN_USE_HEARD_SECS, MAX_USAGE_DAYS};

/// The guardian's setting if it is in force at `now`, else `Nothing`.
fn in_force(setting: GuardianSetting, now: i64) -> GuardianMode {
    match setting.mode {
        GuardianMode::Lock { until } if until.is_none_or(|u| now < u) => setting.mode,
        GuardianMode::Unlock { until } if now < until => setting.mode,
        _ => GuardianMode::Nothing,
    }
}

fn child_today(conn: &Connection, child: Child, now: i64) -> Result<ChildToday, HubError> {
    let book = store::load_book(conn, &child.child_id)?;
    let day = book.today(now);
    let pool = book.pool(day);
    let mut devices = Vec::new();
    for row in store::devices_of(conn, &child.child_id, false)? {
        let view = book
            .device_view(&row.id, day)
            .ok_or_else(|| HubError::damaged("a paired device is missing from its child's book"))?;
        let heard_lately = row.last_heard_at.is_some_and(|h| now.saturating_sub(h) <= IN_USE_HEARD_SECS);
        devices.push(DeviceToday {
            device_id: row.id,
            name: row.name,
            kind: row.kind,
            platform: row.platform,
            last_heard_at: row.last_heard_at,
            in_use: row.in_use_at_report && heard_lately,
            used_secs: view.used_secs,
            holds_secs: view.reserved_secs,
            waiting_secs: view.waiting_secs,
            giving_back_secs: view.giving_back_secs,
            added_secs: view.added_secs,
            wrong_guesses: view.wrong_guesses,
            pin_unlocked_until: view.pin_unlocked_until.filter(|u| *u > now),
            clock_off_by_secs: row.clock_off_by,
        });
    }
    Ok(ChildToday {
        child_id: child.child_id,
        name: child.name,
        age_band: child.age_band,
        day,
        budget_secs: pool.budget_secs,
        extra_secs: pool.extra_secs,
        used_secs: pool.used_secs,
        left_secs: pool.budget_secs.map(|b| b.saturating_add(pool.extra_secs).saturating_sub(pool.used_secs)),
        over_secs: pool.over_secs,
        guardian: in_force(book.guardian_setting(), now),
        waiting_requests: u32::try_from(book.waiting_requests(now).len()).unwrap_or(u32::MAX),
        devices,
    })
}

/// Most used first; ties by app identifier.
fn sort_apps(apps: &mut [AppUsage]) {
    apps.sort_by(|a, b| b.secs.cmp(&a.secs).then_with(|| a.app_id.cmp(&b.app_id)));
}

impl Hub {
    /// Every child's day so far, children in the order they were added:
    /// time used and left, the budget and extra time, a lock or unlock in
    /// force, and each device with when it was last heard from and whether
    /// it is in use.
    pub fn family_today(&self, now: i64) -> Result<Vec<ChildToday>, HubError> {
        store::children(&self.conn)?.into_iter().map(|child| child_today(&self.conn, child, now)).collect()
    }

    /// One child's day so far.
    pub fn child_today(&self, child_id: &str, now: i64) -> Result<ChildToday, HubError> {
        let child = store::child(&self.conn, child_id)?;
        child_today(&self.conn, child, now)
    }

    /// A child's usage from home day `from_day` to `to_day`, both included,
    /// at most [`crate::MAX_USAGE_DAYS`] days: app by app, most used first,
    /// on each device and in total. Days kept in detail and days folded into
    /// daily totals are read alike. The usage is which app and for how long,
    /// and nothing else.
    pub fn usage(&self, child_id: &str, from_day: i64, to_day: i64) -> Result<UsageView, HubError> {
        store::child(&self.conn, child_id)?;
        let span = to_day.checked_sub(from_day);
        if span.is_none_or(|s| s < 0 || s >= MAX_USAGE_DAYS as i64) {
            return Err(HubError::DaysOutOfRange { most: MAX_USAGE_DAYS });
        }
        let mut stmt = self.conn.prepare(
            "SELECT device_id, app_id, sum(secs), sum(counted) FROM (
                 SELECT device_id, app_id, secs, CASE WHEN counts THEN secs ELSE 0 END AS counted
                 FROM slice WHERE child_id = ?1 AND day BETWEEN ?2 AND ?3
                 UNION ALL
                 SELECT device_id, app_id, secs, counted_secs
                 FROM day_total WHERE child_id = ?1 AND day BETWEEN ?2 AND ?3
             ) GROUP BY device_id, app_id",
        )?;
        let rows = stmt
            .query_map(params![child_id, from_day, to_day], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?, row.get::<_, i64>(3)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let names: BTreeMap<String, (String, bool)> = store::devices_of(&self.conn, child_id, true)?
            .into_iter()
            .map(|d| (d.id, (d.name, d.removed_at.is_some())))
            .collect();
        let mut per_device: BTreeMap<String, Vec<AppUsage>> = BTreeMap::new();
        let mut total: BTreeMap<String, AppUsage> = BTreeMap::new();
        for (device_id, app_id, secs, counted) in rows {
            let usage = AppUsage { app_id: app_id.clone(), secs: to_u64(secs), counted_secs: to_u64(counted) };
            let sum = total.entry(app_id.clone()).or_insert(AppUsage { app_id, secs: 0, counted_secs: 0 });
            sum.secs = sum.secs.saturating_add(usage.secs);
            sum.counted_secs = sum.counted_secs.saturating_add(usage.counted_secs);
            per_device.entry(device_id).or_default().push(usage);
        }

        let mut devices: Vec<DeviceUsage> = per_device
            .into_iter()
            .map(|(device_id, mut apps)| {
                sort_apps(&mut apps);
                let (name, removed) = names.get(&device_id).cloned().unwrap_or_else(|| (device_id.clone(), true));
                let secs = apps.iter().fold(0u64, |s, a| s.saturating_add(a.secs));
                DeviceUsage { device_id, name, removed, secs, apps }
            })
            .collect();
        devices.sort_by(|a, b| b.secs.cmp(&a.secs).then_with(|| a.name.cmp(&b.name)));
        let mut apps: Vec<AppUsage> = total.into_values().collect();
        sort_apps(&mut apps);
        Ok(UsageView {
            child_id: child_id.to_string(),
            from_day,
            to_day,
            secs: apps.iter().fold(0u64, |s, a| s.saturating_add(a.secs)),
            counted_secs: apps.iter().fold(0u64, |s, a| s.saturating_add(a.counted_secs)),
            apps,
            devices,
        })
    }
}
