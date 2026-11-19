# limo-phone-app

LiMo for Android: one Expo app, built in two forms. The **child form** goes
on a child's phone and holds every Android component LiMo will use there:
an accessibility service, a foreground service, a device admin, receivers
and the activity for the ways out of a lock. The **guardian form** goes on a
parent's phone and holds none of them.

**What it is now.** The skeleton both forms are built on. Every component of
the child form is declared once, in one manifest, as a stub that does
nothing; later pieces fill the Kotlin folders and leave the manifest alone.
The theme comes from the design, the three fonts and the icon set are in,
and each form shows one plain start screen in Paper and Ink that says which
form it is and that setup is not built yet.

**What it is not.** It counts nothing, covers nothing and limits nothing.
The child form carries the Rust engine and the phone's side of the link
(see [The Rust engine](#the-rust-engine)), but nothing calls them yet; it
has no link to the master PC and no setup. It holds no permission to open a
network socket. It has been built and its files inspected on the
development PC only: nothing here has run on a phone or an emulator.

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
```

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
app/
  child/index.tsx         the child form's start screen
  guardian/index.tsx      the guardian form's start screen
src/
  form.ts                 which form this build is (from the app's embedded config)
  host.ts                 THE seam: host.request(name, args), see below
  theme/                  Paper and Ink for React Native
    index.ts              colours, radius, border, space, the hard shadow
    fonts.ts, fonts.json  the font families and Fraunces' optical sizes
  icons/Icon.tsx          draws a design icon with react-native-svg
  icons/types.ts
  ui/StartScreen.tsx      the plain start screen both forms share
  env.d.ts
  generated/              written by scripts/make-design.mjs, git ignored
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
`src/generated/`, `modules/limo-child/android/src/generated/`.

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
- **The maker's mark** ends both start screens: "MADE BY", the LucidLoom AI
  logo at `tokens.maker.logoSize`, the name. It is not a link.
- **Safe areas.** Android draws apps edge to edge, and proof three's page
  ran under the navigation bar. The start screen fills the display with
  paper and pads its content by the safe area insets
  (react-native-safe-area-context), so no word sits under the status bar,
  the navigation bar or a cut out. The status and navigation bar icons are
  dark, and the window behind the first frame is paper, never the dark theme.

## The host seam

`src/host.ts` is proof six's seam: screens call `host.request(name, args)`
and nothing else, and only `host.ts` knows who answers. Today:

| Place | When | Answers |
| --- | --- | --- |
| `child-module` | the child module is linked (the child form) | `LimoChildModule` in Kotlin |
| `none` | no native module (the guardian form, until its link exists) | what JavaScript knows by itself |
| `mock` | a development run with no native module | the same, never in a release build |

Requests are typed in one list (`Requests`), with what each is sent and
answers, so a screen cannot ask for one that does not exist. Errors are a
`HostError` with a `code`. The guardian's link on the phone and the console
in a Tauri window on the PC join here as further places, not in the
screens. JavaScript is never on the enforcement path: the Kotlin side calls
the Rust engine itself.

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
  `watch/` learns of a call without `READ_PHONE_STATE` (the audio mode, and
  the dialler in front, need no permission). Each adds a line to the
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
  (`Fraunces 28` and so on). If these screens are ever exported for the web,
  `src/theme/fonts.ts` needs the CSS family names there; the app does not
  build for the web now.
- **The launcher icon** is a stand in from the icon set until LiMo has its
  own mark.
- **The maker's logo** is drawn at `tokens.maker.logoSize` (26); the phone
  sheet the lead is drawing uses 22. The tokens win until they change.
