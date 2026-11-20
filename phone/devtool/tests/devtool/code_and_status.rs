//! `code` and `status`: a code made on this PC works once on the phone's
//! own engine; `status` reads what the real engine kept, through a day of
//! use, a lock, wrong tries and a request.

use limo_core::{PinAction, TryOutcome};
use limo_phone::{Front, PhonePermissions};
use limo_phone_dev::{DONE, FAILED, REFUSED};

use crate::fake::{Bench, NOW, PIN, STATE};

/// The code `code` printed: the line of 8 digits in two groups.
fn printed_code(bench: &Bench) -> String {
    bench
        .ui
        .lines
        .iter()
        .rev()
        .map(|l| l.trim())
        .find(|l| l.len() == 9 && l.as_bytes()[4] == b' ' && l.replace(' ', "").chars().all(|c| c.is_ascii_digit()))
        .expect("a code was printed")
        .to_string()
}

#[test]
fn a_code_needs_no_phone_works_once_on_it_and_is_never_logged() {
    let mut bench = Bench::new();
    assert_eq!(bench.run("code"), REFUSED);
    assert!(bench.printed().contains("There is no test family on this PC. Run setup first."));

    assert_eq!(bench.setup("setup --child Asha --minutes 20"), DONE);
    bench.clear();
    assert_eq!(bench.run("code --minutes 30"), DONE);
    assert!(bench.phone.calls.is_empty(), "code asks nothing of the phone");
    let printed = bench.printed();
    assert!(printed.contains("code is scaffolding until the guardian's phone app exists."));
    assert!(printed.contains("A one time code for Asha's test phone, worth 30 minutes, for today only:"));
    assert!(printed.contains("On the phone's lock, choose \"Type a one time code\" and type its 8 digits."));
    let code = printed_code(&bench);
    assert_eq!(bench.ui.lines.iter().filter(|l| l.contains(&code)).count(), 1, "printed once");
    assert!(bench.log().contains("code made a one time code of 30 minutes."));
    assert!(!bench.log().contains(&code) && !bench.log().contains(&code.replace(' ', "")));

    // On the phone's own engine: it adds 30 minutes, once.
    bench.phone.with_engine(|engine, _| {
        let first = engine.try_code(&code, NOW).unwrap();
        assert!(matches!(first.outcome, TryOutcome::Added { .. }), "{first:?}");
        assert_eq!(first.message, "30 minutes added.");
        let again = engine.try_code(&code, NOW).unwrap();
        assert!(matches!(again.outcome, TryOutcome::AlreadyUsed), "{again:?}");
    });

    // The next is another code.
    bench.clear();
    assert_eq!(bench.run("code"), DONE);
    assert_ne!(printed_code(&bench), code);
}

#[test]
fn a_phone_takes_ten_codes_a_day() {
    let mut bench = Bench::new();
    assert_eq!(bench.setup("setup --child Asha --minutes 20"), DONE);
    for _ in 0..10 {
        assert_eq!(bench.run("code"), DONE);
    }
    bench.clear();
    assert_eq!(bench.run("code"), FAILED);
    assert!(bench.printed().contains("The code could not be made:"), "{}", bench.printed());
}

#[test]
fn status_reads_what_the_real_engine_kept_through_a_day() {
    let mut bench = Bench::new();
    assert_eq!(bench.setup("setup --child Asha --minutes 20 --block com.example.game"), DONE);

    // Kept at pairing, before any use: setup is not finished on the phone,
    // and status says so before anything else of the day.
    bench.clear();
    assert_eq!(bench.run("status"), DONE, "{}", bench.printed());
    let printed = bench.printed();
    for line in [
        "status reads only; it changes nothing on the phone.",
        "Child: Asha (this PC's test family), on its test phone.",
        "Setup is not finished on this phone. LiMo counts nothing and covers nothing until a guardian finishes setup on the phone.",
        "Time left today: 20 min (held on the phone: there is no master to reach).",
        "Used today: 0 min used of 20 min. On this phone 0 min.",
        "Apps today: none used for a minute or more.",
        "Locked: no.",
        "Wrong tries of a code or the PIN: none.",
    ] {
        assert!(bench.ui.lines.iter().any(|l| l == line), "missing \"{line}\" in:\n{printed}");
    }
    let setup_at = bench.ui.lines.iter().position(|l| l.starts_with("Setup is not finished")).unwrap();
    let time_at = bench.ui.lines.iter().position(|l| l.starts_with("Time left today")).unwrap();
    assert!(setup_at < time_at, "{printed}");
    assert!(bench.phone.changed_nothing());

    // Two minutes on the phone during setup count nothing.
    bench.phone.use_app("org.example.settings", 2 * 60);
    bench.clear();
    assert_eq!(bench.run("status"), DONE);
    assert!(bench.ui.lines.iter().any(|l| l == "Used today: 0 min used of 20 min. On this phone 0 min."), "{}", bench.printed());

    // The guardian finishes setup on the phone; then six minutes of reading
    // and two of drawing.
    bench.phone.finish_setup();
    bench.clear();
    assert_eq!(bench.run("status"), DONE);
    assert!(bench.ui.lines.iter().any(|l| l.starts_with("Setup was finished on the phone today at ")), "{}", bench.printed());
    assert!(!bench.printed().contains("Setup is not finished"), "{}", bench.printed());
    bench.phone.use_app("org.example.reader", 6 * 60);
    bench.phone.use_app("org.example.drawing", 2 * 60);
    bench.clear();
    assert_eq!(bench.run("status"), DONE);
    let printed = bench.printed();
    for line in [
        "Time left today: 12 min (held on the phone: there is no master to reach).",
        "Used today: 8 min used of 20 min. On this phone 8 min.",
        "Apps today:",
        "  org.example.reader: 6 min",
        "  org.example.drawing: 2 min",
        "Locked: no.",
    ] {
        assert!(bench.ui.lines.iter().any(|l| l == line), "missing \"{line}\" in:\n{printed}");
    }
    assert!(printed.contains("Use not yet sent to a master: "), "{printed}");

    // The rest of the day used: locked, and why.
    bench.phone.use_app("org.example.reader", 13 * 60);
    bench.clear();
    assert_eq!(bench.run("status"), DONE);
    let printed = bench.printed();
    assert!(printed.contains("Used today: 20 min used of 20 min."), "{printed}");
    assert!(bench.ui.lines.iter().any(|l| l == "Time left today: 0:00."), "{printed}");
    let locked = bench.ui.lines.iter().find(|l| l.starts_with("Locked: yes")).expect("locked");
    assert!(locked.contains("until tomorrow at 00:00"), "{locked}");
    assert!(locked.contains("The lock says: "), "{locked}");
    assert!(bench.phone.changed_nothing());
}

#[test]
fn status_says_wrong_tries_a_request_that_waits_and_never_a_pin_a_code_or_a_reason() {
    let mut bench = Bench::new();
    assert_eq!(bench.setup("setup --child Asha --minutes 20"), DONE);
    bench.phone.finish_setup();
    bench.phone.use_app("org.example.reader", 20 * 60 + 5);
    bench.run("code");
    let code = printed_code(&bench);
    bench.phone.with_engine(|engine, phone| {
        let now = phone.wall_secs;
        engine.tick(&phone.facts(true, Front::app("org.example.reader")));
        assert!(engine.try_pin("000000", PinAction::AddMinutes { minutes: 15 }, now).is_ok());
        assert!(engine.try_pin("111111", PinAction::AddMinutes { minutes: 15 }, now).is_ok());
        engine.ask_for_time(30, "For the school project", now).unwrap();
    });
    bench.clear();
    assert_eq!(bench.run("status"), DONE);
    let printed = bench.printed();
    assert!(printed.contains("Wrong tries of a code or the PIN since the last right PIN: 2."), "{printed}");
    assert!(printed.contains("A request waits for an answer: 30 minutes, asked today at "), "{printed}");
    for secret in [PIN, code.as_str(), &code.replace(' ', ""), "school project", "000000", "111111"] {
        assert!(!printed.contains(secret), "status printed {secret}:\n{printed}");
    }
    let state = bench.phone.files.get(STATE).unwrap().clone();
    assert!(!state.is_empty());
}

#[test]
fn status_of_a_phone_with_no_state_another_familys_or_a_damaged_one() {
    let mut bench = Bench::new();
    assert_eq!(bench.run("status"), DONE);
    assert!(bench.printed().contains("LiMo on this phone keeps no state yet, so no family is on it. Run setup."));

    let mut ours = Bench::new();
    assert_eq!(ours.setup("setup --child Asha --minutes 20"), DONE);
    let mut other = Bench::new();
    other.phone.files = ours.phone.files.clone();
    assert_eq!(other.run("status"), DONE);
    assert!(other.printed().contains("This phone holds a family this PC did not make: child child-"), "{}", other.printed());
    assert!(other.printed().contains("Time left today: 20 min"));

    let mut damaged = Bench::new();
    damaged.phone.files.insert(STATE.to_string(), b"not LiMo's at all".to_vec());
    assert_eq!(damaged.run("status"), DONE);
    assert!(damaged.printed().contains("LiMo's state on the phone cannot be read. The phone's kept state is not LiMo's."), "{}", damaged.printed());
    assert!(damaged.phone.changed_nothing());
}

#[test]
fn status_names_what_is_switched_off_in_words_as_the_phone_kept_it() {
    /// The phone's engine sees these permissions at one call, and keeps it.
    fn phone_sees(bench: &mut Bench, permissions: PhonePermissions) {
        bench.phone.with_engine(|engine, phone| {
            let mut facts = phone.facts(true, Front::Home);
            facts.permissions = permissions;
            engine.tick(&facts);
        });
    }
    let switched_off = |bench: &Bench| {
        bench.ui.lines.iter().filter(|l| l.to_lowercase().contains("switched off")).cloned().collect::<Vec<String>>()
    };
    let mut bench = Bench::new();
    assert_eq!(bench.setup("setup --child Asha --minutes 20"), DONE);

    // Before LiMo has looked at all, and with everything on.
    for permissions in [None, Some(PhonePermissions::ALL)] {
        if let Some(permissions) = permissions {
            phone_sees(&mut bench, permissions);
        }
        bench.clear();
        assert_eq!(bench.run("status"), DONE);
        assert_eq!(switched_off(&bench), ["Nothing LiMo needs was switched off when it last looked."]);
    }

    // All six off.
    let none = PhonePermissions {
        notifications: false,
        phone: false,
        usage_access: false,
        accessibility: false,
        battery: false,
        protection: false,
    };
    phone_sees(&mut bench, none);
    bench.clear();
    assert_eq!(bench.run("status"), DONE);
    assert_eq!(
        switched_off(&bench),
        ["Switched off on the phone when LiMo last looked: notifications, phone, usage access, accessibility, battery, protection."]
    );

    // Each one alone, in its own words.
    for (permissions, words) in [
        (PhonePermissions { notifications: false, ..PhonePermissions::ALL }, "notifications"),
        (PhonePermissions { phone: false, ..PhonePermissions::ALL }, "phone"),
        (PhonePermissions { usage_access: false, ..PhonePermissions::ALL }, "usage access"),
        (PhonePermissions { accessibility: false, ..PhonePermissions::ALL }, "accessibility"),
        (PhonePermissions { battery: false, ..PhonePermissions::ALL }, "battery"),
        (PhonePermissions { protection: false, ..PhonePermissions::ALL }, "protection"),
    ] {
        phone_sees(&mut bench, permissions);
        bench.clear();
        assert_eq!(bench.run("status"), DONE);
        assert_eq!(switched_off(&bench), [format!("Switched off on the phone when LiMo last looked: {words}.")]);
    }
    // No name of the engine's own, nor anything run together, in any line.
    assert!(!bench.printed().contains("usageaccess") && !bench.printed().contains("usage_access"));
}

#[test]
fn status_goes_on_with_this_pcs_clock_when_the_phones_cannot_be_read() {
    let mut bench = Bench::new();
    assert_eq!(bench.setup("setup --child Asha --minutes 20"), DONE);
    bench.phone.zone = "local".to_string();
    bench.clear();
    assert_eq!(bench.run("status"), DONE);
    assert!(bench.printed().contains("The phone's clock could not be read, so this PC's clock stands in for it."));
    assert!(bench.printed().contains("Time left today: 20 min"));
}
