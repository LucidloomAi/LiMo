# LiMo spike 07: the cable proof

A small, throwaway proof for the LiMo screen time limiter. It is not the real
product. It answers one question on real hardware, and then it can be
removed:

Can LiMo's program on the PC reach a child's phone over the USB cable by
itself, with nothing of Google's installed, and do everything adding a phone
needs: be allowed by the phone, install an app, hand that app a secret that is
never on a command line, carry a connection from the phone's own loopback to
the PC, start the app, and switch USB debugging off again? And which driver
does Windows bind to the phone's debugging interface, on a PC that has had
Android tools and on one that never had?

Four routes are compared, step by step:

| Route | `--via` | What it is |
| --- | --- | --- |
| This program's own adb over WinUSB | `winusb` | adb's protocol written here (`pc\src\protocol.rs`), over Windows' own WinUSB, the phone found and opened the way Google's adb finds and opens it on Windows. No libusb, no nusb, no adb.exe, no adb server. Added after the first run (below). |
| adb_client over libusb | `libusb` | adb's protocol in Rust (the `adb_client` crate, 4.0.0) inside this program, straight to the phone over USB through its own USB layer (rusb and libusb, compiled in). No adb.exe, no adb server. |
| adb_client over nusb | `nusb` | The same adb_client, its bytes carried to the phone by nusb 0.2.9, a USB layer in Rust alone. No adb.exe, no adb server, no C. |
| Google's adb | `adb` | Google's own adb.exe, run as a separate program, the way LiMo's phone development tool runs it. |

The owner chooses between speaking adb from Rust and shipping Google's adb
once the runs are done; this proof gives the facts for that choice, not a
verdict.

**The first run, 21 November 2026, at the PC that builds LiMo, with the test
phone.** Windows had bound WinUSB to the phone's debugging interface, from
Samsung's own driver package, under Samsung's own composite driver
(`dg_ssudbus`). Both adb_client routes failed before the phone asked anything:
libusb with "Operation not supported or unimplemented on this platform", nusb
with "incompatible driver is installed for this device". Google's adb reached
the phone and passed every step but `reverse` (see [Results](#results)). So
the fourth route opens the phone as Google's adb does, with no program of
Google's. The run also showed two faults of this proof, both fixed: `reverse`
sent its request to a test app whose screen was already open, and the test
app ended its sending side to say it was done, which on adb ends the whole
connection and cut the echo short.

## Contents

- [What it proves, and what it does not](#what-it-proves-and-what-it-does-not)
- [The two parts](#the-two-parts)
- [What the program never does](#what-the-program-never-does)
- [Building](#building)
- [The WinUSB route](#the-winusb-route)
- [The test app's door and loopback](#the-test-apps-door-and-loopback)
- [The run at the PC, with the phone](#the-run-at-the-pc-with-the-phone)
- [The run on a PC that never had Android tools](#the-run-on-a-pc-that-never-had-android-tools)
- [Results](#results)
- [What each outcome would mean](#what-each-outcome-would-mean)
- [Learned while building it](#learned-while-building-it)
- [Licences](#licences)
- [Tests](#tests)
- [Layout](#layout)

## What it proves, and what it does not

Once the runs are done, they show, for each route:

1. Whether it reaches the phone's debugging interface at all, with the
   driver Windows bound to it on this PC, and on a PC that never had Android
   tools.
2. Whether the phone asks "Allow USB debugging?" for the route's key, and
   whether "Always allow" holds from one run to the next.
3. Whether a release build signed with a key of its own installs over the
   cable with no question from Play Protect, and whether a second install
   replaces the first.
4. Whether a secret written through `content write` from standard input
   reaches a door that only the adb shell may open, every byte as sent.
5. Whether `reverse` carries a connection the app makes to 127.0.0.1 on the
   phone to a server on the PC's own loopback, with 256 KiB each way.
6. Whether `am start` opens the app.
7. Whether `settings put global adb_enabled 0` from the shell switches USB
   debugging off, and that the cable link ends there.

It does not pair a phone, run LiMo's link, touch LiMo itself, or say anything
about phones other than the test phone (a Galaxy S20 Ultra on Android 13, One
UI 5.1) or PCs other than the two it is run on. The test app is a stand in
for LiMo's child form: the real door, with the real offer, is built in the
child form later, to what this proof finds.

## The two parts

- `pc\`, a Rust program, `limo-cable-spike.exe`: one step a run, each by one
  route.
- `phone\`, a plain Kotlin Android app, "LiMo spike 07", package
  `ai.lucidloom.limo.spike07`, minSdk 26 (Android 8.0, as LiMo's child form),
  target 36. One screen, plain framework views, no libraries. Its one
  permission is `INTERNET`, for its loopback client. It is installed as a
  release build, signed with a throwaway key (see [Building](#building)).

## What the program never does

- It installs and removes only its own test app, `ai.lucidloom.limo.spike07`:
  never LiMo, never anything else. `install` reads the APK on the PC first and
  refuses any other package and any debug build before anything reaches the
  phone.
- It never touches the phone's accounts, a device owner, or any setting but
  one: USB debugging, switched off by `debugging-off`, the last step, run on
  its own, only after OFF is typed.
- Every step prints first exactly what it will do to the phone, then each
  command as it sends it (`> COMMAND`), then what came back. `plan` prints the
  whole run for a person to read before starting.
- Every command it can send is in one place (`pc\src\phone.rs`), and the only
  services its own adb opens are in one place (`pc\src\protocol.rs`); a test
  reads each of them.
- The sample secret is random, made fresh in each run, never printed, never
  kept, never in any argument, and wiped from memory after.
- It never shows a serial number in full and writes it to no file: on screen
  only its last four characters (`...1A2B`), in the log of `--verbose` too.
  The WinUSB route's device path, which holds the serial number, is never
  shown at all.
- It installs nothing on the PC, needs no administrator rights, and writes
  one file: its own adb key, `%LOCALAPPDATA%\LiMo-spike07-TEST-ONLY\adbkey`.
- With `--key adb` (the WinUSB route only, for this proof only) it reads the
  adb key of this Windows account, Google's adb's own, and only reads it:
  never writes, copies or moves it, never prints it, and never says where it
  is beyond "the adb key of this Windows account". Its fingerprint is shown,
  as the phone shows it.
- Every call to the phone has a time limit; one that does not end is given
  up and said.
- By the `adb` route, every step ends with `adb kill-server` and then looks
  that nothing listens on 127.0.0.1:5037 any more.

Exit codes: 0 done, 1 failed, 2 refused (nothing was changed on the phone).

## Building

### The PC program

Needs Rust 1.98 (MSVC) and the Visual Studio 2022 C++ build tools: libusb and
`ring` are compiled from C. In `pc\`:

```
cargo build --release
cargo test
```

The program is `pc\target\release\limo-cable-spike.exe`, one file of about
3.7 MB. The C runtime is linked in (`pc\.cargo\config.toml`), and so is
libusb, so it runs on a Windows 11 PC with nothing installed; WinUSB is part
of Windows.

For the PC that never had Android tools, build it so that it carries no
folder name of the build PC (a Rust program keeps the paths of its sources
for its error messages). In PowerShell, in `pc\`:

```
$env:RUSTFLAGS = "-C target-feature=+crt-static --remap-path-prefix=$env:USERPROFILE=user --remap-path-prefix=$((Get-Location).Path)=pc"
cargo build --release --target-dir target\stripped
Remove-Item Env:RUSTFLAGS
```

and copy `pc\target\stripped\release\limo-cable-spike.exe`. (The first line
repeats the C runtime flag, since `RUSTFLAGS` takes the place of the one in
`config.toml`.)

### The test app

Needs JDK 17 and the Android SDK with platform `android-36` and build tools
36. In `phone\`:

```
.\gradlew.bat assembleRelease testDebugUnitTest
```

The APK is `phone\app\build\outputs\apk\release\app-release.apk`, the one
`install` takes unless given `--apk`. Gradle runs without a daemon and
compiles Kotlin in its own process, so nothing keeps running after a build.
Versions: Gradle 8.14.3 (wrapper, copied unchanged from spike 05), Android
Gradle Plugin 8.11.0, Kotlin 2.1.20.

**Signing.** The release build is signed with a throwaway key, never LiMo's
own key and never Android's debug key. The first build makes it with
`keytool`, with a random password kept beside it, in `phone\throwaway-key\`
(git ignored, never committed): RSA 2048, valid for a year, named "CN=LiMo
spike 07 throwaway key, O=Test only". `app\build.gradle.kts` holds no
password. `apksigner verify` reads the built APK as signed with APK Signature
Scheme v2 only, by that one certificate. Later builds keep the same key, so a
new build installs over the old one. Deleting the folder makes a new key at
the next build; a test app signed with the old one must then be removed from
the phone first, or Android refuses the install
(`INSTALL_FAILED_UPDATE_INCOMPATIBLE`).

## The WinUSB route

**How it finds and opens the phone**, as Google's adb does on Windows
(`pc\src\win\winusb.rs`, the only unsafe code beside the driver report):

1. SetupAPI's list of the device interfaces present of the Android USB
   interface class, `{F72FE0D4-CBCB-407D-8814-9ED673D0DD6B}`
   (`SetupDiGetClassDevsW` for interfaces, `SetupDiEnumDeviceInterfaces`,
   `SetupDiGetDeviceInterfaceDetailW`). Google's driver, the makers' drivers
   and an adbd asking WinUSB through Microsoft's descriptors all offer it; on
   the test phone, Samsung's driver package does.
2. Its path opened with `CreateFileW`, to read and write, for overlapped
   input and output, as WinUSB asks; then `WinUsb_Initialize`. A driver that
   is not WinUSB fails here, and is said to.
3. Its two bulk pipes from `WinUsb_QueryInterfaceSettings` and
   `WinUsb_QueryPipe`, the interface checked to be a debugging interface
   (class 0xFF, subclass 0x42, protocol 1).
4. Every transfer overlapped, with an event of its own, so that reading and
   writing run side by side; each with a time limit (`PIPE_TRANSFER_TIMEOUT`,
   with a wait of this program's own behind it that aborts the pipe and waits
   for the transfer's end); writes that fill their last packet exactly end
   with a zero length packet (`SHORT_PACKET_TERMINATE`); reads in exact
   lengths, as adb's own host side reads (see
   [Learned while building it](#learned-while-building-it)).

**The protocol is written here** (`pc\src\protocol.rs`), from AOSP's own
descriptions of it, no code copied: the messages and their header; `CNXN`
with this side's features (`shell_v2`, `cmd`); `AUTH`, the token signed with
this program's own key (PKCS#1 v1.5 with SHA-1's prefix, the token standing
in for the digest, which a test checks against adb_client's own signature),
the public key shown when the phone does not know it, and 45 seconds for the
person to allow it; streams (`OPEN`, `OKAY`, `WRTE`, `CLSE`), each message
acknowledged before the next; the second shell protocol, `shell,v2,raw:`, so
that every command comes back with its exit code and its standard error
apart, and standard input is closed after it is sent; install as Google's adb
streams it, `exec:cmd package install -r -S SIZE`, with `-r`; and `reverse`,
each connection the phone then opens relayed to 127.0.0.1 here. Nothing else:
no file sync, no root, no reboot, no other service, no TLS (only wireless
debugging uses it).

**`--key adb`, for this proof only.** By default the WinUSB route connects
with this program's own key, which the phone has never been shown, so the
phone asks "Allow USB debugging?" and someone must be there to allow it. With
`--key adb` it connects instead with the adb key of this Windows account,
Google's adb's own (in `.android` in the user's folder, or where
`ANDROID_USER_HOME` says), which a phone may have allowed long ago with
"Always allow": then the phone asks nothing, and a step runs with nobody at
the phone. The output says so ("The question on the phone was skipped: the
adb key of this Windows account was allowed on this phone before."). The key
is read only, as PKCS#8 (as adb writes it) or PKCS#1, its text wiped from
memory once read; it is never written, copied, printed or named by its place.
A phone that does not know it asks as for any key. This is for the proof
alone: the real product makes its own key, and the guardian allows it once.

**Why written here, and not a patched adb_client.** adb_client 4.0.0 is about
7,000 lines of code (3,450 of them its message devices, comments and blank
lines left out), and would need changing in
more than its exports to do this: its transport trait and device opened, its
shell made to speak the second protocol for exit codes, its install given
`-r`, its 10 second wait for the person made longer, its key kept; and it
brings rustls, ring, rcgen, regex and chrono, which a route over USB does not
use. The protocol here is about 640 lines of code, counted the same way, with
a fake phone to test it against; the WinUSB part is about 270 and the route
that joins them about 150; together they need no crate this program did not
already have. It is the smaller thing to own for years.

## The test app's door and loopback

**The door** is a content provider, `ai.lucidloom.limo.spike07.cable`,
exported and guarded by `android.permission.DUMP`, which the adb shell and the
system hold and no ordinary app can; the provider checks the caller's
permission a second time itself.

- `content write --uri content://ai.lucidloom.limo.spike07.cable/offer`, with
  the bytes on standard input: the provider hands back the writing end of a
  pipe, reads at most 1024 bytes from it into memory (never to a file),
  judges them (printable text, as an offer is, with line ends at its end let
  through), keeps their length and check, and wipes them. More than 1024
  bytes is refused (`too_long`), as is nothing (`empty`) and anything but
  text (`not_text`).
- `content query --uri content://ai.lucidloom.limo.spike07.cable/status`: one
  row of what the app has seen since its process started, never a secret:

  ```
  Row: 0 takes=1, secret=taken, bytes=155, check=1a2b3c4d, caller_uid=2000, why=none, loopbacks=1, loopback=ok, loopback_bytes=262144, loopback_ms=85, loopback_why=none
  ```

  The check is the first 8 hex digits of the SHA-256 of the bytes taken; the
  PC prints the same for what it sent. User id 2000 is the adb shell.

**The loopback client** runs when the screen is started with the int extra
`loopback_port`: it connects to 127.0.0.1 at that port on the phone, sends
256 KiB (a pattern, nothing secret), reads back exactly that many bytes,
checks them, and only then closes. It never ends its sending side to say it
is done: adb's streams have no half close, so that ends the whole connection
and loses what is still on its way back (the first run's `cut_short`). The
screen takes the extra whether it is starting or already open, and `reverse`
starts it afresh anyway (`am start -S` stops the test app first if it runs),
so a screen left open by `start` changes nothing. The echo server on the PC
keeps the connection until the phone closes it, within a minute.

**The screen** says what the app is, the phone's model and Android version,
whether USB debugging is on as the phone's setting reads now (any app can read
it), what the door last took, and how the last loopback run went.

**Its log**, tag `LiMoSpike07`: `cable secret taken bytes=N check=C uid=U`,
`cable secret refused why=W uid=U`, `loopback start port=P`, `loopback ok|failed
port=P bytes=N ms=M why=W`. Never a secret.

## The run at the PC, with the phone

About 30 minutes for the WinUSB route, at the PC that builds LiMo, with the
test phone. Everything is typed in PowerShell, in this folder
(`LiMo\spikes\07-cable`), after:

```
$s = ".\pc\target\release\limo-cable-spike.exe"
```

### Before you start

1. Build both parts (above). Install the new test app: it was rebuilt after
   the first run.
2. Close Android Studio and any emulator, and stop Google's adb server: `adb
   kill-server`. While it runs it holds the phone's debugging interface, and
   no Rust route can open it (the program says so). For the same reason, do
   not leave `adb logcat` running in another window: the program's own output
   and the test app's screen say all that is needed.
3. On the phone: unlock it; Settings, Display, Screen timeout: 10 minutes;
   USB debugging on (Settings, Developer options). Plug it in with the cable
   that came with it, into a port on the PC itself.
4. Read the whole run once: `& $s plan`.

### Steps

Write what you see in [Results](#results). If a step fails, its own words say
why; write them down, and go on with the next step that does not need it.

1. `& $s drivers`: Windows' lists only, nothing reaches the phone. Write
   down every line under "Debugging interface 1", the four "What each route
   can use" lines among them.
2. `& $s list`: what the WinUSB route, libusb and nusb each see. Nothing
   reaches the phone.
3. With nobody at the phone, first: `& $s connect --via winusb --key adb`.
   It connects with the adb key of this Windows account, which this phone
   allowed long ago. Pass: `The question on the phone was skipped: the adb key
   of this Windows account was allowed on this phone before.`, then `Connected
   through WinUSB`, then `Connected: a SM-G988B, Android 13 (API 33); ...`,
   each answer with `(exit code 0)`. Every other WinUSB step can be run the
   same way, with `--key adb`, while nobody is at the phone. Then, with
   someone at the phone, from 4 on.
4. `& $s connect --via winusb`: it prints this program's key's fingerprint
   (the key the first run made, which the phone has never been shown). The
   phone asks "Allow USB debugging?": check that the fingerprint it shows is
   the same, tick "Always allow from this computer", and tap Allow within 45
   seconds. Pass: `Connected through WinUSB`, then `Connected: a SM-G988B,
   Android 13 (API 33); USB debugging setting: 1; ...`, each answer with
   `(exit code 0)`. Write down whether the prompt showed, whether the
   fingerprints matched, and the seconds it took to connect.
5. `& $s connect --via winusb` again: no prompt now ("Always allow" held).
6. By the WinUSB route, in order (with this program's own key, or with `--key
   adb`):
   - `& $s install --via winusb`. Pass: `Android said: Success` and `The phone
     lists the test app now.` Watch the phone: write down any question it asks
     (Play Protect, or Samsung's own), its exact words, and what you tapped
     (for this test app, installing anyway is fine).
   - `& $s install --via winusb` once more: it sends `-r`, so it replaces.
   - `& $s write-secret --via winusb`. Pass: `The test app took exactly what
     this PC sent: every byte, none changed on the way.`, `written by user id
     2000`, and an exit code.
   - `& $s start --via winusb`. Pass: the test app's screen opens on the phone,
     and shows the door's last write.
   - `& $s reverse --via winusb` (with the screen still open from `start`).
     Pass: `The phone's connection came through the cable: 262144 bytes came
     and went back`, `The test app says: every byte came back`, then `Removed`.
     Write down the milliseconds.
   - `& $s uninstall --via winusb`. Pass: `The test app is gone from the
     phone.`
7. Again by Google's adb, for the fixed `reverse`: `& $s install --via adb`,
   `& $s start --via adb`, `& $s reverse --via adb`, `& $s uninstall --via
   adb`. Each ends with `adb's server is stopped: nothing listens on
   127.0.0.1:5037.`
8. If anything failed by the WinUSB route, run that step once more with
   `--verbose` and keep what it printed (any serial number in it is cut to its
   last four characters).
9. The last check: `& $s install --via winusb` (or by whichever route
   worked), then `& $s debugging-off --via winusb`. It says first that the
   cable link ends there, and asks you to type OFF. Pass: `The link ended as
   the setting changed, as expected.` (or `The phone answered before the link
   ended.`) and `USB debugging looks off`. Then, by hand on the phone:
   Settings, Developer options: is USB debugging off? Open "LiMo spike 07":
   does it say `USB debugging: off`? Then remove the test app by hand (press
   and hold its icon, Uninstall).

After this nothing reaches the phone over the cable until a person switches
USB debugging on again. The libusb and nusb routes need not be run again on
this PC: with this driver, both refused the interface.

## The run on a PC that never had Android tools

About 15 minutes, reading only, on any Windows 11 PC that never had Android
Studio, adb or a phone maker's driver.

1. Copy `limo-cable-spike.exe`, built with the paths left out (see
   [Building](#building)), onto it, with a USB stick. Nothing else is needed.
2. On the phone, switch USB debugging on again (Settings, Developer options).
   Unlock it and plug it in.
3. Within the first minute: `.\limo-cable-spike.exe drivers`. If Windows shows
   a notice about a driver, write down its words.
4. `.\limo-cable-spike.exe list`.
5. `.\limo-cable-spike.exe connect --via winusb`. The phone asks "Allow USB
   debugging?" for that PC: leave "Always allow" unticked, so that the phone
   keeps nothing of that PC, and tap Allow within 45 seconds.
6. `.\limo-cable-spike.exe connect --via libusb` and `.\limo-cable-spike.exe
   connect --via nusb`: the phone asks again each time it is reached (it kept
   nothing); Allow, unticked; libusb's wait is 10 seconds.
7. After five minutes with the phone still in: `.\limo-cable-spike.exe drivers`
   again (Windows may fetch a driver by itself in the meantime).
8. Delete the folder `%LOCALAPPDATA%\LiMo-spike07-TEST-ONLY` and the program
   from that PC, and switch USB debugging off on the phone by hand.

## Results

Leave a row empty when its step was not run yet; say why a step was skipped.
The first run's rows are filled in; the WinUSB route's wait for the next run.

### The phone and the PCs

| Item | Seen |
| --- | --- |
| Phone: model, One UI, Android, build number | SM-G988B, Android 13 (API 33); One UI and build number not written down |
| This PC: Windows version; drivers it had for the phone before | Windows version not written down; Samsung's driver package (oem56.inf, SAMSUNG Electronics Co., Ltd., version 2.21.0.0) |
| The clean PC: Windows version | |
| Date of the run | 21 November 2026 (the first run) |

### At this PC

| Step | Route | Look for | Seen |
| --- | --- | --- | --- |
| drivers | | the interface's driver (service), its package and provider, the device above and its driver, the Android class, the WinUSB GUIDs | "SAMSUNG Android ADB Interface", working; service WINUSB; package oem56.inf from SAMSUNG Electronics Co., Ltd., version 2.21.0.0; above it "SAMSUNG Mobile USB Composite Device", driven by dg_ssudbus; offers the Android USB interface class; GUID {F72FE0D4-CBCB-407D-8814-9ED673D0DD6B} (listed twice, in two cases, since put once) |
| drivers | | what each route can use, by its rules | libusb yes (wrong, rule corrected since), nusb no, Google's adb yes; the WinUSB route's rule is new |
| list | libusb, nusb | what each sees | libusb: "SAMSUNG SAMSUNG_Android" (04e8:6860); nusb: the same, debugging interface 4, the whole device driven by dg_ssudbus |
| connect | libusb | prompt shown; fingerprints the same; seconds to connect; the `Connected` line | failed before any prompt: "USB Error: Operation not supported or unimplemented on this platform"; the key was made |
| connect again | libusb | no prompt | failed the same way, twice more |
| connect | nusb | connected, or nusb's words | failed: "incompatible driver is installed for this device", claiming interface 4 |
| connect | adb | connected; prompt or not | connected, no prompt; SM-G988B, Android 13 (API 33); USB debugging 1; the test app not installed; every exit code 0 |
| install, write-secret, start, reverse, uninstall | libusb, nusb | as below | skipped: connect failed |
| install | adb | `Success`; any question on the phone, its words | Success in 2.7 s; listed after; no question written down |
| install again | adb | replaced, or Android's words (adb sends `-r`) | Success in 2.2 s, replaced |
| write-secret | adb | the same bytes and check; user id; the exit code printed | taken: 155 bytes, check the same as sent, user id 2000; exit code 0 |
| start | adb | the screen opened | started |
| reverse | adb | bytes and milliseconds; the app's verdict; removed | first try, after `start` had left the screen open: "Activity not started, intent has been delivered to currently running top-most instance", 0 bytes, the app said `failed, why cut_short`; again on a fresh install: 262144 bytes came and went back in 8 ms, but the app said `failed, why cut_short`; removed both times. Both faults of this proof, fixed since |
| uninstall | adb | gone | gone, twice |
| every adb step | adb | `adb's server is stopped` | every time |
| connect, `--key adb` | winusb | the question skipped; seconds to connect; the `Connected` lines | |
| connect | winusb | prompt shown; fingerprints the same; seconds to connect; the `Connected` lines | |
| connect again | winusb | no prompt | |
| install | winusb | `Success`; any question on the phone, its words | |
| install again | winusb | replaced (it sends `-r`) | |
| write-secret | winusb | the same bytes and check; user id; the exit code | |
| start | winusb | the screen opened | |
| reverse | winusb | bytes and milliseconds; the app's verdict; removed | |
| uninstall | winusb | gone | |
| start, reverse, again | adb | the fixed `reverse`: every byte back | |
| debugging-off | | the answer line; `USB debugging looks off` | |
| by hand | | Developer options shows USB debugging off; the test app says off | |

### At the PC that never had Android tools

| Step | Route | Look for | Seen |
| --- | --- | --- | --- |
| drivers, first minute | | as above | |
| a notice from Windows | | its words | |
| list | winusb, libusb, nusb | what each sees | |
| connect | winusb | prompt; connected, or the words | |
| connect | libusb | prompt; connected, or the words | |
| connect | nusb | prompt; connected, or the words | |
| drivers, after five minutes | | anything changed | |

### Notes

The first run: the `drivers` rule for libusb said yes for an interface on
WinUSB under Samsung's composite driver, from libusb's own source, which
names that driver; libusb refused it. The rule now says no there. Anything
else: steps that needed a second try, questions the phone asked, anything that
surprised you.

## What each outcome would mean

Each is a fact about the choice; which way to go is the owner's.

| If | Then |
| --- | --- |
| The WinUSB route passes every step on this PC, as Google's adb did | LiMo can reach a phone on WinUSB, Samsung's driver package included, with no program of Google's and no C library: adb spoken by LiMo's own code over Windows' own WinUSB. What it would not reach: a phone whose driver is not WinUSB (an older maker's driver of its own), which Google's adb reaches through its second library (`AdbWinApi.dll`). |
| The WinUSB route connects but a step fails where Google's adb passed | That step's part of the protocol here is wrong or missing; its words, and `--verbose`, say where, and it is a fix in `protocol.rs`. |
| The WinUSB route cannot open the interface that Google's adb opens | Something in how Google's adb opens it is missing here (its words say which call refused it); until found, Google's adb is the only way on such a PC. |
| On the clean PC, `drivers` shows Windows' own WinUSB bound by itself (service `WinUSB`, package `winusb.inf` from Microsoft) with the Android class offered | A family needs no driver installed, and the WinUSB route reaches the phone as it is. |
| On the clean PC, `drivers` shows no working driver | Every route needs a phone maker's driver on a family's PC, installed by the guardian (LiMo installs no driver): the console's words must send them to it. Samsung's driver package binds WinUSB, which the WinUSB route uses. |
| libusb or nusb reach the phone on the clean PC (under Windows' own `usbccgp`) | adb_client could serve such PCs, but not this one: a route that depends on which composite driver Windows chose. |
| Only Google's adb reaches the phone, on both PCs | Shipping Google's adb is the way, under the licence reading below, with its server stopped after every use. |
| A step fails on every route (`content write`, `reverse`, `am start` or `adb_enabled`) | That step must change whatever the route: a door taking the offer some other way (a broadcast, which puts the offer in a command line), pairing over the home network instead of the cable, or the guardian switching USB debugging off by hand. |
| The phone asks a question at install (Play Protect, or Samsung's own) | The console's words must prepare the guardian for it. |
| "Always allow" does not hold for this program's key | The console must expect the prompt at every Add a phone. |

## Learned while building it

From the first run on the phone:

- **libusb refused an interface under Samsung's composite driver.** libusb
  1.0.27's source names `dg_ssudbus` beside Windows' own `usbccgp`, but with
  the interface on WinUSB under it, opening it failed with "Operation not
  supported or unimplemented on this platform". nusb, which names only
  `usbccgp`, refused it too. Google's adb, which finds the interface by the
  Android USB interface class and opens its path with WinUSB itself, reached
  it: that is what the WinUSB route does.
- **adb's streams have no half close.** The test app ended its sending side
  (`shutdownOutput`) to say it was done, as one would on a plain socket; adb
  then ended the whole stream, and the bytes the PC was still sending back
  were lost (`cut_short`, though the PC had sent them all). LiMo's link
  through `reverse` must never end its sending side early: it closes only
  when it is done both ways.
- **`am start` on an open screen** hands the intent to it ("intent has been
  delivered to currently running top-most instance"); an app that acts only
  when it starts would not act. `am start -S` stops the app first.

Read in the source of adb_client 4.0.0, nusb 0.2.9 and libusb 1.0.27:

- **adb_client keeps its transport to itself.** `ADBMessageTransport` and
  `ADBMessageDevice` are in private modules and not exported, so no other
  crate can give adb_client a USB layer of its own. That is why this proof
  carries nusb's bytes through a socket to adb_client's TCP device
  (`pc\src\route_nusb.rs`).
- **adb_client keeps no key.** Given a key file that does not exist, it makes
  a new random key in memory and keeps it nowhere, so the phone would ask
  again at every connection. This program makes its key and keeps it
  (`pc\src\key.rs`); the fingerprint the phone shows is the MD5 of the key in
  Android's own format.
- **10 seconds for the guardian's tap.** Once adb_client has shown the phone
  its key, it waits 10 seconds for the answer, then gives up; and its very
  first read has no time limit at all. This program puts a limit on every
  call from outside (a thread and a channel); its own adb waits 45 seconds.
- **No exit code, no error stream.** adb_client's shell commands use adb's
  first shell protocol: no exit code comes back, and errors arrive mixed in
  the output or not at all. Google's adb and this program's own, with the
  second protocol, carry both.
- **Standard input ends the stream.** In adb_client a command with input goes
  as `exec:` (as `adb exec-in` does): the end of the input is the end of the
  whole stream, so whatever the command prints after it is lost. With the
  second shell protocol, standard input is closed by a packet of its own and
  the answer still comes.
- **Install sends no `-r`** in adb_client (`cmd package install -S SIZE`).
- **Reverse lives in the program.** On a phone reached directly, adb_client
  and this program's own adb relay each connection of a reverse themselves,
  for as long as the program holds the phone; nothing is left on the PC after.
- **Reading in exact lengths.** adb's host side (adb_client's libusb
  transport too) reads each message's header and data in transfers of
  exactly their length, so a phone need not end data that fills its last
  packet with a zero length packet. A USB layer that reads in larger
  transfers would wait on the phone for more, and adb's flow control would
  stall; the nusb relay and the WinUSB route read as adb does
  (`protocol::next_message`).
- **One holder at a time.** WinUSB lets one program hold the phone's
  debugging interface. While Google's adb server runs (Android Studio starts
  one), no Rust route can open the phone, and the other way round.
- **Who reaches what on Windows.** libusb: an interface driven by WinUSB,
  libusbK or libusb0 under Windows' own `usbccgp` (and, by its source, under
  Samsung's `dg_ssudbus`, which the run disproved). nusb: only WinUSB under
  `usbccgp`, with a device interface GUID in the registry, or a whole device
  on WinUSB. Google's adb: an interface of the Android USB interface class,
  through WinUSB or a maker's driver of its own (as adb's Windows client and
  an e/OS change to adbd describe; not read in the source for this proof). The
  WinUSB route here: an interface of that class on WinUSB, whatever is above
  it. `drivers` applies these rules to what Windows bound.
- **Two things for later.** adb_client's TCP device, used for wireless
  debugging only, writes its TLS session keys to the file named by
  `SSLKEYLOGFILE` when that variable is set (rustls' `KeyLogFile`); over USB,
  and through this proof's relay, no TLS is used. And adb_client signs with
  the `rsa` crate 0.9, which carries the RustSec advisory RUSTSEC-2023-0071
  (a timing side channel in private key operations); `rsa` 0.10 is out. The
  WinUSB route signs with the same `rsa` 0.9, already in this program.

## Licences

Facts for the owner, not legal advice. Nothing here is GPL or AGPL; the one
LGPL part is libusb, which the WinUSB route does not use.

**The WinUSB route** adds no crate: its adb is this proof's own code (Apache
License 2.0, as all of LiMo), written from AOSP's own descriptions of the
protocol with no code copied; WinUSB and SetupAPI are Windows' own, reached
through `windows-sys` (already in the program); its key and signature use
`rsa` (already in the program, under adb_client). For its tests only, the
`sha1` crate (Apache 2.0 or MIT, already in the build under adb_client)
checks its signature against adb_client's.

**adb_client 4.0.0** is MIT. Its `usb` feature brings **rusb 0.9.4** (MIT)
and **libusb1-sys 0.7.0**, whose own crate is MIT but which carries the C
source of **libusb 1.0.27**, under the GNU Lesser General Public License,
version 2.1 or any later version (libusb's own files say so). adb_client's
`usb` feature also turns on rusb's `vendored` feature, so libusb is compiled
from that source and linked into the program itself: the built program holds
libusb and needs no libusb DLL. A crate that uses adb_client cannot switch
this off (Cargo's features only add); shipping libusb as a separate DLL would
need adb_client changed, which its MIT licence allows.

What LGPL 2.1 asks of a program with the library linked into it (section 6):
that it comes with the library's complete source, and with the program as
source or object code, so that a person can change the library and link the
program again (or one of the other ways section 6 lists, such as a shared
library, or a written offer of the source); that each copy says plainly that
the library is used in it and is under the LGPL, with a copy of the licence;
and that the terms allow changing the program for one's own use, and
reverse engineering to debug such changes. LiMo's own source is public, under
the Apache License 2.0.

**nusb 0.2.9** is Apache 2.0 or MIT, at the user's choice: Rust alone, no C,
nothing LGPL.

**Google's adb** (`adb.exe`, `AdbWinApi.dll`, `AdbWinUsbApi.dll`, in the SDK
platform tools) comes under the Android Software Development Kit License
Agreement. Its section 3.4: "Except to the extent required by applicable third
party licenses, you may not copy (except for backup purposes), modify, adapt,
redistribute, decompile, reverse engineer, disassemble, or create derivative
works of the SDK or any part of the SDK." Its section 3.5: "Use, reproduction
and distribution of components of the SDK licensed under an open source
software license are governed solely by the terms of that open source software
license and not the License Agreement." adb's source is in the Android Open
Source Project under the Apache License 2.0; whether the prebuilt adb.exe
counts as such a component the agreement does not say. scrcpy, an Apache 2.0
project, ships these three files in its Windows releases (not checked again
for this proof). Google's USB driver for Windows is a separate download, also
from the SDK; LiMo installs no driver.

**This program's crates**, as built for Windows (`cargo tree`; the build only
macros left out):

| Licence | Crates |
| --- | --- |
| MIT | adb_client 4.0.0, rusb 0.9.4, libusb1-sys 0.7.0 (with libusb 1.0.27 inside, LGPL 2.1 or later), generic-array, libm, pem, quick-protobuf, slab, spin |
| Apache 2.0 or MIT | nusb 0.2.9, rsa 0.9.10, md-5 0.10.6, sha2 0.10.9, sha1, digest, log, windows-sys 0.61.2, zeroize, rand, getrandom, chrono, regex, rcgen, base64, num-bigint-dig and the num crates, the pkcs8, pkcs1, der, spki and const-oid crates, time, cfg-if, libc and the rest of the small ones |
| Apache 2.0 or ISC or MIT | rustls 0.23.46 |
| Apache 2.0 and ISC | ring 0.17.14 |
| ISC | rustls-webpki, untrusted |
| BSD 3-Clause | subtle |
| MIT or Zlib or Apache 2.0 | miniz_oxide (reading the APK's manifest) |
| Unlicense or MIT | byteorder, memchr, aho-corasick |
| 0BSD or MIT or Apache 2.0 | adler2 |

New for LiMo here, beside what the devtool already uses: adb_client, rusb,
libusb1-sys and libusb, nusb, rsa (already under adb_client), md-5 and sha2.
A program that kept only the WinUSB route would need of these only `rsa`,
`md-5` and `sha2`, beside `windows-sys`. `apk.rs` is copied from LiMo's phone
development tool.

**The test app:** the Kotlin standard library 2.1.20 (Apache 2.0) in the APK;
JUnit 4.13.2 (EPL 1.0) and Hamcrest 1.3 (BSD 3-Clause) for its tests only;
Gradle 8.14.3, the Android Gradle Plugin 8.11.0 and the Kotlin compiler
(Apache 2.0) to build it.

## Tests

None of them reaches a phone or a USB device.

`cargo test` in `pc\`:

| Where | What | Count |
| --- | --- | --- |
| `src\*.rs` | every command as it crosses the cable; answers read the same with carriage returns; the door's row read whole or not at all; serials cut to four; base32 and base64 as RFC 4648 says; the sample shaped like an offer, its check, its `Debug` hiding it; the key made once and kept, refused when damaged, its public half as Android reads it (n0inv, R squared, the exponent); Google's adb's key read in either format, never written, its place never said, a missing or damaged one said; `--key` and its refusals; the echo server sending back 300 KB, staying until the phone has read everything back and closed, saying a phone that never closes, a port in use said; a call that does not end given up; the command line and its refusals; `adb devices` and adb's answers read; adb's message header (its sum and check), shell packets cut across messages, reading from the phone in transfers that end where each message does, a zero length packet passed over, a broken message refused; the relay's framing; the driver rules for WinUSB, a maker's driver, no driver, a whole device and the test phone as Windows listed it, for each route, and their words; the WinUSB cable's answers; serials cut in a log line | 35 |
| `tests\protocol\` | this program's own adb against a fake phone that speaks adb as AOSP's adbd does, over a wire with USB's shape: the signature the same as adb_client's; a first connection showing the key, the phone reading it whole, and the next needing no question; a key not allowed in time said; a shell command's output, errors and exit code apart; standard input arriving whole and closed after; install streamed with `-r`, never more than the phone takes in a message; `reverse` carrying the app's 256 KiB to an echo server here and back whole, the app closing first, and refused once removed; a service the phone does not have refused; a phone that goes said as lost; the steps (`connect`, `uninstall`, `debugging-off`, `start`) run on the WinUSB route's real cable against it; with `--key adb`, a phone that allowed that key asking nothing and the output saying the question was skipped, the key file left exactly as it was, never copied, its place never said, and a phone that does not know it asking | 11 |
| `tests\stories\` | the steps against a fake phone with a real loopback client, by all four routes: each says what it does before anything reaches the phone and lets go at the end; `connect` reads four facts and nothing else, and says each route's wait; opening problems said, nothing sent; `install` refusing a missing file, other packages and a debug build before the phone, then installing, and Android's refusal explained; the secret crossing once on standard input and never shown, also with carriage returns; changed bytes and a missing app said; `reverse` starting the app afresh, carrying 256 KiB and removed after, and removed after a run with no connection; `start` and `uninstall`; `debugging-off` refused unless OFF is typed, warning first, and nothing reaching the phone after; what `plan` and `help` print | 13 |
| `tests\house_rules.rs` | unsafe code only in `src\win\`, each block and each `unsafe impl` saying why; no allow attributes; every command touching only the test app and one setting; the routes calling only what the steps need, the WinUSB route removing only the test app, and this program's own adb opening only its five kinds of service; no serial printed unmasked, and no key's place printed; no dash punctuation; no path or name of the build PC | 6 |

Three more are ignored by default, run with `--ignored`:
`the_built_test_app` reads the built APK (the test app, a release build);
`this_pcs_drivers` and `this_pcs_android_interfaces` read this PC's own lists
and print only how many there are and which facts could be read, opening
nothing. Clippy (`cargo clippy --all-targets`, warnings as errors) and `cargo
doc --no-deps` are clean.

`.\gradlew.bat testDebugUnitTest` in `phone\`: 8 JVM tests: the door's rules
(an offer shaped text taken with its line feed, the check the same as the
PC's, each refusal, a carriage return changing the check), the status row in
the PC's order with no secret in it, the loopback client against an echo
server, a changed byte and no server, a PC that ends its side first said as
`cut_short`, and the last run kept.

## Layout

```
spikes/07-cable/
  README.md, .gitignore
  pc/                    limo-cable-spike.exe
    Cargo.toml, Cargo.lock, .cargo/config.toml
    src/
      main.rs, lib.rs    the program; the real routes; --verbose's log
      cli.rs             the command line
      plan.rs            what each step does, said first; `plan`; `help`
      steps.rs           the steps
      phone.rs           every command to the phone, and its answers
      cable.rs           the routes' common shape, time limits, problems
      protocol.rs        this program's own adb
      route_winusb.rs    this program's own adb over WinUSB
      route_rust.rs      adb_client's device, every call with a limit
      route_libusb.rs    adb_client over libusb
      route_nusb.rs      adb_client over nusb, through the relay
      route_adb.rs       Google's adb.exe
      key.rs             this program's adb key and its fingerprint
      secret.rs          the sample secret and its check
      echo.rs            the echo server for reverse
      apk.rs             what an APK says it is (from the devtool)
      drivers.rs         the driver report and each route's rules
      ui.rs              the terminal
      win/               the only unsafe code
        drivers.rs       Windows' lists, read only
        winusb.rs        the Android class interfaces, and WinUSB
    tests/
      house_rules.rs
      protocol/          main.rs, fake_adbd.rs
      stories/           main.rs, fake.rs, apk.rs
  phone/                 the test app, LiMo spike 07
    settings.gradle.kts, build.gradle.kts, gradle.properties, gradlew, gradlew.bat, gradle/
    app/build.gradle.kts the throwaway key and the release signing
    app/src/main/        AndroidManifest.xml, res/values/strings.xml
      java/ai/lucidloom/limo/spike07/
        MainActivity.kt  the screen, and the loopback run on its extra
        CableDoor.kt     the guarded door
        Door.kt          the door's rules and its status row
        Loopback.kt      the loopback client
        SpikeState.kt    what the app has seen, in memory
    app/src/test/        DoorTest.kt, LoopbackTest.kt
```
