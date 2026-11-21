//! The phone's side: pairing once, then a session whenever the master is
//! in reach.
//!
//! Both are state machines. The caller connects a socket, starts one,
//! writes what [`PhonePairing::take_outgoing`] or
//! [`PhoneSession::take_outgoing`] gives, feeds every byte read into
//! `receive`, and calls `tick` at the time it asks for (or sooner). Nothing
//! here reads a clock or touches a socket.

use std::collections::VecDeque;
use std::fmt;
use std::net::Ipv4Addr;

use limo_core::{Grant, Report};
use serde::{Deserialize, Serialize};
use snow::{HandshakeState, TransportState};
use zeroize::Zeroizing;

use crate::crypto::{
    PAIR_FINISH_BYTES, PAIR_HELLO_BYTES, PAIR_REPLY_BYTES, SESSION_HELLO_BYTES, SESSION_REPLY_BYTES, pairing_phone,
    session_phone,
};
use crate::discovery::MasterId;
use crate::error::LinkError;
use crate::frame::{Allowed, FrameReader, Kind, Preamble, frame, handshake_read, handshake_write, open, seal};
use crate::keys::{KeyPair, PublicKey};
use crate::messages::{
    AppEntry, CODE_SECRET_BYTES, CodeSecret, MAX_NAME_CHARS, MasterSays, PairRequest, PhoneAbout, PhoneSays, apps_hash,
    check_apps, from_json, read_message, to_json,
};
use crate::offer::Offer;
use crate::{ANSWER_LIMIT_MS, HANDSHAKE_LIMIT_MS, LINK_VERSION, MAX_WAITING, Now, PING_EVERY_MS};

/// What a phone keeps after pairing, to open every session after it. It
/// holds the phone's private key: keep it in the app's private storage, as
/// the bytes [`PairedMaster::to_bytes`] gives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairedMaster {
    /// The master's public key, from the offer. The only key the phone ever
    /// accepts from a master.
    pub master_key: PublicKey,
    /// The child this phone belongs to.
    pub child_id: String,
    /// This phone's id at the master.
    pub device_id: String,
    /// The port the master listens on, from the offer.
    pub port: u16,
    /// The master's addresses, from the offer, to try when discovery finds
    /// nothing. The caller may replace them with what discovery finds.
    pub addresses: Vec<Ipv4Addr>,
    /// This phone's own key pair, made when it paired.
    pub phone_keys: KeyPair,
}

impl PairedMaster {
    /// The master's id, to pick its announcement out on the network.
    pub fn master_id(&self) -> MasterId {
        MasterId::of(&self.master_key)
    }

    /// As bytes (JSON) to keep, in memory that is wiped when dropped.
    pub fn to_bytes(&self) -> Result<Zeroizing<Vec<u8>>, LinkError> {
        to_json(self)
    }

    /// From the bytes [`PairedMaster::to_bytes`] gave.
    pub fn from_bytes(bytes: &[u8]) -> Result<PairedMaster, LinkError> {
        from_json(bytes)
    }
}

/// What a phone gets when it has paired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Welcomed {
    /// What to keep for every session after this one.
    pub paired: PairedMaster,
    /// The first grant: make the device's state from it with
    /// `DeviceState::from_pairing`.
    pub welcome: Grant,
    /// The device's code secret: hand it to `DeviceState::set_code_secret`.
    pub code_secret: CodeSecret,
    /// The master's clock when it answered, unix seconds.
    pub master_now: i64,
    /// The child's name as the master gave it, for the child's own screens;
    /// None from a master that does not say it.
    pub child_name: Option<String>,
}

enum PairingStage {
    AwaitReply { hs: Box<HandshakeState>, deadline: u64 },
    AwaitWelcome { noise: Box<TransportState>, deadline: u64 },
    Done,
    Failed,
}

/// The phone's side of pairing: the handshake with the offer's secret, then
/// `pair`, then the master's `welcome`, then `bye`.
pub struct PhonePairing {
    stage: PairingStage,
    offer: Offer,
    keys: KeyPair,
    /// Kept until the reply comes, then sent as the first message.
    request: Option<PairRequest>,
    reader: FrameReader,
    out: Vec<u8>,
}

impl fmt::Debug for PhonePairing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let stage = match self.stage {
            PairingStage::AwaitReply { .. } => "waiting for the reply",
            PairingStage::AwaitWelcome { .. } => "waiting for the welcome",
            PairingStage::Done => "done",
            PairingStage::Failed => "failed",
        };
        f.debug_struct("PhonePairing")
            .field("stage", &stage)
            .field("offer", &self.offer.id())
            .field("phone_key", &self.keys.public())
            .finish()
    }
}

impl PhonePairing {
    /// Starts pairing with the master an offer names. Makes the phone's
    /// key pair, and the first frame, ready in
    /// [`PhonePairing::take_outgoing`]. The offer's expiry is the master's
    /// to judge: the phone's own clock may be wrong.
    pub fn start(offer: &Offer, request: PairRequest, now: Now) -> Result<PhonePairing, LinkError> {
        let keys = KeyPair::generate()?;
        let preamble = Preamble { version: LINK_VERSION, kind: Kind::Pairing(offer.id()) }.bytes();
        let mut hs = pairing_phone(&keys, offer.secret(), &preamble)?;
        let hello = handshake_write(&mut hs, PAIR_HELLO_BYTES)?;
        let mut body = preamble;
        body.extend_from_slice(&hello);
        Ok(PhonePairing {
            stage: PairingStage::AwaitReply {
                hs: Box::new(hs),
                deadline: now.ms.saturating_add(HANDSHAKE_LIMIT_MS),
            },
            offer: offer.clone(),
            keys,
            request: Some(request),
            reader: FrameReader::default(),
            out: frame(&body),
        })
    }

    /// The bytes to write to the socket now; empty when there are none.
    pub fn take_outgoing(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.out)
    }

    /// True once the welcome has come.
    pub fn is_done(&self) -> bool {
        matches!(self.stage, PairingStage::Done)
    }

    /// Feeds bytes read from the socket. Returns the welcome once it has
    /// come; the `bye` that ends the pairing is then in
    /// [`PhonePairing::take_outgoing`], and the socket can be closed once it
    /// is written. Any error ends the pairing.
    pub fn receive(&mut self, bytes: &[u8], now: Now) -> Result<Option<Welcomed>, LinkError> {
        let result = self.receive_inner(bytes, now);
        if result.is_err() {
            self.stage = PairingStage::Failed;
            self.out.clear();
        }
        result
    }

    fn receive_inner(&mut self, bytes: &[u8], now: Now) -> Result<Option<Welcomed>, LinkError> {
        let mut input = bytes;
        while !input.is_empty() {
            let allowed = match self.stage {
                PairingStage::AwaitReply { .. } => Allowed::Exactly("pairing reply", &[PAIR_REPLY_BYTES]),
                PairingStage::AwaitWelcome { .. } => Allowed::Message,
                PairingStage::Done | PairingStage::Failed => return Err(LinkError::Closed),
            };
            let Some(body) = self.reader.feed(&mut input, allowed)? else {
                break;
            };
            match std::mem::replace(&mut self.stage, PairingStage::Failed) {
                PairingStage::AwaitReply { mut hs, .. } => {
                    handshake_read(&mut hs, &body)?;
                    let theirs = hs.get_remote_static().ok_or(LinkError::Garbled)?;
                    if theirs != self.offer.master_key().as_bytes() {
                        return Err(LinkError::WrongMaster);
                    }
                    let finish = handshake_write(&mut hs, PAIR_FINISH_BYTES)?;
                    self.out.extend(frame(&finish));
                    let mut noise = hs
                        .into_transport_mode()
                        .map_err(|_| LinkError::Internal { what: "finish a pairing handshake" })?;
                    let request = self.request.take().ok_or(LinkError::Internal { what: "find the request" })?;
                    let pair = PhoneSays::Pair {
                        name: request.name,
                        form: request.form,
                        platform: request.platform,
                        android_version: request.android_version,
                    };
                    self.out.extend(frame(&seal(&mut noise, &to_json(&pair)?)?));
                    self.stage = PairingStage::AwaitWelcome {
                        noise: Box::new(noise),
                        deadline: now.ms.saturating_add(ANSWER_LIMIT_MS),
                    };
                }
                PairingStage::AwaitWelcome { mut noise, deadline } => {
                    let text = open(&mut noise, &body)?;
                    let Some(said) = read_message::<MasterSays>(&text)? else {
                        // A kind this version does not know: passed over.
                        self.stage = PairingStage::AwaitWelcome { noise, deadline };
                        continue;
                    };
                    match said {
                        MasterSays::Welcome { welcome, code_secret, master_now, child_name } => {
                            if !input.is_empty() {
                                return Err(LinkError::Unexpected { what: "more after the welcome" });
                            }
                            if welcome.device_id.is_empty() || welcome.child_id.is_empty() {
                                return Err(LinkError::OutOfBounds { what: "a welcome with no device or child" });
                            }
                            if code_secret.expose().len() != CODE_SECRET_BYTES {
                                return Err(LinkError::OutOfBounds { what: "a code secret of the wrong length" });
                            }
                            if child_name.as_ref().is_some_and(|n| n.chars().count() > MAX_NAME_CHARS) {
                                return Err(LinkError::OutOfBounds { what: "a child's name that is too long" });
                            }
                            self.out.extend(frame(&seal(&mut noise, &to_json(&PhoneSays::Bye {})?)?));
                            self.stage = PairingStage::Done;
                            let paired = PairedMaster {
                                master_key: self.offer.master_key(),
                                child_id: welcome.child_id.clone(),
                                device_id: welcome.device_id.clone(),
                                port: self.offer.port(),
                                addresses: self.offer.addresses().to_vec(),
                                phone_keys: self.keys.clone(),
                            };
                            return Ok(Some(Welcomed { paired, welcome, code_secret, master_now, child_name }));
                        }
                        MasterSays::Refused { why, .. } => return Err(LinkError::Refused(why)),
                        MasterSays::Grant { .. } => return Err(LinkError::Unexpected { what: "a grant while pairing" }),
                        MasterSays::ReportNow {} => {
                            return Err(LinkError::Unexpected { what: "a request to report while pairing" });
                        }
                        MasterSays::Pong { .. } => return Err(LinkError::Unexpected { what: "a pong while pairing" }),
                    }
                }
                PairingStage::Done | PairingStage::Failed => return Err(LinkError::Closed),
            }
        }
        Ok(None)
    }

    /// Checks the time. Returns when to call again (`Now::ms`), or None
    /// once the pairing is over. An error ends the pairing.
    pub fn tick(&mut self, now: Now) -> Result<Option<u64>, LinkError> {
        let (deadline, error) = match &self.stage {
            PairingStage::AwaitReply { deadline, .. } => (*deadline, LinkError::HandshakeTooSlow),
            PairingStage::AwaitWelcome { deadline, .. } => (*deadline, LinkError::NoAnswer { report_waiting: false }),
            PairingStage::Done => return Ok(None),
            PairingStage::Failed => return Err(LinkError::Closed),
        };
        if now.ms >= deadline {
            self.stage = PairingStage::Failed;
            self.out.clear();
            return Err(error);
        }
        Ok(Some(deadline))
    }
}

/// What came from the master on a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhoneEvent {
    /// The handshake finished. The session sent its first ping by itself;
    /// send a report now.
    Opened,
    /// The answer to a report: take it with `DeviceState::take_grant`.
    Grant {
        /// The grant (boxed: it is far larger than the other events).
        grant: Box<Grant>,
        /// The master's clock when it answered, unix seconds.
        master_now: i64,
    },
    /// The master asks for a report at once.
    ReportNow,
    /// The answer to a ping.
    Pong {
        /// The master's clock when it answered, unix seconds.
        master_now: i64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Asked {
    Report,
    Ping,
}

enum SessionStage {
    AwaitReply { hs: Box<HandshakeState>, deadline: u64 },
    Open { noise: Box<TransportState> },
    Ended,
    Failed,
}

/// The phone's side of a session with the master it paired with.
pub struct PhoneSession {
    stage: SessionStage,
    device_id: String,
    child_id: String,
    reader: FrameReader,
    out: Vec<u8>,
    waiting: VecDeque<(Asked, u64)>,
    last_sent: u64,
    /// What this session last told the master the phone is.
    about_said: Option<PhoneAbout>,
}

impl fmt::Debug for PhoneSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let stage = match self.stage {
            SessionStage::AwaitReply { .. } => "waiting for the reply",
            SessionStage::Open { .. } => "open",
            SessionStage::Ended => "ended",
            SessionStage::Failed => "failed",
        };
        f.debug_struct("PhoneSession")
            .field("stage", &stage)
            .field("device_id", &self.device_id)
            .field("waiting", &self.waiting.len())
            .finish()
    }
}

impl PhoneSession {
    /// Starts a session with the master this phone paired with. The first
    /// frame is ready in [`PhoneSession::take_outgoing`].
    pub fn start(paired: &PairedMaster, now: Now) -> Result<PhoneSession, LinkError> {
        let preamble = Preamble { version: LINK_VERSION, kind: Kind::Session }.bytes();
        let mut hs = session_phone(&paired.phone_keys, &paired.master_key, &preamble)?;
        let hello = handshake_write(&mut hs, SESSION_HELLO_BYTES)?;
        let mut body = preamble;
        body.extend_from_slice(&hello);
        Ok(PhoneSession {
            stage: SessionStage::AwaitReply { hs: Box::new(hs), deadline: now.ms.saturating_add(HANDSHAKE_LIMIT_MS) },
            device_id: paired.device_id.clone(),
            child_id: paired.child_id.clone(),
            reader: FrameReader::default(),
            out: frame(&body),
            waiting: VecDeque::new(),
            last_sent: now.ms,
            about_said: None,
        })
    }

    /// The bytes to write to the socket now; empty when there are none.
    pub fn take_outgoing(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.out)
    }

    /// True while the session is open: the handshake finished, and it has
    /// neither ended nor failed.
    pub fn is_open(&self) -> bool {
        matches!(self.stage, SessionStage::Open { .. })
    }

    /// True while a report waits for its grant. If the socket closes now,
    /// the phone should take it that the master cannot be reached
    /// (`DeviceState::lost_contact`).
    pub fn report_waiting(&self) -> bool {
        self.waiting.iter().any(|(asked, _)| *asked == Asked::Report)
    }

    /// Feeds bytes read from the socket. Returns what came, in order. Any
    /// error ends the session, and nothing more goes out: what was waiting
    /// to be written is dropped, and so is anything that came before the
    /// bad frame in the same bytes (a grant lost this way costs nothing:
    /// grants are levels with numbers, and the next session brings a newer
    /// one).
    pub fn receive(&mut self, bytes: &[u8], now: Now) -> Result<Vec<PhoneEvent>, LinkError> {
        let mut events = Vec::new();
        let result = self.receive_inner(bytes, now, &mut events);
        match result {
            Ok(()) => Ok(events),
            Err(error) => {
                self.stage = SessionStage::Failed;
                self.out.clear();
                Err(error)
            }
        }
    }

    fn receive_inner(&mut self, bytes: &[u8], now: Now, events: &mut Vec<PhoneEvent>) -> Result<(), LinkError> {
        let mut input = bytes;
        while !input.is_empty() {
            let allowed = match self.stage {
                SessionStage::AwaitReply { .. } => Allowed::Exactly("session reply", &[SESSION_REPLY_BYTES]),
                SessionStage::Open { .. } => Allowed::Message,
                SessionStage::Ended | SessionStage::Failed => return Err(LinkError::Closed),
            };
            let Some(body) = self.reader.feed(&mut input, allowed)? else {
                break;
            };
            match std::mem::replace(&mut self.stage, SessionStage::Failed) {
                SessionStage::AwaitReply { mut hs, .. } => {
                    handshake_read(&mut hs, &body)?;
                    let noise =
                        hs.into_transport_mode().map_err(|_| LinkError::Internal { what: "finish a session handshake" })?;
                    self.stage = SessionStage::Open { noise: Box::new(noise) };
                    events.push(PhoneEvent::Opened);
                    // The first message proves to the master that this is a
                    // live phone, not an old handshake played again.
                    self.send(&PhoneSays::Ping {}, Some(Asked::Ping), now)?;
                }
                SessionStage::Open { mut noise } => {
                    let text = open(&mut noise, &body)?;
                    let said = read_message::<MasterSays>(&text);
                    self.stage = SessionStage::Open { noise };
                    let Some(said) = said? else {
                        // A kind this version does not know: passed over.
                        continue;
                    };
                    match said {
                        MasterSays::Grant { grant, master_now } => {
                            if grant.device_id != self.device_id || grant.child_id != self.child_id {
                                return Err(LinkError::OtherDevice);
                            }
                            self.answered(Asked::Report, "a grant nobody asked for")?;
                            events.push(PhoneEvent::Grant { grant: Box::new(grant), master_now });
                        }
                        MasterSays::Pong { master_now } => {
                            self.answered(Asked::Ping, "a pong nobody asked for")?;
                            events.push(PhoneEvent::Pong { master_now });
                        }
                        MasterSays::ReportNow {} => events.push(PhoneEvent::ReportNow),
                        MasterSays::Refused { why, .. } => return Err(LinkError::Refused(why)),
                        MasterSays::Welcome { .. } => {
                            return Err(LinkError::Unexpected { what: "a welcome on a session" });
                        }
                    }
                }
                SessionStage::Ended | SessionStage::Failed => return Err(LinkError::Closed),
            }
        }
        Ok(())
    }

    fn answered(&mut self, asked: Asked, what: &'static str) -> Result<(), LinkError> {
        let at = self.waiting.iter().position(|(a, _)| *a == asked).ok_or(LinkError::Unexpected { what })?;
        self.waiting.remove(at);
        Ok(())
    }

    fn send(&mut self, message: &PhoneSays, asks: Option<Asked>, now: Now) -> Result<(), LinkError> {
        let SessionStage::Open { noise } = &mut self.stage else {
            return Err(match self.stage {
                SessionStage::AwaitReply { .. } => LinkError::NotOpen,
                _ => LinkError::Closed,
            });
        };
        if asks.is_some() && self.waiting.len() >= MAX_WAITING {
            return Err(LinkError::TooManyWaiting);
        }
        let body = seal(noise, &to_json(message)?)?;
        self.out.extend(frame(&body));
        self.last_sent = now.ms;
        if let Some(asked) = asks {
            self.waiting.push_back((asked, now.ms.saturating_add(ANSWER_LIMIT_MS)));
        }
        Ok(())
    }

    /// Sends a report from this phone's `DeviceState::report`. Its grant
    /// comes as [`PhoneEvent::Grant`] within [`ANSWER_LIMIT_MS`], or the
    /// session ends with [`LinkError::NoAnswer`]. A report for another
    /// device is refused, and changes nothing.
    pub fn send_report(&mut self, report: &Report, now: Now) -> Result<(), LinkError> {
        if report.device_id != self.device_id || report.child_id != self.child_id {
            return Err(LinkError::OtherDevice);
        }
        self.send(&PhoneSays::Report { report: Box::new(report.clone()) }, Some(Asked::Report), now)
    }

    /// Sends the phone's app list, with its hash. Nothing answers it.
    pub fn send_apps(&mut self, apps: Vec<AppEntry>, now: Now) -> Result<(), LinkError> {
        check_apps(&apps, None)?;
        let hash = apps_hash(&apps);
        self.send(&PhoneSays::Apps { apps, hash }, None, now)
    }

    /// Tells the master what the phone is, when this session has not said
    /// exactly that yet: call it when the session opens, and again whenever
    /// any of it may have changed (an update of the app changes its
    /// version). True when it was sent, false when the session had said it
    /// already. Nothing answers it. Never on a pairing: the first session
    /// after it says it.
    pub fn send_about(&mut self, about: &PhoneAbout, now: Now) -> Result<bool, LinkError> {
        if self.about_said.as_ref() == Some(about) {
            return Ok(false);
        }
        self.send(&PhoneSays::about(about)?, None, now)?;
        self.about_said = Some(about.clone());
        Ok(true)
    }

    /// Says goodbye and ends the session; write what
    /// [`PhoneSession::take_outgoing`] gives, then close the socket. Before
    /// the handshake has finished there is nothing to say it with, and the
    /// session simply ends.
    pub fn send_bye(&mut self, now: Now) -> Result<(), LinkError> {
        match self.stage {
            SessionStage::Open { .. } => {
                self.send(&PhoneSays::Bye {}, None, now)?;
                self.stage = SessionStage::Ended;
                Ok(())
            }
            SessionStage::AwaitReply { .. } => {
                self.stage = SessionStage::Ended;
                Ok(())
            }
            SessionStage::Ended | SessionStage::Failed => Err(LinkError::Closed),
        }
    }

    /// Checks the time: sends a ping when nothing went for
    /// [`PING_EVERY_MS`], and ends the session when the handshake or an
    /// answer is late. Returns when to call again (`Now::ms`), or None once
    /// the session has ended.
    pub fn tick(&mut self, now: Now) -> Result<Option<u64>, LinkError> {
        match &self.stage {
            SessionStage::AwaitReply { deadline, .. } => {
                if now.ms >= *deadline {
                    self.stage = SessionStage::Failed;
                    self.out.clear();
                    return Err(LinkError::HandshakeTooSlow);
                }
                Ok(Some(*deadline))
            }
            SessionStage::Open { .. } => {
                if self.waiting.iter().any(|(_, deadline)| now.ms >= *deadline) {
                    let report_waiting = self.report_waiting();
                    self.stage = SessionStage::Failed;
                    self.out.clear();
                    return Err(LinkError::NoAnswer { report_waiting });
                }
                if self.waiting.is_empty() && now.ms.saturating_sub(self.last_sent) >= PING_EVERY_MS {
                    self.send(&PhoneSays::Ping {}, Some(Asked::Ping), now)?;
                }
                let next_ping = self.last_sent.saturating_add(PING_EVERY_MS);
                let next_answer = self.waiting.iter().map(|(_, deadline)| *deadline).min();
                Ok(Some(next_answer.map_or(next_ping, |answer| answer.min(next_ping))))
            }
            SessionStage::Ended => Ok(None),
            SessionStage::Failed => Err(LinkError::Closed),
        }
    }
}
