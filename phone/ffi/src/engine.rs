//! `PhoneEngine` for Kotlin: the phone's engine behind a lock, so that
//! Kotlin may call it from any thread. Each method converts what came in,
//! calls the engine's own method of the same name, and converts what comes
//! back. The rules, the words, the counting and the kept state are all the
//! engine's; read its README for what each call does and when to keep the
//! state.

use std::sync::{Arc, Mutex, MutexGuard};

use limo_phone as engine;
use zeroize::Zeroizing;

use crate::convert;
use crate::shapes::{
    EngineError, GrantTaken, LoadProblem, PhoneFacts, PhoneOrders, PhoneView, PinAction, TimeRequest, TryAnswer,
};

/// The engine of LiMo on a child's phone. Call [`PhoneEngine::tick`] about
/// once a second while the screen is in use and at once on any change, carry
/// out the orders, and keep the state whenever `save_state` (or
/// [`PhoneEngine::should_save`]) says so, before anything else.
///
/// After [`EngineError::Broken`] or a panic (Kotlin's `InternalException`),
/// close this engine and load a new one from the kept state: a restart at
/// any instant is safe.
#[derive(uniffi::Object)]
pub struct PhoneEngine {
    inner: Mutex<engine::PhoneEngine>,
}

impl std::fmt::Debug for PhoneEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PhoneEngine")
    }
}

impl PhoneEngine {
    /// Wraps an engine for Kotlin.
    pub(crate) fn wrap(engine: engine::PhoneEngine) -> Arc<PhoneEngine> {
        Arc::new(PhoneEngine { inner: Mutex::new(engine) })
    }

    /// The engine, or [`EngineError::Broken`] when an earlier call panicked
    /// while it held the lock: what it holds may then be half changed, so it
    /// is not used again.
    fn engine(&self) -> Result<MutexGuard<'_, engine::PhoneEngine>, EngineError> {
        self.inner.lock().map_err(|_| EngineError::Broken)
    }
}

#[uniffi::export]
impl PhoneEngine {
    /// A new phone from a welcome that came by another way than the link
    /// (the development tool over the cable): the core's welcome `Grant` as
    /// JSON, and the phone's 32 byte code secret, at `wall_secs`. Keep the
    /// state at once. A pairing over the link makes its engine with
    /// `PhonePairing.makeEngine` instead, and the code secret never reaches
    /// Kotlin.
    ///
    /// The code secret is wiped here once the engine holds it; Kotlin should
    /// fill its own array with zeros.
    #[uniffi::constructor]
    pub fn paired(welcome: Vec<u8>, code_secret: Vec<u8>, wall_secs: i64) -> Result<Arc<PhoneEngine>, EngineError> {
        let mut code_secret = Zeroizing::new(code_secret);
        let welcome = convert::grant_from(&welcome).ok_or(EngineError::NotAWelcome)?;
        // The engine keeps the secret's own bytes in its state; nothing is
        // copied on the way.
        let engine = engine::PhoneEngine::paired(&welcome, std::mem::take(&mut *code_secret), wall_secs)?;
        Ok(PhoneEngine::wrap(engine))
    }

    /// The phone from the bytes kept last. It never fails: a state that
    /// cannot be loaded gives an engine that locks the phone, says why in
    /// [`PhoneEngine::load_problem`], and never asks to be kept.
    ///
    /// The bytes hold the code secret and the PIN record: this layer wipes
    /// its copy once read, and Kotlin should fill its own with zeros.
    #[uniffi::constructor]
    pub fn load(state: Vec<u8>) -> Arc<PhoneEngine> {
        let state = Zeroizing::new(state);
        PhoneEngine::wrap(engine::PhoneEngine::load(&state))
    }

    /// Why the kept state could not be loaded, if it could not.
    pub fn load_problem(&self) -> Result<Option<LoadProblem>, EngineError> {
        Ok(self.engine()?.load_problem().map(convert::load_problem_out))
    }

    /// One call: takes the facts, counts the time since the last call, and
    /// returns the orders.
    pub fn tick(&self, facts: PhoneFacts) -> Result<PhoneOrders, EngineError> {
        let facts = convert::facts_in(facts)?;
        let orders = self.engine()?.tick(&facts);
        convert::orders_out(orders)
    }

    /// The report to send to the master now, as the core's `Report` in JSON,
    /// for `PhoneSession.sendReport` as it is: Kotlin never reads it. Keep
    /// the state before sending it. None when the kept state could not be
    /// loaded.
    pub fn outgoing_report(&self, wall_secs: i64) -> Result<Option<Vec<u8>>, EngineError> {
        let report = self.engine()?.outgoing_report(wall_secs);
        report
            .map(|report| convert::to_json(&report).ok_or(EngineError::Internal { what: "write a report as JSON" }))
            .transpose()
    }

    /// Takes the master's answer to a report: the core's `Grant` in JSON, as
    /// a `LinkEvent` carried it. Keep the state before acting on it, then
    /// call [`PhoneEngine::tick`] again at once.
    pub fn take_grant(&self, grant: Vec<u8>, wall_secs: i64) -> Result<GrantTaken, EngineError> {
        let grant = convert::grant_from(&grant).ok_or(EngineError::NotAGrant)?;
        Ok(convert::grant_taken_out(self.engine()?.take_grant(&grant, wall_secs)))
    }

    /// A report got no answer in time, or the master could not be reached.
    pub fn lost_contact(&self) -> Result<(), EngineError> {
        self.engine()?.lost_contact();
        Ok(())
    }

    /// The child asks for `minutes` more today, with a short reason or none.
    /// Keep the state at once.
    pub fn ask_for_time(&self, minutes: i32, reason: String, wall_secs: i64) -> Result<TimeRequest, EngineError> {
        let minutes = convert::count_in(minutes, "the minutes asked for are below zero")
            .map_err(|what| EngineError::OutOfRange { what })?;
        let request = self.engine()?.ask_for_time(minutes, &reason, wall_secs)?;
        Ok(convert::request_out(request))
    }

    /// A one time code typed on the lock, as UTF-8 bytes (digits, with or
    /// without the space). Keep the state at once, right or wrong, before
    /// showing the answer.
    ///
    /// This layer wipes its copy of the code when the call ends; Kotlin
    /// should fill its own array with zeros and clear the field it was
    /// typed in.
    pub fn try_code(&self, code: Vec<u8>, wall_secs: i64) -> Result<TryAnswer, EngineError> {
        let code = Zeroizing::new(code);
        let typed = convert::typed(&code, "the code is not text")?;
        let answer = self.engine()?.try_code(typed, wall_secs)?;
        Ok(convert::try_out(answer))
    }

    /// A guardian types the PIN on the lock, as UTF-8 bytes, to do what
    /// `action` says. Keep the state at once, right or wrong, before showing
    /// the answer.
    ///
    /// This layer wipes its copy of the PIN when the call ends; Kotlin
    /// should fill its own array with zeros and clear the field it was
    /// typed in.
    pub fn try_pin(&self, pin: Vec<u8>, action: PinAction, wall_secs: i64) -> Result<TryAnswer, EngineError> {
        let pin = Zeroizing::new(pin);
        let typed = convert::typed(&pin, "the PIN is not text")?;
        let action = convert::pin_action_in(action)?;
        let answer = self.engine()?.try_pin(typed, action, wall_secs)?;
        Ok(convert::try_out(answer))
    }

    /// A guardian types the PIN on the guard, as UTF-8 bytes, to open the
    /// pages that could switch LiMo off or remove it for a few minutes
    /// (`GUARD_OPEN_SECS` in the engine). Wrong PINs count with the lock's.
    /// Keep the state at once, right or wrong, before showing the answer,
    /// then tick. A right PIN answers `RELEASE_CONFIRMED`: the core's own
    /// way of saying only that the PIN was right.
    ///
    /// This layer wipes its copy of the PIN when the call ends; Kotlin
    /// should fill its own array with zeros and clear the field it was
    /// typed in.
    pub fn try_guard_pin(&self, pin: Vec<u8>, wall_secs: i64) -> Result<TryAnswer, EngineError> {
        let pin = Zeroizing::new(pin);
        let typed = convert::typed(&pin, "the PIN is not text")?;
        let answer = self.engine()?.try_guard_pin(typed, wall_secs)?;
        Ok(convert::try_out(answer))
    }

    /// A guardian finished setup on this phone (left setup's last screen).
    /// Until then the engine counts nothing and covers nothing. True when
    /// this call finished it, false when it was finished already, which
    /// changes nothing. Keep the state at once, then tick again.
    /// [`EngineError::CannotCheck`] when the kept state could not be loaded.
    pub fn finish_setup(&self, wall_secs: i64) -> Result<bool, EngineError> {
        Ok(self.engine()?.finish_setup(wall_secs)?)
    }

    /// True when the host should keep the state now.
    pub fn should_save(&self) -> Result<bool, EngineError> {
        Ok(self.engine()?.should_save())
    }

    /// The bytes to keep, in place of what was kept before, before acting on
    /// anything else. Empty when the kept state could not be loaded: then
    /// write nothing.
    ///
    /// They hold the code secret and the PIN record: write them to the
    /// app's private storage (kept out of backups), then fill the array
    /// with zeros.
    pub fn save(&self) -> Result<Vec<u8>, EngineError> {
        Ok(self.engine()?.save())
    }

    /// The bytes [`PhoneEngine::save`] gave could not be written: the
    /// engine asks to be kept again at the next call.
    pub fn save_failed(&self) -> Result<(), EngineError> {
        self.engine()?.save_failed();
        Ok(())
    }

    /// The phone as the engine holds it now, for the child's own screen;
    /// None when the kept state could not be loaded.
    pub fn view(&self) -> Result<Option<PhoneView>, EngineError> {
        self.engine()?.view().map(convert::view_out).transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_engine_whose_lock_was_poisoned_says_it_is_broken_and_is_not_used() {
        let phone = PhoneEngine::load(Vec::new());
        let poisoned = std::panic::catch_unwind(|| {
            let _held = phone.inner.lock().unwrap();
            panic!("a panic while the engine is held");
        });
        assert!(poisoned.is_err());
        assert_eq!(phone.should_save(), Err(EngineError::Broken));
        assert_eq!(phone.save(), Err(EngineError::Broken));
        assert_eq!(phone.view(), Err(EngineError::Broken));
        assert_eq!(phone.lost_contact(), Err(EngineError::Broken));
    }
}
