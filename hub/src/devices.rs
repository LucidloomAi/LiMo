//! Devices: pairing, removing, and the exchange of reports and grants.

use std::collections::BTreeSet;

use limo_core::{
    CODE_SECRET_BYTES, DeviceKind, DeviceView, Grant, MasterBook, Report, ReportOutcome, SECONDS_PER_DAY,
};
use rusqlite::{Connection, params};

use crate::error::HubError;
use crate::hub::Hub;
use crate::random;
use crate::store::{self, DeviceRow, add_event, kind_key, to_i64};
use crate::text::{self, TextField};
use crate::types::{DeviceInfo, EventKind, Pairing};
use crate::{CLOCK_FAR_OFF_SECS, IN_USE_FRESH_SECS};

/// A clock offset, as the core keeps it, when it is far off
/// ([`CLOCK_FAR_OFF_SECS`]); None when it is close enough or not known.
pub(crate) fn far_off(off_by: Option<i64>) -> Option<i64> {
    off_by.filter(|o| o.saturating_abs() > CLOCK_FAR_OFF_SECS)
}

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

/// Stores the slices the core took for the first time from a report: the
/// core counts each slice once (by its marks), and returns exactly those it
/// took, so each is stored once. Each slice's stored length is capped at a
/// day: a slice never spans more than its day, and a faulty one must not
/// swamp the totals (the core's own count saturates).
///
/// A slice the core takes is never for a day already folded into daily
/// totals: `tidy` folds days before the detail window (7 days or more) and,
/// in the same transaction, has the core forget the days before yesterday,
/// whose slices the core then no longer takes.
fn keep_slices(conn: &Connection, child_id: &str, outcome: &ReportOutcome) -> Result<(), HubError> {
    let mut stmt = conn.prepare(
        "INSERT OR IGNORE INTO slice (child_id, id, device_id, app_id, day, start_at, end_at, secs, counts)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;
    for slice in &outcome.new_slices {
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

/// Updates what the hub keeps about a device after a report, from what the
/// core now says of it (`after`) against what it said before (`before`):
/// when it was last heard from, always; whether it is in use, from its
/// newest report only. Records an event when the device reports more wrong
/// guesses than before, when its clock is found far off (once, until a
/// report finds it right again), and for each request for more time that
/// reached the master.
fn note_report(
    conn: &Connection,
    row: &DeviceRow,
    report: &Report,
    outcome: &ReportOutcome,
    before: &DeviceView,
    after: &DeviceView,
    now: i64,
) -> Result<(), HubError> {
    let child = Some(row.child_id.as_str());
    if let Some(off_by) = far_off(after.clock_off_by_secs)
        && far_off(before.clock_off_by_secs).is_none()
    {
        add_event(conn, now, child, &EventKind::ClockFarOff { device_id: row.id.clone(), off_by_secs: off_by })?;
    }
    if after.wrong_guesses > before.wrong_guesses {
        add_event(conn, now, child, &EventKind::WrongGuesses { device_id: row.id.clone(), count: after.wrong_guesses })?;
    }
    for request in &outcome.new_requests {
        let what = EventKind::RequestArrived {
            device_id: row.id.clone(),
            request_id: request.id.clone(),
            minutes: request.minutes,
        };
        add_event(conn, now, child, &what)?;
    }
    let in_use = if outcome.newest {
        let fresh_from = report.sent_at.saturating_sub(IN_USE_FRESH_SECS);
        report.slices.iter().any(|s| s.end > s.start && s.end >= fresh_from)
    } else {
        row.in_use_at_report
    };
    conn.execute(
        "UPDATE device SET last_heard_at = ?1, in_use_at_report = ?2 WHERE id = ?3",
        params![after.last_seen, in_use, row.id],
    )?;
    Ok(())
}

/// Pairs a device inside a change: registers it in the child's book with
/// its code secret, writes its row and records the event. The name and
/// platform are already checked, and the child exists.
pub(crate) fn pair_in(
    conn: &Connection,
    child_id: &str,
    name: String,
    kind: DeviceKind,
    platform: &str,
    secret: &[u8],
    now: i64,
) -> Result<Pairing, HubError> {
    if kind == DeviceKind::OnMaster && store::devices_of(conn, child_id, false)?.iter().any(|d| d.kind == kind) {
        return Err(HubError::MasterPcAlreadyPaired);
    }
    let device_id = store::new_id(conn, "device-", "SELECT 1 FROM device WHERE id = ?1")?;
    let mut book = store::load_book(conn, child_id)?;
    let welcome = book.add_device(&device_id, kind, now).map_err(HubError::from_master)?;
    book.set_code_secret(&device_id, secret.to_vec()).map_err(HubError::from_master)?;
    store::save_book(conn, child_id, &book, now)?;
    conn.execute(
        "INSERT INTO device (id, seq, child_id, name, kind, platform, paired_at, removed_at, last_heard_at,
             in_use_at_report)
         VALUES (?1, (SELECT coalesce(max(seq), 0) + 1 FROM device), ?2, ?3, ?4, ?5, ?6, NULL, ?6, 0)",
        params![device_id, child_id, name, kind_key(kind), platform, now],
    )?;
    add_event(conn, now, Some(child_id), &EventKind::DevicePaired { device_id: device_id.clone(), name })?;
    Ok(Pairing { child_id: child_id.to_string(), device_id, welcome, code_secret: secret.to_vec() })
}

/// Removes a device inside a change (see [`Hub::remove_device`]). A device
/// state the agent kept for it on the master PC goes too, with the code
/// secret in it.
pub(crate) fn remove_in(conn: &Connection, row: &DeviceRow, now: i64) -> Result<(), HubError> {
    let mut book = store::load_book(conn, &row.child_id)?;
    book.remove_device(&row.id, now).map_err(HubError::from_master)?;
    store::save_book(conn, &row.child_id, &book, now)?;
    conn.execute("UPDATE device SET removed_at = ?1 WHERE id = ?2", params![now, row.id])?;
    conn.execute("DELETE FROM code_made WHERE device_id = ?1", [&row.id])?;
    conn.execute("DELETE FROM master_pc WHERE device_id = ?1", [&row.id])?;
    let what = EventKind::DeviceRemoved { device_id: row.id.clone(), name: row.name.clone() };
    add_event(conn, now, Some(&row.child_id), &what)?;
    Ok(())
}

/// Lets the child's book take a report from `row`'s device inside a change,
/// writes the book, keeps the slices it took for the first time, and notes
/// what the report shows (see [`Hub::handle_report`]).
pub(crate) fn take_report(
    conn: &Connection,
    row: &DeviceRow,
    book: &mut MasterBook,
    report: &Report,
    now: i64,
) -> Result<Grant, HubError> {
    if row.child_id != report.child_id {
        return Err(HubError::DeviceOfAnotherChild { device_id: report.device_id.clone() });
    }
    let day = book.today(now);
    let before = book.device_view(&row.id, day).map_err(HubError::from_master)?;
    let outcome = book.handle_report(report, now).map_err(HubError::from_master)?;
    let after = book.device_view(&row.id, day).map_err(HubError::from_master)?;
    store::save_book(conn, &row.child_id, book, now)?;
    keep_slices(conn, &row.child_id, &outcome)?;
    note_report(conn, row, report, &outcome, &before, &after, now)?;
    Ok(outcome.grant)
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
    ///
    /// The agent on the master PC pairs each child's device there with
    /// [`Hub::pair_master_pc`] instead, which also keeps the device's state
    /// in the file.
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
        let secret = random::bytes(CODE_SECRET_BYTES as usize)?;
        let tx = self.change()?;
        store::child(&tx, child_id)?;
        let pairing = pair_in(&tx, child_id, name, kind, &platform, &secret, now)?;
        tx.commit()?;
        Ok(pairing)
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
    /// time for it at once, so the child's other devices get that time and
    /// its away share (the core's `MasterBook::remove_device`). Time added on
    /// it by a code or the PIN that it had not used is dropped. Its use so
    /// far stays counted in the child's day, and stays in the history, under
    /// its name, marked as removed. Use it made and never reported is not
    /// counted: remove a device after its last report when it can still make
    /// one.
    ///
    /// For a child's device on the master PC, the state the agent kept for
    /// it is deleted too. The agent pairs a new one the next time the child's
    /// account is in use.
    pub fn remove_device(&mut self, device_id: &str, now: i64) -> Result<(), HubError> {
        let tx = self.change()?;
        let row = store::active_device(&tx, device_id)?;
        remove_in(&tx, &row, now)?;
        tx.commit()?;
        Ok(())
    }

    /// Handles a report from any child's device and returns the grant to
    /// send back.
    ///
    /// The child's book takes the report, and the new book, the slices of
    /// use the book took for the first time, what the hub keeps about the
    /// device and the events it causes are written in one transaction, which
    /// is committed to the disk before this returns. So the grant is on
    /// record before it can leave: if anything fails, there is no grant,
    /// nothing is written, and the device, which has had no answer, keeps its
    /// slices and asks again.
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
        let grant = take_report(&tx, &row, &mut book, report, now)?;
        tx.commit()?;
        Ok(grant)
    }

    /// The devices worth asking to report at once, across all children, as
    /// each child's book names them: phones that can give time back, phones
    /// that must acknowledge a lowered allowance, waiting devices once time
    /// is free, and devices that have not yet reported holding the current
    /// rules or the guardian's newest lock, unlock, PIN or answer. Ask each
    /// one that can be reached; the PC's own agent is local.
    pub fn wanted_reports(&self, now: i64) -> Result<Vec<String>, HubError> {
        let mut wanted = BTreeSet::new();
        for child in store::children(&self.conn)? {
            wanted.extend(store::load_book(&self.conn, &child.child_id)?.wanted_reports(now));
        }
        Ok(wanted.into_iter().collect())
    }
}
