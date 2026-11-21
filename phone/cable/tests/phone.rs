//! LiMo's way to a phone, against the fake phone (`src/fake.rs`): a real
//! adb conversation over a fake cable, every call LiMo makes, and what the
//! PC sees for each kind of cable and driver. No phone, no USB.

mod common;

use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use limo_cable::commands::{Installed, OFFER_DOOR, PACKAGE, Started, install_advice};
use limo_cable::fake::{CableKind, Driver, FakeLimo, FakePhone, Person};
use limo_cable::key::AdbKey;
use limo_cable::open::Cable;
use limo_cable::phone::{CallError, DebuggingOff, InstallError, OpenProblem, Phone, Waits};

use common::apk::apk_file;

/// LiMo's key for these tests, made once in a folder of their own.
fn key() -> AdbKey {
    static KEY: OnceLock<AdbKey> = OnceLock::new();
    KEY.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("limo-cable-phone-key-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        AdbKey::load_or_make(&dir).unwrap()
    })
    .clone()
}

fn quick() -> Waits {
    Waits { prompt: Duration::from_secs(2), ..Waits::default() }
}

/// The fake phone, connected with LiMo's key, the person allowing it.
fn open(phone: &mut FakePhone) -> Phone {
    phone.open(&key(), quick(), &mut || {}).expect("connected")
}

#[test]
fn the_first_connection_asks_the_person_and_the_next_one_does_not() {
    let mut phone = FakePhone::new();
    let mut asked = 0;
    let connected = phone.open(&key(), quick(), &mut || asked += 1).unwrap();
    assert_eq!(asked, 1, "the caller is told when the phone asks");
    assert_eq!(phone.seen().prompts, 1);
    let facts = connected.facts().clone();
    assert_eq!((facts.manufacturer.as_str(), facts.model.as_str(), facts.android.as_str()), ("samsung", "SM-G988B", "13"));
    assert_eq!(facts.sdk, Some(33));
    assert_eq!(facts.usb_debugging, Some(true));
    assert!(!facts.too_old());
    connected.close();
    // The phone kept the key: no question now, even with nobody there.
    phone.state().person = Person::Away;
    let mut asked_again = 0;
    let again = phone.open(&key(), quick(), &mut || asked_again += 1).unwrap();
    assert_eq!(asked_again, 0);
    assert_eq!(phone.seen().prompts, 1);
    again.close();
}

#[test]
fn a_key_not_allowed_in_time_is_said() {
    let mut phone = FakePhone::with(|s| s.person = Person::Away);
    let started = std::time::Instant::now();
    let mut asked = 0;
    let result = phone.open(&key(), quick(), &mut || asked += 1);
    assert_eq!(result.err(), Some(OpenProblem::NotAllowed));
    assert_eq!(asked, 1);
    assert!(started.elapsed() >= Duration::from_secs(2) && started.elapsed() < Duration::from_secs(10));
}

#[test]
fn each_cable_and_driver_is_seen_as_the_pc_would_see_it() {
    // The test phone: Samsung's driver package, WinUSB under Samsung's own
    // composite driver, the Android class offered.
    let mut galaxy = FakePhone::new();
    let look = galaxy.look();
    let interfaces = look.interfaces.unwrap();
    assert_eq!(interfaces.len(), 1);
    assert_eq!(interfaces[0].driver.as_deref(), Some("WINUSB"));
    assert_eq!(interfaces[0].above.as_deref(), Some("dg_ssudbus"));
    assert!(interfaces[0].on_winusb());
    assert_eq!(look.windows.unwrap().len(), 2);
    // A cable that only charges: nothing.
    let mut charge = FakePhone::with(|s| s.cable = CableKind::ChargeOnly);
    let look = charge.look();
    assert!(look.windows.unwrap().is_empty() && look.interfaces.unwrap().is_empty());
    assert_eq!(charge.open(&key(), quick(), &mut || {}).err(), Some(OpenProblem::NoPhone));
    // USB debugging off: the phone's files, no debugging interface.
    let mut off = FakePhone::with(|s| s.debugging = false);
    assert_eq!(off.look().windows.unwrap().len(), 1);
    // A maker's own driver: listed, but LiMo cannot open it.
    let mut makers = FakePhone::with(|s| s.driver = Driver::MakersOwn);
    assert!(matches!(makers.open(&key(), quick(), &mut || {}), Err(OpenProblem::DriverNotUsable(_))));
    // No driver: Windows sees the interface not working.
    let mut none = FakePhone::with(|s| s.driver = Driver::None);
    assert!(!none.look().windows.unwrap()[1].working);
    // Another program holding it.
    let mut held = FakePhone::with(|s| s.held_by_another = true);
    assert_eq!(held.open(&key(), quick(), &mut || {}).err(), Some(OpenProblem::Busy));
}

#[test]
fn limo_is_looked_for_installed_with_replace_and_with_no_question() {
    let mut phone = FakePhone::new();
    let connected = open(&mut phone);
    assert_eq!(connected.limo().unwrap(), None);
    let apk = apk_file("limo-child-release.apk", PACKAGE, Some(false));
    assert_eq!(connected.install_limo(&apk).unwrap(), Installed::Done);
    let limo = connected.limo().unwrap().unwrap();
    assert_eq!(limo.version_name.as_deref(), Some("0.3.0"));
    assert_eq!(limo.version_code, Some(1));
    // Again, over it: replaced.
    assert_eq!(connected.install_limo(&apk).unwrap(), Installed::Done);
    assert_eq!(connected.limo().unwrap().unwrap().version_code, Some(2));
    let seen = phone.seen();
    assert_eq!(seen.installs, 2);
    assert_eq!(seen.prompts, 1, "only the key was asked about, never an install");
    let size = std::fs::metadata(&apk).unwrap().len();
    assert!(seen.services.contains(&format!("exec:cmd package install -r -S {size}")));
    assert!(seen.largest_write <= 256 * 1024, "never more than the phone takes in a message");
}

#[test]
fn the_apk_sent_is_the_one_checked_even_when_the_file_changes_on_the_way() {
    // The bad order, forced: LiMo checks a release build and tells the
    // phone its size; then, before the bytes go, the file becomes a debug
    // build of another size.
    let apk = apk_file("limo-child-release.apk", PACKAGE, Some(false));
    let checked = std::fs::read(&apk).unwrap();
    let mut debug = common::apk::apk_bytes(PACKAGE, Some(true));
    debug.extend_from_slice(&[0; 100]);
    let mut phone = FakePhone::with(|s| s.change_on_install = Some((apk.clone(), debug.clone())));
    let waits = Waits { install: Duration::from_secs(5), ..quick() };
    let connected = phone.open(&key(), waits, &mut || {}).unwrap();
    assert_eq!(connected.install_limo(&apk).unwrap(), Installed::Done);
    assert_eq!(std::fs::read(&apk).unwrap(), debug, "the file did change on the way");
    let seen = phone.seen();
    assert!(seen.installed == vec![checked.clone()], "the phone got exactly the build that was checked");
    assert!(seen.services.contains(&format!("exec:cmd package install -r -S {}", checked.len())));
}

#[test]
fn only_limos_release_build_is_installed_and_a_limo_of_another_key_is_left_alone() {
    let mut phone = FakePhone::with(|s| s.limo = Some(FakeLimo { version_code: 7, version_name: "0.2.0".to_string(), other_key: true }));
    let connected = open(&mut phone);
    for (name, package, debuggable, refusal) in [
        ("other.apk", "com.example.other", Some(false), "NotLimo"),
        ("debug.apk", PACKAGE, Some(true), "DebugBuild"),
    ] {
        let apk = apk_file(name, package, debuggable);
        let error = connected.install_limo(&apk).unwrap_err();
        let kind = match error {
            InstallError::NotLimo(_) => "NotLimo",
            InstallError::DebugBuild => "DebugBuild",
            _ => "other",
        };
        assert_eq!(kind, refusal, "{name}");
    }
    assert!(matches!(connected.install_limo(&PathBuf::from("no-such.apk")), Err(InstallError::NotAnApk(_))));
    assert_eq!(phone.seen().installs, 0, "nothing sent for any of them");
    // LiMo signed with another key: Android refuses, and it stays.
    let apk = apk_file("limo-child-release.apk", PACKAGE, Some(false));
    let refused = connected.install_limo(&apk).unwrap();
    let Installed::Refused { code, .. } = refused else { panic!("refused") };
    assert_eq!(code, "INSTALL_FAILED_UPDATE_INCOMPATIBLE");
    assert!(install_advice(&code).contains("leaves it as it is"));
    assert_eq!(connected.limo().unwrap().unwrap().version_code, Some(7), "never removed");
    assert!(phone.seen().services.iter().all(|s| !s.contains("uninstall")));
}

#[test]
fn the_offer_goes_through_the_door_from_memory_whole() {
    let mut phone = FakePhone::with(|s| s.limo = Some(FakeLimo { version_code: 1, version_name: "0.3.0".to_string(), other_key: false }));
    let connected = open(&mut phone);
    let offer = b"limo1:AAAABBBBCCCCDDDDEEEEFFFFGGGGHHHH\n";
    let handed = connected.hand_offer(offer).unwrap();
    assert!(handed.ok(), "{handed:?}");
    assert_eq!(phone.state().offers, vec![offer.to_vec()]);
    let seen = phone.seen();
    assert!(seen.commands.contains(&format!("content write --uri {OFFER_DOOR}")));
    assert!(seen.commands.iter().all(|c| !c.contains("limo1:")), "the offer is never in a command");
    assert!(matches!(connected.hand_offer(&[]), Err(CallError::Failed(_))));
    assert!(matches!(connected.hand_offer(&[b'A'; 1025]), Err(CallError::Failed(_))));
    // With no LiMo on the phone, the door is not there, and that is said.
    let mut bare = FakePhone::new();
    let without = open(&mut bare);
    let handed = without.hand_offer(offer).unwrap();
    assert!(!handed.ok());
    assert_eq!(handed.exit, Some(1));
}

#[test]
fn limo_is_started_and_an_open_screen_is_handed_the_start() {
    let mut phone = FakePhone::new();
    let connected = open(&mut phone);
    assert!(matches!(connected.start_limo().unwrap(), Started::Refused(_)), "not installed yet");
    let apk = apk_file("limo-child-release.apk", PACKAGE, Some(false));
    connected.install_limo(&apk).unwrap();
    assert_eq!(connected.start_limo().unwrap(), Started::Started);
    assert_eq!(connected.start_limo().unwrap(), Started::HandedToOpenScreen);
}

#[test]
fn reverse_carries_the_apps_connection_both_ways_and_the_app_closes_first() {
    let mut phone = FakePhone::new();
    let connected = open(&mut phone);
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    // An echo server, as the service will listen, that stays until the
    // phone closes.
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = vec![0u8; 64 * 1024];
        let mut total = 0;
        loop {
            let n = stream.read(&mut buffer).unwrap_or(0);
            if n == 0 {
                break;
            }
            stream.write_all(&buffer[..n]).unwrap();
            total += n;
        }
        total
    });
    assert!(phone.app_connects(port).is_err(), "nothing carries the port yet");
    connected.reverse(port).unwrap();
    let mut app = phone.app_connects(port).unwrap();
    let sent: Vec<u8> = (0..256 * 1024).map(|i| (i % 253) as u8).collect();
    app.write(&sent).unwrap();
    let back = app.read_exact(sent.len(), Duration::from_secs(10)).unwrap();
    assert_eq!(back, sent, "every byte back");
    app.close();
    assert_eq!(server.join().unwrap(), sent.len());
    connected.remove_reverse(port).unwrap();
    assert!(phone.app_connects(port).is_err(), "removed");
    let seen = phone.seen();
    assert!(seen.services.contains(&format!("reverse:forward:tcp:{port};tcp:{port}")));
    assert!(seen.services.contains(&format!("reverse:killforward:tcp:{port}")));
}

#[test]
fn switching_usb_debugging_off_ends_the_link_and_the_pc_sees_it_go() {
    let mut phone = FakePhone::new();
    let connected = open(&mut phone);
    let off = connected.debugging_off();
    assert!(matches!(off, DebuggingOff::Answered | DebuggingOff::LinkEnded), "{off:?}");
    let look = phone.look();
    assert!(look.interfaces.unwrap().is_empty());
    assert_eq!(look.windows.unwrap().len(), 1, "the phone's files only");
    assert_eq!(phone.open(&key(), quick(), &mut || {}).err(), Some(OpenProblem::NoPhone));
    assert!(phone.seen().commands.contains(&"settings put global adb_enabled 0".to_string()));
}

#[test]
fn an_unplugged_phone_is_said_as_lost_and_the_phone_hears_only_limos_commands() {
    let mut phone = FakePhone::new();
    let connected = open(&mut phone);
    let _ = connected.limo();
    phone.state().plugged = false;
    // The fake's cable goes with the next message it reads.
    let _ = connected.limo();
    std::thread::sleep(Duration::from_millis(100));
    assert!(matches!(connected.limo(), Err(CallError::Lost(_))));
    let seen = phone.seen();
    assert!(seen.refused.is_empty(), "nothing the phone did not know: {:?}", seen.refused);
    for command in &seen.commands {
        assert!(limo_cable::commands::Ask::every().iter().any(|a| &a.command() == command), "{command}");
    }
}

#[test]
fn the_signature_of_the_phones_token_is_the_one_adb_makes() {
    use rsa::pkcs1v15::Pkcs1v15Sign;
    let token = [7u8; 20];
    let key = key();
    let pem = std::fs::read_to_string(std::env::temp_dir().join(format!("limo-cable-phone-key-{}", std::process::id())).join("adbkey")).unwrap();
    let private = <rsa::RsaPrivateKey as rsa::pkcs8::DecodePrivateKey>::from_pkcs8_pem(&pem).unwrap();
    let ours = limo_cable::protocol::sign_token(&private, &token).unwrap();
    let adbs = private.sign(Pkcs1v15Sign::new::<sha1::Sha1>(), &token).unwrap();
    assert_eq!(ours, adbs, "PKCS#1 v1.5 with SHA-1's prefix, the token as the digest");
    assert!(!key.fingerprint().is_empty());
}
