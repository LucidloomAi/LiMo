//! What the agent on the master PC needs from the hub: each child's device
//! on the PC, kept here with its state; the exchange between that device and
//! the child's book, in one transaction; the book's revision; the PC's own
//! settings; and the events that only the agent can see.
//!
//! The agent is the first caller of the device side of the rules core on
//! the master PC. Its devices talk to the child's book through the same
//! report and grant a phone sends, only in process. The hub keeps each such
//! device's state in the same file as the book, and writes the two in one
//! transaction, so that the book that made a grant and the device that took
//! it can never disagree after a power cut.

use limo_core::{CODE_SECRET_BYTES, DeviceKind, DeviceState};
use rusqlite::{Connection, OptionalExtension};

use crate::devices::{pair_in, remove_in, take_report};
use crate::error::HubError;
use crate::hub::Hub;
use crate::random;
use crate::store::{self, DeviceRow, MasterPcRow, add_event, note_once, to_u32};
use crate::text::{self, TextField};
use crate::types::{EventKind, MasterPcExchange, MasterPcRecord, PcSettings, UnlinkedAccounts};
use crate::{MAX_IDLE_MINUTES, MIN_IDLE_MINUTES};

/// The platform label of a child's device on the master PC.
const PLATFORM: &str = "windows";

fn record(row: MasterPcRow, book_revision: u64) -> MasterPcRecord {
    MasterPcRecord {
        child_id: row.child_id,
        device_id: row.device_id,
        state: row.state,
        notes: row.notes,
        book_revision,
    }
}

/// The live device of a state the agent hands in, when it is a child's
/// device on the master PC whose state the hub keeps; why not otherwise.
fn master_pc_device(conn: &Connection, state: &DeviceState) -> Result<DeviceRow, HubError> {
    let device_id = state.device_id();
    let row = store::active_device(conn, &device_id)?;
    let kept = conn
        .query_row("SELECT 1 FROM master_pc WHERE device_id = ?1", [&device_id], |_| Ok(()))
        .optional()?
        .is_some();
    if row.kind != DeviceKind::OnMaster || row.child_id != state.child_id() || !kept {
        return Err(HubError::NotMasterPc { device_id });
    }
    Ok(row)
}

impl Hub {
    /// The child's device on the master PC, with its kept state, pairing it
    /// first when the child has none. It is paired once: called again, this
    /// returns the same device and state, and changes nothing.
    ///
    /// Pairing makes the device in the child's book (a device of kind "on
    /// the master", which holds no away allowance), makes its state from the
    /// welcome grant with its code secret, and keeps that state in the file,
    /// all in one transaction. So a power cut can never leave a device
    /// paired with no state, and the agent can never pair a second one.
    ///
    /// A device of that kind paired before the hub kept states (with
    /// [`Hub::pair_device`]) has no state here; it is removed, its use so far
    /// staying counted, and a new one is paired in its place, in the same
    /// transaction. `name` is the name the guardian sees for it, such as the
    /// PC's name.
    pub fn pair_master_pc(&mut self, child_id: &str, name: &str, now: i64) -> Result<MasterPcRecord, HubError> {
        let name = text::clean(name, TextField::DeviceName)?;
        let secret = random::bytes(CODE_SECRET_BYTES as usize)?;
        let tx = self.change()?;
        store::child(&tx, child_id)?;
        let pc = store::devices_of(&tx, child_id, false)?.into_iter().find(|d| d.kind == DeviceKind::OnMaster);
        if let Some(row) = &pc {
            if let Some(kept) = store::master_pc_row(&tx, &row.id)? {
                let revision = store::book_revision(&tx, child_id)?;
                return Ok(record(kept, revision));
            }
            remove_in(&tx, row, now)?;
        }
        let pairing = pair_in(&tx, child_id, name, DeviceKind::OnMaster, PLATFORM, &secret, now)?;
        let mut state = DeviceState::from_pairing(&pairing.welcome, now);
        state
            .set_code_secret(secret)
            .map_err(|_| HubError::internal("a new code secret was not taken by the device"))?;
        store::save_master_pc_row(&tx, &state, "", now)?;
        let book_revision = store::book_revision(&tx, child_id)?;
        tx.commit()?;
        Ok(MasterPcRecord {
            child_id: child_id.to_string(),
            device_id: pairing.device_id,
            state,
            notes: String::new(),
            book_revision,
        })
    }

    /// The child's device on the master PC with its kept state, if it has
    /// one. Reads only: a child with none, or with one paired before the hub
    /// kept states, has `None` (see [`Hub::pair_master_pc`]).
    pub fn master_pc(&self, child_id: &str) -> Result<Option<MasterPcRecord>, HubError> {
        store::child(&self.conn, child_id)?;
        let Some(row) =
            store::devices_of(&self.conn, child_id, false)?.into_iter().find(|d| d.kind == DeviceKind::OnMaster)
        else {
            return Ok(None);
        };
        let Some(kept) = store::master_pc_row(&self.conn, &row.id)? else { return Ok(None) };
        Ok(Some(record(kept, store::book_revision(&self.conn, child_id)?)))
    }

    /// Keeps the state of a child's device on the master PC and the agent's
    /// notes, in one transaction. Refused for a device that is not one
    /// ([`HubError::NotMasterPc`]), or that was removed meanwhile
    /// ([`HubError::DeviceRemoved`]): the agent then pairs a new one.
    pub fn save_master_pc(&mut self, state: &DeviceState, notes: &str, now: i64) -> Result<(), HubError> {
        let tx = self.change()?;
        master_pc_device(&tx, state)?;
        store::save_master_pc_row(&tx, state, notes, now)?;
        tx.commit()?;
        Ok(())
    }

    /// One exchange between a child's device on the master PC and the
    /// child's book, in process, with the same report and grant a phone
    /// sends: the device writes its report (asking for more time when
    /// `want_more`), the book takes it, the device takes the grant, and the
    /// book, the slices of use, the device's new state and the agent's
    /// `notes` are written in one transaction, committed to the disk before
    /// this returns. So the master has kept the book that made the grant,
    /// and the device has kept the state that took it, before any of that
    /// time can be used. A grant that asks to be acknowledged at once is
    /// acknowledged in the same transaction.
    ///
    /// `state` is changed only when everything was written. If anything
    /// fails, nothing is written, `state` is as it was, and the agent tries
    /// again later.
    pub fn exchange_on_master_pc(
        &mut self,
        state: &mut DeviceState,
        want_more: bool,
        notes: &str,
        now: i64,
    ) -> Result<MasterPcExchange, HubError> {
        let tx = self.change()?;
        let row = master_pc_device(&tx, state)?;
        let mut book = store::load_book(&tx, &row.child_id)?;
        let mut next = state.clone();
        let report = next.report(now, want_more);
        let mut grant = take_report(&tx, &row, &mut book, &report, now)?;
        next.take_grant(&grant, now);
        if grant.acknowledge_now {
            // A device on the master PC holds no away allowance, so this does
            // not happen; it is done as the core asks all the same.
            let report = next.report(now, want_more);
            grant = take_report(&tx, &row, &mut book, &report, now)?;
            next.take_grant(&grant, now);
        }
        store::save_master_pc_row(&tx, &next, notes, now)?;
        let book_revision = store::book_revision(&tx, &row.child_id)?;
        tx.commit()?;
        *state = next;
        Ok(MasterPcExchange { grant, book_revision })
    }

    /// A child's book revision: it grows by one every time the book is
    /// written, by an exchange with any device, by a guardian's action or
    /// by [`Hub::tidy`]. The agent compares it with the revision its own last
    /// exchange left, to see at once that something changed.
    pub fn book_revision(&self, child_id: &str) -> Result<u64, HubError> {
        store::book_revision(&self.conn, child_id)
    }

    /// The settings of LiMo on the master PC.
    pub fn pc_settings(&self) -> Result<PcSettings, HubError> {
        let (idle, unlinked): (i64, String) = self.conn.query_row(
            "SELECT idle_minutes, unlinked_accounts FROM settings WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let unlinked_accounts = UnlinkedAccounts::from_key(&unlinked)
            .ok_or_else(|| HubError::damaged(format!("the setting for accounts not linked is {unlinked:?}")))?;
        Ok(PcSettings { idle_minutes: to_u32(idle), unlinked_accounts })
    }

    /// Sets after how many minutes with no keyboard or mouse input the count
    /// pauses on the master PC: [`crate::MIN_IDLE_MINUTES`] to
    /// [`crate::MAX_IDLE_MINUTES`]. It takes effect at the agent's next look.
    pub fn set_idle_minutes(&mut self, minutes: u32) -> Result<(), HubError> {
        if !(MIN_IDLE_MINUTES..=MAX_IDLE_MINUTES).contains(&minutes) {
            return Err(HubError::SettingOutOfRange { min: MIN_IDLE_MINUTES, max: MAX_IDLE_MINUTES });
        }
        let tx = self.change()?;
        tx.execute("UPDATE settings SET idle_minutes = ?1 WHERE id = 1", [minutes])?;
        tx.commit()?;
        Ok(())
    }

    /// Sets what happens on the master PC in a Windows account that is
    /// linked to no child and not marked as not limited. Needs a family, so
    /// that a PC being set up is never locked; deleting the family sets it
    /// back to [`UnlinkedAccounts::Free`].
    pub fn set_unlinked_accounts(&mut self, unlinked: UnlinkedAccounts) -> Result<(), HubError> {
        let tx = self.change()?;
        store::require_family(&tx)?;
        tx.execute("UPDATE settings SET unlinked_accounts = ?1 WHERE id = 1", [unlinked.key()])?;
        tx.commit()?;
        Ok(())
    }

    /// Notes for the guardian that a Windows account that is not linked was
    /// in use on the master PC, once for each account and day: true when it
    /// was noted now. `day` is the PC's own day, counted from 1 January
    /// 1970. A name that cannot be shown is left out.
    pub fn note_unlinked_account_used(
        &mut self,
        account_id: &str,
        account_name: &str,
        day: i64,
        now: i64,
    ) -> Result<bool, HubError> {
        let account_id = text::clean(account_id, TextField::AccountId)?;
        let account_name = text::clean(account_name, TextField::AccountName).unwrap_or_default();
        let tx = self.change()?;
        let noted = note_once(&tx, None, &EventKind::UnlinkedAccountUsed { account_id, account_name, day }, now)?;
        tx.commit()?;
        Ok(noted)
    }

    /// Notes for the guardian that the master PC's clock moved by `by_secs`
    /// against its steady count (forward is positive).
    pub fn note_clock_jump(&mut self, by_secs: i64, now: i64) -> Result<(), HubError> {
        let tx = self.change()?;
        add_event(&tx, now, None, &EventKind::ClockJumped { by_secs })?;
        tx.commit()?;
        Ok(())
    }

    /// Notes for the guardian that the master PC's time zone changed.
    pub fn note_time_zone_change(&mut self, from_offset_secs: i32, to_offset_secs: i32, now: i64) -> Result<(), HubError> {
        let tx = self.change()?;
        add_event(&tx, now, None, &EventKind::TimeZoneChanged { from_offset_secs, to_offset_secs })?;
        tx.commit()?;
        Ok(())
    }

    /// Notes for the guardian that a blocked program was closed on a
    /// child's device on the master PC, once for each program and home day:
    /// true when it was noted now.
    pub fn note_blocked_program_closed(
        &mut self,
        device_id: &str,
        program: &str,
        day: i64,
        now: i64,
    ) -> Result<bool, HubError> {
        let program = text::clean(program, TextField::ProgramName)?;
        let tx = self.change()?;
        let row = store::active_device(&tx, device_id)?;
        let what = EventKind::BlockedProgramClosed { device_id: row.id.clone(), program, day };
        let noted = note_once(&tx, Some(&row.child_id), &what, now)?;
        tx.commit()?;
        Ok(noted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_note_is_kept_once() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = Hub::open(&dir.path().join("family.limo")).unwrap();
        assert!(hub.note_unlinked_account_used("S-1-5-21-7", "Guest", 20_775, 100).unwrap());
        assert!(!hub.note_unlinked_account_used("S-1-5-21-7", "Guest", 20_775, 200).unwrap());
        assert!(hub.note_unlinked_account_used("S-1-5-21-7", "Guest", 20_776, 300).unwrap());
        // A name with a hidden character is left out, not refused.
        assert!(hub.note_unlinked_account_used("S-1-5-21-8", "Gu\u{202E}est", 20_775, 400).unwrap());
        let kinds: Vec<EventKind> = hub.events(10, None).unwrap().into_iter().map(|e| e.what).collect();
        assert_eq!(kinds.len(), 3);
        assert_eq!(
            kinds[0],
            EventKind::UnlinkedAccountUsed { account_id: "S-1-5-21-8".into(), account_name: String::new(), day: 20_775 }
        );
    }
}
