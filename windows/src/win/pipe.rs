//! The anonymous pipes of a helper's channel, and the threads that read and
//! write them.
//!
//! An anonymous pipe has no name another program could open or make first:
//! both ends exist from the moment it is made, held only by the process that
//! made it, until one end is handed on.

use std::ffi::c_void;
use std::sync::mpsc::{SyncSender, sync_channel};

use serde::de::DeserializeOwned;
use windows::Win32::Foundation::{ERROR_BROKEN_PIPE, HANDLE, HANDLE_FLAG_INHERIT, HANDLE_FLAGS, SetHandleInformation};
use windows::Win32::Storage::FileSystem::{FILE_TYPE_PIPE, GetFileType, ReadFile, WriteFile};
use windows::Win32::System::Pipes::CreatePipe;
use zeroize::Zeroize;

use super::{Owned, win32_code};
use crate::protocol::{ChannelProblem, Decoder, Frame};

/// How many frames may wait for a helper that is not reading before the
/// service calls it not reading.
pub(crate) const WAITING_FRAMES: usize = 8;

/// One way of a channel: its read end and its write end, neither
/// inheritable.
pub(crate) fn pipe() -> windows::core::Result<(Owned, Owned)> {
    let mut read = HANDLE::default();
    let mut write = HANDLE::default();
    // SAFETY: writes two new handles, owned at once by `Owned`.
    unsafe { CreatePipe(&mut read, &mut write, None, 0)? };
    match (Owned::new(read), Owned::new(write)) {
        (Some(read), Some(write)) => Ok((read, write)),
        _ => Err(windows::core::Error::from_thread()),
    }
}

/// Marks a handle as one a child process may inherit, or not.
pub(crate) fn set_inheritable(handle: &Owned, inheritable: bool) -> windows::core::Result<()> {
    let flags = if inheritable { HANDLE_FLAG_INHERIT } else { HANDLE_FLAGS(0) };
    // SAFETY: changes one flag of a handle owned here.
    unsafe { SetHandleInformation(handle.raw(), HANDLE_FLAG_INHERIT.0, flags) }
}

/// True when a handle is a pipe.
pub(crate) fn is_pipe(handle: HANDLE) -> bool {
    // SAFETY: only asks what kind of object a handle is; a value that is not
    // a handle gives an unknown kind.
    unsafe { GetFileType(handle) == FILE_TYPE_PIPE }
}

/// Reads what is there, up to the buffer's size. Ok(0) when the other end
/// is closed.
fn read_some(handle: HANDLE, buffer: &mut [u8]) -> Result<usize, u32> {
    let mut read: u32 = 0;
    // SAFETY: writes at most the buffer's length into it.
    match unsafe { ReadFile(handle, Some(buffer), Some(&mut read), None) } {
        Ok(()) => Ok(read as usize),
        Err(e) if win32_code(&e) == ERROR_BROKEN_PIPE.0 => Ok(0),
        Err(e) => Err(win32_code(&e)),
    }
}

/// Writes all of `bytes`.
fn write_all(handle: HANDLE, mut bytes: &[u8]) -> Result<(), u32> {
    while !bytes.is_empty() {
        let mut written: u32 = 0;
        // SAFETY: reads at most the slice's length from it.
        unsafe { WriteFile(handle, Some(bytes), Some(&mut written), None) }.map_err(|e| win32_code(&e))?;
        if written == 0 {
            return Err(0);
        }
        bytes = bytes.get(written as usize..).unwrap_or(&[]);
    }
    Ok(())
}

/// What a reading thread hands on.
pub(crate) enum Incoming<T> {
    /// A whole message.
    Message(T),
    /// Something that is not a message; reading has stopped.
    Broken(ChannelProblem),
    /// The other end was closed; reading has stopped.
    Closed,
}

/// Starts a thread that reads messages of type `T` from `handle` and hands
/// each to `deliver`, until the first problem or the end, which it hands on
/// last. It stops early when `deliver` returns false. False when the thread
/// could not be started.
pub(crate) fn spawn_reader<T, F>(name: &str, handle: Owned, mut deliver: F) -> bool
where
    T: DeserializeOwned + 'static,
    F: FnMut(Incoming<T>) -> bool + Send + 'static,
{
    std::thread::Builder::new()
        .name(name.to_string())
        .spawn(move || {
            let mut decoder = Decoder::new();
            let mut chunk = vec![0u8; 4_096];
            loop {
                let read = match read_some(handle.raw(), &mut chunk) {
                    Ok(0) => {
                        deliver(Incoming::Closed);
                        break;
                    }
                    Ok(read) => read,
                    Err(code) => {
                        deliver(Incoming::Broken(ChannelProblem::ReadFailed { code }));
                        break;
                    }
                };
                let pushed = decoder.push(&chunk[..read]);
                chunk[..read].zeroize();
                if let Err(problem) = pushed {
                    deliver(Incoming::Broken(problem));
                    break;
                }
                let mut stop = false;
                loop {
                    match decoder.next_message::<T>() {
                        Ok(Some(message)) => {
                            if !deliver(Incoming::Message(message)) {
                                stop = true;
                                break;
                            }
                        }
                        Ok(None) => break,
                        Err(problem) => {
                            deliver(Incoming::Broken(problem));
                            stop = true;
                            break;
                        }
                    }
                }
                if stop {
                    break;
                }
            }
            chunk.zeroize();
            drop(handle);
        })
        .is_ok()
}

/// Starts a thread that writes each frame it is given to `handle`, in
/// order. Dropping the sender ends it and closes the handle; so does a
/// failed write. None when the thread could not be started.
pub(crate) fn spawn_writer(name: &str, handle: Owned, after: impl FnOnce() + Send + 'static) -> Option<SyncSender<Frame>> {
    let (tx, rx) = sync_channel::<Frame>(WAITING_FRAMES);
    std::thread::Builder::new()
        .name(name.to_string())
        .spawn(move || {
            for frame in rx {
                if write_all(handle.raw(), &frame).is_err() {
                    break;
                }
            }
            drop(handle);
            after();
        })
        .ok()?;
    Some(tx)
}

/// A raw value from the command line as a handle.
pub(crate) fn handle_from(value: usize) -> HANDLE {
    HANDLE(value as *mut c_void)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{FromHelper, encode};
    use std::sync::mpsc;

    /// A pipe inside this test process: frames written at one end come out
    /// whole at the other, and closing the write end ends the reader. Only
    /// this process's own handles are touched.
    #[test]
    fn frames_cross_a_pipe_of_this_process_and_the_end_is_seen() {
        let (read, write) = pipe().unwrap();
        assert!(is_pipe(read.raw()) && is_pipe(write.raw()));
        set_inheritable(&read, false).unwrap();
        let (tx, rx) = mpsc::channel();
        assert!(spawn_reader::<FromHelper, _>("test reader", read, move |incoming| {
            let what = match incoming {
                Incoming::Message(m) => format!("{m:?}"),
                Incoming::Broken(p) => format!("broken: {p}"),
                Incoming::Closed => "closed".to_string(),
            };
            tx.send(what).is_ok()
        }));
        let writer = spawn_writer("test writer", write, || {}).unwrap();
        writer.send(encode(&FromHelper::Hello { protocol: 1 }).unwrap()).unwrap();
        writer.send(encode(&FromHelper::LockOnScreen { seq: 3 }).unwrap()).unwrap();
        drop(writer);
        let got: Vec<String> = rx.iter().take(3).collect();
        assert_eq!(got, vec!["Hello { protocol: 1 }", "LockOnScreen { seq: 3 }", "closed"]);
        assert!(!is_pipe(handle_from(0)));
    }
}
