//! The file: made, closed, reopened, copied, refused.

use limo_core::{DailyBudget, DeviceKind};
use limo_hub::{AccountLink, Hub, HubError, PasswordOutcome, SCHEMA_VERSION};

use crate::common::*;

/// A family with something of everything in it.
fn fill(hub: &mut Hub) -> (String, String) {
    let asha = child(hub, "Asha");
    let ravi = child(hub, "Ravi");
    let mut rules = hub.rules(&asha.child_id).unwrap();
    rules.budgets.saturday = DailyBudget::Minutes(180);
    rules.always_allowed_apps = vec!["phone".into()];
    hub.replace_rules(&asha.child_id, rules, at(7, 5)).unwrap();
    hub.set_pin("482915", at(7, 6)).unwrap();
    let mut pc = pair(hub, &asha.child_id, "Family PC", DeviceKind::OnMaster, at(7, 10));
    let mut phone = pair(hub, &asha.child_id, "Asha's phone", DeviceKind::Portable, at(7, 10));
    exchange(hub, &mut pc, at(8, 0), true);
    use_app(hub, &mut pc, "game", at(8, 0), at(8, 7));
    exchange(hub, &mut phone, at(8, 10), true);
    use_app(hub, &mut phone, "video", at(8, 10), at(8, 14));
    hub.give_extra(&asha.child_id, 20, "tap-1", at(9, 0)).unwrap();
    hub.link_account("S-1-5-21-1001", "asha", AccountLink::Child { child_id: asha.child_id.clone() }).unwrap();
    hub.link_account("S-1-5-21-1000", "Mum", AccountLink::NotLimited).unwrap();
    hub.lock_now(&ravi.child_id, Some(at(20, 0)), at(9, 30)).unwrap();
    (asha.child_id, ravi.child_id)
}

#[test]
fn a_file_made_today_reopens_with_everything_in_it() {
    let place = place();
    let mut hub = new_family(&place.path);
    let (asha, ravi) = fill(&mut hub);
    let now = at(10, 0);
    let today = day_of(now);
    let before = (
        hub.children().unwrap(),
        hub.rules(&asha).unwrap(),
        hub.devices(&asha).unwrap(),
        hub.family_today(now).unwrap(),
        hub.usage(&asha, today, today).unwrap(),
        hub.extra_time_today(&asha, now).unwrap(),
        hub.accounts().unwrap(),
        hub.events(100, None).unwrap(),
        hub.wanted_reports(now).unwrap(),
    );
    assert_eq!(before.0.len(), 2);
    assert_eq!(before.1.version, 2);
    assert_eq!(before.4.secs, 11 * MIN as u64);
    hub.close().unwrap();

    let mut hub = Hub::open(&place.path).unwrap();
    let after = (
        hub.children().unwrap(),
        hub.rules(&asha).unwrap(),
        hub.devices(&asha).unwrap(),
        hub.family_today(now).unwrap(),
        hub.usage(&asha, today, today).unwrap(),
        hub.extra_time_today(&asha, now).unwrap(),
        hub.accounts().unwrap(),
        hub.events(100, None).unwrap(),
        hub.wanted_reports(now).unwrap(),
    );
    assert_eq!(after, before);
    assert!(hub.has_family().unwrap() && hub.has_pin().unwrap());
    assert_eq!(hub.child_today(&ravi, now).unwrap().guardian, limo_core::GuardianMode::Lock { until: Some(at(20, 0)) });
    assert_eq!(hub.check_password(PASSWORD, now).unwrap(), PasswordOutcome::Right);
}

#[test]
fn a_file_from_a_newer_version_is_refused_and_left_as_it_was() {
    let place = place();
    let mut hub = new_family(&place.path);
    fill(&mut hub);
    hub.close().unwrap();
    raw(&place.path).pragma_update(None, "user_version", SCHEMA_VERSION + 1).unwrap();
    let bytes = std::fs::read(&place.path).unwrap();

    let err = Hub::open(&place.path).unwrap_err();
    assert_eq!(err, HubError::FileTooNew { file_version: SCHEMA_VERSION + 1, known_version: SCHEMA_VERSION });
    let message = err.to_string();
    plain(&message);
    assert!(message.contains("newer version of LiMo") && message.contains("not changed"), "{message}");
    assert_eq!(std::fs::read(&place.path).unwrap(), bytes, "the file was changed");
}

#[test]
fn files_that_are_not_family_files_are_refused() {
    let place = place();
    std::fs::write(&place.path, "Dear diary, today I played for nine hours.").unwrap();
    assert_eq!(Hub::open(&place.path).unwrap_err(), HubError::NotAFamilyFile);
    assert!(std::fs::read_to_string(&place.path).unwrap().starts_with("Dear diary"));

    // Another program's database, with tables of its own.
    let other = place.dir.path().join("other.sqlite");
    raw(&other).execute_batch("CREATE TABLE notes (text TEXT); INSERT INTO notes VALUES ('keep me');").unwrap();
    let bytes = std::fs::read(&other).unwrap();
    assert_eq!(Hub::open(&other).unwrap_err(), HubError::NotAFamilyFile);
    assert_eq!(std::fs::read(&other).unwrap(), bytes);
    plain(&HubError::NotAFamilyFile.to_string());

    // An empty file is a new store.
    let empty = place.dir.path().join("empty.limo");
    std::fs::write(&empty, b"").unwrap();
    let hub = Hub::open(&empty).unwrap();
    assert!(!hub.has_family().unwrap());
    assert_eq!(hub.children().unwrap(), vec![]);
}

#[test]
fn a_second_hub_on_the_same_file_is_refused_until_the_first_closes() {
    let place = place();
    let first = new_family(&place.path);
    let err = Hub::open(&place.path).unwrap_err();
    assert_eq!(err, HubError::InUse);
    plain(&err.to_string());
    // Any other reader is kept out too.
    let reader = raw(&place.path);
    assert!(reader.query_row("SELECT count(*) FROM child", [], |r| r.get::<_, i64>(0)).is_err());
    drop(reader);
    // The first hub still works.
    assert!(first.has_family().unwrap());
    first.close().unwrap();

    let second = Hub::open(&place.path).unwrap();
    assert!(second.has_family().unwrap());
    // Dropping a hub closes it as well.
    drop(second);
    assert!(Hub::open(&place.path).is_ok());
}

#[test]
fn the_file_alone_is_the_whole_family_once_the_hub_is_closed() {
    let place = place();
    let mut hub = new_family(&place.path);
    let (asha, _) = fill(&mut hub);
    let today_view = hub.child_today(&asha, at(10, 0)).unwrap();
    hub.close().unwrap();

    // Beside the file there is at most an empty journal: SQLite empties it
    // after every transaction and leaves it in place.
    for entry in std::fs::read_dir(place.dir.path()).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().to_string();
        if name != "family.limo" {
            assert_eq!(name, "family.limo-journal");
            assert_eq!(entry.metadata().unwrap().len(), 0, "the journal is not empty");
        }
    }
    let copy_dir = tempfile::tempdir().unwrap();
    let copy = copy_dir.path().join("copy.limo");
    std::fs::copy(&place.path, &copy).unwrap();
    let hub = Hub::open(&copy).unwrap();
    assert_eq!(hub.child_today(&asha, at(10, 0)).unwrap(), today_view);
}

#[test]
fn a_damaged_file_gives_plain_errors_and_never_a_panic() {
    let place = place();
    let mut hub = new_family(&place.path);
    let (asha, _) = fill(&mut hub);
    hub.close().unwrap();

    // A row that cannot be read: the password's rounds as text.
    raw(&place.path).execute("UPDATE family SET password_rounds = 'many'", []).unwrap();
    let mut hub = Hub::open(&place.path).unwrap();
    let err = hub.check_password(PASSWORD, at(10, 0)).unwrap_err();
    assert!(matches!(err, HubError::Damaged { .. }), "{err:?}");
    plain(&err.to_string());
    // The rest of the family still works.
    assert!(hub.child_today(&asha, at(10, 0)).is_ok());
    hub.close().unwrap();

    // Garbage over the middle of the file: whatever each call meets, it
    // answers or says what is wrong.
    let mut bytes = std::fs::read(&place.path).unwrap();
    let middle = bytes.len() / 2;
    for b in &mut bytes[middle..middle + 4_096] {
        *b = 0xA5;
    }
    std::fs::write(&place.path, &bytes).unwrap();
    match Hub::open(&place.path) {
        Err(err) => plain(&err.to_string()),
        Ok(mut hub) => {
            let now = at(10, 0);
            let mut errors = Vec::new();
            errors.extend(hub.children().err());
            errors.extend(hub.family_today(now).err());
            errors.extend(hub.usage(&asha, day_of(now), day_of(now)).err());
            errors.extend(hub.events(100, None).err());
            errors.extend(hub.wanted_reports(now).err());
            errors.extend(hub.tidy(now).err());
            errors.extend(hub.add_child("Mira", limo_hub::AgeBand::UnderSix, IST, now).err());
            assert!(errors.iter().any(|e| matches!(e, HubError::Damaged { .. })), "{errors:?}");
            for err in errors {
                plain(&err.to_string());
            }
        }
    }
}

#[test]
fn a_missing_folder_is_a_plain_error() {
    let place = place();
    let path = place.dir.path().join("no such folder").join("family.limo");
    let err = Hub::open(&path).unwrap_err();
    assert!(matches!(err, HubError::CannotOpen { .. }), "{err:?}");
    plain(&err.to_string());
}
