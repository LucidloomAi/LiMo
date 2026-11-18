//! Children, and their rules.

use limo_core::{MasterBook, MasterError, RuleSet};
use rusqlite::{Connection, params};

use crate::error::HubError;
use crate::hub::Hub;
use crate::store::{self, add_event};
use crate::text::{self, TextField};
use crate::types::{AgeBand, Child, EventKind};

/// [`HubError::ChildNameTaken`] if another child already has this name,
/// ignoring case.
fn check_name_free(conn: &Connection, name: &str, except: Option<&str>) -> Result<(), HubError> {
    let wanted = name.to_lowercase();
    for child in store::children(conn)? {
        if Some(child.child_id.as_str()) != except && child.name.to_lowercase() == wanted {
            return Err(HubError::ChildNameTaken { name: name.to_string() });
        }
    }
    Ok(())
}

impl Hub {
    /// Adds a child with the core's default rules (two hours every day,
    /// half of it as the away share, ten minute leases), in the home time
    /// zone given as seconds from UTC: the master PC's zone, 19800 for
    /// India. The child gets the family's PIN, if one is set. The name is 1
    /// to 40 characters of any script, and no two children share one.
    pub fn add_child(&mut self, name: &str, age_band: AgeBand, utc_offset_secs: i32, now: i64) -> Result<Child, HubError> {
        let name = text::clean(name, TextField::ChildName)?;
        let tx = self.change()?;
        let family = store::family(&tx)?;
        check_name_free(&tx, &name, None)?;
        let child_id = store::new_id(&tx, "child-", "SELECT 1 FROM child WHERE id = ?1")?;
        let mut book = MasterBook::new(&child_id, RuleSet::new(1, utc_offset_secs)).map_err(HubError::from_master)?;
        if let Some(pin) = family.pin {
            book.set_pin_record(pin).map_err(HubError::from_master)?;
        }
        tx.execute(
            "INSERT INTO child (id, seq, name, age_band, added_at, folded_before)
             VALUES (?1, (SELECT coalesce(max(seq), 0) + 1 FROM child), ?2, ?3, ?4, NULL)",
            params![child_id, name, age_band.key(), now],
        )?;
        store::save_book(&tx, &child_id, &book, now)?;
        tx.commit()?;
        Ok(Child { child_id, name, age_band, added_at: now })
    }

    /// Every child, in the order they were added.
    pub fn children(&self) -> Result<Vec<Child>, HubError> {
        store::children(&self.conn)
    }

    /// One child.
    pub fn child(&self, child_id: &str) -> Result<Child, HubError> {
        store::child(&self.conn, child_id)
    }

    /// Renames a child.
    pub fn rename_child(&mut self, child_id: &str, name: &str) -> Result<Child, HubError> {
        let name = text::clean(name, TextField::ChildName)?;
        let tx = self.change()?;
        store::child(&tx, child_id)?;
        check_name_free(&tx, &name, Some(child_id))?;
        tx.execute("UPDATE child SET name = ?1 WHERE id = ?2", params![name, child_id])?;
        let child = store::child(&tx, child_id)?;
        tx.commit()?;
        Ok(child)
    }

    /// Changes a child's age band.
    pub fn set_age_band(&mut self, child_id: &str, age_band: AgeBand) -> Result<Child, HubError> {
        let tx = self.change()?;
        store::child(&tx, child_id)?;
        tx.execute("UPDATE child SET age_band = ?1 WHERE id = ?2", params![age_band.key(), child_id])?;
        let child = store::child(&tx, child_id)?;
        tx.commit()?;
        Ok(child)
    }

    /// Removes a child for good: the child's rules and book, devices, usage
    /// in detail and in daily totals, requests, codes made, linked Windows
    /// accounts and events are all deleted. The space they took in the file
    /// is overwritten, and the file is rebuilt so that no copy of them is
    /// left (when the disk has room for that; the deletion stands either
    /// way). It cannot be undone.
    pub fn remove_child(&mut self, child_id: &str) -> Result<(), HubError> {
        let tx = self.change()?;
        store::child(&tx, child_id)?;
        // Everything else goes with the child's row (ON DELETE CASCADE).
        tx.execute("DELETE FROM child WHERE id = ?1", [child_id])?;
        tx.commit()?;
        self.compact();
        Ok(())
    }

    /// A child's current rules.
    pub fn rules(&self, child_id: &str) -> Result<RuleSet, HubError> {
        Ok(store::load_book(&self.conn, child_id)?.rules())
    }

    /// Replaces a child's rules and returns them as taken. The hub sets the
    /// version, one more than the current one, whatever `rules.version`
    /// says. The core checks the rules; when it refuses them,
    /// [`HubError::InvalidRules`] carries its problems unchanged, each with
    /// the field it is about and a sentence for the guardian.
    ///
    /// The new rules reach each device with its next grant; ask the devices
    /// in [`Hub::wanted_reports`] to report, which names every device that
    /// has not yet taken them.
    pub fn replace_rules(&mut self, child_id: &str, rules: RuleSet, now: i64) -> Result<RuleSet, HubError> {
        let tx = self.change()?;
        store::child(&tx, child_id)?;
        let mut book = store::load_book(&tx, child_id)?;
        let current = book.rules().version;
        let mut rules = rules;
        rules.version = current
            .checked_add(1)
            .ok_or(HubError::Refused { problem: MasterError::RulesNotNewer { current, given: current } })?;
        book.set_rules(rules.clone()).map_err(HubError::from_master)?;
        store::save_book(&tx, child_id, &book, now)?;
        add_event(&tx, now, Some(child_id), &EventKind::RulesChanged { version: rules.version })?;
        tx.commit()?;
        Ok(rules)
    }
}
