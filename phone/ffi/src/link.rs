//! The phone's side of the link for Kotlin: reading a pairing offer,
//! pairing once, and a session whenever the master is in reach. Each is the
//! link crate's own state machine behind a lock; Kotlin owns the sockets
//! and the clock, writes what `take_outgoing` gives, feeds what it reads
//! into `receive`, and calls `tick` when asked. Read the link's README for
//! the frames, the handshakes and what each promises.
//!
//! Times here are the phone's elapsed count in milliseconds
//! (`SystemClock.elapsedRealtime()`); the phone's side of the link never
//! reads the wall clock.

use std::sync::{Arc, Mutex, MutexGuard};

use limo_link::{CodeSecret, Form, Now};
use zeroize::Zeroizing;

use crate::convert;
use crate::engine::PhoneEngine;
use crate::shapes::{AppEntry, EngineError, LinkError, LinkEvent, PairRequest};

/// The link's clock from Kotlin's elapsed count.
fn now(now_ms: i64) -> Result<Now, LinkError> {
    let ms = convert::ms_in(now_ms, "the elapsed count is below zero").map_err(|what| LinkError::OutOfRange { what })?;
    Ok(Now { ms, unix: 0 })
}

/// A lock, or [`LinkError::Broken`] when an earlier call panicked while it
/// was held.
fn hold<T>(lock: &Mutex<T>) -> Result<MutexGuard<'_, T>, LinkError> {
    lock.lock().map_err(|_| LinkError::Broken)
}

/// IPv4 addresses as dotted text.
fn dotted(addresses: &[std::net::Ipv4Addr]) -> Vec<String> {
    addresses.iter().map(ToString::to_string).collect()
}

// ---------------------------------------------------------------------------

/// A pairing offer, read from its `limo1:` text. It holds the offer's
/// secret, which never leaves Rust and is wiped when the offer is closed.
#[derive(uniffi::Object)]
pub struct Offer {
    inner: limo_link::Offer,
}

impl std::fmt::Debug for Offer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.inner, f)
    }
}

#[uniffi::export]
impl Offer {
    /// Reads an offer's text, as UTF-8 bytes. Spaces and line ends around it
    /// are ignored. Refused with [`LinkError::BadOffer`], whose words never
    /// quote the text.
    ///
    /// This layer wipes its copy of the text; the text holds the offer's
    /// secret, so Kotlin should fill its own array with zeros and keep no
    /// string of it longer than it must.
    #[uniffi::constructor]
    pub fn from_text(text: Vec<u8>) -> Result<Arc<Offer>, LinkError> {
        let text = Zeroizing::new(text);
        let read = match std::str::from_utf8(&text) {
            Ok(text) => limo_link::Offer::from_text(text),
            Err(_) => Err(limo_link::OfferProblem::NotAnOffer),
        };
        let inner = read.map_err(|problem| LinkError::from(limo_link::LinkError::BadOffer(problem)))?;
        Ok(Arc::new(Offer { inner }))
    }

    /// The master's id, 16 hex characters, as it announces itself.
    pub fn master_id(&self) -> String {
        self.inner.master_id().to_hex()
    }

    /// The port the master listens on.
    pub fn port(&self) -> i32 {
        i32::from(self.inner.port())
    }

    /// The master's IPv4 addresses on the home network, best first.
    pub fn addresses(&self) -> Vec<String> {
        dotted(self.inner.addresses())
    }

    /// When the offer expires, unix seconds by the master's clock.
    pub fn expires_at(&self) -> i64 {
        self.inner.expires_at()
    }

    /// True when the offer looks old by the phone's clock. Only for saying
    /// so: the master decides, since the phone's clock may be wrong.
    pub fn looks_old(&self, wall_secs: i64) -> bool {
        self.inner.is_expired(wall_secs)
    }
}

// ---------------------------------------------------------------------------

/// What a phone keeps after pairing, read back from the bytes
/// `PhonePairing.pairedMaster` gave. It holds the phone's private key, which
/// stays in Rust and is wiped when this is closed. Keep one for as long as
/// the process runs, and start each session from it.
#[derive(uniffi::Object)]
pub struct PairedMaster {
    inner: limo_link::PairedMaster,
}

impl std::fmt::Debug for PairedMaster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.inner, f)
    }
}

#[uniffi::export]
impl PairedMaster {
    /// From the kept bytes. This layer wipes its copy; Kotlin should fill its
    /// own array with zeros once this is made.
    #[uniffi::constructor]
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Arc<PairedMaster>, LinkError> {
        let bytes = Zeroizing::new(bytes);
        let inner = limo_link::PairedMaster::from_bytes(&bytes)?;
        Ok(Arc::new(PairedMaster { inner }))
    }

    /// The child this phone belongs to.
    pub fn child_id(&self) -> String {
        self.inner.child_id.clone()
    }

    /// This phone's id at the master.
    pub fn device_id(&self) -> String {
        self.inner.device_id.clone()
    }

    /// The master's id, 16 hex characters: its announcement's `id` TXT
    /// record.
    pub fn master_id(&self) -> String {
        self.inner.master_id().to_hex()
    }

    /// The port the master listens on.
    pub fn port(&self) -> i32 {
        i32::from(self.inner.port)
    }

    /// The master's addresses from the offer, to try when discovery finds
    /// nothing.
    pub fn addresses(&self) -> Vec<String> {
        dotted(&self.inner.addresses)
    }
}

// ---------------------------------------------------------------------------

struct Pairing {
    link: limo_link::PhonePairing,
    /// What to keep, once welcomed.
    paired: Option<limo_link::PairedMaster>,
    master_now: Option<i64>,
    /// The welcome and the code secret, until the engine is made from them.
    welcome: Option<(limo_core::Grant, CodeSecret)>,
    /// The child's name the welcome gave.
    child_name: Option<String>,
}

/// The phone's side of pairing, once, with the master an offer names:
/// the handshake with the offer's secret, then the phone's request, the
/// master's welcome, and a goodbye.
#[derive(uniffi::Object)]
pub struct PhonePairing {
    inner: Mutex<Pairing>,
}

impl std::fmt::Debug for PhonePairing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PhonePairing")
    }
}

#[uniffi::export]
impl PhonePairing {
    /// Starts pairing. Makes the phone's key pair; the first frame is ready
    /// in [`PhonePairing::take_outgoing`]. Connect to the offer's addresses
    /// and port, or to what discovery found.
    #[uniffi::constructor]
    pub fn start(offer: Arc<Offer>, request: PairRequest, now_ms: i64) -> Result<Arc<PhonePairing>, LinkError> {
        let android_version = match request.android_version {
            Some(v) => Some(
                convert::count_in(v, "the Android version is below zero")
                    .map_err(|what| LinkError::OutOfRange { what })?,
            ),
            None => None,
        };
        let request =
            limo_link::PairRequest { name: request.name, form: Form::Child, platform: request.platform, android_version };
        let link = limo_link::PhonePairing::start(&offer.inner, request, now(now_ms)?)?;
        Ok(Arc::new(PhonePairing {
            inner: Mutex::new(Pairing { link, paired: None, master_now: None, welcome: None, child_name: None }),
        }))
    }

    /// The bytes to write to the socket now; empty when there are none.
    pub fn take_outgoing(&self) -> Result<Vec<u8>, LinkError> {
        Ok(hold(&self.inner)?.link.take_outgoing())
    }

    /// Feeds bytes read from the socket. True once the welcome has come (in
    /// this call): write what [`PhonePairing::take_outgoing`] gives (the
    /// goodbye), keep [`PhonePairing::paired_master`], make the engine with
    /// [`PhonePairing::make_engine`] and keep its state, then close the
    /// socket. Any error ends the pairing.
    pub fn receive(&self, bytes: Vec<u8>, now_ms: i64) -> Result<bool, LinkError> {
        let at = now(now_ms)?;
        let mut pairing = hold(&self.inner)?;
        let Some(welcomed) = pairing.link.receive(&bytes, at)? else {
            return Ok(false);
        };
        pairing.paired = Some(welcomed.paired);
        pairing.master_now = Some(welcomed.master_now);
        pairing.welcome = Some((welcomed.welcome, welcomed.code_secret));
        pairing.child_name = welcomed.child_name;
        Ok(true)
    }

    /// Checks the time. Returns when to call again (the elapsed count, in
    /// milliseconds), or None once the pairing is over. An error ends it.
    pub fn tick(&self, now_ms: i64) -> Result<Option<i64>, LinkError> {
        let at = now(now_ms)?;
        Ok(hold(&self.inner)?.link.tick(at)?.map(convert::long))
    }

    /// True once the welcome has come.
    pub fn is_done(&self) -> Result<bool, LinkError> {
        Ok(hold(&self.inner)?.link.is_done())
    }

    /// The master's clock when it welcomed the phone, unix seconds.
    pub fn master_now(&self) -> Result<Option<i64>, LinkError> {
        Ok(hold(&self.inner)?.master_now)
    }

    /// The child's name the master's welcome gave, as it came (the host
    /// cleans it before it keeps it); None before the welcome, and from a
    /// master that does not say it.
    pub fn child_name(&self) -> Result<Option<String>, LinkError> {
        Ok(hold(&self.inner)?.child_name.clone())
    }

    /// The bytes to keep for every session after this one. They hold the
    /// phone's private key: write them to the app's private storage, then
    /// fill the array with zeros, and read them back with
    /// `PairedMaster.fromBytes`.
    pub fn paired_master(&self) -> Result<Vec<u8>, LinkError> {
        let pairing = hold(&self.inner)?;
        let paired = pairing.paired.as_ref().ok_or(LinkError::NotWelcomed)?;
        let bytes = paired.to_bytes()?;
        Ok(bytes.to_vec())
    }

    /// The phone's engine, made from the welcome and the code secret, which
    /// never leave Rust; once only. Keep its state at once.
    pub fn make_engine(&self, wall_secs: i64) -> Result<Arc<PhoneEngine>, EngineError> {
        let mut pairing = self.inner.lock().map_err(|_| EngineError::Broken)?;
        let (welcome, code_secret) = pairing.welcome.take().ok_or(EngineError::NotWelcomed)?;
        let engine = limo_phone::PhoneEngine::paired(&welcome, code_secret.expose().to_vec(), wall_secs)?;
        // The link's copy of the secret is wiped as it is dropped here.
        drop(code_secret);
        Ok(PhoneEngine::wrap(engine))
    }
}

// ---------------------------------------------------------------------------

/// The phone's side of a session with the master it paired with.
#[derive(uniffi::Object)]
pub struct PhoneSession {
    inner: Mutex<limo_link::PhoneSession>,
}

impl std::fmt::Debug for PhoneSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PhoneSession")
    }
}

#[uniffi::export]
impl PhoneSession {
    /// Starts a session with the master this phone paired with. The first
    /// frame is ready in [`PhoneSession::take_outgoing`].
    #[uniffi::constructor]
    pub fn start(paired: Arc<PairedMaster>, now_ms: i64) -> Result<Arc<PhoneSession>, LinkError> {
        let session = limo_link::PhoneSession::start(&paired.inner, now(now_ms)?)?;
        Ok(Arc::new(PhoneSession { inner: Mutex::new(session) }))
    }

    /// The bytes to write to the socket now; empty when there are none.
    pub fn take_outgoing(&self) -> Result<Vec<u8>, LinkError> {
        Ok(hold(&self.inner)?.take_outgoing())
    }

    /// True while the session is open.
    pub fn is_open(&self) -> Result<bool, LinkError> {
        Ok(hold(&self.inner)?.is_open())
    }

    /// True while a report waits for its grant. If the socket closes now,
    /// call `PhoneEngine.lostContact`.
    pub fn report_waiting(&self) -> Result<bool, LinkError> {
        Ok(hold(&self.inner)?.report_waiting())
    }

    /// Feeds bytes read from the socket. Returns what came, in order. Any
    /// error ends the session: close the socket, and when it is
    /// [`LinkError::NoAnswerToReport`], or [`PhoneSession::report_waiting`]
    /// was true, call `PhoneEngine.lostContact`.
    pub fn receive(&self, bytes: Vec<u8>, now_ms: i64) -> Result<Vec<LinkEvent>, LinkError> {
        let at = now(now_ms)?;
        let events = hold(&self.inner)?.receive(&bytes, at)?;
        events.into_iter().map(convert::event_out).collect()
    }

    /// Sends a report from `PhoneEngine.outgoingReport`, as it is. Its grant
    /// comes as a [`LinkEvent`] within ten seconds, or the session ends with
    /// [`LinkError::NoAnswerToReport`]. A report for another device, or bytes
    /// that are not a report, are refused and change nothing.
    pub fn send_report(&self, report: Vec<u8>, now_ms: i64) -> Result<(), LinkError> {
        let at = now(now_ms)?;
        let report = convert::report_from(&report).ok_or(LinkError::NotAReport)?;
        Ok(hold(&self.inner)?.send_report(&report, at)?)
    }

    /// Sends the phone's app list. Nothing answers it.
    pub fn send_apps(&self, apps: Vec<AppEntry>, now_ms: i64) -> Result<(), LinkError> {
        let at = now(now_ms)?;
        let apps = apps.into_iter().map(convert::app_in).collect();
        Ok(hold(&self.inner)?.send_apps(apps, at)?)
    }

    /// Says goodbye and ends the session; write what
    /// [`PhoneSession::take_outgoing`] gives, then close the socket.
    pub fn send_bye(&self, now_ms: i64) -> Result<(), LinkError> {
        let at = now(now_ms)?;
        Ok(hold(&self.inner)?.send_bye(at)?)
    }

    /// Checks the time: pings when nothing went for a minute, and ends the
    /// session when the handshake or an answer is late. Returns when to call
    /// again (the elapsed count, in milliseconds), or None once the session
    /// has ended.
    pub fn tick(&self, now_ms: i64) -> Result<Option<i64>, LinkError> {
        let at = now(now_ms)?;
        Ok(hold(&self.inner)?.tick(at)?.map(convert::long))
    }
}
