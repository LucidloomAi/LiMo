# limo-cable

The PC's side of the USB cable to a child's phone. One Rust library that
holds what Windows sees on the PC's USB ports, LiMo's own way to the phone
over the cable, every call LiMo makes on a phone there, and a small state
machine that says, at each moment, what is plugged in, what to tell the
person, and what can be done next. The console uses it to add a phone; the
phone development tool uses its look at the USB ports and its APK reader.

**What it is not.** It has no screen and no words of the console's own
beyond what a guardian reads for each state of the cable; it does not pair a
phone (the link does that, over the port this crate reverses, between the
service and the phone); it holds no family's data; it installs no driver
and no program on the PC; and it is not the phone's side of the cable (the
child form's door and its start, which are the phone's own).

**What it never does on a phone.** It installs, hands an offer to, and
starts LiMo's child form, and nothing else; it removes no app, not even a
LiMo signed with another key; it reads a few properties and one setting, and
writes one setting, USB debugging, only to switch it off, only when asked by
its own call. Every command is in one place (`src/commands.rs`), and a test
reads each one.

## Contents

- [Why LiMo's own way](#why-limos-own-way)
- [How the console calls it](#how-the-console-calls-it)
- [What is plugged in, and the cases](#what-is-plugged-in-and-the-cases)
- [Opening the phone through WinUSB](#opening-the-phone-through-winusb)
- [LiMo's own adb](#limos-own-adb)
- [The key](#the-key)
- [Every call on the phone](#every-call-on-the-phone)
- [Never show the key unasked](#never-show-the-key-unasked)
- [The state machine](#the-state-machine)
- [What the others must do](#what-the-others-must-do)
- [The fake phone](#the-fake-phone)
- [The example](#the-example)
- [What real hardware has shown, and what it has not](#what-real-hardware-has-shown-and-what-it-has-not)
- [Tests](#tests)
- [Layout](#layout)
- [Dependencies and licences](#dependencies-and-licences)

## Why LiMo's own way

LiMo's own proof (`spikes/07-cable`) compared, on 21 November 2026, four
ways to the test phone (a Galaxy S20 Ultra on Android 13, One UI 5.1) on the
PC that builds LiMo, where Windows had bound WinUSB to the phone's debugging
interface from Samsung's driver package, under Samsung's own composite
driver (`dg_ssudbus`):

- the `adb_client` crate over libusb failed before the phone asked
  anything: "USB Error: Operation not supported or unimplemented on this
  platform";
- the same crate over nusb failed: "incompatible driver is installed for
  this device";
- Google's adb.exe reached the phone and passed every step;
- LiMo's own adb over Windows' own WinUSB, opening the phone as Google's adb
  does, passed every step: connect (the question on the phone, and none the
  next time), install twice, an offer through a guarded door, start,
  `reverse` with every byte back, and USB debugging off.

So this crate carries the last way only, which needs no program of
Google's, no C library and no USB layer but Windows' own. The libusb and
nusb ways are not carried over: with the driver many PCs get from Windows
Update for a Samsung phone, neither opens the phone, and both bring more to
own (libusb is C, under the LGPL, compiled into the program). Google's adb
stays in the phone development tool only, as before.

## How the console calls it

```
let mut cable = WindowsCable;                       // the PC's side
let key = AdbKey::for_this_account()?;              // LiMo's own key, kept
let mut state = State::NotReady(Case::NoPhone);
loop {
    match next(&state)[0] {
        Next::LookAgain => state = step(state, Event::Looked(cable.look())),   // once a second
        Next::Connect => {                          // quietly: the key is never shown here
            state = step(state, Event::Connecting);
            match cable.open(&key, waits, Knock::Quiet) {
                Ok(phone) => state = step(state, Event::Connected(phone.facts().clone())),
                // OpenProblem::NotKnown: the phone does not know this PC; the
                // state is Unknown, and nothing more is tried while it stays
                Err(problem) => state = step(state, Event::OpenFailed(problem)),
            }
        }
        Next::Ask => {                              // only on the guardian's tap, once
            state = step(state, Event::Connecting);
            // on_prompt: the phone shows "Allow USB debugging?" now; give_up
            // ends the wait (the phone gone, Add a phone closed)
            match cable.open(&key, waits, Knock::Asking { on_prompt: &mut || tell_the_screen_asking(), give_up: &|| closed() }) {
                Ok(phone) => state = step(state, Event::Connected(phone.facts().clone())),
                Err(problem) => state = step(state, Event::OpenFailed(problem)),
            }
        }
        Next::CheckLimo => ... phone.limo() ...           Event::LimoChecked
        Next::InstallLimo => ... phone.install_limo(apk)  Event::Installed
        Next::HandOffer => ... phone.hand_offer(offer)    Event::OfferHanded
        Next::ReverseLink => ... phone.reverse_link()     Event::Reversed
        Next::StartLimo => ... phone.start_limo()         Event::Started
        Next::SwitchDebuggingOff => ... phone.debugging_off()  Event::DebuggingOff
        Next::Unplug => break,
    }
    show(tell(&state));                             // the line, and what to do
}
```

`tests/flow.rs` runs this loop, whole, against the fake phone. `Event::Lost`
goes in whenever a call says the link is gone; a look while connected that
shows the phone gone does the same.

## What is plugged in, and the cases

[`Cable::look`](src/open.rs) reads two of Windows' lists, both read only:

- **Windows' list of USB devices** (`src/usb.rs`, `src/win/usb.rs`, moved
  here from the phone development tool, which uses them still): the devices
  that look like a phone, by what they are and never by a maker's list (a
  portable device on the USB bus itself, or a debugging interface, whose
  compatible id is `USB\Class_FF&SubClass_42&Prot_01`), each with its name,
  its bus and whether it works. No serial number, no instance id.
- **SetupAPI's list of the interfaces that offer the Android USB interface
  class** (`src/win/winusb.rs`), the class by which Google's adb finds a
  phone on Windows: each one's name, its driver, and the driver of the
  device above it. Its path, which holds the phone's serial number, is kept
  in memory to open it and never shown.

[`flow::seen`](src/flow.rs) makes a state of them: one interface on WinUSB is
a phone LiMo can open; two are two phones; none is a case of
[`usb::decide`](src/usb.rs), which the console's words then say:

| Windows sees | The line | What to do |
| --- | --- | --- |
| nothing that looks like a phone | This PC sees no phone on its USB ports. | The cable may only charge: many cables carry no data. Use the cable that came with the phone, or one known to move files, in a port on the PC itself, not a hub; and unlock the phone. |
| a portable device, no debugging interface | This PC sees what looks like a phone (name), but USB debugging is off on it. | Switch USB debugging on, on the phone: Settings, About phone, tap Build number seven times; then Settings, Developer options, USB debugging. If it is on already, unplug the phone and plug it in again, unlocked. |
| a debugging interface with no working driver | This PC sees the phone's debugging connection (name), but has no driver for it. | Install the phone maker's USB driver on this PC (for a Samsung phone, Samsung's Android USB Driver for Windows), then unplug the phone and plug it in again. LiMo installs no driver. |
| a working debugging interface on a driver that is not WinUSB | This PC sees the phone's debugging connection (name), but its driver is not one LiMo can use. | Install the phone maker's current USB driver on this PC, then unplug the phone and plug it in again. |
| neither list could be read | This PC's list of USB devices could not be read. | Plug the phone in with its own cable, unlock it, and check that USB debugging is on. |

## Opening the phone through WinUSB

[`WindowsCable::open`](src/open.rs), as Google's adb opens a phone on
Windows, with no program of Google's:

1. Google's adb server must not be running (127.0.0.1:5037): while it runs it
   holds the phone. LiMo never stops it (it is someone else's, often Android
   Studio's), and says so.
2. The one interface of the Android USB class on WinUSB, from the list
   above; none, two, or one on another driver are each said.
3. Its path opened with `CreateFileW` (to read and write, overlapped, as
   WinUSB asks), then `WinUsb_Initialize`; its two bulk pipes found with
   `WinUsb_QueryInterfaceSettings` and `WinUsb_QueryPipe`, the interface
   checked to be a debugging interface.
4. Every transfer overlapped, with an event of its own, so that reading and
   writing run side by side; each with a time limit (`PIPE_TRANSFER_TIMEOUT`,
   and a wait of LiMo's own behind it that aborts the pipe and waits for the
   transfer's end, so that no buffer is let go while Windows may still write
   into it); a write that fills its last packet exactly ends with a zero
   length packet (`SHORT_PACKET_TERMINATE`); reads in exact lengths, as adb's
   own host side reads, so that a phone that sends no zero length packet
   after such data is never waited on.

This is the only unsafe code besides the read of Windows' lists, all of it
in `src/win/`, each block with its reason; a test checks it, and which
Windows calls each file makes (the look only reads; nothing anywhere sets,
installs, removes, enables or disables a device).

## LiMo's own adb

[`protocol.rs`](src/protocol.rs), written for LiMo from AOSP's own
descriptions, no code copied, over any [`Wire`](src/protocol.rs) (WinUSB, or
the fake phone's in memory cable):

- **Messages:** a 24 byte header (command, two arguments, the data's length
  and its byte sum, the command with every bit flipped), then the data; on
  USB the header is one transfer and the data the next.
- **Connecting:** `CNXN` with version 0x01000001, at most 1 MiB a message,
  and the features LiMo speaks (`shell_v2`, `cmd`). The phone sends a token;
  LiMo signs it with its own key (PKCS#1 v1.5 with SHA-1's prefix, the token
  standing in for the digest, as adb signs; a test checks it against the
  `sha1` crate's signature). A phone that knows the key answers at once; one
  that does not sends a new token. **A quiet connection stops there,
  showing the phone nothing** (`Knock::Quiet`, `OpenProblem::NotKnown`): only
  an asking one (`Knock::Asking`), made when a person has just asked for it,
  shows the key, once; the caller is told that the phone asks the person
  now, and the person has the prompt's wait to allow it (the console gives
  ten minutes, so that a late Allow still connects), the wait ending early
  when the caller gives it up. See [Never show the key unasked](#never-show-the-key-unasked).
- **Streams:** `OPEN`, `OKAY`, `WRTE` and `CLSE`, each message acknowledged
  before the next. adb's streams have no half close: either side's close
  ends both directions.
- **The shell, with its exit code:** the second shell protocol, with no
  terminal (`shell,v2,raw:`): output, errors and the exit code come back
  apart; standard input is sent and then closed, so a command that reads to
  its end ends.
- **Install:** `exec:cmd package install -r -S SIZE`, the APK streamed,
  Android's answer read to the end of the stream, as Google's adb installs.
  The APK is read once, into memory: the bytes checked are the bytes sent,
  and SIZE is their length, so a file changed or cut during an install can
  neither put another build on the phone nor leave the phone waiting for
  bytes that never come.
- **Reverse:** `reverse:forward:tcp:P;tcp:P` and `reverse:killforward:tcp:P`;
  each connection the phone then opens to the port is relayed to
  127.0.0.1:P on this PC, for as long as the phone is held.

Nothing else: no file sync, no root, no reboot, no other service, no TLS
(only wireless debugging uses it). A test reads every service it opens.

## The key

[`AdbKey`](src/key.rs): RSA 2048, as adb's own, made the first time and
kept in `%LOCALAPPDATA%\LiMo\adbkey` for the guardian's Windows account (the
console runs there; the child's account cannot read that folder). It is
written as PKCS#8, as adb writes its own, and read as PKCS#8 or PKCS#1. Two
callers making it at the same moment share one. A damaged file is never
replaced quietly: [`AdbKey::replace`](src/key.rs) makes a new one, keeping the
old file as `adbkey.damaged`, and a phone that allowed the old key asks again.
The phone is shown the public half in Android's own format, named
`limo@limo` (a name of no person and no PC); "Allow USB debugging?" shows
its fingerprint, which [`AdbKey::fingerprint`](src/key.rs) gives as Android
computes it, for the console to show beside the question. Its `Debug` shows
the fingerprint only, and the key is wiped from memory when dropped.

LiMo never reads Google's adb's key. The example below can, for a trial run
with nobody at the phone; that is the example's own code, not this crate's.

## Every call on the phone

[`Phone`](src/phone.rs), once [`Cable::open`](src/open.rs) has connected it:

| Call | Sent | What comes back |
| --- | --- | --- |
| `Phone::connect` (by `open`) | `getprop ro.product.manufacturer`, `ro.product.model`, `ro.build.version.release`, `ro.build.version.sdk`; `settings get global adb_enabled` | `facts()`: the maker, the model, the Android version and API level, whether USB debugging is on; `too_old()` below Android 8.0 |
| `limo()` | `pm list packages ai.lucidloom.limo.child`; `dumpsys package ai.lucidloom.limo.child` | LiMo's version code and name, or none |
| `install_limo(apk)` | the APK, read on the PC once, into memory, and checked (LiMo's child form, a release build, or nothing is sent), those same bytes streamed to `cmd package install -r -S SIZE` | `Installed::Done`, or `Refused` with Android's code; `commands::install_advice` gives the words. A LiMo signed with another key is refused by Android and left as it is |
| `hand_offer(offer)` | `content write --uri content://ai.lucidloom.limo.child.cable/offer`, the offer (1 to 1024 bytes) on its standard input, never in a command line, wiped once sent | the command's exit code and what it printed |
| `reverse_link()`, `remove_link_reverse()` | `reverse` of the link's port, 47810 (`reverse(port)` for any other) | done, or why not |
| `start_limo()` | `am start -n ai.lucidloom.limo.child/.MainActivity` | `Started`, `HandedToOpenScreen` (the screen was open, and Android handed it the start), or `Refused` with Android's words |
| `debugging_off()` | `settings put global adb_enabled 0` | `Answered`, `LinkEnded` (the link ended as the setting changed), or `NoAnswer`; the phone is let go in every case |
| `close()` | nothing | the link ends, and with it any reverse |

Every call has a time limit ([`Waits`](src/phone.rs): ten seconds for the
phone to answer at all, two minutes for the person, half a minute a call,
three minutes an install). A lost link is said as [`CallError::Lost`].

## Never show the key unasked

**What was found** in the owner's first Add a phone, on the test phone
(Galaxy, Android 13), 21 Nov 2026, from the phone's own log: the console
connected and showed this PC's key the moment Add a phone opened, while the
phone was at its lock screen. Samsung's `AdbDebuggingManager` logged
`startConfirmation: isLockScreenMode`, put nothing on the screen and never
answered; from then on adbd logged `adbd_auth: prompt currently pending,
skipping` for every later connection (about one every two minutes, eleven
in all), so no question could show any more: not after unlocking, not after
the cable was pulled out and in. Only a restart of adbd cleared it, which
switching USB debugging off and on does. The key itself was right: Google's
own tool gives the same fingerprint for LiMo's key file.

**The rule since.** LiMo never shows its key to a phone unless the guardian
has just asked for it, and never asks twice by itself. A connection is quiet
unless told otherwise (`Knock::Quiet`): `CNXN`, the token signed; a phone
that sends a second token does not know this PC, and the connection stops
there with nothing more sent (`OpenProblem::NotKnown`). Only an asking
connection (`Knock::Asking`) sends the public key, once, and waits for the
person, as long as its caller says, ending early when the caller gives it up
or the phone goes. The state machine keeps a phone that does not know this
PC, or that was asked and did not allow it, as it is while it stays plugged
in: nothing is tried by itself until the phone goes and comes back, or a
person asks again.

**What a person can do** when a phone shows no question: unlock the phone,
then switch USB debugging off and on again on it (Settings, Developer
options), which restarts what answers on the phone, then ask again.

## The state machine

[`flow.rs`](src/flow.rs), pure: `seen(look)`, `step(state, event)`,
`next(state)`, `tell(state)`.

| State | `next` | `tell` |
| --- | --- | --- |
| `NotReady(case)` | look again | the case's line and what to do (above) |
| `TwoPhones(n)` | look again | n phones are plugged in; unplug all but one |
| `Ready` | connect (quietly) | the phone is plugged in, with USB debugging on; unlock it |
| `Unknown` | look again, or ask (a person's tap) | the phone does not know this PC yet; unlock it, then ask it to allow this PC: it asks once. Nothing is tried while it stays plugged in |
| `Connecting` | look again | connecting; unlock the phone |
| `Asking` | look again | The phone asks: Allow USB debugging? Tick "Always allow from this computer", then tap Allow. |
| `Connected(progress)` | the first not done of: check LiMo, install LiMo, hand the offer, reverse the link, start LiMo; and switching USB debugging off | where adding the phone stands, or what went wrong and what to do (an install refused, an offer not taken, USB debugging still on) |
| `Stopped(problem)` | look again (unplug, for an Android too old); after an ask not allowed in time, or given up, look again or ask, nothing tried while the phone stays | not allowed in time (unlock and ask again; if no question shows, USB debugging off and on first); another program holds the phone; Android too old; the connection ended |
| `Done` | unplug | USB debugging is off again on the phone. Unplug the phone. |

The console skips installing when the phone holds the version it carries;
the state machine offers it whenever LiMo was not installed in this
connection. None of its words names adb.

## What the others must do

**The console** (in the guardian's Windows account): holds a
[`WindowsCable`](src/open.rs) and LiMo's [`AdbKey`](src/key.rs), runs the
loop above while its Add a phone screen is open, shows [`tell`](src/flow.rs)
and the key's fingerprint beside "Allow USB debugging?", connects quietly by itself and asks the phone only on the guardian's tap, carries LiMo's
child form as a release APK for [`install_limo`](src/phone.rs), asks the
service for a pairing offer and hands it with
[`hand_offer`](src/phone.rs), holds the [`Phone`](src/phone.rs) for as long as
the pairing and the phone's setup run over the reversed port (the reverse
ends when it lets go), and calls [`debugging_off`](src/phone.rs) on the
guardian's tap. Its own requests for the phone call these, one each: look
(`look` and `seen`), install, pair (`hand_offer`, `reverse_link`,
`start_limo`), debugging off, done (`close`).

**The service**: listens on 127.0.0.1:47810, where the reverse brings the
phone's connections, and never ends its sending side of a connection
before it is done both ways: adb's streams have no half close, and bytes
still on their way back would be lost (the proof's first run lost them so).

**The phone's side** (LiMo's child form): a content provider at
`content://ai.lucidloom.limo.child.cable/offer`, exported and guarded by
`android.permission.DUMP`, that takes the offer from what `content write`
writes into the file it opens (at most 1024 bytes) and refuses one while
LiMo is paired already; its screen `ai.lucidloom.limo.child/.MainActivity`
acts on a start whether it is starting or already open (Android hands an
open screen the start, as `HandedToOpenScreen` says), connecting to
127.0.0.1:47810; and, like the service, it never ends its sending side
early. The `INTERNET` permission, for that connection.

## The fake phone

`src/fake.rs`, feature `fake`, for this crate's tests and the console's:
[`FakePhone`](src/fake.rs) is a [`Cable`](src/open.rs) whose phone behaves as
the test phone did on 21 November 2026. Windows' lists show it on Samsung's
driver package (WinUSB under `dg_ssudbus`, the Android class offered), or,
as a test sets it, on a cable that only charges, with USB debugging off, with
no driver, with a maker's driver LiMo cannot open, or held by another
program. Its adbd speaks the protocol as AOSP's does over a wire shaped like
USB: it asks for the key, checks the signature against the keys it allows,
shows an unknown key and lets the person allow it or not, now or later while
the PC still waits, keeps an allowed key (a test may give it one at the
start, `FakeState::knows`: a phone that allowed this PC before), records the
kind of every `AUTH` the PC sends (`Seen::auth`: a signature, or a public
key), ends its link the moment it is unplugged, answers LiMo's commands as Android did, installs with no question (and
refuses a LiMo of another key as Android does), takes an offer through the
door, hands `am start` to an open screen, keeps reverses, switches USB
debugging off and goes. A test can have it change a file at the worst
moment, as an install's stream opens, and it keeps the bytes of every
install it takes. [`FakePhone::app_connects`](src/fake.rs) plays LiMo's
app connecting to a reversed port; its close ends the stream both ways.
Every command and service is recorded, and one it does not know is refused.

## The example

`examples/phone_model.rs` connects to the one phone on this PC's USB ports,
the way LiMo will, and prints what it is. It says first what it will do to
the phone (nothing changes there: it reads, then lets go). For a person at
the PC with the phone, in `phone\cable`:

```
cargo run --example phone_model
cargo run --example phone_model -- --key adb
```

The first connects with LiMo's own key for this Windows account, quietly:
a phone that does not know it is asked "Allow USB debugging?" only after the
person presses Enter, with the phone unlocked, and once; an input that ends
first (closed, or empty) asks nothing, and the example says so and stops. The second connects with
the adb key of this Windows account instead (Google's adb's own), which a
phone may have allowed long ago, for a run with nobody at the phone: that key
is only read, never written, copied or shown, and its place is never said.
It is for trying only; LiMo uses its own key.

## What real hardware has shown, and what it has not

Shown on 21 November 2026, by the proof this crate is built from, on the
test phone at the PC that builds LiMo: every call above. Not yet shown:
this crate itself on the phone (the example is the first run); a PC that
never had Android tools (what Windows binds there by itself); Android 8 to
12 and phones of other makers; a phone whose Android sends a zero length
packet after data that fills its last packet (read correctly either way, by
the tests).

## Tests

`cargo test`, with no phone and no USB device:

| Where | What | Count |
| --- | --- | --- |
| `src/*.rs` | the look: a phone recognised on the USB bus only, a stick never, the debugging interface by its compatible id in any case, each case from what is listed; the APK reader's numbers; the commands as they cross the cable, the answers read with carriage returns, versions, `am start` handed to an open screen, Android's install refusals and their words; adb's messages (the header's sum and check), shell packets cut across messages, messages read in transfers that end where they do, a zero length packet passed over, a broken one refused; the key made once and kept, a damaged one never replaced unasked, its public half as Android reads it, base64, both PEM forms; WinUSB known in any case; the state machine's states from what is plugged in, adding a phone step by step, what goes wrong said and tried again, no word naming adb | 21 |
| `tests/phone.rs` | against the fake phone: the first connection asking and the next, quiet, connecting; a quiet connection to a phone that does not know the key sending no public key (the bytes counted); an asking one showing it once, a late Allow still connecting; the wait given up, and ended by an unplug; a key not allowed in time; each cable and driver as the PC sees it; LiMo looked for, installed with replace and no question; only LiMo's release build installed, a LiMo of another key refused and left; the APK sent the one checked, even when the file changes after the phone was told its size; the offer through the door from memory, whole, never in a command; LiMo started, an open screen handed the start; `reverse` carrying the app's 256 KiB both ways, the app closing first; USB debugging off ending the link, the PC seeing it go; an unplugged phone lost; the phone hearing only LiMo's commands; the signature the one adb makes | 15 |
| `tests/flow.rs` | the console's loop against the fake phone, from a cable that only charges to unplugging, a quiet connection finding the phone does not know this PC and the ask after it; no driver and a driver LiMo cannot use, never opened; unplugged while connected, and on again, quietly, with no second question | 3 |
| `tests/house_rules.rs` | unsafe code only in `src/win/`, each block and each `unsafe impl` saying why; which Windows calls each file makes, the look only reading; no allow attributes; nothing printed by its Debug form; every command and service touching only LiMo, none removing an app; no device path and no key printed; no dash punctuation | 6 |

Three more are ignored by default, run with `--ignored`: `real_apk` (an APK
named in `LIMO_TEST_APK`), and `this_pcs_usb_list` and
`this_pcs_android_interfaces`, which read this PC's own lists and print only
how many there are, opening nothing. `cargo clippy --all-targets
--all-features` with warnings as errors and `cargo doc --no-deps` are clean;
the crate builds without its default features too, as the phone development
tool takes it.

## Layout

```
phone/cable/
  Cargo.toml, README.md
  src/
    lib.rs        the crate's notes and its modules
    usb.rs        what Windows sees on the USB ports, and the cases (from the devtool)
    apk.rs        what an APK says it is (from the devtool)
    open.rs       the PC's side of the cable: the look, and opening the phone
    phone.rs      the phone connected, and every call on it
    key.rs        LiMo's own adb key
    protocol.rs   LiMo's own adb
    commands.rs   every command sent to a phone, and how each answer is read
    flow.rs       the state machine the console drives, and its words
    fake.rs       the fake phone (feature fake)
    win/          on Windows only, and the only unsafe code here
      mod.rs      Windows' device lists, shared
      usb.rs      Windows' list of USB devices, read only (from the devtool)
      winusb.rs   the Android class interfaces, and WinUSB
  examples/
    phone_model.rs
  tests/
    house_rules.rs, phone.rs, flow.rs
    common/       mod.rs, apk.rs (APKs built in memory)
```

## Dependencies and licences

| Crate | Version | Licence | Used for |
| --- | --- | --- | --- |
| rsa | 0.9.10 | MIT or Apache-2.0 | LiMo's key, and the signature of the phone's token (feature `adb`); the version the proof ran with |
| md-5 | 0.10.6 | MIT or Apache-2.0 | the key's fingerprint as the phone shows it (feature `adb`) |
| zeroize | 1.9.2 | Apache-2.0 or MIT | wiping the offer and the key's text |
| miniz_oxide | 0.8.9 | MIT or Zlib or Apache-2.0 | inflating the manifest inside an APK |
| adler2 | 2.0.1 | 0BSD or MIT or Apache-2.0 | used by miniz_oxide |
| windows-sys, windows-link | 0.61.2, 0.2.1 | MIT or Apache-2.0 | on Windows: the device lists, and WinUSB |
| sha1 | 0.10.7 | MIT or Apache-2.0 | tests only: the signature checked against adb's |

Under `rsa` and `md-5`, each MIT or Apache-2.0 unless said: num-bigint-dig,
num-integer, num-iter, num-traits, pkcs1, pkcs8, der, spki, const-oid,
pem-rfc7468, base64ct, rand 0.8, rand_chacha, rand_core 0.6, ppv-lite86,
getrandom 0.2, signature, digest, crypto-common, block-buffer, typenum,
smallvec, lazy_static and cfg-if; generic-array, spin and libm (MIT);
zerocopy (BSD 2-Clause or Apache-2.0 or MIT); subtle (BSD 3-Clause). `rsa` 0.9 carries
the RustSec advisory RUSTSEC-2023-0071, a timing side channel in private key
operations: here the key signs one token a connection, for a phone on the
cable; `rsa` 0.10 is out, and moving to it is a choice for later. Nothing is
GPL, LGPL or AGPL. No program of Google's ships with LiMo.

The crate is under the Apache License 2.0, as is all of LiMo.
