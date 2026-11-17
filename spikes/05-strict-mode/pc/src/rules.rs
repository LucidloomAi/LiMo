//! What may run, what must be refused, and what each known failure means in
//! plain words. Pure, so every rule is unit tested.

use crate::parse::{Accounts, DeviceLine, Owner, OwnerError, Suspended, User};

pub const PACKAGE: &str = "ai.lucidloom.limo.spike05";
pub const COMPONENT: &str = "ai.lucidloom.limo.spike05/.AdminReceiver";
pub const ACTIVITY: &str = "ai.lucidloom.limo.spike05/.MainActivity";
pub const LOG_TAG: &str = "LiMoSpike05";

/// The one device to talk to, or why there is none.
pub fn pick_device(list: &[DeviceLine]) -> Result<String, String> {
    match list {
        [] => Err("No phone found. Plug it in with a USB cable, unlock it, and check that USB debugging is on (Settings, Developer options).".to_string()),
        [one] => match one.state.as_str() {
            "device" => Ok(one.serial.clone()),
            "unauthorized" => Err(format!(
                "The phone {} has not allowed this PC yet. Unlock it and tap Allow on the \"Allow USB debugging?\" prompt.",
                one.serial
            )),
            "offline" => Err(format!(
                "The phone {} is offline. Unplug it and plug it in again.",
                one.serial
            )),
            other => Err(format!(
                "The phone {} is in the state \"{other}\", not ready for adb.",
                one.serial
            )),
        },
        many => Err(format!(
            "More than one phone or emulator is connected ({}). Unplug the others or close the emulator, so this program cannot pick the wrong one.",
            many.iter()
                .map(|d| format!("{} {}", d.serial, d.state))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// Is LiMo spike 05 the Device Owner? None if the owners could not be read.
pub fn owner_is_us(owners: &Option<Vec<Owner>>) -> Option<bool> {
    owners.as_ref().map(|list| {
        list.iter()
            .any(|o| o.device_owner && o.component.starts_with(&format!("{PACKAGE}/")))
    })
}

/// Everything the PC read about the phone for `check` and `setup`.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    pub android: Option<String>,
    pub sdk: Option<String>,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub users: Option<Vec<User>>,
    pub accounts: Option<Accounts>,
    pub owners: Option<Vec<Owner>>,
    pub installed: Option<bool>,
    pub test_only: Option<bool>,
    pub admin: Option<bool>,
    pub device_provisioned: Option<String>,
    pub user_setup_complete: Option<String>,
}

/// What stands in the way of `dpm set-device-owner`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blocker {
    Accounts { count: u32, types: Vec<String> },
    AccountsUnreadable,
    SeveralUsers { users: Vec<User> },
    UsersUnreadable,
    AlreadyOwnerUs,
    OtherDeviceOwner { component: String },
    ProfileOwner { user: u32, component: String },
    OwnersUnreadable,
}

/// Every blocker, in the order a person should deal with them. Anything that
/// could not be read is a blocker too: if in doubt, do not set an owner.
pub fn blockers(f: &Facts) -> Vec<Blocker> {
    let mut v = Vec::new();
    match &f.owners {
        None => v.push(Blocker::OwnersUnreadable),
        Some(list) => {
            for o in list {
                let ours = o.component.starts_with(&format!("{PACKAGE}/"));
                if o.device_owner && ours {
                    v.push(Blocker::AlreadyOwnerUs);
                } else if o.device_owner {
                    v.push(Blocker::OtherDeviceOwner {
                        component: o.component.clone(),
                    });
                } else if o.profile_owner {
                    v.push(Blocker::ProfileOwner {
                        user: o.user,
                        component: o.component.clone(),
                    });
                }
            }
        }
    }
    match &f.accounts {
        None => v.push(Blocker::AccountsUnreadable),
        Some(a) if a.count > 0 => v.push(Blocker::Accounts {
            count: a.count,
            types: a.types.clone(),
        }),
        Some(_) => {}
    }
    match &f.users {
        None => v.push(Blocker::UsersUnreadable),
        Some(u) if u.is_empty() => v.push(Blocker::UsersUnreadable),
        Some(u) if u.len() > 1 => v.push(Blocker::SeveralUsers { users: u.clone() }),
        Some(_) => {}
    }
    v
}

/// One blocker in plain words, with what to do about it.
pub fn explain_blocker(b: &Blocker) -> String {
    match b {
        Blocker::Accounts { count, types } => format!(
            "There {} {count} account{} on the phone (types: {}). Android refuses to set a Device Owner while any account exists. \
             Remove every account: Settings, Accounts and backup, Manage accounts. Samsung's own apps can add accounts that are easy to miss; \
             the types above say which apps they belong to. Then run check again.",
            if *count == 1 { "is" } else { "are" },
            if *count == 1 { "" } else { "s" },
            if types.is_empty() { "not shown".to_string() } else { types.join(", ") }
        ),
        Blocker::AccountsUnreadable => "The accounts on the phone could not be read (adb shell dumpsys account). Without knowing there are none, setup will not go on.".to_string(),
        Blocker::SeveralUsers { users } => format!(
            "The phone has {} users ({}). Android refuses to set a Device Owner unless there is exactly one. \
             On a Samsung phone Secure Folder (often user 150) and Dual Messenger (often user 95) each count as a user: \
             remove Secure Folder and turn Dual Messenger off for every app, then run check again.",
            users.len(),
            users.iter().map(|u| format!("{} \"{}\"", u.id, u.name)).collect::<Vec<_>>().join(", ")
        ),
        Blocker::UsersUnreadable => "The users on the phone could not be read (adb shell pm list users). Without knowing there is only one, setup will not go on.".to_string(),
        Blocker::AlreadyOwnerUs => "LiMo spike 05 already is the Device Owner. Nothing to set up. Use status, or release it first.".to_string(),
        Blocker::OtherDeviceOwner { component } => format!(
            "Another app is already the Device Owner: {component}. A phone has only one. This spike never touches another app's ownership."
        ),
        Blocker::ProfileOwner { user, component } => format!(
            "A profile owner is set on user {user}: {component}. That is usually a work profile. Android refuses a Device Owner while it is there."
        ),
        Blocker::OwnersUnreadable => "The owners on the phone could not be read (adb shell dpm list-owners). Without knowing there are none, setup will not go on.".to_string(),
    }
}

/// One known set-device-owner failure in plain words.
pub fn explain_owner_error(e: OwnerError) -> &'static str {
    match e {
        OwnerError::Accounts => "Android says there are accounts on the phone. Remove every account (Settings, Accounts and backup, Manage accounts), then run check again.",
        OwnerError::SeveralUsers => "Android says there is more than one user. On a Samsung phone that is often Secure Folder or Dual Messenger. Remove them, then run check again.",
        OwnerError::OwnerAlreadySet => "Android says a Device Owner is already set. Run status to see which.",
        OwnerError::ProfileOwnerSet => "Android says a profile owner (a work profile) is already set on this user.",
        OwnerError::AlreadySetUp => "Android says the phone is already set up (\"set-up\" or \"provisioned\"). Stock Android 13 lets adb set an owner after setup as long as there are no accounts and one user, but some builds refuse anyway. This spike does not try to get round it; write it down in the results.",
        OwnerError::UnknownAdmin => "Android does not know the admin receiver: the app is not installed, or the APK is a different build. Run check.",
        OwnerError::UserNotRunning => "Android says the user is not running. Unlock the phone and try again later by hand.",
        OwnerError::NotSystemUser => "Android says the owner can only be set on the phone's main (system) user.",
        OwnerError::Paired => "Android says the device is already paired (this applies to watches).",
        OwnerError::ShellNotAllowed => "Android did not let adb's shell do this. Check that USB debugging is on and that the phone was unlocked when the PC was allowed.",
    }
}

/// One adb install failure code in plain words.
pub fn explain_install_failure(code: &str) -> String {
    match code {
        "INSTALL_FAILED_TEST_ONLY" => "The APK is a test only build and needs adb install -t. setup always passes -t, so this should not happen with it.".to_string(),
        "INSTALL_FAILED_UPDATE_INCOMPATIBLE" => format!(
            "A copy of {PACKAGE} signed with a different key is installed (built on another PC?). If status shows it is neither owner nor admin, remove it with uninstall and run setup again."
        ),
        "INSTALL_FAILED_VERSION_DOWNGRADE" => "A newer version of the app is installed. Remove it with uninstall first.".to_string(),
        "INSTALL_FAILED_INSUFFICIENT_STORAGE" => "The phone is out of storage.".to_string(),
        "INSTALL_FAILED_OLDER_SDK" => "The phone's Android is older than the app's minimum, Android 9.".to_string(),
        "INSTALL_FAILED_USER_RESTRICTED" => "The phone refused installing over USB. Some phones have a separate switch for it in Developer options.".to_string(),
        "INSTALL_FAILED_ABORTED" => "The install was cancelled on the phone.".to_string(),
        c if c.starts_with("INSTALL_PARSE_FAILED") => "The APK file is damaged or not signed. Build it again.".to_string(),
        "unknown" => "adb did not say why. Check the path to the APK.".to_string(),
        other => format!("Android refused the install with {other}."),
    }
}

/// Why `uninstall` must not run, or None if it may. Unknown counts as "no".
pub fn uninstall_refusal(owner: Option<bool>, admin: Option<bool>) -> Option<String> {
    match (owner, admin) {
        (Some(true), _) => Some("LiMo spike 05 is still the Device Owner. Release it first: release --pin 2468, or force-release.".to_string()),
        (_, Some(true)) => Some("LiMo spike 05 is still an active admin. Release it first: release --pin 2468, or force-release.".to_string()),
        (None, _) => Some("Could not read whether LiMo spike 05 is the Device Owner (dpm list-owners). Not uninstalling while in doubt.".to_string()),
        (_, None) => Some("Could not read whether LiMo spike 05 is an active admin (dumpsys device_policy). Not uninstalling while in doubt.".to_string()),
        (Some(false), Some(false)) => None,
    }
}

/// Whether there is anything to release. Only "known to be neither owner nor
/// admin" says no; in doubt, ask the app (or run the escape hatch) anyway.
pub fn anything_to_release(owner: Option<bool>, admin: Option<bool>) -> bool {
    !(owner == Some(false) && admin == Some(false))
}

/// Suspensions a Device Owner may have made: those whose suspender is
/// "android" (Android 13 suspends a Device Owner's packages under
/// PLATFORM_PACKAGE_NAME, believed to be "android"; that value was not
/// checked in the source), and, to be safe, those whose suspender is not
/// shown. A suspension that names only some other app is that app's.
pub fn owner_suspensions(list: &[Suspended]) -> Vec<String> {
    list.iter()
        .filter(|s| s.by.is_empty() || s.by.iter().any(|b| b == "android"))
        .map(|s| s.package.clone())
        .collect()
}

/// The four restrictions this spike sets, by their Android names.
pub const SPIKE_RESTRICTIONS: [&str; 4] = ["no_safe_boot", "no_factory_reset", "no_add_user", "no_config_date_time"];

/// The spike's own restrictions found in force.
pub fn spike_restrictions_in_force(effective: &[String]) -> Vec<String> {
    SPIKE_RESTRICTIONS
        .iter()
        .filter(|r| effective.iter().any(|e| e == *r))
        .map(|r| r.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(serial: &str, state: &str) -> DeviceLine {
        DeviceLine {
            serial: serial.into(),
            state: state.into(),
        }
    }

    fn owner(component: &str, device: bool, profile: bool) -> Owner {
        Owner {
            user: 0,
            component: component.into(),
            device_owner: device,
            profile_owner: profile,
        }
    }

    fn healthy() -> Facts {
        Facts {
            android: Some("13".into()),
            sdk: Some("33".into()),
            manufacturer: Some("samsung".into()),
            model: Some("SM-G988B".into()),
            users: Some(vec![User { id: 0, name: "Owner".into() }]),
            accounts: Some(Accounts { count: 0, types: vec![] }),
            owners: Some(vec![]),
            installed: Some(false),
            test_only: None,
            admin: Some(false),
            device_provisioned: Some("1".into()),
            user_setup_complete: Some("1".into()),
        }
    }

    #[test]
    fn picks_exactly_one_ready_device() {
        assert_eq!(pick_device(&[dev("R5CN70", "device")]), Ok("R5CN70".to_string()));
        assert!(pick_device(&[]).unwrap_err().contains("No phone found"));
        assert!(pick_device(&[dev("R5", "unauthorized")]).unwrap_err().contains("Allow"));
        assert!(pick_device(&[dev("R5", "offline")]).unwrap_err().contains("offline"));
        assert!(pick_device(&[dev("R5", "recovery")]).unwrap_err().contains("recovery"));
        let two = pick_device(&[dev("R5", "device"), dev("emulator-5554", "device")]).unwrap_err();
        assert!(two.contains("More than one") && two.contains("emulator-5554"));
    }

    #[test]
    fn a_clean_phone_has_no_blockers() {
        assert_eq!(blockers(&healthy()), vec![]);
    }

    #[test]
    fn accounts_found() {
        let mut f = healthy();
        f.accounts = Some(Accounts {
            count: 2,
            types: vec!["com.google".into(), "com.osp.app.signin".into()],
        });
        let b = blockers(&f);
        assert_eq!(
            b,
            vec![Blocker::Accounts {
                count: 2,
                types: vec!["com.google".into(), "com.osp.app.signin".into()]
            }]
        );
        let text = explain_blocker(&b[0]);
        assert!(text.contains("2 accounts") && text.contains("com.osp.app.signin"));
    }

    #[test]
    fn two_users() {
        let mut f = healthy();
        f.users = Some(vec![
            User { id: 0, name: "Owner".into() },
            User { id: 150, name: "Secure Folder".into() },
        ]);
        let b = blockers(&f);
        assert!(matches!(&b[..], [Blocker::SeveralUsers { users }] if users.len() == 2));
        assert!(explain_blocker(&b[0]).contains("150 \"Secure Folder\""));
    }

    #[test]
    fn owner_already_set() {
        let mut f = healthy();
        f.owners = Some(vec![owner("com.other.mdm/.Admin", true, false)]);
        assert_eq!(
            blockers(&f),
            vec![Blocker::OtherDeviceOwner {
                component: "com.other.mdm/.Admin".into()
            }]
        );
        f.owners = Some(vec![owner(COMPONENT, true, false)]);
        assert_eq!(blockers(&f), vec![Blocker::AlreadyOwnerUs]);
        f.owners = Some(vec![owner("com.work/.Po", false, true)]);
        assert!(matches!(&blockers(&f)[..], [Blocker::ProfileOwner { .. }]));
    }

    #[test]
    fn anything_unreadable_blocks() {
        let mut f = healthy();
        f.owners = None;
        f.accounts = None;
        f.users = None;
        assert_eq!(
            blockers(&f),
            vec![Blocker::OwnersUnreadable, Blocker::AccountsUnreadable, Blocker::UsersUnreadable]
        );
        let mut g = healthy();
        g.users = Some(vec![]);
        assert_eq!(blockers(&g), vec![Blocker::UsersUnreadable]);
    }

    #[test]
    fn everything_at_once_in_order() {
        let mut f = healthy();
        f.owners = Some(vec![owner("com.other.mdm/.Admin", true, false)]);
        f.accounts = Some(Accounts { count: 1, types: vec!["com.google".into()] });
        f.users = Some(vec![User { id: 0, name: "Owner".into() }, User { id: 95, name: "DUAL_APP".into() }]);
        let b = blockers(&f);
        assert_eq!(b.len(), 3);
        assert!(matches!(b[0], Blocker::OtherDeviceOwner { .. }));
        assert!(matches!(b[1], Blocker::Accounts { count: 1, .. }));
        assert!(matches!(b[2], Blocker::SeveralUsers { .. }));
        assert!(explain_blocker(&b[1]).contains("There is 1 account on"));
    }

    #[test]
    fn owner_is_us_reads_the_list() {
        assert_eq!(owner_is_us(&None), None);
        assert_eq!(owner_is_us(&Some(vec![])), Some(false));
        assert_eq!(owner_is_us(&Some(vec![owner(COMPONENT, true, false)])), Some(true));
        assert_eq!(owner_is_us(&Some(vec![owner("com.other/.A", true, false)])), Some(false));
        // Our package as a mere profile owner is not "Device Owner".
        assert_eq!(owner_is_us(&Some(vec![owner(COMPONENT, false, true)])), Some(false));
    }

    #[test]
    fn uninstall_is_refused_while_owner_admin_or_unknown() {
        assert!(uninstall_refusal(Some(true), Some(true)).unwrap().contains("Device Owner"));
        assert!(uninstall_refusal(Some(false), Some(true)).unwrap().contains("active admin"));
        assert!(uninstall_refusal(None, Some(false)).unwrap().contains("in doubt"));
        assert!(uninstall_refusal(Some(false), None).unwrap().contains("in doubt"));
        assert_eq!(uninstall_refusal(Some(false), Some(false)), None);
    }

    #[test]
    fn release_runs_unless_known_to_be_free() {
        assert!(anything_to_release(Some(true), Some(true)));
        assert!(anything_to_release(Some(false), Some(true)));
        assert!(anything_to_release(None, Some(false)));
        assert!(anything_to_release(Some(false), None));
        assert!(!anything_to_release(Some(false), Some(false)));
    }

    #[test]
    fn every_known_error_has_words() {
        for e in [
            OwnerError::Accounts,
            OwnerError::SeveralUsers,
            OwnerError::OwnerAlreadySet,
            OwnerError::ProfileOwnerSet,
            OwnerError::AlreadySetUp,
            OwnerError::UnknownAdmin,
            OwnerError::UserNotRunning,
            OwnerError::NotSystemUser,
            OwnerError::Paired,
            OwnerError::ShellNotAllowed,
        ] {
            assert!(explain_owner_error(e).len() > 20);
        }
        assert!(explain_install_failure("INSTALL_FAILED_TEST_ONLY").contains("-t"));
        assert!(explain_install_failure("INSTALL_FAILED_UPDATE_INCOMPATIBLE").contains("different key"));
        assert!(explain_install_failure("INSTALL_PARSE_FAILED_NO_CERTIFICATES").contains("not signed"));
        assert!(explain_install_failure("INSTALL_FAILED_SOMETHING").contains("INSTALL_FAILED_SOMETHING"));
    }

    #[test]
    fn owner_suspensions_are_the_ones_by_android_or_unknown() {
        let list = vec![
            Suspended { package: "com.sec.android.app.popupcalculator".into(), by: vec!["android".into()] },
            Suspended { package: "com.example.unknown".into(), by: vec![] },
            Suspended { package: "com.example.wellbeing".into(), by: vec!["com.google.android.apps.wellbeing".into()] },
            Suspended { package: "com.example.both".into(), by: vec!["com.android.shell".into(), "android".into()] },
        ];
        assert_eq!(
            owner_suspensions(&list),
            vec![
                "com.sec.android.app.popupcalculator".to_string(),
                "com.example.unknown".to_string(),
                "com.example.both".to_string()
            ]
        );
        assert!(owner_suspensions(&[]).is_empty());
    }

    #[test]
    fn spike_restrictions_found() {
        let eff = vec!["no_add_managed_profile".to_string(), "no_safe_boot".to_string(), "no_factory_reset".to_string()];
        assert_eq!(spike_restrictions_in_force(&eff), vec!["no_safe_boot".to_string(), "no_factory_reset".to_string()]);
        assert!(spike_restrictions_in_force(&[]).is_empty());
    }
}
