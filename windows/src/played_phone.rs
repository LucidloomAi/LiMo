//! A phone played over the link, with real sockets: the link's phone side
//! ([`limo_link::PhonePairing`], [`limo_link::PhoneSession`]) and the
//! core's device ([`limo_core::DeviceState`]), as LiMo for Android will run
//! them. For the tests, which play a phone against the service over the
//! loopback address, and for the example program, which plays one from
//! another PC on the home network. Only with the `played-phone` feature;
//! never part of the service.
//!
//! Waiting is done through a `pump`, called while the phone waits for the
//! master: it moves whatever has to move meanwhile (in the tests, the
//! service and the fake PC's clock; in the example, nothing but time) and
//! returns the time now.

#![forbid(unsafe_code)]

use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use limo_core::{Decision, DeviceState, Grant, GrantOutcome};
use limo_link::{Form, LinkError, Now, Offer, PairRequest, PairedMaster, PhoneEvent, PhonePairing, PhoneSession};
use zeroize::Zeroize;

/// How long a phone waits for the master in real time, at most.
pub const WAIT_LIMIT: Duration = Duration::from_secs(20);

/// A socket to the master, read without blocking.
#[derive(Debug)]
pub struct Wire {
    stream: TcpStream,
}

impl Wire {
    /// Connects to the master.
    pub fn connect(address: SocketAddr) -> Result<Wire, String> {
        let stream = TcpStream::connect_timeout(&address, Duration::from_secs(5)).map_err(|e| format!("could not connect: {e}"))?;
        stream.set_nonblocking(true).map_err(|e| e.to_string())?;
        let _ = stream.set_nodelay(true);
        Ok(Wire { stream })
    }

    /// Writes all of `bytes`, whatever they are.
    pub fn send(&mut self, mut bytes: &[u8]) -> Result<(), String> {
        let give_up = Instant::now() + Duration::from_secs(5);
        while !bytes.is_empty() {
            match self.stream.write(bytes) {
                Ok(0) => return Err("the master closed the connection".to_string()),
                Ok(n) => bytes = &bytes[n..],
                Err(e) if e.kind() == ErrorKind::WouldBlock && Instant::now() < give_up => std::thread::sleep(Duration::from_millis(2)),
                Err(e) => return Err(format!("the master closed the connection ({e})")),
            }
        }
        Ok(())
    }

    /// What came, if anything; an error once the master closed the
    /// connection.
    pub fn take(&mut self) -> Result<Option<Vec<u8>>, String> {
        let mut buffer = vec![0u8; 64 * 1024];
        let got = match self.stream.read(&mut buffer) {
            Ok(0) => Err("the master closed the connection".to_string()),
            Ok(n) => Ok(Some(buffer[..n].to_vec())),
            Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::Interrupted => Ok(None),
            Err(e) => Err(format!("the master closed the connection ({e})")),
        };
        buffer.zeroize();
        got
    }

    /// True once the master has closed the connection: waits up to
    /// `WAIT_LIMIT`, pumping.
    pub fn closed(&mut self, pump: &mut dyn FnMut() -> Now) -> bool {
        let give_up = Instant::now() + WAIT_LIMIT;
        while Instant::now() < give_up {
            pump();
            match self.take() {
                Err(_) => return true,
                Ok(_) => std::thread::sleep(Duration::from_millis(2)),
            }
        }
        false
    }
}

/// A phone that paired: what it keeps, and its device's state.
#[derive(Debug)]
pub struct PlayedPhone {
    /// What it keeps to open its sessions (with its private key).
    pub paired: PairedMaster,
    /// Its device's state, as the core keeps it.
    pub state: DeviceState,
}

/// Why playing the phone stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhoneProblem {
    /// The master said no, with the link's reason.
    Refused(limo_link::Refusal),
    /// Anything else, in words.
    Other(String),
}

impl std::fmt::Display for PhoneProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PhoneProblem::Refused(why) => write!(f, "the master PC refused: {}", why.sentence()),
            PhoneProblem::Other(what) => write!(f, "{what}"),
        }
    }
}

fn problem(error: LinkError) -> PhoneProblem {
    match error {
        LinkError::Refused(why) => PhoneProblem::Refused(why),
        other => PhoneProblem::Other(other.to_string()),
    }
}

fn other(text: String) -> PhoneProblem {
    PhoneProblem::Other(text)
}

impl PlayedPhone {
    /// Pairs with the master at `address` with an offer, as LiMo for
    /// Android's child form does.
    pub fn pair(address: SocketAddr, offer: &Offer, name: &str, pump: &mut dyn FnMut() -> Now) -> Result<PlayedPhone, PhoneProblem> {
        let now = pump();
        let mut wire = Wire::connect(address).map_err(other)?;
        let request =
            PairRequest { name: name.to_string(), form: Form::Child, platform: "android".to_string(), android_version: Some(34) };
        let mut pairing = PhonePairing::start(offer, request, now).map_err(problem)?;
        wire.send(&pairing.take_outgoing()).map_err(other)?;
        let give_up = Instant::now() + WAIT_LIMIT;
        while Instant::now() < give_up {
            let now = pump();
            if let Some(bytes) = wire.take().map_err(other)? {
                if let Some(welcomed) = pairing.receive(&bytes, now).map_err(problem)? {
                    let _ = wire.send(&pairing.take_outgoing());
                    let mut state = DeviceState::from_pairing(&welcomed.welcome, now.unix);
                    state.set_code_secret(welcomed.code_secret.expose().to_vec()).map_err(|e| other(e.to_string()))?;
                    return Ok(PlayedPhone { paired: welcomed.paired, state });
                }
                wire.send(&pairing.take_outgoing()).map_err(other)?;
            }
            pairing.tick(now).map_err(problem)?;
            std::thread::sleep(Duration::from_millis(1));
        }
        Err(other("the master did not answer in time".to_string()))
    }

    /// Opens a session with the master at `address`, as the paired phone.
    pub fn open(&self, address: SocketAddr, pump: &mut dyn FnMut() -> Now) -> Result<Session, PhoneProblem> {
        let now = pump();
        let mut wire = Wire::connect(address).map_err(other)?;
        let mut session = PhoneSession::start(&self.paired, now).map_err(problem)?;
        wire.send(&session.take_outgoing()).map_err(other)?;
        let mut open = Session { wire, session, held: Vec::new() };
        let give_up = Instant::now() + WAIT_LIMIT;
        while Instant::now() < give_up {
            let events = open.events(pump)?;
            if events.contains(&PhoneEvent::Opened) {
                open.held.extend(events.into_iter().filter(|e| *e != PhoneEvent::Opened));
                return Ok(open);
            }
            open.held.extend(events);
            std::thread::sleep(Duration::from_millis(1));
        }
        Err(other("the session did not open in time".to_string()))
    }

    /// Records `secs` seconds of `app` in front, ending at `end`.
    pub fn use_app(&mut self, app: &str, end: i64, secs: i64) {
        self.state.record_use(app, end - secs, end);
    }

    /// What the phone's rules decide now for `app` in front.
    pub fn decide(&mut self, now: i64, app: Option<&str>) -> Decision {
        self.state.decide(now, app)
    }
}

/// A session open with the master.
pub struct Session {
    wire: Wire,
    session: PhoneSession,
    held: Vec<PhoneEvent>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session").field("held", &self.held.len()).finish_non_exhaustive()
    }
}

impl Session {
    /// What came now, after one pump; writes what the session has to send.
    fn events(&mut self, pump: &mut dyn FnMut() -> Now) -> Result<Vec<PhoneEvent>, PhoneProblem> {
        let now = pump();
        let mut events = Vec::new();
        if let Some(bytes) = self.wire.take().map_err(other)? {
            events = self.session.receive(&bytes, now).map_err(problem)?;
        }
        self.session.tick(now).map_err(problem)?;
        let out = self.session.take_outgoing();
        if !out.is_empty() {
            self.wire.send(&out).map_err(other)?;
        }
        Ok(events)
    }

    /// Sends a report and waits for its grant, which the phone takes.
    pub fn exchange(&mut self, phone: &mut PlayedPhone, want_more: bool, pump: &mut dyn FnMut() -> Now) -> Result<Box<Grant>, PhoneProblem> {
        let now = pump();
        let report = phone.state.report(now.unix, want_more);
        self.session.send_report(&report, now).map_err(problem)?;
        let out = self.session.take_outgoing();
        self.wire.send(&out).map_err(other)?;
        let give_up = Instant::now() + WAIT_LIMIT;
        while Instant::now() < give_up {
            for event in self.events(pump)? {
                match event {
                    PhoneEvent::Grant { grant, .. } => {
                        let now = pump();
                        let outcome = phone.state.take_grant(&grant, now.unix);
                        if outcome == GrantOutcome::NotForThisDevice {
                            return Err(other("a grant for another device came".to_string()));
                        }
                        return Ok(grant);
                    }
                    other_event => self.held.push(other_event),
                }
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        Err(other("no grant came in time".to_string()))
    }

    /// Sends a report and does not wait: its grant is held when it comes,
    /// and taken by [`Session::take_held_grants`].
    pub fn send_report(&mut self, phone: &mut PlayedPhone, want_more: bool, pump: &mut dyn FnMut() -> Now) -> Result<(), PhoneProblem> {
        let now = pump();
        let report = phone.state.report(now.unix, want_more);
        self.session.send_report(&report, now).map_err(problem)?;
        let out = self.session.take_outgoing();
        self.wire.send(&out).map_err(other)
    }

    /// The grants held so far, taken by the phone; how many.
    pub fn take_held_grants(&mut self, phone: &mut PlayedPhone, now: Now) -> usize {
        let mut taken = 0;
        self.held.retain(|event| match event {
            PhoneEvent::Grant { grant, .. } => {
                phone.state.take_grant(grant, now.unix);
                taken += 1;
                false
            }
            _ => true,
        });
        taken
    }

    /// Waits up to `limit` (real time) for the master to ask for a report,
    /// pumping; true when it asked.
    pub fn asked_to_report(&mut self, limit: Duration, pump: &mut dyn FnMut() -> Now) -> Result<bool, PhoneProblem> {
        if let Some(at) = self.held.iter().position(|e| *e == PhoneEvent::ReportNow) {
            self.held.remove(at);
            return Ok(true);
        }
        let give_up = Instant::now() + limit;
        while Instant::now() < give_up {
            let events = self.events(pump)?;
            if events.contains(&PhoneEvent::ReportNow) {
                self.held.extend(events.into_iter().filter(|e| *e != PhoneEvent::ReportNow));
                return Ok(true);
            }
            self.held.extend(events);
            std::thread::sleep(Duration::from_millis(1));
        }
        Ok(false)
    }

    /// Keeps the session alive for a while: answers pings and holds what
    /// came. An error when the master ended it.
    pub fn idle(&mut self, pump: &mut dyn FnMut() -> Now) -> Result<(), PhoneProblem> {
        let events = self.events(pump)?;
        self.held.extend(events);
        Ok(())
    }

    /// Sends the phone's apps.
    pub fn send_apps(&mut self, apps: Vec<limo_link::AppEntry>, pump: &mut dyn FnMut() -> Now) -> Result<(), PhoneProblem> {
        let now = pump();
        self.session.send_apps(apps, now).map_err(problem)?;
        let out = self.session.take_outgoing();
        self.wire.send(&out).map_err(other)
    }

    /// Says goodbye and closes.
    pub fn bye(mut self, pump: &mut dyn FnMut() -> Now) {
        let now = pump();
        if self.session.send_bye(now).is_ok() {
            let out = self.session.take_outgoing();
            let _ = self.wire.send(&out);
        }
    }

    /// The socket, for tests that write what they like on it.
    pub fn wire(&mut self) -> &mut Wire {
        &mut self.wire
    }
}
