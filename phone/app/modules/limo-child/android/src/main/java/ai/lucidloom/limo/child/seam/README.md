# seam

The Kotlin side of the seam between the child's screens and the phone:
every `child.<call>` request the screens send (`ChildHost` in
`phone/app/src/childSeam.ts`, sent by `src/host.ts`), answered with
exactly the shape the readers there accept, and the `change` event that
tells the screens when an answer may have changed. Kotlin, package
`ai.lucidloom.limo.child.seam`, in the child module; `host/LimoChildModule`
hands each request here.

**What it is.** A small layer that asks the engine's host (host/) for its
view, the watch (watch/) for the permissions and each app's name, and
Android for its own screens, and turns what they give into the screens'
answers. It keeps two marks of its own for setup, apart from the engine's
state, and tells the watch whenever the app's main activity is LiMo's own
screen in front.

**What it is not.** It decides nothing about rules and makes no sentence
about time, rules or a step that is off: every such word is the engine's,
passed through (the engine's README, "The child's own screens" and "The
words"). All it adds is each app's own name in place of its package. It
draws nothing (show/ does), reads no fact itself (watch/ does), and calls
the engine only through the host. Nothing here has run on a phone yet: see
[What only a phone can prove](#what-only-a-phone-can-prove).

## Contents

- [The requests](#the-requests)
- [Opening a step](#opening-a-step)
- [What is kept, and where](#what-is-kept-and-where)
- [The child's name](#the-childs-name)
- [The change event](#the-change-event)
- [LiMo's own screen](#limos-own-screen)
- [Threads and failures](#threads-and-failures)
- [What it logs](#what-it-logs)
- [Tests](#tests)
- [What only a phone can prove](#what-only-a-phone-can-prove)
- [Left open](#left-open)
- [Layout](#layout)

## The requests

Each request runs on Expo's own queue (`expo.modules.AsyncFunctionQueue`),
never on the main thread. An error reaches JavaScript with a `code`.

| Request | Answer | From |
| --- | --- | --- |
| `child.pairing` | `{ paired: false }`, or `{ paired: true, childName }` | the host: paired whenever it runs an engine (a state is kept), even one that cannot read its state and so locks the phone, with the child's name the host keeps, or `your child` (see [The child's name](#the-childs-name)). Never "not paired" while an engine runs, or the phone would keep its limits while its own screen said it was not set up |
| `child.agreed` | true or false | the seam's marks for this pairing; false when not paired. With a kept state the engine cannot read, the marks kept are taken as they are |
| `child.agree` | null | kept in the seam's marks; refused with `NotPaired` when not paired, and `CannotRead` when the engine cannot read its kept state |
| `child.introSeen`, `child.markIntroSeen` | true or false; null | the same, for the child's introduction (C1 to C3) |
| `child.steps` | `{ notifications, phone, usage, accessibility, battery, protection }`, each `"on"` or `"off"` | the watch's `permissions()`, read from Android at this moment, never remembered: notifications enabled, the Phone permission granted, usage access, LiMo's accessibility service switched on in Settings, left out of battery optimisation, the device admin active |
| `child.openStep` with `{ step }` | null, once Android's screen is open or its question answered | see [Opening a step](#opening-a-step); `NotOpened` when nothing could be opened, which the screens show as "Android did not open it. Try again." |
| `child.today` | `TodayView` | the engine's `today`, turned (below) |
| `child.rules` | `RulesView` | the engine's `rules_words`, turned (below) |
| `child.maker` | a string | `Build.MANUFACTURER` exactly as Android gives it, such as `samsung` |
| `child.askForMoreTime` | null, once it opened | opens LiMo's ways out screen at asking for more time, with no lock behind it (`show.WaysOutActivity.openAsking`); `NotPaired` when not paired |
| `about` | `{ form, module, android, sdk }` | as before |

Any other name is refused with `UnknownRequest`. The screens read the
app's version from its own config, so no request asks for it.

**Today, from the engine's `today`:**

| Seam | Engine |
| --- | --- |
| `left.kind` | `left` is `left`, `no_limit` is `no-limit`, `open` is `open`; a kind the seam does not know is refused, never guessed |
| `left.words`, `used`, `where`, `bedtime` | `left.words`, `used`, `where_used`, `bedtime`, as they are |
| `usedPart` | `used_part`, a number, or null with no limit |
| `apps` | `apps`, in the engine's order (longest first), each `{ name, minutes }`: the app's name from the watch for its `package`, and the engine's `minutes`. An app with no name is left out, never shown as its package |
| `guardianNeeded` | `guardian_needed.off`, as two lists that line up: `off`, the steps in setup order (`usage_access` is the step `usage`), and `words`, the engine's sentence for each. A permission the seam does not know is left out with its sentence; nothing left is null |

**The rules, from the engine's `rules_words`:** `timeADay` and
`screensOff` (null when no hours are off) as they are; `alwaysOpen` and
`notAllowed` as the apps' names, in the rules' order, an app with no name
left out.

**The apps' names** come from the watch's `launchableApps()`: the apps a
child can open, each with its label as the watch cleaned it. An app that is
not installed, or has no label, has no name. The list is slow to make (it
loads every label), so it is kept while the seam listens to the watch,
which says when apps change, and made again for each request otherwise.

A view that is not as the engine's README says is refused with `BadView`,
naming the field and never quoting it.

## Opening a step

What each of the six opens, on each Android (`StepPlans`, tried in order
until one opens, from the app's own activity so that Back comes back to
LiMo):

| Step | Android 8 to 12 (26 to 32) | Android 13 and later (33 and up) | Resolves |
| --- | --- | --- | --- |
| notifications | LiMo's notification settings (`ACTION_APP_NOTIFICATION_SETTINGS`), else LiMo's page in the list of apps | Android's question for `POST_NOTIFICATIONS`; where Android will not ask (refused for good, or granted and switched off in Settings), the pages before | once the question is answered; a page once open |
| phone | Android's question for `READ_PHONE_STATE` ("make and manage phone calls"); refused for good, LiMo's page in the list of apps (`ACTION_APPLICATION_DETAILS_SETTINGS`), where Permissions, Phone can be allowed | the same | the same |
| usage | the list of apps with usage access (`ACTION_USAGE_ACCESS_SETTINGS`) | from Android 10 (29), LiMo's own usage access page (the same action with LiMo's package), else the list | once open |
| accessibility | Android's accessibility settings (`ACTION_ACCESSIBILITY_SETTINGS`), with the hint many phones' Settings read to scroll to LiMo's row and light it up (`:settings:fragment_args_key`, the service's component). No app may open another's accessibility page itself: Android keeps that page for its own apps | the same | once open |
| battery | Android's question whether LiMo may run in the background (`keep/Battery.question`, `ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS`), else the list of apps left out | the same | once it answers |
| protection | Android's page asking to make LiMo a device admin app (`ACTION_ADD_DEVICE_ADMIN` for `keep/AdminReceiver`), which shows the admin's own description (below) | the same | once it answers |

**A question Android will not ask.** The two permission questions go
through Expo's permission service, which notes each one asked and whether
Android will ask again. When it will not, the step's page is opened in
place of the question. When the answer comes back no and Android had no
reason to ask again (LiMo had asked before, Android offered no second
chance before asking, and will not ask now), Android said no without
showing anything, and the page is opened then too. A no the person gave in
the question is left as it is.

**A screen that answers** (battery, protection) is started for a result;
the step resolves when the result comes, or when the app is back in front
if it never does, so the screens never wait for ever.

**The device admin's words.** `limo_admin_description` in
`res/values/limo_strings.xml`, which Android shows on that page and in its
list of device admin apps, says exactly what `keep/AdminReceiver` does
today: it holds no policy at all, so it cannot wipe the phone, change or
use the screen lock, or switch off the camera; it is asked only so that
LiMo cannot be removed while it is on; and switching it off in Settings
lets LiMo be removed. No separate explanation is passed with the request,
so the page does not say the same twice.

## What is kept, and where

The seam keeps one small file of its own, the app's private
`files/limo-seam/setup.json`, written all or nothing with Android's
`AtomicFile`, apart from the engine's state (`files/limo/state`, which it
never touches):

```
{"device":"<the engine's device id>","agreed":true,"intro_seen":false}
```

The marks belong to one pairing: they count only while the engine's view
names the same device id. Paired again (a new welcome, a new device id),
the guardian agrees again and the child sees the introduction again. The
child's name is not here: the host keeps it (below). Like everything of the app, the
file is never backed up and never moved to a new phone (`allowBackup` is
false and the data extraction rules exclude everything). A file that cannot
be read counts as none: setup starts again.

## The child's name

The host keeps it (`PhoneHost.childName`, host/README.md, "The child's
name"): the pairing gives it, and the child's screens and the lock read it
from that one place. The seam keeps no name of its own.

| The host has | `child.pairing` answers |
| --- | --- |
| an engine and a name | `{ paired: true, childName }`, the name as the host keeps it |
| an engine and no name | `{ paired: true, childName: "your child" }` |
| no engine | `{ paired: false }` |

**"your child" is the fallback**, plain words that read in every sentence
the screens make with a name: "This phone is now your child's.", "I am
your child's guardian, and I agree.", "Hand the phone to your child.". It
can happen only when the host kept no name for this pairing: its file was
lost or damaged, could not be written at pairing, or the pairing gave none.
Pairing the phone again puts the name back. It is never "not paired" while
an engine runs: the phone would then keep its limits while its own screen
said it was not set up.

## The change event

The module sends `change` with `{ what }`, `what` one of `pairing`,
`agreement`, `steps`, `intro`, `today`, `rules`, whenever an answer may
have changed:

| Told by | What may have changed |
| --- | --- |
| the host: the phone paired or forgotten | pairing, agreement, intro, today, rules |
| the host: the view (the time left's words, the cover, the permissions, the engine's notes, a way out) | today, rules |
| the watch: a permission | steps, today |
| the watch: apps installed, removed or changed (the names are made again), the clock, the zone | today, rules |
| the seam itself: agreeing, the introduction seen | agreement, intro |
| the app back in front | everything, each sent only if it changed while away |

**What throttles it** (`SeamChanges`):

- **Only while the screens listen and the app is in front.** The seam
  listens to the host and the watch only between Expo's
  `OnStartObserving` and `OnStopObserving` for `change`, and only while the
  app's activity is in front. Nothing runs otherwise.
- **Gathered.** What may have changed is noted and looked at 150 ms later,
  so a burst of signals is one look.
- **At most once a second.** A look comes at least a second after the one
  before. The engine's words change once a minute while time is counted,
  and every second only in the last minute, when Today shows the seconds.
- **Only what really changed.** At each look the seam works out the answer
  the screens would get now for each kind noted (one view from the host,
  one list of names for all of them) and sends a kind only when its answer
  differs from the one it last sent. When the screens begin to listen,
  the answers then are taken as sent, since the screens have just asked.

## LiMo's own screen

The watch takes LiMo's own word that one of its screens is in front
(`PhoneWatch.limoScreenShown`, watch/README.md, rule 10): a fact, which
holds with usage access off too, when the front would otherwise be
unknown, and so counted and maybe covered. Usage access is off all through
setup until its third step, and again whenever someone switches it off.
The ways out screen (`show/WaysOutActivity`) gives its own word; the app's
main activity (setup, the child's introduction, Today, How it works) gives
it through `OwnScreen`, from the two hooks Expo hands `LimoChildModule`
(`OnActivityEntersForeground`, `OnActivityEntersBackground`). The rules:

- **In front: true. Leaving it: false.** So a minute on setup or on Today
  is never counted, Today's "Looking at this screen does not use your
  time." holds, and no cover stands over LiMo's own screens.
- **At once, on the main thread, in order.** The watch holds one word for
  all of LiMo's screens, the last given. When the ways out screen opens
  over Today, Today's pause says false and then the ways out screen's
  resume says true; a false given later, through another thread, would
  land after that true and leave the ways out screen unknown and covered.
  So the word is told on the thread that hands it, before the hook
  returns, before anything else the hook does.
- **Never in split screen.** In split screen, or a floating window
  (`Activity.isInMultiWindowMode`), the word is false: another app shares
  the screen, and "LiMo is in front" would let it run uncounted and
  uncovered with usage access off. Unknown is the safe answer there. The
  main activity handles its own size changes (its `configChanges` take
  `screenSize`, `screenLayout` and `smallestScreenSize`), so it can enter or
  leave split screen without a pause, and on Android 10 and later both
  halves stay resumed. Android tells only the activity itself
  (`onMultiWindowModeChanged`), which the app's generated main activity
  does not pass on, so `OwnScreen` watches the activity's window instead:
  each time it is laid out again (as it is when its size changes), it looks
  at `isInMultiWindowMode` and says the word again if that changed.
- **Never throws into the activity.** The watch is told inside a guard,
  and so is everything the two hooks do in `LimoChildModule`.

**On which thread Expo calls the two hooks, as read in the sources.**
React Native calls its lifecycle listeners on the main thread, inside the
activity's own `onResume` and `onPause` (`ReactActivityDelegate` to
`ReactDelegate` to `ReactHostImpl.onHostResume` and `onHostPause` to
`ReactLifecycleStateManager` to `ReactContext.onHostResume` and
`onHostPause`, all `@ThreadConfined(UI)`, in
`node_modules/react-native/ReactAndroid/src/main/java/com/facebook/react/`).
Expo's `AppContext.onHostResume` and `onHostPause`
(`node_modules/expo-modules-core/android/src/main/java/expo/modules/kotlin/AppContext.kt`)
post `ACTIVITY_ENTERS_FOREGROUND` and `ACTIVITY_ENTERS_BACKGROUND` to each
module on that same thread, at once (`ModuleRegistry.post`, then
`ModuleHolder.post`), so a word given there keeps its place.

**The first resume of a cold start is not missed, but may come late, on
another thread.** React Native starts after the activity has resumed. Two
paths bring that resume to Expo, and both reach the module: when React
Native's context is ready it calls `ReactContext.onHostResume` for an
activity already resumed (`ReactLifecycleStateManager.resumeReactContextIfHostResumed`,
from `ReactHostImpl`), and a listener added to a context already resumed
is called at once by `ReactContext.addLifecycleEventListener`, through
`runOnUiQueueThread` (`ReactContext.java`). Either way on the main thread.
But Expo's `ModuleRegistry` holds every event that comes before its modules
are created (`addToQueueIfNeeded`) and hands them over, in order, from
`postOnCreate`, which `NativeModulesProxy.getConstants` runs when
JavaScript first reaches Expo's modules
(`node_modules/expo-modules-core/android/src/main/java/expo/modules/kotlin/ModuleRegistry.kt`,
`.../adapters/react/NativeModulesProxy.java`): on that thread, not the
main one. So `OwnScreen` takes a word that comes on another thread over to
the main thread, and takes it only if no word came in order on the main
thread meanwhile (any such word is newer: events are held only until the
modules exist). A pause between the two is never undone by a late resume.

## Threads and failures

Requests run on Expo's queue; the change event's work on the seam's own
thread ("LiMo seam"); Android's screens are opened on the main thread. The
host answers on the main thread, so the seam waits for its view on its own
threads only, at most five seconds (`NoAnswer`). Nothing here throws into
Android: a request that fails is refused with a code (`NotPaired`,
`CannotRead`, `BadView`, `BadArgs`, `NotOpened`, `NoAnswer`, `Failed`), and
a failure in the change event's work is logged.

## What it logs

Android's log, tag `LiMo` (`adb logcat -s LiMo`), a line per step opened,
in words that hold no name, label or sentence:

| Line | When |
| --- | --- |
| `seam step=<step> asked granted=<true, false>` | A permission question was answered. |
| `seam step=<step> answered` | Battery's or the device admin's screen answered. |
| `seam step=<step> opened page=<page>` | A settings page opened (`app_notifications`, `app_details`, `usage_for_limo`, `usage_list`, `accessibility_list`, `battery_list`). |
| `seam step=<step> not_opened` | Nothing could be opened. |
| `seam pairing no_child_name fallback` | Paired, with no name kept: the screens say `your child`. Once, and again only after a name was seen. |
| `seam failed what=<what> step=<step> error=<kind>` | Opening a step failed: `open_step` (anything while opening it, which then answers `NotOpened`) or `ask` (Expo's permission service refused to ask, and the next way was tried). |
| `seam failed what=<what> error=<kind>` | Anything else that failed: `marks_read`, `read_permission`, `ways_out`, `own_screen`, or the change event's `look`, `note`, `front`, `listening`, `stop`. |

## Tests

`npm run test:child` (in the app) runs them with the module's other JVM
tests. The seam's answers are made from the engine's REAL view, through the
bindings that ship, from a made up family (the testkit's), as the other
tests do; a day with no limit and hours off are set in the welcome, as a
guardian would set them on the PC.

| Test | What |
| --- | --- |
| `SeamTodayTest` | A day with a limit, in the engine's words, each app by its name, the nameless app left out and the uninstalled casino left out of the rules; a day with no limit (no bar); a guardian's unlock ("Open until 6:00 pm"); hours off on Today and in the rules, and in 24 hours; three and then all six permissions off, as the steps in setup order with the engine's sentences, and none once on; long names as they are; a view that is not the engine's refused by its field, a kind it does not know refused, a permission it does not know left out |
| `SeamRequestsTest` | The three answers to the pairing: not paired (not set up, nothing kept, Today and the rules refused, the steps and the maker answered), paired with a name (named, agreeing and the introduction kept for this pairing and told, paired again starting again with the new name), and paired with no name (`your child`, logged once, setup going on); a kept state that cannot be read still paired, with nothing to show; each step on only when Android says so in this reading; the change event's answers asking the host and the watch once for all; the marks file |
| `SeamChangesTest` | A burst gathered into one look; at most one look a second; only what changed sent; a failure sent once; one order; what the host's and the watch's words may change |
| `StepPlansTest` | Every step on every Android from 8.0 to 16: the question or the page, refused for good, granted and switched off; when a page follows a question |
| `OwnScreenTest` | The main activity's word to the watch's real core, with usage access off: in front it is LiMo, leaving it unknown; the ways out screen opened over Today and closed again, each said in Android's order, LiMo throughout, and the same false told late leaving the ways out screen unknown; split screen false, entered and left with no pause through the window's layout, a layout that changed nothing saying nothing; the first resume of a cold start handed over late, taken, or dropped when a pause came in order first; a watch that fails never throwing into the activity |
| `OwnScreenThreadsTest` (Robolectric) | On the main thread the watch is told before the hook returns, nothing left waiting; a word from another thread waits for the main thread, and a newer one in order wins; an activity in split screen says false, and its window being laid out says true and false again as it leaves and enters split screen without a pause |
| `SeamHouseRulesTest` | No log line carries a name, a label, a sentence or the view; no sentence about time is written here; no dash punctuation |

**The contract with the screens.** Each answer the tests above make is
also written as a small JSON file into the app's `tests/seam-answers/`
(`{ "request": ..., "answer": ... }`, made up names only), and the app's
jest test `tests/seamContract.test.ts` reads every one through the readers
in `src/childSeam.ts`. A change on either side that breaks the other fails
there. **To refresh the files**, run `npm run test:child`: every run writes
them again, and a file changes on disk only when its content does, so `git
status` shows any answer that changed. Then `npm test`. A file the jest
test does not expect, or one missing, fails it too.

What no test here covers: Expo's own turning of the answers (Kotlin maps,
lists, strings, numbers and nulls) into JavaScript values, and Android
opening its screens. Both need a phone.

## What only a phone can prove

For the owner, on the Galaxy S20 Ultra (Android 13, One UI 5.1), with the
debug build paired by the development tool and `adb logcat -s LiMo`. Each
step is checked twice: the stamp says NOT ON YET until Android's own
screen is used, then ON once back in LiMo.

1. **Paired, by itself.** With S0 showing, pair the phone from the PC: S1
   comes by itself, naming the child (the `change` event). If it says "This
   phone is now your child's.", the host kept no name: the log says `host
   paired no_child_name` and `seam pairing no_child_name fallback`.
2. **Agreed, and kept.** Tick and Continue on S2, swipe LiMo away, open it:
   it starts at the first step not on, not at S1.
3. **Notifications.** "Allow notifications": Android's own question over
   LiMo, "Allow LiMo to send you notifications?", Allow and Don't allow.
   Tap Allow: ON (`seam step=notifications asked granted=true`). On a
   second phone or after clearing LiMo's data: Don't allow twice, then the
   step's button opens LiMo's notification settings instead
   (`opened page=app_notifications`), with its main switch to turn on.
4. **Phone.** "Allow": Android's question, "Allow LiMo to make and manage
   phone calls?". Allow: ON. Refused twice, the button opens LiMo's App
   info page, where Permissions, then Phone, then Allow.
5. **Usage access.** "Open Android's settings": on Android 13 LiMo's own
   usage page should open, with one switch ("Allow usage tracking" on One
   UI), `opened page=usage_for_limo`. If the log says `usage_list`, One UI
   took only the list (Settings, Apps, Special access, Usage data access):
   find LiMo there. Switch on, Back: ON.
6. **Accessibility.** Android's accessibility settings open; look whether
   LiMo's row is scrolled to and lit up (the hint), or whether One UI shows
   its top page (then Installed apps, LiMo). Switch on, accept One UI's
   warning, Back twice: ON. If the switch is greyed out as a restricted
   setting, note it: App info, the menu at the top right, Allow restricted
   settings (an install by cable is not expected to need it).
7. **Battery.** "Allow": Android's question over LiMo, "Let app always run
   in background?". Allow: ON at once (`seam step=battery answered`). The
   Samsung note shows under it on this phone.
8. **Protection.** "Turn on protection": Android's page "Activate device
   admin app?", with LiMo's words exactly as in `limo_admin_description`
   and no list of powers (or an empty one). Activate: ON
   (`seam step=protection answered`). Then try to uninstall LiMo from the
   home screen: One UI must refuse until it is switched off under Device
   admin apps, which shows the same words.
9. **A step Android will not open.** None is expected; a step that logs
   `not_opened` shows "Android did not open it. Try again.". Note which.
10. **Done, the introduction, Today.** S9 ticks each step; C1 to C3; Got
    it shows Today. Swipe LiMo away and open it: Today at once.
11. **Today and How it works in the engine's words.** Play a game for two
    minutes, open LiMo: the time left matches the notification, the game
    is listed by its own name, an app with no icon is not. How it works
    lists the always open and blocked apps by name, leaving out any not
    installed.
12. **Changes while Today shows.** In split screen with Settings, switch
    usage access off: within a second or so the banner says "Usage access
    is switched off, so LiMo cannot count the time in each app." Switch it
    on again: the banner goes. With Today showing, forget the pairing from
    the PC: S0 comes by itself.
13. **Ask for more time.** Tap it on Today: LiMo's ways out screen opens at
    asking for more time, with no lock strip at its top (show/README.md
    says what it shows and does). Back closes it and Today shows again.
14. **LiMo's own screens are not counted with usage access off.** Switch
    usage access off for LiMo in Android's settings and come back to LiMo.
    Run `limo-phone-dev status` on the PC and note the time used today;
    stay a full minute on LiMo's setup screens (forget and pair the phone
    again to see them) and a full minute on Today, then run `status` again:
    the time used today has not moved. Meanwhile the notification says the
    time is paused ("paused" after the time left), and the watch's log
    (`adb logcat -s LiMoWatch`) says `front now limo`. Then open a game for
    a minute: that minute is counted.
15. **The ways out screen over Today, with usage access off.** With usage
    access off, tap "Ask for more time" on Today: the ways out screen opens
    and nothing covers it, also when the day is used up and the phone is
    locked; Back shows Today again, never covered.
16. **Split screen.** With usage access off, put LiMo in split screen with
    a game: the minutes in the game count (`status`), and LiMo is not said
    to be in front (the watch's log says `front now unknown`, not `front now
    limo`, while it shares the screen). Take it out of split screen without
    leaving it: `front now limo` again. If
    either does not happen, the window's layout did not tell `OwnScreen`
    (see "Left open").
17. **Expo's turning of the answers.** Every screen above showing its words
    is the proof that maps, lists, strings, numbers and nulls reach
    JavaScript as the readers expect.

## Left open

- **The protection step's words** say LiMo "cannot be switched off or
  removed without the guardian PIN". Today the device admin only stops
  LiMo being removed while it is on, and anyone can switch it off in
  Settings; the base protection has to make the step's words true, or they
  change. The admin's own description says only what is true now.
- **Split screen without a pause** is caught from the main activity's
  window being laid out again, not from Android's own word for it
  (`Activity.onMultiWindowModeChanged`), which only the activity hears and
  the app's generated main activity does not pass on. That a layout always
  follows the change, and that `isInMultiWindowMode` already says the new
  mode then, is what a phone must show (check 16). Passing Android's own
  word on would need a change to the generated main activity through a
  config plugin.
- **Android's own screens opened from LiMo are not LiMo.** A permission
  question over LiMo, the battery question, the device admin page and
  Settings pause the main activity, so with usage access off the front is
  unknown while they show: counted, and under a lock covered. That is the
  watch's rule for any other app, kept here; it means the walk from
  Today's banner into Settings to switch usage access back on runs under
  the lock when the day is used up.
- **Accessibility, twice.** The step is on when the service is switched on
  in Settings; the engine's banner when it is switched on and connected (or
  was switched on less than a minute ago). The two can differ for a moment
  after a restart, never for long.
- **Restricted settings** (Android 13 and later) hold back the
  accessibility switch for an app installed from a file on the phone.
  LiMo is installed over the cable; a family that installs it otherwise
  will need App info, Allow restricted settings. Not handled here.
- **A permission refused for good on its first question** (Android 11 and
  later, tapping outside the question) counts as refused for good in Expo's
  permission service, so the next tap opens the step's page rather than
  asking again. Harmless: the page grants it too.

## Layout

```
seam/
  README.md
  ChildSeam.kt     the requests answered with data, and the small interfaces they ask (host, phone, marks, log)
  SeamAnswers.kt   the turning of the engine's view and the watch's readings into the screens' shapes, pure
  SeamJson.kt      a small strict JSON reader for the engine's view, pure
  SeamChanges.kt   the change event's throttle, pure
  StepPlans.kt     what each step opens on each Android, pure
  StepOpener.kt    opening Android's screens and questions, and the ways out
  OwnScreen.kt     whether the main activity is LiMo's own screen in front, for the watch
  Seam.kt          the seam of the process: its thread, the host and the watch, the marks file, the change event, the own screen
src/test/java/ai/lucidloom/limo/child/seam/
  SeamRig.kt       the engine's phone, the host and phone as the seam sees them, JSON out, the answer files
  SeamTodayTest.kt, SeamRequestsTest.kt, SeamChangesTest.kt, StepPlansTest.kt, OwnScreenTest.kt,
  OwnScreenThreadsTest.kt, SeamHouseRulesTest.kt
```
