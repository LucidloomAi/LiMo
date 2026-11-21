# limo-phone-ffi

The bridge between the Kotlin of LiMo's Android app (the child form) and
the Rust that decides: the phone's engine ([`../engine`](../engine/),
`limo-phone`) and the phone's side of the link to the master PC
([`../../link`](../../link/), `limo-link`). UniFFI generates the Kotlin
that calls it; the app's build makes the library and the bindings and puts
them in the child form only.

**What it is.** A thin layer. Each call converts what came from Kotlin,
calls the engine's or the link's own function of the same name, and
converts what comes back. The rules, the counting, every word the phone
shows, the kept state and when to keep it, the frames, the handshakes and
the crypto are all the engine's and the link's; their READMEs say what each
call does.

**What it is not.** It decides nothing, holds no state of its own beyond
the values it wraps, has no clock, no files and no sockets, and never calls
into Kotlin. It is not the Android host (the foreground service that calls
the engine once a second, the overlays, the ways out activity, the socket
that carries the link): those are the app's later pieces. It does not
cross the master's side of the link, the hub, discovery or anything of the
guardian form.

## Contents

- [How Kotlin calls it](#how-kotlin-calls-it)
- [What crosses, value by value](#what-crosses-value-by-value)
- [Shapes, and why](#shapes-and-why)
- [What does not cross, and why](#what-does-not-cross-and-why)
- [Errors](#errors)
- [Threads, lifetimes and a panic](#threads-lifetimes-and-a-panic)
- [Secrets](#secrets)
- [Unsafe code](#unsafe-code)
- [The testkit](#the-testkit)
- [Building](#building)
- [Tests](#tests)
- [What only a phone can prove](#what-only-a-phone-can-prove)
- [What the engine and the link should gain](#what-the-engine-and-the-link-should-gain)
- [Dependencies and licences](#dependencies-and-licences)
- [Layout](#layout)

## How Kotlin calls it

The generated Kotlin is in package `ai.lucidloom.limo.child.ffi`. In the
host (a later piece), roughly:

```kotlin
// Pairing, once: the offer came over the USB cable.
val offer = Offer.fromText(offerBytes); offerBytes.fill(0)
val pairing = PhonePairing.start(offer, PairRequest("Asha's phone", "android", Build.VERSION.SDK_INT), elapsed())
socket.write(pairing.takeOutgoing())
while (!pairing.receive(socket.read(), elapsed())) socket.write(pairing.takeOutgoing())
socket.write(pairing.takeOutgoing())                    // the goodbye
keepPrivately("pairing", pairing.pairedMaster())        // holds the phone's private key
val engine = pairing.makeEngine(wallSecs())             // the code secret never reaches Kotlin
keepPrivately("state", engine.save())

// After a restart:
val engine = PhoneEngine.load(readPrivately("state"))
val paired = PairedMaster.fromBytes(readPrivately("pairing"))

// About once a second while the screen is in use, and at once on any change:
val orders = engine.tick(PhoneFacts(wallSecs(), elapsed(), bootCount(), offset(), is24h(), autoTime(),
    interactive(), keyguard(), callState(), front(), session?.isOpen() == true, prodded, permissions()))
if (orders.saveState) keepPrivately("state", engine.save())  // before anything else
show(orders.cover); notify(orders.timeLeft.words); orders.warning?.let(::float); log(orders.notes)
if (orders.sendReport) {
    val report = engine.outgoingReport(wallSecs())!!
    keepPrivately("state", engine.save())
    session.sendReport(report, elapsed())
}

// On the session, for each event:
//   OPENED: tick (it orders a report)   GRANT: engine.takeGrant(event.grant!!, wallSecs()),
//   keep the state, tick again          REPORT_NOW: the next facts say prodded
// A LinkException.NoAnswerToReport, or the socket closing while reportWaiting(): engine.lostContact()

// From the ways out activity:
val answer = engine.tryCode(typedBytes, wallSecs()); typedBytes.fill(0)
keepPrivately("state", engine.save())
```

Every rule of the engine's README holds as it is: keep the state whenever
the engine says so and before anything else, carry out every order as a
level, call `tick` again at once after a grant, never draw over what the
orders leave uncovered.

## What crosses, value by value

Kotlin's own number types cross: `Long` for times, durations and seconds,
`Int` for counts and minutes. Going in, a number below zero where none can
be (the elapsed count, the boot count, minutes) is refused with
`EngineException.OutOfRange`; coming out, a number too big for its type
(none is, in practice) is held at the largest one.

**The engine**, `PhoneEngine`, an object:

| Call | In | Out |
| --- | --- | --- |
| `PhoneEngine.paired` | the welcome `Grant` as JSON bytes, the code secret as bytes, the wall clock | a new engine, setup not finished on it yet. For a welcome that came by another way than the link (the development tool over the cable) |
| `PhonePairing.makeEngine` | the wall clock | a new engine from the link's welcome; the code secret never leaves Rust |
| `PhoneEngine.load` | the kept bytes | an engine; never fails (a state that cannot be read gives one that locks the phone) |
| `loadProblem` | | `LoadProblem?`: a kind (`EMPTY`, `NOT_LIMO`, `NEWER`, `DAMAGED`), the form for `NEWER`, the words |
| `tick` | `PhoneFacts` | `PhoneOrders` |
| `outgoingReport` | the wall clock | the core's `Report` as JSON bytes, or null |
| `takeGrant` | the core's `Grant` as JSON bytes (from a `LinkEvent`), the wall clock | `GrantTaken`: `TAKEN`, `STALE` or `NOT_FOR_THIS_DEVICE`, and `reportAgain` |
| `lostContact` | | |
| `askForTime` | minutes, a reason, the wall clock | `TimeRequest`, the core's record field by field |
| `tryCode` | the typed code as UTF-8 bytes, the wall clock | `TryAnswer` |
| `tryPin` | the typed PIN as UTF-8 bytes, a `PinAction`, the wall clock | `TryAnswer` |
| `tryGuardPin` | the PIN typed on the guard as UTF-8 bytes, the wall clock | `TryAnswer`: `RELEASE_CONFIRMED` for a right PIN (the core's way of saying only that it was right), with the engine's words for until when the guarded pages are open; wrong PINs count with the lock's |
| `finishSetup` | the wall clock | true when this call finished setup on the phone, false when it was finished already (nothing changes). Until it is, the engine counts and covers nothing. Keep the state at once, then tick |
| `shouldSave`, `save`, `saveFailed` | | a flag; the bytes to keep; |
| `view` | | `PhoneView?`: the ids, the day, counted, beyond and open seconds, how many slices and notes wait, the clock correction, whether setup is finished (`setupFinished`), and the engine's whole view as JSON, the child's Today, the rules in words and setup among it (below) |
| `versions()` (a function) | | this layer's, the engine's, the core's and the link crate's versions, and the link version it speaks |

**`PhoneFacts`** (in), field for field the engine's: `wallSecs`,
`elapsedMs`, `bootCount`, `utcOffsetSecs`, `hours24`, `autoTime`,
`interactive`, `keyguardLocked`, `call` (`IDLE`, `RINGING`, `ONGOING`),
`front`, `inReach`, `prodded`, `permissions`, and `guarded` (last, with a
default of null, so Kotlin that does not name it still builds). `guarded`
is a `GuardedPage?`: `DEVICE_ADMIN` or `ACCESSIBILITY`, a page in front that
could switch LiMo off or remove it, as the watch recognised it (the
engine's README, rule 12). `front` is a `Front`: a
`kind` (`UNKNOWN`, `HOME`, `LIMO`, `DIALLER`, `EMERGENCY_DIALLER`, `APP`)
with `packageName` and `label`, both optional and needed (the package) only
for `APP`. `permissions` is a `PhonePermissions` of six, in the order setup
asks for them: `notifications`, `phone` (Android's Phone permission,
`READ_PHONE_STATE`), `usageAccess`, `accessibility`, `battery` (left out of
battery optimisation) and `protection` (LiMo's device admin), each true only
when Android says so.

**The view's JSON** is the engine's own `PhoneView`. The child's screens
read two parts of it, every word in them the engine's (the engine's README,
"The child's own screens", says what each means):

```json
{
  "today": {
    "left": { "kind": "left", "words": "1 h 38 min" },
    "used": "22 min used of 2 h",
    "used_part": 0.189,
    "where_used": "On this phone 22 min",
    "bedtime": "Screens are off from 9:00 pm to 7:00 am.",
    "apps": [
      { "package": "org.example.game", "secs": 721, "minutes": "12 min" },
      { "package": "org.example.maps", "secs": 540, "minutes": "9 min" }
    ],
    "guardian_needed": { "off": [
      { "permission": "accessibility",
        "words": "Accessibility is switched off, so LiMo cannot see which app is open." }
    ] }
  },
  "rules_words": {
    "time_a_day": "2 h",
    "screens_off": "9:00 pm to 7:00 am",
    "always_open": ["org.example.clock"],
    "not_allowed": ["org.example.casino"]
  },
  "setup": { "finished": false, "finished_at": null, "words": "Setup is not finished on this phone." }
}
```

`setup` says whether a guardian has finished setup on the phone; until
then nothing is counted or covered, and `words` is the engine's sentence
for it (null once finished; `finished_at` is when, by LiMo's clock, null for
a state kept before setup was kept).

Once the master has said how much of the day went on the child's other
devices, `used` is the whole day and `where_used` says both, "On this phone
22 min · on the PC 17 min" (or "· on other devices 17 min"); until then
this phone's use only, as above. The figure comes in the grant's JSON and
the engine keeps it; nothing about it crosses this layer on its own.

`left.kind` is `left`, `no_limit` or `open`; `used_part` is null on a day
with no limit; `bedtime` and `screens_off` are null when no hours are off;
`guardian_needed` is null while all six are on, and its `permission` is
`notifications`, `phone`, `usage_access`, `accessibility`, `battery` or
`protection`. Apps are packages, never names: Kotlin puts each app's name in
before the screens show it.

**`PhoneOrders`** (out): `cover`, `timeLeft`, `warning`, `sendReport`,
`saveState`, `notes`, `nextCheckInSecs`.

- `cover` is a `Cover`: a `kind` (`NOTHING`, `BLOCK`, `LOCK`, `GUARD`), with
  `block` set for a block, `lock` for the lock and `guard` for the guard
  (the last field, with a default of null).
- `GuardCover`: `page` (`GuardedPage`), `stamp`, `title`, `message`,
  `canUsePin`, `tryWaitUntil`, `waitWords`, `ways` (the PIN's one `WayRow`,
  or none with no PIN set up), `nothingToDo`, `callOn`, `backToCall`.
- `BlockCover`: `packageName`, `title`, `message`, `goHome`, `callOn`,
  `backToCall`.
- `LockCover`: `why` (`LockWhy`, the engine's nine reasons), `stamp`,
  `title`, `message`, `until`, `untilWords`, `allowedPackages`,
  `canAskForTime`, `canUseCode`, `canUsePin`, `tryWaitUntil`, `waitWords`,
  `latestRequest`, `requestLine` (`text`, `stamp`), `ways` (each a `WayRow`:
  `way` of `ASK`, `CODE`, `PIN`; `title`; `sub`; `off`), `nothingToDo`
  (`bold`, `line`), `callOn`, `backToCall`. `latestRequest` is a
  `RequestShown`: the `TimeRequest`, its `answer` (`WAITING`, `GRANTED`,
  `REFUSED`) with `grantedMinutes`, `answeredAt` and `reachedMaster`.
- `TimeLeft`: `secondsLeft`, `counting`, `unlockedUntil`, `away`, `locked`,
  `words`.
- `WarningShown`: `cause` (`TIME_RUNS_OUT`, `BLOCKED_HOURS_START`,
  `UNLOCK_ENDS`), `minutes`, `lockAt`, `away`, `message`, `hint`,
  `whenWords` (`when` in the engine is a Kotlin keyword).
- `notes`: each note as the engine's JSON, such as
  `{"note":"blocked_app_opened","package":"org.example.casino","day":20775,"at":1794997860}`.
  They are for the host's log: the engine itself carries the kinds the
  master takes in the report's bytes (`"notes"`, each with its `"id"`) until
  a grant's bytes confirm them, and the view's `notesKept` counts those
  still waiting and the newest of the kinds that stay on the phone.

**`TryAnswer`**: `kind` (`ADDED`, `UNLOCKED`, `RELEASE_CONFIRMED`, `WRONG`,
`MUST_WAIT`, `MALFORMED`, `ALREADY_USED`, `NOT_SET`), `message`,
`opensNow`, and for the kinds that have them `addedSecs`, `unlockedUntil`,
`wrongGuesses`, `waitUntil`. **`PinAction`**: a `kind` (`ADD_MINUTES`,
`UNLOCK_UNTIL`, `CONFIRM_RELEASE`) with `minutes` or `until`.

**The link**, each an object:

| Object | Calls |
| --- | --- |
| `Offer` | `Offer.fromText(bytes)`; `masterId()` (16 hex), `port()`, `addresses()` (dotted IPv4), `expiresAt()`, `looksOld(wallSecs)` |
| `PhonePairing` | `PhonePairing.start(offer, PairRequest(name, platform, androidVersion), nowMs)`; `takeOutgoing()`, `receive(bytes, nowMs)` (true once welcomed), `tick(nowMs)`, `isDone()`, `masterNow()`, `pairedMaster()` (the bytes to keep), `makeEngine(wallSecs)` (once) |
| `PairedMaster` | `PairedMaster.fromBytes(bytes)`; `childId()`, `deviceId()`, `masterId()`, `port()`, `addresses()` |
| `PhoneSession` | `PhoneSession.start(paired, nowMs)`; `takeOutgoing()`, `isOpen()`, `reportWaiting()`, `receive(bytes, nowMs)` (a list of `LinkEvent`), `sendReport(bytes, nowMs)`, `sendApps(list of AppEntry(packageName, label), nowMs)`, `sendBye(nowMs)`, `tick(nowMs)` |

A `LinkEvent` is a `kind` (`OPENED`, `GRANT`, `REPORT_NOW`, `PONG`), with
`grant` (the core's `Grant` as JSON bytes) for a grant and `masterNow` for a
grant and a pong. Times on the link are the elapsed count in milliseconds;
the phone's side of the link never reads the wall clock.

## Shapes, and why

The surface keeps to the shapes the plan set for it: **objects with
methods, plain records, lists, optional values, byte arrays and flat
errors**, and enums without data for the kinds, which cross as an index
exactly as a flat error's variant does. Proof three ran a record, a string
and a flat error through JNA on a phone and left objects untested; the JVM
tests here run every shape used, through the same generated Kotlin, on the
PC. Nothing is async and nothing calls back from Rust into Kotlin.

Every Rust enum that carries data was given one of two shapes:

| Value | Shape | Why |
| --- | --- | --- |
| What is in front (`Front`) | a record with a kind and optional fields | Kotlin builds it at every call; a kind and two optional strings are what the accessibility service has |
| The cover (`Cover`) | a record with a kind and one optional record per kind | Kotlin acts on every field of it: draws it, names the app, lists the ways out |
| A code's or a PIN's outcome (`TryOutcome` in `TryAnswer`) | a record with a kind and optional fields | the ways out activity acts on the kind (close on a right one, release on `RELEASE_CONFIRMED`, show the wait) |
| A PIN's action (`PinAction`) | a record with a kind and optional fields | the activity knows it from how it was opened |
| A link event (`PhoneEvent`) | a record with a kind and optional fields | the host acts on the kind |
| A request's answer (`RequestAnswer`) | a kind and `grantedMinutes` | the lock may show it |
| Why a state could not be loaded (`LoadProblem`) | a kind, the form, the words | for the host's log and a guardian's screen |
| The notes (`PhoneNote`) | the engine's JSON text, one per note | the host only logs them; the engine keeps them for the master itself. A record would be a dozen optional fields Kotlin never reads |
| The report (`Report`) and the grant (`Grant`) | the core's JSON, as bytes | they only travel between the engine and the link, both in Rust; Kotlin carries them and never reads them, so a change in the core changes nothing here |
| The kept state, the kept pairing | bytes | Kotlin writes them to a file and hands them back |
| The view (`PhoneView`) | a few numbers, and the whole as JSON | the child's own screens are React Native, which reads JSON as it is; Kotlin puts each app's name in for its package and passes it on. The rules and the decision inside it are large and change with the core |

The error enums carry data in Rust too (`PhoneError::Refused { problem }`,
`LinkError::Refused(why)`, `LinkError::NoAnswer { report_waiting }`): they
cross as flat errors, with each refusal and each reason as a variant of its
own, so Kotlin can tell them apart without reading words.

## What does not cross, and why

- **The master's side of the link, the hub, the core's `DeviceState`,
  `Decision` and `RuleSet` as records.** The phone has no use for the first
  two; the others are inside the engine, which gives the orders and the
  view.
- **Discovery names and TXT records.** `PairedMaster.masterId()` and the
  offer's addresses are enough to connect; reading an announcement's TXT
  records (`limo_link::discovery::read_txt`) and the service type are left
  for the piece that writes the discovery, which can add them here
  in a few lines.
- **The guardian form.** The library is in the child form only, and a
  pairing request always says the child's form.
- **Callbacks and async functions.** Not needed: the engine and the link
  are fed facts and bytes and give orders and bytes; Kotlin owns every
  clock, thread and socket. Proof three left both untested.
- **The engine's limits as constants** (`ANSWER_WAIT_MS` and the rest). The
  link's session ends a report unanswered for ten seconds by itself
  (`LinkException.NoAnswerToReport`), and the orders say when to call
  again.

## Errors

Flat: Kotlin gets a sealed class with one exception class per variant, and
the message is the words below. None holds a PIN, a code or a secret.

`EngineException`: one variant for each refusal of the core
(`SecretWrongLength`, `MinutesOutOfRange`, `ReasonTooLong`,
`NoLimitToday`, `RequestWaiting`, `TooManyRequests`, `UnlockOutOfRange`),
each with the core's own words ("A request from this device is still
waiting for an answer."); `CannotCheck` (the kept state could not be
loaded, so nothing can be changed); and this layer's own: `OutOfRange` (a
number below zero, a field missing for its kind, bytes that are not text),
`NotAGrant`, `NotAWelcome`, `NotWelcomed` (no welcome yet, or its engine
made already), `Broken` (see below) and `Internal` (a bug).

`LinkException`: one variant for each of the link's errors, with its
words, with a refusal split by its reason (`RefusedRemoved`,
`RefusedUnknown`, `RefusedOtherDevice`, `RefusedVersion`,
`RefusedOfferUsed`, `RefusedOfferExpired`, `RefusedNotSupported`) and an
unanswered question by whether a report was waiting (`NoAnswerToReport`,
`NoAnswer`); and this layer's own: `NotAReport`, `NotWelcomed`,
`OutOfRange`, `Broken`. The link's rule holds: an error from `receive` or
`tick` ends the link (close the socket); one from a call that sends
changes nothing. A test checks every variant of the core's refusals and of
the link's errors crosses to a variant of its own, with the same words.

## Threads, lifetimes and a panic

Each object holds its value behind a lock, so Kotlin may call it from any
thread; calls on one object run one at a time. Objects are Kotlin
`AutoCloseable`s: `close()` one when it is done with (UniFFI's cleaner
frees it otherwise when it is collected). Closing an `Offer`, a
`PairedMaster` or a `PhonePairing` drops the secret it holds, which is
wiped.

A panic inside Rust reaches Kotlin as UniFFI's `InternalException`. If it
happened while an object's lock was held, what the object holds may be half
changed, so every later call on it says `Broken`. For the engine: close it
and load a new one from the kept state; the engine's README shows a restart
at any instant is safe. For a link object: close the socket and start
again. Nothing in the engine or the link is known to panic.

## Secrets

The secrets: a typed code or PIN, the phone's code secret, the offer's
secret (inside its text) and the phone's private key (inside the kept
pairing). All cross as byte arrays, never as Kotlin strings, so Kotlin can
wipe its own copy.

| What | This layer | Kotlin must |
| --- | --- | --- |
| A typed code or PIN | reads it as text without a copy and wipes its bytes when the call ends | read the field into a `CharArray`, make the bytes, fill both with zeros after the call, and clear the field |
| The code secret, pairing over the link | never leaves Rust: `makeEngine` hands the engine its own copy, and the link's is wiped | nothing |
| The code secret, `PhoneEngine.paired` | moved into the engine, nothing copied | fill its array with zeros |
| An offer's text | wipes its bytes once read; the offer holds the secret in memory that is wiped when it is closed | fill its array with zeros, keep no string of it longer than needed, close the `Offer` after pairing |
| The kept pairing (`pairedMaster`) | a copy goes to Kotlin; the link's is wiped | write it to private storage, fill the array with zeros; read it back with `PairedMaster.fromBytes`, then fill that array too |
| The kept state (`save`, `load`) | wipes its copy of the bytes handed to `load` | write it to private storage kept out of backups (the app already excludes everything), then fill the array with zeros |

Not wiped, by this layer or anyone: the buffers UniFFI copies each value
through on its way across (allocated and freed by Rust, never zeroed), the
copies JNA makes on the Kotlin side, the engine's state in memory (the core
keeps the code secret and the PIN record in plain memory, as it does on the
PC), a code secret of the wrong length that the core refuses, and what the
link's README lists. A secret's life in memory is short and the process is
the app's own; what this table buys is that no secret lingers in a Kotlin
string the garbage collector may keep for a long time.

## Unsafe code

UniFFI's macros generate unsafe code (the exported C functions, the buffers
that cross, the metadata), so this crate cannot say
`#![forbid(unsafe_code)]` as the others do. No line written here by hand is
unsafe. The conversions, which are the only logic here, live in
`src/convert.rs`, where no UniFFI macro expands and which forbids unsafe
code; `src/shapes.rs` holds the records and enums the macros expand on, and
`src/engine.rs` and `src/link.rs` the objects, each method a few lines. A
test checks every source file for unsafe code and checks `convert.rs`
forbids it.

## The testkit

A made up welcome needs the family's books, which live on the master PC.
With the `testkit` feature, `TestFamily` plays the master over a real
family file in a folder the test gives, through the link crate's `dev`
master and the hub: it adds a child with rules, sets the PIN, makes a
welcome and a code secret, makes one time codes, records use on the
child's PC (`use_pc`, `usePc` in Kotlin), answers a report with a grant,
makes a pairing offer, removes a phone, and plays the master's end of a
connection. The Rust tests here and the JVM tests of the app's child
module use it.

The library on a phone never has it:

- no feature is on by default, and nothing but the tests turns it on (the
  tests here through a dev dependency, which never reaches a build of the
  library itself; the JVM tests through the build script's `host` step,
  which builds a separate library for the PC);
- the build script refuses a phone library that holds it: no `testfamily`
  or `testkit` in any case (UniFFI exports every function and its metadata
  by name), and no SQLite (`SQLite format`, which the testkit's family
  file brings);
- `npm run inspect` runs the same check on every native library and dex
  file of both built APKs, and runs it once on the PC library the JVM tests
  load, to show that it finds the testkit where it is: "it holds the
  testkit ("testfamily"); it holds the testkit ("testkit"); it holds
  SQLite".

## Building

From the app, `npm run build:child` runs everything below as part of the
child form's Gradle build; `npm run build:engine` runs the phone step by
itself. The script is [`../app/scripts/build-engine.mjs`](../app/scripts/build-engine.mjs).

**For the phone** (`node scripts/build-engine.mjs android`, Gradle task
`buildRustEngine`, before every build of the child module):

1. `cargo build --lib --release --target aarch64-linux-android`, linked
   with the NDK's own `clang.exe` for Android 8.0 (API 26), with 16 KB page
   alignment, and with the repository, Cargo's folder and the home folder
   written as `limo`, `cargo` and `home` (`--remap-path-prefix`): panic
   messages carry source paths, and a built app must not say who built it
   or where. No cargo-ndk.
2. A check of the library: no testkit, no SQLite, no folder or user name
   of the build PC.
3. The library into `modules/limo-child/android/src/generated/jniLibs/<abi>/`.
4. The Kotlin bindings, generated from that library by UniFFI's own
   generator built from this crate (`--features bindgen`, in its own target
   folder `target/bindgen`), into `src/generated/kotlin`.

**For the JVM tests** (`node scripts/build-engine.mjs host`, Gradle task
`buildRustEngineForTests`, before the module's unit tests only):

1. `cargo build --lib --features testkit` for the PC (a debug build with
   optimised dependencies).
2. The library into `src/generated/test-native/`, which the tests load
   through JNA (`jna.library.path`).
3. A second copy of the bindings, generated from it, in package
   `ai.lucidloom.limo.child.ffi.testkit`, into `src/generated/test-kotlin`.
   The tests drive the engine and the link through the bindings that ship
   and use this copy only for `TestFamily`, which the shipping ones never
   have.

Files are rewritten only when their content changed, and Gradle skips each
task when none of the Rust sources it reads changed.

Needs, beyond what the app needs: Rust 1.89 or later with the
`aarch64-linux-android` target (`rustup target add aarch64-linux-android`);
UniFFI's generator itself needs Rust 1.91 or later (one of its crates,
cargo-platform 0.3.3, asks for it). Both were already on the build PC.

**Times** on the development PC (an AMD Ryzen 9 7950X3D, Windows 11),
crates already downloaded:

| Build | Time |
| --- | --- |
| Clean, for the phone: the library (release, LTO), then UniFFI's generator, built once | 182 s (the library 20 s) |
| Clean, for the JVM tests: the library with the testkit (SQLite among it), and the test bindings | 46 s |
| The phone step again, nothing changed | under 1 s |
| One line of this crate changed: the phone step | 7 s |
| `npm run test:child`, nothing changed (Gradle checks only) | 20 s |
| `npm run build:child`, `android/` made again: Gradle in all, the Rust step within it | 144 to 193 s; the Rust step under 1 s with nothing changed, 17 s after a change |

**Sizes.**

| What | Bytes |
| --- | --- |
| `liblimo_phone_ffi.so` as built (arm64, release, LTO, `opt-level = "s"`) | 2,380,080 |
| The same in the APK, after the Android Gradle Plugin strips its symbol table | 1,365,032 |
| JNA's `libjnidispatch.so` in the APK | 165,992 |
| JNA's classes and the bindings, in the dex files (compressed) | about 173,000 |
| The child form's APK | 30,203,195 (28,441,712 before: about 1.7 MB more, nearly all the three rows above) |
| The guardian form's APK | 28,213,203: its native libraries and dex files the same bytes as before; it holds nothing of this |
| The PC library for the JVM tests, with the testkit and SQLite (debug) | 4,672,512 |

The library on the phone holds the engine, the core, the link with its
crypto, serde_json and UniFFI's runtime; panics unwind (so that one reaches
Kotlin as an exception), which keeps Rust's backtrace printer in it, as
proof three measured.

## Tests

`cargo test` runs three groups:

| Where | What | Count |
| --- | --- | --- |
| `src/convert.rs`, `src/engine.rs` | The facts field by field, each of the six permissions to its own field, each guarded page to its own and none to none; every kind in front, and an app without its package refused; numbers below zero refused, big ones held; a PIN's action with what its kind needs; every refusal of the core and every error of the link to a variant of its own with the same words; JSON that is not a grant or a report; typed bytes that are not text; an engine whose lock was poisoned saying `Broken` | 9 |
| `tests/bridge/` | Through this layer's API only, against the testkit (its phone set up at once, but in the setup story): a phone just paired counting and covering nothing, its words and its view saying setup is not finished, then finished, noted, covering and counting, kept, finishing again changing nothing, a state that cannot be read refusing it; a report out and a grant in as bytes, a stale grant, bytes that are not a grant; the notes as the engine's JSON and the view; the view's JSON carrying Today, the rules in words and what is switched off, by the names Kotlin reads; a ringing call never covered and a call in progress offering the way back; asking for time and the core refusing a second; codes wrong, malformed, the wait after five surviving a crash; the PIN adding, unlocking, confirming a release, refused before it is looked at; four kinds of unreadable state locking the phone and never written over; a welcome that is not one and a secret of the wrong length; crashes after any call; a whole pairing over the link making the engine, a session carrying the report and the grant, the app list and the goodbye; a report unanswered ending the session as `NoAnswerToReport`; a removed phone refused by its reason; bad offers and kept pairings refused without quoting them; the guard crossing as its own record in the engine's words, noted, a wrong and a right PIN on it as records, open through a crash and guarded again five minutes on, bytes that are not text refused, the accessibility settings guarded and nothing before setup is finished; the notes crossing in the report's bytes with their ids, a grant's bytes confirming them and the phone dropping them, the blocked app's note staying; the view's JSON saying how much of the day went on the PC; the versions | 21 |
| `tests/house_rules.rs` | No line written here unsafe; the conversions forbid it and no UniFFI macro expands there; the testkit only behind its feature; nothing silences a lint; no dash punctuation; no machine paths | 6 |

`npm run test:child` (in the app) runs the child module's **JVM tests**,
which drive the real engine and the real link from Kotlin through the
bindings that ship, with the PC library loaded by JNA:

| Test | What |
| --- | --- |
| `EngineDayTest.aShortDayFromAMadeUpWelcome` | paired from a made up welcome; a minute counted in a game; a blocked app covered, named by its label, not counted, noted once; the time used up with warnings at 15, 5 and 1 minutes and the lock ordered with the code offered and the clock still usable; the diallers and LiMo never covered, home covered; a right code adding 15 minutes and opening it; the state kept; a new engine loaded from the kept bytes giving the same orders; the day going on from it |
| `EngineDayTest.inReachTheReportGoesOutAndTheGrantComesBackAsBytes` | the report out, the grant in, a stale grant, `EngineException.NotAGrant` |
| `EngineDayTest.theCoresRefusalsArriveAsTheirOwnExceptionsWithTheirWords` | `EngineException.RequestWaiting` with the core's words, `OutOfRange` for an app with no package, a wrong and a right PIN |
| `EngineDayTest.aKeptStateThatCannotBeReadLocksThePhoneAndIsNeverWrittenOver` | empty bytes: `EMPTY`, the lock `CANNOT_CHECK`, nothing to keep |
| `EngineDayTest.theViewCarriesTodayTheRulesAndWhatIsSwitchedOffAsTheEnginesJson` | the view's JSON with Today's time left, used line and part, where it went, the app list, the Phone permission switched off with its sentence, and the rules in words; the note of it switched off; none needed once it is on again |
| `EngineDayTest.theLibraryNamesItsVersions` | `versions()` |
| `EngineDayTest.atHomeTheNotesGoToTheMasterAndTodaySaysTheUseOnThePc` | 12 minutes on the family PC; the phone's setup finished note waiting away from home, then at home sent, confirmed and dropped; the view's JSON saying "On this phone 1 min · on the PC 12 min" and the whole day in its used line |
| `EngineDayTest.setupCountsAndCoversNothingUntilAGuardianFinishesIt` | a phone just paired: a game and the blocked casino neither counted nor covered, the time left and the view saying setup is not finished; `finishSetup` true and kept, the casino covered and the game counted from the next call; again false with nothing to keep; finished after a reload |
| `LinkTest.aPairingMakesTheEngineAndASessionCarriesTheReportAndTheGrant` | an offer read, a pairing through the testkit's master, the engine made from it, the kept pairing read back, a session opened, a report and its grant, the goodbye, `LinkException.Closed` |
| `LinkTest.aBadOfferIsRefusedWithoutQuotingIt` | `LinkException.BadOffer` |

All 10 pass, in under a second of test time. `cargo build`, `cargo test`,
`cargo clippy --all-targets` and `cargo doc --no-deps` pass with no
warnings and no allow attributes.

## What only a phone can prove

- That JNA loads `liblimo_phone_ffi.so` from the APK on Android 8 to 15
  and on a phone with 16 KB pages, and that the UniFFI checksums match.
- That objects, enums, lists and optional values behave on ART as on the
  JVM. The JVM tests ran the same bindings on the PC; ART is another
  runtime.
- The cost of a `tick` on a phone: proof three measured about 0.25 ms from
  Kotlin into Rust and back for one small record; a tick carries a few
  records each way and decides, and the engine expects to be called at
  most once a second.
- How long `tryPin` and `tryGuardPin` take on a phone (the PIN is hashed
  slowly on purpose, 200,000 rounds; the library is built for size).
- That the cleaner frees objects on Android as on the JVM, and that a
  panic, which never happened here, reaches Kotlin as an exception.
- That code shrinking stays off: JNA binds functions by name, so turning
  R8 on needs keep rules for `com.sun.jna.**` and
  `ai.lucidloom.limo.child.ffi.**`.

## What the engine and the link should gain

What this layer worked around, inside itself:

- **Engine: a way to make it from a pairing's welcome and secret in one
  call.** `PhonePairing.makeEngine` holds the link's `Welcomed` parts
  until it is called, then copies the code secret out of the link's
  `CodeSecret` into the `Vec` the engine takes. A
  `PhoneEngine::paired_with(&Grant, &CodeSecret, now)` (or the engine
  taking `Zeroizing<Vec<u8>>`) would make that one move and no copy.
- **Engine: the notes and the view as plain data for others.** The notes
  cross as the engine's JSON because `PhoneNote` carries data; if the host
  ever needs them typed, the engine could give each a kind and a short
  sentence for a log.
- **Core: a refusal of a code secret of the wrong length that wipes it.**
  `DeviceState::set_code_secret` drops a refused secret without wiping it.
- **Link: a phone's `Now` without the wall clock.** The phone's side never
  reads `Now::unix`; this layer passes 0. A phone constructor taking only
  milliseconds would say so in the types.
- **Link: the kept pairing into a buffer the caller gives**, or as
  `Zeroizing` all the way: today `to_bytes` returns wiped memory, and this
  layer has to copy it out for Kotlin.

## Dependencies and licences

In the library on the phone, beyond the engine's, the core's and the
link's (listed in their READMEs, with their licences):

| Crate | Version | Licence | Used for |
| --- | --- | --- | --- |
| limo-phone, limo-link, limo-core | 0.1.0 (path) | Apache-2.0 | the engine, the link, the report and the grant |
| uniffi, uniffi_core, uniffi_macros, uniffi_meta | 0.32.2 | MPL-2.0 | the exports and their runtime; `uniffi_core` is in the library |
| anyhow, bytes, static_assertions | 1.0.104, 1.12.2, 1.1.0 | MIT or Apache-2.0 (bytes: MIT) | used by `uniffi_core` |
| serde, serde_json | 1.0.229, 1.0.154 | MIT or Apache-2.0 | JSON of the report, the grant, the notes and the view |
| zeroize | 1.9.2 | Apache-2.0 or MIT | wiping this layer's own copies |

Built for the phone but not kept in the library (link time optimisation
drops them), as proof three found: uniffi_pipeline 0.32.2 (MPL-2.0),
tempfile, rustix, indexmap and a few small helpers (MIT, Apache-2.0, or
both). Build time only: UniFFI's macros and their crates (toml, syn, heck
and others), and, for the generator only, uniffi_bindgen and uniffi_udl
(MPL-2.0), clap, askama, goblin, cargo_metadata, weedle2, textwrap and
others (MIT, Apache-2.0, or both). For the tests and the testkit only: the
hub with SQLite (rusqlite and libsqlite3-sys, MIT; SQLite itself is in the
public domain) and tempfile. `Cargo.lock` holds the same versions as the
other crates' locks wherever they share a crate.

MPL-2.0 is a file level copyleft: UniFFI's own files stay under it, and
nothing of them was changed; LiMo's code is not affected. No crate is GPL
or AGPL. The code here was written for LiMo; nothing was copied. The crate
is under the Apache License 2.0, as is all of LiMo.

On the Kotlin side, in the child form's APK: JNA 5.19.1 (Apache-2.0 or
LGPL-2.1-or-later, used under Apache-2.0) and the generated bindings. The
JVM tests add JNA's jar for the PC and JUnit 4.13.2 (EPL-1.0, tests only).

## Layout

```
phone/ffi/
  Cargo.toml, Cargo.lock, README.md, .gitignore
  uniffi.toml           the Kotlin package of the bindings
  uniffi-bindgen.rs     UniFFI's generator, built with --features bindgen
  src/
    lib.rs              crate notes, the public names, versions()
    shapes.rs           the records, enums and errors that cross
    convert.rs          every conversion (forbids unsafe code)
    engine.rs           PhoneEngine
    link.rs             Offer, PairedMaster, PhonePairing, PhoneSession
    testkit.rs          TestFamily (feature testkit)
  tests/
    bridge/             the layer through its own API, with the testkit
      main.rs, common.rs, day.rs, ways_out.rs, kept.rs, link.rs, setup.rs,
      guard.rs, notes.rs
    house_rules.rs
```

In the app: `scripts/build-engine.mjs` (the build), `scripts/test-child.mjs`
(the JVM tests), `modules/limo-child/android/build.gradle` (the two Gradle
tasks), `modules/limo-child/android/src/test/java/ai/lucidloom/limo/child/ffi/`
(the JVM tests), and the generated, git ignored
`modules/limo-child/android/src/generated/{kotlin,jniLibs,test-kotlin,test-native}`.
