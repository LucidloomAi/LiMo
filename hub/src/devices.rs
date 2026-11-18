//! Devices: pairing, removing, and the exchange of reports and grants.

use std::collections::{BTreeMap, BTreeSet};

use limo_core::{DeviceKind, Grant, Report, SECONDS_PER_DAY, Slice};
use rusqlite::{Connection, params};

use crate::error::HubError;
use crate::hub::Hub;
use crate::random;
use crate::retire;
use crate::store::{self, DeviceRow, add_event, kind_key, to_i64};
use crate::text::{self, TextField};
use crate::types::{DeviceInfo, EventKind, Pairing};
use crate::{CLOCK_FAR_OFF_SECS, FAR_DAY_DAYS, IN_USE_FRESH_SECS};

/// The length of a code secret, in bytes, as the core asks.
const CODE_SECRET_BYTES: usize = 32;

fn info(row: DeviceRow) -> DeviceInfo {
    DeviceInfo {
        device_id: row.id,
        child_id: row.child_id,
        name: row.name,
        kind: row.kind,
        platform: row.platform,
        paired_at: row.paired_at,
        last_heard_at: row.last_heard_at,
    }
}

/// Keeps the slices of a report that the hub records: those of the report's
/// own child and device, with an end after their start, for a day not yet
/// folded into daily totals and not absurdly far from today. A slice already
/// kept is kept once. Each slice's length is capped at a day: a slice never
/// spans more than its day, and a faulty one must not swamp the totals.
fn keep_slices(conn: &Connection, child_id: &str, report: &Report, today: i64, folded_before: Option<i64>) -> Result<(), HubError> {
    let sound = |s: &&Slice| {
        s.child_id == report.child_id
            && s.device_id == report.device_id
            && s.end > s.start
            && folded_before.is_none_or(|f| s.day >= f)
            && s.day.saturating_sub(today).saturating_abs() <= FAR_DAY_DAYS
    };
    let mut stmt = conn.prepare(
        "INSERT OR IGNORE INTO slice (child_id, id, device_id, app_id, day, start_at, end_at, secs, counts)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;
    for slice in report.slices.iter().filter(sound) {
        let secs = slice.secs().min(SECONDS_PER_DAY as u64);
        stmt.execute(params![
            child_id,
            slice.id,
            slice.device_id,
            slice.app_id,
            slice.day,
            slice.start,
            slice.end,
            to_i64(secs),
            slice.counts
        ])?;
    }
    Ok(())
}

/// Updates what the hub keeps about a device from a report: when it was
/// heard from always; the rest only from its newest report, as the core
/// does. Records an event when the device reports more wrong guesses than
/// before, or when its clock is found far off.
fn note_report(conn: &Connection, row: &DeviceRow, report: &Report, now: i64) -> Result<(), HubError> {
    let heard = row.last_heard_at.map_or(now, |h| h.max(now));
    // Message numbers start at 1, and the row starts at 0.
    if report.message_number <= row.last_message {
        conn.execute("UPDATE device SET last_heard_at = ?1 WHERE id = ?2", params![heard, row.id])?;
        return Ok(());
    }
    let fresh_from = report.sent_at.saturating_sub(IN_USE_FRESH_SECS);
    let in_use = report.slices.iter().any(|s| s.end > s.start && s.end >= fresh_from);
    let off_by = report.sent_at.saturating_sub(now);
    let clock_off_by = (off_by.saturating_abs() > CLOCK_FAR_OFF_SECS).then_some(off_by);
    if clock_off_by.is_some() && row.clock_off_by.is_none() {
        add_event(conn, now, Some(&row.child_id), &EventKind::ClockFarOff { device_id: row.id.clone(), off_by_secs: off_by })?;
    }
    if report.wrong_guesses > row.wrong_guesses {
        add_event(
            conn,
            now,
            Some(&row.child_id),
            &EventKind::WrongGuesses { device_id: row.id.clone(), count: report.wrong_guesses },
        )?;
    }
    conn.execute(
        "UPDATE device SET last_heard_at = ?1, last_message = ?2, rules_version_seen = ?3, in_use_at_report = ?4,
             wrong_guesses = ?5, clock_off_by = ?6
         WHERE id = ?7",
        params![
            heard,
            to_i64(report.message_number),
            report.rules_version,
            in_use,
            report.wrong_guesses,
            clock_off_by,
            row.id
        ],
    )?;
    Ok(())
}

impl Hub {
    /// Pairs a device with a child. The hub makes the device's id and its 32
    /// byte code secret, registers it in the child's book, and records its
    /// name, its kind (the master PC itself, or a device that can be carried
    /// away) and a platform label such as "windows" or "android". A child
    /// has the master PC at most once.
    ///
    /// Hand the device the [`Pairing`]: it makes its state from the welcome
    /// grant and takes the code secret. Moving a device to another child is
    /// [`Hub::remove_device`] and then pairing it again.
    pub fn pair_device(
        &mut self,
        child_id: &str,
        name: &str,
        kind: DeviceKind,
        platform: &str,
        now: i64,
    ) -> Result<Pairing, HubError> {
        let name = text::clean(name, TextField::DeviceName)?;
        let platform = text::clean(platform, TextField::Platform)?;
        let secret = random::bytes(CODE_SECRET_BYTES)?;
        let tx = self.change()?;
        store::child(&tx, child_id)?;
        if kind == DeviceKind::OnMaster && store::devices_of(&tx, child_id, false)?.iter().any(|d| d.kind == kind) {
            return Err(HubError::MasterPcAlreadyPaired);
        }
        let device_id = store::new_id(&tx, "device-", "SELECT 1 FROM device WHERE id = ?1")?;
        let mut book = store::load_book(&tx, child_id)?;
        let welcome = book.add_device(&device_id, kind, now).map_err(HubError::from_master)?;
        book.set_code_secret(&device_id, secret.clone()).map_err(HubError::from_master)?;
        store::save_book(&tx, child_id, &book, now)?;
        tx.execute(
            "INSERT INTO device (id, seq, child_id, name, kind, platform, paired_at, removed_at, last_heard_at,
                 last_message, rules_version_seen, in_use_at_report, wrong_guesses, clock_off_by)
             VALUES (?1, (SELECT coalesce(max(seq), 0) + 1 FROM device), ?2, ?3, ?4, ?5, ?6, NULL, ?6, 0, ?7, 0, 0, NULL)",
            params![device_id, child_id, name, kind_key(kind), platform, now, welcome.rules.version],
        )?;
        add_event(&tx, now, Some(child_id), &EventKind::DevicePaired { device_id: device_id.clone(), name })?;
        tx.commit()?;
        Ok(Pairing { child_id: child_id.to_string(), device_id, welcome, code_secret: secret })
    }

    /// A child's paired devices, in the order they were paired, with when
    /// each was last heard from. Removed devices are not listed.
    pub fn devices(&self, child_id: &str) -> Result<Vec<DeviceInfo>, HubError> {
        store::child(&self.conn, child_id)?;
        Ok(store::devices_of(&self.conn, child_id, false)?.into_iter().map(info).collect())
    }

    /// One paired device.
    pub fn device(&self, device_id: &str) -> Result<DeviceInfo, HubError> {
        Ok(info(store::active_device(&self.conn, device_id)?))
    }

    /// Renames a device.
    pub fn rename_device(&mut self, device_id: &str, name: &str) -> Result<DeviceInfo, HubError> {
        let name = text::clean(name, TextField::DeviceName)?;
        let tx = self.change()?;
        store::active_device(&tx, device_id)?;
        tx.execute("UPDATE device SET name = ?1 WHERE id = ?2", params![name, device_id])?;
        let row = store::active_device(&tx, device_id)?;
        tx.commit()?;
        Ok(info(row))
    }

    /// Removes a device from the family. Its reports are refused from now
    /// on, its code secret is forgotten, and the master stops holding back
    /// time for it (see the README for time added on it by a code or the
    /// PIN). Its use so far stays in the child's day and history, under its
    /// name, marked as removed.
    pub fn remove_device(&mut self, device_id: &str, now: i64) -> Result<(), HubError> {
        let tx = self.change()?;
        let row = store::active_device(&tx, device_id)?;
        let book = store::load_book(&tx, &row.child_id)?;
        let book = retire::retire(&book, device_id)?;
        store::save_book(&tx, &row.child_id, &book, now)?;
        tx.execute("UPDATE device SET removed_at = ?1 WHERE id = ?2", params![now, device_id])?;
        tx.execute("DELETE FROM code_made WHERE device_id = ?1", [device_id])?;
        add_event(&tx, now, Some(&row.child_id), &EventKind::DeviceRemoved { device_id: row.id.clone(), name: row.name })?;
        tx.commit()?;
        Ok(())
    }

    /// Handles a report from any child's device and returns the grant to
    /// send back.
    ///
    /// The child's book takes the report, and the new book, the slices of
    /// use and what the hub keeps about the device are written in one
    /// transaction, which is committed to the disk before this returns. So
    /// the grant is on record before it can leave: if anything fails, there
    /// is no grant, nothing is written, and the device, which has had no
    /// answer, keeps its slices and asks again.
    ///
    /// A report from a device that is not paired, was removed, or is paired
    /// with another child than the report names is refused.
    pub fn handle_report(&mut self, report: &Report, now: i64) -> Result<Grant, HubError> {
        let tx = self.change()?;
        let row = store::active_device(&tx, &report.device_id)?;
        if row.child_id != report.child_id {
            return Err(HubError::DeviceOfAnotherChild { device_id: report.device_id.clone() });
        }
        let mut book = store::load_book(&tx, &row.child_id)?;
        let grant = book.handle_report(report, now).map_err(HubError::from_master)?;
        store::save_book(&tx, &row.child_id, &book, now)?;
        let folded_before: Option<i64> =
            tx.query_row("SELECT folded_before FROM child WHERE id = ?1", [&row.child_id], |r| r.get(0))?;
        keep_slices(&tx, &row.child_id, report, book.today(now), folded_before)?;
        note_report(&tx, &row, report, now)?;
        tx.commit()?;
        Ok(grant)
    }

    /// The devices worth asking to report at once, across all children:
    /// those the core names (phones that can give time back, phones that
    /// must acknowledge a lowered allowance, waiting devices once time is
    /// free, devices that have not taken the guardian's newest lock, unlock,
    /// PIN or answer), and every device that has not yet reported holding
    /// its child's current rules. Ask each one that can be reached; the PC's
    /// own agent is local.
    pub fn wanted_reports(&self, now: i64) -> Result<Vec<String>, HubError> {
        let mut wanted = BTreeSet::new();
        for child in store::children(&self.conn)? {
            let book = store::load_book(&self.conn, &child.child_id)?;
            let current = book.rules().version;
            let active: BTreeMap<String, u32> = store::devices_of(&self.conn, &child.child_id, false)?
                .into_iter()
                .map(|d| (d.id, d.rules_version_seen))
                .collect();
            wanted.extend(book.wanted_reports(now).into_iter().filter(|id| active.contains_key(id)));
            wanted.extend(active.into_iter().filter(|(_, seen)| *seen < current).map(|(id, _)| id));
        }
        Ok(wanted.into_iter().collect())
    }
}
