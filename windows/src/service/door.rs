//! The console's door, the service's side: who may come in, signing in, a
//! session for each connection, and every request answered by the family's
//! file on the service's one thread, between two of its steps.
//!
//! The pipe and its threads are Windows' part (they hand over what came
//! through the [`Os`] seam); everything decided is here, and tested against
//! the fake PC with a real family file.
//!
//! - **Who.** Windows says which account a connection's client runs in,
//!   from the client's own token ([`ClientSeen`]). An account linked to a
//!   child is refused in the guardian's words, at hello and at every
//!   request after it; so is a client whose account Windows would not say.
//! - **Signing in.** With the guardian password, through the family's own
//!   check and its waits (five wrong tries free, then a wait of a minute
//!   that doubles, up to an hour, kept across restarts). Only hello,
//!   `about`, `family_state`, `sign_in`, `sign_out` and `create_family`
//!   (while there is no family) are answered before it.
//! - **A session for each connection.** It ends with the connection, with
//!   `sign_out`, and fifteen minutes after the last request that is not a
//!   look a screen repeats by itself ([`Request::is_repeated_look`]).
//! - **Between two steps.** Requests wait in a queue and are answered by
//!   [`Service::door`], which the service's loop calls whenever something
//!   came and after every step: at most [`MAX_ANSWERS_PER_TURN`] in a turn,
//!   and at most one that carries the password or the PIN, which is hashed
//!   slowly on purpose. So a step is never late by more than one such
//!   request, and the PC's counting, which goes by the steady clock, loses
//!   nothing by it.
//! - **Hostile input.** A frame that is not a message, too long, of another
//!   version, out of turn, or more than [`MAX_MESSAGES_PER_SECOND`] in a
//!   second closes the connection, and nothing of it reaches the family's
//!   file. A request this LiMo cannot read is answered as unknown. A client
//!   that says nothing is closed after [`HELLO_WAIT_MS`]. At most
//!   [`MAX_CONNECTIONS`] are served at once.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::{Duration, Instant};

use limo_core::FREE_WRONG_GUESSES;
use limo_hub::{AccountLink, AccountStanding, HubError, PasswordOutcome, UnlinkedAccounts};
use serde::Serialize;
use serde_json::{Value, json};

use super::Service;
use crate::console::{
    CONSOLE_PROTOCOL_VERSION, FromConsole, HELLO_WAIT_MS, MAX_MESSAGES_PER_SECOND, Problem, ProblemKind, Reply, Request, ToConsole,
    closed_because, encode_to_console,
};
use crate::os::{AccountSeen, ClientSeen, DoorEvent, DoorId, Os, SendProblem};
use crate::protocol::Secret;

/// The most connections served at once. Another is refused with words.
pub const MAX_CONNECTIONS: usize = 4;
/// A session ends this long after the last request that is not a look a
/// screen repeats by itself, in milliseconds: fifteen minutes.
pub const SESSION_IDLE_MS: u64 = 15 * 60 * 1_000;
/// The least time between two requests that carry the password or the PIN
/// from one connection, in milliseconds.
pub const SECRET_SPACING_MS: u64 = 1_000;
/// The most requests answered in one turn between two steps.
pub const MAX_ANSWERS_PER_TURN: usize = 16;
/// A refused connection is closed this long after its refusal, in
/// milliseconds, so that the console can read it first.
pub const REFUSED_CLOSE_MS: u64 = 1_000;
/// After `allow_removal`, LiMo may be removed from this PC for this long, in
/// seconds: ten minutes.
pub const REMOVAL_ALLOWED_SECS: i64 = 600;
/// The most requests waiting for their turn, from every connection.
pub const MAX_WAITING: usize = 256;
/// A request that takes longer than this, in milliseconds, is logged: the
/// next step waited for it.
pub const SLOW_REQUEST_MS: u64 = 500;

/// One connection, as the service keeps it.
#[derive(Debug)]
struct Conn {
    client: ClientSeen,
    opened_at: u64,
    greeted: bool,
    /// While signed in: when the last request came that keeps the session
    /// open.
    signed_in: Option<u64>,
    /// The start of the current second, and the messages in it.
    second: (u64, u32),
    last_secret: Option<u64>,
    /// When a refused connection is closed.
    close_at: Option<u64>,
}

impl Conn {
    fn session(&self) -> String {
        match (self.client.session_id, self.client.pid) {
            (Some(session), Some(pid)) => format!("session {session} (pid {pid})"),
            (Some(session), None) => format!("session {session}"),
            (None, Some(pid)) => format!("an unknown session (pid {pid})"),
            (None, None) => "an unknown session".to_string(),
        }
    }
}

/// The longest request between two steps since the service started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DoorTimes {
    /// How long it took.
    pub longest: Duration,
    /// Which request it was.
    pub request: &'static str,
}

/// The door, as the service keeps it between steps.
#[derive(Debug, Default)]
pub(crate) struct Door {
    conns: BTreeMap<DoorId, Conn>,
    waiting: VecDeque<(DoorId, DoorEvent)>,
    removal_allowed_until: Option<i64>,
    longest: Option<DoorTimes>,
}

/// Why a request was not done: a problem in words already, or a refusal
/// of the family's file still to be put in words (and logged when it is a
/// failure).
enum Fail {
    Problem(Problem),
    Hub(HubError),
}

impl From<HubError> for Fail {
    fn from(error: HubError) -> Fail {
        Fail::Hub(error)
    }
}

/// A value in the hub's own shapes, as JSON.
fn value<T: Serialize>(thing: T) -> Result<Value, Fail> {
    serde_json::to_value(thing).map_err(|_| {
        Fail::Problem(Problem::new(ProblemKind::Failed, "LiMo could not write its answer. Nothing was changed. Please report this."))
    })
}

fn done() -> Result<Value, Fail> {
    Ok(json!({}))
}

/// What a password that was not right means for the guardian, in the
/// words of the sign in screen.
fn password_problem(outcome: PasswordOutcome) -> Problem {
    match outcome {
        PasswordOutcome::Wrong { wait_until: Some(until), .. } | PasswordOutcome::MustWait { until } => {
            let kind = if matches!(outcome, PasswordOutcome::MustWait { .. }) { ProblemKind::MustWait } else { ProblemKind::WrongPassword };
            let message = if kind == ProblemKind::MustWait {
                "Too many wrong tries. Wait before trying again."
            } else {
                "That password is not right. Too many wrong tries: wait before trying again."
            };
            Problem { wait_until: Some(until), tries_before_wait: Some(0), ..Problem::new(kind, message) }
        }
        PasswordOutcome::Wrong { wrong_guesses, wait_until: None } => {
            let left = FREE_WRONG_GUESSES.saturating_sub(wrong_guesses);
            let message = match left {
                0 => "That password is not right.".to_string(),
                1 => "That password is not right. 1 more try before a wait.".to_string(),
                n => format!("That password is not right. {n} more tries before a wait."),
            };
            Problem { tries_before_wait: Some(left), ..Problem::new(ProblemKind::WrongPassword, message) }
        }
        PasswordOutcome::Right => Problem::new(ProblemKind::Failed, "LiMo met something it did not expect. Please report this."),
    }
}

/// What a refusal of the family's file means for the guardian, and whether
/// it is a failure worth the log.
fn hub_problem(error: &HubError) -> (Problem, bool) {
    let kind = match error {
        HubError::NoFamily => ProblemKind::NoFamily,
        HubError::FamilyExists
        | HubError::Refused { .. }
        | HubError::MasterPcAlreadyPaired
        | HubError::DeviceOfAnotherChild { .. }
        | HubError::NotMasterPc { .. }
        | HubError::NoCodeSecret { .. } => ProblemKind::Refused,
        HubError::PasswordTooShort { .. }
        | HubError::PasswordTooLong { .. }
        | HubError::TextEmpty { .. }
        | HubError::TextTooLong { .. }
        | HubError::TextHasHiddenCharacters { .. }
        | HubError::ChildNameTaken { .. }
        | HubError::Code { .. }
        | HubError::PinWrongShape { .. }
        | HubError::Pin { .. }
        | HubError::DaysOutOfRange { .. }
        | HubError::SettingOutOfRange { .. } => ProblemKind::Invalid,
        HubError::UnknownChild { .. } | HubError::UnknownDevice { .. } | HubError::DeviceRemoved { .. } | HubError::UnknownAccount { .. } => {
            ProblemKind::NotFound
        }
        HubError::InvalidRules { problems } => {
            let problem = Problem {
                rule_problems: problems.clone(),
                ..Problem::new(ProblemKind::Rules, "The rules were not taken. Each problem is said beside its field.")
            };
            return (problem, false);
        }
        _ => ProblemKind::Failed,
    };
    let failed = kind == ProblemKind::Failed;
    (Problem::new(kind, error.to_string()), failed)
}

/// True when a request done changes what the PC does, so the engine is
/// called at the next step rather than within the second.
fn changes(request: &Request) -> bool {
    !matches!(
        request,
        Request::SignIn { .. }
            | Request::SignOut
            | Request::About
            | Request::FamilyState
            | Request::FamilyToday
            | Request::ChildToday { .. }
            | Request::Rules { .. }
            | Request::Usage { .. }
            | Request::WaitingRequests
            | Request::Devices { .. }
            | Request::Device { .. }
            | Request::Accounts
            | Request::PcSettings
            | Request::Events { .. }
            | Request::MakeOffer { .. }
            | Request::DeviceLink { .. }
    )
}

/// The name of what came, for the log of a slow turn.
fn event_name(event: &DoorEvent) -> &'static str {
    match event {
        DoorEvent::Message(FromConsole::Request { request, .. }) => request.name(),
        DoorEvent::Message(FromConsole::Hello { .. }) => "hello",
        _ => "a request it could not read",
    }
}

impl<O: Os> Service<O> {
    /// Answers what came to the console's door, between two steps: the
    /// service's loop calls it whenever something came, and after every
    /// step. At most [`MAX_ANSWERS_PER_TURN`] requests are answered in one
    /// turn, and at most one that carries the password or the PIN; the rest
    /// wait for the next turn. Returns how many were answered.
    pub fn door(&mut self) -> usize {
        let now = self.os.steady_ms();
        self.take_door_events(now);
        let mut answered = 0;
        let mut secret_done = false;
        while answered < MAX_ANSWERS_PER_TURN {
            let Some((_, event)) = self.door.waiting.front() else { break };
            let secret = matches!(event, DoorEvent::Message(FromConsole::Request { request, .. }) if request.carries_a_secret());
            if secret && secret_done {
                break;
            }
            let Some((id, event)) = self.door.waiting.pop_front() else { break };
            secret_done |= secret;
            let name = event_name(&event);
            let started = Instant::now();
            self.on_door_event(id, event, now);
            self.note_time(started.elapsed(), name, now);
            answered += 1;
        }
        answered
    }

    /// The longest request between two steps since the service started.
    pub fn door_times(&self) -> Option<DoorTimes> {
        self.door.longest
    }

    /// How many connections to the console's door are signed in now.
    pub fn door_sessions(&self) -> usize {
        self.door.conns.values().filter(|c| c.signed_in.is_some()).count()
    }

    /// Until when LiMo may be removed from this PC, after a guardian allowed
    /// it with the password, unix seconds.
    pub fn removal_allowed_until(&self) -> Option<i64> {
        self.door.removal_allowed_until
    }

    fn note_time(&mut self, took: Duration, name: &'static str, now: u64) {
        if self.door.longest.is_none_or(|t| took > t.longest) {
            self.door.longest = Some(DoorTimes { longest: took, request: name });
        }
        let ms = took.as_millis();
        if ms >= u128::from(SLOW_REQUEST_MS) {
            let text = format!(
                "The console's request {name} took {ms} ms between two steps of the service. The PC's counting goes by the steady clock and loses nothing by it."
            );
            self.log_quiet(&format!("door-slow/{name}"), &text, now);
        }
    }

    /// Takes what came: connections opened, closed and broken at once;
    /// messages into the queue, each connection at most
    /// [`MAX_MESSAGES_PER_SECOND`] a second.
    fn take_door_events(&mut self, now: u64) {
        for (id, event) in self.os.door_received() {
            match event {
                DoorEvent::Note(text) => {
                    let text: String = text.chars().filter(|c| !c.is_control()).take(400).collect();
                    self.log_quiet(&format!("door-note/{text}"), &text, now);
                }
                DoorEvent::Opened(client) => self.door_opened(id, client, now),
                DoorEvent::Closed => self.forget_door(id),
                DoorEvent::Broken(problem) => {
                    if self.door.conns.contains_key(&id) {
                        self.close_door(id, &closed_because(&problem), now);
                    }
                }
                DoorEvent::Message(_) | DoorEvent::BadRequest { .. } => {
                    let Some(conn) = self.door.conns.get_mut(&id) else { continue };
                    if now.saturating_sub(conn.second.0) >= 1_000 {
                        conn.second = (now, 0);
                    }
                    conn.second.1 = conn.second.1.saturating_add(1);
                    if conn.second.1 > MAX_MESSAGES_PER_SECOND {
                        let why = format!("it sent more than {MAX_MESSAGES_PER_SECOND} messages in one second");
                        self.close_door(id, &why, now);
                    } else if self.door.waiting.len() >= MAX_WAITING {
                        self.close_door(id, "more requests were waiting than LiMo takes at once", now);
                    } else {
                        self.door.waiting.push_back((id, event));
                    }
                }
            }
        }
    }

    fn door_opened(&mut self, id: DoorId, client: ClientSeen, now: u64) {
        let conn = Conn { client, opened_at: now, greeted: false, signed_in: None, second: (now, 0), last_secret: None, close_at: None };
        let session = conn.session();
        let client = conn.client.clone();
        let open = self.door.conns.values().filter(|c| c.close_at.is_none()).count();
        self.door.conns.insert(id, conn);
        // Refused at once, before hello, so that a child's account never
        // holds a place a guardian's console needs.
        if let Some((message, why)) = self.refusal(&client) {
            self.refuse(id, message, why, now);
            return;
        }
        if open >= MAX_CONNECTIONS {
            let message = format!(
                "LiMo's settings are already open in {MAX_CONNECTIONS} windows. Close one of them, then try again."
            );
            self.refuse(id, message, "as many consoles as LiMo serves at once are open", now);
            return;
        }
        let text = format!("A console connected to LiMo's settings from {session}.");
        self.log_quiet(&format!("door-open/{session}"), &text, now);
    }

    /// Forgets a connection that is gone, and what it had waiting.
    fn forget_door(&mut self, id: DoorId) {
        self.door.conns.remove(&id);
        self.door.waiting.retain(|(waiting, _)| *waiting != id);
    }

    /// Closes a connection, and says why in the log.
    fn close_door(&mut self, id: DoorId, why: &str, now: u64) {
        let session = self.door.conns.get(&id).map(Conn::session);
        self.os.door_close(id);
        self.forget_door(id);
        if let Some(session) = session {
            let text = format!("The console's connection from {session} was closed, because {why}.");
            self.log_quiet(&format!("door-closed/{session}/{why}"), &text, now);
        }
    }

    /// Refuses a connection in words for the guardian's screen, and closes
    /// it a moment later. `why` is for the log, which never names a child.
    fn refuse(&mut self, id: DoorId, message: String, why: &str, now: u64) {
        let Some(conn) = self.door.conns.get_mut(&id) else { return };
        conn.signed_in = None;
        conn.close_at = Some(now.saturating_add(REFUSED_CLOSE_MS));
        let session = conn.session();
        self.door.waiting.retain(|(waiting, _)| *waiting != id);
        if let Ok(frame) = encode_to_console(&ToConsole::Refused { message }) {
            let _ = self.os.door_send(id, frame);
        }
        let text = format!("LiMo's settings were refused to a console in {session}: {why}.");
        self.log_quiet(&format!("door-refused/{session}/{why}"), &text, now);
    }

    /// Why the door does not open for this client, if it does not: the
    /// sentence for the screen, and the words for the log.
    fn refusal(&self, client: &ClientSeen) -> Option<(String, &'static str)> {
        let Some(account) = &client.account_id else {
            return Some((
                "LiMo could not tell which Windows account this window runs in, so LiMo's settings cannot be opened from it."
                    .to_string(),
                "Windows did not say which account it runs in",
            ));
        };
        match self.hub.account_standing(account) {
            Ok(AccountStanding::Child { child_id }) => {
                let message = match self.hub.child(&child_id) {
                    Ok(child) => format!(
                        "LiMo's settings cannot be opened from {}'s account. A guardian opens them from their own.",
                        child.name
                    ),
                    Err(_) => "LiMo's settings cannot be opened from a child's account. A guardian opens them from their own.".to_string(),
                };
                Some((message, "its account is linked to a child"))
            }
            Ok(_) => None,
            Err(_) => Some((
                "LiMo's settings cannot be opened just now, because the family's file could not be read. Try again in a moment."
                    .to_string(),
                "the family's file could not be read to tell whose account it is",
            )),
        }
    }

    fn on_door_event(&mut self, id: DoorId, event: DoorEvent, now: u64) {
        let Some(conn) = self.door.conns.get_mut(&id) else { return };
        if conn.close_at.is_some() {
            return;
        }
        let greeted = conn.greeted;
        match event {
            DoorEvent::Message(FromConsole::Hello { protocol }) => {
                if greeted {
                    self.close_door(id, "it said hello twice", now);
                } else if protocol != CONSOLE_PROTOCOL_VERSION {
                    let why = format!("it said hello in version {protocol} of the console's protocol, and this LiMo speaks version {CONSOLE_PROTOCOL_VERSION}");
                    self.close_door(id, &why, now);
                } else {
                    conn.greeted = true;
                    let client = conn.client.clone();
                    if let Some((message, why)) = self.refusal(&client) {
                        self.refuse(id, message, why, now);
                    } else {
                        let hello = ToConsole::Hello { protocol: CONSOLE_PROTOCOL_VERSION, version: crate::windows_version() };
                        self.send_to_console(id, &hello, now);
                    }
                }
            }
            DoorEvent::Message(FromConsole::Request { id: ask, request }) => {
                if greeted {
                    self.on_request(id, ask, request, now);
                } else {
                    self.close_door(id, "it sent a request before saying hello", now);
                }
            }
            DoorEvent::BadRequest { id: ask } => {
                if greeted {
                    let problem = Problem::new(ProblemKind::UnknownRequest, "LiMo does not know that request, or cannot read what came with it.");
                    self.send_to_console(id, &ToConsole::Answer { id: ask, reply: Reply::Problem { problem } }, now);
                } else {
                    self.close_door(id, "it sent a request before saying hello", now);
                }
            }
            _ => {}
        }
    }

    /// Hands a message to a console. A console that does not read what it
    /// is sent is closed.
    fn send_to_console(&mut self, id: DoorId, message: &ToConsole, now: u64) {
        let frame = match encode_to_console(message) {
            Ok(frame) => frame,
            Err(_) => {
                let ToConsole::Answer { id: ask, .. } = message else { return };
                let problem = Problem::new(ProblemKind::TooBig, "The answer is longer than LiMo sends at once. Ask for less at a time.");
                match encode_to_console(&ToConsole::Answer { id: *ask, reply: Reply::Problem { problem } }) {
                    Ok(frame) => frame,
                    Err(_) => return,
                }
            }
        };
        match self.os.door_send(id, frame) {
            Ok(()) => {}
            Err(SendProblem::NotReading) => self.close_door(id, "it stopped reading what it is sent", now),
            Err(SendProblem::Gone) => self.forget_door(id),
        }
    }

    fn on_request(&mut self, id: DoorId, ask: u32, request: Request, now: u64) {
        let Some(conn) = self.door.conns.get(&id) else { return };
        let client = conn.client.clone();
        let session = conn.session();
        // Asked again at every request: an account linked to a child while
        // a console was open in it is refused from then on.
        if let Some((message, why)) = self.refusal(&client) {
            self.refuse(id, message, why, now);
            return;
        }
        let wall = self.os.wall_secs();
        let result = self.reply_to(id, &client, &request, now, wall);
        if let Request::SignIn { .. } = request {
            // Whether, never what was typed.
            let what = match &result {
                Ok(_) => "signed in with the guardian password".to_string(),
                Err(problem) if problem.kind == ProblemKind::WrongPassword => "a wrong guardian password was typed".to_string(),
                Err(problem) if problem.kind == ProblemKind::MustWait => "a sign in was not tried: a wait after wrong passwords is in force".to_string(),
                Err(problem) => format!("a sign in was not done ({:?})", problem.kind),
            };
            let text = format!("LiMo's settings in {session}: {what}.");
            self.log_quiet(&format!("door-sign-in/{session}/{what}"), &text, now);
        }
        let reply = match result {
            Ok(value) => {
                if changes(&request) {
                    // The PC follows at its next step, not within the second.
                    self.last_tick = None;
                    let text = format!("From LiMo's settings in {session}: {} done.", request.name());
                    self.os.log(&text);
                }
                Reply::Done { value }
            }
            Err(problem) => Reply::Problem { problem },
        };
        self.send_to_console(id, &ToConsole::Answer { id: ask, reply }, now);
    }

    /// The session's rules, then the request itself.
    fn reply_to(&mut self, id: DoorId, client: &ClientSeen, request: &Request, now: u64, wall: i64) -> Result<Value, Problem> {
        let Some(conn) = self.door.conns.get_mut(&id) else {
            return Err(Problem::new(ProblemKind::Failed, "The connection is gone."));
        };
        if conn.signed_in.is_some_and(|at| now.saturating_sub(at) >= SESSION_IDLE_MS) {
            conn.signed_in = None;
        }
        if !request.open_to_anyone() && conn.signed_in.is_none() {
            return Err(Problem::new(ProblemKind::SignedOut, "Signed out. Type the guardian password to go on."));
        }
        if request.carries_a_secret() {
            if conn.last_secret.is_some_and(|at| now.saturating_sub(at) < SECRET_SPACING_MS) {
                return Err(Problem::new(ProblemKind::Busy, "One thing at a time: try again in a moment."));
            }
            conn.last_secret = Some(now);
        }
        if conn.signed_in.is_some() && !request.is_repeated_look() {
            conn.signed_in = Some(now);
        }
        self.handle(id, client, request, now, wall)
    }

    fn sign_in_here(&mut self, id: DoorId, now: u64) {
        if let Some(conn) = self.door.conns.get_mut(&id) {
            conn.signed_in = Some(now);
        }
    }

    /// A refusal of the family's file as a problem for the screen; a
    /// failure is logged, with what the file said.
    fn hub_fail(&mut self, request: &str, error: &HubError) -> Problem {
        let (problem, failed) = hub_problem(error);
        if failed {
            let text = format!("The console's request {request} failed: {error:?}");
            let now = self.os.steady_ms();
            self.log_quiet(&format!("door-failed/{request}"), &text, now);
        }
        problem
    }

    /// The guardian password again, before something that cannot be undone.
    fn password_again(&mut self, password: &Secret, wall: i64) -> Result<(), Fail> {
        match self.hub.check_password(password.expose(), wall) {
            Ok(PasswordOutcome::Right) => Ok(()),
            Ok(outcome) => Err(Fail::Problem(password_problem(outcome))),
            Err(error) => Err(Fail::Hub(error)),
        }
    }

    fn handle(&mut self, id: DoorId, client: &ClientSeen, request: &Request, now: u64, wall: i64) -> Result<Value, Problem> {
        match self.handle_one(id, client, request, now, wall) {
            Ok(answer) => Ok(answer),
            Err(Fail::Problem(problem)) => Err(problem),
            Err(Fail::Hub(error)) => Err(self.hub_fail(request.name(), &error)),
        }
    }

    fn handle_one(&mut self, id: DoorId, client: &ClientSeen, request: &Request, now: u64, wall: i64) -> Result<Value, Fail> {
        match request {
            Request::SignIn { password } => {
                self.password_again(password, wall)?;
                self.sign_in_here(id, now);
                done()
            }
            Request::SignOut => {
                if let Some(conn) = self.door.conns.get_mut(&id) {
                    conn.signed_in = None;
                }
                done()
            }
            Request::About => Ok(json!({
                "version": crate::windows_version(),
                "protocol": CONSOLE_PROTOCOL_VERSION,
                "pc_name": self.config.pc_name,
            })),
            Request::FamilyState => {
                let has_family = self.hub.has_family()?;
                let has_pin = has_family && self.hub.has_pin().unwrap_or(false);
                let children = if has_family { self.hub.children().map(|c| c.len()).unwrap_or(0) } else { 0 };
                let unlinked = self.hub.pc_settings().map(|s| s.unlinked_accounts).unwrap_or(UnlinkedAccounts::Free);
                let signed_in = self.door.conns.get(&id).is_some_and(|c| c.signed_in.is_some());
                Ok(json!({
                    "has_family": has_family,
                    "has_pin": has_pin,
                    "children": children,
                    "unlinked_accounts": unlinked,
                    "signed_in": signed_in,
                }))
            }
            Request::CreateFamily { password } => {
                self.hub.create_family(password.expose(), wall)?;
                self.sign_in_here(id, now);
                done()
            }
            Request::SetPin { pin } => {
                self.hub.set_pin(pin.expose(), wall)?;
                done()
            }
            Request::ChangePassword { old, new } => match self.hub.change_password(old.expose(), new.expose(), wall)? {
                PasswordOutcome::Right => done(),
                outcome => Err(Fail::Problem(password_problem(outcome))),
            },
            Request::FamilyToday => value(self.hub.family_today(wall)?),
            Request::ChildToday { child } => value(self.hub.child_today(child, wall)?),
            Request::AddChild { name, age_band } => {
                let offset = self.os.utc_offset_secs();
                value(self.hub.add_child(name, *age_band, offset, wall)?)
            }
            Request::RenameChild { child, name } => value(self.hub.rename_child(child, name)?),
            Request::SetAgeBand { child, age_band } => value(self.hub.set_age_band(child, *age_band)?),
            Request::RemoveChild { child, password } => {
                self.password_again(password, wall)?;
                self.hub.remove_child(child)?;
                done()
            }
            Request::Rules { child } => value(self.hub.rules(child)?),
            Request::ReplaceRules { child, rules } => value(self.hub.replace_rules(child, rules.clone(), wall)?),
            Request::Usage { child, from_day, to_day } => value(self.hub.usage(child, *from_day, *to_day)?),
            Request::WaitingRequests => value(self.hub.waiting_requests(wall)?),
            Request::AnswerRequest { child, request, reply } => value(self.hub.answer_request(child, request, *reply, wall)?),
            Request::GiveExtra { child, minutes, tap_id } => value(self.hub.give_extra(child, *minutes, tap_id, wall)?),
            Request::LockNow { child, until } => value(self.hub.lock_now(child, *until, wall)?),
            Request::UnlockUntil { child, until } => value(self.hub.unlock_until(child, *until, wall)?),
            Request::Lift { child } => value(self.hub.lift(child, wall)?),
            Request::MakeCode { device, minutes } => {
                let code = self.hub.make_code(device, *minutes, wall)?;
                Ok(json!({
                    "shown": code.shown,
                    "minutes": code.minutes,
                    "day": code.day,
                    "device_id": code.device_id,
                }))
            }
            Request::Devices { child } => value(self.hub.devices(child)?),
            Request::Device { device } => value(self.hub.device(device)?),
            Request::RenameDevice { device, name } => value(self.hub.rename_device(device, name)?),
            Request::RemoveDevice { device, password } => {
                self.password_again(password, wall)?;
                self.hub.remove_device(device, wall)?;
                done()
            }
            Request::Accounts => self.account_list(client),
            Request::LinkAccount { account, link } => self.link_account(client, account, link),
            Request::UnlinkAccount { account } => {
                self.hub.unlink_account(account)?;
                done()
            }
            Request::PcSettings => value(self.hub.pc_settings()?),
            Request::SetIdleMinutes { minutes } => {
                self.hub.set_idle_minutes(*minutes)?;
                value(self.hub.pc_settings()?)
            }
            Request::SetUnlinkedAccounts { unlinked } => {
                self.hub.set_unlinked_accounts(*unlinked)?;
                value(self.hub.pc_settings()?)
            }
            Request::FinishFirstStart => {
                self.hub.set_unlinked_accounts(UnlinkedAccounts::Locked)?;
                value(self.hub.pc_settings()?)
            }
            Request::Events { limit, before } => value(self.hub.events(*limit, *before)?),
            Request::AllowRemoval { password } => {
                self.password_again(password, wall)?;
                let until = wall.saturating_add(REMOVAL_ALLOWED_SECS);
                self.door.removal_allowed_until = Some(until);
                Ok(json!({ "until": until }))
            }
            Request::DeleteFamily { password } => {
                self.password_again(password, wall)?;
                self.hub.delete_family()?;
                // Nobody is signed in to a family that is gone.
                for conn in self.door.conns.values_mut() {
                    conn.signed_in = None;
                }
                self.door.removal_allowed_until = None;
                done()
            }
            Request::MakeOffer { .. } | Request::DeviceLink { .. } => Err(Fail::Problem(Problem::new(
                ProblemKind::NotYet,
                "Adding a phone is not built into LiMo for Windows yet.",
            ))),
        }
    }

    /// Every Windows account of this PC, with its link, whether it is an
    /// administrator, whether someone is signed in to it now, and whether
    /// this window runs in it; then the links of accounts no longer on this
    /// PC, so that they can be taken away.
    fn account_list(&mut self, client: &ClientSeen) -> Result<Value, Fail> {
        let seen = self.os_accounts()?;
        let links = self.hub.accounts()?;
        let children = self.hub.children()?;
        let signed_in: BTreeSet<String> = self
            .facts
            .as_ref()
            .map(|facts| facts.sessions.iter().map(|s| s.account_id.clone()).collect())
            .unwrap_or_default();
        let link_of = |account: &str| -> Value {
            match links.iter().find(|a| a.account_id == account).map(|a| &a.link) {
                Some(AccountLink::Child { child_id }) => json!({
                    "kind": "child",
                    "child_id": child_id,
                    "child_name": children.iter().find(|c| &c.child_id == child_id).map(|c| c.name.clone()),
                }),
                Some(AccountLink::NotLimited) => json!({ "kind": "not_limited" }),
                None => json!({ "kind": "none" }),
            }
        };
        let mut rows = Vec::new();
        for account in &seen {
            let display = account.full_name.as_deref().map(str::trim).filter(|n| !n.is_empty()).unwrap_or(&account.name);
            rows.push(json!({
                "account_id": account.account_id,
                "name": account.name,
                "display_name": display,
                "is_administrator": account.is_administrator,
                "signed_in": signed_in.contains(&account.account_id),
                "on_this_pc": true,
                "this_window": client.account_id.as_deref() == Some(account.account_id.as_str()),
                "link": link_of(&account.account_id),
            }));
        }
        for gone in links.iter().filter(|l| !seen.iter().any(|a| a.account_id == l.account_id)) {
            rows.push(json!({
                "account_id": gone.account_id,
                "name": gone.display_name,
                "display_name": gone.display_name,
                "is_administrator": false,
                "signed_in": signed_in.contains(&gone.account_id),
                "on_this_pc": false,
                "this_window": false,
                "link": link_of(&gone.account_id),
            }));
        }
        Ok(Value::Array(rows))
    }

    /// Links an account of this PC. The account this window runs in is never
    /// linked to a child from here, so that a guardian is not shut out of
    /// LiMo's settings by a slip.
    fn link_account(&mut self, client: &ClientSeen, account: &str, link: &AccountLink) -> Result<Value, Fail> {
        let seen = self.os_accounts()?;
        let Some(found) = seen.iter().find(|a| a.account_id == account) else {
            return Err(Fail::Problem(Problem::new(ProblemKind::NotFound, "There is no such Windows account on this PC.")));
        };
        if matches!(link, AccountLink::Child { .. }) && client.account_id.as_deref() == Some(account) {
            return Err(Fail::Problem(Problem::new(
                ProblemKind::Refused,
                "LiMo's settings are open in this account, so it cannot be linked to a child from here: a guardian would be shut out of them. Link it from another account.",
            )));
        }
        let display = found.full_name.as_deref().map(str::trim).filter(|n| !n.is_empty()).unwrap_or(&found.name).to_string();
        value(self.hub.link_account(account, &display, link.clone())?)
    }

    /// The accounts of this PC, as Windows lists them; a failure is logged.
    fn os_accounts(&mut self) -> Result<Vec<AccountSeen>, Fail> {
        self.os.accounts().map_err(|problem| {
            let now = self.os.steady_ms();
            self.log_quiet("door-accounts", &format!("The accounts of this PC could not be listed: {problem}."), now);
            Fail::Problem(Problem::new(ProblemKind::Failed, "LiMo could not list the accounts of this PC just now. Try again in a moment."))
        })
    }

    /// Every step: a client that has not said hello within
    /// [`HELLO_WAIT_MS`] is closed, a refused one once its refusal had time
    /// to be read, and a session idle for [`SESSION_IDLE_MS`] ends.
    pub(super) fn watch_door(&mut self, now: u64) {
        let mut to_close = Vec::new();
        let mut ended = Vec::new();
        for (id, conn) in &mut self.door.conns {
            if conn.close_at.is_some_and(|at| now >= at) {
                to_close.push((*id, None));
            } else if !conn.greeted && conn.close_at.is_none() && now.saturating_sub(conn.opened_at) >= HELLO_WAIT_MS {
                to_close.push((*id, Some(format!("it did not say hello within {} seconds", HELLO_WAIT_MS / 1_000))));
            } else if conn.signed_in.is_some_and(|at| now.saturating_sub(at) >= SESSION_IDLE_MS) {
                conn.signed_in = None;
                ended.push(conn.session());
            }
        }
        for (id, why) in to_close {
            match why {
                Some(why) => self.close_door(id, &why, now),
                None => {
                    self.os.door_close(id);
                    self.forget_door(id);
                }
            }
        }
        for session in ended {
            let text = format!(
                "The session of LiMo's settings in {session} ended after {} minutes with no request.",
                SESSION_IDLE_MS / 60_000
            );
            self.os.log(&text);
        }
    }
}
