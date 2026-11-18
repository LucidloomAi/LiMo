# limo-core

The rules core of LiMo: one Rust library that holds all of LiMo's rules and
arithmetic. The Windows service uses it directly. The Android app will use it
through Kotlin bindings that UniFFI generates (not added yet).

It answers two questions:

1. **May this child use this device now, for how long, and if not, why not
   and until when?**
2. **How does one daily budget stay honest across a PC and a phone, with no
   server?**

The library is pure. It has no clock, no files, no network, no threads, no
randomness and nothing global. Every function that needs the time is given
it. The same inputs always give the same answer, on Windows and on a phone.
It has no unsafe code.

This is the first real piece of the product. The proofs in `spikes/` are not
used by it.

## Contents

- [Time and days](#time-and-days)
- [The rules](#the-rules)
- [The decision on a device](#the-decision-on-a-device)
- [One budget across devices](#one-budget-across-devices)
- [A worked day](#a-worked-day)
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

`decide(rules, holding, now, app)`, or `DeviceState::decide(now, app)`, gives a
`Decision` that a Windows or Android service can act on without knowing any
rule:

- **allowed**, for the app in front (or for the device when no app is given);
- **lock**: the reason and when it lifts. The reasons are: this app is
  blocked (never lifts), blocked hours (until they end, joining windows that
  touch), no time today (a budget of 0), today's time is used up, "needs
  master" (the device has spent all it holds, while as far as it knows there
  is more at home), and "held by another device" (the master has nothing free,
  but a phone still holds the rest as away time; it comes when that phone
  reports from home);
- **device_lock**: the lock for ordinary apps, whatever is in front, for the
  lock screen; an always allowed app stays usable under it;
- **counts**: whether time with this app counts (not for always allowed apps);
- **seconds_left**: the best known time left today, for showing and warnings;
- **seconds_in_hand**: what this device may spend without asking anyone; the
  lock comes when it runs out;
- **warning**: when a warning point has just been reached, for time running
  out or for blocked hours about to start. Each point comes once; if two
  points pass at once only the nearer one is given. If more time arrives, the
  points come again;
- **ask_master**: the lease is down to half; ask the master if it can be
  reached;
- **next_check_at**: when to decide again while the app in front stays in use
  (the next warning point, the lock, blocked hours starting or ending, the
  lease running low, the day changing), and **next_check_if_idle_at**, the
  same for an idle device, with only the moments that come with the clock.
  Nobody has to poll every second.

An always allowed app is allowed in every case and does not count. A blocked
app is refused in every case.

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

**Reservations.** The master **reserves**, for every device, everything that
device might still spend without telling it: the lease it may hold and its
away allowance. Reserved time is handed to no one else. A reservation shrinks
only when the device reports what it really used, or its day is over. A lease
that was sent but never confirmed stays reserved, because the device may have
received and spent it.

**Slices.** Use is reported as **slices**: child, device, app, day, start,
end, whether it counts, and an id unique for the child. A slice handed over
twice is counted once. Use across a day boundary is cut into one slice per
day. A device hands over every slice the master has not confirmed with every
report, so a lost report loses nothing.

**Messages.** There are two, both plain data:

- a **report** (device to master): its day, what it holds and has used, the
  newest grant and rules it has taken (that is the acknowledgement), its
  unconfirmed slices, and whether it wants more time;
- a **grant** (master to device): the lease it may have for the day, in all;
  its away allowance for the day; its away share for later days; the current
  rules; which slices are now counted; what the master would say is left for
  it; how much away time other phones still hold; and whether to acknowledge
  at once (when its away allowance was just lowered).

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
two agree without talking.

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
  lowered allowance, and waiting devices once time is free for them.

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

## What it guarantees

**As long as a day's budget is not cut while some device still holds the
higher figure, the time that counts on all of a child's devices together
never passes that day's budget plus extra time. Not by one lease, not by one
second.** The design needs no slack at all. This holds through any mix of use
on several devices at once, going out of reach and coming back, lost,
repeated and late messages, day changes, and every other kind of rule change
(a higher budget, blocked hours, apps, the away share, the lease length).

Why, in five steps:

1. A device spends only what it holds: lease left plus away allowance left.
   The decision gives the exact moment that runs out, and the device locks
   then.
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
   allowances together are never more than the budget.
5. Every slice counts once, and a report's figures count only if they are
   the newest.

Taking time back from a phone at home changes none of this. A lower away
allowance only lowers what the phone may hold. The master goes on reserving the
old figure until the phone acknowledges the lower one, and a phone that never
hears never acknowledges.

So what is counted plus what is reserved never passes the budget plus extra
time, and everything really used is always inside what is counted plus what is
reserved. The simulation checks both of these at every step of hundreds of
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
   sends a grant, a device after it takes one.
7. **Clocks.** A clock set forward starts a new day early, with a fresh away
   allowance. Both platforms lock the clock down (see `docs/PLAN.md`), and
   reporting clock jumps is a later piece.
8. **Honest devices.** The master believes what a device reports. LiMo's
   enforcement runs on the devices anyway. Absurd figures from a faulty
   device (a used time of billions of seconds, a slice from the year 1 to the
   year 9999) can neither crash the master nor widen the pool: all the
   arithmetic saturates, and such figures only make the master hold back
   more.

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
`Deserialize`, and save it after every change.

**The master** (the hub, on the master PC), one `MasterBook` per child:

```rust
let mut book = MasterBook::new("asha", rules)?;           // once
let welcome = book.add_device("phone-1", DeviceKind::Portable, now)?;
// ...send `welcome` to the phone with its pairing; it makes its state from it.

// For every report that arrives:
let grant = book.handle_report(&report, now)?;
save(&book);                                              // before sending
send(grant);
// Then ask the devices the master wants to hear from to report at once:
// phones that can give time back to a waiting device, phones that must
// acknowledge a lower allowance, and waiting devices once time is free.
for id in book.wanted_reports(now) {
    prod(id);                  // if it can be reached; the PC's agent is local
}

// When the guardian changes the rules:
book.set_rules(new_rules)?;    // they reach each device with its next grant;
                               // push a grant to devices in reach at once

// For the guardian's screens:
book.pool(day);                // used, held back, free
book.device_view("phone-1", day);   // used, held back, waiting for, giving back
book.usage(day);               // every slice, in time order
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
        match send(state.report(now, want_more)) {
            Ok(grant) => {
                state.take_grant(&grant, now);
                if grant.acknowledge_now {           // its away allowance was lowered
                    if let Ok(g) = send(state.report(now, want_more)) { state.take_grant(&g, now); }
                }
                d = state.decide(now, Some(&app));
            }
            Err(_) => state.lost_contact(),
        }
    }
    act_on(&d);              // lock or unlock, show the warning, show the time left
    save(&state);
    sleep_until(if in_use { d.next_check_at } else { d.next_check_if_idle_at },
                or the app in front changing, or input after idleness);
}
```

Rules for callers:

- **Report regularly while in reach**, not only when `ask_master` is set:
  usage then reaches the master, and on a day with no limit a device has no
  reason to ask for time, so it would never hear of new rules. The hub should
  also push a grant to every device in reach when the rules change.
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
- **Decide again when the app in front changes.**

## Tests

`cargo test` runs four groups, all deterministic:

| File | What | Count |
| --- | --- | --- |
| `src/*.rs` | Unit tests: day and weekday from unix seconds and offset, negative offsets, a day start not at midnight, far past and far future dates, splitting at day boundaries, validation, blocked windows, the master's books, absurd figures and far times | 31 |
| `tests/cases.toml` with `tests/cases.rs` | The file of cases a product owner can read and extend (55 cases, 4 rules checks), a check that a wrong expectation fails with the case's name, and the file's reader | 3 tests |
| `tests/accounting.rs` | The shared budget as short stories: the worked day above, a phone at home giving its share to the PC, a phone that then leaves, a phone away that gives nothing, a lowering grant that is lost, two phones at home, a lowered phone that left, a need that lapses, and JSON round trips of every message and state | 22 |
| `tests/simulation.rs` | The simulation below | 1 |

**The simulation** plays the master, the PC and one or two phones through 300
generated scenarios of two or three days each, from a seeded generator of its
own. Each scenario mixes use on every device (often at the same time), phones
going out of reach without a word, reports and grants lost (up to half of
them), messages repeated and arriving late, rule changes (budgets, blocked
hours, away share, lease length, and in some scenarios the home zone and the
day start), extra time and day changes. In three scenarios in four, a hub
asks the devices the master wants to hear from to report, at the end of every
step; in the rest, time moves only at the devices' own reports. At every step
it checks:

1. for every day, the time that counts on all devices together is at most
   that day's budget plus extra time, where the budget is the highest for
   that day among the rule versions in play. With unchanged rules that is
   just the budget: zero slack;
2. for every device and day, what the master counts plus what it holds back
   covers everything the device really used;
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
   needs it.

A typical run covers about 400,000 steps, 1.85 million exchanges, 210,000
lost messages, 30,000 late grants taken, 1,400 rule changes and about 6,000
times a phone at home gave time back (430 hours in all), with about three days
in four ending at their limit. Check 5 applied about 140,000 times. The run
fails if the scenarios stop reaching the limits, losing messages, giving time
back or reaching check 5, so that it can never pass without testing anything.
With the giving back switched off in the library, check 5 fails on the first
scenario.

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
    device.rs     DeviceState: the device side
    master.rs     MasterBook: the pool, reservations, grants, the usage record
  tests/
    cases.toml    the file of cases
    cases.rs      reads and checks it
    accounting.rs the shared budget as stories
    simulation.rs the generated days
```

Public types are plain structs and enums of strings, vectors, options, whole
numbers and booleans, with no generics, lifetimes or tuples in their
signatures, so that UniFFI can expose them later. `MasterBook` and
`DeviceState` keep their fields private and are changed through methods;
behind UniFFI they would become objects, or a thin layer of free functions
that take and return them.

## Dependencies and licences

The library itself depends on `serde` only.

| Crate | Version | Licence | Used for |
| --- | --- | --- | --- |
| serde | 1.0.229 | MIT or Apache-2.0 | the library: `Serialize` and `Deserialize` on every public type |
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
cycles from 1 March.

The crate is under the Apache License 2.0, as is all of LiMo.

## Not in this piece

The ways out (extra time on request, one time codes, the guardian PIN) are the
next piece. What it can use:

- `MasterBook::add_extra_secs(day, secs)` adds to a day's pool; leases then
  hand it out like any other time. A request answered at home needs nothing
  more.
- A one time code or a PIN typed on a device away from home adds time the
  master does not know of yet. The natural place is a third figure in
  `Holding` beside the lease and the away allowance (time added on the device,
  reported with the next report), and the same figure added to the pool's
  extra time when the master hears of it.
- A zero day with time in hand is already allowed, so extra time on a day
  with no screen time works without change.
- Guardian release and tamper reports belong to the services, not here.
