//! The lock of a Windows account not set up in LiMo takes the guardian PIN.
//!
//! Once the first start is finished, an account LiMo was not told about is
//! locked (administrators never are). A guardian at such a lock types the
//! PIN and chooses for how long, and the account is open until then: the
//! engine, which has no child for the account, still notes once a day that
//! it was used. What a right PIN opened is kept in the service's memory
//! only, so a restart of the service locks the account again, the safe side.
//!
//! The PIN is checked by the family's file itself ([`Hub::check_pin`]), with
//! its own wrong guesses and wait, kept in the file, by the same rule as the
//! password's and a device's: four free, the fifth sets a wait of a minute,
//! each further one doubles it, up to an hour. A child's own device on this
//! PC counts nothing for it.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use limo_core::{MAX_EXTRA_MINUTES, MAX_UNLOCK_SECS, PinAction, TryOutcome};
use limo_hub::{Hub, HubError, PinOutcome};

use crate::protocol::Answer;

/// Accounts not set up that a guardian opened with the PIN, and the wait
/// the family's file last said.
#[derive(Debug, Default)]
pub(crate) struct OpenAccounts {
    open: BTreeMap<String, i64>,
    wait_until: Option<i64>,
}

impl OpenAccounts {
    /// Until when the account is open, if a guardian opened it and the time
    /// has not run out. A time run out is forgotten.
    pub(crate) fn open_until(&mut self, account_id: &str, now: i64) -> Option<i64> {
        match self.open.get(account_id) {
            Some(&until) if now < until => Some(until),
            Some(_) => {
                self.open.remove(account_id);
                None
            }
            None => None,
        }
    }

    /// The end of the wait in force after wrong guesses, as the family's
    /// file last said it, for the lock to show.
    pub(crate) fn wait_at(&self, now: i64) -> Option<i64> {
        self.wait_until.filter(|w| now < *w)
    }

    /// The guardian PIN typed on the lock of `account_id`, to do `action`:
    /// minutes open the account for that long, an unlock until then. The
    /// answer is the lock's, as the engine would give it.
    pub(crate) fn try_pin(&mut self, hub: &mut Hub, account_id: &str, pin: &str, action: PinAction, now: i64) -> Answer {
        let until = match action {
            PinAction::AddMinutes { minutes } if (1..=MAX_EXTRA_MINUTES).contains(&minutes) => {
                now.saturating_add(i64::from(minutes) * 60)
            }
            PinAction::UnlockUntil { until } if until > now && until <= now.saturating_add(MAX_UNLOCK_SECS) => until,
            PinAction::ConfirmRelease => {
                return Answer::Refused {
                    message: "There is nothing to release on an account that is not set up in LiMo.".to_string(),
                };
            }
            _ => return Answer::Refused { message: "That time cannot be chosen. Choose up to a day.".to_string() },
        };
        if !limo_core::typed_pin_is_well_formed(pin) {
            return Answer::Tried { outcome: TryOutcome::Malformed };
        }
        let outcome = match hub.check_pin(pin, now) {
            Ok(PinOutcome::Right) => {
                self.wait_until = None;
                if self.open.len() >= 64 {
                    self.open.clear();
                }
                self.open.insert(account_id.to_string(), until);
                TryOutcome::Unlocked { until }
            }
            Ok(PinOutcome::Wrong { wrong_guesses, wait_until }) => {
                self.wait_until = wait_until;
                TryOutcome::Wrong { wrong_guesses, wait_until }
            }
            Ok(PinOutcome::MustWait { until }) => {
                self.wait_until = Some(until);
                TryOutcome::MustWait { until }
            }
            Err(HubError::NoPin) => TryOutcome::NotSet,
            Err(_) => {
                return Answer::Refused {
                    message: "LiMo could not read the family's file just now. Try again in a moment.".to_string(),
                };
            }
        };
        Answer::Tried { outcome }
    }
}
