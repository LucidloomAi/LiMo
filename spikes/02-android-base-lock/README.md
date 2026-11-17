# LiMo spike 02: Android base lock (accessibility service only)

A small, throwaway proof for the LiMo screen time limiter. It is not the real
product. It answers one Android question before the real build starts, and
then it can be removed:

Without Device Owner rights, using only an accessibility service, can we
cover a blocked app fast enough, and what gets around it?

The test phone is a Samsung Galaxy S20 Ultra on Android 13 (One UI 5.1). The
device testing is a checklist in `TESTING.md`.

## What it proves, and what it does not

Once `TESTING.md` has been worked through on the phone, it shows:

1. Whether a build that did not come from a store can be given the
   accessibility service on Android 13, where "restricted settings" may block
   it: once installed with `adb install`, once installed by tapping the APK.
2. How fast the cover appears after a blocked app comes to the front, measured
   on the phone itself, and whether the app's content shows before it.
3. Which ordinary moves get around the cover (Recents, split screen, pop up
   view, links, notifications, the lock screen and so on).
4. What a child has to do to turn it off, and what each way costs them.

It does not:

- use Device Owner, device admin, usage stats or any other permission. It asks
  for no permissions at all;
- try to stop uninstalling, force stopping or turning the service off. Those
  are only recorded;
- count time or apply limits. It covers ticked apps while blocking is on, that
  is all;
- say anything about other phones, other One UI versions or Android 14 and
  later, which add rules of their own (see "Unknowns" below);
- measure battery use over a day.

## How it works

Two parts in one app, `ai.lucidloom.limo.spike02`: an accessibility service
(`BlockService`) and one screen (`MainActivity`). They share state in one
process (`SpikeState`). The pure rules are in `CoverRules`, with unit tests.

### Which events

The service asks only for `TYPE_WINDOW_STATE_CHANGED`. An app's window sends
this event when it becomes visible or gets focus back: a new activity, a
dialog or pop up, coming back from Recents, the lock screen going away. Of
each event the service reads two fields and nothing else: the package name
and the event's own time stamp (`event.eventTime`).

It does not use `TYPE_WINDOWS_CHANGED`. Android keeps the list of windows up
to date only for services that may read window content and ask for
interactive windows. This service may not read window content (see the
privacy rule), so that event would arrive only by luck, when some other
service such as TalkBack has window tracking on, and `getWindows()` would
return nothing anyway. Listening to it would make the result depend on what
else is installed.

### When "the app in front" changes

Every window state event is taken to mean "this package is now in front",
except events from:

- this app itself. The activity tells the service directly in `onResume`
  when it comes to the front, so the cover's own window, which also sends an
  event, can never be mistaken for an app coming to the front;
- System UI (`com.android.systemui`): the notification shade, quick
  settings, the volume panel, the power menu, the lock screen. These are drawn
  over the app in front and go away again;
- a few One UI overlays of the same kind: the Edge panel, the fingerprint
  overlay, the screenshot toolbar and the Game Booster panel. These package
  names are best guesses from memory; the test log shows which packages
  really send events;
- keyboards: every installed input method, looked up at run time, plus the
  keyboard in use.

An ignored event changes nothing: the cover stays as it was.

Every other event counts, including the launcher. The home screen, the app
drawer, folders and Recents are all launcher windows on One UI, and each of
them means the blocked app is no longer the one in front. The `android`
package (the share sheet, the app chooser, system dialogs) counts too.

Then the rule is simple. The cover is shown when blocking is on, the package
in front is on the blocked list, it is not a package that is never covered,
the lock screen is not up and no phone call is ringing or in progress.
Otherwise the cover is removed. A second event for the package already
covered changes nothing, so there is no flicker from repeated events.

Blind spots this rule has by design, which the test should show:

- split screen and pop up view: only the window that last sent an event
  counts, so a blocked app can stay visible next to an allowed one;
- another app's floating window (a chat bubble, a screen recorder toolbar)
  that sends an event while the blocked app is still visible behind it;
- Recents shows a picture of the blocked app, because Recents is the
  launcher.

### Never covered

Whatever the list says, the service never covers: this app; every launcher
(home screen app); every dialer, the system dialer and the default dialer, the
in call screen and the emergency dialer; every app that opens Settings, and
Samsung's accessibility settings. Most are looked up on the phone at run time
(`SystemPackages`); a fixed list in `CoverRules` adds the common names in case
a lookup misses one. The activity shows these apps as "never covered" with the
tick box disabled, and the adb extras refuse them (logged as
`refused ... reason=never_cover`). Whether the real product should cover some
Settings pages is a later question.

### The cover

A `TYPE_ACCESSIBILITY_OVERLAY` window, added with the service's own window
manager, so it needs no "display over other apps" permission. It fills the
screen, including the status bar and navigation bar areas (Android 11 and
later), and is opaque: a dark background, "LiMo spike: this app is blocked",
the app's name, and one button, "Go to home screen", which performs the
global Home action.

Things that follow from the window type, and that the test should confirm:

- accessibility overlays sit above the notification shade, the navigation
  bar and the keyboard. So with navigation buttons the Back, Home and Recents
  buttons are expected to be under the cover and not respond; with swipe
  gestures the system handles the gesture itself, so Home and Back should
  still work;
- the cover is not focusable. The blocked app keeps key focus, so the Back
  gesture and hardware keys still go to it, and its sound keeps playing under
  the cover;
- it would also sit above the lock screen. That is why the cover is removed
  when the screen turns off and is never shown while the lock screen is up.
  After unlocking, the cover comes back if the blocked app is still in front.

The cover is removed when:

- a package that is not blocked comes to the front (`front_changed`);
- blocking is switched off, or the 10 minutes run out (`blocking_off`,
  `safety_stop`);
- the screen turns off, or the lock screen is up (`screen_off`, `keyguard`);
- a phone call rings or is in progress (`phone_call`). The cover sits above
  the notification shade, so it could hide a heads up incoming call. This
  uses the audio mode, which needs no permission; on Android 12 and later a
  change of mode is noticed at once, on older versions at the next event.
  Internet calls are not counted;
- the service is turned off or stopped (`service_off`). Android should also
  remove an accessibility service's overlay windows by itself when it
  unbinds, so this holds even if the service code never runs;
- "Go to home screen" was tapped and no event that counts arrived within 3
  seconds (`home_fallback`). Normally the launcher's event removes the cover;
  this fallback makes sure a missed event can never leave it stuck, and the
  log shows when it was needed.

### How time is measured

For each new cover window the service logs the milliseconds from the event's
own time to the cover's first frame:

- the start is `event.eventTime`, on the uptime clock. It is stamped in the
  blocked app's own process when the app sends the event, so the figure
  includes delivery to the service, the decision, adding the window and
  drawing it;
- the end, on Android 10 and later with hardware drawing (the normal case), is
  the frame commit callback: the first frame has been rendered and handed to
  the display system. It reaches the glass at the next screen refresh, a
  further 8 to 17 ms at 120 to 60 Hz. On older phones, or without hardware
  drawing, the end is the screen refresh after the first draw. The log says
  which with `method=commit` or `method=vsync`.

What the figure does not include: the time between the tap and the blocked
app's window becoming visible. The app's content can be on screen for a
moment before it sends the event. That is why the test also asks for a check
by eye.

A cover that appears after unlocking, after a call, or after a setting
change is measured from that moment instead, and is listed with its trigger
but left out of the median and the slowest.

The activity keeps the last 50 measurements, newest first, with the median
(the mean of the two middle values for an even count, rounded down) and the
slowest. They are kept in memory only.

### The safety stop

Blocking turns itself off 10 minutes after it is switched on, every time; the
activity shows the time left. The deadline is stored as a wall clock time, and
the deadline rule treats a deadline more than 10 minutes ahead (which only a
clock change can produce) as off. So a clock change can only end blocking
early, never extend it. The stored deadline survives the app being killed and
a reboot, so the reboot test in `TESTING.md` can see whether blocking comes
back by itself; it still ends at the original time.

Ways out of a cover, at any time: the "Go to home screen" button; the Home
gesture; turning the screen off; an incoming call; waiting out the 10 minutes;
turning the service off in Accessibility settings (Settings is never covered);
and, as a last resort, a forced restart (hold the Side key and Volume down for
about 7 seconds).

### The privacy rule

LiMo records an app's name and how long it was used, never what is on the
screen. In this spike:

- the service config sets `canRetrieveWindowContent` to false, so the system
  gives it no window content at all, and `event.getSource()` is always null;
- the code reads only the package name and the time stamp of each event. It
  never reads event text, content descriptions, class names or node trees;
- there is no key event filtering, no gestures, no touch exploration;
- `flagIncludeNotImportantViews` is set. It only widens which window state
  events arrive (an app's root view is often judged "not important for
  accessibility", and its events could otherwise be dropped). It gives no
  content, because content retrieval is off;
- the log holds package names, times and decisions, nothing else.

## Build

Needs JDK 17 and the Android SDK with platform `android-36` and build tools
`35.0.0` (both already on the build PC). In this folder:

```
.\gradlew.bat assembleDebug
.\gradlew.bat testDebugUnitTest
```

The APK is `app\build\outputs\apk\debug\app-debug.apk`.

Versions: Gradle 8.14.3 (wrapper), Android Gradle Plugin 8.11.0, Kotlin
2.1.20, compileSdk and targetSdk 36, minSdk 26. No third party libraries in
the app; the unit tests use JUnit 4.13.2, which does not ship in the APK.

`local.properties` is not needed when `ANDROID_HOME` is set, as it is on the
build PC. Android Studio writes it by itself when the folder is opened. To
make it by hand, put one line in a new file `local.properties` in this
folder, with your own SDK path, for example
`sdk.dir=C\:\\Users\\you\\AppData\\Local\\Android\\Sdk`. It is listed in
`.gitignore` and must never be committed.

The unit tests (`CoverRulesTest`) cover the pure rules: which events count,
what may be covered, the whole cover decision, the 10 minute deadline rule,
the time left text, parsing the adb package list, the median and the slowest.

## Driving it from a PC

Install, from this folder:

```
adb install -r app\build\outputs\apk\debug\app-debug.apk
```

Set the blocked list and switch blocking on (a fresh 10 minutes). The list is
package names separated by commas, with no spaces:

```
adb shell am start -n ai.lucidloom.limo.spike02/.MainActivity --es block "com.android.chrome,com.google.android.youtube" --ez on true
```

Switch blocking off:

```
adb shell am start -n ai.lucidloom.limo.spike02/.MainActivity --ez on false
```

Empty the blocked list:

```
adb shell am start -n ai.lucidloom.limo.spike02/.MainActivity --ez clear true
```

Empty the list of cover timings (the activity's "Clear timings" button does
the same):

```
adb shell am start -n ai.lucidloom.limo.spike02/.MainActivity --ez clear_timings true
```

Extras can be combined in one command. `block` replaces the whole list.
Names that are not valid package names, and
packages that are never covered, are left out and logged as `refused`. Each
command brings the activity to the front, which never gets covered. The
extras are applied once; turning the phone does not apply them again.

Do not use `am start -S` or `am force-stop` with this app: force stopping it
is one of the things under test, and Android may turn the service off when
an app is force stopped.

Read the log:

```
adb logcat -s LiMoSpike
```

Uninstall:

```
adb uninstall ai.lucidloom.limo.spike02
```

### Log lines

All lines use the tag `LiMoSpike`:

| Line | Meaning |
| --- | --- |
| `cover package=P shown_ms=N method=M trigger=T` | A cover was shown for P; N ms from the start (see above) to its first frame. `trigger` is `event`, `unlock` or `state`. |
| `cover package=P relabel` | A cover already up now names another blocked app. |
| `uncover package=P reason=R` | The cover was removed. R is one of the reasons listed above; `front_changed` also names the new package. |
| `event package=P counted=yes` or `no` | A window state event, and whether it counted as a change of the app in front. |
| `home pressed package=P` | "Go to home screen" was tapped. |
| `state blocked=...`, `state blocking=on reason=...` | The list or the switch changed, and why: `switch`, `intent`, `safety_stop` or `deadline_rule`. |
| `state timings=cleared` | The list of timings was emptied. |
| `intent ...`, `refused package=P reason=R` | Extras received from adb, and names left out. |
| `service connected ...`, `service disconnected` | The service started or stopped. |
| `phone android=... installing_package=... package_source=... debuggable=...` | Logged when the activity starts. |

## Unknowns

- How Android 14 and later treat this. Android 14 holds back some events
  from services that are not accessibility tools when an app marks content as
  sensitive, and later versions tighten restricted settings. Only Android 13
  is tested here.
- Whether One UI sends extra window events (from the launcher during app
  launch, or from overlays not listed above) that cause a cover to drop or
  flicker. The `event` lines will show it.
- Whether the frame commit time is close to what the eye sees. The by eye
  check in `TESTING.md` is the cross check.
- `MODE_IN_COMMUNICATION` (internet calls) is deliberately not treated as a
  call, so a call in a blocked app such as a game's voice chat does not keep
  the cover away. Whether the real product should treat calls differently is a
  later question.
- The adb extras let anyone on the phone change the list. Fine for a spike,
  never for the real product.
