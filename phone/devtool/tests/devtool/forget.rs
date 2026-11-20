//! `forget`, `uninstall` and `log`.

use limo_phone_dev::{DONE, FAILED, REFUSED};

use crate::fake::{App, Bench, FORGET, PAIRING, REMOVE, STATE};

#[test]
fn forget_wipes_the_phone_and_this_pc_so_that_setup_runs_again() {
    let mut bench = Bench::new();
    assert_eq!(bench.setup("setup --child Asha --minutes 20"), DONE);
    bench.clear();
    assert_eq!(bench.run("forget"), DONE, "{}", bench.printed());
    let printed = bench.printed();
    assert!(printed.contains("Asked LiMo to wipe the test family. Android: Broadcast completed: result=0"), "{printed}");
    assert!(printed.contains("LiMo wiped it: its kept state is gone from the phone."));
    assert!(printed.contains("Deleted the test family on this PC."));
    assert!(bench.phone.shell_commands().contains(&FORGET.to_string()));
    assert!(!bench.phone.files.contains_key(STATE));
    assert!(!bench.places().family_file().exists());
    assert!(bench.log().contains("forget wiped the test family on the phone and deleted it on this PC."));

    bench.clear();
    assert_eq!(bench.setup("setup --child Mira --minutes 30"), DONE, "{}", bench.printed());
    assert!(bench.printed().contains("LiMo took it: its state on the phone now holds Mira's test phone."));
}

#[test]
fn forget_leaves_another_familys_state_alone() {
    let mut first = Bench::new();
    assert_eq!(first.setup("setup --child Asha --minutes 20"), DONE);
    let mut bench = Bench::new();
    assert_eq!(bench.setup("setup --child Mira --minutes 20"), DONE);
    // This PC's test family, but the phone holds the first one's.
    bench.phone.files = first.phone.files.clone();
    bench.clear();
    assert_eq!(bench.run("forget"), REFUSED);
    assert!(bench.printed().contains("LiMo on this phone holds a family this PC did not make (device device-"), "{}", bench.printed());
    assert!(bench.printed().contains("forget --this-pc-only deletes it alone."));
    assert!(bench.phone.changed_nothing());
    assert_eq!(bench.phone.files, first.phone.files);
    assert!(bench.places().family_file().exists());

    // A damaged state: whose it is cannot be told.
    bench.phone.files.insert(STATE.to_string(), b"limo-phone-state\nformat 1\n".to_vec());
    bench.clear();
    assert_eq!(bench.run("forget"), REFUSED);
    assert!(bench.printed().contains("so this PC cannot tell whose it is."));
    assert!(bench.phone.changed_nothing());
}

#[test]
fn forget_this_pc_only_asks_nothing_of_any_phone() {
    let mut bench = Bench::new();
    assert_eq!(bench.setup("setup --child Asha --minutes 20"), DONE);
    bench.phone.devices.clear();
    bench.clear();
    assert_eq!(bench.run("forget --this-pc-only"), DONE);
    assert!(bench.phone.calls.is_empty());
    assert!(bench.printed().contains("Deleted the test family on this PC."));
    assert!(bench.printed().contains("The phone was not asked."));
    assert!(!bench.places().family_file().exists());
    assert!(bench.phone.files.contains_key(STATE), "the phone keeps it");
    bench.clear();
    assert_eq!(bench.run("forget --this-pc-only"), DONE);
    assert!(bench.printed().contains("There was no test family on this PC."));
}

#[test]
fn forget_with_limo_gone_from_the_phone_deletes_this_pcs_side() {
    let mut bench = Bench::new();
    assert_eq!(bench.setup("setup --child Asha --minutes 20"), DONE);
    bench.clear();
    assert_eq!(bench.run("uninstall"), DONE);
    assert!(bench.printed().contains("Removed LiMo from the phone, with everything it kept."));
    assert!(bench.printed().contains("The test family on this PC is still there: run forget to delete it too."));
    bench.clear();
    assert_eq!(bench.run("forget"), DONE);
    assert!(bench.printed().contains("LiMo is not on this phone, so nothing of the test family is left on it."));
    assert!(!bench.places().family_file().exists());
}

#[test]
fn forget_removes_a_pairing_file_never_taken_and_refuses_a_release_build() {
    let mut bench = Bench::new();
    bench.phone.files.insert(PAIRING.to_string(), b"{}".to_vec());
    assert_eq!(bench.run("forget"), DONE);
    assert!(bench.printed().contains("Removed a pairing file LiMo had not taken."));
    assert!(bench.phone.shell_commands().contains(&REMOVE.to_string()));
    assert!(bench.phone.files.is_empty());

    let mut release = Bench::new();
    assert_eq!(release.setup("setup --child Asha --minutes 20"), DONE);
    release.phone.app = Some(App { debuggable: false, version: "0.1.0".to_string() });
    release.clear();
    assert_eq!(release.run("forget"), REFUSED);
    assert!(release.printed().contains("LiMo on this phone is a release build now"));
    assert!(release.places().family_file().exists());
    assert!(release.phone.changed_nothing());
}

#[test]
fn forget_says_so_when_limo_does_not_wipe_its_state() {
    let mut bench = Bench::new();
    assert_eq!(bench.setup("setup --child Asha --minutes 20"), DONE);
    bench.phone.receiver = false;
    bench.clear();
    assert_eq!(bench.run("forget"), FAILED);
    assert!(bench.printed().contains("LiMo did not wipe its state within 15 seconds. The test family on this PC was kept, so forget can run again."));
    assert!(bench.places().family_file().exists());
}

#[test]
fn uninstall_refused_while_limo_is_a_device_admin_and_harmless_twice() {
    let mut bench = Bench::new();
    bench.phone.admin = true;
    assert_eq!(bench.run("uninstall"), REFUSED);
    assert!(bench.printed().contains("Android will not remove LiMo while it is a device admin on this phone (its protection)."));
    assert!(bench.printed().contains("This tool never changes it."));
    assert!(bench.phone.app.is_some());

    bench.phone.admin = false;
    bench.clear();
    assert_eq!(bench.run("uninstall"), DONE);
    let uninstalls: Vec<_> = bench.phone.calls.iter().filter(|c| c.args.contains(&"uninstall".to_string())).collect();
    assert_eq!(uninstalls[0].args, vec!["-s", "R5CN70TEST", "uninstall", "ai.lucidloom.limo.child"]);
    assert_eq!(uninstalls[0].timeout_secs, 60);
    bench.clear();
    assert_eq!(bench.run("uninstall"), DONE);
    assert!(bench.printed().contains("LiMo's child form is not installed on this phone. Nothing was changed."));
}

#[test]
fn log_shows_only_limos_lines_and_stops_at_its_limit() {
    let mut bench = Bench::new();
    bench.phone.log = vec![
        "--------- beginning of main".to_string(),
        "11-20 21:42:03.123 I/LiMo    ( 1234): engine started".to_string(),
        "11-20 21:42:03.200 I/ActivityManager(  999): Start proc 1234:ai.lucidloom.limo.child".to_string(),
        "11-20 21:42:04.001 W/LiMoWatch( 1234): accessibility is off".to_string(),
    ];
    assert_eq!(bench.run("log --minutes 2"), DONE);
    let printed = bench.printed();
    assert!(printed.contains("Following LiMo's log on the phone (tags LiMo and LiMoWatch). Ctrl+C stops it; it stops by itself after 2 minutes."));
    assert!(printed.contains("I/LiMo    ( 1234): engine started"));
    assert!(printed.contains("W/LiMoWatch( 1234): accessibility is off"));
    assert!(!printed.contains("ActivityManager") && !printed.contains("beginning of main"));
    assert!(printed.contains("Stopped after 2 minutes. Run log again to go on."));
    let logcat = bench.phone.calls.iter().find(|c| c.args.contains(&"logcat".to_string())).unwrap();
    assert_eq!(logcat.args, vec!["-s", "R5CN70TEST", "logcat", "-v", "time", "-s", "LiMo:V", "LiMoWatch:V"]);
    assert_eq!(logcat.timeout_secs, 120);
    assert!(bench.phone.changed_nothing());
}
