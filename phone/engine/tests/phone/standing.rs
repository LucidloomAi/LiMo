//! How the phone stands, told to the master: each of the six steps as it is
//! done, during setup and after; the protection level and USB debugging from
//! the first call, and every change after; nothing told twice through a
//! crash or a restart; a phone set up before any of it was told telling all
//! of it once; and a permission that flaps never taking the room of the
//! other notes.

use std::collections::BTreeSet;

use limo_core::PhoneNote as Note;
use limo_phone::{Permission, PhoneEngine, PhoneNote, PhonePermissions};

use crate::common::*;

/// The permissions with these on and the others off.
fn with(on: &[Permission]) -> PhonePermissions {
    let has = |p: Permission| on.contains(&p);
    PhonePermissions {
        notifications: has(Permission::Notifications),
        phone: has(Permission::Phone),
        usage_access: has(Permission::UsageAccess),
        accessibility: has(Permission::Accessibility),
        battery: has(Permission::Battery),
        protection: has(Permission::Protection),
    }
}

/// What the reports from the `from`th on told of how the phone stands, each
/// note once (a note rides along until it is confirmed), in words.
fn told(f: &Family, from: usize) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for note in f.sent[from..].iter().flat_map(|r| r.notes.iter()) {
        if !seen.insert(note.id.clone()) {
            continue;
        }
        let said = match &note.note {
            Note::PermissionOn { permission, .. } => format!("{permission:?} on"),
            Note::PermissionOff { permission, .. } => format!("{permission:?} off"),
            Note::Protection { level, .. } => format!("protection {level:?}"),
            Note::UsbDebugging { on, .. } => format!("usb debugging {}", if *on { "on" } else { "off" }),
            _ => continue,
        };
        out.push(said);
    }
    out
}

/// The notes of how the phone stands given since `from`, in the orders.
fn noted(f: &Family, from: usize) -> usize {
    f.notes[from..]
        .iter()
        .filter(|n| {
            matches!(
                n,
                PhoneNote::PermissionOn { .. }
                    | PhoneNote::PermissionOff { .. }
                    | PhoneNote::Protection { .. }
                    | PhoneNote::UsbDebugging { .. }
            )
        })
        .count()
}

#[test]
fn a_phone_just_paired_tells_each_step_as_the_guardian_does_it() {
    // Paired over the cable: nothing switched on yet, USB debugging on.
    let mut f = Family::setting_up(at(16, 0));
    f.permissions = with(&[]);
    f.usb_debugging = Some(true);
    f.in_reach = true;
    f.run(5);
    // The first report says the protection level and USB debugging, and no
    // step: none is done.
    assert_eq!(told(&f, 0), ["protection Off", "usb debugging on"]);

    // The guardian does the six, one after another, on the phone: each is
    // told at the next call, and only that one.
    let mut done = Vec::new();
    for step in Permission::ALL {
        done.push(step);
        let from = f.sent.len();
        f.permissions = with(&done);
        f.run(2);
        assert_eq!(told(&f, from), [format!("{step:?} on")]);
        // The child's own Today says what is off from what the call saw,
        // whatever the notes said.
        let off = f.view().today.guardian_needed.map(|g| g.off.len()).unwrap_or(0);
        assert_eq!(off, 6 - done.len());
    }

    // The PC switches USB debugging off at the end of Add a phone.
    let from = f.sent.len();
    f.usb_debugging = Some(false);
    f.run(2);
    assert_eq!(told(&f, from), ["usb debugging off"]);

    // Setup finished: the guard covers from now on, and that is base
    // protection.
    let from = f.sent.len();
    let wall = f.wall();
    assert_eq!(f.way_out(|e, _| e.finish_setup(wall)), Ok(true));
    f.run(2);
    assert_eq!(told(&f, from), ["protection Base"]);
    assert!(f.view().today.guardian_needed.is_none());
    assert!(f.view().notes.is_empty(), "all confirmed");
}

#[test]
fn setup_finished_with_a_step_passed_over_tells_five_on_and_nothing_of_the_sixth_until_it_is_on() {
    let mut f = Family::setting_up(at(16, 0));
    let five = [
        Permission::Notifications,
        Permission::Phone,
        Permission::UsageAccess,
        Permission::Accessibility,
        Permission::Protection,
    ];
    f.permissions = with(&five);
    f.in_reach = true;
    f.run(5);
    let wall = f.wall();
    assert_eq!(f.way_out(|e, _| e.finish_setup(wall)), Ok(true));
    f.run(MIN);
    assert_eq!(told(&f, 0), [
        "Notifications on",
        "Phone on",
        "UsageAccess on",
        "Accessibility on",
        "Protection on",
        "protection Off",
        "protection Base",
    ]);
    // Nothing is said of the battery: it was never on. The child's Today
    // says a guardian is needed for it.
    let needed = f.view().today.guardian_needed.unwrap();
    assert_eq!(needed.off.iter().map(|o| o.permission).collect::<Vec<_>>(), [Permission::Battery]);
    // Days later a guardian switches it on: told then.
    f.run(10 * MIN);
    let from = f.sent.len();
    f.permissions = PhonePermissions::ALL;
    f.run(2);
    assert_eq!(told(&f, from), ["Battery on"]);
}

#[test]
fn a_step_switched_off_after_setup_and_on_again_is_told_both_times() {
    let mut f = Family::new(at(16, 0));
    f.in_reach = true;
    f.screen_on = false;
    f.run(20);
    assert_eq!(told(&f, 0).len(), 7, "six steps on and base protection, at the first call");
    // Accessibility switched off: the guard goes with it.
    let from = f.sent.len();
    f.permissions = PhonePermissions { accessibility: false, ..PhonePermissions::ALL };
    f.run(2);
    assert_eq!(told(&f, from), ["Accessibility off", "protection Off"]);
    // And on again.
    let from = f.sent.len();
    f.permissions = PhonePermissions::ALL;
    f.run(2);
    assert_eq!(told(&f, from), ["Accessibility on", "protection Base"]);
    // The protection switched off on its own: the level goes, the steps say which.
    let from = f.sent.len();
    f.permissions = PhonePermissions { protection: false, ..PhonePermissions::ALL };
    f.run(2);
    assert_eq!(told(&f, from), ["Protection off", "protection Off"]);
}

#[test]
fn nothing_is_told_again_through_a_crash_or_a_restart() {
    let mut f = Family::new(at(16, 0));
    f.usb_debugging = Some(false);
    f.in_reach = true;
    f.screen_on = false;
    f.run(20);
    assert_eq!(told(&f, 0).len(), 8);
    let from = f.notes.len();
    let sent = f.sent.len();
    f.crash();
    f.run(MIN);
    f.reboot(5 * MIN);
    f.run(MIN);
    assert_eq!(noted(&f, from), 0);
    assert!(told(&f, sent).is_empty());
    // Away from home it is the same, and what changed meanwhile is told
    // once home.
    f.in_reach = false;
    f.usb_debugging = Some(true);
    f.run(MIN);
    f.crash();
    f.run(MIN);
    let sent = f.sent.len();
    f.in_reach = true;
    f.run(5);
    assert_eq!(told(&f, sent), ["usb debugging on"]);
}

#[test]
fn a_phone_set_up_before_any_of_this_was_told_tells_all_its_steps_at_the_first_call() {
    // A phone set up and in use, kept by a LiMo that did not tell how it
    // stands: its state has no word of what the master was told.
    let mut f = Family::new(at(16, 0));
    f.permissions = PhonePermissions { battery: false, ..PhonePermissions::ALL };
    f.run(30);
    let text = String::from_utf8(f.kept.clone()).unwrap();
    let body = text.splitn(4, '\n').nth(3).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(body).unwrap();
    assert!(value.as_object().unwrap().contains_key("told"), "this LiMo keeps it");
    value.as_object_mut().unwrap().remove("told");
    let body = serde_json::to_string(&value).unwrap();
    let blob = format!("limo-phone-state\nformat 1\ncheck {:016x}\n{body}", fnv(body.as_bytes()));
    let mut engine = PhoneEngine::load(blob.as_bytes());
    assert_eq!(engine.load_problem(), None);
    let orders = engine.tick(&f.facts());
    let told: Vec<String> = orders
        .notes
        .iter()
        .filter_map(|n| match n.for_master()? {
            Note::PermissionOn { permission, .. } => Some(format!("{permission:?} on")),
            Note::PermissionOff { permission, .. } => Some(format!("{permission:?} off")),
            Note::Protection { level, .. } => Some(format!("protection {level:?}")),
            _ => None,
        })
        .collect();
    // Every step that is on, none that is off, and the protection level.
    assert_eq!(told, [
        "Notifications on",
        "Phone on",
        "UsageAccess on",
        "Accessibility on",
        "Protection on",
        "protection Base",
    ]);
    assert!(orders.save_state, "kept at once, so a restart tells nothing again");
    let mut again = PhoneEngine::load(&engine.save());
    assert!(again.tick(&f.facts()).notes.is_empty());
}

#[test]
fn a_permission_that_flaps_costs_two_notes_and_the_newest_wins() {
    let mut f = Family::new(at(16, 0));
    f.set_rules(|_| {});
    // Away from home, notifications switched off and on five hundred times:
    // two notes wait about them, the last time they were off and the newest
    // (on), and nothing else is pushed out.
    f.front = limo_phone::Front::app(CASINO);
    f.set_rules(|r| r.blocked_apps = vec![CASINO.into()]);
    f.run(2);
    f.front = limo_phone::Front::app(GAME);
    for i in 0..500 {
        f.permissions = PhonePermissions { notifications: i % 2 == 1, ..PhonePermissions::ALL };
        f.tick();
    }
    let view = f.view();
    let about: Vec<bool> = view
        .notes
        .iter()
        .filter_map(|n| match n {
            PhoneNote::PermissionOn { permission: Permission::Notifications, .. } => Some(true),
            PhoneNote::PermissionOff { permission: Permission::Notifications, .. } => Some(false),
            _ => None,
        })
        .collect();
    assert_eq!(about, [false, true], "off a moment ago, on now");
    assert!(view.notes.iter().any(|n| matches!(n, PhoneNote::BlockedAppOpened { .. })), "the others stay");
    assert!(view.notes.len() <= 4, "{:?}", view.notes);
    // Home: the master hears it was switched off, and that it is on now.
    let sent = f.sent.len();
    f.in_reach = true;
    f.run(5);
    assert_eq!(told(&f, sent), ["Notifications off", "Notifications on"]);
}

/// FNV-1a, 64 bits, as the kept state's check.
fn fnv(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
