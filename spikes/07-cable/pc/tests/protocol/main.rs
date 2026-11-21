//! This program's own adb (`src/protocol.rs`), and the WinUSB route's cable
//! on it (`src/route_winusb.rs`), against a fake phone that speaks adb as
//! AOSP's adbd does (`fake_adbd.rs`). No phone, no USB.

mod fake_adbd;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use limo_cable_spike::cable::{Cable, CableError, Connector, OpenProblem, Via};
use limo_cable_spike::echo::Echo;
use limo_cable_spike::key::{self, Key, KeySource};
use limo_cable_spike::phone::Ask;
use limo_cable_spike::protocol::{AdbError, ConnectError, Connection, public_key_message, sign_token};
use limo_cable_spike::route_winusb::connect_over;
use limo_cable_spike::steps::{self, Exit};
use limo_cable_spike::ui::{Recorder, Ui};
use rsa::RsaPrivateKey;
use rsa::traits::PublicKeyParts;

use fake_adbd::{Control, FakeAdbd, Phone, pattern};

const WAIT: Duration = Duration::from_secs(10);

/// This test run's own key file, made once.
fn key_file() -> PathBuf {
    static MADE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    MADE.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("limo-spike07-protocol-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        key::load_or_make(&dir).unwrap().path
    })
    .clone()
}

fn private() -> RsaPrivateKey {
    key::private_key(&key_file()).unwrap()
}

fn public() -> Vec<u8> {
    public_key_message(&key::public_key_base64(&private()), key::KEY_NAME)
}

/// A connection to a fresh fake phone with these settings.
fn connect(phone: Phone, prompt_wait: Duration) -> (FakeAdbd, Result<Connection, ConnectError>, u32) {
    let (adbd, wire) = FakeAdbd::start(Arc::new(Mutex::new(phone)));
    let mut prompts = 0;
    let result = Connection::connect(Box::new(wire), &private(), &public(), prompt_wait, &mut || prompts += 1);
    (adbd, result, prompts)
}

#[test]
fn the_signature_is_the_one_adb_makes_of_its_token() {
    let key = private();
    let token = [7u8; 20];
    let ours = sign_token(&key, &token).unwrap();
    let adb_clients = key.sign(rsa::pkcs1v15::Pkcs1v15Sign::new::<sha1::Sha1>(), &token).unwrap();
    assert_eq!(ours, adb_clients, "PKCS#1 v1.5 with SHA-1's prefix, the token as the digest");
}

#[test]
fn a_first_connection_shows_the_key_and_the_next_one_needs_no_question() {
    let (adbd, result, prompts) = connect(Phone::default(), WAIT);
    let connection = result.expect("connected once allowed");
    assert_eq!(prompts, 1);
    assert!(connection.shell_v2);
    assert_eq!(connection.max_data(), 4096, "the phone's own limit, smaller than this side's");
    let seen = adbd.seen();
    assert_eq!(seen.prompts, 1);
    assert_eq!(seen.banner, "host::features=shell_v2,cmd");
    assert_eq!(seen.key_names, vec![key::KEY_NAME.to_string()]);
    let key = private();
    assert_eq!(seen.keys_shown[0].n(), key.n(), "the phone read this program's own key");
    assert_eq!(seen.keys_shown[0].e(), key.e());
    connection.close();
    // The phone now knows the key: no question.
    let known = adbd.phone.lock().unwrap().known.clone();
    let (again, result, prompts) = connect(Phone { known, ..Phone::default() }, WAIT);
    assert!(result.is_ok());
    assert_eq!(prompts, 0);
    assert_eq!(again.seen().prompts, 0);
}

#[test]
fn a_key_not_allowed_in_time_is_said() {
    let started = std::time::Instant::now();
    let (adbd, result, prompts) = connect(Phone { person_allows: false, ..Phone::default() }, Duration::from_secs(1));
    assert_eq!(result.err(), Some(ConnectError::NotAllowed(Duration::from_secs(1))));
    assert_eq!(prompts, 1);
    assert_eq!(adbd.seen().prompts, 1);
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn a_shell_command_gives_its_output_its_errors_and_its_exit_code_apart() {
    let (_adbd, result, _) = connect(Phone::default(), WAIT);
    let connection = result.unwrap();
    let model = connection.shell("getprop ro.product.model", None, WAIT).unwrap();
    assert_eq!(model.stdout, b"SM-G988B\n");
    assert_eq!(model.exit, Some(0));
    let failed = connection.shell("false", None, WAIT).unwrap();
    assert_eq!(failed.stderr, b"nope\n");
    assert_eq!(failed.exit, Some(1));
}

#[test]
fn standard_input_arrives_whole_and_is_closed_after() {
    let (adbd, result, _) = connect(Phone::default(), WAIT);
    let connection = result.unwrap();
    let secret = limo_cable_spike::secret::Sample::new();
    let out = connection.shell(&Ask::DoorWrite.command(), Some(secret.bytes()), WAIT).unwrap();
    assert_eq!(out.exit, Some(0));
    let seen = adbd.seen();
    assert_eq!(seen.stdin, secret.bytes());
    assert!(seen.stdin_closed);
    assert_eq!(seen.services.last().unwrap(), &format!("shell,v2,raw:{}", Ask::DoorWrite.command()));
}

#[test]
fn install_streams_the_apk_with_replace_and_never_more_than_the_phone_takes() {
    let (adbd, result, _) = connect(Phone::default(), WAIT);
    let connection = result.unwrap();
    let apk = std::env::temp_dir().join(format!("limo-spike07-protocol-{}.apk", std::process::id()));
    let bytes = pattern(70_001);
    std::fs::write(&apk, &bytes).unwrap();
    assert_eq!(connection.install(&apk, WAIT).unwrap(), "Success");
    let seen = adbd.seen();
    assert_eq!(seen.services, vec!["exec:cmd package install -r -S 70001".to_string()]);
    assert_eq!(seen.installed_bytes, bytes);
    assert!(seen.largest_write <= 4096);
    let _ = std::fs::remove_file(&apk);
}

#[test]
fn reverse_carries_the_apps_connection_to_this_pc_and_back_whole() {
    let (adbd, result, _) = connect(Phone::default(), WAIT);
    let connection = result.unwrap();
    let echo = Echo::bind(0).unwrap();
    let port = echo.port();
    let server = std::thread::spawn(move || echo.serve_one(WAIT, WAIT));
    connection.reverse(port, WAIT).unwrap();
    adbd.control.send(Control::AppConnects { port, bytes: 256 * 1024 }).unwrap();
    let echoed = server.join().unwrap().unwrap().expect("the app's connection came");
    assert_eq!(echoed.bytes, 256 * 1024);
    // The app closes once it has every byte back; give it a moment to be recorded.
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(adbd.seen().app_received, pattern(256 * 1024), "every byte back, none changed, the app closing first");
    connection.reverse_remove(port, WAIT).unwrap();
    assert_eq!(
        adbd.seen().services,
        vec![format!("reverse:forward:tcp:{port};tcp:{port}"), format!("reverse:killforward:tcp:{port}")]
    );
    // Once removed, a connection to that port is refused.
    adbd.control.send(Control::AppConnects { port, bytes: 10 }).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(adbd.seen().app_refused, 1);
}

#[test]
fn a_service_the_phone_does_not_have_is_refused_and_a_phone_that_goes_is_lost() {
    let (adbd, result, _) = connect(Phone::default(), WAIT);
    let connection = result.unwrap();
    assert!(matches!(connection.exec("cmd package list packages", WAIT), Err(AdbError::Refused(_))));
    adbd.control.send(Control::Unplug).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    let gone = connection.shell("getprop ro.product.model", None, WAIT);
    assert!(matches!(gone, Err(AdbError::Lost(_))), "{gone:?}");
}

/// The real WinUSB route's cable, over a fake phone that keeps its state
/// between steps, as the phone does, connecting with `key`.
struct OverFake {
    phone: Arc<Mutex<Phone>>,
    running: Mutex<Vec<FakeAdbd>>,
    key: Key,
}

impl Connector for OverFake {
    fn open(&self, via: Via, ui: &mut dyn Ui) -> Result<Box<dyn Cable>, OpenProblem> {
        assert_eq!(via, Via::Winusb);
        if !self.phone.lock().unwrap().adb_enabled {
            return Err(OpenProblem::NoPhone);
        }
        let (adbd, wire) = FakeAdbd::start(self.phone.clone());
        self.running.lock().unwrap().push(adbd);
        let private = self.key.load().map_err(OpenProblem::Other)?;
        connect_over(Box::new(wire), &private, self.key.source, WAIT, ui)
    }

    fn seen(&self, _via: Via) -> Result<Vec<String>, String> {
        Ok(if self.phone.lock().unwrap().adb_enabled { vec!["the fake phone".to_string()] } else { Vec::new() })
    }
}

#[test]
fn the_steps_run_on_the_winusb_route_against_a_phone_that_speaks_adb() {
    let own = key::load_or_make(key_file().parent().unwrap()).unwrap();
    let fake = OverFake { phone: Arc::new(Mutex::new(Phone::default())), running: Mutex::new(Vec::new()), key: own.clone() };
    let mut ui = Recorder::default();
    assert_eq!(steps::connect(&fake, Via::Winusb, Some(&own), &mut ui), Exit::Done, "{}", ui.text());
    let text = ui.text();
    assert!(text.contains("The phone did not know this program's key, so it has been shown it"), "the prompt is said");
    assert!(text.contains("Connected through WinUSB"));
    assert!(text.contains("this program waits 45 seconds"));
    assert!(text.contains("Connected: a SM-G988B, Android 13 (API 33); USB debugging setting: 1; the test app: not installed."));
    assert!(text.contains("(exit code 0)"), "the WinUSB route carries exit codes");
    // The next step needs no question.
    let mut ui = Recorder::default();
    assert_eq!(steps::uninstall(&fake, Via::Winusb, &mut ui), Exit::Done, "{}", ui.text());
    assert!(!ui.text().contains("has been shown it"));
    assert!(ui.text().contains("The phone asked nothing: it allowed this program's key before"));
    // And USB debugging off ends the link.
    let mut ui = Recorder { answers: vec!["OFF".to_string()], ..Recorder::default() };
    assert_eq!(steps::debugging_off(&fake, Via::Winusb, &mut ui), Exit::Done, "{}", ui.text());
    assert!(ui.text().contains("USB debugging looks off"));
    let mut ui = Recorder::default();
    assert_eq!(steps::start(&fake, Via::Winusb, &mut ui), Exit::Failed);
}

#[test]
fn the_adb_key_of_this_windows_account_skips_the_question_and_is_never_written_or_shown() {
    use rsa::pkcs8::{EncodePrivateKey, LineEnding};
    // A stand in for Google's adb's key, which the phone allowed long ago.
    let dir = std::env::temp_dir().join(format!("limo-spike07-google-key-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("adbkey");
    let google = RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).unwrap();
    let pem = google.to_pkcs8_pem(LineEnding::LF).unwrap().to_string();
    std::fs::write(&path, &pem).unwrap();
    let before = std::fs::metadata(&path).unwrap().modified().unwrap();
    let phone = Phone { known: vec![google.to_public_key()], ..Phone::default() };
    let key = key::google_adb_at(path.clone()).unwrap();
    let fake = OverFake { phone: Arc::new(Mutex::new(phone)), running: Mutex::new(Vec::new()), key: key.clone() };
    let mut ui = Recorder::default();
    assert_eq!(steps::connect(&fake, Via::Winusb, Some(&key), &mut ui), Exit::Done, "{}", ui.text());
    let text = ui.text();
    assert!(text.contains("connects with the adb key of this Windows account"));
    assert!(text.contains(&format!("Its fingerprint, as the phone shows it: {}", key.fingerprint)));
    assert!(text.contains("The question on the phone was skipped: the adb key of this Windows account was allowed on this phone before."));
    assert!(text.contains("For this proof only"));
    assert!(!text.contains("has been shown it"), "no question");
    assert_eq!(fake.phone.lock().unwrap().seen.prompts, 0);
    assert!(!text.contains(&dir.display().to_string()) && !text.contains("adbkey"), "its place is never said");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), pem, "read only");
    assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), before);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "never copied beside it");
    // A phone that does not know it asks, and the question is said.
    let fake = OverFake { phone: Arc::new(Mutex::new(Phone::default())), running: Mutex::new(Vec::new()), key: key.clone() };
    let mut ui = Recorder::default();
    assert_eq!(steps::connect(&fake, Via::Winusb, Some(&key), &mut ui), Exit::Done);
    assert!(ui.text().contains("The phone did not know the adb key of this Windows account, so it has been shown it"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_winusb_cable_says_a_lost_phone_as_lost() {
    let phone = Arc::new(Mutex::new(Phone::default()));
    let (adbd, wire) = FakeAdbd::start(phone);
    let cable = connect_over(Box::new(wire), &private(), KeySource::Own, WAIT, &mut Recorder::default()).unwrap();
    drop(adbd);
    std::thread::sleep(Duration::from_millis(200));
    assert!(matches!(cable.shell(&Ask::Model), Err(CableError::Lost(_))));
    let said = cable.close();
    assert!(said[0].contains("WinUSB's handle is closed"));
}
