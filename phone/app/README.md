# limo-phone-app

LiMo for Android: one Expo app, built in two forms. The **child form** goes
on a child's phone and holds every Android component LiMo will use there:
an accessibility service, a foreground service, a device admin, receivers
and the activity for the ways out of a lock. The **guardian form** goes on a
parent's phone and holds none of them.

**What it is now.** The skeleton both forms are built on, and the child
form's own screens. Every component of the child form is declared once, in
one manifest, as a stub that does nothing; later pieces fill the Kotlin
folders and leave the manifest alone. The theme comes from the design, the
three fonts and the icon set are in. The child form has its screens, drawn
to the approved design: setup, the child's introduction, Today and How it
works (see [The child form's screens](#the-child-forms-screens)). They ask
for everything through one seam that Kotlin answers later; until it does,
the phone counts as not paired and the child form shows "This phone is not
set up yet." The guardian form still shows one plain start screen.

**What it is not.** It counts nothing, covers nothing and limits nothing.
The child form carries the Rust engine and the phone's side of the link
(see [The Rust engine](#the-rust-engine)), but nothing calls them yet; it
has no link to the master PC, and the child module does not answer the
screens yet. It holds no permission to open a network socket. It has been
built and its files inspected on the development PC only: nothing here has
run on a phone or an emulator.

## Contents

- [Building](#building)
- [The two forms](#the-two-forms)
- [Layout](#layout)
- [The child module](#the-child-module)
- [The Rust engine](#the-rust-engine)
- [Permissions](#permissions)
- [Nothing leaves the phone](#nothing-leaves-the-phone)
- [The design in the app](#the-design-in-the-app)
- [The host seam](#the-host-seam)
- [The child form's screens](#the-child-forms-screens)
- [The child's seam](#the-childs-seam)
- [The preview](#the-preview)
- [Tests](#tests)
- [Scripts](#scripts)
- [Checks](#checks)
- [Versions, and why](#versions-and-why)
- [Dependencies and licences](#dependencies-and-licences)
- [Left open](#left-open)

## Building

Needs, on Windows: Node 22, JDK 17, and the Android SDK with platform
`android-36`, build tools `36.0.0`, NDK `27.1.12297006` and CMake `3.22.1`,
plus a ninja 1.12 or later (Visual Studio 2022's "C++ CMake tools for
Windows" has one and is found by itself; or set `LIMO_NINJA`). The build
never installs SDK packages: a missing one fails the build with its name.
`ANDROID_HOME` must be set.

From this folder:

```
npm ci
npm run build:child        # out/limo-child-release.apk
npm run build:guardian     # out/limo-guardian-release.apk
npm run build:both         # both, one after the other
npm run inspect            # looks inside both and checks them
npm run typecheck
npm run test:child         # the child module's JVM tests: the Rust engine from Kotlin
npm test                   # the screens' tests (jest), against the mock host
npm run preview            # the screens in a browser: dist/preview/index.html
npm run check:release-js   # no mock and no preview in either form's JavaScript
```

The last three need nothing but Node.

The child form's build also needs Rust with the `aarch64-linux-android`
target: see [The Rust engine](#the-rust-engine).

`npm ci` also runs `scripts/make-design.mjs` (see [Scripts](#scripts)). A
build makes `android/` again from nothing for the form it builds, with
`expo prebuild`, so nothing of the other form is ever left in it, and then
runs `gradlew assembleRelease` with `LIMO_FORM` set. `android/` is never
edited by hand and is not committed: everything in it comes from
`app.config.ts` and the config plugins in `plugins/`.

The APKs are signed with the debug key `expo prebuild` writes, only so that
they install over USB. A release key is the owner's decision, later.

The first build on a PC downloads Gradle 9.3.1, the Android Gradle Plugin
and the React Native and Expo libraries into the user's Gradle cache; later
builds use the cache. Gradle runs without a daemon and compiles Kotlin in its
own process, so nothing keeps running after a build.

**Debug builds.** `gradlew assembleDebug` works, but a debug build has no
`INTERNET` either, so it cannot load JavaScript from a development server.
Release builds carry their JavaScript inside and need none.

## The two forms

`LIMO_FORM` picks the form when the app is built. There is no default: every
Expo command that reads `app.config.ts` fails without it, so a form is never
picked by accident.

| | Child form | Guardian form |
| --- | --- | --- |
| Package | `ai.lucidloom.limo.child` | `ai.lucidloom.limo` |
| Name on the phone | LiMo | LiMo Guardian |
| Launcher icon | the design's `hourglass` on paper | the design's `guardian` on paper |
| Child module (`modules/limo-child`) | linked | left out of autolinking |
| Components of its own | the seven stubs below | none |
| Oldest Android | 8.0 (API 26) | 8.0 (API 26) |

Both forms can be on one phone at once: their packages differ. The form is
also written into `android/gradle.properties` as `limo.form`, and
`scripts/form-check.gradle` stops any Gradle run whose `LIMO_FORM` differs.
That matters because Expo reads `app.config.ts` again while Gradle bundles
the JavaScript: `android/` made for one form and a bundle made for the other
must never meet.

## Layout

```
app.config.ts             the two forms: package, name, icon, permissions, plugins
index.ts, App.tsx         the entry point; App picks the form's start screen
App.web.tsx               in a browser only: the preview (never in a built app)
app/
  child/index.tsx         the child form: its screens (src/child)
  guardian/index.tsx      the guardian form's start screen
src/
  form.ts                 which form this build is (from the app's embedded config)
  host.ts                 THE seam: host.request(name, args) and childHost, see below
  childSeam.ts            the child's part of the seam, as plain data, and its readers
  child/                  the child form's screens
    ChildApp.tsx          which screen shows, and what each tap asks of the host
    SetupScreens.tsx      S0 to S8, and C1 to C3
    AppScreens.tsx        Today (E and T) and How it works (H)
    words.ts              the screens' own words
    HostContext.tsx       which ChildHost the screens ask
    useHostValue.ts       an answer from the host, asked again when it may change
  ui/paper/               the parts the phone drawings are made of
    Page.tsx              the page, the safe areas, a pinned button
    Tabs.tsx              the floating tab bar
    parts.tsx             card, button, label, stamp, tile, divider, top row, dots, maker
    text.tsx              the type styles, measured from the drawings
  mock/childMock.ts       a made up phone in every state: the preview and the tests only
  preview/                the preview's page (App.web.tsx): every screen in every state
  theme/                  Paper and Ink for React Native
    index.ts              colours, radius, border, space, the hard shadow
    fonts.ts, fonts.json  the font families and Fraunces' optical sizes
  icons/Icon.tsx          draws a design icon with react-native-svg
  icons/types.ts
  ui/StartScreen.tsx      the plain start screen of the guardian form
  env.d.ts
  generated/              written by scripts/make-design.mjs, git ignored
tests/                    the screens' tests (jest)
jest.config.js
modules/limo-child/       the child module (a local Expo module)
  expo-module.config.json, index.ts
  android/build.gradle
  android/src/main/AndroidManifest.xml      every component, once
  android/src/main/java/ai/lucidloom/limo/child/
    host/   watch/   show/   keep/   link/   strict/
  android/src/main/res/   the accessibility and admin settings, words, a theme
  android/src/debug/      the development receiver (folder dev/), debug builds only
  android/src/test/       the JVM tests of the Rust engine from Kotlin
  android/src/generated/  written by scripts/make-design.mjs and scripts/build-engine.mjs, git ignored
plugins/
  with-limo-android.js    Gradle settings, theme, backups, launcher icon, debug manifests
  with-limo-form.js       what differs between the forms
  with-limo-fonts.js      the three fonts and their licences
scripts/                  see Scripts
```

Ignored and made by the build: `node_modules/`, `android/`, `out/`,
`dist/` (the preview), `src/generated/`,
`modules/limo-child/android/src/generated/`.

The screens live in `app/<form>/` so that, if the app takes up Expo Router
later, they are already where its routes would be. It does not use Expo
Router now: see [Versions, and why](#versions-and-why).

## The child module

`modules/limo-child` is a local Expo module, linked in the child form only.
Its manifest declares every Android component the child form will need, each
a stub that does nothing yet. The Kotlin lives in
`ai.lucidloom.limo.child.<folder>`, one folder per later piece; the module's
own namespace (`ai.lucidloom.limo.child.module`) only names its R class,
which must not clash with the app's.

| Folder | Class | Manifest entry | Guard | What it will do |
| --- | --- | --- | --- | --- |
| `watch/` | `WatchService` | accessibility service, exported, `res/xml/limo_watch_service.xml` | `BIND_ACCESSIBILITY_SERVICE` | which app is in front, by package name only; its overlay windows carry the covers |
| `watch/` | `ClockReceiver` | receiver, not exported: `TIME_SET`, `TIMEZONE_CHANGED` | | clock changes, as facts for the engine |
| `show/` | `WaysOutActivity` | activity, not exported, own task, not in Recents | | ask with a typed reason, a code, the PIN |
| `keep/` | `KeepService` | foreground service, not exported, type `specialUse` | | hosts the engine, shows the time left notification |
| `keep/` | `RestartReceiver` | receiver, not exported: `BOOT_COMPLETED`, `MY_PACKAGE_REPLACED` | | starts LiMo again after a restart or an update |
| `keep/` | `AlarmReceiver` | receiver, not exported, no filter | | LiMo's own alarms |
| `keep/` | `AdminReceiver` | device admin receiver, exported, `res/xml/limo_admin.xml` (no policies) | `BIND_DEVICE_ADMIN` | the base protection; later the device owner of strict mode |
| `dev/` (debug only) | `DevReceiver` | receiver in `src/debug`, exported: `ai.lucidloom.limo.child.DEV` | `android.permission.DUMP` (only adb and the system hold it) | the development tool's commands over the cable |
| `host/` | `LimoChildModule` | none (an Expo module) | | the child's side of the host seam; later the engine host |
| `link/` | none | none | | the link to the master PC, run inside `KeepService` |
| `strict/` | none | none | | strict mode: the device owner is `keep/AdminReceiver` |

Each stub says in its own file what it is and what it will do. The class
names are now part of the app: Android remembers an accessibility service a
guardian switched on, a device admin and a device owner by class name, so
renaming one later switches it off on every phone.

The accessibility service's settings are proof two's: window state changes
only, no window content (`canRetrieveWindowContent="false"`), no gestures, no
key filtering, `isAccessibilityTool="false"`. The device admin asks for no
policy at all, as in proof five. The words Android shows for both in
Settings say what this build does: nothing yet.

`LimoChildModule` answers one request, `about`, so the start screen can show
that the module is linked; any other name is refused with the code
`UnknownRequest`.

## The Rust engine

The child form carries the phone's Rust: the engine and the phone's side of
the link, through the bridge crate [`../ffi`](../ffi/) (`limo-phone-ffi`)
and the Kotlin bindings UniFFI generates for it, which call it through JNA.
Its README says what crosses and in which shape. Nothing calls it yet: the
engine host (a later piece) will, from Kotlin, never from JavaScript.

**The build step.** The child module's Gradle task `buildRustEngine` runs
`scripts/build-engine.mjs android` before the module is compiled, so
`npm run build:child` gives an APK with the library inside. It builds
`liblimo_phone_ffi.so` for each ABI of `reactNativeArchitectures` (now
`arm64-v8a`) with the NDK's clang, refuses a library that holds the
testkit, SQLite or a path of the build PC, and generates the bindings from
it, into the module's git ignored `src/generated/` (`jniLibs/` and
`kotlin/`, package `ai.lucidloom.limo.child.ffi`). Gradle skips it when no
Rust source changed. `npm run build:engine` runs the same step by itself.
The guardian form leaves the module out of autolinking, so its build never
runs the step and its APK gains nothing: no library, no JNA, no bindings.

Needs, once per PC: Rust (1.91 or later, for UniFFI's generator) and
`rustup target add aarch64-linux-android`. The first build of UniFFI's
generator takes about two and a half minutes; after that a Rust change costs
seconds.

**The JVM tests.** `npm run test:child` makes `android/` for the child form
if it is not already, and runs `gradlew :limo-child:testReleaseUnitTest`.
Before the tests, the task `buildRustEngineForTests` runs
`scripts/build-engine.mjs host`: the same crate for this PC, with the
testkit (a family on the master PC, played over a temporary file), and a
second copy of the bindings for the testkit only, in `src/generated/`
(`test-native/` and `test-kotlin/`). The tests, in
`modules/limo-child/android/src/test/java/ai/lucidloom/limo/child/ffi/`,
drive the real engine and the real link from Kotlin through the bindings
that ship, with the PC library loaded by JNA: a short day from a made up
welcome to a right code and a reload, and a pairing and a session over the
link. Nothing in the APK depends on this step.

**What `npm run inspect` adds.** The child form must hold
`lib/<abi>/liblimo_phone_ffi.so` for every ABI it names, exporting the
engine, with `libjnidispatch.so` and the bindings in the dex; the guardian
form must hold none of the three; and neither may hold the testkit in any
native library or dex file. The same testkit check runs once on the PC
library of the JVM tests, which has the testkit, to show that it finds it.

## Permissions

What each built APK holds, as `aapt2` reads it (see [Checks](#checks)):

| Permission | Child form | Guardian form | Why |
| --- | --- | --- | --- |
| `FOREGROUND_SERVICE` | yes | no | `keep/KeepService` runs in the foreground (Android 9 and later) |
| `FOREGROUND_SERVICE_SPECIAL_USE` | yes | no | its type, `specialUse` (Android 14 and later) |
| `POST_NOTIFICATIONS` | yes | no | the time left notification (Android 13 and later asks the person) |
| `RECEIVE_BOOT_COMPLETED` | yes | no | `keep/RestartReceiver`, after a restart |
| `PACKAGE_USAGE_STATS` | yes | no | usage access for `watch/`; a person grants it in Settings |
| `<package>.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION` | yes | yes | AndroidX's own, signature level: lets the app's receivers registered in code stay private on Android 12 and older. No other app can hold it |

That is all: the guardian form holds nothing but AndroidX's private one.

Besides LiMo's own, the merged manifest of both forms has three components
from libraries: `androidx.startup.InitializationProvider` (not exported),
`androidx.profileinstaller.ProfileInstallReceiver` (guarded by `DUMP`, so
only adb and the system reach it) and expo-file-system's
`FileSystemFileProvider` (not exported), with its `<queries>` entry for
`OPEN_DOCUMENT_TREE`.

Each permission of the child form is in the module's manifest with a comment
saying why. Neither form asks for the camera, the microphone, location,
contacts, accounts, the phone's state, shared storage, or "draw over other
apps": the covers will be accessibility overlays, which need no permission.
`app.config.ts` lists everything no form may hold, and the build removes each
one even when React Native's template or a library asks for it
(`tools:node="remove"`). The guardian form also refuses the child form's
own permissions, so nothing of the child's can reach it by accident.
`plugins/with-limo-android.js` removes `SYSTEM_ALERT_WINDOW` from the debug
manifests too, where React Native's template adds it.

Not declared yet, for the pieces that need them to decide: `INTERNET` and
`ACCESS_NETWORK_STATE` (the link), exact alarms (`SCHEDULE_EXACT_ALARM` or
`USE_EXACT_ALARM`), and asking to be left out of battery optimisation.

## Nothing leaves the phone

LiMo sends nothing to anyone. What in this app could contact anything, and
what is done about it:

| What | Could | Done |
| --- | --- | --- |
| Any socket at all | React Native carries OkHttp for `fetch`, `XMLHttpRequest` and WebSocket; `expo` carries `expo/fetch`; `expo-file-system` can download; `expo-asset` and `expo-font` can load from a URL | Neither form holds `INTERNET`, so Android gives the app no network socket at all, whatever code asks. Nothing in the app calls any of them. |
| Expo's command line tool | sends usage data from the build PC by default | `scripts/build.mjs` sets `EXPO_NO_TELEMETRY=1` |
| Over the air updates (`expo-updates`) | checks a server for new JavaScript | not installed; `updates.enabled: false` and `checkAutomatically: NEVER` in case it ever were |
| Android's backup | copies the app's files to the Google account, or to a new phone in a transfer | `allowBackup="false"`, and data extraction rules that exclude every file from both |
| The development server link | debug builds of React Native fetch JavaScript from the PC | release builds carry the JavaScript inside; debug builds have no `INTERNET` either |
| `@expo/dom-webview` (a WebView, linked by `expo`) | a WebView can load pages | nothing renders one; with no `INTERNET` it could reach nothing |
| A link into the app | another app opening a screen inside LiMo | no `scheme`, no `VIEW` intent filter; the template's `<queries>` for web links is removed |
| Crash reporting, analytics, ads | | none is installed; `AD_ID` is refused |

`npm run inspect` checks the permissions and the intent filters in the
built files, and that the build PC's user name and home folder appear in
none of the APK's files.

## The design in the app

The design stays the one source: `scripts/make-design.mjs` reads it and
writes what the app uses into `src/generated/` and
`modules/limo-child/android/src/generated/`, both git ignored, on every
install, type check and build.

- **Tokens.** `design/tokens.json` becomes `src/generated/tokens.ts` (a typed
  constant; the CSS shadows become numbers) and
  `limo_tokens.xml` (Android colours and sizes for the Kotlin views, rgba as
  `#aarrggbb`). The script stops when a token the app needs is missing.
  `src/theme/` adds what React Native needs on top: `space()`, the font
  families, and the card's hard shadow, drawn as a second shape set down and
  to the right because React Native has no shadow without blur.
- **Fonts.** `plugins/with-limo-fonts.js` copies the three variable fonts
  from `../../screens/fonts`, unchanged, into `res/font`, each licence into
  `assets/licences/<font>/OFL.txt`, and registers the families with React
  Native. React Native cannot set a variable font's axes from a style, so
  each weight (400, 500, 600, 700) is an entry in an Android font family that
  points at the one file with its axes set. Fraunces is one family per
  optical size (18, 28, 44, 72); `display(size)` picks the one nearest the
  font size, as a browser sets `opsz` to the size. On Android 8.0 and 8.1
  React Native asks only for normal or bold.
- **Icons.** Every icon is painted by `design/icons/clay.js` from
  `design/icons/shapes.js`, so the app never paints one another way. The
  script checks that `design/icons/svg/` is up to date, then writes each
  icon twice: as a small tree that `src/icons/Icon.tsx` draws with
  react-native-svg, and as an Android vector drawable
  (`limo_icon_<name>.xml`, and `_ink` for the plain icons that change on an
  ink surface) for the child's Kotlin views. A vector drawable has no
  gradient in a shape's own box, so `scripts/vector-drawable.mjs` draws such
  a shape in a group that stretches a 1 by 1 square onto the box; the
  gradients come out exact. All 44 were compared with the design's SVG side
  by side, at 96 and at 24, and matched.
- **The launcher icon** is an adaptive icon: the design's icon (`hourglass`
  for the child, `guardian` for the guardian, set in `app.config.ts`) on
  paper, from the same converter. Expo's placeholder pictures are removed.
  It is a stand in until LiMo has its own mark.
- **The maker's mark**: "MADE BY", the LucidLoom AI logo, the name. It is
  not a link. The guardian's start screen draws the logo at
  `tokens.maker.logoSize` (26); the child form's screens at 22, as the
  design says for a phone.
- **Safe areas.** Android draws apps edge to edge, and proof three's page
  ran under the navigation bar. The start screen fills the display with
  paper and pads its content by the safe area insets
  (react-native-safe-area-context), so no word sits under the status bar,
  the navigation bar or a cut out. The status and navigation bar icons are
  dark, and the window behind the first frame is paper, never the dark theme.

## The host seam

`src/host.ts` is proof six's seam: screens call `host.request(name, args)`
(or `childHost`, below) and nothing else, and only `host.ts` knows who
answers. Today:

| Place | When | Answers |
| --- | --- | --- |
| `child-module` | the child module is linked (the child form) | `LimoChildModule` in Kotlin |
| `none` | no native module (the guardian form, until its link exists) | what JavaScript knows by itself |
| `mock` | a development run with no native module | the same, never in a release build; it holds no made up data |

Requests are typed in one list (`Requests`), with what each is sent and
answers, so a screen cannot ask for one that does not exist. Errors are a
`HostError` with a `code`. The guardian's link on the phone and the console
in a Tauri window on the PC join here as further places, not in the
screens. JavaScript is never on the enforcement path: the Kotlin side calls
the Rust engine itself.

## The child form's screens

The child form's screens are the approved drawings
(`../../design/screens/phone-setup.html`, and frame E of
`../../design/screens/phone.html`), built in React Native and measured
against them: every size, gap, border, shadow, stamp and dot is the
drawings' own, and the words are word for word. In a browser at two pixels
per point they match the drawings to within the smoothing of the letters
(see [The preview](#the-preview)).

| Screen | What | States |
| --- | --- | --- |
| S0 | This phone is not set up yet: connect it to the family PC | not paired; nothing to tap |
| S1 | Welcome, for a guardian, naming the child: six short steps | Start pinned |
| S2, S2b | What LiMo does here, and the guardian agrees | Continue switched off until the box is ticked |
| S3 to S8 | One step each: notifications, phone, usage access, accessibility, battery, protection ("Step 2 of 6" and six dots) | NOT ON YET (coral) with the step's own button, or ON (teal), "Android says it is on." and Next; still off after two tries: a line and "Go on without it" |
| S9 | This phone is set up | six rows, a teal tick for each step Android says is on |
| C1 to C3 | For the child: what LiMo counts, what it never looks at, how to ask for more time | Next, Next, Got it |
| E | Today, under the floating tab bar | the time left, the bar, where the time went, the hours off, Ask for more time, the apps |
| T | Today with the banner | steps switched off: Needs a guardian, the engine's sentence for the first, "And 2 more" when there are more, and a button that shows a guardian each step |
| H | How it works, the second tab | the child's rules, what LiMo does, the version |

Where to start is worked out when the app opens, from the host: not paired
is S0 and nothing else; paired with no guardian agreed is S1; agreed with
the introduction not seen is the first step not on, or S9; after that,
Today. The pairing is watched: S0 moves on by itself when the PC pairs the
phone. Android's back button goes back a screen.

**The Phone step** asks for Android's Phone permission, only so that LiMo
knows a call is ringing or on and a lock never hides it. Its icon is the
handset (`call`), so it differs from S1's picture of the phone. Its note,
drawn like the Samsung note, quotes Android's own words for the permission
and says that LiMo never makes a call. The Samsung note on the Battery step shows only when
the host says the phone is a Samsung (`maker()`, below); the Battery step
waits for that answer before it shows, so the note never comes in late and
moves the stamp.

**A step is never taken on trust.** Its screen shows ON only when the host
says Android says so, asks again whenever the app comes back to the front
(as it does after Android's own settings) and after Android's own screen
was opened. Each step's button opens Android's own screen through the host
and does nothing else. The banner's button walks a guardian through the
steps that are off, in setup order, then back to Today.

**A step never traps a guardian.** When Android's screen for a step has
been opened twice on one visit (or could not be opened) and the step is
still off once Android has been asked again, the line under the stamp says
"Android has not turned this on. You can go on and come back to it from
Today." and an outlined "Go on without it" comes under it, inside the card;
the pinned button stays, to try again. The stamp and everything above it
stay where they were. A try counts only once Android's answer after it is
in, so the line never shows for a moment before a step turns on. Going on
goes to the next step, or to done; from the banner, to the next step after
it that is still off, or back to Today. The tries are kept with the step's
place in the screens, so going back to it finds the way on still there.
Every step works so, protection and accessibility too. Today's banner then
names the step, because the engine says what is off.

**Words.** Setup's words and the labels are the screens'
(`src/child/words.ts`). Every word about time or rules, and the sentence
the banner says about a step that is off, comes from the engine through
the host, and the screens make no sentence about any of them. With more
than one step off, the banner shows the first step's sentence and, under
it, "And 2 more" as a small mono label, like the line under Today's bar.
A time keeps its am or pm, and a span its unit, on one line. The words name
the child and never say "her" or "him".

**The theme's rules that are easy to break in React Native.** The hard
shadow is a second shape behind a card, which takes no room. A dashed
divider is drawn, because Android's own dashed border on one side is not
reliable. Nothing changes size when it is picked, pressed, ticked or
switched off, and the line under a step's stamp keeps its room when empty.
The tab bar floats, set in by 14; Android cannot blur what is behind a
view, so it takes the design's own answer for that, the card colour at
nearly full strength, with the hairline drawn as a ring so it shows on
every Android and the soft shadow where Android draws one (9.0 and later).
The page keeps room at its end as tall as the bar, measured. A pinned
button's strip grows with its button, and the page scrolls above it.

**Long names and large text.** The child's name at the top stays on one
line and ends in an ellipsis; everywhere else names wrap. With very large
text, a tab's label wraps to a second line (both tabs grow together), a
rule's label takes at most half its row, and the banner's stamp goes under
its tile. Everything was tried with a 43 character name, long app names,
and text at 1.3 and 2 times its size.

**For a screen reader.** Everything to tap is at least 48 high and has a
role (button, checkbox, tab) and a label. Headings, the step's state and
the section labels are read as such; icons and dots are skipped, except
the introduction's "1 of 3". Each screen reads in the order it is drawn,
with the pinned button and then the tab bar last.

## The child's seam

The screens ask for everything through one interface, `ChildHost` in
`src/childSeam.ts`. Every call is asynchronous and every answer plain data,
so the Kotlin side can be built against it. In the app it is `childHost` in
`src/host.ts`, which sends each call to the child module as a request (the
names below) and checks the answer with the reader beside it. The screens
get it through `src/child/HostContext.tsx`; only the preview and the tests
pass another.

| Call | Request to the child module | Answer |
| --- | --- | --- |
| `pairing()` | `child.pairing` | `{ paired: false }`, or `{ paired: true, childName }` |
| `agreed()` | `child.agreed` | whether a guardian has agreed to S2 |
| `agree()` | `child.agree` | nothing: keep that a guardian agreed |
| `steps()` | `child.steps` | `{ notifications, phone, usage, accessibility, battery, protection }`, each `"on"` or `"off"`, as Android says now, never remembered; a missing one is off |
| `openStep(step)` | `child.openStep` with `{ step }` | nothing, once Android's screen is open or its question answered; for `phone`, Android's own question for the Phone permission |
| `introSeen()` | `child.introSeen` | whether the child has seen C1 to C3 |
| `markIntroSeen()` | `child.markIntroSeen` | nothing: keep it |
| `today()` | `child.today` | `TodayView`, below |
| `rules()` | `child.rules` | `{ timeADay, screensOff, alwaysOpen, notAllowed }`: words, and app names |
| `version()` | none | the version the app was built as, from its own config |
| `maker()` | `child.maker` | who made the phone, as Android says it (`Build.MANUFACTURER`), passed through as it is, such as `"samsung"`; `isSamsung()` reads it, whatever its case. No answer means not a Samsung |
| `askForMoreTime()` | `child.askForMoreTime` | nothing: opens LiMo's ways out activity at asking, and does nothing else |
| `subscribe(listener)` | the module's `change` event, `{ what }` | `what` is `pairing`, `agreement`, `steps`, `intro`, `today` or `rules`; the screens ask again |

`TodayView`: `left` (`{ kind: "left" | "no-limit" | "open", words }`, such
as "1 h 12 min", "No limit today", "Open until 9:30 pm"), `used` ("48 min
used of 2 h"), `usedPart` (0 to 1 for the bar, or null with no limit),
`where` ("On this phone 31 min · on the PC 17 min"), `bedtime` ("Screens
are off from 9:00 pm to 7:00 am.", or null), `apps` (`{ name, minutes }`,
names and minutes only) and `guardianNeeded`, null while every step is on.

`guardianNeeded` is `{ off, words }`: `off` the steps that are off, in
setup order, and `words` beside it the engine's sentence for each, as the
host gives it, so `words[i]` is said of `off[i]`:

```
{ "off":   ["phone", "battery"],
  "words": ["Phone access is switched off, so a lock could hide a call.",
            "Android may put LiMo to sleep to save battery."] }
```

The reader refuses an answer where the two lists do not line up (another
length, a sentence that is not a string or is empty, `off` not a list). A
step this app does not know is left out with its sentence, the rest are
put in setup order, each once, and nothing left is null.

**What the engine has and what it needs.** "1 h 12 min" is the engine's
`time_left` span, "Open until 9:30 pm" and "No limit today" its time left
words. The lines "48 min used of 2 h", "On this phone 31 min · on the PC
17 min" and "Screens are off from 9:00 pm to 7:00 am.", a day's minutes per
app, the rules as words, and the sentence for each step that is off, are
not all in `phone/engine` yet: they belong in its `words.rs` and its view,
so that Kotlin passes words through and never makes them.

**Until Kotlin answers.** The child module answers only `about` today. A
request it does not know is refused, and `childHost.pairing()` takes that as
not paired, so the child form shows S0 and asks nothing else. A malformed
answer fails with a message naming the field; a step reads as on only for
the exact word `"on"`.

## The preview

`npm run preview` writes a static page of every screen in every state,
from the app's own code, through react-native-web, driven by the mock host
(`src/mock/childMock.ts`): the drawings' screens at 390 by 844 and at 360
by 640, the states the drawings do not show (each step on, the Battery
step on a phone that is not a Samsung, three steps off on Today, other
times left, no limit, open until), and a long name with long app names at
1.3 and 2 times the text size. Taps work: opening a step's Android screen
switches the step on after a moment, as a guardian would.

Open `dist/preview/index.html` from disk, in any browser; there is no
server. Expo's web export names its script and pictures from the root of a
web server, which from disk would be the root of the drive, so
`scripts/preview.mjs` makes those names relative and stops if any is left;
it copies the three fonts and their licences beside the page. Add
`#one=S5b&size=390` (or `360`, and `&scale=1.3` for larger text) to the
address for one phone alone, for a picture.

It is a development tool. `App.web.tsx` is its page: Metro takes it only
for the web, and Android's bundle takes `App.tsx`. The preview is never in
either built app, and neither is the mock: see [Tests](#tests).

What it cannot show: Android's own text layout and line breaking (the
browser's stands in), Fraunces at the phone's fixed optical sizes (a
browser sets it to the font size, as the drawings do), Android's font size
setting (the preview multiplies the text instead), and anything of Android
itself.

## Tests

`npm test` runs 102 tests with jest (Expo's preset for Android), against the
mock host. Nothing in them runs on a phone.

| File | What |
| --- | --- |
| `tests/setup.test.tsx` | S0, also as the app shows it with no native side; S0 moving on when the PC pairs; six steps in their order; Continue switched off until the box is ticked; each step not on yet and then on; the Phone step's words and note; a step shown on only when the host says so, and asked again when the app is back in front; Android's screen not opening; the Samsung note on a Samsung only, and not when the maker is unknown; each of the six steps offering to go on after two tries still off, and setup going on; a try counted only when it comes back off; the way on turning into Next; tries kept when going back; done; the introduction to Today, whose banner names the step gone past |
| `tests/app.test.tsx` | Today in the engine's words, never "counting"; times kept whole; other times left; asking for more time; the banner in the host's words only, with one, two and three steps off, and putting each right; going on past a step from the banner; How it works; every tab keeping its size; long names |
| `tests/seam.test.ts` | the readers of what Kotlin sends, the steps a guardian is needed for and their sentences lining up among them, and who made the phone; the app's host with no child module: not paired, nothing else answered |
| `tests/access.test.tsx` | a role, a label and 48 of height for everything to tap, the way on and the banner with three off among them; headings; the reading order |
| `tests/release.test.ts` | no screen names a native module, the network or a link out, or holds the engine's sentences for a step that is off; only `src/host.ts` reaches the child module; the app's imports, as Android resolves them from `index.ts`, never reach the mock, the preview or the tests |

`npm run check:release-js` shows the last on the bundles themselves: it
makes each form's JavaScript as Gradle's release build does (`expo
export:embed --platform android --dev false`), checks that it is the app's,
and that it holds neither the mock's mark, nor the made up names, nor the
preview's words; then, as a control, finds all of them in the preview's
own bundle.

## Scripts

`scripts/` is the entry point for later pieces' scripts: add
`scripts/<name>.mjs` and an npm script for it in `package.json`. Rust built
as part of the Android build belongs in the module's `android/build.gradle`
as a Gradle task instead, as proof three did it.

| Script | npm | What |
| --- | --- | --- |
| `make-design.mjs` | `make:design`, `postinstall` | the design into `src/generated` and the module's `src/generated` |
| `icons.mjs` | | paints the icons with `clay.js`; used by `make-design` and the launcher icon |
| `vector-drawable.mjs` | | one painted icon as an Android vector drawable |
| `build.mjs` | `build:child`, `build:guardian`, `build:both` | prebuild and a release build of a form, into `out/` |
| `build-engine.mjs` | `build:engine` | the Rust bridge and its bindings, for the phone (run by the child module's Gradle build) or for the JVM tests on this PC |
| `test-child.mjs` | `test:child` | the child module's JVM tests |
| `inspect.mjs` | `inspect` | looks inside `out/*.apk` and checks them |
| `preview.mjs` | `preview` | the screens' preview, into `dist/preview`, made to open from disk |
| `check-release-js.mjs` | `check:release-js` | each form's release JavaScript holds no mock and no preview |
| | `test` | the screens' tests (jest, `jest.config.js`) |
| `newer-ninja.gradle` | | from proof three: a ninja that copes with long Windows paths, now reaching the app's own C++ build too |
| `no-build-paths.gradle` | | keeps the build PC's folders out of the C and C++ libraries |
| `form-check.gradle` | | stops a Gradle run whose `LIMO_FORM` differs from `android/`'s form |

## Checks

All on the development PC, Windows 11, Node 22.13.1, JDK 17.0.15, Android
build tools 36.0.0, 19 Nov 2026:

- `npm run typecheck`: no errors.
- `npm run build:both`: both forms build from a deleted `android/`, with
  Gradle caches warm: Gradle took 156 s for the child form and 140 s for
  the guardian form; `expo prebuild` about 2 s each.
- `npm run inspect`: every check passes for both. Among them: the child form
  has each of its seven stubs with its guard and none of the development
  receiver; the guardian form has no accessibility service, no device admin
  receiver, no usage access, no foreground service and no class of the child
  module in its dex files; neither holds `INTERNET` or anything on the never
  list; neither can be opened from a link; backups are off; neither is
  debuggable; native libraries are 16 KB aligned; signed with the debug key.
- The child form's merged **debug** manifest, made once with
  `gradlew :app:processDebugMainManifest`: the development receiver is there,
  guarded by `DUMP`; `SYSTEM_ALERT_WINDOW` and `INTERNET` are not.
- The 44 icons as vector drawables, turned back into SVG and laid beside the
  design's own SVG at 96 and 24: the same.

With the Rust engine in the child form (the same day, later):
`npm run build:both` took Gradle 193 s for the child form (the Rust step 17
s of it, after a Rust change) and 134 s for the guardian form;
`npm run test:child` ran its 7 JVM tests, all passing; `npm run inspect`
passed every check, the new ones among them; `npm run typecheck` had no
errors.

| | Child form | Guardian form |
| --- | --- | --- |
| APK | 30,203,195 bytes (28.8 MiB) | 28,213,203 bytes (26.9 MiB) |
| Native libraries (React Native, Hermes, Expo, the codegen of svg and safe area) | 18,457,552 | 16,926,528 |
| of which the Rust bridge (`liblimo_phone_ffi.so`) and JNA (`libjnidispatch.so`) | 1,365,032 and 165,992 | none |
| Dex files | 7,601,426 | 7,422,229 |
| Fonts (three files, compressed) | 426,456 | 426,456 |
| Other resources (the icon drawables, React Native's own, `resources.arsc`) | 1,801,916 | 1,596,133 |
| JavaScript bundle (Hermes bytecode) | 1,400,700 | 1,400,700 |

The child module itself costs about 230 KB (its classes and the icon
drawables), and the Rust engine about 1.7 MB more (the library, JNA, and
JNA's classes with the bindings in the dex). Nearly all of the rest is
React Native and Expo, as in proof three.

**What the inspection found on the way.** The first build's codegen
libraries for react-native-svg and react-native-safe-area-context held the
path of a React Native header in the Gradle cache, under the user's home
folder: a `__FILE__` in that header. `scripts/no-build-paths.gradle` now
compiles every C and C++ file with `-ffile-prefix-map` for the repository,
Gradle's folder and the home folder, and the check passes. The Rust library
does the same with `--remap-path-prefix` (`scripts/build-engine.mjs`), as
proof six found for its exe, and the check passes for it too.

Not checked, because it needs a phone: that the start screens look right
and clear the system bars, that Android picks each font weight and optical
size from the variable fonts as expected, the launcher icon on a real
launcher, and the first start time.

With the child form's screens (the same day, later), all on the PC:
`npm run typecheck` had no errors; `npm test` ran its 65 tests, all
passing; `npm run check:release-js` passed for both forms, with its control;
`npm run make:design` was up to date. Every drawn screen (S0 to S8, S4b, C1
to C3, E, T, H) was pictured from the preview and from the drawings at 390
by 844 and at 360 by 640, two pixels per point, and compared pixel by
pixel: under half a percent of the pixels of any screen differ by more than
an eighth of the way from paper to ink, and those are the smoothing of
letters and the phase of the dashes. The differences that remain are
chosen: the tab bar is solid where the drawing is glass (Android cannot
blur), and "17 MIN" stays on one line at 360 where the drawing leaves "MIN"
alone. Android itself was not run: no Gradle build was made for this.

With six steps, the engine's sentences in the banner, the phone's maker and
the way on past a step (20 Nov 2026), all on the PC: `npm run typecheck`
had no errors; `npm test` ran its 102 tests, all passing; `npm run
check:release-js` passed for both forms, with its control. S1, the six
steps (S3 to S8, S5b done, S6c still off), S9 and T were pictured from the
preview and from the redrawn sheet at 390 by 844, two pixels per point:
under a tenth of a percent of the pixels differ on each, the smoothing of
letters, and a quarter of a percent on S9, the phase of its five dashed
dividers. No Gradle build was made.

## Versions, and why

- **Expo SDK 57**: expo 57.0.33, React Native 0.86.3, React 19.2.3, Hermes,
  the New Architecture. It is the newest SDK that proof three's build recipe
  supports on this PC without installing SDK packages: React Native 0.86
  asks for exactly what proof three used (compile SDK 36, build tools
  36.0.0, NDK 27.1.12297006, Android Gradle Plugin 8.12.0), so only Gradle
  9.3.1 is new. SDK 58 (React Native 0.88.1) is newer but asks for compile
  SDK 37, build tools 37.0.0 and the Android Gradle Plugin 9.2.1, which are
  not installed, and the build may not install them. SDK 55, proof three's,
  would also build, but is two SDKs older for no gain.
- **No Expo Router yet.** Proof six's console used it, and the plan names
  it. In SDK 57, its Stack imports `@expo/ui` on Android, which brings
  Jetpack Compose and Material 3 (an alpha); and with npm's default peer
  handling it also installs and links Reanimated, Worklets and Gesture
  Handler, three more native libraries with C++ builds. With the router,
  autolinking found 12 Expo modules and 8 React Native libraries; without
  it, 9 and 3. For two plain screens that is a lot of native code on a
  child's phone. The screens sit in `app/<form>/` so that taking up the
  router later is a move of a few lines in `App.tsx`; whether to, and when,
  is the lead's call when the real screens come.
- **`.npmrc`: `legacy-peer-deps=true`.** npm installs no peer dependency by
  itself, so every native library in the app is one that `package.json`
  names or that a named package needs.
- **react-native-svg 15.15.4** draws the icons; **react-native-safe-area-context
  5.7.0** gives the insets; **expo-constants** reads the form from the app's
  embedded config. These are the versions SDK 57 lists for itself.
- **TypeScript 5.9.3**, strict, with unused names as errors.
- **arm64-v8a only**, as in proof three. See [Left open](#left-open).

## Dependencies and licences

What ships in the APK:

| What | Version | Licence | Role |
| --- | --- | --- | --- |
| React Native (with Hermes, Yoga, fbjni, Fresco, OkHttp) | 0.86.3 | MIT (its parts: MIT or Apache-2.0) | the app |
| React | 19.2.3 | MIT | the screens |
| expo, expo-modules-core | 57.0.33, 57.0.26 | MIT | the app shell, the Modules API |
| expo-asset, expo-constants, expo-file-system, expo-font, expo-keep-awake, @expo/dom-webview, @expo/log-box | 57.0.x | MIT | linked by `expo` itself; only expo-constants is used |
| react-native-svg | 15.15.4 | MIT | the icons |
| react-native-safe-area-context | 5.7.0 | MIT | the insets |
| LiMo's Rust bridge (`../ffi`), with the engine, the core and the link | 0.1.0 | Apache-2.0 (its crates are listed in its README) | child form only |
| JNA | 5.19.1 (aar) | Apache-2.0 or LGPL-2.1-or-later, used under Apache-2.0 | child form only: Kotlin to Rust |
| AndroidX, Kotlin standard library | as React Native pins them | Apache-2.0 | Android support |
| Fraunces, Instrument Sans, Spline Sans Mono | the variable fonts in `../../screens/fonts` | SIL Open Font License 1.1, each licence in the APK | the type |
| The LucidLoom AI logo | `../../docs/images/lucidloom-logo.png` | LucidLoom AI's own | the maker's mark |
| The NDK's `libc++_shared.so` | NDK 27.1.12297006 | Apache-2.0 with LLVM exception | C++ runtime |

Development only, not in the APK (each is named in `devDependencies`, and
none of them holds native code, so autolinking finds the same 10 Expo
modules and 3 React Native libraries as before they were added):

| What | Version | Licence | Role |
| --- | --- | --- | --- |
| jest | 29.7 | MIT | runs the screens' tests |
| jest-expo | 57.0.x | MIT | Expo's jest preset for this SDK |
| @react-native/jest-preset | 0.86.3 | MIT | React Native's jest preset, which jest-expo builds on (named because `.npmrc` installs no peer by itself) |
| @testing-library/react-native | 13.3.3 | MIT | finds what a screen shows by its role and label; it uses the react-test-renderer jest-expo brings |
| @types/jest | 29.5.x | MIT | types for the tests |
| react-dom, react-native-web | 19.2.3, 0.21.x | MIT | the preview in a browser |

None of them sends anything anywhere; the preview loads no font or script
from the network.

Build only, not in the APK: Expo's command line tool, Metro and Babel
(MIT); TypeScript (Apache-2.0); Gradle, the Android Gradle Plugin and Kotlin
(Apache-2.0); lightningcss (MPL-2.0, a CSS tool of Expo's Metro setup);
node-forge (BSD-3-Clause or GPL-2.0, used by Expo's tool under BSD-3-Clause);
and the other packages in `package-lock.json` (500 in all: 413 MIT, the rest
ISC, Apache-2.0, BSD, BlueOak-1.0.0 and similar). No code was copied from
GPL or AGPL projects. `newer-ninja.gradle` is proof three's; the start files
follow Expo's blank template (0BSD).

## Left open

- **32 bit phones.** Only `arm64-v8a` is built, as in proof three. Phones
  with Android 8 that a child is handed down are sometimes 32 bit, and a 32
  bit phone refuses the APK. Adding `armeabi-v7a` is one value in
  `plugins/with-limo-android.js` and one Rust target, and makes the native
  part about half as big again.
- **The release key.** Both forms are signed with the debug key. A private
  key, needed before strict mode, is the owner's decision.
- **Expo Router.** See [Versions, and why](#versions-and-why).
- **For the pieces that fill the folders:** the foreground service type
  (`specialUse` now; Android also has `systemExempted` for device admin and
  device owner apps), exact alarms, leaving battery optimisation, and how
  `watch/` learns of a call (the owner decided on 20 Nov 2026 that setup
  asks for Android's Phone permission, for this only). Each adds a line to the
  module's manifest when it is decided. And an accessibility overlay draws
  above every activity, so the cover has to step aside while
  `WaysOutActivity` is open.
- **Libraries linked but unused.** `expo` links expo-file-system, expo-asset,
  expo-font, expo-keep-awake and `@expo/dom-webview` by itself. The JavaScript
  bundle names none of the last three, so they could be left out of
  autolinking; not done, because it cannot be tried on a phone here. Fresco's
  GIF and animated WebP support (`expo.gif.enabled`, about 320 KB) is unused
  too.
- **The console on the PC.** The theme's font families are Android ones
  (`Fraunces 28` and so on). In a browser `display()` quotes the name, as
  CSS needs, and the preview declares the families with `@font-face`; a
  console in a Tauri window would do the same. `App.web.tsx` is the
  preview's page now: a console for the web would take it over and move
  the preview to its own entry.
- **The launcher icon** is a stand in from the icon set until LiMo has its
  own mark.
- **The maker's logo** is 22 on the child form's screens, as the phone
  sheet says, and `tokens.maker.logoSize` (26) on the guardian's start
  screen.
- **For the child module (Kotlin):** answer the `child.*` requests and send
  the `change` event of [The child's seam](#the-childs-seam). Until then the
  child form shows S0 on every phone.
- **For the engine:** the words Today and How it works need that it does
  not have yet (see [The child's seam](#the-childs-seam)), and a day's
  minutes per app, which the core keeps only until they are reported.
- **Not drawn, decided here for the lead to look at:** the line "Android
  did not open it. Try again." under a step's stamp (S6c draws the line
  for a step still off after two tries); the banner with more
  than one step off ("And 2 more" under the first sentence); Today with no
  limit or open until (the engine's words take the time's place, without
  "Left today"); How it works leaving out a rule with nothing in it; the
  list of apps left out with no app used today.
- **For the child module (Kotlin), from the six steps:** answer `child.maker`
  with `Build.MANUFACTURER`; `child.steps` with `phone`; `child.openStep`
  for `phone` with Android's own question for the Phone permission, which
  needs `READ_PHONE_STATE` in the module's manifest (not declared yet); and
  `guardianNeeded` with the engine's sentences beside the steps.
- **On a small phone** the way on past a step sits under the stamp, inside
  the card, so on the longer steps at 360 by 640 the guardian scrolls to
  it, as to the stamp itself. The screen does not scroll by itself.
- **App tiles** on Today show the app's first letter, as the drawing does;
  the seam carries names only, not Android's icons.
- **Both forms carry the child form's JavaScript**, because `App.tsx`
  imports both forms' screens and picks one when the app starts. The
  guardian's has no child module to drive them; a later piece may split
  the bundle by form.
