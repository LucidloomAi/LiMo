//! The six commands, written against the [Adb] trait so the unit tests can
//! run each one against canned adb output and see exactly what it would run.

use crate::adb::{Adb, Out};
use crate::parse::{self, InstallOutcome, RemoveOutcome, SetOwnerOutcome, UninstallOutcome};
use crate::rules::{self, Facts, ACTIVITY, COMPONENT, LOG_TAG, PACKAGE};
use std::time::Duration;

const T_SHORT: Duration = Duration::from_secs(30);
const T_LONG: Duration = Duration::from_secs(90);
const T_INSTALL: Duration = Duration::from_secs(180);

/// How long `release` waits for the app to let go.
pub const RELEASE_WAIT_S: u64 = 15;

fn say(s: impl AsRef<str>) {
    println!("{}", s.as_ref());
}

fn yn(b: Option<bool>) -> &'static str {
    match b {
        Some(true) => "yes",
        Some(false) => "no",
        None => "could not read",
    }
}

/// Runs `adb -s SERIAL ARGS...`. None if it timed out or could not run.
fn run_on(adb: &mut dyn Adb, serial: &str, args: &[&str], timeout: Duration) -> Option<Out> {
    let mut a: Vec<&str> = vec!["-s", serial];
    a.extend_from_slice(args);
    match adb.run(&a, timeout) {
        Ok(o) if !o.timed_out => Some(o),
        Ok(_) => None,
        Err(e) => {
            say(format!("  {e}"));
            None
        }
    }
}

fn shell(adb: &mut dyn Adb, serial: &str, cmd: &[&str], timeout: Duration) -> Option<Out> {
    let mut a: Vec<&str> = vec!["shell"];
    a.extend_from_slice(cmd);
    run_on(adb, serial, &a, timeout)
}

fn find_device(adb: &mut dyn Adb) -> Result<String, String> {
    let out = adb.run(&["devices"], T_SHORT)?;
    if out.timed_out {
        return Err("adb devices did not answer within 30 seconds. Try adb kill-server, then run this again.".to_string());
    }
    let serial = rules::pick_device(&parse::devices(&out.text()))?;
    say(format!("Phone: {serial}"));
    Ok(serial)
}

fn device_or_say(adb: &mut dyn Adb) -> Option<String> {
    match find_device(adb) {
        Ok(s) => Some(s),
        Err(e) => {
            say(format!("Refused: {e}"));
            say("Nothing was changed on the phone.");
            None
        }
    }
}

// reading (never changes anything)

fn read_owners(adb: &mut dyn Adb, serial: &str) -> Option<Vec<parse::Owner>> {
    shell(adb, serial, &["dpm", "list-owners"], T_SHORT).and_then(|o| parse::owners(&o.text()))
}

fn read_admin(adb: &mut dyn Adb, serial: &str) -> Option<bool> {
    shell(adb, serial, &["dumpsys", "device_policy"], T_LONG).and_then(|o| parse::admin_listed(&o.text(), PACKAGE))
}

fn read_installed(adb: &mut dyn Adb, serial: &str) -> Option<bool> {
    let o = shell(adb, serial, &["pm", "list", "packages", PACKAGE], T_SHORT)?;
    let text = o.text();
    if text.contains("Exception") || text.contains("Error:") {
        return None;
    }
    Some(parse::package_listed(&text, PACKAGE))
}

fn read_package(adb: &mut dyn Adb, serial: &str) -> Option<parse::PackageDump> {
    shell(adb, serial, &["dumpsys", "package", PACKAGE], T_LONG).and_then(|o| parse::package_dump(&o.text(), PACKAGE))
}

fn read_suspended(adb: &mut dyn Adb, serial: &str) -> Option<Vec<parse::Suspended>> {
    shell(adb, serial, &["dumpsys", "package", "packages"], T_LONG).and_then(|o| parse::suspended_packages(&o.text()))
}

fn read_restrictions(adb: &mut dyn Adb, serial: &str) -> Option<Vec<String>> {
    shell(adb, serial, &["dumpsys", "user"], T_SHORT).and_then(|o| parse::effective_restrictions(&o.text()))
}

fn read_log(adb: &mut dyn Adb, serial: &str) -> Option<Vec<String>> {
    run_on(adb, serial, &["logcat", "-d", "-s", LOG_TAG], T_SHORT).map(|o| parse::log_messages(&o.text(), LOG_TAG))
}

fn read_setting(adb: &mut dyn Adb, serial: &str, namespace: &str, key: &str) -> Option<String> {
    shell(adb, serial, &["settings", "get", namespace, key], T_SHORT)
        .filter(Out::ok)
        .map(|o| o.stdout.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn gather(adb: &mut dyn Adb, serial: &str) -> Facts {
    let props = shell(adb, serial, &["getprop"], T_SHORT)
        .map(|o| parse::getprop(&o.stdout))
        .unwrap_or_default();
    let users = shell(adb, serial, &["pm", "list", "users"], T_SHORT).and_then(|o| parse::users(&o.text()));
    let accounts = shell(adb, serial, &["dumpsys", "account"], T_LONG).and_then(|o| parse::accounts(&o.text()));
    let owners = read_owners(adb, serial);
    let installed = read_installed(adb, serial);
    let test_only = if installed == Some(true) {
        read_package(adb, serial).and_then(|p| p.test_only)
    } else {
        None
    };
    let admin = read_admin(adb, serial);
    let device_provisioned = read_setting(adb, serial, "global", "device_provisioned");
    let user_setup_complete = read_setting(adb, serial, "secure", "user_setup_complete");
    Facts {
        android: props.get("ro.build.version.release").cloned(),
        sdk: props.get("ro.build.version.sdk").cloned(),
        manufacturer: props.get("ro.product.manufacturer").cloned(),
        model: props.get("ro.product.model").cloned(),
        users,
        accounts,
        owners,
        installed,
        test_only,
        admin,
        device_provisioned,
        user_setup_complete,
    }
}

fn or_unknown(v: &Option<String>) -> &str {
    v.as_deref().unwrap_or("could not read")
}

fn print_facts(f: &Facts) {
    say("");
    say("What the phone says:");
    say(format!(
        "  Model: {} {}",
        or_unknown(&f.manufacturer),
        or_unknown(&f.model)
    ));
    say(format!("  Android: {} (API {})", or_unknown(&f.android), or_unknown(&f.sdk)));
    match &f.users {
        Some(u) => say(format!(
            "  Users: {} ({})",
            u.len(),
            u.iter().map(|x| format!("{} \"{}\"", x.id, x.name)).collect::<Vec<_>>().join(", ")
        )),
        None => say("  Users: could not read"),
    }
    match &f.accounts {
        Some(a) if a.count == 0 => say("  Accounts: 0"),
        Some(a) => say(format!(
            "  Accounts: {} (types: {}; names are never shown)",
            a.count,
            if a.types.is_empty() { "not shown".to_string() } else { a.types.join(", ") }
        )),
        None => say("  Accounts: could not read"),
    }
    match &f.owners {
        Some(o) if o.is_empty() => say("  Device or profile owners: none"),
        Some(o) => {
            for x in o {
                let kind = match (x.device_owner, x.profile_owner) {
                    (true, _) => "Device Owner",
                    (false, true) => "profile owner",
                    _ => "owner",
                };
                say(format!("  {kind} on user {}: {}", x.user, x.component));
            }
        }
        None => say("  Device or profile owners: could not read"),
    }
    say(format!(
        "  LiMo spike 05: installed {}, test only {}, active admin {}",
        yn(f.installed),
        if f.installed == Some(true) { yn(f.test_only) } else { "n/a" },
        yn(f.admin)
    ));
    say(format!(
        "  Setup finished on the phone: device_provisioned={}, user_setup_complete={}. On Android 13 adb may still set an owner after setup, when there are no accounts and one user.",
        or_unknown(&f.device_provisioned),
        or_unknown(&f.user_setup_complete)
    ));
}

fn print_verdict(b: &[rules::Blocker]) {
    say("");
    if b.is_empty() {
        say("Can set-device-owner work: yes. Nothing is in the way.");
        return;
    }
    say("Can set-device-owner work: no. In the way:");
    for (i, x) in b.iter().enumerate() {
        say(format!("  {}. {}", i + 1, rules::explain_blocker(x)));
    }
}

/// After a release: are the spike's restrictions gone, and is anything still suspended?
fn report_leftovers(adb: &mut dyn Adb, serial: &str) {
    match read_restrictions(adb, serial) {
        Some(eff) => {
            let mine = rules::spike_restrictions_in_force(&eff);
            if mine.is_empty() {
                say("  Restrictions this spike sets, in force now: none.");
            } else {
                say(format!("  Still in force: {}. Run status; if they stay, see README.md.", mine.join(", ")));
            }
        }
        None => say("  Restrictions in force: could not read (dumpsys user)."),
    }
    match read_suspended(adb, serial) {
        Some(list) if list.is_empty() => say("  Suspended packages: none."),
        Some(list) => {
            for s in &list {
                let by = if s.by.is_empty() { "not shown".to_string() } else { s.by.join(", ") };
                say(format!("  Still suspended: {} (suspended by: {by})", s.package));
            }
            if !rules::owner_suspensions(&list).is_empty() {
                say(RECOVERY_TEXT);
            }
        }
        None => say("  Suspended packages: could not read (dumpsys package packages)."),
    }
}

/// What to do about a suspension a Device Owner left behind, in plain words.
const RECOVERY_TEXT: &str = "  A suspension by \"android\" is one a Device Owner made. Android does not lift it when the owner is removed, and with no owner left nothing else can. \
To lift it, before anyone adds an account to the phone (setup needs a phone with none): \
1. setup --apk ...; \
2. adb shell am start -n ai.lucidloom.limo.spike05/.MainActivity --ez suspend_apps false; \
3. release --pin 2468 (release also lifts every suspension it can find).";

/// How many times force-release looks again after asking the app to lift its suspensions.
const LIFT_CHECKS: u32 = 5;
const LIFT_EVERY: Duration = Duration::from_secs(2);

/// Before force-release removes the owner: while the app is still the owner,
/// ask it to lift its suspensions, because Android will not once the owner is
/// gone. Best effort only: the escape hatch must work even if the app is
/// broken, so whatever happens here, force-release goes on.
fn lift_suspensions_first(adb: &mut dyn Adb, serial: &str) {
    say("Step 1 of 2: lift suspended apps while LiMo spike 05 is still the owner.");
    match read_suspended(adb, serial) {
        Some(list) if rules::owner_suspensions(&list).is_empty() => {
            say("  No app is suspended by a Device Owner. Nothing to lift.");
            return;
        }
        Some(list) => say(format!(
            "  Suspended by a Device Owner: {}. Asking the app to lift them.",
            rules::owner_suspensions(&list).join(", ")
        )),
        None => say("  Could not read which apps are suspended. Asking the app to lift its suspensions anyway."),
    }
    let asked = run_on(
        adb,
        serial,
        &["shell", "am", "start", "-n", ACTIVITY, "--ez", "suspend_apps", "false"],
        T_SHORT,
    )
    .map(|o| parse::am_start(&o.text()));
    match asked {
        Some(Ok(())) => {}
        Some(Err(e)) => {
            say(format!("  The app could not be asked ({e}). Going on without it."));
            return;
        }
        None => {
            say("  Asking the app did not answer. Going on without it.");
            return;
        }
    }
    for _ in 0..LIFT_CHECKS {
        adb.sleep(LIFT_EVERY);
        if let Some(list) = read_suspended(adb, serial) {
            if rules::owner_suspensions(&list).is_empty() {
                say("  Lifted: no app is suspended by a Device Owner any more.");
                return;
            }
        }
    }
    say(format!(
        "  Still suspended after {} seconds. Going on; the report below says what is left and how to lift it.",
        LIFT_CHECKS as u64 * LIFT_EVERY.as_secs()
    ));
}

// the commands

/// Read only: can set-device-owner work, and if not, what is in the way.
pub fn check(adb: &mut dyn Adb) -> i32 {
    say("check: read only, changes nothing on the phone.");
    let Some(serial) = device_or_say(adb) else {
        return 1;
    };
    let f = gather(adb, &serial);
    print_facts(&f);
    let b = rules::blockers(&f);
    print_verdict(&b);
    if b.is_empty() {
        0
    } else {
        1
    }
}

/// check, then install, confirm test only, set-device-owner once, confirm,
/// and open the app so its timer starts.
pub fn setup(adb: &mut dyn Adb, apk: &str, deadline_minutes: Option<u32>) -> i32 {
    say("setup: makes LiMo spike 05 the Device Owner of the connected phone. First a read only check.");
    let Some(serial) = device_or_say(adb) else {
        return 1;
    };
    let f = gather(adb, &serial);
    print_facts(&f);
    let b = rules::blockers(&f);
    print_verdict(&b);
    if !b.is_empty() {
        say("Setup refused. Nothing was changed on the phone.");
        return 1;
    }

    say("");
    say("Step 1 of 4: install the app as a test only build. This changes the phone.");
    let Some(out) = run_on(adb, &serial, &["install", "-t", "-r", apk], T_INSTALL) else {
        say("  The install did not finish. Setup stopped; run check to see what is on the phone.");
        return 1;
    };
    match parse::install(&out.text()) {
        InstallOutcome::Success => say("  Installed."),
        InstallOutcome::Failure { code } => {
            say(format!("  Install failed: {code}. {}", rules::explain_install_failure(&code)));
            say("Setup stopped. The phone has no new owner.");
            return 1;
        }
    }

    say("Step 2 of 4: check that the installed copy is test only.");
    let test_only = read_package(adb, &serial).and_then(|p| p.test_only);
    if test_only != Some(true) {
        say(format!(
            "  Test only: {}. Refused: an owner that is not test only cannot be removed with force-release; only its own release or a factory reset would remove it. Nothing was made owner.",
            yn(test_only)
        ));
        return 1;
    }
    say("  Test only: yes, so force-release (adb shell dpm remove-active-admin) will work on it.");

    say("Step 3 of 4: make it the Device Owner. This runs once and is never retried.");
    let Some(out) = shell(adb, &serial, &["dpm", "set-device-owner", COMPONENT], T_SHORT) else {
        say("  set-device-owner did not answer. Not retried. Run status to see where the phone is.");
        return 1;
    };
    match parse::set_owner(&out.text()) {
        SetOwnerOutcome::Success => say("  Android said Success."),
        SetOwnerOutcome::Failed { known, first_error } => {
            say(format!("  Android refused: {first_error}"));
            for e in &known {
                say(format!("  {}", rules::explain_owner_error(*e)));
            }
            if known.is_empty() {
                say("  This program does not know this error. Write it down in the results.");
            }
            let admin = read_admin(adb, &serial);
            say(format!(
                "  When this fails Android removes the admin it had just added. LiMo spike 05 is an active admin now: {}.",
                yn(admin)
            ));
            if admin != Some(false) {
                say("  To remove it: force-release.");
            }
            say("Setup stopped. Not retried.");
            return 1;
        }
    }

    say("Step 4 of 4: confirm, and open the app so its dead man's timer starts.");
    if rules::owner_is_us(&read_owners(adb, &serial)) != Some(true) {
        say("  dpm list-owners does not show LiMo spike 05 as Device Owner, although set-device-owner said Success. Run status.");
        return 1;
    }
    say("  Confirmed: LiMo spike 05 is the Device Owner.");
    let minutes_text = deadline_minutes.map(|d| d.to_string());
    let mut start: Vec<&str> = vec!["shell", "am", "start", "-n", ACTIVITY];
    if let Some(m) = &minutes_text {
        start.extend_from_slice(&["--ei", "deadline_minutes", m.as_str()]);
    }
    let minutes = deadline_minutes.unwrap_or(30);
    match run_on(adb, &serial, &start, T_SHORT).map(|o| parse::am_start(&o.text())) {
        Some(Ok(())) => say("  The app is open on the phone."),
        Some(Err(e)) => say(format!(
            "  The app could not be opened ({e}). Its timer still starts the first time any part of it runs; open it on the phone to be sure."
        )),
        None => say("  Opening the app did not answer. Open it on the phone so its timer starts."),
    }

    say("");
    say("LiMo spike 05 is now the Device Owner. Nothing is restricted yet.");
    say(format!(
        "Its dead man's timer releases the phone by itself within {minutes} minutes."
    ));
    say("Ways out, in order:");
    say("  1. release --pin 2468 (or the app's \"Release this phone\" button)");
    say("  2. wait for the dead man's timer");
    say("  3. force-release");
    0
}

/// Read only: owner, admin, restrictions, suspensions, the app's last log lines.
pub fn status(adb: &mut dyn Adb) -> i32 {
    say("status: read only, changes nothing on the phone.");
    let Some(serial) = device_or_say(adb) else {
        return 1;
    };
    let owners = read_owners(adb, &serial);
    let admin = read_admin(adb, &serial);
    let installed = read_installed(adb, &serial);
    let pkg = if installed == Some(true) {
        read_package(adb, &serial)
    } else {
        None
    };
    say("");
    say(format!("LiMo spike 05 is the Device Owner: {}", yn(rules::owner_is_us(&owners))));
    if let Some(list) = &owners {
        for o in list.iter().filter(|o| !o.component.starts_with(&format!("{PACKAGE}/"))) {
            say(format!("  Another owner on user {}: {}", o.user, o.component));
        }
    }
    say(format!("LiMo spike 05 is an active admin: {}", yn(admin)));
    say(format!(
        "Installed: {}, test only: {}, version: {}",
        yn(installed),
        yn(pkg.as_ref().and_then(|p| p.test_only)),
        pkg.as_ref().and_then(|p| p.version_name.clone()).unwrap_or_else(|| "n/a".to_string())
    ));

    say("");
    match read_restrictions(adb, &serial) {
        Some(eff) => {
            say(format!(
                "Restrictions in force on user 0: {}",
                if eff.is_empty() { "none".to_string() } else { eff.join(", ") }
            ));
            let mine = rules::spike_restrictions_in_force(&eff);
            say(format!(
                "  Of the four this spike sets: {}",
                if mine.is_empty() { "none".to_string() } else { mine.join(", ") }
            ));
        }
        None => say("Restrictions in force: could not read (dumpsys user)."),
    }
    // Android writes this setting when no_safe_boot is set (1) and cleared (0).
    say(format!(
        "safe_boot_disallowed: {}",
        read_setting(adb, &serial, "global", "safe_boot_disallowed").unwrap_or_else(|| "could not read".to_string())
    ));
    match read_suspended(adb, &serial) {
        Some(list) if list.is_empty() => say("Suspended packages: none"),
        Some(list) => {
            for s in &list {
                let by = if s.by.is_empty() { "not shown".to_string() } else { s.by.join(", ") };
                say(format!("Suspended: {} (by: {by})", s.package));
            }
        }
        None => say("Suspended packages: could not read (dumpsys package packages)."),
    }

    say("");
    match read_log(adb, &serial) {
        Some(m) if m.is_empty() => say("The app's log: no lines kept by the phone (a restart clears them)."),
        Some(m) => {
            say("The app's last log lines of each kind:");
            for prefix in ["state ", "suspended ", "timer ", "release done", "boot seen"] {
                if let Some(line) = parse::last_starting(&m, prefix) {
                    say(format!("  {line}"));
                }
            }
        }
        None => say("The app's log: could not read."),
    }
    0
}

/// Asks the app to release (with the PIN), waits up to 15 seconds and checks.
pub fn release(adb: &mut dyn Adb, pin: &str) -> i32 {
    say("release: asks the app to release the phone. The phone should be unlocked with its screen on.");
    let Some(serial) = device_or_say(adb) else {
        return 1;
    };
    let owner = rules::owner_is_us(&read_owners(adb, &serial));
    let admin = read_admin(adb, &serial);
    if !rules::anything_to_release(owner, admin) {
        say("LiMo spike 05 is neither Device Owner nor an admin. Nothing to release.");
        return 0;
    }
    say(format!("Before: Device Owner {}, active admin {}.", yn(owner), yn(admin)));
    let asked = run_on(
        adb,
        &serial,
        &["shell", "am", "start", "-n", ACTIVITY, "--ez", "release", "true", "--es", "pin", pin],
        T_SHORT,
    )
    .map(|o| parse::am_start(&o.text()));
    match asked {
        Some(Ok(())) => {}
        Some(Err(e)) => {
            say(format!("The app could not be asked: {e}. The ways out that remain: the dead man's timer, and force-release."));
            return 1;
        }
        None => {
            say("Asking the app did not answer. The ways out that remain: the dead man's timer, and force-release.");
            return 1;
        }
    }

    say(format!("Waiting up to {RELEASE_WAIT_S} seconds for the owner to go."));
    let mut owner_gone = false;
    let mut admin_gone = false;
    for _ in 0..RELEASE_WAIT_S {
        adb.sleep(Duration::from_secs(1));
        if !owner_gone {
            owner_gone = rules::owner_is_us(&read_owners(adb, &serial)) == Some(false);
        }
        if owner_gone {
            admin_gone = read_admin(adb, &serial) == Some(false);
            if admin_gone {
                break;
            }
        }
    }

    if owner_gone && admin_gone {
        say("Released: no Device Owner is left, and LiMo spike 05 is no longer an admin.");
        report_leftovers(adb, &serial);
        say("adb uninstall works now: limo-strict-spike uninstall.");
        return 0;
    }
    if owner_gone {
        say(format!(
            "The Device Owner is gone, but LiMo spike 05 is still listed as an admin after {RELEASE_WAIT_S} seconds. Run status in a moment; if it stays, force-release."
        ));
    } else {
        say(format!("LiMo spike 05 is still the Device Owner after {RELEASE_WAIT_S} seconds."));
    }
    if let Some(m) = read_log(adb, &serial) {
        let lines: Vec<&String> = m.iter().filter(|l| l.starts_with("release")).collect();
        if !lines.is_empty() {
            say("The app's release lines:");
            for l in lines.iter().rev().take(12).rev() {
                say(format!("  {l}"));
            }
        }
    }
    say("A wrong PIN shows as \"release refused reason=wrong_pin\". The ways out that remain: the dead man's timer, and force-release.");
    1
}

/// The escape hatch: lift suspensions through the app if it still can, then
/// dpm remove-active-admin, then check.
pub fn force_release(adb: &mut dyn Adb) -> i32 {
    say("force-release: the escape hatch. Removes LiMo spike 05 as admin and Device Owner with adb, without needing the app.");
    say("In plain words: Android lifts the restrictions when the owner is removed this way, but NOT suspended apps, and once the owner is gone nothing can lift them. So while LiMo spike 05 is still the owner, this first asks it to lift them.");
    let Some(serial) = device_or_say(adb) else {
        return 1;
    };
    let owner = rules::owner_is_us(&read_owners(adb, &serial));
    let admin = read_admin(adb, &serial);
    if !rules::anything_to_release(owner, admin) {
        say("LiMo spike 05 is neither Device Owner nor an admin. Nothing to remove.");
        report_leftovers(adb, &serial);
        return 0;
    }
    say(format!("Before: Device Owner {}, active admin {}.", yn(owner), yn(admin)));
    if owner != Some(false) {
        lift_suspensions_first(adb, &serial);
    }
    say("Step 2 of 2: remove the owner and admin.");
    match shell(adb, &serial, &["dpm", "remove-active-admin", COMPONENT], T_SHORT).map(|o| parse::remove_admin(&o.text())) {
        Some(RemoveOutcome::Success) => say("  Android said Success."),
        Some(RemoveOutcome::NotTestOnly) => say(
            "  Android refused. It removes an admin this way only if it is a test only build, and only while it is an admin. \
             If LiMo spike 05 is still the owner, the ways out that remain are the app's own release (release --pin 2468) and its dead man's timer; \
             the last resort is a factory reset from the recovery menu, which erases the phone (see README.md).",
        ),
        Some(RemoveOutcome::UnknownAdmin) => say("  Android does not know this admin."),
        Some(RemoveOutcome::Failed(e)) => say(format!("  Android said: {e}")),
        None => say("  remove-active-admin did not answer."),
    }
    let owner_after = rules::owner_is_us(&read_owners(adb, &serial));
    let admin_after = read_admin(adb, &serial);
    say(format!(
        "After: Device Owner {}, active admin {}.",
        yn(owner_after),
        yn(admin_after)
    ));
    report_leftovers(adb, &serial);
    if owner_after == Some(false) && admin_after == Some(false) {
        say("Released from the PC.");
        0
    } else {
        1
    }
}

/// Refuses while owner or admin (or unknown), then adb uninstall.
pub fn uninstall(adb: &mut dyn Adb) -> i32 {
    say("uninstall: removes LiMo spike 05, but only once it is neither Device Owner nor an admin.");
    let Some(serial) = device_or_say(adb) else {
        return 1;
    };
    if read_installed(adb, &serial) == Some(false) {
        say("LiMo spike 05 is not installed. Nothing to do.");
        return 0;
    }
    let owner = rules::owner_is_us(&read_owners(adb, &serial));
    let admin = read_admin(adb, &serial);
    if let Some(why) = rules::uninstall_refusal(owner, admin) {
        say(format!("Refused: {why}"));
        say("Nothing was changed on the phone.");
        return 1;
    }
    if let Some(list) = read_suspended(adb, &serial) {
        if !list.is_empty() {
            say(format!(
                "Note: still suspended: {}. Android does not lift these by itself. Once the app is gone, setup, then --ez suspend_apps false, then release can still lift them, as long as no account has been added to the phone; it is simpler to do that first.",
                list.iter().map(|s| s.package.as_str()).collect::<Vec<_>>().join(", ")
            ));
        }
    }
    let outcome = run_on(adb, &serial, &["uninstall", PACKAGE], T_LONG).map(|o| parse::uninstall(&o.text()));
    // adb's answer alone is not enough: on the test phone a blocked uninstall
    // answers DELETE_FAILED_INTERNAL_ERROR, which is also the answer for a
    // package that is not there. So look again before saying anything.
    let still_installed = read_installed(adb, &serial);
    if still_installed == Some(false) {
        say("Uninstalled: LiMo spike 05 is no longer listed on the phone.");
        return 0;
    }
    let still = yn(still_installed);
    match outcome {
        Some(UninstallOutcome::Success) => say(format!(
            "adb said Success, but LiMo spike 05 is still listed: {still}. Run status."
        )),
        Some(UninstallOutcome::StillAdmin) => say(
            "Android refused: the app is still an admin or owner (DELETE_FAILED_DEVICE_POLICY_MANAGER). Run status.",
        ),
        Some(UninstallOutcome::Blocked) => say(
            "Android refused: uninstalling is still blocked (DELETE_FAILED_OWNER_BLOCKED). Run status.",
        ),
        Some(UninstallOutcome::InternalError) => say(format!(
            "Android refused without a reason (DELETE_FAILED_INTERNAL_ERROR), and LiMo spike 05 is still listed: {still}. \
             On the test phone that is also what a blocked uninstall answers. Run status; if the app is still owner or admin, release it first."
        )),
        Some(UninstallOutcome::NotInstalled) => say(format!(
            "Android says it is not installed, yet it is still listed: {still}. Run status."
        )),
        Some(UninstallOutcome::Failed(e)) => say(format!("Android refused: {e}")),
        None => say("adb uninstall did not answer."),
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEVICES_ONE: &str = "List of devices attached\nR5CN70\tdevice\n\n";
    const DEVICES_NONE: &str = "List of devices attached\n\n";
    const DEVICES_UNAUTHORIZED: &str = "List of devices attached\nR5CN70\tunauthorized\n\n";
    const GETPROP: &str = "[ro.build.version.release]: [13]\n[ro.build.version.sdk]: [33]\n[ro.product.manufacturer]: [samsung]\n[ro.product.model]: [SM-G988B]\n";
    const USERS_ONE: &str = "Users:\n\tUserInfo{0:Owner:c13} running\n";
    const USERS_TWO: &str = "Users:\n\tUserInfo{0:Owner:c13} running\n\tUserInfo{150:Secure Folder:10001030} running\n";
    const ACCOUNTS_NONE: &str = "User UserInfo{0:Owner:c13}:\n  Accounts: 0\n";
    const ACCOUNTS_ONE: &str = "User UserInfo{0:Owner:c13}:\n  Accounts: 1\n    Account {name=someone@gmail.com, type=com.google}\n";
    const NO_OWNERS: &str = "no owners\n";
    const OWNER_US: &str = "1 owner:\nUser  0: admin=ai.lucidloom.limo.spike05/.AdminReceiver,DeviceOwner\n";
    const OWNER_OTHER: &str = "1 owner:\nUser  0: admin=com.other.mdm/.Admin,DeviceOwner\n";
    const DP_ADMIN: &str = "Current Device Policy Manager state:\n  Enabled Device Admins (User 0, provisioningState: 3):\n    ai.lucidloom.limo.spike05/.AdminReceiver:\n      uid=10345\n";
    const DP_NONE: &str = "Current Device Policy Manager state:\n  Enabled Device Admins (User 0, provisioningState: 0):\n";
    const PKG_LISTED: &str = "package:ai.lucidloom.limo.spike05\n";
    const PKG_TEST_ONLY: &str = "  Package [ai.lucidloom.limo.spike05] (1):\n    versionName=0.1-spike\n    flags=[ DEBUGGABLE HAS_CODE TEST_ONLY ]\n    User 0: ceDataInode=1 installed=true suspended=false\n";
    const PKG_NOT_TEST_ONLY: &str = "  Package [ai.lucidloom.limo.spike05] (1):\n    flags=[ DEBUGGABLE HAS_CODE ]\n";
    const INSTALL_OK: &str = "Performing Streamed Install\nSuccess\n";
    const INSTALL_BAD: &str = "adb: failed to install x.apk: Failure [INSTALL_FAILED_UPDATE_INCOMPATIBLE: signatures do not match]\n";
    const SET_OWNER_OK: &str = "Success: Device owner set to package ai.lucidloom.limo.spike05/.AdminReceiver\nActive admin set to component ai.lucidloom.limo.spike05/.AdminReceiver\n";
    const SET_OWNER_ACCOUNTS: &str = "Exception occurred while executing 'set-device-owner':\njava.lang.IllegalStateException: Not allowed to set the device owner because there are already some accounts on the device.\n";
    const AM_OK: &str = "Starting: Intent { cmp=ai.lucidloom.limo.spike05/.MainActivity (has extras) }\n";
    const AM_MISSING: &str = "Error type 3\nError: Activity class {ai.lucidloom.limo.spike05/ai.lucidloom.limo.spike05.MainActivity} does not exist.\n";
    const REMOVE_OK: &str = "Success: Admin removed ai.lucidloom.limo.spike05/.AdminReceiver\n";
    const USER_DUMP_NONE: &str = "Users:\n  UserInfo{0:Owner:c13}\n    Effective restrictions:\n      none\n    Has profile owner: false\n";
    const PACKAGES_NONE_SUSPENDED: &str = "  Package [com.android.chrome] (b2):\n    User 0: ceDataInode=2 installed=true suspended=false\n";
    const PACKAGES_ONE_SUSPENDED: &str = "  Package [com.sec.android.app.popupcalculator] (a1):\n    User 0: ceDataInode=1 installed=true suspended=true\n      suspendingPackage=android\n";

    fn out(s: &str) -> Out {
        Out {
            code: Some(0),
            stdout: s.to_string(),
            stderr: String::new(),
            timed_out: false,
        }
    }

    /// A fake adb. Each rule answers every call whose line contains its key;
    /// the rule added last wins. A rule with several answers gives them in
    /// turn and then repeats the last. Every call is recorded.
    struct Fake {
        rules: Vec<(String, Vec<Out>)>,
        calls: Vec<String>,
    }

    impl Fake {
        fn new() -> Self {
            Fake {
                rules: Vec::new(),
                calls: Vec::new(),
            }
        }

        fn on(mut self, key: &str, texts: &[&str]) -> Self {
            self.rules.push((key.to_string(), texts.iter().map(|t| out(t)).collect()));
            self
        }

        /// A phone where set-device-owner can work, app not installed.
        fn clean() -> Self {
            Fake::new()
                .on("devices", &[DEVICES_ONE])
                .on("getprop", &[GETPROP])
                .on("pm list users", &[USERS_ONE])
                .on("dumpsys account", &[ACCOUNTS_NONE])
                .on("dpm list-owners", &[NO_OWNERS])
                .on("pm list packages", &[""])
                .on("dumpsys device_policy", &[DP_NONE])
                .on("settings get", &["1\n"])
                .on("dumpsys user", &[USER_DUMP_NONE])
                .on("dumpsys package packages", &[PACKAGES_NONE_SUSPENDED])
                .on("logcat", &[""])
        }

        fn count(&self, key: &str) -> usize {
            self.calls.iter().filter(|c| c.contains(key)).count()
        }

        fn index(&self, key: &str) -> usize {
            self.calls
                .iter()
                .position(|c| c.contains(key))
                .unwrap_or_else(|| panic!("no call with {key} in {:?}", self.calls))
        }

        /// No call that changes anything on the phone.
        fn changed_nothing(&self) -> bool {
            ["install", "set-device-owner", "remove-active-admin", "am start", "uninstall"]
                .iter()
                .all(|k| self.count(k) == 0)
        }
    }

    impl Adb for Fake {
        fn run(&mut self, args: &[&str], _timeout: Duration) -> Result<Out, String> {
            let line = args.join(" ");
            self.calls.push(line.clone());
            for (key, outs) in self.rules.iter_mut().rev() {
                if line.contains(key.as_str()) {
                    return Ok(if outs.len() > 1 { outs.remove(0) } else { outs[0].clone() });
                }
            }
            Ok(out(""))
        }

        fn sleep(&mut self, _d: Duration) {}
    }

    // check

    #[test]
    fn check_passes_on_a_clean_phone_and_changes_nothing() {
        let mut f = Fake::clean();
        assert_eq!(check(&mut f), 0);
        assert!(f.changed_nothing());
    }

    #[test]
    fn check_fails_with_accounts_and_changes_nothing() {
        let mut f = Fake::clean().on("dumpsys account", &[ACCOUNTS_ONE]);
        assert_eq!(check(&mut f), 1);
        assert!(f.changed_nothing());
    }

    #[test]
    fn no_phone_or_unauthorized_refuses_every_command() {
        for devices in [DEVICES_NONE, DEVICES_UNAUTHORIZED] {
            for run in [check, status, force_release, uninstall] {
                let mut f = Fake::clean().on("devices", &[devices]);
                assert_eq!(run(&mut f), 1);
                assert_eq!(f.calls, vec!["devices".to_string()]);
            }
            let mut f = Fake::clean().on("devices", &[devices]);
            assert_eq!(setup(&mut f, "x.apk", None), 1);
            assert_eq!(f.calls, vec!["devices".to_string()]);
            let mut f = Fake::clean().on("devices", &[devices]);
            assert_eq!(release(&mut f, "2468"), 1);
            assert_eq!(f.calls, vec!["devices".to_string()]);
        }
    }

    // setup refusals: nothing is installed or set

    #[test]
    fn setup_refuses_with_accounts() {
        let mut f = Fake::clean().on("dumpsys account", &[ACCOUNTS_ONE]);
        assert_eq!(setup(&mut f, "x.apk", None), 1);
        assert!(f.changed_nothing());
    }

    #[test]
    fn setup_refuses_with_two_users() {
        let mut f = Fake::clean().on("pm list users", &[USERS_TWO]);
        assert_eq!(setup(&mut f, "x.apk", None), 1);
        assert!(f.changed_nothing());
    }

    #[test]
    fn setup_refuses_when_an_owner_is_already_set() {
        let mut f = Fake::clean().on("dpm list-owners", &[OWNER_OTHER]);
        assert_eq!(setup(&mut f, "x.apk", None), 1);
        assert!(f.changed_nothing());
        let mut g = Fake::clean().on("dpm list-owners", &[OWNER_US]);
        assert_eq!(setup(&mut g, "x.apk", None), 1);
        assert!(g.changed_nothing());
    }

    #[test]
    fn setup_refuses_when_anything_is_unreadable() {
        for key in ["dpm list-owners", "dumpsys account", "pm list users"] {
            let mut f = Fake::clean().on(key, &[""]);
            assert_eq!(setup(&mut f, "x.apk", None), 1, "with {key} unreadable");
            assert!(f.changed_nothing());
        }
    }

    #[test]
    fn setup_stops_when_the_install_fails() {
        let mut f = Fake::clean().on("install -t -r", &[INSTALL_BAD]);
        assert_eq!(setup(&mut f, "x.apk", None), 1);
        assert_eq!(f.count("set-device-owner"), 0);
    }

    #[test]
    fn setup_refuses_a_copy_that_is_not_test_only() {
        let mut f = Fake::clean()
            .on("install -t -r", &[INSTALL_OK])
            .on("dumpsys package ai.lucidloom", &[PKG_NOT_TEST_ONLY]);
        assert_eq!(setup(&mut f, "x.apk", None), 1);
        assert_eq!(f.count("set-device-owner"), 0);
        assert_eq!(f.count("am start"), 0);
    }

    #[test]
    fn setup_never_retries_a_refused_owner() {
        let mut f = Fake::clean()
            .on("install -t -r", &[INSTALL_OK])
            .on("dumpsys package ai.lucidloom", &[PKG_TEST_ONLY])
            .on("dpm set-device-owner", &[SET_OWNER_ACCOUNTS]);
        assert_eq!(setup(&mut f, "x.apk", None), 1);
        assert_eq!(f.count("set-device-owner"), 1);
        assert_eq!(f.count("am start"), 0);
        assert_eq!(f.count("remove-active-admin"), 0);
    }

    #[test]
    fn setup_happy_path_in_order() {
        let mut f = Fake::clean()
            .on("install -t -r", &[INSTALL_OK])
            .on("dumpsys package ai.lucidloom", &[PKG_TEST_ONLY])
            .on("dpm set-device-owner", &[SET_OWNER_OK])
            .on("dpm list-owners", &[NO_OWNERS, OWNER_US])
            .on("am start", &[AM_OK]);
        assert_eq!(setup(&mut f, "C:\\spike\\app-debug.apk", Some(2)), 0);
        assert_eq!(f.count("set-device-owner"), 1);
        assert!(f.index("install -t -r C:\\spike\\app-debug.apk") < f.index("dumpsys package ai.lucidloom"));
        assert!(f.index("dumpsys package ai.lucidloom") < f.index("dpm set-device-owner"));
        assert!(f.index("dpm set-device-owner ai.lucidloom.limo.spike05/.AdminReceiver") < f.index("am start"));
        assert!(f.calls.iter().any(|c| c
            == "-s R5CN70 shell am start -n ai.lucidloom.limo.spike05/.MainActivity --ei deadline_minutes 2"));
    }

    #[test]
    fn setup_without_a_deadline_opens_the_app_plainly() {
        let mut f = Fake::clean()
            .on("install -t -r", &[INSTALL_OK])
            .on("dumpsys package ai.lucidloom", &[PKG_TEST_ONLY])
            .on("dpm set-device-owner", &[SET_OWNER_OK])
            .on("dpm list-owners", &[NO_OWNERS, OWNER_US])
            .on("am start", &[AM_OK]);
        assert_eq!(setup(&mut f, "x.apk", None), 0);
        assert!(f.calls.iter().any(|c| c == "-s R5CN70 shell am start -n ai.lucidloom.limo.spike05/.MainActivity"));
    }

    #[test]
    fn setup_reports_when_the_owner_does_not_show() {
        let mut f = Fake::clean()
            .on("install -t -r", &[INSTALL_OK])
            .on("dumpsys package ai.lucidloom", &[PKG_TEST_ONLY])
            .on("dpm set-device-owner", &[SET_OWNER_OK]);
        assert_eq!(setup(&mut f, "x.apk", None), 1);
        assert_eq!(f.count("set-device-owner"), 1);
        assert_eq!(f.count("am start"), 0);
    }

    // release

    #[test]
    fn release_sends_the_pin_and_waits_for_owner_and_admin() {
        let mut f = Fake::clean()
            .on("dpm list-owners", &[OWNER_US, OWNER_US, OWNER_US, NO_OWNERS])
            .on("dumpsys device_policy", &[DP_ADMIN, DP_ADMIN, DP_NONE])
            .on("am start", &[AM_OK]);
        assert_eq!(release(&mut f, "2468"), 0);
        assert!(f.calls.iter().any(|c| c
            == "-s R5CN70 shell am start -n ai.lucidloom.limo.spike05/.MainActivity --ez release true --es pin 2468"));
        assert_eq!(f.count("remove-active-admin"), 0);
        assert_eq!(f.count("dumpsys user"), 1);
    }

    #[test]
    fn release_gives_up_after_fifteen_seconds_and_does_nothing_else() {
        let mut f = Fake::clean()
            .on("dpm list-owners", &[OWNER_US])
            .on("dumpsys device_policy", &[DP_ADMIN])
            .on("am start", &[AM_OK]);
        assert_eq!(release(&mut f, "2468"), 1);
        assert_eq!(f.count("dpm list-owners"), 1 + RELEASE_WAIT_S as usize);
        assert_eq!(f.count("am start"), 1);
        assert_eq!(f.count("remove-active-admin"), 0);
        assert_eq!(f.count("logcat"), 1);
    }

    #[test]
    fn release_with_nothing_to_release_sends_nothing() {
        let mut f = Fake::clean();
        assert_eq!(release(&mut f, "2468"), 0);
        assert!(f.changed_nothing());
    }

    #[test]
    fn release_stops_when_the_app_cannot_be_asked() {
        let mut f = Fake::clean()
            .on("dpm list-owners", &[OWNER_US])
            .on("dumpsys device_policy", &[DP_ADMIN])
            .on("am start", &[AM_MISSING]);
        assert_eq!(release(&mut f, "2468"), 1);
        assert_eq!(f.count("dpm list-owners"), 1);
    }

    // force-release

    #[test]
    fn force_release_removes_and_verifies() {
        let mut f = Fake::clean()
            .on("dpm list-owners", &[OWNER_US, NO_OWNERS])
            .on("dumpsys device_policy", &[DP_ADMIN, DP_NONE])
            .on("remove-active-admin", &[REMOVE_OK]);
        assert_eq!(force_release(&mut f), 0);
        assert_eq!(f.count("dpm remove-active-admin ai.lucidloom.limo.spike05/.AdminReceiver"), 1);
        // Nothing suspended: the app is not asked.
        assert_eq!(f.count("am start"), 0);
        // Once before (step 1) and once in the report.
        assert_eq!(f.count("dumpsys package packages"), 2);
    }

    #[test]
    fn force_release_asks_the_app_to_lift_suspensions_first() {
        let mut f = Fake::clean()
            .on("dpm list-owners", &[OWNER_US, NO_OWNERS])
            .on("dumpsys device_policy", &[DP_ADMIN, DP_NONE])
            .on("dumpsys package packages", &[PACKAGES_ONE_SUSPENDED, PACKAGES_NONE_SUSPENDED])
            .on("am start", &[AM_OK])
            .on("remove-active-admin", &[REMOVE_OK]);
        assert_eq!(force_release(&mut f), 0);
        assert!(f.calls.iter().any(|c| c
            == "-s R5CN70 shell am start -n ai.lucidloom.limo.spike05/.MainActivity --ez suspend_apps false"));
        assert!(f.index("--ez suspend_apps false") < f.index("remove-active-admin"));
        assert_eq!(f.count("remove-active-admin"), 1);
        // Never the app's release, and never a PIN.
        assert_eq!(f.count("--ez release"), 0);
    }

    #[test]
    fn force_release_goes_on_when_the_app_cannot_be_asked() {
        let mut f = Fake::clean()
            .on("dpm list-owners", &[OWNER_US, NO_OWNERS])
            .on("dumpsys device_policy", &[DP_ADMIN, DP_NONE])
            .on("dumpsys package packages", &[PACKAGES_ONE_SUSPENDED])
            .on("am start", &[AM_MISSING])
            .on("remove-active-admin", &[REMOVE_OK]);
        assert_eq!(force_release(&mut f), 0);
        assert_eq!(f.count("am start"), 1);
        assert_eq!(f.count("remove-active-admin"), 1);
        // Read before asking, then in the report: no waiting on a broken app.
        assert_eq!(f.count("dumpsys package packages"), 2);
    }

    #[test]
    fn force_release_goes_on_when_the_suspension_stays() {
        let mut f = Fake::clean()
            .on("dpm list-owners", &[OWNER_US, NO_OWNERS])
            .on("dumpsys device_policy", &[DP_ADMIN, DP_NONE])
            .on("dumpsys package packages", &[PACKAGES_ONE_SUSPENDED])
            .on("am start", &[AM_OK])
            .on("remove-active-admin", &[REMOVE_OK]);
        assert_eq!(force_release(&mut f), 0);
        assert_eq!(f.count("remove-active-admin"), 1);
        // Once before asking, LIFT_CHECKS while waiting, once in the report.
        assert_eq!(f.count("dumpsys package packages"), 1 + LIFT_CHECKS as usize + 1);
    }

    #[test]
    fn force_release_does_not_ask_a_mere_admin() {
        let mut f = Fake::clean()
            .on("dumpsys device_policy", &[DP_ADMIN, DP_NONE])
            .on("dumpsys package packages", &[PACKAGES_ONE_SUSPENDED])
            .on("remove-active-admin", &[REMOVE_OK]);
        assert_eq!(force_release(&mut f), 0);
        assert_eq!(f.count("am start"), 0);
        assert_eq!(f.count("remove-active-admin"), 1);
    }

    #[test]
    fn force_release_with_nothing_to_remove() {
        let mut f = Fake::clean();
        assert_eq!(force_release(&mut f), 0);
        assert!(f.changed_nothing());
    }

    #[test]
    fn force_release_runs_even_when_nothing_can_be_read() {
        let mut f = Fake::clean().on("dpm list-owners", &[""]).on("dumpsys device_policy", &[""]);
        assert_eq!(force_release(&mut f), 1);
        assert_eq!(f.count("remove-active-admin"), 1);
    }

    #[test]
    fn force_release_reports_a_refusal() {
        let mut f = Fake::clean()
            .on("dpm list-owners", &[OWNER_US])
            .on("dumpsys device_policy", &[DP_ADMIN])
            .on(
                "remove-active-admin",
                &["java.lang.SecurityException: Attempt to remove non-test admin ComponentInfo{x/y} 0\n"],
            );
        assert_eq!(force_release(&mut f), 1);
        assert_eq!(f.count("remove-active-admin"), 1);
    }

    // uninstall

    #[test]
    fn uninstall_refused_while_owner_admin_or_unknown() {
        let cases: [(&str, &str); 3] = [(OWNER_US, DP_ADMIN), (NO_OWNERS, DP_ADMIN), ("", DP_NONE)];
        for (owners, dp) in cases {
            let mut f = Fake::clean()
                .on("pm list packages", &[PKG_LISTED])
                .on("dpm list-owners", &[owners])
                .on("dumpsys device_policy", &[dp]);
            assert_eq!(uninstall(&mut f), 1);
            assert_eq!(f.count("uninstall ai.lucidloom"), 0);
        }
    }

    #[test]
    fn uninstall_runs_once_free_and_warns_about_suspensions() {
        let mut f = Fake::clean()
            .on("pm list packages", &[PKG_LISTED, ""])
            .on("dumpsys package packages", &[PACKAGES_ONE_SUSPENDED])
            .on("uninstall ai.lucidloom", &["Success\n"]);
        assert_eq!(uninstall(&mut f), 0);
        assert_eq!(f.count("uninstall ai.lucidloom"), 1);
        // Checked before and after.
        assert_eq!(f.count("pm list packages"), 2);
    }

    #[test]
    fn uninstall_internal_error_is_judged_by_looking_again() {
        // As seen on the test phone for a blocked uninstall: still installed.
        let mut f = Fake::clean()
            .on("pm list packages", &[PKG_LISTED])
            .on("uninstall ai.lucidloom", &["Failure [DELETE_FAILED_INTERNAL_ERROR]\n"]);
        assert_eq!(uninstall(&mut f), 1);
        // The same answer, but the package really is gone.
        let mut g = Fake::clean()
            .on("pm list packages", &[PKG_LISTED, ""])
            .on("uninstall ai.lucidloom", &["Failure [DELETE_FAILED_INTERNAL_ERROR]\n"]);
        assert_eq!(uninstall(&mut g), 0);
        // adb said Success, yet it is still listed.
        let mut h = Fake::clean()
            .on("pm list packages", &[PKG_LISTED])
            .on("uninstall ai.lucidloom", &["Success\n"]);
        assert_eq!(uninstall(&mut h), 1);
    }

    #[test]
    fn uninstall_not_installed_does_nothing() {
        let mut f = Fake::clean();
        assert_eq!(uninstall(&mut f), 0);
        assert!(f.changed_nothing());
    }

    // status

    #[test]
    fn status_only_reads() {
        let mut f = Fake::clean()
            .on("pm list packages", &[PKG_LISTED])
            .on("dumpsys package ai.lucidloom", &[PKG_TEST_ONLY])
            .on("dpm list-owners", &[OWNER_US])
            .on("dumpsys device_policy", &[DP_ADMIN])
            .on(
                "logcat",
                &["11-17 10:23:45.123 1 2 I LiMoSpike05: state admin=yes owner=yes deadline_min=28\n"],
            );
        assert_eq!(status(&mut f), 0);
        assert!(f.changed_nothing());
    }
}
