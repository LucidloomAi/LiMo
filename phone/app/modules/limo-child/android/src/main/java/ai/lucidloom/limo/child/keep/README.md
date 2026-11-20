# keep

What keeps LiMo running on the child's phone: the foreground service that
keeps the process alive and shows the time left, the warnings, starting
again after a restart or an update, the alarm, and a helper for leaving
LiMo out of battery optimisation. Kotlin, package
`ai.lucidloom.limo.child.keep`, in the child module.

**What it is.** `Keeper.start`, called whenever LiMo's process may have
just started; `KeepService`, a foreground service of type `specialUse`;
the two notifications, in the engine's words; `RestartReceiver` and
`AlarmReceiver`; `Battery`.

**What it is not.** It does no work of its own: the engine's host (host/)
runs on its own thread whether the service runs or not, and the service
only keeps the process alive. It decides nothing and makes no sentence about
time. `AdminReceiver` (the device admin) is still a stub: it is the base
protection's, a later piece. Nothing here has run on a phone yet.

## Contents

- [When the keeper starts](#when-the-keeper-starts)
- [The notifications](#the-notifications)
- [What Android allows a specialUse service](#what-android-allows-a-specialuse-service)
- [The alarm](#the-alarm)
- [Battery](#battery)
- [Tests](#tests)
- [What only a phone can prove](#what-only-a-phone-can-prove)
- [Left open](#left-open)
- [Layout](#layout)

## When the keeper starts

`Keeper.start(context, why)` starts the engine's host, and when a state is
kept (the phone is paired) and `KeepService` is not running yet, asks
Android to start it in the foreground. It never throws, and may be called
as often as wanted.

| From | When |
| --- | --- |
| `host/LimoChildModule` | The app opens (the Expo module is made). The app is in front, so Android lets the service start. |
| `watch/WatchService` | The accessibility service connects: after a restart, after LiMo's process was stopped, or when a guardian switches it on. |
| `RestartReceiver` | `BOOT_COMPLETED` (the phone restarted) and `MY_PACKAGE_REPLACED` (LiMo was updated). |
| `AlarmReceiver` | LiMo's own alarm, at most fifteen minutes apart. |
| dev/`DevReceiver` | After the test family's pairing (debug builds only). |
| The host's ticks | When the service is not running (Android refused it, or stopped it), it is asked for again, at most once a minute, until it takes. |

The two receivers hold their broadcast (`goAsync`) until the host's first
tick is done, so Android does not stop the process before it. With no state
kept, `KeepService` stops at once; `Keeper.stop` (from a forgotten pairing)
stops it and takes the notifications away.

## The notifications

**The time left.** One ongoing notification on a quiet channel, the same
one the foreground service shows. Its title is the engine's words for the
time left ("1 h 12 min left", "1 h 12 min paused", "No limit today", "Open
until 9:30 pm", or the lock's title, such as "Today's time is used up.").
While time is counted (and the phone is not locked, open under an unlock,
or without a limit), Android's own chronometer counts down to the moment
the time runs out (`setUsesChronometer`, `setChronometerCountDown`), so the
seconds move without LiMo. It is posted again only when what it shows
changes: the words (once a minute while counting, every second in the last
minute), the countdown starting or stopping, or its end moving by more than
two seconds (time added, the clock set). Before the first tick it has no
words, and Android shows LiMo's name alone.

**A warning.** On a channel of its own, which Android shows as a heads up
notification: the engine's message as the title ("5 minutes of screen time
left today."), its hint as the text ("Save what you are doing."), and its
when as the small line at the top ("Locks about 9:47 pm"). It goes away by
itself a minute after the lock it warns of. The host also hands each
warning to show/, which may float it over the app in front.

Tapping either opens LiMo. The small icon is the design's hourglass as a
plain silhouette (`res/drawable/limo_notify.xml`), since Android draws a
status bar icon from its shape alone.

**Words that are not the engine's.** Only Android's labels, in
`res/values/limo_strings.xml`, which Android shows in LiMo's notification
settings:

| Name | Words |
| --- | --- |
| `limo_channel_time_left` | Time left |
| `limo_channel_time_left_about` | How much screen time is left today. |
| `limo_channel_warnings` | Warnings |
| `limo_channel_warnings_about` | A few minutes before the phone locks. |

There is no title of LiMo's own: the time left notification's title is the
engine's words.

**Without the notifications permission** (Android 13 and later, until setup
asks for it) Android shows neither notification in the shade; the service
runs all the same, and Android's list of running apps still shows it.

## What Android allows a specialUse service

Read on 20 Nov 2026 from Android's own pages: "Foreground service types"
(`developer.android.com/develop/background-work/services/fgs/service-types`),
"Restrictions on starting a foreground service from the background"
(`.../fgs/restrictions-bg-start`), "Launch a foreground service"
(`.../fgs/launch`), "Foreground service timeouts" (`.../fgs/timeout`),
"Changes to foreground services" (`.../fgs/changes`), the behaviour changes
of Android 14 and 15 (`developer.android.com/about/versions/14/behavior-changes-all`,
`.../15/behavior-changes-15`), Android 13's "Foreground services task
manager" (`.../13/changes/fgs-manager`) and "Notification runtime
permission" (`developer.android.com/develop/ui/views/notifications/notification-permission`).
What they say, and what was built from it:

| Android says | Built |
| --- | --- |
| From Android 14 every foreground service declares a type; `specialUse` needs the permission `FOREGROUND_SERVICE_SPECIAL_USE` beside `FOREGROUND_SERVICE`, and a `<property android:name="android.app.PROPERTY_SPECIAL_USE_FGS_SUBTYPE">` in the `<service>` saying what it is for. No runtime permission is needed for it. On Google Play the use is reviewed in the Play Console. | The manifest had both permissions and the property; the service passes `FOREGROUND_SERVICE_TYPE_SPECIAL_USE` to `startForeground` on Android 14 and later, and the plain call before. LiMo is not on Google Play (it is installed over the cable). |
| From Android 12 an app aimed at it may not start a foreground service while in the background, with exemptions. Among them: after `BOOT_COMPLETED` or `MY_PACKAGE_REPLACED`; while it has a visible activity; after the user taps its notification; when the user has left the app out of battery optimisation; an exact alarm for something the user asked for. Not among them: an inexact alarm, an accessibility service bound by the system, an active device admin (only device and profile owners are). A refused start throws `ForegroundServiceStartNotAllowedException`. | Started from the app opening and the two restart broadcasts, which are allowed. From the accessibility service connecting and from the alarm it may be refused unless LiMo is left out of battery optimisation: then it is logged (`keep start_refused`) and asked for again from the host's ticks, at most once a minute, until it takes (for example when the app is next opened). The host ticks without it. |
| Android 15 forbids starting some types from a `BOOT_COMPLETED` receiver: `dataSync`, `camera`, `mediaPlayback`, `phoneCall`, `mediaProjection`, `microphone`. `specialUse` is not among them. | `RestartReceiver` starts it on boot. |
| Only `dataSync` and `mediaProcessing` have a time limit (six hours a day), and `shortService` about three minutes. `specialUse` has none. | Nothing to handle; `START_STICKY` so Android starts it again after stopping the process for memory. |
| From Android 14 a person can swipe away a foreground service's notification, even one set ongoing, except while the phone is locked; media and call notifications are exempt, `specialUse` is not. The pages do not say what happens to the service; it is not stopped by it. | The time left notification comes back by itself at its next change (the next minute while counting). Swiping it away stops nothing. |
| From Android 13 the notification shade lists running foreground services with a Stop button; Stop ends the whole app, not only the service, removes its notification, sends no callback, and leaves its alarms in place. No exemption is listed for a device admin or an accessibility app. | Any child can tap it. LiMo's alarm (at most fifteen minutes away) and the accessibility service connecting again bring it back; the lock, if one was on, is on again at the first tick. A phone check (below). |
| Without the notifications permission a foreground service still runs; its notice shows in the list of running apps, not in the shade. | Nothing to handle. |
| The notification of a foreground service may be held back for ten seconds unless it asks to be shown at once (Android 12 and later). | `setForegroundServiceBehavior(FOREGROUND_SERVICE_IMMEDIATE)`. |

Not read anywhere, so not relied on: the time Android gives a service
between `startForegroundService` and `startForeground` (the service calls
it first thing in `onStartCommand`).

## The alarm

Set by the host (`host/AlarmWakeups`), received here (`AlarmReceiver`,
not exported, action `ai.lucidloom.limo.child.keep.WAKE`, from a pending
intent only LiMo makes). An ordinary alarm (`setAndAllowWhileIdle`,
`ELAPSED_REALTIME_WAKEUP`), never an exact one: LiMo holds no exact alarm
permission. With the screen off it wakes the host at the engine's next
check; always, it is at most fifteen minutes away, so a LiMo that was
stopped comes back by itself. Alarms do not survive a restart of the phone;
`RestartReceiver` starts the host, which sets it again.

## Battery

`Battery.askToLeaveOutOfOptimisation(context)`, for the setup screens'
Battery step ("Android asks once. Choose Allow."). Nothing calls it yet: the
piece that answers the screens' requests does, from `child.openStep`. It
shows Android's own question (`ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS`,
which the manifest's `REQUEST_IGNORE_BATTERY_OPTIMIZATIONS` lets LiMo ask),
or Android's list of apps left out where a phone has no such question, and
says whether one opened. Whether LiMo is left out is the watch's to read
(`batteryExempt`). Left out, LiMo may also start its service from its alarm
after it was stopped (Android 12 and later).

## Tests

`npm run test:child` runs `KeeperLookTest`: the time left as the engine's
words with Android's countdown while time is counted, and none while
paused, locked, under an unlock or with no limit; posted again only when the
words change, the countdown starts or stops, or its end moves more than two
seconds, never at every tick; a warning as the engine's message, hint and
when, gone a minute after the lock; and the host's schedule
(`host/HostTimes`): a tick every second with the screen on and an alarm set
about every ten minutes, not every second; with the screen off the engine's
next check by tick and by alarm, never further than fifteen minutes, moved
up for an earlier check. The service, the receivers and the notifications
themselves are Android's and are checked on the phone. The host's tests
(host/README.md) check what the keeper is handed.

## What only a phone can prove

For the owner, on the Galaxy S20 Ultra (Android 13), with `adb logcat -s
LiMo`:

1. **The notification.** After pairing: an ongoing "30 min left" (the test
   family's time) in the shade, the chronometer counting down while a game
   is in front, "paused" with the screen on the home screen. Tapping it opens
   LiMo.
2. **The warnings** as heads up notifications at 15, 5 and 1 minutes, over
   a game, with the hint and "Locks about ...". With the ringer on, a sound.
3. **Restart.** Restart the phone and unlock it: the notification is back
   without opening LiMo (`RestartReceiver`), and `keep start_refused` is not
   in the log.
4. **Started from the background.** With LiMo not left out of battery
   optimisation, stop it from the shade's list of running apps, then wait:
   does it come back through the accessibility service connecting, or only
   at the alarm, and is `keep start_refused why=accessibility` or
   `why=alarm` logged? Then the same with LiMo left out (Battery step): it
   should start at once.
5. **The Stop button** (Android 13's list of running apps): after Stop,
   LiMo's lock (if one was on) is back within fifteen minutes at most, and
   at once if the accessibility service reconnects.
6. **Swiping the notification away** is refused while locked and allowed
   otherwise on Android 14 and later; the Galaxy has Android 13, where it
   cannot be swiped away at all. Worth trying on a phone with 14 or later.
7. **One UI's own battery rules**: whether "Never sleeping apps" (the
   Samsung note on the Battery step) is needed for the alarm to come on time
   with the screen off.

## Left open

- **The device admin** (`AdminReceiver`) is still a stub, for the base
  protection. Its words in Settings still say this early build does nothing.
- **Google Play.** `REQUEST_IGNORE_BATTERY_OPTIMIZATIONS` and a `specialUse`
  service are both reviewed by Google Play; LiMo is not on it.
- **`systemExempted`**, a foreground service type for device admin and
  device owner apps, could take `specialUse`'s place once the protection is
  built; the pages list a device admin among the apps that may use it.

## Layout

```
keep/
  README.md
  Keeper.kt          start and stop, and the notifications (NotificationKeeper, the host's KeeperDisplay)
  KeeperLook.kt      what the notifications show and when they are posted again, pure
  KeepService.kt     the foreground service, type specialUse
  RestartReceiver.kt boot and update
  AlarmReceiver.kt   LiMo's alarm
  Battery.kt         Android's question for leaving LiMo out of battery optimisation
  AdminReceiver.kt   the device admin, a stub (base protection, later)
res/values/limo_strings.xml    the channels' names
res/drawable/limo_notify.xml   the small icon
src/test/java/ai/lucidloom/limo/child/keep/KeeperLookTest.kt
```
