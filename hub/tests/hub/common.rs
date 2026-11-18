//! What every story needs: a folder, a family, a clock, and devices on the
//! other side of the exchange.

use std::path::{Path, PathBuf};

use limo_core::{CivilDate, DeviceKind, DeviceState, Grant, RuleSet};
use limo_hub::{AgeBand, Child, Hub, Pairing};
use rusqlite::Connection;
use tempfile::TempDir;

/// A minute, in seconds.
pub const MIN: i64 = 60;
/// An hour, in seconds.
pub const HOUR: i64 = 3_600;
/// India, the home zone in every story.
pub const IST: i32 = 19_800;
/// The guardian's password in every story.
pub const PASSWORD: &str = "correct horse battery";

/// Wednesday 18 November 2026 at `hour:minute`, home time.
pub fn at(hour: u8, minute: u8) -> i64 {
    RuleSet::new(1, IST).calendar().at(CivilDate::new(2026, 11, 18).unwrap(), hour, minute, 0)
}

/// The home day of a moment.
pub fn day_of(t: i64) -> i64 {
    RuleSet::new(1, IST).calendar().day_of(t)
}

/// A temporary folder with a path for the family file in it. The folder is
/// deleted when this is dropped.
pub struct Place {
    pub dir: TempDir,
    pub path: PathBuf,
}

pub fn place() -> Place {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("family.limo");
    Place { dir, path }
}

/// A hub on a new file with a family set up.
pub fn new_family(path: &Path) -> Hub {
    let mut hub = Hub::open(path).unwrap();
    hub.create_family(PASSWORD, at(7, 0)).unwrap();
    hub
}

/// Adds a child.
pub fn child(hub: &mut Hub, name: &str) -> Child {
    hub.add_child(name, AgeBand::TenToTwelve, IST, at(7, 0)).unwrap()
}

/// A device on the other side: its state, made from the pairing as the
/// device would make it, with its code secret.
pub struct Device {
    pub id: String,
    pub state: DeviceState,
}

pub fn pair(hub: &mut Hub, child_id: &str, name: &str, kind: DeviceKind, now: i64) -> Device {
    let platform = if kind == DeviceKind::OnMaster { "windows" } else { "android" };
    let pairing: Pairing = hub.pair_device(child_id, name, kind, platform, now).unwrap();
    let mut state = DeviceState::from_pairing(&pairing.welcome, now);
    state.set_code_secret(pairing.code_secret.clone()).unwrap();
    Device { id: pairing.device_id, state }
}

/// One exchange: the device reports, the hub answers, the device takes the
/// grant (and reports again at once when the grant says so).
pub fn exchange(hub: &mut Hub, device: &mut Device, now: i64, want_more: bool) -> Grant {
    let report = device.state.report(now, want_more);
    let grant = hub.handle_report(&report, now).unwrap();
    device.state.take_grant(&grant, now);
    if grant.acknowledge_now {
        let report = device.state.report(now, want_more);
        let again = hub.handle_report(&report, now).unwrap();
        device.state.take_grant(&again, now);
        return again;
    }
    grant
}

/// The device uses `app` from `from` to `to` and reports at `to`.
pub fn use_app(hub: &mut Hub, device: &mut Device, app: &str, from: i64, to: i64) -> Grant {
    device.state.record_use(app, from, to);
    exchange(hub, device, to, true)
}

/// A plain connection to the file, for looking inside it with the hub
/// closed.
pub fn raw(path: &Path) -> Connection {
    Connection::open(path).unwrap()
}

/// Rows in a table that match a condition.
pub fn count(conn: &Connection, table: &str, condition: &str) -> i64 {
    conn.query_row(&format!("SELECT count(*) FROM {table} WHERE {condition}"), [], |r| r.get(0)).unwrap()
}

/// True when the file holds these bytes anywhere, in use or not.
pub fn file_holds(path: &Path, needle: &str) -> bool {
    let bytes = std::fs::read(path).unwrap();
    bytes.windows(needle.len()).any(|w| w == needle.as_bytes())
}

/// Messages a guardian may be shown are plain sentences with no dash
/// punctuation.
pub fn plain(message: &str) {
    assert!(message.ends_with('.'), "{message}");
    assert!(!message.contains(" - ") && !message.contains('\u{2014}') && !message.contains('\u{2013}'), "{message}");
}
