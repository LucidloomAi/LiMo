//! Whether something plays in the helper's session: whether a program of
//! this Windows session plays sound, and whether a program keeps the screen
//! awake. The helper puts both in its report once a second. What counts is
//! decided in `crate::helper` (`audio_session_plays`, `heard_lately`,
//! `screen_kept_awake`), with no Windows code; here are only the calls.
//!
//! **Whether, never what.** The only thing read about a sound is its
//! loudness meter, a number Windows keeps for each audio session, which is
//! compared with zero. No sound is captured, recorded, analysed or kept.
//! Of each audio session only these are asked: whether it is running, whether
//! it is Windows' system sounds session, the process that made it (only to
//! ask Windows which session that process is in, and then forgotten), and
//! its meter. Its name, its icon and its identifier, which can name a
//! program's file, are never asked for, and nothing read here leaves this
//! module but a yes or a no.
//!
//! **Its own thread.** The sound is read on a thread of its own, about once
//! a second, so that the Windows audio service, which answers these calls,
//! can never hold back the rest of the report. The thread notes when it last
//! heard sound; the report says sound plays when that was within
//! [`crate::helper::SOUND_HELD_MS`]. A read that fails, or a thread that is
//! stuck, therefore says no within a few seconds.
//!
//! **Devices coming and going.** Every read starts afresh: the sound
//! devices active at that moment, each one's audio sessions, each one's
//! meter. Headphones plugged in are read from the next second; a device
//! taken away in the middle of a read fails its own calls, which count as
//! no, and the other devices are still read. A PC with no sound device, or
//! with the Windows audio service stopped, reads no every time.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{S_FALSE, S_OK, STATUS_SUCCESS};
use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
use windows::Win32::Media::Audio::{
    AudioSessionStateActive, DEVICE_STATE_ACTIVE, IAudioSessionControl, IAudioSessionControl2, IAudioSessionManager2, IMMDevice,
    IMMDeviceEnumerator, MMDeviceEnumerator, eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize};
use windows::Win32::System::Power::{CallNtPowerInformation, SystemExecutionState};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::core::Interface;

use crate::helper::{AudioSessionSeen, audio_session_plays, heard_lately, screen_kept_awake};

/// How often the sound is read.
const LISTEN_EVERY: Duration = Duration::from_secs(1);

/// True when a program asks Windows to keep the screen awake: the whole
/// PC's execution state, as `CallNtPowerInformation` gives it for
/// `SystemExecutionState`, has `ES_DISPLAY_REQUIRED` set. It is the whole
/// PC's, not this session's alone. False when it cannot be read.
pub(crate) fn screen_kept_awake_now() -> bool {
    let mut state: u32 = 0;
    // SAFETY: for this level the call's page says the input buffer must be
    // null, and the output is one ULONG: `state`, with its size given. The
    // call writes nothing else.
    let status = unsafe {
        CallNtPowerInformation(SystemExecutionState, None, 0, Some((&raw mut state).cast()), size_of::<u32>() as u32)
    };
    status == STATUS_SUCCESS && screen_kept_awake(state)
}

/// Listens for sound on a thread of its own, and says whether a program of
/// this session plays sound now.
pub(crate) struct Listener {
    /// When sound was last heard, in milliseconds from `origin`, plus one;
    /// 0 for never. The listening thread writes it; nothing else does.
    heard: Arc<AtomicU64>,
    origin: Instant,
}

impl Listener {
    /// Starts listening. When the thread cannot be started, sound is never
    /// heard, and the report says so.
    pub(crate) fn start() -> Listener {
        let listener = Listener { heard: Arc::new(AtomicU64::new(0)), origin: Instant::now() };
        let heard = Arc::clone(&listener.heard);
        let origin = listener.origin;
        let _ = std::thread::Builder::new().name("limo helper sound".into()).spawn(move || listen(&heard, origin));
        listener
    }

    /// True when a program of this session played sound within the last few
    /// seconds.
    pub(crate) fn playing_sound(&self) -> bool {
        let heard = self.heard.load(Ordering::Relaxed).checked_sub(1);
        heard_lately(heard, millis_since(self.origin))
    }
}

/// Milliseconds since `origin`.
fn millis_since(origin: Instant) -> u64 {
    u64::try_from(origin.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// COM on this thread, in the multithreaded apartment, for as long as this
/// lives: every successful start, `S_FALSE` included, is ended once, on the
/// same thread, when it is dropped.
struct Com;

impl Com {
    fn start() -> Option<Com> {
        // SAFETY: starts COM on this thread; ended by `Drop`, on this thread,
        // since `Com` is made and dropped inside the listening thread only.
        let started = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        started.is_ok().then_some(Com)
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        // SAFETY: ends the start made in `Com::start`, once. Every interface
        // the thread held was released before: each read makes and drops its
        // own.
        unsafe { CoUninitialize() };
    }
}

/// The listening thread: a read about once a second, until the listener is
/// dropped.
fn listen(heard: &Arc<AtomicU64>, origin: Instant) {
    let Some(com) = Com::start() else { return };
    if let Some(own_session) = session_of(std::process::id()) {
        while Arc::strong_count(heard) > 1 {
            if sound_now(own_session) {
                heard.store(millis_since(origin).saturating_add(1), Ordering::Relaxed);
            }
            std::thread::sleep(LISTEN_EVERY);
        }
    }
    drop(com);
}

/// The Windows session a process is in. None when Windows would not say,
/// for a process of another account, say, or one that has ended.
fn session_of(pid: u32) -> Option<u32> {
    let mut session = 0u32;
    // SAFETY: writes one number into `session`.
    unsafe { ProcessIdToSessionId(pid, &mut session) }.ok().map(|()| session)
}

/// True when a program of `own_session` plays sound on any sound device
/// active now. COM must be started on this thread. False on any error.
fn sound_now(own_session: u32) -> bool {
    // SAFETY: COM is started on this thread (the caller holds a `Com`). The
    // enumerator, the device list and every device are released when
    // dropped, before this returns.
    unsafe {
        let Ok(devices) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else {
            return false;
        };
        let Ok(active) = devices.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) else { return false };
        let count = active.GetCount().unwrap_or(0);
        (0..count).any(|n| active.Item(n).is_ok_and(|device| device_plays(&device, own_session)))
    }
}

/// True when a program of `own_session` plays sound on this device.
fn device_plays(device: &IMMDevice, own_session: u32) -> bool {
    // SAFETY: COM is started on this thread. The session manager, its list
    // and every session are released when dropped.
    unsafe {
        let Ok(manager) = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else { return false };
        let Ok(sessions) = manager.GetSessionEnumerator() else { return false };
        let count = sessions.GetCount().unwrap_or(0);
        (0..count).any(|n| sessions.GetSession(n).is_ok_and(|session| audio_session_plays(&seen(&session), own_session)))
    }
}

/// What decides whether one audio session counts, and nothing else.
fn seen(session: &IAudioSessionControl) -> AudioSessionSeen {
    // SAFETY: COM is started on this thread. Each call writes only its own
    // result; the second interface and the meter are released when dropped.
    unsafe {
        let active = session.GetState().ok().map(|state| state == AudioSessionStateActive);
        let peak = session.cast::<IAudioMeterInformation>().ok().and_then(|meter| meter.GetPeakValue().ok());
        let Ok(session) = session.cast::<IAudioSessionControl2>() else {
            return AudioSessionSeen { active, peak, ..AudioSessionSeen::default() };
        };
        let system_sounds = match session.IsSystemSoundsSession() {
            said if said == S_OK => Some(true),
            said if said == S_FALSE => Some(false),
            _ => None,
        };
        // A session that spans several processes gives the first one, with
        // a code that is still a success.
        let windows_session = session.GetProcessId().ok().and_then(session_of);
        AudioSessionSeen { active, system_sounds, windows_session, peak }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helper::DISPLAY_REQUIRED;
    use windows::Win32::System::Power::ES_DISPLAY_REQUIRED;

    #[test]
    fn the_display_bit_is_windows_own() {
        assert_eq!(DISPLAY_REQUIRED, ES_DISPLAY_REQUIRED.0);
    }

    #[test]
    fn this_process_is_in_a_session_and_a_process_that_is_not_there_is_in_none() {
        // A harmless read of the test's own process.
        assert!(session_of(std::process::id()).is_some());
        // Process ids are multiples of four, so this one is never a process.
        assert_eq!(session_of(0xFFFF_FFFF), None);
    }

    #[test]
    fn asking_whether_this_pc_plays_sound_or_keeps_the_screen_awake_never_fails() {
        // Harmless reads: nothing is shown, played, recorded or changed. The
        // answers depend on the PC the test runs on, with or without a
        // sound device, so only that they come is checked.
        let _ = screen_kept_awake_now();
        let own_session = session_of(std::process::id()).unwrap();
        let com = Com::start().expect("COM starts on a new thread");
        let _ = sound_now(own_session);
        // Twice, as the thread does every second.
        let _ = sound_now(own_session);
        drop(com);
        // A COM start and end of its own, again, on the same thread.
        let again = Com::start().expect("COM starts again");
        drop(again);
        // A listener that never heard sound says no; one that heard it just
        // now says yes.
        let listener = Listener { heard: Arc::new(AtomicU64::new(0)), origin: Instant::now() };
        assert!(!listener.playing_sound());
        listener.heard.store(1, Ordering::Relaxed);
        assert!(listener.playing_sound());
    }
}
