//! Each phone the tool cannot work with, and adb missing: every command
//! that needs a phone says what to do, in its own words, and changes
//! nothing.

use limo_phone_dev::{FAILED, REFUSED};

use crate::fake::{App, Bench, SERIAL};

/// Every command that needs a phone, as typed.
const PHONE_COMMANDS: [&str; 6] = ["check", "setup --child Asha --minutes 20", "status", "log --minutes 1", "forget", "uninstall"];

fn every_command_refuses(bench_with: impl Fn(&mut Bench), says: &str) {
    for command in PHONE_COMMANDS {
        let mut bench = Bench::new();
        bench_with(&mut bench);
        bench.ui.typed.extend(["482915".to_string(), "482915".to_string()]);
        assert_eq!(bench.run(command), REFUSED, "{command}: {}", bench.printed());
        assert!(bench.printed().contains(says), "{command}: {}", bench.printed());
        // Only the list of phones was asked for; nothing was sent to one.
        assert!(bench.phone.phone_calls().is_empty(), "{command}: {:?}", bench.phone.calls);
        assert!(!bench.places().family_file().exists(), "{command}");
        // The PIN is asked for only once a phone is there to take it.
        assert!(bench.ui.prompts.is_empty(), "{command}");
    }
}

#[test]
fn no_phone_attached() {
    every_command_refuses(
        |b| b.phone.devices.clear(),
        "No phone is attached. Plug the phone in with a USB cable, unlock it, and check that USB debugging is on",
    );
    every_command_refuses(|b| b.phone.devices.clear(), "This tool never turns it on by itself.");
}

#[test]
fn two_phones_attached() {
    every_command_refuses(
        |b| b.phone.devices.push(("emulator-5554".to_string(), "device".to_string())),
        "More than one phone or emulator is attached (R5CN70TEST device, emulator-5554 device). Unplug the others",
    );
}

#[test]
fn a_phone_that_has_not_allowed_this_pc() {
    every_command_refuses(
        |b| b.phone.devices = vec![(SERIAL.to_string(), "unauthorized".to_string())],
        "has not allowed this PC yet. Unlock it and tap Allow on the \"Allow USB debugging?\" prompt.",
    );
}

#[test]
fn a_phone_offline() {
    every_command_refuses(
        |b| b.phone.devices = vec![(SERIAL.to_string(), "offline".to_string())],
        "is offline. Unplug it and plug it in again",
    );
}

#[test]
fn adb_not_found() {
    every_command_refuses(|b| b.phone.adb_missing = true, "adb was not found");
    let mut bench = Bench::new();
    bench.phone.adb_missing = true;
    bench.run("check");
    assert!(bench.phone.calls.is_empty());
    assert!(bench.printed().contains("Install Android's SDK platform tools, or set ANDROID_HOME"));
}

#[test]
fn forget_with_no_phone_points_at_this_pc_only() {
    let mut bench = Bench::new();
    bench.phone.devices.clear();
    assert_eq!(bench.run("forget"), REFUSED);
    assert!(bench.printed().contains("If that phone is gone for good, forget --this-pc-only deletes the test family on this PC alone."));
}

#[test]
fn a_release_build_cannot_be_reached_and_says_so() {
    for command in ["setup --child Asha --minutes 20", "status"] {
        let mut bench = Bench::new();
        bench.phone.app = Some(App { debuggable: false, version: "0.1.0".to_string() });
        assert_eq!(bench.run(command), REFUSED, "{command}");
        assert!(bench.printed().contains("LiMo on this phone is a release build. Android refuses run-as for a release build"), "{command}");
        assert!(bench.printed().contains("Run uninstall (it removes LiMo with everything it kept), then install the debug child form."));
        assert!(bench.phone.changed_nothing(), "{command}");
        assert!(!bench.places().family_file().exists());
    }
}

#[test]
fn a_phone_without_limo_says_install_first() {
    for command in ["setup --child Asha --minutes 20", "status"] {
        let mut bench = Bench::new();
        bench.phone.app = None;
        assert_eq!(bench.run(command), REFUSED, "{command}");
        assert!(bench.printed().contains("LiMo's child form is not installed on this phone. Run install --apk PATH"), "{command}");
        assert!(bench.phone.changed_nothing(), "{command}");
    }
}

#[test]
fn a_phone_whose_maker_broke_run_as_says_so() {
    let mut bench = Bench::new();
    bench.phone.run_as_broken = true;
    assert_eq!(bench.run("setup --child Asha --minutes 20"), REFUSED);
    assert!(bench.printed().contains("Android refused run-as for LiMo on this phone: \"run-as: couldn't stat"));
    assert!(bench.printed().contains("Some makers' builds of Android break run-as"));
    assert!(bench.phone.changed_nothing());
}

#[test]
fn an_android_older_than_eight_is_refused() {
    let mut bench = Bench::new();
    bench.phone.sdk = 25;
    bench.phone.release = "7.1.2".to_string();
    assert_eq!(bench.run("setup --child Asha --minutes 20"), REFUSED);
    assert!(bench.printed().contains("Android 7.1.2 (API 25)"));
    assert!(bench.printed().contains("LiMo needs Android 8.0 (API 26) or later"));
    assert!(bench.phone.changed_nothing());
}

#[test]
fn a_phone_unplugged_part_way_or_not_answering_is_said_plainly() {
    let mut bench = Bench::new();
    assert_eq!(bench.setup("setup --child Asha --minutes 20"), 0);

    // Listed, then gone after two calls: unplugged.
    bench.clear();
    bench.phone.gone_after = Some(2);
    assert_eq!(bench.run("status"), FAILED);
    assert!(bench.printed().contains("The phone is no longer there: it was unplugged, or the cable is loose."), "{}", bench.printed());

    // A call that never ends is stopped at its timeout.
    bench.clear();
    bench.phone.gone_after = None;
    bench.phone.hangs_on = Some(crate::fake::READ.to_string());
    assert_eq!(bench.run("status"), FAILED);
    assert!(bench.printed().contains("The phone did not answer within 30 seconds, so the call was stopped."), "{}", bench.printed());
    let read = bench.phone.calls.iter().find(|c| c.shell() == Some(crate::fake::READ)).unwrap();
    assert_eq!(read.timeout_secs, 30);
}

#[test]
fn every_call_has_a_timeout() {
    let mut bench = Bench::new();
    assert_eq!(bench.setup("setup --child Asha --minutes 20"), 0);
    bench.run("code");
    bench.run("status");
    bench.run("check");
    bench.run("log --minutes 2");
    bench.run("forget");
    bench.run("uninstall");
    assert!(bench.phone.calls.len() > 20);
    for call in &bench.phone.calls {
        assert!(call.timeout_secs > 0 && call.timeout_secs <= 180 || call.args.contains(&"logcat".to_string()), "{call:?}");
        if call.args.contains(&"logcat".to_string()) {
            // Followed: the minutes asked for; read once: 30 seconds.
            let read_once = call.args.contains(&"-d".to_string());
            assert_eq!(call.timeout_secs, if read_once { 30 } else { 120 });
        }
    }
}
