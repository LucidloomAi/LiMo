//! Reading and writing rows. Every function takes a connection, which is a
//! transaction whenever something is written, so a caller composes them into
//! one all or nothing change.

use limo_core::{DeviceKind, DeviceState, MasterBook, PinRecord};
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::KEY_BYTES;
use crate::error::HubError;
use crate::password::PasswordRecord;
use crate::random;
use crate::types::{AgeBand, Child, EventKind, PhoneForm};

/// Unsigned seconds as SQLite's signed integers, without wrapping.
pub(crate) fn to_i64(n: u64) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

/// SQLite's signed integers as unsigned, with anything below zero as zero.
pub(crate) fn to_u64(n: i64) -> u64 {
    u64::try_from(n).unwrap_or(0)
}

/// A count as a `u32`, without wrapping.
pub(crate) fn to_u32(n: i64) -> u32 {
    u32::try_from(n.max(0)).unwrap_or(u32::MAX)
}

/// The kind as stored.
pub(crate) fn kind_key(kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::OnMaster => "on_master",
        DeviceKind::Portable => "portable",
    }
}

fn kind_from_key(key: &str) -> Result<DeviceKind, HubError> {
    match key {
        "on_master" => Ok(DeviceKind::OnMaster),
        "portable" => Ok(DeviceKind::Portable),
        other => Err(HubError::damaged(format!("a device has an unknown kind {other:?}"))),
    }
}

/// What the family row holds about the password and the PIN.
pub(crate) struct FamilyRow {
    pub(crate) password: PasswordRecord,
    pub(crate) wrong_passwords: u32,
    pub(crate) password_wait_until: Option<i64>,
    pub(crate) pin: Option<PinRecord>,
    /// Wrong guesses of the PIN typed on the master PC itself.
    pub(crate) wrong_pins: u32,
    /// The end of the wait they set.
    pub(crate) pin_wait_until: Option<i64>,
}

/// The family row, or [`HubError::NoFamily`].
pub(crate) fn family(conn: &Connection) -> Result<FamilyRow, HubError> {
    let row = conn
        .query_row(
            "SELECT password_salt, password_rounds, password_hash, wrong_passwords, password_wait_until, pin_record,
                 wrong_pins, pin_wait_until
             FROM family WHERE id = 1",
            [],
            |row| {
                Ok((
                    (row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?, row.get::<_, Vec<u8>>(2)?),
                    (row.get::<_, i64>(3)?, row.get::<_, Option<i64>>(4)?),
                    row.get::<_, Option<String>>(5)?,
                    (row.get::<_, i64>(6)?, row.get::<_, Option<i64>>(7)?),
                ))
            },
        )
        .optional()?;
    let Some(((salt, rounds, hash), (wrong, wait_until), pin, (wrong_pins, pin_wait_until))) = row else {
        return Err(HubError::NoFamily);
    };
    let pin = match pin {
        Some(json) => Some(serde_json::from_str::<PinRecord>(&json)?),
        None => None,
    };
    Ok(FamilyRow {
        password: PasswordRecord { salt, rounds: to_u32(rounds), hash },
        wrong_passwords: to_u32(wrong),
        password_wait_until: wait_until,
        pin,
        wrong_pins: to_u32(wrong_pins),
        pin_wait_until,
    })
}

/// True when a family is set up.
pub(crate) fn has_family(conn: &Connection) -> Result<bool, HubError> {
    Ok(conn.query_row("SELECT 1 FROM family WHERE id = 1", [], |_| Ok(())).optional()?.is_some())
}

/// [`HubError::NoFamily`] unless a family is set up.
pub(crate) fn require_family(conn: &Connection) -> Result<(), HubError> {
    if has_family(conn)? { Ok(()) } else { Err(HubError::NoFamily) }
}

/// The columns [`child_from_row`] reads, in order.
pub(crate) const CHILD_COLUMNS: &str = "id, name, age_band, added_at";

/// A child from a row of [`CHILD_COLUMNS`].
pub(crate) fn child_from_row(row: &Row<'_>) -> rusqlite::Result<(String, String, String, i64)> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
}

/// Makes a [`Child`] from what [`child_from_row`] read.
pub(crate) fn make_child(raw: (String, String, String, i64)) -> Result<Child, HubError> {
    let (child_id, name, band, added_at) = raw;
    let age_band =
        AgeBand::from_key(&band).ok_or_else(|| HubError::damaged(format!("a child has an unknown age band {band:?}")))?;
    Ok(Child { child_id, name, age_band, added_at })
}

/// The child with this id, or [`HubError::UnknownChild`].
pub(crate) fn child(conn: &Connection, child_id: &str) -> Result<Child, HubError> {
    let raw = conn
        .query_row(&format!("SELECT {CHILD_COLUMNS} FROM child WHERE id = ?1"), [child_id], child_from_row)
        .optional()?
        .ok_or_else(|| HubError::UnknownChild { child_id: child_id.to_string() })?;
    make_child(raw)
}

/// Every child, in the order they were added.
pub(crate) fn children(conn: &Connection) -> Result<Vec<Child>, HubError> {
    let mut stmt = conn.prepare(&format!("SELECT {CHILD_COLUMNS} FROM child ORDER BY seq"))?;
    let raws = stmt.query_map([], child_from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    raws.into_iter().map(make_child).collect()
}

/// A child's book, from its saved JSON.
pub(crate) fn load_book(conn: &Connection, child_id: &str) -> Result<MasterBook, HubError> {
    let json: String = conn
        .query_row("SELECT json FROM book WHERE child_id = ?1", [child_id], |row| row.get(0))
        .optional()?
        .ok_or_else(|| HubError::UnknownChild { child_id: child_id.to_string() })?;
    let book: MasterBook = serde_json::from_str(&json)?;
    if book.child_id() != child_id {
        return Err(HubError::damaged("a child's book belongs to another child"));
    }
    Ok(book)
}

/// A book as JSON.
fn book_json(book: &MasterBook) -> Result<String, HubError> {
    serde_json::to_string(book).map_err(|_| HubError::internal("a book could not be written as JSON"))
}

/// Writes a child's book, and adds one to its revision.
pub(crate) fn save_book(conn: &Connection, child_id: &str, book: &MasterBook, now: i64) -> Result<(), HubError> {
    conn.execute(
        "INSERT INTO book (child_id, json, saved_at, revision) VALUES (?1, ?2, ?3, 1)
         ON CONFLICT (child_id) DO UPDATE SET
             json = excluded.json, saved_at = excluded.saved_at, revision = book.revision + 1",
        params![child_id, book_json(book)?, now],
    )?;
    Ok(())
}

/// Writes a child's book in a file of version 2, which has no revisions:
/// for the work of the step to version 2 only, which runs before the step
/// to version 3 has added the column.
pub(crate) fn save_book_before_revisions(
    conn: &Connection,
    child_id: &str,
    book: &MasterBook,
    now: i64,
) -> Result<(), HubError> {
    conn.execute(
        "INSERT INTO book (child_id, json, saved_at) VALUES (?1, ?2, ?3)
         ON CONFLICT (child_id) DO UPDATE SET json = excluded.json, saved_at = excluded.saved_at",
        params![child_id, book_json(book)?, now],
    )?;
    Ok(())
}

/// A child's book revision: it grows by one every time the book is written.
pub(crate) fn book_revision(conn: &Connection, child_id: &str) -> Result<u64, HubError> {
    let revision: i64 = conn
        .query_row("SELECT revision FROM book WHERE child_id = ?1", [child_id], |row| row.get(0))
        .optional()?
        .ok_or_else(|| HubError::UnknownChild { child_id: child_id.to_string() })?;
    Ok(to_u64(revision))
}

/// What the hub keeps for a child's device on the master PC: the device's
/// state and the agent's notes, both as JSON.
pub(crate) struct MasterPcRow {
    pub(crate) device_id: String,
    pub(crate) child_id: String,
    pub(crate) state: DeviceState,
    pub(crate) notes: String,
}

/// The saved state of a device on the master PC, if there is one.
pub(crate) fn master_pc_row(conn: &Connection, device_id: &str) -> Result<Option<MasterPcRow>, HubError> {
    let raw: Option<(String, String, String)> = conn
        .query_row("SELECT child_id, state, notes FROM master_pc WHERE device_id = ?1", [device_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .optional()?;
    let Some((child_id, state, notes)) = raw else { return Ok(None) };
    let state: DeviceState = serde_json::from_str(&state)?;
    if state.device_id() != device_id || state.child_id() != child_id {
        return Err(HubError::damaged("a device state on the master PC belongs to another device"));
    }
    Ok(Some(MasterPcRow { device_id: device_id.to_string(), child_id, state, notes }))
}

/// Writes the saved state of a device on the master PC, creating the row
/// when there is none.
pub(crate) fn save_master_pc_row(
    conn: &Connection,
    state: &DeviceState,
    notes: &str,
    now: i64,
) -> Result<(), HubError> {
    let json = serde_json::to_string(state)
        .map_err(|_| HubError::internal("a device state could not be written as JSON"))?;
    conn.execute(
        "INSERT INTO master_pc (device_id, child_id, state, notes, saved_at) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT (device_id) DO UPDATE SET
             state = excluded.state, notes = excluded.notes, saved_at = excluded.saved_at",
        params![state.device_id(), state.child_id(), json, notes, now],
    )?;
    Ok(())
}

/// A device row: what the hub keeps of a device beside the child's book.
/// The rest (its rules version, news taken, wrong guesses, clock) the book
/// answers ([`limo_core::MasterBook::device_view`]); `last_heard_at` is a
/// copy of the book's figure, kept so that devices can be listed without
/// reading the book.
#[derive(Debug, Clone)]
pub(crate) struct DeviceRow {
    pub(crate) id: String,
    pub(crate) child_id: String,
    pub(crate) name: String,
    pub(crate) kind: DeviceKind,
    pub(crate) platform: String,
    pub(crate) paired_at: i64,
    pub(crate) removed_at: Option<i64>,
    pub(crate) last_heard_at: Option<i64>,
    pub(crate) in_use_at_report: bool,
}

const DEVICE_COLUMNS: &str = "id, child_id, name, kind, platform, paired_at, removed_at, last_heard_at, in_use_at_report";

type RawDevice = ((String, String, String, String, String), (i64, Option<i64>, Option<i64>, bool));

fn raw_device(row: &Row<'_>) -> rusqlite::Result<RawDevice> {
    Ok((
        (row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?),
        (row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?),
    ))
}

fn make_device(raw: RawDevice) -> Result<DeviceRow, HubError> {
    let ((id, child_id, name, kind, platform), (paired_at, removed_at, last_heard_at, in_use_at_report)) = raw;
    Ok(DeviceRow {
        id,
        child_id,
        name,
        kind: kind_from_key(&kind)?,
        platform,
        paired_at,
        removed_at,
        last_heard_at,
        in_use_at_report,
    })
}

/// The device with this id, removed or not, if it was ever paired.
pub(crate) fn device(conn: &Connection, device_id: &str) -> Result<Option<DeviceRow>, HubError> {
    let raw = conn
        .query_row(&format!("SELECT {DEVICE_COLUMNS} FROM device WHERE id = ?1"), [device_id], raw_device)
        .optional()?;
    raw.map(make_device).transpose()
}

/// The device with this id if it is paired now, or why not.
pub(crate) fn active_device(conn: &Connection, device_id: &str) -> Result<DeviceRow, HubError> {
    let row = device(conn, device_id)?.ok_or_else(|| HubError::UnknownDevice { device_id: device_id.to_string() })?;
    if row.removed_at.is_some() {
        return Err(HubError::DeviceRemoved { device_id: device_id.to_string() });
    }
    Ok(row)
}

/// A child's devices, removed ones too when asked, in the order they were
/// paired.
pub(crate) fn devices_of(conn: &Connection, child_id: &str, with_removed: bool) -> Result<Vec<DeviceRow>, HubError> {
    let filter = if with_removed { "" } else { " AND removed_at IS NULL" };
    let mut stmt = conn.prepare(&format!(
        "SELECT {DEVICE_COLUMNS} FROM device WHERE child_id = ?1{filter} ORDER BY seq"
    ))?;
    let raws = stmt.query_map([child_id], raw_device)?.collect::<rusqlite::Result<Vec<_>>>()?;
    raws.into_iter().map(make_device).collect()
}

/// A new id with this prefix that `taken` says is free. With 64 random bits
/// a clash is all but impossible; it is still checked, and a few tries are
/// made.
pub(crate) fn new_id(conn: &Connection, prefix: &str, taken: &str) -> Result<String, HubError> {
    for _ in 0..8 {
        let id = random::id(prefix)?;
        let used = conn.query_row(taken, [&id], |_| Ok(())).optional()?.is_some();
        if !used {
            return Ok(id);
        }
    }
    Err(HubError::NoRandomness)
}

/// Records an event.
pub(crate) fn add_event(conn: &Connection, at: i64, child_id: Option<&str>, what: &EventKind) -> Result<(), HubError> {
    let detail = serde_json::to_string(what).map_err(|_| HubError::internal("an event could not be written as JSON"))?;
    conn.execute(
        "INSERT INTO event (at, child_id, kind, detail) VALUES (?1, ?2, ?3, ?4)",
        params![at, child_id, what.key(), detail],
    )?;
    Ok(())
}

/// Records an event unless the same event, to the letter, is already on
/// record. True when it was recorded now.
pub(crate) fn note_once(conn: &Connection, child_id: Option<&str>, what: &EventKind, now: i64) -> Result<bool, HubError> {
    let detail = serde_json::to_string(what).map_err(|_| HubError::internal("an event could not be written as JSON"))?;
    let seen = conn
        .query_row("SELECT 1 FROM event WHERE kind = ?1 AND detail = ?2", params![what.key(), detail], |_| Ok(()))
        .optional()?
        .is_some();
    if seen {
        return Ok(false);
    }
    add_event(conn, now, child_id, what)?;
    Ok(true)
}

/// Adds the row the hub keeps beside a device that can be carried away: its
/// link key if the link gave one, the form of the app and its Android
/// version, and no notes yet.
pub(crate) fn add_phone(
    conn: &Connection,
    device_id: &str,
    child_id: &str,
    link_key: Option<&[u8; KEY_BYTES]>,
    form: PhoneForm,
    android_version: Option<u32>,
) -> Result<(), HubError> {
    conn.execute(
        "INSERT INTO phone (device_id, child_id, link_key, form, android_version, notes, released_at)
         VALUES (?1, ?2, ?3, ?4, ?5, '[]', NULL)",
        params![device_id, child_id, link_key.map(|k| &k[..]), form.key(), android_version],
    )?;
    Ok(())
}
