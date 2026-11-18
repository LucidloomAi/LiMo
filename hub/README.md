# limo-hub

The hub store of LiMo: one Rust library that keeps one family's data on the
master PC, in one SQLite file. The Windows service holds one `Hub` and calls
it; the hub calls the rules core ([`../core`](../core/)) for every rule, pool
and grant.

**What it is.** The family's guardian password, its children, each child's
rules and book (the core's `MasterBook`), the paired devices, the usage, the
guardian PIN, the record of one time codes made, the Windows accounts on the
master PC, the state of each child's device on the master PC, the PC's own
settings, and a short record of events. It answers the devices' reports with
grants and gives the guardian's screens what they show.

**What it is not.** It has no network, no Windows code, no user interface, no
threads and no clock. The caller passes the time (unix seconds, by the master
PC's clock) into every call that needs it, exactly as the core does. It does
not carry or encrypt messages, find devices, or keep sign in sessions: it
checks the password, and the guardian's screen decides who may act. It has no
unsafe code.

## Contents

- [The file and its tables](#the-file-and-its-tables)
- [How the service calls it](#how-the-service-calls-it)
- [The promises](#the-promises)
- [Usage: in detail, then in daily totals](#usage-in-detail-then-in-daily-totals)
- [Devices](#devices)
- [The master PC's own devices](#the-master-pcs-own-devices)
- [The guardian password](#the-guardian-password)
- [The guardian PIN](#the-guardian-pin)
- [Events](#events)
- [What it costs](#what-it-costs)
- [Tests](#tests)
- [Dependencies and licences](#dependencies-and-licences)
- [Not in this piece](#not-in-this-piece)

## The file and its tables

One SQLite file. Its version is SQLite's `user_version` (now 3), and its
`application_id` marks it as LiMo's ("LiMo" in ASCII). When a file is
opened, every step it needs to reach this code's version runs in one
transaction; a file from a newer version of LiMo is refused, unchanged, with
a plain message. Each step is kept in `src/schema.rs` and never changed once
released; a new version adds a step. A step is SQL, and may also have work on
the books that SQL cannot do.

- **Version 1**: the first tables.
- **Version 2**: the rules core now answers what the hub kept of each
  device's newest report (its message number, the rules version it holds,
  its wrong guesses, how far off its clock is), so those four columns of
  `device` go. Every child's book is loaded and saved in the core's present
  shape: a book that kept whole slices keeps marks instead, and a device the
  first version removed (kept in the book as a retired record, see
  [Devices](#devices)) is removed by the core itself. The test
  `a_file_made_by_the_first_hub_opens_converts_and_goes_on` opens a file made
  by the first version (`tests/fixtures/store_v1.limo`) and checks that every
  view shows what the first version showed, and that exchanges, pairing,
  removing and tidying go on.
- **Version 3**: what the agent on the master PC needs (see
  [The master PC's own devices](#the-master-pcs-own-devices)): a revision on
  each book, two settings for the PC, and the `master_pc` table. It only adds;
  nothing a file holds changes. The step to version 2 writes the books in the
  shape a version 2 file has, so a file from the first version still goes
  through both steps. The test
  `a_file_made_by_the_hub_before_the_agent_opens_and_goes_on` opens a file
  made by the hub before this change (`tests/fixtures/store_v2.limo`), checks
  that every view shows what that hub showed, and goes on.

The tables, at version 3:

- `settings`: how many days of usage are kept in detail (35 unless changed);
  after how many idle minutes the count pauses on the master PC (3 unless
  changed); and what happens in a Windows account that is not linked (free
  unless changed).
- `family`: one row. The password's salt, rounds and hash; the wrong guesses
  and the end of the wait they set; the PIN record, so that children added
  later get it.
- `child`: id, its place in the order children were added, name, age band,
  when added, and the day before which usage was folded into totals.
- `book`: each child's `MasterBook`, as the core's own JSON: a few kilobytes
  (see [Usage](#usage-in-detail-then-in-daily-totals)); and its revision,
  which grows by one every time the book is written.
- `device`: id, its place in the order devices were paired, child, name,
  kind, platform label, when paired and removed, when last heard from, and
  whether its newest report showed it in use. The rest of what the master
  knows of a device (the rules version it holds, whether it has the latest
  news, its wrong guesses, how far off its clock is) the child's book
  answers (`MasterBook::device_view`). When it was last heard from is a copy
  of the book's figure, written with every exchange, so that devices can be
  listed without reading the books; whether it is in use is the hub's own.
- `slice`: usage in detail: child, device, app, home day, start, end, seconds
  and whether it counted.
- `day_total`: usage folded by day, device and app: seconds, and seconds that
  counted.
- `code_made`: for each device and day, the counters of the one time codes
  the hub made.
- `account`: Windows accounts: identifier, display name, and the child, or
  none for "not limited".
- `master_pc`: for each child's device on the master PC, the device's state
  (the core's `DeviceState`, as JSON) and the agent's own notes, as the agent
  hands them in.
- `event`: what happened, typed (see [Events](#events)).

Every row about a child points at the child's row and goes with it when the
child is removed. Ids are made by the hub: `child-` or `device-` and 16 hex
digits from the operating system's random numbers.

**Age bands.** A child's age is one of four bands: under 6, 6 to 9, 10 to 12,
and 13 to 17 (`AgeBand`). The hub only keeps it; a later piece may suggest
starting rules from it.

## How the service calls it

```rust
let mut hub = Hub::open(&path)?;                 // at start; refused if another hub has it
if !hub.has_family()? {
    hub.create_family(&password, now)?;          // first setup
}
let asha = hub.add_child("Asha", AgeBand::TenToTwelve, utc_offset_secs, now)?;
let pairing = hub.pair_device(&asha.child_id, "Asha's phone", DeviceKind::Portable, "android", now)?;
// ...hand `pairing.welcome` and `pairing.code_secret` to the device over the
// encrypted pairing link; the device makes its DeviceState from them.

// For every report that arrives:
let grant = hub.handle_report(&report, now)?;    // on the disk before it returns
send(grant);
for device_id in hub.wanted_reports(now)? {      // then prod who should report now
    prod(device_id);                             // if it can be reached; the PC's agent is local
}

// The guardian, after signing in with hub.check_password(...):
hub.replace_rules(&child_id, rules, now)?;       // the core sets the version
hub.give_extra(&child_id, 30, &tap_id, now)?;    // the same tap id gives once
hub.answer_request(&child_id, &request_id, RequestReply::Grant { minutes: 20 }, now)?;
hub.lock_now(&child_id, None, now)?;             // or unlock_until, or lift
hub.set_pin(&pin, now)?;                         // 6 to 8 digits, every child at once
let code = hub.make_code(&device_id, 30, now)?;  // read out code.shown
// ...after each of these, prod hub.wanted_reports(now) as above.

// What the guardian sees:
hub.family_today(now)?;                          // each child: used, left, budget, extra, devices
hub.usage(&child_id, from_day, to_day)?;         // app by app, per device and in total
hub.extra_time_today(&child_id, now)?;           // where today's extra came from
hub.waiting_requests(now)?;                      // across all children
hub.events(50, None)?;                           // newest first

hub.tidy(now)?;                                  // once an hour
hub.close()?;                                    // at stop
```

The agent on the master PC (`../agent`) calls `pair_master_pc`, `master_pc`,
`save_master_pc`, `exchange_on_master_pc`, `book_revision`, `pc_settings`
and the `note_*` calls itself; see
[The master PC's own devices](#the-master-pcs-own-devices). The guardian's
console sets the PC's settings with `set_idle_minutes` and
`set_unlinked_accounts`.

Rules for callers:

- **One hub per file, one thread at a time.** `Hub` can be moved between
  threads but not shared; keep it in one thread, or behind a mutex.
- **Prod the devices in `wanted_reports` after every exchange and every
  guardian action.** It names the devices each child's book wants to hear
  from: time to give back or take, and every device that has not yet
  reported holding its child's current rules or the guardian's newest news,
  so a rule change reaches devices in reach at once. The master learns that a
  device took news from the device's next report, so a device is named until
  it has reported twice.
- **The hub keeps no sessions.** The guardian's screen checks the password
  when the guardian signs in, and must ask for it again before
  `delete_family` and `remove_child`.
- **The home zone.** A child's rules carry the zone as a fixed offset from
  UTC, given when the child is added. If the PC's zone changes, or daylight
  saving moves it, the service replaces each child's rules with the new
  offset.
- **Moving a device to another child** is `remove_device` and then
  `pair_device` for the other child: the device gets a new id and a new code
  secret, and must be set up again with the new pairing.
- **Passing extra time with a tap id.** `give_extra` takes an id the screen
  made for that tap, so that a tap sent twice (retried after a lost answer)
  gives once. An empty tap id lets the hub make a new id every time.

## The promises

**Saved before sent.** `handle_report` loads the child's book from the file,
lets the core handle the report, then writes the new book, the slices of use
and what the hub keeps about the device, and commits, all in one SQLite
transaction. The grant is returned only after the commit has returned, and
the file runs with `synchronous = FULL`, so a commit returns only when it is
on the disk. The caller can only send what it was given, so the book that
made a grant is on the disk before the grant can leave. If anything fails,
the transaction is rolled back, the call returns an error and no grant, and
the device, which got no answer, keeps its slices and asks again. The hub
keeps nothing in memory between calls: every call reads what it needs from
the file. The test `every_grant_is_on_the_disk_before_it_is_returned` closes
and reopens the hub after every exchange of a morning and checks the views
and the grant numbers carry on; `an_exchange_that_fails_part_way_leaves_nothing_behind`
makes the step after the book is written fail and checks that nothing
changed.

**All or nothing.** Every call that changes something does it in one
transaction, and a crash or a power cut in the middle rolls back to the file
as it was before the call. Setting or removing the PIN changes every child's
book in one transaction, so no child is left with the old PIN at the master.

**Deleted means deleted.** `remove_child` deletes the child's rules and book,
devices (with the state of its device on the master PC), usage in detail and
in totals, requests, codes made, linked Windows accounts and events.
`delete_family` deletes all of that for every child, the password and the
PIN, the accounts and the events, and resets the settings, leaving an empty,
valid store. The file runs with SQLite's
`secure_delete`, so deleted rows are overwritten with zeros where they lay,
and after these two calls the file is rebuilt (`VACUUM`) so that no copy of
a deleted key is left in an index's inner pages. The rebuild is done on a
best effort basis: if it cannot run (a full disk), the deletion still stands
and the call still succeeds. The tests search the file's bytes for the
deleted names, apps and reasons, with tables that span many pages.

Removing a **device** is not deleting: the device is unpaired, its code
secret is forgotten and its reports are refused, but the time it was used
stays in the child's history, under its name, marked as removed.

**One hub per file.** The file is opened in SQLite's exclusive locking mode
and the hub writes to it at once, so it holds the file locked until it is
closed or dropped. A second hub on the same file, in this process or another,
is refused at once with `HubError::InUse` ("The family's data is open in
another LiMo program"); so is any other program that tries to read it. The
lock goes with the process, so a crashed service leaves no lock behind.

**Safe to copy while the service is stopped.** The file uses a rollback
journal that SQLite empties after every transaction, so the file alone holds
everything committed. After a clean stop, an empty `-journal` file may sit
beside it; it need not be copied. After a crash or a power cut, start the
service once (which repairs the file from its journal) before copying, or copy
the journal file too.

**Secrets stay out of sight, not out of the file.** The password, the PIN and
the code secrets never appear in `Debug` output or in error messages:
`Pairing` and `Hub` print no secret, the core's `OneTimeCode` hides its
digits, and error messages from saved data quote no part of it. They are in
the file, in the forms the core defines: the password as a salted slow hash,
the PIN as the core's salted slow hash, and each device's 32 byte code
secret as it is in the core's book and, for each child's device on the
master PC, in that device's state, which also holds the PIN record. **The
file's protection comes from the
Windows permissions the installer sets on its folder, so that a child's
account cannot read it, not from this crate.** It is not encrypted.

## Usage: in detail, then in daily totals

A report hands over slices of use, every one the master has not confirmed,
so the same slice comes many times. The hub keeps exactly the slices the core
took for the first time (`ReportOutcome::new_slices`): the core counts each
slice once, and says which ones it took, so the hub keeps each once, in the
same transaction, and what the hub shows for a day is what the core counted.
The hub has no rule of its own for which slices to keep. Each slice is
stored at most a day long: a slice never spans more than its day, and a
faulty one must not swamp the totals (the core's own count saturates). A
slice the core takes is never for a day already folded into daily totals:
`tidy` folds days before the detail window (7 days or more) and, in the same
transaction, has the core forget the days before yesterday, whose slices the
core then no longer takes.

`tidy` folds the days before the detail window (35 days unless changed,
7 to 400) into daily totals per day, device and app, and removes those
slices. The usage views read detail and totals alike, so folding changes
nothing they show; the test
`folding_keeps_daily_totals_equal_to_what_the_slices_summed_to` checks every
day of forty. A slice that arrives later for a folded day is not kept: it
may be one already folded.

**The core's books keep only today and yesterday** (`BOOK_DAYS`). A book is
written on every exchange. It does not hold the slices: for each device and
day it holds the highest slice number taken and the counted seconds (see the
core's README, "What a book holds"). So a book is small: for two devices, 800
slices a day each and two days, about 2.2 KB of JSON, where it was 745 KB
when books held every slice. `tidy` calls `forget_days_before`, and writes
the book only when that says it changed. Nothing the core needs is lost: a
grant is only made for a device whose day is within one of the master's, and
a request can only be answered while its day lasts. The hub's own tables
hold the usage that is shown.

LiMo records which app was used and for how long, and nothing else.

## Devices

**Kinds.** A device is the master PC itself (`DeviceKind::OnMaster`: no away
allowance; a child has it once) or a device that can be carried away
(`DeviceKind::Portable`). Its platform label is free text from the service,
such as "windows" or "android".

**In use.** A device is in use when the master heard from it within 10
minutes (`IN_USE_HEARD_SECS`), and that newest report carried use that ended
at most 2 minutes before it was sent, by the device's own clock
(`IN_USE_FRESH_SECS`). Comparing the use with the report's own sending time
keeps a device with a wrong clock right. A device in use reports every few
minutes, so it stays in use; an idle report, or ten minutes of silence, ends
it.

**Clocks far off.** The core keeps, from each device's newest report, how far
its sending time was from the master's clock. When that is more than 5
minutes (`CLOCK_FAR_OFF_SECS`), the device view shows by how much, and an
event is recorded once, until a report finds the clock right again. (For a
file carried from version 1, a clock already far off is noted once more at
its first report after the upgrade.)

**Removed devices.** `remove_device` calls the core's
`MasterBook::remove_device`, in the same transaction as the hub's own rows:

- its counted use stays in the child's day, and in the history under its
  name, marked as removed;
- the master holds nothing back for it any more, so the child's other
  devices get that time, and the other phones its away share, at once;
- time added on it by a code or the PIN that it had not used, as its newest
  report said, is dropped from the day's extra time (the part it used stays
  counted); `extra_time_today` shows it as `dropped_secs`;
- its record leaves the book at once, its code secret with it; its reports
  are refused, first by the hub (`HubError::DeviceRemoved`) and then by the
  core;
- for a child's device on the master PC, the state kept for it goes too,
  with the code secret in it; the agent pairs a new one the next time the
  child is on the PC.

Use it made and never reported is not counted. So the service should, when
it can, have the device report once more before removing it (its "release"
step). The first version of the hub, when the core had no such call, kept a
removed device in the book as a retired record and held back all the time
added on it, even the part it used; files from that version are converted
when opened (see [The file and its tables](#the-file-and-its-tables)). On the
days such a book still keeps (today and yesterday at most) the time added on
the retired device stays in the day's extra, since the retired record no
longer says how much of it was used.

## The master PC's own devices

On the master PC each child has one device of kind "on the master": the PC
itself, as that child uses it. The agent on the master PC keeps its state
(the core's `DeviceState`), and the hub keeps that state in the same file as
the books, written in the same transactions, so that the book that made a
grant and the device that took it can never disagree after a power cut.

- **`pair_master_pc(child, name, now)`** returns the child's device on the
  master PC with its kept state, pairing it first when the child has none:
  the device is added to the child's book, its state is made from the
  welcome grant with its code secret, and the state is kept, all in one
  transaction. Called again, it returns the same device and changes nothing,
  so the device is paired once and never twice. A device of that kind paired
  before the hub kept states (with `pair_device`) has no state here; it is
  removed, its use staying counted, and a new one paired in its place, in the
  same transaction.
- **`master_pc(child)`** reads it, and pairs nothing.
- **`exchange_on_master_pc(state, want_more, notes, now)`**: one exchange
  between the device and the child's book, in process, with the same report
  and grant a phone sends. The device writes its report, the book takes it,
  the device takes the grant, and the book, the slices of use, the device's
  new state and the agent's notes are written in one transaction, committed
  to the disk before it returns. The state handed in changes only when all of
  it was written. A grant that asks to be acknowledged at once is
  acknowledged in the same transaction (a device on the master PC holds no
  away allowance, so it is never asked).
- **`save_master_pc(state, notes, now)`** keeps the state and the notes
  alone. Both calls refuse a device that is not a child's device on the
  master PC with a kept state (`HubError::NotMasterPc`), or that was removed
  (`HubError::DeviceRemoved`); the agent then pairs a new one.
- **`book_revision(child)`** grows by one every time the child's book is
  written: by an exchange with any device, a guardian's action, or `tidy`.
  The agent compares it with the revision its own last exchange left, to see
  with one small read that a guardian acted or another device reported.
- **`pc_settings()`**, **`set_idle_minutes(1 to 60)`** and
  **`set_unlinked_accounts(Free or Locked)`**: after how many minutes with no
  input the count pauses on the master PC (3 unless changed), and what
  happens in a Windows account linked to no child and not marked as not
  limited (free unless changed). Locked can be set only once a family is set
  up, and `delete_family` sets both back, so a PC being set up is never
  locked.
- **`note_unlinked_account_used`**, **`note_blocked_program_closed`**,
  **`note_clock_jump`** and **`note_time_zone_change`** record the events only
  the agent can see (see [Events](#events)). The first two record each event
  once: the same event, to the letter, already on record is not recorded
  again, so an account is noted once a day and a program once a day.

The agent's notes are kept as given; the hub does not read them.

## The guardian password

8 to 256 characters of any kind. It is kept only as PBKDF2-HMAC-SHA256 with
600,000 rounds (the 2023 OWASP figure for this hash), a 16 byte salt and a 32
byte hash, compared in constant time; about 50 ms on the development PC. The
rounds are stored with the record: a record made with fewer rounds than
today's is rehashed at the next right password. Changing the password needs
the old one; a new password that cannot be taken is refused before the old
one is looked at.

Wrong passwords slow down in the same steps as wrong PINs and codes on a
device (the core's `wait_after_wrong_guesses`): the first four are free, the
fifth sets a wait of a minute, and each further one doubles it, up to an
hour. During a wait nothing is checked and nothing is counted. The count and
the end of the wait are in the file, so a restart changes nothing; the wait
is kept as the moment it ends, so a clock set back makes it longer, never
shorter. A right password clears the count.

## The guardian PIN

6 to 8 digits (`MIN_PIN_DIGITS`, `MAX_PIN_DIGITS`), by the owner's decision.
The hub refuses a PIN of fewer than 6 digits, or with anything but digits in
it, itself, with `HubError::PinWrongShape` ("A PIN is 6 to 8 digits, with
nothing else in it."), before the core sees it; the core refuses them too
(`PinError::NotSixToEightDigits`), and anything else the core refuses comes
back as `HubError::Pin`, with the core's reason as it is. The PIN is hashed
once by the core (`PinRecord::new`) and the same record is given to every
child's book in one transaction, and kept for children added later. A PIN of
4 or 5 digits set before (the core once took them) goes on working on the
devices until the guardian sets a new one: a record does not say how long its
PIN is.

## Events

Typed, small and kept for 90 days, at most 2,000 (`EVENT_KEEP_DAYS`,
`MAX_EVENTS`): a device paired or removed, rules changed, the PIN set or
removed, the password changed, a lock, an unlock or a lift from the master,
more wrong guesses reported by a device than before, a device's clock found
far off, and a child's request for more time reaching the master (from the
requests the core took for the first time, so each request is noted once).
From the agent on the master PC: a Windows account that is not linked used
on the PC (once for each account and the PC's day), a blocked program closed
(once for each program and the child's day), the PC's clock moved against
its steady count, and the PC's time zone changed.
Each has the time by the master's clock and the child it is
about (none for the PIN and the password). `events(limit, before_id)` lists
the newest first, a page at a time.

## What it costs

Measured on the development PC (AMD Ryzen 9 7950X3D, NVMe disk), release
build, with a heavy day: two devices, 800 slices a day each, a report every
two minutes, and two days in the book (2.6 KB of JSON; 730 KB when books
kept every slice, when `handle_report` took about 10 ms, `family_today` 3 ms
and `wanted_reports` 2.5 ms):

| Call | Time |
| --- | --- |
| `handle_report`, written to the disk | about 6 ms |
| `family_today` | under 0.1 ms |
| `wanted_reports` | under 0.1 ms |
| `tidy`, folding 3,200 slices | about 20 ms |
| `check_password` | about 50 ms |

Most of an exchange is now waiting for the disk to confirm the write. **Call
`tidy` once an hour.** It does nothing that is not due, so calling it more
often is harmless; calling it less often only lets the detail grow a little
meanwhile.

## Tests

`cargo test` runs two groups, all with real files in temporary folders:

| Where | What | Count |
| --- | --- | --- |
| `src/*.rs` | Unit tests: a version 1 file carried to version 2, to version 3 and on to a test version 4 through the migration path (and then refused by version 3 code); version 3's settings refusing a value they do not know; a fresh file's tables and settings; password known answers (worked out separately with Python's `hashlib`); names of any script, and hidden characters refused; ids and seeded randomness; the same note kept once | 11 |
| `tests/hub/` | The stories: a whole day of a family with the views checked at every stage; a file made by the first version of the hub opened, converted and going on, with every view as that version showed it; a file made by the hub before the agent (version 2) opened and going on, its PC device paired the old way replaced once; files made, reopened, copied, refused when newer or foreign, damaged in a row or in the middle, and two hubs on one file; every grant on the disk before it is returned; an exchange and a change that fail part way leaving nothing; reports that do not fit and hostile reports; wanted reports after new rules; in use; clocks far off; removing a child and deleting the family, down to the file's bytes; removed devices, their time freed and the code time they did not use dropped; a removed device leaving the book at once and its use with its days; wrong passwords and the wait across a restart; secrets out of `Debug`, errors and the file; folding forty days, and a late slice for a folded day neither counted nor kept; trimming books, codes and events; hostile names, unknown ids and numbers out of range. For the master PC: its device paired once and its state kept; an exchange writing the book and the device state together, and one that fails part way leaving both as they were; the book revision growing with every change to the book; a PC device paired the old way replaced once; only a kept PC device saved, and a removed one taking its state with it; the PC's settings kept, checked and reset with the family; notes kept once where once is asked; the kept state hiding its secrets | 44 |

`cargo build`, `cargo test`, `cargo doc --no-deps` and
`cargo clippy --all-targets -- -D warnings` pass with no warnings and no
allow attributes. Dependencies are built optimised even in debug and test
builds, and this crate with a little optimisation, so that the slow hashes
take as long as they will in the product; overflow checks stay on.

## Dependencies and licences

| Crate | Version | Licence | Used for |
| --- | --- | --- | --- |
| limo-core | 0.1.0 (path) | Apache-2.0 | every rule, pool and grant |
| rusqlite | 0.43.0 | MIT | the SQLite file |
| libsqlite3-sys | 0.40.0 | MIT; the SQLite source it builds in is in the public domain | SQLite itself, built into the program (`bundled`) |
| serde, serde_core, serde_derive | 1.0.229 | MIT or Apache-2.0 | the public types, and the books as JSON |
| serde_json | 1.0.154 | MIT or Apache-2.0 | the books, the PIN record and events as JSON |
| getrandom | 0.4.3 | MIT or Apache-2.0 | ids, salts and code secrets from the operating system |
| sha2 | 0.11.1 | MIT or Apache-2.0 | SHA-256 for the password hash |
| pbkdf2 | 0.13.0 | MIT or Apache-2.0 | the password's slow hash |
| ctutils | 0.4.8 | Apache-2.0 or MIT | comparing password hashes in constant time |
| hmac, digest, crypto-common, block-buffer, hybrid-array, cmov, cpufeatures, typenum, cfg-if | 0.13.0, 0.11.3, 0.2.2, 0.12.1, 0.4.15, 0.5.4, 0.3.1, 1.20.1, 1.0.5 | MIT or Apache-2.0 | parts of the RustCrypto crates above |
| bitflags, fallible-iterator, fallible-streaming-iterator, smallvec | 2.13.2, 0.3.0, 0.1.9, 1.16.6 | MIT or Apache-2.0 | parts of rusqlite |
| itoa, memchr, zmij | 1.0.19, 2.8.3, 1.0.23 | MIT or Apache-2.0; Unlicense or MIT; MIT | parts of serde_json |
| proc-macro2, quote, syn, unicode-ident | 1.0.107, 1.0.47, 3.0.10, 1.0.27 | MIT or Apache-2.0 (unicode-ident also Unicode-3.0) | used by serde_derive when building; nothing of them is in the library |
| cc, find-msvc-tools, shlex, pkg-config, vcpkg | 1.11.2, 0.1.15, 2.0.1, 0.3.34, 0.2.15 | MIT or Apache-2.0 | used by libsqlite3-sys when building, to compile SQLite |
| libc | 0.2.191 | MIT or Apache-2.0 | used by cpufeatures and getrandom on Android and Linux only |
| r-efi | 6.0.0 | MIT or Apache-2.0 or LGPL-2.1-or-later (taken as MIT or Apache-2.0) | used by getrandom on UEFI only; never built for Windows or Android |
| tempfile, fastrand, once_cell, windows-sys, windows-link, rustix, linux-raw-sys, errno | 3.27.0, 2.5.0, 1.21.4, 0.61.2, 0.2.1, 1.1.6, 0.12.1, 0.3.15 | MIT or Apache-2.0 (rustix and linux-raw-sys also Apache-2.0 with LLVM exception) | tests only: temporary folders |

Nothing is GPL or AGPL. The code here was written for LiMo; nothing was
copied. The crate is under the Apache License 2.0, as is all of LiMo.

## Not in this piece

- The network, pairing by QR, finding devices, and encrypting messages.
- Handing a device's code secret to a guardian phone at home, and taking
  back the code counters a guardian phone used away from home.
- Telling the guardian when a request arrives or an event is recorded (the
  service can watch `waiting_requests` and `events`).
- Changing the home zone of every child at once, and daylight saving.
- Backup and restore.
- A Cargo workspace for the repository.
