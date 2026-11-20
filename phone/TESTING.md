# Testing LiMo on a child's phone

These are the steps to try LiMo's child form on a real phone, with the owner
there, before pairing over the network exists: the PC tool
[`devtool/`](devtool/) (`limo-phone-dev`) installs a debug build of the app
over the USB cable and puts a test family on it. The steps go from the
safest to the riskiest; stop at any part, and undo it as its last lines say.
Every step says what to look for, and the results go in the table at the
end.

**Some steps wait for parts of the app that are not built yet.** Each is
marked **Waits for**, with the piece it needs: the host (the engine's
thread, the service that keeps LiMo running with its time left
notification, the restart, and the debug receiver that takes the test
family), the Kotlin side of the screens' seam, and the covers and the ways
out. Until those land, such a step gives what it says under it, or nothing
to see; the lead finishes this page when they do.

Everything is undone by two commands, each safe to run twice:
`limo-phone-dev uninstall` removes LiMo from the phone with everything it
kept, and `limo-phone-dev forget --this-pc-only` deletes the test family on
the PC. The tool never changes anything on the phone but LiMo's own app and
LiMo's own files: developer options and USB debugging are turned on and off
by a person, on the phone, and the tool never touches the phone's accounts,
its Device Owner or its settings.

## Before starting

- **A test phone**, Android 8.0 or later, ideally one with nothing personal
  on it: LiMo's child form asks for accessibility and usage access. The
  first run is planned on the Galaxy.
- **On the phone, by hand:** developer options on (Settings, About phone,
  Software information, tap Build number seven times) and USB debugging on
  (Settings, Developer options). Some phones also have a setting that
  blocks installs or commands over USB (Samsung's Auto Blocker, or "Install
  via USB" on others): if an install is refused, part C says what to do.
  The tool never changes any of these.
- **On the PC:** Android's SDK platform tools, with `adb` on the PATH or
  `ANDROID_HOME` set. Build the tool: in `phone\devtool`, `cargo build
  --release`. Every command below is run from `phone\devtool\target\release`
  as `.\limo-phone-dev ...` in PowerShell; the exit code is in
  `$LASTEXITCODE`. Each call it makes to the phone is printed first, as
  `> adb ...`.
- **The debug child form.** **Waits for** the host's piece, which brings
  the debug receiver and a debug build of the child form (being built as
  `npm run build:child:debug` in `phone\app`, giving
  `out\limo-child-debug.apk`; the lead confirms the command here). The
  release build, `npm run build:child`, is refused by the tool on purpose
  (part C, step 1). Below, the debug APK is called `DEBUG.apk`.
- **The two clocks.** The phone's clock and the PC's should agree to a few
  minutes: `setup` refuses when they are 50 minutes or more apart, since
  LiMo's debug build takes a pairing file only when it was made within an
  hour of the phone's own clock.
- **An app to block**: a harmless app on the phone, such as its calculator,
  and its package name. `adb shell pm list packages calc` lists the
  packages with "calc" in their names (on a Galaxy,
  `com.sec.android.app.popupcalculator`); below it is called `BLOCKED`.
- **A second terminal** for the log: `.\limo-phone-dev log` there follows
  what LiMo says, through every part, until Ctrl+C.
- **A test PIN** of 6 to 8 digits, written down: parts D and J use it.

## Part A: no phone, nothing changed

The phone unplugged.

1. `.\limo-phone-dev help`: first what it never does (only LiMo's own app
   and files; never developer options or USB debugging; never accounts,
   Device Owner or settings), then the commands, then the folder of the test
   family. Exit code 0.
2. `.\limo-phone-dev check`: `No phone is attached. Plug the phone in with
   a USB cable, unlock it, and check that USB debugging is on (Settings,
   Developer options). This tool never turns it on by itself.` Exit code 2.
3. `.\limo-phone-dev code`: `There is no test family on this PC. Run setup
   first.` Exit code 2.
4. `.\limo-phone-dev setup --child Asha --minutes 10 --block calculator`:
   `--block takes an app's package name, such as com.example.game, not
   "calculator".` Exit code 2, and no PIN asked for.

Undo: nothing to undo.

## Part B: the phone attached, read only

1. Plug the phone in and unlock it. If it asks "Allow USB debugging?", do
   not answer yet: run `.\limo-phone-dev check` first. Look for `The phone
   SERIAL has not allowed this PC yet. Unlock it and tap Allow on the "Allow
   USB debugging?" prompt.` and exit code 2. Then tap Allow on the phone.
2. `.\limo-phone-dev check`. Look for: `adb:` with its version and path;
   `Phone:` and the serial; `Android` with its version, API level, maker
   and model; `LiMo's child form: not installed.`; `The test family on this
   PC: none.`; `Next: install --apk PATH, with the debug child form.` Exit
   code 0.
3. If an emulator or a second phone is at hand, attach it too and run
   `check`: `More than one phone or emulator is attached (...)`, both
   listed. Detach it again.

Undo: nothing to undo.

## Part C: installing the debug child form

1. The refusals, with nothing sent to the phone (no `> adb` line at all),
   if the release builds are there from an earlier build:
   `.\limo-phone-dev install --apk ..\..\..\app\out\limo-child-release.apk`
   says it is LiMo's child form but a release build, and
   `--apk ..\..\..\app\out\limo-guardian-release.apk` says it is the
   guardian form. Both: `Nothing was installed.`, exit code 2.
2. **Waits for** `DEBUG.apk`. `.\limo-phone-dev install --apk DEBUG.apk`:
   `The APK is LiMo's child form, version ..., a debug build.`, then the
   install; the phone may ask about it on its screen. Look for `Installed
   LiMo's child form on the phone.` If Android refuses, the tool says its
   words and what to do; copy them exactly. A refusal for an install over
   USB is the phone's own setting, which a person may change on the phone.
3. `.\limo-phone-dev check`: `LiMo's child form: installed, version ...,
   a debug build: run-as works, so this tool can reach LiMo's files.` This
   is the first sign that `run-as` works on this phone. Then `LiMo's state
   on the phone: none yet, so no family is on it.` and `Next: setup ...`.
   If it says instead that Android refused `run-as`, copy the line: the
   rest of this page cannot be done on that phone.

Undo: `.\limo-phone-dev uninstall`.

## Part D: a test family, with a short day and one blocked app

1. **Waits for** the host's debug receiver.
   `.\limo-phone-dev setup --child Asha --minutes 10 --block BLOCKED`.
   Type the test PIN twice; nothing shows as it is typed. Look for, in
   order: `The phone's clock is in UTC+...: the test family's days and
   bedtime follow it.` (the phone's own zone); `Made the test family on
   this PC: Asha, 10 minutes a day, the away share at 100 percent ...`;
   `Blocked for Asha: BLOCKED.`; `Wrote the pairing file into LiMo's own
   files on the phone (N bytes, through adb's standard input).`; `Asked
   LiMo to take it. Android: Broadcast completed: result=0`; and within 15
   seconds `LiMo took it: its state on the phone now holds Asha's test
   phone.` Exit code 0.
   - Without the receiver, after 15 seconds: `LiMo did not take the test
     family within 15 seconds ...` and `The pairing file was removed from
     the phone, and the test family on this PC deleted again, so setup can
     run again.` Exit code 1. That is expected until the receiver is built.
   - If it says `the pairing file arrived with N bytes of M`, or `run-as`
     or `cat` refused, copy it exactly: it is whether standard input
     reaches `run-as` on this phone, which only a real phone can show.
2. The same command again: `A test family is already on this PC: Asha, 10
   minutes a day, with its test phone paired. setup makes one only once.`
   and what `forget` does. No PIN asked for, no `> adb` line. Exit code 2.
3. `.\limo-phone-dev check`: `LiMo's state on the phone: it holds this PC's
   test family, Asha's test phone.`
4. `.\limo-phone-dev status`: `Child: Asha (this PC's test family), on its
   test phone.`, `Time left today: 10 min (held on the phone: there is no
   master to reach).`, `Used today: 0 min used of 10 min. On this phone 0
   min.`, `Locked: no.`, `Wrong tries of a code or the PIN: none.`
5. `type $env:LOCALAPPDATA\LiMo-phone-dev-TEST-ONLY\phone-dev.log`: a line
   `setup made a test family with one child, 10 minutes a day, 1 blocked
   and 0 always allowed apps, and paired one phone.`, with no name and no
   PIN in it.

Undo: part L, or `.\limo-phone-dev uninstall` and
`.\limo-phone-dev forget --this-pc-only`.

## Part E: LiMo on the phone, and what it needs

**Waits for** the host and the Kotlin side of the screens' seam: until
then LiMo shows "not set up".

1. Open LiMo on the phone. Its setup asks for six things, in this order:
   notifications, phone, usage access, accessibility, battery, protection.
   Give each. Write down any that the phone's Android would not turn on.
2. `.\limo-phone-dev status`: `Nothing LiMo needs was switched off when it
   last looked.` Switch accessibility off in Settings, open an app for a
   few seconds, and `status` again: `Switched off on the phone when LiMo
   last looked: accessibility.` Switch it on again.

Undo: nothing; part F goes on from here.

## Part F: counting, and the time left in the notification

**Waits for** the host (the engine's thread and the service with its
notification).

1. Pull down the notification shade: LiMo's notification says `10 min
   left`.
2. Use an app that is not blocked (a browser) for 3 minutes. The
   notification counts down. `.\limo-phone-dev status`: `Used today: 3 min
   used of 10 min`, and under `Apps today:` the browser's package with `3
   min`.
3. Turn the screen off for 2 minutes, then on, and leave Android's own lock
   screen up for a minute: neither counts. The home screen does not count
   either. `status` says the same 3 minutes (give or take seconds).
4. In the log terminal: lines of the tags `LiMo` and `LiMoWatch` only. Look
   that none holds an app's name as shown under its icon, a PIN, a code or
   anything typed.

Undo: nothing; part G goes on from here.

## Part G: the blocked app

**Waits for** the covers.

1. Open `BLOCKED`. Within a second LiMo covers it, naming it by its name
   under its icon ("... is not allowed." and "It is blocked by your family's
   rules."), and goes to the home screen. Open it again: the same.
2. `.\limo-phone-dev status`: `BLOCKED` is not under `Apps today`, and the
   time used did not grow while it was in front.

Undo: nothing; part H goes on from here.

## Part H: the day running out

**Waits for** the covers (the warnings and the lock).

1. Use the browser until the time runs out. At 5 minutes and at 1 minute a
   warning floats over it: `5 minutes of screen time left today.` and
   `Save what you are doing.`, then `Save what you are doing now.`
2. At zero the lock covers the screen: `Today's time is used up.`, when it
   opens again, and the ways out.
3. `.\limo-phone-dev status`: `Locked: yes, today's time is used up, until
   tomorrow at 00:00.` and `The lock says: Today's time is used up.`
4. Under the lock, the phone app and an emergency call stay reachable.
   Ring the test phone from another phone: the lock steps aside for the
   ringing call. Answer, then press Home during the call: the lock offers
   `Back to the call`. Never dial an emergency number to test this.

Undo: nothing; part I goes on from here, on the lock.

## Part I: a one time code

**Waits for** the ways out.

1. `.\limo-phone-dev code`: `A one time code for Asha's test phone, worth
   15 minutes, for today only:` and the code, as two groups of four digits.
   No `> adb` line: it needs no phone.
2. On the lock, `Type a one time code`, and type it: `15 minutes added.`
   and the lock goes. `status`: about 15 minutes left.
3. When the 15 minutes are used and the lock is back, type the same code
   again: `That code was already used today.`
4. The tool's log: `code made a one time code of 15 minutes.`, and no
   digits of the code.

Undo: nothing; part J goes on from here, on the lock.

## Part J: the PIN, wrong tries and a request

**Waits for** the ways out.

1. On the lock, `Guardian PIN`, and a wrong PIN: `That PIN is not right. 4
   more tries before a wait.` `.\limo-phone-dev status`: `Wrong tries of a
   code or the PIN since the last right PIN: 1.`
2. The test PIN, adding 15 minutes: `15 minutes added.` and the lock goes.
   `status`: `Wrong tries of a code or the PIN: none.`
3. When the lock is back, `Ask for more time`, 30 minutes, with a reason
   typed. `status`: `A request waits for an answer: 30 minutes, asked today
   at ...`, and the reason nowhere in what it prints.

Undo: nothing; part K goes on from here.

## Part K: a restart

**Waits for** the host (the restart).

1. With some time used and the phone open, restart the phone and unlock
   it. LiMo's notification comes back by itself. `status`: the time used is
   what it was before the restart, give or take a few seconds.
2. With the phone locked by LiMo (the time used up), restart it again: at
   the first unlock the lock is back at once.
3. Settings, Apps, LiMo, Force stop: write down what happens, and when LiMo
   is back.

Undo: nothing; part L goes on from here.

## Part L: forget and uninstall

1. **Waits for** the host's debug receiver for the wipe.
   `.\limo-phone-dev forget`: `Asked LiMo to wipe the test family. Android:
   Broadcast completed: result=0`, `LiMo wiped it: its kept state is gone
   from the phone.`, `Deleted the test family on this PC.` Exit code 0.
   Without the receiver: `LiMo did not wipe its state within 15 seconds.
   The test family on this PC was kept, so forget can run again.` Then do
   step 3 and run `forget` again: it finds LiMo gone and deletes the PC's
   side.
2. `.\limo-phone-dev check`: `LiMo's state on the phone: none yet, so no
   family is on it.` and `The test family on this PC: none.`
3. `.\limo-phone-dev uninstall`: `Removed LiMo from the phone, with
   everything it kept.` If LiMo's protection was given in part E, Android
   refuses while LiMo is a device admin: switch it off on the phone
   (Settings, Security, Device admin apps; the place differs by maker), then
   run `uninstall` again. The tool never switches it off.
4. `.\limo-phone-dev check`: `LiMo's child form: not installed.`

Undo: this part is the undoing.

## Finish

1. `.\limo-phone-dev uninstall` and `.\limo-phone-dev forget
   --this-pc-only`: both say there is nothing left, and finish.
2. On the phone, by hand, if they were turned on for this: Settings,
   Developer options, Revoke USB debugging authorisations, then USB
   debugging off, then developer options off.
3. The folder `%LOCALAPPDATA%\LiMo-phone-dev-TEST-ONLY` now holds only the
   tool's log, and may be deleted.

## Results

| Check | Expected | Seen |
| --- | --- | --- |
| A1 to A4 | as written, exit codes 0, 2, 2, 2 | |
| B1 phone not allowed yet | the "has not allowed this PC" message, exit code 2 | |
| B2 check: adb, phone, Android, not installed | yes; note the Android version and model | |
| B3 two attached | both listed, refused | |
| C1 release and guardian builds refused, no adb line | yes | |
| C2 debug build installed | yes; copy any refusal | |
| C3 check: a debug build, run-as works | yes | |
| D1 setup: the zone, the family, written N bytes, broadcast, taken | yes; note N and how long it took | |
| D1 without the receiver: undone on both sides | until the receiver is built | |
| D2 second setup refused, nothing sent | yes | |
| D3 check: this PC's test family | yes | |
| D4 status: Asha, 10 min, 0 used, not locked, no wrong tries | yes | |
| D5 log line with no name and no PIN | yes | |
| E1 six permissions in order | note any the phone would not turn on | |
| E2 status: nothing off; accessibility off said | yes | |
| F1 notification `10 min left` | yes | |
| F2 counted and each app's minutes in status | yes | |
| F3 screen off, Android's lock screen and home not counted | yes | |
| F4 log: only LiMo's tags, no names, PIN, code or typed text | yes | |
| G1 blocked app covered, named, sent home | yes; note how fast | |
| G2 blocked app not counted | yes | |
| H1 warnings at 5 and 1 minutes | yes | |
| H2 the lock at zero | yes | |
| H3 status: locked, why, until when | yes | |
| H4 phone and emergency call reachable; ringing call not covered; back to the call | yes | |
| I1 code printed, no adb line | yes | |
| I2 code adds 15 minutes | yes | |
| I3 the same code refused | yes | |
| I4 log: no digits | yes | |
| J1 wrong PIN, status 1 wrong try | yes | |
| J2 right PIN adds 15 minutes, wrong tries cleared | yes | |
| J3 request waits in status, no reason printed | yes | |
| K1 restart: notification back, same time used | yes | |
| K2 locked before, locked at once after | yes | |
| K3 force stop | note what happens | |
| L1 forget: wiped, PC side deleted | yes | |
| L2 check: nothing on either side | yes | |
| L3 uninstall; device admin refusal if protection was on | yes | |
| L4 check: not installed | yes | |
| Finish: both commands twice, nothing left | yes | |
