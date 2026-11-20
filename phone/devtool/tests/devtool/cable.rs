//! When adb sees no phone, what Windows lists on the PC's USB ports says
//! why: one story for each case, every command saying the same, and
//! Windows' list never read when adb already sees a phone.

use limo_phone_dev::REFUSED;
use limo_phone_dev::usb::{Kind, UsbDevice};

use crate::fake::{Bench, SERIAL};

/// Every command that needs a phone, as typed.
const PHONE_COMMANDS: [&str; 6] = ["check", "setup --child Asha --minutes 20", "status", "log --minutes 1", "forget", "uninstall"];

fn portable(name: &str) -> UsbDevice {
    UsbDevice { kind: Kind::PortableDevice, name: Some(name.to_string()), bus: "USB".to_string(), working: true }
}

fn debugging(name: &str, working: bool) -> UsbDevice {
    UsbDevice { kind: Kind::DebugInterface, name: Some(name.to_string()), bus: "USB".to_string(), working }
}

/// A USB stick's volume as Windows lists it: a portable device, on the
/// software bus Windows makes such volumes on.
fn stick() -> UsbDevice {
    UsbDevice { kind: Kind::PortableDevice, name: Some("E:\\".to_string()), bus: "SWD".to_string(), working: true }
}

/// `check` with adb seeing nothing and Windows listing these: the lines it
/// printed after the phone was looked for.
fn check_with(seen: Vec<UsbDevice>) -> Bench {
    let mut bench = Bench::new();
    bench.phone.devices.clear();
    bench.usb.seen = Ok(seen);
    assert_eq!(bench.run("check"), REFUSED, "{}", bench.printed());
    assert_eq!(bench.usb.looked, 1, "Windows' list read once");
    assert!(bench.phone.phone_calls().is_empty(), "nothing sent to any phone");
    assert!(bench.ui.lines.iter().any(|l| l == "Nothing was changed on the phone."));
    bench
}

/// The line that says what this PC sees, and the one after it.
fn sees_and_advice(bench: &Bench) -> (String, String) {
    let at = bench.ui.lines.iter().position(|l| l.starts_with("This PC sees")).expect("a line of what this PC sees");
    (bench.ui.lines[at].clone(), bench.ui.lines[at + 1].clone())
}

#[test]
fn windows_sees_no_phone_the_cable_probably_only_charges() {
    let bench = check_with(vec![]);
    assert_eq!(
        sees_and_advice(&bench),
        (
            "This PC sees no phone on its USB ports.".to_string(),
            "The cable probably only charges: many cables carry no data. Use the cable that came with the phone, or one \
             known to move files; plug it into a port on the PC itself, not a hub; and unlock the phone."
                .to_string()
        )
    );
}

#[test]
fn windows_sees_the_phone_and_adb_does_not_usb_debugging_is_off() {
    let bench = check_with(vec![portable("Galaxy S20 Ultra 5G")]);
    assert_eq!(
        sees_and_advice(&bench),
        (
            "This PC sees what looks like a phone on its USB ports (Galaxy S20 Ultra 5G), but adb does not.".to_string(),
            "If that is the phone, its cable carries data, but USB debugging is off. Switch it on, on the phone \
             (Settings, Developer options); this tool never turns it on by itself. If it is on already, unplug the \
             phone and plug it in again, unlocked. If it is something else that offers pictures or files, such as a \
             camera, the phone's own cable probably only charges: use the one that came with the phone."
                .to_string()
        )
    );
    // A phone Windows gives no name is still a phone.
    let bench = check_with(vec![UsbDevice { kind: Kind::PortableDevice, name: None, bus: "USB".to_string(), working: true }]);
    assert_eq!(sees_and_advice(&bench).0, "This PC sees what looks like a phone on its USB ports, but adb does not.");
}

#[test]
fn a_usb_stick_alone_is_no_phone_the_cable_probably_only_charges() {
    // A stick or a memory card joins Windows' portable device class too,
    // as a volume on Windows' software bus: it is not taken for a phone.
    let bench = check_with(vec![stick()]);
    let (sees, advice) = sees_and_advice(&bench);
    assert_eq!(sees, "This PC sees no phone on its USB ports.");
    assert!(advice.starts_with("The cable probably only charges"));
    assert!(!bench.printed().contains("E:\\"), "the stick is never named");
}

#[test]
fn a_usb_stick_beside_a_phone_is_left_out_and_only_the_phone_named() {
    let bench = check_with(vec![stick(), portable("Galaxy S20 Ultra 5G")]);
    assert_eq!(
        sees_and_advice(&bench).0,
        "This PC sees what looks like a phone on its USB ports (Galaxy S20 Ultra 5G), but adb does not."
    );
    let bench = check_with(vec![portable("Galaxy S20 Ultra 5G"), stick(), debugging("SAMSUNG_Android", false)]);
    assert_eq!(
        sees_and_advice(&bench).0,
        "This PC sees the debugging interface of a phone (Galaxy S20 Ultra 5G), but has no working driver for it."
    );
    assert!(!bench.printed().contains("E:\\"));
}

#[test]
fn windows_sees_the_debugging_interface_with_no_working_driver() {
    let bench = check_with(vec![portable("Galaxy S20 Ultra 5G"), debugging("SAMSUNG_Android", false)]);
    assert_eq!(
        sees_and_advice(&bench),
        (
            "This PC sees the debugging interface of a phone (Galaxy S20 Ultra 5G), but has no working driver for it."
                .to_string(),
            "The phone maker's USB driver has to be installed on this PC before adb can reach the phone. Install it, \
             then unplug the phone and plug it in again. This tool installs nothing."
                .to_string()
        )
    );
    // With no portable device, the interface's own name.
    let bench = check_with(vec![debugging("SAMSUNG_Android", false)]);
    assert_eq!(
        sees_and_advice(&bench).0,
        "This PC sees the debugging interface of a phone (SAMSUNG_Android), but has no working driver for it."
    );
}

#[test]
fn windows_sees_the_debugging_interface_working_and_adb_lists_nothing() {
    let bench = check_with(vec![portable("Galaxy S20 Ultra 5G"), debugging("ADB Interface", true)]);
    assert_eq!(
        sees_and_advice(&bench),
        (
            "This PC sees the debugging interface of a phone (Galaxy S20 Ultra 5G), with its driver working, but adb \
             does not list it."
                .to_string(),
            "Unplug the phone and plug it in again, unlocked; if adb still lists nothing, run adb kill-server, then \
             this again."
                .to_string()
        )
    );
}

#[test]
fn every_command_says_the_same_and_forget_still_points_at_this_pc_only() {
    for command in PHONE_COMMANDS {
        let mut bench = Bench::new();
        bench.phone.devices.clear();
        bench.usb.seen = Ok(vec![portable("Galaxy S20 Ultra 5G")]);
        bench.ui.typed.extend(["482915".to_string(), "482915".to_string()]);
        assert_eq!(bench.run(command), REFUSED, "{command}");
        assert!(bench.printed().contains("This PC sees what looks like a phone on its USB ports (Galaxy S20 Ultra 5G), but adb does not."), "{command}");
        assert_eq!(bench.usb.looked, 1, "{command}");
        assert!(bench.ui.prompts.is_empty(), "{command}");
        if command == "forget" {
            assert!(bench.printed().contains("forget --this-pc-only deletes the test family on this PC alone."));
        }
    }
}

#[test]
fn windows_is_never_asked_when_adb_lists_a_phone() {
    // Ready, not allowed, offline, or two of them: adb's own answer is
    // enough, and Windows' list is not read at all.
    for devices in [
        vec![(SERIAL, "device")],
        vec![(SERIAL, "unauthorized")],
        vec![(SERIAL, "offline")],
        vec![(SERIAL, "device"), ("emulator-5554", "device")],
    ] {
        for command in PHONE_COMMANDS {
            let mut bench = Bench::new();
            bench.phone.devices = devices.iter().map(|(s, st)| (s.to_string(), st.to_string())).collect();
            bench.usb.seen = Ok(vec![portable("Galaxy S20 Ultra 5G")]);
            bench.ui.typed.extend(["482915".to_string(), "482915".to_string()]);
            bench.run(command);
            assert_eq!(bench.usb.looked, 0, "{command} with {devices:?}");
            assert!(!bench.printed().contains("This PC sees"), "{command} with {devices:?}");
        }
    }
    // The existing words stay for those cases.
    let mut bench = Bench::new();
    bench.phone.devices = vec![(SERIAL.to_string(), "unauthorized".to_string())];
    assert_eq!(bench.run("check"), REFUSED);
    assert!(bench.printed().contains("has not allowed this PC yet. Unlock it and tap Allow"));
}
