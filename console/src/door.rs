//! The console's half of the door: who must be on the other end before the
//! console says anything, the connection, and what a guardian is told when
//! the door cannot be asked.
//!
//! Any program on the PC can make a named pipe, a child's too, and could
//! make one called `limo.console` before LiMo's service does, or while it is
//! stopped. So before the console sends anything, a password above all, it
//! asks Windows who made the pipe it reached:
//!
//! - the process that serves it runs in session 0, where only services run
//!   (`GetNamedPipeServerSessionId`);
//! - the pipe belongs to LocalSystem, the account LiMo's service runs as,
//!   or to the Administrators group (the pipe's owner, from its security
//!   descriptor). Windows gives what a LocalSystem process makes to that
//!   group, not to LocalSystem itself, because the group is the default
//!   owner in LocalSystem's token: the installed service's door reads back
//!   as the group's, which is how this was found;
//! - that process's program file is LiMo's own, in `Program Files\LiMo`
//!   (`GetNamedPipeServerProcessId`, then `QueryFullProcessImageNameW`).
//!
//! The first two are what a program in a child's account, or in any
//! account that is not an administrator, cannot fake: making a process in
//! session 0, or a pipe that belongs to LocalSystem or to the
//! Administrators group, takes an administrator who has said yes to
//! Windows' own question: until then an administrator's programs may not
//! make anything in that group's name. The third is checked whenever Windows lets the guardian's
//! account read the service's program file; when it does not (a standard
//! account may not be allowed to look into a LocalSystem process), the
//! first two decide, and the console says so in its log. If one check
//! fails, the console closes the pipe having written nothing.
//!
//! The console opens the pipe at the identification level only, so whatever
//! serves it can learn who the guardian is but can never act as the
//! guardian.

#![forbid(unsafe_code)]

use std::fmt;

/// LocalSystem's security identifier: the account LiMo's service runs as.
pub const LOCAL_SYSTEM: &str = "S-1-5-18";

/// The Administrators group's security identifier: the owner Windows gives
/// to what a LocalSystem process makes, unless the process names another.
pub const ADMINISTRATORS: &str = "S-1-5-32-544";

/// Who must have made the door.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerMustBe {
    /// The Windows session it runs in: 0 for a service.
    pub session: u32,
    /// The account that owns the pipe, as a security identifier.
    pub owner: String,
    /// The full path of its program file.
    pub image: String,
}

impl ServerMustBe {
    /// LiMo's service: session 0, LocalSystem, `limo.exe` in LiMo's folder
    /// under `program_files` (Windows' own Program Files folder, read from
    /// Windows, never from an environment variable a person could change).
    pub fn limo_service(program_files: &str) -> ServerMustBe {
        let folder = program_files.trim_end_matches(['\\', '/']);
        ServerMustBe { session: 0, owner: LOCAL_SYSTEM.to_string(), image: format!(r"{folder}\LiMo\limo.exe") }
    }

    /// True when a pipe that belongs to `owner` can be this server's: the
    /// account itself, and for LocalSystem the Administrators group too,
    /// since that is whose name Windows puts on what LocalSystem makes.
    pub fn owner_fits(&self, owner: &str) -> bool {
        owner.eq_ignore_ascii_case(&self.owner)
            || (self.owner.eq_ignore_ascii_case(LOCAL_SYSTEM) && owner.eq_ignore_ascii_case(ADMINISTRATORS))
    }
}

/// What the console saw of the other end. None where Windows would not
/// say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerSeen {
    /// Its process id.
    pub pid: u32,
    /// Its session.
    pub session: Option<u32>,
    /// The pipe's owner, as a security identifier.
    pub owner: Option<String>,
    /// Its program file, when this account may read it.
    pub image: Option<String>,
}

/// Why the other end is not LiMo's service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotTheService {
    /// It runs in another session, or Windows would not say which.
    Session {
        /// The session it runs in.
        seen: Option<u32>,
    },
    /// The pipe belongs to another account, or Windows would not say which.
    Owner,
    /// Its program file is another one.
    Image,
}

impl fmt::Display for NotTheService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NotTheService::Session { seen: Some(s) } => write!(f, "it runs in session {s}, not in the services' session"),
            NotTheService::Session { seen: None } => write!(f, "Windows would not say which session it runs in"),
            NotTheService::Owner => write!(f, "the pipe does not belong to the account LiMo's service runs as"),
            NotTheService::Image => write!(f, "its program file is not LiMo's"),
        }
    }
}

/// How the program file was checked, when the server passed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageCheck {
    /// Read, and LiMo's own.
    Matched,
    /// This account may not read it; the session and the owner decided.
    Unreadable,
}

/// Checks the other end, in the order that costs least: the session, the
/// owner, the program file.
pub fn check_server(seen: &ServerSeen, must: &ServerMustBe) -> Result<ImageCheck, NotTheService> {
    if seen.session != Some(must.session) {
        return Err(NotTheService::Session { seen: seen.session });
    }
    if !seen.owner.as_deref().is_some_and(|o| must.owner_fits(o)) {
        return Err(NotTheService::Owner);
    }
    match seen.image.as_deref() {
        Some(image) if same_path(image, &must.image) => Ok(ImageCheck::Matched),
        Some(_) => Err(NotTheService::Image),
        None => Ok(ImageCheck::Unreadable),
    }
}

/// True when two Windows paths name the same file as written: case does
/// not matter, either slash will do, and a `\\?\` in front is ignored.
pub fn same_path(a: &str, b: &str) -> bool {
    fn plain(p: &str) -> String {
        let p = p.strip_prefix(r"\\?\").unwrap_or(p);
        p.replace('/', "\\").trim_end_matches('\\').to_lowercase()
    }
    plain(a) == plain(b)
}

/// Why the console could not ask the door. Each has the words the screen
/// shows a guardian.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DoorProblem {
    /// Nothing serves the door: the service is not running.
    NotRunning,
    /// Every instance of the door stayed busy.
    Busy,
    /// Something else serves it; nothing was sent.
    NotTheService(NotTheService),
    /// The door stopped answering, closed, or answered with something that
    /// is not the door's.
    Lost(String),
    /// The page asked faster than the door takes: nothing was sent.
    TooFast,
}

impl DoorProblem {
    /// A short code for the page.
    pub fn code(&self) -> &'static str {
        match self {
            DoorProblem::NotRunning => "not_running",
            DoorProblem::Busy => "busy",
            DoorProblem::NotTheService(_) => "not_the_service",
            DoorProblem::Lost(_) => "lost",
            DoorProblem::TooFast => "too_fast",
        }
    }

    /// What the screen says to a guardian.
    pub fn words(&self) -> &'static str {
        match self {
            DoorProblem::NotRunning => "LiMo’s service is not running on this PC, so its settings cannot open.",
            DoorProblem::Busy => "LiMo’s service is busy. Try again in a moment.",
            DoorProblem::NotTheService(_) => {
                "Something that is not LiMo’s service answered, so the console sent it nothing. Your password was not sent."
            }
            DoorProblem::Lost(_) => "LiMo’s service stopped answering. Try again: the console asks it afresh, and you sign in again.",
            DoorProblem::TooFast => "The settings asked LiMo’s service too often at once. Try again in a moment.",
        }
    }
}

impl fmt::Display for DoorProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DoorProblem::NotRunning => write!(f, "nothing serves the door"),
            DoorProblem::Busy => write!(f, "every instance of the door stayed busy"),
            DoorProblem::NotTheService(why) => write!(f, "the door is not LiMo's service: {why}"),
            DoorProblem::Lost(why) => write!(f, "the door was lost: {why}"),
            DoorProblem::TooFast => write!(f, "the page asked faster than the door takes"),
        }
    }
}

impl std::error::Error for DoorProblem {}

/// What became of a request that reached the door.
#[derive(Debug, Clone, PartialEq)]
pub enum DoorReply {
    /// The door's reply, as it came: `{"outcome":"done","value":...}` or
    /// `{"outcome":"problem","problem":{...}}`.
    Reply(serde_json::Value),
    /// The door does not open for this connection, in its own sentence for
    /// the screen (such as a child's account). The connection is closed.
    Refused(String),
}

/// The most messages the console sends in one second. The door closes a
/// connection that sends more than 20; a page that asks too often is told
/// so instead, and nothing is sent.
pub const MAX_SENT_PER_SECOND: usize = 15;

#[cfg(windows)]
pub use connection::Door;

#[cfg(windows)]
mod connection {
    use std::collections::VecDeque;
    use std::time::{Duration, Instant};

    use super::{DoorProblem, DoorReply, ImageCheck, MAX_SENT_PER_SECOND, ServerMustBe, ServerSeen, check_server};
    use crate::protocol::{Decoder, FromDoor, PROTOCOL_VERSION, PageRequest, encode, hello};
    use crate::win::Owned;
    use crate::win::pipe::{CLIENT_ACCESS, OpenProblem, open_client, read_some, server_seen, waiting_bytes, write_all};

    /// How long to wait for a free instance of the door.
    const OPEN_WAIT: Duration = Duration::from_secs(2);
    /// How long the hello may take.
    const HELLO_WAIT: Duration = Duration::from_secs(5);
    /// How long an answer may take before the door counts as lost: a sign
    /// in is hashed slowly on purpose, and waits behind others'.
    const ANSWER_WAIT: Duration = Duration::from_secs(20);
    /// How often to look for an answer while waiting.
    const LOOK_EVERY: Duration = Duration::from_millis(2);

    /// The console's end of the door. It keeps one connection, since a
    /// sign in belongs to a connection; a lost one is dropped, and the next
    /// request opens a new one, signed out.
    pub struct Door {
        name: String,
        must: ServerMustBe,
        open: Option<Open>,
        next_id: u32,
        sent: VecDeque<Instant>,
        notes: Vec<String>,
        answer_wait: Duration,
    }

    struct Open {
        pipe: Owned,
        decoder: Decoder,
        seen: ServerSeen,
    }

    impl std::fmt::Debug for Door {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Door").field("name", &self.name).field("open", &self.open.is_some()).finish()
        }
    }

    impl Door {
        /// A door at the pipe `name`, which only a server that passes `must`
        /// is told anything through.
        pub fn new(name: &str, must: ServerMustBe) -> Door {
            Door { name: name.to_string(), must, open: None, next_id: 0, sent: VecDeque::new(), notes: Vec::new(), answer_wait: ANSWER_WAIT }
        }

        /// The same door with a shorter wait for an answer: for the tests,
        /// which cannot wait twenty seconds for a door that says nothing.
        #[cfg(test)]
        pub(crate) fn answer_wait(mut self, wait: Duration) -> Door {
            self.answer_wait = wait;
            self
        }

        /// LiMo's own door, served by LiMo's service. None when Windows will
        /// not say where its Program Files folder is.
        pub fn limo() -> Option<Door> {
            let folder = crate::win::token::program_files()?;
            Some(Door::new(crate::protocol::DOOR_PIPE, ServerMustBe::limo_service(&folder)))
        }

        /// What the console saw of the server it is connected to.
        pub fn server(&self) -> Option<&ServerSeen> {
            self.open.as_ref().map(|o| &o.seen)
        }

        /// What happened that the log should say, since the last call: never
        /// a request's arguments.
        pub fn take_notes(&mut self) -> Vec<String> {
            std::mem::take(&mut self.notes)
        }

        /// Closes the connection, which ends its sign in.
        pub fn close(&mut self) {
            self.open = None;
        }

        /// Counts one message to send, or refuses it when the console has
        /// sent [`MAX_SENT_PER_SECOND`] in the last second.
        fn may_send(&mut self) -> bool {
            let now = Instant::now();
            while self.sent.front().is_some_and(|t| now.duration_since(*t) >= Duration::from_secs(1)) {
                self.sent.pop_front();
            }
            if self.sent.len() >= MAX_SENT_PER_SECOND {
                return false;
            }
            self.sent.push_back(now);
            true
        }

        /// Asks one thing and waits for its answer. The server is checked
        /// and greeted when the connection opens, before the request is
        /// written. A refusal closes the connection.
        pub fn ask(&mut self, request: PageRequest) -> Result<DoorReply, DoorProblem> {
            if self.open.is_none() {
                if !self.may_send() {
                    return Err(DoorProblem::TooFast);
                }
                match self.connect()? {
                    Connected::Open(open) => self.open = Some(open),
                    Connected::Refused(message) => return Ok(DoorReply::Refused(message)),
                }
            }
            if !self.may_send() {
                return Err(DoorProblem::TooFast);
            }
            self.next_id = self.next_id.wrapping_add(1);
            let id = self.next_id;
            let Some(open) = self.open.as_mut() else { return Err(DoorProblem::NotRunning) };
            let answer = round_trip(open, request, id, self.answer_wait);
            match &answer {
                Ok(DoorReply::Reply(_)) => {}
                Ok(DoorReply::Refused(_)) => {
                    self.notes.push("the door refused this connection, and it was closed".to_string());
                    self.open = None;
                }
                Err(problem) => {
                    self.notes.push(format!("the connection was dropped: {problem}"));
                    self.open = None;
                }
            }
            answer
        }

        fn connect(&mut self) -> Result<Connected, DoorProblem> {
            let pipe = open_client(&self.name, CLIENT_ACCESS, OPEN_WAIT).map_err(|p| match p {
                OpenProblem::NotThere => DoorProblem::NotRunning,
                OpenProblem::Busy => DoorProblem::Busy,
                OpenProblem::Failed(code) => DoorProblem::Lost(format!("opening the door failed with Win32 error {code}")),
            })?;
            let seen = server_seen(&pipe);
            match check_server(&seen, &self.must) {
                Ok(ImageCheck::Matched) => {}
                Ok(ImageCheck::Unreadable) => self.notes.push(
                    "the door's program file could not be read from this account; its session and its owner decided".to_string(),
                ),
                Err(why) => {
                    self.notes.push(format!("nothing was sent to the pipe: {why}"));
                    return Err(DoorProblem::NotTheService(why));
                }
            }
            let mut open = Open { pipe, decoder: Decoder::new(), seen };
            let frame = encode(&hello()).map_err(|p| DoorProblem::Lost(p.to_string()))?;
            write_all(&open.pipe, &frame).map_err(|code| DoorProblem::Lost(format!("writing hello failed with Win32 error {code}")))?;
            match read_message(&mut open, HELLO_WAIT)? {
                FromDoor::Hello { protocol, .. } if protocol == PROTOCOL_VERSION => Ok(Connected::Open(open)),
                FromDoor::Hello { protocol, .. } => {
                    Err(DoorProblem::Lost(format!("the door speaks version {protocol} of its protocol, and this console {PROTOCOL_VERSION}")))
                }
                FromDoor::Refused { message } => {
                    self.notes.push("the door refused this connection at hello".to_string());
                    Ok(Connected::Refused(message))
                }
                FromDoor::Answer { .. } => Err(DoorProblem::Lost("the door answered before it was asked".to_string())),
            }
        }
    }

    enum Connected {
        Open(Open),
        Refused(String),
    }

    fn round_trip(open: &mut Open, request: PageRequest, id: u32, wait: Duration) -> Result<DoorReply, DoorProblem> {
        let frame = request.into_frame(id).map_err(|p| DoorProblem::Lost(p.to_string()))?;
        write_all(&open.pipe, &frame).map_err(|code| DoorProblem::Lost(format!("writing failed with Win32 error {code}")))?;
        drop(frame);
        match read_message(open, wait)? {
            FromDoor::Answer { id: got, reply } if got == id => Ok(DoorReply::Reply(reply)),
            FromDoor::Answer { id: got, .. } => Err(DoorProblem::Lost(format!("the door answered request {got} when {id} was asked"))),
            FromDoor::Refused { message } => Ok(DoorReply::Refused(message)),
            FromDoor::Hello { .. } => Err(DoorProblem::Lost("the door said hello again".to_string())),
        }
    }

    /// The next message from the door, waiting at most `wait`.
    fn read_message(open: &mut Open, wait: Duration) -> Result<FromDoor, DoorProblem> {
        let until = Instant::now() + wait;
        let mut chunk = zeroize::Zeroizing::new(vec![0u8; 16 * 1024]);
        loop {
            if let Some(message) = open.decoder.next_message::<FromDoor>().map_err(|p| DoorProblem::Lost(p.to_string()))? {
                return Ok(message);
            }
            let waiting = waiting_bytes(&open.pipe).map_err(|_| DoorProblem::Lost("the door was closed".to_string()))?;
            if waiting == 0 {
                if Instant::now() >= until {
                    return Err(DoorProblem::Lost("no answer came in time".to_string()));
                }
                std::thread::sleep(LOOK_EVERY);
                continue;
            }
            let read = read_some(&open.pipe, &mut chunk)
                .map_err(|code| DoorProblem::Lost(format!("reading failed with Win32 error {code}")))?;
            if read == 0 {
                return Err(DoorProblem::Lost("the door was closed".to_string()));
            }
            open.decoder.push(&chunk[..read]).map_err(|p| DoorProblem::Lost(p.to_string()))?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service_seen() -> ServerSeen {
        ServerSeen {
            pid: 812,
            session: Some(0),
            owner: Some(LOCAL_SYSTEM.to_string()),
            image: Some(r"C:\Program Files\LiMo\limo.exe".to_string()),
        }
    }

    #[test]
    fn limo_s_service_passes() {
        let must = ServerMustBe::limo_service(r"C:\Program Files");
        assert_eq!(must.image, r"C:\Program Files\LiMo\limo.exe");
        assert_eq!(ServerMustBe::limo_service(r"C:\Program Files\").image, must.image);
        assert_eq!(check_server(&service_seen(), &must), Ok(ImageCheck::Matched));
        let shouty = ServerSeen { image: Some(r"\\?\C:\PROGRAM FILES\LiMo\LIMO.EXE".to_string()), ..service_seen() };
        assert_eq!(check_server(&shouty, &must), Ok(ImageCheck::Matched));
    }

    #[test]
    fn the_installed_service_s_door_belongs_to_the_administrators_group_and_passes() {
        // As read back from the service installed on a PC: session 0, and
        // the Administrators group as the pipe's owner.
        let must = ServerMustBe::limo_service(r"C:\Program Files");
        let installed = ServerSeen { owner: Some(ADMINISTRATORS.to_string()), ..service_seen() };
        assert_eq!(check_server(&installed, &must), Ok(ImageCheck::Matched));
        let unread = ServerSeen { image: None, ..installed.clone() };
        assert_eq!(check_server(&unread, &must), Ok(ImageCheck::Unreadable));
        // The group's name opens nothing by itself: the session still decides first.
        let elsewhere = ServerSeen { session: Some(1), ..installed };
        assert_eq!(check_server(&elsewhere, &must), Err(NotTheService::Session { seen: Some(1) }));
        // And it stands in for LocalSystem only: a door that must belong to
        // another account is not the group's.
        let own = ServerMustBe { session: 1, owner: "S-1-5-21-1-2-3-1001".to_string(), image: must.image.clone() };
        assert!(own.owner_fits("S-1-5-21-1-2-3-1001"));
        assert!(!own.owner_fits(ADMINISTRATORS));
        // An administrator's account is not the group.
        assert!(!must.owner_fits("S-1-5-21-1-2-3-1001"));
    }

    #[test]
    fn an_unreadable_program_file_leaves_the_session_and_the_owner_to_decide() {
        let must = ServerMustBe::limo_service(r"C:\Program Files");
        let unread = ServerSeen { image: None, ..service_seen() };
        assert_eq!(check_server(&unread, &must), Ok(ImageCheck::Unreadable));
        // Unreadable never lets another session or owner through.
        let elsewhere = ServerSeen { image: None, session: Some(1), ..service_seen() };
        assert_eq!(check_server(&elsewhere, &must), Err(NotTheService::Session { seen: Some(1) }));
    }

    #[test]
    fn a_pipe_made_in_a_child_s_session_is_refused_first() {
        let must = ServerMustBe::limo_service(r"C:\Program Files");
        // A program in Asha's account, copied to look like LiMo's: still
        // session 1, still owned by Asha's account.
        let squatter = ServerSeen {
            pid: 4040,
            session: Some(1),
            owner: Some("S-1-5-21-1-2-3-1001".to_string()),
            image: Some(r"C:\Program Files\LiMo\limo.exe".to_string()),
        };
        assert_eq!(check_server(&squatter, &must), Err(NotTheService::Session { seen: Some(1) }));
        let unknown = ServerSeen { session: None, ..service_seen() };
        assert_eq!(check_server(&unknown, &must), Err(NotTheService::Session { seen: None }));
    }

    #[test]
    fn another_owner_or_another_program_is_refused() {
        let must = ServerMustBe::limo_service(r"C:\Program Files");
        let other_owner = ServerSeen { owner: Some("S-1-5-19".to_string()), ..service_seen() };
        assert_eq!(check_server(&other_owner, &must), Err(NotTheService::Owner));
        let no_owner = ServerSeen { owner: None, ..service_seen() };
        assert_eq!(check_server(&no_owner, &must), Err(NotTheService::Owner));
        let other_program = ServerSeen { image: Some(r"C:\Windows\System32\svchost.exe".to_string()), ..service_seen() };
        assert_eq!(check_server(&other_program, &must), Err(NotTheService::Image));
        let lookalike = ServerSeen { image: Some(r"C:\Program Files\LiMo\limo.exe.bak".to_string()), ..service_seen() };
        assert_eq!(check_server(&lookalike, &must), Err(NotTheService::Image));
    }

    #[test]
    fn every_problem_has_plain_words_with_no_dash() {
        let all = [
            DoorProblem::NotRunning,
            DoorProblem::Busy,
            DoorProblem::NotTheService(NotTheService::Owner),
            DoorProblem::Lost("x".into()),
            DoorProblem::TooFast,
        ];
        for p in &all {
            let words = p.words();
            assert!(words.ends_with('.') && !words.contains(" - ") && !words.contains('\u{2014}'), "{words}");
            assert!(!p.code().is_empty());
        }
        assert!(DoorProblem::NotTheService(NotTheService::Image).words().contains("password was not sent"));
    }
}
