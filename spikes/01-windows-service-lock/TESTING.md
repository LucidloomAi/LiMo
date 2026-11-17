# Testing the LiMo service and lock spike

This checklist is for a person at the PC. Part A is a safe developer check.
Part B runs as an administrator. Part C is the real test, signed in as a
standard account. Parts B and C are done twice, once per helper mode.

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
away by itself, and `$LASTEXITCODE` is 0.

A3. Show the overlay and use the escape chord:
```
.\limo-spike overlay --seconds 20
```
While it is up press Ctrl+Shift+F12. Pass: it closes at once, well before 20
seconds, and `$LASTEXITCODE` is 3.

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
Pass: the log shows a helper launch with a pid and session id for each lock,
a helper exit with code 0 when each lock ended, and the B5 line if you ran it.

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
`5 refused, 0 succeeded, 0 not applicable`, and `status` still shows the
service `Running`. Attempt b lists the service process; its line may say the
image path was unreadable, which is fine as long as it is REFUSED.

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
   `limo-spike.exe`.
5. When the lock ends, read the report and the status:
   ```
   Get-Content "$env:TEMP\limo-spike-probe.txt"
   & "$env:ProgramFiles\LiMoSpike\limo-spike.exe" status
   ```

Expected in `user` mode:
- a, stop the service: REFUSED.
- b, end the service process: REFUSED.
- b, end the helper process: SUCCEEDED, and the overlay comes back within about
  a second. The log shows "helper exited ... code=1" followed by "relaunched
  helper ... gap_ms=N".
- c, d and e: REFUSED.
- Summary: `5 refused, 1 succeeded`.
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
uninstall itself. Then repeat Part B (B1 is the install you just did) and Part
C. In Part C, make the temp copy again, because the installed exe is new.

Expected in `system` mode: everything is refused, including ending the helper.
The C2 summary reads `6 refused, 0 succeeded`, and End task on the helper in
Task Manager is denied.

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

As the standard account you may also delete `%TEMP%\limo-probe.exe` and
`%TEMP%\limo-spike-probe.txt`.

---

## Results

Fill in what you saw. "R" refused, "S" succeeded, or a short note.

| Check | Expected `user` | Expected `system` | Seen `user` | Seen `system` |
| --- | --- | --- | --- | --- |
| A2, A3: overlay on both monitors, escape chord exits 3 | yes | yes | | |
| B3: lock appears within about 1 s and ends by itself | yes | yes | | |
| B4: early unlock ends the lock after about 10 s | yes | yes | | |
| B5: far future deadline ignored, no overlay | yes | yes | | |
| C1 a: stop the service | R | R | | |
| C1 b: end the service process | R | R | | |
| C1 c: overwrite control.txt | R | R | | |
| C1 d: create a file in Program Files\LiMoSpike | R | R | | |
| C1 e: delete the installed exe | R | R | | |
| C1: service still Running afterwards | yes | yes | | |
| C2 a: stop the service under a lock | R | R | | |
| C2 b: end the service process | R | R | | |
| C2 b: end the helper process | S | R | | |
| C2 c, d, e | R | R | | |
| C2 probe summary line | 5 refused, 1 succeeded | 6 refused, 0 succeeded | | |
| Alt+Tab, Windows key, Ctrl+Shift+Esc, Alt+F4 | nothing happens | nothing happens | | |
| Ctrl+Alt+Del screen appears | yes | yes | | |
| Task Manager End task on the helper | works, overlay returns | denied | | |

How long between the helper dying and the overlay coming back (user mode):
`gap_ms` from the "relaunched helper" log line: ______ ms. By eye: ______.
The logged figure runs from the moment the service noticed the death to the
moment the new helper process started; the service checks every 100 ms, so the
real gap is at most 100 ms more, plus the moment the new window takes to draw.
