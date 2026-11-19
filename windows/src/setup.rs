//! `limo dev-setup`'s work on the family's file, with no Windows code: a
//! family, one child, a Windows account linked to the child, a small daily
//! budget, programs to block if asked, and the PIN. Scaffolding until the guardian's console exists.

#![forbid(unsafe_code)]

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
        return Err("The family's file already has a family. To start again, run limo uninstall and limo install.".to_string());
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

#[cfg(test)]
mod tests {
    use super::*;
    use limo_hub::AccountStanding;

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
}
