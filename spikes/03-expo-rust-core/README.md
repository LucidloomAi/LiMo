# LiMo spike 03: the Rust core inside the Expo app

A small, throwaway proof for the LiMo screen time limiter. It is not the real
product. It answers one question before the real build starts, and then it can
be removed:

Can one Rust crate hold the time rules, be used as an ordinary Rust library on
Windows, and be called from the Expo Android app through Kotlin bindings that
UniFFI generates, inside an Expo native module? And how heavy is that?

Everything is built on the Windows PC: no EAS, no cloud builds, no accounts.
The phone test is a short checklist in `TESTING.md`.

## What it proves, and what it does not

On the PC (done, see "Results on the PC" below):

1. The crate builds and its tests pass on Windows as an ordinary Rust library
   (`cargo test`). That is the "same code on Windows" half.
2. The same crate builds as a shared library for Android `arm64-v8a`, with
   only the NDK that is already installed, and without cargo-ndk.
3. UniFFI generates the Kotlin bindings from that library, and an Expo local
   module (Kotlin, Expo Modules API) compiles against them.
4. A release style APK builds with Gradle on this PC, signed with the debug
   keystore, with the JavaScript bundle, the Rust library and JNA inside.

On the phone (`TESTING.md`):

5. The whole chain runs: JavaScript, the Kotlin module, the generated
   bindings over JNA, Rust, and back, with no development server.
6. A Rust error reaches JavaScript as an error that carries its name
   (`ZeroLease`).
7. How long a call takes, the first one (which loads the libraries) and an
   average one.

It does not:

- test the kinds of UniFFI types the real core may need later: objects with a
  lifetime (Rust structs held by Kotlin), callbacks from Rust into Kotlin
  (for example a clock), async functions, or enums with data;
- build a Windows program that uses the crate. On Windows it is only a Rust
  library with tests. Linking it into the Windows service is ordinary Rust and
  was not in doubt;
- build for 32 bit ARM or x86 phones. Only `arm64-v8a` (see "Decisions");
- run code shrinking (R8). It is off, as in Expo's default release build. JNA
  and the bindings would need keep rules if it were turned on;
- touch iOS, Expo Go (which cannot load custom native code at all) or a
  development client;
- sign for a store. The release APK is signed with the debug keystore, only so
  that it installs over USB.

## Layout

```
03-expo-rust-core/
  core/                         the Rust crate limo_core_spike
    Cargo.toml, Cargo.lock
    src/lib.rs                  the rules, the UniFFI exports, the tests
    uniffi-bindgen.rs           UniFFI's bindings generator as a small binary
    uniffi.toml                 Kotlin package name for the bindings
  app/                          the Expo app (TypeScript)
    package.json, package-lock.json, app.json, tsconfig.json
    index.ts, App.tsx           the one screen and the startup check
    plugins/with-spike-gradle-properties.js
                                config plugin: ABI, no SDK auto install, newer ninja
    scripts/build-core.mjs      builds Rust for Android, generates bindings, copies both
    scripts/newer-ninja.gradle  points the C++ builds at a ninja that copes with long paths
    modules/limo-core/          the local Expo module
      expo-module.config.json
      index.ts                  the JavaScript side: coreVersion, decideLease, logLine
      android/build.gradle      JNA, the generated sources, the Rust build task
      android/src/main/java/ai/lucidloom/limo/core/LimoCoreModule.kt
      android/src/generated/    build output, git ignored: bindings and .so files
    android/                    made by `expo prebuild`, git ignored
  README.md, TESTING.md, .gitignore
```

## How a call travels

1. `App.tsx` calls `decideLease(3600, 3300, 600)` from `modules/limo-core`.
2. `modules/limo-core/index.ts` calls the native module `LimoCore`, found with
   `requireNativeModule`. The functions are synchronous Expo functions, so the
   call goes straight through JSI, without a promise or a thread hop.
3. `LimoCoreModule.kt` checks that each number fits a Rust `u32` (JavaScript
   numbers are doubles, and the Expo Modules API has no unsigned types), builds
   a `BudgetInput` and calls `decideLease` in the generated bindings.
4. The generated `limo_core_spike.kt` writes the record into a byte buffer and
   calls the Rust function through JNA (direct mapping, `Native.register`).
5. Rust decides, writes the `LeaseDecision` into a buffer, and the bindings
   read it back into a Kotlin data class.
6. The module turns it into a plain object for JavaScript, with the time
   Kotlin spent inside the Rust call (`kotlinToRustNanos`).

Errors: `CoreError` is a "flat" UniFFI error. Kotlin gets a sealed class
`CoreException` with one subclass per variant and the Rust `Display` text as
the message. The module turns it into an Expo `CodedException` whose code is
the variant name, so in JavaScript `error.code` is `"ZeroLease"` or
`"UsedMoreThanADay"`. The module adds two codes of its own: `InvalidInput`
(a number that is not a whole number from 0 to 4294967295) and
`CoreUnavailable` (the Rust library could not be loaded or called).

## The rule

`decide_lease(BudgetInput { daily_budget_secs, used_secs, lease_secs })`:

- time left = budget minus used, never below zero;
- granted = the smaller of the lease asked for and the time left;
- remaining = time left minus granted;
- locked = granted is zero;
- errors, checked in this order: `ZeroLease` when the lease is 0,
  `UsedMoreThanADay` when used is over 86400.

`core_version()` returns the crate version, `0.1.0`.

## The startup check

When the app starts, it calls the core by itself, once per process, and writes
these lines to logcat with the tag `LiMoSpike03` (and shows them at the bottom
of the screen):

```
core_version=0.1.0 decide_lease(3600,3300,600) granted=300 remaining=0 locked=false
decide_lease(3600,0,0) error=ZeroLease
timing first_call_ms=N calls=1000 js_avg_us=N kotlin_to_rust_avg_us=N
```

The third line is the cost: the first call (which loads JNA and the Rust
library), then the average of 1000 calls from JavaScript and the part of each
spent between Kotlin and Rust. If the Rust library cannot be loaded, the
first line says `error=CoreUnavailable` and a fourth line,
`core_version_failed ...`, says why. The JavaScript formats the lines and the
Kotlin module writes them with `android.util.Log.i`, so seeing them proves the
whole round trip.

## Build

Needs, on Windows: Rust (stable, with the MSVC toolchain), Node 22, JDK 17,
the Android SDK with platform `android-36`, build tools `36.0.0`, NDK
`27.1.12297006` and CMake `3.22.1`, and a ninja 1.12 or later. Visual Studio
2022's "C++ CMake tools for Windows" component has one, and it is found by
itself (see "Rough edges", the first one). All of these were already on the
build PC.

Once per PC:

```
rustup target add aarch64-linux-android
```

Then, from this spike folder, in PowerShell:

```
cd core
cargo test
cd ..\app
$env:EXPO_NO_TELEMETRY = "1"
npm ci
npx expo prebuild --platform android --no-install
cd android
.\gradlew.bat assembleRelease
```

The APK is
`app\android\app\build\outputs\apk\release\app-release.apk`. It is a release
build: the JavaScript bundle is inside (Hermes bytecode), so it runs without
a development server, and it is signed with the debug keystore that
`expo prebuild` puts in `app\android\app\debug.keystore`, so it installs over
USB. `assembleRelease` is the Gradle task that makes it.

`assembleRelease` also builds the Rust library and the bindings: the module's
Gradle task `buildRustCore` runs `app\scripts\build-core.mjs` before the
module is compiled, and is skipped when nothing in `core\` changed. The same
script runs by itself with `npm run build:core` (from `app\`); what it does:

1. finds the NDK (`ANDROID_HOME\ndk\27.1.12297006`, or `ANDROID_NDK_HOME`)
   and its `clang.exe`;
2. `cargo build --lib --release --target aarch64-linux-android`, with the
   NDK's clang as the linker (`--target=aarch64-linux-android24`) and 16 KB
   page alignment, set through environment variables for that one cargo run;
3. copies `liblimo_core_spike.so` to
   `modules\limo-core\android\src\generated\jniLibs\arm64-v8a\`;
4. runs UniFFI's generator from the crate
   (`cargo run --features bindgen --bin uniffi-bindgen -- generate <the .so> --language kotlin --no-format`)
   and copies the Kotlin file to `modules\limo-core\android\src\generated\kotlin\`.

Files are only rewritten when their content changed, so Gradle skips the work
after them on a rebuild.

`npx expo prebuild` makes `app\android\` from `app.json`. It is git ignored
and can be deleted and made again at any time (`--clean`); nothing in it is
written by hand. The few Android settings the spike needs come from
`app.json` and the config plugin in `app\plugins\`.

`local.properties` is not needed when `ANDROID_HOME` is set.

## Decisions

- **Expo SDK 55** (expo 55.0.32, React Native 0.83.10), as asked. It brings
  Gradle 9.0.0 and the Android Gradle Plugin 8.12.0 instead of the 8.14.3 and
  8.11.0 already in the Gradle cache, so the first build downloaded them. No
  reason found to go back to SDK 54.
- **No cargo-ndk.** The script points cargo at the NDK's `clang.exe`
  directly. Nothing to install, and the script is short.
- **Bindings and `.so` files are build output, not committed.** Committing
  the Kotlin file would save the one slow step of a clean checkout (building
  UniFFI's generator, about 1 to 2 minutes, once), but UniFFI checks at load
  time that bindings and library come from the same build, and a committed
  file that is out of date fails only on the phone. Generating it every time
  is safer and costs about 2 s per Rust change.
- **`app\android\` is not committed** (Expo's "continuous native generation").
- **arm64-v8a only.** It is the S20's ABI and nearly every phone sold in the
  last years. The APK says so (`native-code: 'arm64-v8a'`), so a 32 bit phone
  refuses to install it instead of crashing. Adding ABIs: one value in the
  config plugin plus `rustup target add` for each.
- **Synchronous Expo functions.** The calls take microseconds, so a promise
  and a thread hop would cost more than the call itself.
- **JNA from Maven Central** (`net.java.dev.jna:jna:5.19.1@aar`), the
  dependency UniFFI's Kotlin bindings expect. It brings its own
  `libjnidispatch.so`.
- **No permissions.** The template declares INTERNET, SYSTEM_ALERT_WINDOW,
  VIBRATE and two storage permissions; `app.json` removes all five. The APK
  declares only AndroidX's own app private permission.
- **The SDK is never changed by the build.** The config plugin sets
  `android.builder.sdkDownload=false`, so a missing SDK package fails the
  build with its name instead of being installed. NDK `27.1.12297006`, build
  tools `36.0.0` and CMake `3.22.1` are what React Native 0.83 pins, and all
  are installed.

## Tools and libraries

| What | Version | Licence | Role | In the APK |
| --- | --- | --- | --- | --- |
| Rust | 1.98.1, target `aarch64-linux-android` | MIT or Apache-2.0 | compiler, std | std, linked into the `.so` |
| UniFFI (`uniffi`, `uniffi_core`, `uniffi_macros`, `uniffi_meta`, `uniffi_pipeline`, `uniffi_bindgen`) | 0.32.2 | MPL-2.0 | Rust exports, Kotlin generator | `uniffi_core` in the `.so`; the Kotlin file it writes |
| Other crates in the `.so` | anyhow 1.0.104, bytes 1.12.1, static_assertions 1.1.0 and small helpers | MIT or Apache-2.0 (bytes: MIT) | used by `uniffi_core` | yes, what is left after LTO |
| Crates used only by the generator | clap, askama, goblin, cargo_metadata and others | MIT, Apache-2.0, or both; no GPL | build time only | no |
| JNA | 5.19.1 (aar) | Apache-2.0 or LGPL-2.1-or-later, used under Apache-2.0 | Kotlin to native calls | yes |
| Expo | 55.0.32 (expo-modules-core 55.0.27, expo-modules-autolinking 55.0.28) | MIT | app shell, Modules API, prebuild | yes |
| expo-status-bar | 55.0.6 | MIT | status bar | yes |
| React Native | 0.83.10 | MIT | UI | yes |
| React | 19.2.0 | MIT | UI | yes |
| Hermes (hermes-compiler) | 0.14.1 | MIT | JavaScript engine and bytecode compiler | yes |
| TypeScript | 5.9.3 | Apache-2.0 | type check | no |
| Gradle | 9.0.0 (wrapper) | Apache-2.0 | Android build | no |
| Android Gradle Plugin | 8.12.0 | Apache-2.0 | Android build | no |
| Kotlin | 2.1.20 | Apache-2.0 | module and bindings | Kotlin standard library, yes |
| Android NDK | 27.1.12297006 | Android SDK licence | clang linker for Rust, C++ for React Native | `libc++_shared.so` |
| CMake | 3.22.1 (SDK) | BSD-3-Clause | C++ builds of React Native and Expo | no |
| ninja | 1.12.1, from Visual Studio 2022 | Apache-2.0 | runs the C++ builds | no |
| JDK | 17.0.15 (Microsoft) | GPL-2.0 with Classpath Exception | runs Gradle | no |

No code was copied from GPL or AGPL projects. The app's starting files follow
Expo's blank TypeScript template (0BSD).

## What was installed on the build PC

- `rustup target add aarch64-linux-android` (the Rust standard library for
  that target). Nothing else through rustup; no cargo-ndk.
- Cargo downloads into the user's Cargo cache: UniFFI 0.32.2 and its
  dependencies.
- npm: 576 packages into `app\node_modules` only. No global installs.
- Gradle downloads into the user's Gradle cache: the Gradle 9.0.0
  distribution, the Android Gradle Plugin 8.12.0, React Native, Expo and
  AndroidX artifacts, JNA 5.19.1.
- No SDK packages, nothing elevated, no changes to other repos. Visual
  Studio's existing `ninja.exe` is used where it is, not copied.

## Results on the PC

- `cargo test`: 13 tests pass on Windows (`x86_64-pc-windows-msvc`).
- `tsc --noEmit`: no errors.
- `assembleRelease`: succeeds from a clean checkout (all 300 tasks run).
- The APK, checked with `aapt2`, `apksigner`, `zipalign` and by listing it:
  - package `ai.lucidloom.limo.spike03`, label "LiMo spike 03", version 0.1.0,
    minSdk 24, targetSdk 36, launch activity
    `ai.lucidloom.limo.spike03.MainActivity`;
  - `native-code: 'arm64-v8a'`, and `lib/arm64-v8a/` holds
    `liblimo_core_spike.so` and `libjnidispatch.so`;
  - `assets/index.android.bundle` is Hermes bytecode (it starts with the
    Hermes magic number) and contains the screen's strings;
  - the dex files contain JNA, the generated bindings and `LimoCoreModule`;
  - signed with the "Android Debug" certificate (APK signature scheme v2);
  - `zipalign -c -P 16` passes: every native library is 16 KB aligned, so
    it would also load on phones with 16 KB pages (Android 15 and later).

### APK size

The APK is 24,339,991 bytes (23.2 MiB). Native libraries are stored
uncompressed, so their size in the APK is their file size.

| Part | Bytes in the APK | Share |
| --- | --- | --- |
| `liblimo_core_spike.so` (the Rust core) | 391,712 | 1.6 % |
| `libjnidispatch.so` (JNA) | 165,992 | 0.7 % |
| JNA's Java classes and the bindings, inside the dex files | not measured separately; JNA's `classes.jar` is 223,133 bytes before dexing, the bindings file is 47 KB of source | under 1 % |
| React Native, Hermes and Expo native libraries (13 files) | 13,429,936 | 55 % |
| dex files (React Native, Expo, AndroidX, Kotlin, JNA, bindings) | 7,119,969 compressed | 29 % |
| JavaScript bundle | 1,034,984 | 4 % |
| resources (1.2 MB), signature and alignment padding | about 2.2 MB | 9 % |

So the whole Rust chain (library, JNA and bindings) costs well under 1 MB,
about 3 % of an APK that React Native and Expo make 23 MB anyway.

Inside the Rust library: 625,408 bytes as built, 391,712 after the Android
Gradle Plugin strips the symbol table for packaging. Of its code, UniFFI and
the rules are about 37 KB. About 157 KB is the standard library's backtrace
printer (gimli, addr2line, miniz_oxide), which Rust links in to print panic
messages. That could be removed later with `panic = "abort"`, but then a
Rust panic would end the app instead of reaching Kotlin as an exception, so
it is left as it is.

### Build times

On the build PC, other builds sometimes running at the same time, so these
are rough.

| Build | Time |
| --- | --- |
| `cargo test`, first run (crates already downloaded) | 9.0 s |
| `cargo test`, again | 0.9 s |
| `npm install`, first (576 packages) / `npm ci` with the npm cache warm | 20.8 s / 11.4 s |
| `npx expo prebuild` | 1.4 to 1.7 s |
| First Gradle build ever on this PC (downloads Gradle 9.0.0, AGP 8.12.0, React Native and Expo artifacts) | 133.5 s until the C++ failure (see "Rough edges"), then 170.2 s for the build that worked: about 5 minutes |
| Clean checkout, caches warm, no Gradle daemon running: `cargo test` + `npm ci` + prebuild + `assembleRelease` | 7.6 + 11.4 + 1.7 + 210.7 s = 3 min 51 s |
| Of that, the Rust step inside Gradle (Android build 13.5 s, then the first build of UniFFI's generator), partly in parallel with the rest | 114 s |
| `app\android\` made again with `prebuild --clean`, `node_modules` kept | 145.9 s |
| Warm, nothing changed (fresh Gradle daemon) | 19.6 s |
| Warm, one line of Rust changed (Rust step 3.7 s, cargo 0.8 s) | 21.8 s |
| Warm, that Rust change undone | 11.4 s |
| Warm, one line of JavaScript changed | 16.6 s |

## Rough edges

1. **Windows path length, the one real blocker.** The first build failed in
   the C++ part of expo-modules-core:
   `ninja: error: mkdir(CMakeFiles/expo-modules-core.dir/C_/Shwetank/Work/.../app): No such file or directory`.
   CMake puts object files for sources outside its own folder under a copy
   of their full path, so the folder names pass 260 characters when the
   project sits as deep as this one. Windows long paths are already on
   (`LongPathsEnabled=1`), but the ninja 1.10.2 that comes with the SDK's
   CMake 3.22.1 does not use them; ninja 1.12 and later do. The Android
   Gradle Plugin looks for ninja next to CMake first, then in the SDK's CMake
   folders, and only then on the PATH, so putting a newer ninja on the PATH
   does nothing. What works: passing `-DCMAKE_MAKE_PROGRAM=<newer ninja>` to
   every CMake build, which the plugin then uses for both the configure and
   the build step. `scripts\newer-ninja.gradle` does that with Visual Studio
   2022's ninja 1.12.1 (found with `vswhere`, or set `LIMO_NINJA`), and the
   config plugin adds it to `android\build.gradle` on every prebuild. Other
   ways out, not tried here: a newer CMake from the SDK Manager (not allowed
   on this PC for this spike), or a project folder with a short path such as
   `C:\l\`. The real LiMo repo sits just as deep, so it will need the same.
   Git also prints "Filename too long" warnings for folders inside
   `node_modules`; they are all ignored files, so it does not matter.
2. **Expo SDK 55 means Gradle 9.0.0 and AGP 8.12.0**, not the versions
   already cached, so the first build is slower than expected. Gradle also
   warns that some plugins use features removed in Gradle 10.
3. **The Gradle daemon ends after every full build.** Expo's
   `gradle.properties` caps the daemon's metaspace at 512 MB, and the Kotlin
   compiler, run inside the daemon, fills it; Gradle then retires the daemon
   ("running out of JVM Metaspace"). I ran Kotlin inside the daemon
   (`-Pkotlin.compiler.execution.strategy=in-process`) only so that no
   Kotlin daemon was left running on a shared PC. For daily work, leave the
   default (a separate Kotlin daemon) or raise `MaxMetaspaceSize` to 1024m.
4. **Error text from Kotlin.** A UniFFI error enum with no fields gives
   Kotlin exceptions with an empty message. Marking it `flat_error` gives the
   Rust `Display` text instead. Expo then wraps the message as
   "Call to function 'LimoCore.decideLease' has been rejected. → Caused by:
   ..." but keeps the `code`, so the name survives; `errorText()` in the
   module's `index.ts` takes the last line.
5. **No unsigned numbers in the Expo Modules API**, and JavaScript numbers
   are doubles. The module takes doubles and checks each one is a whole
   number from 0 to 4294967295 before it becomes a Rust `u32`.
6. **UniFFI pulls in more than it needs.** With default features, `uniffi`
   compiles `cargo_metadata` and friends into the Android build. The crate
   turns default features off and turns them on only for the generator
   binary (`--features bindgen`). Even so, `uniffi_pipeline` and a few
   helpers (tempfile, rustix, indexmap) are still compiled for Android; LTO
   drops them from the library, so they cost compile time only.
7. **ktlint is not installed**, and UniFFI warns that it cannot format the
   Kotlin file. The script passes `--no-format`.
8. **16 KB pages.** Rust's Android target with this NDK links with 4 KB
   alignment by default. The script adds `-z max-page-size=16384`; without
   it the library would not load on a phone with 16 KB pages, and Google
   Play would refuse the app. JNA 5.19.1's library is already 16 KB aligned.
9. **Expo Go cannot run this app** (it has its own native code). Only a
   build made with Gradle, or a development client, can.
10. **Code shrinking is off**, as in Expo's default release build. If it is
    turned on, R8 needs keep rules for `com.sun.jna.**` and
    `ai.lucidloom.limo.core.ffi.**`, because JNA binds the native functions
    by name. Not tested.
11. **The Expo prebuild warned** that `userInterfaceStyle` needs
    expo-system-ui; the key was taken out of `app.json`.
12. **PowerShell shows cargo's and npx's normal progress output in red** as
    `NativeCommandError`, because they write it to stderr. It is not an
    error.

## What only the phone can tell

- that the app opens without a development server and the screen is
  legible (the top padding assumes the status bar height Android reports);
- that JNA and the Rust library load on Samsung's Android 13 and the two
  log lines appear;
- that a Rust error arrives in JavaScript with `code` set to its name. This
  follows from Expo's source (a wrapped error keeps the code of its cause),
  but has not been seen running;
- the real cost of a call. Expected: a first call of a few milliseconds
  (loading two libraries and checking UniFFI's checksums), then some
  microseconds per call, most of it JNA and UniFFI's buffer copying, not the
  rule itself.

## First run on the phone, 17 Nov 2026

Installed over USB with `adb install -r` on the Galaxy S20 Ultra (SM-G988B,
Android 13, One UI 5.1) and started with `adb shell am start`. No development
server was running. The phone was locked at the time, so the screen itself has
not been looked at yet; the startup check ran and logged:

```
I/LiMoSpike03: core_version=0.1.0 decide_lease(3600,3300,600) granted=300 remaining=0 locked=false
I/LiMoSpike03: decide_lease(3600,0,0) error=ZeroLease
I/LiMoSpike03: timing first_call_ms=60.939 calls=1000 js_avg_us=311.62 kotlin_to_rust_avg_us=245.21
```

So on a real phone: JNA loaded the Rust library straight from the APK, the
record went in and the record came out, and the typed error reached
JavaScript with its name. The first call took 61 ms, which is the library
loading. After that a call cost about 0.3 ms from JavaScript, of which about
0.25 ms was between Kotlin and Rust. That is slower than the "some
microseconds" guessed above, and was measured with the screen locked, when
Android may slow the processor. It is still far below anything a person
notices, but the real core should not be called in a tight loop.

## Unknowns

- How UniFFI's objects, callbacks and async functions behave through Expo;
  the real core may need at least callbacks (a clock or a store passed in).
- Whether the bindings generator's first build time (1 to 2 minutes on a
  clean checkout) matters for the real project's build server.
- Whether a newer CMake from the SDK Manager would fix the path problem by
  itself. Not tried, because this spike may not install SDK packages.
