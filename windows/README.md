# limo-windows

LiMo for Windows: the Windows service on the master PC, built around the
agent's engine ([`../agent`](../agent/)), and the helper it starts in each
signed in session that needs one. One program, `limo.exe`, does both, and
installs and removes itself for testing.

The helper shows the lock screen, the time left display and its notices as
the agreed design draws them ([`../design`](../design/)), from the pictures
of [`../screens`](../screens/): see [The drawn display](#the-drawn-display).
A plain display with no styling takes their place at once if they cannot be
shown.

**What it is not.** It has no network, no guardian's console and no
installer for families. It has been built and tested on the development PC
against a fake PC only: nothing here has run as a service yet, and no window
of the drawn display has been on a screen. [TESTING.md](TESTING.md) is how
it will be tried, with the owner at the PC.

## Contents

- [Commands](#commands)
- [How it is put together](#how-it-is-put-together)
- [Once a second: the facts](#once-a-second-the-facts)
- [The orders](#the-orders)
- [The helpers](#the-helpers)
- [The channel](#the-channel)
- [The guarded launch](#the-guarded-launch)
- [Closing programs, and what is never closed](#closing-programs-and-what-is-never-closed)
- [The fallback](#the-fallback)
- [The test cap](#the-test-cap)
- [The Display trait](#the-display-trait)
- [The drawn display](#the-drawn-display)
- [The data folder, and what each account can see](#the-data-folder-and-what-each-account-can-see)
- [What is logged](#what-is-logged)
- [Tests](#tests)
- [Left open](#left-open)
- [Dependencies and licences](#dependencies-and-licences)
- [Layout](#layout)

## Commands

| Command | Who | What |
| --- | --- | --- |
| `limo install [--dev]` | administrator | Copies the program to `%ProgramFiles%\LiMo`, makes `%ProgramData%\LiMo` for SYSTEM and Administrators only, creates the service `LiMo` (LocalSystem, automatic, restarted 1, 1 and then 10 seconds after a failure), and starts it. `--dev` also switches the test cap on. Refuses, changing nothing, when the service is installed or the data folder already exists. |
| `limo uninstall` | administrator | Stops and deletes the service and removes both folders, the family's file with them. Safe to run twice. Refuses to run from the installed copy, which cannot delete itself. |
| `limo status` | anyone | The service's state; whether this terminal is elevated; the test cap; and the last 20 lines of the log, when this account may read them. |
| `limo dev-setup --child NAME --account ACCOUNT [--minutes N] [--block PROGRAM]...` | administrator, service stopped | Scaffolding until the guardian's console exists: a family with a guardian password and PIN typed at the prompt (not shown), one child, the Windows account linked to the child, N minutes a day (10 when not given, 1 to 240), and each program given blocked. Refuses while the service runs, since the service holds the family's file, and when the file already has a family. |
| `limo dev-code [--minutes 15\|30\|60] [--child NAME]` | administrator, service stopped | Scaffolding until the guardian's phone app exists: makes the next one time code for the test child's device on this PC, worth the minutes given (15 when not given), for the child's day today, and prints it. Each run makes a new code, and the family's file records it; a device takes ten a day. The device is paired first if the engine has not yet seen the child on this PC, as the engine would pair it. `--child` is needed only when the family has more than one child. The log says a code was made, never the code. Refuses while the service runs. |
| `limo service` | the service control manager | The service. Run from a terminal it says so and does nothing. |
| `limo helper --channel R:W [--plain]` | the service | The helper. Run by hand, without the channel it was born with, it says so and does nothing. `--plain`: show the plain display; the service adds it when the settings file asks. |

Exit codes: 0 done, 1 failed, 2 refused.

## How it is put together

```
the service control manager
        |  stop
        v
service thread  (src/win/service.rs runs src/service.rs every 100 ms)
  Engine + Hub  <- only here
  WinOs         -> sessions, processes, version resources, launches,
        |          ending processes, disconnecting sessions, the log
        |  one reader thread and one writer thread per helper,
        |  talking to the service thread through channels
        v
helper in each session that needs one  (src/win/helper.rs)
  reader thread  -> user interface thread (HelperCore + Display:
                    the drawn display, or the plain one in its place)
  report thread  -> writer thread -> the service
```

- **The seam.** Everything the service decides is in `src/service.rs`,
  written against the `Os` trait (`src/os.rs`): the clock, the sessions, the
  processes, a program for a process, starting and ending a helper, its
  channel, ending a process, disconnecting a session, the test cap and the
  log. The real `Os` is `WinOs` in `src/win/`; the tests use a fake one. The
  engine and the family's file are real in the tests too.
- **One thread for the engine and the hub.** The service thread owns both,
  and calls `Service::step` every 100 ms. The handler the service control
  manager calls runs on another thread and only sends "stop" through a
  channel. Each helper's channel has a reader thread and a writer thread,
  which hand messages to and from the service thread through channels.
- **Unsafe code only in `src/win/`.** Every other module says
  `#![forbid(unsafe_code)]`, and a test (`tests/house_rules.rs`) checks it.
  Each unsafe block says why it is sound.
- **What is decided, and where.** `service.rs`: the facts, the orders, the
  helpers, the held warning, the fallback, the test cap. `close.rs`: the
  never close list. `programs.rs`: programs and their cache.
  `protocol.rs`: the channel's messages, frames and limits. `helper.rs`:
  the helper's side, the `Display` trait, and the choice of the drawn or the
  plain display. `drawn.rs`: everything the drawn display decides. `quiet.rs`:
  keeping the log readable. `settings.rs`, `cli.rs`, `setup.rs`, `text.rs`,
  `about.rs`: names and places, the command line, `dev-setup`'s and
  `dev-code`'s work, the plain display's words, and words for what a helper
  got.

## Once a second: the facts

| Fact | Where it comes from |
| --- | --- |
| `wall_secs` | The system clock. |
| `steady_ms` | `QueryInterruptTime`, in milliseconds: counted from start up, and it cannot be set. Its own page does not say whether it counts through sleep; the page of its "unbiased" twin says that one "does not include time the system spends in sleep or hibernation", unlike this one, which Windows uses so that timers that would have run out during a sleep run out on waking. Not yet seen through a real sleep; TESTING.md checks it. |
| `utc_offset_secs` | `GetTimeZoneInformation`: the zone's bias, and the daylight bias when daylight time is in force. |
| Sessions | `WTSEnumerateSessionsW`: every session but session 0 with a user name. The account's SID and whether it is an administrator come from the session's user token (`WTSQueryUserToken`): the Administrators group among the token's groups, enabled or kept for denying only (a UAC filtered token), or among its linked token's. The state: in use when the session is active and Windows does not say it is locked (`WTSSessionInfoEx`), locked when it does, disconnected otherwise. |
| Programs running | Read by the service itself, never taken from a helper: `WTSEnumerateProcessesExW` for every process with its session, then for each one its start (`GetProcessTimes`) and its file's path (`QueryFullProcessImageNameW`). A program is the file's name, and the original file name and company name from its version resource. The same resource's description (`FileDescription`, such as "Game Launcher") is read beside them and kept in the cache with them, but never handed to the engine: it only names a program in the notice that it was closed, and in the log line for the close. All three are properties of the program's file, written in by its maker and the same for every copy; none is a window title, a document's name or a web address, and LiMo reads none of those. LiMo's own processes are left out. |
| Idle, front, full screen | From the session's helper, about once a second: milliseconds since the last input (`GetLastInputInfo`), the process id of the window in front, and whether something is in full screen (the window in front covers its monitor and is not the desktop or the taskbar, or Windows says a full screen program or a presentation is running). |

- **Version resources are read once per file.** The cache keeps each
  file's program, with its description beside it, by its path and last
  write time, and each running process's by its id and start. A file is
  read again only after it is written. Tests: `facts::a_files_version_resource_is_read_once_until_the_file_is_written`,
  `orders::a_closed_program_is_named_by_its_files_description_when_it_has_one`.
- **The service finds the program in front itself.** The helper sends only a
  process id. The service looks it up in its own list (or alone, if it
  started since), checks it is in that same session, and makes the program
  itself; a process of another session counts as nothing in front, and is
  logged. LiMo's own windows count as nothing in front. It never takes a
  name from a helper. Test: `facts::the_program_in_front_is_found_by_the_service_itself_and_only_in_that_session`.
- **No fresh report means time counts.** With no report from the session's
  helper in the last 3 seconds (`REPORT_FRESH_MS`), or no helper: idle 0,
  not full screen, nothing in front. A missing helper never gives free time.
  Test: `facts::with_no_fresh_report_input_counts_as_just_now_so_a_missing_helper_gives_no_free_time`.

## The orders

- **`helper_needed`**: exactly one helper runs in that session, started the
  guarded way; see [The helpers](#the-helpers).
- **`show`**: handed to the session's helper at once whenever it changes,
  with the child's name (read from the family's file, again each minute),
  and again every 5 seconds (`RESHOW_MS`) when it does not. Each change gets
  a new number (`seq`).
- **A warning** comes from the engine in one tick only. The service holds it
  for that session, and puts it in every show handed over, until the helper
  says it has shown it (`WarningShown`) or its lock time has passed. Tests:
  `orders::a_warning_is_held_until_the_helper_says_it_showed_it`,
  `orders::a_warning_never_confirmed_is_held_until_its_lock_time_passes`.
- **`close`**: see [Closing programs](#closing-programs-and-what-is-never-closed).
- **`prod_devices`**: there is no network yet; ignored, and said once in the
  log. Test: `orders::phones_the_books_want_to_hear_from_are_ignored_and_said_once`.
- **`problems`**: logged, each one at most once every ten minutes, with a
  count of the times held back. Test:
  `orders::the_engines_problems_are_logged_without_repeating_every_second`.

## The helpers

- **One per session that needs one.** Started when the order says so, and
  told to quit when it no longer does (ended 3 seconds later if it has not).
  A session that ends takes its helper with it.
- **Back at once when it dies.** The service looks every 100 ms, and starts a
  new one in the same step; the log line says `gap_ms`. A helper that ends
  within 2 seconds of starting is started again after a second, so a
  helper that cannot run does not spin.
- **A launch that fails** is tried again every second, and logged once.
- **It must say hello first,** within 10 seconds, or it is ended and started
  again.
- **What it got** (UIAccess, integrity level, z order band, its process
  object's access list) it says once over the channel, and the service logs
  it, as the first spike's report did.

Tests: every test in `tests/service/helpers.rs`.

## The channel

**Born with it.** The service makes two anonymous pipes, one each way
(anonymous pipes go one way only), before it starts the helper. It marks
only the helper's two ends as inheritable, starts the helper with handle
inheritance on and the list of handles it may inherit limited to exactly
those two (`PROC_THREAD_ATTRIBUTE_HANDLE_LIST`), puts their two values on
the helper's command line, and closes its own copies of them. An anonymous
pipe has no name another program could open or make first, and both its
ends exist from the moment it is made. Nothing else of the service's is
inherited. The helper checks that both values are pipes, and marks them not
inheritable before it does anything else.

**One thing only real hardware can tell.** Microsoft's page for
`CreateProcessAsUserW` says, of `bInheritHandles`, under "Terminal
Services": "You cannot inherit handles across sessions. Additionally, if this
parameter is TRUE, you must create the process in the same session as the
caller." The service is in session 0 and the helper in the user's, so read
literally this forbids the design. The remark dates from Windows XP, when a
process for another session was made by a helper inside that session; since
Windows Vista the caller makes the process itself, and the handles it
inherits are copied from the caller's own table, whatever the session. That
is the reason to expect it to work, and why it was built as the brief asks.
But it has not been tried here. If it does not hold, the helper is never started
(or never says hello): the log says so every second, nothing is shown, and
the fallback disconnects any session ordered locked. There is no quiet
fallback to a pipe anyone can open. TESTING.md, part D, is where this is
settled.

**Frames.** Each message is the protocol version (2 bytes), the length of
what follows (4 bytes), both little endian, then that many bytes of JSON. A
message may be at most 64 KiB (`MAX_MESSAGE_BYTES`); the longest real one is
under 2 KiB. A frame that says it is longer is refused from its header,
before any of it is kept, and a reader never holds more than two frames'
worth. The protocol is version 2: version 2 added the program's name to a
notice, and `shows_plain`. A helper and a service of different versions
never talk: the first frame closes the channel.

| Helper to service | |
| --- | --- |
| `hello` | First, and only first: the protocol version. |
| `about` | Once: what the helper got (UIAccess, integrity, band, its access list). |
| `report` | About once a second: idle milliseconds, the process id in front, full screen. |
| `lock_on_screen` | The lock handed over with this number covers every monitor. |
| `warning_shown` | This warning (its cause, minutes and lock time) is on the screen. |
| `ask_for_time`, `try_code`, `try_pin` | The three ways out, each with a number for its answer. |
| `shows_plain` | The helper shows the plain display, not the drawn one, and why, for the log. It changes nothing else. The service keeps at most 300 characters of the why, with control characters taken out. |

| Service to helper | |
| --- | --- |
| `show` | What to show, with its number and the child's name. |
| `notice` | A program was closed: the engine's sentence, and the program's name beside it (the description its file carries, such as "Game Launcher", at most 60 characters; or, when it has none, its file name as it is now, as the engine's sentence names it), so the drawn notice can say "Game Launcher was closed." with the reason under it. |
| `answer` | The answer to a way out: asked, tried (with the outcome), or refused (why). |
| `quit` | Close everything and end. |

**What closes a channel.** A frame of another version, too long, or not
JSON; a message nobody knows, or with a field it should not have (a
`window_title`, say); a message before hello, hello twice, or "what I got"
twice; a lock confirmed that was never sent; or a helper that stops reading
what it is sent. The service ends that helper and starts another. Nothing
from a closed channel reaches the engine, and none of it can make the
service panic. The log says why, in plain words, and never quotes what was
sent. Tests: every test in `tests/service/channel.rs`, and the unit tests
in `src/protocol.rs`.

**The ways out.** The session a way out belongs to is always the one the
service started that helper in; no message carries a session. At most one
way out a second is taken from a helper; another in that second is refused
with "One thing at a time". A right PIN that confirms a release is answered
as the engine gives it, and logged: releasing the master PC is not built yet.

**A PIN or a code.** It travels from the helper to the service as a
`Secret`, whose `Debug` output is `Secret(hidden)` and which is wiped from
memory when dropped. Each frame's buffer is made at its full size at once, so
it never grows and leaves a copy behind, and is wiped when dropped; the
reader wipes every frame it has read, and what it read from the pipe. The
service hands the PIN to the engine for that one call and keeps nothing.
It is never logged: the log says only which way out was tried and what
became of it ("the guardian PIN was tried from the lock screen: wrong").
Error messages about a frame never quote it, not even the JSON reader's
own, which can. What is not wiped: the pipe's buffer inside Windows, which
only the two ends can read, and a passing copy the JSON reader makes of a
string with an escape in it. Tests: `protocol::tests::a_pin_never_shows_in_debug_output_or_in_an_error`,
`protocol::tests::what_was_read_is_wiped_from_the_decoder`,
`channel::a_pin_or_a_code_is_never_logged_kept_or_shown`.

**What the service believes from a helper.** Only the three things a
program inside the session must say (idle, the process in front, full
screen), and only while fresh; the confirmations of a lock and a warning;
the ways out, which the engine judges; and its word that it shows the plain
display, which only goes in the log. A helper that lies about idle
time could stop the count: that is why the helper is guarded (below), so
that the child's own programs cannot reach it, its channel or its handles.

## The guarded launch

Ported from the first spike's `guarded` mode, which held on real hardware on
17 Nov 2026, with the channel added and nothing redesigned
(`src/win/launch.rs`). The helper runs as the signed in user, with the
user's own token, so a bug in it yields nothing more than the user's own
rights. Before it starts, the service sets the UIAccess flag on the token
(its topmost windows sit in the UIAccess band, above every ordinary always
on top window), raises the token to medium plus integrity (`S-1-16-8448`,
one step above the user's own programs, which then cannot inject into it,
post messages to it, tamper with its threads or rewrite its access list),
and gives the process object an access list that lets the user only see it
and wait for it (`0x101000`), never end it. The three access list variants
are tried in the spike's order, `admin-owner`, `owner-rights`,
`owner-rights-no-label`; the first that starts is kept.

A step that fails never stops the launch, because a weaker lock is better
than no lock: each failure is logged once, and the launch line says what
the helper got, and which display it was told to show:
`uiaccess=1 integrity=set dacl=admin-owner channel=inherited display=drawn`.
With every variant refused it starts with none (`dacl=failed`). The one thing
it never starts without is its channel. It is started on `winsta0\default`
with no console window (`CREATE_NO_WINDOW`), by the full path of the
program, which is also named on its command line.

## Closing programs, and what is never closed

For each program the engine orders closed in a session, the service ends
every running copy of it in that session: every process whose program
(file name, original name and maker) is that program. Then it tells the
session's helper the reason, at once or as soon as a helper runs. Before it
ends a process, Windows is asked again, on the process itself, that it is
still in that session, started at the same moment, from the same file, so a
process id given to another process meanwhile is never ended.

It never ends:

- a process in session 0, or in any session but the order's;
- a process of LiMo's own: the service, any of its helpers (by process id),
  or any process whose file is the service's own program file;
- a process whose file it could not read, since it then cannot tell what it
  is (said in the log);
- a part of Windows on the list below, when its file is inside the Windows
  folder (such as `C:\Windows`), where a standard account cannot write. The
  name alone spares nothing: a game renamed `explorer.exe` in a Downloads
  folder is closed. A family that blocks one of these is told so in the log.

| File | Why LiMo never closes it |
| --- | --- |
| csrss.exe | It runs the session itself; closing it stops Windows with a blue screen. |
| winlogon.exe | It signs the session in and out and shows the secure sign in screen; closing it ends the session or stops Windows. |
| wininit.exe | It starts and watches the core of Windows; closing it stops Windows. |
| smss.exe | It starts every session; closing it stops Windows. |
| lsass.exe | It checks passwords and keeps everyone signed in; closing it restarts Windows. |
| services.exe | It runs every Windows service, LiMo's own among them. |
| svchost.exe | It runs Windows services, some of them inside a signed in session. |
| dwm.exe | It draws every window; closing it blanks the screen. |
| fontdrvhost.exe | It draws the fonts every program uses. |
| explorer.exe | It is the desktop, the taskbar and the Start menu; without it the session cannot be used. |
| sihost.exe | It runs the shell's own parts: notifications, keyboard layouts, the action centre. |
| startmenuexperiencehost.exe | It is the Start menu. |
| shellexperiencehost.exe | It is the clock, the notification area and other parts of the taskbar. |
| ctfmon.exe | It handles typing: keyboard layouts and input in other languages. |
| textinputhost.exe | It is the touch keyboard and the emoji and input language panels. |
| conhost.exe | It draws every console window; LiMo closes the program in the console instead. |
| taskhostw.exe | It runs Windows' own tasks for the signed in user. |
| runtimebroker.exe | It checks what Store programs may do; closing it breaks them all. |
| dllhost.exe | It runs parts of Windows on behalf of other programs, such as pictures in folders. |
| userinit.exe | It sets up the session when someone signs in. |
| logonui.exe | It is the Windows sign in and lock screen. |
| consent.exe | It asks for an administrator's permission; closing it would stop a guardian from giving it. |

Each close is logged once for each program and session, and again at most
every ten minutes with a count; the line names the file, with the
program's description after it in brackets when its file has one. A
process Windows refuses to end is logged the same way. The notice handed to
the helper carries the engine's sentence and the program's name, as a
child knows it: the description its file carries ("Game Launcher"), with
whitespace trimmed and each run of it made one space, control characters
dropped, and cut to 60 characters with an ellipsis; or, when there is none
(or nothing is left of it), its file name as it is now ("Game.exe"), the
name the engine's own sentence uses. The engine's sentence, its facts and
the family's records stay as they were: they never carry the description.
Tests: `orders::a_blocked_program_is_closed_in_its_own_session_only_every_copy_and_logged_once`,
`orders::a_closed_program_is_named_by_its_files_description_when_it_has_one`,
`orders::limos_own_processes_and_the_parts_of_windows_are_never_closed`, and
the unit tests in `src/close.rs` and `src/programs.rs`.

## The fallback

When a session is ordered locked, is the one on the screen, and no helper
has confirmed the lock is on the screen within 5 seconds
(`LOCK_CONFIRM_MS`), the service disconnects the session
(`WTSDisconnectSession`): the Windows sign in screen covers it, and its
programs stay open. It is logged. The 5 seconds start when the lock is
ordered, or when the helper that had confirmed it dies; a new helper must
confirm again. A session locked by Windows, or not on the screen, is not
disconnected; nor is any session whose order is not a lock. Tests: every
test in `tests/service/fallback.rs`.

## The test cap

For testing with the owner present only. `limo install --dev` writes
`{"test_cap": true}` to `%ProgramData%\LiMo\settings.json`; the service reads
it every 10 seconds. With it on, a lock that has been on in a session for 120
seconds is lifted: the session shows nothing (time is still not counted,
since the engine still has it locked), and the log says, each time:
"TEST CAP: the lock in session N has been on for 120 seconds, so the test
cap lifted it. This is a test install (limo install --dev). A real install
has no test cap." Switched off, the lock comes back. `limo install` without
`--dev` writes no settings file, and a file that cannot be read is no cap.
Tests: `fallback::the_test_cap_lifts_a_lock_after_120_seconds_and_says_so_loudly`,
`fallback::without_the_test_cap_a_lock_stays`.

The same file can force the plain display, for testing:
`{"test_cap": true, "plain_display": true}`, written by hand. The helper
runs as the signed in user and cannot read the data folder, so the service
reads the switch (with the test cap, every 10 seconds) and starts each new
helper with `--plain`; a helper already running keeps its display until it
is started again. The launch line says `display=plain`, the helper says why
it shows the plain display, and `limo status` says `PLAIN DISPLAY`. Tests:
`settings::tests::the_plain_display_is_forced_only_when_the_file_says_so_plainly`,
`launch::tests::the_command_line_asks_for_the_plain_display_when_the_settings_do`,
`helper::tests::the_settings_can_ask_for_the_plain_display_and_the_service_is_told`.

## The Display trait

What draws, in the helper (`src/helper.rs`). It has two implementations:
the drawn display (`src/win/drawn.rs`, see below), which the helper uses;
and the plain display (`src/win/display.rs`), ported from the spike's
overlay: a borderless window over each monitor for a lock, with the title,
the child's name, the message and when it opens again, and a small window
by the clock for the time left, with no ways out and no styling. Each must
do exactly this:

- **`show(ShowNow) -> Shown`**: put up what the service orders, every time
  it is called, as a level; the same show again changes nothing on the
  screen (it comes about every five seconds when nothing changed).
  `Nothing`: nothing of LiMo on the screen. `TimeLeft`: a small window above
  every other, never activated and never taking the keyboard, with the time
  left, whether it is counted, a warning for a while, and the end of a
  guardian's unlock. `Locked`: a window over every monitor, above every
  other window and put back on top a few times a second, the keyboard kept
  from every other program while it is up, the lock screen's title, message,
  when it opens again and the child's name, and the ways out the
  `LockScreen` offers. It returns whether a lock covers every monitor and
  whether the warning is on the screen, and must never say a lock is up when
  it is not: the service disconnects the session when no lock is confirmed.
- **`notice(text, program)`**: show that a program was closed, and why, for
  a while; `program` is its name.
- **`answer(ask, answer)`**: show the answer to a way out it sent.
- **`pump()`**: called four times a second on the same thread: keep the lock
  on top, count down, let notices fade.
- **`input()`**: called on the same thread when the display's own windows
  or keyboard hook noted that the child did something: take it.

Ways out go to the service only through the `Outbox` the display is made
with (`ask_for_time`, `try_code`, `try_pin`); a typed PIN or code goes as a
`Secret` and is not kept. The display never shows or keeps a window title,
the name of a document or a web address; the service sends none, and the
only program name it shows is the one a notice carries, of a program LiMo
closed: the description in the program's own file, or else that file's
name. The
helper reads the class name of the window in front only to tell the desktop
and the taskbar from a program in full screen, and sends none of it.

**The choice, and the fallback to the plain display** (`Chooser` in
`src/helper.rs`). The helper makes the drawn display. If it cannot start
(its window classes, its two small windows, or a first picture of the lock
fail), the plain display is used from the start. If it fails later (a lock
window or a bitmap that cannot be made, a picture a window will not take,
or any drawing step that panics, which is caught), or if a lock it was
ordered to show does not cover every monitor, the plain display takes over
at once: it is made and shown what was last ordered, in the same call, and
only then is the drawn display dropped (its windows destroyed, its hook
removed, its bitmaps wiped), so a lock is never off the screen between the
two. Either way the service is told once, with why (`shows_plain`), and
logs it. It never goes back to the drawn display; the next helper tries it
again. Tests: the five `helper::tests` from
`a_healthy_drawn_display_is_used_and_nothing_is_told` to
`the_settings_can_ask_for_the_plain_display_and_the_service_is_told`, and
`channel::a_helper_that_shows_the_plain_display_is_logged_in_plain_words_and_nothing_else_changes`.

## The drawn display

The lock screen, the time left display and its notices from
[`../screens`](../screens/) (`limo-screens`), on the screen. What it
decides is in `src/drawn.rs`, with no Windows code, and tested there
(`drawn::tests`); `src/win/drawn.rs` only makes the windows, copies the
pictures onto them and hands in what the child does. Its window procedures
and its keyboard hook only note what happened and wake the helper's loop
(`WM_LIMO_INPUT`); nothing is drawn inside them, so the hook returns at
once.

**The lock.** One borderless window over each monitor, the same kind as the
plain display's: topmost tool windows (in the UIAccess band when the helper
has UIAccess), put back on top four times a second, the main one brought to
the front, with the monitors looked at again every two seconds and at once
when Windows says the displays changed. The main monitor shows the lock
screen (`Lock::draw`), every other one `draw_other`. Each picture is drawn
at its window's size in real pixels (the helper is per monitor DPI aware)
into a bitmap of the window's own, after every input and about once a
second; the crate's own cache makes an unchanged picture cheap, and the
bitmap is put on the window only when a pixel of it changed
(`copy_to_bgra`). A window is shown only once its picture is in it, and the
lock is confirmed to the service only when every monitor has its window.

**Clicks and keys.** A click on the main monitor's window goes to the lock
as `Input::Click`, in the picture's pixels. Keys come from the low level
keyboard hook, which keeps every key from every other program while a lock
is up, as the plain display's does. The lock gets:

- Backspace, Enter and Escape, always;
- the digits of the top row and of the number pad, whatever the layout and
  Shift make of them, so a PIN or a code types the same on every keyboard
  (on a French layout the top row's 1 types "&" without Shift, and is still
  1 here);
- while the ask panel is open, the characters of the reason, turned from
  keys into text with the user's own keyboard layout (`ToUnicodeEx`, with the
  helper thread's layout), the Shifts and Caps Lock, and AltGr. Ctrl or Alt
  alone, or the Windows key, is a shortcut and types nothing. The hook keeps
  the modifier keys from Windows too, so the display follows them itself
  (`Mods`), each side; Caps Lock stays as it was when the lock came, since
  its key is kept from Windows too.

**Out of scope: dead keys and input method editors.** A dead key (such as
the accent key of many European layouts) types nothing, and the letter after
it types alone; Windows is asked not to keep the dead key waiting, so
nothing else is changed by it. Input method editors (Chinese, Japanese,
Korean and others) are not used: a key types what the layout alone gives
for it. A key that gives two characters types the first.

**The ways out.** A way out the lock gives goes to the service through the
helper's `Outbox`, once; the number the outbox gives it is kept, and only
the answer that carries that number reaches the lock, and only while the
same lock is up. A lock taken down, or one that stopped waiting after 30
seconds, forgets it. The answer goes in as the lock's own `Answer`, case
for case. A typed PIN or code moves from the lock's wiped store into the
protocol's `Secret` with no copy left behind: the lock's buffer is taken
(`Typed::take`), not copied, the secret wipes it when the message is
dropped after it is written, and the lock's picture with digits on it is
wiped by the lock. The bitmaps the lock's pictures are copied into are
wiped when their windows go. Nothing typed is ever logged.

**The PC's clock.** The lock is told the PC's time zone offset now
(`GetTimeZoneInformation`, with daylight time when in force) and whether
the signed in user's own Windows time format is a 12 or a 24 hour clock
(`LOCALE_STIMEFORMAT`: an `H` in it is 24 hours, an `h` 12, text in quotes
left out). Both are read again with every show and every second while a
lock is up.

**The time left display and its notices.** Two small layered windows that
take each pixel's alpha, so their corners and the space beside their hard
shadows are clear: never activated, never taking the keyboard, above
ordinary windows and put back on top four times a second. The time left
display starts above the taskbar's clock: its card 16 design pixels in from
the right edge of the main monitor's work area and 14 above its bottom, as
the design places it over a 48 pixel taskbar, at that monitor's scale, with
its hard shadow beyond (the design's 3 pixels for the display, 4 for a
notice). It can be dragged with the mouse (the pointer shows it can move);
a drag is kept inside the work area of the monitor it is over, or the
nearest, shadow and all, and it keeps its place, by its card's bottom right
corner, while the helper runs. A notice appears above it, the two cards'
right edges in line and 14 design pixels apart, or below it when there is
no room above, and kept on the screen. OK puts a
notice away; only a click on the OK button's place, which the crate gives,
does. A warning goes by itself after 30 seconds and a notice of a program
closed after 10, as the plain display's do; a newer notice is shown over an
older one, which comes back when the newer goes, if its own time has not
run out. The "warning shown" confirmation is sent as before, once the time
left display is on the screen with the warning in its show. While a lock
is up, neither is shown; a notice that comes meanwhile waits its time.

**A program closed.** The notice says "Game Launcher was closed." with "It
is blocked by your family's rules." under it, as designed, from the name
the service sends (`Notice::Blocked` in `limo-screens`): the program's own
description, as in the design's sample, or its file name ("Game.exe was
closed.") when its file carries none. Without a name, the notice says "A
program was closed." with the service's sentence under it.

## The data folder, and what each account can see

`limo install` makes `%ProgramData%\LiMo` itself, refusing if it already
exists (a standard account can make folders in ProgramData, so one that is
there may not be LiMo's), with its protection set from the first moment:
`D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)`, SYSTEM and Administrators full
control, nothing inherited from ProgramData, and nobody else anything. The
family's file (with the PIN record and the code secrets), the log and the
settings file are made inside it and get the same.

`limo status` from:

- **an elevated terminal**: the service's state, the test cap, and the last
  20 lines of the log;
- **an administrator's terminal that is not elevated, or a standard
  account**: the service's state only (Windows lets any interactive user
  ask for it), and a line saying the log is in a folder only SYSTEM and
  Administrators may read. An administrator not elevated has the
  Administrators group for denying only, so Windows refuses it too.

## What is logged

One line each, with the local time, in `%ProgramData%\LiMo\limo.log`, moved
to `limo.log.old` past 1 MiB. Control characters are taken out of every
line, so nothing can forge a line. Logged: the service starting and
stopping; each helper started (with what the launch did, and `gap_ms` when
it replaced one that died; at most every 10 seconds for one session, with a
count); what each helper says it got; a helper ended, closed and why, or
not quitting; each lock confirmed; a launch step that failed; programs
closed, spared or not closed, by file name (a close with the program's
description after it, when its file has one); ways out tried and their
outcome; the fallback disconnecting a session; the test cap; the engine's
problems; a gap between two looks of more than 5 seconds, or the two clocks
moving apart (a sleep, or a clock set); the phone notice; a helper that
shows the plain display, and why; `install`, `dev-setup` and `dev-code`
(with no names, and no code). Anything that goes on happening is logged
once and then at most every ten minutes.

Never logged: a PIN or a code; the reason a child typed; a window title, a
file being edited or a web address (LiMo never has them); a program's
folder; a child's name. `dev-code` prints its code in the terminal it runs
in, and nowhere else. The line for what a helper got carries its process
object's access list, with the user's security identifier in it.

## Tests

`cargo test` runs three groups.

| Where | What | Count |
| --- | --- | --- |
| `src/*.rs` | The channel's frames and limits, wiping and hiding secrets; the helper's side and the outbox; the choice of display and the fallback to the plain one; the command line (with `--plain` and `dev-code`); the never close list and paths; names from version resources; a description trimmed, collapsed, cleaned and cut, and the name a notice shows, by description or file name; quiet logging; settings and places, and the switch for the plain display; the plain display's words; what a helper got; `dev-setup` and `dev-code` on a real file, the code tried on the device it was made for | 32 |
| `src/drawn/tests.rs` | The drawn display's decisions, with no window: Windows keys to the lock's keys, with the layout's text handed in; the modifiers and the keyboard state; text and points from Windows; the PC's clock from the time format; where the time left display starts, a drag kept on the screen, where a notice goes; pixels into a bitmap, and only a change saying so; the notices' turns; a way out sent once and only its own answer back to the same lock; an answer for a lock taken down, or one that waited too long, dropped; a PIN moved into its secret with nothing left behind; a reason typed with the layout's text, shortcuts typing nothing; keys reaching nothing but a lock; the same lock again keeping what the child did; the time left display and its notices only while the PC may be used, OK, and the notice by name; the lock drawn on the main monitor and the others; the service's answers as the lock's | 19 |
| `src/win/*.rs` | Pure functions, and harmless reads of the test's own process only: the time zone offset; the clocks, and this account's own time format; session states and administrators; this process's own account; the order version resources are read in; this process's own entry, file and version resource (none); frames across a pipe inside this process; the three access lists and that they parse for this user; the command line a helper gets, with and without `--plain`; a log line; full screen; a helper run by hand doing nothing; what this process got | 19 |
| `tests/service/` | The stories, against a fake PC with the real engine and a real family file: facts, helpers, the channel, the orders and the fallback (below) | 36 |
| `tests/house_rules.rs` | Unsafe code only in `src/win/`; no allow attributes; no dash punctuation | 3 |

The stories: `facts.rs` (6): no fresh report counts as input just now; a
fresh report's idle, full screen and program in front; the program in front
only from the same session, never LiMo's own; version resources read once
per file; session 0 never in the facts, administrators marked; an
administrator's account not linked never locked. `helpers.rs` (10): one per
session that needs one; back within a step; a helper that ends at once
started again after a second; a failing launch tried every second, logged
once; no hello, replaced; no longer needed, told to quit and ended;
handed over at once and every five seconds; the child's name; a session
ending; a clean stop. `channel.rs` (7): eleven kinds of bad input each
closing the channel and restarting the helper, none reaching the engine; a
helper that stops reading; a way out belongs to its helper's session; a PIN
or code never logged, kept or shown; one way out a second; a helper that
shows the plain display, logged in plain words, one line, cut short; a way
out from a session not a child's refused. `orders.rs` (7): a warning held
until shown, its words the time actually left; held until its lock time; a
blocked program closed in its own session only, every copy, logged once,
its notice with the program's name; a closed program named by its file's
description, or its file name when it has none, a long one cut and control
characters dropped, the file still read once, and nothing of it in the
facts or the family's records; LiMo's own processes and the parts of
Windows never closed; phones ignored and said once; problems logged once in
a while.
`fallback.rs` (6): disconnected after 5 seconds unconfirmed; confirmed in
time, left alone; a helper that dies must confirm again; only a lock in a
session on the screen; the test cap; no cap, the lock stays.

None of the tests starts or stops a service, installs anything, writes
under Program Files or ProgramData, ends, suspends or launches a process,
locks or disconnects a session, or shows a window. The one window made is
the band check's, 1 by 1 pixel, off screen, never shown, and destroyed at
once, as the spike's `bandcheck` did.

`cargo build`, `cargo test`, `cargo doc --no-deps` and
`cargo clippy --all-targets -- -D warnings` pass with no warnings and no
allow attributes. Dependencies are built optimised in debug and test builds,
as in the hub and the agent, so the slow password and PIN hashes stay quick.

## Left open

- **What a confirmed release means on the master PC**: logged, nothing else.
- **No family's file, no limits.** While the family's file cannot be opened
  (such as while `dev-setup` has it), the service tries again every five
  seconds and limits nothing meanwhile.
- **Daylight saving** reaches the facts, but each child's home zone is a
  fixed offset in the rules (the agent's README says so).
- **The lock and the mouse**: the lock keeps the keyboard; the mouse moves,
  over LiMo's own windows only.
- **Only real hardware can tell** how the drawn display looks and feels on
  a screen: that the pictures are sharp at every monitor's own scaling; that
  the keyboard hook's keys come with the layout the child last used (the
  helper's own thread's layout is asked), and Caps Lock as its light shows;
  that a drag and a click on OK never activate the small windows; that the
  hook always returns in time while a large picture is being drawn (a
  first draw at 3840 by 2160 takes longest); and that the pictures go on
  monitors of different scalings in the right places. [TESTING.md](TESTING.md),
  parts I to L, is where.
- **The plain display names no program.** It shows the engine's sentence,
  which names the file ("gamelauncher.exe is blocked by your family's
  rules, so LiMo closed it."); only the drawn notice shows the description.
- **A description is the maker's word.** Whoever builds a program writes
  its description, so a program can call itself anything there. It only
  changes the name a notice shows to the child; what is blocked, counted
  and recorded never comes from it.

## Dependencies and licences

| Crate | Version | Licence | Used for |
| --- | --- | --- | --- |
| limo-agent, limo-hub, limo-core | 0.1.0 (path) | Apache-2.0 | the engine, the family's file, the rules |
| limo-screens | 0.1.0 (path) | Apache-2.0 | the lock screen, the time left display and its notices, drawn in software |
| tiny-skia, tiny-skia-path | 0.12.0 | BSD-3-Clause | through limo-screens: shapes and the pixel buffer |
| resvg, usvg | 0.50.0 | Apache-2.0 or MIT | through limo-screens: the design's icons |
| harfrust | 0.16.0 | MIT | through limo-screens: shaping text |
| read-fonts, font-types | 0.47.0, 0.12.7 | MIT or Apache-2.0 | through limo-screens: reading the fonts |
| ab_glyph_rasterizer | 0.1.10 | Apache-2.0 | through limo-screens: glyphs into pixels |
| the fonts Fraunces, Instrument Sans and Spline Sans Mono | as Google Fonts publishes them | SIL Open Font License 1.1 | through limo-screens, built into the program: the design's type |
| windows, windows-core, windows-result, windows-strings, windows-link, windows-numerics, windows-collections, windows-future, windows-threading | 0.62.2, 0.62.2, 0.4.1, 0.5.1, 0.2.1, 0.3.1, 0.3.2, 0.3.2, 0.2.1 | MIT or Apache-2.0 | calling Windows; the same version as the first spike |
| windows-implement, windows-interface | 0.60.2, 0.59.3 | MIT or Apache-2.0 | used by windows-core when building |
| windows-service | 0.8.1 | MIT or Apache-2.0 | the service control manager; the same version as the first spike |
| windows-sys, widestring, bitflags | 0.61.2, 1.2.1, 2.13.2 | MIT or Apache-2.0 | parts of windows-service |
| zeroize | 1.9.2 | Apache-2.0 or MIT | wiping a typed PIN or code, and the frames that carried it |
| serde, serde_json | 1.0.229, 1.0.154 | MIT or Apache-2.0 | the channel's messages and the settings file |
| syn | 2.0.119 | MIT or Apache-2.0 | used by windows-implement and windows-interface when building |
| tempfile | 3.27.0 | MIT or Apache-2.0 | tests only: a family file in a temporary folder |

Everything else under these is listed, with its licence, in the hub's, the
core's and the screens' READMEs; what limo-screens brings resolves to the
same versions here as there. Among those: arrayref (BSD-2-Clause),
foldhash and zlib-rs (Zlib), yoke and zerofrom (Unicode-3.0), adler2
(0BSD or MIT or Apache-2.0) and memchr (Unlicense or MIT), where LiMo
takes MIT or Apache-2.0 when a choice is offered. Nothing is GPL or AGPL.
The fonts are the one thing not under a software licence: the SIL Open
Font License 1.1 lets them be bundled with software as long as each copy
carries their copyright and licence, which `../screens/fonts/` does; the
screens' README says more. The code here was written for LiMo; the guarded
launch, the overlay and the helper's report are ported from LiMo's own
first spike. The crate is under the Apache License 2.0, as is all of LiMo.

## Layout

```
windows/
  Cargo.toml, Cargo.lock, README.md, TESTING.md, .gitignore
  src/
    main.rs       limo.exe
    lib.rs        the crate's notes and its modules
    os.rs         the Os trait: the seam
    service.rs    the service's work: facts, orders, helpers, fallback
    protocol.rs   the channel: messages, frames, limits, Secret
    helper.rs     the helper's side, the Display trait, the Outbox, the
                  choice of display and the fallback to the plain one
    drawn.rs      everything the drawn display decides
    drawn/tests.rs  its tests
    close.rs      the never close list and paths
    programs.rs   programs and their cache, the name a notice shows
    quiet.rs      keeping the log readable
    settings.rs   names, places, the settings file
    cli.rs        the command line
    setup.rs      dev-setup's and dev-code's work on the family's file
    text.rs       the plain display's words
    about.rs      words for what a helper got
    win/          everything that calls Windows
      mod.rs        commands, handles, strings, errors
      service.rs    the service control manager's entry, the service thread
      os.rs         WinOs, the real Os
      sessions.rs   sessions, accounts, administrators, disconnecting
      processes.rs  processes, version resources, ending a process
      launch.rs     the guarded launch, with the channel
      pipe.rs       the pipes, and their reader and writer threads
      helper.rs     limo helper: its threads and its loop
      drawn.rs      the drawn display: its windows, bitmaps and hook
      display.rs    the plain display
      about.rs      reading what a helper got
      clock.rs      the clocks and local times
      logfile.rs    the log
      console.rs    typing without it showing
      install.rs    install, uninstall, status, dev-setup, dev-code
  tests/
    house_rules.rs
    service/      main.rs, common.rs (the fake PC), facts.rs, helpers.rs,
                  channel.rs, orders.rs, fallback.rs
```
