//! The test family on this PC: one file of the hub's own kind, in the
//! tool's own folder ([`crate::places`]), with one child and at most one
//! phone. It is made by `setup`, read by `check`, `status` and `code`, and
//! deleted by `forget`.
//!
//! The guardian password the hub asks for when a family is made is random,
//! never shown and never kept: nothing in this tool asks for it. The PIN is
//! the one typed at `setup`, kept only as the core's salted slow hash.

use limo_core::{BlockedWindow, DailyBudget, DeviceKind, RuleSet, Weekday, WeekBudgets};
use limo_hub::{AgeBand, DeviceInfo, Hub, Pairing};
use zeroize::Zeroizing;

use crate::pairing::hex;
use crate::places::Places;

/// The name the phone is paired under.
pub const PHONE_NAME: &str = "Test phone";
/// The away share of the test family: all of the day, since the phone has
/// no master to reach.
pub const AWAY_SHARE: u8 = 100;

/// What `setup` was asked to make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The child's name.
    pub child: String,
    /// Minutes a day, every day.
    pub minutes: u32,
    /// Apps that are blocked, by package name.
    pub block: Vec<String>,
    /// Apps that are always allowed, by package name.
    pub allow: Vec<String>,
    /// Hours screens are off, every day.
    pub bedtime: Option<BlockedWindow>,
    /// The child's home zone: the phone's, as it said at `setup`.
    pub utc_offset_secs: i32,
}

/// The rules the plan asks for, on top of the core's starting rules.
pub fn rules_for(plan: &Plan, mut rules: RuleSet) -> RuleSet {
    rules.utc_offset_secs = plan.utc_offset_secs;
    rules.budgets = WeekBudgets::every_day(DailyBudget::Minutes(plan.minutes));
    rules.blocked_apps = plan.block.clone();
    rules.always_allowed_apps = plan.allow.clone();
    rules.blocked_hours = plan.bedtime.iter().cloned().collect();
    rules.away_share_percent = AWAY_SHARE;
    rules
}

/// A bedtime every day from "HH:MM-HH:MM"; "24:00" may end it.
pub fn parse_bedtime(text: &str) -> Result<BlockedWindow, String> {
    let wrong = || format!("--bedtime takes two times of the day, such as 21:00-07:00, not \"{text}\".");
    let (start, end) = text.split_once('-').ok_or_else(wrong)?;
    let minute = |t: &str, end: bool| -> Option<u16> {
        let (h, m) = t.trim().split_once(':')?;
        if h.is_empty() || h.len() > 2 || m.len() != 2 {
            return None;
        }
        let h: u16 = h.parse().ok()?;
        let m: u16 = m.parse().ok()?;
        let all = h * 60 + m;
        (m < 60 && (all < 1440 || end && all == 1440)).then_some(all)
    };
    let start_minute = minute(start, false).ok_or_else(wrong)?;
    let end_minute = minute(end, true).ok_or_else(wrong)?;
    if start_minute == end_minute {
        return Err(format!("--bedtime starts and ends at the same time ({text}). To have no screen time at all, use --minutes 0."));
    }
    Ok(BlockedWindow { days: Weekday::ALL.to_vec(), start_minute, end_minute })
}

/// True for an Android package name: two or more parts with dots between,
/// each a letter and then letters, digits or underscores.
pub fn is_package_name(name: &str) -> bool {
    let parts: Vec<&str> = name.split('.').collect();
    name.len() <= 255
        && parts.len() >= 2
        && parts.iter().all(|p| {
            let mut chars = p.chars();
            chars.next().is_some_and(|c| c.is_ascii_alphabetic()) && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
}

/// "HH:MM to HH:MM".
pub fn bedtime_text(window: &BlockedWindow) -> String {
    format!("{} to {}", limo_core::rules::clock_text(window.start_minute), limo_core::rules::clock_text(window.end_minute))
}

/// What `setup` made.
#[derive(Debug)]
pub struct Made {
    /// The child's id.
    pub child_id: String,
    /// The pairing for the phone; its `Debug` hides the code secret.
    pub pairing: Pairing,
}

/// A random guardian password, never shown and never kept.
fn random_password() -> Result<Zeroizing<String>, String> {
    let mut raw = Zeroizing::new([0u8; 24]);
    getrandom::fill(&mut *raw).map_err(|_| "This PC gave no random numbers for the test family's password.".to_string())?;
    Ok(hex(&*raw))
}

/// Makes the test family in an empty file: the family with a random
/// password, the child, the rules, the PIN, and one phone paired. Refused,
/// changing nothing, when the file already has a family.
pub fn make(hub: &mut Hub, plan: &Plan, pin: &str, now: i64) -> Result<Made, String> {
    if hub.has_family().map_err(|e| e.to_string())? {
        return Err("The test family's file already has a family.".to_string());
    }
    let password = random_password()?;
    hub.create_family(&password, now).map_err(|e| format!("The test family could not be made: {e}"))?;
    let child = hub
        .add_child(&plan.child, AgeBand::TenToTwelve, plan.utc_offset_secs, now)
        .map_err(|e| format!("The child could not be added: {e}"))?;
    let rules = rules_for(plan, hub.rules(&child.child_id).map_err(|e| e.to_string())?);
    hub.replace_rules(&child.child_id, rules, now).map_err(|e| format!("The rules could not be set: {e}"))?;
    hub.set_pin(pin, now).map_err(|e| format!("The PIN could not be set: {e}"))?;
    let pairing = hub
        .pair_device(&child.child_id, PHONE_NAME, DeviceKind::Portable, "android", now)
        .map_err(|e| format!("The phone could not be paired: {e}"))?;
    Ok(Made { child_id: child.child_id, pairing })
}

/// The test family as the tool reads it back.
#[derive(Debug, Clone)]
pub struct Family {
    /// The child's id.
    pub child_id: String,
    /// The child's name.
    pub child: String,
    /// The child's rules.
    pub rules: RuleSet,
    /// The paired phone, if one is.
    pub phone: Option<DeviceInfo>,
}

impl Family {
    /// The paired phone's id, if one is.
    pub fn device_id(&self) -> Option<&str> {
        self.phone.as_ref().map(|p| p.device_id.as_str())
    }

    /// "Asha, 20 minutes a day".
    pub fn summary(&self) -> String {
        let minutes = match self.rules.budgets.monday {
            DailyBudget::Minutes(m) => format!("{m} minutes a day"),
            DailyBudget::NoLimit => "no limit".to_string(),
        };
        format!("{}, {minutes}", self.child)
    }
}

/// Opens the test family's file when there is one; None when there is
/// none (opening the hub would make one).
pub fn open(places: &Places) -> Result<Option<Hub>, String> {
    if !places.family_file().exists() {
        return Ok(None);
    }
    Hub::open(&places.family_file()).map(Some).map_err(|e| format!("The test family's file could not be opened: {e}"))
}

/// Makes the folder and opens a new file for the test family.
pub fn create(places: &Places) -> Result<Hub, String> {
    std::fs::create_dir_all(&places.dir).map_err(|e| format!("The folder {} could not be made: {e}", places.dir.display()))?;
    Hub::open(&places.family_file()).map_err(|e| format!("The test family's file could not be made: {e}"))
}

/// The test family in an open file, or None when it has none.
pub fn read(hub: &Hub) -> Result<Option<Family>, String> {
    if !hub.has_family().map_err(|e| e.to_string())? {
        return Ok(None);
    }
    let children = hub.children().map_err(|e| format!("The test family's child could not be read: {e}"))?;
    let Some(child) = children.into_iter().next() else { return Ok(None) };
    let rules = hub.rules(&child.child_id).map_err(|e| e.to_string())?;
    let phone = hub.devices(&child.child_id).map_err(|e| e.to_string())?.into_iter().next();
    Ok(Some(Family { child_id: child.child_id, child: child.name, rules, phone }))
}

/// Reads the test family, opening and closing its file; None when there is
/// none.
pub fn read_places(places: &Places) -> Result<Option<Family>, String> {
    let Some(hub) = open(places)? else { return Ok(None) };
    let family = read(&hub);
    let _ = hub.close();
    family
}

/// A one time code `code` made. Its `Debug` hides the digits.
pub struct Code {
    /// The child it is for.
    pub child: String,
    /// The code as it is read out: "1342 0715".
    pub shown: Zeroizing<String>,
    /// The minutes it adds.
    pub minutes: u32,
}

impl std::fmt::Debug for Code {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Code").field("child", &self.child).field("minutes", &self.minutes).field("shown", &"hidden").finish()
    }
}

/// The next one time code for the test phone, worth `minutes`, for the
/// child's day now. Each call makes a new one, and the file records it, so
/// a phone takes at most ten a day.
pub fn make_code(hub: &mut Hub, minutes: u32, now: i64) -> Result<Code, String> {
    let Some(family) = read(hub)? else { return Err("There is no test family on this PC. Run setup first.".to_string()) };
    let Some(device) = family.device_id() else {
        return Err("The test family has no phone. Run forget, then setup again.".to_string());
    };
    let code = hub.make_code(device, minutes, now).map_err(|e| format!("The code could not be made: {e}"))?;
    Ok(Code { child: family.child, shown: Zeroizing::new(code.shown.clone()), minutes: code.minutes })
}

/// Deletes the test family's file and its journal. The log stays.
pub fn delete(places: &Places) -> Result<(), String> {
    for path in [places.family_file(), places.family_journal()] {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{} could not be deleted: {e}", path.display())),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bedtime_is_two_times_and_may_end_at_midnight() {
        assert_eq!(parse_bedtime("21:00-07:00").unwrap(), BlockedWindow { days: Weekday::ALL.to_vec(), start_minute: 1260, end_minute: 420 });
        assert_eq!(parse_bedtime("9:30-24:00").unwrap().end_minute, 1440);
        for bad in ["21:00", "21-07", "25:00-07:00", "21:00-07:60", "24:00-07:00", "21:00-7", ""] {
            assert!(parse_bedtime(bad).is_err(), "{bad}");
        }
        assert!(parse_bedtime("21:00-21:00").unwrap_err().contains("--minutes 0"));
        assert_eq!(bedtime_text(&parse_bedtime("21:00-07:00").unwrap()), "21:00 to 07:00");
    }

    #[test]
    fn package_names_are_checked() {
        for good in ["com.example.game", "org.example.game_2", "a.b"] {
            assert!(is_package_name(good), "{good}");
        }
        for bad in ["game", "com..game", "com.2game", ".com.game", "com.game.", "com.ga me", "notepad.exe!", ""] {
            assert!(!is_package_name(bad), "{bad}");
        }
    }

    #[test]
    fn the_rules_have_the_whole_day_away_and_what_was_asked() {
        let plan = Plan {
            child: "Asha".to_string(),
            minutes: 20,
            block: vec!["com.example.game".to_string()],
            allow: vec!["com.example.clock".to_string()],
            bedtime: Some(parse_bedtime("21:00-07:00").unwrap()),
            utc_offset_secs: 19_800,
        };
        let rules = rules_for(&plan, RuleSet::new(1, 0));
        assert!(rules.is_valid(), "{:?}", rules.validate());
        assert_eq!(rules.away_share_percent, 100);
        assert_eq!(rules.budgets, WeekBudgets::every_day(DailyBudget::Minutes(20)));
        assert_eq!(rules.utc_offset_secs, 19_800);
        assert_eq!(rules.blocked_hours.len(), 1);
        assert_eq!(rules.blocked_apps, vec!["com.example.game".to_string()]);
        assert_eq!(rules.always_allowed_apps, vec!["com.example.clock".to_string()]);
    }
}
