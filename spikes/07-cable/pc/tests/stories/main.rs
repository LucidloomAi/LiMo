//! The steps, against a fake phone (`fake.rs`): what each says, in what
//! order, exactly what it sends, and what it refuses before sending
//! anything.

mod apk;
mod fake;

use limo_cable_spike::cable::{OpenProblem, Via};
use limo_cable_spike::key::Key;
use limo_cable_spike::phone::{ACTIVITY, PACKAGE};
use limo_cable_spike::plan::{StepName, help, plan};
use limo_cable_spike::steps::{self, Exit};
use limo_cable_spike::ui::Recorder;

use fake::{Fake, LOOPBACK_BYTES};

/// One step, run against the fake phone.
type Run = Box<dyn Fn(&Fake, &mut Recorder) -> Exit>;

fn installed() -> Fake {
    Fake::with(|s| s.installed = true)
}

fn key() -> Key {
    Key { path: std::path::PathBuf::from("adbkey"), new: true, fingerprint: "AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89".to_string() }
}

/// The index of the first line that starts with `start`.
fn line_starting(ui: &Recorder, start: &str) -> Option<usize> {
    ui.lines.iter().position(|l| l.starts_with(start))
}

#[test]
fn every_step_says_what_it_does_to_the_phone_before_anything_reaches_it() {
    for via in [Via::Libusb, Via::Nusb, Via::Adb] {
        let runs: Vec<(StepName, Run)> = vec![
            (StepName::Connect, Box::new(move |f, ui| steps::connect(f, via, Some(&key()), ui))),
            (StepName::WriteSecret, Box::new(move |f, ui| steps::write_secret(f, via, ui))),
            (StepName::Start, Box::new(move |f, ui| steps::start(f, via, ui))),
            (StepName::Reverse, Box::new(move |f, ui| steps::reverse(f, via, 0, ui))),
            (StepName::Uninstall, Box::new(move |f, ui| steps::uninstall(f, via, ui))),
        ];
        for (step, run) in runs {
            let fake = installed();
            let mut ui = Recorder::default();
            assert_eq!(run(&fake, &mut ui), Exit::Done, "{step:?} by {via:?}:\n{}", ui.text());
            let does = line_starting(&ui, "What this does to the phone:").expect("said");
            let first_send = line_starting(&ui, "> ").expect("sent something");
            assert!(does < first_send, "{step:?}: said first");
            assert_eq!(fake.sent().last().map(String::as_str), Some("close"), "{step:?}: let go of the phone at the end");
            assert_eq!(fake.sent().iter().filter(|s| s.starts_with("open")).count(), 1, "{step:?}: opened once");
        }
    }
}

#[test]
fn connect_reads_four_facts_and_whether_the_test_app_is_there_and_nothing_else() {
    let fake = Fake::new();
    let mut ui = Recorder::default();
    assert_eq!(steps::connect(&fake, Via::Libusb, Some(&key()), &mut ui), Exit::Done);
    assert_eq!(
        fake.sent(),
        vec![
            "open libusb",
            "shell getprop ro.product.model",
            "shell getprop ro.build.version.release",
            "shell getprop ro.build.version.sdk",
            "shell settings get global adb_enabled",
            "shell pm list packages ai.lucidloom.limo.spike07",
            "close",
        ]
    );
    let text = ui.text();
    assert!(text.contains("Its fingerprint, as the phone shows it: AB:CD:EF:01"));
    assert!(text.contains("%LOCALAPPDATA%\\LiMo-spike07-TEST-ONLY\\adbkey (made just now"));
    assert!(text.contains("adb_client waits 10 seconds"));
    assert!(text.contains("Connected: a SM-G988B, Android 13 (API 33); USB debugging setting: 1; the test app: not installed."));
    // Google's adb's own key is named for that route, and the answers with
    // carriage returns read the same.
    let fake = Fake::with(|s| s.carriage_returns = true);
    let mut ui = Recorder::default();
    assert_eq!(steps::connect(&fake, Via::Adb, None, &mut ui), Exit::Done);
    assert!(ui.text().contains("Google's adb uses its own key"));
    assert!(ui.text().contains("Connected: a SM-G988B, Android 13 (API 33)"));
}

#[test]
fn opening_problems_are_said_and_change_nothing() {
    for (problem, exit, words) in [
        (OpenProblem::AdbServerRunning, Exit::Refused, "adb kill-server"),
        (OpenProblem::TwoPhones(2), Exit::Refused, "Unplug all but the test phone"),
        (OpenProblem::AdbMissing, Exit::Refused, "adb.exe was not found"),
        (OpenProblem::NoPhone, Exit::Failed, "run `drivers`"),
        (OpenProblem::DriverNotUsable("nusb says: incompatible driver".to_string()), Exit::Failed, "that is a result"),
        (OpenProblem::NotAllowed("10 seconds".to_string()), Exit::Failed, "Always allow from this computer"),
    ] {
        let fake = Fake::with(|s| s.open_problem = Some(problem.clone()));
        let mut ui = Recorder::default();
        assert_eq!(steps::start(&fake, Via::Nusb, &mut ui), exit, "{problem:?}");
        assert!(ui.text().contains(words), "{problem:?}: {}", ui.text());
        assert!(fake.sent().is_empty(), "{problem:?}: nothing sent");
    }
}

#[test]
fn install_takes_only_the_test_apps_release_build_and_checks_it_before_the_phone() {
    let refusals = [
        (std::path::PathBuf::from("no-such-file.apk"), "The test app's APK was looked for"),
        (apk::apk_file("other.apk", "ai.lucidloom.limo.child", Some(false)), "not the test app"),
        (apk::apk_file("guardian.apk", "com.example.anything", None), "not the test app"),
        (apk::apk_file("debug.apk", PACKAGE, Some(true)), "a debug build"),
    ];
    for (path, words) in refusals {
        let fake = Fake::new();
        let mut ui = Recorder::default();
        assert_eq!(steps::install(&fake, Via::Libusb, &path, &mut ui), Exit::Refused, "{}", path.display());
        assert!(ui.text().contains(words), "{}: {}", path.display(), ui.text());
        assert!(fake.sent().is_empty(), "nothing sent for {}", path.display());
    }
    let release = apk::apk_file("app-release.apk", PACKAGE, Some(false));
    let fake = Fake::new();
    let mut ui = Recorder::default();
    assert_eq!(steps::install(&fake, Via::Libusb, &release, &mut ui), Exit::Done, "{}", ui.text());
    assert_eq!(
        fake.sent(),
        vec!["open libusb", "install app-release.apk", "shell pm list packages ai.lucidloom.limo.spike07", "close"]
    );
    assert!(ui.text().contains("version 0.1-spike07, a release build"));
    assert!(ui.text().contains("Android said: Success"));
    assert!(ui.text().contains("Play Protect"));
    // Android's refusal, said with what to do.
    let fake = Fake::with(|s| s.install_answer = "Failure [INSTALL_FAILED_UPDATE_INCOMPATIBLE: signatures do not match]".to_string());
    let mut ui = Recorder::default();
    assert_eq!(steps::install(&fake, Via::Adb, &release, &mut ui), Exit::Failed);
    assert!(ui.text().contains("signed with another key"));
}

#[test]
fn the_secret_crosses_once_on_standard_input_and_is_never_shown() {
    for carriage_returns in [false, true] {
        let fake = Fake::with(|s| {
            s.installed = true;
            s.carriage_returns = carriage_returns;
        });
        let mut ui = Recorder::default();
        assert_eq!(steps::write_secret(&fake, Via::Libusb, &mut ui), Exit::Done, "{}", ui.text());
        let sent = fake.sent();
        let inputs: Vec<&String> = sent.iter().filter(|s| s.starts_with("input ")).collect();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0], "input content write --uri content://ai.lucidloom.limo.spike07.cable/offer (155 bytes)");
        let secret = String::from_utf8(fake.state().last_input.clone()).unwrap();
        assert!(secret.starts_with("spike07:"));
        let body = secret.trim_end().trim_start_matches("spike07:").to_string();
        let text = ui.text();
        assert!(!text.contains(&body), "the secret is never shown");
        assert!(!text.contains(secret.trim_end()));
        assert!(text.contains("155 bytes, check "));
        assert!(text.contains("The test app took exactly what this PC sent"));
        assert!(text.contains("written by user id 2000"));
        // Every command is a name or a path, never the secret.
        assert!(sent.iter().all(|s| !s.contains(&body)));
    }
}

#[test]
fn bytes_changed_on_the_way_or_a_missing_app_are_said() {
    let fake = Fake::with(|s| {
        s.installed = true;
        s.input_changed = true;
    });
    let mut ui = Recorder::default();
    assert_eq!(steps::write_secret(&fake, Via::Adb, &mut ui), Exit::Failed);
    assert!(ui.text().contains("156 bytes"), "{}", ui.text());
    assert!(ui.text().contains("changed on the way"));
    let fake = Fake::new();
    let mut ui = Recorder::default();
    assert_eq!(steps::write_secret(&fake, Via::Nusb, &mut ui), Exit::Failed);
    assert!(ui.text().contains("Run install first"));
    assert!(!fake.sent().iter().any(|s| s.starts_with("input")), "nothing written to an app that is not there");
}

#[test]
fn reverse_carries_the_apps_loopback_to_the_echo_server_and_is_removed_after() {
    let fake = installed();
    let mut ui = Recorder::default();
    assert_eq!(steps::reverse(&fake, Via::Nusb, 0, &mut ui), Exit::Done, "{}", ui.text());
    let sent = fake.sent();
    let port: u16 = sent.iter().find_map(|s| s.strip_prefix("reverse ")).and_then(|p| p.parse().ok()).unwrap();
    let order: Vec<String> = sent.into_iter().filter(|s| !s.starts_with("shell content query")).collect();
    assert_eq!(
        order,
        vec![
            "open nusb".to_string(),
            format!("reverse {port}"),
            format!("shell am start -n {ACTIVITY} --ei loopback_port {port}"),
            format!("reverse remove {port}"),
            "close".to_string(),
        ]
    );
    let text = ui.text();
    assert!(text.contains(&format!("{LOOPBACK_BYTES} bytes came and went back")), "{text}");
    assert!(text.contains("The test app says: every byte came back"));
    assert!(text.contains("Removed: nothing on the phone reaches this PC through the cable any more."));
    assert_eq!(fake.state().reverse, None);
}

#[test]
fn a_reverse_with_no_connection_fails_and_is_still_removed() {
    let fake = Fake::with(|s| {
        s.installed = true;
        s.no_loopback = true;
    });
    let mut ui = Recorder::default();
    let started = std::time::Instant::now();
    let wait = std::time::Duration::from_secs(2);
    assert_eq!(steps::reverse_waiting(&fake, Via::Libusb, 0, wait, &mut ui), Exit::Failed);
    assert!(started.elapsed() >= wait, "it waited the whole time");
    assert!(ui.text().contains("No connection came"));
    assert!(fake.sent().iter().any(|s| s.starts_with("reverse remove")));
    assert_eq!(fake.state().reverse, None);
}

#[test]
fn start_and_uninstall_touch_only_the_test_app() {
    let fake = installed();
    let mut ui = Recorder::default();
    assert_eq!(steps::start(&fake, Via::Adb, &mut ui), Exit::Done);
    assert!(ui.text().contains("\"LiMo spike 07\" should be open"));
    let mut ui = Recorder::default();
    assert_eq!(steps::uninstall(&fake, Via::Adb, &mut ui), Exit::Done);
    assert!(fake.sent().contains(&format!("uninstall {PACKAGE}")));
    assert!(ui.text().contains("The test app is gone from the phone."));
    let mut ui = Recorder::default();
    assert_eq!(steps::start(&fake, Via::Adb, &mut ui), Exit::Failed);
    assert!(ui.text().contains("Is it installed?"));
}

#[test]
fn debugging_off_is_last_needs_off_typed_and_says_the_link_has_ended() {
    for answers in [vec![], vec!["no".to_string()], vec!["of".to_string()]] {
        let fake = installed();
        let mut ui = Recorder { answers: answers.clone(), ..Recorder::default() };
        assert_eq!(steps::debugging_off(&fake, Via::Libusb, &mut ui), Exit::Refused, "{answers:?}");
        assert!(fake.sent().is_empty(), "{answers:?}: nothing sent");
        assert!(ui.text().contains("USB debugging stays as it is"));
    }
    let fake = installed();
    let mut ui = Recorder { answers: vec!["OFF".to_string()], ..Recorder::default() };
    assert_eq!(steps::debugging_off(&fake, Via::Libusb, &mut ui), Exit::Done, "{}", ui.text());
    assert_eq!(
        fake.sent(),
        vec![
            "open libusb",
            "shell settings get global adb_enabled",
            "shell settings put global adb_enabled 0",
            "close",
        ]
    );
    let text = ui.text();
    let warned = text.find("THIS IS THE LAST STEP").unwrap();
    let asked = text.find("Type OFF").unwrap();
    assert!(warned < asked, "it says first that the link ends");
    assert!(text.contains("The link ended as the setting changed, as expected."));
    assert!(text.contains("USB debugging looks off"));
    assert!(text.contains("The cable link has ended."));
    // Afterwards nothing reaches the phone.
    let mut ui = Recorder::default();
    assert_eq!(steps::start(&fake, Via::Libusb, &mut ui), Exit::Failed);
}

#[test]
fn the_plan_holds_every_step_by_every_route_and_ends_with_debugging_off() {
    let text = plan();
    for via in ["libusb", "nusb", "adb"] {
        for step in ["connect", "install", "write-secret", "start", "reverse", "uninstall"] {
            assert!(text.contains(&format!("limo-cable-spike {step} --via {via}")), "{step} by {via}");
        }
    }
    let numbered: Vec<&str> = text.lines().filter(|l| l.trim_start().chars().next().is_some_and(|c| c.is_ascii_digit())).collect();
    assert!(numbered.first().unwrap().contains("limo-cable-spike drivers"));
    assert!(numbered.last().unwrap().contains("limo-cable-spike debugging-off"));
    assert!(text.contains("LAST"));
    assert!(text.contains("never had Android tools"));
}

#[test]
fn the_help_says_first_what_it_never_does() {
    let text = help();
    let never = text.find("What it never does").unwrap();
    let usage = text.find("Usage:").unwrap();
    assert!(never < usage);
    for step in StepName::every() {
        assert!(text.contains(step.word()), "{}", step.word());
    }
    assert!(text.contains("never LiMo"));
    assert!(text.contains("last four characters"));
}

#[test]
fn a_command_line_that_is_not_right_is_refused_with_two() {
    let mut ui = Recorder::default();
    assert_eq!(limo_cable_spike::run(&["connect".to_string()], &mut ui), 2);
    assert!(ui.text().contains("needs a route"));
    let mut ui = Recorder::default();
    assert_eq!(limo_cable_spike::run(&["plan".to_string()], &mut ui), 0);
    assert!(ui.text().starts_with("The cable proof"));
}
