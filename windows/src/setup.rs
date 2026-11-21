//! `limo dev-setup`'s, `limo dev-code`'s and `limo reset-password`'s work on
//! the family's file, with no Windows code. `dev-setup`: a family, one
//! child, a Windows account linked to the child, a small daily budget,
//! programs to block if asked, and the PIN; scaffolding until the guardian's
//! console exists. `dev-code`: a one time code for the child's device on
//! this PC; scaffolding until the guardian's phone app exists.
//! `reset-password`: a new guardian password without the old one, for an
//! administrator of this PC whose family forgot it; everything else is
//! kept.

#![forbid(unsafe_code)]

use std::fmt;

use limo_core::{DailyBudget, WeekBudgets};
use limo_hub::{AccountLink, AgeBand, Hub};

/// What `dev-setup` needs, gathered from the command line and the prompts.
pub struct DevFamily<'a> {
    /// The guardian password, as typed.
    pub password: &'a str,
    /// The guardian PIN, as typed.
    pub pin: &'a str,
    /// The child's name.
    pub child: &'a str,
    /// The Windows account's security identifier.
    pub account_id: &'a str,
    /// The Windows account's name.
    pub account_name: &'a str,
    /// The child's daily budget, in minutes.
    pub minutes: u32,
    /// Programs to block for the child, by file name.
    pub block: &'a [String],
    /// The PC's time zone now, seconds from UTC: the child's home zone.
    pub utc_offset_secs: i32,
}

impl std::fmt::Debug for DevFamily<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DevFamily")
            .field("child", &self.child)
            .field("account_name", &self.account_name)
            .field("minutes", &self.minutes)
            .field("block", &self.block)
            .finish_non_exhaustive()
    }
}

/// Makes the family in an empty file. Refused, changing nothing, when the
/// file already has a family. Each step is the hub's own call; a failure
/// part way is reported, and the file keeps the steps before it.
pub fn make_dev_family(hub: &mut Hub, family: &DevFamily<'_>, now: i64) -> Result<String, String> {
    if hub.has_family().map_err(|e| e.to_string())? {
        return Err("The family’s file already has a family. To start again, run limo uninstall and limo install.".to_string());
    }
    hub.create_family(family.password, now).map_err(|e| format!("The family could not be made: {e}"))?;
    let child = hub
        .add_child(family.child, AgeBand::TenToTwelve, family.utc_offset_secs, now)
        .map_err(|e| format!("The child could not be added: {e}"))?;
    hub.link_account(family.account_id, family.account_name, AccountLink::Child { child_id: child.child_id.clone() })
        .map_err(|e| format!("The account could not be linked: {e}"))?;
    let mut rules = hub.rules(&child.child_id).map_err(|e| e.to_string())?;
    rules.budgets = WeekBudgets::every_day(DailyBudget::Minutes(family.minutes));
    rules.blocked_apps = family.block.to_vec();
    hub.replace_rules(&child.child_id, rules, now).map_err(|e| format!("The budget could not be set: {e}"))?;
    hub.set_pin(family.pin, now).map_err(|e| format!("The PIN could not be set: {e}"))?;
    Ok(child.child_id)
}

/// A one time code `dev-code` made. Its `Debug` hides the digits.
pub struct DevCode {
    /// The child it is for.
    pub child: String,
    /// The code as it is read out: "1342 0715".
    pub shown: String,
    /// The minutes it adds.
    pub minutes: u32,
}

impl fmt::Debug for DevCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DevCode").field("child", &self.child).field("minutes", &self.minutes).field("shown", &"hidden").finish()
    }
}

/// Makes the next one time code for a child's device on this PC, worth
/// `minutes`, for the child's home day now. The child is the one named, or
/// the only child when none is named. The device is paired first if the
/// engine has not paired it yet, under `pc_name`, just as the engine does
/// the first time the child is seen here; so the code works as soon as the
/// service runs again. Each call makes a new code, and the hub records it,
/// so a device takes at most ten a day.
pub fn make_dev_code(hub: &mut Hub, child: Option<&str>, minutes: u32, pc_name: &str, now: i64) -> Result<DevCode, String> {
    let children = hub.children().map_err(|e| format!("The family’s children could not be read: {e}"))?;
    let found = match child {
        Some(name) => children.iter().find(|c| c.name.trim().eq_ignore_ascii_case(name.trim())),
        None if children.len() == 1 => children.first(),
        None if children.is_empty() => return Err("The family has no child yet. Run limo dev-setup first.".to_string()),
        None => return Err("The family has more than one child: say which with --child NAME.".to_string()),
    };
    let Some(found) = found else {
        return Err("The family has no child of that name.".to_string());
    };
    let device = hub
        .pair_master_pc(&found.child_id, pc_name, now)
        .map_err(|e| format!("The child’s device on this PC could not be had: {e}"))?;
    let code = hub.make_code(&device.device_id, minutes, now).map_err(|e| format!("The code could not be made: {e}"))?;
    Ok(DevCode { child: found.name.clone(), shown: code.shown.clone(), minutes: code.minutes })
}

/// Why a new guardian password cannot be taken, in the family file's own
/// words, before anything is stopped or opened; None when it can.
pub fn new_password_problem(password: &str) -> Option<String> {
    let chars = password.chars().count();
    if chars < limo_hub::MIN_PASSWORD_CHARS as usize {
        return Some(format!("The password must be at least {} characters long.", limo_hub::MIN_PASSWORD_CHARS));
    }
    if chars > limo_hub::MAX_PASSWORD_CHARS as usize {
        return Some(format!("The password must be at most {} characters long.", limo_hub::MAX_PASSWORD_CHARS));
    }
    None
}

/// `limo reset-password`'s work on the family's file: a new guardian
/// password without the old one, for an administrator of this PC whose
/// family forgot it. The wrong guesses and their wait are cleared, the
/// family's events record it, and everything else is kept: the PIN, the
/// children, their rules and their devices. Refused, changing nothing, when
/// there is no family yet or the new password cannot be taken.
pub fn reset_family_password(hub: &mut Hub, new: &str, now: i64) -> Result<(), String> {
    if !hub.has_family().map_err(|e| format!("The family’s file could not be read: {e}"))? {
        return Err("There is no family on this PC yet, so there is no password to reset. Nothing was changed.".to_string());
    }
    if let Some(problem) = new_password_problem(new) {
        return Err(format!("{problem} Nothing was changed."));
    }
    hub.reset_password(new, now).map_err(|e| format!("The password was not reset: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use limo_hub::{AccountStanding, EventKind, PasswordOutcome};

    #[test]
    fn reset_password_sets_a_new_one_clears_the_wait_and_keeps_everything_else() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = Hub::open(&dir.path().join("family.limo")).unwrap();
        let now = 1_800_000_000;
        assert!(reset_family_password(&mut hub, "a new password", now).unwrap_err().contains("no family"));
        let family = DevFamily {
            password: "correct horse battery",
            pin: "482915",
            child: "Asha",
            account_id: "S-1-5-21-1-2-3-1001",
            account_name: "asha",
            minutes: 10,
            block: &[],
            utc_offset_secs: 19_800,
        };
        let child = make_dev_family(&mut hub, &family, now).unwrap();
        // Forgotten: five wrong tries, and a wait.
        for _ in 0..5 {
            hub.check_password("not the password", now).unwrap();
        }
        assert!(matches!(hub.check_password("correct horse battery", now).unwrap(), PasswordOutcome::MustWait { .. }));
        // A new password that cannot be taken changes nothing.
        assert!(reset_family_password(&mut hub, "short", now).unwrap_err().contains("at least 8"));
        reset_family_password(&mut hub, "a new password", now).unwrap();
        // The new one is right at once; the old one is not.
        assert_eq!(hub.check_password("a new password", now).unwrap(), PasswordOutcome::Right);
        assert!(matches!(hub.check_password("correct horse battery", now).unwrap(), PasswordOutcome::Wrong { .. }));
        // Everything else is kept, and the events say so.
        assert!(hub.has_pin().unwrap());
        assert_eq!(hub.child(&child).unwrap().name, "Asha");
        assert_eq!(hub.account_standing("S-1-5-21-1-2-3-1001").unwrap(), AccountStanding::Child { child_id: child.clone() });
        assert_eq!(hub.rules(&child).unwrap().budgets, WeekBudgets::every_day(DailyBudget::Minutes(10)));
        assert!(hub.events(50, None).unwrap().iter().any(|e| e.what == EventKind::PasswordReset));
        assert_eq!(new_password_problem(&"x".repeat(257)).as_deref(), Some("The password must be at most 256 characters long."));
        assert_eq!(new_password_problem("eight ch"), None);
    }

    #[test]
    fn a_dev_family_is_made_once_with_its_child_account_budget_and_pin() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("family.limo");
        let mut hub = Hub::open(&path).unwrap();
        let family = DevFamily {
            password: "correct horse battery",
            pin: "482915",
            child: "Asha",
            account_id: "S-1-5-21-1-2-3-1001",
            account_name: "asha",
            minutes: 10,
            block: &["notepad.exe".to_string()],
            utc_offset_secs: 19_800,
        };
        let now = 1_800_000_000;
        let child = make_dev_family(&mut hub, &family, now).unwrap();
        assert_eq!(hub.account_standing("S-1-5-21-1-2-3-1001").unwrap(), AccountStanding::Child { child_id: child.clone() });
        let rules = hub.rules(&child).unwrap();
        assert_eq!(rules.budgets, WeekBudgets::every_day(DailyBudget::Minutes(10)));
        assert_eq!(rules.blocked_apps, vec!["notepad.exe".to_string()]);
        assert!(hub.has_pin().unwrap());
        assert_eq!(hub.child(&child).unwrap().name, "Asha");
        // Neither the PIN nor the password shows in its Debug output.
        let shown = format!("{family:?}");
        assert!(!shown.contains("482915") && !shown.contains("horse"), "{shown}");
        // A second time, refused, changing nothing.
        let again = make_dev_family(&mut hub, &DevFamily { child: "Mira", ..family }, now).unwrap_err();
        assert!(again.contains("already has a family"), "{again}");
        assert_eq!(hub.children().unwrap().len(), 1);
    }

    #[test]
    fn a_dev_code_works_once_on_the_childs_device_on_this_pc() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = Hub::open(&dir.path().join("family.limo")).unwrap();
        let now = 1_800_000_000;
        assert!(make_dev_code(&mut hub, None, 15, "Family PC", now).is_err());
        let family = DevFamily {
            password: "correct horse battery",
            pin: "482915",
            child: "Asha",
            account_id: "S-1-5-21-1-2-3-1001",
            account_name: "asha",
            minutes: 10,
            block: &[],
            utc_offset_secs: 19_800,
        };
        let child = make_dev_family(&mut hub, &family, now).unwrap();
        // Before the engine has seen the child here: the device is paired.
        assert!(hub.master_pc(&child).unwrap().is_none());
        let code = make_dev_code(&mut hub, None, 30, "Family PC", now).unwrap();
        assert_eq!((code.child.as_str(), code.minutes), ("Asha", 30));
        assert!(limo_core::code_digits(&code.shown).is_some(), "not 8 digits");
        assert!(!format!("{code:?}").contains(&code.shown[..4]));
        // It is the device the engine will load, and the code works on it,
        // once.
        let mut state = hub.master_pc(&child).unwrap().unwrap().state;
        assert!(matches!(state.try_code(&code.shown, now), limo_core::TryOutcome::Added { .. }));
        assert!(matches!(state.try_code(&code.shown, now), limo_core::TryOutcome::AlreadyUsed));
        // The next is another code; by name too.
        let next = make_dev_code(&mut hub, Some("asha"), 15, "Family PC", now).unwrap();
        assert_ne!(next.shown, code.shown);
        assert!(make_dev_code(&mut hub, Some("Mira"), 15, "Family PC", now).unwrap_err().contains("no child of that name"));
        assert!(make_dev_code(&mut hub, None, 20, "Family PC", now).is_err());
        // Two children: say which.
        hub.add_child("Mira", AgeBand::TenToTwelve, 19_800, now).unwrap();
        assert!(make_dev_code(&mut hub, None, 15, "Family PC", now).unwrap_err().contains("--child"));
        assert_eq!(make_dev_code(&mut hub, Some("Mira"), 60, "Family PC", now).unwrap().child, "Mira");
    }
}
