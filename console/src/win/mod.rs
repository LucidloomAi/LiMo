//! Everything that calls Windows. All of the console's unsafe code is here,
//! and each call is as thin as it can be: what to do is decided in the
//! modules above. The tests here touch only the test's own process, pipes
//! it makes itself under a name nobody else uses, and what any account may
//! read of the PC.

#[cfg(feature = "window")]
pub(crate) mod keys;
pub(crate) mod net;
pub(crate) mod pipe;
pub(crate) mod token;
#[cfg(feature = "window")]
pub(crate) mod webview2;

use windows::Win32::Foundation::{CloseHandle, HANDLE};

/// A kernel handle, closed when dropped.
pub(crate) struct Owned(HANDLE);

// SAFETY: a kernel handle is a number the kernel looks up; it may be used
// and closed from any thread. `Owned` closes it once, when dropped.
unsafe impl Send for Owned {}

impl Owned {
    /// Takes ownership of a handle. None for a null or invalid one.
    pub(crate) fn new(handle: HANDLE) -> Option<Owned> {
        if handle.is_invalid() { None } else { Some(Owned(handle)) }
    }

    /// The handle, still owned here.
    pub(crate) fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY: the handle is owned here and closed once.
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

/// A string as UTF-16 ending in a NUL, for a Windows call.
pub(crate) fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// UTF-16 from Windows, up to its first NUL.
pub(crate) fn from_wide(units: &[u16]) -> String {
    let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
    String::from_utf16_lossy(&units[..end])
}

/// The plain Win32 error number in an error from the `windows` crate.
pub(crate) fn win32_code(error: &windows::core::Error) -> u32 {
    let bits = error.code().0 as u32;
    if bits & 0xFFFF_0000 == 0x8007_0000 { bits & 0xFFFF } else { bits }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_and_strings_cross_to_windows_and_back() {
        let denied = windows::core::Error::from_hresult(windows::core::HRESULT(0x8007_0005_u32 as i32));
        assert_eq!(win32_code(&denied), 5);
        assert_eq!(from_wide(&wide("Asha")), "Asha");
        assert_eq!(from_wide(&[0x41, 0, 0x42]), "A");
        assert!(Owned::new(HANDLE(std::ptr::null_mut())).is_none());
    }
}
