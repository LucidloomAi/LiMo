//! The family: its guardian password, its PIN, and deleting it.

use limo_core::{PinRecord, wait_after_wrong_guesses};
use rusqlite::{Connection, params};

use crate::{DEFAULT_DETAIL_DAYS, DEFAULT_IDLE_MINUTES, MAX_PIN_DIGITS, MIN_PIN_DIGITS};
use crate::error::HubError;
use crate::hub::Hub;
use crate::password::{self, PASSWORD_ROUNDS, PasswordRecord};
use crate::store::{self, add_event};
use crate::{link, random};
use crate::types::{EventKind, PasswordOutcome, PinOutcome};

/// The salt for a new PIN record, in bytes: the core asks for 16 or more.
const PIN_SALT_BYTES: usize = 16;

/// One more wrong guess after `wrong`, at `now`, with the wait `old_wait`
/// set before: the new count and the wait it sets, in the same steps as
/// wrong PINs and codes on a device (the core's `wait_after_wrong_guesses`).
/// The wait is kept as the moment it ends: a clock set back makes it longer,
/// never shorter, and a later wrong guess never shortens it. The one rule for
/// the password and the PIN typed on the master PC.
fn wrong_guess(wrong: u32, old_wait: Option<i64>, now: i64) -> (u32, Option<i64>) {
    let wrong = wrong.saturating_add(1);
    let wait = wait_after_wrong_guesses(wrong);
    let wait_until = (wait > 0).then(|| {
        let until = now.saturating_add(wait);
        old_wait.map_or(until, |old| old.max(until))
    });
    (wrong, wait_until)
}

/// Checks a password inside a change, counting a wrong one and setting the
/// wait it calls for. A right one clears the count, and rehashes the record
/// if it was made with fewer rounds than today's.
fn check_in(conn: &Connection, typed: &str, now: i64) -> Result<PasswordOutcome, HubError> {
    let family = store::family(conn)?;
    if let Some(until) = family.password_wait_until.filter(|w| now < *w) {
        return Ok(PasswordOutcome::MustWait { until });
    }
    if family.password.matches(typed) {
        conn.execute("UPDATE family SET wrong_passwords = 0, password_wait_until = NULL WHERE id = 1", [])?;
        if family.password.rounds < PASSWORD_ROUNDS {
            write_password(conn, &PasswordRecord::rehash(typed)?, None)?;
        }
        return Ok(PasswordOutcome::Right);
    }
    let (wrong, wait_until) = wrong_guess(family.wrong_passwords, family.password_wait_until, now);
    conn.execute(
        "UPDATE family SET wrong_passwords = ?1, password_wait_until = ?2 WHERE id = 1",
        params![wrong, wait_until],
    )?;
    Ok(PasswordOutcome::Wrong { wrong_guesses: wrong, wait_until: wait_until.filter(|w| now < *w) })
}

/// Clears the wrong guesses of the PIN typed on the master PC, and their
/// wait.
fn clear_pin_guesses(conn: &Connection) -> Result<(), HubError> {
    conn.execute("UPDATE family SET wrong_pins = 0, pin_wait_until = NULL WHERE id = 1", [])?;
    Ok(())
}

/// Writes a password record, and when it was changed if given.
fn write_password(conn: &Connection, record: &PasswordRecord, changed_at: Option<i64>) -> Result<(), HubError> {
    conn.execute(
        "UPDATE family SET password_salt = ?1, password_rounds = ?2, password_hash = ?3,
             password_changed_at = coalesce(?4, password_changed_at)
         WHERE id = 1",
        params![record.salt, record.rounds, record.hash, changed_at],
    )?;
    Ok(())
}

/// Every child's id.
fn child_ids(conn: &Connection) -> Result<Vec<String>, HubError> {
    let mut stmt = conn.prepare("SELECT id FROM child ORDER BY seq")?;
    let ids = stmt.query_map([], |row| row.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
    Ok(ids)
}

impl Hub {
    /// True when a family has been set up in this file.
    pub fn has_family(&self) -> Result<bool, HubError> {
        store::has_family(&self.conn)
    }

    /// Sets up the family with its guardian password: 8 to 256 characters.
    /// The password is kept only as a salted slow hash
    /// ([`crate::PASSWORD_ROUNDS`] rounds of PBKDF2-HMAC-SHA256), so this
    /// takes a moment. The master's own key for the link is made here too
    /// ([`Hub::master_key`]).
    pub fn create_family(&mut self, password: &str, now: i64) -> Result<(), HubError> {
        password::check_new(password)?;
        let master_key = link::new_master_key()?;
        let tx = self.change()?;
        if store::has_family(&tx)? {
            return Err(HubError::FamilyExists);
        }
        let record = PasswordRecord::new_password(password)?;
        tx.execute(
            "INSERT INTO family (id, created_at, password_salt, password_rounds, password_hash,
                 password_changed_at, wrong_passwords, password_wait_until, pin_record, pin_changed_at, master_key)
             VALUES (1, ?1, ?2, ?3, ?4, ?1, 0, NULL, NULL, NULL, ?5)",
            params![now, record.salt, record.rounds, record.hash, &master_key[..]],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Checks the guardian password, for signing in to the guardian's
    /// screens. Wrong guesses are counted and slow the next tries down, in
    /// the same steps as wrong PINs and codes on a device: the first four
    /// are free, then a wait of a minute that doubles with each further
    /// wrong guess, up to an hour. During a wait nothing is checked or
    /// counted. The count and the wait are saved, so a restart changes
    /// nothing.
    pub fn check_password(&mut self, password: &str, now: i64) -> Result<PasswordOutcome, HubError> {
        let tx = self.change()?;
        let outcome = check_in(&tx, password, now)?;
        tx.commit()?;
        Ok(outcome)
    }

    /// Changes the guardian password. The old one must be right; it is
    /// checked, and counted when wrong, as [`Hub::check_password`] does. A
    /// new password that cannot be taken is refused before the old one is
    /// looked at.
    pub fn change_password(&mut self, old: &str, new: &str, now: i64) -> Result<PasswordOutcome, HubError> {
        password::check_new(new)?;
        let tx = self.change()?;
        let outcome = check_in(&tx, old, now)?;
        if outcome == PasswordOutcome::Right {
            write_password(&tx, &PasswordRecord::new_password(new)?, Some(now))?;
            add_event(&tx, now, None, &EventKind::PasswordChanged)?;
        }
        tx.commit()?;
        Ok(outcome)
    }

    /// Sets a new guardian password WITHOUT the old one. Only for an
    /// administrator of the master PC who has forgotten the password, through
    /// LiMo's elevated command for that (`limo reset-password`), which is its
    /// one caller; a guardian who knows the password changes it with
    /// [`Hub::change_password`], which asks for the old one. The wrong
    /// guesses and their wait are cleared, and a [`EventKind::PasswordReset`]
    /// is recorded, so the family's history shows it. Everything else is kept:
    /// the PIN, the children, their rules and their devices.
    pub fn reset_password(&mut self, new: &str, now: i64) -> Result<(), HubError> {
        password::check_new(new)?;
        let tx = self.change()?;
        store::require_family(&tx)?;
        write_password(&tx, &PasswordRecord::new_password(new)?, Some(now))?;
        tx.execute("UPDATE family SET wrong_passwords = 0, password_wait_until = NULL WHERE id = 1", [])?;
        add_event(&tx, now, None, &EventKind::PasswordReset)?;
        tx.commit()?;
        Ok(())
    }

    /// True when the family has a guardian PIN.
    pub fn has_pin(&self) -> Result<bool, HubError> {
        Ok(store::family(&self.conn)?.pin.is_some())
    }

    /// Checks the guardian PIN typed on the master PC itself, with no device
    /// at hand: on the lock of a Windows account linked to no child. It has
    /// its own wrong guesses and wait, kept in the family's file, by the same
    /// rule as the password's and a device's: the first four are free, the
    /// fifth sets a wait of a minute, and each further one doubles it, up to
    /// an hour. During a wait nothing is checked or counted. A right PIN
    /// clears the count. A restart changes nothing. Setting or removing the
    /// PIN clears the count too. [`HubError::NoPin`] when no PIN is set.
    pub fn check_pin(&mut self, pin: &str, now: i64) -> Result<PinOutcome, HubError> {
        let tx = self.change()?;
        let family = store::family(&tx)?;
        let record = family.pin.ok_or(HubError::NoPin)?;
        if let Some(until) = family.pin_wait_until.filter(|w| now < *w) {
            return Ok(PinOutcome::MustWait { until });
        }
        if password::pin_matches(&record, pin) {
            clear_pin_guesses(&tx)?;
            tx.commit()?;
            return Ok(PinOutcome::Right);
        }
        let (wrong, wait_until) = wrong_guess(family.wrong_pins, family.pin_wait_until, now);
        tx.execute("UPDATE family SET wrong_pins = ?1, pin_wait_until = ?2 WHERE id = 1", params![wrong, wait_until])?;
        tx.commit()?;
        Ok(PinOutcome::Wrong { wrong_guesses: wrong, wait_until: wait_until.filter(|w| now < *w) })
    }

    /// Sets or changes the guardian PIN, 6 to 8 digits, for every child at
    /// once: every child's book takes the new record in one transaction, so
    /// no child is ever left with the old PIN at the master. Children added
    /// later get it too. It reaches each device with its next grant; ask the
    /// devices in [`Hub::wanted_reports`] to report.
    ///
    /// A PIN of fewer than [`crate::MIN_PIN_DIGITS`] digits, or with anything
    /// but digits in it, is refused here with [`HubError::PinWrongShape`],
    /// before the core sees it. Anything the core refuses comes back as
    /// [`HubError::Pin`], with the core's reason as it is.
    pub fn set_pin(&mut self, pin: &str, now: i64) -> Result<(), HubError> {
        let digits = pin.len() as u64;
        let shaped = (MIN_PIN_DIGITS as u64..=MAX_PIN_DIGITS as u64).contains(&digits)
            && pin.bytes().all(|b| b.is_ascii_digit());
        if !shaped {
            return Err(HubError::PinWrongShape { min_digits: MIN_PIN_DIGITS, max_digits: MAX_PIN_DIGITS });
        }
        let salt = random::bytes(PIN_SALT_BYTES)?;
        let tx = self.change()?;
        store::require_family(&tx)?;
        // Hashed once, slow on purpose, and the same record given to every
        // book.
        let record = PinRecord::new(pin, salt).map_err(|problem| HubError::Pin { problem })?;
        for child_id in child_ids(&tx)? {
            let mut book = store::load_book(&tx, &child_id)?;
            book.set_pin_record(record.clone()).map_err(HubError::from_master)?;
            store::save_book(&tx, &child_id, &book, now)?;
        }
        let json = serde_json::to_string(&record).map_err(|_| HubError::internal("a PIN record could not be written"))?;
        tx.execute("UPDATE family SET pin_record = ?1, pin_changed_at = ?2 WHERE id = 1", params![json, now])?;
        clear_pin_guesses(&tx)?;
        add_event(&tx, now, None, &EventKind::PinSet)?;
        tx.commit()?;
        Ok(())
    }

    /// Removes the guardian PIN for every child at once. Devices then refuse
    /// PIN actions once their next grant arrives. Nothing happens when there
    /// is no PIN.
    pub fn clear_pin(&mut self, now: i64) -> Result<(), HubError> {
        let tx = self.change()?;
        if store::family(&tx)?.pin.is_none() {
            return Ok(());
        }
        for child_id in child_ids(&tx)? {
            let mut book = store::load_book(&tx, &child_id)?;
            book.clear_pin();
            store::save_book(&tx, &child_id, &book, now)?;
        }
        tx.execute("UPDATE family SET pin_record = NULL, pin_changed_at = ?1 WHERE id = 1", [now])?;
        clear_pin_guesses(&tx)?;
        add_event(&tx, now, None, &EventKind::PinRemoved)?;
        tx.commit()?;
        Ok(())
    }

    /// Deletes the whole family: the password, the PIN, the master's key,
    /// every child with its rules, devices, phones' keys, notes and apps,
    /// pairing offers, usage, requests and events, the Windows accounts, and
    /// the settings. What is left is an empty, valid store, as a new file is.
    /// The space the data took is overwritten, and the file is rebuilt so
    /// that no copy of it is left (when the disk has room for that; the
    /// deletion stands either way).
    ///
    /// The hub keeps no sessions: the guardian's screen must ask for the
    /// password before it calls this.
    pub fn delete_family(&mut self) -> Result<(), HubError> {
        let tx = self.change()?;
        tx.execute_batch(
            "DELETE FROM account;
             DELETE FROM event;
             DELETE FROM code_made;
             DELETE FROM day_total;
             DELETE FROM slice;
             DELETE FROM master_pc;
             DELETE FROM phone_apps;
             DELETE FROM offer;
             DELETE FROM phone;
             DELETE FROM device;
             DELETE FROM book;
             DELETE FROM child;
             DELETE FROM family;
             DELETE FROM sqlite_sequence;",
        )?;
        tx.execute(
            "UPDATE settings SET detail_days = ?1, idle_minutes = ?2, unlinked_accounts = 'free' WHERE id = 1",
            [DEFAULT_DETAIL_DAYS, DEFAULT_IDLE_MINUTES],
        )?;
        tx.commit()?;
        self.compact();
        Ok(())
    }
}
