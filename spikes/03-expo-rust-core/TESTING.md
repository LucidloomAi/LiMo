# Testing LiMo spike 03 on the phone

This checklist is for a person with the test phone (Samsung Galaxy S20 Ultra,
Android 13) and the Windows PC that has this folder. It answers one question:
does the chain from the Expo app's JavaScript, through the Kotlin module and
the UniFFI bindings, into the Rust core and back work on a real phone, with no
development server? Write what you see in the Results table at the end.

Commands are typed in PowerShell, in this spike folder
(`spikes\03-expo-rust-core`), unless a step says otherwise. `adb` is on the
PATH of the build PC (it lives in `%ANDROID_HOME%\platform-tools`).

Nothing here changes the phone's settings. The app asks for no permissions.

## Setup

1. USB debugging must be on and this PC allowed (the steps are in spike 02's
   `TESTING.md`, Setup 1 to 3).

2. Check the connection:
   ```
   adb devices
   ```
   Pass: one line with a serial number followed by `device`.

3. If the APK is not built yet, build it (README, "Build"). The APK is:
   ```
   app\android\app\build\outputs\apk\release\app-release.apk
   ```

## Install and read the log

1. Remove any earlier copy (an error saying it is not installed is fine):
   ```
   adb uninstall ai.lucidloom.limo.spike03
   ```

2. Empty the phone's log, so only this run shows:
   ```
   adb logcat -c
   ```

3. Install:
   ```
   adb install -r app\android\app\build\outputs\apk\release\app-release.apk
   ```
   Pass: `Success`.

4. On the phone, open "LiMo spike 03" from the app drawer. No PC server is
   needed: the JavaScript is inside the APK. (From the PC instead:
   `adb shell am start -n ai.lucidloom.limo.spike03/.MainActivity`.)

5. Read the lines the app wrote by itself on start:
   ```
   adb logcat -d -s LiMoSpike03
   ```
   Pass: these lines, after the date, time and process numbers that logcat
   adds:
   ```
   I LiMoSpike03: core_version=0.1.0 decide_lease(3600,3300,600) granted=300 remaining=0 locked=false
   I LiMoSpike03: decide_lease(3600,0,0) error=ZeroLease
   I LiMoSpike03: timing first_call_ms=N calls=1000 js_avg_us=N kotlin_to_rust_avg_us=N
   ```
   The first two lines are the proof. The third is how long the calls took
   on the phone: `first_call_ms` is the very first call, which also loads
   JNA and the Rust library; `js_avg_us` is the average of 1000 calls from
   JavaScript into Rust and back; `kotlin_to_rust_avg_us` is the part of that
   spent between Kotlin and Rust. Copy the numbers into the Results.

   The lines are written once each time the app starts. To see them again,
   close the app (Recents, swipe it away) and open it again, or from the PC:
   ```
   adb shell am force-stop ai.lucidloom.limo.spike03
   adb shell am start -n ai.lucidloom.limo.spike03/.MainActivity
   ```

6. If no lines appear, look for a crash instead:
   ```
   adb logcat -d -s LiMoSpike03 ReactNativeJS AndroidRuntime
   ```
   A line `core_version_failed CoreUnavailable: ...` means the app started
   but the Rust library could not be loaded or called; the rest of that line
   says why. Copy any of these lines into the Results.

## The screen

From the top:

- "LiMo spike 03";
- "Rust core version" and below it `0.1.0`, which came from Rust;
- three number fields: "Daily budget, seconds" 3600, "Used today,
  seconds" 3300, "Lease asked for, seconds" 600;
- a button "Ask the Rust core";
- after a tap, a box with the result (Granted, Remaining, Locked) or the
  error, and how long the call took;
- at the bottom, "Startup check", the same lines as in the log.

## By hand

For each row: type the three numbers, tap "Ask the Rust core", and compare.
Write the "Call took" figures in the Results.

| Case | Budget | Used | Lease | Expected |
| --- | --- | --- | --- | --- |
| Starting values | 3600 | 3300 | 600 | Granted: 300 s, Remaining: 0 s, Locked: no |
| Normal | 3600 | 600 | 600 | Granted: 600 s, Remaining: 2400 s, Locked: no |
| Locks | 3600 | 3600 | 600 | Granted: 0 s, Remaining: 0 s, Locked: yes |
| Zero lease, error | 3600 | 0 | 0 | Error: ZeroLease, and the text "ZeroLease: lease_secs must be more than 0" |

Optional, if there is time:

| Case | Budget | Used | Lease | Expected |
| --- | --- | --- | --- | --- |
| More than a day used | 3600 | 90000 | 600 | Error: UsedMoreThanADay |
| Empty field | 3600 | (empty) | 600 | Error: InvalidInput (from the Kotlin module, not Rust) |
| Tap many times quickly | any | any | any | No crash, the result box updates |

## Cleaning up

```
adb uninstall ai.lucidloom.limo.spike03
```

## Results

| Item | Value |
| --- | --- |
| Phone model, Android version | |
| Date of the test | |
| `adb install` said Success | |
| App opened with no PC server running | |
| Log line 1 as expected | |
| Log line 2 as expected (`error=ZeroLease`) | |
| `first_call_ms` | |
| `js_avg_us` | |
| `kotlin_to_rust_avg_us` | |
| Version on screen `0.1.0` | |
| Starting values: result | |
| Normal: result | |
| Locks: result | |
| Zero lease: result | |
| "Call took" for one tap (JavaScript to Rust and back) | |
| Any crash, red screen or `CoreUnavailable` | |
