//! What the stories share: a clock, a family with the `dev` master over a
//! real family file, and the wire between a phone and the master, which
//! carries bytes in memory and can lose, change, repeat or reorder them.

use std::net::Ipv4Addr;

use limo_core::{DeviceState, Grant, GrantOutcome};
use limo_link::dev::{DevError, DevMaster};
use limo_link::{
    Form, LinkError, MasterEvent, MasterLink, Now, Offer, PairRequest, PairedMaster, PhoneEvent, PhonePairing,
    PhoneSession, Welcomed,
};
use tempfile::TempDir;

/// Thursday 19 November 2026, 07:00 in India.
pub const START: i64 = 1_795_051_800;
/// India's offset from UTC.
pub const IST: i32 = 19_800;
pub const MIN: i64 = 60;
pub const HOUR: i64 = 3_600;

/// Both clocks a link needs, moved together.
#[derive(Debug, Clone, Copy)]
pub struct Clock {
    pub ms: u64,
    pub unix: i64,
}

impl Clock {
    pub fn new() -> Clock {
        Clock { ms: 1_000_000, unix: START }
    }

    pub fn now(&self) -> Now {
        Now { ms: self.ms, unix: self.unix }
    }

    pub fn advance_ms(&mut self, ms: u64) {
        self.ms += ms;
        self.unix += i64::try_from(ms / 1_000).unwrap();
    }

    pub fn advance_secs(&mut self, secs: i64) {
        self.advance_ms(u64::try_from(secs).unwrap() * 1_000);
    }
}

pub fn request(name: &str) -> PairRequest {
    PairRequest { name: name.into(), form: Form::Child, platform: "android".into(), android_version: Some(33) }
}

/// A family file in a temporary folder, the `dev` master over it, and one
/// child.
pub struct Family {
    _dir: TempDir,
    pub master: DevMaster,
    pub child: String,
    pub clock: Clock,
}

impl Family {
    pub fn new() -> Family {
        let dir = tempfile::tempdir().unwrap();
        let clock = Clock::new();
        let mut master = DevMaster::open(dir.path(), clock.unix).unwrap();
        let child = master.add_child("Asha", IST, clock.unix).unwrap();
        Family { _dir: dir, master, child, clock }
    }

    pub fn now(&self) -> Now {
        self.clock.now()
    }

    pub fn offer(&mut self) -> Offer {
        let now = self.clock.unix;
        let child = self.child.clone();
        self.master.make_offer(&child, vec![Ipv4Addr::new(192, 168, 1, 20)], now).unwrap()
    }

    /// Pairs a phone with an offer, through the whole exchange.
    pub fn pair_with(&mut self, offer: &Offer, name: &str) -> Result<Welcomed, Outcome> {
        let now = self.now();
        let mut phone = PhonePairing::start(offer, request(name), now).unwrap();
        let mut link = self.master.accept(now);
        pump_pairing(&mut phone, &mut link, &mut self.master, now)
    }

    /// Pairs a new phone with a new offer.
    pub fn pair(&mut self, name: &str) -> Phone {
        let offer = self.offer();
        let welcomed = self.pair_with(&offer, name).unwrap();
        Phone::from_welcome(welcomed, self.clock.unix)
    }

    /// Opens a session for a phone and runs it until the first pong.
    pub fn open(&mut self, phone: &Phone) -> (PhoneSession, MasterLink) {
        let now = self.now();
        let mut session = PhoneSession::start(&phone.paired, now).unwrap();
        let mut link = self.master.accept(now);
        let pumped = pump(&mut session, &mut link, &mut self.master, now);
        assert_eq!(pumped.phone_error, None);
        assert_eq!(pumped.master_error, None);
        assert_eq!(pumped.phone_events.first(), Some(&PhoneEvent::Opened));
        assert!(matches!(pumped.phone_events.get(1), Some(PhoneEvent::Pong { .. })), "{:?}", pumped.phone_events);
        assert!(matches!(pumped.master_events.first(), Some(MasterEvent::Opened { .. })));
        assert!(session.is_open());
        (session, link)
    }

    /// One report and its grant, through an open session, taken by the
    /// phone; and again at once when the grant asks to be acknowledged.
    pub fn exchange(&mut self, phone: &mut Phone, session: &mut PhoneSession, link: &mut MasterLink, want_more: bool) {
        let now = self.now();
        loop {
            let report = phone.state.report(now.unix, want_more);
            session.send_report(&report, now).unwrap();
            let pumped = pump(session, link, &mut self.master, now);
            assert_eq!(pumped.phone_error, None);
            assert_eq!(pumped.master_error, None);
            let grant = pumped.grant().expect("a grant");
            assert_eq!(phone.state.take_grant(&grant, now.unix), GrantOutcome::Taken);
            if !grant.acknowledge_now {
                break;
            }
        }
    }
}

/// How a pairing ended without a welcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub phone: Option<LinkError>,
    pub master: Option<LinkError>,
}

/// A phone: what it keeps for sessions, and its core state.
pub struct Phone {
    pub paired: PairedMaster,
    pub state: DeviceState,
}

impl Phone {
    pub fn from_welcome(welcomed: Welcomed, now: i64) -> Phone {
        let mut state = DeviceState::from_pairing(&welcomed.welcome, now);
        state.set_code_secret(welcomed.code_secret.expose().to_vec()).unwrap();
        Phone { paired: welcomed.paired, state }
    }

    pub fn id(&self) -> &str {
        &self.paired.device_id
    }
}

pub fn link_error(e: DevError) -> LinkError {
    match e {
        DevError::Link(e) => e,
        DevError::Hub(e) => panic!("the hub refused: {e}"),
    }
}

/// Moves a pairing's bytes both ways until nothing is left to send.
pub fn pump_pairing(
    phone: &mut PhonePairing,
    link: &mut MasterLink,
    master: &mut DevMaster,
    now: Now,
) -> Result<Welcomed, Outcome> {
    let mut welcomed = None;
    let mut outcome = Outcome { phone: None, master: None };
    for _ in 0..20 {
        let to_master = phone.take_outgoing();
        if !to_master.is_empty()
            && outcome.master.is_none()
            && let Err(e) = master.handle(link, &to_master, now)
        {
            outcome.master = Some(link_error(e));
        }
        let to_phone = link.take_outgoing();
        if !to_phone.is_empty() && outcome.phone.is_none() {
            match phone.receive(&to_phone, now) {
                Ok(Some(w)) => welcomed = Some(w),
                Ok(None) => {}
                Err(e) => outcome.phone = Some(e),
            }
        }
        if to_master.is_empty() && to_phone.is_empty() {
            break;
        }
    }
    match welcomed {
        Some(w) if outcome.master.is_none() && outcome.phone.is_none() => Ok(w),
        _ => Err(outcome),
    }
}

/// What a session's pumping brought.
#[derive(Debug, Default)]
pub struct Pumped {
    pub phone_events: Vec<PhoneEvent>,
    pub master_events: Vec<MasterEvent>,
    pub phone_error: Option<LinkError>,
    pub master_error: Option<LinkError>,
}

impl Pumped {
    pub fn grant(&self) -> Option<Grant> {
        self.phone_events.iter().rev().find_map(|e| match e {
            PhoneEvent::Grant { grant, .. } => Some(Grant::clone(grant)),
            _ => None,
        })
    }
}

/// Moves a session's bytes both ways until nothing is left to send.
pub fn pump(session: &mut PhoneSession, link: &mut MasterLink, master: &mut DevMaster, now: Now) -> Pumped {
    let mut pumped = Pumped::default();
    for _ in 0..50 {
        let to_master = session.take_outgoing();
        if !to_master.is_empty() && pumped.master_error.is_none() {
            match master.handle(link, &to_master, now) {
                Ok(events) => pumped.master_events.extend(events),
                Err(e) => pumped.master_error = Some(link_error(e)),
            }
        }
        let to_phone = link.take_outgoing();
        if !to_phone.is_empty() && pumped.phone_error.is_none() {
            match session.receive(&to_phone, now) {
                Ok(events) => pumped.phone_events.extend(events),
                Err(e) => pumped.phone_error = Some(e),
            }
        }
        if to_master.is_empty() && to_phone.is_empty() {
            break;
        }
    }
    pumped
}

/// Every frame in a stream of bytes, whole (length and body).
pub fn frames(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut at = 0;
    while at + 4 <= bytes.len() {
        let len = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        out.push(bytes[at..at + 4 + len].to_vec());
        at += 4 + len;
    }
    assert_eq!(at, bytes.len());
    out
}

/// A small seeded generator for the stories that need chance, so a
/// failure can be run again.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed ^ 0x9e37_79b9_7f4a_7c15)
    }

    pub fn next(&mut self) -> u64 {
        // splitmix64
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    pub fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next() as u8).collect()
    }
}
