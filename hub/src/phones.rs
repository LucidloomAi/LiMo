//! What the master knows of each phone beside the child's book: its notes,
//! its apps, and whether it was told that it was removed.

use std::collections::BTreeSet;

use limo_core::DeviceKind;
use rusqlite::{Connection, OptionalExtension, params};

use crate::error::HubError;
use crate::hub::Hub;
use crate::notes::{Newest, PhoneNote};
use crate::store::{self, DeviceRow, add_event, to_u32};
use crate::text;
use crate::types::{EventKind, PhoneApp, PhoneApps, PhoneForm, PhoneInfo};
use crate::{MAX_APP_LABEL_CHARS, MAX_APPS_HASH_DIGITS, MAX_PACKAGE_BYTES, MAX_PHONE_APPS};

/// What the `phone` table holds for one phone.
struct PhoneRow {
    has_link_key: bool,
    form: PhoneForm,
    android_version: Option<u32>,
    newest: Newest,
    released_at: Option<i64>,
}

/// A row of `phone` as read: whether it has a link key, the form, the
/// Android version, the notes and when it was released.
type RawPhone = (bool, String, Option<i64>, String, Option<i64>);

/// The phone row of a device, or [`HubError::NotAPhone`] for a device of
/// the master PC.
fn phone_row(conn: &Connection, device: &DeviceRow) -> Result<PhoneRow, HubError> {
    let raw: Option<RawPhone> = conn
        .query_row(
            "SELECT link_key IS NOT NULL, form, android_version, notes, released_at FROM phone WHERE device_id = ?1",
            [&device.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .optional()?;
    let Some((has_link_key, form, android_version, notes, released_at)) = raw else {
        return Err(HubError::NotAPhone { device_id: device.id.clone() });
    };
    let form = PhoneForm::from_key(&form).ok_or_else(|| HubError::damaged(format!("a phone has an unknown form {form:?}")))?;
    let notes: Vec<PhoneNote> = serde_json::from_str(&notes)?;
    Ok(PhoneRow {
        has_link_key,
        form,
        android_version: android_version.map(to_u32),
        newest: Newest::from_notes(notes),
        released_at,
    })
}

/// A device, removed or not, or [`HubError::UnknownDevice`].
fn any_device(conn: &Connection, device_id: &str) -> Result<DeviceRow, HubError> {
    store::device(conn, device_id)?.ok_or_else(|| HubError::UnknownDevice { device_id: device_id.to_string() })
}

/// A phone's view, from its device row and its phone row.
fn info(conn: &Connection, device: DeviceRow) -> Result<PhoneInfo, HubError> {
    let phone = phone_row(conn, &device)?;
    let apps: Option<i64> = conn
        .query_row("SELECT json_array_length(apps) FROM phone_apps WHERE device_id = ?1", [&device.id], |row| row.get(0))
        .optional()?;
    Ok(PhoneInfo {
        setup_finished_at: phone.newest.setup_finished_at(),
        switched_on: phone.newest.permissions(true),
        switched_off: phone.newest.permissions(false),
        protection: phone.newest.protection(),
        usb_debugging_on: phone.newest.usb_debugging_on(),
        notes: phone.newest.notes().to_vec(),
        device_id: device.id,
        child_id: device.child_id,
        name: device.name,
        paired_at: device.paired_at,
        last_heard_at: device.last_heard_at,
        form: phone.form,
        android_version: phone.android_version,
        has_link_key: phone.has_link_key,
        apps: apps.map(to_u32),
        removed_at: device.removed_at,
        released_at: phone.released_at,
    })
}

/// Keeps the notes the child's book took for the first time from a phone's
/// report, inside the change that takes the report, in the report's order:
/// each becomes an event and the newest note of its kind for the phone.
/// Reports carry each note until a grant confirms it, so the same note comes
/// many times; the book takes it once, by the phone's numbers, and at most
/// [`crate::MAX_NOTES_PER_REPORT`] from one report.
pub(crate) fn keep_notes(conn: &Connection, device: &DeviceRow, notes: &[PhoneNote], now: i64) -> Result<(), HubError> {
    let mut phone = phone_row(conn, device)?;
    for note in notes {
        let what = EventKind::PhoneNoted { device_id: device.id.clone(), note: note.clone() };
        add_event(conn, now, Some(&device.child_id), &what)?;
        phone.newest.take(note);
    }
    let json = serde_json::to_string(phone.newest.notes())
        .map_err(|_| HubError::internal("a phone's notes could not be written as JSON"))?;
    conn.execute("UPDATE phone SET notes = ?1 WHERE device_id = ?2", params![json, device.id])?;
    Ok(())
}

/// True for a package name as Android makes them: letters, digits, dots and
/// underscores.
fn is_package(name: &str) -> bool {
    (1..=MAX_PACKAGE_BYTES).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_')
}

/// A phone's list made fit to keep: entries whose package name is not one
/// Android makes are left out, as is a package listed twice after its first
/// time; labels have hidden characters taken out and are cut to
/// [`MAX_APP_LABEL_CHARS`], and an empty label becomes the package name.
fn fit_apps(apps: &[PhoneApp]) -> Vec<PhoneApp> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::with_capacity(apps.len());
    for app in apps {
        let package = app.package.trim();
        if !is_package(package) || !seen.insert(package.to_string()) {
            continue;
        }
        let label = text::fit(&app.label, MAX_APP_LABEL_CHARS);
        let label = if label.is_empty() { package.to_string() } else { label };
        out.push(PhoneApp { package: package.to_string(), label });
    }
    out
}

impl Hub {
    /// What the master knows of one phone, removed or not: its form, its
    /// Android version, whether the link knows its key, what its notes say
    /// (setup finished, the permissions switched on and off, its protection,
    /// USB debugging) and the newest note of each kind, how many apps it
    /// listed, and whether, removed, it was told so. Refused with
    /// [`HubError::NotAPhone`] for a child's device on the master PC.
    pub fn phone(&self, device_id: &str) -> Result<PhoneInfo, HubError> {
        info(&self.conn, any_device(&self.conn, device_id)?)
    }

    /// A child's paired phones, in the order they were paired. Removed ones
    /// are not listed; [`Hub::phone`] still answers for them.
    pub fn phones(&self, child_id: &str) -> Result<Vec<PhoneInfo>, HubError> {
        store::child(&self.conn, child_id)?;
        store::devices_of(&self.conn, child_id, false)?
            .into_iter()
            .filter(|d| d.kind == DeviceKind::Portable)
            .map(|d| info(&self.conn, d))
            .collect()
    }

    /// Keeps a phone's list of apps with their labels, and the hash the
    /// phone gave it, replacing the list kept before. True when it was kept
    /// now; false when the same list was kept already, and nothing changed.
    ///
    /// The list is made fit to keep first: an entry whose package name is
    /// not one Android makes is left out, as is a package listed again;
    /// hidden characters are taken out of the labels, which are cut to
    /// [`crate::MAX_APP_LABEL_CHARS`]. Refused with [`HubError::BadAppList`]
    /// for more than [`crate::MAX_PHONE_APPS`] apps, or a hash that is not 1
    /// to 64 hex digits; for a removed device, and for a device that is not
    /// a phone.
    pub fn keep_phone_apps(&mut self, device_id: &str, apps: &[PhoneApp], hash: &str, now: i64) -> Result<bool, HubError> {
        let hash = hash.trim().to_ascii_lowercase();
        let hash_fits = (1..=MAX_APPS_HASH_DIGITS).contains(&hash.len()) && hash.bytes().all(|b| b.is_ascii_hexdigit());
        if apps.len() > MAX_PHONE_APPS || !hash_fits {
            return Err(HubError::BadAppList { most_apps: u32::try_from(MAX_PHONE_APPS).unwrap_or(u32::MAX) });
        }
        let json = serde_json::to_string(&fit_apps(apps))
            .map_err(|_| HubError::internal("a phone's apps could not be written as JSON"))?;
        let tx = self.change()?;
        let device = store::active_device(&tx, device_id)?;
        phone_row(&tx, &device)?;
        let kept: Option<(String, String)> = tx
            .query_row("SELECT apps, hash FROM phone_apps WHERE device_id = ?1", [device_id], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .optional()?;
        if kept.is_some_and(|(a, h)| a == json && h == hash) {
            return Ok(false);
        }
        tx.execute(
            "INSERT INTO phone_apps (device_id, child_id, apps, hash, kept_at) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (device_id) DO UPDATE SET apps = excluded.apps, hash = excluded.hash, kept_at = excluded.kept_at",
            params![device.id, device.child_id, json, hash, now],
        )?;
        tx.commit()?;
        Ok(true)
    }

    /// A phone's list of apps, if it sent one. None too for a removed phone:
    /// its list goes when it is removed.
    pub fn phone_apps(&self, device_id: &str) -> Result<Option<PhoneApps>, HubError> {
        let device = any_device(&self.conn, device_id)?;
        phone_row(&self.conn, &device)?;
        let kept: Option<(String, String, i64)> = self
            .conn
            .query_row("SELECT apps, hash, kept_at FROM phone_apps WHERE device_id = ?1", [device_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .optional()?;
        let Some((apps, hash, kept_at)) = kept else { return Ok(None) };
        Ok(Some(PhoneApps { device_id: device.id, apps: serde_json::from_str(&apps)?, hash, kept_at }))
    }

    /// Notes that a phone removed from the family was told so and let go of
    /// its limits: once, with an event. True when it was noted now; false
    /// when it was noted before. Refused for a device still paired
    /// ([`HubError::DeviceNotRemoved`]) and for one that is not a phone.
    pub fn note_released(&mut self, device_id: &str, now: i64) -> Result<bool, HubError> {
        let tx = self.change()?;
        let device = any_device(&tx, device_id)?;
        let phone = phone_row(&tx, &device)?;
        if device.removed_at.is_none() {
            return Err(HubError::DeviceNotRemoved { device_id: device.id });
        }
        if phone.released_at.is_some() {
            return Ok(false);
        }
        tx.execute("UPDATE phone SET released_at = ?1 WHERE device_id = ?2", params![now, device.id])?;
        let what = EventKind::DeviceReleased { device_id: device.id.clone(), name: device.name.clone() };
        add_event(&tx, now, Some(&device.child_id), &what)?;
        tx.commit()?;
        Ok(true)
    }
}
