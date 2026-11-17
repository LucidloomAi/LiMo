# LiMo spike 04: find the master PC on the home network

A small, throwaway proof for the LiMo screen time limiter. It is not the real
product. It answers one question before the real build starts, and then it
can be removed:

LiMo has no server. One Windows PC in the home is the master, and the phones
talk only to it, only on the home Wi-Fi. Can a phone find that PC on the home
network by itself, and talk to it? And what do Windows and Android put in
the way?

There are two parts: `pc\`, a Rust program for the PC (`limo-hub-spike.exe`),
and `phone\`, a plain Kotlin Android app ("LiMo spike 04"). The test is a
checklist in `TESTING.md`, for a person at the PC with the test phone
(Samsung Galaxy S20 Ultra, Android 13) on the same Wi-Fi.

## What it proves, and what it does not

Once `TESTING.md` has been worked through, it shows:

1. Whether the phone finds the PC with mDNS and DNS-SD, using only Android's
   own `NsdManager`, and how long that takes: from the start of the search to
   "found", and to "resolved" (address, port and TXT records known).
2. Whether the phone can then open a TCP connection to the PC and get a reply,
   and the round trip time.
3. What Windows puts in the way: the firewall prompt (its exact words and
   boxes), the network's category (this PC's Wi-Fi is marked Public), and
   which firewall rule, if any, is needed.
4. Whether a failure is in discovery or in the connection: the app and the PC
   program can both skip discovery and send a HELLO straight to an address.
5. Whether it stays working: one HELLO every 5 seconds for a minute, and again
   after the phone's screen was off for two minutes.
6. What the router does, for example client isolation on a guest network.

It does not:

- encrypt anything, pair anything or hold any secret. Anyone on the same
  network can find this spike, read its id and version, and send it a HELLO.
  That is acceptable only because it is a test that runs for at most an hour
  at a time and carries nothing of value. The real product pairs each phone
  with the PC by scanning a QR code, and encrypts everything after that;
- use IPv6. The PC listens and announces on IPv4 only;
- say anything about other phones, other Android versions or other routers;
- remember the PC's address, or reconnect after the PC's address changes;
- measure battery use;
- change anything in Windows. It never adds a firewall rule and never changes
  a network's category; `TESTING.md` has a person do that, by hand, in an
  elevated terminal, when a step calls for it.

## How it works

### Discovery

The PC program announces itself with mDNS and DNS-SD, using the `mdns-sd`
crate:

| Item | Value |
| --- | --- |
| Service type | `_limo-spike._tcp.local.` |
| Instance name | `--name`, default `LiMo spike on <computer name>` |
| Host name | `limo-spike-<id>.local.` |
| Port | the TCP port it listens on, default 47810 |
| TXT records | `v=<crate version>` (now `0.1.0`), `id=<id>` |

The id is 8 random lowercase hex characters, made each time `serve` starts.
The host name is the spike's own, never the computer name, so it cannot clash
with the name Windows itself answers for over mDNS.

The responder runs on UDP port 5353, which Windows' own mDNS service (the DNS
Client service) also uses; `mdns-sd` opens it with address reuse so both can
share it. It joins the mDNS multicast group on every interface with an IPv4
address. IPv6 is turned off in the responder, because the TCP listener is
IPv4 only. On each interface it answers with that interface's own address:
the phone, on Wi-Fi, gets the Wi-Fi address and never, say, the Tailscale
one. It announces itself when it starts, answers each matching query, and on
the way out sends a "goodbye" (the same records with a time to live of 0) so
phones drop it at once.

The phone uses `NsdManager.discoverServices("_limo-spike._tcp")`. Each
instance found is resolved with `NsdManager.resolveService`, which gives its
address, port and TXT records, and then gets a HELLO.

### The hello

One line each way, over a plain TCP connection:

| Who | Sends |
| --- | --- |
| client | `HELLO <nonce>\n` |
| PC | `LIMO <id> <version> <unix time> <nonce>\n`, then closes |
| PC, to anything else | `ERR\n`, then closes |

- The nonce is 1 to 64 characters, each an ASCII letter, a digit, `-` or `_`.
  The clients send 8 random hex characters and check that the reply carries
  the same nonce, so a reply cannot be an old or stray one.
- One carriage return before the line end is allowed (`\r\n`).
- A line, counting its line end, is at most 256 bytes. The PC never reads more
  than 256 bytes from a connection: if none of them is a line end, it answers
  `ERR`.
- The PC gives each connection 5 seconds in all to send its line, then
  answers `ERR` and closes. At most 8 connections are handled at once; more
  are closed at once. So a stray connection cannot hang the PC program or make
  it grow.
- Example: `HELLO 1a2b3c4d` gets `LIMO ab12cd34 0.1.0 1795435326 1a2b3c4d`.

### What the times mean

- `after_ms` on `found` and `resolved` (phone and `browse`): milliseconds from
  the start of the search (the call to `discoverServices`, or the start of
  `browse`) to the instance being found, and to it being resolved.
- `rtt_ms`: from the start of connecting to the reply's line end, so it
  includes the TCP handshake. `connect_ms` is the handshake alone. Both have
  one decimal.

## The PC program: `limo-hub-spike.exe`

| Command | What it does |
| --- | --- |
| `serve [--port N] [--name TEXT] [--seconds N] [--loopback]` | Listens on TCP (default port 47810; `--port 0` lets Windows choose) on all IPv4 interfaces, announces the service, answers HELLOs. Ends by itself after `--seconds` (default 600, 1 to 3600) and on Ctrl+C, and withdraws the announcement on the way out. `--loopback` listens on 127.0.0.1 only and announces nothing. |
| `browse [--seconds N]` | Looks for `_limo-spike._tcp.local.` for N seconds (default 10, 1 to 120), prints each instance with its addresses, port and TXT records, and sends a HELLO to each of its IPv4 addresses. Exit code 0 if at least one HELLO was answered properly. |
| `hello --host H --port P` | Sends one HELLO straight to H:P, no discovery. Exit code 0 if it was answered properly. |
| `status` | Read only. See below. |

Exit codes: 0 success, 1 an operation failed, 2 bad arguments. Values out of
range (for example `--seconds 4000`) are refused, not clamped.

It needs no administrator rights, installs nothing, runs no service and writes
no files. It only prints.

### `serve` output

One line per event, each starting with the local time (`HH:MM:SS.mmm`):

| Line | Meaning |
| --- | --- |
| `serve start version=... id=... port=... loopback=... stops_in_s=... stops_at=...` | Started; when the safety stop ends it. |
| `listening on IP:PORT interface="..." prefix=/N` | One line per IPv4 address of this PC (for `0.0.0.0`). |
| `mdns registered name="..." type=... host=... port=... txt="v=... id=..."` | The service was handed to the responder. |
| `mdns socket ipv4 udp 5353 bound` | The responder's socket opened (or `not bound:` and why). |
| `mdns interface="..." index=N ipv4=... joined` | The responder joined the mDNS group on that interface (or `join_failed=` and why). |
| `mdns announced name="..." detail="host:interface"` | An announcement went out on that interface. |
| `mdns answered a query via="..."` | A query for this service reached the program and it answered. When the phone searches for the first time after `serve` started, this line should appear; if it never does, the phone's query never reached the program. A later search may get no line even though it works: a query that already lists the answer (the phone remembers it for up to 75 minutes) gets no answer, and the same records are sent at most once a second. |
| `connection peer=IP:PORT received="..." sent="..."` | One per TCP connection: what came in (or what happened instead, such as `(no line end within 5 s)`) and what was sent back. Bytes that are not printable ASCII show as `\xNN`. |
| `stop reason=ctrl_c` or `stop reason=safety_stop`, `after_s=...` | Why it is stopping. |
| `mdns withdrawn name="..."` | The goodbye went out. |
| `serve end` | Done. |

### `status`

Prints, changing nothing and needing no elevation:

- each IPv4 address of this PC with its interface, and the category Windows
  reports for that network (Public, Private, or DomainAuthenticated);
- each Windows Firewall profile as in force now: whether it is on, the default
  for inbound traffic, whether allow rules apply, and whether Windows shows a
  prompt when a new program starts listening; and any third party firewall
  registered with Windows Security;
- every firewall rule that names this program (and any that name a program of
  the same file name at another path), with its action, direction, profile,
  protocol and port;
- whether TCP port 47810 is inside a range Windows keeps for itself (Hyper-V
  and WSL can reserve port ranges; `serve` then fails on that port), and what
  is listening on it now;
- three commands for an elevated PowerShell, which it never runs itself: add
  an allow rule for this program on the Private and Public profiles, limited
  to the local subnet; remove that rule; and remove every rule that names this
  program, including the ones Windows makes when someone answers its prompt.

It reads through a PowerShell child process (`Get-NetConnectionProfile`,
`Get-NetFirewallProfile`, `Get-NetFirewallApplicationFilter`,
`Get-NetFirewallRule`, `Get-NetFirewallPortFilter`, `Get-CimInstance` for the
Windows Security list, `Get-NetTCPConnection`) and
`netsh interface ipv4 show excludedportrange protocol=tcp`. It takes about 2
seconds.

## The phone app: "LiMo spike 04"

Application id `ai.lucidloom.limo.spike04`. One screen, plain framework views,
no third party libraries.

### What it does

- On start it shows whether Wi-Fi is connected and the phone's own IPv4
  address on it, and starts a search.
- A search runs for 30 seconds, then stops by itself. Each instance found is
  listed with its name, address, port and TXT id, and the milliseconds from
  the start of the search to "found" and to "resolved". Each resolved instance
  gets a HELLO; the reply (or the error, in plain words) and the round trip
  time are shown.
- After the first hub has had its HELLO, the app repeats the HELLO to it every
  5 seconds, 12 times (60 seconds), with no tapping, and shows how many
  succeeded, how many failed, and the slowest. It starts whether the first
  HELLO worked or not.
- "Search again" stops the search and the repeat if they are running, clears
  the list and starts a fresh search.
- The direct path, with no discovery: type the PC's address and port and tap
  "Send HELLO to this address", or start the app with extras:
  ```
  adb shell am start -n ai.lucidloom.limo.spike04/.MainActivity --es host 192.168.68.200 --ei port 47810
  ```
  Started this way, it skips the automatic search ("Search again" still
  works) and sends that HELLO as soon as it knows the Wi-Fi state (within
  1.5 seconds), so the socket can be bound to Wi-Fi. If the app is already
  open, the extras still send the HELLO, at once. `--es port 47810` works too.
- When the app goes to the background or the screen turns off, the search and
  the repeat stop (each leaves a line saying so). The app does not search
  again by itself when it comes back: tap "Search again".
- While a search or the repeat runs, the app keeps the screen on, so a short
  screen timeout cannot cut a run short. Afterwards the screen behaves as
  usual.
- Turning the phone does not restart anything.

### Log lines

All lines use the tag `LiMoSpike04`, and every outcome writes one. Read them
with `adb logcat -s LiMoSpike04`. The screen shows the same lines, newest
first.

| Line | Meaning |
| --- | --- |
| `app start android=N model="..."` | The app started. |
| `wifi connected=yes address=IP/N` or `wifi connected=no` | Wi-Fi state, logged when it changes. |
| `search started` | The search was asked for; `after_ms` counts from here. |
| `search running after_ms=N` | Android says the search is running. |
| `search failed error="..."` | Android could not start the search. |
| `search stopped found=N after_ms=N reason=R` | The search ended: `time_up` (30 s), `search_again`, `app_hidden`, or `system` (Android stopped it unasked). |
| `search skipped reason=direct_extras` | Started with host and port extras. |
| `found name="..." after_ms=N` | An instance was found. `found again` if it is found a second time in the same search. |
| `lost name="..." after_ms=N` | Android reports it gone (the PC withdrew it). |
| `resolved name="..." host=IP port=N id=ID after_ms=N` | Address, port and TXT id known. |
| `resolve failed name="..." error="..."` | The resolve failed, timed out (10 s) or was skipped. |
| `hello ok host=IP port=N rtt_ms=X reply="LIMO ..." connect_ms=Y run=R net=W` | A HELLO was answered properly. `run` is `found` (after a resolve), `direct`, or `repeat_N` (N of 12). `net` is `wifi` if the socket was bound to the Wi-Fi network, else `default`. |
| `hello failed host=IP port=N error="..." run=R net=W` | A HELLO failed; the error is in plain words, with the exception's own text in brackets. |
| `repeat started host=IP port=N every_ms=5000 count=12` | The stability repeat began. |
| `repeat done ok=N failed=N slowest_ms=X` | The 12 are done. `slowest_ms` is the slowest success, or `none`. |
| `repeat stopped ok=N failed=N slowest_ms=X reason=R` | Stopped early: `search_again` or `app_hidden`. |
| `intent host="..." port="..."` | Extras received. |

### Permissions, and why

Both are granted at install, with no prompt.

- `INTERNET`: an app needs it to open any socket, so the HELLO needs it.
- `ACCESS_NETWORK_STATE`: to find the Wi-Fi network with `ConnectivityManager`,
  read the phone's own address from it, and bind the HELLO's socket to it.

Not asked for:

- `ACCESS_WIFI_STATE`: nothing here reads `WifiManager`. The Wi-Fi state and
  the phone's address come from `ConnectivityManager`.
- `CHANGE_WIFI_MULTICAST_STATE`: that is for holding a Wi-Fi multicast lock,
  which an app needs when it receives multicast itself. Here the multicast
  work is done by Android's own discovery service, in the system's process,
  not by the app. No test has shown the lock to be needed, so it is left out.
  If the phone never finds the PC while the PC logs `mdns answered a query`
  and the direct path works, the answer is reaching the phone's network but
  not the discovery service; a multicast lock is the next thing to try, in a
  new build.
- No location permission: the network's name (which needs it) is never read.

### NsdManager's quirks, and what the app does about them

- A listener object may not be used again while it is registered. Every search
  gets a new discovery listener and every resolve a new resolve listener.
- Before Android 14 only one resolve may run at a time; a second one fails
  with code 3 ("already active"). Resolves are queued and run one at a time.
  If code 3 comes back anyway, that resolve is tried again after 1 second, at
  most 3 times in all, and each failure is logged.
- A resolve that never calls back would stall the queue, so after 10 seconds
  it is given up, logged as timed out, and the next one runs.
- On older versions callbacks arrive on NsdManager's own thread. Each is
  passed to the main thread, which owns all state; the network work (the
  HELLO) runs on one background thread, one HELLO at a time.
- Callbacks of an earlier search can still arrive after "Search again". Each
  search has a number and the app tells them apart; late ones are logged as
  ignored.
- `resolveService` is deprecated from Android 14, in favour of
  `registerServiceInfoCallback`. The test phone has Android 13, where it is
  the only way, so the spike uses it on every version (the deprecation warning
  is suppressed at that one call). On Android 14 and later the app reads all
  of the resolved addresses (`hostAddresses`) and greets the first IPv4 one;
  on Android 13 there is just one (`host`). The real product should use the
  newer call on Android 14 and later.
- Android's guide says not to keep searching in the background, so the search
  stops when the app is hidden.

### The HELLO's socket

When the Wi-Fi network is known, the socket is created from that network
(`Network.getSocketFactory()`), so the HELLO goes over Wi-Fi even if Android
has made mobile data the default network (which it can do when it thinks the
Wi-Fi has no internet). The log line says which: `net=wifi` or
`net=default`. Connecting may take 3 seconds and the whole exchange 4, so a
HELLO always ends before the next one in the repeat is due. These are the
phone's limits; the PC program's own client (`hello`, `browse`) allows 3 and
5.

## Build

### PC

Needs Rust (built with 1.98.1, MSVC). In `pc\`:

```
cargo build --release
cargo test
```

The program is `pc\target\release\limo-hub-spike.exe`. The unit tests (41)
cover the pure parts: reading and answering protocol lines (good, malformed,
oversized, cut short, timed out), checking replies, the argument checks, the
safety stop arithmetic, quoting, base64 for PowerShell, parsing the status
output and the excluded port ranges, and the printed commands.

### Phone

Needs JDK 17 and the Android SDK with platform `android-36` and build tools
35.0.0 or later (already on the build PC). In `phone\`:

```
.\gradlew.bat assembleDebug
.\gradlew.bat testDebugUnitTest
```

The APK is `phone\app\build\outputs\apk\debug\app-debug.apk`. The unit tests
(18) cover the pure parts: checking the reply line, nonces, the time format,
ports and hosts from the text fields, the TXT id, the NsdManager error words,
the log line shapes, and the repeat's count.

Versions: Gradle 8.14.3 (wrapper), Android Gradle Plugin 8.11.0, Kotlin
2.1.20, compileSdk and targetSdk 36, minSdk 26.

`local.properties` is not needed when `ANDROID_HOME` is set, as it is on the
build PC. To make it by hand, put one line in a new file `local.properties` in
`phone\`, with your own SDK path, for example
`sdk.dir=C\:\\Users\\you\\AppData\\Local\\Android\\Sdk`. It is listed in
`.gitignore` and must never be committed.

## Tools and libraries

The spike's own code is Apache-2.0. Nothing is copied from any other project.

PC program, everything compiled into `limo-hub-spike.exe`:

| Crate | Version | Licence | Why |
| --- | --- | --- | --- |
| mdns-sd | 0.22.0 | Apache-2.0 OR MIT | mDNS and DNS-SD responder and browser (default features off) |
| if-addrs | 0.15.0 | MIT OR BSD-3-Clause | this PC's interface addresses |
| ctrlc | 3.5.2 | MIT OR Apache-2.0 | Ctrl+C handling |
| fastrand | 2.5.0 | Apache-2.0 OR MIT | the random id and nonce |
| flume | 0.12.0 | Apache-2.0 OR MIT | used by mdns-sd |
| mio | 1.2.5 | MIT | used by mdns-sd |
| socket-pktinfo | 0.4.2 | MIT | used by mdns-sd |
| socket2 | 0.6.5 | MIT OR Apache-2.0 | used by mdns-sd |
| spin | 0.9.9 | MIT | used by flume |
| lock_api | 0.4.14 | MIT OR Apache-2.0 | used by spin |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 | used by lock_api |
| log | 0.4.35 | MIT OR Apache-2.0 | used by the crates above |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 | Windows API bindings, used by the crates above |
| windows-link | 0.2.1 | MIT OR Apache-2.0 | used by windows-sys |

`Cargo.lock` also names crates that only other systems use (libc, nix,
cfg-if, cfg_aliases, bitflags, objc2, objc2-encode, block2, dispatch2, wasi).
None of them is compiled on Windows, so none is in the program.

Built with Rust 1.98.1 and the Visual Studio 2022 C++ build tools. The local
time in the event lines comes from the Windows function `GetLocalTime`,
declared in the spike's own code.

Phone app:

| Part | Version | Licence | Where |
| --- | --- | --- | --- |
| Kotlin standard library | 2.1.20 | Apache-2.0 | in the APK |
| JetBrains annotations | 13.0 | Apache-2.0 | in the APK (comes with the standard library) |
| JUnit | 4.13.2 | EPL-1.0 | unit tests only, not in the APK |
| Hamcrest core | 1.3 | BSD-3-Clause | unit tests only (comes with JUnit), not in the APK |
| Gradle (wrapper) | 8.14.3 | Apache-2.0 | build only |
| Android Gradle Plugin | 8.11.0 | Apache-2.0 | build only |
| Kotlin compiler and Gradle plugin | 2.1.20 | Apache-2.0 | build only |

The Gradle wrapper files (`gradlew`, `gradlew.bat`, `gradle\wrapper\`) are
Gradle's own, copied unchanged from spike 02.

## First run on a real home network, 17 Nov 2026

The PC (Windows 11 Home, Wi-Fi at 192.168.68.200/22, the network marked
Public in Windows, a Tailscale interface also present) and the phone (Galaxy
S20 Ultra, SM-G988B, Android 13, Wi-Fi at 192.168.68.51/22) were on the same
home Wi-Fi. `serve` was started from a normal terminal, not elevated. The
phone was driven over USB.

What Windows did:

- The moment `serve` started listening, Windows added two inbound Block rules
  named after the program, for the Private and Public profiles, and showed its
  firewall prompt.
- The owner read the prompt on a second run: the boxes for Private networks
  and Public networks were both ticked. The owner clicked Allow.
- Windows then turned the two rules into Allow rules: Private and Public
  profiles, TCP and UDP, every port, any remote address. That is much wider
  than this program needs.
- Windows' own mDNS service already uses UDP port 5353. The program shared
  the port with it without trouble and joined the multicast group on Wi-Fi,
  Tailscale and loopback.

What the phone did, with the Allow rules in place:

| Check | Result |
| --- | --- |
| Search, first run | found after 233 ms, resolved after 251 ms, to 192.168.68.200 (the Wi-Fi address, not the Tailscale one) |
| First HELLO | ok, 103 ms |
| Repeat, 12 HELLOs in 60 s | 12 ok, 0 failed, slowest 57 ms |
| After the phone's screen was off for 2 minutes | found after 174 ms, HELLO ok in 12 ms |
| Direct path, no discovery | HELLO ok in 12 ms, `net=wifi` |
| `browse` from the PC itself | found after 87 ms, HELLO ok on all three of its addresses |
| The PC program stopping (its safety stop) | the phone logged `lost` within a second of the announcement being withdrawn |

After the program had stopped, the phone's HELLOs failed with a 3 second
connect timeout, not with "connection refused": with nothing listening,
Windows drops the connection attempt silently. So from the failure alone a
phone cannot tell "the program is not running" from "a firewall is in the
way". The withdrawn announcement is what tells it.

Not seen in this run: what the phone gets while the Block rules are still in
place, because the owner answered the prompt within seconds both times; a
network where the router keeps devices apart; and a prompt answered with
Cancel.

What this means for the real product:

- Finding the master PC over mDNS works on an ordinary home network, on a
  network Windows calls Public, once the firewall allows the program.
- The installer runs as an administrator anyway, so it should add its own
  narrow rule (this program, its one TCP port and UDP 5353, the local subnet
  only, both profiles) and the family never sees the prompt. Leaving it to
  the prompt needs an administrator at the PC at the right moment and ends
  with a rule far wider than needed.
- The uninstaller must remove that rule.

Afterwards both test apps were removed from the phone. The four rules Windows
made (two for the program, two for a renamed copy used to see the prompt a
second time) need an administrator to remove:

```
Get-NetFirewallApplicationFilter | Where-Object Program -like '*limo-hub-spike*' | Get-NetFirewallRule | Remove-NetFirewallRule
```

## Unknowns

- Whether Windows shows its firewall prompt at all, which boxes it ticks by
  default on a Public network, and what rules it makes from the answer.
  Earlier prompts on this PC left inbound rules named after the program, one
  for TCP and one for UDP, mostly Allow on the Public profile, and a few
  Block. `status` shows exactly what this program gets.
- Whether the prompt also covers UDP port 5353 for this program, or only the
  TCP port. Windows' built in mDNS rule covers its own DNS Client service, not
  other programs.
- Whether the responder and Windows' own mDNS service share UDP port 5353
  cleanly on this PC. The `mdns socket` and `mdns interface` lines show it.
- Which mDNS code the phone runs. Android 13 shipped with the older
  `mdnsresponder`; Google can replace it with a newer one through system
  updates of the connectivity module. They differ in timing.
- Whether the phone's Wi-Fi drops multicast while the screen is off (many
  phones filter it to save power). The screen off step in `TESTING.md` looks
  at the effect after the screen comes back on, not during.
- Stale entries. If `serve` ends without its goodbye (the window closed, the
  PC slept), phones may still list it for a while: `mdns-sd` gives the
  address records 2 minutes and the service records 75 minutes.
- The router. A network of this size (a /22) is common with mesh Wi-Fi
  systems, whose nodes pass multicast between them; how quickly and how
  reliably is up to the router. Client isolation, often on for guest
  networks, blocks phone to PC traffic entirely.
