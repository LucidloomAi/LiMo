# limo-windows

LiMo for Windows: the Windows service on the master PC, built around the
agent's engine ([`../agent`](../agent/)), and the helper it starts in each
signed in session that needs one. One program, `limo.exe`, does both, and
installs and removes itself for testing.

**What it is not.** It is not the polished lock screen or time left display:
another piece draws those later, to the agreed design in
[`../design`](../design/), behind the `Display` trait described below. It
has no network, no guardian's console and no installer for families. It has
been built and tested on the development PC against a fake PC only: nothing
here has run as a service yet. [TESTING.md](TESTING.md) is how it will be
tried, with the owner at the PC.

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
| `limo service` | the service control manager | The service. Run from a terminal it says so and does nothing. |
| `limo helper --channel R:W` | the service | The helper. Run by hand, without the channel it was born with, it says so and does nothing. |

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
  reader thread  -> user interface thread (HelperCore + Display)
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
  the helper's side and the `Display` trait. `quiet.rs`: keeping the log
  readable. `settings.rs`, `cli.rs`, `setup.rs`, `text.rs`, `about.rs`: names
  and places, the command line, `dev-setup`'s work, the plain display's
  words, and words for what a helper got.

## Once a second: the facts

| Fact | Where it comes from |
| --- | --- |
| `wall_secs` | The system clock. |
| `steady_ms` | `QueryInterruptTime`, in milliseconds: counted from start up, and it cannot be set. Its own page does not say whether it counts through sleep; the page of its "unbiased" twin says that one "does not include time the system spends in sleep or hibernation", unlike this one, which Windows uses so that timers that would have run out during a sleep run out on waking. Not yet seen through a real sleep; TESTING.md checks it. |
| `utc_offset_secs` | `GetTimeZoneInformation`: the zone's bias, and the daylight bias when daylight time is in force. |
| Sessions | `WTSEnumerateSessionsW`: every session but session 0 with a user name. The account's SID and whether it is an administrator come from the session's user token (`WTSQueryUserToken`): the Administrators group among the token's groups, enabled or kept for denying only (a UAC filtered token), or among its linked token's. The state: in use when the session is active and Windows does not say it is locked (`WTSSessionInfoEx`), locked when it does, disconnected otherwise. |
| Programs running | Read by the service itself, never taken from a helper: `WTSEnumerateProcessesExW` for every process with its session, then for each one its start (`GetProcessTimes`) and its file's path (`QueryFullProcessImageNameW`). A program is the file's name, and the original file name and company name from its version resource. LiMo's own processes are left out. |
| Idle, front, full screen | From the session's helper, about once a second: milliseconds since the last input (`GetLastInputInfo`), the process id of the window in front, and whether something is in full screen (the window in front covers its monitor and is not the desktop or the taskbar, or Windows says a full screen program or a presentation is running). |

- **Version resources are read once per file.** The cache keeps each
  file's program by its path and last write time, and each running
  process's by its id and start. A file is read again only after it is
  written. Test: `facts::a_files_version_resource_is_read_once_until_the_file_is_written`.
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
worth.

| Helper to service | |
| --- | --- |
| `hello` | First, and only first: the protocol version. |
| `about` | Once: what the helper got (UIAccess, integrity, band, its access list). |
| `report` | About once a second: idle milliseconds, the process id in front, full screen. |
| `lock_on_screen` | The lock handed over with this number covers every monitor. |
| `warning_shown` | This warning (its cause, minutes and lock time) is on the screen. |
| `ask_for_time`, `try_code`, `try_pin` | The three ways out, each with a number for its answer. |

| Service to helper | |
| --- | --- |
| `show` | What to show, with its number and the child's name. |
| `notice` | A program was closed, and why. |
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
and the ways out, which the engine judges. A helper that lies about idle
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
the helper got: `uiaccess=1 integrity=set dacl=admin-owner channel=inherited`.
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
every ten minutes with a count. A process Windows refuses to end is logged
the same way. Tests: `orders::a_blocked_program_is_closed_in_its_own_session_only_every_copy_and_logged_once`,
`orders::limos_own_processes_and_the_parts_of_windows_are_never_closed`, and
the unit tests in `src/close.rs`.

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

## The Display trait

What draws, in the helper (`src/helper.rs`). Its one implementation now is
the plain display in `src/win/display.rs`, ported from the spike's overlay:
a borderless window over each monitor for a lock, with the title, the
child's name, the message and when it opens again, and a small window by
the clock for the time left. No ways out on screen, no styling. The real
drawing replaces it, and must do exactly this:

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
- **`notice(text)`**: show that a program was closed, and why, for a while.
- **`answer(ask, answer)`**: show the answer to a way out it sent.
- **`pump()`**: called four times a second on the same thread: keep the lock
  on top, count down, let notices fade.

Ways out go to the service only through the `Outbox` the display is made
with (`ask_for_time`, `try_code`, `try_pin`); a typed PIN or code goes as a
`Secret` and is not kept. The display never shows or keeps a window title,
a file name or a web address; the service sends none. The helper reads the
class name of the window in front only to tell the desktop and the taskbar
from a program in full screen, and sends none of it.

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
closed, spared or not closed, by file name only; ways out tried and their
outcome; the fallback disconnecting a session; the test cap; the engine's
problems; a gap between two looks of more than 5 seconds, or the two clocks
moving apart (a sleep, or a clock set); the phone notice; `install` and
`dev-setup` (with no names). Anything that goes on happening is logged once
and then at most every ten minutes.

Never logged: a PIN or a code; the reason a child typed; a window title, a
file being edited or a web address (LiMo never has them); a program's
folder; a child's name. The line for what a helper got carries its process
object's access list, with the user's security identifier in it.

## Tests

`cargo test` runs three groups.

| Where | What | Count |
| --- | --- | --- |
| `src/*.rs` | The channel's frames and limits, wiping and hiding secrets; the helper's side and the outbox; the command line; the never close list and paths; names from version resources; quiet logging; settings and places; the plain display's words; what a helper got; `dev-setup` on a real file | 22 |
| `src/win/*.rs` | Pure functions, and harmless reads of the test's own process only: the time zone offset; the clocks; session states and administrators; this process's own account; the order version resources are read in; this process's own entry, file and version resource (none); frames across a pipe inside this process; the three access lists and that they parse for this user; the command line a helper gets; a log line; full screen; a helper run by hand doing nothing; what this process got | 18 |
| `tests/service/` | The stories, against a fake PC with the real engine and a real family file: facts, helpers, the channel, the orders and the fallback (below) | 34 |
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
ending; a clean stop. `channel.rs` (6): eleven kinds of bad input each
closing the channel and restarting the helper, none reaching the engine; a
helper that stops reading; a way out belongs to its helper's session; a PIN
or code never logged, kept or shown; one way out a second; a way out from a
session not a child's refused. `orders.rs` (6): a warning held until shown;
held until its lock time; a blocked program closed in its own session only,
every copy, logged once; LiMo's own processes and the parts of Windows never
closed; phones ignored and said once; problems logged once in a while.
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
- **The ways out on screen**, styling, and the other monitors' plain page are
  the drawing piece's.
- **The lock and the mouse**: the plain lock keeps the keyboard; the mouse
  moves, over LiMo's own windows only.

## Dependencies and licences

| Crate | Version | Licence | Used for |
| --- | --- | --- | --- |
| limo-agent, limo-hub, limo-core | 0.1.0 (path) | Apache-2.0 | the engine, the family's file, the rules |
| windows, windows-core, windows-result, windows-strings, windows-link, windows-numerics, windows-collections, windows-future, windows-threading | 0.62.2, 0.62.2, 0.4.1, 0.5.1, 0.2.1, 0.3.1, 0.3.2, 0.3.2, 0.2.1 | MIT or Apache-2.0 | calling Windows; the same version as the first spike |
| windows-implement, windows-interface | 0.60.2, 0.59.3 | MIT or Apache-2.0 | used by windows-core when building |
| windows-service | 0.8.1 | MIT or Apache-2.0 | the service control manager; the same version as the first spike |
| windows-sys, widestring, bitflags | 0.61.2, 1.2.1, 2.13.2 | MIT or Apache-2.0 | parts of windows-service |
| zeroize | 1.9.2 | Apache-2.0 or MIT | wiping a typed PIN or code, and the frames that carried it |
| serde, serde_json | 1.0.229, 1.0.154 | MIT or Apache-2.0 | the channel's messages and the settings file |
| syn | 2.0.119 | MIT or Apache-2.0 | used by windows-implement and windows-interface when building |
| tempfile | 3.27.0 | MIT or Apache-2.0 | tests only: a family file in a temporary folder |

Everything else under these is listed, with its licence, in the hub's and
the core's READMEs. Nothing is GPL or AGPL. The code here was written for
LiMo; the guarded launch, the overlay and the helper's report are ported
from LiMo's own first spike. The crate is under the Apache License 2.0, as
is all of LiMo.

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
    helper.rs     the helper's side, the Display trait, the Outbox
    close.rs      the never close list and paths
    programs.rs   programs and their cache
    quiet.rs      keeping the log readable
    settings.rs   names, places, the settings file
    cli.rs        the command line
    setup.rs      dev-setup's work on the family's file
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
      display.rs    the plain display
      about.rs      reading what a helper got
      clock.rs      the clocks and local times
      logfile.rs    the log
      console.rs    typing without it showing
      install.rs    install, uninstall, status, dev-setup
  tests/
    house_rules.rs
    service/      main.rs, common.rs (the fake PC), facts.rs, helpers.rs,
                  channel.rs, orders.rs, fallback.rs
```
