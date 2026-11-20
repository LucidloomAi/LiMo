//! `setup`: the test family on this PC, the pairing file's exact bytes and
//! where they go, the broadcast, and reading back that LiMo took it.

use limo_core::{DailyBudget, Grant, TryOutcome, WeekBudgets};
use limo_phone_dev::family;
use limo_phone_dev::{DONE, FAILED, REFUSED};

use crate::fake::{Bench, NOW, PAIR, PAIRING, PIN, SERIAL, STATE, WRITE, ZONE};

const SETUP: &str = "setup --child Asha --minutes 20 --block com.example.game --allow com.example.clock --bedtime 21:00-07:00";

#[test]
fn setup_puts_the_test_family_on_the_phone_exactly_as_section_0_fixes_it() {
    let mut bench = Bench::new();
    assert_eq!(bench.setup(SETUP), DONE, "{}", bench.printed());
    let printed = bench.printed();
    for line in [
        "setup is scaffolding until pairing over the network exists.",
        "The phone's clock is in UTC+05:30: the test family's days and bedtime follow it.",
        "Made the test family on this PC: Asha, 20 minutes a day, the away share at 100 percent (a phone with no master has its whole day), and the PIN set.",
        "Blocked for Asha: com.example.game.",
        "Always allowed, also under a lock: com.example.clock.",
        "Screens off every day from 21:00 to 07:00.",
        "LiMo took it: its state on the phone now holds Asha's test phone.",
    ] {
        assert!(bench.ui.lines.iter().any(|l| l == line), "missing \"{line}\" in:\n{printed}");
    }
    assert_eq!(bench.ui.prompts, vec!["Guardian PIN (6 to 8 digits, not shown): ", "The same PIN again: "]);

    // Exactly one call carried anything on standard input: the pairing file,
    // written by run-as into LiMo's own files.
    let with_input: Vec<_> = bench.phone.calls.iter().filter(|c| c.stdin.is_some()).collect();
    assert_eq!(with_input.len(), 1);
    assert_eq!(with_input[0].args, vec!["-s", SERIAL, "shell", "-T", WRITE]);
    let bytes = with_input[0].stdin.clone().unwrap();

    // Its bytes: one JSON object, welcome, code_secret_hex, made_at, in that
    // order, and nothing after it.
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(text.starts_with("{\"welcome\":{"), "{text}");
    let secret_at = text.find(",\"code_secret_hex\":\"").unwrap();
    let hex = &text[secret_at + 20..secret_at + 20 + 64];
    assert!(hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()), "{hex}");
    assert_eq!(&text[secret_at + 20 + 64..], format!("\",\"made_at\":{NOW}}}"));
    let welcome: Grant = serde_json::from_str(&text["{\"welcome\":".len()..secret_at]).unwrap();

    // The welcome is the test family's: its child, its phone, its rules,
    // with the whole day held away from home, and the PIN's record.
    let family = family::read_places(&bench.places()).unwrap().unwrap();
    assert_eq!(family.child, "Asha");
    assert_eq!(Some(welcome.device_id.as_str()), family.device_id());
    assert_eq!(welcome.child_id, family.child_id);
    let rules = welcome.rules.clone();
    assert_eq!(rules.away_share_percent, 100);
    assert_eq!(rules.budgets, WeekBudgets::every_day(DailyBudget::Minutes(20)));
    assert_eq!(rules.blocked_apps, vec!["com.example.game".to_string()]);
    assert_eq!(rules.always_allowed_apps, vec!["com.example.clock".to_string()]);
    assert_eq!((rules.blocked_hours[0].start_minute, rules.blocked_hours[0].end_minute), (1260, 420));
    assert_eq!(rules.utc_offset_secs, ZONE);
    assert!(welcome.pin.is_some(), "the welcome carries the PIN's record");
    // The record is a slow hash, never the PIN.
    assert!(!text.contains(PIN));

    // Then the broadcast, by the receiver's name; then reading back.
    let shells = bench.phone.shell_commands();
    let write_at = shells.iter().position(|s| s == WRITE).unwrap();
    assert_eq!(shells[write_at + 1], PAIR);
    assert!(shells[write_at + 2..].iter().any(|s| s.contains("cat files/limo/state")), "read back: {shells:?}");

    // LiMo took it: the pairing file is gone and its kept state is the test
    // phone's, with the whole day of 20 minutes in hand away from home.
    assert!(!bench.phone.files.contains_key(PAIRING));
    let mut engine = bench.engine();
    let view = engine.view().unwrap();
    assert_eq!(Some(view.device_id.as_str()), family.device_id());
    assert_eq!(view.rules, rules);
    let orders = engine.tick(&bench.phone.facts(true, limo_phone::Front::app("org.example.reader")));
    assert_eq!(orders.time_left.seconds_left, Some(20 * 60));
    // The PIN typed at setup works on the phone.
    let answer = engine.try_pin(PIN, limo_core::PinAction::AddMinutes { minutes: 15 }, NOW).unwrap();
    assert!(matches!(answer.outcome, TryOutcome::Added { .. }), "{answer:?}");

    // The tool's log says what was done, with no name and no secret.
    let log = bench.log();
    assert!(log.contains("setup made a test family with one child, 20 minutes a day, 1 blocked and 1 always allowed apps, screens off at night, and paired one phone."), "{log}");
    assert!(!log.contains("Asha") && !log.contains(PIN) && !log.contains(hex));
}

#[test]
fn a_second_setup_says_a_family_is_there_and_what_forget_does() {
    let mut bench = Bench::new();
    assert_eq!(bench.setup("setup --child Asha --minutes 20"), DONE);
    let state = bench.phone.files.get(STATE).cloned();
    let file = std::fs::read(bench.places().family_file()).unwrap();
    bench.clear();
    bench.ui.prompts.clear();
    assert_eq!(bench.setup("setup --child Mira --minutes 60"), REFUSED);
    let printed = bench.printed();
    assert!(printed.contains("A test family is already on this PC: Asha, 20 minutes a day, with its test phone paired. setup makes one only once."), "{printed}");
    assert!(printed.contains("To start again, run forget: it asks LiMo on the phone to wipe the test family, checks that it did, and deletes the test family on this PC. Then setup again."));
    assert!(printed.contains("Nothing was changed."));
    // Nothing asked of the phone, no PIN asked for, nothing changed.
    assert!(bench.phone.calls.is_empty());
    assert!(bench.ui.prompts.is_empty());
    assert_eq!(bench.phone.files.get(STATE).cloned(), state);
    assert_eq!(std::fs::read(bench.places().family_file()).unwrap(), file);
}

#[test]
fn pins_that_differ_are_too_short_or_cannot_be_typed_change_nothing() {
    for (typed, not_a_terminal, says) in [
        (vec!["482915", "482916"], false, "The two PINs differ. Nothing was changed on the phone."),
        (vec!["4829", "4829"], false, "A PIN is 6 to 8 digits, with nothing else in it."),
        (vec!["48291a", "48291a"], false, "A PIN is 6 to 8 digits, with nothing else in it."),
        (vec!["482915"], false, "No PIN was typed the second time."),
        (vec![], false, "No PIN was typed."),
        (vec![], true, "so it must run in a terminal, not with its input from a file or a pipe."),
    ] {
        let mut bench = Bench::new();
        bench.ui.typed.extend(typed.iter().map(|t| t.to_string()));
        bench.ui.not_a_terminal = not_a_terminal;
        assert_eq!(bench.run("setup --child Asha --minutes 20"), REFUSED, "{typed:?}");
        assert!(bench.printed().contains(says), "{typed:?}: {}", bench.printed());
        assert!(bench.phone.changed_nothing(), "{typed:?}");
        assert!(!bench.places().family_file().exists(), "{typed:?}");
        for t in &typed {
            assert!(!bench.printed().contains(t), "a typed PIN was printed: {t}");
        }
    }
}

#[test]
fn a_phone_that_does_not_take_it_is_undone_on_both_sides_and_setup_runs_again() {
    let mut bench = Bench::new();
    bench.phone.receiver = false;
    assert_eq!(bench.setup("setup --child Asha --minutes 20"), FAILED);
    let printed = bench.printed();
    assert!(printed.contains("LiMo did not take the test family within 15 seconds"), "{printed}");
    assert!(printed.contains("Is this debug build one with the development receiver? Run log in another terminal"));
    assert!(printed.contains("The pairing file was removed from the phone, and the test family on this PC deleted again, so setup can run again."));
    // It waited, a second at a time, then gave up.
    assert_eq!(bench.phone.pauses, 15);
    assert!(bench.phone.files.is_empty(), "nothing left in LiMo's files");
    assert!(!bench.places().family_file().exists());

    // The receiver built: setup works.
    bench.phone.receiver = true;
    bench.clear();
    assert_eq!(bench.setup("setup --child Asha --minutes 20"), DONE, "{}", bench.printed());
}

#[test]
fn setup_refuses_a_phone_that_already_holds_a_family() {
    let mut first = Bench::new();
    assert_eq!(first.setup("setup --child Asha --minutes 20"), DONE);
    // Another PC's tool, the same phone.
    let mut bench = Bench::new();
    bench.phone.files = first.phone.files.clone();
    assert_eq!(bench.setup("setup --child Mira --minutes 30"), REFUSED);
    assert!(bench.printed().contains("LiMo on this phone already holds a family, device device-"), "{}", bench.printed());
    assert!(bench.printed().contains("To start afresh, run uninstall (it removes LiMo with everything it kept), then install."));
    assert!(bench.phone.changed_nothing());
    assert!(!bench.places().family_file().exists());
    assert!(bench.ui.prompts.is_empty(), "no PIN asked for");
}

#[test]
fn a_phone_whose_zone_cannot_be_read_changes_nothing() {
    let mut bench = Bench::new();
    bench.phone.zone = "%z".to_string();
    assert_eq!(bench.setup("setup --child Asha --minutes 20"), FAILED);
    assert!(bench.printed().contains("The phone's time zone could not be read"));
    assert!(bench.phone.changed_nothing());
    assert!(!bench.places().family_file().exists());
}

#[test]
fn clocks_far_apart_are_refused_and_a_little_apart_noted() {
    // LiMo's debug build refuses a pairing file made more than an hour
    // from its own clock.
    let mut bench = Bench::new();
    bench.phone.wall_secs = NOW + 2 * 3_600;
    assert_eq!(bench.setup("setup --child Asha --minutes 20"), REFUSED);
    assert!(bench.printed().contains("The phone's clock and this PC's are 120 minutes apart."), "{}", bench.printed());
    assert!(bench.printed().contains("this tool never changes the phone's"));
    assert!(bench.phone.changed_nothing());
    assert!(!bench.places().family_file().exists());
    assert!(bench.ui.prompts.is_empty(), "no PIN asked for");

    let mut bench = Bench::new();
    bench.phone.wall_secs = NOW - 10 * 60;
    assert_eq!(bench.setup("setup --child Asha --minutes 20"), DONE);
    assert!(bench.printed().contains("The phone's clock and this PC's are 10 minutes apart. Near midnight"));

    let mut bench = Bench::new();
    bench.phone.wall_secs = NOW + 30;
    assert_eq!(bench.setup("setup --child Asha --minutes 20"), DONE);
    assert!(!bench.printed().contains("minutes apart"));
}

#[test]
fn the_child_and_the_rules_reach_the_phones_engine_in_any_zone() {
    let mut bench = Bench::new();
    bench.phone.zone = "-0800".to_string();
    assert_eq!(bench.setup("setup --child Mira --minutes 0"), DONE, "{}", bench.printed());
    assert!(bench.printed().contains("The phone's clock is in UTC-08:00"));
    let mut engine = bench.engine();
    assert_eq!(engine.view().unwrap().rules.utc_offset_secs, -28_800);
    let mut facts = bench.phone.facts(true, limo_phone::Front::app("org.example.reader"));
    facts.utc_offset_secs = -28_800;
    let orders = engine.tick(&facts);
    // No screen time today: locked at once.
    assert!(orders.time_left.locked, "{orders:?}");
}
