//! The console's half of the door: who must be on the other end before the
//! console says anything, and what a guardian is told when it is not.
//!
//! Any program on the PC can make a named pipe, a child's too, and could
//! make one called `limo.console` before LiMo's service does, or while it is
//! stopped. So before the console sends anything, a password above all, it
//! asks Windows who made the pipe it reached and checks three things, each
//! of which a program in a child's account cannot fake:
//!
//! - the process that serves it runs in session 0, where only services run
//!   (`GetNamedPipeServerSessionId`);
//! - the pipe belongs to LocalSystem, the account LiMo's service runs as
//!   (the pipe's owner, from its security descriptor);
//! - that process's program file is LiMo's own, in `Program Files\LiMo`
//!   (`GetNamedPipeServerProcessId`, then `QueryFullProcessImageNameW`).
//!
//! If one fails, the console closes the pipe having written nothing. The
//! console also opens the pipe at the identification level only, so that
//! whatever serves it can learn who the guardian is but can never act as
//! the guardian.

#![forbid(unsafe_code)]

use std::fmt;

/// LocalSystem's security identifier: the account LiMo's service runs as.
pub const LOCAL_SYSTEM: &str = "S-1-5-18";

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
    /// Its program file.
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
    /// Its program file is another one, or could not be read.
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

/// Checks the other end, in the order that costs least: the session, the
/// owner, the program file.
pub fn check_server(seen: &ServerSeen, must: &ServerMustBe) -> Result<(), NotTheService> {
    if seen.session != Some(must.session) {
        return Err(NotTheService::Session { seen: seen.session });
    }
    if !seen.owner.as_deref().is_some_and(|o| o.eq_ignore_ascii_case(&must.owner)) {
        return Err(NotTheService::Owner);
    }
    if !seen.image.as_deref().is_some_and(|i| same_path(i, &must.image)) {
        return Err(NotTheService::Image);
    }
    Ok(())
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
    /// The door stopped answering, or answered with something that is not a
    /// message.
    Broken(String),
}

impl DoorProblem {
    /// A short code for the page.
    pub fn code(&self) -> &'static str {
        match self {
            DoorProblem::NotRunning => "not_running",
            DoorProblem::Busy => "busy",
            DoorProblem::NotTheService(_) => "not_the_service",
            DoorProblem::Broken(_) => "broken",
        }
    }

    /// What the screen says to a guardian.
    pub fn words(&self) -> &'static str {
        match self {
            DoorProblem::NotRunning => "LiMo's service is not running on this PC, so its settings cannot open.",
            DoorProblem::Busy => "LiMo's service is busy. Try again in a moment.",
            DoorProblem::NotTheService(_) => {
                "Something that is not LiMo's service answered, so the console sent it nothing. Your password was not sent."
            }
            DoorProblem::Broken(_) => "LiMo's service stopped answering. Close the settings and open them again.",
        }
    }
}

impl fmt::Display for DoorProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DoorProblem::NotRunning => write!(f, "nothing serves the door"),
            DoorProblem::Busy => write!(f, "every instance of the door stayed busy"),
            DoorProblem::NotTheService(why) => write!(f, "the door is not LiMo's service: {why}"),
            DoorProblem::Broken(why) => write!(f, "the door broke: {why}"),
        }
    }
}

impl std::error::Error for DoorProblem {}

#[cfg(windows)]
pub use connection::Door;

#[cfg(windows)]
mod connection {
    use std::time::{Duration, Instant};

    use super::{DoorProblem, ServerMustBe, ServerSeen, check_server};
    use crate::protocol::{Answer, Decoder, Request, encode};
    use crate::stand_in::CLIENT_ACCESS;
    use crate::win::Owned;
    use crate::win::pipe::{OpenProblem, open_client, read_some, server_seen, waiting_bytes, write_all};

    /// How long to wait for a free instance of the door.
    const OPEN_WAIT: Duration = Duration::from_secs(2);
    /// How long an answer may take before the door counts as broken.
    const ANSWER_WAIT: Duration = Duration::from_secs(10);
    /// How often to look for an answer while waiting.
    const LOOK_EVERY: Duration = Duration::from_millis(2);

    /// The console's end of the door. It keeps one connection open, because
    /// the door's sign in belongs to a connection; a broken one is dropped,
    /// and the next request opens a new one, signed out.
    pub struct Door {
        name: String,
        must: ServerMustBe,
        open: Option<Open>,
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
            Door { name: name.to_string(), must, open: None }
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

        /// Closes the connection, which signs it out.
        pub fn close(&mut self) {
            self.open = None;
        }

        /// Asks one thing and waits for its answer. The server is checked
        /// when the connection opens, before anything is written. After a
        /// refusal the connection is closed, as the door closes it, so
        /// nothing more is written into it.
        pub fn ask(&mut self, request: &Request) -> Result<Answer, DoorProblem> {
            if self.open.is_none() {
                self.open = Some(self.connect()?);
            }
            let Some(open) = self.open.as_mut() else { return Err(DoorProblem::NotRunning) };
            let answer = round_trip(open, request);
            if !matches!(answer, Ok(ref a) if !matches!(a, Answer::Refused { .. })) {
                self.open = None;
            }
            answer
        }

        fn connect(&self) -> Result<Open, DoorProblem> {
            let pipe = open_client(&self.name, CLIENT_ACCESS, OPEN_WAIT).map_err(|p| match p {
                OpenProblem::NotThere => DoorProblem::NotRunning,
                OpenProblem::Busy => DoorProblem::Busy,
                OpenProblem::Failed(code) => DoorProblem::Broken(format!("opening the door failed with Win32 error {code}")),
            })?;
            let seen = server_seen(&pipe);
            check_server(&seen, &self.must).map_err(DoorProblem::NotTheService)?;
            Ok(Open { pipe, decoder: Decoder::new(), seen })
        }
    }

    fn round_trip(open: &mut Open, request: &Request) -> Result<Answer, DoorProblem> {
        let frame = encode(request).map_err(|p| DoorProblem::Broken(p.to_string()))?;
        write_all(&open.pipe, &frame).map_err(|code| DoorProblem::Broken(format!("writing failed with Win32 error {code}")))?;
        drop(frame);
        let until = Instant::now() + ANSWER_WAIT;
        let mut chunk = zeroize::Zeroizing::new(vec![0u8; 16 * 1024]);
        loop {
            if let Some(answer) = open.decoder.next_message::<Answer>().map_err(|p| DoorProblem::Broken(p.to_string()))? {
                return Ok(answer);
            }
            let waiting = waiting_bytes(&open.pipe).map_err(|_| DoorProblem::Broken("the door was closed".to_string()))?;
            if waiting == 0 {
                if Instant::now() >= until {
                    return Err(DoorProblem::Broken("no answer came in time".to_string()));
                }
                std::thread::sleep(LOOK_EVERY);
                continue;
            }
            let read = read_some(&open.pipe, &mut chunk)
                .map_err(|code| DoorProblem::Broken(format!("reading failed with Win32 error {code}")))?;
            if read == 0 {
                return Err(DoorProblem::Broken("the door was closed".to_string()));
            }
            open.decoder.push(&chunk[..read]).map_err(|p| DoorProblem::Broken(p.to_string()))?;
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
        assert_eq!(check_server(&service_seen(), &must), Ok(()));
        let shouty = ServerSeen { image: Some(r"\\?\C:\PROGRAM FILES\LiMo\LIMO.EXE".to_string()), ..service_seen() };
        assert_eq!(check_server(&shouty, &must), Ok(()));
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
        let unreadable = ServerSeen { image: None, ..service_seen() };
        assert_eq!(check_server(&unreadable, &must), Err(NotTheService::Image));
    }

    #[test]
    fn every_problem_has_plain_words_with_no_dash() {
        let all = [
            DoorProblem::NotRunning,
            DoorProblem::Busy,
            DoorProblem::NotTheService(NotTheService::Owner),
            DoorProblem::Broken("x".into()),
        ];
        for p in &all {
            let words = p.words();
            assert!(words.ends_with('.') && !words.contains(" - ") && !words.contains('\u{2014}'), "{words}");
            assert!(!p.code().is_empty());
        }
        assert!(DoorProblem::NotTheService(NotTheService::Image).words().contains("password was not sent"));
    }
}
