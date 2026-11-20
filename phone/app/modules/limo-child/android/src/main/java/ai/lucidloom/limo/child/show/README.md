# show

What a child sees from LiMo's Kotlin on the phone: the block screen, the
lock, the warning that floats over the child's own app, and the three ways
out of a lock. Native Android views, drawn to the approved drawings
(`design/screens/phone.html`, frames A to D and F), with every word the
engine's or the drawing's. Kotlin, package `ai.lucidloom.limo.child.show`,
in the child module.

**What it is.** The engine's host hands it each order: the cover as a
level, about once a second with the screen on and at once on any change,
and a warning in the one tick it falls due. It draws the block screen and
the lock in one window of the accessibility service, the warning in a small
second one, and the ways out in an activity of LiMo's own, because a
keyboard cannot type into an overlay. The ways out talk to the engine only
through the host's three calls.

**What it is not.** It decides nothing: whether to cover, what a lock
offers, every sentence about time, rules and answers are the engine's
(`phone/engine/README.md`). It reads no fact (watch/ does), calls no engine
(host/ does), keeps nothing on disk, and holds no permission of its own.
Nothing here has run on a phone yet: see
[What only a phone can prove](#what-only-a-phone-can-prove).

## Contents

- [How the other pieces call it](#how-the-other-pieces-call-it)
- [The covers](#the-covers)
- [The lock and the block screen](#the-lock-and-the-block-screen)
- [The warning](#the-warning)
- [The ways out](#the-ways-out)
- [The look](#the-look)
- [Large text](#large-text)
- [For a screen reader](#for-a-screen-reader)
- [What it logs](#what-it-logs)
- [Pictures without a phone](#pictures-without-a-phone)
- [Tests](#tests)
- [What only a phone can prove](#what-only-a-phone-can-prove)
- [Left open](#left-open)
- [Dependencies and licences](#dependencies-and-licences)
- [Layout](#layout)

## How the other pieces call it

```kotlin
CoverShower.install(app) { Host.of(app).childName } // keep/Keeper.start, each time it runs; only the first call does anything
CoverShower.canFloatWarning()                       // keep/: a card can float now, so the notification stays quiet
WaysOutActivity.openAsking(context)                 // seam/: asking for more time from Today, with no lock behind it
```

| Call | What |
| --- | --- |
| `install(context, childName)` | Makes the process's shower and gives it to the host (`Host.of(context).useShower`). Called again, it does nothing. `keep/Keeper.start` calls it every time it runs, before it starts the host, with the host's own child's name (`Host.of(app).childName`); from then on the host's orders are drawn. `childName` gives the child's name for the lock's label, or null; it is read at each order and never logged. |
| `show(cover)` | The host's `Shower`: the engine's `Cover`, on the main thread, as a level. |
| `warn(warning)` | The host's `Shower`: a warning, once. |
| `canFloatWarning()` | True while the accessibility service is connected, so the warning floats as a card. Any thread. |
| `WaysOutActivity.openAsking(context)` | Opens asking for more time with no lock behind it, so with no strip at the top. Today's "Ask for more time" reaches it through the seam: the screens' request `child.askForMoreTime` (seam/README.md) calls it. |

The lock opens the ways out itself: a tap on a row opens the activity for
that way, with the lock's reason for the strip at the top and the row's
title for the heading.

## The covers

**One window, as proof two drew it.** The block screen and the lock are a
`TYPE_ACCESSIBILITY_OVERLAY` window added through the connected
accessibility service (`WatchService.current`), so they need no permission
of their own. It is opaque and fills the whole screen with Android's bars
(`fitInsetsTypes = 0` and the cut out always, from Android 11; the cut out
from Android 9). It is not focusable: the app under it keeps the keys and
the keyboard, and a person's swipes for Home and Back still reach Android.
Its content keeps clear of the bars and the cut out by the insets Android
gives it, and by Android's own bar sizes when it is given none.

**A level.** `ShowerCore` decides, with no Android in it:

| The order | What happens |
| --- | --- |
| The same plan as the one drawn | Nothing is built again; only the clock at the top may move. |
| A different plan | The window that is up shows the new one (built again in one pass, scrolled where it was when the kind is the same). |
| Nothing | The window is taken away at once (`removeViewImmediate`). |
| A window Android took away (the service switched off) | It is drawn again at the next order. |
| No service connected | Nothing can be drawn; the next order tries again. |

A plan is the order as the screen needs it (`CoverPlan`), so two orders
that would draw the same screen are equal. The engine already orders
nothing over a ringing call, the diallers, the emergency dialler, LiMo's
own screens, Android's lock screen and a dark screen; a cover that is up
comes down at once when its order ends.

**Stepping aside.** A cover is above every app, so when the child taps
something on it that opens another screen (Emergency call, Back to the
call, an app that stays open, the block screen's home button, a way out)
it steps aside at once rather than hide that screen until the engine's next
order says it may go. It follows the orders again as soon as one differs
from the order it stepped aside from (the engine has seen the new screen),
or after three seconds if none does. It steps aside only once the screen
did open: a start Android refused leaves the cover where it is. While the
ways out are on the screen nothing is drawn; when they close, the orders
lead again, and the lock is back as soon as the engine sees the app under
it (the watch confirms the app in front within about a second). Nothing
here ever draws against an order of nothing: a cover kept from before
never stands over a ringing call, a dialler or LiMo.

**Turns, folds, font sizes.** The window's root builds its screen again on
every configuration change (a turn, a fold, the font size, the density),
and lays itself out again on new insets. Wider than 600 the page and the
pinned strip keep 600 in the middle.

**How fast.** Building and adding the window takes a few milliseconds
after the order; the order itself comes from the host's tick, at once on
the watch's change. Proof two measured 25 to 226 ms from an app's window to
its cover on the Galaxy, with a window event alone. Here the watch first
confirms the app by Android's usage record, at once when the record is
written, else at 100, 300 or 1,000 ms; so expect about the same as proof
two, and up to a second when the usage record is late. Each new window logs
`show cover drawn ms=` from the order to its first frame (Android 10 and
later), for the owner's check.

## The lock and the block screen

**The lock (A, A2, C).** From the top: LiMo and the clock (the phone's own
time in 12 or 24 hours as it shows them, and the date, "WED 18 NOV"); the
reason card (the reason's picture at 64, the engine's stamp tilted, the
label, the engine's title and message, "Opens again" and the engine's
until words); "What you can do" with the child's request that waits, the
wait after wrong tries, and a row for each way out the engine offers (none,
one or three) or, with none, the engine's two lines; "Still open" with the
apps that stay open; the plain line and the maker's mark. Pinned to the
bottom: Emergency call, and during a call the engine's "Back to the call"
in teal above it. The page scrolls above the pinned strip on a small phone.

| The engine's order | The plan (`Plans.lock`) |
| --- | --- |
| `why` | The picture: an hourglass for time used up, a moon for bedtime, a calendar for no time today, sync while more time comes, away, devices, a lock for a guardian's lock, an alert when LiMo cannot check, block. |
| `ways` | One row each, in order, with the engine's title and line; one that must wait is dashed, pale, and cannot be opened. |
| `nothingToDo` | Only when no row is offered: its two lines. |
| `requestLine`, `latestRequest` | A dashed note with the stamp, ink while waiting, teal when given, coral for not this time. |
| `waitWords` | A coral note. |
| `allowedPackages` | Tiles, each the app's own icon and name from Android; the default dialler first (the phone is always usable under the lock); each once; only apps Android can open; LiMo left out. With none, no "Still open". |
| `callOn`, `backToCall` | Back to the call pinned above Emergency call. |

**The label.** "MIRA · THIS PHONE" when the host knows the child's name, and
"THIS PHONE" alone when it does not; nothing else on the screen moves. The
name is never logged.

**The block screen (D).** The same top; a card in the middle of what is
left, with the block picture, the stamp BLOCKED, the label, the engine's
title naming the app as the engine gives it and its message, and one ink
button "Go to the home screen" (the accessibility service's own Home
action, or Android's home screen if the service cannot); the line "Nothing
else was closed." and the maker's mark; Emergency call pinned, and Back to
the call above it during a call.

**The taps.** Emergency call tells the watch first
(`emergencyDiallerOpening`), then opens Android's emergency dialler
(`com.android.phone.EmergencyDialer.DIAL`, then `DIAL_EMERGENCY`, then the
dialler). Nothing dials by itself. Back to the call shows the call's own
screen with `TelecomManager.showInCallScreen` when the Phone permission is
granted, and the dialler otherwise. A tile opens its app; the engine then
finds an allowed app in front and orders nothing.

## The warning

**The card (F)** floats at the top of the screen, 8 under the status bar
and 12 from the sides, over the child's own app: the bell, the engine's
message and hint, a dashed line, the engine's when ("LOCKS ABOUT 9:47 PM")
and OK. It goes by itself after thirty seconds, or at OK; a new warning
takes its place with thirty seconds of its own; a cover that comes up takes
it away. Its window is no larger than the card and its shadow, is not
focusable (the app under it keeps the keyboard), and lets every touch
outside it through (`FLAG_NOT_TOUCH_MODAL`).

**Its two forms.** keep/ posts each warning as a notification too. So
that it is not shown twice, keep/ asks `canFloatWarning()`: while the card
can float, the notification waits quietly in the shade (no heads up, sound
or vibration: it is in a group that alerts only for a summary, which is
what Android's own `setSilent` does from Android 12); while the
accessibility service is not connected, it is the heads up notification as
before. See keep/README.md.

## The ways out

`WaysOutActivity`, LiMo's own activity, declared in the module's manifest:
not exported, its own task, left out of the recent apps.

| Screen | What |
| --- | --- |
| B1, asking | How long (15 min, 30 min, 45 min, 1 hour), Why with the phone's own keyboard, the counter ("28 / 200", as the rules core counts characters), and "Send request" pinned above the keyboard. |
| B2, a code | The eight cells, the room for the engine's words, the keys, and "Use this code" pinned. |
| B3, the PIN | In the phone's order: what to give (add 15 min, 30 min or 1 hour; or keep it open for 1 hour, 2 hours or today), then the PIN as dots, then the keys, and "Unlock" pinned. "Today" keeps it open to midnight on the phone's clock, a minute at least, as the PC's lock does. |

The reason stays in sight as the strip at the top of the card (the lock's
picture, its title and "Opens tomorrow, 7:00 am"); Emergency call, the
plain line and the maker's mark come at the end of the page. The model is
`WaysOutModel`, with no Android in it; the screen only shows it.

| The engine's answer | What the screen does |
| --- | --- |
| A request made | Back to the lock, which shows it waiting. |
| A refusal (the core's words, such as "A request from this device is still waiting for an answer.") | Stays, the words in coral under the field. |
| Added, unlocked, a release confirmed | The engine's words in teal ("15 minutes added. The phone opens now.") for a second and a half, then closes; the lock is there still if the phone is still locked (time added at bedtime). |
| Wrong, with tries left | Stays, the engine's words in coral; typing again clears them. |
| Wrong, the fifth, or must wait | Back to the lock, which shows the wait with the code and the PIN greyed. |
| A code of the wrong form, used already, nothing set up | Stays, the engine's words. |
| Not paired, or failed inside LiMo | Back to the lock. |

**Secrets.** A typed code or PIN lives in one char array, never a string;
the call turns it into the bytes the host takes (which the host wipes once
the engine has read them) and fills the array with zeros; Back, leaving the
screen and closing fill it too. It is never logged and never saved with the
screen's state: a screen rebuilt after a turn of the phone starts with
empty cells. The window is secure (`FLAG_SECURE`): it cannot be captured,
and from Android 13 the recent apps list shows no picture of it either.

**Rebuilt in the middle of a call.** A call's answer may come after the
screen that asked was rebuilt; a relay (`CallRelay`) holds it by the call's
number, which the screen keeps in its saved state, and hands it to the new
screen.

**Back** goes to the lock (the system Back, and the Back button). Leaving
the screen any other way (Home, a call) closes it. The screen tells the
watch when it is shown and hidden (`limoScreenShown`), and the cover, which
stands aside while it is up.

## The look

The approved drawings, measured: every size, gap, corner, border and
shadow is in one table, `Look.kt`, in density independent pixels, so that
390 by 844 on the drawing is 390 by 844 dp on a phone; type sizes are in
scaled pixels. `LookTest` checks the table against `design/tokens.json`
and `design/screens/phone.css` themselves.

- **Colours** are the theme's nine and the drawings' washes.
- **Type** is the three bundled fonts, at the weight the drawing sets and,
  for Fraunces, its optical size at the size it is set; on a phone they are
  the app's own variable fonts (`res/font/limo_font_body`, `_mono`,
  `_display`), found by name. Each line is exactly the drawing's line
  height, with the text centred in it as CSS does. The reason's title and
  the plain line are balanced, and the reading text never leaves one word
  alone on its last line, as the drawings set them (`text-wrap: balance`
  and `pretty`). A time never breaks before its am or pm.
- **Borders** are 1.5 inside every box, corners 7, 10 or 14, the hard
  shadow 4 (cards) or 3 (buttons and rows) down and to the right with no
  blur, dividers dashed, the page dotted every 18.
- **Icons** are the design's set as vector drawables (`limo_icon_*`, made
  by `scripts/make-design.mjs`); the small marks (the arrows and the key
  that removes a digit) are ink strokes drawn as the drawings draw them.
- **The maker's mark** is the LucidLoom AI logo at 22 in its own colours,
  between "MADE BY" and the name. On a phone it is the app's own picture,
  found by name; see [Left open](#left-open).

## Large text

Type follows Android's font size; boxes grow with their words and keep at
least 48 to tap. Tried at 1.3 and 2 times, in the pictures below:

- **1.3**: everything as drawn, a little larger; the lock scrolls a little
  above its pinned buttons, as a small phone does.
- **2**: titles and rows take two or three lines; the chips' words wrap
  ("15 / min"), each row of chips and keys keeping one height; the pinned
  strip grows with its buttons ("Back to the call" and "Emergency call"
  take about 230 of the 844 together) and the page scrolls above it.
  Nothing is cut, nothing runs out of its box, and the pinned buttons stay
  in sight.

## For a screen reader

Everything to tap is a button (the chips are radio buttons, checked when
picked) with its words, at least 48 high. The lock reads in its order:
LiMo and the time, the stamp, the label, the title (a heading), the
message, when it opens, what you can do, each row, the apps, the plain
line, "Made by LucidLoom AI", then the pinned buttons. The pictures are
skipped. A cover and the warning say their title when they come up (a pane
title). On B3 the PIN box reads as "6 to 8 digits" and never says the
digits; the key that removes one says so. Words set in capitals are read as
written.

## What it logs

Android's log, tag `LiMo` (`adb logcat -s LiMo`). A line names at most a
package and the kind of a failure; never a name, a label, the engine's
words, a reason, a code or a PIN.

| Line | When |
| --- | --- |
| `show cover now block package=<package> call=<true, false>` | The block screen drawn, or changed; `call=true` while a call is on, so Back to the call shows. |
| `show cover now lock why=<the engine's reason, such as time_used_up> rows=<ways out shown> tiles=<apps still open> call=<true, false>` | The lock drawn, or changed. |
| `show cover drawn ms=<milliseconds>` | A new cover window's first frame, from the order (Android 10 and later). |
| `show cover now nothing` | A cover window taken away. Not said when none was up. |
| `show warning floated` | A warning card. |
| `show home by=<service, intent, none>` | Go to the home screen: by the accessibility service's Home action, by Android's home intent, or not at all. |
| `show emergency opened how=<emergency_dialler, dial_emergency, dial, none>` | Emergency call on a cover; from the ways out the same line ends with ` from=ways_out`. |
| `show back_to_call how=<in_call_screen, dial, none>` | Back to the call. |
| `show cover failed error=<kind>`, `show uncover failed error=<kind>`, `show warning failed error=<kind>`, `show warning_moved failed error=<kind>`, `show warning_gone failed error=<kind>`, `show failed what=<start, emergency_watch, watch> error=<kind>` | Android refused something. |

These lines come only once this shower is installed in the process (see
[How the other pieces call it](#how-the-other-pieces-call-it)). Before
that, the host's stand in logs the covers in its own words (host/README.md,
"What it logs").

## Pictures without a phone

`PicturesTest` draws every state on the PC with Robolectric 4.17 in its
native graphics mode (Android's own drawing code, on Windows too), with
the repository's fonts, the design's icons and the logo, at two pixels per
point, and saves each as `native-<frame>.png` with a text file of where
each text and box stands. They go to the folder `LIMO_PICTURES` names, or
`build/limo-pictures` in the module:

```
LIMO_PICTURES=<folder> npm run test:child
```

| Picture | The drawing's frame |
| --- | --- |
| `native-A`, `native-A-no-name`, `native-A2`, `native-A-small` | A, A without a name, A2, A on a small phone (360 by 640) |
| `native-B1` (the keyboard a grey band, as drawn), `native-B2`, `native-B3` | B1, B2, B3 |
| `native-B-wrong-code`, `native-B-wait`, `native-B-request` | the wrong code, the wait, a request that waits (the parts only) |
| `native-C`, `native-D`, `native-F` | bedtime, the block screen, the warning |
| `native-A2-text-1.3`, `-2`; `native-B3-text-*`; `native-D-text-*` | larger text |

Set beside the drawings' frames, each text stands within a point or two of
the drawing's. What still differs, and why:

- **Android's bars** are under the cover, so the drawing's status bar clock
  and gesture bar are not drawn here; the apps' icons are the drawing's
  letter tiles, since only a phone has the apps.
- **Borders.** Chrome draws the drawings' 1.5 borders as 1 (its computed
  style says `1px`); the phone draws 1.5, as the theme says and as the
  child's React Native screens draw them. A row is therefore 1 taller, and
  what comes after three rows stands up to 4 lower.
- **The stamp's box**: the drawing measures the tilted stamp's outline,
  the phone its own box; their centres are within a point.
- **The letters' smoothing and their exact widths** differ between Chrome
  and Android's text drawing; a caret is not drawn in a picture.

## Tests

`npm run test:child` (in the app) runs them with the module's other JVM
tests.

| Test | What |
| --- | --- |
| `CoverPlanTest` (12) | The lock's and the block screen's decisions: rows, pictures and words for the time used up and bedtime; the tiles (the dialler first, each once, only those Android can open, never LiMo); no way out and the engine's words; the wait greying the code and the PIN; a request and its stamp's colour; Back to the call pinned above Emergency call on both; the label with and without a name, nothing else changing; nothing ordered; a picture of the set for every reason; the drawings' words, the counter, the clock and the end of today |
| `ShowerCoreTest` (12) | The shower as a level: the same order every second drawing once; an order ended taking the cover down at once; a changed order changing the window; a window Android took away drawn again; stepping aside for the emergency dialler until the engine sees it, at most three seconds, an order of nothing followed and the timer then drawing nothing; LiMo's own screen never covered, even by an order of the lock, and the orders leading when it closes; a call ringing as it closes never covered; the warning's thirty seconds, OK, a new one, a cover taking it away, no service |
| `WaysOutModelTest` (10) | Each way out through right and wrong answers: a right code (bytes sent, digits wiped, the engine's words, closing), a wrong one, a malformed one, the wait, a request and the core's refusal, each PIN choice and Today, Back wiping, a failure; the screen rebuilt in the middle of a call getting the answer and never the digits |
| `WaysOutEngineTest` (3) | On the real engine through the host: the lock's plan, a wrong PIN and a right code in the engine's words, a request waiting on the lock with asking no longer offered, five wrong codes greying the rows, the right PIN made to wait; the shower following the real orders as a level, the dialler never covered |
| `LookTest` (5) | The table against the tokens and the drawings' style sheet: colours, shapes, the lock, the ways out, the warning, every kind of text, nothing under 12 |
| `PicturesTest` (10) | The pictures above, and on them: everything to tap at least 48 high, no text cut or running out, the pinned buttons in sight, the small phone scrolling, the reading order, the label without a name moving nothing, the PIN never on the screen, the taps reaching their actions, large text at 1.3 and 2 breaking nothing |
| `ShowHouseRulesTest` (4) | No log line with a name, a label, words, a reason, a code or a PIN; nothing reading another app's screen, the covers accessibility overlays; a typed code or PIN never a string, the ways out secure and never saving digits; no dash punctuation |

## What only a phone can prove

For the owner, on the Galaxy S20 Ultra (Android 13), with the debug build
paired by the development tool and `adb logcat -s LiMo LiMoWatch`:

1. **The block screen.** Open a blocked app: the block screen covers it
   (`show cover now block`, then `show cover drawn ms=`), the status and
   navigation bars covered, nothing of the app showing. Tap "Go to the home
   screen": home, and the cover gone (`show cover now nothing`), and not
   back over the home screen. Repeat 14 times, YouTube among the blocked
   apps (proof two's late event).
2. **The lock.** Use the day up in a game: the lock as drawn, the phone's
   own clock and date at the top, the child's name in the label, the
   allowed apps with their own icons, the dialler first. Tap a tile: the
   app opens and stays open. Go back to the game: locked again.
3. **Emergency call.** From the lock and the block screen: the emergency
   dialler opens at once and is never covered while it is up (`show
   emergency opened how=emergency_dialler` and the watch's `front learned
   emergency_dialler` or `front now emergency_dialler`). Which of the three
   ways opened it. Nothing dials.
4. **A call.** Ring the phone while locked: nothing covers the ringing
   call. Answer, go back to the game: the lock with "Back to the call" in
   teal; tap it: the call's screen (`how=in_call_screen` with the Phone
   permission, `how=dial` without). End the call: Emergency call alone.
5. **The ways out.** Each row opens its screen with the strip; the lock is
   never over it. B1: the keyboard comes up under the pinned "Send
   request", which sits on it. A wrong code: the engine's words in coral.
   Five wrong: back to the lock with the wait and the rows greyed. A right
   code: "15 minutes added. The phone opens now." and the phone open. Back:
   the lock again over the game; how long the game shows first (the watch
   confirming it in front) is worth a note. Turn the phone in the middle
   of a code: the cells empty, the choice kept. A call ringing while a way
   out is open: the call's screen, never the lock.
6. **Secrets.** A screenshot of B2 or B3 is refused, the recent apps list
   shows no picture of it, and nothing typed is in the log.
7. **The warning.** At 15, 5 and 1 minute in a game: the card at the top,
   gone after 30 seconds or at OK; the game under it still takes touches
   outside the card and keeps its keyboard (try a game's chat or a
   browser's search field with the keyboard up). In the shade, the warning
   notification without a heads up or a sound. Then switch accessibility
   off: the next warning is the heads up notification.
8. **Turns, folds and sizes.** Turn the phone on the lock: laid out again,
   the cut out kept clear. Font size and display size to the largest:
   nothing cut, Emergency call in sight. Gesture navigation and three
   buttons both.
9. **TalkBack.** On the lock: the title said when it comes up, then the
   order above; every button and tile named.
10. **The fonts and the logo.** Fraunces, Instrument Sans and Spline Sans
    Mono at their weights, and the logo beside "MADE BY". If the mark shows
    only words, the logo was not found by name (see Left open).

## Left open

- **A pairing by dev/ before the keeper has run.** `keep/Keeper.start`
  installs this shower before it starts the host, so a new process's first
  tick is drawn. But dev/'s receiver pairs first and only then calls
  `Keeper.start(app, "dev_pair")`, so in a process where the keeper has not
  run yet, the pairing's one tick goes to the host's stand in, which only
  logs. The shower, installed straight after, is ticked at once and draws
  the cover. A warning that falls due in that one tick is only logged: no
  card floats, and keep/'s notification of it stays quiet, since a card
  could float. Debug builds only.
- **The logo on a phone.** The module has no copy of the LucidLoom AI logo;
  it is looked up by name in the app: `limo_maker_logo`, then
  `src_generated_lucidloomlogo`, the name the screens' bundle gives
  `src/generated/lucidloom-logo.png` (the built child APK's resource table
  holds it, and the three fonts, by these names). That second name is the
  bundler's choice, so a copy in the module's generated resources (by
  `scripts/make-design.mjs`, beside the tokens and the icons) would make it
  certain. Without either the mark is said in words.
- **Split screen** with a blocked app beside the front, as the host's
  README says.
- **"Today" and "1 hour" on a clock set by hand.** The PIN's keep open
  moments are counted from the phone's wall clock, while the engine judges
  them by LiMo's own clock; when the two differ (a clock set by hand with
  automatic time off), "1 hour" is an hour by the wall clock.
- **A screen the watch does not know.** The cover stands aside for three
  seconds after Emergency call; an emergency dialler the watch could not
  name as one would be covered again after them. The watch's README says
  how it learns it.
- **Not drawn, decided here:** the lock's words when no way out is offered
  (as a dashed note, like a request); a right code's or PIN's answer in
  teal for a second and a half before the screen closes; the cover's
  width above 600; the default dialler as the first tile.

## Dependencies and licences

Nothing in the app's APK beyond Android's own classes. For the JVM tests
only, in the module's `build.gradle`: Robolectric 4.17 (MIT), which brings
Android's own framework for the PC (`android-all-instrumented`, Apache-2.0)
and its own helpers (MIT, Apache-2.0 or BSD); Gradle downloads them into
its cache the first time. `npm run inspect` shows that neither APK holds
any of it. The code here was written for LiMo; nothing was copied.

## Layout

```
show/
  README.md
  Look.kt             the table of the look: colours, sizes, kinds of text
  Words.kt            the drawings' own words, the typographic apostrophe, the clock's words
  CoverPlan.kt        the lock's and the block screen's decisions, from the engine's order (no Android)
  ShowerCore.kt       the shower as a level, stepping aside, the warning's time (no Android)
  WaysOutModel.kt     the ways out's state machine and the relay of answers (no Android)
  Paper.kt            the drawing's boxes, dots, dashes and ink strokes
  Type.kt             the fonts and the logo, the line heights, PaperText
  Parts.kt            the parts a screen is made of
  CoverPage.kt        the screen frame, the lock and the block screen
  WarningView.kt      the warning card
  WaysOutPage.kt      B1, B2 and B3
  CoverShower.kt      the host's Shower: the overlay windows, the taps, the apps, the emergency dialler
  WaysOutActivity.kt  LiMo's own screen of the ways out
src/test/java/ai/lucidloom/limo/child/show/
  ShowFixtures.kt     made up orders in the engine's words, and the fakes
  CoverPlanTest.kt, ShowerCoreTest.kt, WaysOutModelTest.kt, WaysOutEngineTest.kt,
  LookTest.kt, PicturesTest.kt, ShowHouseRulesTest.kt
```
