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
use zeroize::Zeroizing;

use crate::error::HubError;
use crate::{DEFAULT_DETAIL_DAYS, KEY_BYTES};
use crate::{random, store};

/// The version of the file this code writes. A file of a later version is
/// refused, unchanged.
pub const SCHEMA_VERSION: u32 = 4;

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

/// Version 4: what the link between the phones and the master PC needs.
///
/// - `family.master_key`: the master's own private key for the link, 32
///   random bytes, made with the family. The link makes the public key from
///   it.
/// - `offer`: the pairing offers the service made, each for one child: its
///   id, its text form as the link wrote it (which holds its secret), when it
///   was made and when it expires, and when it was used and by which device.
/// - `phone`: for each device that can be carried away, its link key (the
///   phone's public key, none for a phone paired before the link), the form
///   of the app, its Android version, the newest of each kind of note it
///   sent, and when it was told that it was removed.
/// - `phone_apps`: each phone's list of apps with their labels, and the hash
///   the phone gave it.
/// - `family.wrong_pins` and `family.pin_wait_until`: the wrong guesses of
///   the guardian PIN typed on the master PC itself (on the lock of a Windows
///   account linked to no child), and the end of the wait they set.
const V4: &str = "
ALTER TABLE family ADD COLUMN master_key BLOB CHECK (master_key IS NULL OR length(master_key) = 32);
ALTER TABLE family ADD COLUMN wrong_pins INTEGER NOT NULL DEFAULT 0;
ALTER TABLE family ADD COLUMN pin_wait_until INTEGER;

CREATE TABLE offer (
    id TEXT PRIMARY KEY,
    child_id TEXT NOT NULL REFERENCES child(id) ON DELETE CASCADE,
    offer_text TEXT NOT NULL,
    made_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    used_at INTEGER,
    device_id TEXT REFERENCES device(id) ON DELETE CASCADE
);
CREATE INDEX offer_by_child ON offer(child_id);
CREATE INDEX offer_by_device ON offer(device_id);

CREATE TABLE phone (
    device_id TEXT PRIMARY KEY REFERENCES device(id) ON DELETE CASCADE,
    child_id TEXT NOT NULL REFERENCES child(id) ON DELETE CASCADE,
    link_key BLOB UNIQUE CHECK (link_key IS NULL OR length(link_key) = 32),
    form TEXT NOT NULL CHECK (form IN ('child', 'guardian')),
    android_version INTEGER,
    notes TEXT NOT NULL,
    released_at INTEGER
);
CREATE INDEX phone_by_child ON phone(child_id);

CREATE TABLE phone_apps (
    device_id TEXT PRIMARY KEY REFERENCES phone(device_id) ON DELETE CASCADE,
    child_id TEXT NOT NULL REFERENCES child(id) ON DELETE CASCADE,
    apps TEXT NOT NULL,
    hash TEXT NOT NULL,
    kept_at INTEGER NOT NULL
);
CREATE INDEX phone_apps_by_child ON phone_apps(child_id);
";

/// Work that a step's SQL cannot do.
type Work = fn(&Connection) -> Result<(), HubError>;

/// One step from a version to the next: SQL, and work that SQL cannot do.
#[derive(Clone, Copy)]
pub(crate) struct Step {
    sql: &'static str,
    work: Option<Work>,
}

/// Every step from one version to the next: entry `i` moves a file from
/// version `i` to version `i + 1`. Never change an entry once released; add
/// a new one.
const MIGRATIONS: &[Step] = &[
    Step { sql: V1, work: None },
    Step { sql: V2, work: Some(convert_books) },
    Step { sql: V3, work: None },
    Step { sql: V4, work: Some(add_link_records) },
];

/// Version 4's work: a family already set up gets its master key, and every
/// device that can be carried away, removed ones too, gets its row in
/// `phone`: the form of a child's phone, and no link key, since it was
/// paired before the link. Its own SQL only, so that later changes to the
/// hub's functions cannot change what this step does.
fn add_link_records(conn: &Connection) -> Result<(), HubError> {
    let key = Zeroizing::new(random::bytes(KEY_BYTES)?);
    conn.execute("UPDATE family SET master_key = ?1 WHERE id = 1 AND master_key IS NULL", [key.as_slice()])?;
    conn.execute(
        "INSERT INTO phone (device_id, child_id, link_key, form, android_version, notes, released_at)
         SELECT id, child_id, NULL, 'child', NULL, '[]', NULL FROM device WHERE kind = 'portable'",
        [],
    )?;
    Ok(())
}

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
        if let Some(work) = step.work {
            work(&tx)?;
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

        bring_up_to_date(&mut conn, &MIGRATIONS[..3]).unwrap();
        assert_eq!(version(&conn), 3);
        assert!(has_column(&conn, "book", "revision") && has_column(&conn, "master_pc", "notes"));
        let (idle, unlinked): (i64, String) = conn
            .query_row("SELECT idle_minutes, unlinked_accounts FROM settings", [], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap();
        assert_eq!((idle, unlinked.as_str()), (crate::DEFAULT_IDLE_MINUTES as i64, "free"));
        assert!(!has_column(&conn, "family", "master_key"));

        bring_up_to_date(&mut conn, MIGRATIONS).unwrap();
        assert_eq!(version(&conn), 4);
        assert!(has_column(&conn, "family", "master_key") && has_column(&conn, "offer", "offer_text"));
        assert!(has_column(&conn, "phone", "link_key") && has_column(&conn, "phone_apps", "hash"));

        let five = [MIGRATIONS[0], MIGRATIONS[1], MIGRATIONS[2], MIGRATIONS[3], Step { sql: TEST_NEXT, work: None }];
        bring_up_to_date(&mut conn, &five).unwrap();
        assert_eq!(version(&conn), 5);
        let probe: i64 = conn.query_row("SELECT probe FROM settings", [], |row| row.get(0)).unwrap();
        let days: i64 = conn.query_row("SELECT detail_days FROM settings", [], |row| row.get(0)).unwrap();
        assert_eq!((probe, days), (7, DEFAULT_DETAIL_DAYS as i64));
        // Opening it again changes nothing.
        bring_up_to_date(&mut conn, &five).unwrap();
        assert_eq!(version(&conn), 5);
        drop(conn);

        // This code knows up to version 4, so the file is now too new for it.
        assert_eq!(open(&path).unwrap_err(), HubError::FileTooNew { file_version: 5, known_version: 4 });
    }

    #[test]
    fn version_four_gives_a_family_its_master_key_and_each_phone_its_row() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = Connection::open(dir.path().join("family.limo")).unwrap();
        bring_up_to_date(&mut conn, &MIGRATIONS[..3]).unwrap();
        conn.execute_batch(
            "INSERT INTO family (id, created_at, password_salt, password_rounds, password_hash, password_changed_at,
                 wrong_passwords) VALUES (1, 0, x'00', 1, x'00', 0, 0);
             INSERT INTO child (id, seq, name, age_band, added_at) VALUES ('child-1', 1, 'Asha', 'under_six', 0);
             INSERT INTO device (id, seq, child_id, name, kind, platform, paired_at, removed_at, in_use_at_report)
                 VALUES ('device-pc', 1, 'child-1', 'PC', 'on_master', 'windows', 0, NULL, 0),
                        ('device-phone', 2, 'child-1', 'Phone', 'portable', 'android', 0, NULL, 0),
                        ('device-old', 3, 'child-1', 'Old', 'portable', 'android', 0, 5, 0);",
        )
        .unwrap();
        bring_up_to_date(&mut conn, MIGRATIONS).unwrap();
        let key: Vec<u8> = conn.query_row("SELECT master_key FROM family", [], |row| row.get(0)).unwrap();
        assert_eq!(key.len(), KEY_BYTES);
        assert_ne!(key, vec![0u8; KEY_BYTES]);
        let mut stmt = conn.prepare("SELECT device_id, link_key IS NULL, form, notes FROM phone ORDER BY device_id").unwrap();
        let phones: Vec<(String, bool, String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        let none = |id: &str| (id.to_string(), true, "child".to_string(), "[]".to_string());
        assert_eq!(phones, vec![none("device-old"), none("device-phone")]);
        // The key is a key, and nothing else: its length is checked by the file.
        assert!(conn.execute("UPDATE family SET master_key = x'0102'", []).is_err());
        assert!(conn.execute("UPDATE phone SET link_key = x'0102'", []).is_err());
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
