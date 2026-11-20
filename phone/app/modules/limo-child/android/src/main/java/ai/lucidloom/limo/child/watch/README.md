# watch

The part of LiMo's child form that watches the phone: which app is in
front, and every other fact the phone's engine decides from (`PhoneFacts`
in `phone/engine`, whose README says what each fact means). Kotlin, package
`ai.lucidloom.limo.child.watch`, in the child module.

**What it is.** An accessibility service told when a window comes to the
front, by package name only; a reader of Android's own record of which
activity resumed (usage access); a small state machine that decides from
both which app is in front; and one place where all the facts are read,
each from its own Android call with its own fallback, and handed out as the
engine's `PhoneFacts`.

**What it is not.** It decides nothing about limits (the engine does),
draws nothing (show/ draws the covers, as this service's overlay windows),
calls no engine (host/ and keep/ do), keeps no file and opens no socket. It
has no thread of its own. Nothing here has run on a phone yet: see
[What only a phone can prove](#what-only-a-phone-can-prove).

## Contents

- [How the host calls it](#how-the-host-calls-it)
- [The facts](#the-facts)
- [Which app is in front](#which-app-is-in-front)
- [Calls, without the phone permission](#calls-without-the-phone-permission)
- [Permissions](#permissions)
- [What the watch reads, and nothing else](#what-the-watch-reads-and-nothing-else)
- [Signals](#signals)
- [What the manifest should gain](#what-the-manifest-should-gain)
- [Tests](#tests)
- [What only a phone can prove](#what-only-a-phone-can-prove)
- [Left open](#left-open)
- [Layout](#layout)

## How the host calls it

```kotlin
val watch = Watch.of(context)                         // one per process

// At once on every change, on the main thread:
val handle = watch.listen { tick() }

// And once a second while the screen is in use:
fun tick() {
    val orders = engine.tick(watch.factsNow(inReach = session?.isOpen() == true, prodded = prodded))
    // ... the engine's README says what to do with the orders
}

handle.close()                                        // when the host stops
```

| Call | What |
| --- | --- |
| `factsNow(inReach, prodded)` | The engine's `PhoneFacts`, read now. `inReach` and `prodded` are the link's, passed through. |
| `look(inReach, prodded)` | The same facts, with what the engine does not take: other apps still on the screen (`alsoVisible`), every permission, and why the front is unknown when it is. |
| `listen(listener)` | Told when something changed: the front, the screen, a call, the clock, the zone, a permission, the apps. Close the handle to stop. Android's signals are registered only while someone listens. |
| `launchableApps()` | The apps a child can open, each with its label, sorted by label, LiMo left out, each saying whether its time never counts (the home app, a dialler): the list a guardian picks blocked and allowed apps from. |
| `permissions()` | Every permission LiMo needs, read now. |
| `limoScreenShown(shown)` | From LiMo's own activities: `true` in `onResume`, `false` in `onPause`. |
| `emergencyDiallerOpening()` | From show/, just before the lock's emergency call opens the emergency dialler. |
| `WatchService.current` | The connected accessibility service, for show/ to add its covers as its overlay windows; null while Android has not connected it. |

**Threads.** Any call from any thread; they run one at a time. `factsNow`,
`look` and `permissions` cost a handful of binder calls, a few milliseconds
in all: the main thread is fine. `launchableApps` loads every label and may
take a few hundred milliseconds: call it from a thread the host owns. A
listener is called on the main thread, never while the watch holds its
lock, so it may call `factsNow` at once.

**Never throws.** A reading Android refuses becomes that fact's fallback
and a log line. Every callback from Android (the service, the receivers,
the listeners) is guarded, so nothing here can stop the service.

## The facts

All read in one place, `WatchCore.look`, at every call. A reading that
fails gives **the last good reading of the same fact in this process**, or,
with none, the default below, and one log line a minute at most.

| Fact | Android's call | Default with no good reading |
| --- | --- | --- |
| `wallSecs` | `System.currentTimeMillis() / 1000`, rounded down | 0 (it does not fail) |
| `elapsedMs` | `SystemClock.elapsedRealtime()` | 0 (it does not fail) |
| `bootCount` | `Settings.Global.BOOT_COUNT` | 0 |
| `utcOffsetSecs` | `TimeZone.getDefault().getOffset(now)`; Android updates the default zone itself when it changes | 0 |
| `hours24` | `DateFormat.is24HourFormat` | false |
| `autoTime` | `Settings.Global.AUTO_TIME` | false: with automatic time unknown, a clock set by hand buys nothing |
| `interactive` | `PowerManager.isInteractive` | true |
| `keyguardLocked` | `KeyguardManager.isKeyguardLocked` | false |
| `call` | the audio mode, and on Android 8 to 11 the telephony call state: see [Calls](#calls-without-the-phone-permission) | idle |
| `front` | the state machine: see [Which app is in front](#which-app-is-in-front) | unknown |
| `front.label` | `PackageManager.getApplicationLabel`, kept for each package until apps change | none |
| `permissions.notifications` | `NotificationManager.areNotificationsEnabled` | false |
| `permissions.phone` | `checkSelfPermission(READ_PHONE_STATE)` | false |
| `permissions.usageAccess` | `AppOpsManager`, `OPSTR_GET_USAGE_STATS` | false |
| `permissions.accessibility` | see [Permissions](#permissions) | false |
| `permissions.battery` | `PowerManager.isIgnoringBatteryOptimizations` | false |
| `permissions.protection` | `DevicePolicyManager.isAdminActive` | false |
| `inReach`, `prodded` | the link's, passed through | |

Within one process the boot count cannot change (a restart of the phone
ends the process), so the last good reading is always the right one. Only
a new process whose very first reading fails says 0; the engine then notes
a restart, which is all it costs.

## Which app is in front

**Accessibility events propose, usage events confirm.** The state machine
is `FrontWatch`, Android's record is followed by `UsageTrack`; both are pure
Kotlin, tested on the PC.

1. **A window event proposes its package.** The service asks for window
   state changes only and reads the package name of each, nothing else.
   Events from LiMo itself (its covers are its own windows), from a keyboard
   and from the framework's own overlays propose nothing.
2. **The front is the app whose activity resumed last** in Android's usage
   events. A proposal is never taken as it is: it asks for usage to be read
   at once, and again at 100, 300 and 1,000 ms, until usage shows that
   package resumed. One not confirmed within a second is dropped, with a log
   line. A window that is not an activity (a floating bubble, a picture in
   picture window, a keyboard, the shade, LiMo's cover) never resumes one,
   so it never becomes the front.
3. **Proof two's flaw is cured by rule 2.** On the Galaxy, after the cover's
   "Go home", the blocked app sent one more window event after the home
   screen's, and the cover came back over the home screen in 12 of 14 tries.
   Here that late event proposes the blocked app, usage shows the home
   screen resumed after it, and the home screen stays in front.
4. **Usage is also read whenever the facts are**, at most twice a second,
   so an app that came up while the service missed it (or was off) is still
   found within a second.
5. **The framework's overlays never become the front**, by window or by
   activity: System UI (the shade, quick settings, the volume panel, the
   power menu, the lock screen, and on older phones the list of recent
   apps), Android itself (its dialogs, the app chooser), the chooser of
   Android 14 and later, and the permission controller (the dialog an app
   opens to ask for a permission). While one is up, the app under it stays
   in front. These five package names are Android's own, the same on every
   maker; no maker's package name is held anywhere here.
6. **What kind of screen**, in this order: LiMo's package is LiMo; the home
   app (`resolveActivity` on the home intent; with none chosen, every
   preinstalled one) is the home screen, the list of recent apps among it
   on most phones; the default dialler and the system's dialler
   (`TelecomManager`), a preinstalled app with an in call service that
   draws a screen (`InCallService` with `IN_CALL_SERVICE_UI`), or a call
   screen learned (rule 8) is the dialler; a preinstalled app that handles
   the emergency dial actions, or one learned (rule 9), is the emergency
   dialler; during a call, a package with no icon to open it is the call's
   own screen, so the dialler; anything else is an app, with its label.
7. **A call left open is no way around the limits.** During a call an app
   with an icon is an app, exactly as with no call. Only diallers Android
   names, and screens with no icon, are the dialler; a third party app that
   merely says it can dial is an app.
8. **The call's own screen is learned** where Android does not name it (on
   some makers it is another package than the dialler, with no icon): a
   package with no icon that comes to the front within five seconds of a
   call starting or being answered is the dialler from then on, call or no
   call, so a call that later rings silently is never covered.
9. **The emergency dialler is learned the same way**: the first package with
   no icon to come to the front within ten seconds of
   `emergencyDiallerOpening()`. An app with an icon is never learned.
10. **Usage access off: unknown, never a guess.** With one exception,
    because a lock must never stand between a child and a call: a window
    event from the dialler or the emergency dialler still says so. That can
    only take a cover away, never put one up. The same holds while usage has
    shown nothing yet (just after the phone started). LiMo's own screens say
    so through `limoScreenShown`, which is a fact, with usage access off too.
11. **Split screen and picture in picture, as far as events can tell.** In
    split screen both halves are resumed; the one that resumed last is the
    front, the other is in `look().alsoVisible`. An app gone into picture in
    picture is paused and not stopped: the home screen (or whatever came up)
    is the front, and the small window is in `alsoVisible`. Only Android 10
    and later report an activity stopped, so before that only resumed apps
    can be told.
12. **The clock.** Android moves its stored usage events when the clock is
    set, so after `TIME_SET` (or a read that finds the clock went back past
    the last one) the record is read again from six hours back.

**What changed from proof two.** Proof two took every window event from any
package but its own, System UI, a keyboard and a few overlays it guessed for
the Galaxy, and treated every dialler on the phone as never covered. Here a
window event is only a proposal, no maker's name is listed, `android` (the
chooser, system dialogs) is an overlay rather than an app coming to the
front, and only the diallers Android names are diallers, since any app may
say it handles the dial action.

## Calls, without the phone permission

What Android gives an app that holds no phone permission:

- **Android 8 to 11** (API 26 to 30): the telephony call state
  (`TelephonyManager.getCallState`, and `listen` with `LISTEN_CALL_STATE`)
  needs no permission there. It says ringing and off the hook for calls on
  the SIM, a silent ring among them.
- **Android 12 and later**, for an app aimed at them as LiMo is: the
  telephony call state, its listeners and `TelecomManager.isInCall` all ask
  for `READ_PHONE_STATE`. What is left is **the audio mode**
  (`AudioManager.getMode`, and `addOnModeChangedListener`): Android's
  telecom service sets "ringtone" while a call rings aloud, "in call" while
  a call is dialled, on or held, and "call screening" while a screening
  service answers it.
- **Internet calls** ("in communication", such as a voice chat in a game) are
  not calls here, as in proof two: a game's chat must never keep a cover
  away.

**Chosen:** the audio mode on every Android, and the telephony call state as
well on Android 8 to 11; a ring in either rings, then a call on in either is
on. No permission is added. **The gap:** on Android 12 and later a call that
rings with the ringer silenced may leave the audio mode as it was, so it reads
as idle; a cover drawn then could hide its heads up notification. An answered
call is seen at once ("in call"), and a call screen that comes up full screen
is never covered (rule 6). Closing the gap needs `READ_PHONE_STATE` (the
"Phone" permission, asked at setup) and `TelephonyCallback.CallStateListener`.
The owner decided on 20 Nov 2026 that setup asks for it. So far the watch
only reads whether it is granted (`permissions.phone`, false while the
manifest does not ask for it) and hands that to the engine, which tells the
child a guardian is needed while it is off; how a call is read is as above
until the piece that adds the permission to the manifest uses it.

## Permissions

`permissions()` and `look().permissions`, in the order setup asks for them:

| Permission | Android's call |
| --- | --- |
| `notifications` | `NotificationManager.areNotificationsEnabled` |
| `phone` | `checkSelfPermission(READ_PHONE_STATE)` is granted: false while it is not, and always while the manifest does not ask for it |
| `usageAccess` | `AppOpsManager.unsafeCheckOpNoThrow(OPSTR_GET_USAGE_STATS)` (before Android 10, `checkOpNoThrow`); "default" falls back to the permission itself |
| `accessibility` | `Settings.Secure.ACCESSIBILITY_ENABLED` is on and `ENABLED_ACCESSIBILITY_SERVICES` names `WatchService`, in full or short form |
| `accessibilityRunning` | the service is connected now (`onServiceConnected` without `onUnbind` since) |
| `batteryExempt` | `PowerManager.isIgnoringBatteryOptimizations` |
| `deviceAdmin` | `DevicePolicyManager.isAdminActive` for `keep/AdminReceiver` |

The engine takes six, in the same order: `notifications`, `phone`,
`usageAccess`, `accessibility` (see below), `battery` (`batteryExempt`) and
`protection` (`deviceAdmin`). Android tells nothing when the Phone
permission changes in Settings (the app's process is stopped when it is
taken away), so it is read again with the others every five seconds.

They are read again every five seconds, at once when Android says one
changed, and whenever usage tells a different story from the last reading.
**The engine's `accessibility`** is on when the service is switched on and
connected, or switched on and the watch started less than a minute ago:
Android connects the service a moment after LiMo's process starts, and a
guardian should not be told it was switched off at every restart. A force
stop switches the service off in Settings (proof two saw it), so that is
still seen.

## What the watch reads, and nothing else

Of an accessibility event: its type and its package name. Of a usage event:
its kind (resumed, paused, stopped, the phone started), its package and its
time; never the activity's class name. Of an app: its package name and, by
that name, its label. Of a call: whether one rings or is on; never a number,
never who, never how long. Nothing on the screen, no window's title or text:
the service's settings ask Android for no window content, so it has none to
give. Log lines (tag `LiMoWatch`) name at most a package, a fact and the
kind of a failure; never a label, nor an exception's message.

## Signals

While someone listens (`listen`), and only then:

| Signal | Android's call | Tells |
| --- | --- | --- |
| the screen on, off, unlocked | receiver: `SCREEN_ON`, `SCREEN_OFF`, `USER_PRESENT` | `SCREEN` |
| a call, Android 12 and later | `AudioManager.addOnModeChangedListener` | `CALL` |
| a call, Android 8 to 11 | `TelephonyManager.listen(LISTEN_CALL_STATE)` | `CALL` |
| apps installed, removed, changed; the default dialler | receiver: `PACKAGE_ADDED`, `PACKAGE_REMOVED`, `PACKAGE_CHANGED`, `PACKAGE_REPLACED`, `ACTION_DEFAULT_DIALER_CHANGED` | `APPS` |
| keyboards | receiver: `INPUT_METHOD_CHANGED` | nothing; the roles are asked again |
| usage access | `AppOpsManager.startWatchingMode` | `PERMISSIONS` |
| the accessibility setting | a content observer on `ENABLED_ACCESSIBILITY_SERVICES` | `PERMISSIONS` |

Always, from the manifest: `ClockReceiver` (`TIME_SET`, also sent when the
12 or 24 hour setting changes, and `TIMEZONE_CHANGED`) tells `CLOCK` or
`TIME_ZONE`; the service's connecting and stopping tell `PERMISSIONS`; a
change of the front found by a window event or a recheck tells `FRONT`. A
change found while the host itself reads the facts is in what it read, and
is not told again.

## What the manifest should gain

Not edited here (the manifest is not this piece's). On Android 11 and later
LiMo sees only the packages its `<queries>` name, and two kinds of screen
are found only through intents it does not name yet:

```xml
<!-- The screen of a call, where it is another package than the dialler. -->
<intent>
    <action android:name="android.telecom.InCallService" />
</intent>
<!-- The emergency dialler. -->
<intent>
    <action android:name="android.intent.action.DIAL_EMERGENCY" />
</intent>
<intent>
    <action android:name="com.android.phone.EmergencyDialer.DIAL" />
</intent>
```

Without them the watch still finds both by rules 8 and 9 (learned from what
comes to the front), but only after the first call, and only after LiMo
opened the emergency dialler once. These queries need no permission and no
dependency. The Phone permission the owner decided on
(`android.permission.READ_PHONE_STATE`) is for the piece that reads calls
with it; until the manifest asks for it, `permissions.phone` reads false.
The accessibility service's description in `res/values/limo_strings.xml`
still says that this early build does nothing yet.

## Tests

`npm run test:child` (in the app) runs them with the module's other JVM
tests. They run on the PC with no Android at all: `TestPhone` is a
`PhoneSource` whose every reading the test sets, which can refuse any of
them, and whose usage record the test writes as Android does, holding events
back to write them late.

| Where | What | Count |
| --- | --- | --- |
| `FrontTest` | An app in front once its activity resumed and its window said so; proof two's late event after "Go home", fourteen tries, the home screen in front throughout, the event dropped after three rechecks; the home screen's resume written late with the late event first; an event usage has not written yet, confirmed by a recheck; a floating window never in front; the shade, system dialogs, keyboards and LiMo's cover proposing nothing, older phones' recent apps and a permission dialog leaving the app in front; LiMo's activity and its cover; LiMo's word for its screen with usage access off, and a missed word put right; the dialler, a call's own screen learned and still the dialler after the call, a game during a call still a game; a screen with no icon the call's only during a call and learned only as it starts; an in call screen Android names; the emergency dialler named, learned, never learned for an app with an icon or after ten seconds; usage access off: unknown, a dialler still saying so, LiMo's covers changing nothing, back on at once; split screen; picture in picture, and Android 9 and older; an app with no label, a blank one, an unseen one, one that cannot be read; the clock set back, told or not; a restart of the phone in the record; events read twice near the edge, written late, the clock gone back | 19 |
| `FactsTest` | Every fact from its reading in one place; each fact's fallback, its last good reading and then its default; every reading refused at once, logged once each, and the facts back when Android answers; a refusal logged once a minute without its message or a label; the call state from the audio mode and the telephony state, against Android's own numbers, a silent ring on Android 8 to 11; usage event kinds against Android's numbers; the accessibility permission switched on, connected, starting; the permissions read again after five seconds or when told; the accessibility setting in both forms; the launchable apps sorted, LiMo left out, the dialler never counted; labels kept and forgotten; the roles asked again and kept when refused; usage that cannot be read; the Phone permission as Android grants it, each of the six reaching the engine's own field, off when it cannot be read | 14 |
| `HouseRulesTest` | No source here reads a window's content, its text, a class name or changes the service's settings at run time, and a caller's number is never used; no dash punctuation in a comment or this README | 2 |

All 35 pass, with the module's 8 tests of the engine and the link (43 in
all), in well under a second of test time.

## What only a phone can prove

For the owner, on the Galaxy S20 Ultra (Android 13, One UI 5.1), with
`adb logcat -s LiMoWatch`:

1. **Proof two's flaw, gone.** Block an app that sends a late window event
   (YouTube was the one), open it, let the cover come, tap "Go home"; repeat
   14 times. The home screen must never be covered again. The log should
   show `front now home` and then, for the late event,
   `front window_event package=com.google.android.youtube not_confirmed`.
2. **How long usage takes to confirm.** The cover's delay over proof two's
   25 to 226 ms. If a `not_confirmed` line ever names the app that really came
   up, usage was later than a second, and the rechecks need stretching.
3. **The call screen on One UI.** Dial a number from the Phone app and look
   for `front learned call_screen package=...`, and that the call's screen
   reads as the dialler (`front now dialler`). Then answer an incoming call
   while a game is in front. With the manifest's `InCallService` query
   added, the call screen should be named before any call.
4. **A silent ring on Android 13.** Ringer off (and vibration off), a game
   in front, call the phone: the log should say `call now ringing`. If it
   says nothing until the call is answered (`call now ongoing`), that is the
   gap in [Calls](#calls-without-the-phone-permission). Then the same with
   the ringer on, to see the ordinary case.
5. **The emergency dialler.** Which package opens for an emergency call
   from LiMo's lock, and whether it is found from the emergency dial actions
   (with the manifest's queries) or learned (`front learned
   emergency_dialler`).
6. **One UI's recent apps, pop up view and split screen.** Recents should
   read as `front now home` (the launcher). A pop up window and split screen
   should give the last resumed app as the front and the other in
   `alsoVisible`.
7. **Overlays.** The notification shade, the Edge panel, the screenshot
   toolbar, Game Booster and the keyboard: none should change the front.
8. **Usage events of apps LiMo cannot see.** Open an app with no icon (a
   settings page of another package, the in call screen): its usage events
   should still arrive, with its package. Android 11's package visibility is
   not known to filter usage events; this checks it.
9. **The accessibility permission after a restart.** Restart the phone; the
   guardian must not be told the service was switched off. Force stop LiMo
   in Settings; it must be (Android switches the service off then).
10. **Battery.** The host reads the facts once a second while the screen is
    on: about five binder calls each time. Worth a look over a day.

## Left open

- **Split screen with a blocked app in the other half.** The engine takes one
  front; `alsoVisible` says what else shows. Covering for an app beside the
  front needs the host to ask the engine about each, or the engine to take
  the others: for P8 and P9.
- **A third party home app** is the home screen, never counted. One with a
  web search or a feed inside would be uncounted use. Counting only
  preinstalled home apps would close it and count the home screen of a
  family that chose another launcher: the lead's call.
- **A default dialler a child chose** is never covered; only an app that is
  really a dialler can be made the default, so this is narrow.
- **An app that sets the audio mode itself.** Android lets an app with
  `MODIFY_AUDIO_SETTINGS` set some modes, and keeps "in call" for the
  phone's own apps; an app that could set "ringtone" would read as a call
  ringing, which covers nothing and counts nothing. Whether Android 13 lets
  it is for a phone to show; a time limit on how long a ring may last would
  close it.
- **"Back to the call"** (show/): `TelecomManager.showInCallScreen` needs
  `READ_PHONE_STATE`. Without it, opening the default dialler is the way
  back on most phones.
- **The labels** are the app's own, cleaned by the engine before it shows
  them; the list for the guardian is passed as Android gives it.

## Layout

```
watch/
  README.md
  PhoneWatch.kt      the host's interface and its types: WatchLook, LimoPermissions, LaunchableApp, WatchChange
  Watch.kt           the one watch of the process: the lock, the listeners, the rechecks on the main thread
  WatchCore.kt       the decisions, pure: every fact and its fallback, the one place PhoneFacts is made
  FrontWatch.kt      which app is in front, pure: proposals, confirmation, the kinds, learning
  UsageTrack.kt      Android's record of activities, pure: the front, what is still visible, reading in pieces
  Roles.kt           the roles of packages, the framework's overlays, the times and limits
  Readings.kt        PhoneSource (every reading), usage kinds, the call state rules, the accessibility setting
  AndroidPhone.kt    PhoneSource from Android's own calls
  WatchSignals.kt    Android's signals while someone listens
  WatchService.kt    the accessibility service
  ClockReceiver.kt   the clock and the zone
src/test/java/ai/lucidloom/limo/child/watch/
  TestPhone.kt       the phone the tests drive, and the rig
  FrontTest.kt, FactsTest.kt, HouseRulesTest.kt
```
