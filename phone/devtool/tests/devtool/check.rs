//! `check`: reads only, says each fact, and what to do next.

use limo_phone_dev::{DONE, REFUSED};

use crate::fake::{App, Bench, PAIRING, STATE};

#[test]
fn check_on_a_ready_phone_says_each_fact_and_changes_nothing() {
    let mut bench = Bench::new();
    assert_eq!(bench.run("check"), DONE);
    let printed = bench.printed();
    for line in [
        "check reads only; it changes nothing.",
        "adb: adb 1.0.41 (the fake)",
        "Phone: R5CN70TEST",
        "Android 13 (API 33), samsung SM-G988B.",
        "LiMo's child form: installed, version 0.1.0, a debug build: run-as works, so this tool can reach LiMo's files.",
        "LiMo's state on the phone: none yet, so no family is on it.",
        "The test family on this PC: none.",
        "Next: setup --child NAME --minutes N.",
    ] {
        assert!(bench.ui.lines.iter().any(|l| l == line), "missing \"{line}\" in:\n{printed}");
    }
    assert!(bench.phone.changed_nothing());
    assert!(!bench.places().family_file().exists(), "check made no family file");
}

#[test]
fn check_says_what_each_kind_of_phone_needs_next() {
    // No LiMo.
    let mut bench = Bench::new();
    bench.phone.app = None;
    assert_eq!(bench.run("check"), DONE);
    assert!(bench.printed().contains("LiMo's child form: not installed."));
    assert!(bench.printed().contains("Next: install --apk PATH, with the debug child form."));
    assert!(bench.phone.changed_nothing());

    // A release build.
    let mut bench = Bench::new();
    bench.phone.app = Some(App { debuggable: false, version: "0.1.0".to_string() });
    assert_eq!(bench.run("check"), DONE);
    assert!(bench.printed().contains("installed, version 0.1.0, a release build. Android refuses run-as for it, so it cannot take a test family."));
    assert!(bench.printed().contains("Next: uninstall, then install the debug child form."));
    assert!(bench.phone.changed_nothing());

    // Too old an Android.
    let mut bench = Bench::new();
    bench.phone.sdk = 24;
    bench.phone.release = "7.0".to_string();
    assert_eq!(bench.run("check"), REFUSED);
    assert!(bench.printed().contains("LiMo needs Android 8.0 (API 26) or later"));
    assert!(bench.printed().contains("The test family on this PC: none."));
}

#[test]
fn check_tells_this_pcs_test_family_from_another_and_a_pairing_not_taken() {
    let mut bench = Bench::new();
    assert_eq!(bench.setup("setup --child Asha --minutes 20"), DONE);
    bench.clear();
    assert_eq!(bench.run("check"), DONE);
    assert!(bench.printed().contains("LiMo's state on the phone: it holds this PC's test family, Asha's test phone."));
    assert!(bench.printed().contains("The test family on this PC: Asha, 20 minutes a day, with its test phone paired."));
    assert!(bench.printed().contains("Next: status, log or code; or forget, to start again."));
    assert!(bench.phone.changed_nothing());

    // The same phone, seen from another PC's test family: not ours.
    let mut other = Bench::new();
    other.phone.files = bench.phone.files.clone();
    assert_eq!(other.run("check"), DONE);
    assert!(other.printed().contains("LiMo's state on the phone: it holds a family this PC did not make (device device-"));
    assert!(other.printed().contains("Next: forget --phone-only, which wipes the family LiMo keeps on this phone and nothing else; then setup."));

    // A damaged state, and a pairing file LiMo has not taken.
    let mut damaged = Bench::new();
    damaged.phone.files.insert(STATE.to_string(), b"limo-phone-state\nformat 1\ncheck 0\n{".to_vec());
    damaged.phone.files.insert(PAIRING.to_string(), b"{}".to_vec());
    assert_eq!(damaged.run("check"), DONE);
    assert!(damaged.printed().contains("LiMo's state on the phone cannot be read. The phone's kept state is damaged."), "{}", damaged.printed());
    assert!(damaged.printed().contains("A pairing file waits in LiMo's files: LiMo has not taken it."));
    assert!(damaged.phone.changed_nothing());

    // The test family on this PC, with LiMo on the phone wiped some other
    // way.
    bench.phone.files.clear();
    bench.clear();
    assert_eq!(bench.run("check"), DONE);
    assert!(bench.printed().contains("Next: the test family on this PC is not on this phone. Run forget, then setup again."));
}
