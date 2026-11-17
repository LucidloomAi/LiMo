# Testing the LiMo strict mode spike

This checklist is for a person at the Windows PC with the test phone (Samsung
Galaxy S20 Ultra, SM-G988B, Android 13, One UI 5.1) plugged in with a USB
cable. It answers four questions, in this order: do all the ways out work
(before anything is restricted), do the restrictions hold, do they survive a
restart, and does release put everything back. Write what you see in the
Results tables at the end.

Commands are typed in PowerShell, in this spike folder
(`LiMo\spikes\05-strict-mode`), unless a step says otherwise. Two windows
help:

- window A: the phone's log, left running;
- window B: everything else.

`adb` is on the PATH of the build PC (it lives in
`%ANDROID_HOME%\platform-tools`). The PC program prints every adb line before
it runs it, so window B always shows exactly what was sent to the phone.

## Before you start: safety

- **The ways out, in order.** 1: release with the PIN (`release --pin 2468`,
  or the app's "Release this phone" button). 2: the dead man's timer: the
  phone releases itself when it runs out, 30 minutes after setup unless
  extended, never more than 120 minutes ahead. The app shows the time left.
  3: `force-release` from the PC, which works even if the app is broken.
  Step 1 below proves all three before anything is restricted.
- **The last resort** is a factory reset from the recovery menu. It erases
  everything on the phone. Use it only if all three ways out have failed. How
  is in step 5.
- **`force-release` does not lift suspended apps.** Android 13 lifts the
  restrictions and the uninstall block when the owner goes, but not a
  suspension. Only the app's own release does that. So after a
  `force-release` with an app suspended, run `setup` then
  `release --pin 2468` to lift it, before anyone adds an account. Suspend only
  a harmless app (Calculator) in this test.
- **Never confirm a factory reset in this test.** Step 2 only looks at
  whether the Settings entry is blocked. If a screen with a Reset or Delete all
  button appears, press Back.
- **Do not add any account** (Google, Samsung or any app's) while testing. It
  is the family's, and with an account on the phone `setup` cannot run again.
- **Keep the phone unlocked with its screen on** whenever you send it a
  command. Setting the screen timeout to 10 minutes helps: Settings, Display,
  Screen timeout.
- If anything seems stuck at any point, go to step 5, "If anything goes
  wrong".

## Setup

1. Build both parts, if not done already:
   ```
   cd pc
   cargo build --release
   cd ..\phone
   .\gradlew.bat assembleDebug
   cd ..
   ```
   The PC program is `pc\target\release\limo-strict-spike.exe`, the APK
   `phone\app\build\outputs\apk\debug\app-debug.apk`.

2. On the phone, turn on USB debugging if it is not on yet: Settings, About
   phone, Software information, tap Build number seven times; then Settings,
   Developer options, USB debugging on. Plug the phone in and accept the
   "Allow USB debugging?" prompt (tick "Always allow from this computer").

3. Check the connection:
   ```
   adb devices
   ```
   Pass: one line with a serial number followed by `device`. If it says
   `unauthorized`, unlock the phone and accept the prompt from step 2. Unplug
   any other phone and close any emulator: the PC program refuses to choose
   between two.

4. In window A, start the phone's log and leave it running:
   ```
   adb logcat -c
   adb logcat -s LiMoSpike05
   ```
   A restart of the phone ends it; start it again afterwards. See "Reading
   the log over USB" below for saving it to a file.

5. Write down, for the Results table: Settings, About phone, Software
   information: One UI version, Android version, Build number.

---

## Step 0: before anything

0.1. On the phone, check by hand:
- Settings, Accounts and backup, Manage accounts: no account at all.
- No Secure Folder (it would show in the app drawer and under Settings,
  Security and privacy).
- Settings, Advanced features, Dual Messenger: off for every app.

0.2. In window B:
```
.\pc\target\release\limo-strict-spike.exe check
$LASTEXITCODE
```
It only reads. Pass: `$LASTEXITCODE` is 0 and the last line says
`Can set-device-owner work: yes. Nothing is in the way.` Under "What the
phone says": `Users: 1 (0 "Owner")`, `Accounts: 0`,
`Device or profile owners: none`.

If it says no, it lists what is in the way and what to do about each. Fix
those by hand and run `check` again. Nothing was changed on the phone.

Write down: the Users, Accounts and owners lines, the
`device_provisioned` and `user_setup_complete` values, and anything that was
in the way and how you fixed it.

---

## Step 1: the ways out first, with nothing restricted

Do not go on to step 2 unless all three of 1.2, 1.4 and 1.6 passed.

### 1.1 Set up

```
.\pc\target\release\limo-strict-spike.exe setup --apk phone\app\build\outputs\apk\debug\app-debug.apk
$LASTEXITCODE
```

Pass: exit code 0; window B shows "Step 1 of 4" to "Step 4 of 4", `Installed.`,
`Test only: yes`, `Android said Success.`,
`Confirmed: LiMo spike 05 is the Device Owner.` and `The app is open on the phone.`
The app is open on the phone and shows `Device Owner: yes`,
`Installed as test only: yes`, and the timer at about 30 minutes. Window A
shows `admin enabled`, `timer started deadline_min=30 ...` and
`state admin=yes owner=yes deadline_min=30`.

If `set-device-owner` was refused, window B quotes Android's words and
explains them. It is never retried. Write the words down; that is a result.

Write down: anything unexpected in windows A or B; any notice Samsung showed
on the phone (for example about the phone being managed).

Optional, while nothing is restricted:
```
adb uninstall ai.lucidloom.limo.spike05
```
Expected: `Failure [DELETE_FAILED_DEVICE_POLICY_MANAGER]`: Android does not
let an owner be uninstalled even without the uninstall block. Write down what
it said.

### 1.2 Way out 1: release with the PIN

```
.\pc\target\release\limo-strict-spike.exe release --pin 2468
$LASTEXITCODE
```

Pass: exit code 0 and
`Released: no Device Owner is left, and LiMo spike 05 is no longer an admin.`,
then `Restrictions this spike sets, in force now: none.` and
`Suspended packages: none.` Window A shows `release start trigger=adb`, nine
`release step=` lines starting with `no_factory_reset` and ending with
`active_admin`, then `release done owner=no admin=no`.

Then:
```
.\pc\target\release\limo-strict-spike.exe check
```
Pass: `Device or profile owners: none` and `active admin no`.

Write down: the exit code, the `release step=` lines that were not `ok`
(`skipped` or `failed`), and how many seconds the release took.

### 1.3 Set up again, and try a wrong PIN

```
.\pc\target\release\limo-strict-spike.exe setup --apk phone\app\build\outputs\apk\debug\app-debug.apk
.\pc\target\release\limo-strict-spike.exe release --pin 1111
$LASTEXITCODE
```

Pass: setup as in 1.1; then the release waits 15 seconds and says
`LiMo spike 05 is still the Device Owner after 15 seconds.`, exit code 1.
Window A shows `release refused reason=wrong_pin source=adb` and nothing was
changed.

### 1.4 Way out 3: force-release from the PC

The phone is still the owner from 1.3.
```
.\pc\target\release\limo-strict-spike.exe force-release
$LASTEXITCODE
```

Pass: exit code 0, `Android said Success.`,
`After: Device Owner no, active admin no.`, `Released from the PC.`, and
`Restrictions this spike sets, in force now: none.` Window A shows nothing
from the app at that moment: force-release does not run the app. Later (when
its alarm comes, or when you open it) it logs
`timer cleared reason=not_admin`.

Write down: exactly what window B printed after `> adb ... remove-active-admin ...`.

### 1.5 Set up with a 2 minute timer

```
.\pc\target\release\limo-strict-spike.exe setup --apk phone\app\build\outputs\apk\debug\app-debug.apk --deadline-minutes 2
```

This is the same as setup followed by
`adb shell am start -n ai.lucidloom.limo.spike05/.MainActivity --ei deadline_minutes 2`.

Pass: setup as in 1.1, and the app shows about 2 minutes left. Window A
shows `timer set deadline_min=2` and `alarm scheduled exact=yes ...`.

### 1.6 Way out 2: the dead man's timer

1. Close the app: open Recents and swipe LiMo spike 05 away.
2. Turn the screen off with the Side key. Write down the time.
3. Do nothing for 3 minutes.
4. Then:
   ```
   .\pc\target\release\limo-strict-spike.exe status
   ```

Pass: `LiMo spike 05 is the Device Owner: no` and
`LiMo spike 05 is an active admin: no`. Window A shows
`timer fired source=alarm reason=deadline_passed`, the nine release steps and
`release done owner=no admin=no`, about 2 minutes after 1.5.

Write down: the time of `timer fired` (window A shows times if saved with
`-v time`, see below), its `source=`, and `exact=` from the alarm line.

If it did not fire within 5 minutes: unlock the phone and open the app (any
start of the app runs the same check). Write down whether that released it,
and from which `source=`. If not, use `force-release`.

---

## Step 2: strict mode

### 2.1 Set up, and make time

```
.\pc\target\release\limo-strict-spike.exe setup --apk phone\app\build\outputs\apk\debug\app-debug.apk
```

Steps 2 and 3 together can take longer than 30 minutes. In the app, tap
"Extend 30 minutes" once or twice (at most 120 minutes from now), or:
```
adb shell am start -n ai.lucidloom.limo.spike05/.MainActivity --ez extend true
```
Keep an eye on the time left throughout; extend again when it drops below 15
minutes.

### 2.2 Choose one harmless app to suspend

Calculator is a good choice. Check its package name:
```
adb shell pm list packages calculator
```
On this phone it is expected to be `com.sec.android.app.popupcalculator`. Use
what it prints below.

### 2.3 Apply strict mode

```
adb shell am start -n ai.lucidloom.limo.spike05/.MainActivity --es suspend "com.sec.android.app.popupcalculator" --ez apply true
```

(The app's text field and "Suspend these", then "Apply strict mode", do the
same.)

Pass: window A shows
`suspend list=com.sec.android.app.popupcalculator source=adb`, then
`apply start source=adb steps=block_uninstall:on,no_config_date_time:on,no_add_user:on,no_safe_boot:on,suspend_apps:on,no_factory_reset:on`,
six `apply step=... ok value=on` lines in that order, and
`suspended list=com.sec.android.app.popupcalculator`. In the app every switch
is on, and each restriction says `Set by this app: yes, in force on this phone: yes`.

### 2.4 Check by adb

```
.\pc\target\release\limo-strict-spike.exe status
```

Pass: `Of the four this spike sets: no_safe_boot, no_factory_reset, no_add_user, no_config_date_time`
(in any order) and `Suspended: com.sec.android.app.popupcalculator (by: android)`.
Write down the whole "Restrictions in force on user 0" line.

### 2.5 Uninstalling is blocked

1. Settings, Apps, LiMo spike 05. Write down what the Uninstall button looks
   like and what happens when you tap it.
2. Long press the LiMo spike 05 icon in the app drawer, Uninstall. Write
   down what happens.
3. From the PC:
   ```
   adb uninstall ai.lucidloom.limo.spike05
   ```
   Pass: `Failure [DELETE_FAILED_OWNER_BLOCKED]` or
   `Failure [DELETE_FAILED_DEVICE_POLICY_MANAGER]`. Write down which.

### 2.6 The suspended app cannot be opened

Tap Calculator in the app drawer. Pass: it does not open; Android shows a
message instead (something like "Calculator isn't available"). Write down the
message, and whether the icon looks greyed out. If Calculator was in Recents,
tap its card too.

### 2.7 Date and time are locked

Settings, General management, Date and time. Pass: the switches and "Set
date" and "Set time" are greyed out, or a message says it is blocked. Change
nothing. Write down what you saw.

### 2.8 Adding users is blocked

One UI does not show a Users page on this phone, so there is nothing to try
by hand. The `no_add_user` entry in 2.4 is the check. Optional: search
Settings for "users" and write down what comes up.

### 2.9 Factory reset from Settings is blocked

**Look, never confirm.** Settings, General management, Reset, then tap
"Factory data reset" once.

Pass: it is greyed out, or a message says it is blocked by the admin, or the
next screen has no working Reset button. If a screen appears with a Reset (or
Delete all) button that looks usable, **do not tap it: press Back.** That
counts as a fail. Write down exactly what you saw.

### 2.10 Safe mode is blocked

Keep at least 15 minutes on the timer before this one.

1. The power menu way: press the Side key and Volume down together briefly
   (or swipe down twice from the top and tap the power icon). Touch and hold
   "Power off".
   Pass: no safe mode prompt appears, or tapping it does nothing. If a "Safe
   mode" button appears, write that down: that is a fail of the restriction.
   You may tap it to see what happens; if the phone really starts in safe
   mode ("Safe mode" at the bottom left of the screen), restart it normally
   (power menu, Restart) to leave safe mode. LiMo does not run in safe mode,
   so its timer only carries on after that restart; `force-release` from the
   PC still works in safe mode.
2. Optional, the key way: power the phone off, then turn it on, and as soon
   as the Samsung logo appears hold Volume down until the lock screen. Pass:
   no "Safe mode" at the bottom left. Unlock it, restart the log in window A,
   and check `status` still shows the owner.

Write down what each way did.

### 2.11 Optional: one switch at a time

In the app, turn one switch off and on again (for example "Safe mode
blocked"). Pass: window A shows `apply step=no_safe_boot ok value=off`, then
`... value=on`, and `status` follows.

---

## Step 3: restart while owner

3.1. Make sure at least 15 minutes are left (extend if not). Write down the
minutes left and the time.

3.2. Restart the phone: power menu, Restart (or `adb reboot`).

3.3. Wait for the lock screen, then unlock the phone. In window A start the
log again:
```
adb wait-for-device
adb logcat -s LiMoSpike05
```

3.4. Pass:
- window A shows `boot seen owner=yes`, then
  `state admin=yes owner=yes deadline_min=N` where N is what was left before
  the restart minus the minutes it took, and `alarm scheduled ...`;
- `.\pc\target\release\limo-strict-spike.exe status` shows the owner, all
  four restrictions and Calculator suspended;
- the app shows the timer counting down from about the same point, and every
  switch on.

If `boot seen` does not appear within a minute of unlocking, open the app and
write that down.

Write down: the `boot seen` and `state` lines, the minutes before and after,
and anything that was off after the restart.

---

## Step 4: release with the PIN, and check that everything is back

4.1. Release:
```
.\pc\target\release\limo-strict-spike.exe release --pin 2468
$LASTEXITCODE
```
Pass: exit code 0, `Released: ...`,
`Restrictions this spike sets, in force now: none.` and
`Suspended packages: none.` Window A shows the nine release steps in order
(`no_factory_reset` first, `unsuspend` second, `active_admin` last), each `ok`
(or `skipped`), and `release done owner=no admin=no`.

4.2. Check by hand that everything is back:
- Calculator opens normally.
- Settings, General management, Date and time: no longer greyed out.
- Settings, General management, Reset: "Factory data reset" opens its
  explanation screen. **Do not reset: press Back.**
- Power menu, touch and hold "Power off": the safe mode prompt appears again
  (if it did not appear before the test, write that down instead). Tap
  Cancel.
- Settings, Apps, LiMo spike 05: Uninstall is available. Do not tap it; the
  next step uninstalls from the PC.

4.3. Uninstall:
```
.\pc\target\release\limo-strict-spike.exe uninstall
$LASTEXITCODE
```
Pass: exit code 0 and `Uninstalled.` Then
```
adb shell pm list packages spike05
```
prints nothing.

4.4. Optional, but worth it: `force-release` with strict mode on. Android
13's source says removing the owner from the PC lifts the four restrictions
and the uninstall block, but leaves a Device Owner's suspension in place.
This checks that Samsung's build does the same, and that the way to lift the
suspension works.
```
.\pc\target\release\limo-strict-spike.exe setup --apk phone\app\build\outputs\apk\debug\app-debug.apk
adb shell am start -n ai.lucidloom.limo.spike05/.MainActivity --es suspend "com.sec.android.app.popupcalculator" --ez apply true
.\pc\target\release\limo-strict-spike.exe status
.\pc\target\release\limo-strict-spike.exe force-release
```
Pass for `status`: all four restrictions in force and Calculator suspended,
as in 2.4.

Expected from `force-release`: `After: Device Owner no, active admin no.`,
`Restrictions this spike sets, in force now: none.`, and
`Still suspended: com.sec.android.app.popupcalculator (suspended by: android)`.
Write down what it reported if different. By hand: Date and time is usable
again, and Calculator still does not open. If any of the four restrictions
is still in force, that is the most important finding of this step: write it
down, then lift it as below.

Then lift the suspension and clean up:
```
.\pc\target\release\limo-strict-spike.exe setup --apk phone\app\build\outputs\apk\debug\app-debug.apk
.\pc\target\release\limo-strict-spike.exe release --pin 2468
.\pc\target\release\limo-strict-spike.exe uninstall
```
Pass for the clean up: release reports `Suspended packages: none.` and
Calculator opens. Window A's `unsuspend` or `unsuspend_sweep` line names it.

---

## Step 5: cleaning up, and if anything goes wrong

### Cleaning up

```
.\pc\target\release\limo-strict-spike.exe uninstall
.\pc\target\release\limo-strict-spike.exe check
```
Pass: `LiMo spike 05: installed no` and `Device or profile owners: none`.
Turn off USB debugging (Settings, Developer options) if the phone is not
used for testing again soon.

### If anything goes wrong

At any step, take the ways out in this order, and run `status` after each to
see where things stand:

1. **Release with the PIN**:
   `.\pc\target\release\limo-strict-spike.exe release --pin 2468`, or the
   app's "Release this phone" button with 2468. If it fails, it prints the
   app's release lines; write them down.
2. **Wait for the dead man's timer.** The app shows when it runs out; it is
   never more than 120 minutes after the last extension. If the app was force
   stopped, Android dropped its alarm: open the app once, or restart the
   phone, and the timer acts at once if it is due.
3. **force-release**:
   `.\pc\target\release\limo-strict-spike.exe force-release`. It works even if
   the app crashes on start. Then run `uninstall`.

Particular cases:

- `setup` says something is in the way: nothing was changed on the phone. Fix
  what it names by hand and run `check` again.
- `status` after `force-release` still shows one of the four restrictions or a
  suspended package: set up again and release with the PIN (`setup`, then
  `release --pin 2468`). Release lifts everything it can find.
- `uninstall` is refused with `DELETE_FAILED_OWNER_BLOCKED` after a release:
  run `status`; then `force-release`, then `uninstall` again.
- The app itself is broken: `force-release` first. A fixed build can also be
  put on top with `adb install -t -r phone\app\build\outputs\apk\debug\app-debug.apk`
  even while it is the owner.
- The phone is in safe mode: restart it normally (power menu, Restart).
- `adb devices` shows nothing after a restart: unlock the phone, unplug and
  plug it in again, and accept the USB debugging prompt if it shows.

**The last resort, only if all three ways out failed: factory reset from the
recovery menu.** It erases everything on the phone. The phone has no Google
or Samsung account and this app sets no factory reset protection, so the
phone then starts its setup screens like a new one, with no account lock.
On a Galaxy S20 Ultra (from memory, not tried in this spike): turn the phone
off; connect it to the PC with the USB cable (One UI 3 and later wants a
cable for this); hold Volume up and the Side key together until the recovery
menu appears; use the volume keys to reach "Wipe data/factory reset" and the
Side key to choose it, then confirm; afterwards choose "Reboot system now".

---

## Reading the log over USB

Live, in window A:
```
adb logcat -s LiMoSpike05
```

With times, saved to a file to attach to the results:
```
adb logcat -d -v time -s LiMoSpike05 | Out-File -Encoding utf8 limo-spike05-log.txt
```
The phone keeps only a limited log, and a restart clears it, so save it
before step 3 and again at the end. All log lines are explained in
`README.md`, "Log lines".

---

## Results

### The phone

| Item | Value |
| --- | --- |
| Model | |
| One UI version | |
| Android version | |
| Build number | |
| Date of the test | |

### Step 0: before anything

| Item | Value |
| --- | --- |
| `check` exit code | |
| Users line | |
| Accounts line | |
| Owners line | |
| `device_provisioned`, `user_setup_complete` | |
| Anything in the way, and how it was fixed | |

### Step 1: the ways out, nothing restricted

| Check | Pass or fail | Notes (exit codes, times, odd lines) |
| --- | --- | --- |
| 1.1 setup | | |
| 1.1 Samsung notice when the owner was set | | |
| 1.1 optional: `adb uninstall` while owner, no block | | |
| 1.2 release with the PIN | | |
| 1.2 release steps not `ok` | | |
| 1.3 wrong PIN refused, nothing changed | | |
| 1.4 force-release | | |
| 1.5 setup with 2 minutes | | |
| 1.6 the timer released the phone by itself | | |
| 1.6 `timer fired` source, minutes after setup, `exact=` | | |

### Step 2: strict mode

| Check | Expected | What happened |
| --- | --- | --- |
| 2.3 apply: six `ok` lines in order | yes | |
| 2.4 `status`: the four restrictions, Calculator suspended | yes | |
| 2.4 full restriction list | | |
| 2.5 Uninstall in Settings | greyed out or refused | |
| 2.5 Uninstall from the icon | refused | |
| 2.5 `adb uninstall` | `DELETE_FAILED_OWNER_BLOCKED` or `DELETE_FAILED_DEVICE_POLICY_MANAGER` | |
| 2.6 Calculator | does not open, a message instead | |
| 2.7 Date and time | greyed out or blocked | |
| 2.8 Adding users | `no_add_user` in force | |
| 2.9 Factory data reset in Settings | blocked; never confirmed | |
| 2.10 Safe mode from the power menu | no prompt, or the prompt does nothing | |
| 2.10 Safe mode with the keys (optional) | starts normally | |
| 2.11 One switch at a time (optional) | off and on, logged | |

### Step 3: restart while owner

| Item | Value |
| --- | --- |
| Minutes left before the restart, and the time | |
| `boot seen` line | |
| `state` line after the restart | |
| Still owner | |
| Restrictions after the restart | |
| Calculator still suspended | |
| Timer consistent with the time that passed | |

### Step 4: release with the PIN, everything back

| Check | Expected | What happened |
| --- | --- | --- |
| 4.1 release exit code and report | 0, none in force, none suspended | |
| 4.1 release steps in order | `no_factory_reset` first, `active_admin` last | |
| 4.2 Calculator opens | yes | |
| 4.2 Date and time usable | yes | |
| 4.2 Factory data reset reachable (not confirmed) | yes | |
| 4.2 Safe mode prompt back | yes | |
| 4.2 Uninstall available in Settings | yes | |
| 4.3 `uninstall` | `Uninstalled.` | |
| 4.4 optional: force-release with strict mode on: owner and admin gone | yes | |
| 4.4 optional: the four restrictions gone after force-release | yes, per Android 13's source | |
| 4.4 optional: Calculator still suspended after force-release | yes, per Android 13's source | |
| 4.4 optional: setup and release lifted it | yes | |

### Notes

Anything else: messages Samsung showed, steps that needed a second try,
anything that surprised you.
