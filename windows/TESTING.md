# Testing LiMo for Windows on the PC

These are the steps to try `limo.exe` on a real PC, with the owner there.
They go from the safest to the riskiest; stop at any part, and undo it as
its last lines say. Every part says what to look for, and the results go in
the table at the end. They were first run on 19 November 2026: what was seen
is in that table, with notes under it.

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
- To compare the screens with the design, open the design's own pages from
  the repository (`design/screens/`), or the pictures `cargo run --example
  preview` writes in the `screens` folder (`screens\target\preview\`).
- From any lock, you can always get back to your own account: press
  Ctrl+Alt+Del and choose Switch user. From there, `sc.exe stop LiMo` (in an
  elevated terminal) ends every lock at once.
- Between parts, switch between the accounts; do not sign out of the child's.
  Signing out ends its helper with everything else in it, and the next sign
  in is a new session with a new number.
- For a picture of LiMo's screens, use a phone. In the first run a screen
  capture taken by a program in the child's account showed the desktop
  without LiMo's windows.

## Part A: no install, nothing changed

From an ordinary (not elevated) PowerShell:

1. `.\limo help`: the list of commands, `dev-code` among them, marked as
   scaffolding until the guardian's phone app exists. Exit code 0.
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

## Part C: a family, and a one time code, for testing

1. Elevated: `.\limo dev-setup --child Asha --account asha --minutes 20`
   with the service running. It must refuse, changing nothing: the service
   holds the family's file. `.\limo dev-code` must refuse the same way.
2. `sc.exe stop LiMo`, then
   `.\limo dev-setup --child Asha --account asha --minutes 20 --block notepad.exe --block explorer.exe`
   (with your test account's name after `--account`). Type a guardian
   password (8 or more characters) twice and a PIN (6 to 8 digits) twice;
   neither shows as you type. Look for `Made the family: ...` and
   `Blocked for Asha: notepad.exe, explorer.exe.`
3. Still with the service stopped: `.\limo dev-code --minutes 15`. It says
   it is scaffolding until the guardian's phone app exists, and prints `A
   one time code for Asha's device on this PC, worth 15 minutes, for today
   only:` and the code, as two groups of four digits. Write the code down:
   part J uses it, and it works today only. Run it a second time: a
   different code. Write that down too, as the spare.
4. `sc.exe start LiMo`. `.\limo status` ends with `dev-setup made a family
   with one child and 20 minutes a day.`, two lines `dev-code made a one time
   code of 15 minutes.` (with no code in them), and a new `The service
   started`.

Undo: `.\limo uninstall`.

## Part D: the helper in the child's session (the channel)

This part settles the one thing only real hardware can tell (see "The
channel" in README.md): whether the helper, started in another session than
the service's, gets its channel by inheritance.

1. Switch to the child's account. Within a second or two the time left
   display appears above the taskbar's clock: a small rounded card with an
   hourglass, `20 min` and a small `LEFT`. It never becomes the active
   window, so it does not take the keyboard from what you are doing.
2. Switch back to your own account, and in the elevated terminal run
   `.\limo status`. Look for, with your session numbers:
   - `Started the helper in session N: pid=P uiaccess=1 integrity=set
     dacl=admin-owner channel=inherited display=drawn`. The `dacl=` may be
     `owner-rights` or `owner-rights-no-label` instead: write down which,
     and any line before it saying a variant failed, with its Win32 error.
   - `The helper in session N (pid P) says it got: uiaccess=1
     integrity=0x2100 (medium plus) band=2 (UIAccess band)
     process_sddl=...`. This line came to the service over the channel, so
     it shows the helper was born with its channel. Write down the
     `process_sddl`: the child's SID (the long `S-1-5-21-...`) should have
     only `0x101000`.
   - No line saying the helper `shows the plain display`. If there is one,
     copy it exactly: it says why the drawn display could not start.
3. These would mean the channel was not inherited across sessions:
   `The helper could not be started in session N: CreateProcessAsUserW
   failed: Win32 error ...`, or `did not say hello within 10 seconds`
   again every ten seconds. Copy the lines exactly; that is the finding the
   brief asks for. The service then shows the child nothing, and only the
   fallback (part N) covers a lock.
4. Your own session: no `Started the helper` line for it, and nothing of
   LiMo on its screen.

Undo: `.\limo uninstall`.

## Part E: counting

In the child's session:

1. The time left display counts down in minutes, rounded up.
2. Leave the keyboard and mouse alone for 4 minutes. After 3 minutes the
   display turns grey and says `PAUSED` instead of `LEFT`; touch the mouse
   and it is back.
3. Play a video in full screen (a video file in the Media Player, full
   screen) and leave it alone for 4 minutes: it does not pause.
4. Lock the session with Windows key and L, wait a minute, and sign in
   again: the minutes left did not go down while it was locked.
5. At 15 minutes left, the display turns marigold, and a notice appears
   above it, their right edges in line: a bell, `15 minutes of screen time
   left today.`, `The PC locks when the time is used up.`, `LOCKS ABOUT` and
   a time, and an OK button. It goes by itself after 30 seconds.

Steps 6 to 9 check what the first run found (note 2 at the end): a session
is also counted while a program of it plays sound, or while a program keeps
the screen awake. They use about 15 minutes of the child's time. To run them
on their own, do parts B to D with `--minutes 30` in part C's `dev-setup`,
then these steps; on a run of every part, use `--minutes 40` there instead,
so that parts F to H still have time (the display then starts at 30 or 40
where the parts say 20).

6. Play a video with sound in a browser window, not in full screen (the
   window must not cover the screen), and leave the keyboard and mouse
   alone for 4 minutes. The display keeps counting down, and never says
   `PAUSED`.
7. Mute the same video with the player's own mute button (not the PC's
   volume), let it play, and leave the PC alone for 4 minutes: it still
   counts, because the player keeps the screen awake. If it says `PAUSED`
   after 3 minutes, write down which browser it was: that player does not
   ask Windows to keep the screen awake in a way LiMo can see.
8. Pause the video, and leave the PC alone: after 3 minutes the display
   says `PAUSED`, as in step 2. Touch the mouse, and it is back.
9. Sound alone: play music with no video (a music file in the Media
   Player, its window minimized) and leave the PC alone for 4 minutes: it
   keeps counting. Do the same with music in a browser tab, the browser's
   window minimized: it keeps counting too. If there are headphones, plug
   them in, or take them out, while it plays: it goes on counting. Stop the
   music and leave the PC alone: after 3 minutes, `PAUSED`.

Undo: nothing; part F goes on from here.

## Part F: blocked programs

In the child's session:

1. Open Notepad. Within about a second it closes, and a notice shows above
   the time left display: a blocked sign, `Notepad was closed.`, and under
   it `It is blocked by your family's rules.`, for 10 seconds. The name is
   the description Notepad's own file carries, without a closing `.exe`
   (Windows 11's Notepad describes itself as "Notepad.exe"); were there
   none, the notice would name the file, `Notepad.exe was closed.`. Open it
   again: it closes again.
2. The taskbar and the desktop stay: `explorer.exe` is blocked too, but is
   part of Windows, and LiMo never closes it.
3. Switch back and run `.\limo status` (elevated). Look for one line
   `Closed Notepad.exe in session N (1 running copy): ...`, however many
   times it was opened in ten minutes. The count may be 2 the first time:
   Windows' own starter and the app it starts. A program whose description
   says more than its file name has it in brackets after the file name, as
   in `Closed gl.exe ("Game Launcher")`. And one line `The family's rules
   block explorer.exe in session N, but it is part of Windows and LiMo never
   closes it: ...`, said once for the session.

Undo: nothing; part G goes on from here.

## Part G: the child cannot end the helper

1. In the child's session, open Task Manager (Ctrl+Shift+Esc), Details,
   find `limo.exe`, and End task. It must be refused. Then in a PowerShell
   there: `taskkill /F /IM limo.exe`: refused (access denied). The time left
   display stays.
2. From your own account, elevated, end the child's helper only, by its
   session (never by name alone: that would end the service too):
   `Get-Process limo | Where-Object { $_.SessionId -eq N } | Stop-Process -Force`
   with N the child's session from the log. Then `.\limo status`: a new
   `Started the helper in session N: ... gap_ms=0` line (shown when the last
   such line for that session is more than 10 seconds old). Switch to the
   child's session: the display is back, where it starts, above the clock.

Undo: nothing; part H goes on from here.

## Part H: the lock, and the test cap

In the child's session, use the PC until the 20 minutes are used up.

1. Warnings at 5 minutes and at 1 minute, each above the time left display
   for 30 seconds. At 5: `Save what you are doing.` At 1: `Save what you are
   doing now.`, with the seconds counting down in coral, and the time left
   display in coral too, in minutes and seconds.
2. At zero the lock covers every monitor. The main monitor shows the
   design's lock screen: the paper with its dots, LiMo's mark and the clock
   at the top, and a card with an hourglass, a `LOCKED` stamp, `ASHA · THIS
   PC`, `Today's time is used up.`, `Today's screen time is used up.`,
   `OPENS AGAIN` with when, and on its right `WHAT YOU CAN DO`: ask for more
   time, type a one time code, guardian PIN. Every other monitor says to
   look at the main one.
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

## Part I: the drawn lock, against the design

The test cap would lift each lock after two minutes, too soon for the next
parts: switch it off, and the lock from part H comes back. Your own account
is never locked, and the PIN you set opens the child's.

1. Elevated:
   `Set-Content -Path C:\ProgramData\LiMo\settings.json -Value '{"test_cap": false}' -Encoding ascii`.
   Within 10 seconds the log says `The test cap was switched off, so the
   lock in session N is back.` Switch to the child's account.
2. Set the lock screen beside the design's `lock-time_used_up` picture (at
   your monitor's size, or the nearest of 1366, 1920 and 1280 wide). Look
   for the same layout, colours, borders, hard shadows, dashed dividers, the
   turned `LOCKED` stamp, icons and type; sharp text and edges (nothing
   blurred or stretched) at this monitor's own Windows scaling; the card in
   the middle; `Nothing was closed. Your programs are still open behind this
   screen.` at the foot and `MADE BY LucidLoom AI` at the bottom right.
   Note anything that differs, with a photo if you can.
3. Every other monitor: the paper, LiMo's mark and the clock, and the words
   saying to look at the main screen, sharp at that monitor's own size and
   scaling.
4. The clock at the top right is the PC's time, as the taskbar shows it,
   in the 12 or 24 hours of the child's own Windows setting (Settings, Time
   and language, Date and time, in the child's account), and so are the
   times on the card. Note which you saw. To see it follow the setting,
   change it in the child's account at any time before a lock (for
   instance before part H): the next lock shows the other one.
5. Press letters, digits, Enter, Escape, the Windows key and Alt+Tab: with
   no panel open, nothing happens on the lock, and nothing reaches any other
   program.
6. Move the mouse over each way out. Click on the main card; then click on
   another monitor's lock: nothing happens there.

Undo: nothing; part J goes on from here.

## Part J: the ways out

Still on the lock in the child's account.

1. **Asking for time.** Click `Ask for more time`. Choose `30 min`. Type a
   reason with Shift (a capital letter and `!`), digits from the top row,
   an accented letter if the keyboard has an AltGr key for one, and a few
   letters then Backspace: each shows in the field as typed, by your own
   keyboard layout. A dead key (such as the accent key of a European
   layout) types nothing, and the letter after it types alone: that is
   expected, dead keys are out of scope. Press Enter: the button says
   `Sending…`, then the panel closes, and the card says `You asked for 30
   min at` the time, `WAITING`, and the ask row is gone. In the log (from
   your own account): `Session N: a request for more time was tried from
   the lock screen: asked for 30 minutes; it waits for a guardian.` and no
   word of the reason.
2. **Escape.** Click `Type a one time code`, type three digits, and press
   Escape: back to the ways out. Open it again: the cells are empty. Do the
   same in `Guardian PIN`.
3. **A wrong then a right PIN, adding time.** Click `Guardian PIN`. Type a
   wrong PIN (six digits, from the top row or the number pad) and press
   Enter: `Checking…`, then `That PIN is not right. 4 more tries before a
   wait.` and the dots gone. Type the right PIN, choose `15 min`, and click
   the button or press Enter: `Checking…`, then `15 minutes added.`, `The PC
   opens now.`, and within a second or two the lock goes and the time left
   display shows about 15 minutes. In the log: `the guardian PIN was tried
   from the lock screen: wrong (...)` then `right; 15 minutes added`, and
   never the PIN.

Then the time left display, part K, while those 15 minutes run out; when
they have, the lock is back, for the rest of this part.

4. **The wait after five wrong tries.** Click `Type a one time code` and
   type `1111 1111` (it is not today's code), then Enter: `That code is not
   right.` with the tries left. Do it four more times, so that five wrong
   tries are made since the right PIN of step 3. After the fifth, the panel
   closes, and the card says `Too many wrong tries. The next try is at` a
   time a minute on, with the code and PIN rows greyed, each saying `Wait
   until` that time; a click on them does nothing. When the minute has
   passed (the clock at the top moves on), the rows come back by
   themselves.
5. **A one time code.** Click `Type a one time code`, type the code from
   part C (spaces are not typed; the cells show it as two groups of four),
   and press Enter: `Checking…`, then `15 minutes added.` and the lock goes.
   In the log: `a one time code was tried from the lock screen: right; 15
   minutes added`, and never the code.

Undo: nothing; part K goes on from here (it is done between steps 3 and 4).

## Part K: the time left display and the notices

In the child's session, while the time added in part J runs.

1. **Its place.** The time left display sits above the taskbar's clock,
   its right edge about 16 pixels in from the right edge of the screen (a
   little more on a large screen, which scales it), and a little above the
   taskbar. Its corners are round, with nothing square behind them, and its
   hard shadow falls down and to the right with the desktop showing beside
   it. Click on a program, then on the display: the program keeps the
   keyboard (type, and the letters go to it).
2. **Dragging.** Drag the display with the mouse (the pointer shows a four
   way arrow over it): it follows. Drag it past each edge of the screen: it
   stops inside, never under the taskbar. If there are two monitors, drag it
   onto the other one: it stays there whole. Let go, open and close a
   program, and wait for the minutes to change: it keeps its place. A lock
   and back (step 5 below) keeps it there too.
3. **The closed notice.** Open Notepad: the notice appears above the
   display, their right edges in line (below it, if you dragged the display
   to the top of the screen), with `Notepad was closed.` and `It is
   blocked by your family's rules.` Click beside OK on the notice: nothing.
   Click OK: it goes. Open Notepad again and leave it: the notice goes by
   itself after 10 seconds.
4. **Each warning.** At 5 and at 1 minute left, as in part H: each appears
   above the display, OK puts it away, and it goes by itself after 30
   seconds. A warning put away with OK does not come back. If Notepad's
   notice and a warning come together, the newer shows, and the older comes
   back when it goes, if its own time has not run out.
5. At zero, the lock is back: go on with part J, step 4.

Undo: nothing; part L goes on from here.

## Part L: forcing the plain display

1. Elevated:
   `Set-Content -Path C:\ProgramData\LiMo\settings.json -Value '{"test_cap": false, "plain_display": true}' -Encoding ascii`.
   `.\limo status` shows `PLAIN DISPLAY: each helper started from now on
   shows the plain display, not the drawn one.`
2. End the child's helper by its session, as in part G, step 2. In the log:
   `Started the helper in session N: ... display=plain` and `The helper in
   session N (pid P) shows the plain display, not the drawn one, because
   the settings file asks for the plain display.`
3. Switch to the child's account: the plain display shows: a dark window
   by the clock with the time left in plain text (`N min left today.`). Its
   lock, when the time runs out, is plain and dark too, with the title, the
   child's name, the message and when it opens again, and no ways out.
4. Elevated:
   `Set-Content -Path C:\ProgramData\LiMo\settings.json -Value '{"test_cap": false}' -Encoding ascii`,
   and end the child's helper by its session again: the launch line says
   `display=drawn`, and the drawn display is back.

Undo: nothing; part M goes on from here.

## Part M: sleep

1. With the child signed in, put the PC to sleep for at least 2 minutes,
   then wake it and sign in to your own account.
2. In `.\limo status`, look for `Since the last look, N s passed by the
   steady count and M s by the wall clock.` If N and M are both about the
   length of the sleep, the steady count goes on through sleep, as the
   engine wants. If N is a second or so while M is the sleep's length, it
   does not: write both down.

Undo: nothing.

## Part N: the fallback (riskiest, last, optional)

This makes the helper unable to start while a lock is ordered, to see the
session disconnected. Your own account is never locked, and the Windows
sign in screen always lets you switch back to it.

1. The test cap is off since part I. The child's session must be locked by
   LiMo: if its time is not used up, use it up.
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

## Part O: the door of LiMo's settings

This part tries the door through which the guardian's console will ask the
service for everything it shows and changes (see "The console's door" in
README.md): opened from your own account, refused from the child's, the
wait after wrong passwords, and a family made through it with no
`dev-setup`. The console's window is not built yet; `limo dev-console`, a
small client typed at by hand, knocks in its place, from the account it
runs in, as the console will. It sends nothing unless the door is served
from session 0, where services run. Every answer is printed as it came;
`help` lists what can be typed. About 30 minutes. It needs a fresh install,
with no family.

1. Elevated, from the build folder: `.\limo uninstall` if LiMo is
   installed, then `.\limo install --dev`. Do not run `dev-setup`.
   `.\limo status`: the log ends with `The service started` and `The door
   for LiMo's settings is open.`
2. **Opened from your own account.** From an ordinary PowerShell (not
   elevated), in the build folder: `.\limo dev-console`. Look for `The door
   is served by process P in session 0, where services run.` and `LiMo for
   Windows 0.1.0 answered. Type help.` Type `state`: `"has_family": false`.
   Type `today`: `Not done (SignedOut): Signed out. Type the guardian
   password to go on.`
3. **A family made through the door.** Type `create-family` and a guardian
   password twice (8 or more characters; it does not show): `Done.` Then
   `set-pin` and a PIN twice (6 to 8 digits): `Done.` Then `add-child Asha`:
   `Done:` with Asha's `"child_id"`; copy it. Then `minutes CHILD_ID 20`
   (the id you copied): `Done:` with the rules, 20 minutes every day. Then
   `accounts`: every account of this PC, each with its `account_id` (an
   `S-1-5-21-...`), whether it is an administrator, whether someone is
   signed in to it, and `"this_window": true` on your own. Write down which
   are listed. Then `link ASHA_SID CHILD_ID`, with the child's test
   account's `account_id`, and `free YOUR_SID` with your own: `Done:` each.
   Then `finish`: `"unlinked_accounts": "locked"`. Then `today`: Asha, with
   a budget of 1200 seconds. Leave dev-console open.
4. Switch to the child's account (sign in if it is not): within a second or
   two the time left display shows `20 min`. The family made through the
   door counts, with no `dev-setup`.
5. **Refused from the child's account.** In a PowerShell in the child's
   account: `& "C:\Program Files\LiMo\limo.exe" dev-console`. It says `The
   door is served by process P in session 0, ...`, then `LiMo's settings
   cannot be opened from Asha's account. A guardian opens them from their
   own.`, and ends; `$LASTEXITCODE` is 2. Nothing else can be typed. Switch
   back to your own account.
6. **A wrong password's wait.** In your dev-console: `sign-out` (`Done.`),
   then `sign-in` with a wrong password: `Not done (WrongPassword): That
   password is not right. 4 more tries before a wait.` Again with wrong
   ones: 3, 2, then `1 more try before a wait.` The fifth wrong one: `That
   password is not right. Too many wrong tries: wait before trying again.`
   with when the wait ends. At once `sign-in` with the right password: `Not
   done (MustWait)`, and it was not tried. After a minute, the right
   password: `Done.`
7. **A lock from the door.** `request lock_now {"child":"CHILD_ID"}`:
   `Done:` with the lock. Switch to the child's account at once: the lock
   covers the screen (the test cap lifts it after 120 seconds). Switch
   back: `request lift {"child":"CHILD_ID"}`, and the child's account opens
   again within a second or two.
8. Elevated: `.\limo status`. Look for: `A console connected to LiMo's
   settings from session N (pid P).` for yours; `LiMo's settings were
   refused to a console in session M (pid Q): its account is linked to a
   child.` for the child's; `From LiMo's settings in session N (pid P):
   create_family done.` and lines like it; `a wrong guardian password was
   typed`. No line holds the password, the PIN, or the child's name. No line
   says a request `took ... ms between two steps`. If the 20 lines are too
   few, open the log in Notepad from the elevated PowerShell:
   `notepad C:\ProgramData\LiMo\limo.log`.
9. Optional, with a third standard account that is in no family (its
   `link` left as `"kind": "none"` in step 3): sign in to it. It is locked,
   `This account is not set up yet.`, with `Guardian PIN` among the ways
   out. A wrong PIN: `That PIN is not right. 4 more tries before a wait.`
   The right PIN with `15 min`: the account opens, and nothing of LiMo
   shows; it locks again after 15 minutes. In the log: `the guardian PIN,
   on the lock of an account not set up in LiMo, was tried from the lock
   screen: right`.
10. Optional, elevated: `.\limo reset-password`, and a new password twice.
    It stops the service for a moment, then says `The guardian password is
    set.` and `The service is started again.` In a new `.\limo dev-console`, `sign-in`
    with the old password: not right; with the new one: `Done.` `today`:
    Asha is still there, with her 20 minutes.

Undo: type `quit` in dev-console; then `.\limo uninstall`.

## Finish

1. Elevated, from the build folder: `.\limo uninstall`. It stops and removes
   the service and removes `C:\Program Files\LiMo` and `C:\ProgramData\LiMo`.
2. Run it again: `The service LiMo was not installed.`, and it finishes.
3. `.\limo status`: `Service LiMo: not installed.`
4. Remove the child's test account if it was made for this.

## Results

| Check | Expected | Seen |
| --- | --- | --- |
| A1 to A5 | as written | as written |
| B1 access list of the data folder | only SY and BA, FA, begins D:P | `D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)` |
| B2 status elevated: Running, test cap on, two log lines | yes | yes |
| B3 status not elevated: state only; dir refused | yes | yes |
| B4 child: dir refused, sc stop refused, status state only | yes | yes; reading the log and the settings file refused too; `sc.exe stop` gave error 5 |
| C1 dev-setup and dev-code refused while the service runs | yes | yes |
| C2, C4 family made, service started again | yes | yes |
| C3 dev-code prints a code, a different one the second time; the log has no code | yes | yes |
| D1 time left display above the clock in the child's session | yes | yes |
| D2 launch line: uiaccess=1 integrity=set display=drawn, which dacl= | note which | `dacl=admin-owner`, `channel=inherited`; no variant failed |
| D2 "says it got" line: uiaccess=1, 0x2100, band 2 | yes | yes |
| D2 process_sddl: child's SID gets only 0x101000 | yes | yes: `O:BAD:P(A;;0x1fffff;;;SY)(A;;0x1fffff;;;BA)(A;;0x101000;;;`the child's SID`)S:AI(ML;;NWNR;;;MP)` |
| D2 no "shows the plain display" line | none | none |
| D3 channel failure lines | none | none |
| D4 own session: no helper, nothing shown | yes | yes |
| E2 paused after 3 idle minutes, grey | yes | yes |
| E3 full screen video keeps counting | yes | yes; a video in a window does not (note 2) |
| E4 nothing counted while locked by Windows | yes | yes |
| E5 warning at 15 minutes, above the display, gone after 30 s | yes | yes |
| E6 a video with sound in a browser window, left alone 4 minutes, never PAUSED | yes | |
| E7 the same video muted, left alone 4 minutes, still counting; which browser | yes | |
| E8 the video paused, the PC left alone: PAUSED after the idle minutes | yes | |
| E9 music alone, minimized, left alone 4 minutes, counting, in the Media Player and in a browser tab; headphones in or out; PAUSED 3 minutes after it stops | yes | |
| F1 Notepad closed each time, "Notepad was closed." | yes | closed each time, 1.1 s after it opened; the notice named it "Notepad.exe" (note 3) |
| F2 explorer.exe kept | yes | yes |
| F3 one close line, one never closes line | yes | one close line; the never closes line came again every 10 minutes (note 4) |
| G1 child cannot end the helper | refused | refused: End task, and `taskkill /F` |
| G2 back after ended by an administrator, gap_ms | note gap_ms | `gap_ms=0` |
| H1 warnings at 5 and 1 minutes, the last counting down | yes | yes |
| H2 lock on every monitor, keys swallowed, nothing on top | yes | yes, on three monitors; no always on top program was tried |
| H3 lifted after 120 s | yes | in the log, 121 s after the lock was confirmed; not watched on the screen |
| H4 confirmed line, TEST CAP line, no disconnect | yes | yes |
| I2 main monitor's lock against the design, sharp at its scaling | note differences | nothing looked wrong at 100% scaling; not set beside the design, no pictures |
| I3 other monitors: look at the main one, sharp | yes | yes |
| I4 the lock's clock in the PC's 12 or 24 hours | note which | not noted |
| I5 keys with no panel open do nothing, reach nothing | yes | yes |
| J1 a request with a typed reason, by the layout; reason not logged | yes | yes |
| J2 Escape leaves a panel and keeps nothing typed | yes | yes |
| J3 wrong PIN, then right PIN adds 15 minutes and the lock goes | yes | yes |
| J4 five wrong tries: a wait of a minute, rows greyed, back after it | yes | yes |
| J5 the one time code adds 15 minutes; the code not logged | yes | yes |
| K1 the display's place, clear corners and shadow, never takes the keyboard | yes | in its place, nothing looked wrong; the keyboard was not tried |
| K2 drag follows, stops at the edges, onto another monitor, keeps its place | yes | yes |
| K3 closed notice above the display, OK only on OK, gone by itself after 10 s | yes | yes |
| K4 each warning above the display, OK, gone by itself after 30 s | yes | each warning seen; OK on a warning was not tried |
| L2 launch line display=plain, and the "shows the plain display" line | yes | yes |
| L3 the plain display shows | yes | yes, and its lock on three monitors |
| L4 drawn again after the switch is off | yes | yes |
| M2 steady N s and wall M s after sleep | note both | not run |
| N4 disconnected within about 6 s, programs still open | yes | yes, both times |
| N5 the two log lines | yes | yes; `gap_ms=112949` once the program was put back |
| O1 fresh install, the door open in the log | yes | |
| O2 dev-console from your account: served from session 0, answered; state with no family; today signed out | yes | |
| O3 family, PIN, child, 20 minutes, accounts listed (which), links, finish locked, today | yes | |
| O4 the child's display shows 20 min with no dev-setup | yes | |
| O5 dev-console from the child's account refused in those words, exit code 2 | yes | |
| O6 wrong passwords: 4, 3, 2, 1 tries left, a wait after the fifth, the right one not tried during it, right after a minute | yes | |
| O7 lock_now from the door reaches the child's screen at once; lift opens it | yes | |
| O8 log lines, and none with a password, PIN or name, none slow | yes | |
| O9 optional: an account not set up locked, the guardian PIN opens it for 15 minutes | yes | |
| O10 optional: reset-password, old password wrong, new right, the family kept | yes | |
| Finish: uninstall twice, status not installed | yes | yes |

## Notes from the first run, 19 November 2026

Windows 11 Home (build 26200), three monitors of 1920 by 1080 at 100%
scaling, an administrator's account and a standard account for the child.
Every part but M was run, in this order: A to H, I to K, L, N, the rest of
J, Finish.

1. **The channel is inherited across sessions.** The helper, started in the
   child's session by the service in session 0, was born with both ends of
   its channel and said hello at once, every time: at sign in, after it was
   ended by an administrator, with the plain display, and after the program
   was hidden and put back in part N.
2. **A video in a window pauses the count.** A video left playing in a
   browser window, not in full screen, stopped being counted after 3 idle
   minutes: 13 minutes of screen time took 31. That is the rule as it was
   written, and it lets a child watch without limit. Decided the same day:
   a session is also counted while it plays sound, or while a program keeps
   the screen awake. Built since; part E, steps 6 to 9, check it. Not yet
   run on a PC.
3. **Windows 11's Notepad describes itself as "Notepad.exe".** So the
   notice said "Notepad.exe was closed." and the log `Closed Notepad.exe
   ("Notepad.exe")`. Changed after the run: a description loses a closing
   `.exe`, and the log leaves out a description that only repeats the file's
   name. The first close found 2 running copies: Windows' own starter in
   System32 and the app it starts.
4. **One line filled the log.** "The family's rules block explorer.exe
   ..., but it is part of Windows and LiMo never closes it" came back every
   10 minutes, each time with a count near 590. Changed after the run: it is
   said once for a session.
5. **Two texts.** The hints said `sc stop LiMo`, which PowerShell reads as
   another command; they say `sc.exe` now. A sentence that ends with
   Windows' own message had two full stops; it has one now.
6. **Signing out of the child's account** ends its helper with the session.
   The log then says the helper ended and that one was started again: the
   service tried once more while the session was closing. Nothing is wrong.
7. **Not seen yet:** sleep (part M); the lock set beside the design, and
   monitors of different scalings (part I); an always on top program over
   the lock (part H).
