# host

The engine's host on LiMo's child form: the one owner of the one Rust
engine of the process (`PhoneEngine`, through the bridge in
`phone/ffi`), and everything the engine's README asks of a host. Kotlin,
package `ai.lucidloom.limo.child.host`, in the child module.

**What it is.** A thread of its own that loads the kept state, calls the
engine with the watch's facts about once a second while the screen is on
and at once on any change, keeps the state exactly when the engine says,
and hands out the orders: the cover to show/, the time left and the
warnings to keep/, the engine's notes to the log. It carries the ways out
of the lock (asking for time, a code, the PIN) to the engine and the
answers back, and gives the engine's view to the child's screens.

**What it is not.** It decides nothing (the engine does), reads no fact
itself (watch/ does), draws nothing (show/ does), and keeps nothing alive
(keep/ does). It has no link to the master PC yet, so the master is never
in reach and no report is ever sent. It does not answer the screens'
requests itself: `LimoChildModule` hands each to the seam (seam/README.md),
which asks the host for its view; the module also starts the keeper when
the app opens. Nothing here has run on a phone yet: see [What only a phone can prove](#what-only-a-phone-can-prove).

## Contents

- [How the other pieces call it](#how-the-other-pieces-call-it)
- [When it ticks, and what wakes it](#when-it-ticks-and-what-wakes-it)
- [What it does with the orders](#what-it-does-with-the-orders)
- [The child's name](#the-childs-name)
- [The kept state](#the-kept-state)
- [When something goes wrong](#when-something-goes-wrong)
- [What it logs](#what-it-logs)
- [Tests](#tests)
- [What only a phone can prove](#what-only-a-phone-can-prove)
- [Left open](#left-open)
- [Layout](#layout)

## How the other pieces call it

```kotlin
val host = Host.of(context)        // one per process, any thread; keep/Keeper starts it
```

Every call returns at once and may be made from any thread; the work runs
on the host's thread ("LiMo engine"), the only one that calls the engine.
Every answer comes back on the main thread. Nothing throws.

**For the covers and the ways out (show/):**

| Call | What |
| --- | --- |
| `useShower(shower)` | Who draws the covers. `Shower.show(cover)` is called on the main thread at every tick (about once a second with the screen on, at once on any change), with the engine's `Cover` as a level: draw it, change it, or take it away for `NOTHING`; the same cover twice changes nothing. `Shower.warn(warning)` once, in the tick a warning falls due. show/'s shower is given by `keep/Keeper.start`, through `CoverShower.install(app) { Host.of(app).childName }`, each time the keeper starts (only the first call in a process gives it), and the host ticks at once when it is given. Until then a stand in logs each cover as it changes and each warning (see [What it logs](#what-it-logs)). |
| `askForTime(minutes, reason) { answer -> }` | The child asks for more time, with the reason typed (or empty). `WayOut.Done(TimeRequest)`, or `WayOut.Refused(e)` with the core's own words in `e.message`, such as "A request from this device is still waiting for an answer.". |
| `tryCode(bytes) { answer -> }` | A one time code typed on the lock, as UTF-8 bytes. `WayOut.Done(TryAnswer)` with the engine's words (`message`, `opensNow`), whatever the kind. |
| `tryPin(bytes, PinAction(...)) { answer -> }` | A guardian's PIN, the same way; the action (`ADD_MINUTES` with minutes, `UNLOCK_UNTIL`, `CONFIRM_RELEASE`) is the activity's to know. |

A way out's answer is one of `Done`, `Refused` (the core said no, in its
words), `NotPaired`, or `Failed` (something failed inside LiMo and is in the
log; trying again may work). Before the answer is handed back, the state is
kept (right or wrong, as the engine's README asks) and the engine is ticked
again, so a lock that a right code opened is gone by the time the answer
arrives.

**Secrets.** A code or PIN goes in as a `ByteArray` and the host fills it
with zeros once the engine has read it, whatever happens, even when the
call could not be made. The caller reads the field into a `CharArray`,
makes the bytes, fills the `CharArray` with zeros and clears the field.
The reason a child types is not a secret, but it is never logged.

**For the screens (the seam's Kotlin side, seam/):**

| Call | What |
| --- | --- |
| `paired` | A state is kept on this phone, read from the file itself, right at once on any thread. True also while that state cannot be read (the phone is then locked). |
| `childName` | The child's name the pairing gave, for the child's screens and the lock; null when not paired, or when no name is kept (see [The child's name](#the-childs-name)). Read from its file, right at once on any thread. |
| `view { v -> }` | `HostView(paired, json, problem)`: the engine's whole view as JSON (snake case), with `today` and `rules_words` for Today and How it works, as the engine's README says; null when not paired or when the kept state cannot be read, and then `problem` says why. Apps in it are packages: the seam puts each app's name in, from the watch. |
| `finishSetup { answer -> }` | A guardian finished setup on the phone: they left setup's last screen (the seam calls it for `child.finishSetup`, and again for `child.markIntroSeen` in case that request was lost). Until the engine has it, it counts nothing and covers nothing, and the time left it hands the keeper says "Setup is not finished on this phone.". `WayOut.Done(true)` when this finished it, `Done(false)` when it was finished already (nothing changes, nothing is written), `Refused` (`CannotCheck`) when the kept state cannot be read, `NotPaired`, `Failed`. As with a way out, the state is kept before the answer and the engine ticked at once, so a blocked app in front is covered there and then. |
| `listen { what -> }` | Told on the main thread: `PAIRING` when the phone was paired or forgotten (and so its name changed), `VIEW` when the time left's words, the cover, the permissions or the engine's notes changed, or after a way out. Ask again then. Close the handle to stop. |

**For pairing (dev/ now, the link later):**

| Call | What |
| --- | --- |
| `pair(welcome, codeSecret, childName) { ok -> }` | A new engine from the core's welcome grant (JSON bytes) and the 32 byte code secret, both filled with zeros after, for the child called `childName`. Its state is written before the engine is used; if it cannot be written, nothing changes, the name neither, and the answer is false. Then the name is kept (or, with none, any older one removed). It replaces any state and name kept before, a state that cannot be read among them. |
| `forget { }` | The kept state and the child's name deleted, nothing covered, nothing ticking, the keeper stopped (`KeeperDisplay.cleared`). |

**For receivers (keep/):** `start()` (idempotent) and `wake(done)`, a tick
at once with `done` run after it, so a receiver can hold its broadcast
until the tick is done.

## When it ticks, and what wakes it

All in `HostTimes`:

| When | Tick |
| --- | --- |
| The screen on (`interactive`) | Once a second, on the host's thread (`Handler.postDelayed`). |
| Anything the watch says changed: the app in front, the screen on or off or unlocked, a call, the clock, the zone, a permission, the apps | At once, in place of the tick that was waiting, so a burst of changes gives one tick. Screen on and unlock come this way. |
| The screen off | No tick every second: nothing counts. The next tick at the engine's next check (`next_check_in_secs`), at most fifteen minutes away, on the host's thread while the phone is awake, and by an alarm that wakes it from sleep. |
| Always | An alarm at most fifteen minutes away, so a LiMo stopped from Android's list of running apps comes back. While the screen is on it is set again only when it comes within five minutes, about every ten minutes. |
| A way out, a pairing | At once, after it. |
| A failure | At the usual next tick; a minute when there was no engine to ask. |

**The alarm** is an ordinary one: `AlarmManager.setAndAllowWhileIdle` on
the elapsed clock (`ELAPSED_REALTIME_WAKEUP`), to `keep/AlarmReceiver` by a
pending intent only LiMo makes. It needs no permission, and Android lets it
through deep sleep now and then. It is not exact: Android may deliver it
late, by up to about three quarters of the wait. Nothing counts meanwhile,
and turning the screen on ticks at once, so a late alarm costs only a
notification's words being late. LiMo holds no exact alarm permission.

## What it does with the orders

In this order, at every tick:

1. **Keep the state**, when the orders say so (`save_state`), before
   anything else. Never otherwise.
2. **The cover** to the `Shower`, on the main thread, as a level.
3. **The time left** to the keeper (keep/), every tick; the keeper posts
   its notification again only when what it shows changed.
4. **A warning**, in the one tick it falls due, to the keeper (a
   notification: a heads up one, or a quiet one while show/'s card can
   float) and to the `Shower` (show/ floats it as a card over the app in
   front while the accessibility service is connected).
5. **The engine's notes**, each logged as the engine's JSON (`note
   {"note":"blocked_app_opened","package":"org.example.casino",...}`). They
   hold no sentence and no name.
6. **A report** is never sent: there is no link, so the facts never say the
   master is in reach and the engine never orders one. Were it ordered, one
   log line says so.
7. **The next tick**, as above.

## The child's name

The welcome grant holds no name, so the pairing hands the host the child's
name beside it (the development tool's pairing file now, `dev/README.md`).
The host keeps it in a small file of its own beside the engine's state,
the app's private `files/limo/child.json`, `{"child_name":"Asha"}`, written
all or nothing with `AtomicFile` (`ChildName`, `AtomicNameStore`), out of
backups like everything of the app. It is the one home of the name: the
child's screens (seam/) and the lock read it from `childName`.

- **Cleaned** before it is kept: control characters and direction marks
  dropped, trimmed, cut at 40 characters (never inside one); nothing left
  is no name, as the development tool cleans it.
- **Changed** only by `pair` (kept, or removed when the pairing gives
  none) and `forget` (removed), each told as `PAIRING`.
- **No name** while paired: the pairing gave none, its file could not be
  written, or it was lost or damaged (one that cannot be read is no name).
  The phone stays paired and keeps its limits; the screens then say "your
  child" (seam/README.md).
- **Never** in a log line (only `host paired no_child_name` says there is
  none), a report, a note or the engine's state.

## The kept state

One file, the app's private `files/limo/state`, written with Android's
`AtomicFile` (all or nothing; a write cut short leaves the old bytes). The
app keeps it out of backups and device transfers (`allowBackup="false"`
and data extraction rules that exclude everything). The bytes are the
engine's own; the host never reads them. Every byte array that held them,
or a welcome, a code secret, a code or a PIN, is filled with zeros after
use.

| Found at start | What the host does |
| --- | --- |
| No file | Not paired: no engine, no tick, no alarm, no watching (so no Android signals are held). It waits for `pair`. |
| A file the engine reads | The engine from it; the first tick decides, so a lock that was on is on again at once. |
| A file the engine cannot read (empty, not LiMo's, from a newer LiMo, cut short, changed) | Exactly what the engine's README says: the engine fails safe, a lock that says LiMo cannot check the time left, with no way out on the phone and calls, the diallers and LiMo still open. It never asks to be kept, and the host never writes over the file (an empty `save()` is never written), so a guardian can look at it and a newer LiMo finds it again. A guardian pairs the phone again, which replaces it. |
| A file Android cannot read (an I/O error) | The same, from no bytes at all: the phone is locked, the file left as it is. Read again at the next start of the process. |

## When something goes wrong

Nothing here can stop LiMo's process: every call on the host's thread, every
answer and every listener is guarded.

- **An engine that reports itself broken** (`EngineException.Broken`, after
  a panic inside Rust) or **a panic** (UniFFI's `InternalException`): the
  engine is closed and made again from the kept state, as the bridge's
  README asks, and the next tick comes a second later. During a way out the
  answer is `Failed`.
- **Any other failure** (a fact that could not be read, an engine call
  that threw, the library missing): logged, and the next tick tries again.
- **A state that cannot be written**: `saveFailed()` is called, and the
  engine asks again at the next tick.
- **A shower or a listener that throws**: logged; the next tick shows again.

## What it logs

Android's log, tag `LiMo` (`adb logcat -s LiMo`). A line names at most a
package and the kind of a failure; never a label, a name, anything typed,
a code, a PIN, nor an error's own words.

| Line | When |
| --- | --- |
| `host not_paired waiting` | Started with no state kept. |
| `host paired`, `host paired no_child_name`, `host pair_refused error=<kind>` | A pairing taken, with the child's name or without one, or refused by the engine. |
| `host forgotten` | The state was deleted. |
| `host state_unreadable kind=<empty, not_limo, newer, damaged> phone_locked` | The kept state could not be read. |
| `host engine_broken error=<kind> loading_again` | The engine was made again. |
| `host failed what=<what> error=<kind>` | Anything else that failed; at most once a minute for each. |
| `host report_ordered no_link` | Never expected before the link. |
| `note <the engine's JSON>` | Each of the engine's notes for the guardian. |
| `way_out <ask, code, pin> answer=<done, added, unlocked, wrong, must_wait, ...>` | Each way out, by the kind of its answer. |
| `host setup <finished, already, refused error=CannotCheck, not_paired, failed>` | Setup finished on the phone, or asked for again. |
| `cover now nothing`, `cover now block package=<package>`, `cover now lock why=<the engine's reason, such as time_used_up>`, `warning now cause=<time_runs_out, blocked_hours_start, unlock_ends> minutes=<minutes>` | The stand in shower: each cover as it changes, and each warning, while show/'s shower is not installed in the process. |

Once `keep/Keeper.start` has installed show/'s shower, the covers and the
warnings are show/'s to log, in its own lines (show/README.md, "What it
logs"): `show cover now block package=<package> call=<true, false>`, `show
cover now lock why=<the engine's reason> rows=<ways out shown>
tiles=<apps still open> call=<true, false>`, `show cover now nothing`,
`show warning floated` and the rest. `Keeper.start` installs the shower
before it starts the host, so a new process's first tick is already show/'s.
The stand in still speaks once in one case, in debug builds only: a pairing
that dev/ makes before the keeper has run in the process, since its
receiver pairs first and calls `Keeper.start` after. The shower is then
installed straight after, and the host ticks again at once, so the cover is
drawn then.

## Tests

`npm run test:child` (in the app) runs them with the module's other JVM
tests. They drive `HostCore`, the host without Android, against the REAL
engine through the bindings that ship (the PC library, loaded by JNA), with
the testkit's family playing the master PC for the welcome and the codes.
The phone is made up (`HostRig`): facts the test sets, the state file in
memory, a clock the test moves, and the host's tick and alarm run when they
fall due. It is set up at once (a guardian finishes setup as soon as it is
paired), but in `HostSetupTest`. `EngineSpy` wraps each engine to record
its orders and to fail on purpose.

| Test | What |
| --- | --- |
| `HostDayTest.aDayThroughTheHost` | Not paired: waiting, no tick, no alarm, no watching, nothing written, a way out answered `NotPaired`; paired: kept, told, watching; a minute in a game, a tick every second, the time left reaching the keeper, the alarm never more than fifteen minutes away; a blocked app told by the watch, ticked at once, its cover to the shower, its note logged by package and never by label; the three warnings to the keeper and the shower; the lock; a wrong PIN and a right code from LiMo's screen, each secret wiped, kept before the answer, the lock lifted at once; a request and the core's refusal in its words, the reason never logged; the view's JSON; the right PIN |
| `HostDayTest.theStateIsKeptExactlyWhenTheEngineSaysAndBeforeAnythingElse` | Through counting, a blocked app, the home screen and the screen off: one write for the pairing and one for each tick whose orders said so, no more and no fewer; in each such tick the write comes before the cover is handed out |
| `HostDayTest.aKilledProcessStartsAgainWithTheSameOrders` | Killed while counting: the new process loads what was kept and goes on counting, at most five seconds lost; killed while locked: the first tick of the new process gives the same cover and time left as the last of the old; an allowed app still usable |
| `HostTroubleTest.aKeptStateThatCannotBeReadLocksThePhoneAndIsNeverWrittenOver` | A damaged state: the lock that says LiMo cannot check, the dialler still open, never written over in a minute of ticks, no way out (the code wiped all the same), the view saying why; pairing again replaces it |
| `HostTroubleTest.aKeptStateAndroidCannotReadLocksThePhoneToo` | An I/O error reading it: locked, logged without the error's words, the file as it was |
| `HostTroubleTest.anEngineThatReportsItselfBrokenIsMadeAgainFromTheKeptState` | `Broken` at a tick, a panic at a tick, and `Broken` during a way out (answered `Failed`, the code wiped): each time the engine closed, loaded again from the kept state, counting on; another failure only logged, by its kind |
| `HostTroubleTest.aStateThatCannotBeWrittenIsAskedForAgain` | Writes refused: logged once, and written at the next tick once there is room |
| `HostSetupTest.untilSetupIsFinishedNothingIsCountedOrCoveredAndTheNotificationSaysSo` | Paired, not set up: a minute in a game and the blocked casino neither counted nor covered, no warning, the keeper given "Setup is not finished on this phone.", the view saying so; finished: kept before the answer, the view told, the casino covered at once, logged; counting from then on; finished again changing nothing and writing nothing; a new process on what was kept still finished |
| `HostSetupTest.setupCannotBeFinishedWhenNotPairedOrWhenTheKeptStateCannotBeRead` | Not paired; a state that cannot be read refused as `CannotCheck`, nothing written, logged by its kind |
| `HostTroubleTest.aPairingWhoseStateCannotBeKeptChangesNothing` | A pairing that cannot be written: refused, the secret wiped, the engine made for it closed, still not paired, nothing ticking, nothing told |
| `HostTroubleTest.withTheScreenOffNothingTicksEverySecondAndTheAlarmWakesTheHost` | The screen off: one tick at once, the time left paused, at most one tick in ten minutes, the alarm at most fifteen minutes away and firing, set again; the screen on: a tick at once and every second again |
| `HostTroubleTest.forgettingThePairingStopsEverything` | The state deleted, no tick, no alarm, no watching, nothing covered, the keeper cleared, told; a way out answered `NotPaired` |
| `HostNameTest` (6) | The name from a pairing cleaned and kept beside the state, read by a new process, removed by forgetting, told, never logged; a pairing with no name keeping none and removing an older one; a name file lost or damaged is no name, and a name that cannot be written leaves the phone paired; not paired, no name whatever the file holds; cleaning (direction marks, control characters, cut at 40 without splitting a character); the file read back exactly, anything else refused |
| `HostHouseRulesTest` (2) | No log line in host/, keep/ or dev/ interpolates a label, the child's name, a reason, a message, words, a title, a welcome, a secret, a code or a PIN; no dash punctuation in their files, READMEs, the module's words and manifests |

The keeper's look and the schedule are tested in `keep/KeeperLookTest`, the
test family's pairing file in `dev/DevPairingTest`.

## What only a phone can prove

For the owner, on the Galaxy S20 Ultra, with `adb logcat -s LiMo LiMoWatch`
and the debug build (`npm run build:child:debug`, paired with the
development tool):

1. **JNA and the engine on the phone.** After pairing the log has `host
   paired` (with dev/'s `dev pair taken`) and no `host failed` line, and the
   notification shows "30 min left" (or the test family's time). A `host
   failed what=engine_load error=UnsatisfiedLinkError` means the library
   did not load.
2. **Once a second, and quiet with the screen off.** With the screen on in
   a game, the notification's words change once a minute and the
   chronometer counts down between. With the screen off for ten minutes,
   no work: Android's battery page for LiMo should show next to nothing.
3. **At once on a change.** Open a blocked app: show/ logs `show cover now
   block package=<package> call=false`, then `show cover drawn
   ms=<milliseconds>`, within a few hundred milliseconds of the watch's
   `front now app package=<package>`.
4. **The alarm.** Screen off for twenty minutes, then `adb shell dumpsys
   alarm | findstr limo`: one alarm of LiMo's, about fifteen minutes ahead or
   at the engine's next check.
5. **The lock after a restart.** Use the day up, restart the phone: the log
   after unlocking shows the lock again at the first tick after the unlock
   (`show cover now lock why=time_used_up rows=<ways out shown>
   tiles=<apps still open> call=false`), before anything is opened.
6. **Killed.** Stop LiMo from the notification shade's list of running apps
   (Android 13 and later): it should come back by itself, by the
   accessibility service being connected again or by the alarm within
   fifteen minutes, with no `host failed` line in its new start; a lock that
   was on is drawn again (`show cover now lock ...`).
7. **An unreadable state.** Over the cable, with `adb shell run-as
   ai.lucidloom.limo.child` (the development tool has no command for it),
   put garbage in place of `files/limo/state` and restart LiMo: `host
   state_unreadable kind=not_limo phone_locked`, and the file unchanged
   after (`run-as ... cat files/limo/state`).
8. **How long a tick takes** on the phone (proof three measured 0.25 ms for
   one small record across the bridge); and how long `tryPin` takes, since
   the PIN is hashed slowly on purpose.

## Left open

- **The name over the network.** When pairing over the network is built,
  the master's welcome will carry the child's name, and the link hands it
  to `pair` as the development tool does now.
- **The link.** `inReach` and `prodded` are always false, and a report is
  never sent; the link adds the session, `outgoingReport`,
  `takeGrant`, `lostContact` and the wake ups the engine asks for in reach
  (at least once a minute).
- **Split screen with a blocked app beside the front** (the watch's
  `alsoVisible`): the host gives the engine the one front. Asking the engine
  about each app beside it, or the engine taking a list, is for the lead.
- **An unreadable file read again** only at the next start of the process.
  A file Android could not read for a moment stays locked until then.
- **The view is asked for, not pushed.** The screens ask again when told
  `VIEW`, which is at most once a second while time is counted (the words
  change once a minute, so in practice once a minute).

## Layout

```
host/
  README.md
  PhoneHost.kt        the interface for the other pieces: PhoneHost, Shower, WayOut, HostView, HostListener, HostChange
  HostCore.kt         the host with no Android in it, and the small interfaces it needs (state, facts, engines, clock, timer, alarm, keeper, log)
  ChildName.kt        the child's name: cleaned, its file's text, and where it is kept (NameStore)
  HostTimes.kt        when it ticks and sets its alarm, and its times
  Host.kt             the host of the process: its thread, AtomicFile for the state and the name, the watch's facts, the alarm, the main thread
  LimoChildModule.kt  the Expo module: starts the keeper when the app opens; hands the screens' requests and the change event to seam/
src/test/java/ai/lucidloom/limo/child/host/
  HostRig.kt          the made up phone, the state in memory, the engine watched
  HostDayTest.kt, HostTroubleTest.kt, HostNameTest.kt, HostSetupTest.kt, HostHouseRulesTest.kt
```
