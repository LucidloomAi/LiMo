//! A phone that holds a family this PC does not have, as the first real run
//! left one: `check`, `status`, `forget` and `setup` say so and change
//! nothing, and `forget --phone-only` is the one way to clean it, LiMo's
//! own state alone, wiped by LiMo itself and checked.

use limo_phone_dev::{DONE, FAILED, REFUSED};

use crate::fake::{App, Bench, FORGET, STATE};

/// A phone holding the family another PC's tool made, and a PC with none.
fn orphan() -> Bench {
    let mut elsewhere = Bench::new();
    assert_eq!(elsewhere.setup("setup --child Asha --minutes 20"), DONE);
    let mut bench = Bench::new();
    bench.phone.files = elsewhere.phone.files.clone();
    bench.phone.child_name = elsewhere.phone.child_name.clone();
    bench
}

#[test]
fn each_command_says_so_and_forget_phone_only_cleans_it() {
    let mut bench = orphan();
    assert_eq!(bench.run("check"), DONE);
    assert!(bench.printed().contains("LiMo's state on the phone: it holds a family this PC did not make (device device-"));
    assert!(bench.printed().contains("Next: forget --phone-only, which wipes the family LiMo keeps on this phone and nothing else; then setup."));

    bench.clear();
    assert_eq!(bench.run("status"), DONE);
    assert!(bench.printed().contains("Only what its state says is shown; forget --phone-only wipes it."), "{}", bench.printed());

    bench.clear();
    assert_eq!(bench.run("forget"), REFUSED);
    assert!(bench.printed().contains("forget leaves it alone: forget --phone-only wipes it, and nothing else."));
    assert!(!bench.printed().contains("The test family on this PC was kept"), "this PC has none");
    assert!(bench.phone.changed_nothing());

    bench.clear();
    assert_eq!(bench.setup("setup --child Mira --minutes 30"), REFUSED);
    assert!(bench.printed().contains("To start afresh, run forget --phone-only"));
    assert!(bench.phone.changed_nothing());

    bench.clear();
    assert_eq!(bench.run("forget --phone-only"), DONE, "{}", bench.printed());
    let printed = bench.printed();
    for line in [
        "forget --phone-only asks LiMo on the phone to wipe the family it keeps, whoever's it is, checks that it did, and leaves the test family on this PC as it is.",
        "Asked LiMo to wipe it. Android: Broadcast completed: result=0",
        "LiMo wiped it: its kept state is gone from the phone.",
        "Next: setup, to make a new test family.",
    ] {
        assert!(bench.ui.lines.iter().any(|l| l == line), "missing \"{line}\" in:\n{printed}");
    }
    assert!(printed.contains("LiMo on the phone holds a family this PC did not make (device device-"));
    // Only LiMo's own wipe was asked for, and it is gone.
    assert_eq!(bench.phone.shell_commands().iter().filter(|s| *s == FORGET).count(), 1);
    assert!(!bench.phone.files.contains_key(STATE) && bench.phone.child_name.is_none());
    assert!(bench.log().contains("forget --phone-only wiped LiMo's state on the phone."));

    bench.clear();
    assert_eq!(bench.setup("setup --child Mira --minutes 30"), DONE, "{}", bench.printed());
}

#[test]
fn the_first_runs_orphan_whose_state_cannot_be_read_is_cleaned_too() {
    let mut bench = orphan();
    bench.phone.mangles_exec_out = true;
    assert_eq!(bench.run("forget"), FAILED);
    assert!(bench.printed().contains("forget --phone-only wipes the state LiMo keeps on the phone without reading it."), "{}", bench.printed());
    assert!(bench.phone.files.contains_key(STATE));
    bench.clear();
    assert_eq!(bench.run("forget --phone-only"), DONE, "{}", bench.printed());
    assert!(bench.printed().contains("LiMo on the phone holds a state this tool could not read back ("));
    assert!(!bench.phone.files.contains_key(STATE));

    // A damaged state, likewise.
    let mut bench = Bench::new();
    bench.phone.files.insert(STATE.to_string(), b"limo-phone-state\nformat 1\n".to_vec());
    assert_eq!(bench.run("forget --phone-only"), DONE);
    assert!(bench.printed().contains("LiMo on the phone holds a state it cannot read (The phone's kept state is"), "{}", bench.printed());
    assert!(!bench.phone.files.contains_key(STATE));
}

#[test]
fn forget_phone_only_leaves_this_pcs_family_and_refuses_what_it_cannot_do() {
    // This PC's own: the phone is wiped, the family here stays, and says so.
    let mut bench = Bench::new();
    assert_eq!(bench.setup("setup --child Asha --minutes 20"), DONE);
    bench.clear();
    assert_eq!(bench.run("forget --phone-only"), DONE);
    assert!(bench.printed().contains("LiMo on the phone holds this PC's test family, Asha's test phone."));
    assert!(bench.printed().contains("The test family on this PC is still there: forget --this-pc-only deletes it, and setup needs it gone."));
    assert!(bench.places().family_file().exists());
    assert!(!bench.phone.files.contains_key(STATE));

    // Nothing kept: nothing to do.
    bench.clear();
    assert_eq!(bench.run("forget --phone-only"), DONE);
    assert!(bench.printed().contains("LiMo on the phone keeps no state: there is nothing to wipe."));
    assert!(bench.phone.changed_nothing());

    // No LiMo; a release build; a receiver that does not wipe.
    let mut bench = Bench::new();
    bench.phone.app = None;
    assert_eq!(bench.run("forget --phone-only"), DONE);
    assert!(bench.printed().contains("LiMo is not on this phone, so there is nothing of LiMo's to wipe."));
    let mut bench = orphan();
    bench.phone.app = Some(App { debuggable: false, version: "0.1.0".to_string() });
    assert_eq!(bench.run("forget --phone-only"), REFUSED);
    assert!(bench.printed().contains("LiMo on this phone is a release build"));
    assert!(bench.phone.changed_nothing());
    let mut bench = orphan();
    bench.phone.receiver = false;
    assert_eq!(bench.run("forget --phone-only"), FAILED);
    assert!(bench.printed().contains("LiMo did not wipe its state within 15 seconds."));
    assert!(bench.phone.files.contains_key(STATE));
}
