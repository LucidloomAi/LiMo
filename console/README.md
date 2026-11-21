# limo-console

LiMo's guardian console on the master PC: `limo-console.exe`, one window
in which a guardian sees the family's day and changes its rules. The
window shows the guardian form's own React Native screens from
[`../phone/app`](../phone/app/), exported for the web and drawn by
WebView2 in a Tauri v2 window. It holds no state of its own: every action
goes to LiMo's service through **the door**, a named pipe that only the
service makes, and the service alone holds the family's file.

**What it is now.** The console's shell. It speaks the door's protocol as
LiMo for Windows serves it ([`../windows/README.md`](../windows/README.md),
"The console's door"): it checks who serves the door before it says
anything, says hello, and forwards each request the page makes, by its
name and its arguments, one at a time; it keeps nothing. A service that is
absent, lost, or not LiMo's is said plainly on the screen. "More info"
opens LucidLoom AI's page in the PC's own browser, and nothing else leaves
the window. WebView2 starts with the switches measured below, and its keys
and menu are switched off through its own settings.

**Adding a phone** over the USB cable is the console's own work: five
requests (`phone.look`, `phone.install`, `phone.pair`,
`phone.debugging_off`, `phone.done`) that the console answers itself,
with LiMo's own way to the phone ([`../phone/cable`](../phone/cable/), no
program of Google's), on a thread of their own so the window never waits
for the phone. Pairing asks the service for an offer through the door,
hands it to LiMo on the phone over the cable, carries the phone's
127.0.0.1:47810 to the service's link, starts LiMo on the phone and holds
the phone open until the service says it is paired (see [Adding a
phone](#adding-a-phone)).

It is tested against **LiMo for Windows' own door**: its pipe threads,
everything it decides, and a real family's file, run in the test's own
process (no service started, nothing elevated); the self check signs in at
the same door, served in the builder's account by `examples/real_door.rs`.
Adding a phone is tested against limo-cable's fake phone, a throwaway app,
and the door as a stand in and as LiMo for Windows' own.

**What it is not.** It has not yet talked to the installed service, as
LocalSystem in session 0: that is a person's run (see [What a person
checks](#what-a-person-checks)). It has not reached a real phone: that is
the lead's run (see [A first run on a real phone](#a-first-run-on-a-real-phone)).
The guardian screens are built in `../phone/app`, not here. No installer,
no signing. It has run only in the self check, on the development PC: a
normal window, not focused, not on top, that closes itself within a minute.

## Contents

- [Building](#building)
- [The window](#the-window)
- [The door](#the-door)
- [Adding a phone](#adding-a-phone)
- [A first run on a real phone](#a-first-run-on-a-real-phone)
- [The self check](#the-self-check)
- [What a person checks](#what-a-person-checks)
- [The first proof](#the-first-proof)
- [Opening time](#opening-time)
- [What WebView2 contacts](#what-webview2-contacts)
- [The browser's keys and menu](#the-browsers-keys-and-menu)
- [Nothing of the build PC in the exe](#nothing-of-the-build-pc-in-the-exe)
- [Tests](#tests)
- [Layout](#layout)
- [Dependencies and licences](#dependencies-and-licences)
- [Left open](#left-open)

## Building

Needs, on Windows: Rust (1.89 or later, MSVC), Node 22, and in
`../phone/app` its packages (`npm ci`). The WebView2 Runtime to run it
(part of Windows 11). Nothing is installed and nothing runs elevated.

From this folder, in PowerShell:

```
cargo test --no-default-features                                   # the tests, against the real door: no window, no page
powershell -ExecutionPolicy Bypass -File tools\build-release.ps1   # the page, the exe, and the check for the build PC's names
cargo build --example real_door --no-default-features              # the real door, for the self check
powershell -ExecutionPolicy Bypass -File tools\run-self-check.ps1 -Name first -Fresh
powershell -ExecutionPolicy Bypass -File tools\run-self-check.ps1 -Name keys -More "--probe-keys"
powershell -ExecutionPolicy Bypass -File tools\run-self-check.ps1 -Name keys-alone -More "--probe-keys --no-page-guard"
powershell -ExecutionPolicy Bypass -File tools\run-self-check.ps1 -Name keys-control -More "--probe-keys --keys-on --no-page-guard"
powershell -ExecutionPolicy Bypass -File tools\picture.ps1 -Size 1100x760
cargo run --example webview2_contacts --no-default-features -- --hold-ms 50000
cargo run --example make_icon --no-default-features                # icons/icon.ico, again
```

and in `../phone/app`: `npm run build:console`, `npm test`,
`npm run typecheck`.

The window needs the page built first (`npm run build:console` in
`../phone/app`, which `tools\build-release.ps1` runs): Tauri embeds
`../phone/app/dist/console` in the exe, and the build stops with that
sentence when it is missing. The tests need neither: without the default
feature `window` the crate is the door's client, the switches and the
measurement's parts. They build LiMo for Windows and the hub as test only
dependencies (the first build takes a minute or two), which never reach the
exe. Dependencies are optimised in debug and test builds, as in the hub,
because the guardian password is hashed slowly on purpose.

Logs, pictures and measurements go to `target\self-check` and
`target\measure`, which git ignores.

## The window

`src/main.rs`. A Tauri v2 window (`tauri` 2.12.7, as spike 06), titled
"LiMo settings", 1100 by 760 (the console's drawings are laid out for
about that), resizable down to 360 by 560, never full screen, never on
top.

- **The page is inside the exe.** `tauri.conf.json` names
  `../phone/app/dist/console` as `frontendDist`; Tauri embeds it, brotli
  compressed, and serves it at `http://tauri.localhost/`. No development
  server, no remote address. The crate always builds with Tauri's
  `custom-protocol` feature, so even a debug build never looks for one.
- **One command.** `build.rs` lists `request` in an app manifest, so it
  needs the permission `allow-request`, which the one capability
  (`capabilities/console.json`) grants to the window `main`, and nothing
  else: no core plugins, no events, no window calls.
- **What a request does.** The page calls `request(name, args)`. Some
  names are the console's own: `more_info`, the six requests for adding a
  phone (`phone.*`, see [Adding a phone](#adding-a-phone)) and the self
  check's; every other is the door's, forwarded as it is (see [The
  door](#the-door)), except `make_offer`, which only the console asks.
  What the page gets back from the door: its reply as it came, `{"outcome":"done","value":...}`
  or `{"outcome":"problem","problem":{"kind":...,"message":...}}` with the
  service's own sentence; `{"outcome":"refused","message":...}` when the
  door does not open for this Windows account; or, when the door could not
  be asked at all, an error `{ code, words }` with the console's sentence.
- **"More info"** (`more_info`): LucidLoom AI's page, `https://lucidloom.ai`,
  in the browser Windows opens web addresses with (`win/shell.rs`). The
  address is fixed in the exe; the request takes nothing from the page and
  is refused with anything. It is the one way out of the window. No screen
  shows the link yet: it belongs on the About screen, still to be drawn.
- **No window of its own** may a page open: a new window is refused
  (`on_new_window`), as navigation is.
- **Content security policy**: `default-src 'self'; script-src 'self';
  style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self';
  connect-src ipc: http://ipc.localhost; object-src 'none'; base-uri 'none';
  form-action 'none'; frame-src 'none'`. **Tauri puts a nonce on every
  `<style>` of the page and adds it to `style-src`, and a browser then
  ignores `'unsafe-inline'` for style elements** (it still allows style
  attributes): react-native-web's own style element, which it makes at run
  time with no nonce, was refused (one violation in the first run). The
  page therefore carries that element, empty, with react-native-web's id
  (`react-native-stylesheet`): Tauri gives it a nonce and react-native-web
  uses it. Since then, 0 violations in every run.
- **Navigation** to anything but the console's own pages is refused
  (`on_navigation`).
- **WebView2's own settings**, which Tauri 2.12 does not pass on, are set
  through the window's controller (`win/webview2.rs`): the browser's
  accelerator keys and the right click menu off. Tauri's own already turn
  off the developer tools in a release build, zooming and the status bar.
  The self check reads all five back.
- **WebView2's switches** come from `webview.rs`: the console starts it
  with `CONSOLE`, now both `--disable-background-networking` and
  `--disable-component-update` beside Tauri's own (see
  [What WebView2 contacts](#what-webview2-contacts)).
- **WebView2's data** stays in Tauri's place for the program,
  `%LOCALAPPDATA%\ai.lucidloom.limo.console`, in the guardian's own
  profile. The self check gives it a folder under `target` instead.

The page is built in `../phone/app` (`npm run build:console`,
`scripts/build-console.mjs`): one bundle from `console.tsx`, made with
Expo's `export:embed` for the web, so neither the preview nor the mock is
in it (the script checks the bundle for both); a page written by the
script with the three fonts as `@font-face` rules; the fonts with their
licences; the logo. 9 files, 1.44 MB, of which the script is 653 KB.

## The door

`protocol.rs`, `door.rs`, `win/pipe.rs`, `win/token.rs`. The service's side
is LiMo for Windows' (`../windows/README.md`, "The console's door"); this is
the console's, written to the same wire and tested against the service's
own code (see [Tests](#tests)).

**The wire.** The named pipe `\\.\pipe\limo.console`. Each message is a
frame: the protocol's version (2 bytes, now 1), the length (4 bytes), both
little endian, then that many bytes of JSON; a frame over 1 MiB, or of
another version, is refused from its header alone. The console says hello
first, `{"say":"hello","protocol":1}`, and is answered with the service's
hello, or `{"say":"refused","message":...}` and the connection closed.
Then requests, one at a time, each with a number of the console's own:
`{"say":"request","id":7,"request":{"name":"lock_now","args":{...}}}`,
answered `{"say":"answer","id":7,"reply":{...}}`. No error about a frame
quotes any part of it.

**Forwarding.** The console knows no request by heart: the page asks by a
name (1 to 40 of `a` to `z` and `_`) and an object of arguments, and the
console forwards both; the service reads each, and answers one it cannot
read with `unknown_request`, keeping the connection. A request with no
arguments, or empty ones, goes with none, as the service reads it. Once its
frame is written, every string in its arguments (a password, a PIN) is
wiped, and so is the frame. The console sends at most 15 messages in a
second (`MAX_SENT_PER_SECOND`): the door closes a connection that sends
more than 20, so a page that asks too often is told so instead, and
nothing is sent.

**Who serves the door.** Any program can make a named pipe, a child's too,
and could make `limo.console` first. So before it writes anything the
console asks Windows who serves the pipe it reached:

1. the serving process runs in session 0, where only services run
   (`GetNamedPipeServerSessionId`);
2. the pipe belongs to LocalSystem (`S-1-5-18`), the account the service
   runs as, or to the Administrators group (`S-1-5-32-544`) (its owner,
   from its security descriptor). Windows gives what a LocalSystem
   process makes to that group, not to LocalSystem itself, because the
   group is the default owner in LocalSystem's token: the installed
   service's door reads back as the group's. That was found in the owner's
   first run against the installed service, 21 Nov 2026, when the window
   refused the real service; the group is taken for LocalSystem only
   (`ServerMustBe::owner_fits`);
3. the serving process's program file is `limo.exe` in LiMo's folder under
   Windows' own Program Files folder (`GetNamedPipeServerProcessId`, then
   the image path; the folder from `SHGetKnownFolderPath`, never from an
   environment variable).

The first two are what a program in a child's account, or in any account
that is not an administrator, cannot fake: making a process in session 0,
or a pipe that belongs to LocalSystem or to the Administrators group,
takes an administrator who has said yes to Windows' own question. The
third is checked whenever Windows lets the guardian's account read the
service's program file. It often does not: a standard account may not
look into a LocalSystem process, and **neither may an administrator's
account that is not elevated** (seen in the same run), so there the first
two decide, and the window's own log says so. If a check fails the
console closes the pipe having written nothing, writes in its log what it
saw (see [The window's own log](#the-windows-own-log)), and the screen says:
"Something that is not LiMo’s service answered, so the console sent it
nothing. Your password was not sent." The console opens the pipe asking
for read, write data and write attributes only (`0x12018b`, within the
`0x12019b` the door grants) and lends its token at the **identification**
level, so whatever serves the pipe can read who the guardian is and can
never act as the guardian.

**One connection.** A sign in belongs to a connection, so the console keeps
one; after a refusal or any fault it is dropped, and the next request opens
a new one, signed out.

**What the screen is told** when the door cannot be asked:

| Code | When | Words |
| --- | --- | --- |
| `not_running` | no pipe of that name | "LiMo’s service is not running on this PC, so its settings cannot open." |
| `busy` | every instance stayed busy for 2 s | "LiMo’s service is busy. Try again in a moment." |
| `not_the_service` | a check above failed | "Something that is not LiMo’s service answered, so the console sent it nothing. Your password was not sent." |
| `lost` | the door closed, answered with something that is not its own, answered another request's number, or nothing within 5 s of hello or 20 s of a request (a sign in is hashed slowly on purpose) | "LiMo’s service stopped answering. Try again: the console asks it afresh, and you sign in again." |
| `too_fast` | the console's own limit | "The settings asked LiMo’s service too often at once. Try again in a moment." |

Each of these comes to the page as `{ "code", "words", "log" }`, `log`
being where the window's own log is (`%LOCALAPPDATA%\LiMo\console.log`),
for a screen that has room to say so. Everything else the screen shows is
the service's own sentence: a wrong password with the tries left, a wait,
`busy` for a second password within a second, a child's account refused,
signed out after fifteen idle minutes.

### The window's own log

`src/console_log.rs`. When the window says the door cannot be asked, the
reason is in **`%LOCALAPPDATA%\LiMo\console.log`**, the signed in account's
own file (beside LiMo's key for phones), which that account can read and no
other standard account can. It holds, each line with its time in UTC:

- the window opening, with its version;
- the door's notes: what served the pipe when the check refused it (its
  process, its session, the pipe's owner, its program file, against what
  LiMo's service is), a program file this account may not read, a refusal
  at hello, a connection dropped and why;
- each request that could not be asked, by its name only, and why (the
  service not running, lost, busy, too fast, not LiMo's service);
- the same for the requests the console asks for a phone;
- a line for each ask of the phone, and its end (allowed, not allowed in
  time, given up), and for each phone found that does not know this PC:
  never the key, its fingerprint or the phone's serial number.

Never a request's arguments, a password or a PIN: no line is made from
them. Before a line is written, anything of an account is taken out: a
security identifier that is not a well known one becomes "(an account's
own identifier)" (LocalSystem's, the Administrators group's and the like
stay, as they name no person), a folder under `\Users\` becomes
`\Users\(an account)\`, and this account's and this PC's names are taken
out wherever they are. It is kept short: past about 96 KB only its last
300 lines are kept. Writing it never stops the window. The self check
writes its own beside its log, never in the builder's AppData.

**On the screen.** The sign in shows the console's sentence (the table
above) in the line under the field, which keeps room for two lines; at a
phone's width "Something that is not LiMo’s service answered..." already
fills both, and a third would move the button (as the first proof found).
So the screen is left as drawn, and the answer carries `log` for a drawing
that makes room for it.

## Adding a phone

`src/phone/`. The Add a phone screens (frames A1 to A9 of
[`../design/screens/console.html`](../design/screens/console.html), and the
cable's strips at its foot) are built in `../phone/app`; the console gives
them five requests of its own, through the same `request(name, args)`,
answered by the console and never sent to the service.

**A thread of its own** (`worker.rs`) holds the cable and, once
connected, the phone. A request starts a step and answers at once, and
`phone.look` reads where everything stands (`look.rs`), so the window never
waits for the phone, not while the phone asks "Allow USB debugging?" (up to
ten minutes) nor while Android installs (up to three). The thread starts at
the first `phone.look`. Once a second it looks at the USB ports (read only)
and hands the look to limo-cable's state machine (`limo_cable::flow`). By
itself it connects **quietly** as soon as a phone LiMo can open is plugged
in: it never shows the phone this PC's key, so a phone that does not know
this PC is only said to (the strip's `unknown`), and nothing more is tried
while it stays plugged in. The phone is asked to allow this PC only by
`phone.ask`, the guardian's tap, once (see [Never show the key
unasked](#never-show-the-key-unasked)). Connected, it looks whether LiMo is
on the phone; when the phone holds the version this PC carries, nothing is
installed. What went wrong when connecting stays said for 5 seconds, then
it tries again; an Android too old stays said until the phone is unplugged.
A minute with no request and it lets go of the phone and stops.

- **The cable** (`counted.rs`) is limo-cable's own way, through Windows'
  WinUSB, opened as `WindowsCable` opens it, with the wire counting what it
  sends: the install's progress is what really went down the cable.
- **LiMo's key** is limo-cable's, `%LOCALAPPDATA%\LiMo\adbkey` in the
  guardian's account, made at first use. A damaged one is made anew, and a
  phone that knew the old asks again.
- **LiMo's app for phones** is `phone\limo-child.apk` beside
  `limo-console.exe`, read once when the looking starts: LiMo's child form
  (`ai.lucidloom.limo.child`) as a release build, or it is not put on a
  phone. limo-cable sends the very bytes it checked.
- **Pairing** (`phone.pair`), through the door on the guardian's signed in
  connection (`door_service.rs`): `devices` for the child (the devices
  before), then `make_offer`; the offer's text is kept in memory and the
  rest of the answer wiped; its `port` must be the link's, 47810. The offer
  goes to LiMo on the phone through its guarded door (on standard input,
  then wiped), the phone's 127.0.0.1:47810 is carried to this PC's, where
  the service's link listens, and LiMo is started on the phone. Then once a
  second `devices` again, until a new device is listed and `device_link`
  answers for it: paired. A pairing is waited for as long as its offer
  lasts (ten minutes), never more than fifteen.
- **The phone is held open** while the pairing waits and after it (the
  phone's own setup and its first reports go over the cable until it
  reaches the PC on the home Wi-Fi), until `phone.debugging_off`,
  `phone.done`, or a minute with no request. Every two seconds after
  pairing, `device_link` says how the phone reaches the PC.
- **Unplugged while the pairing waits**, the pairing goes on waiting (LiMo
  on the phone has the offer, with the PC's home addresses). Plugged in
  again, the link's port is carried again; nothing is handed or installed
  twice.
- **`make_offer` is the console's own**: a page asking for it is refused
  (`protocol::CONSOLE_ONLY`), so an offer's secret reaches no page and no
  answer.
- **The self check never reaches a phone**: every `phone.*` request is
  refused in it, since the PC it runs on may have one plugged in.

### The requests

Each answer is a value, or an error `{ "code", "words" }` with the sentence
the screen shows.

| Request | Arguments | Answer | Errors |
| --- | --- | --- | --- |
| `phone.look` | none | the look (below) | `unknown_request` |
| `phone.ask` | none | `{"started": true}`: the phone is shown this PC's key, once, and asked to allow it; the ask waits up to ten minutes for Allow, and ends early when the phone goes, at `phone.done`, or a minute after the screens' last request; never made again by itself | `no_phone`, `busy`, `already_connected`, `unknown_request` |
| `phone.install` | none | `{"started": true}`; `{"started": false}` when LiMo is on the phone already | `no_phone`, `too_old`, `busy`, `no_app` |
| `phone.pair` | `{"child": "<child id>"}`, nothing more | `{"started": true}`; `{"started": false}` when paired already | `no_phone`, `too_old`, `not_yet`, `busy`, `unknown_request` |
| `phone.debugging_off` | none | `{"started": true}`; `{"started": false}` when off already | `no_phone`, `too_old`, `busy` |
| `phone.done` | none | `{"done": true}` | none |

| Code | Words |
| --- | --- |
| `no_phone` | "No phone is connected yet. Plug it in, unlocked, with USB debugging on." |
| `too_old` | "This phone has Android 7.1. LiMo needs Android 8.0 or later." (its version) |
| `busy` | "The phone is busy with another step. Wait a moment, then try again." |
| `already_connected` | "The phone is connected already: it knows this PC." |
| `not_yet` | "LiMo is not on the phone yet." |
| `no_app` | the app's line, below |
| `unknown_request` | "phone.pair takes the child, as {"child": "<its id>"}, and nothing else"; "phone.look takes nothing" (and so on); "that is not the name of a request" |

### The look

What `phone.look` answers, here while LiMo goes on the phone:

```json
{
  "cable": { "case": "ok", "line": "Connected: Galaxy S20 Ultra 5G.", "more": "Android 13." },
  "fingerprint": null,
  "asked_secs": null,
  "phone": { "name": "Galaxy S20 Ultra 5G", "model": "SM-G988B", "maker": "samsung", "android": "13",
             "recent_enough": true, "limo": { "looked": true, "on_it": false, "version": null },
             "this_pc_carries": "0.3.0" },
  "checks": [ { "ok": true, "words": "One phone plugged in" },
              { "ok": true, "words": "Android 13, recent enough for LiMo" },
              { "ok": true, "words": "LiMo is not on it yet" } ],
  "install": { "stage": "sending", "sent": 0.62, "line": "Sending LiMo to the phone.", "advice": null },
  "pairing": { "stage": "not_asked", "device": null, "ways": [], "line": null, "advice": null },
  "debugging": { "stage": "on", "line": null, "advice": null },
  "busy": "installing",
  "held": true
}
```

**`cable`**, the strip at the foot of every step, in the drawing's words:

| `case` | `line` | `more` | Drawn |
| --- | --- | --- | --- |
| `looking` | "Looking at this PC’s USB ports." | "" | not drawn: before the first look, under a second |
| `none` | "No phone on a USB port." | "If it is plugged in, the cable may only charge it. Try the one that came with the phone." | none |
| `off` | "A phone is plugged in." | "USB debugging is off on it." | off |
| `driver` | "No working driver for this phone." | "Install its maker’s USB driver for Windows, such as Samsung’s Android USB Driver." | driver, coral |
| `connecting` | "A phone is plugged in." | "Connecting to it." | not drawn: as allow, for a moment |
| `unknown` | "A phone is plugged in." | "It does not know this PC yet." | not drawn: a phone with USB debugging on that does not know this PC; nothing was asked |
| `allow` | "The phone is asking." | "Waiting for Allow on the phone." | allow: asked by `phone.ask`, waiting |
| `ok` | "Connected: Galaxy S20 Ultra 5G." | "Android 13." | ok, teal |
| `trouble` | what went wrong | what to do | not drawn: coral |

`trouble` is limo-cable's own words (with the screens' apostrophe):
another program holds the phone ("Another program on this PC holds the
phone’s connection (Android Studio, or Google’s adb)." "Close it, then
unplug the phone and plug it in again."), not allowed in time ("The phone
did not allow this PC." "Unplug the phone, plug it in again unlocked, and
tap Allow when it asks."), Android too old, two phones, the connection
ended, the phone not reached, the USB list unreadable, a driver LiMo cannot
open; and the console's own: LiMo's key could not be had, and "USB
debugging is on again on the phone." After USB debugging is off the strip
says what is plugged in: `off` while the phone is still in (A9's foot),
`none` once it is out. The phone's name is the one Windows shows for it,
else its model.

**`fingerprint`**: the key's fingerprint as the phone shows it, sixteen
pairs ("3C:91:5A:07:..."), while `case` is `unknown` or `allow` (A3);
otherwise null. **`asked_secs`**: whole seconds since the phone was asked,
while `case` is `allow`; otherwise null.

**`phone`**: null until a phone has connected, then the last one's: its
`name`, `model`, `maker`, `android`, `recent_enough`, `limo` (`looked`,
`on_it`, `version`) and `this_pc_carries`, the version of the app here.

**`checks`**, A4's ticks, from the moment it connects: `ok` and `words`.
"One phone plugged in"; "Android 13, recent enough for LiMo", or not ok
"Android 7.1, older than the Android 8.0 LiMo needs"; once LiMo was looked
for, one of "LiMo is not on it yet", "LiMo 0.3.0 is on it already", "LiMo
0.2.0 is on it, and this PC carries 0.3.0", "LiMo is on it".

**`install`**: `stage`, `sent`, `line`, `advice`.

| `stage` | `sent` | `line`, and `advice` |
| --- | --- | --- |
| `not_asked` | null | null |
| `not_needed` | null | "LiMo 0.3.0 is on the phone already." |
| `sending` | 0 to 1, in hundredths: how much of the app is on the phone | "Sending LiMo to the phone." |
| `installing` | 1 once all is sent; null on a cable that does not count | "The phone is installing LiMo." |
| `done` | null | "LiMo is on the phone." |
| `refused` | null | "The phone did not take LiMo.", with limo-cable's advice by Android's code: a LiMo of another maker left as it is, a newer LiMo on the phone, no room, installs over the cable blocked (Auto Blocker, "Install via USB"), stopped on the phone |
| `failed` | null | "The phone’s connection ended while LiMo was being put on it." "Plug the phone in again, unlocked, then try again."; "The phone did not finish installing LiMo in time." "Look at the phone’s screen for a question, then try again."; "The phone did not answer as expected." "Try again; if it happens again, unplug the phone and plug it in again." |
| `no_app` | null | "LiMo’s app for phones is missing from this PC.", or "The app for phones beside this program is not LiMo’s own."; "Install LiMo on this PC again: it puts the app for phones beside the console." |

Over USB the app is sent in a second or two; most of the wait is Android
installing it, which says nothing until it is done. So `sent` fills
quickly, then `installing` stays without a figure.

**`pairing`**: `stage`, `device`, `ways`, `line`, `advice`.

| `stage` | `line` |
| --- | --- |
| `not_asked` | null |
| `asking_the_service` | "Asking LiMo’s service for a pairing." |
| `handing` | "Handing the pairing to LiMo on the phone." |
| `waiting` | "Waiting for LiMo on the phone to pair with this PC." |
| `paired` | "Over the cable, with this PC.", "Over the home Wi-Fi, with this PC." or "With this PC." (no session open yet); `device` is `{"device_id", "name"}`, the device the service paired |
| `failed` | the service's own sentence, such as "The link for phones is not running on this PC, so no phone can pair with it now. The service’s log says why." or its signed out one; or "LiMo on the phone did not take this PC’s pairing." "Open LiMo on the phone; if it says it belongs to a family already, it was set up before."; "The phone did not answer as expected."; "The pairing offer ran out before LiMo on the phone paired." "Try again."; "LiMo’s service gave no pairing offer."; the connection ended |

`ways` is every way the phone has reached the PC since it paired, in the
order first seen: `cable`, `home_network`.

**`debugging`**: `on` (LiMo has not switched it off; no line),
`switching_off` ("Switching USB debugging off."), `off` ("USB debugging is
off."), `still_on` ("USB debugging is still on." "Switch it off on the
phone: Settings, Developer options, USB debugging.").

**`busy`**: the call running now, `connecting`, `asking`, `checking`, `installing`,
`pairing`, `switching_off`, or null. **`held`**: true while the console
holds the phone open.

### What the screens need to know

- Ask `phone.look` once a second from A1 to A9, as the drawing says; a
  minute with no request and the console lets go of the phone. Ask
  `phone.done` when Add a phone closes, by Done or by leaving it; a
  `phone.look` after that starts afresh, and connects again, quietly, to a
  phone with USB debugging on.
- The console connects quietly as soon as a phone with USB debugging on is
  plugged in, even on A1: a phone that allowed this PC before connects (`ok`)
  with no question; one that does not know this PC is `unknown`, and nothing
  is shown to it.
- **A3's button asks the phone**: `phone.ask`, once, when the guardian taps
  it with the phone unlocked; never by itself. While `case` is `unknown` the
  screen shows the fingerprint and the button; at `allow` the phone asks, and
  the screen may show `asked_secs`; at `ok` it moves on. An ask that
  nobody answered in time is `trouble` ("The phone did not allow this PC in
  time."), and stays until the phone goes or the guardian asks again: the
  button then says to unlock the phone and ask again, and that a phone that
  shows no question wants USB debugging off and on first.
- Steps that wait for the phone move on by what the look says: A2 when
  `case` is no longer `none`, `off` or `driver`; A3 as above; A4 shows `checks`, asks
  `phone.install` once `phone.limo.looked` is true, and moves on at
  `install.stage` `done` or `not_needed`; A5 asks `phone.pair` with the
  child chosen at A1, and moves on at `pairing.stage` `paired`, whose
  `device.device_id` is what `rename_device` takes (the screen asks the door
  for that itself); A6 is the phone's own steps, from the service
  (`device`), not from here; A7 waits for `home_network` in `pairing.ways`;
  A8's "Switch it off for me" asks `phone.debugging_off`, and moves on at
  `debugging.stage` `off`; A9's ticks are `install`, `pairing`, the ways and
  `debugging`.
- Every line in an answer is for the screen as it stands, with the
  typographic apostrophe the drawings use. No answer holds a secret: never
  an offer, never the key, only its fingerprint.

### Never show the key unasked

**What the owner's first Add a phone found**, on the test phone (Galaxy,
Android 13), 21 Nov 2026, from the phone's own log: the window connected
and sent this PC's key the moment Add a phone opened, on step 1, while the
phone was at its lock screen. Samsung's `AdbDebuggingManager` logged
`startConfirmation: isLockScreenMode`, put nothing on the screen and never
answered. From then on adbd logged `adbd_auth: prompt currently pending,
skipping` for every later attempt (one about every two minutes, eleven of
them): no question could show any more, not after unlocking, not after the
cable was pulled out and in. Only a restart of adbd cleared it, which USB
debugging off and on does. The key itself was right: Google's own tool gives
the same fingerprint for LiMo's key file.

**The rule since.** LiMo never shows its key to a phone unless the guardian
has just asked for it, and never asks twice by itself. `phone.look` only
ever makes quiet connections (limo-cable's `Knock::Quiet`: the token signed,
and nothing more sent to a phone that does not know the key); after one
found the phone does not know this PC, nothing is tried again until the
phone goes and comes back, or the guardian asks. `phone.ask` makes one
asking connection, which shows the key once and waits ten minutes for the
answer, so that a late Allow still connects; it ends early when the phone
goes, at `phone.done`, or a minute after the screens' last request, and is
never made again by itself. The window's log has a line for each ask and
for each phone that did not know this PC (no key, no fingerprint, no
serial number).

**What a person can do** when the phone shows no question: unlock the
phone, then switch USB debugging off and on again on it (Settings,
Developer options), then ask again.

### The answers as files, for the screens' tests

`src/phone_answers.rs` writes the answers of the six requests, as the page
gets them, into `../phone/app/tests/window-answers/`, one file per request
and case (`look-none.json`, `look-allow.json`, `install-started.json` and so
on): the answers the window itself gives, as against the door's in
`console-answers/`, with a README of its own there. Each holds
`request` (`phone.look` and so on), `case` (in a few words), `asked` (the
name and the arguments), and `answer` (what the request resolved with) or
`error` (what it was rejected with, `{code, words}`). It is a test with the
console's others (`cargo test --no-default-features`, on Windows), or
alone:

```
cargo test --no-default-features phone_answers
```

It plays limo-cable's fake phone through the console's real desk and
writes each answer as it came: no phone, USB debugging off, no driver, the
phone not knowing this PC (nothing asked), the ask, not allowed in time, connected,
LiMo put on, paired over the cable, then over the home Wi-Fi too, USB
debugging switched off, done; LiMo there already, older, of another maker;
no app on this PC, an Android too old, another program holding the phone,
the phone unplugged; the service making no offer, an offer that runs out;
and the requests' own answers and errors (started, not needed, busy, no
phone, not yet, no child, too old, no app, already connected, arguments a
request does not take). The moments too short for the fake phone to hold,
or whose seconds would change from run to run (looking, connecting, the
phone asking with `asked_secs` 12, two phones, sending, installing, asking the service, handing,
switching off, USB debugging still on, an install whose connection ended)
come from the same answer code
(`phone::look`), given that moment's state.

A run with no change writes no file: what would differ is a fixed sample,
the door's own where it has one (the key's fingerprint as the drawing has
it, the child `child-0000000000000001`, the phone
`device-0000000000000002`, "Asha’s phone"). No file holds an offer, a key,
a password or anything of this PC, and the test checks so. A `.json` file
in that folder that a run did not write is removed; its README is left.

## A first run on a real phone

For the lead, with the Galaxy S20 Ultra (Android 13) and LiMo for Windows
installed on this PC. Until the Add a phone screens exist,
`examples/add_phone.rs` asks the same five requests the screens will, of
the same desk, against the real cable and the real door, and prints each
part of the look when it changes. It never prints the password, and no
answer holds the offer.

1. **The service**, installed and running as on a family's PC
   (`limo install --dev`, `../windows/TESTING.md`), with a family and a
   child. Its link must listen: `netstat -ano | findstr 47810` shows
   `127.0.0.1:47810` listening (when not, the service's log says why, and
   pairing will say so in the service's own sentence).
2. **The app**: LiMo's child form as a signed release APK (a debug build
   is refused before anything is sent). For the example it is passed with
   `--app`; for the console it goes beside `limo-console.exe` as
   `phone\limo-child.apk`.
3. **Nothing else holding the phone**: close Android Studio, and stop
   Google's adb server (`adb kill-server`) if it runs; the strip says so if
   one does.
4. **The phone**: USB debugging on (Settings, About phone, Software
   information, Build number seven times; then Settings, Developer options,
   USB debugging), Auto Blocker off (Settings, Security and privacy),
   unlocked, plugged in with its own cable.
5. **Run it** from `console/`, in the guardian's own Windows account, not
   elevated, with the guardian password in a file of its own (deleted
   after):
   `cargo run --release --no-default-features --example add_phone -- --password-file <file> --app <apk> --leave-debugging-on`
   (and `--child <id>` when the family has more than one child; the
   example lists them). With `--leave-debugging-on` the phone can be added
   again over the cable without a person switching USB debugging on again;
   a second run without it tries the switch off.
6. **Watch**: the example connects quietly and says the phone does not know
   this PC (the strip's `unknown`, with the fingerprint); nothing shows on the
   phone yet. Unlock the phone, then press Enter: only now is the phone
   asked, once (an input that ends first asks nothing: the example says so
   and stops), and it asks "Allow USB debugging?": compare the fingerprint,
   tick "Always allow from this computer", tap Allow (there are ten
   minutes). If no question shows, switch USB debugging off and on again on
   the phone, then run the example again (see [Never show the key
   unasked](#never-show-the-key-unasked)). Then the checks, `sending` with `sent` rising, `installing`,
   `done` (or `not_needed`), `waiting`, LiMo opening on the phone, `paired`
   with the device's id and `cable`, then up to two minutes for
   `home_network`.
7. **Write down**: the printed lines (with their seconds), whether `sent`
   moved before `installing`, any `trouble` or `failed` line, how long from
   Allow to `paired`, what LiMo showed on the phone, and whether the phone
   reached the PC over the home Wi-Fi.

The same run through the window, once the screens are built: open
`limo-console.exe` from the same account, sign in, Add a phone; the
console finds the app beside itself.

## The self check

`limo-console.exe --self-check <log> --door-base <pipe> --door-server
<program> --password-file <file>` (`src/self_check.rs`, and the page's
driver in `../phone/app/src/console/selfCheck.ts`), run by
`tools\run-self-check.ps1`, which first starts LiMo for Windows' own door
(`tools\door.ps1`, `examples/real_door.rs`): two doors in the builder's own
account, each with a real family's file in a temporary folder, as a family
is after its first start (a guardian password new each run, written to a
file and never on a command line, a PIN, and a child called Asha, so that
the screens open on Today after signing in); at the second this account is
linked to Asha. The console
checks them by the rule this account, this session, that program, as it
checks the service by LocalSystem, session 0, `limo.exe`.

The page then signs in as a guardian would, by the DOM: it types into the
field with the input's own value setter and presses the button with a
click, through the same handlers, `host.request` and Rust command a
guardian's typing reaches. Steps, each written to the log with the
milliseconds since the process started, beside the door's own words:

1. `js_start`;
2. `sign_in_drawn`: once the door answered `about` and the button is on;
   the three fonts loaded, and what the browser drew with (the heading in
   "Fraunces 28" at 28 px, the field 48 high on card paper, the button
   marigold);
3. `wrong_password`: the door's "That password is not right. 4 more tries
   before a wait.", the field empty again;
4. `signed_in`: the right password a second later (the door answers a
   second password within a second `busy`), and the signed in card;
5. `ready_for_keys` and `keys` (see [The browser's keys and
   menu](#the-browsers-keys-and-menu));
6. `more_info`: the console says what it would open,
   `{"address":"https://lucidloom.ai","opened":false}`; the self check opens
   nothing;
7. `child_account`: the second door; the page reloads and shows the
   door's refusal, "LiMo’s settings cannot be opened from Asha’s account. A
   guardian opens them from their own.", with no password field;
8. `not_the_service`: the guardian's door again, checked by LiMo's own rule
   (session 0, LocalSystem, `Program Files\LiMo\limo.exe`); the console
   writes nothing, and the screen says the password was not sent;
9. `not_running`: a door nobody serves; the screen says the service is not
   running.

Then it writes `done` and exits with 0 after `--hold-ms`; the door stops,
and its log is printed. A step missing after 25 s writes which and exits
with 1; a window closed early exits with 1; whatever happens the process
ends before 57 s. Outside self check mode none of it is reachable: the
page's driver starts only when the exe put its mark on the page, Rust
refuses every self check request without a self check, and the door
options are refused without `--self-check`. In it, every request for a
phone is refused: the self check never reaches a phone. Options, only with
`--self-check`: `--hold-ms`, `--switches`, `--proxy`, `--webview-data`,
`--size`, `--probe-keys`, `--keys-on`, `--no-page-guard`, and the three
door options, the door's pipe always `\\.\pipe\limo.console.<something>`,
never LiMo's own.

On 21 Nov 2026 (a first start, and a later one with the key probe): every
step seen, 0 violations of the content security policy, the door's log
saying "a wrong guardian password was typed", "signed in with the guardian
password", and "refused to a console ... its account is linked to a child".
The sign in was drawn 360 to 560 ms after the process started; a sign in
at the door took 66 to 94 ms. Run again the same evening with the requests
for a phone in the exe and the guardian screens built: all 10 steps in
2.1 s, exit code 0.

`tools\picture.ps1` takes a picture of the console's own window during a
self check (`PrintWindow`, the window alone, nothing else on the screen),
at 1100 by 760 and at 390 by 844: `target\self-check\sign-in-*.png`.

## What a person checks

What no builder may do: the installed service, as LocalSystem in session
0, and keys typed into the focused window. In the owner's run with LiMo
installed (`limo install --dev`, `../windows/TESTING.md`), about ten
minutes:

1. From the guardian's own Windows account, open `limo-console.exe`: the
   sign in shows, a wrong password gets the door's words, the right one
   opens. From an administrator's account that is not elevated (the usual
   way of signing in to Windows), and from a standard guardian account if
   there is one, Windows does not let the console read the service's
   program file: the console then opens on the session and the owner alone,
   and its log says "the door's program file could not be read from this
   account; its session and its owner decided". Seen in the owner's first
   run, 21 Nov 2026, from an administrator's account.
2. From the child's account (`asha`): the door's refusal, no password field.
3. Stop the service (`sc stop LiMo` from an administrator's window): the
   next sign in says the service stopped answering, and then that it is not
   running. Start it again: the console signs in again.
4. In the console's window, press F5, Ctrl+R, Ctrl+P, Ctrl+F, F12,
   Ctrl+Shift+I, Ctrl and the plus key, Alt and the left arrow, and right
   click the page and the field: nothing may happen but typing in the field.
5. **The window's own log.** After any of these, open
   `%LOCALAPPDATA%\LiMo\console.log` (type it into Explorer's address bar):
   a line for the window opening, the door's notes and every request that
   could not be asked, by name; no password, no account's own identifier,
   no user's name or folder. When the window says something that is not
   LiMo's service answered, the line "what served the pipe: ..." there says
   what it saw (the owner's first run would have read "the pipe owned by
   S-1-5-32-544" there).

The owner's first run found that the installed service's pipe belongs to
the Administrators group, not to LocalSystem, and that the window
refused it for that: fixed (see [The door](#the-door)), and the reason
the window now has a log a person can read.
## The first proof

What the console's technology had to show before the shell was built on
it, each on this PC on 21 Nov 2026 (Windows 11 Home, WebView2 Runtime
158.0.4370.33). Point 2 was proven then against a stand in for the
service's half; the shell now speaks to the real door's code (see [The
door](#the-door) and [Tests](#tests)).

| | | Result |
| --- | --- | --- |
| 1 | The app's own sign in screen, from its tokens, fonts and icons, exported for the web with a `console` place in the host seam, in a Tauri v2 window, opening in under half a second | **Proven for a later start, barely**: drawn with its three fonts 399 to 498 ms after the process started (median 481 ms); a first start with a new WebView2 data folder 510 ms (see [Opening time](#opening-time)) |
| 2 | The door's two halves in miniature: the access list and the check of who is on the other end, a child's account refused, one round trip for any other; the console checking the server before it sends a password | **Proven in this account**: 32 tests over real pipes and the self check; the service's own half, as LocalSystem in session 0, is the owner's run |
| 3 | What WebView2 still contacts with each switch | **Measured: fewer, not none.** `--disable-background-networking` removes its one request through its own network stack (`config.edge.skype.com`); with every switch, every later start still opens one HTTPS connection to a Microsoft address from WebView2's browser process, about 2.5 s in, past any proxy (see [What WebView2 contacts](#what-webview2-contacts)) |
| 4 | F5, Ctrl+P, Ctrl+F and the right click menu do nothing in the release window | **Partly**: WebView2's own settings for them are off and read back off, the page stops the keys itself, and a right click and F5 reach the page and open or reload nothing; a real key press needs the window in front, which the builder may not take, so one minute of a person is still owed |
| 5 | A release exe with no user name of the build PC | **Proven**: 0 times for the user name, the home folder, the PC's name and the repository's folder (the unremapped debug exe holds the user name 1,935 times) |

## Opening time

From the first line of `main` to the page's report that the sign in is
on screen with its three fonts loaded and the door's `about` answered,
read from the self check's log (`tools\run-self-check.ps1 -More "--size
1100x760"`, six runs one after another, 21 Nov 2026; other programs,
builds among them, were running on the PC):

| Run | Window made | WebView2's settings set | Page loaded | Sign in drawn | Every step done |
| --- | --- | --- | --- | --- | --- |
| a new WebView2 data folder | 383 ms | 383 ms | 495 ms | **510 ms** | 686 ms |
| later starts, 5 runs | 302 to 396 ms | the same | 390 to 489 ms | **399 to 498 ms, median 481 ms** | 577 to 681 ms |

At the self check's smaller size (720 by 640), the 20 runs of the
measurement below drew the sign in in 356 to 629 ms, 18 of them under 500
ms. Most of the time passes before the window exists: Tauri's setup and
WebView2's browser process starting. The page's own script runs about 40
ms after its load starts, and from a click to the answer on screen takes
11 to 12 ms, the door's round trip under 1 ms of it. Not measured: a cold
start after a restart, which will be slower.

So a later start opens in under half a second, with little to spare, and
a first start just over it.

## What WebView2 contacts

LiMo promises that nothing leaves the home. The console's page fetches
nothing (its policy allows only its own files and Tauri's bridge, with 0
violations), but WebView2 is a browser, and spike 06 saw it open an HTTPS
connection to a Microsoft address by itself. Measured on 21 Nov 2026,
WebView2 Runtime 158.0.4370.33, Windows 11 Home, without elevation and
sending nothing anywhere ourselves (`examples/webview2_contacts.rs`):

- each run is the release exe's self check, signing in and held open 50
  seconds (the builder's window must close within a minute), with a new
  WebView2 data folder (**first** start) or the one the run before left
  (**later** start);
- every 50 ms, every TCP connection with a remote end of the console or a
  process it started, from Windows' own table, with the process's part in
  WebView2 (browser, network service, renderer) from its command line;
- **host log** runs: WebView2 started with `--proxy-server` pointed at a
  proxy on a loopback port picked at run time, which writes down the host
  and port each request names, refuses it (403) and passes nothing on.
  A connection that still appears in Windows' table went around the proxy.
  **Open** runs: no proxy, so what is really opened, by address.

| Switches (beside Tauri's own) | Host log, first start | Host log, later start | Open, first start | Open, later start |
| --- | --- | --- | --- | --- |
| none: Tauri's own alone | asked `config.edge.skype.com:443` at 0.27 s and again at 1.3 s | the same, **and** the browser process straight to 40.99.10.34:443 at 3.1 s | the network service to 52.123.252.36:443 at 0.23 s (a repeat: 150.171.22.17:443 at 0.29 s) | the browser process to 40.99.9.82:443 at 2.5 s |
| `--disable-background-networking` | nothing | the browser process to 52.98.50.18:443 at 2.4 s | nothing | the browser process to 40.99.10.66:443 at 2.6 s |
| `--disable-component-update` | asked `config.edge.skype.com:443` at 0.42 s and 1.4 s | the same, and the browser process to 40.99.10.2:443 at 2.9 s | the network service to 150.171.22.17:443 at 0.24 s | the browser process to 40.99.10.66:443 at 2.4 s |
| **both** (the console's) | nothing | the browser process to 40.99.10.114:443 at 2.4 s | nothing | the browser process to 52.98.33.130:443 at 2.4 s |

Every address is on port 443, and every connection was a single one. The
browser process's addresses (40.99.x and 52.98.x) are in the same blocks
as the two spike 06 saw. Who holds each block was not looked up: that
would mean asking a server outside the PC. Nothing else was opened or
asked for in 50 seconds by any process of the console.

What it means:

1. **`config.edge.skype.com`** is Edge's configuration and experiments
   service, asked by WebView2's network service at once on every start
   whose last answer is not still fresh (an open later start did not ask:
   it had the answer of the run before). `--disable-background-networking`
   stops it; `--disable-component-update` does not.
2. **One connection from WebView2's browser process on every later start,
   with every switch**, about 2.5 seconds in. It goes around the proxy,
   so it is made outside Chromium's network stack, and it never happens on
   a first start. Its name could not be read: Windows' DNS cache, which
   holds it, can be listed only by an administrator on this PC (`ipconfig
   /displaydns` says "The requested operation requires elevation"), and
   WebView2 looks up its own names. That it is WebView2's diagnostic data,
   sent at the start after the session it describes, fits everything seen
   and fits Microsoft's documentation, read before this proof, which says
   WebView2 sends required diagnostic data whatever Windows' setting; it
   is **not confirmed**. No switch tried stops it.
3. **Component updates** showed nothing in 50 seconds with any set; a
   browser checks its parts minutes after it starts, so that switch is
   kept for a console left open.

So the honest answer is **fewer connections, not none**: with the
console's switches, nothing on a first start, and one HTTPS connection to
Microsoft at each later start. Not seen here, and to be said plainly
rather than assumed absent: what a console open for minutes sends later,
what Windows itself sends for WebView2, and UDP (WebView2's own name
lookups, QUIC), which Windows' table does not show.

The connection that remains is WebView2's, not LiMo's, and none of the
switches tried stops it. What could still be tried, each measured the
same way and none of them tried here: Windows' own diagnostic data
setting, and whether a policy of Edge's for its diagnostic data reaches
WebView2 (not known; a policy would need an administrator, so the
installer would set it). Until one is shown to stop it, the first start's
welcome and this README say it.

## The browser's keys and menu

**Switched off twice.** WebView2's own settings (`win/webview2.rs`):
`AreBrowserAcceleratorKeysEnabled` false, which Microsoft says turns off
the keys that reach the browser rather than the page (F5 and Ctrl+R,
Ctrl+P, Ctrl+F and F3, zoom, the developer tools, back and forward), and
`AreDefaultContextMenusEnabled` false. Every self check reads them back:
`accelerator keys Some(false), context menus Some(false), devtools
Some(false), zoom Some(false), status bar Some(false)`. And the page's own
guard (`../phone/app/src/console/pageGuard.ts`), for a WebView2 that would
not honour them: it stops those keys and the right click in the page, and
never touches a field's own keys (typing, Tab, Enter, copy, paste, undo,
select all).

**What the probe shows** (`--probe-keys`, `win/keys.rs`). Without taking
the window to the front, which would mix the builder's keys with the
owner's typing, the only way in is a message posted to WebView2's own
window. The probe posts a letter, Ctrl+P, Ctrl+F, a right click and F5,
one at a time, and writes down the windows of the console's processes and
the page loads after each. With everything on, and with the page's guard
off (WebView2's settings alone):

| Sent | Reached the page | Opened or reloaded |
| --- | --- | --- |
| a letter X | no | nothing |
| Ctrl+P | no (not even the Ctrl key) | nothing |
| Ctrl+F | no | nothing |
| a right click | yes (the page saw `contextmenu`) | no menu window |
| F5 | yes (the page saw `F5`) | no reload |

**Why that is not proof for the keys.** The control run (`--keys-on
--no-page-guard`: WebView2's keys left on, read back on, and no page
guard) gave the same: F5 reached the page and did **not** reload it.
WebView2 does not act on a posted key; only keys typed into the focused
window trigger its accelerators, and posted letters and Ctrl do not get
through at all. So the probe shows what reaches the page and that nothing
opens, and that F5, Ctrl+P and Ctrl+F do nothing rests on WebView2's own
setting, read back, and on the page's guard. The right click is the
nearest to proof: it reached the page as a real right click, and no menu
showed; the control leaves the right click out, because a menu would float
above every window on the PC for a moment.

**What a person checks** (a minute, in the owner's run): in the console's
window, press F5, Ctrl+R, Ctrl+P, Ctrl+F, F12, Ctrl+Shift+I, Ctrl and the
plus key, Alt and the left arrow, and right click the page and the field:
nothing may happen but typing in the field.

## Nothing of the build PC in the exe

Rust writes source paths into an exe for its panic messages, and spike 06's
exe held the build PC's user name 251 times. `tools\build-release.ps1`
builds with each of the build PC's folders remapped to a plain name (the
home folder to `home`, Cargo's to `cargo`, rustup's to `rustup`, the
repository to `limo`), passed in `CARGO_ENCODED_RUSTFLAGS` so a folder
with a space in it stays one flag, and then searches the exe for the user
name (as ASCII and as UTF-16, either byte alignment, any case), the home
folder, the PC's name and the repository's folder. It prints only the
counts, never the names, and fails on any that is not 0.

21 Nov 2026: all four 0, in an exe of 3,939,840 bytes (3.76 MiB); with
the cable in it, the same evening, all four 0 in 4,303,872 bytes (4.10
MiB). The control: the debug exe, built without the remapping, holds the user name
1,935 times in ASCII alone, so the search sees what it looks for.

## Tests

`cargo test --no-default-features`: 62 tests, no window and no page,
about nine seconds once built (21 Nov 2026, all passed; the answers for
the screens take most of it). They open named pipes and loopback sockets in the
test's own account, under names of their own
(`limo.console.test.<process>.<n>`), never LiMo's own door, and nothing
else. No test reaches a real phone or a real service.

**Adding a phone** (`phone_tests.rs`), through the six requests as the
screens ask them, against limo-cable's fake phone (its cable held as the
console holds the real one), a throwaway app (`test_apk.rs`: a zip with
Android's binary manifest, LiMo's package, version 0.3.0, a release
build), LiMo's key made once in a temporary folder, and the link's port
one the test's own listener got from Windows on 127.0.0.1, never 47810 and
never 8081:

- the whole of it, through the stand in door over a real pipe, signed in:
  each of the cable's drawn cases in its words (a cable that only charges,
  USB debugging off, no driver, a maker's driver), with nothing opened
  before there is a phone LiMo can open; a phone that has never seen this
  PC connected quietly and said to be `unknown`, with the fingerprint, the
  fake phone sent no public key however often the screens looked (the
  `AUTH` messages counted), one quiet connection and no more; asked, the
  phone asking once, a second ask refused as busy, not allowed in time and
  said so, not asked again by itself; asked again, allowed; connected,
  with the phone's name, Android and checks; pairing refused before LiMo
  is on it; LiMo put on, the bytes on the phone the app's own, and not
  again; the offer the door made handed to LiMo on the phone, the phone's
  port carried to the listener, LiMo started, and the door asked `devices`
  then `make_offer`; the phone's app reaching the listener through the
  cable both ways; paired once the door lists the device, over the cable,
  then over the home Wi-Fi; USB debugging off, and the strip saying `off`;
  the offer's secret in no answer;
- the screens never wait: while the phone asks, each look answers in under
  150 ms, and a step asked then is refused at once;
- an ask waits long: the screens looking for 2.5 s while the phone asks send
  it nothing, `asked_secs` counts, a late Allow still connects, the key
  shown once; the window's log has a line for the phone not knowing this PC
  and for the ask, with no fingerprint; an ask once connected refused;
- an ask ends when the phone is unplugged, and at `phone.done`, at once, not
  after its wait;
- an ask refused with no phone, with nothing plugged in, with arguments,
  and for a phone that knows this PC, which is never asked;
- LiMo at this PC's version not put on again; the service refusing an
  offer, in its own words, with nothing handed; asked again, paired;
- an older LiMo replaced; one of another maker refused with limo-cable's
  advice, and pairing refused until it is on;
- no phone, an unknown request, the app missing or not LiMo's, an Android
  too old (connected once, never again while it stays plugged in), another
  program holding the phone;
- unplugged while the pairing waits: still waiting, and once back the
  link's port carried again, the offer and the app never twice; then
  paired;
- an offer that runs out, said; asked again, a new offer handed;
- LiMo for Windows' own door: signed out, in its own words; signed in, with
  a child made through the door, no offer while its link for phones is not
  running, in its own words, and nothing reaching the phone; a page may not
  ask for an offer.

The window's own log (`console_log.rs`): nothing of an account written and well known identifiers kept; what a refused server was, logged with nothing of an account; the log keeping its last lines and never growing past its limit; a log with no place writing nothing. The answers for the screens (`phone_answers.rs`, see [The answers as files](#the-answers-as-files-for-the-screens-tests)): 46 files, the same in every run.

And in `phone/`: each drawn case's words, the fingerprint only while the
phone asks, the install's progress from the cable's count (`look.rs`); the
new device found and the ways read (`worker.rs`); the counting wire, and
the requests' names and arguments (`mod.rs`).

**Against the real door** (`real_door_tests.rs`). LiMo for Windows' own
door runs in the test's process (`support/real_door.rs`): its `RealDoor`
(the pipe and its threads, with the door's own access list), its
`Service` answering the door between steps on a quiet PC of the test's own
(no one signed in, no programs), and a real family's file in a temporary
folder made with `Hub::create_family`. Nothing is installed or started as a
service; nothing runs elevated. The console's `Door` asks it over the pipe:

- `about`, `family_state`, then `family_today` refused `signed_out`; a
  wrong password answered in the door's words with 4 tries left; a second
  password within a second answered `busy`; the right one signs in, and
  `family_today` is done; a request the door cannot read, and one with an
  argument it does not take, answered `unknown_request` by number, the
  connection staying signed in; the door's log saying who knocked (this
  process), and never the password;
- the test's own account linked to Asha: refused at hello in the door's
  words, the password never sent;
- the door checked by LiMo's own rule: refused for its session, and the
  door told of no console at all;
- the service gone: the next request lost, then, with the service back, a
  new connection, signed out;
- forty requests at once: those past the console's limit refused by the
  console, none by the door, and the connection still open;
- every frame the console writes read by the service's own reader
  (`ConsoleReader`), the requests by name and number, a request the
  service cannot read still read as one by its number; every message the
  service writes (`encode_to_console`) read by the console: hello, a
  refusal, done, a wait with its time, a wrong password with its tries.

**Against a stand in that misbehaves** (`door_tests.rs`, `stand_in.rs`, test
only; the stand in also answers `devices`, `make_offer` and `device_link`
from a family a test sets, once signed in, as the door does): hello first
and every request numbered on one connection; a refusal
at hello in the door's sentence, with only the hello sent; nothing sent to
a pipe that is not LiMo's service; a door that closes, sends nonsense,
answers another number, or says nothing, each a lost door; nothing serving
the door; a page asking too often; the console asking for no more than it
needs, and the door's list granting it.

| Where | What else |
| --- | --- |
| `protocol.rs` | a page's request on the wire as the service reads it, with no arguments when it has none; only a name of the right shape and an object forwarded; a page never asks for an offer, the console may; a password hidden in `Debug`; every string wiped at any depth; the door's answers read; a wrong version and a too long frame refused from the header; a message that is not the door's refused without quoting it; the longest message fits, one byte more does not |
| `door.rs` | LiMo's service passes the check, in any case and with `\\?\`; an unreadable program file leaves the session and the owner to decide, and never lets another session through; a pipe in a child's session refused first; another owner or program refused; every problem has plain words |
| `measure.rs` | a request line gives its host and never its path, credentials or anything else; the process tree holds descendants only; the host log on a real loopback port writes down a CONNECT and refuses it, and never takes port 8081 |
| `win/` | Windows errors and strings; this process's own account, session, program file and Program Files folder; a port read from network order; this process's own loopback connection seen with the right ports |
| `webview.rs` | every set of switches keeps Tauri's own first and has a name |

The page's own tests are in `../phone/app/tests/console.test.tsx` (`npm
test` there): the sign in against a host of the test's own (the door's
words shown as they come, a refusal with no field, an absent or lost
service said plainly), the reader of the door's replies, the `console`
place and "More info" sent to the one command, the page's key guard, and
what the console's entry reaches.

## Layout

```
Cargo.toml, Cargo.lock, build.rs, tauri.conf.json
capabilities/console.json   the window may call request, and nothing else
icons/icon.ico              the design's guardian icon on paper (examples/make_icon.rs)
src/
  main.rs                   the window, the one command, "More info", the self check's doors and key probe
  self_check.rs             the self check's log, steps, deadline and hard limit
  lib.rs                    the crate's parts, for the window, the tests and the examples
  protocol.rs               what crosses the door: frames, hello, a page's request forwarded and wiped
  door.rs                   the console's half: the server's check, the connection, the words
  phone/                    adding a phone: the six requests and the desk (mod.rs), the thread
                            that holds the cable (worker.rs), the look and its words (look.rs),
                            the real cable counting what it sends (counted.rs), the service
                            through the door (door_service.rs)
  webview.rs                WebView2's switches by name
  measure.rs                the process tree, its connections, the host log
  real_door_tests.rs        against LiMo for Windows' own door, in the test's process
  door_tests.rs, stand_in.rs  against a stand in that misbehaves (tests only)
  phone_tests.rs, test_apk.rs  adding a phone against the fake phone and a throwaway app (tests only)
  phone_answers.rs          the requests for a phone's answers, as files for the screens' tests (tests only)
  console_log.rs            the window's own log, with nothing of an account in it
  win/                      every call into Windows, and all the unsafe code
    pipe.rs  token.rs  net.rs  webview2.rs  keys.rs  shell.rs
support/
  real_door.rs              LiMo for Windows' own door served in a process, for the tests and the examples
examples/
  real_door.rs              the real door for the self check, in the builder's account
  add_phone.rs              a phone added from a terminal, with the real cable and the real door
  webview2_contacts.rs      the measurement of what WebView2 contacts
  make_icon.rs              paints icons/icon.ico from design/icons/svg/guardian.svg
tools/
  build-release.ps1         the page, the exe with the build PC's folders remapped, and the names check
  door.ps1                  starts and stops the real door for a self check
  run-self-check.ps1        one self check against the real door, and its log
  picture.ps1               a picture of the console's own window during a self check
```

Made by builds and git ignored: `target/` (with `target\self-check` and
`target\measure`), and tauri-build's `gen/` and
`permissions/autogenerated/`.
## Dependencies and licences

What ships in `limo-console.exe` (163 crates in its tree, 131 before the
cable, from `cargo tree -e normal,no-proc-macro`; the linker may drop
some):

| What | Version | Licence |
| --- | --- | --- |
| tauri, tauri-runtime-wry, wry | 2.12.7, 2.12.5, 0.57.2 | Apache-2.0 OR MIT |
| tao | 0.37.2 | Apache-2.0 |
| webview2-com, webview2-com-sys | 0.39.1 | MIT (named directly here, for WebView2's settings) |
| windows, windows-core, windows-sys | 0.62.2, 0.62.2, 0.61.2 | MIT OR Apache-2.0 |
| serde, serde_json | 1.0.229, 1.0.154 | MIT OR Apache-2.0 |
| zeroize | 1.9.2 | Apache-2.0 OR MIT |
| limo-cable | 0.1.0 | Apache-2.0 (this repository, `../phone/cable`) |
| rsa, md-5 (through limo-cable: LiMo's adb key, and its fingerprint) | 0.9.10, 0.10.6 | MIT OR Apache-2.0 |
| under them: num-bigint-dig, num-integer, num-iter, pkcs1, pkcs8, der, spki, const-oid, pem-rfc7468, base64ct, signature, rand 0.8, rand_chacha, rand_core 0.6, ppv-lite86, getrandom 0.2, lazy_static | | MIT OR Apache-2.0 |
| libm | 0.2.16 | MIT |
| zerocopy, zerocopy-derive | 0.8.64 | BSD-2-Clause OR Apache-2.0 OR MIT |
| brotli, brotli-decompressor, alloc-stdlib, alloc-no-stdlib | 9.1.3, 6.3.2, 0.3.0, 3.0.0 | BSD-3-Clause (brotli: BSD-3-Clause AND MIT) |
| ICU4X crates (icu_*, zerovec, yoke, tinystr, writeable, litemap, potential_utf, zerofrom, zerotrie) via url and idna | 2.4.0 and others | Unicode-3.0 |
| option-ext (via dirs) | 0.2.0 | MPL-2.0: file level, its source stays as published on crates.io |
| memchr, aho-corasick, walkdir, same-file, winapi-util | | Unlicense OR MIT |
| raw-window-handle | 0.6.2 | MIT OR Apache-2.0 OR Zlib |
| dunce | 1.0.5 | CC0-1.0 OR MIT-0 OR Apache-2.0 |
| dpi | 0.1.2 | Apache-2.0 AND MIT |
| every other crate in the tree | | MIT, Apache-2.0, or either |
| **Microsoft Edge WebView2 Runtime** | 158.0.4370.33 on this PC | **Microsoft's own licence, not open source**; part of Windows 11, not in the exe (spike 06's README, "WebView2", says what its terms ask) |
| The page: React, react-native-web, Expo, react-native-svg, react-native-safe-area-context | as `../phone/app` pins them | MIT |
| The three fonts | `../screens/fonts` | SIL Open Font License 1.1, each licence beside it in the page |

Every licence above lets LiMo ship under Apache-2.0; the WebView2 Runtime
is Microsoft's and comes with Windows, and its terms ask for notices.
`rsa` 0.9 carries the RustSec advisory RUSTSEC-2023-0071, a timing side
channel in private key operations; in LiMo the key signs one token a
connection, for a phone on the cable (limo-cable's README says the same;
moving to `rsa` 0.10 is a choice for later). miniz_oxide (MIT OR Zlib OR
Apache-2.0), which reads the app's manifest, and windows-sys were in the
tree already. No program of Google's ships with the console.

Build only: tauri-build 2.7.6 (Apache-2.0 OR MIT). For the tests and the
examples only, never in the exe: LiMo for Windows and the hub (this
repository, Apache-2.0, with what they bring, listed in their READMEs:
SQLite among it, public domain), tempfile 3.27 (MIT OR Apache-2.0), resvg
0.50.0 (Apache-2.0 OR MIT) and tiny-skia 0.12.0 (BSD-3-Clause), and
limo-cable's fake phone (its `fake` feature, Apache-2.0). No npm
package was added. No code was copied from GPL or AGPL projects; the
throwaway app's manifest is made as limo-cable's own tests make it.

## Left open

- **The person's run** (see [What a person checks](#what-a-person-checks)):
  the installed service, the child's account, the service stopped and
  started, and the keys in the focused window.
- **"More info" has no place on a screen yet.** The request works and the
  self check shows what it would open; its link waits for the About
  screen's drawing.
- **WebView2's contact over minutes.** Each measured run held the window
  for 50 seconds, since the builder's window must close within a minute;
  a guardian keeps the console open for minutes, and a browser uploads
  some of what it sends at intervals. To be measured for ten minutes with
  the owner, and the direct connection named then (an administrator can
  read Windows' DNS cache right after a run: `ipconfig /displaydns`).
- **Whatever remains is said to families**: in this README, in the first
  start's welcome, and in the third party notices (the WebView2 Runtime's
  terms ask for notices).
- **The sign in's drawing.** The screen is built from the console's
  description, ahead of its drawing; the approved drawing decides its
  look and its words. The window's icon is the design's guardian icon on
  paper until LiMo has its own mark.
- **Windows 10, a cold start after a restart, a PC without WebView2**: not
  tried.
- **The first run on a real phone** (see [A first run on a real
  phone](#a-first-run-on-a-real-phone)): the lead's. Nothing of adding a
  phone has reached a real phone or the installed service yet; the fake
  phone behaves as the test phone did on 21 Nov 2026.
- **A pairing to the end against the real link.** The tests pair against
  the stand in door; LiMo for Windows' own door, in a test, has no link for
  phones and says so, which a test shows. A door host with a link on a
  loopback port of the test's own, and a played phone that pairs through
  the fake phone's cable, would test the whole of it against LiMo for
  Windows' own code.
- **The install's progress needs a copy.** limo-cable's install says
  nothing until it ends, so the console opens the phone as `WindowsCable`
  does, with a wire that counts what it sends (`phone/counted.rs`). Were
  limo-cable to give the progress of an install (a callback, or a count of
  its own), that copy would go.
- **Connecting at A1.** The console connects to a phone with USB debugging
  on from the first look, before "Start", but quietly: nothing is shown to a
  phone that does not know this PC until the guardian asks at A3.
- **What LiMo for Windows offers the console's tests.** The tests and the
  self check run the real door with a quiet PC of their own
  (`support/real_door.rs`), which must implement every method of `Os`.
  LiMo for Windows is adding a door host for other crates' tests (its
  `door-host` feature, not committed when this was written); once it is in,
  the quiet PC can go.
- **The door's words** now write the typographic apostrophe ("LiMo’s"),
  as the app does, and the tests follow them; the console's own words for a
  phone do too, and limo-cable's are set the same way as they pass.