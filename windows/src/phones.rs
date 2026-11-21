//! The service's link with the family's phones: the TCP listener, and one
//! thread for each connection, each running the link's master side
//! ([`limo_link::MasterLink`]) over its socket.
//!
//! The family's file is never touched here. Everything that needs it (an
//! offer to look up, a phone to pair, a report to answer, an app list to
//! keep, a session to note) is a [`LinkCall`] handed to the service's one
//! thread, which answers it between two of its steps; the connection waits
//! for the answer. What every connection needs to read at once (the master's
//! key pair, and the phones it may meet) is a [`LinkBook`] the service
//! keeps up to date. So the cryptography of every handshake runs on the
//! connection's own thread, and the PC's counting never waits for a phone.
//!
//! Plain sockets, no unsafe code: each connection's socket is non blocking,
//! and its thread looks at it, at the service's orders and at the link's
//! clock every [`LINK_POLL_MS`]. The link's timings ([`limo_link`]'s
//! handshake limit, silence limit) run on a [`LinkClock`], which the tests
//! make the fake PC's.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::fmt;
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, SyncSender, TryRecvError, channel, sync_channel};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use limo_core::{Grant, Report};
use limo_link::{
    AppEntry, CodeSecret, Directory, KeyPair, KnownPhone, LinkError, MasterEvent, MasterLink, Now, OfferId, OfferOnFile,
    PairRequest, PublicKey, Refusal,
};
use serde::Serialize;
use zeroize::Zeroize;

/// How often a connection's thread looks at its socket, its orders and the
/// clock, in milliseconds.
pub const LINK_POLL_MS: u64 = 10;
/// The most connections served at once; more are closed at once.
pub const MAX_LINK_CONNECTIONS: usize = 32;
/// The most connections served at once from one address; more are closed at
/// once. A family's phone has one session; a pairing and a session at once
/// is two.
pub const MAX_LINK_PER_ADDRESS: usize = 4;
/// The most calls on the family's file one connection may make in a minute
/// (a report each, an app list, a pairing). A phone reports about once a
/// minute, more while it is asked to; one that calls more is closed.
pub const MAX_CALLS_PER_MINUTE: u32 = 60;
/// How long a connection waits for the service to answer a call, in
/// milliseconds; then the link ends.
pub const CALL_WAIT_MS: u64 = 5_000;
/// How long a write may wait for a phone to read, in milliseconds.
pub const WRITE_LIMIT_MS: u64 = 5_000;

/// The time, for the link: a steady count of milliseconds and the wall
/// clock.
pub trait LinkClock: Send + Sync {
    /// Now.
    fn now(&self) -> Now;
}

/// The PC's own clocks.
#[derive(Debug)]
pub struct SystemClock {
    started: Instant,
}

impl SystemClock {
    /// The clocks, the steady count starting now.
    pub fn new() -> SystemClock {
        SystemClock { started: Instant::now() }
    }
}

impl Default for SystemClock {
    fn default() -> SystemClock {
        SystemClock::new()
    }
}

impl LinkClock for SystemClock {
    fn now(&self) -> Now {
        let ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let unix = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
        Now { ms, unix }
    }
}

/// How a phone's connection came: through the USB cable (the phone's own
/// loopback, carried to the PC's by the cable), or over the home network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Way {
    /// From the PC's loopback: the cable.
    Cable,
    /// From another address: the home network.
    HomeNetwork,
}

impl Way {
    /// The way a connection from `peer` came.
    pub fn of(peer: &SocketAddr) -> Way {
        if peer.ip().is_loopback() { Way::Cable } else { Way::HomeNetwork }
    }

    /// In words, for the log.
    pub fn words(self) -> &'static str {
        match self {
            Way::Cable => "through the cable",
            Way::HomeNetwork => "over the home network",
        }
    }
}

/// One connection, as the service names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConnId(pub u64);

/// A phone the master paired with its welcome: the first grant and the
/// device's code secret, which is wiped when dropped.
pub struct Paired {
    /// The first grant.
    pub welcome: Grant,
    /// The device's code secret.
    pub code_secret: CodeSecret,
}

impl fmt::Debug for Paired {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Paired").field("device_id", &self.welcome.device_id).finish_non_exhaustive()
    }
}

/// Why the service did not say yes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoGo {
    /// Tell the phone no, with this reason; the link ends.
    Refuse(Refusal),
    /// Say nothing and close: the family's file could not answer now. The
    /// phone takes it that the PC cannot be reached, and comes again.
    Close,
}

/// What a connection asks of the service, answered on its one thread.
#[derive(Debug)]
pub enum LinkAsk {
    /// An offer this master made, by its id.
    Offer {
        /// Its id.
        offer_id: OfferId,
        /// Where the answer goes.
        reply: SyncSender<Option<OfferOnFile>>,
    },
    /// A phone that holds an offer asks to pair.
    Pair {
        /// The offer.
        offer_id: OfferId,
        /// The phone's static key.
        phone_key: PublicKey,
        /// What it says about itself.
        request: PairRequest,
        /// Where the answer goes.
        reply: SyncSender<Result<Paired, NoGo>>,
    },
    /// A known phone's session opened. Refused when the phone was removed.
    Opened {
        /// The phone.
        device_id: String,
        /// Its child.
        child_id: String,
        /// Where the answer goes.
        reply: SyncSender<Result<(), NoGo>>,
    },
    /// A report from the session's phone.
    Report {
        /// The report, checked by the link to be the session's own.
        report: Box<Report>,
        /// Where the answer goes.
        reply: SyncSender<Result<Box<Grant>, NoGo>>,
    },
    /// The phone's apps.
    Apps {
        /// The phone.
        device_id: String,
        /// The apps.
        apps: Vec<AppEntry>,
        /// Their hash, checked by the link.
        hash: String,
    },
    /// The connection ended, and why when it was not a goodbye, in plain
    /// words for the log.
    Ended {
        /// Why, when something was wrong.
        why: Option<String>,
    },
}

/// A call from a connection.
#[derive(Debug)]
pub struct LinkCall {
    /// The connection.
    pub conn: ConnId,
    /// Where it came from.
    pub peer: SocketAddr,
    /// What it asks.
    pub ask: LinkAsk,
}

/// What the service tells a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkOrder {
    /// Ask the phone to report at once.
    ReportNow,
    /// End the link and close the socket: a newer session of the same phone
    /// took its place, or the service is stopping.
    Close,
}

/// What every connection reads at once: the master's key pair (none while
/// there is no family) and the phones it may meet.
#[derive(Debug, Clone, Default)]
pub struct LinkBook {
    /// The master's key pair, from the family's file.
    pub keys: Option<KeyPair>,
    /// Every phone paired with an offer. A removed phone is listed as not
    /// removed: its session opens, and the service tells it itself, so
    /// that the family's file can note that it was told.
    pub phones: Vec<KnownPhone>,
}

/// The counts of connections served, in all and by address.
#[derive(Default)]
struct Live {
    total: usize,
    by_address: HashMap<IpAddr, usize>,
}

#[derive(Clone, Default)]
struct Counts(Arc<Mutex<Live>>);

impl Counts {
    fn lock(&self) -> MutexGuard<'_, Live> {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn take(&self, address: IpAddr) -> bool {
        let mut live = self.lock();
        let of_address = live.by_address.get(&address).copied().unwrap_or(0);
        if live.total >= MAX_LINK_CONNECTIONS || of_address >= MAX_LINK_PER_ADDRESS {
            return false;
        }
        live.total += 1;
        *live.by_address.entry(address).or_insert(0) += 1;
        true
    }

    fn give_back(&self, address: IpAddr) {
        let mut live = self.lock();
        live.total = live.total.saturating_sub(1);
        if let Some(count) = live.by_address.get_mut(&address) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                live.by_address.remove(&address);
            }
        }
    }
}

/// What the threads share.
#[derive(Clone)]
struct Shared {
    book: Arc<RwLock<LinkBook>>,
    calls: Sender<LinkCall>,
    orders: Arc<Mutex<HashMap<ConnId, Sender<LinkOrder>>>>,
    stop: Arc<AtomicBool>,
    clock: Arc<dyn LinkClock>,
    wake: Arc<dyn Fn() + Send + Sync>,
    counts: Counts,
}

impl Shared {
    fn call(&self, conn: ConnId, peer: SocketAddr, ask: LinkAsk) {
        let _ = self.calls.send(LinkCall { conn, peer, ask });
        (self.wake)();
    }

    fn orders(&self) -> MutexGuard<'_, HashMap<ConnId, Sender<LinkOrder>>> {
        self.orders.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The master's records as one connection's link needs them: the phones
/// from the book, and an offer by asking the service.
struct ConnDirectory<'a> {
    shared: &'a Shared,
    conn: ConnId,
    peer: SocketAddr,
}

impl Directory for ConnDirectory<'_> {
    fn offer(&self, id: &OfferId) -> Option<OfferOnFile> {
        let (reply, answer) = sync_channel(1);
        self.shared.call(self.conn, self.peer, LinkAsk::Offer { offer_id: *id, reply });
        answer.recv_timeout(Duration::from_millis(CALL_WAIT_MS)).ok().flatten()
    }

    fn phones(&self) -> Vec<KnownPhone> {
        self.shared.book.read().map(|book| book.phones.clone()).unwrap_or_default()
    }
}

/// One connection's thread.
struct Connection {
    conn: ConnId,
    peer: SocketAddr,
    stream: TcpStream,
    orders: Receiver<LinkOrder>,
    shared: Shared,
    /// The start of the current minute, and the calls in it.
    minute: (u64, u32),
}

/// How a connection ended.
enum End {
    /// Quietly: a goodbye, the phone closing, a pairing done, a close order.
    Quiet,
    /// Something was wrong, in plain words for the log.
    Problem(String),
}

impl Connection {
    fn run(mut self) {
        let end = self.serve();
        let why = match end {
            End::Quiet => None,
            End::Problem(why) => Some(why),
        };
        self.shared.orders().remove(&self.conn);
        self.shared.call(self.conn, self.peer, LinkAsk::Ended { why });
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
        self.shared.counts.give_back(self.peer.ip());
    }

    /// Writes all of `bytes`, waiting while the phone reads.
    fn write_all(&mut self, mut bytes: &[u8]) -> Result<(), End> {
        let give_up = Instant::now() + Duration::from_millis(WRITE_LIMIT_MS);
        while !bytes.is_empty() {
            match self.stream.write(bytes) {
                Ok(0) => return Err(End::Quiet),
                Ok(n) => bytes = &bytes[n..],
                Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::Interrupted => {
                    if Instant::now() > give_up {
                        return Err(End::Problem("it stopped reading what it is sent".to_string()));
                    }
                    std::thread::sleep(Duration::from_millis(LINK_POLL_MS));
                }
                Err(_) => return Err(End::Quiet),
            }
        }
        Ok(())
    }

    /// Counts one call on the family's file; refused past the minute's.
    fn count_call(&mut self, now: Now) -> Result<(), End> {
        if now.ms.saturating_sub(self.minute.0) >= 60_000 {
            self.minute = (now.ms, 0);
        }
        self.minute.1 += 1;
        if self.minute.1 > MAX_CALLS_PER_MINUTE {
            return Err(End::Problem(format!("it asked more than {MAX_CALLS_PER_MINUTE} times in a minute")));
        }
        Ok(())
    }

    /// Asks the service and waits for its answer.
    fn ask<T>(&self, make: impl FnOnce(SyncSender<T>) -> LinkAsk) -> Result<T, End> {
        let (reply, answer) = sync_channel(1);
        self.shared.call(self.conn, self.peer, make(reply));
        answer
            .recv_timeout(Duration::from_millis(CALL_WAIT_MS))
            .map_err(|_| End::Problem("the service did not answer in time".to_string()))
    }

    fn serve(&mut self) -> End {
        if self.stream.set_nonblocking(true).is_err() {
            return End::Problem("its socket could not be set up".to_string());
        }
        let _ = self.stream.set_nodelay(true);
        let keys = self.shared.book.read().ok().and_then(|book| book.keys.clone());
        let Some(keys) = keys else {
            return End::Problem("there is no family on this PC yet, so no phone can be served".to_string());
        };
        let mut link = MasterLink::accept(&keys, self.shared.clock.now());
        drop(keys);
        let mut buffer = vec![0u8; 16 * 1024];
        let end = loop {
            if self.shared.stop.load(Ordering::SeqCst) {
                break End::Quiet;
            }
            match self.orders.try_recv() {
                Ok(LinkOrder::Close) | Err(TryRecvError::Disconnected) => break End::Quiet,
                Ok(LinkOrder::ReportNow) => {
                    let _ = link.report_now();
                }
                Err(TryRecvError::Empty) => {}
            }
            let now = self.shared.clock.now();
            let read = match self.stream.read(&mut buffer) {
                Ok(0) => break End::Quiet,
                Ok(n) => n,
                Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::Interrupted => 0,
                Err(_) => break End::Quiet,
            };
            if read > 0 {
                let directory = ConnDirectory { shared: &self.shared, conn: self.conn, peer: self.peer };
                let events = link.receive(&buffer[..read], now, &directory);
                buffer[..read].zeroize();
                match events {
                    Ok(events) => {
                        if let Err(end) = self.act(&mut link, events, now) {
                            let out = link.take_outgoing();
                            let _ = self.write_all(&out);
                            break end;
                        }
                    }
                    Err(error) => {
                        let out = link.take_outgoing();
                        let _ = self.write_all(&out);
                        break End::Problem(link_problem(&error));
                    }
                }
            }
            let ticked = link.tick(now);
            let out = link.take_outgoing();
            if !out.is_empty()
                && let Err(end) = self.write_all(&out)
            {
                break end;
            }
            match ticked {
                Err(error) => break End::Problem(link_problem(&error)),
                Ok(None) => break End::Quiet,
                Ok(Some(_)) if link.is_ended() => break End::Quiet,
                Ok(Some(_)) => {}
            }
            if read == 0 {
                std::thread::sleep(Duration::from_millis(LINK_POLL_MS));
            }
        };
        buffer.zeroize();
        end
    }

    /// What came from the phone, acted on with the service's answers.
    fn act(&mut self, link: &mut MasterLink, events: Vec<MasterEvent>, now: Now) -> Result<(), End> {
        for event in events {
            match event {
                MasterEvent::PairRequest { offer_id, phone_key, request } => {
                    self.count_call(now)?;
                    let sent = match self.ask(|reply| LinkAsk::Pair { offer_id, phone_key, request, reply })? {
                        Ok(paired) => link.welcome(&paired.welcome, paired.code_secret.expose(), now),
                        Err(no) => refuse(link, no)?,
                    };
                    sent.map_err(|e| End::Problem(link_problem(&e)))?;
                }
                MasterEvent::Opened { device_id, child_id } => {
                    if let Err(no) = self.ask(|reply| LinkAsk::Opened { device_id, child_id, reply })? {
                        refuse(link, no)?.map_err(|e| End::Problem(link_problem(&e)))?;
                    }
                }
                MasterEvent::Report { report } => {
                    self.count_call(now)?;
                    let sent = match self.ask(|reply| LinkAsk::Report { report: Box::new(report), reply })? {
                        Ok(grant) => link.grant(&grant, now),
                        Err(no) => refuse(link, no)?,
                    };
                    sent.map_err(|e| End::Problem(link_problem(&e)))?;
                }
                MasterEvent::Apps { apps, hash } => {
                    self.count_call(now)?;
                    if let Some(phone) = link.phone() {
                        let device_id = phone.device_id.clone();
                        self.shared.call(self.conn, self.peer, LinkAsk::Apps { device_id, apps, hash });
                    }
                }
                MasterEvent::Bye => return Err(End::Quiet),
            }
        }
        Ok(())
    }
}

/// The service's no: a refusal said to the phone, or the end of the link
/// with nothing said.
fn refuse(link: &mut MasterLink, no: NoGo) -> Result<Result<(), LinkError>, End> {
    match no {
        NoGo::Refuse(why) => Ok(link.refuse(why)),
        NoGo::Close => Err(End::Problem("the family's file could not answer just now".to_string())),
    }
}

/// Why a link ended, in words for the log: the link's own, which never
/// quote what was sent.
fn link_problem(error: &LinkError) -> String {
    let text = error.to_string();
    text.trim_end().trim_end_matches('.').to_string()
}

/// The thread that takes each connection.
struct Acceptor {
    listener: TcpListener,
    shared: Shared,
}

impl Acceptor {
    fn run(self) {
        let mut next: u64 = 0;
        while !self.shared.stop.load(Ordering::SeqCst) {
            match self.listener.accept() {
                Ok((stream, peer)) => {
                    if !self.shared.counts.take(peer.ip()) {
                        // Closed at once, before anything is read.
                        drop(stream);
                        continue;
                    }
                    next += 1;
                    let conn = ConnId(next);
                    let (order_tx, orders) = channel();
                    self.shared.orders().insert(conn, order_tx);
                    let connection = Connection { conn, peer, stream, orders, shared: self.shared.clone(), minute: (0, 0) };
                    let shared = self.shared.clone();
                    if std::thread::Builder::new().name("limo link connection".to_string()).spawn(move || connection.run()).is_err()
                    {
                        shared.orders().remove(&conn);
                        shared.counts.give_back(peer.ip());
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::Interrupted => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(_) => std::thread::sleep(Duration::from_millis(200)),
            }
        }
    }
}

/// The listener and its connections. Dropping it ends them all.
pub struct LinkServer {
    local: SocketAddr,
    calls: Receiver<LinkCall>,
    shared: Shared,
    acceptor: Option<JoinHandle<()>>,
}

impl fmt::Debug for LinkServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinkServer").field("local", &self.local).finish_non_exhaustive()
    }
}

impl LinkServer {
    /// Listens on `bind` (on the PC, every IPv4 address at port 47810; in
    /// the tests, the loopback address at a port the system chooses).
    /// `wake` is called, on another thread, whenever a connection asks
    /// something, so that the service answers it at once.
    pub fn start(bind: SocketAddr, clock: Arc<dyn LinkClock>, wake: Arc<dyn Fn() + Send + Sync>) -> std::io::Result<LinkServer> {
        let listener = TcpListener::bind(bind)?;
        listener.set_nonblocking(true)?;
        let local = listener.local_addr()?;
        let (calls_tx, calls) = channel();
        let shared = Shared {
            book: Arc::new(RwLock::new(LinkBook::default())),
            calls: calls_tx,
            orders: Arc::new(Mutex::new(HashMap::new())),
            stop: Arc::new(AtomicBool::new(false)),
            clock,
            wake,
            counts: Counts::default(),
        };
        let acceptor = Acceptor { listener, shared: shared.clone() };
        let acceptor = std::thread::Builder::new().name("limo link".to_string()).spawn(move || acceptor.run())?;
        Ok(LinkServer { local, calls, shared, acceptor: Some(acceptor) })
    }

    /// The address it listens on.
    pub fn local_addr(&self) -> SocketAddr {
        self.local
    }

    /// Everything the connections asked since the last call.
    pub fn calls(&mut self) -> Vec<LinkCall> {
        self.calls.try_iter().collect()
    }

    /// Hands an order to a connection; false when it is gone.
    pub fn order(&self, conn: ConnId, order: LinkOrder) -> bool {
        self.shared.orders().get(&conn).is_some_and(|tx| tx.send(order).is_ok())
    }

    /// Replaces what every connection reads: the master's keys and the
    /// phones. New connections and new handshakes see it at once.
    pub fn set_book(&self, book: LinkBook) {
        if let Ok(mut held) = self.shared.book.write() {
            *held = book;
        }
    }

    /// How many connections are served now.
    pub fn connections(&self) -> usize {
        self.shared.counts.lock().total
    }
}

impl Drop for LinkServer {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        for order in self.shared.orders().values() {
            let _ = order.send(LinkOrder::Close);
        }
        if let Some(acceptor) = self.acceptor.take() {
            let _ = acceptor.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_connection_from_loopback_came_through_the_cable() {
        assert_eq!(Way::of(&"127.0.0.1:5000".parse().unwrap()), Way::Cable);
        assert_eq!(Way::of(&"192.168.1.20:5000".parse().unwrap()), Way::HomeNetwork);
        assert_eq!(serde_json::to_string(&Way::HomeNetwork).unwrap(), "\"home_network\"");
    }

    #[test]
    fn connections_are_counted_in_all_and_by_address() {
        let counts = Counts::default();
        let a: IpAddr = "192.168.1.20".parse().unwrap();
        let b: IpAddr = "192.168.1.21".parse().unwrap();
        for _ in 0..MAX_LINK_PER_ADDRESS {
            assert!(counts.take(a));
        }
        assert!(!counts.take(a), "one address has its most");
        assert!(counts.take(b));
        counts.give_back(a);
        assert!(counts.take(a));
        assert_eq!(counts.lock().total, MAX_LINK_PER_ADDRESS + 1);
    }

    #[test]
    fn the_system_clock_counts_on() {
        let clock = SystemClock::new();
        let first = clock.now();
        assert!(first.unix > 1_700_000_000);
        assert!(clock.now().ms >= first.ms);
    }
}
