//! The door's named pipe: opening it as the console, and asking Windows who
//! serves it. Making and serving a pipe (`serve`) is here for the tests'
//! stand in only.

use std::time::{Duration, Instant};

use windows::Win32::Foundation::{
    ERROR_BROKEN_PIPE, ERROR_FILE_NOT_FOUND, ERROR_PIPE_BUSY, ERROR_PIPE_NOT_CONNECTED, ERROR_SUCCESS, HANDLE, HLOCAL, LocalFree,
};
use windows::Win32::Security::Authorization::{GetSecurityInfo, SE_KERNEL_OBJECT};
use windows::Win32::Security::{OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_NONE, OPEN_EXISTING, ReadFile, SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT,
    WriteFile,
};
use windows::Win32::System::Pipes::{GetNamedPipeServerProcessId, GetNamedPipeServerSessionId, PeekNamedPipe, WaitNamedPipeW};
use windows::core::PCWSTR;

use super::token::{image_of, sid_text};
use super::{Owned, wide, win32_code};
use crate::door::ServerSeen;

/// The access the console asks for: read (`FILE_GENERIC_READ`), write the
/// data and write the attributes (`FILE_WRITE_DATA`, `FILE_WRITE_ATTRIBUTES`),
/// and nothing more. Not `GENERIC_WRITE`, which would ask for the right to
/// make an instance of the pipe too (`FILE_APPEND_DATA`); Windows checks that
/// right when an instance is made, not here, and the door's access list
/// keeps it to LocalSystem and Administrators. The door grants interactive
/// users `0x12019b`, of which this is a part.
pub(crate) const CLIENT_ACCESS: u32 = 0x0012_018b;

/// Why a client could not open the door.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpenProblem {
    /// No pipe of that name.
    NotThere,
    /// Every instance stayed busy until the wait ran out.
    Busy,
    /// Another Win32 error.
    Failed(u32),
}

/// Opens a pipe as a client, waiting up to `wait` for a free instance.
///
/// It asks for `access` only, and lends the server its token at the
/// identification level: the server can read who the client is, and can
/// never act as it.
pub(crate) fn open_client(name: &str, access: u32, wait: Duration) -> Result<Owned, OpenProblem> {
    let name = wide(name);
    let until = Instant::now() + wait;
    loop {
        // SAFETY: opens an existing pipe; the handle is owned by `Owned`.
        let opened = unsafe {
            CreateFileW(
                PCWSTR(name.as_ptr()),
                access,
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(SECURITY_SQOS_PRESENT.0 | SECURITY_IDENTIFICATION.0),
                None,
            )
        };
        match opened {
            Ok(handle) => return Owned::new(handle).ok_or(OpenProblem::Failed(0)),
            Err(e) if win32_code(&e) == ERROR_FILE_NOT_FOUND.0 => return Err(OpenProblem::NotThere),
            Err(e) if win32_code(&e) == ERROR_PIPE_BUSY.0 => {
                let left = until.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    return Err(OpenProblem::Busy);
                }
                let ms = u32::try_from(left.as_millis()).unwrap_or(u32::MAX).max(1);
                // SAFETY: waits for an instance of the named pipe.
                let _ = unsafe { WaitNamedPipeW(PCWSTR(name.as_ptr()), ms) };
            }
            Err(e) => return Err(OpenProblem::Failed(win32_code(&e))),
        }
    }
}

/// What the client can learn of the server: its process id and session
/// from the pipe, the pipe's owner from its security descriptor, and the
/// program file from the process, when this account may read it.
pub(crate) fn server_seen(pipe: &Owned) -> ServerSeen {
    let mut pid: u32 = 0;
    let mut session: u32 = 0;
    // SAFETY: each writes into the value handed to it.
    let pid_ok = unsafe { GetNamedPipeServerProcessId(pipe.raw(), &mut pid) }.is_ok();
    // SAFETY: as above.
    let session_ok = unsafe { GetNamedPipeServerSessionId(pipe.raw(), &mut session) }.is_ok();
    ServerSeen {
        pid: if pid_ok { pid } else { 0 },
        session: session_ok.then_some(session),
        owner: owner_of(pipe.raw()),
        image: if pid_ok { image_of(pid) } else { None },
    }
}

/// A kernel object's owner, as a security identifier string.
pub(crate) fn owner_of(handle: HANDLE) -> Option<String> {
    let mut owner = PSID::default();
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    // SAFETY: the owner points into the descriptor, which Windows allocates
    // and which is freed with LocalFree once the owner is copied.
    unsafe {
        let found = GetSecurityInfo(
            handle,
            SE_KERNEL_OBJECT,
            OWNER_SECURITY_INFORMATION,
            Some(&mut owner),
            None,
            None,
            None,
            Some(&mut descriptor),
        );
        if found != ERROR_SUCCESS {
            return None;
        }
        let text = sid_text(owner);
        let _ = LocalFree(Some(HLOCAL(descriptor.0)));
        text
    }
}

/// Writes all of `bytes`.
pub(crate) fn write_all(pipe: &Owned, mut bytes: &[u8]) -> Result<(), u32> {
    while !bytes.is_empty() {
        let mut written: u32 = 0;
        // SAFETY: reads at most the slice's length from it.
        unsafe { WriteFile(pipe.raw(), Some(bytes), Some(&mut written), None) }.map_err(|e| win32_code(&e))?;
        if written == 0 {
            return Err(0);
        }
        bytes = bytes.get(written as usize..).unwrap_or(&[]);
    }
    Ok(())
}

/// Reads what is there, up to the buffer's size, waiting until something
/// is. Ok(0) when the other end is closed: a server that has let its client
/// go says "not connected" rather than "broken".
pub(crate) fn read_some(pipe: &Owned, buffer: &mut [u8]) -> Result<usize, u32> {
    let mut read: u32 = 0;
    // SAFETY: writes at most the buffer's length into it.
    match unsafe { ReadFile(pipe.raw(), Some(buffer), Some(&mut read), None) } {
        Ok(()) => Ok(read as usize),
        Err(e) if [ERROR_BROKEN_PIPE.0, ERROR_PIPE_NOT_CONNECTED.0].contains(&win32_code(&e)) => Ok(0),
        Err(e) => Err(win32_code(&e)),
    }
}

/// How many bytes wait to be read, without waiting. Err(code) when the
/// other end has gone.
pub(crate) fn waiting_bytes(pipe: &Owned) -> Result<u32, u32> {
    let mut available: u32 = 0;
    // SAFETY: only asks how much is waiting.
    unsafe { PeekNamedPipe(pipe.raw(), None, 0, None, Some(&mut available), None) }.map_err(|e| win32_code(&e))?;
    Ok(available)
}

/// Making and serving a pipe, for the tests' stand in.
#[cfg(test)]
pub(crate) mod serve {
    use std::ffi::c_void;

    use windows::Win32::Foundation::{ERROR_PIPE_CONNECTED, ERROR_SUCCESS, GetLastError, HLOCAL, LocalFree};
    use windows::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo,
        SDDL_REVISION_1, SE_KERNEL_OBJECT,
    };
    use windows::Win32::Security::{DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
    use windows::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, FlushFileBuffers, PIPE_ACCESS_DUPLEX};
    use windows::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
        PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };
    use windows::core::{PCWSTR, PWSTR};

    use crate::win::{Owned, wide, win32_code};

    /// Makes one instance of a pipe to serve, with the access list `sddl`,
    /// refused when the name is taken.
    pub(crate) fn create_server(name: &str, sddl: &str) -> Result<Owned, u32> {
        let name = wide(name);
        let text = wide(sddl);
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        // SAFETY: the descriptor is made by Windows from the text and freed
        // with LocalFree once the pipe exists; the handle is owned by `Owned`.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(PCWSTR(text.as_ptr()), SDDL_REVISION_1, &mut descriptor, None)
                .map_err(|e| win32_code(&e))?;
            let attributes = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: descriptor.0,
                bInheritHandle: false.into(),
            };
            let handle = CreateNamedPipeW(
                PCWSTR(name.as_ptr()),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                64 * 1024,
                64 * 1024,
                0,
                Some(&attributes),
            );
            let code = GetLastError();
            let _ = LocalFree(Some(HLOCAL(descriptor.0)));
            Owned::new(handle).ok_or(code.0)
        }
    }

    /// Waits until a client opens this instance.
    pub(crate) fn wait_for_client(pipe: &Owned) -> Result<(), u32> {
        // SAFETY: waits on a pipe owned here, with no overlapped structure.
        match unsafe { ConnectNamedPipe(pipe.raw(), None) } {
            Ok(()) => Ok(()),
            Err(e) if win32_code(&e) == ERROR_PIPE_CONNECTED.0 => Ok(()),
            Err(e) => Err(win32_code(&e)),
        }
    }

    /// Lets the client go, so the instance can wait for the next one.
    pub(crate) fn let_client_go(pipe: &Owned) {
        // SAFETY: on a pipe owned here.
        unsafe {
            let _ = FlushFileBuffers(pipe.raw());
            let _ = DisconnectNamedPipe(pipe.raw());
        }
    }

    /// A pipe's owner and access list, as SDDL.
    pub(crate) fn sddl_of(pipe: &Owned) -> Option<String> {
        let parts = OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        // SAFETY: both buffers come from Windows and are freed with
        // LocalFree once read.
        unsafe {
            let found = GetSecurityInfo(pipe.raw(), SE_KERNEL_OBJECT, parts, None, None, None, None, Some(&mut descriptor));
            if found != ERROR_SUCCESS {
                return None;
            }
            let mut text = PWSTR::null();
            let converted = ConvertSecurityDescriptorToStringSecurityDescriptorW(descriptor, SDDL_REVISION_1, parts, &mut text, None);
            let _ = LocalFree(Some(HLOCAL(descriptor.0)));
            converted.ok()?;
            let sddl = text.to_string().ok();
            let _ = LocalFree(Some(HLOCAL(text.0 as *mut c_void)));
            sddl
        }
    }
}
