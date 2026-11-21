//! The guardian password, the PIN, and secrets kept out of sight.

use limo_core::{DeviceKind, PinAction, TryOutcome};
use limo_hub::{EventKind, Hub, HubError, PasswordOutcome};

use crate::common::*;

use PasswordOutcome::{MustWait, Right, Wrong};

#[test]
fn a_family_is_set_up_once_with_a_long_enough_password() {
    let place = place();
    let mut hub = Hub::open(&place.path).unwrap();
    assert_eq!(hub.add_child("Asha", limo_hub::AgeBand::UnderSix, IST, at(7, 0)).unwrap_err(), HubError::NoFamily);
    assert_eq!(hub.check_password(PASSWORD, at(7, 0)).unwrap_err(), HubError::NoFamily);
    let short = hub.create_family("seven c", at(7, 0)).unwrap_err();
    assert_eq!(short, HubError::PasswordTooShort { min_chars: 8 });
    let long = hub.create_family(&"x".repeat(257), at(7, 0)).unwrap_err();
    assert_eq!(long, HubError::PasswordTooLong { max_chars: 256 });
    assert!(!hub.has_family().unwrap());
    // Eight characters of any kind will do, spaces included.
    hub.create_family("पासवर्ड यह है", at(7, 0)).unwrap();
    assert_eq!(hub.create_family(PASSWORD, at(7, 1)).unwrap_err(), HubError::FamilyExists);
    assert_eq!(hub.check_password("पासवर्ड यह है", at(7, 2)).unwrap(), Right);
    for err in [short, long, HubError::NoFamily, HubError::FamilyExists] {
        plain(&err.to_string());
    }
}

#[test]
fn wrong_passwords_slow_down_and_the_wait_survives_a_restart() {
    let place = place();
    let mut hub = new_family(&place.path);
    let t = at(9, 0);
    for n in 1..=4 {
        assert_eq!(hub.check_password("wrong guess", t).unwrap(), Wrong { wrong_guesses: n, wait_until: None });
    }
    assert_eq!(hub.check_password("wrong guess", t).unwrap(), Wrong { wrong_guesses: 5, wait_until: Some(t + MIN) });
    // During the wait nothing is checked, not even the right password, and
    // nothing is counted.
    assert_eq!(hub.check_password(PASSWORD, t + 30).unwrap(), MustWait { until: t + MIN });
    hub.close().unwrap();

    let mut hub = Hub::open(&place.path).unwrap();
    assert_eq!(hub.check_password(PASSWORD, t + 59).unwrap(), MustWait { until: t + MIN });
    // A clock set back makes the wait longer, never shorter.
    assert_eq!(hub.check_password(PASSWORD, t - HOUR).unwrap(), MustWait { until: t + MIN });
    // The next wrong guess doubles the wait.
    assert_eq!(hub.check_password("wrong guess", t + MIN).unwrap(), Wrong {
        wrong_guesses: 6,
        wait_until: Some(t + 3 * MIN)
    });
    assert_eq!(hub.check_password(PASSWORD, t + 3 * MIN).unwrap(), Right);
    // A right password clears the count.
    assert_eq!(hub.check_password("wrong guess", t + 4 * MIN).unwrap(), Wrong { wrong_guesses: 1, wait_until: None });
    // A password longer than any that can be set is simply wrong.
    assert_eq!(hub.check_password(&"y".repeat(100_000), t + 4 * MIN).unwrap(), Wrong {
        wrong_guesses: 2,
        wait_until: None
    });
}

#[test]
fn changing_the_password_needs_the_old_one() {
    let place = place();
    let mut hub = new_family(&place.path);
    let t = at(9, 0);
    // A new password that cannot be taken is refused before the old one is
    // looked at: nothing is counted.
    assert_eq!(hub.change_password("nope nope", "short", t).unwrap_err(), HubError::PasswordTooShort { min_chars: 8 });
    assert_eq!(hub.check_password("nope nope", t).unwrap(), Wrong { wrong_guesses: 1, wait_until: None });
    assert_eq!(hub.change_password("nope nope", "a new long password", t).unwrap(), Wrong {
        wrong_guesses: 2,
        wait_until: None
    });
    assert_eq!(hub.check_password(PASSWORD, t).unwrap(), Right);
    assert_eq!(hub.change_password(PASSWORD, "a new long password", t).unwrap(), Right);
    assert_eq!(hub.check_password(PASSWORD, t).unwrap(), Wrong { wrong_guesses: 1, wait_until: None });
    assert_eq!(hub.check_password("a new long password", t).unwrap(), Right);
    let events = hub.events(10, None).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!((events[0].child_id.clone(), events[0].what.clone()), (None, EventKind::PasswordChanged));
}

#[test]
fn a_pin_of_four_or_five_digits_is_refused_and_changes_nothing() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let mut pc = pair(&mut hub, &asha, "PC", DeviceKind::OnMaster, at(7, 0));
    let short = ["1234", "12345"];

    // With no PIN yet: refused in plain words, and nothing changes.
    let before = (hub.family_today(at(8, 0)).unwrap(), hub.events(100, None).unwrap());
    for pin in short {
        let err = hub.set_pin(pin, at(8, 0)).unwrap_err();
        assert_eq!(err, HubError::PinWrongShape { min_digits: 6, max_digits: 8 });
        assert_eq!(err.to_string(), "A PIN is 6 to 8 digits, with nothing else in it.");
    }
    assert!(!hub.has_pin().unwrap());
    assert_eq!((hub.family_today(at(8, 0)).unwrap(), hub.events(100, None).unwrap()), before);
    assert_eq!(exchange(&mut hub, &mut pc, at(8, 1), false).pin, None);
    assert!(!pc.state.has_pin());

    // With a six digit PIN set and taken by the PC, a short one does not
    // replace it.
    hub.set_pin("482915", at(9, 0)).unwrap();
    exchange(&mut hub, &mut pc, at(9, 1), false);
    exchange(&mut hub, &mut pc, at(9, 2), false);
    assert_eq!(pc.state.pin_version(), 1);
    let before = (hub.family_today(at(9, 3)).unwrap(), hub.events(100, None).unwrap());
    for pin in short {
        assert_eq!(hub.set_pin(pin, at(9, 3)).unwrap_err(), HubError::PinWrongShape { min_digits: 6, max_digits: 8 });
    }
    assert_eq!((hub.family_today(at(9, 3)).unwrap(), hub.events(100, None).unwrap()), before);
    assert_eq!(exchange(&mut hub, &mut pc, at(9, 4), false).pin, None, "no new PIN to send");
    assert_eq!(pc.state.pin_version(), 1);
    let outcome = pc.state.try_pin("482915", PinAction::ConfirmRelease, at(9, 5)).unwrap();
    assert_eq!(outcome, TryOutcome::ReleaseConfirmed, "the six digit PIN still works");
}

#[test]
fn secrets_never_show_in_debug_output_errors_or_the_file() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let pairing = hub.pair_device(&asha, "Phone", DeviceKind::Portable, "android", at(7, 0)).unwrap();
    let text = format!("{pairing:?}");
    let secret_list = format!("{:?}", pairing.code_secret);
    assert!(!text.contains(&secret_list[1..20]), "{text}");
    assert!(text.contains("hidden (32 bytes)"), "{text}");
    let code = hub.make_code(&pairing.device_id, 30, at(8, 0)).unwrap();
    assert!(!format!("{code:?}").contains(&code.digits));
    let hub_text = format!("{hub:?}");
    assert!(hub_text.starts_with("Hub") && !hub_text.contains(PASSWORD), "{hub_text}");

    hub.set_pin("13572468", at(8, 0)).unwrap();
    let mut seen = Vec::new();
    seen.push(format!("{:?}", hub.check_password("not the password", at(8, 1)).unwrap()));
    seen.push(format!("{:?}", hub.change_password(PASSWORD, "tiny", at(8, 1)).unwrap_err()));
    seen.push(hub.change_password(PASSWORD, "tiny", at(8, 1)).unwrap_err().to_string());
    seen.push(format!("{:?}", hub.set_pin("97531", at(8, 1)).unwrap_err()));
    seen.push(hub.set_pin("97531", at(8, 1)).unwrap_err().to_string());
    seen.push(format!("{:?}", hub.events(10, None).unwrap()));
    for text in &seen {
        for secret in [PASSWORD, "not the password", "tiny", "13572468", "97531"] {
            assert!(!text.contains(secret), "{secret} shows in {text}");
        }
    }
    hub.close().unwrap();
    // The password is in the file only as its salted hash.
    assert!(!file_holds(&place.path, PASSWORD));
}

#[test]
fn a_forgotten_password_is_reset_and_everything_else_is_kept() {
    let place = place();
    let mut hub = Hub::open(&place.path).unwrap();
    assert_eq!(hub.reset_password("a brand new password", at(7, 0)).unwrap_err(), HubError::NoFamily);
    hub.create_family(PASSWORD, at(7, 0)).unwrap();
    let asha = child(&mut hub, "Asha").child_id;
    let mut phone = pair_phone(&mut hub, &asha, "Phone", 1, at(7, 0));
    hub.set_pin("482915", at(7, 1)).unwrap();
    let mut rules = hub.rules(&asha).unwrap();
    rules.away_share_percent = 30;
    hub.replace_rules(&asha, rules, at(7, 2)).unwrap();
    exchange(&mut hub, &mut phone, at(7, 3), false);
    // Someone has been guessing: a wait is in force.
    for _ in 0..5 {
        hub.check_password("wrong guess", at(8, 0)).unwrap();
    }
    assert!(matches!(hub.check_password(PASSWORD, at(8, 0)).unwrap(), MustWait { .. }));
    let kept = |hub: &Hub| {
        (hub.rules(&asha).unwrap(), hub.family_today(at(8, 1)).unwrap(), hub.master_key().unwrap(), hub.link_phones().unwrap())
    };
    let before = kept(&hub);

    // A new password that cannot be taken is refused, and nothing changes.
    let err = hub.reset_password("short", at(8, 1)).unwrap_err();
    assert_eq!(err, HubError::PasswordTooShort { min_chars: 8 });
    hub.reset_password("a brand new password", at(8, 1)).unwrap();
    // The old password no longer opens, the new one does, and the wait is
    // gone with the old count.
    assert_eq!(hub.check_password(PASSWORD, at(8, 2)).unwrap(), Wrong { wrong_guesses: 1, wait_until: None });
    assert_eq!(hub.check_password("a brand new password", at(8, 2)).unwrap(), Right);
    // Everything else is as it was: the PIN, the rules, the devices.
    assert_eq!(kept(&hub), before);
    assert_eq!(hub.rules(&asha).unwrap().away_share_percent, 30);
    assert_eq!(hub.check_pin("482915", at(8, 3)).unwrap(), Right);
    exchange(&mut hub, &mut phone, at(8, 4), false);
    assert_eq!(phone.state.try_pin("482915", PinAction::ConfirmRelease, at(8, 5)).unwrap(), TryOutcome::ReleaseConfirmed);
    // The family's history says it was a reset, not a change.
    let events = hub.events(10, None).unwrap();
    assert_eq!((events[0].at, events[0].child_id.clone(), events[0].what.clone()), (at(8, 1), None, EventKind::PasswordReset));
    assert!(!events.iter().any(|e| e.what == EventKind::PasswordChanged));
    hub.close().unwrap();
    let mut hub = Hub::open(&place.path).unwrap();
    assert_eq!(hub.check_password("a brand new password", at(8, 6)).unwrap(), Right);
    assert!(!file_holds(&place.path, "a brand new password"));
}

#[test]
fn the_pin_typed_on_the_pc_slows_down_and_the_wait_survives_a_restart() {
    let place = place();
    let mut hub = new_family(&place.path);
    let t = at(9, 0);
    let err = hub.check_pin("482915", t).unwrap_err();
    assert_eq!(err, HubError::NoPin);
    plain(&err.to_string());
    hub.set_pin("482915", at(8, 0)).unwrap();

    assert_eq!(hub.check_pin("482915", t).unwrap(), Right);
    for n in 1..=4 {
        assert_eq!(hub.check_pin("111111", t).unwrap(), Wrong { wrong_guesses: n, wait_until: None });
    }
    assert_eq!(hub.check_pin("111111", t).unwrap(), Wrong { wrong_guesses: 5, wait_until: Some(t + MIN) });
    // During the wait nothing is checked, not even the right PIN, and
    // nothing is counted.
    assert_eq!(hub.check_pin("482915", t + 30).unwrap(), MustWait { until: t + MIN });
    // Its count is its own: the password is not held up by it.
    assert_eq!(hub.check_password(PASSWORD, t + 30).unwrap(), Right);
    hub.close().unwrap();

    let mut hub = Hub::open(&place.path).unwrap();
    assert_eq!(hub.check_pin("482915", t + 59).unwrap(), MustWait { until: t + MIN });
    // A clock set back makes the wait longer, never shorter.
    assert_eq!(hub.check_pin("482915", t - HOUR).unwrap(), MustWait { until: t + MIN });
    // The next wrong guess doubles the wait.
    assert_eq!(hub.check_pin("111111", t + MIN).unwrap(), Wrong { wrong_guesses: 6, wait_until: Some(t + 3 * MIN) });
    assert_eq!(hub.check_pin("482915", t + 3 * MIN).unwrap(), Right);
    // A right PIN clears the count; anything that is not a PIN is wrong.
    assert_eq!(hub.check_pin("111111", t + 4 * MIN).unwrap(), Wrong { wrong_guesses: 1, wait_until: None });
    assert_eq!(hub.check_pin("not a pin", t + 4 * MIN).unwrap(), Wrong { wrong_guesses: 2, wait_until: None });

    // A new PIN clears the count, and only it opens; with no PIN there is
    // nothing to check.
    for _ in 0..5 {
        hub.check_pin("111111", t + 5 * MIN).unwrap();
    }
    hub.set_pin("13572468", t + 6 * MIN).unwrap();
    assert_eq!(hub.check_pin("482915", t + 6 * MIN).unwrap(), Wrong { wrong_guesses: 1, wait_until: None });
    assert_eq!(hub.check_pin("13572468", t + 6 * MIN).unwrap(), Right);
    hub.clear_pin(t + 7 * MIN).unwrap();
    assert_eq!(hub.check_pin("13572468", t + 7 * MIN).unwrap_err(), HubError::NoPin);
}
