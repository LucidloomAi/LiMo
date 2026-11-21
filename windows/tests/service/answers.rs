//! The door's answers as files, for the screens' own tests: a small family
//! played through the real door, over a real named pipe, with the fake PC's
//! clock, and each message written as it leaves the door, one JSON file per
//! request and case, into `phone/app/tests/console-answers/`. The family is
//! made by the first start's own requests; then a child with rules, linked
//! accounts, a time request waiting, the PC's device and a paired phone, a
//! wrong password and its wait, and a refusal for a child's account.
//!
//! What would change from run to run is written as a fixed sample: the
//! family file's random ids, the link's port, the offer and its ids, a one
//! time code, and this test's own Windows account. No password, PIN, key or
//! code is written, and the test checks so. A file is written only when it
//! changed, and one this run did not write is removed, so a run with no
//! change in the door changes no file.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use limo_core::{BlockedWindow, DailyBudget, PhoneNote, PhonePermission, RequestReply, RuleSet, WeekBudgets, Weekday};
use limo_hub::{AccountLink, AgeBand, UnlinkedAccounts};
use limo_link::Offer;
use limo_windows::console::{CONSOLE_PROTOCOL_VERSION, FromConsole, Request, ToConsole};
use limo_windows::played_phone::PlayedPhone;
use limo_windows::protocol::{FromHelper, Secret};
use limo_windows::{DOOR_SDDL, DoorClient, RealDoor, this_account};
use serde_json::{Value, json};

use crate::common::*;
use crate::pipe::{connect, pipe_name};

/// This test's own Windows account, as the files show it.
const GUARDIAN: &str = "S-1-5-21-1000";
const ASHA: &str = "S-1-5-21-1001";
const GUEST: &str = "S-1-5-21-1009";
const NEW_PASSWORD: &str = "a new password for the family";
const NOTES: &str = r"C:\Program Files\Notes\notes.exe";
/// The link's port, as the files show it.
const SAMPLE_PORT: u16 = 47810;
/// The real time between two requests: well under the door's limit.
const PACE: Duration = Duration::from_millis(60);
/// What a secret the console sent is written as.
const NOT_WRITTEN: &str = "(a secret: not written here)";

/// The folder the screens' tests read.
fn answers_folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("phone").join("app").join("tests").join("console-answers")
}

/// The answers of one run, made stable.
struct Answers {
    /// This test's own account, never written.
    me: String,
    /// The link's real port.
    port: u16,
    next_id: u32,
    /// File name to text.
    files: BTreeMap<String, String>,
    /// A random value seen to its sample.
    samples: BTreeMap<String, String>,
    /// How many samples of each kind were made.
    made: BTreeMap<String, u64>,
    /// Text that must never be in a file.
    never: Vec<String>,
}

impl Answers {
    fn new(me: &str, port: u16) -> Answers {
        Answers {
            me: me.to_string(),
            port,
            next_id: 0,
            files: BTreeMap::new(),
            samples: BTreeMap::new(),
            made: BTreeMap::new(),
            never: vec![me.to_string(), PASSWORD.to_string(), NEW_PASSWORD.to_string(), PIN.to_string()],
        }
    }

    /// A sample of `kind` for a random value: the same one every time the
    /// value is seen, and the same in every run, as values are met in the
    /// same order.
    fn sample_of(&mut self, real: &str, kind: &str, make: impl FnOnce(u64) -> String) -> String {
        if let Some(sample) = self.samples.get(real) {
            return sample.clone();
        }
        let n = self.made.entry(kind.to_string()).or_insert(0);
        *n += 1;
        let sample = make(*n);
        self.samples.insert(real.to_string(), sample.clone());
        sample
    }

    /// A string with this test's account and the family file's random ids
    /// as samples.
    fn stable_text(&mut self, text: &str) -> String {
        let mut text = text.replace(&self.me, GUARDIAN);
        for prefix in ["child-", "device-", "hub/"] {
            let mut out = String::new();
            let mut rest = text.as_str();
            while let Some(at) = rest.find(prefix) {
                let (before, from) = rest.split_at(at);
                out.push_str(before);
                let tail = &from[prefix.len()..];
                let hex = tail.len() >= 16 && tail[..16].chars().all(|c| c.is_ascii_hexdigit());
                if hex {
                    let real = &from[..prefix.len() + 16];
                    let sample = self.sample_of(real, prefix, |n| format!("{prefix}{n:016x}"));
                    out.push_str(&sample);
                    rest = &from[prefix.len() + 16..];
                } else {
                    out.push_str(prefix);
                    rest = tail;
                }
            }
            out.push_str(rest);
            text = out;
        }
        text
    }

    /// A value with every run's own randomness as samples; `key` is the
    /// field it is in.
    fn stable(&mut self, value: &Value, key: Option<&str>) -> Value {
        match value {
            Value::Object(map) => Value::Object(map.iter().map(|(k, v)| (k.clone(), self.stable(v, Some(k)))).collect()),
            Value::Array(items) => Value::Array(items.iter().map(|v| self.stable(v, key)).collect()),
            Value::String(text) => match key {
                Some("offer") => {
                    self.never.push(text.clone());
                    Value::String("limo1:(an offer for a phone: it holds a secret, so it is not written here)".into())
                }
                Some("offer_id" | "master_id") => {
                    let len = text.len();
                    Value::String(self.sample_of(text, key.unwrap_or_default(), |n| format!("{n:0len$x}")))
                }
                Some("shown") => {
                    // A one time code: its shape, never its digits.
                    self.never.push(text.chars().filter(char::is_ascii_alphanumeric).collect());
                    let mut n = 0;
                    Value::String(
                        text.chars()
                            .map(|c| {
                                if c.is_ascii_alphanumeric() {
                                    n += 1;
                                    char::from_digit(n % 10, 10).unwrap_or('0')
                                } else {
                                    c
                                }
                            })
                            .collect(),
                    )
                }
                _ => Value::String(self.stable_text(text)),
            },
            Value::Number(number) if key == Some("port") && number.as_u64() == Some(u64::from(self.port)) => json!(SAMPLE_PORT),
            other => other.clone(),
        }
    }

    /// The request as the console sent it, with no secret in it.
    fn asked(&mut self, request: &Request) -> Value {
        let mut asked = serde_json::to_value(request).unwrap();
        if let Some(args) = asked.get_mut("args").and_then(Value::as_object_mut) {
            for key in ["password", "pin", "old", "new"] {
                if let Some(secret) = args.get_mut(key) {
                    *secret = json!(NOT_WRITTEN);
                }
            }
        }
        self.stable(&asked, None)
    }

    /// Keeps one file.
    fn keep(&mut self, file: &str, request: &str, case: &str, asked: Option<Value>, message: &ToConsole) {
        assert!(!self.files.contains_key(file), "{file} twice");
        let answer = serde_json::to_value(message).unwrap();
        let answer = self.stable(&answer, None);
        let pretty = |v: &Value| serde_json::to_string_pretty(v).unwrap().replace('\n', "\n  ");
        let mut text = String::from("{\n");
        text.push_str(&format!("  \"request\": {},\n", json!(request)));
        text.push_str(&format!("  \"case\": {},\n", json!(limo_windows::text::typographic(case))));
        if let Some(asked) = &asked {
            text.push_str(&format!("  \"asked\": {},\n", pretty(asked)));
        }
        text.push_str(&format!("  \"answer\": {}\n}}\n", pretty(&answer)));
        self.files.insert(file.to_string(), text);
    }

    /// Sends a request a second after the last (the door takes one secret
    /// a second), and keeps its answer; the answer as it came, for the
    /// story to go on with.
    fn ask(&mut self, pc: &mut Pc, client: &mut DoorClient, file: &str, case: &str, request: Request) -> Value {
        pc.run(1);
        // The door's thread counts messages in real seconds, as a screen
        // sends them: a few a second at most.
        std::thread::sleep(PACE);
        self.next_id += 1;
        let id = self.next_id;
        let asked = self.asked(&request);
        let name = request.name();
        client.send(&FromConsole::Request { id, request }).unwrap();
        let message = next_message(pc, client);
        assert!(matches!(&message, ToConsole::Answer { id: got, .. } if *got == id), "{file}: {message:?}");
        self.keep(file, name, case, Some(asked), &message);
        serde_json::to_value(&message).unwrap()["reply"].clone()
    }

    /// As [`Answers::ask`], for a request that must be done: its value.
    fn done(&mut self, pc: &mut Pc, client: &mut DoorClient, file: &str, case: &str, request: Request) -> Value {
        let reply = self.ask(pc, client, file, case, request);
        assert_eq!(reply["outcome"], "done", "{file}: {reply}");
        reply["value"].clone()
    }

    /// As [`Answers::ask`], for a request that must meet a problem of
    /// `kind`.
    fn problem(&mut self, pc: &mut Pc, client: &mut DoorClient, file: &str, case: &str, kind: &str, request: Request) {
        let reply = self.ask(pc, client, file, case, request);
        assert_eq!(reply["problem"]["kind"], kind, "{file}: {reply}");
    }

    /// Says hello for a new client, and keeps what the door said to it.
    fn hello(&mut self, pc: &mut Pc, client: &mut DoorClient, file: &str, case: &str) -> ToConsole {
        client.send(&FromConsole::Hello { protocol: CONSOLE_PROTOCOL_VERSION }).unwrap();
        let message = next_message(pc, client);
        let asked = json!({ "say": "hello", "protocol": CONSOLE_PROTOCOL_VERSION });
        self.keep(file, "hello", case, Some(asked), &message);
        message
    }

    /// Every request in the door's own list has at least one file: the
    /// names are read from the list itself, so a request added to the door
    /// and not played here fails the test.
    fn every_request_has_a_file(&self) {
        let source = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src").join("console.rs")).unwrap();
        let list = source.split("pub fn name(&self)").nth(1).and_then(|rest| rest.split("\n    }\n").next()).unwrap();
        let names: Vec<&str> = list.lines().filter_map(|line| line.split("=> \"").nth(1)).filter_map(|rest| rest.split('"').next()).collect();
        assert!(names.len() > 30, "{names:?}");
        let played: Vec<&str> =
            self.files.values().filter_map(|text| text.split("\"request\": \"").nth(1)?.split('"').next()).collect();
        for name in names {
            assert!(played.contains(&name), "no file for {name}");
        }
    }

    /// Writes the files where they changed, removes those this run did not
    /// write, after checking that none holds a secret or anything of this
    /// PC.
    fn write(&self) {
        let pc_name = std::env::var("COMPUTERNAME").unwrap_or_default().to_lowercase();
        let user = std::env::var("USERNAME").unwrap_or_default().to_lowercase();
        for (file, text) in &self.files {
            for never in self.never.iter().filter(|n| n.len() >= 4) {
                assert!(!text.contains(never.as_str()), "{file} holds a secret or this PC's account");
            }
            let lower = text.to_lowercase();
            for mine in [&pc_name, &user].into_iter().filter(|n| n.len() >= 4) {
                assert!(!lower.contains(mine.as_str()), "{file} names this PC or its user");
            }
            assert!(!lower.contains(r"\\users\\"), "{file} holds a path of this PC");
        }
        let folder = answers_folder();
        // The screens' tests live beside this crate; without them there is
        // nowhere to write.
        if !folder.parent().is_some_and(|tests| tests.is_dir()) {
            return;
        }
        std::fs::create_dir_all(&folder).unwrap();
        for (file, text) in &self.files {
            let path = folder.join(format!("{file}.json"));
            // A checkout may have turned the line endings; that is no change.
            let there = std::fs::read_to_string(&path).ok().map(|t| t.replace("\r\n", "\n"));
            if there.as_deref() != Some(text.as_str()) {
                std::fs::write(&path, text).unwrap();
            }
        }
        for entry in std::fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            if path.extension().is_some_and(|e| e == "json") && !self.files.contains_key(&stem) {
                std::fs::remove_file(&path).unwrap();
            }
        }
    }
}

/// The next message for the client: the door answers with no step of the
/// service, so the fake clock stands still while the pipe carries it, and
/// every time in an answer is the same in every run.
fn next_message(pc: &mut Pc, client: &mut DoorClient) -> ToConsole {
    let give_up = Instant::now() + Duration::from_secs(10);
    loop {
        pc.service.door();
        pc.service.link();
        if let Some(message) = client.read(5).unwrap() {
            return message;
        }
        assert!(Instant::now() < give_up, "nothing came");
    }
}

fn secret(text: &str) -> Secret {
    Secret::new(text.into())
}

/// Asha's rules: an hour a day, bedtime on school nights, a game blocked.
fn ashas_rules(rules: &Value) -> RuleSet {
    let mut rules: RuleSet = serde_json::from_value(rules.clone()).unwrap();
    rules.budgets = WeekBudgets::every_day(DailyBudget::Minutes(60));
    rules.blocked_hours = vec![BlockedWindow {
        days: vec![Weekday::Sunday, Weekday::Monday, Weekday::Tuesday, Weekday::Wednesday, Weekday::Thursday],
        start_minute: 21 * 60,
        end_minute: 7 * 60,
    }];
    rules.blocked_apps = vec!["game.exe".into()];
    rules
}

#[test]
fn a_small_family_through_the_real_door_writes_each_answer_the_screens_read() {
    let me = this_account().unwrap();
    let mut pc = Pc::empty(at(16, 0));
    let name = pipe_name();
    pc.os().real_door = Some(RealDoor::open_with(&name, || {}, Duration::from_secs(30), &format!("{DOOR_SDDL}(A;;GA;;;{me})")));
    pc.os().accounts = vec![account(&me, "guardian", true), account(ASHA, "asha", false), account(GUEST, "guest", false)];
    pc.login(1, &me, "guardian");
    pc.os().sessions[0].is_administrator = true;
    let address = pc.with_link();
    pc.run(1);
    let mut a = Answers::new(&me, address.port());
    let mut client = connect(&name);
    let c = &mut client;
    let p = &mut pc;

    // The first start, from a family file with no family.
    a.hello(p, c, "hello", "the door says hello back, with its protocol and LiMo's version");
    a.done(p, c, "family-state-no-family", "LiMo just installed: no family yet", Request::FamilyState);
    a.done(p, c, "about", "what LiMo for Windows is", Request::About);
    a.problem(p, c, "sign-in-no-family", "a sign in before there is a family", "no_family", Request::SignIn { password: secret(PASSWORD) });
    a.done(p, c, "create-family", "the family made, and this window signed in", Request::CreateFamily { password: secret(PASSWORD) });
    a.done(p, c, "family-state-first-start", "a family with no child yet, signed in", Request::FamilyState);
    a.done(p, c, "set-pin", "the PIN set", Request::SetPin { pin: secret(PIN) });
    let asha = a.done(p, c, "add-child", "Asha added", Request::AddChild { name: "Asha".into(), age_band: AgeBand::TenToTwelve });
    let asha = asha["child_id"].as_str().unwrap().to_string();
    a.problem(p, c, "add-child-invalid", "a child with no name", "invalid", Request::AddChild { name: "  ".into(), age_band: AgeBand::SixToNine });
    a.done(p, c, "accounts-first-start", "the accounts of the PC, none linked yet", Request::Accounts);
    a.done(p, c, "link-account-not-limited", "this window's own account, not limited", Request::LinkAccount {
        account: me.clone(),
        link: AccountLink::NotLimited,
    });
    a.problem(p, c, "link-account-this-window", "this window's own account cannot be linked to a child", "refused", Request::LinkAccount {
        account: me.clone(),
        link: AccountLink::Child { child_id: asha.clone() },
    });
    a.done(p, c, "link-account-child", "Asha's account linked to Asha", Request::LinkAccount {
        account: ASHA.into(),
        link: AccountLink::Child { child_id: asha.clone() },
    });
    a.problem(p, c, "link-account-not-found", "an account that is not on this PC", "not_found", Request::LinkAccount {
        account: "S-1-5-21-1999".into(),
        link: AccountLink::NotLimited,
    });
    a.done(p, c, "link-account-guest", "the guest account, not limited", Request::LinkAccount { account: GUEST.into(), link: AccountLink::NotLimited });
    let rules = a.done(p, c, "rules-new-child", "a new child's rules, as LiMo starts them", Request::Rules { child: asha.clone() });
    let rules = ashas_rules(&rules);
    let mut wrong = rules.clone();
    wrong.blocked_hours[0].start_minute = 2_000;
    wrong.budgets = WeekBudgets::every_day(DailyBudget::Minutes(2_000));
    a.problem(p, c, "replace-rules-problems", "rules with a bedtime and a budget that cannot be", "rules", Request::ReplaceRules {
        child: asha.clone(),
        rules: wrong,
    });
    a.done(p, c, "replace-rules", "an hour a day, bedtime on school nights, a game blocked", Request::ReplaceRules {
        child: asha.clone(),
        rules,
    });
    a.done(p, c, "rules", "Asha's rules", Request::Rules { child: asha.clone() });
    a.done(p, c, "set-idle-minutes", "idle after 10 minutes", Request::SetIdleMinutes { minutes: 10 });
    a.done(p, c, "set-unlinked-accounts", "accounts linked to nobody are free", Request::SetUnlinkedAccounts { unlinked: UnlinkedAccounts::Free });
    a.done(p, c, "finish-first-start", "the first start finished: accounts linked to nobody are locked", Request::FinishFirstStart);
    a.done(p, c, "pc-settings", "the PC's settings", Request::PcSettings);
    a.done(p, c, "family-state", "the family set up, signed in", Request::FamilyState);
    a.done(p, c, "accounts", "the accounts, linked", Request::Accounts);

    // Asha uses the PC for five minutes, and asks for more time.
    p.login(7, ASHA, "asha");
    p.run(1);
    p.greet(7);
    p.run(2);
    let notes = p.start(7, NOTES);
    for _ in 0..300 {
        p.report(7, 0, Some(notes), false);
        p.run(1);
    }
    p.say(7, FromHelper::AskForTime { ask: 1, minutes: 20, reason: "For the school project".into() });
    p.run(1);

    a.done(p, c, "family-today", "every child's day: Asha has used five minutes", Request::FamilyToday);
    let today = a.done(p, c, "child-today", "Asha's day", Request::ChildToday { child: asha.clone() });
    let waiting = a.done(p, c, "waiting-requests", "Asha asks for 20 minutes", Request::WaitingRequests);
    let request = waiting[0]["request"]["request"]["id"].as_str().unwrap().to_string();
    a.done(p, c, "answer-request", "15 of them granted", Request::AnswerRequest {
        child: asha.clone(),
        request,
        reply: RequestReply::Grant { minutes: 15 },
    });
    a.done(p, c, "waiting-requests-none", "nothing waiting", Request::WaitingRequests);
    a.done(p, c, "give-extra", "10 minutes more for today", Request::GiveExtra { child: asha.clone(), minutes: 10, tap_id: "tap-1".into() });
    a.done(p, c, "lock-now", "Asha locked until a guardian lifts it", Request::LockNow { child: asha.clone(), until: None });
    a.done(p, c, "lift", "the lock lifted", Request::Lift { child: asha.clone() });
    a.done(p, c, "unlock-until", "unlocked until 18:00", Request::UnlockUntil { child: asha.clone(), until: at(18, 0) });
    a.done(p, c, "lift-unlock", "the unlock lifted", Request::Lift { child: asha.clone() });
    let day = today["day"].as_i64().unwrap_or_default();
    a.done(p, c, "usage", "Asha's usage over the last week", Request::Usage { child: asha.clone(), from_day: day - 6, to_day: day });
    let devices = a.done(p, c, "devices", "Asha's devices: the PC", Request::Devices { child: asha.clone() });
    let pc_device = devices[0]["device_id"].as_str().unwrap().to_string();
    a.done(p, c, "make-code", "a one time code for the PC, worth 15 minutes", Request::MakeCode { device: pc_device.clone(), minutes: 15 });

    // A phone is paired, and its session comes through the cable.
    let made = a.done(p, c, "make-offer", "an offer for a phone of Asha's", Request::MakeOffer { child: asha.clone() });
    let offer = Offer::from_text(made["offer"].as_str().unwrap()).unwrap();
    let mut phone = PlayedPhone::pair(address, &offer, "Asha’s phone", &mut || p.answer_link()).unwrap();
    let mut session = phone.open(address, &mut || p.answer_link()).unwrap();
    session.exchange(&mut phone, false, &mut || p.answer_link()).unwrap();
    let phone_device = phone.paired.device_id.clone();
    a.done(p, c, "devices-with-phone", "Asha's devices: the PC and a phone", Request::Devices { child: asha.clone() });
    a.done(p, c, "device", "the phone", Request::Device { device: phone_device.clone() });
    a.done(p, c, "device-link", "the phone's session, through the cable", Request::DeviceLink { device: phone_device.clone() });

    a.done(p, c, "rename-device", "the phone renamed", Request::RenameDevice { device: phone_device.clone(), name: "Asha’s tablet".into() });
    a.done(p, c, "device-pc", "the PC's own device", Request::Device { device: pc_device });
    a.done(p, c, "rename-child", "Asha renamed, to the same name", Request::RenameChild { child: asha.clone(), name: "Asha".into() });
    a.done(p, c, "set-age-band", "Asha is 13 now", Request::SetAgeBand { child: asha.clone(), age_band: AgeBand::ThirteenToSeventeen });
    a.problem(p, c, "child-today-not-found", "a child that is not in the family", "not_found", Request::ChildToday {
        child: "child-ffffffffffffffff".into(),
    });
    a.problem(p, c, "change-password-wrong", "the old password not right", "wrong_password", Request::ChangePassword {
        old: secret(NEW_PASSWORD),
        new: secret(PASSWORD),
    });
    a.done(p, c, "change-password", "the password changed", Request::ChangePassword { old: secret(PASSWORD), new: secret(NEW_PASSWORD) });
    a.done(p, c, "events", "what happened, newest first", Request::Events { limit: 50, before: None });

    // Signed out, a wrong password and its wait.
    a.done(p, c, "sign-out", "signed out", Request::SignOut);
    a.problem(p, c, "family-today-signed-out", "a request that needs a sign in", "signed_out", Request::FamilyToday);
    a.done(p, c, "family-state-signed-out", "the family, signed out", Request::FamilyState);
    let wrong = || Request::SignIn { password: secret("not the password") };
    a.problem(p, c, "sign-in-wrong-password", "the first wrong password", "wrong_password", wrong());
    a.problem(p, c, "sign-in-wrong-password-second", "the second", "wrong_password", wrong());
    a.problem(p, c, "sign-in-wrong-password-third", "the third", "wrong_password", wrong());
    a.problem(p, c, "sign-in-wrong-password-last-free", "the fourth: one more try before a wait", "wrong_password", wrong());
    a.problem(p, c, "sign-in-wrong-password-wait", "the fifth: a wait of a minute begins", "wrong_password", wrong());
    a.problem(p, c, "sign-in-must-wait", "the right password, during the wait", "must_wait", Request::SignIn { password: secret(NEW_PASSWORD) });
    p.run(61);
    a.done(p, c, "sign-in", "the right password, after the wait", Request::SignIn { password: secret(NEW_PASSWORD) });

    // A request this LiMo does not know.
    p.run(1);
    std::thread::sleep(PACE);
    a.next_id += 1;
    let id = a.next_id;
    let unknown = format!(r#"{{"say":"request","id":{id},"request":{{"name":"take_over_the_pc","args":{{}}}}}}"#);
    let mut frame = CONSOLE_PROTOCOL_VERSION.to_le_bytes().to_vec();
    frame.extend_from_slice(&(unknown.len() as u32).to_le_bytes());
    frame.extend_from_slice(unknown.as_bytes());
    c.write_raw(&frame).unwrap();
    let message = next_message(p, c);
    a.keep("unknown-request", "take_over_the_pc", "a request this LiMo does not know", Some(json!({ "name": "take_over_the_pc", "args": {} })), &message);

    // What the phone needs a guardian for, as its notes come in.
    a.done(p, c, "device-phone-setup-not-finished", "a phone paired, with setup not finished on it", Request::Device {
        device: phone_device.clone(),
    });
    let at = p.now();
    for permission in PhonePermission::ALL {
        phone.note(PhoneNote::PermissionOn { permission, at });
    }
    phone.note(PhoneNote::SetupFinished { at: at + 60 });
    phone.note(PhoneNote::Protection { level: limo_core::ProtectionLevel::Base, at: at + 60 });
    phone.note(PhoneNote::PermissionOff { permission: PhonePermission::Accessibility, at: at + 120 });
    session.exchange(&mut phone, false, &mut || p.answer_link()).unwrap();
    a.done(p, c, "device-phone-step-off", "a phone with one step switched off: accessibility", Request::Device {
        device: phone_device.clone(),
    });
    a.done(p, c, "family-today-phone-step-off", "every child's day, with accessibility off on Asha's phone", Request::FamilyToday);
    phone.note(PhoneNote::PermissionOn { permission: PhonePermission::Accessibility, at: at + 180 });
    phone.note(PhoneNote::UsbDebugging { on: true, at: at + 240 });
    session.exchange(&mut phone, false, &mut || p.answer_link()).unwrap();
    a.done(p, c, "device-phone-usb-debugging-on", "a phone with USB debugging left on", Request::Device { device: phone_device.clone() });
    phone.note(PhoneNote::UsbDebugging { on: false, at: at + 300 });
    session.exchange(&mut phone, false, &mut || p.answer_link()).unwrap();
    a.done(p, c, "device-phone-all-well", "a phone set up, every step on, USB debugging off: nothing needed", Request::Device {
        device: phone_device.clone(),
    });

    a.done(p, c, "remove-device", "the phone removed", Request::RemoveDevice { device: phone_device, password: secret(NEW_PASSWORD) });
    a.done(p, c, "unlink-account", "the guest account's link taken away", Request::UnlinkAccount { account: GUEST.into() });
    a.done(p, c, "allow-removal", "LiMo may be uninstalled for the next few minutes", Request::AllowRemoval { password: secret(NEW_PASSWORD) });
    session.bye(&mut || p.answer_link());

    // A console from a child's account is refused at the door. The family's
    // file links this test's own account to Asha for it, and back.
    p.hub().link_account(&me, "guardian", AccountLink::Child { child_id: asha.clone() }).unwrap();
    p.run(1);
    let mut refused = connect(&name);
    a.hello(p, &mut refused, "refused-child-account", "a console opened from Asha's account");
    drop(refused);
    p.hub().link_account(&me, "guardian", AccountLink::NotLimited).unwrap();

    // The family removed, from a console of its own.
    let mut last = connect(&name);
    let l = &mut last;
    a.hello(p, l, "hello-again", "the door's hello to the next console");
    a.done(p, l, "sign-in-again", "signed in", Request::SignIn { password: secret(NEW_PASSWORD) });
    a.done(p, l, "remove-child", "Asha removed", Request::RemoveChild { child: asha, password: secret(NEW_PASSWORD) });
    a.done(p, l, "delete-family", "the family deleted", Request::DeleteFamily { password: secret(NEW_PASSWORD) });
    a.done(p, l, "family-state-deleted", "no family any more, and nobody signed in", Request::FamilyState);

    a.every_request_has_a_file();
    a.write();
}
