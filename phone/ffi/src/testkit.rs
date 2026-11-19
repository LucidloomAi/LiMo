//! A family for tests on the PC: the master played over a real family file
//! in a folder the test gives (a temporary one), with the link crate's
//! `dev` master. Only with the `testkit` feature, which the library on a
//! phone never has: the build script refuses a phone library that holds it,
//! and `npm run inspect` looks for it in both built apps.
//!
//! It makes what only the master PC can make: a child with rules, a
//! welcome and a code secret for a phone, one time codes, the grant that
//! answers a report, a pairing offer, and the master's end of a link.

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use limo_core::{DailyBudget, DeviceKind, WeekBudgets};
use limo_link::dev::DevMaster;
use limo_link::{MasterLink, Now};

use crate::convert;

/// Why the testkit could not do what a test asked.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Error)]
#[uniffi(flat_error)]
pub enum TestkitError {
    /// It failed, in these words.
    Failed {
        /// What went wrong.
        message: String,
    },
}

impl std::fmt::Display for TestkitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TestkitError::Failed { message } => f.write_str(message),
        }
    }
}

impl std::error::Error for TestkitError {}

fn failed(what: &str, error: impl std::fmt::Display) -> TestkitError {
    TestkitError::Failed { message: format!("{what}: {error}") }
}

/// A child's rules, as a test sets them.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TestRules {
    /// The budget, the same every day, in minutes.
    pub minutes_a_day: i32,
    /// Apps that can never be used.
    pub blocked_apps: Vec<String>,
    /// Apps that never use up the budget and stay usable under the lock.
    pub allowed_apps: Vec<String>,
    /// How much of the day a phone may hold for away from home, 0 to 100.
    pub away_share_percent: i32,
    /// The home time zone, seconds from UTC.
    pub utc_offset_secs: i32,
}

/// What the master hands a phone at pairing, made without the link.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TestWelcome {
    /// The phone's id at the master.
    pub device_id: String,
    /// The core's welcome `Grant`, as JSON.
    pub welcome: Vec<u8>,
    /// The phone's 32 byte code secret.
    pub code_secret: Vec<u8>,
}

struct Family {
    master: DevMaster,
    link: Option<MasterLink>,
}

/// A family on the master PC, played over a family file in a folder.
#[derive(uniffi::Object)]
pub struct TestFamily {
    inner: Mutex<Family>,
}

impl std::fmt::Debug for TestFamily {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TestFamily")
    }
}

impl TestFamily {
    fn family(&self) -> Result<MutexGuard<'_, Family>, TestkitError> {
        self.inner.lock().map_err(|_| TestkitError::Failed { message: "an earlier call panicked".into() })
    }
}

#[uniffi::export]
impl TestFamily {
    /// Makes the family file `family.limo` in `folder`, with a family set up
    /// and a new master key pair.
    #[uniffi::constructor]
    pub fn new(folder: String, wall_secs: i64) -> Result<Arc<TestFamily>, TestkitError> {
        let master = DevMaster::open(Path::new(&folder), wall_secs).map_err(|e| failed("open the family file", e))?;
        Ok(Arc::new(TestFamily { inner: Mutex::new(Family { master, link: None }) }))
    }

    /// Adds a child with these rules; returns the child's id.
    pub fn add_child(&self, name: String, rules: TestRules, wall_secs: i64) -> Result<String, TestkitError> {
        let mut family = self.family()?;
        let minutes = u32::try_from(rules.minutes_a_day).map_err(|e| failed("the minutes", e))?;
        let share = u8::try_from(rules.away_share_percent).map_err(|e| failed("the away share", e))?;
        let child = family
            .master
            .add_child(&name, rules.utc_offset_secs, wall_secs)
            .map_err(|e| failed("add the child", e))?;
        let hub = family.master.hub();
        let mut set = hub.rules(&child).map_err(|e| failed("read the rules", e))?;
        set.budgets = WeekBudgets::every_day(DailyBudget::Minutes(minutes));
        set.blocked_apps = rules.blocked_apps;
        set.always_allowed_apps = rules.allowed_apps;
        set.away_share_percent = share;
        hub.replace_rules(&child, set, wall_secs).map_err(|e| failed("set the rules", e))?;
        Ok(child)
    }

    /// Sets the family's guardian PIN, which reaches each phone in a grant.
    pub fn set_pin(&self, pin: String, wall_secs: i64) -> Result<(), TestkitError> {
        self.family()?.master.hub().set_pin(&pin, wall_secs).map_err(|e| failed("set the PIN", e))
    }

    /// Pairs a phone with the child at the master, without the link: the
    /// welcome and the code secret a phone would be handed.
    pub fn welcome(&self, child_id: String, phone_name: String, wall_secs: i64) -> Result<TestWelcome, TestkitError> {
        let mut family = self.family()?;
        let pairing = family
            .master
            .hub()
            .pair_device(&child_id, &phone_name, DeviceKind::Portable, "android", wall_secs)
            .map_err(|e| failed("pair the phone", e))?;
        let welcome = convert::to_json(&pairing.welcome).ok_or_else(|| failed("write the welcome", "JSON"))?;
        Ok(TestWelcome { device_id: pairing.device_id.clone(), welcome, code_secret: pairing.code_secret.clone() })
    }

    /// A one time code for a phone, of 15, 30 or 60 minutes: its 8 digits.
    pub fn make_code(&self, device_id: String, minutes: i32, wall_secs: i64) -> Result<String, TestkitError> {
        let minutes = u32::try_from(minutes).map_err(|e| failed("the minutes", e))?;
        let code =
            self.family()?.master.hub().make_code(&device_id, minutes, wall_secs).map_err(|e| failed("make a code", e))?;
        Ok(code.digits.clone())
    }

    /// The master's answer to a report from `PhoneEngine.outgoingReport`:
    /// the grant, as JSON, for `PhoneEngine.takeGrant`.
    pub fn answer(&self, report: Vec<u8>, wall_secs: i64) -> Result<Vec<u8>, TestkitError> {
        let report = convert::report_from(&report).ok_or_else(|| failed("read the report", "not a report"))?;
        let grant =
            self.family()?.master.hub().handle_report(&report, wall_secs).map_err(|e| failed("answer the report", e))?;
        convert::to_json(&grant).ok_or_else(|| failed("write the grant", "JSON"))
    }

    /// A pairing offer for the child, in its `limo1:` text, as UTF-8 bytes.
    pub fn make_offer(&self, child_id: String, wall_secs: i64) -> Result<Vec<u8>, TestkitError> {
        let addresses = vec![std::net::Ipv4Addr::new(192, 168, 1, 20)];
        let offer =
            self.family()?.master.make_offer(&child_id, addresses, wall_secs).map_err(|e| failed("make an offer", e))?;
        Ok(offer.to_text().as_bytes().to_vec())
    }

    /// Removes a phone at the master: its next session is refused.
    pub fn remove_phone(&self, device_id: String, wall_secs: i64) -> Result<(), TestkitError> {
        self.family()?.master.remove_phone(&device_id, wall_secs).map_err(|e| failed("remove the phone", e))
    }

    /// A new connection from a phone: the master's end of a link, in place
    /// of any before it.
    pub fn connect(&self, now_ms: i64, wall_secs: i64) -> Result<(), TestkitError> {
        let ms = u64::try_from(now_ms).map_err(|e| failed("the elapsed count", e))?;
        let mut family = self.family()?;
        let link = family.master.accept(Now { ms, unix: wall_secs });
        family.link = Some(link);
        Ok(())
    }

    /// Feeds bytes from the phone into the master's end, which acts on what
    /// came as the Windows service will (pairs, answers reports); returns the
    /// bytes it would write back.
    pub fn exchange(&self, bytes: Vec<u8>, now_ms: i64, wall_secs: i64) -> Result<Vec<u8>, TestkitError> {
        let ms = u64::try_from(now_ms).map_err(|e| failed("the elapsed count", e))?;
        let now = Now { ms, unix: wall_secs };
        let mut family = self.family()?;
        let Family { master, link } = &mut *family;
        let link = link.as_mut().ok_or_else(|| failed("exchange", "no connection"))?;
        master.handle(link, &bytes, now).map_err(|e| failed("the master's end", e))?;
        Ok(link.take_outgoing())
    }
}
