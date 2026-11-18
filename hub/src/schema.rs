//! The file: how it is opened, its tables, and how it moves between versions.
//!
//! The file's version is SQLite's `user_version`. A file is recognised as
//! LiMo's by its `application_id`. Each entry of [`MIGRATIONS`] moves a file
//! from one version to the next; all of them that a file needs run in one
//! transaction when it is opened. A file with a version newer than this code
//! knows is refused before anything is written to it.

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, TransactionBehavior};

use crate::DEFAULT_DETAIL_DAYS;
use crate::error::HubError;

/// The version of the file this code writes. A file of a later version is
/// refused, unchanged.
pub const SCHEMA_VERSION: u32 = 1;

/// "LiMo" in ASCII: marks the file as a LiMo family file.
pub(crate) const APPLICATION_ID: i32 = 0x4C69_4D6F;

/// Version 1: the first tables.
const V1: &str = "
CREATE TABLE settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    detail_days INTEGER NOT NULL
);

CREATE TABLE family (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    created_at INTEGER NOT NULL,
    password_salt BLOB NOT NULL,
    password_rounds INTEGER NOT NULL,
    password_hash BLOB NOT NULL,
    password_changed_at INTEGER NOT NULL,
    wrong_passwords INTEGER NOT NULL,
    password_wait_until INTEGER,
    pin_record TEXT,
    pin_changed_at INTEGER
);

CREATE TABLE child (
    id TEXT PRIMARY KEY,
    seq INTEGER NOT NULL,
    name TEXT NOT NULL,
    age_band TEXT NOT NULL,
    added_at INTEGER NOT NULL,
    folded_before INTEGER
);

CREATE TABLE book (
    child_id TEXT PRIMARY KEY REFERENCES child(id) ON DELETE CASCADE,
    json TEXT NOT NULL,
    saved_at INTEGER NOT NULL
);

CREATE TABLE device (
    id TEXT PRIMARY KEY,
    seq INTEGER NOT NULL,
    child_id TEXT NOT NULL REFERENCES child(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    platform TEXT NOT NULL,
    paired_at INTEGER NOT NULL,
    removed_at INTEGER,
    last_heard_at INTEGER,
    last_message INTEGER NOT NULL,
    rules_version_seen INTEGER NOT NULL,
    in_use_at_report INTEGER NOT NULL,
    wrong_guesses INTEGER NOT NULL,
    clock_off_by INTEGER
);
CREATE INDEX device_by_child ON device(child_id);

CREATE TABLE slice (
    child_id TEXT NOT NULL REFERENCES child(id) ON DELETE CASCADE,
    id TEXT NOT NULL,
    device_id TEXT NOT NULL REFERENCES device(id) ON DELETE CASCADE,
    app_id TEXT NOT NULL,
    day INTEGER NOT NULL,
    start_at INTEGER NOT NULL,
    end_at INTEGER NOT NULL,
    secs INTEGER NOT NULL,
    counts INTEGER NOT NULL,
    PRIMARY KEY (child_id, id)
) WITHOUT ROWID;
CREATE INDEX slice_by_day ON slice(child_id, day);
CREATE INDEX slice_by_device ON slice(device_id);

CREATE TABLE day_total (
    child_id TEXT NOT NULL REFERENCES child(id) ON DELETE CASCADE,
    day INTEGER NOT NULL,
    device_id TEXT NOT NULL REFERENCES device(id) ON DELETE CASCADE,
    app_id TEXT NOT NULL,
    secs INTEGER NOT NULL,
    counted_secs INTEGER NOT NULL,
    PRIMARY KEY (child_id, day, device_id, app_id)
) WITHOUT ROWID;
CREATE INDEX day_total_by_device ON day_total(device_id);

CREATE TABLE code_made (
    device_id TEXT NOT NULL REFERENCES device(id) ON DELETE CASCADE,
    day INTEGER NOT NULL,
    counter INTEGER NOT NULL,
    minutes INTEGER NOT NULL,
    made_at INTEGER NOT NULL,
    PRIMARY KEY (device_id, day, counter)
) WITHOUT ROWID;

CREATE TABLE account (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    child_id TEXT REFERENCES child(id) ON DELETE CASCADE
);
CREATE INDEX account_by_child ON account(child_id);

CREATE TABLE event (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    at INTEGER NOT NULL,
    child_id TEXT REFERENCES child(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    detail TEXT NOT NULL
);
CREATE INDEX event_by_child ON event(child_id);
CREATE INDEX event_by_time ON event(at);
";

/// Every step from one version to the next: entry `i` moves a file from
/// version `i` to version `i + 1`. Never change an entry once released; add
/// a new one.
const MIGRATIONS: &[&str] = &[V1];

/// Opens the file, creating it when it does not exist, brings it up to date,
/// and keeps it locked for this connection alone until the connection
/// closes.
pub(crate) fn open(path: &Path) -> Result<Connection, HubError> {
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let mut conn = Connection::open_with_flags(path, flags)?;
    // A second hub on the same file is refused at once, not after a wait.
    conn.busy_timeout(Duration::ZERO)?;
    // Once this connection has written, it keeps the file locked until it
    // closes: no other program can read or write it meanwhile.
    conn.pragma_update(None, "locking_mode", "EXCLUSIVE")?;
    // A rollback journal, emptied after every transaction: the file alone
    // always holds everything committed, so it is safe to copy while the
    // service is stopped, and old pages do not linger in the journal.
    let mode: String = conn.pragma_update_and_check(None, "journal_mode", "TRUNCATE", |row| row.get(0))?;
    if !mode.eq_ignore_ascii_case("truncate") {
        return Err(HubError::Storage { detail: format!("the journal mode could not be set; it is {mode}") });
    }
    // A commit returns only once it is on the disk.
    conn.pragma_update(None, "synchronous", "FULL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    // Deleted rows are overwritten with zeros, not left in free space.
    conn.pragma_update(None, "secure_delete", "ON")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    bring_up_to_date(&mut conn, MIGRATIONS)?;
    Ok(conn)
}

/// Checks the file and runs the migrations it needs, in one exclusive
/// transaction. The transaction also takes the lock this connection keeps.
pub(crate) fn bring_up_to_date(conn: &mut Connection, migrations: &[&str]) -> Result<(), HubError> {
    let known = u32::try_from(migrations.len()).unwrap_or(u32::MAX);
    let tx = conn.transaction_with_behavior(TransactionBehavior::Exclusive)?;
    let application_id: i32 = tx.pragma_query_value(None, "application_id", |row| row.get(0))?;
    let version: i64 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let tables: i64 = tx.query_row("SELECT count(*) FROM sqlite_schema", [], |row| row.get(0))?;
    let fresh = application_id == 0 && version == 0 && tables == 0;
    if !fresh && application_id != APPLICATION_ID {
        return Err(HubError::NotAFamilyFile);
    }
    let version = u32::try_from(version).map_err(|_| HubError::NotAFamilyFile)?;
    if version > known {
        // Nothing has been written: the transaction is dropped unchanged.
        return Err(HubError::FileTooNew { file_version: version, known_version: known });
    }
    for step in migrations.iter().skip(version as usize) {
        tx.execute_batch(step)?;
    }
    if fresh {
        tx.pragma_update(None, "application_id", APPLICATION_ID)?;
        tx.execute("INSERT INTO settings (id, detail_days) VALUES (1, ?1)", [DEFAULT_DETAIL_DAYS])?;
    }
    if version < known {
        tx.pragma_update(None, "user_version", known)?;
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A second version that only adds a column, to show that a version 1
    /// file is carried forward by the migration path.
    const TEST_V2: &str = "ALTER TABLE settings ADD COLUMN probe INTEGER NOT NULL DEFAULT 7;";

    fn version(conn: &Connection) -> i64 {
        conn.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap()
    }

    #[test]
    fn a_version_one_file_is_carried_to_version_two() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("family.limo");
        drop(open(&path).unwrap());

        let mut conn = Connection::open(&path).unwrap();
        assert_eq!(version(&conn), 1);
        bring_up_to_date(&mut conn, &[V1, TEST_V2]).unwrap();
        assert_eq!(version(&conn), 2);
        let probe: i64 = conn.query_row("SELECT probe FROM settings", [], |row| row.get(0)).unwrap();
        let days: i64 = conn.query_row("SELECT detail_days FROM settings", [], |row| row.get(0)).unwrap();
        assert_eq!((probe, days), (7, DEFAULT_DETAIL_DAYS as i64));
        // Opening it again changes nothing.
        bring_up_to_date(&mut conn, &[V1, TEST_V2]).unwrap();
        assert_eq!(version(&conn), 2);
        drop(conn);

        // This code knows only version 1, so the file is now too new for it.
        assert_eq!(open(&path).unwrap_err(), HubError::FileTooNew { file_version: 2, known_version: 1 });
    }

    #[test]
    fn a_fresh_file_gets_every_table_and_its_marks() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(&dir.path().join("family.limo")).unwrap();
        let id: i32 = conn.pragma_query_value(None, "application_id", |row| row.get(0)).unwrap();
        assert_eq!(id, APPLICATION_ID);
        assert_eq!(version(&conn), SCHEMA_VERSION as i64);
        let fk: i64 = conn.pragma_query_value(None, "foreign_keys", |row| row.get(0)).unwrap();
        let secure: i64 = conn.pragma_query_value(None, "secure_delete", |row| row.get(0)).unwrap();
        assert_eq!((fk, secure), (1, 1));
    }
}
