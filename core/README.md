# limo-core

The rules core of LiMo: one Rust library that holds all of LiMo's rules and
arithmetic. The Windows service uses it directly. The Android app will use it
through Kotlin bindings that UniFFI generates (not added yet).

It answers two questions:

1. **May this child use this device now, for how long, and if not, why not
   and until when?**
2. **How does one daily budget stay honest across a PC and a phone, with no
   server?**

It also holds the **ways out**, so that a child is never stuck and a guardian
can act with no server: extra time, given at home or asked for by the child;
one time codes read over a phone call; the guardian PIN typed on the child's
device; and a lock or an unlock a guardian sets from the master.

The library is pure. It has no clock, no files, no network, no threads, no
randomness and nothing global. Every function that needs the time is given
it, and every random byte (a code secret, a salt) is made by the caller and
passed in. The same inputs always give the same answer, on Windows and on a
phone. It has no unsafe code.

This is the first real piece of the product. The proofs in `spikes/` are not
used by it.

## Contents

- [Time and days](#time-and-days)
- [The rules](#the-rules)
- [The decision on a device](#the-decision-on-a-device)
- [One budget across devices](#one-budget-across-devices)
- [A worked day](#a-worked-day)
- [What a book holds](#what-a-book-holds)
- [Notes for the guardian](#notes-for-the-guardian)
- [The other devices' use](#the-other-devices-use)
- [Removing a device](#removing-a-device)
- [The ways out](#the-ways-out)
- [Security of codes and the PIN](#security-of-codes-and-the-pin)
- [What it guarantees](#what-it-guarantees)
- [Where the guarantee stops](#where-the-guarantee-stops)
- [Things a family will notice](#things-a-family-will-notice)
- [How the two sides call it](#how-the-two-sides-call-it)
- [Tests](#tests)
- [How to add a test case](#how-to-add-a-test-case)
- [Layout](#layout)
- [Dependencies and licences](#dependencies-and-licences)
- [Not in this piece](#not-in-this-piece)

## Time and days

Time is always a whole number of seconds since 1 January 1970 UTC ("unix
seconds"), as an `i64`.

Days and weekdays are always those of the **home** time zone: the zone of the
master PC. The rules carry it as a fixed offset from UTC in seconds (19800
for India). A phone abroad still counts home days, never the days of the place
it is in.

A **home day** is the stretch of time one daily budget covers. It starts at
the rules' day start (midnight unless the guardian moves it, for example to
04:00) and is numbered by the calendar date it starts on, counted from
1 January 1970. With a day start of 04:00, Wednesday's day runs from 04:00 on
Wednesday to 04:00 on Thursday, and 02:00 on Thursday still uses Wednesday's
budget.

A device's day never moves back. If its clock is set back, or new rules move
the day start later or the home zone back, it keeps counting on the day it is
already in until the calendar catches up. So a clock set back neither locks a
device nor gives it a new day.

## The rules

`RuleSet`, one per child, with a version number that grows with every change:

| Field | Meaning | Default |
| --- | --- | --- |
| `version` | Grows with every change. | 1 |
| `utc_offset_secs` | The home zone, as seconds from UTC. | set from the master PC |
| `day_start_minute` | When a new day starts, minute of the day. | 0 (midnight) |
| `budgets` | Minutes for each weekday, `Minutes(0)` for none, or `NoLimit`. | 120 every day |
| `blocked_hours` | Windows such as bedtime: weekdays, a start and an end minute. A window whose end is not after its start runs past midnight and belongs to the weekday it starts on. | none |
| `always_allowed_apps` | Apps that never use up the budget and stay usable when locked. | none |
| `blocked_apps` | Apps that can never be used. | none |
| `away_share_percent` | How much of a day's budget a phone may hold for use out of reach. | 50 |
| `lease_minutes` | How much time a device in reach is handed at a time. | 10 |
| `warning_minutes` | How long before a lock a warning comes. | 15, 5, 1 |

`RuleSet::validate` returns every problem it finds, each with the field it is
about and a sentence for the guardian ("Blocked hours number 1 starts and ends
at 21:00. To block a whole day, run it from 00:00 to 24:00, or set that day's
budget to 0."). App identifiers are opaque strings.

## The decision on a device

`decide(rules, holding, standing, now, app)`, or
`DeviceState::decide(now, app)`, gives a `Decision` that a Windows or Android
service can act on without knowing any rule. `standing` says how the
guardian's actions and the ways out stand on the device (a lock or unlock, a
wait after wrong guesses, whether the child may ask for time, whether the
device holds a PIN record and a code secret); `DeviceState` builds it.

- **allowed**, for the app in front (or for the device when no app is given);
- **lock**: the reason and when it lifts. The reasons are: this app is
  blocked (never lifts), blocked hours (until they end, joining windows that
  touch), no time today (a budget of 0), today's time is used up, "needs
  master" (the device has spent all it holds, while as far as it knows there
  is more at home), "held by another device" (the master has nothing free,
  but a phone still holds the rest as away time; it comes when that phone
  reports from home), and "locked by a guardian" (from the master, until a
  given time or until lifted);
- **device_lock**: the lock for ordinary apps, whatever is in front, for the
  lock screen; an always allowed app stays usable under it;
- **counts**: whether time with this app counts (not for always allowed apps);
- **seconds_left**: the best known time left today, for showing and warnings;
- **seconds_in_hand**: what this device may spend without asking anyone; the
  lock comes when it runs out;
- **warning**: when a warning point has just been reached, for time running
  out, for blocked hours about to start, or for a guardian's unlock about to
  end when a lock follows it. Each point comes once; if two points pass at
  once only the nearer one is given. If more time arrives, the points come
  again. While an unlock keeps the device open, a lock it covers gets no
  warning;
- **ask_master**: the lease is down to half; ask the master if it can be
  reached. Never under a guardian's lock: time would not open the device;
- **next_check_at**: when to decide again while the app in front stays in use
  (the next warning point, the lock, blocked hours starting or ending, the
  lease running low, the day changing, a guardian's lock or unlock ending,
  the wait after wrong guesses ending), and **next_check_if_idle_at**, the
  same for an idle device, with only the moments that come with the clock.
  Nobody has to poll every second;
- **unlocked_until**: when a guardian's unlock keeps the device open, when it
  ends;
- what the lock screen should offer: **can_ask_for_time** (today has a limit,
  the device is not locked for blocked hours or by a guardian, and no request
  from it is waiting), **can_use_pin**, **can_use_code**, and
  **try_wait_until**, the end of a wait in force before the next PIN or code
  try.

An always allowed app is allowed in every case and does not count. A blocked
app is refused in every case, under an unlock too. Otherwise a guardian's
lock comes first, then a guardian's unlock (which lifts blocked hours and a
lack of time), then the rules.

"Until when" is worked out by walking forward through blocked hours and days:
at 21:00 with the day used up and bedtime until 07:00, the answer is 07:00 the
next morning.

## One budget across devices

The master PC keeps, for each child and each day, a **pool**: the day's
budget, plus any extra time given for that day, less the use it has been told
about.

**Leases.** A device in reach of the master never holds the pool. It takes a
**lease** (10 minutes by default), spends it, reports, and asks again when the
lease is down to half. The PC that runs the master takes leases the same way,
through the same code, only without a network.

**The away allowance.** A phone can go out of reach at any moment without
saying so: at school, on mobile data, with the PC switched off, or because a
child turned Wi-Fi off. So a phone also holds an **away allowance**: time it
may spend when its lease is gone and the master cannot be reached. For a day,
it is the away share (half by default) of that day's budget, less the away
time already used that day. Spending always takes the lease first.

**Time added on the device.** A device may hold a third figure: time added on
it by a one time code or the guardian PIN (see [The ways out](#the-ways-out)),
and how much of it is used. Spending takes the lease first, then the time
added on the device, then the away allowance, which is the part the master
can ask a phone at home to give back.

**Reservations.** The master **reserves**, for every device, everything that
device might still spend without telling it: the lease it may hold, its away
allowance, and the time added on it that the master has heard of. Reserved
time is handed to no one else. A reservation shrinks only when the device
reports what it really used, or its day is over. A lease that was sent but
never confirmed stays reserved, because the device may have received and
spent it.

**Slices.** Use is reported as **slices**: child, device, app, day, start,
end, whether it counts, and an id: the device id, a slash and a number that
grows by one with every slice the device makes. A slice handed over twice is
counted once. Use across a day boundary is cut into one slice per day. A
device hands over every slice the master has not confirmed with every
report, so a lost report loses nothing. How the master counts each slice
once without keeping it is under [What a book holds](#what-a-book-holds).

**Messages.** There are two, both plain data:

- a **report** (device to master): its day, what it holds and has used, the
  newest grant and rules it has taken (that is the acknowledgement), its
  unconfirmed slices, and whether it wants more time. Also the time added on
  it and each unconfirmed piece of it, its unconfirmed requests for more time,
  its wrong guesses, the PIN version it holds, the newest guardian news it has
  taken, and a PIN unlock in force. And its notes for the guardian not yet
  confirmed (see [Notes for the guardian](#notes-for-the-guardian));
- a **grant** (master to device): the lease it may have for the day, in all;
  its away allowance for the day; its away share for later days; the current
  rules; which slices are now counted; what the master would say is left for
  it; how much away time other phones still hold; and whether to acknowledge
  at once (when its away allowance was just lowered). Also which pieces of
  added time and which requests the master now has, the guardian's lock or
  unlock, the PIN record while the device may not hold its newest version,
  the day's requests with their answers, and the guardian news number. And
  which notes the master now has, and the use on the child's other devices
  that day (see [The other devices' use](#the-other-devices-use)).

Every figure is a level ("your lease for today is 1800 seconds in all"),
never a change ("600 more"), and every message carries a number that only
grows. A device takes a grant only if it is newer than any it has taken; the
master keeps a report's figures only if it is the newest from that device. So
lost, repeated and late messages can never make anything count twice or go
missing.

**A new day with the phone away.** The phone works out its new away allowance
alone, from the rules and the away share it holds. The master works out the
same figure for every rule version and away share the phone might hold (the
ones it last acknowledged, and any sent since) and reserves the largest. The
two agree without talking. Two details keep it so:

- A phone that has not yet reported from the master's current day can still
  start that day on its own. That happens when its report came late, its clock
  is behind, or the home zone moved. So any grant to such a phone gives it an
  away share no larger than that day can still cover: what is already held
  back for the phone, plus what is free. A larger share comes with a later
  grant.
- When a grant brings new rules that start a new day on the phone, the phone
  starts it with the share and the rules of that same grant, never with its
  old share and the new rules. The master reserves only for the pairs it
  sent.

**Rule changes with the phone away.** The phone keeps its old rules until it
hears from the master. The master keeps every rule version a device might
still hold and reasons about what the device can spend under each.

**More than one phone.** Each phone gets the away share, but the master never
lets the shares the phones might hold add up to more than 100 percent. With an
away share of 75 percent and two phones, the first gets 75 and the second 25.

**Never more than the pool can cover.** The master never raises a phone's away
allowance beyond what is free, and after a budget cut it lowers it to what the
pool can still cover.

**A phone at home gives its away time back.** The away allowance is held back
from the others only while the phone might be away. When a device asks for
time and the pool cannot fill its lease, the master keeps the shortfall as
that device's **unmet need**. The next phone that reports has its away
allowance lowered by the needs of the others, never below what it has already
used away, and its grant asks it to acknowledge at once. Until it does, the
master keeps holding back the old figure, because the phone may never have
received the lower one. Once acknowledged, the time is free, and the waiting
device gets it at its next ask.

- A phone out of reach never reports, so nothing is ever taken from a phone
  that cannot hear. The guarantee below is untouched.
- Free time that a device is waiting for goes to it first, before any device
  that asked later. If several devices wait, the one that asked first is
  served first.
- If two phones are at home, the first to report gives back. A phone that is
  itself waiting for time is not drained for another device. A lowering that
  is not yet acknowledged counts against the need for three minutes, so two
  phones are not both cut for one need. If the phone lowered first does not
  acknowledge within three minutes (it may have left right after reporting),
  the next phone at home is asked instead.
- A device that is waiting asks again every minute. A need that has not been
  asked for again within three minutes (`NEED_KEPT_SECS`) lapses: the device
  went idle or out of reach, and time is no longer kept for it.
- When nobody waits any more and time is free again, a phone's allowance goes
  back up to its share at its next report, never above what is free.
- `MasterBook::wanted_reports(now)` names the devices worth asking to report
  right away: phones that could give time back, phones that must acknowledge a
  lowered allowance, waiting devices once time is free for them, and devices
  that have not yet reported holding the current rules, or the guardian's
  newest lock or unlock, PIN, or answer to a request. New rules are news like
  the others. The book cannot know which devices are in reach, so it names
  them all; asking one that cannot be reached costs nothing.

## A worked day

Two hours a day, an away share of half, ten minute leases. The phone and the
PC were paired the evening before. Minutes on the right are the master's view
of Wednesday's pool. This day is a test (`the_worked_day_in_the_readme` in
`tests/accounting.rs`), so the numbers are checked.

| Time | What happens | Used | Held back | Free |
| --- | --- | --- | --- | --- |
| 00:00 | A new day. The phone gives itself an away allowance of 60 minutes; the master holds back the same 60 for it without hearing from it. | 0 | 60 | 60 |
| 07:30 | The phone, at home, asks for a lease and gets 10 minutes. | 0 | 70 | 50 |
| 08:00 | The phone leaves for school without a word. The master still holds 70 back for it. | 0 | 70 | 50 |
| 10:00 to 10:45 | At school, out of reach, the phone is used for 45 minutes: its 10 minute lease, then 35 minutes of its away allowance. 25 are left. | 0 | 70 | 50 |
| 16:00 to 16:50 | The PC at home takes 10 minute leases from the 50 free minutes. At 16:50 it locks: the rest is held for the phone, which is at school and cannot give it back. The PC says so ("held by another device") and asks again every minute. | 50 | 70 | 0 |
| 17:00 | The phone comes home and reports its 45 minutes; the master now holds back only its 25 minutes of away allowance. The PC waits for 10, so the phone's allowance is lowered by 10 and it acknowledges at once. The 10 minutes are free, and the PC gets them. | 95 | 25 | 0 |
| 17:00 to 17:25 | The PC goes on. Each time its lease runs low, the phone at home gives back a little more, until its away allowance is all used. The PC locks at 17:25: today's time is used up. | 120 | 0 | 0 |
| 19:00 | The phone has nothing left: the PC used it at home. | 120 | 0 | 0 |

Total: 120 minutes, exactly the budget, though the phone was out of reach
half the day and the master never knew where it was. The PC got 75 minutes,
the phone 45.

## What a book holds

A `MasterBook` is saved after every exchange, so it is kept small. It holds
the child's rules (the current version and any older one a device may still
hold), each paired device's newest report figures, the grants it has not
acknowledged and the highest note number taken from it, the ids of removed
devices, the PIN record, the guardian's lock
or unlock, and for each day it keeps: the counted seconds of each device (and
of devices since removed), each piece of extra time, each request with its
answer, and each device's **mark**.

**It does not keep the slices.** For each device and day it keeps the highest
slice number it has taken, its mark, and takes a slice of that day only when
its number is above the mark. That counts every slice exactly once, through
lost, repeated and late reports, because:

1. a device numbers its slices 1, 2, 3 and on, and its day never moves back,
   so the slices of one day carry numbers in one unbroken run;
2. every report carries every slice the master has not confirmed;
3. a device drops a slice only once a grant has confirmed it, that is after
   the master took it.

So when the mark is at n, the report that brought slice n also carried every
slice of the day numbered below n that was not yet confirmed, and the
confirmed ones had been taken before: every number up to n has been taken.
The slices of one report are taken together, so their order in it does not
matter, and a slice listed twice counts once. A slice whose id is not the
device id, a slash and a number is confirmed and not counted, as a slice that
names another device is: no device makes one. The simulation checks all of
this against a ledger of its own at every report (see [Tests](#tests)).

**What a report brought.** `MasterBook::handle_report` returns a
`ReportOutcome`: the grant, and the slices, pieces of time added on the device
and requests it took for the first time, with whether the report was the
newest from its device. A caller that keeps its own record (the hub keeps
usage for the guardian's screens) stores exactly those, and so stores each
once, however often a device sends it. `MasterBook::counted_use(day)` gives
each device's counted seconds for a day as the book has them.

**How big.** Two devices, a slice every 108 seconds each and a report every
two minutes, over two days (3,200 slices): a book that kept every slice was
744,818 bytes of JSON; it is now about 2,200 bytes (test
`a_busy_book_stays_small`). The hub has the book forget the days before
yesterday (`forget_days_before`, which says whether anything changed), so it
stays that size.

**Books saved by earlier versions** still load and count the same. A book
saved when books kept every slice has them folded into marks as it loads (the
counted seconds were kept beside the slices and are taken as they are), and is
saved in the new shape. Every field added since the first piece is filled in
when missing. The tests load books, device states and messages saved by the
first piece, by the code before marks, by the code with marks, and by the
code with notes (`tests/fixtures/piece_four_*`).

**A device that loses its saved state** and numbers its slices from 1 again
is not covered (it was not before either; see
[Where the guarantee stops](#where-the-guarantee-stops)): on a day the book
already has a mark for it, its new slices are taken as counted already until
their numbers pass the mark; on a later day they count. Its message numbers
start again too, so the master ignores its figures meanwhile. The way out is
to remove it and pair it again, under a new id.

## Notes for the guardian

A phone notes what a guardian should know, and the master PC shows it. The
kinds are all in `src/notes.rs`, in one place (`PhoneNote`), and a kind added
later is added there:

| Kind | Fields | What |
| --- | --- | --- |
| `setup_finished` | `at` | A guardian finished setup on the phone. Until this note comes, the master knows that setup is not finished. |
| `permission_off`, `permission_on` | `permission`, `at` | One of the six things LiMo needs (`notifications`, `phone`, `usage_access`, `accessibility`, `battery`, `protection`, in the order setup asks for them) was switched off, or on. |
| `protection` | `level`, `at` | The phone's protection level now: `off`, `base` or `strict`. |
| `usb_debugging` | `on`, `at` | USB debugging was found on, or off again. |
| `guarded_page_opened` | `page`, `day`, `at` | A page that could switch LiMo off (`device_admin` or `accessibility`) came to the front and was covered by the guard; once a day for each page. |
| `guard_opened` | `until`, `at` | A right guardian PIN opened the guarded pages. |
| `restarted` | `at` | The phone was started again. |
| `clock_moved` | `by_secs`, `followed`, `at` | The phone's clock moved against its steady count; `followed` when LiMo followed it. |

Times are by the phone's own clock. In JSON a note reads
`{"note":"permission_off","permission":"usage_access","at":1794997860}`.

**Carried like slices.** A device numbers its notes 1, 2, 3 and on; each
travels in every report as a `SentNote` with the id `<device>/note/<n>`
until a grant confirms it (`Grant::confirmed_notes`), and the device drops
it then. The master takes each note once, by a mark per device as for
slices but for all days at once, and names the new ones in
`ReportOutcome::new_notes`, the newest 200 at most (`MAX_NOTES_PER_REPORT`,
also the most a device keeps). A caller that shows notes keeps exactly
those, in the same change that keeps the book: the grant confirms them, so a
note not kept by then is gone. A note whose id is not the device's is
confirmed and not taken; a device restored from an old copy has its notes
taken as known until its numbers pass the mark.

**A newer device.** Reading a report leaves out every note of a kind, or
with a value, that this version does not know, and takes the rest of the
report: a phone newer than its PC still reports. A field the reader does not
know inside a known kind is passed over. A report that carries no notes is
written exactly as before, with no field for them.

The core makes no notes and keeps none on the device side: the phone's
engine makes them and keeps them with its own state, and puts them in the
report (`../phone/engine`).

## The other devices' use

Every grant carries the use on the child's other devices that day, as the
master counts it when it makes the grant (`Grant::others_used_secs`: every
device but the one the grant is for, removed ones among them), and whether
all of it was on the master PC, the child's device of kind `OnMaster`
(`Grant::others_all_on_master`; false with none, and false when some was on
a removed device, whose kind is not kept). With the device's own use it is
the child's whole day, so a phone can say "on the PC 17 min" beside its own
31.

A device keeps the figure of the newest grant it took for its day
(`DeviceState::others_used(day)`, an `OthersUsed` with the day, the seconds
and whether all on the master PC). It is what the master knew then, and
stays the best known figure out of reach: unlike the time left, it is not
forgotten by `lost_contact()`. It is never shown for another day, and a
grant from a master that does not say (`None`, one made before this field)
leaves the device knowing nothing of the others for that day.

## Removing a device

`MasterBook::remove_device(id, now)` removes a device from the child, for
good:

- **Its use stays counted.** Its counted seconds stay with every day the book
  keeps, apart from the live devices (`counted_use` shows them as removed), so
  removing a device never gives its used time back to the child.
- **It holds nothing any more.** No lease, no away allowance, no away share.
  What the master held back for it is free for the other devices at once,
  and the other phones can be given its away share at their next report.
- **Time added on it and not used is dropped.** Time a code or the PIN added
  on that device belonged to it: no other device may spend it. The part its
  newest report said it used stays counted, as extra time given and used; the
  rest is dropped from the day's extra time (`ExtraTime::dropped_secs`, the
  pieces it added last first), so it is neither held back nor handed to
  another device.
- **Its record goes**: the grants it has not acknowledged, an unmet need, its
  code secret. Anything that arrives from it later (a late report) is refused
  with `MasterError::DeviceRemoved` and changes nothing. Removing an unknown
  device, or one twice, is an error, and a removed id is never paired again: a
  device paired anew gets a new id, so a late report from the old one can
  never be taken for the new one's.

Use the device made and never reported is not counted: the master trusted the
device to report it, and no longer holds anything back for it. So a device is
best removed after its last report, when it can still make one, or when it is
gone for good (lost, broken, reset). See
[Where the guarantee stops](#where-the-guarantee-stops).

## The ways out

A child must never be stuck, and a guardian must be able to act with no
server. These are the ways, all here as pure logic; the services show them
and carry the messages.

**Extra time given at home.** A guardian gives minutes for today at the
master: `MasterBook::give_extra(id, minutes, now)`, 1 to 1440 minutes. Every
piece of extra time is a record with an id. The same id given twice counts
once, so a screen that sends one tap twice gives nothing more, and
`MasterBook::extra_time(day)` shows where a day's extra came from: given at
the master, a request granted, a code, or the PIN, with the device. The time
goes into the day's pool, and devices in reach get it with their next leases
(a device waiting at the lock screen within a minute). `add_extra_secs`, from
the first piece, still works; it records each call under a new id.

**Asking for more time.** On any device the child can ask for 1 to 240 minutes,
with a short reason or none (`DeviceState::ask_for_time`). The request travels
with the next report, and with every report after until the master confirms
it, like a slice: a request made at school reaches the master when the phone
comes home, and a lost message loses nothing. The master keeps it once, by its
id (`MasterBook::waiting_requests`). A guardian answers it once
(`MasterBook::answer_request`): granted, with a number of minutes that may
differ from the number asked for, or refused. Granted minutes go into the
pool of the request's day as extra time, once. Every grant carries the day's
requests and their answers, so the answer reaches all of the child's devices
and their lock screens can say what happened (`DeviceState::requests_today`).
A device has at most one request waiting and makes at most five a day. A
request can be answered only while its day lasts.

**Time added on a device.** With no network, time can be added on the device
itself, by a one time code or the guardian PIN. It belongs to that device and
that day, and can be spent at once. Each piece has an id (the device, `code`,
the day and the code's selector; or the device, `pin` and a number), travels
with every report until the master confirms it, and joins that day's extra
time at the master once. From then on the master holds back, for that device,
the part not yet used, as it does its lease and its away allowance. It is not
given back to other devices; if the device is removed, the part it did not
use is dropped (see [Removing a device](#removing-a-device)).

**One time codes.** A guardian who is not at home reads a short code over a
phone call; the child types it on the lock screen.

- Each device has its own code secret: 32 random bytes made by the caller at
  pairing, kept by the master (`MasterBook::set_code_secret`) and by the
  device (`DeviceState::set_code_secret`). The master hands it to guardian
  phones while they are at home (`MasterBook::code_secret`).
- A guardian phone makes the next code with
  `next_code(secret, device, day, minutes, counters made today)`: 15, 30 or
  60 minutes, for that device and that home day. A device takes at most ten
  codes a day (counters 0 to 9); running out is an error to show.
- A code is 8 digits, shown as two groups of four: "1342 0715". The first two
  are the selector: the minutes' index times ten plus the counter, so 13 is
  the fourth code of the day, worth 30 minutes. The last six come from
  HMAC-SHA256 keyed with the code secret, over: the label
  `LiMo one time code v1` and a zero byte; the device id's length as 8 bytes
  big-endian, then the id in UTF-8; the home day as 8 bytes big-endian; the
  selector as one byte. The first 8 bytes of the result, read big-endian,
  modulo one million, are the six digits. Known answer tests fix all of it.
- On the device (`DeviceState::try_code`) spaces and hyphens are ignored, and
  anything that is not then 8 digits is refused without counting as a guess.
  A well formed code that is not right for this device today counts as a
  wrong guess: a code for another device, a code from another day, or one
  made up. A right code works once (the device remembers the selectors it has
  used that day); typed again it is refused as used, without counting. The
  secret digits are compared in constant time.
- Guardian phones that make codes for the same device on the same day must
  know each other's counters, or two of them can make the same code (the
  second is then refused as used). Phones that cannot share can split the
  counters between them.

**The guardian PIN.** 6 to 8 digits (the owner's decision), typed by a
guardian on the child's device, with no network.

- It is set at the master (`MasterBook::set_pin(pin, salt)`, with a salt of 16
  bytes or more made by the caller) and kept only as a salted slow hash. A PIN
  of fewer than 6 digits is refused (`PinError::NotSixToEightDigits`, "A PIN
  is 6 to 8 digits, with nothing else in it."). Earlier versions took 4 or 5
  digits, and a record made then goes on working: **a record does not know
  how long its PIN is**, so a device checks any typed PIN of 4 to 8 digits
  against it (`MIN_TYPED_PIN_DIGITS`). No new record can be made from fewer
  than 6, and setting a new PIN replaces the old one everywhere. The
  record reaches every device with grants, with a version, so a changed PIN
  replaces the old one wherever a grant arrives; `MasterBook::clear_pin`
  removes it. A device that has not heard of a change keeps the old PIN. A
  device with no record refuses PIN actions.
- A right PIN on the device (`DeviceState::try_pin`) can add minutes for
  today, as time added on the device; unlock the device until a given time, at
  most a day ahead, whatever the budget or the blocked hours say; or confirm
  "release this device" (the library only says the PIN was right; the service
  releases the device). An action that could not be done (no minutes, an
  unlock in the past) is refused before the PIN is looked at.

**The wait after wrong guesses.** Codes and the PIN share one counter of
wrong guesses on each device. The first four are free. After the fifth, the
next try must wait a minute, and each further wrong guess doubles the wait:
1, 2, 4, 8, 16 and 32 minutes, then an hour each time. During a wait nothing
is tried, so nothing is counted. A right PIN clears the counter; a right code
does not. The counter and the end of the wait are part of the device's saved
state, so a restart changes nothing. The wait is kept as the moment it ends,
so a clock set back makes it longer, never shorter. Reports carry the count,
so the guardian sees it (`DeviceView::wrong_guesses`).

**Lock now and unlock, from the master.**

- **Lock now** (`MasterBook::lock_now(until, now)`): all of the child's
  devices lock, until a given time or until lifted, with their own reason,
  "locked by a guardian". Always allowed apps stay usable, and a blocked app
  stays blocked. A locked device does not ask for time.
- **Unlock** (`MasterBook::unlock_until(until, now)`, at most a day ahead):
  blocked hours and a lack of time are lifted until then. Blocked apps stay
  blocked.
- **Lift** (`MasterBook::lift(now)`): the rules alone decide again.

Each is a level with a version, carried by every grant, and
`wanted_reports` names the devices that have not taken it yet, so the hub can
ask them to report at once. A device out of reach hears of it when it is back.

The PIN can unlock one device on the spot. When a lock and an unlock are both
in force on a device, the one set last wins until it ends; then the other, if
it has not ended, applies again. Lifting at the master ends a PIN unlock set
before it. The moments compared come from two clocks, the master's and the
device's, both meant to be right.

**Time used during an unlock still counts.** An unlock only keeps the device
open. Use is recorded and counted as always: from the lease, the time added on
the device and the away allowance, in that order, and when all of that is
spent, beyond it. So the day's budget can run out and stay out: when the
unlock ends, the device locks for lack of time, and the master has counted
everything, so the other devices get nothing more that day. During an unlock
the device still asks the master for leases, every minute while it holds
nothing, so that time the master can give is spent before the device goes
beyond what it holds.

**What a family will notice.**

- A code works on the phone it was made for, on the day it was made for, once.
  Read the code for the right device.
- A request answered while the phone is out of reach puts the time into the
  day's pool; the phone gets it when it is back in reach. To give a phone
  away from home time at once, read it a code.
- Time added on the phone stays with the phone. If the child moves to the PC,
  that time does not follow, and if the phone is removed, what it did not use
  is gone.
- A removed phone's used time still counts for the day; the time it held is
  free for the other devices at once.
- A changed PIN reaches a phone away from home only when it comes back; until
  then the old PIN works there.
- After a film night unlocked from the master, the day's budget is likely used
  up; the rest of the day is locked unless a guardian gives more.
- A lock until lifted on a phone that then leaves home stays on it until the
  phone is back in reach. A guardian with the PIN can still unlock it there.

## Security of codes and the PIN

**Codes.** The selector is not secret; the six secret digits make a guess right
once in a million. The wait after wrong guesses allows at most 33 tries in the
first 24 hours of guessing (5 at once, 6 more as the wait grows to 32 minutes,
then one an hour) and 24 a day after that. So the chance of guessing a working
code is about 1 in 30,000 in the first day, and about 1 in 115 over a year of
guessing every hour, day and night. A guessed code adds at most an hour, once.
Without the code secret no one can make codes; the secret is kept by the
master, the device and the guardian phones only, and never appears in `Debug`
output, error messages or logs.

**The PIN.** It is kept only as PBKDF2-HMAC-SHA256 with 200,000 rounds, a salt
of 16 bytes or more and a 32 byte hash, compared in constant time.

- Why PBKDF2 and not Argon2id: it is built from the same two pieces as the
  codes (HMAC and SHA-256), so it adds one small crate instead of four; and
  for a PIN of six to eight digits the choice of hash changes little, as
  below.
- Why 200,000 rounds: the check must stay well under a second on a 2020
  phone. Measured on this project's PC (an AMD Ryzen 9 7950X3D, which has
  SHA instructions), one check takes about 16 ms in a release build. The same
  check with the hashing done in software only takes about 54 ms there, so a
  2020 phone with the ARMv8 SHA instructions (nearly all 64-bit ones), which
  sha2 finds and uses at run time, should take about 0.03 to 0.1 s, and the
  slowest, a 32-bit or older phone hashing in software, about 0.3 to 0.6 s.
  These phone figures are estimates, not measurements: run
  `cargo test --release --test ways_out -- --ignored --nocapture` on a phone
  to measure. Each record carries its rounds, so a later version can change
  the cost without breaking records already made.
- **A PIN is protected mainly by the devices keeping the record private, not
  by the hash.** Anyone who copies the record from a device (a rooted phone,
  a backup) can try every PIN: a 6 digit PIN in minutes, an 8 digit one in
  about a day on one desktop and in hours on a graphics card. On the device
  itself, guessing is slowed by the wait: 24 tries a day, so within a year of
  patient guessing a 6 digit PIN is found about once in 115, a 7 digit one
  about once in 1,140, an 8 digit one about once in 11,400. That is why a
  PIN is now 6 digits or more. (A 4 digit PIN set by an earlier version, still
  in use until the guardian sets a new one, is found within a year about 9
  times in 10.)

**What is protected, and what is not.**

- Protected: a code works only on its device and day, once; codes cannot be
  made without the secret; guessing is slowed and seen by the guardian; time a
  device adds is counted once at the master; the PIN is never kept or sent,
  only its salted hash; no secret or PIN appears in `Debug` output, error
  messages or logs; secrets and PINs are compared in constant time.
- Not protected by this library: the state files and the messages themselves.
  The services must keep the device's state, which holds the code secret and
  the PIN record, out of the child's reach (Android's private app storage and
  keystore, Windows' protected service data), and must encrypt messages
  between devices. A device that has been taken apart can make codes and find
  a short PIN; the master believes the time a device says it added ("honest
  devices", below). A clock set forward shortens a wait, as it starts a new
  day early; the platforms lock the clock down.

## What it guarantees

**As long as a day's budget is not cut while some device still holds the
higher figure, the time that counts on all of a child's devices together
never passes that day's budget plus all of its extra time, whether given at
the master (directly or by answering a request) or added on a device by a code
or the PIN, each piece counted once. Not by one lease, not by one second.**
The one exception is chosen by a guardian: while an unlock keeps a device
open with nothing left in hand, its use goes on counting beyond (see
[The ways out](#the-ways-out)); everything else stays within the bound. The
design needs no slack at all. This holds through any mix of use on several
devices at once, going out of reach and coming back, lost, repeated and late
messages, day changes, every other kind of rule change (a higher budget,
blocked hours, apps, the away share, the lease length), extra time given at
the master and added on devices in and out of reach, guardian locks and
unlocks, restarts, and devices removed after their last report and new ones
paired in their place.

Why, in six steps:

1. A device spends only what it holds: lease left, plus time added on it
   left, plus away allowance left. The decision gives the exact moment that
   runs out, and the device locks then, unless a guardian's unlock keeps it
   open.
2. Whatever a device can hold, the master has reserved. It works this out
   from the device's newest report and every grant sent since, any of which
   may have arrived. Grants are numbered levels, and a device takes only a
   newer one, so a device can never hold something the master does not
   count.
3. The master hands out time (a lease, or a larger away allowance) only from
   what is free: budget plus extra, less what is counted, less what is
   reserved.
4. At a new day a phone gives itself its away allowance without asking. The
   master hands out away shares that add up to at most 100 percent, so these
   allowances together are never more than the budget. For a day the master
   is already handing out, a phone that may still start it on its own is
   never sent a share larger than that day can still cover.
5. Every slice counts once, and a report's figures count only if they are
   the newest.
6. Time added on a device is extra beyond the budget. Until the master hears
   of it, it is in no reservation and in no pool: only that device can spend
   it. When the master hears of it, from any report (each piece travels with
   every report until confirmed, and counts once by its id), the day's extra
   grows by the piece and the device's reservation by the part not yet used,
   so what the master can hand out does not change. Every report that carries
   use of a piece carries the piece too.

Taking time back from a phone at home changes none of this. A lower away
allowance only lowers what the phone may hold. The master goes on reserving the
old figure until the phone acknowledges the lower one, and a phone that never
hears never acknowledges.

So what is counted plus what is reserved never passes the budget plus the
extra time the master knows of, and everything really used is always inside
what is counted plus what is reserved plus the time added on devices that the
master has not yet heard of (plus use beyond what a device held, under an
unlock). The simulation checks all of these at every step of hundreds of
generated days (see [Tests](#tests)).

## Where the guarantee stops

These are the only cases that need slack, and how much:

1. **A day's budget is cut while a device still holds the higher figure.**
   That is a cut made during the day, or a cut made earlier that a phone away
   for days has not heard of. Devices can still spend what they already hold
   under the higher figure, because they cannot hear of the cut at once:
   - a device in reach: the rest of its lease, at most one lease each, until
     its next report (half a lease of use later at the most). The phone's away
     allowance is lowered with the same report, down to nothing if the pool is
     already overdrawn;
   - a phone out of reach: its lease and its whole away allowance under its
     old rules, until it comes home.

   So after a cut the day's total stays within the highest budget for that day
   in any rule version a device held or the master used, plus extra time. It
   can pass the new budget by more than one lease, and this cannot be helped:
   the test `a_cut_while_the_phone_is_away_can_pass_the_new_budget_by_more_than_a_lease`
   shows a cut from 120 to 30 minutes while the phone is at school, and the
   day ending at 120. With every device in reach, a cut costs at most the
   leases already out (test `a_cut_while_every_device_is_in_reach...`).
2. **A day with no limit is given one.** A device that still holds the old
   rules can spend any amount until it hears. The master holds back the whole
   day meanwhile, so no other device gets time from it.
3. **The home zone or the day start changes.** Each device counts days by its
   own rules until it hears of the change, and the limit holds for each day as
   the devices counted it. Use near the boundary can be put on the
   neighbouring day, by up to the size of the change.
4. **Late locks.** The decision gives the exact moment of a lock. Use after
   that moment (the agent was slow, the lock screen took a moment) is recorded
   truthfully and counts on top. On the proof phones a lock took well under a
   second.
5. **Use recorded late.** An agent must record use promptly, before the day
   changes. Use recorded after its day ended still counts on that day, but
   could not be held to its limit.
6. **Lost state.** A device that loses its saved state (reinstalled, restored
   from an old backup) is not covered, nor a master that loses its book. Each
   side must save its state before acting on a change: the master before it
   sends a grant, a device after it takes one. A device restored from a
   backup numbers its slices again from where the backup left off, and on a
   day the master already took higher numbers from it, its new use there is
   not counted (see [What a book holds](#what-a-book-holds)). Remove such a
   device and pair it again.
7. **Clocks.** A clock set forward starts a new day early, with a fresh away
   allowance, and shortens a wait after wrong guesses. Both platforms lock the
   clock down (see `docs/PLAN.md`), and reporting clock jumps is a later
   piece. A clock set back only makes a wait longer.
8. **Honest devices.** The master believes what a device reports, including
   the time it says a code or the PIN added. LiMo's enforcement runs on the
   devices anyway. Absurd figures from a faulty device (a used time of
   billions of seconds, a slice from the year 1 to the year 9999, a piece of
   added time of more than a day) can neither crash the master nor widen the
   pool: all the arithmetic saturates, such pieces are not taken as extra
   time, and such figures only make the master hold back more.
9. **An unlock.** A guardian's unlock keeps a device open when it holds
   nothing more; that use counts on top of the bound, by exactly as much as
   was used beyond what the device held. The master counts all of it, so the
   pool shows it as over (`PoolView::over_secs`), and no other device is
   given time from that day.
10. **A removed device.** Removing a device frees at once everything the
    master held back for it. Use it had made and not yet reported was inside
    that, so the day can pass its limit by that use: at most what was held
    back for it (its lease, its away allowance, the time added on it), and
    only what it had really used and not reported. Removed after its last
    report, it costs nothing. A removed device that goes on being used beside
    the others, still holding its old state and away share, is not covered:
    the service on it must be released (the PIN confirms "release this
    device") or the device be gone. The simulation removes devices both ways,
    and allows check 1 only the use a removed device had not reported, within
    what was held back for it (see [Tests](#tests)).

## Things a family will notice

- **With the phone at home, the PC can use the whole day.** It gets the
  budget less what the phone itself used, and less the rest of the last lease
  the phone took while in use (at most one lease; leases are not taken back).
  See `with_the_phone_lying_at_home_the_pc_gets_the_whole_budget`.
- **A phone that leaves after the PC used the time has less away time.** If
  the PC used the phone's away share in the morning, the phone at school has
  only what is left. That is the fair price of one budget: the time was spent
  at home. See
  `the_pc_gets_all_but_what_the_phone_used_and_still_holds_then_the_phone_leaves`.
- **With the phone away, the PC waits for it.** The PC shows "held by another
  device" and gets the rest when the phone comes home and reports, minus what
  the phone spent away.
- **A phone that spent its away allowance at school has, at home, only what
  the master can lease it.** Its warnings follow what the master last said is
  left, not only what it holds, so it does not warn wrongly.
- **A device locked for lack of time asks again every minute** while it can
  reach the master, so extra time, or time a phone gives back, reaches it
  within a minute. With the hub prodding phones (below) it usually arrives
  before the PC's lease runs out, and the PC never locks at all.

## How the two sides call it

Both sides keep their state in plain data that derives `Serialize` and
`Deserialize`, and save it after every change. State saved by an earlier
version loads: every field added since is filled in when missing, and a book
that kept whole slices has them folded into marks (see
[What a book holds](#what-a-book-holds)).

**The master** (the hub, on the master PC), one `MasterBook` per child:

```rust
let mut book = MasterBook::new("asha", rules)?;           // once
let welcome = book.add_device("phone-1", DeviceKind::Portable, now)?;
// ...send `welcome` to the phone with its pairing; it makes its state from it.

// For every report that arrives:
let outcome = book.handle_report(&report, now)?;
keep(&outcome.new_slices);     // what the book took for the first time, once:
                               // also outcome.new_added, outcome.new_requests
                               // and outcome.new_notes
save(&book);                   // with what was kept, before sending
send(outcome.grant);
// Then ask the devices the master wants to hear from to report at once:
// phones that can give time back to a waiting device, phones that must
// acknowledge a lower allowance, waiting devices once time is free, and
// devices that have not yet taken the newest rules or guardian news.
for id in book.wanted_reports(now) {
    prod(id);                  // if it can be reached; the PC's agent is local
}

// When the guardian changes the rules:
let taken = book.change_rules(new_rules)?;   // the book sets the next version;
                                             // they reach each device with its
                                             // next grant, and the devices are
                                             // in wanted_reports until they have

// For the guardian's screens:
book.pool(day);                // used, held back, free
book.device_view("phone-1", day)?;  // used, held back, waiting for, giving back,
                                    // added on it, wrong guesses, a PIN unlock,
                                    // the rules version it holds, whether it has
                                    // the latest news, how far off its clock is
book.counted_use(day);         // each device's counted seconds, removed ones too
book.extra_time(day);          // where the day's extra came from

// When the guardian removes a device (after its last report if it can make one):
book.remove_device("phone-1", now)?;   // its use stays counted; what it held is
                                       // free at once; later reports are refused

// Once an hour or so, keep the book small:
if book.forget_days_before(today - 1) { save(&book); }

// At pairing, with 32 random bytes made by the service for this device:
book.set_code_secret("phone-1", secret.clone())?;   // the device takes it too
// Once, and whenever the guardian changes it (salt: 16 random bytes or more):
book.set_pin(&pin, salt)?;     // 6 to 8 digits; slow on purpose; reaches
                               // devices with grants

// The guardian at the master. Each reaches the devices with their next
// grant; then prod the devices in `wanted_reports`, as above.
book.give_extra("tap-7f3a", 30, now)?;        // the same id twice counts once
book.lock_now(Some(until), now)?;             // or None: until lifted
book.unlock_until(until, now)?;               // at most a day ahead
book.lift(now);
for request in book.waiting_requests(now) {   // shown to the guardian
    book.answer_request(&request.request.id, RequestReply::Grant { minutes: 20 }, now)?;
}
// A guardian phone at home takes `book.code_secret("phone-1")?` and later,
// away, makes codes with `next_code(secret, "phone-1", day, 30, counters_made)`.
```

**A device** (the Windows agent for each child, the Android service), one
`DeviceState` per child:

```rust
let mut state = DeviceState::from_pairing(&welcome, now);   // once

loop {
    let now = clock();
    state.record_use(&app, last_mark, now);   // active use only, since the last mark
    let mut d = state.decide(now, Some(&app));
    if d.ask_master || prodded_by_the_hub || a_few_minutes_since_the_last_report {
        let want_more = d.ask_master && in_use;   // an idle device only reports
        let mut report = state.report(now, want_more);
        report.notes = my_unconfirmed_notes();    // a device that makes notes
        match send(report) {
            Ok(grant) => {
                state.take_grant(&grant, now);
                forget_notes(&grant.confirmed_notes);   // from any grant for it
                if grant.acknowledge_now {           // its away allowance was lowered
                    if let Ok(g) = send(state.report(now, want_more)) { state.take_grant(&g, now); }
                }
                d = state.decide(now, Some(&app));
            }
            Err(_) => state.lost_contact(),
        }
    }
    act_on(&d);              // lock or unlock, show the warning, show the time left;
                             // offer "ask", the PIN and "type a code" as `d` says
    save(&state);
    sleep_until(if in_use { d.next_check_at } else { d.next_check_if_idle_at },
                or the app in front changing, or input after idleness);
}

// At pairing, the code secret the master also keeps:
state.set_code_secret(secret)?;
// On the lock screen. Each changes the state: save it, then decide again.
state.ask_for_time(30, "For the school project", now)?;  // goes with the next report
state.try_code("1342 0715", now);                        // TryOutcome: Added, Wrong, MustWait, ...
state.try_pin(&pin, PinAction::AddMinutes { minutes: 20 }, now)?;
state.try_pin(&pin, PinAction::UnlockUntil { until }, now)?;
state.try_pin(&pin, PinAction::ConfirmRelease, now)?;    // the service then releases
state.requests_today(now);                               // what was asked, and the answers
state.others_used(day);                                  // the other devices' use that day
```

Rules for callers:

- **Report regularly while in reach**, not only when `ask_master` is set:
  usage then reaches the master, and on a day with no limit a device has no
  reason to ask for time, so it would never hear of new rules. After the rules
  change, every device is in `wanted_reports` until it reports holding them.
- **Keep what the outcome names, not what the report carries.** A report
  carries every slice not yet confirmed, so the same slice comes many times;
  `ReportOutcome` names each once. The same holds for notes, and a note must
  be kept before the grant that confirms it is sent.
- **Ask for more (`want_more`) only while in use.** An idle device reports
  without asking, so that it neither takes a lease it will not use nor keeps a
  need open. A device waiting at the lock screen keeps asking every minute, as
  its decision tells it to; a need not asked for again within three minutes
  lapses.
- **When a grant says `acknowledge_now`, report again at once**, asking for
  more if the first report did. The lowered away allowance only frees time
  once the master knows the phone has it.
- **The hub asks the devices in `wanted_reports` to report at once.** Without
  that, time a phone gives back moves only at the phone's next regular report,
  and the PC waits until then.
- **Call `lost_contact()` when the master cannot be reached.** The device then
  stops trusting what the master last said is left, and its warnings follow
  the time in hand.
- **Decide again when the app in front changes**, and after a code, the PIN or
  a request.
- **Save the device's state after every code or PIN try**, right or wrong:
  the wrong guesses and the codes used must survive a restart.
- **Keep secrets secret.** The code secret, the PIN record and the typed PIN
  must stay out of logs, crash reports and screens. The library's `Debug`
  output already hides them; the state files hold them, so they belong in
  private storage.

## Tests

`cargo test` runs seven groups, all deterministic:

| File | What | Count |
| --- | --- | --- |
| `src/*.rs` | Unit tests: day and weekday from unix seconds and offset, negative offsets, a day start not at midnight, far past and far future dates, splitting at day boundaries, validation, blocked windows, the master's books, slice numbers read from ids, absurd figures and far times; known answers for codes and PIN hashes (worked out separately in Python), codes bound to their device and day, PIN shapes (6 to 8 digits new, a 4 digit record made earlier still verifying), the waits after wrong guesses, which lock or unlock is in force, spending order, secrets kept out of `Debug`, note numbers read from ids, and the six permissions once each in setup order | 51 |
| `tests/cases.toml` with `tests/cases.rs` | The file of cases a product owner can read and extend (89 cases, 34 of them for the ways out, and 4 rules checks), a check that a wrong expectation fails with the case's name, and the file's reader | 3 tests |
| `tests/accounting.rs` | The shared budget as short stories: the worked day above, a phone at home giving its share to the PC, a phone that then leaves, a phone away that gives nothing, a lowering grant that is lost, two phones at home, a lowered phone that left, a need that lapses, a larger share that must not reach a phone late into a spent day, new rules that start a day, and JSON round trips of every message and state (including state saved before the latest fields) | 24 |
| `tests/ways_out.rs` | The ways out as stories: extra time given twice with one id; a request made at school, answered at home, and seen on every device; a request too late or answered twice; a code used at school and counted once at home; the same code on the PC; a code from yesterday; all ten codes of a day; the PIN adding time with no network; wrong guesses and the growing wait across a restart and a clock set back; a changed and a removed PIN; a lock from the master and lifting it; an unlock from the master that keeps counting; a PIN unlock over a lock from the master; state saved by the first piece still loading; secrets kept out of `Debug` and errors; JSON round trips of the new fields. One more, not run by default, measures the PIN hash on the machine it runs on | 16, and 1 |
| `tests/callers.rs` | What the hub, the master's first real caller, needs, as stories: a removed phone keeping its use and freeing what it held and its away share at once; anything from a removed device refused without a change; time added on a removed device and not used, dropped; use a removed phone never reported, not counted; the report outcome naming each new slice, piece and request once; slices lost, repeated, late, out of order, twice in one report and with ids no device makes; a device restored from a backup; a busy book's size; new rules as news; the book setting the next rules version; how far each device's clock is; forgetting days; a book saved with whole slices (by the code before marks, with a four digit PIN) loading and counting the same; a book saved with marks | 14 |
| `tests/notes.rs` | Notes and the other devices' use as stories: notes carried until confirmed and taken once through lost, repeated, late, reversed and stale exchanges, twice in one report, with ids no device makes, across a book saved and loaded, with marks of their own for each device; a report with more notes than a device keeps giving the newest; notes of a removed device refused with its report; every kind in its JSON; a report from a newer device with kinds and values this version does not know, still taken; the grant's figure of the other devices' use (none, all on the PC, the PC hearing of the phone, a tablet, a removed tablet, a stale grant, a master that does not say, the next day), kept by the device out of reach; reports, grants, books and states saved by the first piece, before marks and with marks still reading and taking notes; a book, a phone's state, a report and a grant saved by this version | 8 |
| `tests/simulation.rs` | The simulation below | 1 |

**The simulation** plays the master, the PC and one or two phones through 300
generated scenarios of two or three days each, from a seeded generator of its
own. Each scenario mixes use on every device (often at the same time), phones
going out of reach without a word, reports and grants lost (up to half of
them), messages repeated and arriving late, rule changes (budgets, blocked
hours, away share, lease length, and in some scenarios the home zone and the
day start), extra time and day changes. In three scenarios in four, a hub
asks the devices the master wants to hear from to report, at the end of every
step; in the rest, time moves only at the devices' own reports. In two
scenarios in three a guardian also takes the ways out: gives extra time at the
master by id (sometimes the same id twice), makes one time codes that are
typed on the devices in and out of reach (some hours later, some on the wrong
device, some again), adds time and unlocks with the PIN on the devices,
changes the PIN, locks and unlocks from the master and lifts, and answers the
children's requests; children guess codes, alone or several in a row, and
devices restart from their saved state. In half the scenarios the guardian
also removes a device now and then, right after its report or without one (a
phone lost with use it never reported), and most often pairs a new one in its
place under a new id; reports the removed device sent before turn up late.
Devices also make notes for the guardian now and then, from a generator of
their own (so the notes change none of a scenario's other draws), carry them
in every report until a grant confirms them, and drop them then.
The simulation keeps its own ledger of every slice made, of every piece of
extra time, of every note, of every second used beyond what a device held,
and of the use a removed device had not reported, within what the master
held back for it. At every step it checks:

1. for every day, the time that counts on all devices together, less the time
   used beyond what a device held while an unlock kept it open, and less the
   use removed devices had not reported (within what was held back for them),
   is at most that day's budget plus all of its extra time (given at the
   master, granted on request, added by codes and the PIN), where the budget
   is the highest for that day among the rule versions in play. With
   unchanged rules and no device removed unreported, that is just the budget
   plus the extra: zero slack. And the master never counts more extra time
   than was given: each piece once;
2. for every device and day, what the master counts plus what it holds back
   covers everything the device really used, apart from time added on it that
   the master has not heard of yet and use beyond what it held under an
   unlock;
3. right after an exchange that went through both ways, a device locked for
   lack of time means the master had nothing free for it (free time may be
   waiting for a device that asked first, and only for it); after two such
   exchanges with the same rules and allowance, the master's own figure of
   time left for that device is zero too. So a device in reach is never locked
   while the master has time for it, beyond the one exchange it takes to hear;
4. no device ever holds more than one lease of unspent lease;
5. after a hub round in which no message was lost, a device in reach and in
   use that is still locked for lack of time means that no other phone in
   reach holds away time it could give back (unless that phone is waiting
   itself). So a phone at home never keeps its share from a device that
   needs it;
6. use beyond what a device held happens only while an unlock keeps it open;
   a device under a guardian's lock (as its own standing says, not as the
   decision being checked says) records no counted use; and for every device
   and day, what the master counts plus the slices still on the way is all
   the device used, so an unlock never makes the master count less;
7. a removed device's counted use stays in the master's days exactly as the
   master had taken it; every report from it that turns up later is refused
   and leaves the book unchanged, byte for byte; removing it twice and pairing
   its id again are refused;
8. at every report the master handles, every new slice, piece of added time
   and request in its outcome is one a device made, taken once, and
   everything the report carried has been taken by then; and for every device
   and day the master's count is exactly the sum of the slices it returned;
9. at every report the master handles, every new note in its outcome is one a
   device made, taken once, and every note the report carried has been taken
   by then; and the grant's figure of the other devices' use is the master's
   own count for that day of every device but the one it is for, removed
   ones among them, saying all of it was on the master PC exactly when it
   was.

A typical run covers about 410,000 steps, 1.4 million exchanges, 165,000
lost messages, 25,000 late grants taken, 1,400 rule changes and about 7,400
times a phone at home gave time back (500 hours in all), with about half the
days ending at their limit (the extra time raises many limits). Check 5
applied about 100,000 times and checks 6 and 8 about 1.7 million times each.
On the ways out it gives extra time about 740 times (110 of them again by the
same id), uses about 1,250 codes (340 out of reach, 280 typed again, 40 on a
later day, 125 on the wrong device), adds time with the PIN about 900 times
(250 out of reach), unlocks with the PIN 600 times, changes the PIN 290
times, hits the wait after wrong guesses 2,000 times, locks from the master
290 times and unlocks 330, makes 5,300 requests (3,500 granted, 1,700
refused), uses 200 hours beyond what devices held under unlocks, and restarts
devices 2,850 times. It removes about 410 devices (330 right after a report,
80 with use they never reported, 38 hours of it in all), pairs 320 new ones in
their place, refuses 330 late reports from removed devices, and checks
310,000 new slices, 2,100 pieces of added time and 5,250 requests in report
outcomes. Devices make about 27,500 notes, and the master takes 26,700 of
them as new (the rest are still on their way when the scenario ends, or on
a device since removed); check 9 looks at the other devices' use in about
1.4 million grants, 230,000 of them with all of it on the master PC. A run
of 8,000 scenarios (10.8 million steps, 36 million
exchanges, 9,900 devices removed, 12,600 late reports from them refused)
passes in about nine minutes. The run fails if the scenarios stop reaching
the limits, losing messages, giving time back, reaching check 5, taking any
one of the ways out, removing and pairing devices both ways, refusing a late
report from a removed device, finding new slices, added time, requests and
notes in outcomes, or checking the other devices' use, all on the PC among
it, so that it can never pass without testing them. With the giving
back switched off in the library, check 5 fails on the first scenario. So
does check 2 with the master not holding back time added on a device, check 6
with the decision ignoring a guardian's lock, check 1 with spending that
never draws on time added on the device, check 1 again with the pool leaving
out a removed device's use, check 7 with a removed device's use dropped,
check 8 with a slice at its mark taken again, check 9 with a note at its mark
taken again, and check 9 again with the other devices' use leaving out a
removed device's (these two within the first 40 scenarios).

To run more scenarios, one scenario, or see every event of one:

```
LIMO_SIM_SCENARIOS=5000 cargo test --test simulation -- --nocapture
LIMO_SIM_SEED=<seed from a failure> LIMO_SIM_TRACE=1 cargo test --test simulation -- --nocapture
```

(In PowerShell: `$env:LIMO_SIM_SCENARIOS="5000"` first.) The test profile is
built with a little optimisation (`opt-level = 1`), so the whole suite takes
seconds; overflow checks stay on.

## How to add a test case

Open `tests/cases.toml`. Its first lines explain every field. A case looks
like this:

```toml
[[case]]
name = "At 20:55 with 4 minutes left and bedtime at 21:00, the lock comes at 20:59"
rules = "school_week"
time = "Wed 2026-11-18 20:55"
app = "game"
used = "86m"
lease_left = "4m"
expect = { allowed = true, left = "4m", warning = "5 minutes before time runs out", next_check = "Wed 2026-11-18 20:58" }
```

1. Pick a rule set from the top of the file, or add one under `[rules.<name>]`.
2. Copy a case that is close to the new one, give it a new name, and change
   the time, what is held and what is expected. Leave out any expectation that
   does not matter.
3. Run `cargo test --test cases`. A case that does not hold fails with its
   name and what was expected against what came. Misspelt field names are
   refused, and a weekday that does not match its date is caught.

Times are home time ("Wed 2026-11-18 20:55"), or the clock of a travelling
phone with its own zone (`device_time = "2026-11-18 10:25 -05:00"`).
Durations are written `4m`, `90s`, `1h 30m` or `4m 30s`.

The ways out have fields of their own: `added_left` (time added on the device
by a code or the PIN), `guardian_lock` (a home time, or "until lifted"),
`guardian_unlock`, `guardian_lifted`, `pin_unlock` (each with an optional
`guardian_set` or `pin_set`, the moment it was set), `wrong_guesses`,
`wait_until`, `request_waiting`, `has_pin` and `has_codes`; and expectations
`unlocked_until`, `can_ask`, `can_use_pin`, `can_use_code`, `try_wait_until`
and `idle_check`. The first lines of the file explain each one.

## Layout

```
core/
  Cargo.toml, Cargo.lock, README.md, .gitignore
  src/
    lib.rs        crate notes and the public names
    time.rs       unix seconds, the home calendar, dates, splitting at day boundaries
    rules.rs      RuleSet, budgets, blocked windows, validation, defaults
    blocked.rs    blocked hours in real time: are we in them, when do they end, when next
    decide.rs     Holding (what a device holds), decide, Decision, locks and warnings
    messages.rs   Slice, Report, Grant
    notes.rs      every kind of note for the guardian, SentNote, reading a
                  report's notes from a newer device
    device.rs     DeviceState: the device side, with codes, the PIN, requests
                  and the other devices' use
    master.rs     MasterBook: the pool, reservations, grants, marks and counted
                  use, removing devices, extra time, requests, the PIN and the
                  guardian's lock or unlock
    extra.rs      extra time with ids, time added on a device, requests and answers
    codes.rs      one time codes: making them, checking them, the code secret
    pin.rs        the PIN record, its slow hash, what a right PIN may do
    guardian.rs   lock and unlock, which one is in force, the wait after wrong guesses
  tests/
    cases.toml    the file of cases
    cases.rs      reads and checks it
    accounting.rs the shared budget as stories
    ways_out.rs   the ways out as stories, and state saved by the first piece
    callers.rs    what the hub needs as stories, and books saved before and
                  after marks
    notes.rs      notes and the other devices' use as stories, and messages,
                  books and states saved before and with them
    fixtures/     state and messages saved by the first piece (piece_one), by
                  the code before marks (piece_two), by the code with marks
                  (piece_three) and by the code with notes (piece_four)
    simulation.rs the generated days
```

Public types are plain structs and enums of strings, vectors, options, whole
numbers and booleans, with no generics, lifetimes or tuples in their
signatures, so that UniFFI can expose them later. `MasterBook` and
`DeviceState` keep their fields private and are changed through methods;
behind UniFFI they would become objects, or a thin layer of free functions
that take and return them.

## Dependencies and licences

The library depends on `serde`, and, for the codes and the PIN, on four crates
of the RustCrypto project with their default features off: `hmac`, `sha2`,
`pbkdf2` and `ctutils`. No crypto is written here by hand. The newer
RustCrypto releases are used because `sha2` 0.11 uses the ARMv8 SHA-256
instructions on phones that have them, found at run time, with no assembler;
they need Rust 1.88, so `rust-version` is 1.88.

| Crate | Version | Licence | Used for |
| --- | --- | --- | --- |
| serde | 1.0.229 | MIT or Apache-2.0 | the library: `Serialize` and `Deserialize` on every public type |
| hmac | 0.13.0 | MIT or Apache-2.0 | the library: HMAC-SHA256 for one time codes, and inside PBKDF2 |
| sha2 | 0.11.1 | MIT or Apache-2.0 | the library: SHA-256 |
| pbkdf2 | 0.13.0 | MIT or Apache-2.0 | the library: the PIN's slow hash |
| ctutils | 0.4.8 | Apache-2.0 or MIT | the library: comparing code digits and PIN hashes in constant time |
| digest, crypto-common, block-buffer, hybrid-array, cmov, cpufeatures | 0.11.3, 0.2.2, 0.12.1, 0.4.15, 0.5.4, 0.3.1 | MIT or Apache-2.0 | parts of the RustCrypto crates above |
| typenum | 1.20.1 | MIT or Apache-2.0 | used by hybrid-array |
| cfg-if | 1.0.5 | MIT or Apache-2.0 | used by sha2 |
| libc | 0.2.191 | MIT or Apache-2.0 | used by cpufeatures on Android and Linux only, to ask which instructions the processor has |
| serde_core, serde_derive | 1.0.229 | MIT or Apache-2.0 | parts of serde |
| proc-macro2, quote, syn | 1.0.107, 1.0.47, 3.0.10 | MIT or Apache-2.0 | used by serde_derive when building; nothing of them is in the library |
| unicode-ident | 1.0.27 | (MIT or Apache-2.0) and Unicode-3.0 | used by proc-macro2 when building |
| serde_json | 1.0.154 | MIT or Apache-2.0 | tests only: JSON round trips |
| itoa, zmij, memchr | 1.0.19, 1.0.23, 2.8.3 | MIT or Apache-2.0; MIT; Unlicense or MIT | tests only, used by serde_json |
| toml | 1.2.1 | MIT or Apache-2.0 | tests only: reading the file of cases |
| toml_parser, toml_datetime, toml_writer, serde_spanned | 1.1.5, 1.1.3, 1.2.0, 1.1.3 | MIT or Apache-2.0 | tests only, used by toml |
| winnow | 1.0.5 | MIT | tests only, used by toml |

`Cargo.lock` also lists indexmap, hashbrown and equivalent (MIT or
Apache-2.0), optional parts of toml that are not built. Nothing is GPL or
AGPL. The code here was written for LiMo; nothing was copied. The date
arithmetic in `time.rs` follows the well known method of counting in 400 year
cycles from 1 March. The one time code follows the shape of the HOTP and TOTP
codes of RFC 4226 and RFC 6238 (an HMAC over a counter, cut to decimal digits),
but is its own construction, written out under
[The ways out](#the-ways-out).

The crate is under the Apache License 2.0, as is all of LiMo.

## Not in this piece

These belong to the services, or to later pieces:

- Making the random bytes (code secrets, salts), keeping state files and the
  secrets in them private, and encrypting messages between devices.
- Sharing which code counters each guardian phone has used, and handing the
  code secret to guardian phones at home.
- Releasing a device once the PIN confirmed it, and tamper reports.
- Telling the guardian when a request arrives (the hub records an event from
  each report's `ReportOutcome::new_requests`; showing it is the service's).
- Reporting clock jumps.
- The UniFFI bindings for Android.
