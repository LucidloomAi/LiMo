# limo-phone

The engine of LiMo on a child's Android phone: the part that decides,
second by second, what the phone shows. It holds no Android code.
Everything the phone knows comes in as plain **facts**, and everything to be
done goes out as plain **orders**. The host (LiMo's Android service, in
Kotlin) calls the engine about once a second and carries out what comes
back.

A phone is one device of one child, in the rules core's sense
([`../../core`](../../core/), `DeviceState`). It shares the child's one
daily budget with the master PC and the child's other devices through the
same report and grant every device uses, and works through a whole day out
of reach of the master on its away allowance. The engine is the core's
second real caller on the device side; the first is the PC's engine
([`../../agent`](../../agent/)), whose patterns it follows and whose code it
does not share.

**What it is not.** It is not the Android service, the accessibility
service that watches the app in front and draws the covers, the screens of
the ways out, the foreground notification, the alarms, the file the state is
kept in, the link to the master (`link/`, a later piece), or the UniFFI
wrapper that Kotlin calls (`phone/ffi`, a later piece). It has no clock, no
threads, no network and no files of its own, and no unsafe code. It builds
and passes its tests on any system.

## Contents

- [How the host calls it](#how-the-host-calls-it)
- [The facts](#the-facts)
- [The orders](#the-orders)
- [The child's own screens](#the-childs-own-screens)
- [What LiMo records, and nothing else](#what-limo-records-and-nothing-else)
- [The rules it keeps](#the-rules-it-keeps)
- [The words](#the-words)
- [How time is counted](#how-time-is-counted)
- [When it talks to the master](#when-it-talks-to-the-master)
- [What is kept, and when](#what-is-kept-and-when)
- [A kept state that cannot be read](#a-kept-state-that-cannot-be-read)
- [The clock](#the-clock)
- [Left open](#left-open)
- [What the core and the hub should gain](#what-the-core-and-the-hub-should-gain)
- [Tests](#tests)
- [Dependencies and licences](#dependencies-and-licences)
- [Layout](#layout)

## How the host calls it

```rust
// Once, at pairing, with what the master made: the welcome grant and the
// phone's 32 byte code secret. Setup is not finished yet: nothing counts.
let mut engine = PhoneEngine::paired(&welcome, code_secret, wall_secs)?;
write_state(&engine.save());

// Once, when the guardian leaves setup's last screen on the phone:
engine.finish_setup(wall_secs)?;          // true the first time, then false
write_state(&engine.save());              // and tick again at once

// After a restart of the process or the phone:
let mut engine = PhoneEngine::load(&read_state());   // never fails; see below

loop {                                    // once a second while the screen is
    let facts = PhoneFacts { .. };        // in use, at once on any change
    let orders = engine.tick(&facts);
    if orders.save_state {
        write_state(&engine.save());      // before anything else
    }
    show(&orders.cover);                  // a level: nothing, a block, the lock
    notify(&orders.time_left.words);      // LiMo's notification
    if let Some(w) = &orders.warning { float(w); }
    log(&orders.notes);
    if orders.send_report {
        let report = engine.outgoing_report(wall_secs).unwrap();
        write_state(&engine.save());      // the report changed what is kept
        match link.send(report) {         // later: the link, within 10 seconds
            Some(grant) => {
                let taken = engine.take_grant(&grant, wall_secs);
                write_state(&engine.save());   // before acting on it
                // taken.report_again: the next call orders a report at once
                engine.tick(&facts);           // and act on these orders
            }
            None => engine.lost_contact(),
        }
    }
    sleep_until(next event, or orders.next_check_in_secs);
}

// From the lock, in LiMo's own activity:
engine.ask_for_time(20, "For the school project", wall_secs)?;
engine.try_code("1342 0715", wall_secs)?;
engine.try_pin(&pin, PinAction::AddMinutes { minutes: 20 }, wall_secs)?;
write_state(&engine.save());              // after each, right or wrong
```

Rules for callers:

- **Call at once when a fact changes**: the screen on or off, Android's lock
  screen shown or gone, the app in front, a call ringing, starting or
  ending, the master in or out of reach, a prod. While the screen is in use,
  call once a second as well. While it is off, call within
  `next_check_in_secs` (counted with the elapsed count) and, while the
  master is in reach, at least once a minute. Any spacing up to ten seconds
  counts exactly; a step longer than that counts nothing.
- **Keep the state whenever the engine says so, before doing anything
  else.** One rule covers it all: after any call, if `should_save()` is
  true (a tick says so in `save_state`), write `save()` in place of the old
  bytes, then go on. See [What is kept, and when](#what-is-kept-and-when).
  If a write fails, call `save_failed()`; the engine asks again.
- **Carry out every order as a level**, every time. The same order carried
  out twice does no harm; one missed is given again at the next call. The
  things given in one call only are a warning and the notes.
- **Never draw over what the orders leave uncovered.** The engine already
  leaves a ringing call, the diallers, LiMo and Android's own lock screen
  uncovered; the host must step aside for a ringing call at once, as it
  calls the engine at once. A cover drawn during a call in progress says so
  (`call_on`): put a button on it with the words it gives ("Back to the
  call") that brings the call's screen back.
- **Report only when told, and carry the answer back.** A report not
  answered within ten seconds (`ANSWER_WAIT_MS`) is a `lost_contact()`. After
  a grant, call `tick` again at once.
- **Keep the state private.** It holds the phone's code secret and the PIN
  record (Android's private app storage, kept out of backups).

## The facts

`PhoneFacts`, at every call:

| Field | What |
| --- | --- |
| `wall_secs` | The phone's wall clock, unix seconds. It says what time it is; it is never trusted for how long anything lasted. |
| `elapsed_ms` | `SystemClock.elapsedRealtime()`: milliseconds since the phone started, counting deep sleep. It cannot be set. Every duration comes from it. |
| `boot_count` | `Settings.Global.BOOT_COUNT`: how many times the phone has started. With `elapsed_ms` it tells a restart of the phone from anything else. |
| `utc_offset_secs` | The phone's time zone now. Times the child reads are said in it; days and blocked hours follow the home zone in the rules. |
| `hours_24` | `DateFormat.is24HourFormat`: "21:42" or "9:42 pm". |
| `auto_time` | `Settings.Global.AUTO_TIME`: the network sets the clock, so nobody can set it by hand. See [The clock](#the-clock). |
| `interactive` | The screen is on (`PowerManager.isInteractive`). |
| `keyguard_locked` | Android's own lock screen shows (`KeyguardManager.isKeyguardLocked`). |
| `call` | `Idle`, `Ringing` or `Ongoing`, from the call state. Never who, never how long. |
| `front` | What is in front: `Unknown` (the watch is off, or has seen nothing), `Home` (the launcher or the recent apps), `LiMo`, `Dialler` (the default dialler, or the screen of a call in progress), `EmergencyDialler`, or `App { package, label }`. The label is the app's name under its icon, when the host has it: it is shown on the block screen, and never kept or reported. |
| `in_reach` | The master can be reached: the link has a session. False until the link is built. |
| `prodded` | The master asked for a report since the last call. |
| `permissions` | The six things LiMo needs, in the order setup asks for them: `notifications`, `phone` (Android's Phone permission, only to know that a call rings or is on), `usage_access`, `accessibility` (the service on), `battery` (left out of battery optimisation), `protection` (its device admin). Each is true only when Android says so. See rule 10. |

**The engine holds no list of makers' package names.** Which app is the home
screen, the dialler or the emergency dialler differs from maker to maker, so
the host says so in `front`: the default launcher from the home intent, the
default dialler from `TelecomManager`, the emergency dialler and LiMo's own
screens from what the accessibility events show. A test checks that the
engine's code names no package.

## The orders

`PhoneOrders`, from every call:

- `cover`, what to draw over the screen:
  - **`Nothing`**: the app in front may be used.
  - **`Block { package, title, message, go_home, call_on, back_to_call }`**:
    a blocked app is in front. Cover it, naming it by its label ("Game
    Launcher is not allowed." / "It is blocked by your family's rules.";
    with no label, "This app is not allowed."), and send the child to the
    home screen. During a call in progress `call_on` is true, with the words
    "Back to the call".
  - **`Lock(LockScreen)`**: the phone is locked. Cover whatever is in front.
    The `LockScreen` says why (`LockWhy`), with a stamp ("Locked", or "One
    moment" while more time comes), a short title and plain words; until
    when, as a moment and in words ("Tomorrow, 7:00 am", or "When a guardian
    unlocks it" with no moment known); **the apps that stay usable under the
    lock** (`allowed_packages`: the rules' always allowed list, for the host
    to show as tiles that open them; the phone, the emergency call and LiMo
    itself are always usable besides); whether the child may ask for more
    time, type a code, or have a guardian type the PIN; the wait after wrong
    tries, as a moment and in words; the child's newest request today with
    its answer, in words; the ways out as rows to show, each with its words
    and whether it must wait; when none is offered, what to say instead;
    and, during a call in progress, `call_on` with the words "Back to the
    call". While an allowed app is in front the cover is `Nothing`: the
    child opened it from the lock.
- `time_left`: the best known time left today (None with no limit); whether
  time is counted now; when a guardian's unlock ends; whether the phone is
  away (out of reach, holding only its own time); whether it is locked; and
  the words for LiMo's notification ("1 h 12 min left", "1 h 12 min
  paused", "No limit today", "Open until 9:30 pm", or the lock's title;
  "Setup is not finished on this phone." until a guardian finishes setup).
- `warning`: in the one call it falls due, and only where it can be read
  (where a cover may be drawn): what for, the point reached, when the lock
  comes, whether it is this phone's away time running out, and three lines
  of words.
- `send_report`: send a report to the master now. Only ever true in reach.
- `save_state`: keep the state now.
- `notes`: what the guardian should know, noted in this call.
- `next_check_in_secs`: call again within this many seconds if nothing
  happens.

## The child's own screens

`view()` gives the phone as the engine holds it. Two parts of it are for
the child's own screens in the app, with every word in them, as at the last
call (before any call, as at the last moment kept). The screens never make
a sentence about time or rules themselves. An app is its package name
here: the Android side puts in each app's name.

**`today`**, for Today:

| Field | What |
| --- | --- |
| `left` | The time left, shown large: a `kind` and its `words`. `left`: "1 h 12 min", "14 min", "0:42" under a minute (the time left display's words, rounded up). `no_limit`: "No limit today". `open`, while a guardian's unlock keeps the phone open: "Open until 9:30 pm". |
| `used` | "48 min used of 2 h": this phone's counted use (see below) of the day's budget, rounded down, so that with the time left, rounded up, it makes the day. "48 min used today" on a day with no limit or with no time. Use beyond the day under an unlock is said as it is: "2 h 10 min used of 2 h". |
| `used_part` | The part used, from 0 to 1 in thousandths, for the bar; none on a day with no limit; 1 on a day with no time. |
| `where_used` | Where the time went: "On this phone 31 min". See below. |
| `bedtime` | "Screens are off from 9:00 pm to 7:00 am.", or none when no hours are off today. |
| `apps` | Today on this phone: each app's `package`, its `secs` and its `minutes` in words ("12 min", "1 h 5 min"), longest first; apps under a minute, and use the host could not name, left out. Every app's use is listed, also of an app whose time does not count (an always allowed one). |
| `guardian_needed` | None while all six permissions are on; otherwise `off`, the permissions that are off in setup order, each with its one sentence (see [The words](#the-words)). |

**`setup`**: whether a guardian has finished setup on the phone
(`finished`), when (`finished_at`, by LiMo's clock; none for a state kept
before setup was kept), and, while it is not, its `words`: "Setup is not
finished on this phone.". See rule 11.

**`rules_words`**, for How it works: `time_a_day` ("2 h", "No limit",
"No screen time", or each day's time when the days differ),
`screens_off` ("9:00 pm to 7:00 am", with the days when not every day, or
none), `always_open` (the packages that stay open under a lock) and
`not_allowed` (the packages that are blocked).

**What a phone knows of the child's other devices: only its own use.** A
grant tells it `left_for_you_secs`, how much this phone may still use
today, which the master works out by taking away what every other device
used *and* what the others hold and have not used, and what they wait for,
and adding any time the guardian gave; `held_elsewhere_secs`, the away
time other phones hold and have not used; and `used_secs_reported`, this
phone's own use as the master had it. None of these is the other devices'
use, and the rest of the core's grant and the link's messages carry none
either. So Today says only "On this phone 31 min", and its `used` line is
this phone's own counted use. It never guesses the rest. The words for the
day the core says more are settled and tested ("On this phone 31 min · on
the PC 17 min" when all of the rest was on the family PC, "· on other
devices 17 min" when it was on more than one, or the phone cannot tell
which); see [What the core and the hub should gain](#what-the-core-and-the-hub-should-gain).

**Each app's use today** is kept in the state, beside the device state, and
added to where the device state records use, from the slices the core
makes, so the two always agree and survive a restart together. **How the
day's change is known:** each slice carries the home day the core put it on
(`Slice.day`, from the rules' calendar, the home zone and the day start,
cut at the day's start and never moving back, even when the clock does). A
slice of a later day empties the count and starts that day; Today shows it
only while its day is the day of the last decision, so after the day
changes and before any use, Today lists no apps. The count holds at most
200 apps (`MAX_APPS_A_DAY`); a new app beyond them takes the place of the
one with the least time.

**The hours screens are off**, on Today, are the blocked hours that start
on today's home date, as the rules count them (a window belongs to the day
it starts on), in order, with those that touch or overlap said as one, on
the phone's own clock and zone, in 12 or 24 hours as the phone shows them.

## What LiMo records, and nothing else

The types make LiMo's promise hold: **it records which app was used and for
how long, and nothing else.** An app is its package name. Its label (the
name under its icon) may come with it, only to be shown back to the child
on the block screen, cleaned first: it is never kept in the state, never in
a report, never in a note. There is no field for a window's text, a web
address, a message, a contact, a number dialled or anything typed, anywhere
in this crate, so the engine cannot record one, report one or put one in an
order. Whether a call is ringing or in progress is all LiMo knows of it.
The dialler, the home screen and LiMo itself are not recorded at all: they
are not use of an app. What reaches the master is what the core's slices
hold: the app's package name, the day, the start and the end, and whether it
counted. Each app's use today, kept for the child's own Today screen, is a
package name and seconds, and stays on the phone. The notes hold no
sentence and no name: a permission switched off is noted by which it is,
and setup finished by when.
Tests: `kept::an_apps_label_is_shown_but_never_kept_nor_reported`,
`today::today_says_the_time_left_the_time_used_and_each_apps_minutes_and_starts_again_the_next_day`.

## The rules it keeps

Each rule below is an owner's decision or the lead's; the tests named show
it.

**1. What counts on a phone.** The owner's decision of 19 Nov 2026: **time
counts while the screen is on and unlocked with an app that counts in
front. Sound with the screen off does not count.** Android's own lock
screen does not count. There is no idle rule: the screen's own timeout
plays that part. The home screen, LiMo itself, the dialler and the
emergency dialler never count, and nothing counts while a call rings. An
app the host cannot name counts, under an empty name: the screen is in use
all the same. Whether an app's time counts against the budget is then the
core's own rule (an always allowed app is recorded and does not count).
What is covered is never in use. **The rule is one function, `app_in_use`**,
with its own tests, so it can change in one place. Tests:
`counts::tests::*`,
`covers::what_counts_is_an_app_in_front_of_an_unlocked_screen`.

**2. Never covered, and a call is no way around the limits.** The lead's
rules. **A lock never stands between a child and a call for help:** while
a call rings nothing is covered and nothing counts, so it can always be
answered; and the dialler (the screen of a call in progress among it), the
emergency dialler and LiMo's own screens are never covered and never
count, call or no call. Android's own lock screen (which has its own
emergency call) and a screen that is off are never covered either. **But a
call in progress is no way around the limits:** a child could ring any
number, leave the call open and go back to a game, so during a call in
progress any other app in front counts and is covered exactly as it would
be with no call, and the home screen is covered by a lock as with no call.
Such a cover says a call is on (`call_on`) and gives the words of the way
back to it, "Back to the call". The cover rule is one function too,
`may_cover`. The fake phone checks at every call of every test that nothing
is covered over a ringing call or the diallers, and that a cover during a
call in progress offers the way back; one test tries every mix of the facts
that matter, locked, open and blocked, and checks that each cover during a
call in progress is the one the same moment gives with no call. Tests:
`covers::a_ringing_call_is_never_covered_nor_the_diallers_limo_or_the_keyguard`,
`covers::with_the_time_used_up_and_a_call_on_the_game_is_covered_and_the_dialler_is_not`,
`covers::a_call_left_open_does_not_stop_the_count_in_a_game`,
`kept::a_state_that_cannot_be_loaded_locks_the_phone_and_is_left_as_it_is`.

**3. The lock, and the apps it leaves usable.** The decision behind every
order is the core's own (`DeviceState::decide`, for the app in front). When
it gives a lock for the device, the phone is covered by the lock unless the
app in front is one the core allows under it: the always allowed apps stay
usable, and their time is recorded and does not count. The fake phone
checks at every call that the lock covers exactly when the core says the
phone is locked and the app in front is not allowed. Tests:
`covers::allowed_apps_stay_usable_under_the_lock_and_do_not_count`, and the
check in every test.

**4. Blocked apps.** A blocked app in front (by its package name, exactly) is
covered, named by its label when the host gives one, and the child sent
home, whatever the time left; its time is not counted, and the guardian is
told once a day for each app, by its package
(`PhoneNote::BlockedAppOpened`). The label is the app's own word, so it is
cleaned as the PC cleans a program's description: spaces trimmed and each
run of them made one, control characters and direction marks dropped, and
cut at 60 characters (`MAX_LABEL_CHARS`) with an ellipsis. Tests:
`covers::a_blocked_app_is_covered_the_child_sent_home_and_the_guardian_told_once_a_day`,
`words::tests::the_block_screen_names_the_app_by_its_label_cleaned`.

**5. One budget, away and home.** The phone exchanges with the child's book
through the same report and grant every device uses; the shared budget is
the core's. Out of reach it spends what it holds (its lease, the time added
on it, its away allowance), and then locks with its own reason, **the away
time is used up**, more comes when it is back home. Its warnings say so. At
home it reports, asks for more while the child is at it, and gives its away
time back when the master asks. The fake phone checks at every call that
the time counted on all of the child's devices, reported or not, stays
within the highest budget in play plus the extra time. Tests:
`day::the_worked_day_with_the_phone_away_then_home` (the core README's worked
day with the real engine on the phone: 75 minutes on the PC, 45 on the
phone, 120 in all),
`day::a_phone_spends_its_away_time_is_warned_as_away_and_opens_again_at_home`,
`day::a_plain_day_at_home_runs_to_zero_with_each_warning_once`,
`day::a_prod_from_the_master_brings_a_report_at_once_and_only_in_reach`.

**6. Ways out, from the lock.** `ask_for_time`, `try_code` and `try_pin` call
the core's own, with its waits after wrong guesses; each answer comes with
its words. The master hears of each at the next report. **A code is offered
only for a lock that time can open** (a lack of time): a code adds time, and
time does not open blocked hours or a guardian's lock. The PIN is offered
whenever the phone holds a PIN record. Tests: every test in `ways_out`.

**7. Warnings only when they can be read.** A warning floats over the app
in front, so it is given only where a cover may be drawn (rule 2): never
over a ringing call, the screen of a call in progress, the diallers or LiMo,
nor with the screen off or Android's lock screen showing. A point that
passes meanwhile is warned of when the child is back in an app, with the
time actually left; during a call in progress, a game in front is warned of
as ever. A call that sends a report asking for more time gives
no warning: the call right after the grant decides with what the master
gave (the PC's engine exchanges before it decides, to the same end). Tests:
`day::a_warning_waits_for_the_screen_and_says_the_time_actually_left`,
`day::a_plain_day_at_home_runs_to_zero_with_each_warning_once`.

**8. The clock.** See [The clock](#the-clock). Tests: every test in `clock`.

**9. Restarts.** See [What is kept, and when](#what-is-kept-and-when). Tests:
every test in `restart` and `kept`.

**10. Permissions switched off.** The lead's rule, with the owner's
decision of 20 Nov 2026 to ask for the Phone permission. Each of the six
that goes from on to off is noted for the guardian
(`PhoneNote::PermissionOff`, by which it is), and while any is off Today
says a guardian is needed, with one sentence for each. **Nothing else
changes in what the engine counts or covers, for any of the six:** the
engine takes the app in front, the call and the screen from the other
facts, as before. What changes is what the host can know and do. Without
accessibility it cannot say which app is in front (`Front::Unknown`, which
counts all the same) nor draw a cover; without usage access it cannot
confirm the app in front; without notifications it cannot show the time
left or a warning in Android's shade; without the Phone permission it knows
less about calls (a call that rings with the ringer silenced may read as no
call); without the battery exemption Android may stop LiMo; without
protection LiMo can be removed. Tests:
`today::a_guardian_is_needed_while_anything_is_switched_off_and_nothing_counted_or_covered_changes`,
`clock::a_permission_switched_off_is_noted`.

**11. Setup is the guardian's time, not the child's.** The lead's rule,
from the first run on a real phone (20 Nov 2026), where a guardian setting
up the phone used six of the child's thirty minutes on Android's own
pages, and those pages stood in the child's Today. **From pairing until a
guardian finishes setup on the phone, the engine counts nothing and covers
nothing**: no lock, no block screen (the guardian is holding the phone),
no warning, no request for more time to the master, and no note of a
permission switched off (the guardian is switching them on, and Android
may say one is on a moment before it is). The time left says so in place
of itself, "Setup is not finished on this phone.", and so does the view
(`setup`), so a phone never sits unfinished unnoticed: LiMo's notification
shows those words. What the core decides goes on underneath (the day, the
rules, reports in reach), and Today still says what is switched off.
`finish_setup(now)` finishes it: the host calls it when the guardian leaves
setup's last screen on the phone, whether every step is on or one was
passed. From the next call the phone counts and covers as any phone does.
**Once finished it stays finished**: it is in the kept state, so nothing
short of wiping LiMo's data (which wipes the pairing too) brings it back;
calling it again changes nothing and asks for nothing to be kept. The
guardian is told once, by a note (`PhoneNote::SetupFinished`, with when);
the master paired the phone, so until that note comes it knows setup is
not finished. A state kept before setup was kept counts as finished: see
[A kept state that cannot be read](#a-kept-state-that-cannot-be-read).
Tests: every test in `setup`.

## The words

Every sentence the phone shows comes from the engine, in `src/words.rs`.
They are the PC's own sentences (the agent's lock reasons and warnings, the
lock screen's ways out, its answers to a code or a PIN, its wait, the time
left display), with "PC" changed to "phone" and "programs" to "apps";
three settled by the lead from the design sheet (today's time used up, the
PIN's line, and the block screen naming the app); and what only a phone
needs: its away time used up, more coming when it is back home, and the way
back to a call. Times are said on the phone's own clock and zone, in 12 or
24 hours as the phone shows them. Apostrophes are plain ASCII ones, as the
agent writes them.

The lock, by reason: title, then message.

| Reason | Title | Message |
| --- | --- | --- |
| `BlockedHours` | Screen time is off for now. | This is a time when the phone is not used. |
| `NoTimeToday` | No screen time today. | Today has no screen time. |
| `TimeUsedUp` | Today's time is used up. | You have used all of today's screen time. |
| `WaitingForTime` | Getting more time. | LiMo is getting more time. This takes a moment. |
| `AwayTimeUsedUp` | The away time is used up. | This phone has used all the time it may use away from home. More comes when it is back home. |
| `HeldByAnotherDevice` | The rest is on another device. | The rest of today's time is on another device. It comes here when that device is back home. |
| `GuardianLock` | Locked by a guardian. | A guardian has locked the phone for now. |
| `CannotCheck` | LiMo cannot check the time. | LiMo cannot check the time left just now. Ask a guardian. |
| `AppBlocked` | This app is not allowed. | It is blocked by your family's rules. |

The block screen names the app by its label when the host gives one: "Game
Launcher is not allowed." / "It is blocked by your family's rules."; with
no label, the title is "This app is not allowed.". On the block screen and
on the lock, during a call in progress: "Back to the call".

The rest of the lock: the stamp "Locked", or "One moment" while waiting for
time. Until when: "Today, 9:30 pm", "Tomorrow, 7:00 am", "Friday, 7:00 am",
"Friday 27 November, 7:00 am", "In a moment"; with no moment known, "When a
guardian unlocks it", "When that device is back home", "When this phone is
back home". The ways out: "Ask for more time" / "A guardian sees your
request in LiMo"; "Type a one time code" / "A guardian who is away can read
you 8 digits"; "Guardian PIN" / "For a guardian who is with you"; under
one that must wait, "Wait until 9:47 pm". The wait: "Too many wrong tries.
The next try is at 9:47 pm." (or "... is tomorrow, 7:00 am."). The request:
"You asked for 30 min at 9:32 pm", stamped "Waiting", "Given 15 min" or
"Not this time". With no way out offered, by reason:

| Reason | In bold | Under it |
| --- | --- | --- |
| `WaitingForTime` | Nothing to do here. | The phone opens by itself. |
| `AwayTimeUsedUp` | Nothing to do here for now. | More time comes when this phone is back home. |
| `CannotCheck` | Ask a guardian. | A guardian can look at LiMo on the family PC to see what is wrong. |
| `GuardianLock` | A guardian can unlock it. | From LiMo on their phone, or in LiMo on the family PC. |
| `BlockedHours` | Nothing to do here for now. | The phone opens again by itself. |
| `AppBlocked` | Nothing to do here. | It is blocked by your family's rules. |
| `TimeUsedUp`, `NoTimeToday`, `HeldByAnotherDevice` | Ask a guardian. | A guardian can give more time from LiMo on their phone. |

Two of these are not a word for word change: on the PC they say "in LiMo on
this PC", which on a child's phone would point the guardian at the wrong
device, so they say "the family PC".

The answers to a code or a PIN: "30 minutes added." (or "1 hour added.");
"Open until 10:42 pm."; "That was right."; "That code is not right. 4 more
tries before a wait." (and "1 more try", and with no count once the wait has
started: "That PIN is not right."); after the fifth, the wait in words as
above; "A code has 8 digits."; "A PIN has 6 to 8 digits."; "That code was
already used today."; "Codes are not set up on this phone yet."; "A
guardian PIN is not set up yet."; and under a right one when the phone is
open after it, "The phone opens now.". A request refused, and the core's
other refusals, are in the core's own words ("A request from this device is
still waiting for an answer.").

The time left: "1 h 12 min left", "1 h 12 min paused", "No limit today",
"Open until 9:30 pm", "0:42 left" under a minute, or the lock's title while
locked.

The warnings, with the time actually left, never more than the point
reached ("6 minutes" on a day of 6 minutes, never "0 minutes"):

| Warning | Message | Hint |
| --- | --- | --- |
| The day's time runs out | 5 minutes of screen time left today. | Save what you are doing. (at 15: The phone locks when the time is used up.) |
| The away time runs out | 5 minutes of away time left today. | Save what you are doing. (at 15: More time comes when this phone is back home.) |
| Blocked hours start | The phone locks in 5 minutes, for the hours it is not used. | Finish what you are doing. |
| An unlock ends | The unlock ends in 5 minutes, and the phone locks then. | Save what you are doing. |

At 1 minute the hint is "Save what you are doing now.". Each comes with
when: "Locks about 9:47 pm" for time running out, "Locks at 9:00 pm" for
the others.

Today, settled by the lead from the design sheets (`phone.html` frame E,
`phone-setup.html` frames H and T): the time left, "1 h 12 min", "14 min",
"0:42", "No limit today", "Open until 9:30 pm"; under the bar, "48 min used
of 2 h", or "48 min used today"; where it went, "On this phone 31 min" (and,
once the core says more, "On this phone 31 min · on the PC 17 min" or
"On this phone 31 min · on other devices 17 min"); the hours,
"Screens are off from 9:00 pm to 7:00 am.", "Screens are off from 12:00 pm
to 1:00 pm and from 9:00 pm to 7:00 am.", "Screens are off all day."; each
app's minutes, "12 min", "1 h 5 min". Spans used are rounded down: "0 min",
"48 min", "2 h", "1 h 5 min".

How it works: the time a day, "2 h", "No limit", "No screen time", or by
days, "1 h 30 min from Monday to Friday; 3 h on Saturday and Sunday"; the
hours screens are off, "9:00 pm to 7:00 am", "21:00 to 07:00", "All day",
or by days, "9:00 pm to 7:00 am from Sunday to Thursday; 10:00 pm to 8:00
am on Friday and Saturday". Days are "from Monday to Friday" for three or
more in a row (across the week's end too), else "on Saturday and Sunday",
"on Tuesday, Saturday and Sunday", "on Friday", and nothing for every day.

Until a guardian finishes setup on the phone, in place of the time left and
in the view: "Setup is not finished on this phone.".

What is switched off, one sentence each, settled by the lead, in the order
setup asks for them:

| Permission | Today says |
| --- | --- |
| `notifications` | Notifications are switched off, so LiMo cannot show the time left or a warning. |
| `phone` | Phone access is switched off, so a lock could hide a call. |
| `usage_access` | Usage access is switched off, so LiMo cannot count the time in each app. |
| `accessibility` | Accessibility is switched off, so LiMo cannot see which app is open. |
| `battery` | Android may put LiMo to sleep to save battery. |
| `protection` | Protection is switched off, so LiMo can be removed. |

## How time is counted

- **Steps.** Between two calls the engine measures the step with the
  elapsed count. A step counts when, at the call before, the child was
  using an app that was not covered (rule 1), on the same start of the
  phone. **It belongs to that app and counts in full**, even if the screen
  went off or another app came in front by the call that ends it: the host
  calls at once when a fact changes, so a step ends where the use ended. It
  is never more than the time the phone held at the call before, so the
  lock is never late by a second's worth of use. A step longer than
  10 seconds (`MAX_COUNTED_STEP_MS`) counts nothing: LiMo was held up or
  stopped. A step across a restart of the phone counts nothing.
- **A stretch, then slices.** Counted milliseconds gather in a stretch for
  the app in use. Its whole seconds go into the device state as one slice
  (`DeviceState::record_use`) before every call into the core that may move
  its day (a report, a grant, a way out, a decision that keeps a warning),
  when the app changes, when counting stops and when the clock jumps; a part
  of a second stays for more use of the same app. So use across midnight is
  cut where the day changes, and a slice is a stretch of use of one app.
- **Decisions every call** are made on a copy of the device state with the
  stretch recorded, so every order is exactly what it would be had each
  second been recorded.
- **LiMo's clock places the use**: a stretch is recorded ending at LiMo's
  clock at its last counted step; its length is the elapsed count's.

## When it talks to the master

Only while the master is in reach, a report is ordered when any of these
holds:

- it is the first call in reach since the engine started;
- the master asked (`prodded`);
- the phone asks for time (the core's `ask_master`, while the child looks at
  the phone, locked or not): at once for a new day, otherwise at most once a
  minute while it still needs time;
- there is use the master has not had, and the last exchange was a minute
  ago or more (`REPORT_EVERY_SECS`);
- the last exchange was five minutes ago or more, whatever there is to say
  (`REPORT_IDLE_SECS`), so that new rules reach the phone on a day with no
  limit;
- a way out changed the state and has not reached the master;
- a grant asked for a report at once (it lowered the away allowance).

Not while a report waits for its answer, nor within ten seconds of one that
got none, so a master that does not answer is asked again every ten
seconds, not every second. A report asks for more time only while the child
looks at the phone. Out of reach, the engine stops trusting what the master
last said is left (the core's `lost_contact`), so its warnings follow the
time it holds.

## What is kept, and when

**What.** One blob: a first line `limo-phone-state`, a line `format 1`, a
line `check` with a 64 bit FNV-1a sum of the rest, and the rest as JSON: the
core's `DeviceState` (with the code secret and the PIN record), and the
engine's own notes: the stretch not yet recorded, the last look at which the
child's time was being counted, LiMo's clock (the last reading, the start
count and the correction it keeps), the notes for the guardian not yet
taken, the permissions last seen, the blocked apps noted today, each app's
use today (see [The child's own screens](#the-childs-own-screens)), and
whether setup is finished, and when (`"setup":"not_finished"`, or
`"setup":{"finished":{"at":...}}`).
The check is against damage, not against anyone who can write the file.

**When the host must write it.** Always the same way: after a call, when
`should_save()` is true, write `save()` before doing anything else. It is
true:

| After | Because |
| --- | --- |
| `paired` | the state is new |
| `outgoing_report`, before sending the report | the report's number and the slices it carries are in the state |
| `take_grant`, before acting on the grant | the core's rule: a device keeps a grant before it spends it |
| `ask_for_time`, `try_code`, `try_pin`, right or wrong, before showing the answer | the request, the time added, the code used and the wrong guesses must survive a restart |
| `finish_setup`, the first time | setup finished must survive a restart; called again, nothing changes and nothing is asked |
| a `tick` that recorded a slice, gave a warning, changed what the core holds, started or stopped counting, noted something for the guardian, or saw a permission or the clock's correction change | all of these are kept at once (`save_state`) |
| a `tick` while counting just goes on | every five seconds (`SAVE_PROGRESS_EVERY_MS`), not more often |

While nothing is counted and nothing changes, it is not written at all.

**Why a restart at any instant is safe.**

- *Nothing is counted twice.* The look is used once: as soon as a call has
  counted a step from it, the engine holds no look until the end of the
  call, so anything kept in between has none. What is kept is always a
  stretch and a look from the same moment. After a restart the engine counts
  one step from the kept look to the first call, by the elapsed count; that
  time was not in the kept stretch.
- *Nothing handed out is handed out again.* The state is kept after a grant
  is taken and before it is acted on, and after a report is written and
  before it is sent, so the master and the phone agree after any crash, as
  the core's rule asks ("the master saves before it sends a grant, a device
  after it takes one").
- *A lock that was on is on again at the first call.* Locks come from the
  device state (what it holds, the guardian's lock, blocked hours), which is
  kept; the first call decides from it. Warnings given, wrong guesses, their
  wait and the codes used are in the device state too.
- *What a crash can lose.* At most the last five seconds counted, and only
  when LiMo is not back within ten seconds, or the screen went off
  meanwhile.

## A kept state that cannot be read

`PhoneEngine::load` never fails. A state that is empty, not LiMo's, written
by a newer LiMo (the app was replaced by an older one), cut short or
changed gives an engine that **fails safe**, and `load_problem()` says why.
Safe means:

- the phone is covered by a lock that says LiMo cannot check the time left
  (`LockWhy::CannotCheck`), with no way out on the phone: the PIN record and
  the code secret were in the state;
- calls, the dialler, the emergency dialler and LiMo itself stay open, as
  always, and nothing is drawn over Android's own lock screen;
- it reports nothing and takes no grant, and it never asks to be kept, so
  the bytes stay as they are for a guardian to look at, and a state from a
  newer LiMo is still there when that LiMo is back;
- a guardian pairs the phone again from the family PC, which replaces it.

A state kept by an earlier version of this engine loads: every field after
the device state is filled in when missing, and the form number says which
form it is (this is the first). A state kept before the six permissions
holds two: the other four read as off, and since only a permission going
from on to off is noted, nothing is noted for them; one kept before each
app's use was counted shows no apps on Today until there is use again.
**A state kept before setup was kept counts as finished**: its phone was
paired and in use already, and taking it as not finished would stop it
keeping its limits until someone found it and finished a setup it had
done. A new pairing keeps "not finished" in so many words, so the two are
never mixed up. A state restored from an old copy cannot be
told from a fresh one here; the core's README says what that costs (its new
slices on a day the master already took higher numbers from it do not
count), and the host keeps the file out of Android's backups.

## The clock

- **Durations come from the elapsed count alone.** The wall clock says what
  time it is (the day, blocked hours, when a lock ends) and nothing else.
- **LiMo's clock.** While **automatic time** is on, the network sets the
  wall clock and nobody can set it by hand: LiMo follows it. When it jumps
  (more than 5 seconds against the elapsed count, `CLOCK_JUMP_MS`), what was
  open is recorded where it was, the step is counted by the elapsed count,
  and the jump is noted (`PhoneNote::ClockMoved`, followed). While automatic
  time is **off**, a jump is somebody setting the clock: LiMo keeps its own
  clock, the wall clock less the jumps it has seen, run on by the elapsed
  count, and notes the jump (not followed). **A clock set by hand buys
  nothing**: set back, bedtime still comes on time; set forward, no new day
  starts early, so no fresh away allowance. When automatic time is switched
  on again, LiMo follows the network's clock once more. The times the child
  reads, and the times in reports, are LiMo's.
- **Across a restart of the phone** the elapsed count starts again, so the
  two clocks cannot be compared. LiMo keeps the correction it had, and only
  makes sure its clock does not go back: a clock set back while the phone
  was off is caught (LiMo goes on from where it last kept its clock); one
  set forward while it was off cannot be told from time passing. The
  restart is noted (`PhoneNote::Restarted`).
- **The time zone.** A change is noted (`PhoneNote::TimeZoneChanged`) and
  changes nothing counted: days and blocked hours follow the home zone in
  the rules; the times the child reads follow the phone's zone.
- **Sleep and LiMo held up.** A step longer than ten seconds is never
  counted. One of a minute or more while the phone was in use means LiMo was
  stopped or held up while the child used the phone, and is noted
  (`PhoneNote::NotWatched`).

## Left open

These are not built here; recommendations are in the report that came with
this piece.

- **Carrying the notes to the master.** The core's report has no place for
  them yet; the engine keeps the newest 200 (`MAX_NOTES_KEPT`) in the state
  until it has.
- **A clock set forward while the phone was off**, with automatic time off,
  starts a new day early. The master's own clock, which the link will carry
  (`master_now`), would settle it.
- **A call left open with its own screen in front** is never covered and
  never counts (rule 2): the child can only stay on the call there. Calls
  made inside other apps (a video call in a messaging app) are that app's
  use, and count; whether Android reports them as calls is for the host.
- **Labels that are the same on every lock** ("What you can do", "Still
  open", "Go to the home screen", "Emergency call", "Send request"): left to
  the screens, which may ask for them here.
- **The other devices' use on Today.** Today's `used` line and its bar are
  this phone's own use until the core says more (see below). On a day the
  child also used the PC, "31 min used of 2 h" sits beside a time left of
  1 h 12 min that the PC's 17 minutes have already come off.
- **Today's hours across midnight.** Today names the hours that start
  today. In the morning, the end of last night's bedtime is not named
  again: the lock says until when.
- **Rules days in another zone.** How it works says the hours on the
  phone's clock and the days as the rules name them; on a phone far from
  home a window can start on the day before or after the one named.

## What the core and the hub should gain

What this piece worked around, inside the engine:

- **Core: a decision without changing anything** (`DeviceState::decide`
  keeps the warnings it gives). The engine probes a clone of the state each
  call and decides on the real one only when warnings change, as the agent
  does. A `DeviceState::decide_probe(&self, now, app) -> Decision` that does
  not keep warnings, and `keep_warned(&mut self, &Decision)`, would remove
  the clone.
- **Core: the device's own day** (`Holding.day` as kept, not as it would be
  at a moment): `holding_at(now)` moves it. The engine works around it by
  recording the stretch before every call that may move the day.
- **Core: notes in the report.** `Report` needs a list of notes (a clock
  moved, a restart, a permission off, a gap watched, a blocked app opened),
  carried like slices until a grant confirms them by id, as the plan says;
  then `outgoing_report` carries `PhoneNote`s and drops them when confirmed.
- **Core: the lock a warning is about.** `Warning` does not say whether the
  lock that comes is for the day's time or for the away time; the engine
  works it out by recording the time in hand on a clone and deciding at the
  lock's moment. A `Warning.reason: LockReason` would say it.
- **Core: which locks a code can open.** `Decision.can_use_code` is true
  whenever the device holds a code secret; the engine offers a code only
  for a lack of time. The core could say so itself.
- **Core: the master's clock.** A grant carrying `master_now` (or the link's
  `Pong`) would let LiMo's clock be set right in reach and across restarts.
- **Core: the other devices' use in the grant.** The master knows each
  device's use that day; a grant carrying the use on the child's other
  devices (with whether all of it was on the master PC) would let Today say
  "On this phone 31 min · on the PC 17 min" and a `used` line of the whole
  day. The words are ready (`words::where_words` takes it); only the figure
  is missing. Until it comes, the phone says what it knows.
- **Hub: note events** for each `PhoneNote`, and the phone's form and
  protection level.

## Tests

`cargo test` runs three groups. The stories drive the engine with a fake
phone (`tests/phone/common.rs`): its own wall clock, elapsed count, start
count, time zone and screen, a state file that holds only what the engine
asked to keep, the master reached or not, and, when a story wants it, the
child's device on the master PC driven with its own device state, all
against a real hub on a real file in a temporary folder. Its phone is set
up at once (a guardian finishes setup as soon as it is paired), but in the
setup stories, which start from a phone just paired. At every call the
fake phone checks that nothing is covered over a ringing call, the diallers, LiMo,
Android's lock screen or a dark screen; that the lock covers exactly when
the core says the phone is locked and the app in front is not allowed, once
setup is finished, and that before it nothing is covered, counted or warned
of and the time left says setup is not finished; that
time is counted only while an app is in use and uncovered; that the
phone's counted use never goes down within a day nor grows by more than the
elapsed count allows; and that the time counted on all of the child's
devices, reported or not, stays within the highest budget in play plus the
extra time, less use beyond what a device held under an unlock.

| Where | What | Count |
| --- | --- | --- |
| `src/counts.rs` | The rule of what counts, a call in progress no way around it, and what is never covered | 5 |
| `src/words.rs` | Times on the phone's clock; spans and the time left; every lock's short title beside its message; warnings with the time actually left, away or not, with hints and when; the answers to a code or a PIN; a request and its answer; the block screen naming the app by its label, cleaned and cut; Today's time left, the time used and its part, where it went in its three forms; the hours screens are off today, on the phone's clock and zone, joined, several, all day; the rules in words, by days, across the week's end; the six sentences for what is switched off | 11 |
| `src/kept.rs` | The check catching a change anywhere in the kept state; notes kept up to the limit; counting going on told from any other change; each app's use added up for one day, the nameless left out, a later day starting afresh, bounded | 4 |
| `tests/phone/day.rs` | The core README's worked day with the PC beside the phone; the away time used up, warned of as away, opening again at home; a plain day at home to zero with each warning once, the home screen covered, the 24 hour clock and when to call again; a warning that waits for the screen and says the time actually left; a prod from the master, only in reach; a master that does not answer asked again every ten seconds | 6 |
| `tests/phone/covers.rs` | Allowed apps usable under the lock and recorded without counting; never a cover over a ringing call, the diallers, LiMo, the keyguard or a dark screen, and during a call in progress the same cover as with no call, through every mix of those facts; with the time used up and a call on, the game and the home screen covered with the way back to the call, the dialler not; a call left open not stopping the count in a game, nor a block, nor the lock; a blocked app covered, named by its label, sent home, never counted, noted once a day and across a restart; what counts, case by case | 6 |
| `tests/phone/ways_out.rs` | A request made at home, answered and shown; a request made away, carried home, refused; a code at school adding time once, wrong codes counting down to a wait that survives a crash, the guardian seeing both at home; the PIN adding time and unlocking with no network, only the PIN at bedtime, use under the unlock counted beyond, an unlock refused, a release confirmed | 4 |
| `tests/phone/clock.rs` | A clock set by hand not followed, bedtime on time, no new day, kept through a crash, followed again with automatic time; a clock set by the network followed; a time zone change; a restart of the phone in the middle of a step, and a clock set back while it was off; LiMo's process restarted in the middle of a step, and held up for a minute; a permission switched off | 6 |
| `tests/phone/restart.rs` | One script for a whole day played uninterrupted, with a crash after every single call (over 4,000), and with crashes here and there: every call orders and counts the same (a call while locked among them), and each day's total is the same; a crash losing at most five seconds; a lock on again at the first call | 3 |
| `tests/phone/kept.rs` | A kept state read back the same; an app's label shown but never kept, reported or in the view; a save that could not be written asked for again; empty, foreign, newer, cut short and changed states failing safe; a state with only the fields the first form needs; a state kept with two permissions and no count of each app loading and noting nothing new | 6 |
| `tests/phone/setup.rs` | Twenty minutes of setup on Android's settings, LiMo and a game counting nothing, listing nothing, warning of nothing, the time left saying setup is not finished, a permission switched on and off not noted; finished, noted once, counting from the next call, a permission switched off noted after; a blocked app and the hours screens are off neither covering nor warning before, the lock and the block screen at the first call after; not finished kept through a crash, finished kept through a crash and a restart, finishing again changing nothing; a state kept before setup was kept counting as finished; a state that cannot be read refusing it | 5 |
| `tests/phone/today.rs` | Today through a day: the time left, the time used and its part, where it went, bedtime, each app longest first with the short and the nameless left out, the same after a crash, at bedtime, and starting again the next morning; a day with no limit and a guardian's unlock; How it works by days, on a 24 hour phone, and with no blocked hours; all six permissions switched off, noted in setup order, sent for a guardian with six sentences, while a phone with all on is told the same at every call through a game, a blocked app, the lock, a call and the dialler | 4 |
| `tests/house_rules.rs` | No unsafe code; no allow attributes; no makers' package names in the engine; no dash punctuation in anything a person reads, nor in any sentence the phone shows | 5 |

The whole suite takes under a minute on the development PC, most of it the
hub writing to its file as the product does.

`cargo build`, `cargo test`, `cargo doc --no-deps` and
`cargo clippy --all-targets -- -D warnings` pass with no warnings and no
allow attributes.

## Dependencies and licences

| Crate | Version | Licence | Used for |
| --- | --- | --- | --- |
| limo-core | 0.1.0 (path) | Apache-2.0 | every decision, and the device side of the shared budget |
| serde | 1.0.229 | MIT or Apache-2.0 | the facts, the orders and the kept state; the core already depends on it |
| serde_json | 1.0.154 | MIT or Apache-2.0 | the kept state as JSON, as the hub writes the books |
| limo-hub | 0.1.0 (path) | Apache-2.0 | tests only: the family's file on the master PC |
| tempfile | 3.27.0 | MIT or Apache-2.0 | tests only: temporary folders |

Everything under these is listed in the core's and the hub's READMEs, with
its licence; `Cargo.lock` holds the same versions as theirs, and this crate
adds none. Nothing is GPL or AGPL. The code here was written for LiMo;
nothing was copied. The check is the FNV-1a hash, a public domain method.
The crate is under the Apache License 2.0, as is all of LiMo.

## Layout

```
phone/engine/
  Cargo.toml, Cargo.lock, README.md, .gitignore
  src/
    lib.rs        crate notes, the public names and the limits
    facts.rs      PhoneFacts, Front, CallState, PhonePermissions, Permission
    counts.rs     what counts on a phone, and what is never covered
    orders.rs     PhoneOrders, Cover, LockScreen, LockWhy, the ways out,
                  TimeLeft, WarningShown, TryAnswer, GrantTaken, PhoneNote
    words.rs      every sentence the phone shows
    today.rs      the child's own screens: Today and the rules in words
    kept.rs       the kept state: the stretch, the look, LiMo's clock, the
                  notes, each app's use today, setup, and the blob's form
    engine.rs     PhoneEngine: the call, the report and the grant, the ways
                  out, setup, keeping, the view
    error.rs      PhoneError, LoadProblem, in plain words
  tests/
    house_rules.rs
    phone/
      main.rs     the one story program
      common.rs   the fake phone, the PC beside it, and its checks
      day.rs, covers.rs, ways_out.rs, clock.rs, restart.rs, kept.rs, today.rs, setup.rs
```
