# limo-console

LiMo's guardian console on the master PC: `limo-console.exe`, one window
in which a guardian sees the family's day and changes its rules. The
window shows the guardian form's own React Native screens from
[`../phone/app`](../phone/app/), exported for the web and drawn by
WebView2 in a Tauri v2 window. It holds no state of its own: every action
goes to LiMo's service through **the door**, a named pipe that only the
service makes, and the service alone holds the family's file.

**What it is now.** A first shell, kept, with one screen: the sign in. It
proves on this PC, on 21 Nov 2026, that the chosen way holds: the app's
own screen in the window, opening in about half a second; the door's two
halves in miniature, over real pipes; what WebView2 still contacts on its
own, measured for each switch; the browser's keys and menu switched off;
and a release exe that holds nothing of the PC that built it. Each is
said below with what was measured.

**What it is not.** It has never talked to LiMo's service: the service's
half of the door is not built yet, so a stand in plays it (`stand_in`).
It has one screen, built ahead of its drawing. No Add a phone, no
installer, no signing. It has run only in the self check, on the
development PC: a normal window, not focused, not on top, that closes
itself within a minute.

## Contents

- [Building](#building)
- [What was proven](#what-was-proven)
- [The window](#the-window)
- [The door](#the-door)
- [The self check](#the-self-check)
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
cargo test --no-default-features                                   # the tests: no window, no page, seconds
powershell -ExecutionPolicy Bypass -File tools\build-release.ps1   # the page, the exe, and the check for the build PC's names
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
feature `window` the crate is the door's two halves, the switches and the
measurement's parts, and builds in seconds.

Logs, pictures and measurements go to `target\self-check` and
`target\measure`, which git ignores.

## What was proven

The five things the console's technology had to show before it is built
on, each on this PC (Windows 11 Home, WebView2 Runtime 158.0.4370.33):

| | | Result |
| --- | --- | --- |
| 1 | The app's own sign in screen, from its tokens, fonts and icons, exported for the web with a `console` place in the host seam, in a Tauri v2 window, opening in under half a second | **Proven for a later start, barely**: drawn with its three fonts 399 to 498 ms after the process started (median 481 ms); a first start with a new WebView2 data folder 510 ms (see [Opening time](#opening-time)) |
| 2 | The door's two halves in miniature: the access list and the check of who is on the other end, a child's account refused, one round trip for any other; the console checking the server before it sends a password | **Proven in this account**: 32 tests over real pipes and the self check; the service's own half, as LocalSystem in session 0, is the owner's run |
| 3 | What WebView2 still contacts with each switch | **Measured: fewer, not none.** `--disable-background-networking` removes its one request through its own network stack (`config.edge.skype.com`); with every switch, every later start still opens one HTTPS connection to a Microsoft address from WebView2's browser process, about 2.5 s in, past any proxy (see [What WebView2 contacts](#what-webview2-contacts)) |
| 4 | F5, Ctrl+P, Ctrl+F and the right click menu do nothing in the release window | **Partly**: WebView2's own settings for them are off and read back off, the page stops the keys itself, and a right click and F5 reach the page and open or reload nothing; a real key press needs the window in front, which the builder may not take, so one minute of a person is still owed |
| 5 | A release exe with no user name of the build PC | **Proven**: 0 times for the user name, the home folder, the PC's name and the repository's folder (the unremapped debug exe holds the user name 1,935 times) |

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
- **What a request does.** The page calls `request(name, args)`. Rust turns
  it into a typed `Request` (`protocol.rs`); a name the console does not
  know is refused before anything is sent. The password travels on as a
  wiped `Secret`. Rust asks the door and hands back its answer, or, when
  the door cannot be asked, `{ code, words }` with the sentence the screen
  shows.
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

`protocol.rs`, `door.rs`, `stand_in.rs`, `win/pipe.rs`, `win/token.rs`.

**What crosses it.** The named pipe `\\.\pipe\limo.console`. Each message
is a frame, as on LiMo for Windows' helper channel: the protocol's version
(2 bytes, now 1), the length (4 bytes), both little endian, then that many
bytes of JSON; a frame over 1 MiB, or of another version, is refused from
its header alone. Requests now: `about` and `sign_in { password }`.
Answers: `about`, `signed_in`, `wrong_password { tries_before_wait }`,
`must_wait { until }`, `refused { why: child_account { child } |
unknown_account }`. No error about a frame quotes any part of it.

**The console's half** (`door::Door`). Any program can make a named pipe,
a child's too, and could make `limo.console` first. So before it writes
anything the console asks Windows who serves the pipe it reached, and
checks three things a program in a child's account cannot fake:

1. the serving process runs in session 0, where only services run
   (`GetNamedPipeServerSessionId`);
2. the pipe belongs to LocalSystem (`S-1-5-18`), the account the service
   runs as (its owner, from its security descriptor);
3. the serving process's program file is `limo.exe` in LiMo's folder under
   Windows' own Program Files folder (`GetNamedPipeServerProcessId`, then
   the image path; the folder from `SHGetKnownFolderPath`, never from an
   environment variable).

If one fails the console closes the pipe having written nothing, and the
screen says: "Something that is not LiMo's service answered, so the
console sent it nothing. Your password was not sent." The console opens
the pipe asking only for what a client needs (read, write data, write
attributes) and lends its token at the **identification** level, so
whatever serves the pipe can read who the guardian is and can never act as
the guardian. It keeps one connection, since a sign in belongs to a
connection, and drops it after a refusal or any fault.

**The service's half, in miniature** (`stand_in::StandIn`,
`StandInServer`). What the service's door will do, run in the builder's
own account:

- **The access list** `D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x12018b;;;IU)`:
  LocalSystem and Administrators full control; interactive users read,
  write data and write attributes; nobody else. Remote clients are refused
  by the pipe itself (`PIPE_REJECT_REMOTE_CLIENTS`), and the first instance
  is made with `FILE_FLAG_FIRST_PIPE_INSTANCE`, so a pipe someone made
  first under that name is never served beside.
- **Not "GW" for interactive users.** For a pipe, Windows' generic write
  right includes `FILE_APPEND_DATA`, which on a pipe is
  `FILE_CREATE_PIPE_INSTANCE`: the right to serve the pipe too. With the
  list that looks right, `(A;;GRGW;;;IU)`, a program in the test's
  (interactive, not elevated) account made a second instance of the door
  beside the first; with `0x12018b` Windows refused it (access denied). A
  child's program with the right to serve the door could answer the next
  console that knocks. The client then asks for exactly `0x12018b`, never
  `GENERIC_WRITE`, which this list refuses (also tested).
- **Who is on the other end.** After the first bytes, the door reads the
  client's process id from the pipe (`GetNamedPipeClientProcessId`) and
  that process's token's account, and the account of the token the client
  lent the pipe (`ImpersonateNamedPipeClient`, read, and back at once;
  the process ends if it cannot go back). Only when both name the same
  account is it known; a client that lends no token is refused as unknown.
- **A child's account is refused** whatever it asks, in the screen's
  words: "LiMo's settings cannot be opened from Asha's account. A guardian
  opens them from their own." Its tries are never counted, and the
  connection is closed.
- **The password's waits** as the rules core counts them: four wrong tries
  free ("3 more tries before a wait"), a minute's wait after the fifth,
  doubling, at most an hour; a right password inside a wait still waits.

What only the owner's run can show: the real door made by the service as
LocalSystem in session 0, opened from the guardian's account and refused
from the child's. One question for it: whether a standard guardian account
may read the image path of a LocalSystem process. If Windows refuses, the
console refuses the real service on the third check; the first two would
still hold alone.

## The self check

`limo-console.exe --self-check <log>` (`src/self_check.rs`, and the page's
driver in `../phone/app/src/console/selfCheck.ts`). The exe makes two
stand in doors in the builder's own account, one where the account is a
guardian's and one where it is marked as Asha's, with a password new each
run. The page then signs in as a guardian would, by the DOM: it types into
the field with the input's own value setter and presses the button with a
click, through the same handlers, `host.request` and Rust command a
guardian's typing reaches. Steps, each written to the log with the
milliseconds since the process started:

1. `js_start`;
2. `sign_in_drawn`: the field and the button on screen, the three fonts
   loaded, and what the browser drew with (the heading in "Fraunces 28" at
   28 px, the field 48 high on card paper, the button marigold);
3. `wrong_password`: "That password is not right. 3 more tries before a
   wait.", the field empty again;
4. `signed_in`: the right password, the signed in card;
5. `ready_for_keys` and `keys` (see [The browser's keys and
   menu](#the-browsers-keys-and-menu));
6. `child_account`: the door that marks the account as Asha's; the page
   reloads and shows the refusal, with no password field;
7. `not_the_service`: the guardian's door again, checked with LiMo's own
   rule (session 0, LocalSystem, `Program Files\LiMo\limo.exe`); the page
   reloads, the screen says the password was not sent, and the stand in
   shows that connection wrote nothing.

Then it writes `done` and exits with 0 after `--hold-ms`. A step missing
after 25 s writes which and exits with 1; a window closed early exits with
1; whatever happens the process ends before 57 s. Outside self check mode
none of it is reachable: the page's driver starts only when the exe put
its mark on the page, and Rust refuses every self check request without a
self check. Options, only with `--self-check`: `--hold-ms`, `--switches`,
`--proxy`, `--webview-data`, `--size`, `--probe-keys`, `--keys-on`,
`--no-page-guard`.

`tools\picture.ps1` takes a picture of the console's own window during a
self check (`PrintWindow`, the window alone, nothing else on the screen),
at 1100 by 760 and at 390 by 844: `target\self-check\sign-in-*.png`.

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

21 Nov 2026: all four 0, in an exe of 3,939,840 bytes (3.76 MiB). The
control: the debug exe, built without the remapping, holds the user name
1,935 times in ASCII alone, so the search sees what it looks for.

## Tests

`cargo test --no-default-features`: 32 tests, no window and no page, in
under a second once built. They open named pipes and loopback sockets in
the test's own account, under names of their own, and nothing else.

| Where | What |
| --- | --- |
| `protocol.rs` | frames whole and in pieces; the wire as the service will read it; a password hidden in `Debug`; a page's request by name, and only one that exists; a wrong version and a too long frame refused from the header; a malformed message refused without quoting it; the longest message fits, one byte more does not |
| `door.rs` | LiMo's service passes the check, in any case and with `\\?\`; a pipe in a child's session refused first; another owner, another program, an unreadable one refused; every problem has plain words |
| `stand_in.rs` | a guardian signs in, a wrong try says how many are left; the fifth starts a wait that even the right password cannot pass, then doubles; a child's account refused whatever it asks, its tries not counted; an account that cannot be told refused; secrets compared whole |
| `door_tests.rs` (real pipes) | a console signs in through a real pipe: one connection, the requests in order, the client known by process and lent token, lent at the identification level; a child's account refused and the connection closed; nothing written to a door that is not LiMo's service, each of the three checks alone; nothing serving the door said plainly; the access list read back as written; a second server beside the door refused with the door's list and allowed with "GW"; a client asking for generic write refused; four hostile frames each closing the connection without a word, and a good request after them |
| `measure.rs` | a request line gives its host and never its path, credentials or anything else; the process tree holds descendants only; the host log on a real loopback port writes down a CONNECT and refuses it, and never takes port 8081 |
| `win/` | Windows errors and strings; this process's own account, session, program file and Program Files folder; a port read from network order; this process's own loopback connection seen with the right ports |
| `webview.rs` | every set of switches keeps Tauri's own first and has a name |

The page's own tests are in `../phone/app/tests/console.test.tsx` (`npm
test` there): the sign in against a host of the test's own, the reader of
the door's answers, the `console` place, the page's key guard, and what
the console's entry reaches.

## Layout

```
Cargo.toml, Cargo.lock, build.rs, tauri.conf.json
capabilities/console.json   the window may call request, and nothing else
icons/icon.ico              the design's guardian icon on paper (examples/make_icon.rs)
src/
  main.rs                   the window, the one command, the self check's doors and key probe
  self_check.rs             the self check's log, steps, deadline and hard limit
  lib.rs                    the crate's parts, for the window, the tests and the examples
  protocol.rs               what crosses the door, and its frames
  door.rs                   the console's half: the server's check, the words, the connection
  stand_in.rs               the service's half in miniature: access list, client check, refusal, waits
  door_tests.rs             both halves over real pipes
  webview.rs                WebView2's switches by name
  measure.rs                the process tree, its connections, the host log
  win/                      every call into Windows, and all the unsafe code
    pipe.rs  token.rs  net.rs  webview2.rs  keys.rs
examples/
  webview2_contacts.rs      the measurement of what WebView2 contacts
  make_icon.rs              paints icons/icon.ico from design/icons/svg/guardian.svg
tools/
  build-release.ps1         the page, the exe with the build PC's folders remapped, and the names check
  run-self-check.ps1        one self check, and its log
  picture.ps1               a picture of the console's own window during a self check
```

Made by builds and git ignored: `target/` (with `target\self-check` and
`target\measure`), and tauri-build's `gen/` and
`permissions/autogenerated/`.

## Dependencies and licences

What ships in `limo-console.exe` (131 crates in its tree, from `cargo tree
-e normal,no-proc-macro`; the linker may drop some):

| What | Version | Licence |
| --- | --- | --- |
| tauri, tauri-runtime-wry, wry | 2.12.7, 2.12.5, 0.57.2 | Apache-2.0 OR MIT |
| tao | 0.37.2 | Apache-2.0 |
| webview2-com, webview2-com-sys | 0.39.1 | MIT (named directly here, for WebView2's settings) |
| windows, windows-core, windows-sys | 0.62.2, 0.62.2, 0.61.2 | MIT OR Apache-2.0 |
| serde, serde_json | 1.0.229, 1.0.154 | MIT OR Apache-2.0 |
| zeroize | 1.9.2 | Apache-2.0 OR MIT |
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

Build only: tauri-build 2.7.6 (Apache-2.0 OR MIT). For `make_icon` only,
never in the exe: resvg 0.50.0 (Apache-2.0 OR MIT) and tiny-skia 0.12.0
(BSD-3-Clause). No npm package was added. No code was copied from GPL or
AGPL projects.

## Left open

- **The service's half of the door**, in LiMo for Windows, as LocalSystem
  in session 0, with the access list and the client check above; then the
  owner's run: the door opened from the guardian's account and refused from
  the child's, and whether a standard guardian account may read the image
  path of a LocalSystem process (if not, the third check needs another
  way, and the first two hold alone meanwhile).
- **A person's minute with the keys and the menu** in the real window (see
  [The browser's keys and menu](#the-browsers-keys-and-menu)).
- **WebView2's contact over minutes.** Each measured run held the window
  for 50 seconds, since the builder's window must close within a minute;
  a guardian keeps the console open for minutes, and a browser uploads
  some of what it sends at intervals. To be measured for ten minutes with
  the owner, and the direct connection named then (an administrator can
  read Windows' DNS cache right after a run: `ipconfig /displaydns`).
- **Whatever remains is said to families**, as the decision of 18 Nov
  says: in this README, in the first start's welcome, and in the third
  party notices (the WebView2 Runtime's terms ask for notices).
- **The sign in's drawing.** The screen is built from the console's
  description, ahead of its drawing; the approved drawing decides its
  look and its words. The window's icon is the design's guardian icon on
  paper until LiMo has its own mark.
- **What the door keeps per connection**: the sign in, and signing out by
  itself after 15 minutes with no input, belong to the service's half.
- **The console's normal mode has not been opened**, since it would stay
  open until someone closes it. It differs from the self check only in
  its door (`Door::limo()`, LiMo's own rule, which the tests and the self
  check's last step use) and its window's size and focus.
- **Windows 10, a cold start after a restart, a PC without WebView2**: not
  tried.
