# limo-agent

The engine of LiMo's agent on the master PC: the part of the Windows service
that decides, second by second, what happens in each Windows session. It
holds no Windows code. Everything the PC knows comes in as plain **facts**,
and everything to be done goes out as plain **orders**. The service calls the
engine about once a second and carries out what comes back.

On the master PC each child has one device of kind "on the master": the PC
itself, as that child uses it. The engine is the first real caller of the
device side of the rules core ([`../core`](../core/), `DeviceState`). It
talks to the child's book in the family's file ([`../hub`](../hub/)) through
the same report and grant a phone sends, only in process, and keeps the
device's state in the same file, written in the same transactions as the
book.

**What it is not.** It is not the Windows service, the helper that shows the
time left and draws the lock, the code that reads windows and processes or
closes a program, the named pipes, the installer, the network link to the
phones or the guardian's console. It has no clock, no threads, no network and
no files of its own, and no unsafe code. It builds and passes its tests on
any system.

## Contents

- [How the service calls it](#how-the-service-calls-it)
- [The facts](#the-facts)
- [The orders](#the-orders)
- [What LiMo records, and nothing else](#what-limo-records-and-nothing-else)
- [The rules it keeps](#the-rules-it-keeps)
- [How time is counted](#how-time-is-counted)
- [When it talks to the book](#when-it-talks-to-the-book)
- [What is kept, and when](#what-is-kept-and-when)
- [The clock](#the-clock)
- [When the family's file fails](#when-the-familys-file-fails)
- [Left open](#left-open)
- [Tests](#tests)
- [Dependencies and licences](#dependencies-and-licences)
- [Layout](#layout)

## How the service calls it

```rust
let mut hub = Hub::open(&path)?;                    // the family's file
let mut engine = Engine::new(&computer_name);       // the name the guardian sees

loop {                                              // about once a second
    let facts = Facts { wall_secs, steady_ms, utc_offset_secs, sessions };
    let orders = engine.tick(&mut hub, &facts);
    for order in &orders.sessions {
        // start LiMo's helper in the session if order.helper_needed;
        // hand it order.show; close every running copy of each program in
        // order.close, telling the child the reason
    }
    for device_id in &orders.prod_devices {
        prod(device_id);                            // phones, if in reach
    }
    for problem in &orders.problems {
        log(problem);
    }
    // once an hour: hub.tidy(now), as the hub's README says
}

// From a lock screen, through the helper (after at least one tick since the
// service started, so that the engine knows the session):
engine.ask_for_time(&mut hub, session_id, 20, "For the school project", now)?;
engine.try_code(&mut hub, session_id, "1342 0715", now)?;
engine.try_pin(&mut hub, session_id, &pin, PinAction::AddMinutes { minutes: 20 }, now)?;

// At a clean stop (an update, a shutdown):
engine.stop(&mut hub);
```

Rules for callers:

- **One engine per family file**, and the hub it is handed at every call is
  the service's one hub (the hub's README: one hub per file, one thread at a
  time). The guardian's console uses the same hub between calls; whatever it
  changes takes effect at the engine's next call.
- **Call about once a second.** Any spacing up to ten seconds counts exactly;
  a step longer than that counts nothing (see
  [How time is counted](#how-time-is-counted)).
- **Carry out every order as a level**, every time. Carrying out the same
  order twice does no harm, and an order missed is given again at the next
  call. The one thing given in a single call is a warning; it names what it
  is about, so a helper handed the same one twice can tell.
- **Prod the devices in `prod_devices`** when the network can reach them. The
  PC's own devices are never in it; the engine has already exchanged for
  them.
- **Call `stop` before a clean stop.** Without it nothing is wrong, but the
  last few seconds counted may be lost (see
  [What is kept, and when](#what-is-kept-and-when)).
- **Give the steady count from a counter that keeps counting while the PC
  sleeps**, on Windows the interrupt time (`QueryInterruptTime`). One that
  stops during sleep works too, but then each sleep is noted for the guardian
  as the clock moving forward.

## The facts

`Facts`, at every call:

| Field | What |
| --- | --- |
| `wall_secs` | The PC's wall clock, unix seconds. It says what time it is; it is never trusted for how long anything lasted. |
| `steady_ms` | A count of milliseconds that cannot be set and only grows while the PC runs. Every duration the engine counts comes from it. |
| `utc_offset_secs` | The PC's time zone now. Only used to notice a change, and to say on which of the PC's days an account that is not linked was used. |
| `sessions` | Every logged in Windows session. |

`Session`: its id; the account's identifier (the same opaque string the hub
links to a child) and the account's name as Windows shows it; whether it is
the one in use (`InUse`: on the screen and not locked by Windows), `Locked`
or `Disconnected`; whether the account is in the Windows Administrators group
(`is_administrator`, false when not given), even when it is not running
elevated; seconds since the last keyboard or mouse input; whether something
plays in full screen; the program in front; the programs running.

`Program`: its file name, the original file name its maker recorded inside
the file (`OriginalFilename`, when there is one) and its maker
(`CompanyName`, when there is one). LiMo names a program by its original file
name when it has one, otherwise by its file name, in lower case
(`Program::app_id`): "robloxplayerbeta.exe". A folder in front of a file name
is dropped. The guardian's lists in the rules should use these names.

## The orders

`Orders`, from every call:

- `sessions`: one `SessionOrder` for every session in the facts, in the same
  order: the child it belongs to, if any; whether LiMo's helper must run in it
  (`helper_needed`); what the helper shows (`show`); and the programs to close
  in it (`close`), each with its reason in plain words ("homework.exe is
  blocked by your family's rules, so LiMo closed it.").
- `prod_devices`: the family's other devices the books want to hear from now.
- `problems`: what went wrong with the family's file on the way, for the log.

`Show` is one of:

- **`Nothing`**: the account is not limited (a guardian's), or not linked and
  free. The helper need not run.
- **`TimeLeft`**: the session may be used. `seconds_left` (None on a day with
  no limit); `counting`, false while time is not being counted (idle, an
  always allowed program in front, the session not in use); a `warning` in
  the one call it falls due, with its words ("5 minutes of screen time left
  today.", "The PC locks in 5 minutes, for the hours it is not used.", "The
  unlock ends in 1 minute, and the PC locks then."); and `unlocked_until`
  while a guardian's unlock keeps the PC open. A warning's `minutes` is the
  warning point reached, which tells one warning from another; its words
  say the time actually left until its `lock_at`, in whole minutes rounded
  up and never less than 1, which is the point itself on most days. On a
  day of 6 minutes the first point reached is 15 minutes, and its words are
  "6 minutes of screen time left today.".
- **`Locked`**: cover the whole screen. The `LockScreen` says why (`LockWhy`:
  blocked hours, no time today, today's time used up, waiting for time,
  the rest of the time on another device, a guardian's lock, an account not
  linked, LiMo cannot check), in a short title for the top of the lock screen
  ("Today's time is used up.", `LockWhy::title`) and in plain words
  (`LockWhy::message`); until when, when known; whether
  the child may ask for more time, type a one time code, or have a guardian
  type the PIN; until when a wait after wrong guesses lasts; and the child's
  newest request today with its answer.

## What LiMo records, and nothing else

The types make LiMo's promise hold: **it records which program was used and
for how long, and nothing else.** A program is its file name, its original
file name and its maker. There is no field for a window title, a document, a
file being edited, a web address or anything typed, anywhere in this crate,
so the engine cannot record one, hand one to the hub, or put one in an order.
What reaches the family's file is what the core's slices hold: the program's
name, the day, the start and the end, and whether it counted.

## The rules it keeps

Each rule below is an owner's decision; the tests named show it.

**1. Whose time.** At every call the engine asks the hub which child each
session's account is (`Hub::account_standing`). *Not limited*: nothing is
counted and nothing is shown. *A child's*: the child's device on this PC
decides. *Not linked*: a setting in the hub, `PcSettings::unlinked_accounts`,
`Free` (the default) or `Locked`; either way the guardian is told the first
time such an account is in use each day (`EventKind::UnlinkedAccountUsed`,
kept once per account and day). Locked, the session gets a lock screen that
offers no way out: a guardian links the account from their own. The setting
can be set to locked only once a family is set up, and deleting the family
sets it back to free, so a PC being set up is never locked. **An account that
is not linked and is in the Windows Administrators group is never locked,
whatever the setting says: it shows nothing** (the owner's decision of 18 Nov
2026), so a guardian's own account cannot be locked out of the PC. It is
still noted as used, as any account that is not linked is. An
administrator's account linked to a child is the child's, limited like any
other. Tests:
`sessions::an_account_that_is_not_linked_is_free_or_locked_as_the_guardian_chose`,
`sessions::a_guardians_session_beside_a_childs_changes_nothing_for_the_child`,
`sessions::an_administrators_account_that_is_not_linked_is_never_locked`,
`sessions::an_administrators_account_linked_to_a_child_is_limited_like_any_childs`.

**2. Active time only.** Time counts only while the child's session is the
one in use and not locked by Windows. After the idle minutes with no keyboard
or mouse input (3 unless the guardian changes `PcSettings::idle_minutes`, 1 to
60) the count pauses, unless something plays in full screen. **The minutes
just before the pause count**: the count runs on through them and stops
exactly the idle minutes after the last input. A child reading or watching
something not in full screen uses the PC then, and walking away costs at
most those minutes. When input comes back, counting starts again at that
call; the step in which it came back does not count. Test:
`day::time_pauses_after_the_idle_minutes_unless_something_plays_in_full_screen`.

**3. Usage per program.** Each stretch of use is recorded against the
program that was in front, as the core's slices. Whether it counts is the
core's own rule (`RuleSet::is_always_allowed`, by the program's name): time
in a program on the always allowed list is recorded and does not count. A
program renamed to an always allowed name is still known by its original
name. Test: `day::a_program_on_the_always_allowed_list_does_not_count`.

**4. The lock.** The decision behind every order is the core's own
(`DeviceState::decide`). When it gives a lock for the device (time used up,
blocked hours, no time today, a guardian's lock, waiting, held elsewhere),
every one of the child's sessions is ordered locked; when it gives none, none
is. The fake PC checks this at every call of every test. A warning is carried
in the one call it falls due: the core keeps the warnings given, and the
engine keeps them by letting the device state decide itself whenever a
decision changes them. **A warning never says more time than there is**:
its words come from the time left when it is given, not from the point it
reached, so a day of 6 minutes is first warned of with "6 minutes", and a
bedtime or an unlock that is closer than a point says how close. Tests:
`day::a_plain_day_runs_to_zero_with_each_warning_given_once` (15, 5 and 1
minutes, each once, at its moment),
`day::a_short_day_is_warned_of_with_the_time_actually_left` (6 and 10
minutes),
`ways_out::an_unlock_shorter_than_a_warning_point_is_warned_of_with_the_time_it_has`,
`ways_out::new_rules_take_effect_at_the_next_call_with_their_warnings`
(a bedtime 9 minutes away).

**5. Blocked programs.** At every call, every program running in a child's
session (any session, in use or not) and the one in front are checked
against the child's blocked list, and each one on it is ordered closed with a
short reason, whatever the time left. A program is on the list when the list
names its original file name, so **renaming the file does not get around
it**; the list naming its file name as it is now closes it too. Its time in
front does not count, and the PC is not locked for it. The guardian is told
once a day for each program (`EventKind::BlockedProgramClosed`), under the
name the list gave. **What still gets around it**: a program with no
original file name recorded inside it, renamed. Nothing inside it says what
it is. What would close that hole: identifying programs also by their
digital signature (the signer's name, which renaming cannot change, for
signed programs) and, for unsigned ones, by a hash of the file, so that a
renamed copy is still known; the guardian's console would offer programs by
name and add those to the list. A child who builds a new program, or edits
one's bytes, is beyond any of these. Test:
`programs::a_blocked_program_is_closed_whenever_it_runs_even_renamed`.

**6. One budget.** The PC's device for each child exchanges with the child's
book through `Hub::exchange_on_master_pc`: the device writes its report
(`DeviceState::report`), the book takes it (`MasterBook::handle_report`), the
device takes the grant (`DeviceState::take_grant`), all through the same
messages a phone sends. The shared budget is the core's; the engine follows
its rules for callers: it reports at least once a minute while there is use
to report, asks for more time only while the child's session is in use,
acknowledges at once when a grant asks, and exchanges whenever the book wants
to hear from it. The fake PC checks at every call that the time counted on
all of a child's devices, reported or not, less use beyond what a device held
under an unlock, stays within the highest budget in play plus the extra
time. Tests: `phone::the_worked_day_with_the_phone_away_then_home` (the core
README's worked day, with the real engine on the PC: 75 minutes on the PC, 45
on the phone, 120 in all), `phone::the_pc_and_a_phone_at_home_used_at_once_share_one_budget`.

**7. Ways out, from the lock screen.** `Engine::ask_for_time`,
`Engine::try_code` and `Engine::try_pin` call the core's own
`DeviceState::ask_for_time`, `try_code` and `try_pin`, with its waits after
wrong guesses, and then exchange with the book at once, which keeps the
state: so a request reaches the guardian at once, time added by a code or
the PIN is in the book at once, and wrong guesses and codes used survive a
restart. A guardian's action in the hub (extra time, an answer, lock now, an
unlock, a lift, new rules, a PIN) changes the child's book, and the engine
sees that at its next call (`Hub::book_revision`) and exchanges then, so it
takes effect at the next call, with no restart. A right PIN's "release this
device" is passed back as the core gives it; what a release means on the
master PC is the service's to decide. Tests: every test in `ways_out`.

**8. The clock.** See [The clock](#the-clock). Tests: every test in `clock`.

**9. Restarts.** See [What is kept, and when](#what-is-kept-and-when).
Tests: every test in `restart`, and restarts in the random run.

**10. Several sessions.** Only a session in use counts; a child with two
accounts, both logged in, is counted once. A guardian's session beside a
child's changes nothing for the child. The child's device on this PC is
paired the first time the child's account is seen in the facts
(`Hub::pair_master_pc`), in one transaction with its state, so it is paired
once and never twice, through restarts. If the guardian removes it, the next
call pairs a new one; if the child is removed, its accounts are no longer
linked. Tests: `sessions::two_children_switching_sessions_count_only_while_in_use`,
`sessions::the_pc_device_is_paired_the_first_time_a_child_is_seen_and_never_twice`.

## How time is counted

- **Steps.** Between two calls the engine measures the step with the steady
  count. A step counts for a child when, at the call before, the child's time
  was being counted (the session in use, input within the idle minutes or
  something in full screen, the PC not locked, the program in front not
  blocked), and the same session is in use now. It is cut where the idle
  minutes ran out, and it is never more than the time the device held at the
  call before, so the lock is never late by a second's worth of use. A step
  longer than 10 seconds (`MAX_COUNTED_STEP_MS`) counts nothing: the PC
  slept, or the service was held up, and what happened meanwhile is not
  known. The step belongs to the program that was in front at the call
  before.
- **A stretch, then slices.** Counted milliseconds gather in a stretch for the
  program in front. Its whole seconds go into the device state as one slice
  (`DeviceState::record_use`) before every exchange, when the program
  changes, when counting stops, when the clock jumps, and when a warning falls
  due; a part of a second stays for more use of the same program. So a slice
  is about a minute of use, not a second: two hours of use make about 130
  slices.
- **Decisions every call.** At every call the engine decides on a copy of the
  device state with the stretch recorded, so every order is exactly what it
  would be had each second been recorded.
- **The wall clock places the use.** A stretch is recorded ending at the
  wall clock time of its last counted step, so its day and its blocked hours
  are the wall clock's; its length is the steady count's.

## When it talks to the book

An exchange happens, for a child on the PC, when any of these holds:

- it is the first call for the child since the engine started;
- the child's book wants to hear from the device (`Hub::wanted_reports`);
- the child's book changed since the engine's own last exchange: a guardian
  acted, or another device reported (`Hub::book_revision`);
- the device asks for time (the core's `ask_master`, while the child's
  session is in use): at once for a new day, otherwise at most once a minute
  while it still needs time;
- there is use not yet reported and the last exchange was a minute ago or
  more;
- a way out changed the state and its exchange has not gone through.

A child that leaves the PC (logs off) gets one more exchange if it has use
not yet reported, and the engine lets go of it. A child's device the books
want to hear from while the child is not on the PC gets one exchange, to take
the news. The engine asks for no lease for a child who is not using the PC,
so a session switched away shows "waiting for time", and gets its lease in
the same call it is used again.

## What is kept, and when

**Where.** The hub keeps, for each child's device on the master PC, the
core's `DeviceState` and the engine's own notes (the stretch not yet
recorded, and the last look at which the child's time was being counted), in
its `master_pc` table, beside the books. See the hub's README.

**When.**

- At every exchange, the book, the slices, the device state and the notes
  are written in **one transaction**, committed to the disk before the
  device acts on the grant. The core's rule ("the master saves before it
  sends a grant, a device after it takes one") holds with nothing in
  between: neither side can ever have kept a grant the other has not.
- Whenever anything else changes (a slice recorded, a warning given, a way
  out, counting starting or stopping, another program or session), the state
  and the notes are kept at once.
- While counting just goes on, the stretch and the look are kept every five
  seconds. They are always kept together.

**Why a restart at any instant is safe.**

- *Nothing is counted twice.* The look is used once: as soon as a call has
  counted a step from it, the engine holds no look until the end of the
  call, so anything kept in between has none. What is kept is always a stretch
  and a look from the same moment. After a restart the engine counts one step
  from the kept look to the first call, by the steady count; that time was
  not in the kept stretch.
- *Nothing handed out is handed out again.* The book that made a grant and the
  device state that took it are written in one transaction. After a power cut
  both are as they were before it, or both as after it.
- *A lock that was on is on again at the first call.* Locks come from the
  device state (what it holds, the guardian's lock, blocked hours), which is
  kept; the first call decides from it. Warnings given, wrong guesses, the
  wait they set and the codes used are in the device state too, so none is
  given, forgotten or reused again.
- *What a crash can lose.* At most the last five seconds counted, and only when
  the service is not back within ten seconds (the step from the kept look is
  then too long to count), or the child has left meanwhile. A clean stop with
  `Engine::stop` loses nothing. This is a choice: keeping the stretch at
  every call too would lose nothing at all, at five times the writes.

**What it costs.** On the development PC (AMD Ryzen 9 7950X3D, NVMe disk) one
write takes about 6 ms, most of it waiting for the disk. While a child's time
is counted the engine writes about once every five seconds, and exchanges
once a minute; while nobody is counted it does not write at all.

## The clock

- **Durations come from the steady count alone.** The wall clock says what
  time it is (the day, blocked hours, when a lock ends) and nothing else.
- **A jump.** When the wall clock and the steady count disagree by more than 5
  seconds between two calls (`CLOCK_JUMP_MS`), the clock was set or
  corrected. What was open is recorded where it was, the step is counted by
  the steady count, and the guardian is told (`EventKind::ClockJumped`, with
  how far). Set back, the device's day never moves back (the core's rule);
  set forward over midnight, the new day starts, as the passing of time would
  start it. Neither gains nor loses a counted second. A child's account on
  Windows cannot set the system clock.
- **The time zone.** A change is noted (`EventKind::TimeZoneChanged`) and
  changes nothing counted: days and blocked hours follow each child's home
  zone, which is in the rules. A child's account on Windows can change the
  time zone, so the engine never follows it.
- **Sleep and a service held up.** A long gap between two calls is never
  counted. With a steady count that goes on through sleep, a sleep is a long
  step; with one that stops, it looks like the wall clock jumping forward, and
  is noted.
- **Across a restart** the engine compares the clocks only to decide whether
  the first step can be counted; a jump while the service was stopped is not
  noted.

## When the family's file fails

The engine never stops giving orders. When a read or a write fails, it goes
on from what it holds in memory, says so in `Orders::problems`, and tries a
write for that child again after five seconds. A child whose record cannot be
read at all when the engine first needs it gets a lock screen that says
LiMo cannot check the time left (`LockWhy::CannotCheck`): the engine does not
let a child use the PC unlimited because the file is broken. When the hub
says a child's device was removed, the engine lets go of it and pairs a new
one at the next call.

## Left open

These are not built here. Recommendations are in the report that came with
this piece.

- **Always allowed programs under a lock.** On Windows LiMo's lock covers the
  whole screen, so a program on the always allowed list cannot be used while
  the PC is locked; such programs only do not count while the PC is open. An
  "allowed programs only" mode, with the lock lifted for those programs and
  everything else closed, would be a different lock, for a later piece.
- **The hole left by renamed unsigned programs** (rule 5).
- **The default for accounts that are not linked** (rule 1): free, as the
  brief asks; locked is recommended once setup is done.
- **Daylight saving and a moved home.** The home zone is a fixed offset in the
  rules; following daylight saving needs the console or the service to
  replace each child's rules at the change.
- **A clock moved while the service was stopped** is not noted.
- **The helper's own window**, when in front, counts as any program does.

## Tests

`cargo test` runs two groups. The stories drive the engine with a fake PC
(`tests/pc/common.rs`): a fake wall clock, steady count and time zone,
Windows sessions, phones driven with their own device states, all against a
real hub on a real file in a temporary folder. At every call the fake PC
checks that the orders came with no problem, that every child's session is
locked exactly when the core says the PC is locked, that a child's counted
use never goes down within a day nor grows by more than the steady count
allows, and that the time counted on all of a child's devices stays within
the highest budget in play plus the extra time, less use beyond what a device
held under a guardian's unlock. That last part is kept as it happens, and
remembered after the child leaves the PC: a device's holding cannot tell it
later, since the master may raise a phone's away allowance to what it already
used, or lower it below.

| Where | What | Count |
| --- | --- | --- |
| `src/facts.rs` | Program names: the original name when there is one, folders dropped, lists matched by either name for blocking | 2 |
| `src/orders.rs` | Every lock's short title, beside its message; a warning's words at its point, short of it for each cause, and for any numbers | 4 |
| `tests/pc/day.rs` | A plain day to zero with each warning once; days of 6 and 10 minutes warned of with the time they have; idle pauses and full screen; the always allowed list; midnight in use; a budget cut and raised in the middle of the day | 6 |
| `tests/pc/programs.rs` | A blocked program renamed, closed again and again, noted once a day, and the hole; one child's list closing nothing in other sessions | 2 |
| `tests/pc/sessions.rs` | Two children switching; a guardian's session; an account not linked, free and locked; an administrator's account not linked never locked, and one linked to a child limited; the PC device paired once, through restarts, re-paired after the guardian removes it | 6 |
| `tests/pc/ways_out.rs` | Extra time, lock now and lift; an unlock that counts beyond; an unlock shorter than a warning point, warned of with the time it has; a request granted and refused; a code once, wrong codes and their wait across a crash; the PIN adding, unlocking and confirming a release; new rules with their warnings | 7 |
| `tests/pc/phone.rs` | The core README's worked day with the real engine; the PC and a phone at home used at once | 2 |
| `tests/pc/clock.rs` | The wall clock set back and forward; forward over midnight; a time zone change; sleep both ways and a service held up | 4 |
| `tests/pc/restart.rs` | One script for a whole day played uninterrupted, with a crash after every single call (over 4,000), with crashes here and there, and with clean stops: every call shows and counts the same, and each day's total is the same; a crash losing at most five seconds and a clean stop nothing; a lock on again at the first call | 4 |
| `tests/pc/random.rs` | The seeded random run below | 1 |

**The random run** plays six scenarios of two days each by default, in
parallel, from a seeded generator of its own: two children and a guardian
switching sessions, programs changing (always allowed, blocked and renamed,
unknown), idle and full screen, Windows locking, the guardian giving extra
time, locking, lifting, unlocking, changing budgets and bedtime and answering
requests, codes and the PIN typed right and wrong, a phone of the same child
used at home and away, the clock set back and forward, the time zone
changed, the PC asleep, the service held up, crashing, stopping cleanly, and
the PC turned off and on; calls come one to nine seconds apart. A default run
makes about 30,000 calls. It fails if it stops reaching any of these. To run
more, or one scenario:

```
LIMO_AGENT_SCENARIOS=48 LIMO_AGENT_DAYS=3 cargo test --test pc -- random --nocapture
LIMO_AGENT_SEED=<seed from a failure> LIMO_AGENT_TRACE=1 cargo test --test pc -- random --nocapture
```

A failure names its seed. With `LIMO_AGENT_TRACE` set, the run prints what
happened at each call and every part of the budget check.

(In PowerShell: `$env:LIMO_AGENT_SCENARIOS="24"` first.) The whole suite takes
about two minutes on the development PC, most of it waiting for the disk:
the stories write to a real file, as the product does.

`cargo build`, `cargo test`, `cargo doc --no-deps` and
`cargo clippy --all-targets -- -D warnings` pass with no warnings and no
allow attributes.

## Dependencies and licences

| Crate | Version | Licence | Used for |
| --- | --- | --- | --- |
| limo-core | 0.1.0 (path) | Apache-2.0 | every decision, and the device side of the shared budget |
| limo-hub | 0.1.0 (path) | Apache-2.0 | the family's file |
| serde | 1.0.229 | MIT or Apache-2.0 | the facts, the orders and the engine's notes |
| serde_json | 1.0.154 | MIT or Apache-2.0 | the engine's notes, kept in the family's file as JSON |
| tempfile | 3.27.0 | MIT or Apache-2.0 | tests only: temporary folders |
| rusqlite | 0.43.0 | MIT | tests only: looking inside the family's file |

Everything under these is listed in the hub's and the core's READMEs, with
its licence; this crate adds none. Nothing is GPL or AGPL. The code here was
written for LiMo; nothing was copied. The crate is under the Apache License
2.0, as is all of LiMo.

## Layout

```
agent/
  Cargo.toml, Cargo.lock, README.md, .gitignore
  src/
    lib.rs        crate notes, the public names and the limits
    facts.rs      Facts, Session, Program: what the service hands in
    orders.rs     Orders, SessionOrder, Show, LockScreen: what it gets back
    error.rs      EngineError, in plain words
    engine.rs     Engine: the call, the ways out, a clean stop, the view
    child.rs      one child's device on the PC: steps, the stretch, the
                  look, exchanges and what is kept
  tests/pc/
    main.rs       the one test program
    common.rs     the fake PC, its phones and its checks
    day.rs, programs.rs, sessions.rs, ways_out.rs, phone.rs, clock.rs,
    restart.rs, random.rs
```
