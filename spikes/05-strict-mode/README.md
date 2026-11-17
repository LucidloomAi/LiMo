# LiMo spike 05: strict mode (Device Owner), and above all its release

A small, throwaway proof for the LiMo screen time limiter. It is not the real
product. It answers one question before the real build starts, and then it
can be removed:

Can the family's Windows PC make LiMo the Device Owner of a child's Android
phone over USB, do the strict mode restrictions hold, and does release put
every single thing back?

The plan's rule is that a guardian can always release the phone, and that
the release path is built and tested first. So this spike is mostly about
getting out again.

There are two parts: `pc\`, a Rust program for the PC
(`limo-strict-spike.exe`), and `phone\`, a plain Kotlin Android app ("LiMo
spike 05"). The test is a checklist in `TESTING.md`, for a person at the PC
with the test phone (Samsung Galaxy S20 Ultra, SM-G988B, Android 13, One UI
5.1) on USB.

## What it proves, and what it does not

Once `TESTING.md` has been worked through, it shows:

1. Whether `dpm set-device-owner` works from the PC on this phone, and in
   plain words what is in the way when it does not.
2. That each way out works on its own, with nothing restricted: the release
   with the PIN, the dead man's timer, and `force-release` from the PC.
3. Whether each restriction holds: uninstalling, safe mode, factory reset
   from Settings, adding users, changing the clock, and opening a suspended
   app.
4. Whether all of that survives a restart, and whether the timer does.
5. That release puts everything back: every restriction off, the suspended
   app opens again, and `adb uninstall` works.

It does not:

- count time, apply limits or talk to a LiMo PC. It sets restrictions on
  command and lifts them, that is all;
- try to stop the child from doing anything else (force stopping, turning
  things off in Settings and so on). Those are other spikes;
- use any of the stronger Device Owner powers (see "Deliberately not used");
- say anything about other phones, other One UI versions or Android 14 and
  later;
- protect the release with anything better than a fixed PIN, `2468`. The
  real product uses the guardian's own PIN, and never passes it on a command
  line.

## What Device Owner is, and why setup needs an empty phone

A Device Owner is one app that Android lets manage the whole phone through
`DevicePolicyManager`: it can set user restrictions that the person at the
phone cannot turn off, block uninstalling, suspend apps and much more. A
phone has at most one. Normally a company sets it up while the phone is new,
during the setup screens.

The other way, meant for developers and testing, is adb:

```
adb shell dpm set-device-owner ai.lucidloom.limo.spike05/.AdminReceiver
```

Android allows this only when:

- there are no accounts on the phone (Google, Samsung, or any app's), unless
  every account's app has marked it as fine for a managed phone. In practice:
  remove them all;
- there is exactly one user. On a Samsung phone Secure Folder and Dual
  Messenger each add a user (often 150 and 95);
- there is no Device Owner and no profile owner (work profile) yet.

The reason is that an owner gets a lot of control: Android will not hand that
over quietly on a phone where someone's accounts and data are already set
up. `limo-strict-spike check` reads all of this and says exactly what is in
the way.

## The escape hatch: a test only build

The app's manifest says `android:testOnly="true"`. This must never be
removed. For a test only app Android lets adb remove the admin, even while it
is the Device Owner, without the app's help:

```
adb shell dpm remove-active-admin ai.lucidloom.limo.spike05/.AdminReceiver
```

That is `limo-strict-spike force-release`. It works even if the app is
broken, hangs or crashes on start. The cost: a test only APK installs only
with `adb install -t`, and `setup` refuses to make an owner of an installed
copy that is not test only. That check matters because Android records
whether an admin is test only once, when it is first made an admin, and goes
by that record from then on (`isAdminTestOnlyLocked` reads the saved
`testOnlyAdmin`, in Android 13's `DevicePolicyManagerService`).

If the app is ever broken in a way that blocks a release, a fixed build can
replace it with `adb install -t -r` even while it is the owner: blocking
uninstall does not block updates (from memory, not verified on this phone).

## What the app sets, and why each is safe to undo

Only while it is the Device Owner, each switchable on its own from the
screen and from adb, each logged, and each undone by release.

| What | Android call | What it blocks |
| --- | --- | --- |
| Uninstall blocked | `setUninstallBlocked(admin, own package, true)` | Uninstalling LiMo spike 05, from Settings and from `adb uninstall` |
| Date and time locked | `addUserRestriction(no_config_date_time)` | Changing the date, time and time zone in Settings |
| Adding users blocked | `addUserRestriction(no_add_user)` | Creating new users (One UI does not show Users on this phone anyway) |
| Safe mode blocked | `addUserRestriction(no_safe_boot)` | Starting in safe mode, where LiMo would not run |
| Listed apps suspended | `setPackagesSuspended(admin, list, true)` | Opening those apps at all: Android shows "not available" instead |
| Factory reset blocked | `addUserRestriction(no_factory_reset)` | Factory data reset from Settings. Not the recovery menu: nothing an app does can block that |

Why each is safe to undo:

- Release undoes each one itself, explicitly, every time, before it gives up
  ownership: it does not rely on Android to do it. Undoing something that was
  never set does no harm, so release runs every step whatever it believes was
  set.
- The four user restrictions and the uninstall block are also cleared by
  Android itself when the Device Owner goes, by either route. So even
  `force-release`, which never runs the app's code, leaves none of them
  behind. (Evidence: "What Android clears by itself" below.)
- Suspension is the exception: Android does **not** lift it by itself when
  the owner goes. Android 13's source suspends a Device Owner's packages
  under the system's own name, `android`, and nothing in the code that clears
  an owner lifts those suspensions. So after `force-release` a suspended app
  stays suspended until this app, made owner again, lifts it. That is why the
  app is built so a suspension can never be forgotten:
  - it writes the package down in its own storage before it asks Android to
    suspend it;
  - release lifts that list, then looks at every installed package and lifts
    any suspension it can (the "sweep"), in case the list was lost.
    Switching suspension off (`--ez suspend_apps false`) does the same two
    things;
  - `force-release` first asks the app, while it is still the owner, to lift
    its suspensions (`--ez suspend_apps false`), waits up to 10 seconds, and
    only then removes the owner. If the app is broken it goes on without it;
  - the PC program's `status`, `force-release` and `uninstall` read every
    suspended package from the phone and say which are left and how to lift
    them;
  - the list is at most 10 apps, each must have an icon in the app drawer,
    and a never suspend list (below) applies.

  If a suspension is left behind anyway (the owner was removed with the raw
  `dpm remove-active-admin`, or the app could not be asked), the way to lift
  it is, before anyone adds an account (setup needs a phone with none):

  1. `limo-strict-spike setup --apk phone\app\build\outputs\apk\debug\app-debug.apk`
  2. `adb shell am start -n ai.lucidloom.limo.spike05/.MainActivity --ez suspend_apps false`
  3. `limo-strict-spike release --pin 2468` (release lifts suspensions too;
     step 2 lets you see it happen before the owner goes)

  The adb shell's own `pm unsuspend` is no help: in Android 13's source it
  names the shell (`com.android.shell`) as the one lifting, not the platform
  (how the package manager treats that difference was not read in the
  source; from memory it lifts only the shell's own suspensions). Step 4.4 of
  `TESTING.md` shows all this on the real phone, since Samsung's build could
  differ from Android's source.

Never suspended, whatever the list says: this app; every launcher, dialer,
Settings app and keyboard; the default SMS app; every active device admin;
and a fixed list that includes Android itself, System UI, the adb shell,
phone and emergency calls, the package installer and permission controller,
and Google Play services. Launchers, dialers, Settings, keyboards and the SMS
app are looked up on the phone at run time, the same idea as spike 02's never
cover list. The rules are in `SuspendRules.kt`.

### Factory reset: restricted last, released first

Of everything strict mode does, the factory reset from Settings is the one a
tester most needs if something goes wrong: it is the way back that needs no
PC, no app and no PIN. So it is the last thing turned on, after everything
else has worked, and the first thing release turns off, before anything that
could fail or take time. If a release is cut short half way (the app is
killed, the phone restarts), the factory reset from Settings is already back.

### The order

Apply strict mode, in this order:

1. block uninstalling
2. lock the date and time
3. block adding users
4. block safe mode
5. suspend the listed apps
6. block factory reset

`releaseEverything()`, the one release used by every way out (the button,
the adb intent and the timer):

1. allow factory reset
2. lift the suspensions this app recorded (`unsuspend`)
3. lift any other suspension Android reports, in case the record was lost
   (`unsuspend_sweep`)
4. allow safe mode
5. allow adding users
6. unlock the date and time
7. unblock uninstalling
8. stop being Device Owner (`clearDeviceOwnerApp`)
9. stop being an active admin (`removeActiveAdmin`), and wait up to 5
   seconds for Android to finish it

Steps 1 to 7 are exactly the reverse of applying. Each step has its own
try block, so one failure cannot stop the rest, and each writes one log line.
Device Owner must go before the admin: Android silently ignores a request to
remove an admin that is still the owner. Clearing the owner also starts
removing the admin by itself, and Android finishes that only after it has
delivered "admin disabled" to the app and the app has answered. Two things
follow, both learned the hard way:

- the app answers "admin disabled" at once and never waits on its own work
  there;
- Android delivers this app's broadcasts one after another, so "admin
  disabled" also queues behind any broadcast the app is still holding open.
  The timer's alarm receiver holds its broadcast open (`goAsync`) while the
  release runs, to keep the process alive. In the first build that alarm, and
  a second one queued behind it, made the timer's release wait on itself: the
  admin step failed after 5 seconds, twice, and the admin went only once both
  had given up. Now, just before step 9 waits, the release finishes every
  broadcast the app is holding (`HoldSet`), and holds no new one until the
  wait is over. The work those broadcasts started still runs. The log shows
  how many it let go: `broadcasts_finished=N`.

If the app is still owner or admin at the end, the timer stays armed and the
release runs again a minute later. A release first replaces the deadline
alarms with that one minute retry alarm, so no deadline alarm fires in the
middle of it, and a retry still comes if the process dies half way. For 20
seconds after a release ends, the timer does not start another one: a check
queued during the release (a second alarm, "admin disabled") can still see
the admin listed for a moment, and is logged as
`timer skipped ... reason=release_just_ran` instead of running it all again.
That window is shorter than the one minute retry, so a release that did not
finish is still tried again on time.

After step 9 nothing of the app's remains in force and `adb uninstall`
works.

### What Android clears by itself when the owner goes

Read in Android 13's own source (AOSP, branch `android13-release`, file
`services/devicepolicy/java/com/android/server/devicepolicy/DevicePolicyManagerService.java`):

- The app's `clearDeviceOwnerApp` lands in `clearDeviceOwner`, which first
  requires the phone to be unlocked (`enforceUserUnlocked`), then calls
  `clearDeviceOwnerLocked` and then starts removing the admin.
- adb's `dpm remove-active-admin` lands in `forceRemoveActiveAdmin`, which
  refuses an admin that is not test only ("Attempt to remove non-test
  admin"), calls the same `clearDeviceOwnerLocked` when the admin is the
  owner, and removes the admin at once without telling the app.
- `clearDeviceOwnerLocked` sets the owner's user restrictions to none and
  pushes that to the user manager (through `clearUserPoliciesLocked` and
  `pushUserRestrictions`), and calls `clearBlockUninstallForUser`, which
  lifts every uninstall block. So the four restrictions and the uninstall
  block go with the owner, by either route.
- Nothing in those methods lifts package suspensions. The Device Owner's
  `setPackagesSuspended` suspends under `PLATFORM_PACKAGE_NAME`, and the only
  calls that lift those (`unsuspendForSuspendingPackage`) are in the code for
  a work profile's personal app suspension, not in any path that clears an
  owner. The value of `PLATFORM_PACKAGE_NAME` was not checked in the source;
  it is believed to be `android`, and the PC program treats a suspension by
  `android` (or by an unnamed suspender) as a Device Owner's. The phone shows
  the real name as `suspendingPackage=` in `adb shell dumpsys package NAME`.
- With `no_safe_boot` set, Android writes the global setting
  `safe_boot_disallowed` as 1, and as 0 when it is cleared (Android 13's
  `UserRestrictionsUtils`). It can be read with
  `adb shell settings get global safe_boot_disallowed`; `status` shows it.

That is stock Android. Samsung builds its own from it and could differ, so
the test checks it on the phone: step 4.4 in `TESTING.md` applies full
strict mode, removes the owner from the PC, and looks at what is left.

Android also does a few things of its own when an owner is set, and undoes
them when it goes: adb's `set-device-owner` marks the user's provisioning
state as finalized, and clearing the owner sets it back to unmanaged;
`clearDeviceOwnerLocked` turns the system backup service back on, turns off
security and network logging, and clears the "device owner type". If
`set-device-owner` fails, the shell command removes the admin it had just
added (all read in the Android 13 source). From memory, not verified: when the owner is set Android adds
the restriction `no_add_managed_profile` (you will see it in `status`) and
shows "this phone is managed" style notices, Samsung may show its own, and a
read only system property that says an owner is present may keep saying so
until the next restart.

## The ways out, in order

1. **Release with the PIN.** The app's "Release this phone" button asks for
   the PIN; or from the PC:
   ```
   .\pc\target\release\limo-strict-spike.exe release --pin 2468
   ```
   which sends
   ```
   adb shell am start -n ai.lucidloom.limo.spike05/.MainActivity --ez release true --es pin 2468
   ```
   and waits up to 15 seconds until `dpm list-owners` shows no owner and the
   app is no longer an admin. The phone should be unlocked with its screen on.
2. **The dead man's timer.** Whenever the app finds itself an admin or
   Device Owner it keeps a release deadline: 30 minutes from the moment it
   became owner. It can be lowered at any time, or extended by exactly 30
   minutes at a time, never to more than 120 minutes from now. When it runs
   out, the app runs the same `releaseEverything()` by itself, with no PIN.
   Details below.
3. **`force-release` from the PC**, the escape hatch:
   ```
   .\pc\target\release\limo-strict-spike.exe force-release
   ```
   It first says, in plain words, that Android lifts the restrictions when the
   owner is removed this way but not suspended apps. Then, if the app is
   still the owner and any app is suspended, it asks the app to lift them
   (`--ez suspend_apps false`) and waits up to 10 seconds. Then, whatever
   happened, it runs
   `adb shell dpm remove-active-admin ai.lucidloom.limo.spike05/.AdminReceiver`
   and checks the result. That last part needs no app code at all, so it
   works even if the app is broken. Afterwards it says whether any of the
   four restrictions or any suspension is left, and if a suspension is left,
   how to lift it (above).
4. **Factory reset from the recovery menu**, for completeness only. It works
   below Android, so nothing this app does can block it, but it erases
   everything on the phone: apps, photos, settings, and the owner with them.
   This app sets no factory reset protection policy, and the phone has no
   Google or Samsung account (setup needs that), so after a recovery reset
   the phone starts the setup screens like a new one with no account lock.
   How to reach the menu on an S20 Ultra is in `TESTING.md`. Use it only if
   1 to 3 have all failed.

## The dead man's timer

- The deadline is kept on two clocks: the wall clock, which survives a
  restart but can be changed, and the elapsed clock, which cannot be changed
  but starts from zero at every boot. The time left is always the smaller of
  the two, so moving the clock forward ends the timer early and moving it
  back gains nothing. A deadline more than 120 minutes ahead (only a clock
  change or a damaged record can make one) releases the phone at once: if in
  doubt, release.
- After a restart, the time left is the smaller of what the wall clock says
  and what was left at the last check before the restart minus the time since
  boot. If the deadline passed while the phone was off, the phone is released
  as soon as the timer can run.
- It runs when the activity is closed and after a restart: two exact alarms
  (one on each clock; the wall clock one is set 5 seconds after the deadline,
  so that with no clock change the elapsed one comes first and the release
  it starts cancels the other), a receiver for the restart, the first
  unlock, an update of the app and a change of the clock, and a check every
  time any part of the app starts (the process itself, the screen, every
  receiver), plus every 5 seconds while the screen is open.
- Android lets an owner step down only once the phone has been unlocked since
  it started: `clearDeviceOwner` and `removeActiveAdmin` both call
  `enforceUserUnlocked` (Android 13's source). The app is not "direct boot
  aware", so after a restart it cannot run before that first unlock anyway.
  A deadline that passed while the phone was off or still locked is acted on
  right after the first unlock (`BOOT_COMPLETED`, and `USER_UNLOCKED` if
  Android delivers it to this kind of receiver, which from memory it may
  not).
- The timer never gives up. If a release does not finish, the app logs
  `release incomplete ...` (with `user_unlocked=`), keeps the deadline, and
  tries again a minute later, and on every unlock, restart and start of any
  part of the app.
- `--ei deadline_minutes N` (0 to 120) only ever lowers it; 0 releases at
  once. "Extend 30 minutes" (`--ez extend true`) adds exactly 30, up to 120
  from now. Being made an admin again never extends a running deadline.
- Exact alarms: `USE_EXACT_ALARM` on Android 13 and later and
  `SCHEDULE_EXACT_ALARM` on Android 12, both granted at install. If Android
  refuses exact alarms anyway, the app sets inexact ones and the log says
  `exact=no`.
- If the app is force stopped, Android drops its alarms. The timer then acts
  the next time any part of the app starts (opening it, a restart). The
  reboot test covers this route.

## Deliberately not used

None of these is undone reliably by clearing the owner, or each can leave a
phone locked, wiped or cut off if something goes wrong. If unsure whether
something is cleared with the owner, the spike does not use it.

| Not used | Why |
| --- | --- |
| Factory reset protection policies | They decide who may set the phone up after a reset. A mistake could lock the phone after a recovery reset. |
| `setKeyguardDisabled` | Changes the lock screen; the screen lock is the family's, not LiMo's. |
| Lock task (kiosk) mode | Can trap the phone in one app. |
| Password or PIN changes or resets | The phone's own lock is never touched; a wrong one locks everyone out. |
| `wipeData` | Erases the phone. |
| Network, VPN or proxy policies | Can cut the phone off from the network, and from the PC. |
| System update policies | Can hold back security updates. |
| Disabling the camera | Not needed for the question, and touches another app's job. |
| Hiding apps (`setApplicationHidden`) | A hidden app is easy to lose track of; suspension shows the app greyed out instead. |
| `setStatusBarDisabled` | Can hide the way to Settings and notifications. |
| Anything that touches accounts | Accounts are the family's. |

The admin's policy list (`res/xml/device_admin.xml`) is empty on purpose:
Android then refuses this admin the old device admin powers (wiping, locking
the screen, password rules, camera, keyguard features) even if code asked for
them.

## The phone app: "LiMo spike 05"

Application id `ai.lucidloom.limo.spike05`, minSdk 28 (Android 9), target 36.
One screen, plain framework views, no third party libraries.

### The screen

- Owner: whether the app is an active admin, whether it is the Device Owner,
  whether it was installed as test only.
- Dead man's timer: minutes and seconds left, and the clock time; the button
  "Extend 30 minutes".
- Strict mode: "Apply strict mode", then one switch per item, each with
  whether this app set it and whether it is in force on the phone.
- Apps to suspend: a text field (package names, commas), "Suspend these", and
  each listed app with whether it is suspended now.
- Release: "Release this phone", which asks for the PIN.
- This phone: Android version, model, build, the number of users (readable
  only while Device Owner), and accounts (an app cannot read other apps'
  accounts without asking, so the PC's `check` reads them).
- The newest 60 log lines.

### Driven from the PC

Each of these opens the app (or reaches it if it is open) and is applied
once:

```
adb shell am start -n ai.lucidloom.limo.spike05/.MainActivity --ez apply true
adb shell am start -n ai.lucidloom.limo.spike05/.MainActivity --es suspend "com.samsung.android.calendar"
adb shell am start -n ai.lucidloom.limo.spike05/.MainActivity --ez release true --es pin 2468
adb shell am start -n ai.lucidloom.limo.spike05/.MainActivity --ei deadline_minutes 2
adb shell am start -n ai.lucidloom.limo.spike05/.MainActivity --ez extend true
```

One switch per item, `true` or `false`:

```
adb shell am start -n ai.lucidloom.limo.spike05/.MainActivity --ez block_uninstall true
adb shell am start -n ai.lucidloom.limo.spike05/.MainActivity --ez no_config_date_time true
adb shell am start -n ai.lucidloom.limo.spike05/.MainActivity --ez no_add_user true
adb shell am start -n ai.lucidloom.limo.spike05/.MainActivity --ez no_safe_boot true
adb shell am start -n ai.lucidloom.limo.spike05/.MainActivity --ez suspend_apps true
adb shell am start -n ai.lucidloom.limo.spike05/.MainActivity --ez no_factory_reset true
```

- `--es suspend` replaces the list and suspends it; an empty list
  (`--es suspend ""`) lifts every suspension this app made.
  `--ez suspend_apps false` also lifts them, then sweeps every installed
  package for any other suspension it can lift, the same as release does. Names that are
  not package names, packages on the never suspend list, packages without an
  app drawer icon and anything past the tenth are left out and logged as
  `suspend refused`.
- Extras can be combined. A release request does nothing else. Otherwise the
  order is: the deadline, extend, the list, then the switches, turned off in
  release order first and turned on in apply order after.
- Anything that needs the owner logs `failed ... error=not_owner` when the
  app is not the owner.

### Log lines

Tag `LiMoSpike05`, one line per event, every outcome leaves one. Read them
with `adb logcat -s LiMoSpike05`. The screen shows the same lines.

| Line | Meaning |
| --- | --- |
| `state admin=yes|no owner=yes|no deadline_min=N` | After every check, action and release. `N` is whole minutes left, rounded up, or `none`. While the screen is open it is logged only when it changes. |
| `apply start source=S steps=...` | A set of switch changes begins, in the order listed. |
| `apply step=NAME ok value=on|off` | One item done. For suspension: `count=N` when on; `count=N swept=M` when off (its own list, then the sweep). |
| `apply step=NAME failed value=on|off error=TEXT` | One item failed; `error=not_owner` when the app is not the owner. |
| `suspend list=a,b|none source=S` | The list to suspend was set. |
| `suspend refused package=P reason=R` | Left out: `bad_name`, `this_app`, `never_suspend`, `no_launcher_entry`, `too_many`. |
| `suspended list=a,b|none` | Packages this app suspended that are suspended now. |
| `release start trigger=button|adb|timer owner=... admin=... user_unlocked=yes|no` | A release begins. |
| `release incomplete owner=... admin=... user_unlocked=... next_try_s=60` | The release did not finish; the deadline is kept and it tries again. |
| `release step=NAME ok` | A release step worked. The names, in order: `no_factory_reset`, `unsuspend`, `unsuspend_sweep`, `no_safe_boot`, `no_add_user`, `no_config_date_time`, `uninstall_block`, `device_owner`, `active_admin`. |
| `release step=NAME failed error=TEXT` | A release step failed; the rest still ran. |
| `release step=NAME skipped reason=not_owner|not_admin` | Nothing to do for that step: the app was no longer owner (so Android had already cleared it) or no longer admin. |
| `release done owner=no admin=no` | The release finished and verified. Any `yes` here means it did not finish, and it runs again in a minute. |
| `release refused reason=wrong_pin|no_pin source=S` | The PIN was wrong or missing; nothing was changed. |
| `timer started deadline_min=N source=S` | A deadline was set because the app found itself admin or owner. |
| `timer kept deadline_min=N source=admin_enabled` | Made admin again while a deadline was running: it was not extended. |
| `timer set deadline_min=N`, `timer extended deadline_min=N` | Lowered, or extended by 30. |
| `timer refused ... reason=R` | `not_lower`, `out_of_range`, `at_most`, `already_due`, `no_deadline`, `not_admin`. |
| `timer fired source=S reason=R` | The timer released the phone. `R` is `deadline_passed`, `clock_moved_forward` or `deadline_too_far`. `S` says which check found it: `alarm`, `boot`, `activity`, `process_start` and so on. |
| `timer skipped source=S reason=release_just_ran ago_ms=N` | A check found the deadline passed less than 20 seconds after a release ended, and did not run another one. |
| `release step=active_admin ok waited_ms=N broadcasts_finished=N` | The admin went after N ms; the release let go of N held broadcasts first. `failed error=still_admin_after_ms=5000 ...` means it did not go within 5 seconds (the release then retries in a minute). |
| `broadcast not held source=S reason=release_waiting` | A timer broadcast arrived while a release was waiting for Android to remove the admin; its check still runs, but the broadcast is let go at once. |
| `timer cleared reason=not_admin source=S` | The app is no longer an admin (for example after `force-release`): the deadline is forgotten. |
| `alarm scheduled exact=yes|no in_min=N`, `... retry_s=60` | The timer's alarms were set. |
| `boot seen owner=yes|no` | The phone restarted and was unlocked. |
| `admin enabled`, `admin disabled` | Android told the app. |
| `intent release=... pin=given|missing ...` | Extras received from adb. The PIN itself is never logged. |
| `app start android=... sdk=... model=... test_only=yes|no` | The screen opened. |

### Permissions, and why

All granted at install, with no prompt: `RECEIVE_BOOT_COMPLETED` (the timer
after a restart), `USE_EXACT_ALARM` and on Android 12 `SCHEDULE_EXACT_ALARM`
(the timer on time), `QUERY_ALL_PACKAGES` (the release's sweep looks at every
installed package; fine for a spike that never goes to a store). Nothing
about accounts, the camera, the network or the screen lock.

## The PC program: `limo-strict-spike.exe`

It wraps `adb` and prints, before running it, every adb line it runs. It
finds adb on the PATH, then in `ANDROID_HOME` or `ANDROID_SDK_ROOT`, then in
`%LOCALAPPDATA%\Android\Sdk\platform-tools`, and talks to exactly one
connected device (with `-s SERIAL`). It needs no administrator rights and
writes no files.

| Command | What it does |
| --- | --- |
| `check` | Read only. adb's version, the one device, Android version and model, users (`pm list users`), accounts (`dumpsys account`; only their types are shown, never the names), owners (`dpm list-owners`), whether the app is installed and test only, whether it is an admin (`dumpsys device_policy`), and whether setup has finished on the phone. Then, in plain words, whether `set-device-owner` can work and, if not, exactly what is in the way. Anything it cannot read counts as in the way. |
| `setup --apk PATH [--deadline-minutes N]` | Runs `check` and refuses if anything is in the way. Then `adb install -t -r`, then checks the installed copy is test only (and refuses if not), then `dpm set-device-owner` once, then reads back `dpm list-owners`, then opens the app (`am start`, with `--ei deadline_minutes N` if given, 1 to 29) so its timer starts at once. It recognises the known failures and explains them. It never retries and changes nothing else. |
| `status` | Read only. Owner, admin, installed and test only, the restrictions in force on user 0 (`dumpsys user`), the setting `safe_boot_disallowed`, every suspended package (`dumpsys package packages`), and the app's last `state`, `suspended`, `timer`, `release done` and `boot seen` lines (`logcat -d`). |
| `release --pin PIN` | Sends the adb intent, waits up to 15 seconds, and checks with `dpm list-owners` and `dumpsys device_policy` that no owner and no admin is left. Then reports any restriction or suspension still in force. On failure it prints the app's release log lines. |
| `force-release` | Says in plain words that Android does not lift suspended apps when the owner is removed. Step 1: if the app is (or may be) still the owner and an app is suspended by a Device Owner, asks the app to lift its suspensions (`am start ... --ez suspend_apps false`) and checks every 2 seconds for up to 10; if the app cannot be asked or does not manage it, it goes on. Step 2: `dpm remove-active-admin`, then the same checks and report as `release`, including any suspension left and how to lift it. It never sends the PIN or asks for a release. It runs even when nothing could be read; it does nothing when the app is known to be neither owner nor admin. |
| `uninstall` | Refuses while the app is owner or admin, or when that cannot be read. Warns about any suspension still in force. Then `adb uninstall`, and then looks again with `pm list packages`: on the test phone a blocked uninstall answers `DELETE_FAILED_INTERNAL_ERROR`, the same as a package that is not there, so only the second look says whether the app went. |

Exit codes: 0 success, 1 refused or failed, 2 bad arguments. `check` exits
1 when `set-device-owner` cannot work.

Known failures it explains, using the texts in Android 13's source
(`computeProvisioningErrorString`) plus wording older and Samsung builds are
reported to use: accounts on the phone, more than one user, an owner or
profile owner already set, the phone already set up ("already set-up", or the
"already provisioned" some builds give), an unknown admin (app not installed
or a different build), the user not running, not the system user, a watch
already paired, the shell not allowed;
for installs: `INSTALL_FAILED_TEST_ONLY`, `INSTALL_FAILED_UPDATE_INCOMPATIBLE`,
`INSTALL_FAILED_VERSION_DOWNGRADE`, storage, an old Android, installing over
USB refused, a cancelled install, a damaged APK; for `force-release`: a
build that is not test only; for `uninstall`: `DELETE_FAILED_DEVICE_POLICY_MANAGER`
and `DELETE_FAILED_OWNER_BLOCKED`. An error it does not know is printed as
Android wrote it.

## Build

### PC

Needs Rust (built with 1.98.1, MSVC). In `pc\`:

```
cargo build --release
cargo test
```

The program is `pc\target\release\limo-strict-spike.exe`. The unit tests
(66) cover every parser with canned adb output (devices, getprop, users,
accounts, owners, admins, package flags, suspended packages, restrictions,
log lines, install, set-device-owner with each known error, remove admin,
uninstall, am start), every refusal rule (no phone, an unauthorized phone,
two phones, accounts found, two users, an owner already set, anything
unreadable, not test only, still owner or admin, which suspensions are a
Device Owner's), and every command run against a fake adb that records each
call: setup refuses before changing anything, never runs set-device-owner
twice, release waits and gives up after 15 seconds without doing anything
else, force-release asks the app to lift suspensions before it removes the
owner and goes on when the app cannot help, and so on.

### Phone

Needs JDK 17 and the Android SDK with platform `android-36` and build tools
35.0.0 or later (already on the build PC). In `phone\`:

```
.\gradlew.bat assembleDebug
.\gradlew.bat testDebugUnitTest
```

The APK is `phone\app\build\outputs\apk\debug\app-debug.apk`. The unit tests
(40) cover the pure parts: the deadline arithmetic (starts at 30, never more
than 120 ahead, a clock moved forward releases, a clock moved back cannot
extend, a clock moved far back releases, restarts, being made admin again,
lowering, extending, no second release within 20 seconds of one), parsing
the package list, the never suspend rules and the limit of 10, the order of
apply and release steps, planning switch changes, the PIN, the log line
shapes, and the held broadcasts (`HoldSet`: let go of all before the wait,
none held during it, never finished twice).

Versions: Gradle 8.14.3 (wrapper), Android Gradle Plugin 8.11.0, Kotlin
2.1.20, compileSdk and targetSdk 36, minSdk 28.

`local.properties` is not needed when `ANDROID_HOME` is set, as it is on the
build PC. To make it by hand, put one line in a new file `local.properties` in
`phone\`, with your own SDK path, for example
`sdk.dir=C\:\\Users\\you\\AppData\\Local\\Android\\Sdk`. It is listed in
`.gitignore` and must never be committed.

## Tools and libraries

The spike's own code is Apache-2.0. Nothing is copied from any other project.

PC program: no crates at all, only the Rust standard library (MIT OR
Apache-2.0), built with Rust 1.98.1 and the Visual Studio 2022 C++ build
tools. It runs `adb` from the Android SDK platform tools (Apache-2.0), which
is not shipped with it.

Phone app:

| Part | Version | Licence | Where |
| --- | --- | --- | --- |
| Kotlin standard library | 2.1.20 | Apache-2.0 | in the APK |
| JetBrains annotations | 13.0 | Apache-2.0 | in the APK (comes with the standard library) |
| JUnit | 4.13.2 | EPL-1.0 | unit tests only, not in the APK |
| Hamcrest core | 1.3 | BSD-3-Clause | unit tests only (comes with JUnit), not in the APK |
| Gradle (wrapper) | 8.14.3 | Apache-2.0 | build only |
| Android Gradle Plugin | 8.11.0 | Apache-2.0 | build only |
| Kotlin compiler and Gradle plugin | 2.1.20 | Apache-2.0 | build only |

The Gradle wrapper files (`gradlew`, `gradlew.bat`, `gradle\wrapper\`) are
Gradle's own, copied unchanged from spike 02.

## First run on the phone, 17 Nov 2026

A spare Galaxy S20 Ultra (SM-G988B, Android 13, One UI 5.1), factory reset by
its owner with no account added, one user, USB debugging on. Everything was
run from the PC over USB with `limo-strict-spike` and plain `adb`. The phone
sat at its PIN screen for most of the run; the commands still reached the app.

| Check | Result |
| --- | --- |
| `check` | "Can set-device-owner work: yes" |
| `setup` | Android said Success and the owner was confirmed, eight times out of eight |
| Way out 1, `release --pin 2468`, nothing restricted | every step ok, owner and admin gone in about a second |
| Way out 3, `force-release`, nothing restricted | Android said Success, owner and admin gone |
| Way out 2, the timer at 2 minutes, app closed, screen off, first build | the phone released itself, but the admin was removed about 10 s late (the flaw described under "The ways out") |
| The same on the fixed build | one release run, 1.3 s from the alarm to "release done", `active_admin ok waited_ms=750 broadcasts_finished=1` |
| Strict mode applied, Samsung Calendar suspended | all six apply steps ok |
| `status` with strict mode on | the four restrictions in force, plus `no_add_managed_profile` and `no_add_clone_profile`, which Android adds by itself; `safe_boot_disallowed` 1; Calendar suspended by `android` |
| A raw `adb uninstall` with strict mode on | `Failure [DELETE_FAILED_INTERNAL_ERROR]`, the app stayed |
| `release --pin 2468` with strict mode on | every step ok, `unsuspend ok count=1`; afterwards no restrictions, Calendar not suspended, `safe_boot_disallowed` 0 |
| `force-release` with strict mode on (step 4.4 part A) | the app lifted the suspension first, then Android removed the owner; nothing left behind |
| The raw `dpm remove-active-admin` with strict mode on (part B) | owner, admin and all restrictions gone, but Calendar still suspended by `android`; `pm unsuspend` from the shell answered "new suspended state: true" and changed nothing |
| The way back after part B | `setup`, then `--ez suspend_apps false` (Calendar no longer suspended), then `release`: clean |

So on this Samsung build Android behaves as its source says. Clearing the
owner lifts the user restrictions, the uninstall block and the safe boot
setting, by either route. It does not lift package suspensions, and nothing
but a Device Owner can lift them afterwards.

Not done yet in this run, because both need the phone's PIN: looking at the
screen with strict mode on (what a child sees on opening the suspended app,
and how Settings shows the restrictions), and a restart while owner.

## Unknowns

- Whether Samsung's build behaves like Android 13's source when the owner
  goes: restrictions and the uninstall block cleared, suspensions left in
  place. The app's own release lifts suspensions explicitly, and
  `force-release` asks the app to lift them first, so only a broken app
  leaves one behind; step 4.4 in `TESTING.md` checks all of it.
- The value of `PLATFORM_PACKAGE_NAME`, believed to be `android` (see above).
- Whether One UI's power menu honours `no_safe_boot` the way stock Android's
  does, and what it shows. The test records it.
- What Samsung shows for each blocked setting (greyed out, a message, or
  nothing), and whether Samsung adds notices of its own when an owner is
  set.
- Whether this Samsung build gives the "already provisioned" refusal at all.
- Every adb output this program reads was checked against Android 13's
  source (AOSP, `android13-release`): `dpm list-owners`, `set-device-owner`,
  `remove-active-admin`, `dumpsys device_policy`, `dumpsys package`,
  `dumpsys user`, `pm list users` and `dumpsys account`. Samsung's build could
  still print something different. Each parser says "could not read" rather
  than guess, and anything unreadable stops `setup`.
- Whether the alarms are exact on this phone (`exact=` in the log).
