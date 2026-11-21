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

The service also serves the door of LiMo's settings: a named pipe through
which the guardian's console, a window in a guardian's own session, asks
the service for everything it shows and changes, since the service alone
holds the family's file. See [The console's door](#the-consoles-door).

And it serves the link with the family's phones (`../link`): it listens for
them on TCP port 47810, pairs a phone with a pairing offer the console asks
for, answers each phone's reports with the family file's grants, so that a
phone and the PC spend one budget, asks a phone to report when the books
want to hear from it, and announces itself on the home network while the
family has a phone. See [The link with the phones](#the-link-with-the-phones).

**What it is not.** It has no window of its own for the guardian (the
console is a program of its own, not built yet; `limo dev-console` knocks
on the door by hand meanwhile), no installer for families, and it adds no
firewall rule (see [The firewall rule](#the-firewall-rule)). It first ran
as a service on a real PC on 19 November 2026, with the owner, and the
drawn display was on the screen then; [TESTING.md](TESTING.md) says how it
is tried and what was seen. The door has been built and tested on the
development PC, against the fake PC and over a real named pipe in the
tester's own account; its run at the PC is TESTING.md, part O. The link has
been tested over real loopback sockets with a phone played by the link's
own phone side; its run with a second PC is TESTING.md, part P.

## Contents

- [Commands](#commands)
- [How it is put together](#how-it-is-put-together)
- [Once a second: the facts](#once-a-second-the-facts)
- [The orders](#the-orders)
- [The helpers](#the-helpers)
- [The channel](#the-channel)
- [The console's door](#the-consoles-door)
- [The lock of an account not set up](#the-lock-of-an-account-not-set-up)
- [The link with the phones](#the-link-with-the-phones)
- [The announcement on the home network](#the-announcement-on-the-home-network)
- [The firewall rule](#the-firewall-rule)
- [The example phone](#the-example-phone)
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
| `limo reset-password` | administrator | For a family that forgot the guardian password: a new one, typed twice at the prompt and never shown, checked for its length before anything is touched. The service holds the family's file, so it is stopped for a moment and started again. The wrong guesses and their wait are cleared, the family's events record a reset, and everything else is kept: the PIN, the children, their rules and their devices. Takes nothing on its command line, where a terminal would keep a password. |
| `limo dev-console` | anyone | Scaffolding until the guardian's console exists: knocks on the door of LiMo's settings from the account it runs in, as the console will, and sends one request for each line typed (`help` lists them: `state`, `create-family`, `sign-in`, `set-pin`, `add-child`, `minutes`, `accounts`, `link`, `free`, `finish`, `today`, `offer`, `remove-device`, `request NAME JSON`). It sends nothing unless the door is served from session 0, where services run. A password or a PIN is asked for at a prompt that does not show it, never taken on the line. |
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
        |
        |  the console's door (src/win/door.rs): a thread that takes
        |  each connection to \\.\pipe\limo.console, and a thread for
        |  each connection; what comes wakes the service thread, which
        |  answers between two steps (src/service/door.rs)
        v
helper in each session that needs one  (src/win/helper.rs)
  reader thread  -> user interface thread (HelperCore + Display:
                    the drawn display, or the plain one in its place)
  report thread  -> writer thread -> the service
  sound thread   -> report thread (whether sound plays: src/win/playing.rs)
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
  which hand messages to and from the service thread through channels. The
  console's door wakes the service thread through the same channel as
  "stop", and the thread answers the door (`Service::door`) between two
  steps; the steps keep their cadence whatever the door does.
- **Unsafe code only in `src/win/`.** Every other module says
  `#![forbid(unsafe_code)]`, and a test (`tests/house_rules.rs`) checks it.
  Each unsafe block says why it is sound.
- **What is decided, and where.** `service.rs`: the facts, the orders, the
  helpers, the held warning, the fallback, the test cap. `close.rs`: the
  never close list. `programs.rs`: programs and their cache.
  `protocol.rs`: the channel's messages, frames and limits. `helper.rs`:
  the helper's side, the `Display` trait, the choice of the drawn or the
  plain display, and what counts as playing sound or keeping the screen
  awake. `drawn.rs`: everything the drawn display decides. `quiet.rs`:
  keeping the log readable. `settings.rs`, `cli.rs`, `setup.rs`, `text.rs`,
  `about.rs`: names and places, the command line, `dev-setup`'s,
  `dev-code`'s and `reset-password`'s work, the plain display's words, and
  words for what a helper got. `console.rs`: the door's messages, frames and
  limits. `service/door.rs`: everything the door decides.
  `service/unlinked.rs`: the guardian PIN on the lock of an account not set
  up. `devconsole.rs`: `dev-console`'s lines.

## Once a second: the facts

| Fact | Where it comes from |
| --- | --- |
| `wall_secs` | The system clock. |
| `steady_ms` | `QueryInterruptTime`, in milliseconds: counted from start up, and it cannot be set. Its own page does not say whether it counts through sleep; the page of its "unbiased" twin says that one "does not include time the system spends in sleep or hibernation", unlike this one, which Windows uses so that timers that would have run out during a sleep run out on waking. Not yet seen through a real sleep; TESTING.md checks it. |
| `utc_offset_secs` | `GetTimeZoneInformation`: the zone's bias, and the daylight bias when daylight time is in force. |
| Sessions | `WTSEnumerateSessionsW`: every session but session 0 with a user name. The account's SID and whether it is an administrator come from the session's user token (`WTSQueryUserToken`): the Administrators group among the token's groups, enabled or kept for denying only (a UAC filtered token), or among its linked token's. The state: in use when the session is active and Windows does not say it is locked (`WTSSessionInfoEx`), locked when it does, disconnected otherwise. |
| Programs running | Read by the service itself, never taken from a helper: `WTSEnumerateProcessesExW` for every process with its session, then for each one its start (`GetProcessTimes`) and its file's path (`QueryFullProcessImageNameW`). A program is the file's name, and the original file name and company name from its version resource. The same resource's description (`FileDescription`, such as "Game Launcher") is read beside them and kept in the cache with them, but never handed to the engine: it only names a program in the notice that it was closed, and in the log line for the close. All three are properties of the program's file, written in by its maker and the same for every copy; none is a window title, a document's name or a web address, and LiMo reads none of those. LiMo's own processes are left out. |
| Idle, front, full screen, playing | From the session's helper, about once a second: milliseconds since the last input (`GetLastInputInfo`), the process id of the window in front, whether something is in full screen (the window in front covers its monitor and is not the desktop or the taskbar, or Windows says a full screen program or a presentation is running), whether a program of the session plays sound, and whether a program keeps the screen awake (both below). The engine's `playing` is true when either of the last two is. |

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
  not full screen, nothing playing, nothing in front. A missing helper never
  gives free time.
  Test: `facts::with_no_fresh_report_input_counts_as_just_now_so_a_missing_helper_gives_no_free_time`.
- **Sound, or the screen kept awake, keeps the count going.** A session is
  also counted with nobody at the keyboard or the mouse while a program of
  it plays sound, or while a program keeps the screen awake (the owner's
  decision of 19 November 2026: in the first run a video left playing in a
  browser window, not in full screen, stopped being counted after the idle
  minutes, and 13 minutes of screen time took 31). The helper says each in
  its report; the service hands the engine one fact, `playing`, true when
  either is, and the engine treats it as it treats full screen (its README,
  rule 2). A PC that is silent and left alone still pauses. Test:
  `facts::sound_or_the_screen_kept_awake_keeps_the_count_going_with_no_input`.

**Sound** (`src/win/playing.rs`; what counts is decided in `src/helper.rs`,
`audio_session_plays`). On a thread of its own, about once a second, the
helper asks Windows for every sound device that plays (`IMMDeviceEnumerator::EnumAudioEndpoints`
for `eRender` in `DEVICE_STATE_ACTIVE`, which its page explains as "the
audio adapter that connects to the endpoint device is present and
enabled"): every one, not only the default, since a program can play to a
device that is not the default. On each it activates the session manager
(`IMMDevice::Activate` for `IAudioSessionManager2`, as that interface's page
says) and lists the device's audio sessions (`GetSessionEnumerator`). An
audio session counts when all of these hold:

- it is active (`IAudioSessionControl::GetState`, whose page says "The
  state is active if the session has one or more streams that are
  running");
- it is not the system sounds session (`IAudioSessionControl2::IsSystemSoundsSession`,
  `S_OK` for it). That session carries Windows' own short sounds, a
  notification's chime or an error's ding, for every program at once: not
  something a child plays, and not one program of the session. It never
  counts;
- the process that made it (`IAudioSessionControl2::GetProcessId`; for a
  session of several processes its page says it gives "the initial
  identifier of the process that created the session") is in the helper's
  own Windows session (`ProcessIdToSessionId`, "the Remote Desktop Services
  session under which the specified process is running"), so sound from
  another account's session, or from Windows' own in session 0, never
  counts for this one. **Expected, and not documented:** that Windows
  answers it for the browser's or the player's own process. Its page says
  the caller needs "the PROCESS_QUERY_INFORMATION access right" to the
  process; the helper runs as the child, one integrity step above the
  child's programs, so it should have it for them. Where it is refused,
  that audio session counts as no; TESTING.md part E, step 9, in a browser
  tab, shows it;
- its loudness meter is above zero (`IAudioMeterInformation::GetPeakValue`,
  asked of the audio session itself). The meter's page says the value is
  "a number in the normalized range from 0.0 to 1.0" and is "the peak value
  recorded during the previous device period". A stream that is open and
  silent, such as a paused video, reads 0.0 and does not count; the
  quietest sound heard does. **Expected, and not documented:** the meter's
  pages describe it for a whole device, got through `IMMDevice::Activate`,
  and no page says that an audio session gives a meter of its own. The
  helper asks each audio session for one (`QueryInterface`); a session that
  gives none counts as silent, so if Windows never gives one, sound never
  counts and only the screen kept awake does. Music alone, in TESTING.md
  part E, step 9, is what shows it. Were the meter the device's and not the
  session's, an active but silent stream of this session would count while
  another one played on the same device; no step tells the two apart.

Anything Windows will not say counts as no. A meter reads only the last few
milliseconds, so a pause between two sentences of a video can read as
silence: sound heard counts for 3 seconds (`SOUND_HELD_MS`), and a paused
video stops counting that long after its sound stopped. That also makes a
read that fails, or a thread that is stuck, say no within 3 seconds, so the
audio service can never hold back the rest of the report. COM is started on
that thread for the multithreaded apartment (`CoInitializeEx`) and ended
there when the thread ends (`CoUninitialize`); every interface a read makes
is released before the read ends. Each read starts afresh, so a device
plugged in (headphones) is read from the next second, a device taken away
in the middle of a read fails only its own calls, and a PC with no sound
device, or with the Windows audio service stopped, reads no. A read takes
about 3 ms on the development PC.

**The screen kept awake** (`CallNtPowerInformation` for
`SystemExecutionState`, on the report thread). Its page says that for this
level "The lpInBuffer parameter must be NULL", and that the output
"receives a ULONG value containing the system execution state buffer",
which "may contain any combination of the following values:
ES_SYSTEM_REQUIRED, ES_DISPLAY_REQUIRED, or ES_USER_PRESENT". With
`ES_DISPLAY_REQUIRED` set, a program keeps the screen awake; the page of
`SetThreadExecutionState` says "Multimedia applications, such as video
players and presentation applications, must use ES_DISPLAY_REQUIRED when
they display video for long periods of time without user input".
**Expected, and not documented:** that browsers and video players do so
while a video plays, muted or not, and that they ask in a way that shows in
this state. A program can also ask through a power request (`PowerSetRequest`
with `PowerRequestDisplayRequired`), and neither page says whether such a
request shows in the execution state. A muted video in a browser, TESTING.md
part E, step 7, is what shows it; where it does not, a muted video pauses
after the idle minutes, as before. A read that fails says no. **It is the
whole PC's state, not one session's**: a program keeping the screen awake
in another account's session counts for the child's session in use too.
That is accepted because the engine counts only the session in use, and
this only matters while that session would otherwise be idle.

**Whether, never what.** Of a sound LiMo learns only whether it is coming
out: one meter, compared with zero. No sound is captured, recorded,
analysed or kept. Of an audio session the helper asks only the four things
above; its name, its icon and its identifier, which can name a program's
file, are never asked for. The process id is used once, to ask its Windows
session, and forgotten. The report carries two yes or no answers, and the
engine one; nothing of what plays, or which program plays it, reaches the
service, the engine or the family's file.

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
- **The lock of an account not set up** takes the guardian PIN; see
  [The lock of an account not set up](#the-lock-of-an-account-not-set-up).
- **`prod_devices`**: each phone named, when a session of its is open, is
  asked to report at once, at most every five seconds; one out of reach is
  asked when it comes. See [The link with the phones](#the-link-with-the-phones).
  Tests: `link::a_lock_from_the_console_reaches_the_phone_within_seconds`,
  `orders::a_phone_the_books_want_to_hear_from_with_no_session_open_changes_nothing_here`.
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

**One thing only real hardware could tell.** Microsoft's page for
`CreateProcessAsUserW` says, of `bInheritHandles`, under "Terminal
Services": "You cannot inherit handles across sessions. Additionally, if this
parameter is TRUE, you must create the process in the same session as the
caller." The service is in session 0 and the helper in the user's, so read
literally this forbids the design. The remark dates from Windows XP, when a
process for another session was made by a helper inside that session; since
Windows Vista the caller makes the process itself, and the handles it
inherits are copied from the caller's own table, whatever the session. That
was the reason to expect it to work, and why it was built as the brief asks.
It holds: in the first run on a real PC (19 November 2026, Windows 11 Home,
build 26200) the helper, started in the child's session by the service in
session 0, was born with both ends and said hello at once, every time it was
started (`channel=inherited` in the log). Where it ever did not hold, the
helper would never be started (or never say hello): the log says so every
second, nothing is shown, and the fallback disconnects any session ordered
locked. There is no quiet fallback to a pipe anyone can open. TESTING.md,
part D, is where this is checked.

**Frames.** Each message is the protocol version (2 bytes), the length of
what follows (4 bytes), both little endian, then that many bytes of JSON. A
message may be at most 64 KiB (`MAX_MESSAGE_BYTES`); the longest real one is
under 2 KiB. A frame that says it is longer is refused from its header,
before any of it is kept, and a reader never holds more than two frames'
worth. The protocol is version 3: version 2 added the program's name to a
notice, and `shows_plain`; version 3 added to a report whether the session
plays sound and whether a program keeps the screen awake. A helper and a
service of different versions never talk: the first frame closes the
channel.

| Helper to service | |
| --- | --- |
| `hello` | First, and only first: the protocol version. |
| `about` | Once: what the helper got (UIAccess, integrity, band, its access list). |
| `report` | About once a second: idle milliseconds, the process id in front, full screen, whether a program of the session plays sound (`playing_sound`), and whether a program keeps the screen awake (`screen_kept_awake`). Each of the last two is only a yes or a no, and both must be there. |
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
`window_title`, say) or without one it must have (a report that says
nothing of sound); a message before hello, hello twice, or "what I got"
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

**What the service believes from a helper.** Only the things a program
inside the session must say (idle, the process in front, full screen,
sound, the screen kept awake), and only while fresh; the confirmations of a
lock and a warning; the ways out, which the engine judges; and its word
that it shows the plain display, which only goes in the log. A helper that
lies about idle time could stop the count: that is why the helper is
guarded (below), so that the child's own programs cannot reach it, its
channel or its handles. A helper that said something plays when nothing
does could only make more time count, never less.

## The console's door

The guardian's console never opens the family's file: the hub holds it
alone, in SQLite's exclusive mode, on the service's one thread. The console
asks the service through the door, a named pipe that only the service makes
(`src/win/door.rs`). What the door decides is in `src/service/door.rs`,
written against the `Os` seam and tested against the fake PC with a real
family file; its messages and frames are in `src/console.rs`.

**The pipe.** `\\.\pipe\limo.console`.

- Its access list is `D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x12019b;;;IU)`:
  SYSTEM and Administrators full control; interactive users read, write and
  the attributes (`0x12019b`), never the right to make an instance of the
  pipe (`FILE_CREATE_PIPE_INSTANCE`, the same bit as `FILE_APPEND_DATA`).
  Windows checks that right when a program makes an instance with
  `CreateNamedPipeW`, not when a client opens the pipe; the tests show a
  process that is not elevated refused a second instance of a pipe it made
  itself, with this list. So no program in a child's account, or in any
  standard account, can serve the door in LiMo's place while it is open.
- The first instance is made with `FILE_FLAG_FIRST_PIPE_INSTANCE`, which
  fails when the name is already taken: a program that took the name before
  the service started is found out, said in the log, and the door is tried
  again every five seconds. The next instance is always made before a
  connected one is served, so the name is never without one of LiMo's while
  the door is open.
- `PIPE_REJECT_REMOTE_CLIENTS`: nobody on another PC can connect.
- A client opens the pipe at the identification level
  (`SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION`): a program posing as
  the door could learn who the client is, never act as it. The console
  checks that the door is served from session 0 before it sends a password
  (`GetNamedPipeServerProcessId`, `GetNamedPipeServerSessionId`);
  `limo dev-console` does so already.

**Who is knocking.** After a client's first bytes are read, its
connection's thread takes on the client's own token for a moment
(`ImpersonateNamedPipeClient`), reads the token's user (`OpenThreadToken`,
checked as the service itself) and goes back to its own (`RevertToSelf`)
before anything else; a thread that could not go back ends at once.
Microsoft's page says the server takes on "the security context of the last
message read from the pipe", which is why the first bytes are read first.
The client's process id and session (`GetNamedPipeClientProcessId`,
`GetNamedPipeClientSessionId`) go to the log only. At hello, and again at
every request after it, the service asks the family's file whose the
account is (`Hub::account_standing`):

- **a child's**: refused at once, as soon as Windows says whose it is
  (before hello), in the words of the sign in screen, "LiMo's settings
  cannot be opened from Asha's account. A guardian opens them from their
  own.", and closed a second later; nothing it sends is answered, and it
  never holds a place a guardian's console needs. An account linked to a
  child while a console is open in it is refused at its next request;
- **an account Windows would not say** (a client that opened the pipe
  anonymously): refused;
- **not linked, or marked not limited**: the door opens, and the guardian
  password is needed for anything but the first screen.

What could fool the refusal, and what holds then. The account is read from
the client's own token by Windows; nothing the client says goes into it, so
a process cannot claim another account. A child who can sign in to another
account that is not linked to a child (a guardian's, whose password the
child knows, or a spare one) can open the door from there, and still meets
the guardian password: the refusal is the first line, the password is the
lock. An account meant for a child but not yet linked is not refused; once
the first start is finished such an account is locked anyway. An
administrator can do anything on the PC, as with the data folder. The
account the console runs in is never linked to a child from the console
(refused with words), so a guardian cannot shut themselves out by a slip.
Its test needs no second Windows account: it links the test's own account
to a child and knocks over a real pipe, through the same seam.

**Frames.** As on the helper's channel: the protocol version (2 bytes), the
length (4 bytes), both little endian, then JSON; at most 1 MiB either way
(`MAX_CONSOLE_MESSAGE_BYTES`), refused from the header when longer; the
console's protocol is version 1. The console says hello first,
`{"say":"hello","protocol":1}`, and is answered
`{"say":"hello","protocol":1,"version":"0.1.0"}`, or
`{"say":"refused","message":"..."}`. Then requests, each with a number of
its own: `{"say":"request","id":7,"request":{"name":"lock_now","args":{"child":"..."}}}`,
answered `{"say":"answer","id":7,"reply":{"outcome":"done","value":...}}`,
the value in the hub's own shapes, or
`{"outcome":"problem","problem":{"kind":"...","message":"..."}}`, with
`wait_until`, `tries_before_wait` and `rule_problems` (each with its
`field`) where they apply. The kinds: `signed_out`, `wrong_password`,
`must_wait`, `invalid`, `rules`, `not_found`, `refused`, `busy`, `not_yet` (kept for later requests; none answers it now),
`unknown_request`, `no_family`, `too_big`, `failed`. A password or a PIN
travels as a `Secret` (hidden in `Debug`, wiped when dropped); every frame
read is wiped, as on the helper's channel. What is not wiped: an answer's
value, which is built as JSON before it is written (a one time code from
`make_code` is in it), and the hub's own copies.

**The requests.** One for each call of the family's file. Those marked "no"
are answered before sign in.

| Request | Arguments | The family's file | Sign in |
| --- | --- | --- | --- |
| `sign_in` | `password` | `check_password`, with its waits | no |
| `sign_out` | | ends this connection's session | no |
| `about` | | LiMo for Windows' version, the protocol, the PC's name | no |
| `family_state` | | `has_family`, `has_pin`, how many children, `unlinked_accounts`, whether signed in | no |
| `create_family` | `password` | `create_family`, while there is none; it signs the connection in | no |
| `set_pin` | `pin` | `set_pin` | yes |
| `change_password` | `old`, `new` | `change_password` | yes |
| `family_today`, `child_today` | `child` | `family_today`, `child_today` | yes |
| `add_child` | `name`, `age_band` | `add_child`, in the PC's time zone now | yes |
| `rename_child`, `set_age_band` | `child`, `name` or `age_band` | the same | yes |
| `remove_child` | `child`, `password` | the password again, then `remove_child` | yes |
| `rules`, `replace_rules` | `child`, `rules` | `rules`, `replace_rules` (refused with the rules' own problems) | yes |
| `usage` | `child`, `from_day`, `to_day` | `usage` | yes |
| `waiting_requests`, `answer_request` | `child`, `request`, `reply` | the same | yes |
| `give_extra` | `child`, `minutes`, `tap_id` | `give_extra` | yes |
| `lock_now`, `unlock_until`, `lift` | `child`, `until` | the same | yes |
| `make_code` | `device`, `minutes` | `make_code`: `shown`, `minutes`, `day`, `device_id` | yes |
| `devices`, `device`, `rename_device` | `child` or `device`, `name` | the same | yes |
| `remove_device` | `device`, `password` | the password again, then `remove_device` | yes |
| `accounts` | | every account of the PC (below) with its link | yes |
| `link_account`, `unlink_account` | `account`, `link` | the same, for an account of this PC only | yes |
| `pc_settings`, `set_idle_minutes`, `set_unlinked_accounts` | `minutes`, `unlinked` | the same | yes |
| `finish_first_start` | | `set_unlinked_accounts(Locked)` | yes |
| `events` | `limit`, `before` | `events` | yes |
| `allow_removal` | `password` | the password again; removal is allowed for ten minutes, kept in the service's memory for the uninstaller to ask (not built yet) | yes |
| `delete_family` | `password` | the password again, then `delete_family`; every session ends | yes |
| `make_offer` | `child` | a pairing offer for a phone of the child's, made by the link with the master's key, the link's port and the PC's home addresses, kept by `keep_offer`: `offer` (its `limo1:` text, which holds its secret: the console hands it to the phone and wipes it), `offer_id`, `expires_at` (ten minutes on), `port`, `addresses`, `master_id`. `failed` while the link is not running | yes |
| `device_link` | `device` | the device's open session, if it has one: `way` (`cable` or `home_network`), `opened_at`, `last_report_at` | yes |

`accounts` lists every normal account of the PC that is not switched off
(`NetUserEnum`, then `NetUserGetInfo` at level 23 for its full name and
security identifier; `src/win/accounts.rs`), with whether it is in the
Administrators group (its direct members, `NetLocalGroupGetMembers`, the
group found by its own identifier `S-1-5-32-544` since its name is in the
PC's language), whether someone is signed in to it now, whether this window
runs in it, and its link; then the links of accounts no longer on the PC,
so that they can be taken away. LiMo changes no account itself.

**Signing in, and the session.** The guardian password, through the family
file's own check and its waits: four wrong tries free (each answered "That
password is not right. 3 more tries before a wait." with the tries left),
the fifth sets a wait of a minute that doubles with each further one, up to
an hour, kept in the file across restarts and the same for every
connection; during a wait nothing is checked. A session belongs to one
connection, and ends with it, with `sign_out`, with `delete_family`, and
fifteen minutes after the last request that is not a look a screen repeats
by itself (`family_today`, `child_today`, `waiting_requests`, `devices`,
`device`, `device_link`, `about`, `family_state`): a screen left open does
not keep a guardian signed in. Then requests are answered `signed_out`, and
the connection stays open for a new sign in.

**Between two steps.** Requests wait in a queue and are answered by
`Service::door`, which the service's loop calls whenever something came and
after every step, never inside one: at most 16 in a turn, and at most one
that carries the password or the PIN, which is hashed slowly on purpose
(600,000 rounds for the password). So a step is late by at most one such
request, and the PC's counting, which goes by the steady clock, loses
nothing by it. A request done that changes something brings the engine's
call forward to the next step, so a lock from the console is on the child's
screen within a step. A request that takes 500 ms or more is logged.
Measured in `door::the_step_time_with_the_door_busy` (the test build, on the
development PC, 21 November 2026): a step took 0.13 ms on average with the
door idle and 0.16 ms with it busy (two consoles asking ten times a second
each, and a sign in every second, while a child's session counted), the
longest 6 to 8 ms and 12 to 13 ms (the engine's call writing the family's
file); the door's turn between two steps took 6.5 ms on average and at most
about 70 ms, for a sign in; and 30 seconds were counted in 30 seconds either
way.

**What closes a connection.** A frame of another version, longer than 1
MiB, or not JSON; a message nobody knows, or with a field it should not
have; a request before hello, hello twice, or hello in another version;
more than 20 messages in one second (`MAX_MESSAGES_PER_SECOND`, checked by
the connection's thread and by the service); no first bytes, or no hello,
within 10 seconds (`HELLO_WAIT_MS`); a console that does not read what it is
sent within five seconds; more requests waiting than LiMo takes at once. A
fifth connection while four are open is refused with words. The pipe's own
thread closes at once, before reading anything, a connection beyond four
from one Windows session (it knows the client's session as soon as it
connects) or beyond sixteen in all, so a program in a child's session that
opens connection after connection, saying nothing, never takes the places
of a guardian's console in another session. A request of
the right shape that this LiMo cannot read (an unknown name, or arguments of
the wrong kind) is answered `unknown_request` by its number, and the
connection stays. Nothing of a closed connection reaches the family's file,
and the log says why without quoting what was sent. Two passwords from one
connection within a second: the second is answered `busy`. Tests: every
test in `tests/service/door.rs` and `tests/service/pipe.rs`, and the unit
tests in `src/console.rs`.

## The lock of an account not set up

Once the first start is finished (`finish_first_start`), a Windows account
LiMo was not told about is locked (administrators never are: the agent's
README, rule 1). Its lock takes the guardian PIN, as decided on 18 November
2026: the service shows the engine's lock with the PIN offered when the
family has one, and answers the PIN itself, since the engine has no child
for the account (`src/service/unlinked.rs`). The PIN is checked by the
family's file (`Hub::check_pin`), with its own wrong guesses and wait, kept
in the file, by the same rule as the password's; the lock shows the wait.
A right PIN opens the account for what the guardian chose on the lock (the
minutes, or until a time, at most a day): it shows nothing until then, and
the lock comes back after. What it opened is kept in the service's memory
only, so a restart of the service locks the account again, the safe side.
The engine still notes once a day that the account was used. Confirming a
release there is refused with words; a one time code is a child's device's
and is refused there by the engine. Nothing of the PIN is logged. Test:
`door::the_lock_of_an_account_not_set_up_takes_the_guardian_pin`.

## The link with the phones

The link (`../link`, `limo-link`) is everything between a phone and the PC
but the sockets: frames, the two handshakes (`Noise_XXpsk3` to pair with an
offer, `Noise_KK` for every session after), messages, offers, timings. The
service owns the sockets (`src/phones.rs`) and answers what needs the
family's file on its one thread (`src/service/link.rs`).

**The listener.** TCP on port 47810 (`limo_link::DEFAULT_PORT`) on every
IPv4 address of the PC, loopback included: the USB cable's tunnel (Android's
`adb reverse`) arrives at the PC's loopback. One thread takes the
connections; each connection has a thread of its own, running the link's
master side (`MasterLink`) over a socket that does not block, looking at it,
at the service's orders and at the clock every 10 ms. A connection from the
loopback came through the cable (`cable`), any other over the home network
(`home_network`). While the port cannot be had (another program holds it)
the service logs it and tries again every minute; the rest of LiMo goes on.

**The family's file stays on the service's one thread.** A connection
never touches it. What needs it is a call handed to the service, which the
connection waits for (at most 5 seconds, then the link ends): an offer by
its id (`Hub::offer`), a pairing (`Hub::pair_device_with_offer`, which pairs
with the offer's child, keeps the phone's key and marks the offer used in
one transaction), a session opened, a report (`Hub::handle_report`, its
grant on the disk before it is sent, the phone's notes kept with it), an
app list (`Hub::keep_phone_apps`), and the end of a connection.
`Service::link` answers them between two steps, at most 16 a turn and at
most one pairing, called by the service's loop whenever something came and
after every step. A report answered brings the engine's next call forward
to the next step, so the PC follows a phone's use at once. What every
handshake reads (the master's key pair, `Hub::master_key`, and every phone
with its key, `Hub::link_phones`) is a book the service keeps for the
connections, read again at once after a pairing or a guardian's change and
at least every 10 seconds; so the cryptography runs on the connections'
threads, never on the service's.

**What is refused, and how.** An offer used already, or expired (the
master's clock), is refused with the link's reason (`offer_used`,
`offer_expired`) once the phone has shown it holds the offer's secret; an
offer the PC never made gets no answer. A guardian's phone, and a phone key
the family knows already, `not_supported`. A removed phone opens its
session and is told at once that it was removed (`removed`); the family's
file notes that it was told (`Hub::note_released`), and the log says so.
A report the family's file refuses ends the link with the reason. A newer
session of a phone takes the place of the older, which is closed.

**Prods.** The engine names the phones the books want to hear from
(`Orders::prod_devices`, from `Hub::wanted_reports`: time to give back or
take, new rules, a guardian's lock). Each, when a session of its is open, is
asked to report at once (`report_now`), at most every five seconds; a lock
from the console reaches a phone at home within a second or two.

**Hostile input.** Nothing a connection sends reaches the family's file
unless the link let it through its handshake, and none of it runs on the
service's thread. Garbage before the handshake, a frame said to be longer
than a step allows, a session handshake from a key the PC never paired, and
a peer that connects and says nothing (5 seconds, the link's handshake
limit) each end the connection, with a line in the log that never quotes
what was sent. More than four connections at once from one address, or 32
in all, are closed at once, before anything is read. A paired phone that
calls on the family's file more than 60 times in a minute is cut off (a
phone reports about once a minute). The tests run each of these while a
child's session counts on the PC, and the count goes on second for second.

**Measured** in `link::the_step_time_with_a_phone_talking` (the test build,
on the development PC, 21 November 2026), with a phone reporting every
second through the cable (sixty times as often as a phone does) and garbage
connections coming meanwhile: a step took 0.17 to 0.23 ms on average with
no phone and 1.3 ms with it, since every report brings the engine's next
call forward, the longest 14 to 23 ms; the link's turn between two steps
took 0.6 to 0.8 ms on average, the longest 8 to 73 ms (a report written to
the disk); and the seconds counted were the same within one.

**The day.** `link::a_whole_day_of_one_budget_shared_by_the_pc_and_a_phone`:
Asha has 30 minutes. At 07:00 her phone pairs with an offer from the door
and uses 10 minutes through the cable, reporting every two; at 16:00 the PC
counts 10; at 18:00 the phone hears what is left, the PC's minutes taken off
and the PC's use in its grant (`Grant::others_used_secs`), and uses what it
is given; the PC then uses what it held back for itself (what was left of
its lease) and locks, "Today's time is used up." The master counted 30
minutes in all, the phone's and the PC's together.

### What a phone must do

To match this side, LiMo for Android's child form (its link host):

- connects to the PC at 127.0.0.1:47810 while the cable is in (the PC's
  console sets `adb reverse tcp:47810 tcp:47810`), and otherwise at the
  address the announcement gives (or the offer's addresses), port 47810;
- pairs once with the offer it was handed (`PhonePairing`), keeps
  `PairedMaster` and its `DeviceState` before anything else, and from then
  on opens a session (`PhoneSession`) whenever the PC is in reach, with a
  report at once (`Opened`), a report every minute it is in use, and an
  answer to every `report_now` with a report;
- sends its app list after pairing and whenever its hash changes;
- takes `refused: removed` as the PC's word that it was removed: it
  releases itself, as the family decided (a phone removed on the PC is
  told so at its next contact, releases itself and says it can be removed);
- takes a connection closed with nothing said as the PC out of reach
  (`DeviceState::lost_contact` when a report was waiting) and comes again;
- never calls on the PC more than 60 times in a minute, and never opens
  more than four connections at once.

## The announcement on the home network

So that a paired phone finds the PC on the home network, the service
announces it with multicast DNS service discovery (`mdns-sd`,
`src/announce.rs`), the way proof four did. It announces only while the
family has a phone paired (a PC with no phone says nothing on the network),
and withdraws the announcement, with a goodbye, when the last phone is
removed and when the service stops.

**Exactly what is announced**, for a master whose id is `1a2b3c4d5e6f7a8b`:

| Record | Value |
| --- | --- |
| the service type | `_limo._tcp.local.` |
| the instance | `LiMo 1a2b3c4d._limo._tcp.local.` |
| the host | `limo-1a2b3c4d.local.`, port 47810 |
| TXT | `v=1` (the link's version) and `id=1a2b3c4d5e6f7a8b` (the master id) |
| the address | the PC's IPv4 address on that network |

The master id is the first 8 bytes of a BLAKE2s hash of the master's public
key (`limo_link::MasterId::of`): it says nothing of the family, and a phone
that paired knows it. The PC's name, the family's name, a child, a device or
a guardian is never in it. mDNS also sends the probes and answers that go
with those names, and nothing else; it is multicast to 224.0.0.251, port
5353, which routers do not pass beyond the local network.

**Where it is announced.** Only on the PC's network interfaces whose IPv4
address is in the private ranges home routers give out (10.0.0.0/8,
172.16.0.0/12, 192.168.0.0/16): never on loopback, never on a self assigned
169.254.0.0/16 address, never on the 100.64.0.0/10 that carriers and virtual
private networks use, never on IPv6, never on a public address. Every
interface is switched off in `mdns-sd` first and those switched on after.
What this cannot tell: a laptop taken to a café whose Wi-Fi gives out such
an address would announce there too, saying that a LiMo PC is present and
nothing more. A master PC is the family's home PC. The pairing offer
carries the same addresses.

## The firewall rule

LiMo adds no firewall rule now; the installer will. What it must add, for
`%ProgramFiles%\LiMo\limo.exe` only, on the Private and Public profiles
(a family's Wi-Fi is often called Public), from the local subnet only:
inbound TCP 47810 (the link) and inbound UDP 5353 (the announcement and the
questions phones ask). Without it, a phone on Wi-Fi does not reach the PC
(the cable still does: it arrives at the loopback, which the firewall does
not filter), and Windows either blocks the phone without a word or asks
whoever is at the PC, a question a family should not meet. The rule is removed
at uninstall. In this build's tests nothing listens on any address but
loopback, and nothing is announced.

## The example phone

`cargo run --example limo-phone -- pair --keep FOLDER`, `use --keep FOLDER
--minutes 5` and `look` (`examples/limo-phone.rs`) play a child's phone from
another PC on the home network, with the link's own phone side and the
core's device, as LiMo for Android will run them: it pairs with an offer
pasted from `dev-console`'s `request make_offer`, keeps what a phone keeps
in FOLDER (with the phone's private key: delete FOLDER after), opens
sessions, reports a minute of use every minute and answers `report_now`;
`look` prints the announcement it finds. The tests play the same phone
(`src/played_phone.rs`, built only with the `played-phone` feature, never
in `limo.exe`). TESTING.md, part P, is its run with the owner.

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
and the taskbar from a program in full screen, and sends none of it. Of
sound it learns only whether some is coming out of a program of its session,
from each audio session's loudness meter compared with zero, and sends only
that yes or no: no sound is captured, recorded, analysed or kept, and it
never asks which program plays or what (see
[Once a second: the facts](#once-a-second-the-facts)).

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
shows the plain display, and why; `install`, `dev-setup`, `dev-code` and
`reset-password` (with no names, no code and no password); the console's
door opened, or its name found taken; each console that connects, by its
session and process id; a refusal, and why ("its account is linked to a
child", never the child's name); a sign in, right or wrong, never what was
typed; each request done that changes something, by its name ("replace_rules
done"); a connection closed, and why; a session that ended after fifteen
idle minutes; a request that took 500 ms or more; the guardian PIN on the
lock of an account not set up, and what became of it; the link listening,
or its port taken; a phone paired (its device id, through the cable or over
the home network); a session opened; a removed phone told so; a pairing or
a report refused, and why; a connection closed, its address and why, never
what it sent; the announcement made, with what it holds, and withdrawn.
Anything that goes on happening is logged once and then at most every ten
minutes. Never logged: a pairing offer's text, a key, a code secret.

Never logged: a PIN, a password or a code; the reason a child typed; a window title, a
file being edited or a web address (LiMo never has them); any sound, or
which program plays it (LiMo never has them either); a program's folder; a
child's name. `dev-code` prints its code in the terminal it runs
in, and nowhere else. The line for what a helper got carries its process
object's access list, with the user's security identifier in it.

## Tests

`cargo test` runs three groups.

| Where | What | Count |
| --- | --- | --- |
| `src/*.rs` | The channel's frames and limits, wiping and hiding secrets; the console's requests read by name and arguments, through a frame and back, one that cannot be read answered by its number, one that is not a message refused saying only where, the longest answer and one too long, a password never in `Debug`; the helper's side and the outbox; the choice of display and the fallback to the plain one; the command line (with `--plain`, `dev-code`, `reset-password` and `dev-console`); `dev-console`'s lines, a secret never taken on the line; the never close list and paths; names from version resources; a description trimmed, collapsed, cleaned and cut, and the name a notice shows, by description or file name; quiet logging; settings and places, and the switch for the plain display; the plain display's words; what a helper got; `dev-setup`, `dev-code` and `reset-password` on a real file, the code tried on the device it was made for, the reset clearing the wait and keeping everything else; an audio session counted as playing only when active, not the system sounds, in this session and above zero, anything unknown counting as no; sound heard held for 3 seconds, then stale; the screen kept awake only by the display bit; a connection from loopback is the cable's; connections counted in all and by address; the link's clock; only the private ranges of home routers are home addresses, each once, at most eight; the announcement holds the id and the port and nothing of the family, word for word; this PC's home addresses read | 54 |
| `src/drawn/tests.rs` | The drawn display's decisions, with no window: Windows keys to the lock's keys, with the layout's text handed in; the modifiers and the keyboard state; text and points from Windows; the PC's clock from the time format; where the time left display starts, a drag kept on the screen, where a notice goes; pixels into a bitmap, and only a change saying so; the notices' turns; a way out sent once and only its own answer back to the same lock; an answer for a lock taken down, or one that waited too long, dropped; a PIN moved into its secret with nothing left behind; a reason typed with the layout's text, shortcuts typing nothing; keys reaching nothing but a lock; the same lock again keeping what the child did; the time left display and its notices only while the PC may be used, OK, and the notice by name; the lock drawn on the main monitor and the others; the service's answers as the lock's | 19 |
| `src/win/*.rs` | Pure functions, harmless reads of the test's own process, and harmless reads of what the PC says of itself, which pass whatever the answer: the time zone offset; the clocks, and this account's own time format; session states and administrators; this process's own account; the order version resources are read in; this process's own entry, file and version resource (none); frames across a pipe inside this process; the three access lists and that they parse for this user; the command line a helper gets, with and without `--plain`; a log line; full screen; a helper run by hand doing nothing; what this process got; the display bit as Windows numbers it; this process's own Windows session, and none for a process that is not there; whether this PC plays sound (twice, with COM started and ended) and keeps the screen awake, read and not judged, on a PC with a sound device or none; the door's access list keeps the right to make an instance from interactive users, and parses; this process's own account; this PC's accounts, read and not judged | 26 |
| `tests/service/` | The stories, against a fake PC with the real engine and a real family file: facts, helpers, the channel, the orders, the fallback and the console's door; the door over a real named pipe; and the link with a played phone over real loopback sockets (below) | 82 |
| `tests/house_rules.rs` | Unsafe code only in `src/win/`; no allow attributes; no dash punctuation | 3 |

The stories: `facts.rs` (7): no fresh report counts as input just now; a
fresh report's idle, full screen and program in front; sound, or the screen
kept awake, or both, keeping the count going with no input, a paused video
letting it pause, and a missing report still never saying something plays;
the program in front
only from the same session, never LiMo's own; version resources read once
per file; session 0 never in the facts, administrators marked; an
administrator's account not linked never locked. `helpers.rs` (10): one per
session that needs one; back within a step; a helper that ends at once
started again after a second; a failing launch tried every second, logged
once; no hello, replaced; no longer needed, told to quit and ended;
handed over at once and every five seconds; the child's name; a session
ending; a clean stop. `channel.rs` (7): twelve kinds of bad input each
closing the channel and restarting the helper, none reaching the engine; a
helper that stops reading; a way out belongs to its helper's session; a PIN
or code never logged, kept or shown; one way out a second; a helper that
shows the plain display, logged in plain words, one line, cut short; a way
out from a session not a child's refused (a code; the guardian PIN on such a
lock is the door's story). `orders.rs` (8): a warning held
until shown, its words the time actually left; held until its lock time; a
blocked program closed in its own session only, every copy, logged once,
its notice with the program's name; a closed program named by its file's
description, or its file name when it has none, a long one cut and control
characters dropped, the file still read once, and nothing of it in the
facts or the family's records; LiMo's own processes and the parts of
Windows never closed, and that said once for a session; phones ignored and
said once; problems logged once in a while.
`fallback.rs` (6): disconnected after 5 seconds unconfirmed; confirmed in
time, left alone; a helper that dies must confirm again; only a lock in a
session on the screen; the test cap; no cap, the lock stays.
`door.rs` (24): a family made through the door with no `dev-setup`, and the
PC following it (the child limited, an account not set up locked with the
PIN offered); a child's account refused in the guardian's words, at hello
and after a link made while it was open; a client Windows would not name
refused; the tries left, the wait after five, the wait for every console,
and two passwords in a second; nothing but the first screen's requests
before sign in, and nothing changed by them; a session ending fifteen
minutes after the last request that is not a repeated look; a session per
connection; eleven kinds of bad input each closing the connection and
reaching nothing; requests that cannot be read answered as unknown; a
client that says nothing closed after 10 seconds; a flood closed before any
of it reached the file; two clients at once, and a fifth refused with
words; one password between two steps; a console that stops reading; a
lock from the console on the child's screen within a step; the accounts
with their links; the console's own account never linked to a child from
it; the password again to remove a child, allow removal or delete the
family; rules answered with their own problems; adding a phone not yet; a
code made at the door and never logged; no secret logged, kept or shown;
the lock of an account not set up taking the guardian PIN; and the step
time with the door busy, measured. `pipe.rs` (7), over a real named pipe in
the test's own account: answered, with the door's access list and the
service told by Windows who the client is; the test's own account linked
to a child refused; two clients at once, each with its own session, and a
closed connection's session ended; garbage, a frame too long, a flood and a
silent client each closed, and the door open for the next; only SYSTEM and
Administrators able to make an instance; a door whose name is taken not
opened, and saying so; a fifth connection from one session closed at once,
and the session served again when the others go. `link.rs` (13), a phone
played over real loopback sockets with the link's own phone side and the
core's device: paired with an offer from the door, its key kept, its
session through the cable with a report and a grant, the door's
`device_link` saying `cable`, the announcement made with the first phone
and its words in the log, never the offer's text, its apps kept, its
goodbye; an offer used twice, an offer expired, an offer for no child; a
removed phone told so, the family noting it, and the announcement
withdrawn; a newer session taking the older's place; a lock from the
console reaching the phone by a prod; the rules changed at the console in
the phone's next grant; no offer without the link running; the whole day
of one budget (above); and, while a child's session counts on the PC:
garbage before the handshake, a handshake with a key never paired, a peer
that says nothing for five seconds, a flood of connections from one
address, a frame said to be 4 GiB, and a paired phone reporting more than
60 times in a minute, each closed or cut off with the count going on; and
the step time with a phone talking, measured.

The link's tests listen on the loopback address only, at a port the system
chooses, never on another address; they announce nothing on the network
(the announcer is a fake that notes what it was asked) and add no firewall
rule. `cargo test` also builds the example phone.

None of the tests starts or stops a service, installs anything, writes
under Program Files or ProgramData, ends, suspends or launches a process,
locks or disconnects a session, plays or records a sound, or shows a window.
The pipe tests make named pipes of their own names
(`\\.\pipe\limo.console.test.` and a number), never the door's own, in the
tester's own account, and connect to them from the same process; a door of
the tests adds the tester's account to its access list, since only SYSTEM
and Administrators may make the second instance that two clients need.
The one window made is
the band check's, 1 by 1 pixel, off screen, never shown, and destroyed at
once, as the spike's `bandcheck` did.

`cargo build`, `cargo test`, `cargo doc --no-deps` and
`cargo clippy --all-targets -- -D warnings` pass with no warnings and no
allow attributes. Dependencies are built optimised in debug and test builds,
as in the hub and the agent, so the slow password and PIN hashes stay quick.

## Left open

- **What a confirmed release means on the master PC**: logged, nothing else.
- **The console itself** is a program of its own, not built yet: its window,
  its check that the door is served from session 0 by LiMo's own program,
  and its own sign out after fifteen minutes with no input. The door's
  session ends after fifteen minutes too (above), whatever the console does.
- **The phone's own half of the link** is LiMo for Android's: its
  sockets, its search on the network, and the cable door that hands it the
  offer (see [What a phone must do](#what-a-phone-must-do)).
- **The firewall rule** is the installer's, not built yet.
- **The cable comes in at the loopback,** with every program on this PC:
  a program in a child's account that keeps opening connections to
  127.0.0.1:47810, saying nothing, could keep the cable's connection out
  while a guardian adds a phone (four from one address at once, each
  closed after five seconds). It cannot pair anything (an offer's secret is
  needed) nor pose as a phone (a phone's key is). Telling the cable's
  connections from the PC's own programs is left for the work on the
  cable.
- **Removal allowed** (`allow_removal`) is kept in the service's memory for
  ten minutes; nothing asks for it yet: the uninstaller that will is not
  built.
- **`limo reset-password` stops the service** for a moment, since the
  service holds the family's file; the PC limits nothing meanwhile (a few
  seconds).
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
  parts I to L, is where. The first run (19 November 2026, three monitors of
  1920 by 1080 at 100% scaling) saw the display, its notices, the drag on to
  another monitor, the lock and its ways out all work, typed text included,
  and nothing look wrong. Still to see: monitors of different scalings, a
  large first draw, and the lock set beside the design.
- **A video in a window is counted now, and so is anything else that
  plays.** In the first run a video left playing in a browser window, not
  in full screen, stopped being counted after the idle minutes. Decided the
  same day, and built: a session is also counted while a program of it
  plays sound, or while a program keeps the screen awake (see
  [Once a second: the facts](#once-a-second-the-facts)). Its limits, by
  design: music or a video left playing counts with nobody at the PC, for
  as long as it plays; a program that keeps the screen awake all day (some
  presentation, video call or "keep awake" programs do) counts all day,
  and, since the screen kept awake is the whole PC's, such a program in
  another account's session counts for the child's session in use too; a
  video with no sound, in a player that does not keep the screen awake,
  still pauses after the idle minutes. Not yet seen on a real PC:
  [TESTING.md](TESTING.md), part E, steps 6 to 9.
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
| windows, windows-core, windows-result, windows-strings, windows-link, windows-numerics, windows-collections, windows-future, windows-threading | 0.62.2, 0.62.2, 0.4.1, 0.5.1, 0.2.1, 0.3.1, 0.3.2, 0.3.2, 0.2.1 | MIT or Apache-2.0 | calling Windows; the same version as the first spike. The console's door added one of its features, `Win32_NetworkManagement_NetManagement`, for the accounts of the PC; no crate was added |
| windows-implement, windows-interface | 0.60.2, 0.59.3 | MIT or Apache-2.0 | used by windows-core when building |
| windows-service | 0.8.1 | MIT or Apache-2.0 | the service control manager; the same version as the first spike |
| limo-link | 0.1.0 (path) | Apache-2.0 | the link with the phones; its own crates (snow, curve25519-dalek, chacha20poly1305, blake2, getrandom and their parts) are listed with their licences in its README; curve25519-dalek and subtle are BSD-3-Clause |
| mdns-sd | 0.22.0 | Apache-2.0 or MIT | the announcement on the home network (no default features: no async runtime, no logging). On the network it sends only what [The announcement](#the-announcement-on-the-home-network) says |
| flume, spin, lock_api, scopeguard | 0.12.0, 0.9.9, 0.4.14, 1.2.0 | Apache-2.0 or MIT; spin MIT | parts of mdns-sd |
| mio | 1.2.5 | MIT | part of mdns-sd: its sockets |
| socket2, socket-pktinfo | 0.6.5, 0.4.2 | MIT or Apache-2.0; socket-pktinfo MIT | parts of mdns-sd: multicast sockets |
| if-addrs | 0.15.0 | MIT or BSD-3-Clause (taken as MIT) | the PC's addresses, for the offer and to choose where to announce; also used by mdns-sd |
| fastrand | 2.5.0 | Apache-2.0 or MIT | part of mdns-sd (and tempfile) |
| windows-sys, widestring, bitflags | 0.61.2, 1.2.1, 2.13.2 | MIT or Apache-2.0 | parts of windows-service |
| zeroize | 1.9.2 | Apache-2.0 or MIT | wiping a typed PIN or code, and the frames that carried it |
| serde, serde_json | 1.0.229, 1.0.154 | MIT or Apache-2.0 | the channel's messages and the settings file |
| syn | 2.0.119 | MIT or Apache-2.0 | used by windows-implement and windows-interface when building |
| tempfile | 3.27.0 | MIT or Apache-2.0 | tests only: a family file in a temporary folder |

`Cargo.lock` also names wasi (for mio on WebAssembly only) and fiat-crypto
(a backend of curve25519-dalek built only when asked for); neither is built
for Windows.

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
    service/
      door.rs       the console's door: who, sign in, sessions, requests
      link.rs       the link's calls on the family's file, sessions, prods,
                    offers, the announcement
      unlinked.rs   the guardian PIN on the lock of an account not set up
    phones.rs     the link's listener and its connections' threads
    announce.rs   the home addresses and the announcement (mdns-sd)
    played_phone.rs a phone played over the link (feature played-phone)
    protocol.rs   the channel: messages, frames, limits, Secret
    console.rs    the console's door: messages, frames, limits
    devconsole.rs dev-console's lines
    helper.rs     the helper's side, the Display trait, the Outbox, the
                  choice of display and the fallback to the plain one
    drawn.rs      everything the drawn display decides
    drawn/tests.rs  its tests
    close.rs      the never close list and paths
    programs.rs   programs and their cache, the name a notice shows
    quiet.rs      keeping the log readable
    settings.rs   names, places, the settings file
    cli.rs        the command line
    setup.rs      dev-setup's, dev-code's and reset-password's work on the
                  family's file
    text.rs       the plain display's words
    about.rs      words for what a helper got
    win/          everything that calls Windows
      mod.rs        commands, handles, strings, errors
      service.rs    the service control manager's entry, the service thread
      os.rs         WinOs, the real Os
      sessions.rs   sessions, accounts, administrators, disconnecting
      door.rs       the console's door: the pipe, its threads, who the
                    client is; a client of it
      accounts.rs   the accounts of this PC and the Administrators group
      devconsole.rs limo dev-console
      processes.rs  processes, version resources, ending a process
      launch.rs     the guarded launch, with the channel
      pipe.rs       the pipes, and their reader and writer threads
      helper.rs     limo helper: its threads and its loop
      playing.rs    whether the session plays sound, and whether a program
                    keeps the screen awake
      drawn.rs      the drawn display: its windows, bitmaps and hook
      display.rs    the plain display
      about.rs      reading what a helper got
      clock.rs      the clocks and local times
      logfile.rs    the log
      console.rs    typing without it showing
      install.rs    install, uninstall, status, dev-setup, dev-code,
                    reset-password
  tests/
    house_rules.rs
    service/      main.rs, common.rs (the fake PC), facts.rs, helpers.rs,
                  channel.rs, orders.rs, fallback.rs, door.rs, pipe.rs,
                  link.rs
  examples/
    limo-phone.rs a phone played from another PC
```
