//! Retiring a removed device in a child's book.
//!
//! The core has no call to remove a device from a `MasterBook` yet. Keeping
//! the device as it was would be wrong: the book would go on holding back
//! its lease and its away allowance, today and on every later day, and
//! counting its away share against the other phones'. Dropping its record
//! would be wrong too: its counted use today would drop out of the pool, and
//! the child would get that time again on another device.
//!
//! So the hub **retires** the record, through the book's saved form (the
//! core keeps that form loadable across versions, since books are saved
//! files): the device becomes a device of the master's kind with no report,
//! no grant waiting for an answer, no need and no code secret. Then the core
//! itself works out that:
//!
//! - its counted use stays in the pool of every day the book keeps;
//! - it holds back no lease and no away allowance, today or later, and has
//!   no away share;
//! - time added on it by a code or the PIN stays held back for the days the
//!   book keeps (at most today and yesterday): it was the device's own and
//!   does not pass to another device. Of that, the part the device already
//!   used is held back as well as counted, so on those days the other
//!   devices may have up to that much less. This errs on the strict side.
//!
//! [`drop_forgotten`] later removes a retired record once no day the book
//! keeps mentions the device. The README's report proposes
//! `MasterBook::remove_device` for the core, which would replace all of
//! this.

use std::collections::BTreeSet;

use limo_core::MasterBook;
use serde_json::{Map, Value, json};

use crate::error::HubError;

/// The fields of a device record this module sets. If the core renames one,
/// retiring fails loudly instead of doing half a job.
const FIELDS: [&str; 6] = ["device_id", "kind", "last_report", "unacknowledged", "need", "code_secret"];

fn devices_mut(value: &mut Value) -> Result<&mut Vec<Value>, HubError> {
    value
        .get_mut("devices")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| HubError::internal("the rules core's saved book has no list of devices"))
}

fn record_id(record: &Value) -> Option<&str> {
    record.get("device_id").and_then(Value::as_str)
}

fn check_shape(record: &Map<String, Value>) -> Result<(), HubError> {
    match FIELDS.iter().find(|f| !record.contains_key(**f)) {
        Some(missing) => Err(HubError::internal(format!(
            "the rules core's saved device record has no field {missing:?}; the hub must be updated with the core"
        ))),
        None => Ok(()),
    }
}

/// The book with `device_id` retired. Its counted use stays; it holds back
/// nothing new and has no away share or code secret.
pub(crate) fn retire(book: &MasterBook, device_id: &str) -> Result<MasterBook, HubError> {
    let mut value = serde_json::to_value(book).map_err(|_| HubError::internal("a book could not be read as JSON"))?;
    let devices = devices_mut(&mut value)?;
    let record = devices
        .iter_mut()
        .find(|d| record_id(d) == Some(device_id))
        .and_then(Value::as_object_mut)
        .ok_or_else(|| HubError::UnknownDevice { device_id: device_id.to_string() })?;
    check_shape(record)?;
    record.insert("kind".into(), json!("on_master"));
    record.insert("last_report".into(), Value::Null);
    record.insert("unacknowledged".into(), json!([]));
    record.insert("need".into(), Value::Null);
    record.insert("code_secret".into(), json!([]));
    serde_json::from_value(value).map_err(|_| HubError::internal("a retired book could not be read back"))
}

/// The book without the records of retired devices (those not in `active`)
/// that no day the book keeps mentions any more, and how many were dropped.
/// None when there was nothing to drop.
pub(crate) fn drop_forgotten(book: &MasterBook, active: &BTreeSet<String>) -> Result<Option<(MasterBook, u32)>, HubError> {
    let retired: Vec<String> = book.device_ids().into_iter().filter(|id| !active.contains(id)).collect();
    if retired.is_empty() {
        return Ok(None);
    }
    let mut value = serde_json::to_value(book).map_err(|_| HubError::internal("a book could not be read as JSON"))?;
    // Device ids are the hub's own ("device-" and 16 hex digits), so the id
    // in quotes appears in the kept days only where they name the device:
    // counted use, slices, extra time and requests.
    let days = value
        .get("days")
        .map(Value::to_string)
        .ok_or_else(|| HubError::internal("the rules core's saved book has no list of days"))?;
    let forgotten: BTreeSet<String> =
        retired.into_iter().filter(|id| !days.contains(&format!("\"{id}\""))).collect();
    if forgotten.is_empty() {
        return Ok(None);
    }
    let devices = devices_mut(&mut value)?;
    let before = devices.len();
    devices.retain(|d| record_id(d).is_none_or(|id| !forgotten.contains(id)));
    let dropped = u32::try_from(before - devices.len()).unwrap_or(u32::MAX);
    let book = serde_json::from_value(value).map_err(|_| HubError::internal("a tidied book could not be read back"))?;
    Ok(Some((book, dropped)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use limo_core::{CivilDate, DeviceKind, DeviceState, RuleSet};

    const MIN: u64 = 60;

    fn start() -> i64 {
        RuleSet::new(1, 19_800).calendar().at(CivilDate::new(2026, 11, 18).unwrap(), 8, 0, 0)
    }

    /// The PC and two phones with an away share of 75 percent: phone 1 gets
    /// 75, phone 2 the other 25 (30 minutes of a two hour day), so nothing
    /// is free and no lease is given. Phone 2 adds 15 minutes with a code
    /// and uses 20 minutes: the 15 added, then 5 of its away allowance. Then
    /// it is retired.
    #[test]
    fn a_retired_phone_keeps_its_use_and_holds_back_nothing_new() {
        let now = start();
        let mut rules = RuleSet::new(1, 19_800);
        rules.away_share_percent = 75;
        let mut book = MasterBook::new("kid", rules).unwrap();
        let today = book.today(now);
        book.add_device("pc", DeviceKind::OnMaster, now).unwrap();
        assert_eq!(book.add_device("phone-1", DeviceKind::Portable, now).unwrap().away_percent, 75);
        let welcome = book.add_device("phone-2", DeviceKind::Portable, now).unwrap();
        assert_eq!(welcome.away_percent, 25);
        let mut phone = DeviceState::from_pairing(&welcome, now);
        let secret = vec![9u8; 32];
        book.set_code_secret("phone-2", secret.clone()).unwrap();
        phone.set_code_secret(secret.clone()).unwrap();
        let code = limo_core::make_code(secret, "phone-2", today, 15, 0).unwrap();
        assert!(matches!(phone.try_code(&code.digits, now), limo_core::TryOutcome::Added { .. }));
        let report = phone.report(now, true);
        phone.take_grant(&book.handle_report(&report, now).unwrap(), now);
        let t = now + 20 * MIN as i64;
        phone.record_use("game", now, t);
        let report = phone.report(t, false);
        phone.take_grant(&book.handle_report(&report, t).unwrap(), t);
        let before = book.pool(today);
        assert_eq!((before.used_secs, before.free_secs), (20 * MIN, Some(0)));
        // Held back for it: the 25 minutes of away allowance it has left.
        assert_eq!(book.device_view("phone-2", today).unwrap().reserved_secs, Some(25 * MIN));

        let mut retired = retire(&book, "phone-2").unwrap();
        let view = retired.device_view("phone-2", today).unwrap();
        assert_eq!(view.kind, DeviceKind::OnMaster);
        assert_eq!(view.used_secs, 20 * MIN);
        // Only the time added by the code is still held back, all 15 minutes
        // of it, though it was used: the strict side.
        assert_eq!(view.reserved_secs, Some(15 * MIN));
        assert_eq!(retired.code_secret("phone-2").unwrap(), None);
        let after = retired.pool(today);
        assert_eq!((after.used_secs, after.extra_secs), (before.used_secs, 15 * MIN));
        assert_eq!(after.free_secs, Some(10 * MIN));
        // Tomorrow it holds nothing at all.
        assert_eq!(retired.device_view("phone-2", today + 1).unwrap().reserved_secs, Some(0));
        // It has no away share any more: a new phone gets the 25 percent.
        let welcome = retired.clone().add_device("phone-3", DeviceKind::Portable, now).unwrap();
        assert_eq!(welcome.away_percent, 25);
        let welcome = book.clone().add_device("phone-3", DeviceKind::Portable, now).unwrap();
        assert_eq!(welcome.away_percent, 0, "without retiring, phone 2 kept its share");

        // Once the days that mention it are forgotten, the record goes.
        let active: BTreeSet<String> = ["pc".to_string(), "phone-1".to_string()].into();
        assert!(drop_forgotten(&retired, &active).unwrap().is_none());
        retired.forget_days_before(today + 1);
        let (dropped, n) = drop_forgotten(&retired, &active).unwrap().unwrap();
        assert_eq!(n, 1);
        assert_eq!(dropped.device_ids(), vec!["pc".to_string(), "phone-1".to_string()]);
    }

    #[test]
    fn retiring_an_unknown_device_is_refused() {
        let book = MasterBook::new("kid", RuleSet::new(1, 0)).unwrap();
        assert_eq!(retire(&book, "nope").unwrap_err(), HubError::UnknownDevice { device_id: "nope".into() });
    }
}
