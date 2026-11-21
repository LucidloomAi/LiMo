//! What counts on a phone, and what LiMo never covers. Both are kept here,
//! each in one function, so that they can change in one place.

use crate::facts::{CallState, Front, PhoneFacts};

/// **What counts on a phone.** The app the child is using now, if any: its
/// time is recorded, and counts against the budget unless the rules list it
/// as always allowed (that part is the rules core's own rule).
///
/// The owner's decision of 19 Nov 2026: time counts while the screen is on
/// and unlocked with an app that counts in front. Sound with the screen off
/// does not count.
///
/// - The screen off does not count, even with sound playing: music or a
///   video heard with the screen off is not screen time.
/// - Android's own lock screen does not count: the phone is not in use yet.
/// - There is no idle rule. The screen's own timeout plays that part: a
///   phone left alone turns its screen off.
/// - The home screen, LiMo itself, the dialler and the emergency dialler
///   never count.
/// - Nothing counts while a call rings. **A call in progress is no way
///   around the limits**: the dialler (the call's own screen) does not
///   count, but any other app in front counts as it would with no call.
/// - An app the host cannot name (its watch on the app in front is off)
///   counts, under an empty name: the screen is in use all the same.
///
/// What is covered is not in use either; the engine leaves that out itself,
/// so a blocked app, or anything under the lock, never counts.
pub fn app_in_use(facts: &PhoneFacts) -> Option<String> {
    if !facts.interactive || facts.keyguard_locked || facts.call == CallState::Ringing {
        return None;
    }
    match &facts.front {
        Front::App { package, .. } => Some(package.clone()),
        Front::Unknown => Some(String::new()),
        Front::Home | Front::LiMo | Front::Dialler | Front::EmergencyDialler => None,
    }
}

/// **What LiMo never covers.** True when a cover may be drawn now. A lock
/// must never stand between a child and a call for help, so nothing is
/// covered while a call rings (it can always be answered), and the dialler
/// (the screen of a call in progress among it), the emergency dialler and
/// LiMo's own screens (where the ways out are) are never covered, call or
/// no call. Any other app is covered during a call in progress exactly as it
/// would be with no call, and the home screen under a lock as well: the
/// cover then offers a way back to the call. Android's own lock screen is
/// never covered either: it has its own emergency call, and the phone is not
/// in use behind it; nor is a screen that is off.
pub fn may_cover(facts: &PhoneFacts) -> bool {
    facts.interactive
        && !facts.keyguard_locked
        && facts.call != CallState::Ringing
        && !matches!(facts.front, Front::LiMo | Front::Dialler | Front::EmergencyDialler)
}

/// True while the child holds the phone, on and unlocked: a phone locked for
/// lack of time asks the master for more.
pub(crate) fn screen_in_use(facts: &PhoneFacts) -> bool {
    facts.interactive && !facts.keyguard_locked
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::PhonePermissions;

    fn facts(front: Front) -> PhoneFacts {
        PhoneFacts {
            wall_secs: 1_800_000_000,
            elapsed_ms: 60_000,
            boot_count: 1,
            utc_offset_secs: 19_800,
            hours_24: false,
            auto_time: true,
            interactive: true,
            keyguard_locked: false,
            call: CallState::Idle,
            front,
            in_reach: false,
            prodded: false,
            permissions: PhonePermissions::ALL,
            guarded: None,
            usb_debugging: None,
        }
    }

    #[test]
    fn an_app_in_front_of_an_unlocked_screen_counts() {
        assert_eq!(app_in_use(&facts(Front::app("org.example.game"))), Some("org.example.game".into()));
        // An app the host cannot name counts too, under an empty name.
        assert_eq!(app_in_use(&facts(Front::Unknown)), Some(String::new()));
    }

    #[test]
    fn the_screen_off_or_locked_never_counts_even_with_sound() {
        let mut off = facts(Front::app("org.example.music"));
        off.interactive = false;
        assert_eq!(app_in_use(&off), None);
        let mut locked = facts(Front::app("org.example.game"));
        locked.keyguard_locked = true;
        assert_eq!(app_in_use(&locked), None);
    }

    #[test]
    fn home_limo_the_diallers_and_a_ringing_call_never_count() {
        for front in [Front::Home, Front::LiMo, Front::Dialler, Front::EmergencyDialler] {
            assert_eq!(app_in_use(&facts(front.clone())), None, "{front:?}");
        }
        let mut ringing = facts(Front::app("org.example.game"));
        ringing.call = CallState::Ringing;
        assert_eq!(app_in_use(&ringing), None);
    }

    #[test]
    fn a_call_in_progress_is_no_way_around_the_count() {
        let mut on_call = facts(Front::app("org.example.game"));
        on_call.call = CallState::Ongoing;
        assert_eq!(app_in_use(&on_call), Some("org.example.game".into()));
        for front in [Front::Dialler, Front::EmergencyDialler, Front::LiMo, Front::Home] {
            on_call.front = front.clone();
            assert_eq!(app_in_use(&on_call), None, "{front:?}");
        }
    }

    #[test]
    fn nothing_is_covered_over_a_ringing_call_the_diallers_limo_or_the_keyguard() {
        assert!(may_cover(&facts(Front::app("org.example.game"))));
        assert!(may_cover(&facts(Front::Home)));
        assert!(may_cover(&facts(Front::Unknown)));
        for call in [CallState::Idle, CallState::Ringing, CallState::Ongoing] {
            for front in [Front::LiMo, Front::Dialler, Front::EmergencyDialler] {
                let mut f = facts(front.clone());
                f.call = call;
                assert!(!may_cover(&f), "{call:?} {front:?}");
            }
        }
        for front in [Front::app("org.example.game"), Front::Home, Front::Unknown] {
            let mut ringing = facts(front.clone());
            ringing.call = CallState::Ringing;
            assert!(!may_cover(&ringing), "{front:?}");
            // A call in progress: other apps, and the home screen, as with
            // no call.
            let mut ongoing = facts(front.clone());
            ongoing.call = CallState::Ongoing;
            assert!(may_cover(&ongoing), "{front:?}");
        }
        let mut keyguard = facts(Front::app("org.example.game"));
        keyguard.keyguard_locked = true;
        assert!(!may_cover(&keyguard));
        let mut off = facts(Front::app("org.example.game"));
        off.interactive = false;
        assert!(!may_cover(&off));
    }
}
