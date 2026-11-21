# limo-phone-app

LiMo for Android: one Expo app, built in two forms. The **child form** goes
on a child's phone and holds every Android component LiMo will use there:
an accessibility service, a foreground service, a device admin, receivers
and the activity for the ways out of a lock. The **guardian form** goes on a
parent's phone and holds none of them.

**What it is now.** The skeleton both forms are built on, the child form's
own screens, and the Kotlin under them. Every component of the child form
is declared once, in one manifest; the pieces that fill the Kotlin folders
change it only for what their folder needs. The theme comes from the
design, the three fonts and the icon set are in. The child form has its
screens, drawn to the approved design: setup, the child's introduction,
Today and How it works (see [The child form's screens](#the-child-forms-screens)).
They ask for everything through one seam, which the child module's Kotlin
answers (`seam/`, see [The child's seam](#the-childs-seam)): the phone
counts as paired once a pairing has kept a state on it, and until then the
child form shows "This phone is not set up yet." The guardian form still
shows one plain start screen.

In the child module, the watch (`watch/`) reads the facts and the app in
front; the engine's host (`host/`) calls the Rust engine with them about
once a second, keeps its state, and hands out its orders; the covers
(`show/`) draw the block screen, the lock and the warning in the
accessibility service's overlay windows, and the ways out of a lock in
LiMo's own activity; the keeper (`keep/`) keeps LiMo running with a
foreground service that shows the time left, starts it again after a
restart, wakes it with an alarm, and installs the covers when it starts;
the seam (`seam/`) answers the screens' requests and opens Android's own
screens for setup; and a debug build can be paired with a test family over
the USB cable (`dev/`). Each folder has a README. `link/` and `strict/`
hold nothing yet.

**What it is not.** It has no link to the master PC yet, so a phone runs on
a test family put on it over the cable (`../devtool`). It holds no
permission to open a network socket. It has been built and its files
inspected on the development PC only: nothing here has run on a phone or an
emulator.

## Contents

- [Building](#building)
- [Signing](#signing)
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
npm run build:child:debug  # out/limo-child-debug.apk, for the development tool only
npm run inspect            # looks inside both release files and checks them, their signing too
npm run test:signing       # the release signing, with throwaway keys (some minutes; --quick: seconds)
npm run typecheck
npm run test:child         # the child module's JVM tests: the Rust engine from Kotlin, and each folder's
npm test                   # the screens' tests (jest), against the mock host
npm run preview            # the screens in a browser: dist/preview/index.html
npm run check:release-js   # no mock and no preview in either form's JavaScript
npm run build:console      # the guardian console's page: dist/console, for ../../console
```

The last four need nothing but Node.

The child form's build also needs Rust with the `aarch64-linux-android`
target: see [The Rust engine](#the-rust-engine).

`npm ci` also runs `scripts/make-design.mjs` (see [Scripts](#scripts)). A
build makes `android/` again from nothing for the form it builds, with
`expo prebuild`, so nothing of the other form is ever left in it, and then
runs `gradlew assembleRelease` with `LIMO_FORM` set. `android/` is never
edited by hand and is not committed: everything in it comes from
`app.config.ts` and the config plugins in `plugins/`.

With `LIMO_RELEASE_KEY_FILE` set, the release APKs are signed with the
owner's release key; without it, with Android's debug key, for trying on a
phone over a cable only, and the build says so at its end where it cannot be
missed. See [Signing](#signing).

The first build on a PC downloads Gradle 9.3.1, the Android Gradle Plugin
and the React Native and Expo libraries into the user's Gradle cache; later
builds use the cache. Gradle runs without a daemon and compiles Kotlin in its
own process, so nothing keeps running after a build.

**Debug builds.** `npm run build:child:debug` makes the child form as a
debug build, `out/limo-child-debug.apk`, beside the release files. It is
for the development tool on the PC only: it is debuggable, so the tool can
put a test family's pairing into the app's files with `run-as` (which works
on nothing else), and it holds the development receiver (`dev/`). A debug
build has no `INTERNET` either, so it could not load JavaScript from a
development server; `scripts/debug-bundle.gradle` makes it carry its
JavaScript inside, as a release build does. Same package name as the
release build, but always signed with Android's debug key, so it replaces a
release build only when that one carries the debug key too (see
[Debug and release builds on one phone](#debug-and-release-builds-on-one-phone)).
Never give a debug build to a family.

## Signing

Android installs an update of an app only when it is signed with the same
key as the app already on the phone. LiMo's builds sign so:

| Build | `LIMO_RELEASE_KEY_FILE` set | Not set |
| --- | --- | --- |
| `build:child`, `build:guardian`, `build:both` | the release key it names | Android's debug key, and the build ends with a warning |
| `build:child:debug` | Android's debug key: the variable is not even passed to Gradle | Android's debug key |

Android's debug key is the one `expo prebuild` writes into
`android/app/debug.keystore`. It is the same in every React Native project
and published with it, so anyone can sign an APK with it that Android would
take as an update of LiMo. An APK signed with it is for trying on a phone
over a cable, never for a family. Without the variable, a release build ends
so:

```
###################################################################
#  SIGNED WITH ANDROID'S DEBUG KEY: NOT FOR A FAMILY              #
#                                                                 #
#  LIMO_RELEASE_KEY_FILE is not set, so these are signed          #
#  with Android's debug key:                                      #
#    out\limo-child-release.apk                                   #
#    out\limo-guardian-release.apk                                #
#  For trying on a phone over a cable only, never for a family:   #
#  the debug key is public, so anyone could sign an update that   #
#  Android would accept in place of LiMo.                         #
#  To sign with the release key, see "Signing" in README.md.      #
###################################################################
```

With it:

```
[build] Signed with the release key LIMO_RELEASE_KEY_FILE names: out\limo-child-release.apk and out\limo-guardian-release.apk
[build]   CN=LucidLoom AI, O=LucidLoom AI
[build]   certificate SHA-256 AB:CD:...
```

**The schemes** are Android Gradle's defaults for minSdk 26, and nothing here
changes them: APK Signature Scheme v2 only. v1 (JAR signing) is only for
Android older than 7.0, and v3 is off unless asked for. `npm run inspect`
prints the schemes of each APK.

### Making the release key

The owner does this once, on the PC that builds the releases. The key never
goes into a repository, this one or any other.

1. A folder outside every repository, readable only by your own Windows
   account. In PowerShell:

   ```
   mkdir C:\keys
   icacls C:\keys /inheritance:r /grant:r "${env:USERNAME}:(OI)(CI)F"
   ```

   The second line takes away every permission the folder had from above it
   and gives your own account, alone, full control of it and of every file
   made in it. Not a folder that a cloud service copies by itself.

2. The key, with `keytool`, which comes with the JDK:

   ```
   keytool -genkeypair -v -storetype PKCS12 -keystore C:\keys\limo-release.p12 -alias limo -keyalg RSA -keysize 4096 -validity 10958 -dname "CN=LucidLoom AI, O=LucidLoom AI"
   ```

   RSA 4096, valid for 30 years (10,958 days), under the alias `limo`. The
   name is the maker's, never a person's: it is in every APK, for anyone to
   read. keytool asks for the keystore's password twice: a long one, kept in
   a password manager. In a PKCS12 keystore the key has the same password.

3. The properties file, `C:\keys\limo-release.properties`:

   ```
   storeFile=limo-release.p12
   storePassword=<the keystore's password>
   keyAlias=limo
   keyPassword=<the same password>
   ```

   `storeFile` is a full path, or a path from this file's own folder: the
   keystore's name alone, beside it, is simplest. In a properties file a
   backslash starts an escape and is dropped, so a full path is written with
   forward slashes (`C:/keys/limo-release.p12`). Everything after `=` counts,
   a space at the end of a line too. The file is read as UTF-8.

4. A copy off the PC: the keystore on a USB stick kept somewhere safe, and
   its password in the password manager, never beside it. Without both, no
   update of LiMo can ever be signed again.

### Building with it

```
$env:LIMO_RELEASE_KEY_FILE = 'C:\keys\limo-release.properties'
npm run build:both
npm run inspect
```

That sets the variable for one PowerShell window; `setx
LIMO_RELEASE_KEY_FILE C:\keys\limo-release.properties` keeps it for every
window opened after. Only a release build reads it: a Gradle run that builds
no release, such as `npm run test:child`, never checks it.

**What is checked, and when.** Before anything runs (no prebuild, no
Gradle), `scripts/build.mjs` checks the key with
`scripts/ReleaseKeyCheck.java`, and stops in plain words when it is not
right. Gradle checks it again with the same file, in the task
`limoCheckReleaseKey`, before anything of a release build runs, so a hand
run of `gradlew assembleRelease` is checked too. After the build, apksigner
reads each APK: one signed with any other key is deleted and the build
stops. No message, log or error holds a password, a path of the key or a
value from the properties file, since a build's log may be pasted anywhere.
Android Gradle's own message for a wrong password names the keystore's path;
the check comes first, so it is never reached.

| What is wrong | What the build says |
| --- | --- |
| The variable is not a full path | `LIMO_RELEASE_KEY_FILE must be a full path, such as C:\keys\limo-release.properties.` |
| It points at no file, or at a folder | `LIMO_RELEASE_KEY_FILE points at no file. Set it to the full path of the release key's properties file, or leave it unset for a build signed with Android's debug key.` |
| The file cannot be read | `The file LIMO_RELEASE_KEY_FILE points at cannot be read.` |
| A key is missing or empty | `The file LIMO_RELEASE_KEY_FILE points at has no storePassword. It needs all four: storeFile, storePassword, keyAlias, keyPassword.` (naming each one missing) |
| `storeFile` names nothing | `The keystore that storeFile names does not exist. A storeFile that is not a full path is read from the folder the properties file is in.` With a single backslash in the line, instead of the second sentence: `A backslash in a properties file starts an escape and is dropped: write the path with forward slashes, such as C:/keys/limo-release.p12, or put the keystore beside the properties file and give its name alone.` |
| `storeFile` names a folder | `storeFile names a folder, not a keystore.` |
| It names a file that is no keystore | `The file storeFile names is not a keystore Java can read (PKCS12 or JKS).` |
| `storePassword` is wrong | `The keystore that storeFile names does not open with storePassword.` |
| `keyAlias` names nothing in it | `The keystore holds no entry named by keyAlias.` |
| `keyAlias` names a certificate only | `The entry keyAlias names holds a certificate only, not a key to sign with.` |
| `keyPassword` is wrong | `keyPassword does not open the key that keyAlias names. In a PKCS12 keystore, which keytool makes unless told otherwise, the key's password is the keystore's password.` |
| The key is an Android debug key | `The key keyAlias names is an Android debug key (CN=Android Debug). A release key must be one of the maker's own.` |

A password that ends with a space gets `(storePassword ends with a space,
which counts as part of it.)` after its message. `scripts/build.mjs` prints
the message after `[build] The release key:` and then `[build] Stopped before
building anything.`; Gradle after `[LiMo]`.

### Debug and release builds on one phone

Android refuses to install an APK over one signed with another key (`adb`
says `INSTALL_FAILED_UPDATE_INCOMPATIBLE`, the phone "App not installed").
The debug build is always signed with Android's debug key, which the PC
test tool needs. So once the release builds carry the release key, the
debug build and a release build cannot replace each other: going between
them on one phone means removing LiMo first, which takes its pairing and its
state with it, so the phone is set up again. Without the release key both
carry the debug key and replace each other, as before.

### Losing or changing the key

If the key is lost, or another key is used, Android will never again accept
an update for an installed LiMo: every phone needs LiMo removed and installed
again, and set up again. Nothing gets around this for an app installed by
hand. In strict mode, later, LiMo will be the phone's device owner, which a
family cannot simply remove and install again, so there the same key will
matter even more.

### How it is wired, and why

`expo prebuild` makes `android/` again on every build, so nothing written
into it by hand survives. The signing is a Gradle script,
`scripts/release-signing.gradle`, which `plugins/with-limo-android.js`
applies at the end of `android/app/build.gradle` on every prebuild, after
the template's `android { }` block, so that what it sets wins over the
template's release signing. Applied there, rather than passed by
`scripts/build.mjs` as an init script the way `scripts/debug-bundle.gradle`
is, it holds for every Gradle run of a release build, a hand run of
`gradlew` too, and it sets the signing in the app's own `android { }`
settings, where Android Gradle reads it, with no hook into Android Gradle's
later steps. It reads the variable while Gradle runs, so `android/` never
holds a path or a password of the key. Its check runs as a task before the
release build, not while Gradle reads the build files, so a Gradle run that
builds no release is never stopped by a key. The plugin stops prebuild if
the template ever stops signing its release build with the debug key, so
the script never quietly does something else.

### The tests

`npm run test:signing` makes throwaway keystores with `keytool` in a new
temporary folder, with a random throwaway password, and needs no real key.
It shows that `storeFile` as a bare name, as a path from the properties
file's folder and as a full path all open the key; that each failure above
gives its message and prints no password, no path of the key and no file
name; that `scripts/build.mjs` stops on a wrong key before anything runs and
leaves `android/` alone; then, with real Gradle builds of the child form,
that a release build carries the throwaway certificate (storeFile a bare
name through `scripts/build.mjs`, and a full path through `gradlew` by
hand), that no file under `android/` holds the password afterwards, that
Gradle stops in `limoCheckReleaseKey` with the check's message for a wrong
password and for no file, that Android Gradle's own message for a wrong
password holds no password either, and that the debug build stays on the
debug key with the variable set. The APKs go to a temporary folder, never
to `out/`, and every throwaway key is deleted at the end. It takes about
DURATION on the development PC with Gradle's caches warm, and makes
`android/` again for the child form, so it never runs beside another
Gradle run of this app. `npm run test:signing -- --quick` runs the key
checks alone, without Gradle, in about 20 seconds. Neither is part of `npm
test`, which needs nothing but Node.

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
| Components of its own | the seven below | none |
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
console.tsx               the guardian console's page, for the web only (npm run build:console)
app/
  child/index.tsx         the child form: its screens (src/child)
  guardian/index.tsx      the guardian form's start screen
src/
  form.ts                 which form this build is (from the app's embedded config)
  host.ts                 THE seam: host.request(name, args), childHost and guardianHost, see below
  childSeam.ts            the child's part of the seam, as plain data, and its readers
  guardianSeam.ts         the guardian's part of the seam, and the reader of the door's answers
  guardian/               the guardian's screens: SignIn.tsx (G0) and its words
  console/                the console's page: ConsoleApp.tsx, the page's key guard, the self check's driver
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
  seam-answers/           the child module's real answers, written by its JVM tests, read by seamContract.test.ts
jest.config.js
modules/limo-child/       the child module (a local Expo module)
  expo-module.config.json, index.ts
  android/build.gradle
  android/src/main/AndroidManifest.xml      every component, once
  android/src/main/java/ai/lucidloom/limo/child/
    host/   seam/   watch/   show/   keep/   link/   strict/
  android/src/main/res/   the accessibility and admin settings, words, a theme, the notification's icon
  android/src/debug/      the development receiver (folder dev/), debug builds only
  android/src/test/       the JVM tests: the Rust engine from Kotlin, and each folder's
  android/src/generated/  written by scripts/make-design.mjs and scripts/build-engine.mjs, git ignored
plugins/
  with-limo-android.js    Gradle settings, theme, backups, launcher icon, debug manifests, release signing
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
Its manifest declares every Android component the child form needs. The
Kotlin lives in `ai.lucidloom.limo.child.<folder>`, one folder per piece,
each with its README; the module's own namespace
(`ai.lucidloom.limo.child.module`) only names its R class, which must not
clash with the app's.

| Folder | Class | Manifest entry | Guard | What it does |
| --- | --- | --- | --- | --- |
| `watch/` | `WatchService` | accessibility service, exported, `res/xml/limo_watch_service.xml` | `BIND_ACCESSIBILITY_SERVICE` | which app is in front, by package name only; starts the keeper when it connects; its overlay windows carry the covers (`show/`) |
| `watch/` | `ClockReceiver` | receiver, not exported: `TIME_SET`, `TIMEZONE_CHANGED` | | clock changes, as facts for the engine |
| `show/` | `WaysOutActivity` | activity, not exported, own task, not in Recents | | ask with a typed reason, a code, the PIN |
| `show/` | `CoverShower` | none | | the block screen, the lock and the warning, drawn in the accessibility service's overlay windows as the host orders; installed by `keep/Keeper` |
| `host/` | `Host` | none | | the engine's host: its own thread, the kept state (`files/limo/state`), a tick about once a second, the orders handed out |
| `keep/` | `KeepService` | foreground service, not exported, type `specialUse` | | keeps the process alive while paired; the time left notification |
| `keep/` | `RestartReceiver` | receiver, not exported: `BOOT_COMPLETED`, `MY_PACKAGE_REPLACED` | | starts the keeper again after a restart or an update |
| `keep/` | `AlarmReceiver` | receiver, not exported, no filter | | LiMo's own alarm: wakes the host, brings LiMo back |
| `keep/` | `AdminReceiver` | device admin receiver, exported, `res/xml/limo_admin.xml` (no policies) | `BIND_DEVICE_ADMIN` | (a stub, for the base protection) setup's Protection step asks to turn it on; it holds no policy and only stops LiMo being removed while it is on; later the device owner of strict mode |
| `dev/` (debug only) | `DevReceiver` | receiver in `src/debug`, exported: `ai.lucidloom.limo.child.dev.PAIR`, `ai.lucidloom.limo.child.dev.FORGET` | `android.permission.DUMP` (only adb and the system hold it) | a test family's pairing over the cable, and forgetting it |
| `host/` | `LimoChildModule` | none (an Expo module) | | the child's side of the host seam; starts the keeper when the app opens |
| `seam/` | none | none | | answers the screens' `child.*` requests and sends the `change` event: the engine's view turned into the screens' shapes, Android's screens opened for each step; tells the watch when the main activity is LiMo's own screen in front |
| `link/` | none | none | | not built yet: the link to the master PC, to run inside `KeepService` |
| `strict/` | none | none | | not built yet: strict mode, whose device owner will be `keep/AdminReceiver` |

The class names are part of the app: Android remembers an accessibility
service a guardian switched on, a device admin and a device owner by class
name, so renaming one later switches it off on every phone.

The accessibility service's settings are proof two's: window state changes
only, no window content (`canRetrieveWindowContent="false"`), no gestures, no
key filtering, `isAccessibilityTool="false"`. Its words in Settings say what
it does: LiMo is told which app is in front, by its name only, and never
what is on the screen. The device admin asks for no policy at all, as in
proof five; its words say exactly that, and that it is asked only so that
LiMo cannot be removed while it is on.

`LimoChildModule` answers `about`, and every `child.*` request of
[The child's seam](#the-childs-seam) through `seam/` (its README says what
each does on Android); any other name is refused with the code
`UnknownRequest`.

## The Rust engine

The child form carries the phone's Rust: the engine and the phone's side of
the link, through the bridge crate [`../ffi`](../ffi/) (`limo-phone-ffi`)
and the Kotlin bindings UniFFI generates for it, which call it through JNA.
Its README says what crosses and in which shape. The engine's host
(`host/`) calls it, from Kotlin on a thread of its own, never from
JavaScript.

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
link. Nothing in the APK depends on this step. The same run has the tests of
`watch/`, `host/` (a day through the host on the real engine), `keep/`,
`dev/`, `seam/` and `show/` (the covers and the ways out, with pictures of
every screen drawn on the PC by Robolectric, a test only dependency), each
listed in its folder's README. The release unit
tests also compile `src/debug/java` (source set `testRelease` in the
module's `build.gradle`), so the debug only `dev/` code is tested too;
tests never ship.

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
| `READ_PHONE_STATE` | yes | no | the Phone permission (the owner, 20 Nov 2026), asked at setup: `watch/` knows that a call rings or is on, a silent ring among them, so a lock never hides a call. Never a number, never who, never how long |
| `REQUEST_IGNORE_BATTERY_OPTIMIZATIONS` | yes | no | lets setup show Android's own question whether LiMo may be left out of battery optimisation (`keep/Battery`); Android asks the person |
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
contacts, accounts, the call log, phone numbers, shared storage, exact
alarms, or "draw over other apps": the covers are accessibility overlays
(`show/`), which need no permission. The guardian form does not ask for the
phone's state either. `app.config.ts` lists everything no form may hold, and
the build removes each one even when React Native's template or a library
asks for it (`tools:node="remove"`). The guardian form also refuses the
child form's own permissions, the Phone permission among them, so nothing
of the child's can reach it by accident. `plugins/with-limo-android.js`
removes `SYSTEM_ALERT_WINDOW` from the debug manifests too, where React
Native's template adds it.

Not declared yet, for the piece that needs them to decide: `INTERNET` and
`ACCESS_NETWORK_STATE` (the link). Exact alarms are not needed: the host's
alarm is an ordinary one (`keep/README.md`).

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
| `console` | the page runs in the guardian console's window on the PC (Tauri's `__TAURI_INTERNALS__` is on the page) | the console's Rust side, through its one command `request`, which asks LiMo's service through the door (`../../console/README.md`) |
| `none` | no native module (the guardian form, until its link exists) | what JavaScript knows by itself |
| `mock` | a development run with no native module | the same, never in a release build; it holds no made up data |

Requests are typed in one list (`Requests`), with what each is sent and
answers, so a screen cannot ask for one that does not exist. Errors are a
`HostError` with a `code`. The guardian's link on the phone joins here as a
further place, not in the screens. JavaScript is never on the enforcement
path: the Kotlin side calls the Rust engine itself, and on the PC the
service alone holds the family's file.

**The guardian's part** is `guardianHost` (`src/guardianSeam.ts`):
`about()` and `signIn(password)`, the door's `about` and `sign_in`, each
answered with one `DoorOutcome` (about, signed in, wrong with the tries
left before a wait, a wait until a time, a child's account, an account the
door cannot tell, or a problem in the console's own words). It never throws
for the door's sake. Where nothing answers a guardian yet (the guardian form
on a phone), it answers with a problem that says the settings open in the
console on the family's PC.

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
| S9 | This phone is set up | six rows: a teal tick for each step Android says is on, and for a step that is off an ink ring in the tick's place and NOT ON (coral) at the end of its row, with one sentence under the rows that names it (or says how many) and that the child's Today shows a guardian how to turn it on; no row marked either way when the host cannot say; leaving it finishes setup on the phone, after which LiMo counts and covers |
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
| `finishSetup()` | `child.finishSetup` | nothing, once the phone has it: a guardian left S9 ("Show <child> how it works"), with every step on or one passed. Until then LiMo counts nothing and covers nothing, and its notification says "Setup is not finished on this phone." Asked again changes nothing; the introduction seen asks it again, in case this was lost |
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

**Where the words come from.** All of them are the engine's, in its view's
`today` and `rules_words` (`../engine/README.md`, "The child's own
screens"): the time left ("1 h 12 min", "No limit today", "Open until 9:30
pm"), the line "48 min used of 2 h", where the time went, "Screens are off
from 9:00 pm to 7:00 am.", each app's minutes, the rules in words, and the
sentence for each step that is off. Kotlin puts each app's name in place of
its package (`seam/`) and passes the rest through; it never makes them.
The use on the child's other devices comes from the master in every grant:
once one for the day says it, `used` is the whole day and `where` says
"On this phone 31 min · on the PC 17 min" (or "· on other devices 17
min"); before that, and on a phone that never reaches its master, `where`
says only "On this phone 31 min".

**The Kotlin side** is `modules/limo-child/android/.../seam/` (its README
says what each request does on Android, where agreeing and the
introduction are kept, and what throttles `change`). Where no child module
answers, a request is refused, and `childHost.pairing()` takes that as not
paired, so the child form shows S0 and asks nothing else. A malformed
answer fails with a message naming the field; a step reads as on only for
the exact word `"on"`.

**The contract between the two sides.** The child module's JVM tests turn
the real engine's view, from a made up family, into each answer and write
it into `tests/seam-answers/` (`{ "request", "answer" }`, again at every
`npm run test:child`, and only when it changed); `tests/seamContract.test.ts`
reads every one through the readers above. A change on either side that
breaks the other fails there. To refresh the files, run `npm run
test:child`, then `npm test`.

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

`npm test` runs 130 tests with jest (Expo's preset for Android), against the
mock host. Nothing in them runs on a phone.

| File | What |
| --- | --- |
| `tests/setup.test.tsx` | S0, also as the app shows it with no native side; S0 moving on when the PC pairs; six steps in their order; Continue switched off until the box is ticked; each step not on yet and then on; the Phone step's words and note; a step shown on only when the host says so, and asked again when the app is back in front; Android's screen not opening; the Samsung note on a Samsung only, and not when the maker is unknown; each of the six steps offering to go on after two tries still off, and setup going on; a try counted only when it comes back off; the way on turning into Next; tries kept when going back; done; the introduction to Today, whose banner names the step gone past |
| `tests/app.test.tsx` | Today in the engine's words, never "counting"; times kept whole; other times left; asking for more time; the banner in the host's words only, with one, two and three steps off, and putting each right; going on past a step from the banner; How it works; every tab keeping its size; long names |
| `tests/seam.test.ts` | the readers of what Kotlin sends, the steps a guardian is needed for and their sentences lining up among them, and who made the phone; the app's host with no child module: not paired, nothing else answered |
| `tests/access.test.tsx` | a role, a label and 48 of height for everything to tap, the way on and the banner with three off among them; headings; the reading order |
| `tests/release.test.ts` | no screen names a native module, the network or a link out, or holds the engine's sentences for a step that is off; only `src/host.ts` reaches the child module; the app's imports, as Android resolves them from `index.ts`, never reach the mock, the preview or the tests |
| `tests/seamContract.test.ts` | the child module's real answers (`tests/seam-answers/`, written by its JVM tests) each pass the reader the app's host checks it with, unchanged; none missing and none unknown; the engine's kinds turned, apps by their names only, what is switched off in setup order with its sentences |
| `tests/console.test.tsx` | the console's sign in (G0): one password field after the door answered, the tries left and a wait in words, the field emptied after each try, a child's account told at once with no field, the door's problems in plain words, three lines of room under the field; the reader of the door's answers; the `console` place found by Tauri's bridge and each request sent to the one command by the door's name; the page's key guard, which stops the browser's keys and leaves a field's own alone; the console's entry reaching neither the mock, the preview nor the child's screens |

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
| `build.mjs` | `build:child`, `build:guardian`, `build:both`, `build:child:debug` | prebuild and a release build of a form, into `out/` (or `--out <folder>`); with `--debug`, a debug build (`out/limo-<form>-debug.apk`); checks the release key first and says at its end how the APKs are signed |
| `debug-bundle.gradle` | | a Gradle init script for debug builds only: the JavaScript inside the debug APK, as in a release build |
| `release-signing.gradle` | | applied at the end of `android/app/build.gradle`: release builds signed with the key `LIMO_RELEASE_KEY_FILE` names (see [Signing](#signing)) |
| `ReleaseKeyCheck.java` | | checks that key, run with `java` by `build.mjs` before anything runs and by Gradle before a release build |
| `signing.mjs` | | what `build.mjs`, `inspect.mjs` and the signing tests share: the key check, and how apksigner reads an APK |
| `test-signing.mjs` | `test:signing` | the signing tests, with throwaway keys; `--quick` without Gradle |
| `build-engine.mjs` | `build:engine` | the Rust bridge and its bindings, for the phone (run by the child module's Gradle build) or for the JVM tests on this PC |
| `test-child.mjs` | `test:child` | the child module's JVM tests |
| `inspect.mjs` | `inspect` | looks inside `out/*.apk` and checks them, their signing too |
| `preview.mjs` | `preview` | the screens' preview, into `dist/preview`, made to open from disk |
| `build-console.mjs` | `build:console` | the guardian console's page, into `dist/console`, which the console's exe embeds: one bundle from `console.tsx` (Expo's `export:embed` for the web, so neither the preview nor the mock is in it, which the script checks), a page of its own with the three fonts and react-native-web's style element (see `../../console/README.md`), the fonts with their licences, and the logo |
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

With the engine's host, the keeper, the Phone permission and the debug
build (20 Nov 2026), all on the PC: `npm run test:child` ran its 62 JVM
tests, all passing (43 before: the engine and the link from Kotlin, and the
watch; and 19 new: the host on the real engine, the keeper's look and the
schedule, the test family's pairing file, the watch's calls with and
without the Phone permission, and the folders' house rules), with no
compiler warning in the module; `npm run typecheck` had no errors; `npm run
build:both` built both forms (Gradle 160 s for the child, 139 s for the
guardian); `npm run inspect` passed every check, the child form holding
`READ_PHONE_STATE` and the guardian form not; `npm run build:child:debug`
built `out/limo-child-debug.apk` (55,683,658 bytes, Gradle 135 s):
debuggable, the development receiver guarded by `DUMP` with its two
actions, the JavaScript bundle and the Rust library inside, no `INTERNET`.
The child form's release APK is 30,423,233 bytes (about 220 KB more: the
host, the keeper and their resources); the guardian's 28,273,687.

With the Kotlin side of the child's seam (20 Nov 2026), all on the PC:
`npm run test:child` ran its 96 JVM tests, all passing (62 before, 28 new
in `seam/`: the engine's real view turned into Today and the rules, the
requests paired with a name, with none and not paired, the steps, the
change event's throttle, what each step opens on each Android, and the
folder's house rules; and 6 in `host/` for the child's name the host now
keeps), with no compiler warning in the module, and wrote the 21 answers of
`tests/seam-answers/`; `npm test` ran its 130 tests (28 new, the contract
with those answers), all passing; `npm run typecheck` had no errors; `npm run build:both` built both
forms (Gradle 195 s for the child, 160 s for the guardian; one earlier try
stopped on a file Windows held for a moment while `android/` was made again
for the guardian, and running it again went through); `npm run inspect`
passed every check, the child form holding
`REQUEST_IGNORE_BATTERY_OPTIMIZATIONS` and the guardian form not; `npm run
build:child:debug` built `out/limo-child-debug.apk` (55,716,486 bytes,
Gradle 161 s). The child
form's release APK is 30,456,057 bytes (about 33 KB more: the seam); the
guardian's 28,273,687, as before.

With the covers and the ways out, and the main screen telling the watch
that it is LiMo's own (20 Nov 2026), all on the PC: `npm run test:child`
ran its 160 JVM tests, all passing (96 before; 56 new in `show/`, among them
the ten that draw every cover and way out as a picture; 8 new in `seam/` for
LiMo's own screen: the order with the ways out screen, split screen, a late
first resume); `npm test` ran its 130 tests, all passing; `npm run
typecheck` had no errors; `npm run build:both` built both forms (Gradle 165
s for the child, 140 s for the guardian); `npm run inspect` passed every
check; `npm run build:child:debug` built `out/limo-child-debug.apk`
(55,831,174 bytes, Gradle 129 s). The child form's release APK is
30,521,593 bytes (about 65 KB more: the covers); the guardian's 28,273,687,
as before. A file Windows held for some seconds after a build stopped
`android/` being made again twice; `scripts/build.mjs` now waits and tries
again, for up to about 40 seconds.

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
ISC, Apache-2.0, BSD, BlueOak-1.0.0 and similar). For the child module's
JVM tests only, from Gradle's cache (`modules/limo-child/android/build.gradle`):
JNA's jar for the PC, JUnit 4.13.2 (EPL-1.0) and Robolectric 4.17 (MIT),
which brings Android's own framework for the PC (Apache-2.0); neither APK
holds any of them. No code was copied from GPL or AGPL projects. `newer-ninja.gradle` is proof three's; the start files
follow Expo's blank template (0BSD).

## Left open

- **32 bit phones.** Only `arm64-v8a` is built, as in proof three. Phones
  with Android 8 that a child is handed down are sometimes 32 bit, and a 32
  bit phone refuses the APK. Adding `armeabi-v7a` is one value in
  `plugins/with-limo-android.js` and one Rust target, and makes the native
  part about half as big again.
- **The release key.** The builds can sign with it (see [Signing](#signing));
  the owner makes it and builds with `LIMO_RELEASE_KEY_FILE` set. Until a
  build is made so, the APKs in `out/` carry Android's debug key and are for
  a cable only. Not done: v3 signing, whose key rotation could one day move
  LiMo to a new key while the old one is still at hand (Android 9 and later
  only; Android 8 would still need LiMo removed), and an app bundle for a
  store.
- **Expo Router.** See [Versions, and why](#versions-and-why).
- **The foreground service type:** `specialUse` now; Android also has
  `systemExempted` for device admin and device owner apps, for the base
  protection to weigh.
- **Libraries linked but unused.** `expo` links expo-file-system, expo-asset,
  expo-font, expo-keep-awake and `@expo/dom-webview` by itself. The JavaScript
  bundle names none of the last three, so they could be left out of
  autolinking; not done, because it cannot be tried on a phone here. Fresco's
  GIF and animated WebP support (`expo.gif.enabled`, about 320 KB) is unused
  too.
- **The console on the PC.** Its page has an entry of its own,
  `console.tsx`, so `App.web.tsx` stays the preview's. The theme's font
  families are Android ones (`Fraunces 28` and so on); in a browser
  `display()` quotes the name, as CSS needs, and the console's page
  declares the families with `@font-face`, as the preview does. The sign in
  is built from the console's description, ahead of its drawing; the
  drawing, once approved, decides its look and words.
- **The launcher icon** is a stand in from the icon set until LiMo has its
  own mark.
- **The maker's logo** is 22 on the child form's screens, as the phone
  sheet says, and `tokens.maker.logoSize` (26) on the guardian's start
  screen.
- **The child's name over the network.** It comes with the pairing, and
  the host keeps it beside the engine's state (the child module's
  `host/README.md`, "The child's name"); the development tool's pairing
  file carries it now, and the master's welcome will when pairing over the
  network is built.
- **Not drawn, decided here for the lead to look at:** the line "Android
  did not open it. Try again." under a step's stamp (S6c draws the line
  for a step still off after two tries); the banner with more
  than one step off ("And 2 more" under the first sentence); Today with no
  limit or open until (the engine's words take the time's place, without
  "Left today"); How it works leaving out a rule with nothing in it; the
  list of apps left out with no app used today.
- **On a small phone** the way on past a step sits under the stamp, inside
  the card, so on the longer steps at 360 by 640 the guardian scrolls to
  it, as to the stamp itself. The screen does not scroll by itself.
- **App tiles** on Today show the app's first letter, as the drawing does;
  the seam carries names only, not Android's icons.
- **Both forms carry the child form's JavaScript**, because `App.tsx`
  imports both forms' screens and picks one when the app starts. The
  guardian's has no child module to drive them; a later piece may split
  the bundle by form.
