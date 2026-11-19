# Testing LiMo for Windows on the PC

These are the steps to try `limo.exe` on a real PC, with the owner there.
They go from the safest to the riskiest; stop at any part, and undo it as
its last lines say. Nothing here has been run yet: every part says what to
look for, and the results go in the table at the end.

Everything is undone by `limo uninstall`, run from the build folder in an
administrator terminal: it stops and removes the service and deletes both
folders, the family's file with them.

## Before starting

- Build it: in the `windows` folder, `cargo build --release`. The program is
  `target\release\limo.exe`. Every command below is run from that folder,
  as `.\limo ...` in PowerShell.
- A standard Windows account (not an administrator) for the child, here
  called `asha`; use the name of the test account you have. If there is
  none, make one in Settings, Accounts, Other users, and remove it at the
  end. Sign in to it once beforehand, so Windows has set it up.
- Your own account is an administrator. LiMo never locks an administrator's
  account that is not linked to a child, so your own session should show
  nothing of LiMo at any time. If it ever does, that is a failure: write it
  down.
- Two monitors, if there are two; and an always on top program (such as the
  desktop ticker used for the first spike), if there is one.
- In PowerShell, `sc` means something else: use `sc.exe`.
- The log is `C:\ProgramData\LiMo\limo.log`; `.\limo status` from an
  administrator terminal shows its last 20 lines.

## Part A: no install, nothing changed

From an ordinary (not elevated) PowerShell:

1. `.\limo help`: the list of commands. Exit code 0.
2. `.\limo status`: `Service LiMo: not installed.`, `This terminal is not
   elevated.`, and `There is no log yet.`
3. `.\limo helper`: `limo helper is started by the LiMo service only, with
   its channel. Run by hand it does nothing.` Exit code 2 (`$LASTEXITCODE`).
4. `.\limo service`: it says it is the service control manager's entry and
   does nothing. Exit code 2.
5. `.\limo install`: `install must be run from an administrator terminal.
   Nothing was changed.`

Undo: nothing to undo.

## Part B: install, with the test cap

1. Open PowerShell with "Run as administrator", go to the build folder and
   run `.\limo install --dev`. Look for:
   - `Copied the program to C:\Program Files\LiMo\limo.exe.`
   - `Made the data folder C:\ProgramData\LiMo, for SYSTEM and Administrators
     only.` and the next line, `Its access list, as Windows applied it:`.
     Write that access list down exactly. It should name only `SY` and `BA`,
     each with `FA` (full access), and begin `D:P` (protected: nothing
     inherited). Windows may print extra flags after the `P`; note them.
   - `TEST CAP ON: ...`
   - `Created the service LiMo (LocalSystem, automatic, restarted on
     failure) and started it.`
2. `.\limo status` (still elevated): `Service LiMo: Running.`, `TEST CAP ON`,
   and the log ending with `Installed for testing, with the test cap on.`
   and `The service started (LiMo for Windows 0.1.0).` There is no family
   yet, so every account is free: nothing of LiMo shows anywhere.
3. From an ordinary PowerShell (not elevated): `.\limo status` shows the
   service Running, and says the log is in a folder only SYSTEM and
   Administrators may read. `dir C:\ProgramData\LiMo` says access is denied.
4. Sign in to the child's account (or switch to it), open PowerShell there,
   and run `dir C:\ProgramData\LiMo` and `sc.exe stop LiMo`. Both must be
   refused (access denied). Run `& "C:\Program Files\LiMo\limo.exe" status`:
   the service's state only, and the same line about the log. Switch back.

Undo: `.\limo uninstall` from the elevated terminal.

## Part C: a family, for testing

1. Elevated: `.\limo dev-setup --child Asha --account asha --minutes 20`
   with the service running. It must refuse, changing nothing: the service
   holds the family's file.
2. `sc.exe stop LiMo`, then
   `.\limo dev-setup --child Asha --account asha --minutes 20 --block notepad.exe --block explorer.exe`
   (with your test account's name after `--account`). Type a guardian
   password (8 or more characters) twice and a PIN (6 to 8 digits) twice;
   neither shows as you type. Look for `Made the family: ...` and
   `Blocked for Asha: notepad.exe, explorer.exe.`
3. `sc.exe start LiMo`. `.\limo status` ends with `dev-setup made a family
   with one child and 20 minutes a day.` and a new `The service started`.

Undo: `.\limo uninstall`.

## Part D: the helper in the child's session (the channel)

This part settles the one thing only real hardware can tell (see "The
channel" in README.md): whether the helper, started in another session than
the service's, gets its channel by inheritance.

1. Switch to the child's account. Within a second or two a small window
   appears by the clock: `20 min left today.` It never becomes the active
   window, so it does not take the keyboard from what you are doing.
2. Switch back to your own account, and in the elevated terminal run
   `.\limo status`. Look for, with your session numbers:
   - `Started the helper in session N: pid=P uiaccess=1 integrity=set
     dacl=admin-owner channel=inherited`. The `dacl=` may be
     `owner-rights` or `owner-rights-no-label` instead: write down which,
     and any line before it saying a variant failed, with its Win32 error.
   - `The helper in session N (pid P) says it got: uiaccess=1
     integrity=0x2100 (medium plus) band=2 (UIAccess band)
     process_sddl=...`. This line came to the service over the channel, so
     it shows the helper was born with its channel. Write down the
     `process_sddl`: the child's SID (the long `S-1-5-21-...`) should have
     only `0x101000`.
3. These would mean the channel was not inherited across sessions:
   `The helper could not be started in session N: CreateProcessAsUserW
   failed: Win32 error ...`, or `did not say hello within 10 seconds`
   again every ten seconds. Copy the lines exactly; that is the finding the
   brief asks for. The service then shows the child nothing, and only the
   fallback (part J) covers a lock.
4. Your own session: no `Started the helper` line for it, and nothing of
   LiMo on its screen.

Undo: `.\limo uninstall`.

## Part E: counting

In the child's session:

1. The window counts down in minutes, rounded up.
2. Leave the keyboard and mouse alone for 4 minutes. After 3 minutes the
   window adds `Paused.`; touch the mouse and it goes.
3. Play a video in full screen (a video file in the Media Player, full
   screen) and leave it alone for 4 minutes: no `Paused.`
4. Lock the session with Windows key and L, wait a minute, and sign in
   again: the minutes left did not go down while it was locked.
5. At 15 minutes left, a warning shows in the window for 30 seconds:
   `15 minutes of screen time left today.`

Undo: nothing; part F goes on from here.

## Part F: blocked programs

In the child's session:

1. Open Notepad. Within about a second it closes, and the time left window
   shows `Notepad.exe is blocked by your family's rules, so LiMo closed it.`
   for 10 seconds. Open it again: it closes again.
2. The taskbar and the desktop stay: `explorer.exe` is blocked too, but is
   part of Windows, and LiMo never closes it.
3. Switch back and run `.\limo status` (elevated). Look for one line
   `Closed Notepad.exe in session N (1 running copy): ...`, however many
   times it was opened, and one line `The family's rules block explorer.exe
   in session N, but it is part of Windows and LiMo never closes it: ...`.

Undo: nothing; part G goes on from here.

## Part G: the child cannot end the helper

1. In the child's session, open Task Manager (Ctrl+Shift+Esc), Details,
   find `limo.exe`, and End task. It must be refused. Then in a PowerShell
   there: `taskkill /F /IM limo.exe`: refused (access denied). The time left
   window stays.
2. From your own account, elevated, end the child's helper only, by its
   session (never by name alone: that would end the service too):
   `Get-Process limo | Where-Object { $_.SessionId -eq N } | Stop-Process -Force`
   with N the child's session from the log. Then `.\limo status`: a new
   `Started the helper in session N: ... gap_ms=0` line (shown when the last
   such line for that session is more than 10 seconds old). Switch to the
   child's session: the window is back.

Undo: nothing; part H goes on from here.

## Part H: the lock, and the test cap

In the child's session, use the PC until the 20 minutes are used up.

1. Warnings at 5 minutes and at 1 minute, each in the window for 30
   seconds.
2. At zero the lock covers every monitor: `Today's time is used up.`,
   `Asha`, `Today's screen time is used up.` and when it opens again.
   - Alt+Tab, the Windows key and Ctrl+Shift+Esc do nothing.
   - Ctrl+Alt+Del still shows Windows' own screen; press Escape to come back.
   - An always on top program, if one runs, never shows over the lock.
   - The mouse moves, over LiMo's windows only.
3. After 120 seconds the test cap lifts the lock: it goes, and nothing of
   LiMo shows. That is expected on a test install.
4. Switch back and look in `.\limo status` for `The helper in session N
   confirmed the lock is on the screen.` and `TEST CAP: the lock in session
   N has been on for 120 seconds, so the test cap lifted it. ...`, and for
   no line saying a session was disconnected.

Undo: nothing; part I goes on from here.

## Part I: sleep

1. With the child signed in, put the PC to sleep for at least 2 minutes,
   then wake it and sign in to your own account.
2. In `.\limo status`, look for `Since the last look, N s passed by the
   steady count and M s by the wall clock.` If N and M are both about the
   length of the sleep, the steady count goes on through sleep, as the
   engine wants. If N is a second or so while M is the sleep's length, it
   does not: write both down.

Undo: nothing.

## Part J: the fallback (riskiest, last, optional)

This makes the helper unable to start while a lock is ordered, to see the
session disconnected. Your own account is never locked, and the Windows
sign in screen always lets you switch back to it.

1. Elevated, switch the test cap off, so the lock from part H comes back:
   `Set-Content -Path C:\ProgramData\LiMo\settings.json -Value '{"test_cap": false}' -Encoding ascii`.
   Within 10 seconds the log says `The test cap was switched off, so the
   lock in session N is back.`
2. Rename the installed program, so no new helper can start (the running
   service keeps its copy open):
   `Rename-Item "C:\Program Files\LiMo\limo.exe" limo-off.exe`
3. End the child's helper, by its session, as in part G.
4. Switch to the child's account. Within about 6 seconds the session is
   disconnected and the Windows sign in screen shows. Its programs stay
   open: sign in again as the child, and about 6 seconds later the same
   happens.
5. From the sign in screen, switch to your own account. `.\limo status`
   shows `The helper could not be started in session N: ...` and `No helper
   confirmed the lock on the screen of session N within 5 seconds, so LiMo
   disconnected the session: ...`.

Undo, in this order: `Rename-Item "C:\Program Files\LiMo\limo-off.exe" limo.exe`;
then `.\limo uninstall`.

## Finish

1. Elevated, from the build folder: `.\limo uninstall`. It stops and removes
   the service and removes `C:\Program Files\LiMo` and `C:\ProgramData\LiMo`.
2. Run it again: `The service LiMo was not installed.`, and it finishes.
3. `.\limo status`: `Service LiMo: not installed.`
4. Remove the child's test account if it was made for this.

## Results

| Check | Expected | Seen |
| --- | --- | --- |
| A1 to A5 | as written | |
| B1 access list of the data folder | only SY and BA, FA, begins D:P | |
| B2 status elevated: Running, test cap on, two log lines | yes | |
| B3 status not elevated: state only; dir refused | yes | |
| B4 child: dir refused, sc stop refused, status state only | yes | |
| C1 dev-setup refused while the service runs | yes | |
| C2, C3 family made, service started again | yes | |
| D1 time left window in the child's session | yes | |
| D2 launch line: uiaccess=1 integrity=set, which dacl= | note which | |
| D2 "says it got" line: uiaccess=1, 0x2100, band 2 | yes | |
| D2 process_sddl: child's SID gets only 0x101000 | yes | |
| D3 channel failure lines | none | |
| D4 own session: no helper, nothing shown | yes | |
| E2 paused after 3 idle minutes | yes | |
| E3 full screen video keeps counting | yes | |
| E4 nothing counted while locked by Windows | yes | |
| E5 warning at 15 minutes | yes | |
| F1 Notepad closed each time, with the notice | yes | |
| F2 explorer.exe kept | yes | |
| F3 one close line, one never closes line | yes | |
| G1 child cannot end the helper | refused | |
| G2 back after ended by an administrator, gap_ms | note gap_ms | |
| H1 warnings at 5 and 1 minutes | yes | |
| H2 lock on every monitor, keys swallowed, nothing on top | yes | |
| H3 lifted after 120 s | yes | |
| H4 confirmed line, TEST CAP line, no disconnect | yes | |
| I2 steady N s and wall M s after sleep | note both | |
| J4 disconnected within about 6 s, programs still open | yes | |
| J5 the two log lines | yes | |
| Finish: uninstall twice, status not installed | yes | |
