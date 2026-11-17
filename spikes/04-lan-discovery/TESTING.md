# Testing the LiMo LAN discovery spike

This checklist is for a person at the Windows PC, with the test phone
(Samsung Galaxy S20 Ultra, Android 13) on the same Wi-Fi and plugged into the
PC with a USB cable. It answers four questions: does the phone find the PC by
itself, can it then talk to it, what does Windows put in the way (the
firewall prompt, the Public network category, a firewall rule), and what does
the router do. Write what you see in the Results tables at the end.

Commands are typed in PowerShell, in this spike folder
(`LiMo\spikes\04-lan-discovery`), unless a step says otherwise. Three windows
help:

- window A: the PC program's `serve`, left running;
- window B: the phone's log, left running;
- window C: everything else.

Always run the same copy of the PC program,
`.\pc\target\release\limo-hub-spike.exe`. Windows ties firewall rules to the
program's full path, so a copy somewhere else is a different program to the
firewall.

`adb` is on the PATH of the build PC (it lives in
`%ANDROID_HOME%\platform-tools`).

## Before you start: safety

- `serve` stops by itself: after 10 minutes by default, never more than an
  hour. Ctrl+C in its window stops it at once and withdraws the announcement.
- The PC program changes nothing in Windows by itself. Only two steps ask you
  to change something by hand: a firewall rule (step 4.3) and the network
  category (step 4.4). Step 8 undoes both. Write down what you changed, so
  the clean up undoes exactly that.
- The spike has no pairing, no encryption and no secrets. While `serve`
  runs, anyone on the same network can see it and send it a HELLO. Run it on
  the home network only, never on a public one.
- `adb uninstall ai.lucidloom.limo.spike04` removes the app.

## Setup

1. Build both parts once, if not done already:
   ```
   cd pc
   cargo build --release
   cd ..\phone
   .\gradlew.bat assembleDebug
   cd ..
   ```
   The PC program is `pc\target\release\limo-hub-spike.exe`, the APK
   `phone\app\build\outputs\apk\debug\app-debug.apk`.

2. On the phone, turn on USB debugging if it is not on yet: Settings, About
   phone, Software information, tap Build number seven times; then Settings,
   Developer options, USB debugging on. Plug the phone in and accept the
   "Allow USB debugging?" prompt (tick "Always allow from this computer").

3. Check the connection:
   ```
   adb devices
   ```
   Pass: one line with a serial number followed by `device`. If it says
   `unauthorized`, unlock the phone and accept the prompt from step 2.

4. Make sure the phone is on the same Wi-Fi network as the PC (the same name,
   and not a guest network). Leave mobile data as it is, but write down
   whether it is on.

5. Install the app:
   ```
   adb install -r phone\app\build\outputs\apk\debug\app-debug.apk
   ```
   Pass: `Success`. Do not open the app yet.

6. In window B, start the phone's log and leave it running for the whole
   test:
   ```
   adb logcat -c
   adb logcat -s LiMoSpike04
   ```
   See "Reading the log over USB" below for saving it to a file.

7. Write down, for the Results table: on the phone, Settings, About phone,
   Software information: One UI version and Android version; on the PC,
   Settings, System, About: the Windows edition and version.

---

## Step 1: `status` on the PC

1.1. In window C:
```
.\pc\target\release\limo-hub-spike.exe status
```
It asks Windows through PowerShell and `netsh`, takes about 2 seconds, and
changes nothing.

Pass: `$LASTEXITCODE` is 0, and it prints, in this order:
- the first line, ending `status: read only, changes nothing`, and the
  program's full path;
- under "IPv4 addresses", a line for `"Wi-Fi"` with the PC's address, for
  example `192.168.68.200/22`, and `category=Public` (expected here); a line
  for `"Tailscale"` with its own `100.x.y.z` address; and the loopback line;
- under "Windows Firewall profiles", one line each for Domain, Private and
  Public;
- under "Firewall rules that name this program", `none` on a first run;
- under "TCP port 47810", `reserved by Windows: no` and
  `listening now: nothing`;
- three commands for an elevated PowerShell. Leave them on screen, or copy
  them somewhere: step 4 and step 8 use them.

Write down: the PC's Wi-Fi address (written below as `192.168.68.200`; use
your own), the Wi-Fi line's `category=`, the Public line's
`prompt_on_new_listener=`, and any rules listed.

If any rules are listed already (from an earlier try), Windows will not show
its prompt in step 3. To see the prompt as a first time user would, remove
them first: open PowerShell with "Run as administrator", paste the third
printed command ("Remove every rule that names this program"), then run
`status` again and check it says `none`.

If port 47810 is reserved by Windows, add `--port 0` to every `serve` below,
and use the port that `serve start` prints wherever 47810 appears.

---

## Step 2: loopback sanity on the PC, no network

This checks the program and the protocol on the PC alone. Nothing leaves the
PC and nothing is announced, so no firewall prompt is expected.

2.1. In window A:
```
.\pc\target\release\limo-hub-spike.exe serve --loopback --seconds 30
```

2.2. Within those 30 seconds, in window C:
```
.\pc\target\release\limo-hub-spike.exe hello --host 127.0.0.1 --port 47810
$LASTEXITCODE
```

Pass:
- window A printed `serve start version=0.1.0 id=XXXXXXXX port=47810
  loopback=yes stops_in_s=30 stops_at=HH:MM:SS`, then
  `listening on 127.0.0.1:47810 (loopback only)` and
  `mdns off (loopback only)`;
- window C printed one line `hello ok host=127.0.0.1 port=47810 rtt_ms=X
  connect_ms=Y reply="LIMO XXXXXXXX 0.1.0 <unix time> <nonce>"`, with the
  same id as the `serve start` line, and `$LASTEXITCODE` is 0;
- window A printed `connection peer=127.0.0.1:NNNNN received="HELLO <nonce>"
  sent="LIMO ..."`, with the same nonce;
- after 30 seconds window A printed `stop reason=safety_stop after_s=30`,
  `the safety stop ended serve after 30 s` and `serve end`, and returned to
  the prompt by itself;
- no firewall prompt appeared.

Write down: `rtt_ms`, and whether any prompt appeared.

---

## Step 3: the real thing, with Windows as it is

3.1. Make sure the app is not running:
```
adb shell am force-stop ai.lucidloom.limo.spike04
```

3.2. In window A, a normal PowerShell (NOT "Run as administrator"), start the
hub. 30 minutes is enough for steps 3 to 6; Ctrl+C ends it sooner:
```
.\pc\target\release\limo-hub-spike.exe serve --seconds 1800
```
Expected lines, each starting with the time:
- `serve start version=0.1.0 id=XXXXXXXX port=47810 loopback=no
  stops_in_s=1800 stops_at=HH:MM:SS`;
- one `listening on IP:47810 interface="..." prefix=/N` line per address,
  including `192.168.68.200:47810 interface="Wi-Fi" prefix=/22`;
- `mdns registered name="LiMo spike on <computer name>"
  type=_limo-spike._tcp.local. host=limo-spike-XXXXXXXX.local. port=47810
  txt="v=0.1.0 id=XXXXXXXX"`;
- `mdns socket ipv4 udp 5353 bound`;
- `mdns interface="Wi-Fi" index=N ipv4=192.168.68.200 joined`, and a line
  for each other interface;
- within about 2 seconds, `mdns announced ...` lines.

Write down: the id, whether the socket line says `bound` or `not bound`, and
whether the Wi-Fi interface line says `joined` or `join_failed`. Either
failure is a finding: copy the line exactly.

3.3. The firewall prompt. As `serve` starts listening, Windows may show a
"Windows Security Alert" window. Before touching it, write down (a photo of
the screen with the phone is easiest):
- its exact words, including the program name and path it shows;
- which network boxes it shows, and which of them are ticked;
- its buttons, and whether the main button has a shield on it (which means
  it will ask for an administrator's permission).

Then answer it the way an ordinary person would: leave the boxes exactly as
Windows ticked them and click the main button ("Allow access" or "Allow").
If Windows then asks for an administrator's permission, give it, and note
that it asked. If no prompt appeared within about 10 seconds, write "no
prompt". (Cancelling the prompt makes Windows add Block rules for the
program; step 4.3 deals with those if it happens by mistake.)

3.4. Start the app:
```
adb shell am start -n ai.lucidloom.limo.spike04/.MainActivity
```
It searches by itself for 30 seconds. Watch window B, window A and the phone
screen for about 90 seconds without touching the phone; the screen stays on
by itself while the app works.

Pass, the phone finds and reaches the PC:
- window B shows these lines (the first four may come in another order):
  `app start android=33 model="SM-G988..."`, `search started`,
  `wifi connected=yes address=192.168.68.x/22`,
  `search running after_ms=N`, then `found name="LiMo spike on <computer name>"
  after_ms=N`, `resolved name="..." host=192.168.68.200 port=47810
  id=XXXXXXXX after_ms=N`, `hello ok host=192.168.68.200 port=47810
  rtt_ms=X reply="LIMO XXXXXXXX 0.1.0 ..." connect_ms=Y run=found net=wifi`,
  then `repeat started ...`, `search stopped found=1 after_ms=N
  reason=time_up` when the 30 seconds are up, and, about 60 seconds after
  the first HELLO, `repeat done ok=12 failed=0 slowest_ms=X`;
- the resolved host is the PC's Wi-Fi address from step 1, never the
  Tailscale `100.x.y.z` one, and the id is the one from `serve start`;
- window A shows `mdns answered a query via="Wi-Fi"` (this is the first
  search since `serve` started, so it should appear), and one
  `connection peer=192.168.68.x:NNNNN received="HELLO ..." sent="LIMO ..."`
  line per HELLO, with the phone's address;
- the phone shows the hub's name, address, port and id, "found after N ms,
  resolved after N ms", and "HELLO ok in X ms".

Write down: what the phone shows (found or not, HELLO ok or not, or the
error words), `found ... after_ms`, `resolved ... after_ms`, `rtt_ms`,
`connect_ms`, `net=`, whether `mdns answered a query` appeared in window A
and its `via=`, and the phone's own Wi-Fi address from the app's top line.

3.5. In window C, while `serve` keeps running:
```
.\pc\target\release\limo-hub-spike.exe status
```
Write down every line under "Firewall rules that name this program": these
are the rules Windows made from your answer to the prompt (name, action,
profile, protocol). "TCP port 47810" should now say
`listening now: 0.0.0.0:47810 pid=N program=limo-hub-spike`.

If the phone found the PC and the HELLO worked, step 4 is not needed: go on
to step 5. Otherwise do step 4.

---

## Step 4: if the phone cannot find or reach the PC

`serve` keeps running in window A. If it has stopped, start it again as in
3.2.

4.1. Tell discovery apart from the connection. Send a HELLO straight to the
PC's address, with no discovery (use your PC's Wi-Fi address):
```
adb shell am start -n ai.lucidloom.limo.spike04/.MainActivity --es host 192.168.68.200 --ei port 47810
```
Window B shows `intent host="192.168.68.200" port="47810"`, then either
`hello ok host=192.168.68.200 port=47810 rtt_ms=X reply="LIMO ..." ...
run=direct net=wifi` or `hello failed host=192.168.68.200 port=47810
error="..." run=direct net=...`. The phone shows the same under "Direct
HELLO". The address and port can also be typed into the two fields on the
phone, then "Send HELLO to this address".

How to read steps 3 and 4.1 together:

| Search (step 3) | Direct HELLO (4.1) | Most likely meaning |
| --- | --- | --- |
| found and resolved, HELLO failed | failed | Discovery works; the TCP connection is blocked, most likely by Windows Firewall. |
| not found | ok | TCP gets through; mDNS does not. The phone's query, or the PC's answer, is blocked: the firewall on UDP port 5353 for this program, or the router's handling of multicast. |
| not found | failed | Nothing gets through: the firewall blocks this program entirely, or the router keeps devices apart (step 7). |

The error words help too. "timed out while connecting (3 s)" means nothing
answered: packets dropped without a word, which is what Windows Firewall
does. "connection refused" means the PC answered but nothing listens on that
port: `serve` is not running, or the port is wrong.

Write down: the direct HELLO's result and its error words, if any.

4.2. Which way mDNS is blocked (about a minute). Stop `serve` with Ctrl+C in
window A. On the phone tap "Search again" (or, from the PC, run the
`force-stop` and `am start` commands from 3.1 and 3.4: a fresh start
searches by itself). Within 5 seconds of that, start `serve` again in window
A as in 3.2. Now the phone is already listening when the PC announces
itself, so only the PC's own outgoing announcement has to get through.

Write down: found or not, resolved or not, HELLO ok or not. If the phone
finds the PC this way but not in step 3, the PC's announcements go out but
the phone's queries do not reach the program: inbound UDP port 5353 is
blocked for it. If window A never shows `mdns answered a query` while the
phone searches, that says the same.

4.3. The firewall rule, for the Public profile. In window C run `status`
again and look under "Firewall rules that name this program".

1. Open a second PowerShell with "Run as administrator" (Start, type
   PowerShell, right click, Run as administrator).
2. Only if `status` lists any rule with `action=Block`: paste the third
   command `status` printed ("Remove every rule that names this program").
   A Block rule wins over any Allow rule, so it must go first. The rules
   are already written down from 3.5.
3. Paste the first command `status` printed ("Add an allow rule"). It is
   one long line starting `New-NetFirewallRule -DisplayName 'LiMo spike 04
   hub'`. The rule is for the Public profile, which is the one in force on
   this Wi-Fi (it names the Private profile as well), for every protocol, so
   both TCP port 47810 and mDNS on UDP port 5353, and only for devices on
   the PC's own subnet.
4. Keep the administrator window open for step 4.4 and step 8.

In window C, `status` again. Pass: under "Firewall rules that name this
program", `"LiMo spike 04 hub" action=Allow direction=Inbound
profile=Private, Public protocol=Any local_port=Any enabled=yes`, and no
Block rule.

Stop `serve` with Ctrl+C and start it again as in 3.2, so nothing Windows
decided before the rule is still in force. Then on the phone tap "Search
again" and watch as in 3.4. If it is not found, try the direct HELLO from
4.1 again.

Write down: found or not, HELLO ok or not (search and direct), `rtt_ms`.

4.4. A separate check: what changes on a Private network. This changes one
thing only, the network's category, so first take the rule from 4.3 away
again, if you added it. In the administrator window:
```
Remove-NetFirewallRule -DisplayName 'LiMo spike 04 hub'
```
(the second command `status` printed). Then switch the Wi-Fi network to
Private: Settings, "Network & internet", Wi-Fi, click the network's name
("<name> properties"), and under "Network profile type" choose "Private
network". (Or, in the administrator window, with the Wi-Fi line's `index=`
from `status`:
`Set-NetConnectionProfile -InterfaceIndex 16 -NetworkCategory Private`.)

In window C, `status`. Pass: the Wi-Fi line says `category=Private`. Write
down the rules listed now.

Stop `serve` with Ctrl+C and start it again as in 3.2, so Windows sees it
start listening on a Private network. If a firewall prompt appears, write
down its words and ticked boxes, and answer it as in 3.3. Then on the phone
tap "Search again", and if it is not found, try the direct HELLO from 4.1.

Write down: any prompt, found or not, HELLO ok or not.

Switch it back to Public at once: the same Settings page, "Public network
(Recommended)" (or `Set-NetConnectionProfile -InterfaceIndex 16
-NetworkCategory Public` in the administrator window). In window C,
`status`. Pass: the Wi-Fi line says `category=Public` again.

---

## Step 5: `browse` from the PC itself, while `serve` runs

5.1. With `serve` still running in window A, in window C:
```
.\pc\target\release\limo-hub-spike.exe browse
$LASTEXITCODE
```
It looks for 10 seconds.

Pass:
- `search started type=_limo-spike._tcp.local. for_s=10`;
- `found name="LiMo spike on <computer name>" after_ms=N`;
- `resolved name="..." host=limo-spike-XXXXXXXX.local. addresses=...
  port=47810 id=XXXXXXXX txt="v=0.1.0 id=XXXXXXXX" after_ms=N`, with the id
  from `serve start`, and the Wi-Fi address among the addresses;
- one `hello ok host=IP port=47810 rtt_ms=X connect_ms=Y reply="LIMO ..."`
  line per IPv4 address listed;
- `search done found=1 hello_ok=N hello_failed=0 after_s=10`, and
  `$LASTEXITCODE` is 0;
- window A shows a `connection peer=...` line for each HELLO.

The Tailscale address may be listed here too: the answer reaches `browse`
over the PC's own Tailscale interface, so that is fine. Only the phone must
never get it.

Note: everything here stays inside the PC, and Windows Firewall normally
does not filter traffic between programs on the same PC. So `browse` can pass
when the phone fails. A pass here shows that the program works, not that the
network does. If `browse` fails while `serve` runs, that is a finding in
itself: copy its lines exactly.

Write down: the addresses listed, `found` and `resolved` `after_ms`, each
`rtt_ms`, and any firewall prompt.

---

## Step 6: stability

6.1. The 60 second repeat. It starts by itself after the first HELLO of each
search, sends one HELLO every 5 seconds, 12 times, and ends with
`repeat done ok=N failed=N slowest_ms=X` in window B; the phone shows
"Repeat: done: ok N, failed N, slowest X ms". If step 3 or step 4 already
gave a `repeat done` line with the network as it is now, use it. Otherwise
tap "Search again" and leave the phone alone for 90 seconds.

Pass: `repeat done ok=12 failed=0`. Write down `slowest_ms`. A
`repeat stopped ... reason=app_hidden` line means the screen went off or the
app was left during the minute: run it again.

6.2. Screen off for two minutes. With `serve` running and the app open,
press the phone's Side key to turn the screen off (or, from the PC,
`adb shell input keyevent KEYCODE_SLEEP`). Window B shows
`search stopped ... reason=app_hidden` and `repeat stopped ...
reason=app_hidden` for whichever was still running; it may show neither.
Wait two minutes by the clock. Wake the phone (Side key, or
`adb shell input keyevent KEYCODE_WAKEUP`) and unlock it by hand. The app
comes back but does not search by itself: tap "Search again".

Pass: `search started`, `found ...`, `resolved ...` with the same address,
port and id as before, `hello ok ... run=found net=wifi`, and about 60
seconds later `repeat done ok=12 failed=0 slowest_ms=X`.

Write down: `found` and `resolved` `after_ms` next to the ones from step 3,
whether window A showed `mdns answered a query` this time, and the repeat's
result. A much faster `found` with no `mdns answered a query` line is
expected: the phone may remember the answer from earlier (see README.md,
"`serve` output").

---

## Step 7: what the router does

Many routers have a setting called "AP isolation", "client isolation",
"wireless isolation" or "allow devices to see each other". When isolation is
on, each device on the Wi-Fi can reach the internet but not the other
devices. Guest networks usually have it on, and sometimes the main network
does too. Mesh systems (several router units around the home) also pass
multicast, which mDNS uses, from one unit to the next, which can make
discovery slow or patchy when the phone and the PC are joined to different
units.

With isolation on, the results would look like this, whatever the firewall
rule and the network category:
- the phone finds nothing: `search stopped found=0 ... reason=time_up`;
- window A never shows `mdns answered a query` while the phone searches, and
  never a `connection peer=` line with the phone's address;
- the direct HELLO from 4.1 fails with "timed out while connecting (3 s)", or
  "no route to that address";
- `browse` from the PC itself (step 5) still passes, since it never leaves
  the PC.

So if steps 3, 4.1 and 4.3 all fail in this way, look at the router before
anything else.

7.1. In the router's app or admin page, look for those settings and write
down: the router's make and model, whether it is a mesh system, whether the
PC and the phone are on the main network or a guest one, whether isolation
is on for that network, and, if the app shows it, which unit each device is
joined to. Change nothing here for this test.

7.2. Optional, if the router has a guest network and you want to see the
isolated case for real: join only the phone to the guest network, tap
"Search again", then run the direct HELLO from 4.1. Expected: not found, and
the direct HELLO fails. Then join the phone back to the main Wi-Fi.

---

## Step 8: cleaning up

8.1. The goodbye. On the phone tap "Search again" and wait until it shows
the hub again. Then, while the search still runs (the phone shows "Search:
running, N s of 30"), press Ctrl+C in window A. Pass: window A prints
`stop reason=ctrl_c after_s=N`, `mdns withdrawn name="LiMo spike on
<computer name>"` and `serve end`; window B shows `lost name="LiMo spike on
<computer name>" after_ms=N` within a few seconds. If the phone never found
the PC in this test, just stop `serve` and write "n/a".

8.2. Save the phone's log now (see "Reading the log over USB" below), before
the app is removed.

8.3. The firewall. In the administrator PowerShell, paste the third command
`status` printed ("Remove every rule that names this program"). It removes
the rule from 4.3, if it is still there, and the rules Windows made from its
prompt in step 3, so nothing of this spike stays in the firewall. Then, in
window C, `status`. Pass: "Firewall rules that name this program" says
`none`.

8.4. The network category. If it was changed in 4.4 and not switched back,
switch it back now as at the end of 4.4. `status` must show the Wi-Fi line
with `category=Public`, as at the start.

8.5. The app:
```
adb uninstall ai.lucidloom.limo.spike04
```
Pass: `Success`. Stop the log in window B with Ctrl+C. Turn off USB debugging
(Settings, Developer options) if the phone is not used for testing again
soon. Close the administrator PowerShell.

---

## Reading the log over USB

Live, in window B:
```
adb logcat -s LiMoSpike04
```

Only the outcomes that matter here, from what the phone has kept:
```
adb logcat -d -s LiMoSpike04 | Select-String "found|resolved|hello|repeat|search"
```

Save everything to a file to attach to the results:
```
adb logcat -d -v time -s LiMoSpike04 | Out-File -Encoding utf8 limo-spike04-log.txt
```
The phone keeps only a limited log, so save it soon after the steps that
matter. The app's screen also shows the last 80 lines, newest first, until it
is closed. Every log line is explained in `README.md`, "Log lines". The PC
program prints only to its window: keep window A open until its numbers are
written down.

---

## Results

### The setup

| Item | Value |
| --- | --- |
| Date of the test | |
| PC: Windows edition and version | |
| PC: Wi-Fi address and prefix (step 1) | |
| PC: Wi-Fi category at the start (step 1) | |
| Phone: model, Android version, One UI version | |
| Phone: Wi-Fi address (app's top line) | |
| Phone: mobile data on or off | |
| Router: make and model, mesh or not | |
| Both on the main network, not a guest one | |

### The checks

| Step | Check | Expected | Seen |
| --- | --- | --- | --- |
| 1 | `status`: Wi-Fi category | Public | |
| 1 | `status`: Public `prompt_on_new_listener` | yes | |
| 1 | `status`: rules naming this program | none | |
| 1 | `status`: port 47810 reserved by Windows | no | |
| 2 | Loopback `hello ok`, same id as `serve start` | yes | |
| 2 | Loopback `rtt_ms` | note it | |
| 2 | Firewall prompt for loopback | none | |
| 2 | Safety stop after 30 s | yes | |
| 3 | `mdns socket ipv4 udp 5353 bound` | yes | |
| 3 | `mdns interface="Wi-Fi" ... joined` | yes | |
| 3 | Firewall prompt: exact words | note them | |
| 3 | Prompt: boxes shown, and which were ticked | note them | |
| 3 | Prompt: asked for an administrator's permission | note it | |
| 3 | Rules Windows made from the answer (3.5) | note them | |
| 3 | Phone: found, and `after_ms` | note it | |
| 3 | Phone: resolved, `after_ms`, and host | the PC's Wi-Fi address | |
| 3 | Phone: HELLO ok, `rtt_ms`, `connect_ms`, `net=` | note it (`net=wifi`) | |
| 3 | Window A: `mdns answered a query`, and `via=` | `via="Wi-Fi"` | |
| 4.1 | Direct HELLO (no discovery) | note it | |
| 4.2 | Found when the phone was already searching as `serve` started | note it | |
| 4.3 | With the allow rule: found, HELLO ok | note it | |
| 4.4 | On a Private network: any prompt, found, HELLO ok | note it | |
| 4.4 | Switched back to Public (`status`) | yes | |
| 5 | `browse`: found, addresses listed | note them | |
| 5 | `browse`: every HELLO ok, exit code 0 | yes | |
| 6.1 | Repeat: ok, failed, `slowest_ms` | 12, 0, note it | |
| 6.2 | After two minutes with the screen off, "Search again": found, HELLO ok | yes | |
| 6.2 | `found after_ms` then, next to step 3's | note both | |
| 6.2 | Repeat after the screen off: ok, failed, `slowest_ms` | 12, 0, note it | |
| 7 | Router isolation setting for this network | note it | |
| 7 | Guest network result (optional 7.2) | not found, direct fails | |
| 8.1 | `mdns withdrawn` on the PC, `lost` on the phone | yes | |
| 8.3 | `status` after clean up: rules naming this program | none | |
| 8.4 | `status` after clean up: Wi-Fi category | Public | |
| 8.5 | App uninstalled | yes | |

What was changed in Windows during the test, and undone in step 8 (rule
added, category changed): ______.

Anything Windows showed that this checklist did not expect: ______.
