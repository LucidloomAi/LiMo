# Testing LiMo on a child's phone

These are the steps for trying LiMo's child form on a real phone, with the
owner there, at a PC with the phone in hand. They go from the safest to the
riskiest; stop at any part, and undo it as its last lines say. Every part
says what to do and what to look for, word for word, and the results go in
the table at the end, with notes under it.

**The first run was done on 20 November 2026**, on a Galaxy S20 Ultra,
Android 13 (One UI 5.1), with no SIM. It went through parts A to L, then
forget (part O, steps 6 and 7). Not run: every check that needs a call
(G5, G6, H6, I3, K6 to K8), B1, B4, F2, J2, K11, L2 and L3 as written,
parts M and N (but N5 and N6, seen with codes in part L), and the rest of
part O. What was seen is in the table at the end, with notes under it; the
steps below have been corrected where the run showed them wrong. Part P,
base protection, was built after the run: none of it has been run.

The whole run takes about four and a half hours with the phone in hand, and
longer if the builds below are made for the first time. A day's minutes run
in real time, so parts J, M and N take as long as the time left they wait
for.

**It need not be one sitting.** Parts A to I, and part P after them, take
about two hours; the only covers in them are the block screen over one
blocked app and the guard over a few of Android's settings pages; the lock
on everything comes in part K. To stop after any part from D on, run `.\limo-phone-dev forget`
(part O, steps 6 and 7), so that the test family's short day does not lock
the phone the next day; LiMo itself may stay installed. Another day, start
again at part D with a new test family, go quickly through setup, and carry
on from the part that was reached.

## What this run is, and is not

- **A debug build** of the child form, installed over the USB cable. The PC
  can reach its files through `run-as`, so anyone with a cable can change
  what it keeps: never give a debug build to a family. It is always signed
  with Android's debug key ([`app/README.md`](app/README.md), "Signing"),
  so on a phone that holds a release build signed with the release key,
  LiMo must be removed before it installs.
- **A test family, put on the phone by the PC tool** `limo-phone-dev`
  ([`devtool/`](devtool/)): one child, Asha, with a short day, one blocked
  app, one always allowed app and a bedtime. The tool writes a pairing file
  into LiMo's own files over the cable and asks LiMo to take it. The
  guardian PIN and the one time codes are this PC's.
- **Base protection, its first part: built, and tried in part P.** None of
  it has run on a phone yet, and the names of the pages it knows are from
  Android's source. Once setup is finished, a guard covers what could
  switch LiMo off or remove it (under a lock, the lock covers it first),
  laid out as the lock, and offers only the guardian PIN, which opens those
  pages for five minutes. Guarded: LiMo's device admin page, by its name,
  however it is reached; the moment Android asks the device admin whether
  it may be switched off; and One UI's accessibility settings, by their
  package.
  Android itself refuses to uninstall LiMo while its device admin is on,
  and its own Settings then greys out Force stop and Clear storage in App
  info (part P notes what One UI does). Not covered, so a child can still:
  - on phones whose accessibility settings sit inside Android's own
    Settings (Android's source, and phones close to it), switch LiMo's
    accessibility service off there, since no page name tells that page
    apart; the same through the accessibility shortcut, if one was ever
    given to LiMo. LiMo then covers nothing, and Today says a guardian is
    needed;
  - restart the phone into safe mode, where LiMo does not run (way back 5);
    only strict mode, not built, could stop that;
  - tap Stop for LiMo in the shade's list of running apps (Android 13 and
    later); LiMo comes back by itself within about fifteen minutes;
  - with USB debugging on and a cable, force stop LiMo or clear its data
    with adb (way back 4).
- **Not covered, because it is not built yet:**
  - **Pairing over the network**, and the link to the master PC. The phone
    never reaches a master: it holds the whole day itself, reports nothing,
    and a request for more time waits all day with no one to answer it.
    The phone's first screen points at "Add a phone" in LiMo on the PC; in
    this run the tool stands in for it.
  - **Strict mode**, and anything of a Device Owner. Neither LiMo nor the
    tool touches the phone's Device Owner.
- **Built, but not tried in this run:** bedtime's lock (the bedtime here is
  far away, and shows only on Today and How it works), TalkBack, a blocked
  app in one half of split screen, a lock while usage access is off, and a
  kept state that cannot be read.

## Every way back, before anything locks

LiMo covers the screen in three ways: the block screen over a blocked app,
the guard over a page of Android's settings that could switch LiMo off or
remove it, and the lock over everything but what stays open. Before any
can show, know the ways back, easiest first:

1. **The guardian PIN, typed on the lock or the guard.** Every lock in this
   run offers `Guardian PIN`. Choose what to give (`15 min`, `30 min` or
   `1 hour` added, or kept open for `1 hour`, `2 hours` or `Today`), type
   the test PIN and tap `Unlock`. The guard offers `Guardian PIN` alone,
   with nothing to choose: the test PIN and `Unlock` open every guarded
   page for five minutes, or until the screen goes off or the phone
   restarts, whichever comes first; it adds no time and opens no lock.
   After five wrong tries of a code or the PIN, on the lock and the guard
   together, the next try must wait a minute (and twice as long after each
   further wrong one); the lock and the guard then say until when.
2. **A one time code from the PC.** `.\limo-phone-dev code` prints one, with
   no cable needed; on the lock, `Type a one time code` and type its 8
   digits. A code works once, only today, and only on a lock that a lack of
   time made: never on bedtime's lock or a guardian's, nor on the guard.
3. **`.\limo-phone-dev forget`, over the cable.** It asks LiMo to wipe its
   kept state. LiMo deletes it and the child's name, takes any cover away
   and stops its notification; with no state kept, LiMo is not paired and
   covers nothing, the guard neither, so the device admin's page is then
   open to anyone. The tool checks within 15 seconds that the state is
   gone. It needs the cable in, USB debugging on and this PC allowed, and
   it refuses, changing nothing, when LiMo's state cannot be read or is
   another family's. Then `.\limo-phone-dev forget --phone-only` wipes it:
   it asks LiMo, through its own debug receiver, to wipe the state it
   keeps, whoever's it is, checks that it is gone, and leaves this PC's
   test family as it is.
4. **A force stop over the cable**, when neither forget can:
   `adb shell am force-stop ai.lucidloom.limo.child`. It is the same force
   stop as the button in Android's settings, after which Android switches
   LiMo's accessibility service off (seen in the second spike,
   `spikes/02-android-base-lock`). Every cover is drawn through that
   service, so nothing is covered until a person switches it on again, the
   device admin's page included: the guard is drawn through it too. Then
   switch off LiMo's device admin and run `.\limo-phone-dev uninstall`
   (part O, step 5, says where). This way back has not been tried on a
   phone yet; part O, step 2, tries it where App info's Force stop is
   greyed out.
5. **Android's safe mode, with no cable and no PC.** In safe mode Android
   runs none of the apps a person installed, LiMo among them, so nothing is
   covered, the guard neither, and LiMo can be removed there: switch its
   device admin off, then uninstall it. On a Galaxy: restart the phone
   (holding the side key and Volume down for some seconds forces a restart
   when nothing else answers), and hold Volume down while the Samsung logo
   shows, until
   `Safe mode` shows in a corner of the screen. A plain restart leaves safe
   mode again. This is Android's own behaviour; it has not been tried with
   LiMo on a phone yet.

A plain restart of the phone is no way back: a lock that was on is on again
at the first unlock.

**Never covered**, lock or no lock: a ringing call; the phone app and the
screen of a call in progress; the emergency dialler; LiMo's own screens;
Android's own lock screen; a screen that is off. Under the lock the apps the
family always allows stay usable, the phone app first among them. During a
call in progress any other app is covered as it would be with no call, and
the cover offers `Back to the call`.

## Before starting

### On the PC

- Android's SDK platform tools, with `adb` on the PATH or `ANDROID_HOME`
  set.
- **The tool.** In `phone\devtool`, `cargo build --release`. Every command
  below is run from `phone\devtool\target\release` as
  `.\limo-phone-dev ...` in PowerShell; the exit code is in
  `$LASTEXITCODE`. Each call it makes to the phone is printed first, as
  `> adb ...`.
- **The debug child form.** In `phone\app`, `npm ci`, then
  `npm run build:child:debug`, which makes `out\limo-child-debug.apk`.
  It needs what [`app/README.md`](app/README.md), "Building", lists (Node
  22, JDK 17, the Android SDK's packages, Rust with the
  `aarch64-linux-android` target). From the tool's folder its path is
  `..\..\..\app\out\limo-child-debug.apk`.
- **Two terminals**, both in the tool's folder: one for the commands, one
  for the log, `.\limo-phone-dev log --minutes 720`, which follows LiMo's
  two tags, `LiMo` and `LiMoWatch`, for up to twelve hours. Restart it
  whenever it stops. Run it in a terminal window: started by another
  program with no terminal, it ended at once in the first run (note 15).
  `adb logcat -d -v time -s LiMo:V LiMoWatch:V` prints what LiMo has said
  so far, and ends.
- **A test PIN** of 6 to 8 digits, written down, used for nothing else.
- **The two clocks** agree to a few minutes: `setup` refuses when the
  phone's and the PC's are 50 minutes or more apart, and says so from 5.

### On the phone

The run is written for a Galaxy S20 Ultra with Android 13 (One UI 5.1), and
what to tap is One UI's; another phone's screens differ. Any phone with
Android 8.0 or later can take LiMo.

- **A test phone**, ideally with nothing personal on it: LiMo asks for
  accessibility and usage access, and its covers stand over the phone's
  apps for most of the run. It reads nothing of them.
- **USB debugging on, by a person:** Settings, About phone, Software
  information, tap Build number seven times; then Settings, Developer
  options, USB debugging. The tool never turns either on. Some phones also
  block installs or commands over USB (Samsung's Auto Blocker on newer One
  UI, "Install via USB" on others): part C says what to do if an install is
  refused.
- **The cable in, the phone unlocked, this PC allowed.** When the phone
  asks "Allow USB debugging?", tick "Always allow from this computer", so
  that it never needs to ask again while a cover is up.
- **Installed and not disabled:** YouTube (`com.google.android.youtube`),
  which part D blocks, and an app to always allow: any app with an icon
  other than YouTube and the browser. The calculator is not on every
  Galaxy; the first run used the Clock (`com.sec.android.app.clockpackage`).
  Below it is called `ALLOWED`, and its name is the one under its icon.
  Part B finds both package names.
- **A browser**, the app whose time is counted in this run (Chrome or
  Samsung Internet).
- **A SIM in this phone, and a second phone** that can ring it, with this
  phone's number. Without a SIM no check with a call can be done (G5, G6,
  H6, I3, K6 to K8), as in the first run.
- **The phone's own screen lock PIN**, for the restarts.
- **The battery** above half, and the cable left in: it charges.
- **Sound mode on and Do not disturb off**, so that calls ring.

### Note before anything changes

Write down, to put back at the end: the font size and the screen zoom
(Settings, Display), Auto rotate, the sound mode, the navigation bar
(gestures or buttons), and whether developer options and USB debugging were
on before the run.

### LiMo's protection, and removing LiMo

LiMo's last setup step asks to make it a **device admin app**. Android
does not uninstall an app while it is one:

- `.\limo-phone-dev uninstall` then refuses, saying where a person switches
  it off, and changes nothing. The tool never switches it off.
- `.\limo-phone-dev forget` (and `forget --phone-only`) wipes LiMo's kept
  state and leaves the device admin as it is.
- A person switches it off on the phone, under Device admin apps (part O,
  step 5). Once setup is finished, the guard covers that page first, and
  the guardian PIN opens it for five minutes. Under a lock, Android's
  settings are covered by the lock, so that is done with the phone open.
  After `forget`, or a force stop, nothing covers the page.

### How this page quotes

The phone's words and the tool's are quoted as they show them, except that
the phone sets each apostrophe as a curly one; `TIME` is a time of day,
`SERIAL` the phone's serial number, `N` a number. Labels and stamps the
phone sets in capitals are quoted in capitals. In the log terminal each
line ends with LiMo's words as quoted here; a line said to *begin* with
some words may have more after them, often `package=` and a package name.
No line LiMo logs may hold an app's name as shown under its icon, the
child's name, a PIN, a code or anything typed: look for that throughout.

## Part A: no phone, nothing changed

The phone unplugged.

1. `.\limo-phone-dev help`: first what it never does (only LiMo's own app
   and files; never developer options or USB debugging; never the phone's
   accounts, its Device Owner or its settings), then the commands, then the
   folder of the test family, `%LOCALAPPDATA%\LiMo-phone-dev-TEST-ONLY`.
   Exit code 0.
2. `.\limo-phone-dev check`: `check reads only; it changes nothing.`, the
   `adb:` line, then what this PC sees on its USB ports: `This PC sees no
   phone on its USB ports.` and `The cable probably only charges: many
   cables carry no data. Use the cable that came with the phone, or one
   known to move files; plug it into a port on the PC itself, not a hub;
   and unlock the phone.`, then `Nothing was changed on the phone.` Exit
   code 2. Where Windows' list of USB devices cannot be read, the first line
   is `This PC's list of USB devices could not be read: ...` and the advice
   the older sentence, `No phone is attached. Plug the phone in with a USB
   cable, unlock it, and check that USB debugging is on (Settings,
   Developer options). This tool never turns it on by itself.` (the only
   one the tool said in the first run, before its cable check).
3. `.\limo-phone-dev code`: `code is scaffolding until the guardian's phone
   app exists.` and `There is no test family on this PC. Run setup first.`
   Exit code 2.
4. `.\limo-phone-dev setup --child Asha --minutes 30 --block youtube`:
   `--block takes an app's package name, such as com.example.game, not
   "youtube".` Exit code 2, no PIN asked for, no `> adb` line.
5. `.\limo-phone-dev install --apk nothing.apk`: `There is no file
   nothing.apk. Build the debug child form first (phone/TESTING.md says
   how). Nothing was installed.` Exit code 2.

Undo: nothing to undo.

## Part B: the phone attached, read only

**When adb sees no phone at all**, every command says what Windows sees on
its USB ports, in one line, then what to do, and changes nothing. `NAME` is
the phone's name as Windows shows it, such as `Galaxy S20 Ultra 5G`:

- `This PC sees no phone on its USB ports.`: `The cable probably only
  charges: many cables carry no data. Use the cable that came with the
  phone, or one known to move files; plug it into a port on the PC itself,
  not a hub; and unlock the phone.`
- `This PC sees what looks like a phone on its USB ports (NAME), but adb
  does not.`: `If that is the phone, its cable carries data, but USB
  debugging is off. Switch it on, on the phone (Settings, Developer
  options); this tool never turns it on by itself. If it is on already,
  unplug the phone and plug it in again, unlocked. If it is something else
  that offers pictures or files, such as a camera, the phone's own cable
  probably only charges: use the one that came with the phone.`
- `This PC sees the debugging interface of a phone (NAME), but has no
  working driver for it.`: `The phone maker's USB driver has to be
  installed on this PC before adb can reach the phone. Install it, then
  unplug the phone and plug it in again. This tool installs nothing.`
- `This PC sees the debugging interface of a phone (NAME), with its driver
  working, but adb does not list it.`: `Unplug the phone and plug it in
  again, unlocked; if adb still lists nothing, run adb kill-server, then
  this again.`

When adb lists the phone in any state, Windows' list is not read, and adb's
own answer is said, as in step 1.

1. Plug the phone in and unlock it. If it asks "Allow USB debugging?", do
   not answer yet: run `.\limo-phone-dev check`. Look for `The phone SERIAL
   has not allowed this PC yet. Unlock it and tap Allow on the "Allow USB
   debugging?" prompt. If no prompt shows, unplug the phone and plug it in
   again.` and exit code 2. Then tick "Always allow from this computer" and
   tap Allow.
2. `.\limo-phone-dev check`. Look for: the `adb:` line with its version and
   path; `Phone: SERIAL`; `Android` with its version, API level, maker and
   model; `LiMo's child form: not installed.`; `The test family on this PC:
   none.`; `Next: install --apk PATH, with the debug child form.` Exit
   code 0.
3. The package names, read only: `adb shell pm list packages youtube`, and
   the same for the app to always allow (`clock` finds the Clock,
   `com.sec.android.app.clockpackage`; `calc` a calculator, where there is
   one). Write it down (below called `ALLOWED`). Do the same for the
   browser (`chrome` or `sbrowser`).
4. If an emulator or a second phone is at hand, attach it too and run
   `check`: `More than one phone or emulator is attached (...). Unplug the
   others or close the emulator, so that this tool cannot pick the wrong
   one.`, both listed. Detach it again.

Undo: nothing to undo here; the Finish revokes this PC's permission.

## Part C: installing the debug child form

1. The refusals, with nothing sent to the phone (no `> adb` line at all),
   if the release builds are in `phone\app\out` from an earlier build:
   - `.\limo-phone-dev install --apk ..\..\..\app\out\limo-child-release.apk`:
     `This APK is LiMo's child form, version ..., but a release build. A
     test family can go only on a debug build, because Android refuses
     run-as for a release build. Build the debug child form. Nothing was
     installed.`
   - `.\limo-phone-dev install --apk ..\..\..\app\out\limo-guardian-release.apk`:
     `This APK is LiMo's guardian form (ai.lucidloom.limo), not the child
     form. This tool installs only the child form (ai.lucidloom.limo.child).
     Nothing was installed.`

   Both exit code 2.
2. `.\limo-phone-dev install --apk ..\..\..\app\out\limo-child-debug.apk`:
   `The APK is LiMo's child form, version ..., a debug build.`, then
   `Installing; the phone may ask about it on its screen.` Look at the
   phone. Then `Installed LiMo's child form on the phone.` and `Next: check,
   then setup --child NAME --minutes N.` Exit code 0. If Android refuses,
   the tool says its words (`Android refused the install: ...`) and what to
   do: copy both exactly. A refusal of an install over USB is the phone's
   own setting, which a person may change on the phone.
3. `.\limo-phone-dev check`: `LiMo's child form: installed, version ..., a
   debug build: run-as works, so this tool can reach LiMo's files.` (the
   first sign that `run-as` works on this phone), `LiMo's state on the
   phone: none yet, so no family is on it.`, and `Next: setup --child NAME
   --minutes N.` If it says instead that Android refused `run-as`, copy the
   line: the rest of this page cannot be done on that phone.
4. On the phone, open LiMo from its icon (the hourglass). It shows S0,
   LiMo's name at the top and its card in the middle of the screen:
   `This phone is not set up yet.` and `Connect it to the family PC with a
   USB cable, then choose Add a phone in LiMo on the PC.`, with nothing to
   tap. Compare it with S0 in `design/screens/phone-setup.html`. The log:
   `host not_paired waiting`. Leave S0 on the screen for part D.

Undo: `.\limo-phone-dev uninstall`.

## Part D: a test family

Pick a bedtime that starts at least four and a half hours after the run
begins and has ended before it begins: `23:00-06:00` suits a run that starts
between 06:00 and 18:30. The run must end before midnight, when a new day
starts.

1. With S0 on the phone's screen:
   `.\limo-phone-dev setup --child Asha --minutes 30 --block com.google.android.youtube --allow ALLOWED --bedtime 23:00-06:00`.
   Look for, in order: `setup is scaffolding until pairing over the network
   exists.`; the `adb:`, `Phone:` and `Android` lines; `The phone's clock is
   in UTC+...: the test family's days and bedtime follow it.` (the phone's
   own zone). Then it asks `Guardian PIN (6 to 8 digits, not shown): ` and
   `The same PIN again: `: type the test PIN twice; nothing shows as it is
   typed. Then `Made the test family on this PC: Asha, 30 minutes a day, the
   away share at 100 percent (a phone with no master has its whole day), and
   the PIN set.`; `Blocked for Asha: com.google.android.youtube.`; `Always
   allowed, also under a lock: ALLOWED.`; `Screens off every day from
   23:00 to 06:00.`; `Paired one phone with it: the test phone.`; `Wrote
   the pairing file into LiMo's own files on the phone (N bytes, through
   adb's standard input).`; `Asked LiMo to take it. Android: Broadcast
   completed: result=0`; then, as it reads LiMo's state back, once or
   more, the call
   `> adb -s SERIAL exec-out "run-as ai.lucidloom.limo.child sh -c 'if test
   -e files/limo/state; then echo limo-dev-state; cat files/limo/state; else
   echo limo-dev-none; fi'"` (through `exec-out`, which hands the state's
   bytes over as they are: note 1); and within 15 seconds `LiMo took it:
   its state on the phone now holds Asha's test phone.` and the `Next:`
   line. Exit code 0. Note N and how long it took.
   - If it says `LiMo says it took the test family (its log says: dev pair
     taken), but this tool could not read its state back to check that it
     holds the test phone.`, then `The test family on this PC is kept, so
     that this PC and the phone still match. Run check, then status, to see
     what the phone holds. To start again, run forget: it wipes the test
     family from the phone and from this PC.` Exit code 1. Copy both lines.
   - If LiMo refused the file: `LiMo did not take the test family. Its log
     says: dev pair refused why=WORD` and what to do. Copy both. Then `The
     pairing file was removed from the phone, and the test family on this
     PC deleted again, so setup can run again.` Exit code 1.
   - If it says `LiMo did not take the test family within 15 seconds, and
     its log says nothing of it.`, look in the log terminal: a `dev pair`
     line there, when the tool found none, means the phone's `logcat` does
     not take a time given in unix seconds. Copy what the log shows.
   - If it refuses with `LiMo on this phone already holds ...` (a family
     left by an earlier try), run `.\limo-phone-dev forget --phone-only`,
     which wipes the state LiMo keeps on the phone and nothing else, then
     setup again.
2. On the phone, without a tap, S0 gives way to S1: `FOR A GUARDIAN`,
   `This phone is now Asha's.`, `Six short steps finish the setup. Each one
   asks Android for one thing, and says why.`, and `Start` pinned. If it
   says `This phone is now your child's.`, the name was not kept: the log
   then says `host paired no_child_name` and `seam pairing no_child_name
   fallback`.
3. The log: `dev pair taken` and `host paired`, and no `host failed` line
   (`host failed what=engine_load error=UnsatisfiedLinkError` would mean
   the engine's library did not load). (Settles whether the Rust engine
   loads and runs on this phone.)
4. The same setup command again: `A test family is already on this PC:
   Asha, 30 minutes a day, with its test phone paired. setup makes one only
   once.`, what `forget` does, and `Nothing was changed.` No PIN asked for,
   no `> adb` line. Exit code 2.
5. `.\limo-phone-dev check`: `LiMo's state on the phone: it holds this PC's
   test family, Asha's test phone.`, `The test family on this PC: Asha, 30
   minutes a day, with its test phone paired.` and `Next: status, log or
   code; or forget, to start again.`
6. `.\limo-phone-dev status`, at once: `Child: Asha (this PC's test family),
   on its test phone.`, `Setup is not finished on this phone. LiMo counts
   nothing and covers nothing until a guardian finishes setup on the
   phone.`, `Time left today: 30 min (held on the phone: there
   is no master to reach).`, `Used today: 0 min used of 30 min. On this
   phone 0 min.`, `Apps today: none used for a minute or more.`, `Screens
   are off from 23:00 to 06:00.`, `Locked: no.`, `Wrong tries of a code or
   the PIN: none.`, and a line naming what LiMo found switched off, all six
   at this point: `Switched off on the phone when LiMo last looked:
   notifications, phone, usage access, accessibility, battery, protection.`
7. `type $env:LOCALAPPDATA\LiMo-phone-dev-TEST-ONLY\phone-dev.log`: a line
   for the install, and `setup made a test family with one child, 30
   minutes a day, 1 blocked and 1 always allowed apps, screens off at night,
   and paired one phone.`, each with a time in UTC, and no name and no PIN.

From here until setup is finished on the phone (part F, step 1, `Show Asha
how it works`), LiMo counts nothing and covers nothing, whatever is in
front: Android's own pages, LiMo's own screens and any app tried meanwhile.
Setup is the guardian's time. LiMo's notification says `Setup is not
finished on this phone.` in place of the time left, and `status` says so
first.

Undo: `.\limo-phone-dev forget`, then `.\limo-phone-dev uninstall`.

## Part E: setup on the phone

Each step is a screen of its own, drawn as S3 to S8 in
`design/screens/phone-setup.html`: `STEP 1 OF 6` and six dots, the step's
name, `WHY`, `WHAT HAPPENS`, and a stamp `NOT ON YET` in coral, with the
step's own button pinned at the bottom. Once Android says it is on, the
stamp turns to `ON` in teal, with `Android says it is on.`, and the button
to `Next`. A step is never taken on trust: it shows ON only when Android
says so.

1. **Welcome, and agreeing.** Tap `Start`. S2: `BEFORE YOU START`, `What
   LiMo does here`, three rows (`Counts time`, `Covers and locks`, `Never
   reads`), `Nothing leaves your home. This phone talks only to your family
   PC.`, and the box `I am Asha's guardian, and I agree.` `Continue` is
   switched off until the box is ticked. Tick the box, tap `Continue`.
   (Whether LiMo's own screens are left uncounted while usage access is off
   is settled in part I, step 1: during setup nothing is counted at all.)
2. **Kept.** On the first step, swipe LiMo away from the recent apps and
   open it again: it starts at the first step not on (Notifications), not
   at S1.
3. **Notifications** (step 1). `Allow notifications`: Android's own question
   over LiMo, "Allow LiMo to send you notifications?", with Allow and Don't
   allow. Tap Allow: ON. The log: `seam step=notifications asked
   granted=true`. Pull down the shade: LiMo's notification is there,
   ongoing, its title `Setup is not finished on this phone.`, with no
   countdown.
4. **Phone** (step 2), and **going on without it**, tried here once.
   Its note, `WHAT ANDROID ASKS`: `Android's words are "make and manage
   phone calls". LiMo never makes a call. It only learns that one is
   ringing or on.` Tap `Allow`: Android's question, "Allow LiMo to make and
   manage phone calls?". Tap **Don't allow**: still NOT ON YET (`seam
   step=phone asked granted=false`). Tap `Allow` again and **Don't allow**
   again (if Android does not ask the second time, LiMo opens its App info
   page instead: press Back). Now, under the stamp: `Android has not turned
   this on. You can go on and come back to it from Today.`, and an
   outlined `Go on without it` inside the card; the pinned `Allow` stays.
   Nothing above the line moved. Tap `Go on without it`: step 3. The Phone
   permission stays off until part I.
5. **Usage access** (step 3). `Open Android's settings`: on Android 13
   LiMo's own usage page should open, with one switch ("Allow usage
   tracking" on One UI): the log says `seam step=usage opened
   page=usage_for_limo`. If it says `page=usage_list`, One UI took only the
   list (Settings, Apps, Special access, Usage data access): find LiMo
   there. Switch it on, press Back: ON. (Settles which page One UI opens.)
6. **Accessibility** (step 4). `Open Android's settings`: Android's
   accessibility settings (`seam step=accessibility opened
   page=accessibility_list`), with nothing over them: the guard covers One
   UI's accessibility settings only once setup is finished (part P). Note
   whether LiMo's row is scrolled to and lit up, or One UI shows its top
   page (then Installed apps, LiMo). LiMo's page in Settings says `LiMo is told which app is in front, by its name
   only, to count screen time and to cover an app when it is blocked or the
   time is used up. It is never told what is on the screen.` Switch it on
   and allow the warning Android shows; Back twice: ON. If the switch is
   greyed out as a restricted setting, note it, then: App info, the menu at
   the top right, Allow restricted settings (an install by cable is not
   expected to need it). (Settles whether One UI takes the hint to light up
   LiMo's row.)
7. **Battery** (step 5). `Allow`: Android's question over LiMo, "Let app
   always run in background?". Allow: ON at once (`seam step=battery
   answered`). On a Samsung the note `ON A SAMSUNG PHONE` shows under it:
   `Also open Settings, then Battery (on some phones Battery and device
   care, then Battery), then Background usage limits, then Never sleeping
   apps, and add LiMo there.` Do so, and note whether the phone's Settings
   have that path, or another.
8. **Protection** (step 6). Its why: `So a guardian's PIN is needed to
   switch this off, and LiMo cannot be removed while it is on.` Android
   keeps the second half; the guard, which stands only once setup is
   finished, keeps the first (part P). `Turn on protection`: Android's page
   "Activate device admin app?", with nothing over it, and LiMo's words
   exactly: `LiMo asks for this only so that it cannot be removed from this
   phone while this is on. It uses no other power of a device admin app: it
   cannot wipe the phone, lock it, change its screen lock or switch off its
   camera. To remove LiMo, switch this off first.`, and no list of powers
   (or an empty one). Activate: ON (`seam step=protection answered`).
9. **Done.** S9: `This phone is set up.` and six rows: a teal tick for each
   step Android says is on (all but Phone), and on Phone's row a ring in the
   tick's place and `NOT ON` at its end. Under the rows: `Phone is not on
   yet. Asha's Today screen shows a guardian how to turn it on.`; then
   `Hand the phone to Asha. The next three screens are Asha's.`, and `Show
   Asha how it works`.
10. No step logged `seam step=... not_opened` (which shows `Android did not
    open it. Try again.`). If one did, note which.
11. `.\limo-phone-dev status`: `Setup is not finished on this phone. LiMo
    counts nothing and covers nothing until a guardian finishes setup on the
    phone.`, `Used today: 0 min used of 30 min. On this phone 0 min.`, `Apps
    today: none used for a minute or more.`, and `Switched off on the phone
    when LiMo last looked: phone.` Nothing of setup is counted, Android's
    own pages included.

Undo: part O, steps 5 to 9 (until setup is finished nothing is guarded).

## Part F: the child's introduction, Today and How it works

Set each screen beside its drawing: C1 to C3, H and T in
`design/screens/phone-setup.html`, and Today, frame E, in
`design/screens/phone.html`. Look for the same layout, colours, borders,
hard shadows, dashed dividers, stamps, icons and type (Fraunces, Instrument
Sans and Spline Sans Mono at their weights), nothing cut and nothing under
Android's bars. Note what differs, with a photo. Two differences are
chosen: the tab bar is solid where the drawing is glass, and app tiles on
Today show the app's first letter, as drawn.

1. Tap `Show Asha how it works`. C1: `FOR ASHA`, `What LiMo counts`, `The
   time your screen is on with an app open. Calls, the home screen and LiMo
   itself do not count.`, `1 OF 3`, `Next`. C2: `What LiMo never looks at`,
   `Your messages, what you type, and the pages you visit. It only knows
   which app is open, and for how long.` C3: `How to ask for more time`,
   `When the time is used up, tap Ask for more time. A guardian answers. A
   guardian who is away can read you a code to type.`, `Got it`. Setup is
   finished as the button is tapped: the log says `seam setup finished` and
   `host setup finished`; the notification's title turns to the time left
   (`30 min paused` while LiMo is in front); `.\limo-phone-dev status`:
   `Setup was finished on the phone today at HH:MM.`
2. `Got it` shows Today, under the floating tab bar (`TODAY`, `HOW IT
   WORKS`). Swipe LiMo away and open it: Today at once.
3. **Today with the banner** (T): `NEEDS A GUARDIAN` in coral, `Phone
   access is switched off, so a lock could hide a call.`, and `Show a
   guardian what to do` (do not tap it yet). Under it, the time left large
   (`N min`) with `LEFT TODAY`, `Looking at this screen does not use your
   time.`, the bar, `N min used of 30 min`, `ON THIS PHONE N MIN`, the moon
   with `Screens are off from 11:00 pm to 6:00 am.` (or `23:00 to 06:00`
   on a phone set to 24 hours), and `Ask for more time`. `TODAY ON THIS
   PHONE` lists each app used for a minute or more, by its own name, with
   its minutes.
4. With Today in front, LiMo's notification says `N min paused`: looking at
   LiMo uses no time.
5. **How it works** (H), the second tab: `YOUR RULES` with `TIME A DAY`
   `30 min`, `SCREENS OFF` `11:00 pm to 6:00 am`, `ALWAYS OPEN` with
   `ALLOWED`'s name, `NOT ALLOWED` `YouTube`; `WHAT LIMO DOES`, the three
   rows of C1 to C3; `ABOUT` with `LiMo 0.1.0` (the version of the build)
   and `More info on https://lucidloom.ai`, the address as plain text,
   which opens nothing. Android's Back goes to Today; from Today it leaves
   LiMo.
6. **Asking from Today.** Tap `Ask for more time`: LiMo's own screen of the
   ways out opens at asking (B1 in `phone.html`), with no strip at the top
   since no lock is behind it: `HOW LONG`, four chips (`15 min`, `30 min`,
   `45 min`, `1 hour`), `WHY` with `you can leave this empty`, a counter
   `0 / 200`, and `Send request` pinned above the keyboard. Send nothing:
   press Back, which comes back to Today.

Undo: nothing; part G goes on from here.

## Part G: the notification, and what counts

`status` says the time used; LiMo's notification says the time left, with
Android's own countdown between the minutes while time is counted.

1. **An app that counts.** Open the browser and use it for 3 minutes. The
   log: a line beginning `front now app package=` with the browser's
   package. The notification's title says `N min left`, its words change
   once a minute, and the countdown moves between. `status`: the minutes
   used grew by 3, and under `Apps today:` the browser's package with `3
   min` (once it has a minute, Today lists it by its own name).
2. **What stays the browser.** Over the browser, pull down the shade, open
   the Edge panel, take a screenshot (its toolbar shows) and tap the
   browser's search field (the keyboard comes up): no new `front now` line
   for any of them, and the count goes on. An overlay may log a line
   beginning `front window_event package=` with its own package and ending
   `not_confirmed front=` and the browser's package: that is the watch
   leaving it out. (Settles the watch's question whether One UI's overlays
   change the app in front.)
3. **What does not count.** Open the recent apps: the log line begins
   `front now home` (One UI's list is the launcher's), and the notification
   says `paused`. A minute on the home screen; then the screen off for 2
   minutes; then on, with Android's own lock screen left up for a minute:
   none of it counts. `status`: the same minutes used, give or take
   seconds.
4. **An app always allowed.** Use `ALLOWED` for a minute: the time left
   does not go down. `status`: its package under `Apps today:`
   with `1 min`, and the same time left. Note what the notification says.
5. **A call, ringer on, without the Phone permission.** With the browser in
   front, ring this phone from the second one. The log: `call now ringing`;
   nothing counts while it rings. Answer: `call now ongoing`, and the call's
   screen reads as the dialler (a line beginning `front now dialler`,
   perhaps after one beginning `front learned call_screen package=`):
   note which. It does not count. During the call, go back to the browser
   for a minute: that minute counts (a call is no way around the limits).
   End the call. `status`: the browser's minute is there; the ringing and
   the call's screen are not.
6. **A call made.** From the Phone app, call the second phone: the call's
   screen reads as the dialler again; note whether a `front learned
   call_screen` line came, or One UI's call screen was known before any
   call. End it. (Settles the watch's question about One UI's call screen.)
7. **The log so far** holds lines of the tags `LiMo` and `LiMoWatch` only,
   and none holds an app's name as shown under its icon, the child's name,
   a PIN, a code or anything typed.

Undo: nothing; part H goes on from here.

## Part H: the blocked app, fourteen times

YouTube is blocked because it sends one more window event after the home
screen comes up, which in the second spike brought the cover back over the
home screen in 12 of 14 tries. These steps settle whether that is gone, and
how long LiMo takes to cover an app.

1. **The block screen** (D in `phone.html`). Open YouTube. Within about a
   second it is covered, its status and navigation bars too, nothing of
   YouTube showing: LiMo and the clock at the top; a card with the block
   picture, the stamp `BLOCKED`, `ASHA · THIS PHONE`, `YouTube is not
   allowed.` and `It is blocked by your family's rules.`; one ink button
   `Go to the home screen`; `Nothing else was closed.`; `MADE BY`, the
   LucidLoom AI logo and the name (only words there means the logo was not
   found); and `Emergency call` pinned at the bottom. The log: a line
   beginning `front now app package=com.google.android.youtube`, then `show
   cover now block package=com.google.android.youtube call=false` and `show
   cover drawn ms=N`, and once today a `note` line with `blocked_app_opened`
   and the package.
2. **Going home.** Tap `Go to the home screen`: the home screen, the cover
   gone (`show home by=service`, `show cover now nothing`), and it does not
   come back over the home screen. Look for a line beginning `front now
   home`, and, when YouTube's late window event comes, one beginning `front
   window_event package=com.google.android.youtube not_confirmed`.
3. **Fourteen times.** Do steps 1 and 2 until there are fourteen tries in
   all. Write down each `show cover drawn ms=` value (or their range), and
   whether the home screen was ever covered again. If a `not_confirmed` line
   ever names the app that really came up, write it down: Android's usage
   record came later than a second on this phone.
4. **The emergency call from the block screen.** On the block screen, tap
   `Emergency call`. Android's emergency dialler opens at once, and nothing
   covers it. **Do not place an emergency call, and type no number**: the
   button only opens the dialler, and dials nothing by itself. The log:
   `show emergency opened how=emergency_dialler` (or `how=dial_emergency`,
   or `how=dial`: note which), then a line beginning `front now
   emergency_dialler`. On the Galaxy it is `front now emergency_dialler
   package=com.samsung.android.app.telephonyui`, known from Android's
   roles, with no `front learned` line; on a phone whose emergency dialler
   Android does not name, a line beginning `front learned
   emergency_dialler package=` comes first. Since the watch's fix after the
   first run (note 2), an emergency dialler Android names also logs a line
   beginning `front emergency_wait ended first=` with its package, and
   nothing opened after it (the home screen, Settings, its search) may log
   a line beginning `front learned`. Press Back, and Back again to leave
   YouTube.
5. `status`: YouTube is not under `Apps today:`, and the time used did not
   grow while it was covered.
6. **A silent ring without the Phone permission.** Set the sound mode to
   Mute, with vibration off. Open YouTube, so that the block screen is up,
   and leave it. Ring this phone from the second one. Watch the phone and
   the log: does the incoming call show (full screen, or a pop up over the
   block screen), or is it hidden under the block screen? Does `call now
   ringing` come, or nothing until it is answered? End it from the second
   phone, and write down what was seen. (Settles the watch's question what
   a silent ring does without the Phone permission: a full screen call is
   never covered, a pop up may be.)

Undo: nothing; part I goes on from here, with the sound mode still on
Mute.

## Part I: putting things right from Today

1. **A change while Today shows.** Open LiMo's Today, and put it in split
   screen with Settings (on One UI: the recent apps, LiMo's icon, Open in
   split screen view, then Settings in the other half). In Settings, Apps,
   Special access, Usage data access, LiMo: switch it off. Within a second
   or so Today's banner still says the Phone sentence first, and under it
   `AND 1 MORE`. The log: a line beginning `front now unknown`, and a
   `note` line with `permission_off` and the usage access. Leave usage
   access off and close split screen, so that Today fills the screen: a
   line beginning `front now limo`, since LiMo's own screen says so itself
   and is not said to be in front while it shares the screen. Run
   `.\limo-phone-dev status` and note `Used today`; stay a full minute on
   Today, usage access still off, and run it again: `Used today` has not
   moved. Tap `Ask for more time`: the ways out screen opens and nothing
   covers it; Back shows Today again. If split screen will not work, switch
   usage access off in Settings and come back to Today: note which was
   done. (Settles whether LiMo's screen is known without usage access, so
   that it is left uncounted, and whether leaving split screen is noticed.)
2. **The banner's button.** On Today, tap `Show a guardian what to do`: the
   Phone step first. Have the second phone ready to call. Its `Allow` now
   opens LiMo's App info page (`seam step=phone opened page=app_details`),
   since Android was told no twice: Permissions, Phone, Allow.
3. **The Phone permission while LiMo runs.** At once, within ten seconds of
   allowing it, ring this phone from the second one (the sound mode is
   still Mute): the log should still say `call now ringing`. End it.
   (Settles the watch's question whether a permission granted while LiMo
   runs takes effect for calls.)
4. Back in LiMo: the Phone step ON, `Next`: the Usage access step, NOT ON
   YET. Switch it on as in part E, step 5: ON, `Next`: back to Today, with
   no banner.
5. Set the sound mode back to Sound. `status`: `Nothing LiMo needs was
   switched off when it last looked.`

Undo: nothing; part P goes on from here.

## Part P: base protection, the guard

Lettered P so that the first run's letters stay as they were, and done
here, between parts I and J: the guard needs setup finished, the phone open
with time left and everything LiMo needs switched on, as part I leaves it;
under a lock, the lock covers Android's settings first. Like the block
screen of part H it covers a few pages, never the whole phone. None of this
part has run on a phone yet.

What is guarded, and how it is known (the watch's README, "The guard"):
LiMo's device admin page in Android's Settings, by the class name of its
window, from Android's source
(`com.android.settings.applications.specialaccess.deviceadmin.DeviceAdminAdd`
from Android 10); Android asking LiMo's device admin whether it may be
switched off; and One UI's accessibility settings, by their package,
`com.samsung.accessibility`. The device admin's page is guarded only while
the protection is on. App info is not guarded: Android itself greys out
Force stop and Clear storage there for an active device admin, and sends
Uninstall to the device admin's page.

**The guard** has no drawing yet; it is laid out as the lock (frame A of
`phone.html`): LiMo and the clock at the top; the card with the key (over
the device admin's page) or the shield (over the accessibility settings),
the stamp `PROTECTED`, `ASHA · THIS PHONE`, `A guardian's PIN is needed
here.` and `These settings can switch LiMo off or remove it from this
phone.`; `WHAT YOU CAN DO` with one row, `Guardian PIN` and `Opens these
settings for 5 minutes`; `Nothing was closed. Your apps are still open
behind this screen.`; the maker's mark; `Emergency call` pinned. No `OPENS
AGAIN` and no `STILL OPEN`. Its row opens LiMo's own PIN screen: the strip
at the top with the key or the shield and `A guardian's PIN is needed
here.`, then `Guardian PIN`, `Opens these settings for 5 minutes`, the PIN
as dots, the keys and `Unlock` pinned, with nothing to choose (no `ADD
TIME`, no `OR KEEP IT OPEN FOR`).

First, `status`: `Nothing LiMo needs was switched off when it last
looked.`, the protection among them; note `Time left today`. Nothing counts
under the guard, but Settings' own pages count as any app does, and so do
the guarded pages once the PIN opened them. Part J wants more than 15
minutes left at its start: keep to the steps, and wait on LiMo's Today,
which counts nothing.

1. **The device admin's page, from Device admin apps.** Settings, Security
   and privacy, Other security settings, Device admin apps (Settings'
   search finds "Device admin" on any phone): the list is not guarded. Tap
   LiMo's row: Android opens LiMo's device admin page, and the guard with
   the key covers it at once, its bars too, before anything on it can be
   tapped. The log: `guard now device_admin by=page
   package=com.android.settings`, `show cover now guard page=device_admin
   rows=1 call=false` and `show cover drawn ms=N`, and once today a `note`
   line with `guarded_page_opened` and `device_admin`. Note N, and whether
   any of the page showed first; do not race the guard with a tap. Stay a
   full minute under it, with `status` before and after: `Used today` has
   not moved. If no guard comes, this phone names the page otherwise (the
   probe, steps 10 to 13, learns its name): tap Deactivate, and the guard
   should come over Android's question instead (`keep admin_off asked`,
   `guard now device_admin by=event package=com.android.settings`). Note
   whether the question showed first; if it shows uncovered, tap Cancel,
   never OK. (Settles the watch's question whether One UI names the device
   admin's page as Android's source does.)
2. **Leaving the guard without the PIN.** The guard covers Android's bars
   as the lock does (note 13). With gesture navigation a swipe for Back
   should still reach Android: the page goes, and the guard with it
   (`guard now none`, `show cover now nothing`). With buttons there is no
   Back to press: tap `Emergency call`, then Home on the dialler's own bar.
   Note the navigation and what worked; below, "leave the guard" means
   this. Then open LiMo's row again: covered again.
3. **Wrong PINs.** Tap `Guardian PIN` on the guard: LiMo's PIN screen, as
   above (the log says `guard now none` while LiMo's own screen is in
   front). Type a wrong PIN of 6 digits and tap `Unlock`: `That PIN is not
   right. 4 more tries before a wait.` in coral; a digit typed clears it.
   The log: `way_out guard_pin answer=wrong`. `status`: `Wrong tries of a
   code or the PIN since the last right PIN: 1.` Press Back: the page under
   the guard again (`guard now device_admin by=page
   package=com.android.settings`). Four more wrong PINs: `3 more tries`,
   `2 more tries`, `That PIN is not right. 1 more try before a wait.`, and
   at the fifth, back to the guard, which says `Too many wrong tries. The
   next try is at TIME.` in coral, a minute on, its row dashed and pale
   with `Wait until TIME`; a tap on it does nothing. `status`: `Wrong tries
   of a code or the PIN since the last right PIN: 5.` and `After too many
   wrong tries, the next try is today at TIME.` When the minute has passed,
   the row comes back by itself. These tries count with the lock's codes
   and PIN, as in part N, step 5.
4. **Emergency call on the guard.** Tap `Emergency call`: Android's
   emergency dialler opens at once, and nothing covers it. **Do not place
   an emergency call, and type no number.** The log: `show emergency
   opened how=emergency_dialler` (or another `how=`, as in part H, step 4),
   a line beginning `front now emergency_dialler`, and `guard now none`.
   Press Back: LiMo's device admin page again, under the guard, whether or
   not the page tells anything again.
5. **The right PIN, and five minutes.** `Guardian PIN`, the test PIN,
   `Unlock`: `These settings are open until TIME.` in teal for a second and
   a half, TIME five minutes on; then the PIN screen closes and the page is
   open. Note how long the answer took after `Unlock`. The log: `way_out
   guard_pin answer=release_confirmed`, a `note` line with `guard_opened`,
   and `show cover now nothing`; the watch may say `guard now device_admin
   ...` again as the page comes back, with no guard drawn: what is open is
   the engine's to say. `status`: `Wrong tries of a code or the PIN:
   none.` (a right PIN clears them, so part L starts with none). On the
   page, tap Deactivate: Android's question, which holds LiMo's words,
   `Switching this off lets LiMo be removed from this phone.` (the log:
   `keep admin_off asked`). Tap **Cancel, never OK**: the protection stays
   on. Settings, Accessibility: open too, with no guard. Then LiMo's Today,
   where nothing counts, until a minute before TIME; then LiMo's device
   admin page again (Device admin apps, LiMo), still open. At TIME the
   guard comes back over it by itself.
6. **The screen going off closes them.** On the guard, the right PIN again:
   the page open. Press the side key: the screen goes off. Turn it on and
   unlock: the guard is closed again. Note whether the page, still in
   front, is covered at once at the unlock, with no tap; either way, leave
   it, and open LiMo's row again: covered.
7. **App info.** Settings, Apps, LiMo: App info, which nothing covers. Note
   whether Force stop is greyed out, and, under Storage, whether Clear
   storage is (One UI may name it otherwise); tap neither. Then tap
   Uninstall: Android opens the device admin's page in a form whose one
   button switches the device admin off and removes LiMo in one tap,
   without asking LiMo first, and the guard covers it at once (`guard now
   device_admin by=page package=com.android.settings`). Leave the guard.
   **If no guard comes, press Back at once and tap nothing on that page**:
   its button would remove LiMo and end the run. Note what One UI showed.
8. **The home screen's Uninstall.** `status` first: the protection is not
   listed as switched off (with it off, Android would remove LiMo). On the
   home screen, a long press on LiMo's icon, then Uninstall: the
   installer's question, which is not guarded; a line beginning `front now
   app package=` names the installer's package: note it. Confirm it:
   Android refuses while LiMo is a device admin. Android's own installer
   then offers "Manage device admin apps" in a notification, which opens
   Device admin apps; One UI's may differ: follow what it offers to LiMo's
   device admin page, which the guard covers as in step 1. Note what showed
   at each point, and leave the guard.
9. **One UI's accessibility settings.** Settings, Accessibility: the guard
   with the shield covers them within about a second (`guard now
   accessibility by=package package=com.samsung.accessibility`, `show cover
   now guard page=accessibility rows=1 call=false`, and once today a `note`
   line with `guarded_page_opened` and `accessibility`). Leave the guard.
   Then look for a way to LiMo's accessibility switch from outside that
   package, switching nothing: Settings' search (`accessibility`, and
   LiMo's name) and the quick panel. Each one that shows LiMo's switch
   with no guard over it is a way around the guard: note it. Make no
   accessibility shortcut.

**The page probe** is the way to learn a phone maker's real page names (the
README of the debug build's `dev/`, "The page probe"). It is in debug
builds only, off by default and off again whenever LiMo's process starts.
While it is on, each window event of Android's Settings or the installer
(`com.android.settings`, `com.android.packageinstaller`,
`com.google.android.packageinstaller`) logs one line, tag `LiMoWatch`, such
as:

```
probe window package=com.android.settings class=com.android.settings.applications.specialaccess.deviceadmin.DeviceAdminAdd
```

It never logs another app's class name, nor any text or anything on the
screen; these are the only lines in which LiMo logs a class name.

10. **The probe on.** The log terminal follows both of LiMo's tags (or, in
    a terminal of its own, `adb logcat -s LiMo:V LiMoWatch:V`). In the
    other terminal:

    ```
    adb shell am broadcast -a ai.lucidloom.limo.child.dev.PROBE -n ai.lucidloom.limo.child/ai.lucidloom.limo.child.dev.DevReceiver --ez on true
    ```

    The log: `probe on` and `dev probe on`.
11. **The walk.** With the phone open (no lock on it, time left), walk each
    page once, slowly, a few seconds on each, and say aloud which page it
    is, so that the log can be matched to it afterwards: Settings' first
    page; Security and privacy, Other security settings, Device admin apps;
    LiMo's row there (the device admin's page); its Deactivate, then
    Cancel; Back to the list; Apps, LiMo (App info); its Uninstall (on
    Android 10 to 15 it opens the device admin's page, ready to switch it
    off and remove LiMo: tap nothing on it), then Back; its Force stop, and
    Storage with Clear storage (note whether each is greyed out; tap
    neither); Accessibility (One UI opens a package of its own); from the
    home screen, a long press on LiMo's icon and Uninstall (the installer's
    question), then Cancel. Where the guard covers a page, that is LiMo
    working: type the PIN, which opens these settings for five minutes, and
    carry on.
12. **The probe off.**

    ```
    adb shell am broadcast -a ai.lucidloom.limo.child.dev.PROBE -n ai.lucidloom.limo.child/ai.lucidloom.limo.child.dev.DevReceiver --ez on false
    ```

    The log: `probe off` and `dev probe off`.
13. **What the log says.** Copy it. Each `probe window` line gives a page's
    real package and class; each line beginning `front now app package=`
    the package in front, the installer's among them; each `guard now` line
    what was guarded. A page that switches LiMo off whose class is not in
    the watch's `Guarded.kt`, or the installer under another package than
    the three above, is what to add.

Undo: nothing; part J goes on from here. The probe is off, and the guard
closes again by itself within five minutes.

## Part J: the warnings

Turn Auto rotate on now, from the quick panel: part K needs it under the
lock, where Android's settings are covered.

Use the browser until the warnings come, at 15, 5 and 1 minute left. The
earlier parts used some of the day; if a warning came before this part,
note when.

1. **At 15 minutes** a card floats at the top of the screen over the
   browser (F in `phone.html`): a bell, `15 minutes of screen time left
   today.`, `The phone locks when the time is used up.`, a dashed line,
   `LOCKS ABOUT TIME` and `OK`. The log: `show warning floated`. While it
   shows, tap the browser outside the card and type in its search field:
   the browser takes the touches and keeps the keyboard. The card goes by
   itself after 10 seconds (it stayed 30 in the first run, and sat on the
   browser's address bar; see the notes). At a later warning, swipe the
   card up or to a side: it goes at once (`show warning gone by=swipe`),
   and a tap on its body does nothing. In the shade, the warning is a
   notification too, with no heads up and no sound.
2. **The heads up notification instead.** At about 7 minutes left (the
   notification says `7 min left`), switch LiMo off in Settings,
   Accessibility, Installed apps: the guard covers One UI's accessibility
   settings (part P), so type the test PIN on it first, which opens them
   for five minutes. At 5 minutes there is no card: Android's heads up
   notification shows `5 minutes of screen time left today.`, `Save what
   you are doing.` and `Locks about TIME`, with a sound. Switch
   accessibility on again at once (Settings, or Today's banner and its
   button), within the five minutes; after them, the guard comes back
   over those settings as LiMo's service connects, and the PIN opens them
   again. (Settles the question whether the warning falls back to
   Android's own notification while LiMo cannot draw.)
3. **At 1 minute**, the card again: `1 minute of screen time left today.`,
   `Save what you are doing now.`, `LOCKS ABOUT TIME`. This time tap `OK`:
   it goes, and does not come back. The notification says `0:N left`,
   counting the seconds.
4. A warning's notification goes away by itself a minute after the lock
   it warned of.

Undo: nothing; part K goes on from here.

## Part K: the day runs out: the lock, calls under it, turning the phone

1. **At zero** the lock covers the whole screen within a second or two, the
   bars too. Set it beside frame A of `phone.html`. From the top: LiMo and
   the clock (the phone's own time, in its 12 or 24 hours, and the date,
   such as `FRI 20 NOV`); the card with the hourglass, the stamp `LOCKED`,
   `ASHA · THIS PHONE`, `Today's time is used up.`, `You have used all of
   today's screen time.`, `OPENS AGAIN` and when: the end of the bedtime
   when one runs over midnight, as this test family's does, so `Tomorrow,
   6:00 am` (`Tomorrow, 06:00` on a 24 hour phone); with no bedtime over
   midnight, `Tomorrow, 12:00 am` (`Tomorrow, 00:00`). `WHAT YOU CAN DO`
   with three rows: `Ask for more time` (`A guardian sees your request in
   LiMo`), `Type a one time code` (`A guardian who is away can read you 8
   digits`) and `Guardian PIN` (`For a guardian who is with you`); `STILL
   OPEN` with two tiles, the phone app first, then `ALLOWED`, each with its
   own icon and name; `Nothing was closed. Your apps are still open behind this
   screen.`; the maker's mark; `Emergency call` pinned. Note anything that
   differs, with a photo.
2. The log: `show cover now lock why=time_used_up rows=3 tiles=2
   call=false` and `show cover drawn ms=N`. LiMo's notification's title is
   now `Today's time is used up.`
3. `status`: `Time left today: 0:00.` and `Locked: yes, today's time is used
   up, until tomorrow at 06:00. The lock says: Today's time is used up.`
   (`until tomorrow at 00:00` with no bedtime over midnight; `status` says
   its times in 24 hours.)
4. **Nothing gets past it.** The lock covers the navigation bar too, so
   there is no Home, Back or recent apps button to press, and the quick
   panel does not come down over it, as the first run found (note 13).
   Note which navigation the phone uses: the covers' README expects the
   swipes of gesture navigation still to reach Android, and the home screen
   is covered under a lock all the same.
5. **What stays open.** Tap `ALLOWED`'s tile: it opens and stays open,
   and its time is not taken from the day. Go back to the
   browser: locked again at once. Tap the phone app's tile: the phone app
   opens, never covered. Back to the browser: locked.

Calls under the lock (the Phone permission is on since part I):

6. **A ringing call, ringer on.** With the lock over the browser, ring this
   phone from the second one: the lock steps aside at once for the ringing
   call (`call now ringing`, `show cover now nothing`). Answer it (`call
   now ongoing`): the call's screen, never covered.
7. **Back to the call.** During the call, press Home: the lock is back, and
   `Back to the call`, outlined in teal, is pinned above `Emergency call`
   (`show cover now lock why=time_used_up rows=3 tiles=2 call=true`). Tap
   it: the call's screen (`show back_to_call how=in_call_screen`). End the
   call from this phone: the lock again, with `Emergency call` alone.
8. **A silent ring.** Set the sound mode to Mute, vibration off. Ring this
   phone again: `call now ringing` at once, and nothing covers the incoming
   call, until it ends. End it from the second phone. Set the sound mode
   back to Sound. If the phone has two SIMs, ring the second one too, and
   note what was seen. (Settles the watch's question whether a silent ring
   is seen at once with the Phone permission.)
9. **Emergency call.** On the lock, tap `Emergency call`: Android's
   emergency dialler opens at once and is never covered while it is up.
   **Do not place an emergency call, and type no number**: LiMo only opens
   the dialler, which dials nothing by itself. The log: `show emergency
   opened how=...` and the lines after it, as in part H, step 4. Press
   Back: the lock again.

Turning the phone:

10. With the lock over the browser, turn the phone on its side: the lock is
    laid out again, clear of the camera's cut out and of Android's bars,
    with `Emergency call` in sight. Turn it back.
11. On the lock, tap `Guardian PIN`. Under `OR KEEP IT OPEN FOR`, pick `1
    hour`; type three digits; turn the phone. The screen is built again:
    the dots are gone, `1 hour` is still picked. Turn it back and press
    Back: the lock.

Undo: nothing; part L goes on from here, on the lock.

## Part L: the ways out, at the first lock

Each way out is a screen of its own (B1 to B3 in `phone.html`), with the
lock's reason as a strip at the top of its card (the hourglass, `Today's
time is used up.` and `Opens tomorrow, 6:00 am` with this test family's
bedtime, as in part K, step 1), the main button pinned
at the bottom, and `Emergency call`, the plain line and the maker's mark at
the end of the page. Back always returns to the lock.

1. **A request.** Tap `Ask for more time`. With `30 min` picked, type a
   reason (`To finish my science project`): it shows as typed, and the
   counter counts (`28 / 200`). The keyboard comes up under `Send request`,
   which sits on it. Tap `Send request`: back to the lock, which now shows
   `You asked for 30 min at TIME`, stamped `WAITING`, and only two rows
   (`show cover now lock why=time_used_up rows=2 ...`). The log: a line
   beginning `way_out ask`, and no word of the reason. `status`: `A request
   waits for an answer: 30 minutes, asked today at TIME.`, and the reason
   nowhere. It waits all day: no master answers it in this run.
2. **Back leaves nothing.** Tap `Type a one time code`: `A guardian who is
   away from home can read you a code. It has 8 digits.`, eight cells, the
   keys, `A code works once, only today, and only on this phone.` and `Use
   this code` pinned. Type three digits and press Back: the lock. Open it
   again: the cells are empty.
3. **Secrets.** On the code's screen, try a screenshot: Android refuses
   it. Open the recent apps: no picture of it, and LiMo's ways out are not
   listed. Come back to it.
4. **A wrong code.** Type `1111 1111` (8 digits; the cells show two groups
   of four) and tap `Use this code`: it stays, with `That code is not
   right. 4 more tries before a wait.` in coral. Type a digit: the words
   go. The log: a line beginning `way_out code answer=wrong`. `status`:
   `Wrong tries of a code or the PIN since the last right PIN: 1.`
5. **The right code.** `.\limo-phone-dev code`: `A one time code for Asha's
   test phone, worth 15 minutes, for today only:`, the code as two groups of
   four digits on a line of its own, and that no phone was needed: no
   `> adb` line. On the phone, type it and tap `Use this code`: `15 minutes
   added. The phone opens now.` in teal for a second and a half, then the
   screen closes and the lock goes (`show cover now nothing`). The browser
   is usable; the notification says `15 min left`. The tool's log file has
   `code made a one time code of 15 minutes.` and no digits of the code.

Undo: nothing; part M goes on from here.

## Part M: the code's fifteen minutes

1. **Asking again.** On Today, tap `Ask for more time`, then `Send
   request`: it stays, with `A request from this device is still waiting
   for an answer.` in coral under the field. Press Back.
2. **Larger text.** Settings, Display: the font size and the screen zoom to
   the largest. Today and How it works: nothing cut, tab labels on two
   lines if they need it, the child's name on one line with an ellipsis if
   it is too long. If there is time, also switch the navigation bar between
   gestures and buttons, for part N.
3. **A restart with time left.** Restart the phone and unlock it. The log
   terminal stops (`The log ended: ...`): run `log` again once the phone is
   back. Without opening LiMo, its notification is back, and no line says
   `keep start_refused`. A `note` line with `restarted`. `status`: the time
   used is what it was before the restart, give or take a few seconds, and
   `Nothing LiMo needs was switched off when it last looked.` (Settles the
   watch's question whether a restart wrongly reports accessibility as
   switched off.)
4. **Optional, during a break:** leave the phone with its screen off for
   twenty minutes, then `adb shell dumpsys alarm | findstr limo`: one alarm
   of LiMo's, about fifteen minutes ahead or at the engine's next check;
   and Settings, Apps, LiMo, Battery shows next to nothing used. (Settles
   the host's questions about its alarm and its cost with the screen off.)
5. Use the browser until the lock. Note which warnings come in these
   fifteen minutes (perhaps the one at 15 at once, then 5 and 1), and that
   the cards are whole at the larger text.

Undo: nothing; part N goes on from here, on the lock, with the larger text.

## Part N: the second lock

1. **At the larger text** the lock's titles and rows take more lines,
   nothing is cut or runs out of its box, the page scrolls above the pinned
   strip, and `Emergency call` stays in sight. Two rows: no asking while
   the request waits.
2. **A restart while locked.** Restart the phone; at the first unlock,
   before anything is opened, the lock is back at once. Run `log` again:
   `show cover now lock why=time_used_up ...`.
3. **Stopped from the quick panel.** In the first run the quick panel did
   not come down over the lock (part K, step 4); if it does not, do this
   step in part O instead, with the phone open. If it does, pull it down,
   open Android's list of running apps, and tap Stop for LiMo. The lock
   goes. Write down when it comes back: at once if the accessibility
   service connects again, at the latest by LiMo's alarm, within about
   fifteen minutes; and what the log shows meanwhile (no `host failed`
   line). (Settles the
   keeper's question how LiMo comes back after Android 13's Stop button.)
4. **The code again.** `Type a one time code`, and the code of part L:
   `That code was already used today.` Then type `1234` and tap `Use this
   code`: `A code has 8 digits.` Neither counts as a wrong try.
5. **Wrong tries, and the wait.** `Guardian PIN`, `15 min` under `ADD
   TIME`, a wrong PIN (6 digits), `Unlock`: `That PIN is not right. 3 more
   tries before a wait.` (one wrong try is left from part L). Twice more:
   `2 more tries`, then `That PIN is not right. 1 more try before a wait.`
   Once more, the fifth wrong try: back to the lock, which says `Too many
   wrong tries. The next try is at TIME.` in coral, a minute on, with the
   code and PIN rows dashed and pale, each `Wait until TIME`; a tap on them
   does nothing. `status`: `Wrong tries of a code or the PIN since the last
   right PIN: 5.` and `After too many wrong tries, the next try is today at
   TIME.`
6. When that minute has passed, the rows come back by themselves.
7. **The right PIN.** `Guardian PIN`: what to give first (`ADD TIME`: `15
   min`, `30 min`, `1 hour`; `OR KEEP IT OPEN FOR`: `1 hour`, `2 hours`,
   `Today`), then the PIN as dots, then the keys, `Unlock` pinned. Pick `15
   min`, type the test PIN, `Unlock`: `15 minutes added. The phone opens
   now.` in teal, then the lock goes. Note how long the answer took after
   `Unlock` (the PIN is checked slowly on purpose; this settles how long
   that takes on a phone). The log: a line
   beginning `way_out pin`. `status`: `Wrong tries of a code or the PIN:
   none.`
8. Put the font size, the screen zoom and the navigation bar back.

Undo: nothing; part O goes on from here.

## Part O: LiMo stopped, forgotten and removed

The phone is open, with time left.

1. If part N, step 3 could not be done, do it now: Stop LiMo from the quick
   panel's list of running apps, and write down when its notification
   comes back. The list is the shade's, not Settings', so no guard covers
   it: Stop is one of the ways base protection leaves open.
2. Settings, Apps, LiMo (App info, which nothing covers). Force stop is
   expected greyed out, as Android's own Settings greys it out for an
   active device admin (part P, step 7, noted One UI's). If it is, stop
   LiMo over the cable instead, way back 4, tried here for the first time:
   `adb shell am force-stop ai.lucidloom.limo.child`. Otherwise tap it, and
   confirm, and note it: on this phone a child could do the same. LiMo's
   notification goes. Expected from the watch's README:
   Android switches LiMo's accessibility service off. (Settles the watch's
   question whether a force stop is told to the guardian.)
3. Open YouTube: it is not covered. A force stop is another way base
   protection leaves open: with its accessibility service off, LiMo can
   draw nothing, the guard neither. Leave YouTube. Wait two minutes, and
   note whether LiMo comes back by itself.
4. Open LiMo: Today's banner says `Accessibility is switched off, so LiMo
   cannot see which app is open.` `status`: `Switched off on the phone when
   LiMo last looked: accessibility.` The log: a `note` line with
   `permission_off` and the accessibility. Then `Show a guardian what to
   do`: its `Open Android's settings` opens One UI's accessibility
   settings with nothing over them, since LiMo can draw nothing yet.
   Switch LiMo on there as in part E, step 6: once its service connects,
   the guard covers those settings within a second or two (`guard now
   accessibility by=package package=com.samsung.accessibility`), unless a
   PIN opened the guarded pages less than five minutes before. Type the
   test PIN on it, then Back twice: the step ON, `Next`: Today, with no
   banner. YouTube is covered again.

The protection off, forget and uninstall:

5. **The protection off, by a person, under the guard.** First
   `.\limo-phone-dev uninstall`: `uninstall removes LiMo from the phone,
   with everything it kept there.`, then `Android will not remove LiMo
   while it is a device admin on this phone (its protection). Switch the
   protection off on the phone, on Android's Device admin apps page (the
   search in Settings finds it), then run uninstall again. This tool never
   changes it.` and `Nothing was
   changed on the phone.` Exit code 2. (The home screen's refusal is part
   P, step 8.) Then find Device admin apps (on One UI 5, Settings, Security
   and privacy, Other security settings; Settings' search finds "Device
   admin" on any phone), and LiMo there: its page comes up under the guard,
   as in part P, step 1. Type the test PIN on it: `These settings are open
   until TIME.`, then the page, with the same words as in part E, step 8.
   Tap Deactivate: Android's question, which holds LiMo's words,
   `Switching this off lets LiMo be removed from this phone.` (the log:
   `keep admin_off asked`). Confirm it. The protection is off, and the
   page is guarded no more: the guard stands over it only while the
   protection is on. The log: a `note` line with `permission_off` and the
   protection. Open LiMo: Today's banner says `Protection is switched off,
   so LiMo can be removed.` `status`: `Switched off on the phone when LiMo
   last looked: protection.` If the guard will not open with the right
   PIN, do step 6 first: with no state kept, LiMo covers nothing, the guard
   neither, and the page is open; then switch the device admin off there.
6. **Forget, under a cover.** Open YouTube, so that the block screen is up.
   `.\limo-phone-dev forget`: `forget asks LiMo on the phone to wipe the
   test family, checks that it did, then deletes it on this PC.`, `Asked
   LiMo to wipe the test family. Android: Broadcast completed: result=0`,
   `LiMo wiped it: its kept state is gone from the phone.`, `Deleted the
   test family on this PC.` and `Next: setup, to make a new test family.`
   Exit code 0. Within seconds the block screen is gone and YouTube plays;
   LiMo's notification is gone. The log: `host forgotten` and `dev forget
   done`. Open LiMo: S0, `This phone is not set up yet.`
7. `.\limo-phone-dev check`: `LiMo's state on the phone: none yet, so no
   family is on it.` and `The test family on this PC: none.`
8. `.\limo-phone-dev uninstall`: `uninstall removes LiMo from the phone,
   with everything it kept there.`, then `Removed LiMo from the phone, with
   everything it kept.` Exit code 0.
9. `.\limo-phone-dev check`: `LiMo's child form: not installed.`

Undo: this part is the undoing.

## Finish

1. `.\limo-phone-dev uninstall`: `LiMo's child form is not installed on
   this phone. Nothing was changed.`, exit code 0. `.\limo-phone-dev forget
   --this-pc-only`: `There was no test family on this PC.`, exit code 0.
2. On the phone, put back what was noted before the run: the font size, the
   screen zoom, Auto rotate, the sound mode, the navigation bar. LiMo is
   gone from Never sleeping apps with LiMo itself.
3. If they were off before the run: Settings, Developer options, Revoke USB
   debugging authorisations, then USB debugging off, then developer options
   off.
4. The test calls are in both phones' call logs; delete them if wanted.
5. The folder `%LOCALAPPDATA%\LiMo-phone-dev-TEST-ONLY` now holds only the
   tool's log: read it once more (no name, PIN or code in it), then it may
   be deleted.

## Results

| Check | Expected | Seen |
| --- | --- | --- |
| A1 to A5 | as written, exit codes 0, 2, 2, 2, 2 | as written; A2 said `No phone is attached`, exit 2 (the tool before its cable check) |
| B1 phone not allowed yet | the "has not allowed this PC" message, exit code 2 | not run: this PC was already allowed |
| B2 check: adb, phone, Android, not installed | yes; note the Android version and model | yes: Android 13 (API 33), a Galaxy S20 Ultra |
| B3 package names of YouTube, an app to always allow, the browser | found; note them | `com.google.android.youtube`, `com.android.chrome`; no calculator, the Clock (`com.sec.android.app.clockpackage`) used: note 3 |
| B4 two attached | both listed, refused | not run |
| C1 release and guardian builds refused, no adb line | yes | yes |
| C2 debug build installed | yes; copy any refusal | yes; the phone asked nothing |
| C3 check: a debug build, run-as works | yes | yes |
| C4 S0 on the phone, its card in the middle, as drawn; `host not_paired waiting` | yes | yes; to change: note 16 |
| D1 setup: the zone, the family, N bytes, broadcast, taken | yes; note N and how long it took | failed, then passed: see note 1; the second time 1381 bytes, taken at once |
| D2 S1 by itself, naming Asha | yes | yes |
| D3 `dev pair taken`, `host paired`, no `host failed` | yes | yes: the Rust engine loads and runs on this phone |
| D4 second setup refused, nothing sent | yes | yes |
| D5 check: this PC's test family | yes | yes |
| D6 status: Asha, 30 min, 0 used, not locked, six switched off | yes | yes, the six in words |
| D7 tool's log: no name, no PIN | yes | yes; the failed setup left no line (note 1) |
| E1 S2: Continue off until ticked (the minute not counted moved to I1) | yes | yes: a full minute, 0 min used, `front now limo` |
| E2 swiped away: back at the first step not on | yes | yes, at Notifications |
| E3 Notifications: Android's question, ON, the notification there | yes | the first answer logged `granted=false`; ON through `page=app_notifications`; the notification then there: note 4 |
| E4 Phone refused twice: the line and Go on without it, nothing moved | yes | yes; Android asked both times |
| E5 Usage access: which page opened | `usage_for_limo` or `usage_list`: note which | `usage_for_limo`, one switch |
| E6 Accessibility: row lit up or top page; restricted setting | note which | the top page: note 5; not a restricted setting |
| E7 Battery: ON at once; the Samsung note's path on this phone | note the path | ON at once; the path not written down: note 6 |
| E8 Protection: Android's page with LiMo's words, no powers listed, ON | yes | ON (`seam step=protection answered`); the page's words not noted |
| E9 S9: ticks for all but Phone, which has a ring, `NOT ON` and the sentence under the rows | yes | yes: five ticks, none on Phone; to change: note 16 |
| E10 no `not_opened` | none | none |
| E11 status: setup not finished said first; 0 min used, no apps; phone switched off | yes | phone switched off; 6 min, all on Android's own pages: note 6 |
| F1 C1 to C3 as drawn | note differences | not looked at closely |
| F2 Today at once after swiping away | yes | not tried |
| F3 Today and its banner against E and T | note differences | as drawn |
| F4 notification paused with Today in front | yes | yes: `24 min paused` |
| F5 How it works against H, About with `LiMo 0.1.0` and the address; Back to Today | note differences | as drawn: 30 min, 23:00 to 06:00, Clock, YouTube; Back not noted; to change: note 16 |
| F6 Ask for more time from Today: B1 with no strip | yes | the asking screen opened; its strip not noted; a picture over the cable came back empty: note 8 |
| G1 browser counted: notification, status, Today | yes | Chrome counted: used went from 6 to 9 min, `21 min left` with Android's countdown; Today not noted |
| G2 shade, Edge panel, screenshot toolbar, keyboard: no new front | yes | yes: each logged `not_confirmed front=com.android.chrome`; a clock note meanwhile: note 9 |
| G3 recents read as home; home, screen off, Android's lock screen not counted | yes | yes; `20 min paused` |
| G4 `ALLOWED` recorded, not counted | yes; note the notification's words | the Clock recorded (1 min), not counted; the words not noted |
| G5 ringing not counted; call's screen the dialler; the browser during a call counted | yes | not run: no SIM |
| G6 a call made: call screen learned or known | note which | not run: no SIM |
| G7 log: only LiMo's tags, no names, PIN, code or typed text | yes | yes: the whole log searched |
| H1 block screen as drawn, bars covered, the note once | yes; note differences | as drawn; bars covered; the note once for the day |
| H2 home, not covered again; YouTube's late event not confirmed | yes | never covered over home; no late window event from YouTube in the log |
| H3 fourteen tries, never covered over home | yes; note the ms values | 15 tries: 15 covers up, 15 down, never over home; drawn in 72 to 402 ms, median 244 |
| H4 emergency dialler from the block screen, not covered, nothing dialled; a known one ends the wait (`front emergency_wait ended first=`) | yes; note `how` and how it was known | `how=emergency_dialler`; `front now emergency_dialler package=com.samsung.android.app.telephonyui`, known at once, never covered; nothing dialled. Before the watch's fix, so no `emergency_wait` line; what came after: note 2 |
| H5 YouTube not counted | yes | yes |
| H6 silent ring without the Phone permission over the block screen | note what showed and what the log said | not run: no SIM |
| I1 usage access off while Today shows: AND 1 MORE; out of split screen `front now limo`; a minute on Today not counted; ways out not covered | yes; note split screen or not | switched off in Settings, not in split screen: `AND 1 MORE`, the `permission_off` note, `front now unknown`, then `front now limo` on Today; the ways out not tried |
| I2 Phone step opens App info after two refusals | `page=app_details` | `page=app_details` |
| I3 call within ten seconds of granting: `call now ringing` | yes | not run: no SIM |
| I4 the banner's walk ends on Today with no banner | yes | yes |
| I5 status: nothing switched off | yes | yes |
| P1 the device admin's page from Device admin apps: the guard at once, by its name, as described; a minute under it not counted; the note once | yes; note `ms=`, and whether the page showed first | |
| P2 leaving the guard without the PIN | note the navigation, and what worked | |
| P3 wrong PINs on the guard: the words, counted with the lock's; the fifth: the wait, the row dashed; the row back after a minute | yes | |
| P4 Emergency call from the guard: never covered, nothing dialled; Back: the page guarded again | yes; note `how` | |
| P5 the right PIN: the teal words, the page open, Deactivate shows LiMo's words, Cancel; the accessibility settings open too; the guard back at the five minutes' end; wrong tries cleared | yes; note how long the answer took | |
| P6 the screen off closes them; the page covered at the unlock, or only when opened again | note which | |
| P7 App info: Force stop and Clear storage greyed out or not; Uninstall's page under the guard | note which | |
| P8 the home screen's Uninstall refused; the way on to the device admin's page; the guard over it | note what One UI showed, and the installer's package | |
| P9 One UI's accessibility settings under the guard; any way to LiMo's switch around it | note any found | |
| P10 to P12 the probe on, the walk, the probe off | yes | |
| P13 each page's package and class; names to add to `Guarded.kt` | note them | |
| J1 card at 15: words, keyboard kept, gone after 10 s (30 s in the first run), a swipe puts it away, quiet in the shade | yes | the card floated (`show warning floated`), touches outside it reached Chrome; it sat on the address bar: note 11; the rest not noted |
| J2 accessibility off, after the PIN on the guard: heads up at 5 with a sound | yes | not run |
| J3 card at 1, OK puts it away; notification counts seconds | yes | the cards at 5 and 1 minute floated; OK and the seconds not noted |
| J4 warning notifications gone a minute after the lock | yes | not looked at |
| K1 the lock against A: label, words, rows, tiles, fonts, logo | note differences | as drawn: `OPENS AGAIN Tomorrow, 06:00` (note 12), three rows, tiles Phone and Clock with their own icons |
| K2 log: lock, rows=3, tiles=2; notification is the lock's title | yes | yes: `rows=3 tiles=2 call=false`, drawn in 916 ms; `Today's time is used up.` |
| K3 status: 0:00, locked until the day's end, or until the end of a bedtime over midnight | yes | locked until tomorrow at 06:00 (note 12) |
| K4 the navigation bar covered; quick panel over the lock | note whether the panel came down | no Home, Back or recents to press; the quick panel does not come down: note 13 |
| K5 `ALLOWED`'s tile opens and stays; phone app tile; locked again | yes | yes: the Clock and the Phone app open from their tiles; home and Settings locked again in about half a second |
| K6 ringing call: lock steps aside at once | yes | not run: no SIM |
| K7 Back to the call in teal above Emergency call; opens the call | `how=in_call_screen` | not run: no SIM |
| K8 silent ring with the Phone permission: never covered | yes; two SIMs if any | not run: no SIM |
| K9 emergency dialler from the lock, nothing dialled | yes | yes: opened from the lock, never covered |
| K10 the lock turned: laid out again, cut out clear | yes | the owner turned the phone; what it showed was not written down |
| K11 B3 turned: dots empty, choice kept | yes | not run |
| L1 request: waiting on the lock, two rows, reason not logged or printed | yes | yes: `way_out ask answer=done`, `rows=2`; status `A request waits for an answer: 30 minutes, asked today at 20:38`; the reason nowhere |
| L2 Back leaves the cells empty | yes | not run as written |
| L3 screenshot refused, no picture in recents | yes | not run as written; see F6 |
| L4 wrong code: words in coral, cleared by typing; status 1 wrong try | yes | five wrong codes, not one: note 14 |
| L5 right code: 15 minutes added, lock gone; no digits logged | yes | yes: `way_out code answer=added`, 15 minutes added, the lock gone; no digits in any log |
| M1 second request refused in the core's words | yes | not run: the run was ended |
| M2 Today and How it works at the largest text | note anything cut | not run: the run was ended |
| M3 restart with time left: notification back, time kept, nothing reported off | yes | not run: the run was ended |
| M4 optional: one alarm of LiMo's; battery next to nothing | note both | not run: the run was ended |
| M5 warnings in the code's fifteen minutes | note which | not run: the run was ended |
| N1 the lock at the largest text | note anything cut | not run: the run was ended |
| N2 restart while locked: lock at once | yes | not run: the run was ended |
| N3 Stop from the quick panel: when the lock came back | note how long | not run: the run was ended |
| N4 code already used; 4 digits refused; neither counted | yes | not run: the run was ended |
| N5 wrong PINs count down; fifth: the wait, rows greyed | yes | seen with five wrong codes in part L: back to the lock, a wait of one minute (note 14) |
| N6 rows back after the minute | yes | yes, after the codes of part L |
| N7 right PIN adds 15 minutes, wrong tries cleared | yes; note how long it took | not run: the run was ended |
| O1 Stop from the quick panel, if not done in N3 | note how long | not run: the run was ended |
| O2 Force stop: greyed out in App info, then over the cable, or done; accessibility switched off | note which | not run: the run was ended |
| O3 YouTube not covered; LiMo back by itself or not | note which | not run: the run was ended |
| O4 banner and status: accessibility off, the note; on again, the guard over One UI's accessibility settings, YouTube covered | yes | not run: the run was ended |
| O5 tool's uninstall refused, exit 2; the device admin's page under the guard, the PIN, LiMo's words; switched off, its note, banner and status | yes | not run: the run was ended |
| O6 forget under the block screen: cover gone, S0 | yes | forget with no cover up: wiped, `host forgotten`, `dev forget done`; S0 not noted |
| O7 check: nothing on either side | yes | yes |
| O8 uninstall done | yes | not run: the run was ended |
| O9 check: not installed | yes | not run: the run was ended |
| Finish: nothing left, phone as found | yes | not done: LiMo left installed, with its device admin, accessibility and usage access on |

## Notes from the first run, 20 November 2026

A Galaxy S20 Ultra, Android 13 (One UI 5.1), with no SIM, and a Windows
PC; the debug child form, version 0.1.0. The owner held the phone, and the
commands were run at the PC. Parts A to L were run in order, then forget
(part O, steps 6 and 7), in a little over two hours. Pictures of S5, S9,
Today, How it works, the block screen and the lock were taken over the
cable. Part P, base protection, was built after this run and has not been
run: its Seen cells are empty.

1. **Part D, step 1: the tool misread LiMo's state.** The first setup
   wrote the pairing file and LiMo took it (its log said `dev pair taken`),
   but the tool said it had not. adb on Windows gives every line feed of an
   `adb shell` answer as a carriage return and a line feed, so the kept
   state (1984 bytes) came back as 1987; the engine took it for damaged,
   and the tool then deleted the test family on the PC, leaving the phone
   holding a family no PC knew. Fixed during the run (commit 54e39e9): the
   state is read through `adb exec-out`, which hands its bytes over as they
   are; setup keeps the PC's test family whenever LiMo says `taken` or keeps
   a state; and `forget --phone-only` wipes a family the PC does not have.
   The second setup passed. The failed one left no line in the tool's own
   log.
2. **Part H, step 4, and after: a screen learned as an emergency dialler.**
   Emergency call on the block screen opened Samsung's emergency dialler,
   which LiMo knew at once from Android's roles. The home screen, Settings
   and its search came next; the search (`com.android.settings.intelligence`,
   a package with no icon) came to the front 8.7 seconds after the tap, and
   the log said `front learned emergency_dialler
   package=com.android.settings.intelligence`: until LiMo's process ended,
   it would never have been covered or counted. Fixed after the run (commit
   4d968b7): only the first screen after the tap can be learned, a known
   emergency dialler ends the wait (`front emergency_wait ended first=`
   with its package), and nothing after it is learned. Not yet seen on a
   phone.
3. **Part B, step 3: no calculator on this Galaxy.** The Clock
   (`com.sec.android.app.clockpackage`) was the app always allowed. The
   steps now say to pick any app with an icon.
4. **Part E, step 3: the first Notifications answer logged as not
   granted.** Android asked; after 12 seconds on its question the log said
   `seam step=notifications asked granted=false`. The second tap opened
   LiMo's notification settings (`seam step=notifications opened
   page=app_notifications`), where the owner switched them on, and the
   notification then showed. What was tapped on the question is not known:
   try again on a fresh install.
5. **Part E, step 6: One UI opened Accessibility's top page**, not LiMo's
   row: Installed apps, then LiMo. One UI also offered a shortcut, which
   was declined.
6. **Part E: setup used 6 of the 30 minutes**, all on Android's own pages
   (`com.samsung.accessibility` 2 minutes, `com.android.settings` 2,
   `com.samsung.android.lool` 1; each app's minutes are rounded down) and
   none on LiMo's screens. For the Battery step's Samsung note the owner
   went through Device care (`com.samsung.android.lool`); the path One UI
   5.1 really has was not written down.
7. **Part E: a `permission_off` note for accessibility during setup**,
   though as far as was seen it had not been switched on yet. Why is not
   settled.
8. **Part F, step 6: the asking screen cannot be pictured.** A picture of
   it taken over the cable came back empty (0 bytes): the ways out are
   protected from capture, as built.
9. **Part G, step 2: a clock note.** A `clock_moved` note came during the
   overlay checks: the phone's clock moved 8 seconds back while automatic
   time was on, and LiMo followed it (`followed` true).
10. **Part J: a YouTube link in the browser.** A YouTube link tapped in
    Chrome brought the YouTube app to the front for half a second: it was
    covered in 116 ms and uncovered when Chrome came back. Watching
    youtube.com inside Chrome is not blocked: LiMo blocks apps, not sites.
11. **Part J, step 1: the warning card sat on the browser's address bar.**
    To change.
12. **Part K, steps 1 and 3: "OPENS AGAIN Tomorrow, 06:00".** The test
    family's bedtime runs over midnight (23:00 to 06:00), so the phone
    opens at the bedtime's end, not at midnight; `status` said `until
    tomorrow at 06:00`. The phone was set to 24 hours. The steps now say
    so.
13. **Part K, step 4: no Home to press.** The lock covers the navigation
    bar, so there is no Home, Back or recent apps button to press, and the
    quick panel does not come down over the lock. Which navigation the
    phone used was not written down. Part N, step 3, which needs the quick
    panel, now goes to part O.
14. **Part L, step 4: five wrong codes, and the wait.** Five wrong codes
    were typed, not one. After the fifth the ways out closed, the lock came
    back with the code and PIN rows waiting, and a minute later the rows
    came back by themselves. This stands for part N, steps 5 and 6, with
    codes in place of PINs. The words after each wrong code were not
    written down.
15. **The log follower.** `.\limo-phone-dev log`, started by a program
    with no terminal, ended at once: `The log ended: adb stopped following
    it, perhaps because the phone was unplugged. Run log again to go on.`
    It was not tried in a terminal window; the log was read in snapshots
    with `adb logcat -d` instead.
16. **To change, from the owner.** S0 (part C, step 4): the card in the
    middle of the screen, not at the top. S9 (part E, step 9): with a step
    off, say so, since five ticks of six read as a fault. How it works,
    About (part F, step 5): remove the "Free and open source" line and the
    repository's address, and put "More info on https://lucidloom.ai" in
    their place. The warning card on the address bar (note 11).
17. **Not run, and why.** No SIM: G5, G6, H6, I3, K6 to K8. This PC was
    already allowed: B1. Not tried: B4, F2, J2, K11, and L2 and L3 as
    written. The owner ended the run after part L: parts M and N (but N5
    and N6, note 14), part O but forget (steps 6 and 7, run with no cover
    up), and the Finish. LiMo was left installed on the phone, with its
    device admin, accessibility and usage access on.
18. **Since this run: nothing is counted or covered until setup is
    finished.** The first run saw 6 minutes counted during setup, all on
    Android's own pages (note 6), and a `permission_off` note during it
    (note 7). Since then, from pairing until the guardian taps `Show Asha
    how it works` on S9, LiMo counts nothing, covers nothing and notes no
    permission switched off, and its notification and `status` say setup
    is not finished. Parts D, E and F say so now, and the check that LiMo's
    own screens are not counted with usage access off moved from E1 to I1.
    The Seen cells of the results keep what the first run saw.
19. **Since this run: five screens, and base protection's first part.**
    Note 16's changes were made: S0's card stands in the middle (C4); S9
    marks a step that is off with a ring and `NOT ON`, and says under the
    rows which is off and that Today shows a guardian how to turn it on
    (E9); How it works' About says `LiMo 0.1.0` and `More info on
    https://lucidloom.ai`, with no licence line and no repository address
    (F5). The Battery step's Samsung note names a second path (E7), and the
    Protection step's why is now `So a guardian's PIN is needed to switch
    this off, and LiMo cannot be removed while it is on.` (E8), which the
    guard keeps once setup is finished. The guard is new: part P tries it,
    part J, step 2 and part O, step 4 meet it over One UI's accessibility
    settings, and part O, step 5 now switches the protection off under it,
    before forget. So part O's steps moved: the old steps 4 and 5 are step
    4; the old steps 8 and 9 are step 5, less the home screen's refusal,
    now part P, step 8; forget is still steps 6 and 7; the old steps
    10 and 11 are 8 and 9. The Seen cells of the results keep what the
    first run saw.
