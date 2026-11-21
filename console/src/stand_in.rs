//! A stand in for the service's half of the door, for the tests and the
//! self check. The real half belongs in LiMo's service, which alone holds
//! the family's file; this one holds a password in memory and nothing else,
//! and runs in whatever account runs it.
//!
//! What it shares with the real door is everything a console can see:
//!
//! - the pipe's access list ([`DOOR_SDDL`]): LocalSystem and Administrators
//!   may do anything, an interactive user may read and write, nobody else
//!   may open it, and remote clients are refused by the pipe itself;
//! - its check of who is on the other end: the client's process id from
//!   the pipe, that process's own token, and the token the client lent the
//!   pipe, which must name the same account;
//! - the refusal of an account marked as a child's, in the screen's words;
//! - the password's waits, as the hub counts them: four wrong tries free,
//!   a minute's wait after the fifth.
//!
//! **Why not "GW" for interactive users.** For a pipe, Windows' generic
//! write right includes `FILE_APPEND_DATA`, which on a pipe means
//! `FILE_CREATE_PIPE_INSTANCE`: the right to serve the pipe too. An access
//! list that gave interactive users "GW" would let a program in a child's
//! account make a second instance of LiMo's door and answer the next
//! console that knocks. So interactive users get read, write data and write
//! attributes (`0x12018b`), never `FILE_APPEND_DATA`; the tests show both.

#![forbid(unsafe_code)]

use crate::protocol::{Answer, Refusal, Request, Secret};

/// The door's access list: LocalSystem and Administrators full control;
/// interactive users `FILE_GENERIC_READ`, `FILE_WRITE_DATA` and
/// `FILE_WRITE_ATTRIBUTES` (0x12018b), which is what a client needs and
/// nothing more (no `FILE_APPEND_DATA`, so no new instance of the pipe).
/// Protected, so nothing is inherited.
pub const DOOR_SDDL: &str = "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x12018b;;;IU)";

/// The access list that looks right and is not: "GW" carries the right to
/// make another instance of the pipe. Only the tests use it, to show the
/// hole.
pub const LOOKS_RIGHT_SDDL: &str = "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;IU)";

/// The access a client asks for: read, write data and write attributes.
/// Never `GENERIC_WRITE`, which [`DOOR_SDDL`] does not grant.
pub const CLIENT_ACCESS: u32 = 0x0012_018b;

/// The wrong try that starts the first wait, and the waits, as the rules
/// core counts them for the guardian password: four are free, the fifth
/// starts a wait of a minute, doubling after, at most an hour.
pub const FREE_WRONG_TRIES: u32 = 5;
const FIRST_WAIT_SECS: i64 = 60;
const LONGEST_WAIT_SECS: i64 = 3_600;

/// What the door saw of the client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientSeen {
    /// The client's process id, from the pipe.
    pub pid: u32,
    /// The account of that process's token, as a security identifier.
    pub process_account: Option<String>,
    /// The account of the token the client lent the pipe.
    pub lent_account: Option<String>,
}

impl ClientSeen {
    /// The client's account when both ways of asking agree, and None
    /// otherwise: a client that lent no token, or a process id that names
    /// another account than the one that opened the pipe, is not known.
    pub fn account(&self) -> Option<&str> {
        match (&self.process_account, &self.lent_account) {
            (Some(p), Some(l)) if p.eq_ignore_ascii_case(l) => Some(p),
            _ => None,
        }
    }
}

/// The stand in's state: the password, the accounts marked as children's,
/// and the wrong tries.
#[derive(Debug)]
pub struct StandIn {
    password: Secret,
    children: Vec<(String, String)>,
    wrong: u32,
    wait_until: Option<i64>,
}

impl StandIn {
    /// A family whose password is `password`, with no child's account yet.
    pub fn new(password: Secret) -> StandIn {
        StandIn { password, children: Vec::new(), wrong: 0, wait_until: None }
    }

    /// Marks a Windows account (a security identifier) as the child's
    /// called `child`.
    pub fn with_child_account(mut self, account: &str, child: &str) -> StandIn {
        self.children.push((account.to_string(), child.to_string()));
        self
    }

    /// The answer to one request from a client. A child's account is
    /// refused whatever it asks, and so is an account that cannot be told.
    pub fn answer(&mut self, client: &ClientSeen, request: &Request, now: i64) -> Answer {
        let Some(account) = client.account() else {
            return Answer::Refused { why: Refusal::UnknownAccount };
        };
        if let Some((_, child)) = self.children.iter().find(|(a, _)| a.eq_ignore_ascii_case(account)) {
            return Answer::Refused { why: Refusal::ChildAccount { child: child.clone() } };
        }
        match request {
            Request::About => Answer::About {
                service: "LiMo's door, a stand in for the tests".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                family: true,
            },
            Request::SignIn { password } => self.sign_in(password, now),
        }
    }

    fn sign_in(&mut self, typed: &Secret, now: i64) -> Answer {
        if let Some(until) = self.wait_until.filter(|u| now < *u) {
            return Answer::MustWait { until };
        }
        if same_secret(typed, &self.password) {
            self.wrong = 0;
            self.wait_until = None;
            return Answer::SignedIn;
        }
        self.wrong = self.wrong.saturating_add(1);
        let wait = wait_after_wrong_tries(self.wrong);
        if wait > 0 {
            let until = now.saturating_add(wait);
            self.wait_until = Some(self.wait_until.map_or(until, |old| old.max(until)));
            return Answer::MustWait { until: self.wait_until.unwrap_or(until) };
        }
        Answer::WrongPassword { tries_before_wait: (FREE_WRONG_TRIES - 1).saturating_sub(self.wrong) }
    }
}

/// The wait after this many wrong tries, in seconds.
fn wait_after_wrong_tries(wrong: u32) -> i64 {
    if wrong < FREE_WRONG_TRIES {
        return 0;
    }
    let doublings = wrong - FREE_WRONG_TRIES;
    if doublings >= 6 {
        return LONGEST_WAIT_SECS;
    }
    (FIRST_WAIT_SECS << doublings).min(LONGEST_WAIT_SECS)
}

#[cfg(windows)]
pub use serving::{Served, StandInServer};

#[cfg(windows)]
mod serving {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use zeroize::Zeroizing;

    use super::{CLIENT_ACCESS, ClientSeen, StandIn};
    use crate::protocol::{Answer, Decoder, Request, encode};
    use crate::win::Owned;
    use crate::win::pipe::{client_pid, create_server, lent_account, let_client_go, open_client, read_some, sddl_of, wait_for_client, write_all};
    use crate::win::token::process_account;

    /// What one connection to the stand in did.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Served {
        /// Who was on the other end, as the door saw it.
        pub client: ClientSeen,
        /// The level the client lent its token at: 1 identification, 2
        /// impersonation, 3 delegation. None when it lent none.
        pub lent_level: Option<i32>,
        /// The requests it made, by name, in order.
        pub requests: Vec<&'static str>,
        /// Frames it sent that were not requests, or bytes that were not a
        /// frame.
        pub bad_frames: usize,
        /// Bytes it sent in all.
        pub bytes: usize,
    }

    /// The stand in, serving its pipe on a thread of its own, one client at
    /// a time.
    pub struct StandInServer {
        name: String,
        stop: Arc<AtomicBool>,
        served: Arc<Mutex<Vec<Served>>>,
        sddl: Option<String>,
        thread: Option<JoinHandle<()>>,
    }

    impl StandInServer {
        /// Makes the pipe `name` with the access list `sddl` (refused when
        /// the name exists already) and starts serving it.
        pub fn start(name: &str, sddl: &str, door: StandIn) -> Result<StandInServer, u32> {
            let pipe = create_server(name, sddl, true)?;
            let sddl_seen = sddl_of(pipe.raw());
            let stop = Arc::new(AtomicBool::new(false));
            let served = Arc::new(Mutex::new(Vec::new()));
            let (stop2, served2) = (Arc::clone(&stop), Arc::clone(&served));
            let thread = std::thread::Builder::new()
                .name("limo door stand in".to_string())
                .spawn(move || serve(pipe, door, &stop2, &served2))
                .map_err(|_| 0u32)?;
            Ok(StandInServer { name: name.to_string(), stop, served, sddl: sddl_seen, thread: Some(thread) })
        }

        /// The pipe's owner and access list as Windows reads them back.
        pub fn sddl(&self) -> Option<&str> {
            self.sddl.as_deref()
        }

        /// Every connection so far, the open one too once it has said
        /// something.
        pub fn served(&self) -> Vec<Served> {
            self.served.lock().map(|s| s.clone()).unwrap_or_default()
        }

        /// The pipe's name.
        pub fn name(&self) -> &str {
            &self.name
        }
    }

    impl Drop for StandInServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            // Wakes the thread if it waits for a client.
            let _ = open_client(&self.name, CLIENT_ACCESS, Duration::from_millis(500));
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    fn now() -> i64 {
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
    }

    fn serve(pipe: Owned, mut door: StandIn, stop: &AtomicBool, served: &Mutex<Vec<Served>>) {
        loop {
            if wait_for_client(&pipe).is_err() || stop.load(Ordering::SeqCst) {
                break;
            }
            serve_one(&pipe, &mut door, served);
            let_client_go(&pipe);
            if stop.load(Ordering::SeqCst) {
                break;
            }
        }
    }

    /// Serves one connection until it closes, says something that is not a
    /// request, or is refused.
    fn serve_one(pipe: &Owned, door: &mut StandIn, served: &Mutex<Vec<Served>>) {
        let mut decoder = Decoder::new();
        let mut chunk = Zeroizing::new(vec![0u8; 16 * 1024]);
        let mut index: Option<usize> = None;
        loop {
            let read = match read_some(pipe, &mut chunk) {
                Ok(0) | Err(_) => return,
                Ok(n) => n,
            };
            let pushed = decoder.push(&chunk[..read]);
            // Who is on the other end is read once, after the first bytes:
            // Windows lends the client's token only once something was read.
            let at = *index.get_or_insert_with(|| {
                let pid = client_pid(pipe).unwrap_or(0);
                let lent = lent_account(pipe);
                let client = ClientSeen {
                    pid,
                    process_account: process_account(pid),
                    lent_account: lent.as_ref().map(|(a, _)| a.clone()),
                };
                let mut list = served.lock().unwrap_or_else(|p| p.into_inner());
                list.push(Served { client, lent_level: lent.map(|(_, l)| l), requests: Vec::new(), bad_frames: 0, bytes: 0 });
                list.len() - 1
            });
            let record = |f: &mut dyn FnMut(&mut Served)| {
                let mut list = served.lock().unwrap_or_else(|p| p.into_inner());
                if let Some(s) = list.get_mut(at) {
                    f(s);
                }
            };
            record(&mut |s| s.bytes += read);
            if pushed.is_err() {
                record(&mut |s| s.bad_frames += 1);
                return;
            }
            loop {
                match decoder.next_message::<Request>() {
                    Ok(Some(request)) => {
                        record(&mut |s| s.requests.push(request.name()));
                        let client = served.lock().unwrap_or_else(|p| p.into_inner())[at].client.clone();
                        let answer = door.answer(&client, &request, now());
                        let Ok(frame) = encode(&answer) else { return };
                        if write_all(pipe, &frame).is_err() {
                            return;
                        }
                        if matches!(answer, Answer::Refused { .. }) {
                            return;
                        }
                    }
                    Ok(None) => break,
                    Err(_) => {
                        record(&mut |s| s.bad_frames += 1);
                        return;
                    }
                }
            }
        }
    }
}

/// Compares two secrets in a time that depends on their lengths only.
fn same_secret(a: &Secret, b: &Secret) -> bool {
    let (a, b) = (a.expose().as_bytes(), b.expose().as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    const GUARDIAN: &str = "S-1-5-21-10-20-30-1001";
    const ASHA: &str = "S-1-5-21-10-20-30-1002";

    fn seen(account: &str) -> ClientSeen {
        ClientSeen { pid: 77, process_account: Some(account.to_string()), lent_account: Some(account.to_string()) }
    }

    fn door() -> StandIn {
        StandIn::new(Secret::new("Mira's long sentence".to_string())).with_child_account(ASHA, "Asha")
    }

    fn sign_in(words: &str) -> Request {
        Request::SignIn { password: Secret::new(words.to_string()) }
    }

    #[test]
    fn a_guardian_signs_in_and_a_wrong_try_says_how_many_are_left() {
        let mut door = door();
        assert!(matches!(door.answer(&seen(GUARDIAN), &Request::About, 0), Answer::About { family: true, .. }));
        assert_eq!(door.answer(&seen(GUARDIAN), &sign_in("not it"), 0), Answer::WrongPassword { tries_before_wait: 3 });
        assert_eq!(door.answer(&seen(GUARDIAN), &sign_in("Mira's long sentence"), 0), Answer::SignedIn);
        // A right password clears the count.
        assert_eq!(door.answer(&seen(GUARDIAN), &sign_in("not it"), 0), Answer::WrongPassword { tries_before_wait: 3 });
    }

    #[test]
    fn the_fifth_wrong_try_starts_a_wait_that_even_the_right_password_cannot_pass() {
        let mut door = door();
        let left: Vec<Answer> = (0..4).map(|_| door.answer(&seen(GUARDIAN), &sign_in("no"), 100)).collect();
        assert_eq!(
            left,
            [3, 2, 1, 0].map(|n| Answer::WrongPassword { tries_before_wait: n }).to_vec()
        );
        assert_eq!(door.answer(&seen(GUARDIAN), &sign_in("no"), 100), Answer::MustWait { until: 160 });
        assert_eq!(door.answer(&seen(GUARDIAN), &sign_in("Mira's long sentence"), 159), Answer::MustWait { until: 160 });
        assert_eq!(door.answer(&seen(GUARDIAN), &sign_in("no"), 160), Answer::MustWait { until: 280 });
        assert_eq!(door.answer(&seen(GUARDIAN), &sign_in("Mira's long sentence"), 280), Answer::SignedIn);
        assert_eq!(wait_after_wrong_tries(40), 3_600);
    }

    #[test]
    fn a_child_s_account_is_refused_whatever_it_asks() {
        let mut door = door();
        let refused = Answer::Refused { why: Refusal::ChildAccount { child: "Asha".to_string() } };
        assert_eq!(door.answer(&seen(ASHA), &Request::About, 0), refused);
        assert_eq!(door.answer(&seen(ASHA), &sign_in("Mira's long sentence"), 0), refused);
        // Its tries are never counted.
        assert_eq!(door.answer(&seen(GUARDIAN), &sign_in("no"), 0), Answer::WrongPassword { tries_before_wait: 3 });
    }

    #[test]
    fn an_account_that_cannot_be_told_is_refused() {
        let mut door = door();
        let unknown = Answer::Refused { why: Refusal::UnknownAccount };
        let lent_nothing = ClientSeen { lent_account: None, ..seen(GUARDIAN) };
        assert_eq!(door.answer(&lent_nothing, &Request::About, 0), unknown);
        // A process id that names another account than the token lent.
        let mixed = ClientSeen { lent_account: Some(ASHA.to_string()), ..seen(GUARDIAN) };
        assert_eq!(door.answer(&mixed, &sign_in("Mira's long sentence"), 0), unknown);
        let none = ClientSeen { pid: 1, process_account: None, lent_account: None };
        assert_eq!(door.answer(&none, &Request::About, 0), unknown);
    }

    #[test]
    fn secrets_are_compared_whole() {
        let a = Secret::new("abc".into());
        assert!(same_secret(&a, &Secret::new("abc".into())));
        assert!(!same_secret(&a, &Secret::new("abd".into())));
        assert!(!same_secret(&a, &Secret::new("abcd".into())));
        assert!(!same_secret(&a, &Secret::new(String::new())));
    }
}
