# Testing the LiMo service and lock spike

This checklist is for a person at the PC. Part A is a safe developer check.
Part B runs as an administrator. Part U, also as the administrator, checks that
the lock stays above other always on top apps. Part C is the real test, signed
in as a standard account. Parts B, U and C are done twice, once for each of
the helper modes `user` and `system`. Part G then tries the third mode,
`guarded`, from your own account.

Important: while the lock is up the keyboard does nothing. That is the point
of the test, and it is why every step below is either a one line command, or
something done with the mouse, or handled by the `probe` command, which makes
its attempts unattended.

## Before you start: safety

- Start with short locks. A lock can never last more than 120 seconds. That is
  built into the code twice, so the worst case is waiting 120 seconds.
- The mouse keeps working during a lock.
- Ctrl+Alt+Del always works. From that screen, Sign out always gets you out.
- `uninstall` removes everything.

## Setup

1. Build once, in the spike folder:
   ```
   cargo build --release
   ```
   The binary is `target\release\limo-spike.exe`. The commands below are run
   from the `target\release` folder unless a step says otherwise.

2. The test needs a standard (non administrator) Windows account on this PC.
   To make one on Windows 11 Home: Settings, Accounts, Other users, Add
   account, then "I don't have this person's sign-in information", then "Add a
   user without a Microsoft account". Give it a name and password. A new
   account is standard by default.

3. Two monitors should be attached, so you can see that both get covered.

---

## Part A: safe developer check (no administrator needed)

A1. List monitors:
```
.\limo-spike monitors
```
Pass: one line per monitor with its rectangle, the primary one marked
`primary=true`, then `count: 2`. No window appears.

A2. Show the overlay and press nothing:
```
.\limo-spike overlay --seconds 8
```
Pass: every monitor shows a dark screen reading "LiMo spike: locked" with
"Unlocks in N s" counting down. Typing does nothing. After 8 seconds it goes
away by itself, it prints the report, two lines, the first with
`mode=developer`, and `$LASTEXITCODE` is 0. The first line is expected to say
`uiaccess=0` and band 1 for each window: only a helper started by the service
gets UIAccess. For the same
reason an always on top app may still show over this developer overlay; that
is expected here, and Part U checks the real lock.

A3. Show the overlay and use the escape chord:
```
.\limo-spike overlay --seconds 20
```
While it is up press Ctrl+Shift+F12. Pass: it closes at once, well before 20
seconds, and `$LASTEXITCODE` is 3.

A4. Check the band reading code. This shows nothing on screen:
```
.\limo-spike bandcheck
```
Pass: two lines. The first ends
`uiaccess=0 integrity=0x2000 (medium) windows=1 band[1]=1 (normal desktop band)`.
The second starts `process_sddl=O:` followed by your own SID, and ends with
the label `S:AI(ML;;NWNR;;;ME)`. No window appears, and `$LASTEXITCODE` is 0.
That is from a normal terminal; an administrator terminal says
`integrity=0x3000 (high)` and a different descriptor instead. If the band is
any other number, or unknown, or the second line says `unknown`, write the
lines down exactly.

---

## Part B: administrator account (first pass: helper mode `user`)

Sign in as the administrator. Open PowerShell with "Run as administrator" and
go to the build folder (`target\release`).

B1. Install:
```
.\limo-spike install --helper-as user
```
Pass: it reports copying the binary, preparing the data folder, setting the
helper mode, and creating and starting the service.

B2. Check it:
```
.\limo-spike status
```
Pass: service `Running`, helper mode `user`, lock `cleared`, a start line in
the log.

B3. A short lock that ends by itself:
```
.\limo-spike lock --seconds 20
```
Pass: within about a second both monitors are covered, the countdown runs, and
after 20 seconds the overlay goes away by itself.

B4. The early unlock. The keyboard is dead during the lock, so this must be one
line:
```
.\limo-spike lock --seconds 60; Start-Sleep 10; .\limo-spike unlock
```
Pass: the overlay goes away after about 10 seconds, not 60.

B5. Optional, checks the deadline rule. Write a deadline far in the future by
hand, then clear it after 5 seconds, all in one line:
```
Set-Content "$env:ProgramData\LiMoSpike\control.txt" 9999999999; Start-Sleep 5; .\limo-spike unlock
```
Pass: no overlay appears at all, and the log (B6) has one line starting
"ignoring deadline 9999999999". If an overlay does appear, the rule is broken:
the line clears it after 5 seconds; note it as a failure.

B6. Read the log:
```
.\limo-spike status
```
Pass: the log shows a helper launch with a pid, a session id and `uiaccess=1`
for each lock, a helper exit with code 0 when each lock ended, and the B5 line
if you ran it.

---

## Part U: the lock stays above other always on top apps (first pass: helper mode `user`)

The first real run found a hole: an ordinary always on top app (the owner uses
a desktop ticker) kept showing over the lock. The helper now runs with
UIAccess, which should put the lock above every such app. Stay signed in as
the administrator, in the administrator PowerShell in the build folder
(`target\release`).

U1. Start any always on top app, such as the desktop ticker, and leave it
running where you can see it. If the spike is not installed, install it:
```
.\limo-spike install --helper-as user
```

U2. A short lock. Watch the other app for the whole time; the lock ends by
itself after 20 seconds:
```
.\limo-spike lock --seconds 20
```

U3. Read the log and the helper report:
```
.\limo-spike status
```

Pass:
- the other app does not show over the lock at any moment, not even as a
  flicker;
- the "helper launch" line in the log for this lock says `uiaccess=1`;
- under "helper report:", the report written at the time of this lock says
  `mode=managed` and `uiaccess=1`, with `band[N]=2 (UIAccess band)` for every
  window. In `user` mode it is the one in your temp folder; in `system` mode
  it is the one in the data folder. Ignore an older report with another time,
  such as the one from A2, which says `mode=developer`.

If the log has a line starting "could not set UIAccess on the helper token",
or the report says `uiaccess=0`, a band other than 2, or an unknown band,
write the line down exactly: that is a failure.

Repeat Part U in `system` mode during the second pass.

---

## Part C: signed in as the STANDARD account (first pass: helper mode `user`)

Sign out of the administrator account (Start, your name, Sign out) and sign in
as the standard account. The service keeps running; it does not need the
administrator to be signed in.

Open a normal PowerShell (not as administrator). Commands that need no
elevation use the installed copy, for example:
```
& "$env:ProgramFiles\LiMoSpike\limo-spike.exe" status
```

Run the probe from a copy in your own temp folder. If it ran from the installed
exe, attempt e (delete the installed exe) would be refused simply because the
file is in use, which proves nothing. Make the copy once:
```
Copy-Item "$env:ProgramFiles\LiMoSpike\limo-spike.exe" "$env:TEMP\limo-probe.exe"
```

C1. No lock. Run:
```
& "$env:TEMP\limo-probe.exe" probe --after 0
```
Then:
```
& "$env:ProgramFiles\LiMoSpike\limo-spike.exe" status
```
Pass: every attempt (a to e) is REFUSED, the summary reads
`6 refused, 0 succeeded, 0 not applicable`, and `status` still shows the
service `Running`. Attempt b lists the service process; its line may say the
image path was unreadable, which is fine as long as it is REFUSED. Because
ending it was refused, b has a second line for the service process,
"b. open pid N to rewrite its access list", which must be REFUSED too: that
is the right a determined user would need to hand themselves the right to end
it. The probe only opens and closes; it never changes anything.

C2. Under a lock.

1. Open a second PowerShell with "Run as administrator". Windows asks for the
   administrator's password. In it, type this but do NOT press Enter yet:
   ```
   & "$env:ProgramFiles\LiMoSpike\limo-spike.exe" lock --seconds 60
   ```
2. In the normal terminal start the probe:
   ```
   & "$env:TEMP\limo-probe.exe" probe --after 20
   ```
3. Click into the administrator terminal and press Enter, within about 10
   seconds of starting the probe. The lock starts, and the probe makes its
   attempts about 20 seconds after you started it, while the screen is locked.
   The report's first lines show the time it ran; if the lock had not started
   by then, the helper line will be missing, so run C2 again.
4. While locked, try each of these by hand and note what happens: Alt+Tab, the
   Windows key, Ctrl+Shift+Esc, Alt+F4. Then press Ctrl+Alt+Del, open Task
   Manager from that screen, and with the mouse try End task on each
   `limo-spike.exe`. The helper now runs with UIAccess (Part U), so the lock
   may cover Task Manager as well, even with Task Manager's Always on top
   turned on. If Task Manager cannot be seen over the lock, End task cannot be
   tried by mouse: write "hidden" in that row of the table. Probe attempt b
   makes the same check either way.
5. When the lock ends, read the report and the status:
   ```
   Get-Content "$env:TEMP\limo-spike-probe.txt"
   & "$env:ProgramFiles\LiMoSpike\limo-spike.exe" status
   ```

Expected in `user` mode:
- a, stop the service: REFUSED.
- b, end the service process: REFUSED, and open it to rewrite its access
  list: REFUSED.
- b, end the helper process: SUCCEEDED, and the overlay comes back within about
  a second. The log shows "helper exited ... code=1" followed by "relaunched
  helper ... uiaccess=1 integrity=unchanged dacl=unchanged gap_ms=N". The
  helper was ended, so there is no access list line for it.
- c, d and e: REFUSED.
- Summary: `6 refused, 1 succeeded, 0 not applicable`.
- Alt+Tab, the Windows key, Ctrl+Shift+Esc and Alt+F4 do nothing.
  Ctrl+Alt+Del does show its screen (nothing can block that). End task on the
  helper in Task Manager works, and the overlay comes back within about a
  second.

---

## Second pass: helper mode `system`

Sign out of the standard account and sign back in as the administrator. In an
administrator PowerShell in the build folder (`target\release`):
```
.\limo-spike uninstall
.\limo-spike install --helper-as system
```
Uninstall must run from the build folder copy; the installed copy refuses to
uninstall itself. Then repeat Part B (B1 is the install you just did), Part U
and Part C. In Part C, make the temp copy again, because the installed exe is
new.

Expected in `system` mode: everything is refused, including ending the helper
and opening it to rewrite its access list. The C1 summary reads
`6 refused, 0 succeeded, 0 not applicable`, the C2 summary reads
`8 refused, 0 succeeded, 0 not applicable`, and End task on the helper in Task
Manager is denied.

---

## Part G: guarded mode, tested in your own account

The `guarded` mode runs the helper as the signed in user, with UIAccess, at
medium plus integrity, one step above the user's ordinary programs, and with
an access list on its process that does not let the user end it (see "Why the
guarded mode exists" in README.md). The question is whether the user's own
ordinary programs can end it, or open it to rewrite that access list.

No second Windows account is needed for this part. A helper that runs as you,
attacked from a normal (not elevated) terminal, is the same situation as a
child's helper attacked by the child: the same account on both sides, and the
attacker at ordinary medium integrity. So stay signed in as yourself.

G1. Open PowerShell with "Run as administrator" and go to the build folder
(`target\release`). If the spike is installed, remove it first, then install
it in guarded mode:
```
.\limo-spike uninstall
.\limo-spike install --helper-as guarded
```
Pass: it reports `helper mode set to guarded`, and that the service was
created and started. If you have an always on top app such as the desktop
ticker, start it now and leave it where you can see it.

G2. Still in the administrator PowerShell, type this but do NOT press Enter
yet:
```
.\limo-spike lock --seconds 60
```

G3. Open a second, normal PowerShell (not as administrator), go to the build
folder (`target\release`) and start the probe:
```
.\limo-spike probe --after 20
```
The probe runs from the build folder copy, not the installed exe, so attempt e
is a fair test.

G4. Click into the administrator PowerShell and press Enter, within about 10
seconds of starting the probe. The lock starts, and the probe makes its
attempts about 20 seconds after you started it, while the screen is locked.
The report's first lines show the time it ran; if the lock had not started by
then, the helper line will be missing, so run G2 to G4 again.

G5. While locked:
- watch that the always on top app, if you started one, stays hidden the whole
  time, not even a flicker;
- try Alt+Tab, the Windows key and Ctrl+Shift+Esc;
- then press Ctrl+Alt+Del, open Task Manager from that screen, and note
  whether it can be seen over the lock, and if it can, whether End task on
  `limo-spike.exe` is refused. If it cannot be seen, write "hidden". Note that
  your account is an administrator, and Windows usually runs Task Manager
  elevated for an administrator without asking. An elevated program sits above
  medium plus, so End task may well work here. Write down what happens, but it
  does not count against guarded mode: on a child's standard account Task
  Manager is not elevated. The probe, run from the normal terminal, is the
  check that counts.

G6. After the lock ends, in the normal PowerShell:
```
Get-Content "$env:TEMP\limo-spike-probe.txt"
.\limo-spike status
```
Then in the administrator PowerShell:
```
.\limo-spike uninstall
```

Pass:
- the probe summary reads `8 refused, 0 succeeded, 0 not applicable`: a, c, d
  and e are REFUSED, and so are all four b lines: ending the service process,
  opening it to rewrite its access list, ending the helper, and opening the
  helper to rewrite its access list (the helper's lines name your own
  account);
- the "helper launch" line in the log for this lock says
  `mode=guarded uiaccess=1 integrity=set` and then `dacl=admin-owner`,
  `dacl=owner-rights` or `dacl=owner-rights-no-label`. Any of the three
  passes; write down which;
- under "helper report:", the report in your temp folder written at the time
  of this lock has two lines. The first says `mode=managed`, `uiaccess=1`,
  `integrity=0x2100 (medium plus)` and `band[N]=2 (UIAccess band)` for every
  window. The second, `process_sddl=`, shows what Windows really applied, and
  gives your own SID (the long `S-1-5-21-...` one) only `0x101000`. With
  `dacl=admin-owner` it starts with `O:BA` (owner Administrators). With
  `dacl=owner-rights` or `dacl=owner-rights-no-label` it starts with whatever
  owner Windows picked (write it down) and its access list includes
  `(A;;0x101000;;;OW)`. With `dacl=owner-rights-no-label`, also write down the
  label Windows chose itself: the `S:` part at the end, expected
  `S:(ML;;NWNR;;;MP)` (medium plus);
- nothing else shows over the lock at any moment.

The service tries the access lists in the order `admin-owner`, `owner-rights`,
`owner-rights-no-label`, and moves to the next whenever Windows refuses one. A
log line starting "launching the helper with its protective access list,
variant admin-owner, failed" (or "variant owner-rights") followed by a later
variant in the launch line is therefore a finding, not a failure: write down
each Win32 error number. Error 1307 on `admin-owner` would mean Windows
refused Administrators as the owner of a process started with your token. A
refused `owner-rights` followed by `owner-rights-no-label` would mean Windows
refused the label set at creation.

These are failures; write the lines down exactly:
- a log line starting "could not raise the helper token to medium plus
  integrity", or `integrity=failed` in the launch line;
- `dacl=failed` in the launch line. The log then has one line per variant,
  each naming the variant and its Win32 error; copy all three;
- SUCCEEDED on any b line for the helper. With any of the three `dacl=` values
  and `integrity=0x2100 (medium plus)` in the report, that would mean the two
  layers do not do what README.md says, which is the thing this part exists
  to find out.

---

## Finish: remove everything

Sign in as the administrator. In an administrator PowerShell in the build
folder:
```
.\limo-spike uninstall
.\limo-spike uninstall
```
Pass: the first run stops and removes the service and deletes both folders.
The second run reports the service was not installed and finishes cleanly.
`.\limo-spike status` then says `not installed`.

As the standard account you may also delete `%TEMP%\limo-probe.exe`,
`%TEMP%\limo-spike-probe.txt` and `%TEMP%\helper-report.txt`. The
administrator's own `%TEMP%\helper-report.txt`, left by A2, A3, the `user`
mode locks and Part G, may be deleted too, and so may the
`%TEMP%\limo-spike-probe.txt` that Part G left there.

---

## Results

Fill in what you saw. "R" refused, "S" succeeded, or a short note.

| Check | Expected `user` | Expected `system` | Seen `user` | Seen `system` |
| --- | --- | --- | --- | --- |
| A2, A3: overlay on both monitors, escape chord exits 3 | yes | yes | | |
| A4: bandcheck says uiaccess=0, integrity 0x2000 (medium) and band 1, then a process_sddl line, nothing appears | yes | yes | | |
| B3: lock appears within about 1 s and ends by itself | yes | yes | | |
| B4: early unlock ends the lock after about 10 s | yes | yes | | |
| B5: far future deadline ignored, no overlay | yes | yes | | |
| U2: the always on top app never shows over the lock | yes | yes | | |
| U3: launch line in the log says uiaccess=1 | yes | yes | | |
| U3: helper report says uiaccess=1 and band 2 for every window | yes | yes | | |
| C1 a: stop the service | R | R | | |
| C1 b: end the service process | R | R | | |
| C1 b: open the service process to rewrite its access list | R | R | | |
| C1 c: overwrite control.txt | R | R | | |
| C1 d: create a file in Program Files\LiMoSpike | R | R | | |
| C1 e: delete the installed exe | R | R | | |
| C1 probe summary line | 6 refused, 0 succeeded | 6 refused, 0 succeeded | | |
| C1: service still Running afterwards | yes | yes | | |
| C2 a: stop the service under a lock | R | R | | |
| C2 b: end the service process | R | R | | |
| C2 b: open the service process to rewrite its access list | R | R | | |
| C2 b: end the helper process | S | R | | |
| C2 b: open the helper to rewrite its access list | no line (helper was ended) | R | | |
| C2 c, d, e | R | R | | |
| C2 probe summary line | 6 refused, 1 succeeded | 8 refused, 0 succeeded | | |
| Alt+Tab, Windows key, Ctrl+Shift+Esc, Alt+F4 | nothing happens | nothing happens | | |
| Ctrl+Alt+Del screen appears | yes | yes | | |
| Task Manager End task on the helper | works, overlay returns | denied | | |

How long between the helper dying and the overlay coming back (user mode):
`gap_ms` from the "relaunched helper" log line: ______ ms. By eye: ______.
The logged figure runs from the moment the service noticed the death to the
moment the new helper process started; the service checks every 100 ms, so the
real gap is at most 100 ms more, plus the moment the new window takes to draw.

Part G, helper mode `guarded`, in your own account:

| Check | Expected `guarded` | Seen `guarded` |
| --- | --- | --- |
| G1: install reports helper mode guarded | yes | |
| G5: the always on top app never shows over the lock | yes | |
| G5: Alt+Tab, Windows key, Ctrl+Shift+Esc | nothing happens | |
| G5: Ctrl+Alt+Del screen appears | yes | |
| G5: Task Manager can be seen over the lock | note it | |
| G5: Task Manager End task on the helper | note it (may work, see G5) | |
| G6 a: stop the service | R | |
| G6 b: end the service process | R | |
| G6 b: open the service process to rewrite its access list | R | |
| G6 b: end the helper process | R | |
| G6 b: open the helper to rewrite its access list | R | |
| G6 c, d, e | R | |
| G6 probe summary line | 8 refused, 0 succeeded, 0 not applicable | |
| G6: launch line says mode=guarded uiaccess=1 integrity=set | yes | |
| G6: launch line's dacl= value (admin-owner, owner-rights or owner-rights-no-label all pass) | note which | |
| G6: Win32 error logged for each refused variant, if any | note them | |
| G6: helper report says uiaccess=1, integrity 0x2100 (medium plus) and band 2 for every window | yes | |
| G6: helper report's process_sddl owner (O:BA for admin-owner) | note it | |
| G6: process_sddl gives your SID only 0x101000 (and has an OW entry for the owner-rights variants) | yes | |
| G6: label Windows chose, the S: part of process_sddl (owner-rights-no-label only; expected MP) | note it | |
