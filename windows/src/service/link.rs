//! The link with the family's phones, the service's side: every call a
//! connection makes on the family's file, answered on the service's one
//! thread between two steps; the phones' open sessions and the way each
//! came; the prods; the book every connection reads; the pairing offer the
//! console asks for; and the announcement on the home network.
//!
//! - **Between two steps.** [`Service::link`] answers what the connections
//!   asked, at most [`MAX_LINK_ANSWERS_PER_TURN`] calls a turn and at most
//!   one pairing; the service's loop calls it whenever something came and
//!   after every step, never inside one. A report is answered with the
//!   hub's grant, on the disk before it is sent (`Hub::handle_report`), and
//!   the engine is called at the next step, so the PC follows the phone's
//!   use at once.
//! - **Pairing** with an offer pairs the phone with the offer's child,
//!   keeps its key and marks the offer used in one transaction
//!   (`Hub::pair_device_with_offer`); a used, expired or unknown offer, a
//!   guardian's phone and a key the family knows are refused with the
//!   link's reasons.
//! - **A removed phone** opens its session and is told at once that it was
//!   removed, and the family's file notes that it was told
//!   (`Hub::note_released`).
//! - **One session a phone.** A newer session of a phone replaces the
//!   older, which is closed.
//! - **Prods.** The phones the engine names (`Orders::prod_devices`: the
//!   books want to hear from them) are asked to report at once, when a
//!   session of theirs is open, at most once every [`PROD_SPACING_MS`].
//! - **The announcement** goes out while the family has a phone paired, and
//!   is withdrawn when it has none, and at a stop.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, VecDeque};

use limo_hub::{HubError, PhoneApp, PhoneForm, PhoneRequest};
use limo_link::{CodeSecret, Form, KeyPair, KnownPhone, MasterId, Offer, OfferOnFile, PublicKey, Refusal};
use serde::Serialize;

use super::Service;
use crate::announce::{Announce, NoAnnouncement, what_is_announced};
use crate::os::Os;
use crate::phones::{ConnId, LinkAsk, LinkBook, LinkCall, LinkOrder, LinkServer, NoGo, Paired, Way};

/// The most calls answered in one turn between two steps.
pub const MAX_LINK_ANSWERS_PER_TURN: usize = 16;
/// The book every connection reads is read again from the family's file at
/// least this often, in milliseconds (and at once after a pairing, and after
/// a guardian's change).
pub const BOOK_EVERY_MS: u64 = 10_000;
/// A phone is asked to report at most this often, in milliseconds.
pub const PROD_SPACING_MS: u64 = 5_000;

/// A phone's open session, as the service keeps it.
#[derive(Debug, Clone)]
struct Open {
    conn: ConnId,
    way: Way,
    opened_at: i64,
    last_report_at: Option<i64>,
    prodded_at: Option<u64>,
}

/// A phone's open session, for the console and the tests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PhoneSessionView {
    /// The phone.
    pub device_id: String,
    /// How it came: `cable` or `home_network`.
    pub way: Way,
    /// When it opened, unix seconds by the PC's clock.
    pub opened_at: i64,
    /// When its newest report came, if one has.
    pub last_report_at: Option<i64>,
}

/// A pairing offer made for the console.
#[derive(Debug)]
pub(crate) struct OfferMade {
    pub(crate) offer: Offer,
}

/// Why no offer was made.
#[derive(Debug)]
pub(crate) enum OfferNotMade {
    /// The link is not running.
    NoLink,
    /// The family's file refused, such as for a child it does not have.
    Hub(HubError),
    /// The link refused, in its words.
    Link(String),
}

/// The link, as the service keeps it between steps.
pub(crate) struct Phones {
    server: Option<LinkServer>,
    announcer: Box<dyn Announce>,
    sessions: BTreeMap<String, Open>,
    waiting: VecDeque<LinkCall>,
    book_at: Option<u64>,
    book_dirty: bool,
    announced: Option<MasterId>,
}

impl Default for Phones {
    fn default() -> Phones {
        Phones {
            server: None,
            announcer: Box::new(NoAnnouncement),
            sessions: BTreeMap::new(),
            waiting: VecDeque::new(),
            book_at: None,
            book_dirty: true,
            announced: None,
        }
    }
}

impl std::fmt::Debug for Phones {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Phones").field("server", &self.server).field("sessions", &self.sessions.len()).finish_non_exhaustive()
    }
}

impl Phones {
    /// The port the link listens on, when it does.
    pub(crate) fn port(&self) -> Option<u16> {
        self.server.as_ref().map(|s| s.local_addr().port())
    }

    /// Marks the book to be read again at the next step: a guardian changed
    /// something.
    pub(crate) fn changed(&mut self) {
        self.book_dirty = true;
    }

    /// A phone's open sessions.
    pub(crate) fn sessions_of(&self, device_id: &str) -> Vec<PhoneSessionView> {
        self.sessions
            .get(device_id)
            .map(|open| PhoneSessionView {
                device_id: device_id.to_string(),
                way: open.way,
                opened_at: open.opened_at,
                last_report_at: open.last_report_at,
            })
            .into_iter()
            .collect()
    }
}

/// The link's refusal for a hub's, or None when the hub failed and the
/// connection should close with nothing said.
fn refusal_for(error: &HubError) -> Option<Refusal> {
    match error {
        HubError::OfferUsed => Some(Refusal::OfferUsed),
        HubError::OfferExpired => Some(Refusal::OfferExpired),
        HubError::UnknownOffer | HubError::UnknownDevice { .. } => Some(Refusal::Unknown),
        HubError::GuardianPhone | HubError::LinkKeyKnown => Some(Refusal::NotSupported),
        HubError::DeviceRemoved { .. } => Some(Refusal::Removed),
        HubError::DeviceOfAnotherChild { .. } | HubError::NotAPhone { .. } => Some(Refusal::OtherDevice),
        _ => None,
    }
}

impl<O: Os> Service<O> {
    /// Starts serving the link with this listener, announcing with
    /// `announcer` while the family has a phone.
    pub fn open_link(&mut self, server: LinkServer, announcer: Box<dyn Announce>) {
        let now = self.os.steady_ms();
        let text = format!("The link for the family's phones listens on port {}.", server.local_addr().port());
        self.phones.server = Some(server);
        self.phones.announcer = announcer;
        self.phones.book_dirty = true;
        self.os.log(&text);
        self.watch_link(now);
    }

    /// The phones' open sessions, for the console and the tests.
    pub fn phone_sessions(&self) -> Vec<PhoneSessionView> {
        self.phones.sessions.keys().flat_map(|device| self.phones.sessions_of(device)).collect()
    }

    /// The master id announced now, if the announcement is out.
    pub fn announced(&self) -> Option<MasterId> {
        self.phones.announced
    }

    /// Answers what the link's connections asked, between two steps: the
    /// service's loop calls it whenever something came, and after every
    /// step. At most [`MAX_LINK_ANSWERS_PER_TURN`] calls are answered in one
    /// turn, and at most one pairing. Returns how many were answered.
    pub fn link(&mut self) -> usize {
        let Some(server) = self.phones.server.as_mut() else { return 0 };
        let calls = server.calls();
        self.phones.waiting.extend(calls);
        let now = self.os.steady_ms();
        let wall = self.os.wall_secs();
        let mut answered = 0;
        let mut paired = false;
        while answered < MAX_LINK_ANSWERS_PER_TURN {
            let Some(call) = self.phones.waiting.front() else { break };
            let pairing = matches!(call.ask, LinkAsk::Pair { .. });
            if pairing && paired {
                break;
            }
            let Some(call) = self.phones.waiting.pop_front() else { break };
            paired |= pairing;
            self.on_link_call(call, now, wall);
            answered += 1;
        }
        answered
    }

    fn on_link_call(&mut self, call: LinkCall, now: u64, wall: i64) {
        let LinkCall { conn, peer, ask } = call;
        match ask {
            LinkAsk::Offer { offer_id, reply } => {
                let found = self.hub.offer(&offer_id.to_hex()).ok().flatten().and_then(|record| {
                    let offer = Offer::from_text(&record.text).ok()?;
                    Some(OfferOnFile { offer, used: record.is_used() })
                });
                let _ = reply.send(found);
            }
            LinkAsk::Pair { offer_id, phone_key, request, reply } => {
                let phone = PhoneRequest {
                    name: request.name.clone(),
                    form: match request.form {
                        Form::Child => PhoneForm::Child,
                        Form::Guardian => PhoneForm::Guardian,
                    },
                    platform: request.platform.clone(),
                    android_version: request.android_version,
                };
                let answer = match self.hub.pair_device_with_offer(&offer_id.to_hex(), phone_key.as_bytes(), &phone, wall) {
                    Ok(pairing) => {
                        let way = Way::of(&peer);
                        self.os.log(&format!(
                            "A phone was paired with a child {}, with a pairing offer (device {}).",
                            way.words(),
                            pairing.device_id
                        ));
                        self.phones.book_dirty = true;
                        Ok(Paired { welcome: pairing.welcome.clone(), code_secret: CodeSecret::new(&pairing.code_secret) })
                    }
                    Err(error) => {
                        let text = format!("A phone that asked to pair from {} was refused: {error}", peer.ip());
                        self.log_quiet(&format!("link-pair/{}/{error:?}", peer.ip()), &text, now);
                        Err(refusal_for(&error).map_or(NoGo::Close, NoGo::Refuse))
                    }
                };
                let _ = reply.send(answer);
                // The new phone's session follows at once: it must find
                // its key in the book.
                self.refresh_link_book(now);
            }
            LinkAsk::Opened { device_id, child_id, reply } => {
                let removed = match self.hub.link_phones() {
                    Ok(phones) => phones.iter().find(|p| p.device_id == device_id).map(|p| p.removed),
                    Err(_) => {
                        let _ = reply.send(Err(NoGo::Close));
                        return;
                    }
                };
                match removed {
                    Some(true) => {
                        let _ = reply.send(Err(NoGo::Refuse(Refusal::Removed)));
                        let _ = self.hub.note_released(&device_id, wall);
                        let text = format!("A phone removed from the family (device {device_id}) connected, and was told so.");
                        self.log_quiet(&format!("link-removed/{device_id}"), &text, now);
                    }
                    None => {
                        let _ = reply.send(Err(NoGo::Refuse(Refusal::Unknown)));
                    }
                    Some(false) => {
                        let way = Way::of(&peer);
                        if let Some(older) = self.phones.sessions.get(&device_id)
                            && older.conn != conn
                            && let Some(server) = &self.phones.server
                        {
                            server.order(older.conn, LinkOrder::Close);
                        }
                        self.phones.sessions.insert(
                            device_id.clone(),
                            Open { conn, way, opened_at: wall, last_report_at: None, prodded_at: None },
                        );
                        let _ = reply.send(Ok(()));
                        let text = format!("The phone {device_id} of child {child_id} opened a session {}.", way.words());
                        self.log_quiet(&format!("link-open/{device_id}/{way:?}"), &text, now);
                    }
                }
            }
            LinkAsk::Report { report, reply } => {
                let device_id = report.device_id.clone();
                let answer = match self.hub.handle_report(&report, wall) {
                    Ok(grant) => {
                        if let Some(open) = self.phones.sessions.get_mut(&device_id) {
                            open.last_report_at = Some(wall);
                        }
                        // The PC follows the phone's use at the next step.
                        self.last_tick = None;
                        Ok(Box::new(grant))
                    }
                    Err(error) => {
                        if matches!(error, HubError::DeviceRemoved { .. }) {
                            let _ = self.hub.note_released(&device_id, wall);
                        }
                        let text = format!("A report of the phone {device_id} was refused: {error}");
                        self.log_quiet(&format!("link-report/{device_id}/{error:?}"), &text, now);
                        Err(refusal_for(&error).map_or(NoGo::Close, NoGo::Refuse))
                    }
                };
                let _ = reply.send(answer);
            }
            LinkAsk::Apps { device_id, apps, hash } => {
                let apps: Vec<PhoneApp> = apps.into_iter().map(|a| PhoneApp { package: a.package, label: a.label }).collect();
                if let Err(error) = self.hub.keep_phone_apps(&device_id, &apps, &hash, wall) {
                    let text = format!("The app list of the phone {device_id} was not kept: {error}");
                    self.log_quiet(&format!("link-apps/{device_id}"), &text, now);
                }
            }
            LinkAsk::Ended { why } => {
                self.phones.sessions.retain(|_, open| open.conn != conn);
                if let Some(why) = why {
                    let text = format!("A connection to the link from {} was closed, because {why}.", peer.ip());
                    self.log_quiet(&format!("link-ended/{}/{why}", peer.ip()), &text, now);
                }
            }
        }
    }

    /// Reads the book every connection needs from the family's file: the
    /// master's keys and every phone with its key (a removed one listed as
    /// not removed, so that the service tells it itself); then announces or
    /// withdraws.
    fn refresh_link_book(&mut self, now: u64) {
        let Some(server) = &self.phones.server else { return };
        self.phones.book_at = Some(now);
        self.phones.book_dirty = false;
        let keys = self.hub.master_key().ok().map(|key| KeyPair::from_private(key.expose_private()));
        let phones = self.hub.link_phones().unwrap_or_default();
        let any_phone = phones.iter().any(|p| !p.removed);
        let known: Vec<KnownPhone> = phones
            .into_iter()
            .map(|p| KnownPhone { device_id: p.device_id, child_id: p.child_id, key: PublicKey::from_bytes(p.link_key), removed: false })
            .collect();
        let id = keys.as_ref().map(|k| MasterId::of(&k.public()));
        let port = server.local_addr().port();
        server.set_book(LinkBook { keys, phones: known });
        match id.filter(|_| any_phone) {
            Some(id) if self.phones.announced != Some(id) => match self.phones.announcer.announce(&id, port) {
                Ok(()) => {
                    self.phones.announced = Some(id);
                    self.os.log(&format!("LiMo announces itself on the home network: {}.", what_is_announced(&id, port)));
                }
                Err(problem) => {
                    let text = format!("LiMo could not announce itself on the home network: {problem}. Phones find it by its address meanwhile.");
                    self.log_quiet("link-announce", &text, now);
                }
            },
            Some(_) => {}
            None => {
                if self.phones.announced.take().is_some() {
                    self.phones.announcer.withdraw();
                    self.os.log("LiMo withdrew its announcement on the home network: the family has no phone now.");
                }
            }
        }
    }

    /// Every step: the book read again when a guardian changed something,
    /// and at least every [`BOOK_EVERY_MS`].
    pub(super) fn watch_link(&mut self, now: u64) {
        if self.phones.server.is_none() {
            return;
        }
        if self.phones.book_dirty || self.phones.book_at.is_none_or(|at| now.saturating_sub(at) >= BOOK_EVERY_MS) {
            self.refresh_link_book(now);
        }
    }

    /// Asks the phones the books want to hear from to report, when a session
    /// of theirs is open, each at most every [`PROD_SPACING_MS`].
    pub(super) fn prod_phones(&mut self, devices: &[String], now: u64) {
        let Some(server) = &self.phones.server else { return };
        for device in devices {
            let Some(open) = self.phones.sessions.get_mut(device) else { continue };
            if open.prodded_at.is_some_and(|at| now.saturating_sub(at) < PROD_SPACING_MS) {
                continue;
            }
            if server.order(open.conn, LinkOrder::ReportNow) {
                open.prodded_at = Some(now);
            }
        }
    }

    /// A pairing offer for a child: made by the link with the master's key,
    /// the link's port and the PC's home addresses, and kept by the family's
    /// file. Its text holds its secret.
    pub(crate) fn make_offer_for(&mut self, child_id: &str, wall: i64) -> Result<OfferMade, OfferNotMade> {
        let Some(port) = self.phones.port() else { return Err(OfferNotMade::NoLink) };
        self.hub.child(child_id).map_err(OfferNotMade::Hub)?;
        let key = self.hub.master_key().map_err(OfferNotMade::Hub)?;
        let keys = KeyPair::from_private(key.expose_private());
        drop(key);
        let addresses = self.os.home_addresses();
        let offer = Offer::new(keys.public(), port, addresses, wall).map_err(|e| OfferNotMade::Link(e.to_string()))?;
        self.hub
            .keep_offer(child_id, &offer.id().to_hex(), &offer.to_text(), offer.expires_at(), wall)
            .map_err(OfferNotMade::Hub)?;
        Ok(OfferMade { offer })
    }

    /// Before the service stops: the announcement withdrawn and every
    /// connection closed.
    pub(super) fn stop_link(&mut self) {
        if self.phones.announced.take().is_some() {
            self.phones.announcer.withdraw();
        }
        self.phones.server = None;
    }
}
