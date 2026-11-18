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
use crate::store;

/// The version of the file this code writes. A file of a later version is
/// refused, unchanged.
pub const SCHEMA_VERSION: u32 = 3;

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

/// Version 2: the rules core answers what the hub kept of each device's
/// newest report (its message number, the rules version it holds, its wrong
/// guesses, how far off its clock is), so those columns go. The books are
/// converted too, by [`convert_books`].
const V2: &str = "
ALTER TABLE device DROP COLUMN last_message;
ALTER TABLE device DROP COLUMN rules_version_seen;
ALTER TABLE device DROP COLUMN wrong_guesses;
ALTER TABLE device DROP COLUMN clock_off_by;
";

/// Version 3: what the agent on the master PC needs.
///
/// - `book.revision` grows by one every time a child's book is written, so
///   that the agent can tell, with one small read, that a guardian or another
///   device changed the book since its own last exchange.
/// - `settings.idle_minutes`: after how many minutes with no keyboard or
///   mouse input the count pauses on the master PC (3 unless changed).
/// - `settings.unlinked_accounts`: what happens on the master PC in a Windows
///   account that is linked to no child and not marked as not limited:
///   `free` (the default) or `locked`.
/// - `master_pc`: the device state the agent keeps for each child's device
///   on the master PC (the core's `DeviceState`, as JSON), with the agent's
///   own notes, written in the same transactions as the child's book.
const V3: &str = "
ALTER TABLE book ADD COLUMN revision INTEGER NOT NULL DEFAULT 0;
ALTER TABLE settings ADD COLUMN idle_minutes INTEGER NOT NULL DEFAULT 3;
ALTER TABLE settings ADD COLUMN unlinked_accounts TEXT NOT NULL DEFAULT 'free'
    CHECK (unlinked_accounts IN ('free', 'locked'));

CREATE TABLE master_pc (
    device_id TEXT PRIMARY KEY REFERENCES device(id) ON DELETE CASCADE,
    child_id TEXT NOT NULL REFERENCES child(id) ON DELETE CASCADE,
    state TEXT NOT NULL,
    notes TEXT NOT NULL,
    saved_at INTEGER NOT NULL
);
CREATE INDEX master_pc_by_child ON master_pc(child_id);
";

/// Work on the books that a step's SQL cannot do.
type BookWork = fn(&Connection) -> Result<(), HubError>;

/// One step from a version to the next: SQL, and work on the books that SQL
/// cannot do.
#[derive(Clone, Copy)]
pub(crate) struct Step {
    sql: &'static str,
    books: Option<BookWork>,
}

/// Every step from one version to the next: entry `i` moves a file from
/// version `i` to version `i + 1`. Never change an entry once released; add
/// a new one.
const MIGRATIONS: &[Step] = &[
    Step { sql: V1, books: None },
    Step { sql: V2, books: Some(convert_books) },
    Step { sql: V3, books: None },
];

/// Version 2's work on the books: every child's book is loaded (a book saved
/// with whole slices is folded into marks as it loads) and saved in the
/// core's present shape. A device the first version removed was kept in the
/// book as a retired record; it is now removed by the core itself, at the
/// moment the hub removed it. Its counted use stays with the days the book
/// keeps. The retired record no longer says how much of the time added on
/// it by a code or the PIN it used, so none of that is dropped: on those days
/// (today and yesterday at most) it stays in the day's extra time.
///
/// It reads the tables through the hub's own functions, so a later version
/// that changes the `child`, `device` or `book` tables must keep this step
/// working on a version 2 file, before its own step runs. Version 3 added
/// `book.revision`, so this step writes the books with
/// [`store::save_book_before_revisions`], which writes the columns a version
/// 2 file has.
fn convert_books(conn: &Connection) -> Result<(), HubError> {
    for child in store::children(conn)? {
        let mut book = store::load_book(conn, &child.child_id)?;
        let live: Vec<String> = book.device_ids();
        for row in store::devices_of(conn, &child.child_id, true)? {
            if let Some(removed_at) = row.removed_at
                && live.contains(&row.id)
            {
                book.remove_device(&row.id, removed_at).map_err(HubError::from_master)?;
            }
        }
        let saved_at: i64 = conn.query_row("SELECT saved_at FROM book WHERE child_id = ?1", [&child.child_id], |r| r.get(0))?;
        store::save_book_before_revisions(conn, &child.child_id, &book, saved_at)?;
    }
    Ok(())
}

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
pub(crate) fn bring_up_to_date(conn: &mut Connection, migrations: &[Step]) -> Result<(), HubError> {
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
        tx.execute_batch(step.sql)?;
        if let Some(books) = step.books {
            books(&tx)?;
        }
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

    /// A version after this code's that only adds a column, to show that a
    /// file is carried forward by the migration path.
    const TEST_NEXT: &str = "ALTER TABLE settings ADD COLUMN probe INTEGER NOT NULL DEFAULT 7;";

    fn version(conn: &Connection) -> i64 {
        conn.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap()
    }

    fn has_column(conn: &Connection, table: &str, column: &str) -> bool {
        conn.query_row(
            "SELECT count(*) FROM pragma_table_info(?1) WHERE name = ?2",
            [table, column],
            |row| row.get::<_, i64>(0),
        )
        .unwrap()
            == 1
    }

    #[test]
    fn a_version_one_file_is_carried_forward_step_by_step() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("family.limo");
        // A version 1 file, as the first version of the hub made it.
        let mut conn = Connection::open(&path).unwrap();
        bring_up_to_date(&mut conn, &MIGRATIONS[..1]).unwrap();
        assert_eq!(version(&conn), 1);
        assert!(has_column(&conn, "device", "rules_version_seen"));

        bring_up_to_date(&mut conn, &MIGRATIONS[..2]).unwrap();
        assert_eq!(version(&conn), 2);
        for column in ["last_message", "rules_version_seen", "wrong_guesses", "clock_off_by"] {
            assert!(!has_column(&conn, "device", column), "{column}");
        }
        assert!(has_column(&conn, "device", "in_use_at_report"));
        assert!(!has_column(&conn, "book", "revision"));

        bring_up_to_date(&mut conn, MIGRATIONS).unwrap();
        assert_eq!(version(&conn), 3);
        assert!(has_column(&conn, "book", "revision") && has_column(&conn, "master_pc", "notes"));
        let (idle, unlinked): (i64, String) = conn
            .query_row("SELECT idle_minutes, unlinked_accounts FROM settings", [], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap();
        assert_eq!((idle, unlinked.as_str()), (crate::DEFAULT_IDLE_MINUTES as i64, "free"));

        let four = [MIGRATIONS[0], MIGRATIONS[1], MIGRATIONS[2], Step { sql: TEST_NEXT, books: None }];
        bring_up_to_date(&mut conn, &four).unwrap();
        assert_eq!(version(&conn), 4);
        let probe: i64 = conn.query_row("SELECT probe FROM settings", [], |row| row.get(0)).unwrap();
        let days: i64 = conn.query_row("SELECT detail_days FROM settings", [], |row| row.get(0)).unwrap();
        assert_eq!((probe, days), (7, DEFAULT_DETAIL_DAYS as i64));
        // Opening it again changes nothing.
        bring_up_to_date(&mut conn, &four).unwrap();
        assert_eq!(version(&conn), 4);
        drop(conn);

        // This code knows up to version 3, so the file is now too new for it.
        assert_eq!(open(&path).unwrap_err(), HubError::FileTooNew { file_version: 4, known_version: 3 });
    }

    #[test]
    fn the_settings_of_version_three_refuse_a_value_they_do_not_know() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(&dir.path().join("family.limo")).unwrap();
        assert!(conn.execute("UPDATE settings SET unlinked_accounts = 'open'", []).is_err());
        conn.execute("UPDATE settings SET unlinked_accounts = 'locked'", []).unwrap();
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
