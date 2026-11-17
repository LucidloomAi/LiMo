//! Reading adb's output. Pure, so every parser is unit tested with canned
//! output. The output formats are Android 13's as best known; every parser
//! says "could not read" (None) rather than guess when the text does not
//! look as expected.

use std::collections::BTreeMap;

// adb devices

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceLine {
    pub serial: String,
    pub state: String,
}

/// `adb devices`: one entry per device line, whatever its state.
pub fn devices(text: &str) -> Vec<DeviceLine> {
    let mut out = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty()
            || t.starts_with('*')
            || t.starts_with("List of devices")
            || t.starts_with("adb server")
            || t.starts_with("adb: ")
        {
            continue;
        }
        let mut parts = t.split_whitespace();
        let (Some(serial), Some(state)) = (parts.next(), parts.next()) else {
            continue;
        };
        let state = if state == "no" {
            "no permissions".to_string()
        } else {
            state.to_string()
        };
        out.push(DeviceLine {
            serial: serial.to_string(),
            state,
        });
    }
    out
}

/// `adb version`: the first line's version, for example "1.0.41".
pub fn adb_version(text: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.trim().strip_prefix("Android Debug Bridge version "))
        .map(|v| v.trim().to_string())
}

// getprop

/// `adb shell getprop`: every "[key]: [value]" line.
pub fn getprop(text: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix('[') {
            if let Some((k, v)) = rest.split_once("]: [") {
                let v = v.strip_suffix(']').unwrap_or(v);
                map.insert(k.to_string(), v.to_string());
            }
        }
    }
    map
}

// users

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub id: u32,
    pub name: String,
}

/// `pm list users`: "UserInfo{0:Owner:c13} running" lines.
pub fn users(text: &str) -> Option<Vec<User>> {
    let mut header = false;
    let mut list = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("Users:") {
            header = true;
            continue;
        }
        let Some(start) = t.find("UserInfo{") else {
            continue;
        };
        let inner = &t[start + "UserInfo{".len()..];
        let Some(end) = inner.find('}') else {
            continue;
        };
        let inner = &inner[..end];
        let Some((id, rest)) = inner.split_once(':') else {
            continue;
        };
        let Ok(id) = id.trim().parse::<u32>() else {
            continue;
        };
        let name = match rest.rsplit_once(':') {
            Some((n, _flags)) => n,
            None => rest,
        };
        list.push(User {
            id,
            name: name.to_string(),
        });
    }
    if !header && list.is_empty() {
        None
    } else {
        Some(list)
    }
}

// accounts

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accounts {
    pub count: u32,
    /// Account types, each once, in the order seen. Never the account names.
    pub types: Vec<String>,
}

/// `dumpsys account`: the "Accounts: N" line of each user and the
/// "Account {name=..., type=...}" lines. Only the types are kept: account
/// names are people's email addresses and are never printed.
pub fn accounts(text: &str) -> Option<Accounts> {
    let mut summary: Option<u32> = None;
    let mut listed = 0u32;
    let mut types: Vec<String> = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("Accounts:") {
            if let Ok(n) = rest.trim().parse::<u32>() {
                summary = Some(summary.unwrap_or(0) + n);
            }
        } else if t.starts_with("Account {") {
            listed += 1;
            if let Some(ty) = field(t, "type=") {
                if !types.contains(&ty) {
                    types.push(ty);
                }
            }
        }
    }
    match summary {
        None if listed == 0 => None,
        s => Some(Accounts {
            count: s.unwrap_or(0).max(listed),
            types,
        }),
    }
}

/// The value after `key` up to the next comma, closing brace or space.
fn field(line: &str, key: &str) -> Option<String> {
    let i = line.find(key)?;
    let rest = &line[i + key.len()..];
    let end = rest.find([',', '}', ' ']).unwrap_or(rest.len());
    let v = rest[..end].trim();
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

// owners and admins

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Owner {
    pub user: u32,
    pub component: String,
    pub device_owner: bool,
    pub profile_owner: bool,
}

/// `dpm list-owners`: "no owners", or "1 owner:" followed by lines like
/// "User  0: admin=ai.lucidloom.limo.spike05/.AdminReceiver,DeviceOwner".
pub fn owners(text: &str) -> Option<Vec<Owner>> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    let lower = t.to_ascii_lowercase();
    if lower.contains("unknown command") || lower.contains("exception") || lower.starts_with("error") {
        return None;
    }
    let mut header = false;
    let mut list = Vec::new();
    for line in t.lines() {
        let l = line.trim();
        if l == "no owners" || l.ends_with(" owner:") || l.ends_with(" owners:") {
            header = true;
            continue;
        }
        let Some(rest) = l.strip_prefix("User") else {
            continue;
        };
        let Some((uid, after)) = rest.split_once(": admin=") else {
            continue;
        };
        let Ok(user) = uid.trim().parse::<u32>() else {
            continue;
        };
        let mut parts = after.split(',');
        let component = parts.next().unwrap_or("").trim().to_string();
        let flags: Vec<&str> = parts.map(str::trim).collect();
        list.push(Owner {
            user,
            component,
            device_owner: flags.contains(&"DeviceOwner"),
            profile_owner: flags
                .iter()
                .any(|f| *f == "ProfileOwner" || f.starts_with("ManagedProfileOwner")),
        });
    }
    if !header && list.is_empty() {
        None
    } else {
        Some(list)
    }
}

/// `dumpsys device_policy`: whether `package` has a line in an
/// "Enabled Device Admins" section. None if the text does not look like
/// that dump at all.
pub fn admin_listed(text: &str, package: &str) -> Option<bool> {
    if !text.contains("Device Policy Manager") && !text.contains("Enabled Device Admins") {
        return None;
    }
    let prefix = format!("{package}/");
    let mut in_section = false;
    let mut section_indent = 0usize;
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if t.starts_with("Enabled Device Admins") {
            in_section = true;
            section_indent = indent;
            continue;
        }
        if in_section {
            if indent <= section_indent {
                in_section = false;
                continue;
            }
            if t.starts_with(&prefix) {
                return Some(true);
            }
        }
    }
    Some(false)
}

// packages

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageDump {
    /// From the flags line; None if no flags line was found.
    pub test_only: Option<bool>,
    pub version_name: Option<String>,
}

/// `pm list packages NAME`: whether exactly this package is listed.
pub fn package_listed(text: &str, package: &str) -> bool {
    let want = format!("package:{package}");
    text.lines().any(|l| l.trim() == want)
}

/// `dumpsys package NAME`: the block for that package. None when there is no
/// such block (not installed).
pub fn package_dump(text: &str, package: &str) -> Option<PackageDump> {
    let marker = format!("Package [{package}]");
    let start = text.find(&marker)?;
    let block = &text[start..];
    let end = block[marker.len()..]
        .find("Package [")
        .map(|i| i + marker.len())
        .unwrap_or(block.len());
    let block = &block[..end];
    let mut flags_seen = false;
    let mut test_only = false;
    let mut version_name = None;
    for line in block.lines() {
        let t = line.trim();
        if t.starts_with("flags=[") || t.starts_with("pkgFlags=[") {
            flags_seen = true;
            if t.split_whitespace().any(|w| w == "TEST_ONLY") {
                test_only = true;
            }
        }
        if let Some(v) = t.strip_prefix("versionName=") {
            version_name = Some(v.trim().to_string());
        }
    }
    Some(PackageDump {
        test_only: if flags_seen { Some(test_only) } else { None },
        version_name,
    })
}

/// "true" or "false" right after `key`.
fn token_bool(line: &str, key: &str) -> Option<bool> {
    let i = line.find(key)?;
    let rest = &line[i + key.len()..];
    if rest.starts_with("true") {
        Some(true)
    } else if rest.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suspended {
    pub package: String,
    /// Who suspended it ("android" is a Device Owner or the system), if shown.
    pub by: Vec<String>,
}

/// `dumpsys package packages`: every package whose user 0 line says
/// suspended=true. None if no user line with "suspended=" was seen at all.
pub fn suspended_packages(text: &str) -> Option<Vec<Suspended>> {
    let mut seen_token = false;
    let mut out: Vec<Suspended> = Vec::new();
    let mut current: Option<String> = None;
    let mut current_suspended = false;
    let mut by: Vec<String> = Vec::new();

    fn flush(out: &mut Vec<Suspended>, current: &Option<String>, suspended: bool, by: &[String]) {
        let Some(p) = current else {
            return;
        };
        if !suspended {
            return;
        }
        if let Some(e) = out.iter_mut().find(|s| &s.package == p) {
            for b in by {
                if !e.by.contains(b) {
                    e.by.push(b.clone());
                }
            }
        } else {
            out.push(Suspended {
                package: p.clone(),
                by: by.to_vec(),
            });
        }
    }

    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("Package [") {
            flush(&mut out, &current, current_suspended, &by);
            current = rest.split(']').next().map(str::to_string);
            current_suspended = false;
            by.clear();
            continue;
        }
        if t.starts_with("User 0:") {
            if let Some(b) = token_bool(t, "suspended=") {
                seen_token = true;
                if b {
                    current_suspended = true;
                }
            }
        }
        if let Some(who) = field(t, "suspendingPackage=") {
            if !by.contains(&who) {
                by.push(who);
            }
        }
    }
    flush(&mut out, &current, current_suspended, &by);
    if seen_token {
        Some(out)
    } else {
        None
    }
}

// restrictions

/// `dumpsys user`: the "Effective restrictions:" of user 0, by name. None if
/// that section was not found.
pub fn effective_restrictions(text: &str) -> Option<Vec<String>> {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.iter().position(|l| l.contains("UserInfo{0:"))?;
    let mut i = start + 1;
    while i < lines.len() {
        let l = lines[i];
        if l.contains("UserInfo{") {
            break;
        }
        let t = l.trim();
        if let Some(inline) = t.strip_prefix("Effective restrictions:") {
            let mut items = Vec::new();
            let inline = inline.trim();
            if !inline.is_empty() {
                items.extend(restriction_tokens(inline));
                return Some(items);
            }
            let header_indent = indent_of(l);
            for lj in &lines[i + 1..] {
                if lj.trim().is_empty() || indent_of(lj) <= header_indent {
                    break;
                }
                items.extend(restriction_tokens(lj.trim()));
            }
            return Some(items);
        }
        i += 1;
    }
    None
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Restriction names in a line: "no_add_user", or "no_add_user=true" inside
/// a Bundle. "none", "null" and "=false" entries are left out.
fn restriction_tokens(s: &str) -> Vec<String> {
    s.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '='))
        .filter_map(|tok| {
            let (name, val) = match tok.split_once('=') {
                Some((n, v)) => (n, Some(v)),
                None => (tok, None),
            };
            if name.is_empty() || name == "none" || name == "null" || val == Some("false") {
                return None;
            }
            if !name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            {
                return None;
            }
            Some(name.to_string())
        })
        .collect()
}

// the app's log

/// `adb logcat -d -s TAG`: the message part of every line with that tag.
pub fn log_messages(text: &str, tag: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let idx = line.find(tag)?;
            let after = &line[idx + tag.len()..];
            let colon = after.find(':')?;
            // Only spaces may come between the tag and its colon.
            if !after[..colon].trim().is_empty() {
                return None;
            }
            Some(after[colon + 1..].trim().to_string())
        })
        .collect()
}

/// The newest message that starts with `prefix`.
pub fn last_starting<'a>(messages: &'a [String], prefix: &str) -> Option<&'a str> {
    messages
        .iter()
        .rev()
        .find(|m| m.starts_with(prefix))
        .map(String::as_str)
}

// results of the commands that change something

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallOutcome {
    Success,
    Failure { code: String },
}

/// `adb install`: "Success", or a "Failure [INSTALL_FAILED_...]" line.
pub fn install(text: &str) -> InstallOutcome {
    if text.lines().any(|l| l.trim() == "Success") {
        return InstallOutcome::Success;
    }
    InstallOutcome::Failure {
        code: upper_code(text, "INSTALL_").unwrap_or_else(|| "unknown".to_string()),
    }
}

/// The first "PREFIX_WORDS" code in the text.
fn upper_code(text: &str, prefix: &str) -> Option<String> {
    let i = text.find(prefix)?;
    let code: String = text[i..]
        .chars()
        .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '_')
        .collect();
    if code.len() > prefix.len() {
        Some(code)
    } else {
        None
    }
}

/// The known reasons set-device-owner gives for refusing. The texts are the
/// ones in Android 13's DevicePolicyManagerService (computeProvisioningErrorString),
/// plus the wording older and Samsung builds are reported to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerError {
    Accounts,
    SeveralUsers,
    OwnerAlreadySet,
    ProfileOwnerSet,
    AlreadySetUp,
    UnknownAdmin,
    UserNotRunning,
    NotSystemUser,
    Paired,
    ShellNotAllowed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetOwnerOutcome {
    Success,
    Failed {
        known: Vec<OwnerError>,
        /// The first line that names the error, as Android wrote it.
        first_error: String,
    },
}

/// `dpm set-device-owner`.
pub fn set_owner(text: &str) -> SetOwnerOutcome {
    if text
        .lines()
        .any(|l| l.trim_start().starts_with("Success: Device owner set"))
    {
        return SetOwnerOutcome::Success;
    }
    SetOwnerOutcome::Failed {
        known: owner_errors(text),
        first_error: first_error_line(text),
    }
}

/// Every known reason found in the text, in a fixed order.
pub fn owner_errors(text: &str) -> Vec<OwnerError> {
    let l = text.to_ascii_lowercase();
    let mut v = Vec::new();
    let mut add = |hit: bool, e: OwnerError| {
        if hit && !v.contains(&e) {
            v.push(e);
        }
    };
    add(
        l.contains("already some accounts") || l.contains("accounts on the device"),
        OwnerError::Accounts,
    );
    add(
        l.contains("several users") || l.contains("multiple users") || l.contains("more than one user"),
        OwnerError::SeveralUsers,
    );
    add(
        l.contains("device owner is already set")
            || l.contains("already has a device owner")
            || l.contains("device owner already set"),
        OwnerError::OwnerAlreadySet,
    );
    add(l.contains("already has a profile owner"), OwnerError::ProfileOwnerSet);
    add(
        l.contains("already provisioned")
            || l.contains("already been provisioned")
            || l.contains("already set-up")
            || l.contains("already set up")
            || l.contains("setup is already complete"),
        OwnerError::AlreadySetUp,
    );
    add(l.contains("unknown admin"), OwnerError::UnknownAdmin);
    add(l.contains("not running"), OwnerError::UserNotRunning);
    add(l.contains("is not system user"), OwnerError::NotSystemUser);
    add(l.contains("already paired"), OwnerError::Paired);
    add(
        l.contains("neither user 2000") || l.contains("does not have android.permission"),
        OwnerError::ShellNotAllowed,
    );
    v
}

/// The first line naming an exception or error, else the first line.
fn first_error_line(text: &str) -> String {
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    lines
        .iter()
        .find(|l| {
            (l.contains("Exception:") || l.starts_with("Error")) && !l.starts_with("Exception occurred")
        })
        .or_else(|| lines.first())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "(no output)".to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoveOutcome {
    Success,
    /// Android refused: not a test only admin, or not an admin at all.
    NotTestOnly,
    UnknownAdmin,
    Failed(String),
}

/// `dpm remove-active-admin`.
pub fn remove_admin(text: &str) -> RemoveOutcome {
    let l = text.to_ascii_lowercase();
    if text.lines().any(|x| x.trim_start().starts_with("Success")) {
        RemoveOutcome::Success
    } else if l.contains("non-test admin") {
        RemoveOutcome::NotTestOnly
    } else if l.contains("unknown admin") {
        RemoveOutcome::UnknownAdmin
    } else {
        RemoveOutcome::Failed(first_error_line(text))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UninstallOutcome {
    Success,
    StillAdmin,
    Blocked,
    /// No reason given. Android answers this for a package that is not
    /// installed, and on the test phone (Samsung, Android 13) also for an
    /// uninstall blocked by the owner. Only a fresh look tells which.
    InternalError,
    NotInstalled,
    Failed(String),
}

/// `adb uninstall`.
pub fn uninstall(text: &str) -> UninstallOutcome {
    if text.lines().any(|l| l.trim() == "Success") {
        UninstallOutcome::Success
    } else if text.contains("DELETE_FAILED_DEVICE_POLICY_MANAGER") {
        UninstallOutcome::StillAdmin
    } else if text.contains("DELETE_FAILED_OWNER_BLOCKED") {
        UninstallOutcome::Blocked
    } else if text.contains("DELETE_FAILED_INTERNAL_ERROR") {
        UninstallOutcome::InternalError
    } else if text.contains("Unknown package") {
        UninstallOutcome::NotInstalled
    } else {
        UninstallOutcome::Failed(first_error_line(text))
    }
}

/// `am start`: Ok if the intent reached the activity.
pub fn am_start(text: &str) -> Result<(), String> {
    let errors: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("Error") || l.contains("Exception"))
        .collect();
    if !errors.is_empty() {
        return Err(errors.join(" "));
    }
    if text.contains("Starting: Intent") || text.contains("intent has been delivered") {
        Ok(())
    } else {
        Err(first_error_line(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PKG: &str = "ai.lucidloom.limo.spike05";

    #[test]
    fn devices_lines() {
        let text = "* daemon not running; starting now at tcp:5037\n\
                    * daemon started successfully\n\
                    List of devices attached\n\
                    R5CN70JNGPE\tdevice\n\
                    emulator-5554\toffline\n\
                    ABC\tunauthorized\n\
                    XYZ\tno permissions (user in plugdev group); see [http://developer.android.com/tools/device.html]\n\n";
        let d = devices(text);
        assert_eq!(d.len(), 4);
        assert_eq!(d[0], DeviceLine { serial: "R5CN70JNGPE".into(), state: "device".into() });
        assert_eq!(d[1].state, "offline");
        assert_eq!(d[2].state, "unauthorized");
        assert_eq!(d[3].state, "no permissions");
        assert!(devices("List of devices attached\n\n").is_empty());
    }

    #[test]
    fn adb_version_line() {
        let text = "Android Debug Bridge version 1.0.41\nVersion 36.0.0-13206524\nInstalled as C:\\adb.exe\n";
        assert_eq!(adb_version(text), Some("1.0.41".into()));
        assert_eq!(adb_version("nonsense"), None);
    }

    #[test]
    fn getprop_lines() {
        let text = "[ro.build.version.release]: [13]\n[ro.build.version.sdk]: [33]\n\
                    [ro.product.manufacturer]: [samsung]\n[ro.product.model]: [SM-G988B]\n[empty]: []\nnoise\n";
        let p = getprop(text);
        assert_eq!(p.get("ro.build.version.release").map(String::as_str), Some("13"));
        assert_eq!(p.get("ro.product.model").map(String::as_str), Some("SM-G988B"));
        assert_eq!(p.get("empty").map(String::as_str), Some(""));
        assert_eq!(p.len(), 5);
    }

    #[test]
    fn users_one_and_samsung_extras() {
        let one = "Users:\n\tUserInfo{0:Owner:c13} running\n";
        assert_eq!(users(one), Some(vec![User { id: 0, name: "Owner".into() }]));
        let three = "Users:\n\tUserInfo{0:Owner:c13} running\n\tUserInfo{95:DUAL_APP:20001010}\n\tUserInfo{150:Secure Folder:10001030} running\n";
        let u = users(three).unwrap();
        assert_eq!(u.len(), 3);
        assert_eq!(u[1], User { id: 95, name: "DUAL_APP".into() });
        assert_eq!(u[2], User { id: 150, name: "Secure Folder".into() });
        assert_eq!(users("Error: something"), None);
        assert_eq!(users("Users:\n"), Some(vec![]));
    }

    #[test]
    fn accounts_none_and_some_without_names() {
        let none = "User UserInfo{0:Owner:c13}:\n  Accounts: 0\n\n  Active Sessions: 0\n";
        assert_eq!(accounts(none), Some(Accounts { count: 0, types: vec![] }));
        let some = "User UserInfo{0:Owner:c13}:\n  Accounts: 3\n    Account {name=someone@gmail.com, type=com.google}\n\
                    \x20   Account {name=other@gmail.com, type=com.google}\n    Account {name=x@y.z, type=com.osp.app.signin}\n";
        let a = accounts(some).unwrap();
        assert_eq!(a.count, 3);
        assert_eq!(a.types, vec!["com.google".to_string(), "com.osp.app.signin".to_string()]);
        // The names never come out.
        assert!(!format!("{a:?}").contains('@'));
        assert_eq!(accounts(""), None);
        assert_eq!(accounts("Can't find service: account"), None);
    }

    #[test]
    fn owners_none_ours_and_others() {
        assert_eq!(owners("no owners\n"), Some(vec![]));
        let ours = "1 owner:\nUser  0: admin=ai.lucidloom.limo.spike05/.AdminReceiver,DeviceOwner\n";
        assert_eq!(
            owners(ours),
            Some(vec![Owner {
                user: 0,
                component: "ai.lucidloom.limo.spike05/.AdminReceiver".into(),
                device_owner: true,
                profile_owner: false
            }])
        );
        let two = "2 owners:\nUser  0: admin=com.other.mdm/.Admin,DeviceOwner,Affiliated\n\
                   User 10: admin=com.work/.Po,ManagedProfileOwner(parentUserId=0)\n";
        let o = owners(two).unwrap();
        assert_eq!(o.len(), 2);
        assert!(o[0].device_owner);
        assert!(o[1].profile_owner && !o[1].device_owner);
        assert_eq!(o[1].user, 10);
        assert_eq!(owners(""), None);
        assert_eq!(owners("Unknown command: list-owners"), None);
        assert_eq!(owners("Exception occurred while executing 'list-owners':"), None);
    }

    #[test]
    fn admin_section() {
        let with = "Current Device Policy Manager state:\n  Device Owner: \n    admin=ComponentInfo{ai.lucidloom.limo.spike05/ai.lucidloom.limo.spike05.AdminReceiver}\n\n\
                    \x20 Enabled Device Admins (User 0, provisioningState: 3):\n    ai.lucidloom.limo.spike05/.AdminReceiver:\n      uid=10345\n      testOnlyAdmin=true\n\n\
                    \x20 Profile Owner (User 0): none\n";
        assert_eq!(admin_listed(with, PKG), Some(true));
        let other = "Current Device Policy Manager state:\n  Enabled Device Admins (User 0, provisioningState: 0):\n    com.samsung.android.fmm/.SemFmmDeviceAdminReceiver:\n      uid=1000\n\n  Stats:\n    ai.lucidloom.limo.spike05/.AdminReceiver: not an admin line\n";
        assert_eq!(admin_listed(other, PKG), Some(false));
        let empty = "Current Device Policy Manager state:\n  Immutable state:\n    mHasFeature=true\n";
        assert_eq!(admin_listed(empty, PKG), Some(false));
        // Android 13 also prints the admins being removed, inside the section.
        let removing = "Current Device Policy Manager state:\n\n  Enabled Device Admins (User 0, provisioningState: 0):\n\
                        \x20   Removing Device Admins (User 0): [ComponentInfo{ai.lucidloom.limo.spike05/ai.lucidloom.limo.spike05.AdminReceiver}]\n";
        assert_eq!(admin_listed(removing, PKG), Some(false));
        assert_eq!(admin_listed("Can't find service: device_policy", PKG), None);
    }

    #[test]
    fn package_listing_and_dump() {
        assert!(package_listed("package:ai.lucidloom.limo.spike05\n", PKG));
        assert!(!package_listed("package:ai.lucidloom.limo.spike05.other\n", PKG));
        assert!(!package_listed("", PKG));

        let dump = "Packages:\n  Package [ai.lucidloom.limo.spike05] (c0ffee):\n    userId=10345\n    versionName=0.1-spike\n\
                    \x20   flags=[ DEBUGGABLE HAS_CODE ALLOW_CLEAR_USER_DATA TEST_ONLY ]\n    privateFlags=[ PRIVATE_FLAG_ACTIVITIES_RESIZE_MODE_RESIZEABLE ]\n\
                    \x20   User 0: ceDataInode=1 installed=true hidden=false suspended=false distractionFlags=0 stopped=false notLaunched=false enabled=0 instant=false virtual=false\n";
        assert_eq!(
            package_dump(dump, PKG),
            Some(PackageDump {
                test_only: Some(true),
                version_name: Some("0.1-spike".into()),
            })
        );
        let not_test = dump.replace(" TEST_ONLY", "");
        assert_eq!(package_dump(&not_test, PKG).unwrap().test_only, Some(false));
        let no_flags = "  Package [ai.lucidloom.limo.spike05] (c0ffee):\n    userId=1\n";
        assert_eq!(package_dump(no_flags, PKG).unwrap().test_only, None);
        assert_eq!(package_dump("Unable to find package: ai.lucidloom.limo.spike05\n", PKG), None);
    }

    #[test]
    fn suspended_scan() {
        let text = "Packages:\n  Package [com.sec.android.app.popupcalculator] (a1):\n    userId=10100\n\
                    \x20   User 0: ceDataInode=1 installed=true hidden=false suspended=true distractionFlags=0 stopped=false\n\
                    \x20     Suspend params:\n        suspendingPackage=android dialogInfo=null\n\
                    \x20 Package [com.android.chrome] (b2):\n    User 0: ceDataInode=2 installed=true hidden=false suspended=false distractionFlags=0\n\
                    \x20 Package [com.google.android.youtube] (c3):\n    User 0: ceDataInode=3 installed=true suspended=true\n\
                    Hidden system packages:\n  Package [com.sec.android.app.popupcalculator] (d4):\n    User 0: ceDataInode=1 installed=true suspended=true\n";
        let s = suspended_packages(text).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s[0], Suspended { package: "com.sec.android.app.popupcalculator".into(), by: vec!["android".into()] });
        assert_eq!(s[1], Suspended { package: "com.google.android.youtube".into(), by: vec![] });
        let none = "  Package [com.android.chrome] (b2):\n    User 0: ceDataInode=2 installed=true suspended=false\n";
        assert_eq!(suspended_packages(none), Some(vec![]));
        assert_eq!(suspended_packages("Packages:\n  Package [x.y] (1):\n    userId=1\n"), None);
    }

    #[test]
    fn restrictions_of_user_zero() {
        let text = "Users:\n  UserInfo{0:Owner:c13} serialNo=0 isPrimary=true\n    State: RUNNING_UNLOCKED\n\
                    \x20   Restrictions:\n      none\n    Device policy global restrictions:\n      no_add_user\n\
                    \x20   Effective restrictions:\n      no_add_managed_profile\n      no_add_user\n      no_config_date_time\n      no_factory_reset\n      no_safe_boot\n\
                    \x20   Has profile owner: false\n  UserInfo{150:Secure Folder:10001030}\n    Effective restrictions:\n      no_sms\n";
        assert_eq!(
            effective_restrictions(text),
            Some(vec![
                "no_add_managed_profile".to_string(),
                "no_add_user".to_string(),
                "no_config_date_time".to_string(),
                "no_factory_reset".to_string(),
                "no_safe_boot".to_string()
            ])
        );
        let none = "Users:\n  UserInfo{0:Owner:c13}\n    Effective restrictions:\n      none\n    Has profile owner: false\n";
        assert_eq!(effective_restrictions(none), Some(vec![]));
        let bundle = "  UserInfo{0:Owner:c13}\n    Effective restrictions: Bundle[{no_add_user=true, no_sms=false}]\n";
        assert_eq!(effective_restrictions(bundle), Some(vec!["no_add_user".to_string()]));
        assert_eq!(effective_restrictions("Users:\n  UserInfo{0:Owner:c13}\n    State: RUNNING\n"), None);
        assert_eq!(effective_restrictions(""), None);
    }

    #[test]
    fn log_lines() {
        let text = "--------- beginning of main\n\
                    11-17 10:23:45.123 12345 12360 I LiMoSpike05: state admin=yes owner=yes deadline_min=28\n\
                    11-17 10:23:46.000 12345 12360 I LiMoSpike05: suspended list=none\n\
                    11-17 10:24:00.000 12345 12360 I LiMoSpike05  : state admin=no owner=no deadline_min=none\n\
                    11-17 10:24:00.000 12345 12360 I OtherTag: state admin=maybe\n";
        let m = log_messages(text, "LiMoSpike05");
        assert_eq!(m.len(), 3);
        assert_eq!(last_starting(&m, "state "), Some("state admin=no owner=no deadline_min=none"));
        assert_eq!(last_starting(&m, "suspended "), Some("suspended list=none"));
        assert_eq!(last_starting(&m, "release done"), None);
    }

    #[test]
    fn install_results() {
        assert_eq!(install("Performing Streamed Install\nSuccess\n"), InstallOutcome::Success);
        assert_eq!(
            install("adb: failed to install C:\\a.apk: Failure [INSTALL_FAILED_TEST_ONLY: installPackageLI]\n"),
            InstallOutcome::Failure { code: "INSTALL_FAILED_TEST_ONLY".into() }
        );
        assert_eq!(
            install("Failure [INSTALL_FAILED_UPDATE_INCOMPATIBLE: Package ai.lucidloom.limo.spike05 signatures do not match]"),
            InstallOutcome::Failure { code: "INSTALL_FAILED_UPDATE_INCOMPATIBLE".into() }
        );
        assert_eq!(install("adb: error: cannot stat 'x.apk'"), InstallOutcome::Failure { code: "unknown".into() });
    }

    #[test]
    fn set_owner_success_and_each_known_error() {
        assert_eq!(
            set_owner("Success: Device owner set to package ai.lucidloom.limo.spike05/.AdminReceiver\nActive admin set to component ai.lucidloom.limo.spike05/.AdminReceiver\n"),
            SetOwnerOutcome::Success
        );
        let cases: Vec<(&str, OwnerError)> = vec![
            ("java.lang.IllegalStateException: Not allowed to set the device owner because there are already some accounts on the device.", OwnerError::Accounts),
            ("java.lang.IllegalStateException: Not allowed to set the device owner because there are already several users on the device.", OwnerError::SeveralUsers),
            ("java.lang.IllegalStateException: Trying to set the device owner, but device owner is already set.", OwnerError::OwnerAlreadySet),
            ("java.lang.IllegalStateException: Trying to set the device owner, but the user already has a profile owner.", OwnerError::ProfileOwnerSet),
            ("java.lang.IllegalStateException: Cannot set the device owner if the device is already set-up.", OwnerError::AlreadySetUp),
            ("java.lang.IllegalStateException: Trying to set the device owner, but device is already provisioned.", OwnerError::AlreadySetUp),
            ("java.lang.IllegalArgumentException: Unknown admin: ComponentInfo{ai.lucidloom.limo.spike05/ai.lucidloom.limo.spike05.AdminReceiver}", OwnerError::UnknownAdmin),
            ("java.lang.IllegalStateException: User 0 not running.", OwnerError::UserNotRunning),
            ("java.lang.IllegalStateException: User 0 is not system user.", OwnerError::NotSystemUser),
            ("java.lang.IllegalStateException: Not allowed to set the device owner because this device has already paired.", OwnerError::Paired),
            ("java.lang.SecurityException: Neither user 2000 nor current process has android.permission.MANAGE_DEVICE_ADMINS.", OwnerError::ShellNotAllowed),
        ];
        for (line, want) in cases {
            let text = format!("Exception occurred while executing 'set-device-owner':\n{line}\n\tat com.android.server.devicepolicy.DevicePolicyManagerService.enforceCanSetDeviceOwnerLocked(DevicePolicyManagerService.java:9054)\n");
            match set_owner(&text) {
                SetOwnerOutcome::Failed { known, first_error } => {
                    assert_eq!(known, vec![want], "for {line}");
                    assert_eq!(first_error, line);
                }
                other => panic!("expected a failure for {line}, got {other:?}"),
            }
        }
        match set_owner("java.lang.IllegalStateException: Unexpected @ProvisioningPreCondition: 42\n") {
            SetOwnerOutcome::Failed { known, .. } => assert!(known.is_empty()),
            other => panic!("{other:?}"),
        }
        match set_owner("Error: something new from Knox\n") {
            SetOwnerOutcome::Failed { known, first_error } => {
                assert!(known.is_empty());
                assert_eq!(first_error, "Error: something new from Knox");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn remove_admin_results() {
        assert_eq!(remove_admin("Success: Admin removed ai.lucidloom.limo.spike05/.AdminReceiver\n"), RemoveOutcome::Success);
        // Android 13 prints the full ComponentName.toString() form.
        assert_eq!(
            remove_admin("Success: Admin removed ComponentInfo{ai.lucidloom.limo.spike05/ai.lucidloom.limo.spike05.AdminReceiver}\n"),
            RemoveOutcome::Success
        );
        assert_eq!(
            remove_admin("Exception occurred while executing 'remove-active-admin':\njava.lang.SecurityException: Attempt to remove non-test admin ComponentInfo{ai.lucidloom.limo.spike05/ai.lucidloom.limo.spike05.AdminReceiver} 0\n"),
            RemoveOutcome::NotTestOnly
        );
        assert_eq!(remove_admin("java.lang.IllegalArgumentException: Unknown admin: x"), RemoveOutcome::UnknownAdmin);
        assert_eq!(remove_admin("something odd"), RemoveOutcome::Failed("something odd".into()));
    }

    #[test]
    fn uninstall_results() {
        assert_eq!(uninstall("Success\n"), UninstallOutcome::Success);
        assert_eq!(uninstall("Failure [DELETE_FAILED_DEVICE_POLICY_MANAGER]\n"), UninstallOutcome::StillAdmin);
        assert_eq!(uninstall("Failure [DELETE_FAILED_OWNER_BLOCKED]\n"), UninstallOutcome::Blocked);
        // Seen on the test phone for an uninstall blocked by the owner.
        assert_eq!(uninstall("Failure [DELETE_FAILED_INTERNAL_ERROR]\n"), UninstallOutcome::InternalError);
        assert_eq!(uninstall("Unknown package: ai.lucidloom.limo.spike05\n"), UninstallOutcome::NotInstalled);
        assert_eq!(uninstall("Failure [WHAT]\n"), UninstallOutcome::Failed("Failure [WHAT]".into()));
    }

    #[test]
    fn am_start_results() {
        assert_eq!(am_start("Starting: Intent { cmp=ai.lucidloom.limo.spike05/.MainActivity (has extras) }\n"), Ok(()));
        assert_eq!(
            am_start("Starting: Intent { cmp=ai.lucidloom.limo.spike05/.MainActivity (has extras) }\nWarning: Activity not started, intent has been delivered to currently running top-most instance.\n"),
            Ok(())
        );
        assert!(am_start("Starting: Intent { cmp=ai.lucidloom.limo.spike05/.MainActivity }\nError type 3\nError: Activity class {ai.lucidloom.limo.spike05/ai.lucidloom.limo.spike05.MainActivity} does not exist.\n").is_err());
        assert!(am_start("").is_err());
    }
}
