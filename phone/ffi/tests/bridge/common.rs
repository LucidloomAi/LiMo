//! A fake phone driven only through this layer's API, the same calls
//! Kotlin makes, against a family played by the testkit: its own wall clock
//! and elapsed count, facts as the host would give them, and a state file
//! that holds only what the engine asked to keep.

use std::sync::Arc;

use limo_core::{CivilDate, RuleSet};
use limo_phone_ffi::testkit::{TestFamily, TestRules};
use limo_phone_ffi::{
    CallState, Front, FrontKind, GuardedPage, PhoneEngine, PhoneFacts, PhoneOrders, PhonePermissions,
};
use tempfile::TempDir;

/// India, the home zone.
pub const IST: i32 = 19_800;
/// A game: an ordinary app that counts.
pub const GAME: &str = "org.example.game";
/// A clock app the rules leave always allowed.
pub const CLOCK: &str = "org.example.clock";
/// An app the rules block.
pub const CASINO: &str = "org.example.casino";
/// The guardian's PIN.
pub const PIN: &str = "482915";

/// Wednesday 18 November 2026 at `hour:minute`, home time.
pub fn at(hour: u8, minute: u8) -> i64 {
    RuleSet::new(1, IST).calendar().at(CivilDate::new(2026, 11, 18).unwrap(), hour, minute, 0)
}

/// An app in front, with its label.
pub fn app(package: &str, label: &str) -> Front {
    Front { kind: FrontKind::App, package_name: Some(package.into()), label: Some(label.into()) }
}

/// A kind of screen in front that is not an app.
pub fn screen(kind: FrontKind) -> Front {
    Front { kind, package_name: None, label: None }
}

/// Every permission LiMo needs, given.
pub fn all_permissions() -> PhonePermissions {
    PhonePermissions {
        notifications: true,
        phone: true,
        usage_access: true,
        accessibility: true,
        battery: true,
        protection: true,
    }
}

/// The family, with Mira and her phone.
pub struct Phone {
    _dir: TempDir,
    pub family: Arc<TestFamily>,
    pub child: String,
    pub device_id: String,
    pub engine: Arc<PhoneEngine>,
    /// The state file: only what the engine asked to keep.
    pub kept: Vec<u8>,
    pub wall: i64,
    pub elapsed_ms: i64,
    pub boot: i32,
    pub screen_on: bool,
    pub front: Front,
    pub call: CallState,
    pub in_reach: bool,
    pub auto_time: bool,
    pub permissions: PhonePermissions,
    /// The page in front that could switch LiMo off, as the watch would say.
    pub guarded: Option<GuardedPage>,
    pub orders: Option<PhoneOrders>,
    /// Every note given, as the host's log would have it.
    pub notes: Vec<String>,
    pub reports: u32,
}

/// Mira's rules in every story: 30 minutes a day, the casino blocked, the
/// clock always allowed, and the whole day for away from home.
pub fn rules() -> TestRules {
    TestRules {
        minutes_a_day: 30,
        blocked_apps: vec![CASINO.into()],
        allowed_apps: vec![CLOCK.into()],
        away_share_percent: 100,
        utc_offset_secs: IST,
    }
}

impl Phone {
    /// A family with Mira, whose phone is paired from a made up welcome at
    /// `start` and set up (a guardian finished setup at once), kept at once;
    /// screen on, the game in front, out of reach.
    pub fn new(start: i64) -> Phone {
        let mut phone = Phone::setting_up(start);
        assert!(phone.engine.finish_setup(start).unwrap());
        assert!(phone.engine.should_save().unwrap());
        phone.kept = phone.engine.save().unwrap();
        phone
    }

    /// The same, with the phone just paired: setup not finished.
    pub fn setting_up(start: i64) -> Phone {
        let dir = tempfile::tempdir().unwrap();
        let family = TestFamily::new(dir.path().to_str().unwrap().to_string(), start).unwrap();
        family.set_pin(PIN.into(), start).unwrap();
        let child = family.add_child("Mira".into(), rules(), start).unwrap();
        let welcome = family.welcome(child.clone(), "Phone".into(), start).unwrap();
        assert_eq!(welcome.code_secret.len(), 32);
        let engine = PhoneEngine::paired(welcome.welcome, welcome.code_secret, start).unwrap();
        assert!(engine.should_save().unwrap());
        let kept = engine.save().unwrap();
        assert!(!engine.should_save().unwrap());
        Phone {
            _dir: dir,
            family,
            child,
            device_id: welcome.device_id,
            engine,
            kept,
            wall: start,
            elapsed_ms: 3_600_000,
            boot: 7,
            screen_on: true,
            front: app(GAME, "Game"),
            call: CallState::Idle,
            in_reach: false,
            auto_time: true,
            permissions: all_permissions(),
            guarded: None,
            orders: None,
            notes: Vec::new(),
            reports: 0,
        }
    }

    /// The facts as they stand.
    pub fn facts(&self) -> PhoneFacts {
        PhoneFacts {
            wall_secs: self.wall,
            elapsed_ms: self.elapsed_ms,
            boot_count: self.boot,
            utc_offset_secs: IST,
            hours_24: false,
            auto_time: self.auto_time,
            interactive: self.screen_on,
            keyguard_locked: false,
            call: self.call,
            front: self.front.clone(),
            in_reach: self.in_reach,
            prodded: false,
            permissions: self.permissions,
            guarded: self.guarded,
        }
    }

    /// One call, carried out as the host does: keep the state when told,
    /// log the notes, and carry a report to the family and its grant back.
    pub fn tick(&mut self) -> PhoneOrders {
        let mut orders = self.engine.tick(self.facts()).unwrap();
        self.carry_out(&orders);
        if orders.send_report {
            let report = self.engine.outgoing_report(self.wall).unwrap().expect("a report");
            self.kept = self.engine.save().unwrap();
            let grant = self.family.answer(report, self.wall).unwrap();
            self.reports += 1;
            self.engine.take_grant(grant, self.wall).unwrap();
            self.kept = self.engine.save().unwrap();
            orders = self.engine.tick(self.facts()).unwrap();
            self.carry_out(&orders);
        }
        self.orders = Some(orders.clone());
        orders
    }

    fn carry_out(&mut self, orders: &PhoneOrders) {
        if orders.save_state {
            self.kept = self.engine.save().unwrap();
        }
        self.notes.extend(orders.notes.iter().cloned());
    }

    /// Time passes by `secs` seconds, with a call every second, as the host
    /// makes them while the screen is in use.
    pub fn run(&mut self, secs: u32) -> PhoneOrders {
        let mut orders = self.tick();
        for _ in 0..secs {
            self.wall += 1;
            self.elapsed_ms += 1_000;
            orders = self.tick();
        }
        orders
    }

    /// The process is killed and started again: a new engine from the
    /// state file, with nothing kept on the way out.
    pub fn crash(&mut self) {
        self.engine = PhoneEngine::load(self.kept.clone());
        assert_eq!(self.engine.load_problem().unwrap(), None);
    }

    /// Counted use on the phone today.
    pub fn counted(&self) -> i64 {
        self.engine.view().unwrap().expect("a view").counted_secs
    }
}
