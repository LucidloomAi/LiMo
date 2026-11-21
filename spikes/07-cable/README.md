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

Three routes are compared, step by step:

| Route | `--via` | What it is |
| --- | --- | --- |
| adb_client over libusb | `libusb` | adb's protocol in Rust (the `adb_client` crate, 4.0.0) inside this program, straight to the phone over USB through its own USB layer (rusb and libusb, compiled in). No adb.exe, no adb server. |
| adb_client over nusb | `nusb` | The same adb_client, its bytes carried to the phone by nusb 0.2.9, a USB layer in Rust alone. No adb.exe, no adb server, no C. |
| Google's adb | `adb` | Google's own adb.exe, run as a separate program, the way LiMo's phone development tool runs it. |

The owner chooses between speaking adb from Rust and shipping Google's adb
once the run is done; this proof gives the facts for that choice, not a
verdict.

## Contents

- [What it proves, and what it does not](#what-it-proves-and-what-it-does-not)
- [The two parts](#the-two-parts)
- [What the program never does](#what-the-program-never-does)
- [Building](#building)
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

Once the run is done, it shows, for each route:

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
- Every command it can send is in one place (`pc\src\phone.rs`), and a test
  reads each of them.
- The sample secret is random, made fresh in each run, never printed, never
  kept, never in any argument, and wiped from memory after.
- It never shows a serial number in full and writes it to no file: on screen
  only its last four characters (`...1A2B`), in the log of `--verbose` too.
- It installs nothing on the PC, needs no administrator rights, and writes
  one file: its own adb key, `%LOCALAPPDATA%\LiMo-spike07-TEST-ONLY\adbkey`.
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
libusb, so it runs on a Windows 11 PC with nothing installed.

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
Scheme v2 only, by that one certificate. Deleting the folder makes a new key
at the next build; a test app signed with the old one must then be removed
from the phone first, or Android refuses the install
(`INSTALL_FAILED_UPDATE_INCOMPATIBLE`).

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
256 KiB (a pattern, nothing secret), stops sending, and checks that exactly
those bytes come back, reading while it writes.

**The screen** says what the app is, the phone's model and Android version,
whether USB debugging is on as the phone's setting reads now (any app can read
it), what the door last took, and how the last loopback run went.

**Its log**, tag `LiMoSpike07`: `cable secret taken bytes=N check=C uid=U`,
`cable secret refused why=W uid=U`, `loopback start port=P`, `loopback ok|failed
port=P bytes=N ms=M why=W`. Never a secret.

## The run at the PC, with the phone

About 45 minutes, at the PC that builds LiMo, with the test phone. Everything
is typed in PowerShell, in this folder (`LiMo\spikes\07-cable`), after:

```
$s = ".\pc\target\release\limo-cable-spike.exe"
```

### Before you start

1. Build both parts (above).
2. Close Android Studio and stop Google's adb server: `adb kill-server`. While
   it runs it holds the phone's debugging interface, and neither Rust route
   can open it (the program says so). For the same reason, do not leave `adb
   logcat` running in another window: the program's own output and the test
   app's screen say all that is needed.
3. On the phone: unlock it; Settings, Display, Screen timeout: 10 minutes;
   USB debugging on (Settings, Developer options). Plug it in with the cable
   that came with it, into a port on the PC itself.
4. Read the whole run once: `& $s plan`.

### Steps

Write what you see in [Results](#results). If a step fails, its own words say
why; write them down, and go on with the next step that does not need it.

1. `& $s drivers`: Windows' lists only, nothing reaches the phone. Write
   down every line under "Debugging interface 1", the three "What each route
   can use" lines among them.
2. `& $s list`: what libusb and nusb each see. Nothing reaches the phone.
3. `& $s connect --via libusb`: it prints this program's key's fingerprint.
   The phone asks "Allow USB debugging?": check that the fingerprint it shows
   is the same, tick "Always allow from this computer", and tap Allow within
   10 seconds (adb_client waits no longer; if the time passes, tap Allow
   anyway and run the step again). Pass: `Connected: a SM-G988B, Android 13
   (API 33); USB debugging setting: 1; the test app: not installed.` Write
   down whether the prompt showed, whether the fingerprints matched, and the
   seconds it took to connect.
4. `& $s connect --via libusb` again: no prompt now ("Always allow" held).
5. `& $s connect --via nusb`: the same key, so no prompt. If nusb cannot use
   the driver, it says so in nusb's own words: write them down, and skip the
   other nusb steps.
6. By libusb, in order:
   - `& $s install --via libusb`. Pass: `Android said: Success` and `The phone
     lists the test app now.` Watch the phone: write down any question it asks
     (Play Protect, or Samsung's own), its exact words, and what you tapped
     (for this test app, installing anyway is fine).
   - `& $s install --via libusb` once more: adb_client's install sends no
     `-r`, so this shows whether a second install replaces the app.
   - `& $s write-secret --via libusb`. Pass: `The test app took exactly what
     this PC sent: every byte, none changed on the way.` and `written by user
     id 2000`.
   - `& $s start --via libusb`. Pass: the test app's screen opens on the phone,
     and shows the door's last write.
   - `& $s reverse --via libusb`. Pass: `The phone's connection came through
     the cable: 262144 bytes came and went back`, `The test app says: every
     byte came back`, then `Removed`. Write down the milliseconds.
   - `& $s uninstall --via libusb`. Pass: `The test app is gone from the
     phone.`
7. By nusb, the same six (two installs), with `--via nusb`.
8. By Google's adb, the same six, with `--via adb` (adb.exe is found on the
   PATH or in `ANDROID_HOME`; `--adb PATH` names it otherwise). Each ends with
   `adb's server is stopped: nothing listens on 127.0.0.1:5037.` The phone
   most likely asks nothing at `connect`: adb's own key was allowed on this PC
   before. Write down whether `write-secret` printed an exit code.
9. If anything failed by a Rust route, run that step once more with
   `--verbose` and keep what adb_client and nusb printed (any serial number in
   it is cut to its last four characters).
10. The last check: `& $s install --via libusb` (by whichever route worked),
    then `& $s debugging-off --via libusb`. It says first that the cable link
    ends there, and asks you to type OFF. Pass: `The link ended as the setting
    changed, as expected.` (or `The phone answered before the link ended.`) and
    `USB debugging looks off`. Then, by hand on the phone: Settings, Developer
    options: is USB debugging off? Open "LiMo spike 07": does it say `USB
    debugging: off`? Then remove the test app by hand (press and hold its icon,
    Uninstall).

After this nothing reaches the phone over the cable until a person switches
USB debugging on again.

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
5. `.\limo-cable-spike.exe connect --via libusb`. The phone asks "Allow USB
   debugging?" for that PC: leave "Always allow" unticked, so that the phone
   keeps nothing of that PC, and tap Allow within 10 seconds.
6. `.\limo-cable-spike.exe connect --via nusb`: the phone asks again (it kept
   nothing); Allow, unticked.
7. After five minutes with the phone still in: `.\limo-cable-spike.exe drivers`
   again (Windows may fetch a driver by itself in the meantime).
8. Delete the folder `%LOCALAPPDATA%\LiMo-spike07-TEST-ONLY` and the program
   from that PC, and switch USB debugging off on the phone by hand.

## Results

Leave a row empty when its step was skipped, and say why under Notes.

### The phone and the PCs

| Item | Seen |
| --- | --- |
| Phone: model, One UI, Android, build number | |
| This PC: Windows version; drivers it had for the phone before | |
| The clean PC: Windows version | |
| Date of the run | |

### At this PC

| Step | Route | Look for | Seen |
| --- | --- | --- | --- |
| drivers | | the interface's driver (service), its package and provider, the device above and its driver, the Android class, the WinUSB GUIDs | |
| drivers | | what each route can use, by its rules | |
| list | libusb, nusb | what each sees | |
| connect | libusb | prompt shown; fingerprints the same; seconds to connect; the `Connected` line | |
| connect again | libusb | no prompt | |
| connect | nusb | connected, or nusb's words | |
| connect | adb | connected; prompt or not | |
| install | libusb | `Success`; any question on the phone, its words | |
| install again | libusb | replaced, or Android's words | |
| write-secret | libusb | the same bytes and check; user id | |
| start | libusb | the screen opened | |
| reverse | libusb | bytes and milliseconds; the app's verdict; removed | |
| uninstall | libusb | gone | |
| install | nusb | as above | |
| install again | nusb | as above | |
| write-secret | nusb | as above | |
| start | nusb | as above | |
| reverse | nusb | as above | |
| uninstall | nusb | as above | |
| install | adb | as above | |
| install again | adb | as above (adb sends `-r`) | |
| write-secret | adb | as above; the exit code printed | |
| start | adb | as above | |
| reverse | adb | as above | |
| uninstall | adb | as above | |
| every adb step | adb | `adb's server is stopped` | |
| debugging-off | | the answer line; `USB debugging looks off` | |
| by hand | | Developer options shows USB debugging off; the test app says off | |

### At the PC that never had Android tools

| Step | Route | Look for | Seen |
| --- | --- | --- | --- |
| drivers, first minute | | as above | |
| a notice from Windows | | its words | |
| list | libusb, nusb | what each sees | |
| connect | libusb | prompt; connected, or the words | |
| connect | nusb | prompt; connected, or the words | |
| drivers, after five minutes | | anything changed | |

### Notes

Anything else: steps that needed a second try, questions the phone asked,
anything that surprised you.

## What each outcome would mean

Each is a fact about the choice; which way to go is the owner's.

| If | Then |
| --- | --- |
| libusb reaches the phone on both PCs, every step passing | adb_client could be LiMo's way to the phone as it is, with no program of Google's shipped. libusb is then compiled into LiMo's program (see [Licences](#licences) for what its licence asks). |
| nusb reaches it on both PCs | A route with no C code and no LGPL code exists, but not with adb_client 4.0.0 as it is: its transport trait is private, so LiMo would need adb_client changed (a fork or a change upstream, which its MIT licence allows) to give it a nusb transport. This proof's relay through a loopback socket is not a way for the product. |
| On the clean PC, `drivers` shows Windows' own WinUSB bound by itself (service `WinUSB`, package `winusb.inf` from Microsoft) | A family needs no driver installed. Which Rust route reaches it follows from the device above (`usbccgp`) and the GUIDs in the report. |
| On the clean PC, `drivers` shows no working driver | Every route needs a phone maker's driver on a family's PC, installed by the guardian (LiMo installs no driver): the console's words must send them to it. |
| The Rust routes fail only where a maker's own driver is bound (on this PC, say, Samsung's), while Google's adb works | Either Google's adb.exe ships with LiMo (the licence reading below), or guardians on PCs with that driver are asked to do something about it, or those PCs are not supported. |
| Only Google's adb reaches the phone, on both PCs | The Rust route is out for this phone: shipping Google's adb is the way, under the licence reading below, with its server stopped after every use. |
| A step fails on every route (`content write`, `reverse`, `am start` or `adb_enabled`) | That step must change whatever the route: a door taking the offer some other way (a broadcast, which puts the offer in a command line), pairing over the home network instead of the cable, or the guardian switching USB debugging off by hand. |
| The phone asks a question at install (Play Protect, or Samsung's own) | The console's words must prepare the guardian for it. |
| The bytes through standard input arrive changed | The door must take the offer in a form that survives it, or the route must change. |
| "Always allow" does not hold for the Rust key | The console must expect the prompt at every Add a phone. |

## Learned while building it

Read in the source of adb_client 4.0.0, nusb 0.2.9 and libusb 1.0.27 for this
proof, and true whatever the run shows:

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
  call from outside (a thread and a channel).
- **No exit code, no error stream.** adb_client's shell commands use adb's
  first shell protocol: no exit code comes back, and errors arrive mixed in
  the output or not at all. Google's adb, with the second protocol, carries
  both.
- **Standard input ends the stream.** A command with input goes as `exec:`
  (as `adb exec-in` does): the end of the input is the end of the whole
  stream, so whatever the command prints after it is lost. The door's answer
  is therefore read by a second command.
- **Install sends no `-r`.** adb_client streams the APK to `cmd package
  install -S SIZE`; the second install in the run shows what Android does
  with that.
- **Reverse lives in the program.** On a phone reached directly, adb_client
  relays each connection of a reverse itself, for as long as the program holds
  the phone; nothing is left on the PC after.
- **Reading in exact lengths.** adb's host side (adb_client's libusb
  transport too) reads each message's header and data in transfers of
  exactly their length, so a phone need not end data that fills its last
  packet with a zero length packet. A USB layer that reads in larger
  transfers would wait on the phone for more, and adb's flow control would
  stall; this proof's nusb relay reads as adb does.
- **One holder at a time.** WinUSB lets one program hold the phone's
  debugging interface. While Google's adb server runs (Android Studio starts
  one), no Rust route can open the phone, and the other way round.
- **libusb on Windows** reaches an interface driven by WinUSB, libusbK or
  libusb0, under Windows' own composite driver (`usbccgp`) or Samsung's
  (`dg_ssudbus`), which libusb 1.0.27 names. **nusb 0.2.9** reaches only
  WinUSB, under `usbccgp`, with a device interface GUID in the registry, or a
  whole device on WinUSB. **Google's adb** finds a phone by the Android USB
  interface class, `{F72FE0D4-CBCB-407D-8814-9ED673D0DD6B}`, which Google's and
  the makers' drivers offer, and which an adbd sending Microsoft's OS
  descriptors asks WinUSB to offer (as adb's Windows client and an e/OS change
  to adbd describe; not read in the source for this proof). `drivers` applies
  these rules to what Windows bound.
- **Two things for later.** adb_client's TCP device, used for wireless
  debugging only, writes its TLS session keys to the file named by
  `SSLKEYLOGFILE` when that variable is set (rustls' `KeyLogFile`); over USB,
  and through this proof's relay, no TLS is used. And adb_client signs with
  the `rsa` crate 0.9, which carries the RustSec advisory RUSTSEC-2023-0071
  (a timing side channel in private key operations); `rsa` 0.10 is out.

## Licences

Facts for the owner, not legal advice. Nothing here is GPL or AGPL; the one
LGPL part is libusb.

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
`apk.rs` is copied from LiMo's phone development tool.

**The test app:** the Kotlin standard library 2.1.20 (Apache 2.0) in the APK;
JUnit 4.13.2 (EPL 1.0) and Hamcrest 1.3 (BSD 3-Clause) for its tests only;
Gradle 8.14.3, the Android Gradle Plugin 8.11.0 and the Kotlin compiler
(Apache 2.0) to build it.

This proof's own code is under the Apache License 2.0, as is all of LiMo.

## Tests

None of them reaches a phone or a USB device.

`cargo test` in `pc\`:

| Where | What | Count |
| --- | --- | --- |
| `src\*.rs` | every command as it crosses the cable; answers read the same with carriage returns; the door's row read whole or not at all; serials cut to four; base32 as RFC 4648 says; the sample shaped like an offer, its check, its `Debug` hiding it; the key made once and kept, refused when damaged, its public half as Android reads it (n0inv, R squared, the exponent); the echo server sending back 300 KB and stopping, a port in use said; a call that does not end given up; the command line and its refusals; `adb devices` and adb's answers read; the relay's framing both ways, reading from the phone in transfers that end where each message does, a zero length packet passed over, a broken message stopping it; the driver rules for WinUSB, a maker's driver, no driver and a whole device, and their words; serials cut in a log line | 27 |
| `tests\stories\` | the steps against a fake phone with a real loopback client: each says what it does before anything reaches the phone and lets go at the end; `connect` reads four facts and nothing else; opening problems said, nothing sent; `install` refusing a missing file, other packages and a debug build before the phone, then installing, and Android's refusal explained; the secret crossing once on standard input and never shown, also with carriage returns; changed bytes and a missing app said; `reverse` carrying 256 KiB and removed after, and removed after a run with no connection; `start` and `uninstall`; `debugging-off` refused unless OFF is typed, warning first, and nothing reaching the phone after; what `plan` and `help` print | 13 |
| `tests\house_rules.rs` | unsafe code only in `src\win\`, each block saying why; no allow attributes; every command touching only the test app and one setting; the routes calling only what the steps need; no serial printed unmasked; no dash punctuation; no path or name of the build PC | 6 |

Two more are ignored by default, run with `--ignored`:
`the_built_test_app` reads the built APK (the test app, a release build), and
`this_pcs_drivers` reads this PC's own lists and prints only how many of each
there are and which facts could be read. Clippy (`cargo clippy --all-targets`, warnings as errors) and
`cargo doc --no-deps` are clean.

`.\gradlew.bat testDebugUnitTest` in `phone\`: 7 JVM tests: the door's rules
(an offer shaped text taken with its line feed, the check the same as the
PC's, each refusal, a carriage return changing the check), the status row in
the PC's order with no secret in it, the loopback client against an echo
server, a changed byte and no server, and the last run kept.

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
      win/drivers.rs     Windows' lists, read only: the only unsafe code
    tests/
      house_rules.rs
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
