//! The console's door on Windows: the named pipe and its access list, the
//! thread that takes each connection, a thread for each connection, and who
//! the client is. What the door decides is in the service's `door` module;
//! this only carries bytes and says who is on the other end.
//!
//! **The pipe.** `\\.\pipe\limo.console`, made by the service only:
//!
//! - its access list, [`DOOR_SDDL`], gives SYSTEM and Administrators full
//!   control and interactive users ([`CLIENT_ACCESS`]) read, write and the
//!   attributes, but not the right to make an instance of their own
//!   (`FILE_CREATE_PIPE_INSTANCE`, the same bit as `FILE_APPEND_DATA`).
//!   Windows checks that right when a program makes an instance with
//!   `CreateNamedPipeW`, not when a client opens the pipe, so no standard
//!   account can serve the door in LiMo's place; the tests show a process
//!   that is not elevated refused a second instance of its own pipe;
//! - the first instance is made with `FILE_FLAG_FIRST_PIPE_INSTANCE`, which
//!   fails when the name is already taken, so a program that made the name
//!   first is found out and said in the log; the next instance is always
//!   made before a connected one is handed to its thread, so the name is
//!   never left without one of LiMo's while the door is open;
//! - `PIPE_REJECT_REMOTE_CLIENTS`: nobody on another PC can connect.
//!
//! **Who.** After a client's first bytes are read, the connection's thread
//! takes on the client's own token for a moment
//! (`ImpersonateNamedPipeClient`), reads the token's user
//! (`OpenThreadToken`, as the service itself), and goes back to its own
//! (`RevertToSelf`) before anything else. Microsoft's page says the server
//! takes on "the security context of the last message read from the pipe",
//! which is why the first bytes are read first. The client's process id and
//! session come from `GetNamedPipeClientProcessId` and
//! `GetNamedPipeClientSessionId`, for the log. A client that opened the pipe
//! anonymously has no account to read, and is refused.
//!
//! **The threads.** Every read and write is overlapped, with an event, so
//! one thread per connection reads, writes and is told to close without
//! ever blocking on the other. A client that sends nothing within
//! [`HELLO_WAIT_MS`] is closed; one that sends more than
//! [`MAX_MESSAGES_PER_SECOND`] in a second is closed; one that does not read
//! what it is sent within five seconds is closed. Every buffer a read used
//! is wiped.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::mpsc::{Receiver, Sender, SyncSender, TrySendError, channel, sync_channel};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{
    CloseHandle, ERROR_BROKEN_PIPE, ERROR_IO_PENDING, ERROR_NO_DATA, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, ERROR_PIPE_NOT_CONNECTED, HANDLE,
    HLOCAL, LocalFree, WAIT_EVENT, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows::Win32::Security::Authorization::{
    ConvertSecurityDescriptorToStringSecurityDescriptorW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo,
    SDDL_REVISION_1, SE_KERNEL_OBJECT,
};
use windows::Win32::Security::{DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, RevertToSelf, SECURITY_ATTRIBUTES, TOKEN_QUERY};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, FILE_SHARE_NONE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX, ReadFile,
    SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT, WriteFile,
};
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeClientProcessId, GetNamedPipeClientSessionId, GetNamedPipeServerProcessId,
    GetNamedPipeServerSessionId, ImpersonateNamedPipeClient, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
    PIPE_UNLIMITED_INSTANCES, PIPE_WAIT, WaitNamedPipeW,
};
use windows::Win32::System::Threading::{
    CreateEventW, GetCurrentProcess, GetCurrentThread, INFINITE, OpenProcessToken, OpenThreadToken, SetEvent, WaitForMultipleObjects,
};
use windows::core::{PCWSTR, PWSTR};
use zeroize::{Zeroize, Zeroizing};

use super::sessions::token_user_sid;
use super::{Owned, error_text, win32_code, wide};
use crate::console::{
    ConsoleIncoming, ConsoleReader, FromConsole, HELLO_WAIT_MS, MAX_MESSAGES_PER_SECOND, ToConsole, answer_decoder, encode_from_console,
};
use crate::os::{ClientSeen, DoorEvent, DoorId, SendProblem};
use crate::protocol::{ChannelProblem, Decoder, Frame};

/// The door's access list: SYSTEM and Administrators full control,
/// interactive users [`CLIENT_ACCESS`]; nothing inherited.
pub const DOOR_SDDL: &str = "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x12019b;;;IU)";

/// What a console asks for, and all the access list gives an interactive
/// user: read (`FILE_GENERIC_READ`), and of writing only the data, its
/// attributes and extended attributes (`FILE_WRITE_DATA`,
/// `FILE_WRITE_ATTRIBUTES`, `FILE_WRITE_EA`): never
/// `FILE_CREATE_PIPE_INSTANCE`, which is checked when an instance is made.
pub const CLIENT_ACCESS: u32 = 0x0012_019b;

/// How many frames may wait for a console that is not reading before the
/// service calls it not reading.
const WAITING_FRAMES: usize = 8;
/// The most connections whose threads run at once; more are closed at once,
/// before anything is read.
const MAX_LIVE_CONNECTIONS: usize = 16;
/// The most connections whose threads run at once for one Windows session:
/// a program in a child's session that opens connection after connection,
/// saying nothing, never takes the places of a guardian's console.
const MAX_LIVE_PER_SESSION: usize = 4;
/// How long a write may wait for a console to read, in milliseconds.
const WRITE_LIMIT_MS: u32 = 5_000;
/// How long what was handed over is still written after a close, in
/// milliseconds for each frame.
const CLOSING_WRITE_LIMIT_MS: u32 = 1_000;
/// The size of one read.
const CHUNK_BYTES: usize = 16 * 1024;

/// An event, closed when dropped. It may be set and waited for from any
/// thread.
struct Event(HANDLE);

// SAFETY: an event is a kernel handle, a number the kernel looks up; it may
// be set, waited for and closed from any thread. It is closed once, when
// dropped.
unsafe impl Send for Event {}
// SAFETY: as above; setting and waiting need no exclusive access.
unsafe impl Sync for Event {}

impl Event {
    /// A new event, not set. A manual one stays set until reset; an
    /// automatic one is reset by the wait that sees it.
    fn new(manual: bool) -> Option<Event> {
        // SAFETY: makes a new unnamed event, owned at once by `Event`.
        let handle = unsafe { CreateEventW(None, manual, false, PCWSTR::null()) }.ok()?;
        (!handle.is_invalid()).then_some(Event(handle))
    }

    fn set(&self) {
        // SAFETY: sets an event owned here.
        let _ = unsafe { SetEvent(self.0) };
    }

    fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        // SAFETY: the handle is owned here and closed once.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// One overlapped operation: its event and its `OVERLAPPED`, which stay at
/// one address in memory while an operation is under way.
struct Io {
    event: Event,
    overlapped: Box<OVERLAPPED>,
}

impl Io {
    fn new() -> Option<Io> {
        let event = Event::new(true)?;
        let overlapped = Box::new(OVERLAPPED { hEvent: event.raw(), ..Default::default() });
        Some(Io { event, overlapped })
    }

    /// The `OVERLAPPED` for a new operation, cleared.
    fn fresh(&mut self) -> *mut OVERLAPPED {
        *self.overlapped = OVERLAPPED { hEvent: self.event.raw(), ..Default::default() };
        &mut *self.overlapped
    }

    /// The result of the operation, waiting for it when `wait` is true.
    fn result(&self, handle: HANDLE, wait: bool) -> Result<u32, u32> {
        let mut bytes: u32 = 0;
        // SAFETY: asks about the operation this `OVERLAPPED` started on this
        // handle; writes the count into a local.
        match unsafe { GetOverlappedResult(handle, &*self.overlapped, &mut bytes, wait) } {
            Ok(()) => Ok(bytes),
            Err(e) => Err(win32_code(&e)),
        }
    }

    /// Cancels the operation under way on `handle` and waits until Windows
    /// has finished with its buffer.
    fn cancel(&self, handle: HANDLE) {
        // SAFETY: cancels only this `OVERLAPPED`'s operation, then waits for
        // it to end, so its buffer may be used again.
        unsafe {
            let _ = CancelIoEx(handle, Some(&*self.overlapped));
        }
        let _ = self.result(handle, true);
    }
}

/// Waits for the first of `handles`; its index, or None on the timeout or a
/// failure.
fn wait_any(handles: &[HANDLE], timeout_ms: u32) -> Option<usize> {
    // SAFETY: waits on handles owned by the caller, which outlive the call.
    let got: WAIT_EVENT = unsafe { WaitForMultipleObjects(handles, false, timeout_ms) };
    if got == WAIT_TIMEOUT {
        return None;
    }
    let index = got.0.wrapping_sub(WAIT_OBJECT_0.0) as usize;
    (index < handles.len()).then_some(index)
}

/// Writes all of `bytes` to an overlapped handle, waiting at most `limit_ms`
/// for each part, and giving up when one of `stops` is set. Windows is done
/// with `bytes` when it returns.
fn write_all(handle: HANDLE, bytes: &[u8], io: &mut Io, stops: &[HANDLE], limit_ms: u32) -> Result<(), ()> {
    let mut sent = 0;
    while sent < bytes.len() {
        let overlapped = io.fresh();
        // SAFETY: the buffer outlives the write: this function waits for the
        // write to end, or cancels it and waits, before it returns.
        match unsafe { WriteFile(handle, Some(&bytes[sent..]), None, Some(overlapped)) } {
            Ok(()) => {}
            Err(e) if win32_code(&e) == ERROR_IO_PENDING.0 => {}
            Err(_) => return Err(()),
        }
        let mut waits = vec![io.event.raw()];
        waits.extend_from_slice(stops);
        match wait_any(&waits, limit_ms) {
            Some(0) => match io.result(handle, false) {
                Ok(0) | Err(_) => return Err(()),
                Ok(n) => sent += n as usize,
            },
            _ => {
                io.cancel(handle);
                return Err(());
            }
        }
    }
    Ok(())
}

/// Makes one instance of the door. The first, while LiMo holds none, with
/// `FILE_FLAG_FIRST_PIPE_INSTANCE`, which fails when the name is taken.
fn make_instance(name: &[u16], sddl: &[u16], first: bool) -> Result<Owned, String> {
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    // SAFETY: the descriptor is made by Windows and freed with LocalFree
    // once the instance is made with it.
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(PCWSTR(sddl.as_ptr()), SDDL_REVISION_1, &mut descriptor, None)
            .map_err(|e| error_text(&e))?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: false.into(),
        };
        let mut open = PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED;
        if first {
            open |= FILE_FLAG_FIRST_PIPE_INSTANCE;
        }
        let mode = PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS;
        let handle = CreateNamedPipeW(PCWSTR(name.as_ptr()), open, mode, PIPE_UNLIMITED_INSTANCES, 64 * 1024, 64 * 1024, 0, Some(&attributes));
        let made = if handle.is_invalid() { Err(error_text(&windows::core::Error::from_thread())) } else { Ok(handle) };
        let _ = LocalFree(Some(HLOCAL(descriptor.0)));
        made.and_then(|handle| Owned::new(handle).ok_or_else(|| "Windows gave no handle".to_string()))
    }
}

/// The account of the client on the other end, from its own token, taken
/// on for a moment. Err when the thread could not go back to its own
/// token: it must then end at once.
fn client_account(pipe: HANDLE) -> Result<Option<String>, ()> {
    // SAFETY: takes on the client's token for this thread only; it is put
    // back below before anything else happens.
    if unsafe { ImpersonateNamedPipeClient(pipe) }.is_err() {
        return Ok(None);
    }
    let mut token = HANDLE::default();
    // SAFETY: opens the thread's token for reading, checked as the service
    // itself; the handle is owned at once.
    let opened = unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, true, &mut token) };
    // SAFETY: goes back to the service's own token.
    if unsafe { RevertToSelf() }.is_err() {
        if opened.is_ok() {
            drop(Owned::new(token));
        }
        return Err(());
    }
    if opened.is_err() {
        return Ok(None);
    }
    let Some(token) = Owned::new(token) else { return Ok(None) };
    Ok(token_user_sid(token.raw()))
}

/// Who is on the other end. Err as [`client_account`] says.
fn client_seen(pipe: HANDLE) -> Result<ClientSeen, ()> {
    let mut pid: u32 = 0;
    let mut session: u32 = 0;
    // SAFETY: each writes one number into a local.
    let pid = unsafe { GetNamedPipeClientProcessId(pipe, &mut pid) }.ok().map(|()| pid);
    // SAFETY: as above.
    let session = unsafe { GetNamedPipeClientSessionId(pipe, &mut session) }.ok().map(|()| session);
    let account_id = client_account(pipe)?;
    Ok(ClientSeen { account_id, session_id: session, pid })
}

/// The connections whose threads run, in all and by the client's session.
#[derive(Default)]
struct Live {
    total: usize,
    by_session: HashMap<u32, usize>,
}

/// The counts, shared by the thread that takes connections and each
/// connection's thread.
#[derive(Clone, Default)]
struct Counts(Arc<Mutex<Live>>);

impl Counts {
    fn lock(&self) -> MutexGuard<'_, Live> {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn total(&self) -> usize {
        self.lock().total
    }

    /// Counts a connection in, unless there are as many as may be, in all
    /// or for its session: then false.
    fn take(&self, session: Option<u32>) -> bool {
        let mut live = self.lock();
        let of_session = session.map_or(0, |s| live.by_session.get(&s).copied().unwrap_or(0));
        if live.total >= MAX_LIVE_CONNECTIONS || of_session >= MAX_LIVE_PER_SESSION {
            return false;
        }
        live.total += 1;
        if let Some(session) = session {
            *live.by_session.entry(session).or_insert(0) += 1;
        }
        true
    }

    /// Counts a connection out.
    fn give_back(&self, session: Option<u32>) {
        let mut live = self.lock();
        live.total = live.total.saturating_sub(1);
        if let Some(session) = session
            && let Some(count) = live.by_session.get_mut(&session)
        {
            *count = count.saturating_sub(1);
            if *count == 0 {
                live.by_session.remove(&session);
            }
        }
    }
}

/// The Windows session of the client on the other end, as soon as it is
/// connected.
fn client_session(pipe: HANDLE) -> Option<u32> {
    let mut session: u32 = 0;
    // SAFETY: writes one number into a local.
    unsafe { GetNamedPipeClientSessionId(pipe, &mut session) }.ok().map(|()| session)
}

/// What the service holds for one connection.
struct ConnHandle {
    out: SyncSender<Frame>,
    ready: Arc<Event>,
    close: Arc<Event>,
}

/// One connection's thread.
struct Connection {
    id: DoorId,
    pipe: Owned,
    out: Receiver<Frame>,
    ready: Arc<Event>,
    close: Arc<Event>,
    stop: Arc<Event>,
    events: Sender<(DoorId, DoorEvent)>,
    gone: Sender<DoorId>,
    wake: Arc<dyn Fn() + Send + Sync>,
    first_bytes_wait: Duration,
    session: Option<u32>,
    counts: Counts,
}

impl Connection {
    fn tell(&self, event: DoorEvent) {
        let _ = self.events.send((self.id, event));
        (self.wake)();
    }

    fn run(self) {
        let ended = match (Io::new(), Io::new()) {
            (Some(read), Some(write)) => self.serve(read, write),
            _ => None,
        };
        if let Some(event) = ended {
            self.tell(event);
        }
        let _ = self.gone.send(self.id);
        self.counts.give_back(self.session);
    }

    /// Serves the connection until it ends; what to tell the service then.
    fn serve(&self, mut read: Io, mut write: Io) -> Option<DoorEvent> {
        let pipe = self.pipe.raw();
        let mut chunk = Zeroizing::new(vec![0u8; CHUNK_BYTES]);
        let mut reader = ConsoleReader::new();
        let started = Instant::now();
        let mut opened = false;
        let mut second = (Instant::now(), 0u32);
        let mut reading = false;
        let ended = loop {
            if !reading {
                let overlapped = read.fresh();
                // SAFETY: the chunk outlives the read: the loop waits for it
                // to end, and the end of this function cancels one under way
                // and waits before the chunk is wiped and dropped.
                match unsafe { ReadFile(pipe, Some(&mut chunk[..]), None, Some(overlapped)) } {
                    Ok(()) => reading = true,
                    Err(e) if win32_code(&e) == ERROR_IO_PENDING.0 => reading = true,
                    Err(e) if [ERROR_BROKEN_PIPE.0, ERROR_PIPE_NOT_CONNECTED.0, ERROR_NO_DATA.0].contains(&win32_code(&e)) => {
                        break opened.then_some(DoorEvent::Closed);
                    }
                    Err(e) => break opened.then(|| DoorEvent::Broken(ChannelProblem::ReadFailed { code: win32_code(&e) })),
                }
            }
            let timeout = if opened {
                INFINITE
            } else {
                let left = self.first_bytes_wait.saturating_sub(started.elapsed());
                u32::try_from(left.as_millis()).unwrap_or(INFINITE - 1)
            };
            let handles = [self.close.raw(), self.stop.raw(), self.ready.raw(), read.event.raw()];
            match wait_any(&handles, timeout) {
                Some(0) => {
                    // Told to close: what was handed over is written first,
                    // briefly.
                    for frame in self.out.try_iter() {
                        if write_all(pipe, &frame, &mut write, &[self.stop.raw()], CLOSING_WRITE_LIMIT_MS).is_err() {
                            break;
                        }
                    }
                    break None;
                }
                Some(1) => break None,
                Some(2) => {
                    let mut stuck = false;
                    for frame in self.out.try_iter() {
                        if write_all(pipe, &frame, &mut write, &[self.close.raw(), self.stop.raw()], WRITE_LIMIT_MS).is_err() {
                            stuck = true;
                            break;
                        }
                    }
                    if stuck {
                        break Some(DoorEvent::Broken(ChannelProblem::Unexpected {
                            what: "it stopped reading what it is sent".to_string(),
                        }));
                    }
                }
                Some(3) => {
                    reading = false;
                    let got = match read.result(pipe, false) {
                        Ok(n) => n as usize,
                        Err(code) if [ERROR_BROKEN_PIPE.0, ERROR_PIPE_NOT_CONNECTED.0, ERROR_NO_DATA.0].contains(&code) => {
                            break opened.then_some(DoorEvent::Closed);
                        }
                        Err(code) => break opened.then_some(DoorEvent::Broken(ChannelProblem::ReadFailed { code })),
                    };
                    if got == 0 {
                        continue;
                    }
                    if !opened {
                        match client_seen(pipe) {
                            Ok(client) => {
                                self.tell(DoorEvent::Opened(client));
                                opened = true;
                            }
                            // Still the client's token: nothing more is done.
                            Err(()) => break None,
                        }
                    }
                    let pushed = reader.push(&chunk[..got]);
                    chunk[..got].zeroize();
                    if let Err(problem) = pushed {
                        break Some(DoorEvent::Broken(problem));
                    }
                    let mut problem = None;
                    loop {
                        match reader.next_frame() {
                            Ok(Some(incoming)) => {
                                if second.0.elapsed() >= Duration::from_secs(1) {
                                    second = (Instant::now(), 0);
                                }
                                second.1 += 1;
                                if second.1 > MAX_MESSAGES_PER_SECOND {
                                    problem = Some(ChannelProblem::Unexpected {
                                        what: format!("it sent more than {MAX_MESSAGES_PER_SECOND} messages in one second"),
                                    });
                                    break;
                                }
                                self.tell(match incoming {
                                    ConsoleIncoming::Message(message) => DoorEvent::Message(message),
                                    ConsoleIncoming::BadRequest { id } => DoorEvent::BadRequest { id },
                                });
                            }
                            Ok(None) => break,
                            Err(bad) => {
                                problem = Some(bad);
                                break;
                            }
                        }
                    }
                    if let Some(problem) = problem {
                        break Some(DoorEvent::Broken(problem));
                    }
                }
                None if !opened => {
                    let wait = self.first_bytes_wait;
                    let secs = if wait.subsec_millis() == 0 { wait.as_secs().to_string() } else { format!("{:.1}", wait.as_secs_f64()) };
                    let _ = self.events.send((
                        DoorId(0),
                        DoorEvent::Note(format!(
                            "A program connected to LiMo’s settings and sent nothing for {secs} seconds, so its connection was closed."
                        )),
                    ));
                    break None;
                }
                None => {
                    break Some(DoorEvent::Broken(ChannelProblem::Unexpected { what: "waiting for it failed".to_string() }));
                }
                Some(_) => break None,
            }
        };
        if reading {
            read.cancel(pipe);
        }
        chunk.zeroize();
        ended
    }
}

/// The thread that takes each connection.
struct Acceptor {
    name: Vec<u16>,
    sddl: Vec<u16>,
    stop: Arc<Event>,
    events: Sender<(DoorId, DoorEvent)>,
    handles: Sender<(DoorId, ConnHandle)>,
    gone: Sender<DoorId>,
    wake: Arc<dyn Fn() + Send + Sync>,
    first_bytes_wait: Duration,
    counts: Counts,
}

impl Acceptor {
    fn note(&self, text: String) {
        let _ = self.events.send((DoorId(0), DoorEvent::Note(text)));
        (self.wake)();
    }

    /// True when told to stop within `ms`.
    fn stopped_within(&self, ms: u32) -> bool {
        wait_any(&[self.stop.raw()], ms).is_some()
    }

    fn run(self) {
        let mut next_id: u64 = 0;
        let mut listening: Option<Owned> = None;
        let mut said_taken = false;
        let mut said_open = false;
        let mut said_busy = false;
        let mut said_full = false;
        loop {
            let pipe = match listening.take() {
                Some(pipe) => pipe,
                None => {
                    // With no instance of LiMo's left, the next must be the
                    // first of its name.
                    let first = self.counts.total() == 0;
                    match make_instance(&self.name, &self.sddl, first) {
                        Ok(pipe) => {
                            if !said_open {
                                said_open = true;
                                self.note("The door for LiMo’s settings is open.".to_string());
                            }
                            pipe
                        }
                        Err(problem) if first => {
                            if !said_taken {
                                said_taken = true;
                                self.note(format!(
                                    "The door for LiMo’s settings could not be opened: {problem}. Another program may hold its name. \
                                     LiMo tries again every five seconds."
                                ));
                            }
                            if self.stopped_within(5_000) {
                                return;
                            }
                            continue;
                        }
                        Err(problem) => {
                            // A connection is open; the next instance waits
                            // for it to end, and is tried again shortly.
                            if !said_busy {
                                said_busy = true;
                                self.note(format!(
                                    "The door for LiMo’s settings could not be made ready for another console while one is connected: {problem}."
                                ));
                            }
                            if self.stopped_within(200) {
                                return;
                            }
                            continue;
                        }
                    }
                }
            };
            let Some(mut io) = Io::new() else {
                if self.stopped_within(1_000) {
                    return;
                }
                listening = Some(pipe);
                continue;
            };
            let overlapped = io.fresh();
            // SAFETY: the `OVERLAPPED` stays where it is until the connect
            // ends or is cancelled and waited for, below.
            let connected = match unsafe { ConnectNamedPipe(pipe.raw(), Some(overlapped)) } {
                Ok(()) => true,
                Err(e) if win32_code(&e) == ERROR_PIPE_CONNECTED.0 => true,
                Err(e) if win32_code(&e) == ERROR_IO_PENDING.0 => match wait_any(&[io.event.raw(), self.stop.raw()], INFINITE) {
                    Some(0) => io.result(pipe.raw(), false).is_ok(),
                    _ => {
                        io.cancel(pipe.raw());
                        return;
                    }
                },
                Err(_) => false,
            };
            if !connected {
                // This instance is done with; a new one is made next.
                drop(pipe);
                if self.stopped_within(100) {
                    return;
                }
                continue;
            }
            // The next instance first, so the name is never without one of
            // LiMo's.
            listening = make_instance(&self.name, &self.sddl, false).ok();
            let session = client_session(pipe.raw());
            if !self.counts.take(session) {
                // Closed at once, before anything is read.
                drop(pipe);
                if !said_full {
                    said_full = true;
                    self.note(format!(
                        "More connections to LiMo’s settings were opened at once than LiMo serves (at most {MAX_LIVE_PER_SESSION} from one session, \
                         {MAX_LIVE_CONNECTIONS} in all), so the next ones were closed at once."
                    ));
                }
                continue;
            }
            self.start_connection(&mut next_id, pipe, session);
        }
    }

    fn start_connection(&self, next_id: &mut u64, pipe: Owned, session: Option<u32>) {
        let (Some(ready), Some(close)) = (Event::new(false), Event::new(true)) else {
            self.counts.give_back(session);
            return;
        };
        let (ready, close) = (Arc::new(ready), Arc::new(close));
        *next_id += 1;
        let id = DoorId(*next_id);
        let (out_tx, out_rx) = sync_channel::<Frame>(WAITING_FRAMES);
        if self.handles.send((id, ConnHandle { out: out_tx, ready: ready.clone(), close: close.clone() })).is_err() {
            self.counts.give_back(session);
            return;
        }
        let connection = Connection {
            id,
            pipe,
            out: out_rx,
            ready,
            close,
            stop: self.stop.clone(),
            events: self.events.clone(),
            gone: self.gone.clone(),
            wake: self.wake.clone(),
            first_bytes_wait: self.first_bytes_wait,
            session,
            counts: self.counts.clone(),
        };
        let counts = self.counts.clone();
        let gone = self.gone.clone();
        if std::thread::Builder::new().name("limo door connection".to_string()).spawn(move || connection.run()).is_err() {
            counts.give_back(session);
            let _ = gone.send(id);
        }
    }
}

/// The console's door, served by the service: the pipe and its threads.
/// Dropping it closes the door and every connection.
pub struct RealDoor {
    stop: Arc<Event>,
    events: Receiver<(DoorId, DoorEvent)>,
    handles: Receiver<(DoorId, ConnHandle)>,
    gone: Receiver<DoorId>,
    conns: HashMap<DoorId, ConnHandle>,
    acceptor: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for RealDoor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RealDoor").field("connections", &self.conns.len()).finish_non_exhaustive()
    }
}

impl RealDoor {
    /// Opens the door at `name`. `wake` is called, on another thread,
    /// whenever something came, so that the service answers it at once.
    pub fn open(name: &str, wake: impl Fn() + Send + Sync + 'static) -> RealDoor {
        RealDoor::open_with(name, wake, Duration::from_millis(HELLO_WAIT_MS), DOOR_SDDL)
    }

    /// As [`RealDoor::open`], with the wait for a client's first bytes and
    /// the pipe's access list given: for tests, which cannot wait ten
    /// seconds, and which run in an account that is not SYSTEM. Making a
    /// second instance of a pipe takes the right to make one on its access
    /// list, which [`DOOR_SDDL`] gives SYSTEM and Administrators only; so a
    /// test that serves two clients at once adds its own account.
    pub fn open_with(name: &str, wake: impl Fn() + Send + Sync + 'static, first_bytes_wait: Duration, sddl: &str) -> RealDoor {
        let (events_tx, events) = channel();
        let (handles_tx, handles) = channel();
        let (gone_tx, gone) = channel();
        let stop = Arc::new(Event::new(true).unwrap_or(Event(HANDLE::default())));
        let acceptor = Acceptor {
            name: wide(name),
            sddl: wide(sddl),
            stop: stop.clone(),
            events: events_tx.clone(),
            handles: handles_tx,
            gone: gone_tx,
            wake: Arc::new(wake),
            first_bytes_wait,
            counts: Counts::default(),
        };
        let acceptor = if stop.raw().is_invalid() {
            let _ = events_tx.send((DoorId(0), DoorEvent::Note("The door for LiMo’s settings could not be opened: no event.".into())));
            None
        } else {
            match std::thread::Builder::new().name("limo door".to_string()).spawn(move || acceptor.run()) {
                Ok(thread) => Some(thread),
                Err(_) => {
                    let text = "The door for LiMo’s settings could not be opened: its thread could not be started.";
                    let _ = events_tx.send((DoorId(0), DoorEvent::Note(text.to_string())));
                    None
                }
            }
        };
        RealDoor { stop, events, handles, gone, conns: HashMap::new(), acceptor }
    }

    /// Everything that came since the last call.
    pub fn received(&mut self) -> Vec<(DoorId, DoorEvent)> {
        for (id, handle) in self.handles.try_iter() {
            self.conns.insert(id, handle);
        }
        let events: Vec<(DoorId, DoorEvent)> = self.events.try_iter().collect();
        for id in self.gone.try_iter() {
            self.conns.remove(&id);
        }
        events
    }

    /// Hands a frame to a connection, without waiting for it to be written.
    pub fn send(&mut self, id: DoorId, frame: Frame) -> Result<(), SendProblem> {
        let Some(conn) = self.conns.get(&id) else { return Err(SendProblem::Gone) };
        match conn.out.try_send(frame) {
            Ok(()) => {
                conn.ready.set();
                Ok(())
            }
            Err(TrySendError::Full(_)) => Err(SendProblem::NotReading),
            Err(TrySendError::Disconnected(_)) => Err(SendProblem::Gone),
        }
    }

    /// Closes a connection, once what was handed to it is written.
    pub fn close(&mut self, id: DoorId) {
        if let Some(conn) = self.conns.remove(&id) {
            conn.close.set();
        }
    }
}

impl Drop for RealDoor {
    fn drop(&mut self) {
        self.stop.set();
        for conn in self.conns.values() {
            conn.close.set();
        }
        if let Some(acceptor) = self.acceptor.take() {
            let _ = acceptor.join();
        }
    }
}

/// The account this process runs in, as a security identifier: for the
/// tests, which link it to a child to see the door refuse it.
pub fn this_account() -> Option<String> {
    let mut token = HANDLE::default();
    // SAFETY: opens this process's own token for reading; owned at once.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.ok()?;
    let token = Owned::new(token)?;
    token_user_sid(token.raw())
}

/// Who serves a pipe, as its client sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerSeen {
    /// The serving process's id.
    pub pid: Option<u32>,
    /// The Windows session it runs in: 0 for a service.
    pub session_id: Option<u32>,
}

/// A client of the console's door: for the tests, which knock on a real
/// pipe in their own account, and for `limo dev-console`. It opens the pipe
/// with [`CLIENT_ACCESS`] only, and at the identification level, so a
/// program posing as the door could learn who it is but never act as it.
pub struct DoorClient {
    pipe: Owned,
    read: Io,
    write: Io,
    decoder: Decoder,
    chunk: Zeroizing<Vec<u8>>,
}

impl std::fmt::Debug for DoorClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DoorClient").finish_non_exhaustive()
    }
}

impl DoorClient {
    /// Opens the pipe at `name`, waiting up to `wait_ms` while every
    /// instance is busy.
    pub fn connect(name: &str, wait_ms: u32) -> Result<DoorClient, String> {
        let pipe = open_pipe(name, CLIENT_ACCESS, wait_ms).map_err(|(_, text)| text)?;
        let (Some(read), Some(write)) = (Io::new(), Io::new()) else { return Err("no events".to_string()) };
        Ok(DoorClient { pipe, read, write, decoder: answer_decoder(), chunk: Zeroizing::new(vec![0u8; CHUNK_BYTES]) })
    }

    /// Tries to open the pipe at `name` with `access`, and closes it at
    /// once: Ok when Windows allowed it, or the Win32 error number.
    pub fn try_open(name: &str, access: u32) -> Result<(), u32> {
        open_pipe(name, access, 2_000).map(drop).map_err(|(code, _)| code)
    }

    /// Who serves the pipe.
    pub fn server(&self) -> ServerSeen {
        let mut pid: u32 = 0;
        let mut session: u32 = 0;
        // SAFETY: each writes one number into a local.
        let pid = unsafe { GetNamedPipeServerProcessId(self.pipe.raw(), &mut pid) }.ok().map(|()| pid);
        // SAFETY: as above.
        let session_id = unsafe { GetNamedPipeServerSessionId(self.pipe.raw(), &mut session) }.ok().map(|()| session);
        ServerSeen { pid, session_id }
    }

    /// The pipe's access list, as SDDL.
    pub fn access_list(&self) -> Option<String> {
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        // SAFETY: both buffers come from Windows and are freed with
        // LocalFree once read.
        unsafe {
            let found = GetSecurityInfo(
                self.pipe.raw(),
                SE_KERNEL_OBJECT,
                DACL_SECURITY_INFORMATION,
                None,
                None,
                None,
                None,
                Some(&mut descriptor),
            );
            if found.is_err() {
                return None;
            }
            let mut text = PWSTR::null();
            let converted =
                ConvertSecurityDescriptorToStringSecurityDescriptorW(descriptor, SDDL_REVISION_1, DACL_SECURITY_INFORMATION, &mut text, None);
            let _ = LocalFree(Some(HLOCAL(descriptor.0)));
            converted.ok()?;
            let sddl = text.to_string().ok();
            let _ = LocalFree(Some(HLOCAL(text.0 as *mut c_void)));
            sddl
        }
    }

    /// Writes bytes as they are, whatever they are.
    pub fn write_raw(&mut self, bytes: &[u8]) -> Result<(), String> {
        write_all(self.pipe.raw(), bytes, &mut self.write, &[], WRITE_LIMIT_MS).map_err(|()| "the write did not go through".to_string())
    }

    /// Sends a message.
    pub fn send(&mut self, message: &FromConsole) -> Result<(), String> {
        let frame = encode_from_console(message).map_err(|problem| problem.to_string())?;
        self.write_raw(&frame)
    }

    /// The next message from the service, waiting up to `timeout_ms`: None
    /// when none came in that time; an error when the service closed the
    /// connection or sent what is not a message.
    pub fn read(&mut self, timeout_ms: u32) -> Result<Option<ToConsole>, String> {
        let deadline = Instant::now() + Duration::from_millis(u64::from(timeout_ms));
        loop {
            if let Some(message) = self.decoder.next_message::<ToConsole>().map_err(|problem| problem.to_string())? {
                return Ok(Some(message));
            }
            let left = deadline.saturating_duration_since(Instant::now());
            let pipe = self.pipe.raw();
            let overlapped = self.read.fresh();
            // SAFETY: the chunk outlives the read: it is waited for, or
            // cancelled and waited for, below.
            match unsafe { ReadFile(pipe, Some(&mut self.chunk[..]), None, Some(overlapped)) } {
                Ok(()) => {}
                Err(e) if win32_code(&e) == ERROR_IO_PENDING.0 => {}
                Err(e) => return Err(format!("the connection was closed (Win32 error {})", win32_code(&e))),
            }
            let wait = u32::try_from(left.as_millis()).unwrap_or(INFINITE - 1);
            let got = match wait_any(&[self.read.event.raw()], wait) {
                Some(0) => self.read.result(pipe, false),
                _ => {
                    self.read.cancel(pipe);
                    // A read that ended just as it was cancelled keeps its
                    // bytes.
                    match self.read.result(pipe, false) {
                        Ok(n) if n > 0 => Ok(n),
                        _ => return Ok(None),
                    }
                }
            };
            match got {
                Ok(0) => {}
                Ok(n) => {
                    let n = n as usize;
                    let pushed = self.decoder.push(&self.chunk[..n]);
                    self.chunk[..n].zeroize();
                    pushed.map_err(|problem| problem.to_string())?;
                }
                Err(code) => return Err(format!("the connection was closed (Win32 error {code})")),
            }
        }
    }
}

/// Opens a pipe as a client, at the identification level, waiting while
/// every instance is busy.
fn open_pipe(name: &str, access: u32, wait_ms: u32) -> Result<Owned, (u32, String)> {
    let path = wide(name);
    let deadline = Instant::now() + Duration::from_millis(u64::from(wait_ms));
    loop {
        // SAFETY: opens a pipe by name; the handle is owned at once.
        let opened = unsafe {
            CreateFileW(
                PCWSTR(path.as_ptr()),
                access,
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                None,
            )
        };
        match opened {
            Ok(handle) => return Owned::new(handle).ok_or((0, "Windows gave no handle".to_string())),
            Err(e) if win32_code(&e) == ERROR_PIPE_BUSY.0 && Instant::now() < deadline => {
                let left = deadline.saturating_duration_since(Instant::now()).as_millis();
                // SAFETY: waits for an instance of the pipe by name.
                let _ = unsafe { WaitNamedPipeW(PCWSTR(path.as_ptr()), u32::try_from(left).unwrap_or(1).max(1)) };
            }
            Err(e) => return Err((win32_code(&e), error_text(&e))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_access_list_keeps_the_right_to_make_an_instance_from_everyone_but_system_and_administrators() {
        // FILE_APPEND_DATA, which for a pipe is FILE_CREATE_PIPE_INSTANCE.
        const CREATE_PIPE_INSTANCE: u32 = 0x4;
        assert_eq!(CLIENT_ACCESS & CREATE_PIPE_INSTANCE, 0);
        assert!(DOOR_SDDL.contains(&format!("(A;;{CLIENT_ACCESS:#x};;;IU)")));
        assert!(DOOR_SDDL.starts_with("D:P"));
        for nobody in ["WD", "AU", "BU", "AN", "NU"] {
            assert!(!DOOR_SDDL.contains(&format!(";;;{nobody})")), "{nobody}");
        }
        // It parses as Windows reads it.
        let sddl = wide(DOOR_SDDL);
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        // SAFETY: the descriptor is freed at once.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(PCWSTR(sddl.as_ptr()), SDDL_REVISION_1, &mut descriptor, None).unwrap();
            let _ = LocalFree(Some(HLOCAL(descriptor.0)));
        }
    }

    #[test]
    fn this_processs_own_account_reads() {
        let account = this_account().unwrap();
        assert!(account.starts_with("S-1-5-"), "{account}");
    }
}
