//! The master's side: one [`MasterLink`] for each connection a phone
//! makes, pairing or session, told apart by the phone's first frame.
//!
//! The master's caller keeps the offers it made and the phones it paired,
//! and shows them to the link through [`Directory`]; it pairs the phone with
//! the hub and answers reports with the hub's grants. The link checks
//! everything else: the handshakes, the frames, that a report is for the
//! session's own device and child, and the time.

use std::fmt;

use limo_core::{Grant, Report};
use snow::{HandshakeState, TransportState};

use crate::crypto::{
    PAIR_FINISH_BYTES, PAIR_HELLO_BYTES, PAIR_REPLY_BYTES, SESSION_HELLO_BYTES, SESSION_REPLY_BYTES, pairing_master,
    session_master,
};
use crate::error::LinkError;
use crate::frame::{
    Allowed, FrameReader, Kind, PAIRING_PREAMBLE_BYTES, Preamble, SESSION_PREAMBLE_BYTES, frame, handshake_read,
    handshake_write, open, seal,
};
use crate::keys::{KEY_BYTES, KeyPair, PublicKey};
use crate::messages::{
    AppEntry, CODE_SECRET_BYTES, CodeSecret, MasterSays, PairRequest, PhoneAbout, PhoneSays, Refusal, read_message,
    to_json,
};
use crate::offer::{Offer, OfferId};
use crate::{ANSWER_LIMIT_MS, HANDSHAKE_LIMIT_MS, LINK_VERSION, Now, SILENCE_LIMIT_MS};

/// An offer the master made, as its caller keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfferOnFile {
    /// The offer.
    pub offer: Offer,
    /// True once a phone has paired with it.
    pub used: bool,
}

/// A phone the master paired, as its caller keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownPhone {
    /// Its id at the master.
    pub device_id: String,
    /// The child it belongs to.
    pub child_id: String,
    /// Its static public key, from the pairing handshake.
    pub key: PublicKey,
    /// True once it was removed. A removed phone that connects is told so,
    /// and the link ends.
    pub removed: bool,
}

/// What the master knows, as the link needs it.
pub trait Directory {
    /// An offer this master made, by its id, while the master keeps it.
    fn offer(&self, id: &OfferId) -> Option<OfferOnFile>;

    /// Every phone this master paired, removed ones too, so that a removed
    /// phone can be told. A session's first message is tried against each
    /// in turn (KK names no one in the clear), so the list is a family's
    /// phones, not thousands.
    fn phones(&self) -> Vec<KnownPhone>;
}

/// The simplest [`Directory`]: two lists.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DirectoryList {
    /// The offers.
    pub offers: Vec<OfferOnFile>,
    /// The phones.
    pub phones: Vec<KnownPhone>,
}

impl Directory for DirectoryList {
    fn offer(&self, id: &OfferId) -> Option<OfferOnFile> {
        self.offers.iter().find(|o| o.offer.id() == *id).cloned()
    }

    fn phones(&self) -> Vec<KnownPhone> {
        self.phones.clone()
    }
}

/// What came from a phone, for the master's caller to act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MasterEvent {
    /// A phone that holds the offer asks to pair. Pair it with the offer's
    /// child, mark the offer used in the same step (if it was used
    /// meanwhile, refuse with [`Refusal::OfferUsed`]), keep its key, and
    /// answer with [`MasterLink::welcome`]; or refuse.
    PairRequest {
        /// The offer it used.
        offer_id: OfferId,
        /// The phone's static key, to keep for its sessions.
        phone_key: PublicKey,
        /// What it says about itself.
        request: PairRequest,
    },
    /// A known phone's session is open: its first message has come, so it
    /// is live, not an old handshake played again. A new session of a
    /// phone replaces its old one, now and not before.
    Opened {
        /// The phone.
        device_id: String,
        /// Its child.
        child_id: String,
    },
    /// A report from the session's phone, checked to be its own. Answer it
    /// with the hub's grant through [`MasterLink::grant`].
    Report {
        /// The report.
        report: Report,
    },
    /// The phone's apps.
    Apps {
        /// The apps.
        apps: Vec<AppEntry>,
        /// Their hash, checked.
        hash: String,
    },
    /// What the session's phone is (its maker and model, its Android
    /// version, LiMo's version on it), checked to be within its bounds and
    /// plain text: once after each session opens, and again when any of it
    /// changed. Keep it for the guardian; never log it.
    About {
        /// What the phone is.
        about: PhoneAbout,
    },
    /// The phone said goodbye; the link has ended.
    Bye,
}

enum Stage {
    AwaitHello,
    PairReplied { hs: Box<HandshakeState>, offer_id: OfferId, refuse: Option<Refusal> },
    PairAwaitRequest { noise: Box<TransportState>, offer_id: OfferId, phone_key: PublicKey },
    PairAsked { noise: Box<TransportState> },
    PairDone { noise: Box<TransportState>, until: u64 },
    SessionFresh { noise: Box<TransportState>, phone: KnownPhone },
    SessionOpen { noise: Box<TransportState>, phone: KnownPhone, reports_waiting: usize },
    Ended,
    Failed,
}

impl Stage {
    fn name(&self) -> &'static str {
        match self {
            Stage::AwaitHello => "waiting for the first frame",
            Stage::PairReplied { .. } => "pairing, waiting for the finish",
            Stage::PairAwaitRequest { .. } => "pairing, waiting for the request",
            Stage::PairAsked { .. } => "pairing, waiting for the caller",
            Stage::PairDone { .. } => "paired",
            Stage::SessionFresh { .. } => "session, waiting for the first message",
            Stage::SessionOpen { .. } => "session open",
            Stage::Ended => "ended",
            Stage::Failed => "failed",
        }
    }
}

/// The master's side of one connection.
pub struct MasterLink {
    keys: KeyPair,
    stage: Stage,
    reader: FrameReader,
    out: Vec<u8>,
    accepted: u64,
    last_heard: u64,
    speaks: u8,
}

impl fmt::Debug for MasterLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let device = match &self.stage {
            Stage::SessionFresh { phone, .. } | Stage::SessionOpen { phone, .. } => Some(phone.device_id.as_str()),
            _ => None,
        };
        f.debug_struct("MasterLink")
            .field("stage", &self.stage.name())
            .field("device_id", &device)
            .field("master_key", &self.keys.public())
            .finish()
    }
}

impl MasterLink {
    /// A link for a connection just accepted, with the master's own key
    /// pair.
    pub fn accept(master_keys: &KeyPair, now: Now) -> MasterLink {
        MasterLink {
            keys: master_keys.clone(),
            stage: Stage::AwaitHello,
            reader: FrameReader::default(),
            out: Vec::new(),
            accepted: now.ms,
            last_heard: now.ms,
            speaks: LINK_VERSION,
        }
    }

    /// The bytes to write to the socket now; empty when there are none.
    pub fn take_outgoing(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.out)
    }

    /// The phone of an open session, once its first message has come.
    pub fn phone(&self) -> Option<&KnownPhone> {
        match &self.stage {
            Stage::SessionOpen { phone, .. } => Some(phone),
            _ => None,
        }
    }

    /// True once the link has nothing more to do: write what
    /// [`MasterLink::take_outgoing`] gives, then close the socket.
    pub fn is_ended(&self) -> bool {
        matches!(self.stage, Stage::Ended | Stage::Failed | Stage::PairDone { .. })
    }

    /// Feeds bytes read from the socket, with what the master knows.
    /// Returns what came, in order. Any error ends the link; for an error
    /// before the handshake has finished nothing can be said to the phone,
    /// and the caller closes the socket.
    pub fn receive(&mut self, bytes: &[u8], now: Now, directory: &dyn Directory) -> Result<Vec<MasterEvent>, LinkError> {
        let mut events = Vec::new();
        match self.receive_inner(bytes, now, directory, &mut events) {
            Ok(()) => Ok(events),
            Err(error) => {
                self.stage = Stage::Failed;
                Err(error)
            }
        }
    }

    fn receive_inner(
        &mut self,
        bytes: &[u8],
        now: Now,
        directory: &dyn Directory,
        events: &mut Vec<MasterEvent>,
    ) -> Result<(), LinkError> {
        let mut input = bytes;
        while !input.is_empty() {
            let allowed = match self.stage {
                Stage::AwaitHello => Allowed::Exactly(
                    "first frame",
                    &[SESSION_PREAMBLE_BYTES + SESSION_HELLO_BYTES, PAIRING_PREAMBLE_BYTES + PAIR_HELLO_BYTES],
                ),
                Stage::PairReplied { .. } => Allowed::Exactly("pairing finish", &[PAIR_FINISH_BYTES]),
                Stage::PairAwaitRequest { .. }
                | Stage::PairAsked { .. }
                | Stage::PairDone { .. }
                | Stage::SessionFresh { .. }
                | Stage::SessionOpen { .. } => Allowed::Message,
                Stage::Ended | Stage::Failed => return Err(LinkError::Closed),
            };
            let Some(body) = self.reader.feed(&mut input, allowed)? else {
                break;
            };
            self.last_heard = now.ms;
            match std::mem::replace(&mut self.stage, Stage::Failed) {
                Stage::AwaitHello => self.hello(&body, now, directory)?,
                Stage::PairReplied { mut hs, offer_id, refuse } => {
                    handshake_read(&mut hs, &body)?;
                    let mut key = [0u8; KEY_BYTES];
                    key.copy_from_slice(hs.get_remote_static().ok_or(LinkError::Garbled)?);
                    let mut noise = Box::new(
                        hs.into_transport_mode()
                            .map_err(|_| LinkError::Internal { what: "finish a pairing handshake" })?,
                    );
                    if let Some(why) = refuse {
                        self.say(&mut noise, &MasterSays::Refused { why, speaks: LINK_VERSION })?;
                        self.stage = Stage::Ended;
                        return Ok(());
                    }
                    self.stage = Stage::PairAwaitRequest { noise, offer_id, phone_key: PublicKey::from_bytes(key) };
                }
                Stage::PairAwaitRequest { mut noise, offer_id, phone_key } => {
                    let Some(said) = self.hear(&mut noise, &body)? else {
                        self.stage = Stage::PairAwaitRequest { noise, offer_id, phone_key };
                        continue;
                    };
                    match said {
                        PhoneSays::Pair { name, form, platform, android_version } => {
                            let request = PairRequest { name, form, platform, android_version };
                            events.push(MasterEvent::PairRequest { offer_id, phone_key, request });
                            self.stage = Stage::PairAsked { noise };
                        }
                        PhoneSays::Bye {} => {
                            events.push(MasterEvent::Bye);
                            self.stage = Stage::Ended;
                        }
                        _ => return Err(LinkError::Unexpected { what: "something other than a pairing request" }),
                    }
                }
                Stage::PairAsked { mut noise } => match self.hear(&mut noise, &body)? {
                    None => self.stage = Stage::PairAsked { noise },
                    Some(PhoneSays::Bye {}) => {
                        events.push(MasterEvent::Bye);
                        self.stage = Stage::Ended;
                    }
                    Some(_) => return Err(LinkError::Unexpected { what: "a message after the pairing request" }),
                },
                Stage::PairDone { mut noise, until } => match self.hear(&mut noise, &body)? {
                    None => self.stage = Stage::PairDone { noise, until },
                    Some(PhoneSays::Bye {}) => {
                        events.push(MasterEvent::Bye);
                        self.stage = Stage::Ended;
                    }
                    Some(_) => return Err(LinkError::Unexpected { what: "a message after the pairing request" }),
                },
                Stage::SessionFresh { mut noise, phone } => {
                    // Even a message of a kind this version does not know
                    // proves the phone is live.
                    let said = self.hear(&mut noise, &body)?;
                    events.push(MasterEvent::Opened { device_id: phone.device_id.clone(), child_id: phone.child_id.clone() });
                    self.session_message(noise, phone, 0, said, now, events)?;
                }
                Stage::SessionOpen { mut noise, phone, reports_waiting } => {
                    let said = self.hear(&mut noise, &body)?;
                    self.session_message(noise, phone, reports_waiting, said, now, events)?;
                }
                Stage::Ended | Stage::Failed => return Err(LinkError::Closed),
            }
            if matches!(self.stage, Stage::Ended) && !input.is_empty() {
                return Err(LinkError::Unexpected { what: "more after the end" });
            }
        }
        Ok(())
    }

    /// The first frame: which link, which version, and the handshake's
    /// first message.
    fn hello(&mut self, body: &[u8], now: Now, directory: &dyn Directory) -> Result<(), LinkError> {
        let (preamble, message) = Preamble::read(body)?;
        let prologue = &body[..body.len() - message.len()];
        self.speaks = preamble.version;
        let too_new = preamble.version > LINK_VERSION;
        match preamble.kind {
            Kind::Pairing(offer_id) => {
                if message.len() != PAIR_HELLO_BYTES {
                    return Err(LinkError::NotLiMo);
                }
                let on_file = directory.offer(&offer_id).ok_or(LinkError::UnknownOffer)?;
                let mut hs = pairing_master(&self.keys, on_file.offer.secret(), prologue)?;
                handshake_read(&mut hs, message)?;
                let reply = handshake_write(&mut hs, PAIR_REPLY_BYTES)?;
                self.out.extend(frame(&reply));
                // The phone hears why only once the handshake has proved it
                // holds the offer's secret.
                let refuse = if too_new {
                    Some(Refusal::Version)
                } else if on_file.used {
                    Some(Refusal::OfferUsed)
                } else if on_file.offer.is_expired(now.unix) {
                    Some(Refusal::OfferExpired)
                } else {
                    None
                };
                self.stage = Stage::PairReplied { hs: Box::new(hs), offer_id, refuse };
            }
            Kind::Session => {
                if message.len() != SESSION_HELLO_BYTES {
                    return Err(LinkError::NotLiMo);
                }
                // KK names nobody in the clear: the first message decrypts
                // only for the phone whose key it was made with.
                let mut found = None;
                for phone in directory.phones() {
                    let mut hs = session_master(&self.keys, &phone.key, prologue)?;
                    if handshake_read(&mut hs, message).is_ok() {
                        found = Some((hs, phone));
                        break;
                    }
                }
                let (mut hs, phone) = found.ok_or(LinkError::UnknownPhone)?;
                let reply = handshake_write(&mut hs, SESSION_REPLY_BYTES)?;
                self.out.extend(frame(&reply));
                let mut noise = Box::new(
                    hs.into_transport_mode().map_err(|_| LinkError::Internal { what: "finish a session handshake" })?,
                );
                let refuse = if too_new {
                    Some(Refusal::Version)
                } else if phone.removed {
                    Some(Refusal::Removed)
                } else {
                    None
                };
                if let Some(why) = refuse {
                    self.say(&mut noise, &MasterSays::Refused { why, speaks: LINK_VERSION })?;
                    self.stage = Stage::Ended;
                    return Ok(());
                }
                self.stage = Stage::SessionFresh { noise, phone };
            }
        }
        Ok(())
    }

    fn session_message(
        &mut self,
        mut noise: Box<TransportState>,
        phone: KnownPhone,
        mut reports_waiting: usize,
        said: Option<PhoneSays>,
        now: Now,
        events: &mut Vec<MasterEvent>,
    ) -> Result<(), LinkError> {
        let Some(said) = said else {
            // A kind this version does not know: passed over.
            self.stage = Stage::SessionOpen { noise, phone, reports_waiting };
            return Ok(());
        };
        match said {
            PhoneSays::Ping {} => self.say(&mut noise, &MasterSays::Pong { master_now: now.unix })?,
            PhoneSays::Report { report } => {
                if report.device_id != phone.device_id || report.child_id != phone.child_id {
                    // Said before the link ends, so the phone knows why.
                    self.say(&mut noise, &MasterSays::Refused { why: Refusal::OtherDevice, speaks: LINK_VERSION })?;
                    return Err(LinkError::OtherDevice);
                }
                reports_waiting += 1;
                events.push(MasterEvent::Report { report: *report });
            }
            PhoneSays::Apps { apps, hash } => events.push(MasterEvent::Apps { apps, hash }),
            PhoneSays::About { model, android_version, app_version } => {
                events.push(MasterEvent::About { about: PhoneAbout { model, android_version, app_version } });
            }
            PhoneSays::Bye {} => {
                events.push(MasterEvent::Bye);
                self.stage = Stage::Ended;
                return Ok(());
            }
            PhoneSays::Pair { .. } => return Err(LinkError::Unexpected { what: "a pairing request on a session" }),
        }
        self.stage = Stage::SessionOpen { noise, phone, reports_waiting };
        Ok(())
    }

    /// Decrypts and reads one message from the phone, and checks its
    /// bounds; None for a message of a kind this version does not know,
    /// which is passed over.
    fn hear(&mut self, noise: &mut TransportState, body: &[u8]) -> Result<Option<PhoneSays>, LinkError> {
        let text = open(noise, body)?;
        let Some(said) = read_message::<PhoneSays>(&text)? else {
            return Ok(None);
        };
        said.check()?;
        Ok(Some(said))
    }

    /// Encrypts one message to the phone into the outgoing bytes.
    fn say(&mut self, noise: &mut TransportState, message: &MasterSays) -> Result<(), LinkError> {
        let body = seal(noise, &to_json(message)?)?;
        self.out.extend(frame(&body));
        Ok(())
    }

    /// Answers a pairing request: the hub's first grant for the new device
    /// (`Pairing::welcome`) and its code secret (`Pairing::code_secret`).
    /// The link has then done its work: write the outgoing bytes, and close
    /// the socket (the phone says `bye` first, if the caller waits for it).
    pub fn welcome(&mut self, welcome: &Grant, code_secret: &[u8], now: Now) -> Result<(), LinkError> {
        self.welcome_with_name(welcome, code_secret, None, now)
    }

    /// Answers a pairing request as [`MasterLink::welcome`] does, and tells
    /// the phone the child's name (1 to [`crate::messages::MAX_NAME_CHARS`]
    /// characters), for the child's own screens on it.
    pub fn welcome_child(&mut self, welcome: &Grant, code_secret: &[u8], child_name: &str, now: Now) -> Result<(), LinkError> {
        let chars = child_name.trim().chars().count();
        if chars == 0 || child_name.chars().count() > crate::messages::MAX_NAME_CHARS {
            return Err(LinkError::OutOfBounds { what: "a child's name that is empty or too long" });
        }
        self.welcome_with_name(welcome, code_secret, Some(child_name.to_string()), now)
    }

    fn welcome_with_name(
        &mut self,
        welcome: &Grant,
        code_secret: &[u8],
        child_name: Option<String>,
        now: Now,
    ) -> Result<(), LinkError> {
        let Stage::PairAsked { .. } = self.stage else {
            return Err(self.not_now("a welcome with no pairing request to answer"));
        };
        if code_secret.len() != CODE_SECRET_BYTES {
            return Err(LinkError::OutOfBounds { what: "a code secret of the wrong length" });
        }
        if welcome.device_id.is_empty() || welcome.child_id.is_empty() {
            return Err(LinkError::OutOfBounds { what: "a welcome with no device or child" });
        }
        let message = MasterSays::Welcome {
            welcome: welcome.clone(),
            code_secret: CodeSecret::new(code_secret),
            master_now: now.unix,
            child_name,
        };
        let Stage::PairAsked { mut noise } = std::mem::replace(&mut self.stage, Stage::Failed) else {
            return Err(LinkError::Closed);
        };
        let said = self.say(&mut noise, &message);
        match said {
            Ok(()) => {
                self.stage = Stage::PairDone { noise, until: now.ms.saturating_add(ANSWER_LIMIT_MS) };
                Ok(())
            }
            Err(error) => {
                self.stage = Stage::PairAsked { noise };
                Err(error)
            }
        }
    }

    /// Answers a report of the session's phone with the hub's grant for it.
    /// A grant for another device, or with no report waiting for it, is
    /// refused and changes nothing.
    pub fn grant(&mut self, grant: &Grant, now: Now) -> Result<(), LinkError> {
        let Stage::SessionOpen { noise, phone, reports_waiting } = &mut self.stage else {
            return Err(self.not_now("a grant with no report to answer"));
        };
        if *reports_waiting == 0 {
            return Err(LinkError::Unexpected { what: "a grant with no report to answer" });
        }
        if grant.device_id != phone.device_id || grant.child_id != phone.child_id {
            return Err(LinkError::OtherDevice);
        }
        let body = seal(noise, &to_json(&MasterSays::Grant { grant: grant.clone(), master_now: now.unix })?)?;
        *reports_waiting -= 1;
        self.out.extend(frame(&body));
        Ok(())
    }

    /// Asks the session's phone to report at once (the hub's wanted
    /// reports).
    pub fn report_now(&mut self) -> Result<(), LinkError> {
        let Stage::SessionOpen { noise, .. } = &mut self.stage else {
            return Err(self.not_now("a request to report before the session is open"));
        };
        let body = seal(noise, &to_json(&MasterSays::ReportNow {})?)?;
        self.out.extend(frame(&body));
        Ok(())
    }

    /// Refuses, with a reason, and ends the link: a pairing request the
    /// caller will not grant, or a report the hub refused (a removed or
    /// unknown device). Only possible once the handshake has finished.
    pub fn refuse(&mut self, why: Refusal) -> Result<(), LinkError> {
        let message = MasterSays::Refused { why, speaks: LINK_VERSION };
        let noise = match &mut self.stage {
            Stage::PairAsked { noise } | Stage::SessionFresh { noise, .. } | Stage::SessionOpen { noise, .. } => noise,
            _ => return Err(self.not_now("a refusal before the handshake has finished")),
        };
        let body = seal(noise, &to_json(&message)?)?;
        self.out.extend(frame(&body));
        self.stage = Stage::Ended;
        Ok(())
    }

    fn not_now(&self, what: &'static str) -> LinkError {
        match self.stage {
            Stage::Ended | Stage::Failed => LinkError::Closed,
            Stage::AwaitHello | Stage::PairReplied { .. } | Stage::PairAwaitRequest { .. } | Stage::SessionFresh { .. } => {
                LinkError::NotOpen
            }
            _ => LinkError::Unexpected { what },
        }
    }

    /// The link version the phone said it speaks, once its first frame has
    /// come.
    pub fn phone_speaks(&self) -> u8 {
        self.speaks
    }

    /// Checks the time: the handshake (with the phone's first message after
    /// it) must finish within [`HANDSHAKE_LIMIT_MS`] of the connection, and
    /// an open session ends after [`SILENCE_LIMIT_MS`] with nothing heard.
    /// Returns when to call again (`Now::ms`), or None once the link has
    /// ended.
    pub fn tick(&mut self, now: Now) -> Result<Option<u64>, LinkError> {
        let (deadline, error) = match &self.stage {
            Stage::AwaitHello | Stage::PairReplied { .. } | Stage::PairAwaitRequest { .. } | Stage::SessionFresh { .. } => {
                (self.accepted.saturating_add(HANDSHAKE_LIMIT_MS), LinkError::HandshakeTooSlow)
            }
            Stage::PairAsked { .. } | Stage::SessionOpen { .. } => {
                (self.last_heard.saturating_add(SILENCE_LIMIT_MS), LinkError::Silent)
            }
            Stage::PairDone { until, .. } => {
                if now.ms >= *until {
                    self.stage = Stage::Ended;
                    return Ok(None);
                }
                return Ok(Some(*until));
            }
            Stage::Ended => return Ok(None),
            Stage::Failed => return Err(LinkError::Closed),
        };
        if now.ms >= deadline {
            self.stage = Stage::Failed;
            return Err(error);
        }
        Ok(Some(deadline))
    }
}
