//! COM on the calling thread, for the firewall and the shortcut: started
//! for one call and ended after it.

use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};

/// COM on this thread while this lives. A thread that already has COM in
/// another apartment keeps it, and nothing is ended for it.
pub(crate) struct Com {
    started: bool,
}

impl Com {
    pub(crate) fn start() -> Com {
        // SAFETY: starts COM on this thread; ended by `Drop` on the same
        // thread, since a `Com` lives on the stack of one call.
        let started = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
        Com { started }
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if self.started {
            // SAFETY: ends the start made in `Com::start`, once; every
            // interface of the call was released before.
            unsafe { CoUninitialize() };
        }
    }
}
