# Testing the LiMo Android base lock spike

This checklist is for a person with the test phone (Samsung Galaxy S20 Ultra,
Android 13, One UI 5.1) and the Windows PC that has this folder. It answers
four questions: can the service be turned on at all (restricted settings),
how fast the cover appears, what gets around it, and how a child turns it off.
Write what you see in the Results tables at the end.

Commands are typed in PowerShell, in this spike folder, unless a step says
otherwise. `adb` is on the PATH of the build PC (it lives in
`%ANDROID_HOME%\platform-tools`).

## Before you start: safety

- Blocking turns itself off 10 minutes after it is switched on. The app shows
  the time left. Switching it on again starts a fresh 10 minutes.
- The cover always has a "Go to home screen" button. The home screen, the
  Phone app, the emergency dialer and Settings are never covered, and neither
  is the LiMo spike app.
- Turning the screen off removes the cover, and the lock screen is never
  covered. A ringing or active phone call removes the cover too.
- If anything ever seems stuck: tap "Go to home screen", or press the Side key
  to turn the screen off, or wait out the 10 minutes. Last resort: hold the
  Side key and Volume down together for about 7 seconds to force a restart.
- `adb uninstall ai.lucidloom.limo.spike02` removes everything.

## Setup

1. Turn on Developer options: Settings, About phone, Software information, tap
   Build number seven times, then enter the phone's PIN if asked. "Developer
   options" now shows at the bottom of Settings.

2. Turn on USB debugging: Settings, Developer options, USB debugging on.

3. Connect the phone to the PC with a USB cable. On the phone, a prompt asks
   "Allow USB debugging?" with the PC's key: tick "Always allow from this
   computer" and tap Allow.

4. Check the connection:
   ```
   adb devices
   ```
   Pass: one line with a serial number followed by `device`. If it says
   `unauthorized`, unlock the phone and accept the prompt from step 3.

5. Build the APK once:
   ```
   .\gradlew.bat assembleDebug
   ```
   The APK is `app\build\outputs\apk\debug\app-debug.apk`.

6. Write down, for the Results table: Settings, About phone, Software
   information: One UI version, Android version, Build number. And Settings,
   Display, Navigation bar: "Buttons" or "Swipe gestures".

7. In a second PowerShell window, start the log and leave it running for the
   whole test:
   ```
   adb logcat -c
   adb logcat -s LiMoSpike
   ```
   See "Reading the measurements over USB" below for saving it to a file.

---

## Question 1: restricted settings

Since Android 13, an app installed by tapping an APK file may not be given an
accessibility service until the user opens the app's info page, the three dot
menu, and chooses "Allow restricted settings". Test both ways of installing
and record what each does.

### Case A: installed over USB with adb

A1. Remove any earlier copy (it is fine if this says the app is not
installed):
```
adb uninstall ai.lucidloom.limo.spike02
```

A2. Install:
```
adb install -r app\build\outputs\apk\debug\app-debug.apk
```
Pass: `Success`.

A3. On the phone, open "LiMo spike 02" from the app drawer. Under "This
phone", write down Installing package, Initiating package, Package source and
Debuggable. (The same facts are in the log line starting `phone`.)

A4. Tap "Open Accessibility settings". On One UI: Installed apps (on some
versions "Installed services"), then "LiMo spike 02 blocker". Try to switch it
on.

A5. Record which of these happens:
- it switches on straight away, after a dialog asking to allow full control
  of the device (tap Allow); or
- the switch is greyed out, or a "Restricted setting" message appears.

A6. If it switched on, go back to the app. Pass: "Turned on in Accessibility
settings: yes" and "Running and connected: yes", and the log shows
`service connected`.

### Case B: copied to the phone and installed by tapping it

B1. Uninstall first, so this is a fresh install:
```
adb uninstall ai.lucidloom.limo.spike02
```

B2. Copy the APK to the phone's Download folder:
```
adb push app\build\outputs\apk\debug\app-debug.apk /sdcard/Download/limo-spike02.apk
```

B3. On the phone, open My Files (Samsung's Files app), Downloads (or Internal
storage, Download), and tap `limo-spike02.apk`. If the phone says it is not
allowed to install unknown apps from this source, tap Settings, switch on
"Allow from this source", go back and tap Install. If Play Protect asks to
scan or warns about an unknown app, choose the option that installs anyway,
and note what it said.

B4. Open "LiMo spike 02". Write down the "This phone" facts as in A3.

B5. Before trying the service, open the app's info page (the "Open this
app's info page" button) and tap the three dot menu at the top right. Note
whether "Allow restricted settings" is there yet. Go back.

B6. As in A4 and A5, try to switch the service on, and record what happens.

B7. If it was restricted, allow it:
1. In the LiMo spike app tap "Open this app's info page" (or: Settings, Apps,
   LiMo spike 02).
2. Tap the three dot menu at the top right, then "Allow restricted settings".
3. Confirm with the PIN, pattern or fingerprint if asked.
4. Go back to Accessibility, Installed apps, "LiMo spike 02 blocker", and
   switch it on. Tap Allow.

Record whether the menu item was there after the failed try, and whether the
service then switched on.

B8. Keep this install for Questions 2 to 4. Remove the copied APK:
```
adb shell rm /sdcard/Download/limo-spike02.apk
```

---

## Question 2: speed

Target: the median under 1000 ms, and ideally the slowest too.

C1. Pick two or three apps that are easy to open many times, for example
Chrome, YouTube and Samsung Notes. Their package names are shown under each
name in the app's list ("Apps: tick to block").

C2. Block them and switch blocking on. Either tick them in the app and turn
on the switch, or from the PC (use your own package names, commas, no
spaces):
```
adb shell am start -n ai.lucidloom.limo.spike02/.MainActivity --es block "com.android.chrome,com.google.android.youtube,com.samsung.android.app.notes" --ez on true --ez clear_timings true
```
Pass: the app shows the switch on and about 10 minutes left.

C3. Warm opens, first app. Ten times: from the home screen or app drawer,
open the first app. The cover appears. Tap "Go to home screen". After the
ten, open LiMo spike 02 and write down the median and slowest from "Cover
timings" for this app. Then tap "Clear timings".

C4. Repeat C3 for the second and third app. Switch blocking on again between
apps if the time left is low (running the C2 command again also clears the
timings).

C5. Cold opens, one app. Five times: open Recents, swipe the first app's card
away so it is closed, go home, open the app. Record the median and slowest as
before.

C6. By eye, during C3 to C5: before the cover, do you see the app's own
screen? Write "never", "a blink" or "clearly, about N tenths of a second". If
you have a second phone, film the screen in slow motion for a few opens; that
gives a better figure.

C7. Sound: block YouTube (or any app playing sound), start a video, go home,
and open it again. Does the sound keep playing under the cover?

---

## Question 3: what gets around it

Keep one or two apps blocked with blocking on. For each check, write what
happened: did the cover stay, did it go away, and could you see or use the
blocked app. The "Prediction" column in the Results is what the design
expects; anything different is the interesting part. Watch the log window:
an `uncover` line names the reason, and `event` lines show which package
sent each window event.

D1. Back: open a blocked app, then press Back (the button, or the back
gesture) while the cover is up.

D2. Home: while covered, press Home with the navigation button, or with the
Home gesture if the phone uses gestures.

D3. Recents: while covered, open Recents. Is the blocked app's picture
visible in its card? Then tap the blocked app's card to switch back to it.

D4. Split screen: open Recents, tap the icon above an allowed app's card,
"Open in split screen view", then pick the blocked app for the other half.
Tap in each half in turn.

D5. Pop up view and picture in picture: open Recents, tap the blocked app's
icon, "Open in pop-up view". Then try "Go to home screen" on the cover. If an
app on the phone supports picture in picture (for example Google Maps during
navigation, or a video call app), block it, start it, go home, and see what
the small window does.

D6. From a notification: open a blocked app by tapping one of its
notifications (for example a message in a blocked chat app).

D7. From a link or share sheet: in an allowed app (for example Messages or
Samsung Notes), tap a link that opens a blocked app, such as a youtube.com
link with YouTube blocked. Also try sharing something from an allowed app to
a blocked app.

D8. Rotate: with the cover up, turn the phone sideways and back (auto rotate
on). Does the cover still fill the screen? Any flash of the app?

D9. Notification shade: with the cover up, swipe down from the top. Does the
shade open? Is it drawn over the cover or hidden under it?

D10. Lock and unlock: with the cover up, press the Side key to lock, then
wake and unlock. While locked, is the lock screen fully visible and usable
(no cover over it)? After unlocking, does the cover come back, and is the app
visible in between?

D11. Edge panel (if on: Settings, Display, Edge panels): with the cover up,
try to open the Edge panel from its handle, and open an app from it.

D12. Incoming call: with the cover up, call the phone from another phone.
Can you see and answer the call? After hanging up, does the cover come back?

D13. Navigation buttons: if the phone uses navigation buttons, do Back, Home
and Recents respond while the cover is up? If you can, switch to the other
navigation mode (Settings, Display, Navigation bar) and repeat D1 to D3.

D14. Power menu: with the cover up, open the power menu (hold the Side key
and Volume down briefly, or the Side key alone, depending on the phone's
setting). Is it visible over the cover?

---

## Question 4: how a child turns it off, and what each costs them

Nothing in this spike tries to stop these on purpose. Just record what
happens. For each one, start with blocking on and a blocked app covered, do
it, then open the blocked app again and check whether it is still covered.
Also note in the app (open LiMo spike 02) whether the service is still turned
on and connected. Between checks, put things back: switch the service on
again and run the C2 command.

E1. Accessibility settings: go home, open Settings (never covered),
Accessibility, Installed apps, "LiMo spike 02 blocker", switch it off. Record
how many steps it took and whether any PIN was asked for.

E2. Turning the service off removes a cover at once. Settings is never
covered, so this needs the PC. First check that this app is the only
accessibility service on:
```
adb shell settings get secure enabled_accessibility_services
```
Only if the output names nothing but `ai.lucidloom.limo.spike02`: open a
blocked app so the cover is up, then run
```
adb shell settings delete secure enabled_accessibility_services
```
Pass: the cover goes away at once and the log shows `uncover ... reason=service_off`
(or the cover simply goes, if the service was stopped before it could log).
Switch the service back on in Accessibility settings afterwards. If other
services were listed, skip E2 and note "skipped".

E3. Force stop: Settings, Apps, LiMo spike 02, Force stop, OK. Then open the
blocked app. Is it covered? Is the service still switched on in
Accessibility settings, or did Android switch it off? Does it come back by
itself without opening LiMo spike 02?

E4. Uninstall: long press the LiMo spike 02 icon, Uninstall (or Settings,
Apps, LiMo spike 02, Uninstall). Note any warning shown and whether a PIN was
needed. Then install it again with adb (A2), switch the service on, and run
the C2 command.

E5. Samsung battery settings: Settings, Battery and device care, Battery,
Background usage limits, then add LiMo spike 02 to "Deep sleeping apps"
(and, if offered on the app's own Battery page under Settings, Apps, LiMo
spike 02, choose "Restricted"). Run the C2 command (a fresh 10 minutes),
lock the phone for about 5 minutes, unlock, and open the blocked app. Is it
covered? Is the service still on and connected? Remove the app from the
sleeping list afterwards, and set its Battery page back to what it was.

E6. Reboot: switch blocking on (C2 command, a fresh 10 minutes), restart the
phone (power menu, Restart), unlock it, and within the 10 minutes open the
blocked app without opening LiMo spike 02 first. Is it covered? Then open
LiMo spike 02: is the service on and connected? The log was cleared by the
restart; start `adb logcat -s LiMoSpike` again once `adb devices` shows the
phone.

---

## Reading the measurements over USB

Live, in a second window:
```
adb logcat -s LiMoSpike
```

Each cover shown writes one line like:
```
cover package=com.android.chrome shown_ms=137 method=commit trigger=event
```

Only the measurements, from what the phone has kept:
```
adb logcat -d -s LiMoSpike | Select-String "shown_ms"
```

Save everything to a file to attach to the results:
```
adb logcat -d -v time -s LiMoSpike | Out-File -Encoding utf8 limo-spike02-log.txt
```
The phone keeps only a limited log, so save it soon after each question. The
app itself keeps the last 50 measurements until it is closed or killed. All
log lines are explained in `README.md`, "Log lines".

---

## Cleaning up

```
adb uninstall ai.lucidloom.limo.spike02
adb shell rm /sdcard/Download/limo-spike02.apk
```
The second line is only needed if B8 was skipped; an error there just means
the file is already gone. Turn off USB debugging (Settings, Developer
options) if the phone is not used for testing again soon.

---

## Results

### The phone

| Item | Value |
| --- | --- |
| Model | |
| One UI version | |
| Android version | |
| Build number | |
| Navigation mode (buttons or gestures) | |
| Date of the test | |

### Question 1: restricted settings

| Item | Case A: adb install | Case B: tapped in My Files |
| --- | --- | --- |
| Installing package | | |
| Initiating package | | |
| Originating package | | |
| Package source | | |
| Debuggable | | |
| "Allow restricted settings" in the menu before trying (B5) | n/a | |
| Service switched on straight away, or "restricted setting" | | |
| "Allow restricted settings" in the menu after the failed try | | |
| Service switched on after allowing | | |
| Play Protect or other warnings | | |

### Question 2: speed

| App (package) | Opens | Warm or cold | Median ms | Slowest ms | App visible before the cover, by eye |
| --- | --- | --- | --- | --- | --- |
| | 10 | warm | | | |
| | 10 | warm | | | |
| | 10 | warm | | | |
| | 5 | cold | | | |

Sound kept playing under the cover (C7): ______.
`method` shown in the log lines (`commit` or `vsync`): ______.

### Question 3: what gets around it

| Check | Prediction | What happened |
| --- | --- | --- |
| D1 Back | The app gets Back. If it closes, the cover goes with it; if it only goes back a page, the cover stays. | |
| D2 Home | Gesture: works, cover goes. Button: may not respond under the cover; use "Go to home screen". | |
| D3 Recents, then switch back | Cover goes in Recents and the app's picture shows in its card; the cover returns on switching back. | |
| D4 Split screen | Only the half that last sent an event counts; the blocked half may stay visible. | |
| D5 Pop up view | The cover fills the whole screen. "Go to home screen" may need the 3 second fallback (`home_fallback` in the log). | |
| D5 Picture in picture | The small window may stay visible over the home screen. | |
| D6 From a notification | Covered, like a normal open. | |
| D7 From a link or share sheet | Covered once the blocked app opens. A link that opens inside an allowed app's browser view is not covered. | |
| D8 Rotate | The cover stays and fills the screen. | |
| D9 Notification shade | Cannot be pulled down, or opens hidden under the cover. | |
| D10 Lock and unlock | No cover on the lock screen; the cover returns after unlocking (`trigger=unlock` or `event`). | |
| D11 Edge panel | The handle is under the cover. | |
| D12 Incoming call | The cover goes while it rings and during the call, and returns afterwards. | |
| D13 Navigation buttons | Do not respond while covered. | |
| D14 Power menu | Hidden under the cover. | |

Any package in the `event` lines that you did not expect (for example a
Samsung overlay that made the cover drop): ______.

### Question 4: how a child turns it off

| Way | Steps and any PIN needed | Cover went away | Service still on afterwards | Blocking came back by itself |
| --- | --- | --- | --- | --- |
| E1 Accessibility settings, switch off | | | | |
| E2 Service off while covered (adb) | n/a | | | |
| E3 Force stop | | | | |
| E4 Uninstall | | | | |
| E5 Samsung deep sleeping apps | | | | |
| E6 Reboot | | | | |
