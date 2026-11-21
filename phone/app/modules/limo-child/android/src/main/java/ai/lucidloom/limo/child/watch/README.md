# watch

The part of LiMo's child form that watches the phone: which app is in
front, and every other fact the phone's engine decides from (`PhoneFacts`
in `phone/engine`, whose README says what each fact means). Kotlin, package
`ai.lucidloom.limo.child.watch`, in the child module.

**What it is.** An accessibility service told when a window comes to the
front, by package name (and, for Android's Settings, the package installer
and the package Android names for the device admin's page only, by the
class name of the window, matched and dropped); a reader of Android's own
record of which activity resumed (usage access; for Settings and that same
package only, the class of the activity, matched and dropped); a small
state machine that decides from both which app is in front, and which page
that could switch LiMo off or remove it is in front (the guard, which asks
the package manager which activities those pages are); and one place where
all the facts are read, each from its own Android call with its own
fallback, and handed out as the engine's `PhoneFacts`.

**What it is not.** It decides nothing about limits (the engine does),
draws nothing (show/ draws the covers, as this service's overlay windows),
calls no engine (host/ and keep/ do), keeps no file and opens no socket. It
has no thread of its own. It first ran on a phone on 20 November 2026: see
[The first run](#the-first-run), and [What only a phone can
prove](#what-only-a-phone-can-prove) for what is still to see.

## Contents

- [How the host calls it](#how-the-host-calls-it)
- [The facts](#the-facts)
- [Which app is in front](#which-app-is-in-front)
- [The guard](#the-guard)
- [Calls](#calls)
- [Permissions](#permissions)
- [What the watch reads, and nothing else](#what-the-watch-reads-and-nothing-else)
- [What it logs](#what-it-logs)
- [Signals](#signals)
- [What the manifest gives it](#what-the-manifest-gives-it)
- [Tests](#tests)
- [The first run](#the-first-run)
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
| `look(inReach, prodded)` | The same facts, with what the engine does not take: other apps still on the screen (`alsoVisible`), every permission, why the front is unknown when it is, and how far the guard reaches on this phone (`guardScope`: whether Android named the device admin's page, and where the accessibility settings open; see [The guard](#the-guard)). |
| `listen(listener)` | Told when something changed: the front, the screen, a call, the clock, the zone, a permission, the apps. Close the handle to stop. Android's signals are registered only while someone listens. |
| `launchableApps()` | The apps a child can open, each with its label, sorted by label, LiMo left out, each saying whether its time never counts (the home app, a dialler): the list a guardian picks blocked and allowed apps from. |
| `permissions()` | Every permission LiMo needs, read now. |
| `limoScreenShown(shown)` | From LiMo's own screens: `true` when one comes to the front, `false` when it leaves. The ways out screen says so in its `onResume` and `onPause`; the main activity through the seam (`seam/OwnScreen`), which also says `false` while LiMo shares the screen in split screen or a floating window. The watch keeps one word for all of them, the last given. |
| `emergencyDiallerOpening()` | From show/, just before the lock's emergency call opens the emergency dialler. It reads usage at once (a few milliseconds), so the next screen to come up is the first after the tap (rule 9). |
| `deviceAdminOffAsked()` | From keep/AdminReceiver, when Android asks whether LiMo's device admin may be switched off: raises the guard at once for what is in front and tells `FRONT` (see [The guard](#the-guard)). |
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
| `call` | the audio mode, and the telephony call state where Android tells it (Android 8 to 11; 12 and later with the Phone permission): see [Calls](#calls) | idle |
| `front` | the state machine: see [Which app is in front](#which-app-is-in-front) | unknown |
| `guarded` | the guard: see [The guard](#the-guard) | none |
| the guard's reach | `PackageManager.resolveActivity` and `queryIntentActivities` for three intents: see [The guard](#the-guard) | each intent's last answer in this process; with none, the fixed lists alone |
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
   maker; the only other names held here are the guard's, in one file
   (`Guarded.kt`), each with its reason (see [The guard](#the-guard)).
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
8. **The call's own screen is learned from the first screen after the call
   rings, is dialled or is answered, and from nothing else**, where Android
   does not name it (on some makers it is another package than the dialler,
   with no icon). When the watch sees the call change, it reads usage at
   once, so what was in front before is taken first; then it waits for the
   first activity to come to the front. That first one decides and ends the
   wait, whatever it is: it is learned as the call's screen only when it
   comes within five seconds and is unexplained (none of the known kinds,
   and no icon); the dialler, the home screen, LiMo or an app with an icon
   end the wait with nothing learned, and nothing after them can be
   learned. Five seconds with nothing new in front end it too, and so do
   the call ending, the record read again from the start and a read of
   usage that fails; the wait never starts while usage cannot be read. A
   screen learned is the dialler from then on, call or no call, so a call
   that later rings silently is never covered; like rule 9's, it is held in
   memory only, at most eight. A call's screen that came up before the
   watch saw the call is not learned, though it is the dialler while the
   call lasts (rule 6). This rule has only run on the PC: no call could be
   made on the first run's phone.
9. **The emergency dialler is learned from the first screen after Emergency
   call, and from nothing else.** `emergencyDiallerOpening()` reads usage at
   once, so what was in front before the tap is taken first (LiMo's own
   screen among it, whose window events propose nothing); then the watch
   waits for the first activity to come to the front in Android's record,
   overlays aside. That first one decides and ends the wait, whatever it is:
   it is learned as the emergency dialler only when it comes within ten
   seconds and is unexplained (none of the known kinds, and no icon to open
   it); the emergency dialler Android names, the dialler, the home screen,
   LiMo or an app with an icon end the wait with nothing learned, since the
   person went somewhere else, and nothing after them can be learned. Ten
   seconds with nothing new in front end it too, and so do the record read
   again from the start (the clock was set) and a read of usage that fails.
   The wait never starts while usage cannot be read, nor where Android names
   no dialler and no emergency dialler (a tablet with no calls, where
   Emergency call opens nothing, so the first screen after it would be the
   child's choice). This still learns what the rule is for, an emergency
   dialler drawn by a package Android does not name, because that is the
   screen Emergency call brings up; one already in front under a cover is
   learned when Emergency call brings it up again. What is learned is held
   in memory only, for the life of LiMo's process, at most eight packages:
   no file keeps it, so LiMo or the phone starting again forgets it.
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

## The guard

A child must not be able to switch LiMo off or remove it through Android's
settings. The engine covers such a page with the guard, which offers the
guardian PIN only (phone/engine/README.md, "The guard"); the watch says
which one is in front, in the fact `guarded` (`DEVICE_ADMIN` or
`ACCESSIBILITY`), and nothing at all when it cannot tell. Everything it
recognises is decided in one place, `Guarded.kt`: a few fixed names, each
with its reason and whether it was seen on a phone, and what the package
manager answers on the phone itself. Four ways, from the surest to the
least sure:

| How | What | Recognised by | Seen on a phone |
| --- | --- | --- | --- |
| By event | LiMo's device admin asked to switch off (Deactivate on its page) | Android calls keep/AdminReceiver's `onDisableRequested`, which calls `deviceAdminOffAsked()` | not yet |
| By Android's answer | the device admin's page, whatever its maker named it; accessibility settings in a package of their own, on any maker | the package manager, asked which activity opens `ACTION_ADD_DEVICE_ADMIN` and `ACTION_ACCESSIBILITY_SETTINGS` (below): the page then by its package and class, the accessibility settings' package whole | not yet |
| By package | `com.samsung.accessibility`: One UI's accessibility settings, where LiMo's accessibility service is switched off; kept for a phone whose package manager cannot be asked | the package in front, as usage shows it | the package, yes (the first run, 20 November 2026, Galaxy S20 Ultra, One UI 5.1); under the guard, not yet |
| By the page's name | the device admin's page in `com.android.settings`: `com.android.settings.DeviceAdminAdd` (Android 8.0 to 9) and `com.android.settings.applications.specialaccess.deviceadmin.DeviceAdminAdd` (Android 10 to 15); kept beside Android's answer | its class (below) | no: from Android's source only |

A page known by its class, by Android's answer or by the fixed name, is
told two ways: by the class of its window, which the accessibility event
carries (`Activity` sets it to its own class), and by the class of the
activity that resumed, which Android's usage record carries. The second
needs no window event at all. Android gives that class to any app with
usage access, hiding it only for instant apps (frameworks/base,
`UsageEvents` and `UsageStatsService`, Android 13 and 15), and for an alias
it is the class the alias opens (`ActivityRecord`, `mActivityComponent`),
the same class the window carries.

**Why the page itself as well as the event.** Android's source (Settings,
`DeviceAdminAdd`, the same from Android 8.0 to 15) asks the admin first
only on the way through Deactivate. App info's Uninstall, for an app that
is an active device admin, opens the same page in a mode whose one button
switches the admin off and removes the app in one call
(`uninstallPackageWithActiveAdmins`), with no word to the admin's receiver
first. Only the page itself catches that way, and it also covers the page
before anything is tapped.

**Asking Android which page it is.** At the first need in LiMo's process
the watch asks the package manager about three intents
(`AndroidPhone.answerFor`): `resolveActivity` for the activity each opens,
and `queryIntentActivities` for every activity that handles it. Nothing on
any screen is looked at.

| Intent | What the answer gives the guard |
| --- | --- |
| `DevicePolicyManager.ACTION_ADD_DEVICE_ADMIN` | The device admin's page: every preinstalled activity that handles it, by its package and its class, and for an alias the class it opens, which is what its window and its usage events carry. That package joins the packages whose class names are read. |
| `Settings.ACTION_ACCESSIBILITY_SETTINGS` | Where the accessibility settings open: the preinstalled activity it opens, or, where it opens Android's chooser or an app a person chose, every preinstalled one that handles it. A package that is not Android's Settings is guarded whole, as One UI's is; one inside Settings is said as it is (below). |
| `Settings.ACTION_SETTINGS` | Android's Settings app on this phone, beside `com.android.settings`: never guarded whole, so that a maker's whole Settings app is never covered as its accessibility settings. |

Only preinstalled apps are believed. Anyone can install an app that says
it handles these intents; a child's own app must never make LiMo read its
class names, nor be taken for a guarded page. Android's chooser, the
framework's own, is never taken either. A class that also opens Settings'
first page or its accessibility settings is never taken for the device
admin's page: it would hold many pages, and the guard must not cover
nearly all of Settings. With no answer for `ACTION_SETTINGS`, no package is
guarded whole by Android's answer, only by the fixed list.

**How long the answer is kept.** In memory, for the life of LiMo's
process, and asked again when an app is installed, removed, changed or
updated (the watch's `APPS` signals, `appsChanged`), since an update of
Settings may rename its pages. A refused answer keeps the last good one of
the same intent and is asked again a minute later; with none, the fixed
lists hold. Nothing of it is kept in a file: Android answers at once
whenever LiMo starts, and a new process asks before its first read of
Android's record, so a file would add only something to keep right. The
log says what the guard reaches, once at the start and again when it
changes (`guard pages`, [What it logs](#what-it-logs)), by package only;
`look().guardScope` says it to the host, also without a class.

**What Android's source says the answer covers.** In Android's own
Settings one activity, `DeviceAdminAdd`, is the device admin's page for
all three ways to it: `ACTION_ADD_DEVICE_ADMIN` reaches it through its
intent filter, and both a row of the list of device admin apps and App
info's Uninstall for an active admin open it by its class from Settings'
own code (`DeviceAdminListItem.getLaunchIntent` on Android 13 and
`DeviceAdminSettings.onListItemClick` on Android 9;
`AppButtonsPreferenceController` on Android 13 and `InstalledAppDetails` on
Android 8.0). Only the first is an intent
Android can be asked about; the other two name a class inside Settings. So
the answer names the page the list and Uninstall open wherever a maker kept
that one activity, as Android's source does. A maker whose list or
Uninstall opens another activity is not covered by the answer on those
ways; only the probe on its phone can tell (dev/README.md, "The page
probe").

**The accessibility settings, as Android answers.** On One UI the action
opens a package of its own (on the first run's phone, setup's
Accessibility step opened `com.samsung.accessibility`): guarded whole, by
the answer, on any maker that does the same. On Android's own source the
action opens an activity of Settings (`Settings$AccessibilitySettingsActivity`),
but Settings' own first page opens the same list as a fragment in
`com.android.settings.SubSettings`, and LiMo's own page, where its switch
is, is always such a fragment. Guarding the activity the action opens would
cover one way to the list and none to the switch, while making LiMo's own
setup step look guarded; so it is not guarded, and the watch says so
instead: `guardScope.accessibility` is `IN_SETTINGS`, and the log says
`guard pages accessibility=in_settings package=com.android.settings`. What
a child can do there is in [Left open](#left-open).

**The rule the watch keeps (changed on purpose, narrowly).** A class name
is read from Android in two places, each only after asking the guard's
reach (`GuardReach`) that the package is one it reads:

- the class of a window, in the accessibility service, for Android's
  Settings (`com.android.settings`), the package installer
  (`com.android.packageinstaller`, `com.google.android.packageinstaller`)
  and the package the package manager named for the device admin's page;
- the class of a resumed activity, in Android's usage record
  (`AndroidPhone.usageEvents`), for Settings and that same package only.

Each is only matched against the guarded pages (the fixed list, and the
activities the package manager named for the device admin's page), then
dropped: a usage event keeps the page it matched, never the class. It is
never read for any other app, never logged in a release build, never kept
beyond what the package manager answered, and never sent. That answer
names activities by their class too: it is held in memory as the guard's
reach, and nowhere else. A debug build can log all of these while a person
at the PC switches a probe on (dev/README.md, "The page probe"); a release
build holds no trace of that probe (`npm run inspect`). Nothing else
changes: no text, no content description, no node tree, no window content
of any app. `HouseRulesTest` holds the code to this.

**How long a page stays guarded.** By Android's record first: while usage
shows that the activity in front is a guarded page by its class, that page
is guarded (`by=usage` in the log), whatever told the service or did not.
That guards it at a new process's first look, and whenever it comes back to
the front (from the recent apps, after the lock screen, after the PIN on
LiMo's own screen) with no window event; the host's next look finds it,
within a second of Android writing its resume. Beside that, a page known by
its window or by the event raises the guard for the package then in front
(Settings). It holds while that package's activity in front is the one that
was in front when it was raised, as Android's usage record tells:

- a dialog over the page (Android's own "Deactivate this device admin
  app?" among them) resumes nothing, so the page stays guarded;
- another page of the same app resumes an activity, and the guard ends;
  so do the home screen and any other app coming to the front;
- LiMo's own screens (where the guard's PIN is typed), the dialler and the
  emergency dialler set it aside, and the same page coming back after them
  is guarded again, whether or not its window tells anything again;
- so does the phone put down, its screen off or Android's lock screen up:
  the page waits under it, paused, and its first resume once the phone is
  unlocked is the same page coming back, guarded again whether or not its
  window tells anything; an app opened over the lock screen meanwhile (its
  camera) does not end it, and another page or app after the unlock ends it
  as before. The watch reads the screen and the lock screen at every look,
  and at once on Android's screen signals, so it knows even when the host
  did not look while the screen was off. The engine closes a PIN's five
  minutes at that same moment (phone/engine/README.md, rule 12), so a phone
  handed back with its screen off comes back guarded. This is the watch's
  to keep, not the engine's: only Android's record tells the same page
  coming back (its first resume after the phone was put down) from another
  page of the same app (a resume while in use), and the engine sees only
  the kind of page;
- for a second after it is raised (`CONFIRM_GRACE_MS`) it holds while
  usage shows nothing newer than the raise, since Android may write the
  page's own resume late; so a page's window sent late over the home screen
  covers it for at most that second, and an app that usage shows resumed
  after the raise ends it at once;
- with usage access off, the front is the package that last sent a
  window event, and only another package ends it; Android's record says
  nothing then, so a page with no window event is not seen;
- a raised guard follows Android's record when that record names the same
  app's page in front, so that another page after it ends it as above.

A package guarded whole is guarded while usage shows it in front (a
window event alone, which may come late, never does), or, with usage
access off, while it was the last to send a window event.

**What a class name alone can and cannot tell.** An activity's window
event carries the activity's own class (`Activity` sets it to
`getClass().getName()`), and so does its resume in Android's usage record.
So a page that is an activity of its own can be told apart. Most of
Android's Settings is not: on Android's own source
(and so on phones close to it, such as a Pixel), accessibility settings
and an accessibility service's own page, App info and most other pages
are fragments shown in one activity, `com.android.settings.SubSettings`,
so their windows all carry that one name. LiMo's accessibility page
cannot be told from the Wi-Fi page there, and is not guarded: guarding
`SubSettings` would guard nearly all of Settings, which the guard must
not do. The device admin's page is an activity of its own, so it can be.
A class name cannot tell whose admin the page is for either, so another
app's device admin page is guarded too while LiMo's protection is on (the
engine leaves it open while the protection is off). Other apps' App info
pages are not guarded at all, nor LiMo's: Android itself greys out Force
stop and Clear storage for an active device admin, and sends Uninstall to
the device admin's page (keep/README.md, "What Android refuses an active
device admin").

## Calls

Two readings, each only whether a call rings or is on: never a number,
never who, never how long.

- **The audio mode** (`AudioManager.getMode`, and from Android 12
  `addOnModeChangedListener`), on every Android, with no permission:
  Android's telecom service sets "ringtone" while a call rings aloud, "in
  call" while a call is dialled, on or held, and "call screening" while a
  screening service answers it.
- **The telephony call state** (`TelephonyManager.getCallState`): ringing
  or off the hook for every call on the phone, **a call that rings with the
  ringer silenced among them**, which the audio mode may not show.
  - **Android 8 to 11** (API 26 to 30) tell it to any app: it is read, and
    listened to with `listen(LISTEN_CALL_STATE)`, with or without the Phone
    permission.
  - **Android 12 and later**, for an app aimed at them as LiMo is, tell it,
    and its listener, only with `READ_PHONE_STATE`, the Phone permission
    setup asks for (the owner's decision of 20 Nov 2026). With it, the state
    is read at every look and `TelephonyCallback.CallStateListener` tells
    the watch at once. **Without it the audio mode alone is the
    fallback**: the telephony state is never asked for (Android would
    refuse it), and a silent ring may read as no call.
- **Internet calls** ("in communication", such as a voice chat in a game) are
  not calls here, as in proof two: a game's chat must never keep a cover
  away.

A ring in either reading rings, then a call on in either is on.

**The permission granted while LiMo runs.** Android tells nothing when it
is granted (taking it away stops LiMo's process). The watch reads the
permissions again every five seconds; on Android 12 and later a reading
that differs from the last one makes the watch register Android's call
listener, or drop it, at once. Until then, a ring is still found by the
host's once a second reading.

**What is left.** On a phone with two SIMs, Android's call listener of
Android 12 and later tells of the default SIM's calls only; a silent ring on
the other is found by the reading (once a second while the screen is on, and
the ring turns the screen on). Without the Phone permission the gap above
stays: an answered call is seen at once ("in call"), and a call screen that
comes up full screen is never covered (rule 6), but a heads up notification
of a silent ring could be covered. Today tells the child a guardian is
needed while the permission is off ("Phone access is switched off, so a lock
could hide a call.").

## Permissions

`permissions()` and `look().permissions`, in the order setup asks for them:

| Permission | Android's call |
| --- | --- |
| `notifications` | `NotificationManager.areNotificationsEnabled` |
| `phone` | `checkSelfPermission(READ_PHONE_STATE)` is granted (the manifest asks for it; setup asks the person) |
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

Of an accessibility event: its type and its package name; and, **only when
that package is Android's Settings, the package installer or the package
Android named for the device admin's page**, the name of the window's
class, only to match it against the guarded pages, then dropped: never
logged in a release build, never kept, never sent (see [The
guard](#the-guard)). Of a usage event: its kind (resumed, paused, stopped,
the phone started), its package and its time; and, **only for a resumed
activity of Settings or of that same package**, its class, matched the same
way and dropped, the event keeping only the guarded page it is. Of the
package manager, for three intents only: which activities open them, each
its package, its class and whether it is preinstalled. Of an app: its
package name and, by that name, its label. Of a call: whether one rings or
is on; never a number, never who, never how long. Nothing on the screen, no
window's title or text, no content description, no node: the service's
settings ask Android for no window content, so it has none to give. Log
lines (tag `LiMoWatch`) name at most a package, a fact, a guarded page's
kind and the kind of a failure; never a label, a class name, nor an
exception's message (a debug build's probe, switched on by a person at the
PC, is the one exception: dev/README.md).

## What it logs

Android's log, tag `LiMoWatch` (`adb logcat -s LiMoWatch`), each line as
the code writes it:

| Line | When |
| --- | --- |
| `front now <home, limo, dialler, emergency_dialler, app> package=<package>` | What is in front changed. Every kind but unknown names its package: the launcher's for the home screen, LiMo's own for LiMo. |
| `front now unknown because=<usage_access_off, usage_unreadable, nothing_seen_yet>` | What is in front cannot be told, and why. |
| `front window_event package=<package> not_confirmed front=<package, or none>` | A window event that usage did not confirm within a second, dropped; `front=` is what usage shows in front then. |
| `front learned call_screen package=<package>` | A call's own screen learned: the first screen after a call rang, was dialled or was answered, with no icon (rule 8). |
| `front call_wait ended first=<package>` | The first screen after a call rang, was dialled or was answered was one of the known kinds or has an icon: nothing learned (rule 8). |
| `front call_wait ended because=<five_seconds, call_ended, clock_set, no_usage>` | The wait for a call's screen ended with nothing learned: nothing new in front within five seconds, the call over, the clock set, or a read of usage that failed. |
| `front call_wait not_started because=no_usage` | A call rang, was dialled or was answered while usage cannot be read: nothing will be learned. |
| `front learned emergency_dialler package=<package>` | The emergency dialler learned: the first screen after Emergency call, with no icon (rule 9). |
| `front emergency_wait ended first=<package>` | The first screen after Emergency call was one of the known kinds or has an icon: nothing learned (rule 9). |
| `front emergency_wait ended because=<ten_seconds, clock_set, no_usage>` | The wait after Emergency call ended with nothing learned: nothing new in front within ten seconds, the clock set, or a read of usage that failed. |
| `front emergency_wait not_started because=<no_usage, no_dialler>` | Emergency call while usage cannot be read, or where Android names no dialler and no emergency dialler: nothing will be learned. |
| `call now <idle, ringing, ongoing>` | The call state changed. |
| `guard now <device_admin, accessibility> by=<page, usage, event, package> package=<package>` | A page that could switch LiMo off or remove it is in front: recognised by its window's class, by the class of the activity Android's usage record shows in front, by Android asking the device admin, or by its package. Never the class name. |
| `guard now none` | No such page is in front any more. |
| `guard pages device_admin=named package=<packages>`, `guard pages device_admin=not_named` | What the package manager named for the device admin's page, by its packages only; at the first need in LiMo's process, and when it changes. With `not_named`, the fixed list alone. |
| `guard pages accessibility=<own_package, in_settings, not_named>` with ` package=<packages>` but for `not_named` | Where the accessibility settings open, as Android answered: in a package of their own (guarded whole), inside Android's Settings (not guarded), or not named (the fixed list alone). |
| `fact <fact> unreadable error=<kind> fallback_used` | A reading Android refused, its fallback used; at most once a minute for each fact. |
| `fact launchable_apps unreadable error=<kind>` | The list of apps a child can open could not be read. |
| `fact guard_pages unreadable error=<kind> fallback_used` | The package manager refused an answer for one of the guard's intents: the last good answer of it holds, or the fixed lists; asked again a minute later. |
| `signal <calls, usage_access, accessibility_setting, phone_calls> unavailable error=<kind>` | One of Android's signals could not be registered. |
| `<what> failed error=<kind>`, `look failed error=<kind>`, `listener failed change=<what> error=<kind>`, `clock failed error=<kind>` | Anything else that failed, caught so that the service goes on. |

## Signals

While someone listens (`listen`), and only then:

| Signal | Android's call | Tells |
| --- | --- | --- |
| the screen on, off, unlocked | receiver: `SCREEN_ON`, `SCREEN_OFF`, `USER_PRESENT` | `SCREEN`; the guard reads at once whether the phone is put down |
| a call, Android 12 and later | `AudioManager.addOnModeChangedListener` | `CALL` |
| a call, Android 12 and later, while the Phone permission is granted | `TelephonyManager.registerTelephonyCallback` with a `CallStateListener` | `CALL` |
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

## What the manifest gives it

On Android 11 and later LiMo sees only the packages its `<queries>` name.
Beside the launchable apps, the home apps, the diallers and the keyboards,
the module's manifest names the three intents through which two kinds of
screen are found before they ever come up:

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

Rules 8 and 9 (learned from what comes to the front) remain for a phone
where they name nothing. For the guard, three more, one for each intent the
package manager is asked about ([The guard](#the-guard)):

```xml
<!-- The device admin's page, which App info's Uninstall also opens for an active device admin. -->
<intent>
    <action android:name="android.app.action.ADD_DEVICE_ADMIN" />
</intent>
<!-- The accessibility settings' own entry page. -->
<intent>
    <action android:name="android.settings.ACCESSIBILITY_SETTINGS" />
</intent>
<!-- Android's Settings app itself. -->
<intent>
    <action android:name="android.settings.SETTINGS" />
</intent>
```

| Entry | What it lets LiMo see |
| --- | --- |
| `ADD_DEVICE_ADMIN` | The apps with an activity that handles switching a device admin on or off: on Android's own source Settings alone, and any app a person installed that claims it, which the watch does not believe. |
| `ACCESSIBILITY_SETTINGS` | The apps that open the accessibility settings: Settings, and on One UI Samsung's own accessibility package; again, any app that claims it, not believed. |
| `SETTINGS` | The app that opens Android's Settings: so that it, and a maker's Settings app under another package, is never guarded whole. |

None of them lets LiMo see what those apps show: only that they are
installed, and which of their activities answer the intent. The manifest
also asks for the Phone permission
(`READ_PHONE_STATE`, the child form only; see [Calls](#calls)), and the
accessibility service's description in `res/values/limo_strings.xml` says
what it does: LiMo is told which app is in front, by its name only, and
never what is on the screen.

The accessibility service starts the keeper when it connects
(`keep/Keeper.start`): Android starts LiMo's process to connect it after a
restart or a stop, and the engine's host starts with it.

## Tests

`npm run test:child` (in the app) runs them with the module's other JVM
tests. They run on the PC with no Android at all: `TestPhone` is a
`PhoneSource` whose every reading the test sets, which can refuse any of
them, and whose usage record the test writes as Android does, holding events
back to write them late.

| Where | What | Count |
| --- | --- | --- |
| `FrontTest` | An app in front once its activity resumed and its window said so; proof two's late event after "Go home", fourteen tries, the home screen in front throughout, the event dropped after three rechecks; the home screen's resume written late with the late event first; an event usage has not written yet, confirmed by a recheck; a floating window never in front; the shade, system dialogs, keyboards and LiMo's cover proposing nothing, older phones' recent apps and a permission dialog leaving the app in front; LiMo's activity and its cover; LiMo's word for its screen with usage access off, and a missed word put right; the dialler, a call's own screen learned and still the dialler after the call, a game during a call still a game; a screen with no icon the call's only during a call and learned only as it starts; an in call screen Android names; a package with no icon brought up after the call's own screen never learned, whether that screen was learned or named; one that came up before the call never learned, read before the call was seen or only as it was; the call's own screen, unnamed and first to come up as a call rings or after one answered from a heads up notification, learned and still the dialler after the call; the emergency dialler named, learned, never learned for an app with an icon or after ten seconds; the first run's sequence (the named emergency dialler, the home screen, Settings, then a package with no icon 8.7 seconds after the tap) learning nothing; an emergency dialler Android does not name learned as the first screen after the tap, from LiMo's own screen, when left before the watch read the record, and when brought up again while already in front, and a second package with no icon after it never learned; the dialler, the home screen, LiMo or an app with an icon coming first and ending the wait; no wait where Android names no dialler or with usage off; the wait ended by ten seconds, the clock set and a refused read; usage access off: unknown, a dialler still saying so, LiMo's covers changing nothing, back on at once; split screen; picture in picture, and Android 9 and older; an app with no label, a blank one, an unseen one, one that cannot be read; the clock set back, told or not; a restart of the phone in the record; events read twice near the edge, written late, the clock gone back | 27 |
| `GuardTest` | The device admin's page guarded by its name, through a dialog over it (minutes), ended by another page of Settings and by home, Android 8.0 to 9's name too, the log naming page and package and never a class; a page whose own resume Android writes late guarded at once, and a page's late window over home guarded for at most a second; LiMo's own screen (the PIN) and the emergency dialler setting it aside, the same page guarded again on its return, another page ending it; Android asking the device admin to switch off raising it over its question, held through the dialog, ended by home; One UI's accessibility package guarded while usage shows it, never by a late window alone, and with usage access off by the last window, a page by name held until another app's window; a class name read for Settings and the installer only, the same name from any other app nothing; a page raised by its name, by Android's event and by One UI's package, after the PIN on LiMo's own screen, coming back as the same page with no window event and guarded again after each way away (the screen off with the lock screen, the screen off alone, the lock screen alone, the lock screen's camera, LiMo's own screen, a call), and another page or app after it still ending it; Android's screen signal alone keeping it when nothing looks while the screen is off, and a window event with no guard raised reading nothing of the screen; a device admin's page under a maker's name, which the package manager names (directly, and by an alias), guarded when App info's Uninstall opens it with no window event and no question to the admin, by Android's record alone, and with usage access off by its window, while the same phone with no answer leaves it open; the same page in a maker's own package, its classes and Settings' the only ones read; a guard raised at Deactivate over Android's question, Home, then back from the recent apps with no window event: guarded again where Android names the page, still open where it names nothing; LiMo's process starting again with each kind of page in front, guarded at the first look, except a page known by the question alone, guarded only when Deactivate asks again; accessibility settings that Android opens in a package of their own guarded whole and never read, also after a restart; accessibility settings inside Android's Settings, or inside a maker's Settings under its own package, said as they are (`IN_SETTINGS`) and nothing guarded; Settings' own answer refused, so no package guarded whole but the fixed list's; Android naming nothing, said as it is; the answer kept for the process, asked again when apps change, a refusal keeping the last answer, logged once and asked again a minute later, and a process Android refuses from the start still guarding Android's own names; an activity that also opens Settings' first page never taken for the device admin's page; the fixed lists alone reading as before | 16 |
| `FactsTest` | Every fact from its reading in one place; each fact's fallback, its last good reading and then its default; every reading refused at once, logged once each, and the facts back when Android answers; a refusal logged once a minute without its message or a label; the call state from the audio mode and the telephony state, against Android's own numbers, a silent ring on Android 8 to 11 with or without the Phone permission; on Android 12 and later a silent ring read with the Phone permission, and without it the telephony state never asked for and the audio mode alone the fallback, a permission granted while LiMo runs taken within five seconds, a refused reading falling back to the audio mode, logged once; usage event kinds against Android's numbers; the accessibility permission switched on, connected, starting; the permissions read again after five seconds or when told; the accessibility setting in both forms; the launchable apps sorted, LiMo left out, the dialler never counted; labels kept and forgotten; the roles asked again and kept when refused; usage that cannot be read; the Phone permission as Android grants it, each of the six reaching the engine's own field, off when it cannot be read | 15 |
| `HouseRulesTest` | No source here reads a window's content, its text, or changes the service's settings at run time; a class name read from Android in exactly two places, the service (a window's) and `AndroidPhone` (a resumed activity's in the usage record), each on one line that first asks the guard's reach and hands the name only to `WindowPage`; the package manager's own class names taken in one place; its release copy only matches (no log, no probe), its debug copy hands each name to the probe and matches as release does; the fixed list's packages exactly Settings and the installer; a usage event holding no class, the reach holding only what the package manager named; on a phone where a child's own game claims the guard's intents (chosen to open one) and names its activities like the device admin's pages: only preinstalled answers believed, never Android's chooser, class names read for Settings, the installer and the maker's named package only, for resumed activities Settings and that package only, the game's never read nor guarded, no class in the log; no log line names a class; a caller's number is never used; package names only in the framework's list (`Roles.kt`) and the guard's (`Guarded.kt`); no dash punctuation in a comment or this README | 4 |

All 62 pass, with the module's other tests (the engine and the link from
Kotlin, host/, keep/ and dev/), in well under a second of test time each.

## The first run

**20 November 2026, the Galaxy (Android 13).** Emergency call on LiMo's
cover opened Samsung's emergency dialler, and the watch knew it at once from
Android's roles: `front now emergency_dialler` 61 ms after the tap, with no
`front learned` line. The person then went to the home screen, opened
Settings and tapped its search, a package with no icon, 8.7 seconds after
the tap. Rule 9 as it stood then (any unexplained package within ten
seconds, the window open whatever came first) learned that search as the
emergency dialler, so until LiMo's process ended it would never have been
covered or counted; a child could have done the same with any package that
has no icon. Rule 9 now learns only the first screen after the tap, and
`FrontTest` plays that sequence at those times. Rule 8, a call's own screen,
was narrowed the same way after it. No call could be made on that phone (it
had no SIM), so the call rules (6 and 8) have only run on the PC.

## What only a phone can prove

For the owner, on the Galaxy S20 Ultra (Android 13, One UI 5.1), with
`adb logcat -s LiMoWatch`:

1. **Proof two's flaw, gone.** Block an app that sends a late window event
   (YouTube was the one), open it, let the cover come, tap "Go home"; repeat
   14 times. The home screen must never be covered again. The log should
   show `front now home package=<the launcher's package>` and then, for the
   late event, `front window_event package=com.google.android.youtube
   not_confirmed front=<the launcher's package>`.
2. **How long usage takes to confirm.** The cover's delay over proof two's
   25 to 226 ms. If a `not_confirmed` line ever names the app that really came
   up, usage was later than a second, and the rechecks need stretching.
3. **The call screen on One UI.** Not yet seen: the first run's phone had
   no SIM. Dial a number from the Phone app and look for `front learned
   call_screen package=<package>` or `front call_wait ended first=<package>`
   (the screen named), and that the call's screen reads as the dialler
   (`front now dialler package=<package>`). Then answer an incoming call
   while a game is in front, once with the screen off (the call's full
   screen) and once in use (a heads up notification). With the manifest's
   `InCallService` query added, the call screen should be named before any
   call. Then, during a call, open a settings page with no icon: it should
   read as the dialler while the call lasts, and never log `front learned`.
4. **A silent ring on Android 13.** Ringer off (and vibration off), a game
   in front, call the phone, first **with the Phone permission granted**:
   the log should say `call now ringing` at once (Android's call listener),
   and nothing should be covered until the call ends. Then take the
   permission away in Settings (Android stops LiMo; open it again) and call
   once more: if the log says nothing until the call is answered (`call now
   ongoing`), that is the gap without the permission in [Calls](#calls).
   Then the same with the ringer on, to see the ordinary case. Then grant
   the permission from setup while LiMo runs, and call within ten seconds:
   `call now ringing` should still come. On a phone with two SIMs, a call
   to the second one too.
5. **The emergency dialler.** On the Galaxy the first run answered which
   package opens and how it is found (from the emergency dial actions, see
   [The first run](#the-first-run)). With rule 9 as built the same tap
   should also log `front emergency_wait ended first=<that package>`, and
   nothing opened after it (the home screen, Settings, its search) should
   ever log `front learned`. On another maker: which package opens, and
   whether it is found (`front now emergency_dialler package=<package>`,
   with no `front learned` line) or learned (`front learned
   emergency_dialler package=<package>`).
6. **One UI's recent apps, pop up view and split screen.** Recents should
   read as `front now home package=<the launcher's package>`. A pop up
   window and split screen should give the last resumed app as the front
   and the other in `alsoVisible`.
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
11. **The guard's names on One UI.** At LiMo's start the log should say
    `guard pages device_admin=named package=com.android.settings` and
    `guard pages accessibility=own_package package=com.samsung.accessibility`.
    With the debug build's probe on (dev/README.md, "The page probe"), the
    `probe answered` lines give the package manager's answer, and walking
    the pages listed there gives each page's `probe window` and `probe
    resumed` lines. The device admin's page, reached from the list of
    device admin apps and from App info's Uninstall, should show the same
    class in all three, and log `guard now device_admin by=page` (or
    `by=usage`), covered as it comes up. If a way to it shows another class
    than the answer's, that way is guarded only at Deactivate (`by=event`):
    add the name to `Guarded.kt`. Which package the installer is on the
    Galaxy (a `front now app package=` line at its question) and whether it
    is one of the three in `Guarded.kt`.
12. **Deactivate under the guard.** With the probe off, Settings, Device
    admin apps, LiMo: covered at once (`guard now device_admin by=page`, or
    with no name on the list, `by=event` at Deactivate, then the guard over
    Android's question). Wrong PIN, Back: the page still covered. Emergency
    call, Back: still covered. Right PIN: open; Deactivate shows Android's
    question with LiMo's words ("Switching this off lets LiMo be removed
    from this phone."); Cancel. Then App info, Uninstall: covered again only
    after the five minutes, or at once after the screen was off.
13. **One UI's accessibility settings under the guard.** Settings,
    Accessibility: covered (`guard now accessibility by=package
    package=com.samsung.accessibility`). A guardian's PIN opens them.
    Whether One UI reaches LiMo's accessibility switch by any page outside
    that package (Settings' search, a quick settings tile, the
    accessibility shortcut One UI offered at setup in the first run): each
    one found is a way around the guard.
14. **Timing.** How long after the device admin's page comes up the guard
    is drawn (`show cover drawn ms=`), and whether a quick tap on
    Deactivate can beat it.
15. **The same page after the screen off.** With the device admin's page,
    and then One UI's accessibility settings, open under the PIN: the
    screen off, the lock screen, the screen on, unlocked. The guard must be
    over the same page at once (tag `LiMo`: `show cover now guard`), before
    anything is tapped. The watch writes no new `guard now` line for it,
    since the page never left; whether One UI's page sends a window event
    after the unlock (the probe shows it) is worth a note, since the guard
    no longer needs one. The same with the lock screen's camera opened and
    closed before unlocking.
16. **Home and back over Android's question.** On the device admin's page,
    the PIN, Deactivate, then Home with Android's question still up, and
    back to Settings from the recent apps: covered again at once (`guard
    now device_admin by=usage`), before OK can be tapped.
17. **LiMo's process starting again with the page in front.** With the
    device admin's page open under the PIN, end LiMo's process from the PC
    on the debug build (`adb shell run-as ai.lucidloom.limo.child kill
    <its pid>`; Android's own Force stop is greyed out, and adb's
    `force-stop` would switch the accessibility service off). Once Android
    starts it again (the accessibility service connecting), the page should
    be covered at the new process's first look, with nothing tapped:
    `guard pages ...` and then `guard now device_admin by=usage`.
18. **Usage events' class names.** That Android's usage record gives LiMo
    the class of Settings' resumed activities at all (the `probe resumed`
    lines name a class, not `none`); if it gives none, the guard has the
    window's class and the event only.

## Left open

- **A call kept going.** While a call rings or is on, any package with no
  icon is the dialler (rule 6): never covered, and its time never counts. So
  a child who keeps a call going (to voicemail or to a friend, or on hold)
  can use such a package freely for as long as the call lasts. Apps with an
  icon stay covered. The rule stays because a lock must never hide a call's
  screen, and it ends with the call. Rule 8 has a smaller edge of the same
  kind: where no screen comes up with the call (a ring shown as a heads up
  notification and let ring, or a call answered on its screen already up),
  the first package with no icon the child brings up within five seconds
  is learned as the call's screen and stays the dialler after the call. That
  is one package for each ring or answer, and it needs a real call.
- **Split screen with a blocked app in the other half.** The engine takes one
  front; `alsoVisible` says what else shows. Covering for an app beside the
  front needs the host to ask the engine about each, or the engine to take
  the others: for the host and the covers.
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
- **"Back to the call"** is show/'s: `TelecomManager.showInCallScreen`
  while `READ_PHONE_STATE` is granted, and the dialler otherwise, which
  shows the call on most phones (show/README.md). Whether it does on a
  given phone without the permission is for that phone to show.
- **Emergency call that opens a chooser, or nothing.** Where two apps answer
  the emergency dial action (one of them not preinstalled, which the roles
  do not believe), Android shows its chooser, an overlay; dismissed, the app
  under the cover comes back first, and is learned if it has no icon. The
  same holds where every way of Emergency call is refused on a phone that
  names a dialler. Leaving out the package that was in front at the tap
  would close both, but would also stop Emergency call learning an
  emergency dialler already in front under a cover, the way one is learned
  again after LiMo starts anew. show/ telling the watch when nothing opened
  would close the second.
- **The labels** are the app's own, cleaned by the engine before it shows
  them; the list for the guardian is passed as Android gives it.
- **Accessibility inside Android's own Settings is not guarded.** Where
  the accessibility settings open inside Android's Settings (Android's own
  source, and phones close to it such as a Pixel: `guardScope.accessibility`
  is `IN_SETTINGS`), LiMo's own page there is a fragment in
  `com.android.settings.SubSettings`, which no class name tells from the
  Wi-Fi page (see [The guard](#the-guard)). There a child can switch LiMo's
  accessibility service off; then LiMo can see nothing and cover nothing,
  the device admin's page included, so the child can go on to Deactivate it
  and remove LiMo. This is the widest way left on such phones. What LiMo
  does then: the engine notes the permission switched off for the guardian
  (it reaches the master with the notes), and the child's Today shows the
  banner "Accessibility is switched off, so LiMo cannot see which app is
  open." until it is on again. The watch now says plainly where this is so
  (`IN_SETTINGS`), but nothing tells the guardian before it happens: the
  engine takes no such fact today. The same holds for every way to switch
  the service off that the guard does not cover: a force stop (from
  developer options or adb; Settings' own Force stop is greyed out for an
  active device admin), and the accessibility shortcut, if one was ever
  given to LiMo.
- **A maker whose list or Uninstall opens another activity** than the one
  that handles `ACTION_ADD_DEVICE_ADMIN`, under a name on no list: that way
  is guarded only at Deactivate (by event), and App info's Uninstall there
  removes LiMo in one tap. Android's source opens one activity for all
  three ways, so this needs a maker that split them; the probe shows it
  (`probe answered` against `probe window` and `probe resumed`), and its
  name then goes on the list.
- **A guard raised by Android's question alone**, on a page known by no
  class (neither on the list nor named by the package manager, as in the
  case above): it ends when the person goes Home, and back in Settings from
  the recent apps Android's question is still up and unguarded, so OK
  switches the admin off; and a new process of LiMo's started with that
  question up does not cover it until Deactivate asks again. Telling that
  page from any other page of Settings would take its class, which the
  watch keeps only as far as the package manager names it.
- **A page that tells nothing but Android's record** (no window event
  reached the service) is covered at the host's next look after Android
  writes its resume: within about a second, not at once. Whether a quick
  tap on Uninstall's one button can come first is for a phone to show
  ([What only a phone can prove](#what-only-a-phone-can-prove), 14); with
  its window event, as is usual, it is covered at once.
- **Usage access switched off** leaves the guard the windows' class names
  and the event: a page that sends no window event, or one already in front
  when a new process starts, is then not seen.
- **Settings' and the installer's package names.** Settings is
  `com.android.settings`, the package the package manager names for
  `ACTION_SETTINGS` (never guarded whole) and the package it names for the
  device admin's page (whose class names are read); a maker's installer
  under another package than the two in `Guarded.kt` is not read, which
  costs nothing while no installer page is guarded.
- **Another app's device admin page** is guarded too while LiMo's
  protection is on: a class name cannot tell whose page it is.
- **A restart of LiMo's process with a guarded page in front**, otherwise,
  is closed: nothing of a raise outlives the process, and the watch keeps
  no file, but the new process asks the package manager before its first
  read of Android's record, whose last resume names the page in front by
  its class (or the package guarded whole), so it is guarded at that first
  look. With the device admin on, Settings' Force stop is greyed out, so
  such a restart takes a crash or Android ending LiMo for memory.

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
  Guarded.kt         the guard: the fixed lists, the package manager's answer (GuardReach), and GuardWatch
src/release/java/ai/lucidloom/limo/child/watch/
  WindowPage.kt      release builds: a window's or a resumed activity's class name matched, and dropped
src/debug/watch/ai/lucidloom/limo/child/watch/
  WindowPage.kt      debug builds: the same, handed first to the probe (dev/README.md)
src/test/java/ai/lucidloom/limo/child/watch/
  TestPhone.kt       the phone the tests drive, and the rig
  FrontTest.kt, FactsTest.kt, GuardTest.kt, HouseRulesTest.kt
```
