# limo-phone-dev

A small PC tool that puts a test family on a child's phone over the USB
cable, so that LiMo can be tried on a real phone before pairing over the
network exists. One program, `limo-phone-dev.exe`, that wraps `adb`.

It is scaffolding, like `limo dev-setup` and `limo dev-code` on the PC
([`../../windows`](../../windows/)): it makes a family with a guardian PIN
typed at a hidden prompt, one child, a time a day, blocked and always
allowed apps and a bedtime, pairs one phone with it, and makes one time
codes for it. It goes once the link ([`../../link`](../../link/) and its two
halves) pairs a phone for real.

**What it is not.** It is not how a family will set up a phone: the real
child form pairs with the master PC over the link, at setup. It has no
network of its own, never opens the real family's file, and never runs
`adb` against anything but the one phone attached. It has been built and
tested on the development PC against a fake phone only: nothing here has
run against a real phone yet. [`../TESTING.md`](../TESTING.md) is how it
will be tried, with the owner.

**What it never does on the phone.** It changes nothing on the phone but
LiMo's own app and LiMo's own files. It never turns on developer options or
USB debugging (a person does that, on the phone), and never touches the
phone's accounts, its Device Owner or its settings. The help says so first.

## Contents

- [Commands](#commands)
- [Where the test family lives](#where-the-test-family-lives)
- [What crosses the cable](#what-crosses-the-cable)
- [When something is not right](#when-something-is-not-right)
- [How status reads the phone](#how-status-reads-the-phone)
- [What LiMo's debug build must do](#what-limos-debug-build-must-do)
- [What only a real phone can prove](#what-only-a-real-phone-can-prove)
- [Tests](#tests)
- [Dependencies and licences](#dependencies-and-licences)
- [Layout](#layout)

## Commands

Build it with `cargo build --release` in this folder; the program is
`target\release\limo-phone-dev.exe`. Every command prints what it did and
what to do next, and every call to `adb` it makes, as typed, before it runs
it.

| Command | Phone | What |
| --- | --- | --- |
| `check` | reads only | Is `adb` there; is exactly one phone attached and allowed; which Android; is LiMo's child form installed, which version, and is it a debug build (`run-as` works: only those can take a test family); does LiMo keep a state, and is it this PC's test family, another family, or one it cannot read; is a pairing file waiting that LiMo has not taken. Then the test family on this PC, and what to do next. |
| `install --apk PATH` | installs | Reads the APK first, on the PC: its package must be the child form, `ai.lucidloom.limo.child`, and it must be a debug build (`android:debuggable` in its manifest). Anything else is refused before `adb` is touched: the guardian form, any other app, a release build of the child form, a file that is not an APK. Then `adb install -r -t PATH`: LiMo already there is replaced and keeps its files, so a test family on it stays. Android's refusals are said with what to do (signed with another key, a newer version on the phone, an Android older than 8.0, no room, an install over USB the phone does not allow). |
| `setup --child NAME --minutes N [--block PACKAGE]... [--allow PACKAGE]... [--bedtime HH:MM-HH:MM]` | writes LiMo's files | Takes the child's name with control characters and the marks that turn the direction of writing dropped, and spaces trimmed: 1 to 40 characters, or refused before anything is made. Asks for a guardian PIN twice at a prompt that does not show it (6 to 8 digits); makes the test family on this PC: the child, `N` minutes every day (0 to 1440), the blocked and always allowed apps by package name, a bedtime every day, the away share at 100 percent (so that a phone with no master has its whole day), and the PIN; pairs one phone; writes the pairing file into LiMo's own files through `run-as`, and sends the pairing broadcast, both exactly as "What LiMo's debug build must do" below fixes them; then reads back, for up to 15 seconds, that LiMo took it; when it did not, LiMo's own word for why, read once from its log, with what to do. The child's home zone is the phone's own, read from it (`date +%z`), so the days and the bedtime are what the phone's clock shows. Refused, changing nothing, when this PC has a test family already (it says what there is and what `forget` does), when LiMo on the phone holds any family already, or when the phone's clock and this PC's are 50 minutes or more apart (LiMo's debug build takes a pairing file only when it was made within an hour of its own clock); 5 minutes or more apart is said. |
| `code [--minutes 15\|30\|60]` | none | A one time code for the test phone, worth 15 minutes unless told, for the child's day today, printed once in the terminal and nowhere else. Each run makes a new one, and the test family's file records it; a phone takes ten a day. No phone is needed: the phone checks a code with the secret it shares with this PC. |
| `status` | reads only | What LiMo on the phone holds, as its own engine reads its kept state: the child, the time left, the time used today, each app's minutes, whether it is locked and why and until when, a guardian's unlock, wrong tries of a code or the PIN and the wait they set, a request that waits for an answer, what LiMo last saw switched off, and the use not yet sent to a master. Never a PIN, a code or the reason a child typed. See [How status reads the phone](#how-status-reads-the-phone). |
| `log [--minutes N]` | reads only | Follows the phone's log for LiMo's two tags, `LiMo` and `LiMoWatch` (`adb logcat -v time -s LiMo:V LiMoWatch:V`, and only lines of those tags are shown), until Ctrl+C, or N minutes (60 unless told, at most 720). |
| `forget [--this-pc-only]` | wipes LiMo's state | Reads LiMo's state first. When it is this PC's test family, sends the forget broadcast and checks, for up to 15 seconds, that the state is gone; removes a pairing file LiMo never took; then deletes the test family on this PC, so that `setup` can make a new one. A state of another family, or one that cannot be read, is left alone, and nothing is deleted. With LiMo gone from the phone, only this PC's side is left to delete. `--this-pc-only` asks nothing of any phone: for a phone that is gone. |
| `uninstall` | removes LiMo | `adb uninstall ai.lucidloom.limo.child`: LiMo and everything it kept. Refused by Android while LiMo is a device admin (its protection), which this tool never switches off; it says where a person does. The test family on this PC stays until `forget`. |
| `help` | none | What it never does, the commands, and where the test family lives. |

Exit codes: 0 done, 1 failed, 2 refused (nothing was changed). Everything
that needs neither a phone nor a file (the command line, an APK's package)
is checked before `adb` is touched.

## Where the test family lives

`%LOCALAPPDATA%\LiMo-phone-dev-TEST-ONLY\`, on Windows; elsewhere the same
folder in `$XDG_DATA_HOME` or `~/.local/share`. In it:

- `test-family.limo`: the test family, in the hub's own file
  ([`../../hub`](../../hub/)), with one child and at most one phone. The hub
  wants a guardian password for a family; the tool makes a random one and
  neither shows it nor keeps it, since nothing here asks for it. The PIN is
  kept only as the core's salted slow hash. The phone's 32 byte code secret
  is in it, as in any family's file. The folder is the user's own, readable
  by that user and by administrators, and nothing more: it is a test family,
  so use a test PIN.
- `phone-dev.log`: one line for each change the tool made, with the time in
  UTC: a test family made (with the number of minutes and of apps, no
  names), a code made (with its minutes, never the code), an install, a
  forget, an uninstall. Never a PIN, a code, a name or a secret. Moved to
  `phone-dev.log.old` past 256 KiB.

**Never the real family.** The Windows service keeps the real family's file
in its own data folder, open to SYSTEM and Administrators only. This tool
never names that folder or that file (a test checks it), and the two hubs
lock different files, so the service can run beside this tool. Deleting
`LiMo-phone-dev-TEST-ONLY` by hand is the same as `forget --this-pc-only`.

## What crosses the cable

Every call names the phone (`adb -s SERIAL`), and every shell command runs
with `shell -T`, with no terminal, so that bytes cross as they are. All of
them are in `src/phone.rs`, in one place. **No secret is ever in an
argument**, where another program on the PC or on the phone could read it
from a process list: every argument is fixed text, a package name, an
action's name, a time or the APK's path. The child's name crosses only in
the pairing file.

`setup`, in order:

1. `adb devices`; then reads: `getprop` (the Android version and the model),
   `pm list packages ai.lucidloom.limo.child`, `run-as ai.lucidloom.limo.child id -u`
   (is it a debug build), whether LiMo keeps a state, and `date +%s; date +%z`
   (the phone's zone).
2. The pairing file, **through adb's standard input**, the one call that has
   any:
   `run-as ai.lucidloom.limo.child sh -c 'umask 077 && mkdir -p files/limo-dev && cat > files/limo-dev/pairing.json.part && mv -f files/limo-dev/pairing.json.part files/limo-dev/pairing.json && wc -c < files/limo-dev/pairing.json'`.
   `run-as` runs the shell as LiMo's own app, in its own data folder, so the
   file is LiMo's, readable by LiMo alone (`umask 077`); it is written under
   another name and then moved, so LiMo never sees half of it; its size comes
   back and must match. The bytes go from this tool's memory through a pipe
   to `adb.exe`, to adb's server on this PC (its port on the PC's own
   loopback), over USB to `adbd` on the phone, and into `cat`.
   The file is one JSON object, its four fields in this order, with nothing
   after it:
   `{"welcome":{...},"code_secret_hex":"<64 hex digits>","made_at":<unix seconds>,"child_name":"<name>"}`.
   `welcome` is the core's welcome `Grant` as its own JSON: the child's and
   the phone's ids, the rules, and the PIN's record (the core's salted slow
   hash, never the PIN). `code_secret_hex` is the phone's 32 byte code secret
   in lowercase hex: it is what one time codes are made from, so it is the
   file's real secret. `made_at` is this PC's clock, in unix seconds.
   `child_name` is the child's name as the test family keeps it, 1 to 40
   characters, as a JSON string (quotes and backslashes escaped, any
   script as it is): the welcome carries no name, and the phone has no
   other source for it. The tool makes the bytes in memory that is wiped
   when it is dropped.
3. The broadcast, which carries nothing but names:
   `am broadcast -a ai.lucidloom.limo.child.dev.PAIR -n ai.lucidloom.limo.child/.dev.DevReceiver --include-stopped-packages --receiver-foreground`.
   It names LiMo's debug receiver, so no other app can receive it, and that
   receiver is guarded by `android.permission.DUMP`, which only adb's shell
   and the system hold, so no app on the phone can send it.
4. Reading back, once a second for up to 15 seconds: is the pairing file
   gone and is a state kept, then the state itself,
   `run-as ai.lucidloom.limo.child sh -c 'test -e files/limo/state || exit 3; cat files/limo/state'`,
   read here by the phone's own engine to check that it is the test phone's.
   **The state comes back over the cable**: it holds the same code secret
   and the PIN's record the pairing file held, and is dropped from memory
   once read. `check`, `status` and `forget` read it the same way.

If LiMo does not take the file in time, the tool reads LiMo's log once,
for its tag only and only since just before the broadcast by the phone's
clock (`adb logcat -d -v time -t <secs>.000 -s LiMo:V`, which reads and
ends), and says LiMo's own word for why (`dev pair refused
why=no_child_name`, for example) with what to do; or, when LiMo said
nothing, that the debug build may lack the receiver. Then it removes the
file from the phone (`run-as ... rm -f`) and deletes the test family on
this PC again, so that `setup` can simply run again.

`forget` sends `ai.lucidloom.limo.child.dev.FORGET` the same way, and reads
back that the state is gone. `install` is `adb install -r -t PATH`,
`uninstall` is `adb uninstall ai.lucidloom.limo.child`, `log` is
`adb logcat -v time -s LiMo:V LiMoWatch:V`. `code` sends nothing at all.

**Every call has a timeout**, through one function (`adb::run_adb`): 30
seconds for each ordinary call, 3 minutes for an install, 1 minute for an
uninstall, and the minutes asked for the log. A call that does not end in
time is stopped and said, never left hanging. adb's own server, which keeps
running after the call that starts it, is started first with no pipes at
all, so that it can never hold this tool's output open.

## When something is not right

Each has its own plain message, and none changes anything:

| What | Message, in short |
| --- | --- |
| `adb` not found | Not on the PATH, nor in `platform-tools` under `ANDROID_HOME` or `ANDROID_SDK_ROOT`, nor in `%LOCALAPPDATA%\Android\Sdk\platform-tools`: install the SDK platform tools, or set `ANDROID_HOME`. |
| No phone | Plug it in with a USB cable, unlock it, and check that USB debugging is on (Settings, Developer options). This tool never turns it on by itself. |
| Two phones, or a phone and an emulator | Lists them all: unplug the others or close the emulator, so that the tool cannot pick the wrong one. |
| A phone that has not allowed this PC | Unlock it and tap Allow on the "Allow USB debugging?" prompt; if none shows, plug it in again. |
| Offline, no permissions, or another state | Each said, with what to do. |
| Android older than 8.0 | LiMo needs Android 8.0 (API 26) or later. |
| LiMo not installed | Run `install --apk PATH` with the debug child form first. |
| A release build | Android refuses `run-as` for a release build, so this tool cannot reach LiMo's files, and a test family can go only on a debug build: `uninstall`, then install the debug child form. |
| `run-as` refused for another reason | Android's words, and that some makers' builds break `run-as`. |
| Unplugged part way, or no longer allowed | The phone is no longer there, or no longer allows this PC: plug it in again, unlock it, allow it. |
| A call that does not answer | Stopped after its timeout, and said. |
| A PIN that is not 6 to 8 digits, two that differ, or no terminal to type it in | Said, with nothing made. |
| A child's name longer than 40 characters, or empty once cleaned | Said, before anything is asked or made. |
| LiMo did not take the pairing file | LiMo's word from its log, and what to do: `old_file`, set the clock that is wrong; `no_child_name`, `not_json`, `no_welcome`, `no_code_secret`, `no_made_at`, `code_secret_not_hex` or `code_secret_not_32_bytes`, the tool and the app's debug build were built from different versions, so build both from the same one; `unreadable` or `not_deleted`, run setup again; `engine`, copy what `log` shows. Nothing in the log: is the receiver in this debug build? Both sides are undone. |

## How status reads the phone

It reads LiMo's kept state (`files/limo/state`) and loads it with the
phone's own engine ([`../engine`](../engine/), `PhoneEngine::load`), here on
the PC; what it prints is the engine's view (`view()`): its `today` (the
time left, the time used, each app's minutes, the bedtime) and its
requests.

**A state just loaded has made no decision yet**, so the view alone cannot
say whether the phone is locked now. `status` asks the engine one more
question, on a copy that stays on this PC: what it decides at the phone's
own clock now (read with `date`), with the screen off. (On Android 8 to
10, AtomicFile writes the new state over the file itself, with the old one
kept aside, so a read in that instant may find half of one, which the
engine calls damaged: run `status` again. Android 11 and later write it
beside and then move it.) The facts it gives
say a start of the phone that never was, so the step since the kept moment
counts nothing; the screen is off, so nothing is counted, covered or warned
of; and automatic time is off, so a correction the phone keeps for a clock
set by hand is kept. The lock, its reason and until when come from that
decision; the wrong tries from the report the engine would send its master
(`outgoing_report`). Nothing goes back to the phone. The phone's own answer
can differ by what it counted since it last kept its state, at most a few
seconds while it is in use. When the phone's clock cannot be read, this
PC's stands in, and `status` says so.

What LiMo last saw switched off is said in words, in the order the phone's
setup asks for them (notifications, phone, usage access, accessibility,
battery, protection), never by the engine's own names:

```
Switched off on the phone when LiMo last looked: usage access, accessibility.
```

and when nothing was, or LiMo has not looked yet, `Nothing LiMo needs was
switched off when it last looked.` The words come from one place,
`status::permission_words`, which names every permission with no catch
all, so a permission the engine gains does not build until it has its
words there.

## What LiMo's debug build must do

This section fixes it; the tool and its fake phone keep to it, and
so does the app's side (`dev/DevReceiver` and `dev/DevPairing` in the child
module's `src/debug`, the host's piece, read when this was written):

- The pairing file, `files/limo-dev/pairing.json`, is one strict JSON object
  with four fields, all required: `welcome` (the core's `Grant`, a JSON
  object), `code_secret_hex` (64 hex digits, 32 bytes), `made_at` (the
  PC's unix seconds, a JSON whole number) and `child_name` (a JSON string,
  1 to 40 characters).
- On `ai.lucidloom.limo.child.dev.PAIR`: read the pairing file and delete it
  at once, whatever it held; refuse a file whose `made_at` is more than an
  hour from the phone's clock, and one without `child_name`; hand `welcome`
  as JSON bytes, `code_secret_hex` as its 32 bytes, and the phone's wall
  clock to `PhoneEngine.paired`; keep `save()` in `files/limo/state` with
  Android's AtomicFile; keep the child's name beside it, never inside the
  engine's state; start counting. The fake phone does the same with the
  real engine.
- On `ai.lucidloom.limo.child.dev.FORGET`: delete `files/limo/state`.
- Answer within 15 seconds: the tool reads back that the file is gone and
  the state holds the paired phone, or that the state is gone.
- Say what was done in the log, tag `LiMo`, in a word: `dev pair taken`,
  `dev pair no_file`, `dev pair refused why=<word>` (`not_json`,
  `no_welcome`, `no_code_secret`, `no_made_at`, `no_child_name`,
  `old_file`, `code_secret_not_hex`, `code_secret_not_32_bytes`,
  `unreadable`, `not_deleted`, `engine`), `dev forget done`; never what the
  file held. `setup` reads the word when LiMo did not take the file, and
  `log` shows it.
- The broadcast names the receiver, so it reaches it whatever its intent
  filter says.

## What only a real phone can prove

- That `adb shell -T` on Windows carries standard input to `run-as` on the
  phone and ends it, so that `cat` sees the end of the file; the size check
  catches a file cut short.
- That `run-as` makes `files/limo-dev` in a way LiMo can read and delete
  (its owner and its SELinux label), on an app that has never run.
- That an explicit broadcast with `--include-stopped-packages` reaches the
  debug receiver of an app installed and never opened, on the owner's phone
  and its maker's Android, and how long LiMo takes to answer.
- That the debug child form, built with the debug receiver, takes the file
  as above: the receiver is another piece.
- That `date +%z` gives the zone on the phone's Android (toybox `date`).
- That the phone's `logcat` takes `-t` as unix seconds (`<secs>.000`). If it
  does not, `setup` finds no word in the log and says the receiver may be
  missing; `log` still shows what LiMo said.
- What the owner's phone prints for each failure: a phone not allowed, a
  release build refused by `run-as`, an install refused.
- That the PIN prompt hides what is typed in the owner's terminal
  (Windows Terminal and the old console both).

## Tests

`cargo test` runs three groups, with no phone and no `adb`: `adb` is behind
a small trait, and the stories play a phone with a fake.

| Where | What | Count |
| --- | --- | --- |
| `src/*.rs` | The adb line as typed; a reply's parts; looking for adb where it should be; the command line, each command and each refusal; the help saying first what the tool never does; devices of every state, one picked or each problem said; `run-as` telling a debug build from a release build; the phone's clock and zone; `dumpsys` and the install and uninstall answers; only LiMo's two log tags; no command carrying anything but names; bedtimes and package names; the test family's rules with the whole day away; the folder named for testing; a log line with the time in UTC and no control characters; times on the phone's clock; zones said; a child's name trimmed, its hidden characters dropped, 1 to 40 long; LiMo's own word about a pairing file read from its log, and the advice for each; permissions in words, in setup's order; numbers read from an APK, never past the end (and one ignored test that reads a real APK, below) | 26 |
| `tests/devtool/` | The stories, against the fake phone (below), with the real hub on a real file in a temporary folder and the real phone engine on the phone's side | 47 |
| `tests/house_rules.rs` | No unsafe code; no allow attributes; the service's folder and file never named; nothing sent to the phone that could change anything but LiMo (every command checked: no settings, accounts, device policy, permissions, other apps or rebooting); nothing in the tool's own code printed by its Debug form; no dash punctuation in anything a person reads or in anything printed | 7 |

The fake phone answers exactly the calls the tool is meant to make, written
out letter for letter, and fails a test on any other, so the stories also
pin down what crosses the cable. It plays a phone attached, none, two, one
not allowed, offline, unplugged part way, one whose call never ends, an
Android too old, LiMo missing, a release build, `run-as` broken by its
maker, LiMo a device admin, an install refused, a clock far from the PC's,
an app that refuses every pairing file with a given word. Its debug
receiver does what the section above says, as the app's own does, with the
real engine, the child's name kept beside the state, and its words in its
log. The stories:

- `phones.rs` (12): each phone it cannot work with (none, two, not
  allowed, offline), and `adb` missing, for every command, each with its
  message, nothing sent to the phone, no file made, no PIN asked for;
  `forget` with no phone pointing at `--this-pc-only`; a release build,
  LiMo missing, `run-as` broken by the maker, an Android too old; unplugged
  part way; a call stopped at its timeout; every call with a timeout.
- `check.rs` (3): each fact said and nothing changed; what each kind of
  phone needs next; this PC's test family told from another, a damaged
  state, a pairing file not taken.
- `setup.rs` (10): the pairing file's exact bytes (one JSON object, its
  four fields in order, the secret in lowercase hex, the welcome the test
  family's, with the whole day away and the PIN's record, never the PIN,
  and the child's name), that it is the only call with input and where it
  goes, the broadcast next, reading back, the state on the phone holding
  the test phone with its 20 minutes and the name kept beside it, not in
  it, the PIN working there, the log; the child's name cleaned once, the
  same on the PC, in the file and on the phone, written as a JSON string
  with its quotes escaped, forty characters of any script taken, too long
  or empty refused before anything; a pairing file LiMo refuses
  (`no_child_name`, `old_file`, `no_welcome`, `engine`) explained from its
  log, read once for its tag since the broadcast, both sides undone;
  `setup` twice, the second refused with nothing asked of the phone and
  nothing changed; PINs that differ, are too short, are not digits or
  cannot be typed; a phone without the receiver, its log silent, undone on
  both sides, and `setup` working after; a phone holding a family already;
  a zone that cannot be read; clocks two hours apart refused, ten minutes
  apart said; the rules in another zone, with no time at all.
- `code_and_status.rs` (7): a code that needs no phone, works once on the
  phone's own engine, is printed once and never logged; ten a day;
  `status` from a state the real engine kept, through a day of use, to the
  lock with its reason and until when; wrong tries and a request that waits,
  with no PIN, code or reason printed; what is switched off, as the real
  engine kept it: nothing before LiMo has looked, nothing with all on, all
  six in setup's order, and each one alone in its words; no state, another
  family's, a damaged one; the phone's clock unreadable.
- `forget.rs` (8): `forget` wiping the phone and this PC, and `setup` again;
  another family's state left alone, and a damaged one; `--this-pc-only`;
  LiMo gone; a pairing file never taken; a release build; LiMo not
  wiping; `uninstall` refused while LiMo is a device admin, and twice;
  `log` showing only LiMo's lines and stopping at its limit.
- `secrets.rs` (2): a whole session with refused PINs, setup, three codes,
  status, check, log, forget and uninstall: the PIN, the refused PINs and
  the code secret never printed, logged or in any argument; each code
  printed once, on its own line, never logged; only one call with input.
  A code's and a PIN's `Debug` hide them.
- `apk.rs` (5): the package, the version and the debug flag read from a
  compiled manifest, in UTF-16 and UTF-8, stored and deflated; what is not
  an APK; `install` refusing the guardian form, another app, a release
  build, a file that is not an APK and one that is not there, with nothing
  sent; installing, and again over LiMo with its files kept; Android
  refusing, with what to do; an Android too old.

`LIMO_TEST_APK=path cargo test real_apk -- --ignored` reads a real APK and
prints what it says. It read the debug APKs of spikes 2 and 5 as debug
builds, and `phone/app/out`'s two release builds as the child form and the
guardian form, release builds.

`cargo build`, `cargo test`, `cargo doc --no-deps` and
`cargo clippy --all-targets -- -D warnings` pass with no warnings and no
allow attributes. Dependencies are built optimised in debug and test builds,
as in the hub and the engine, so the slow PIN hash stays quick.

## Dependencies and licences

| Crate | Version | Licence | Used for |
| --- | --- | --- | --- |
| limo-core, limo-hub, limo-phone | 0.1.0 (path) | Apache-2.0 | the rules and the welcome grant; the test family's file; the phone's engine, reading what the phone kept |
| serde, serde_json | 1.0.229, 1.0.154 | MIT or Apache-2.0 | the pairing file |
| zeroize | 1.9.2 | Apache-2.0 or MIT | wiping the typed PIN, the pairing file's bytes and the state read back |
| console | 0.16.6 | MIT | the PIN typed at a prompt that does not show it, with no unsafe code here; without its default features |
| encode_unicode | 1.0.0 | Apache-2.0 or MIT | used by console on Windows |
| windows-sys, windows-link | 0.61.2, 0.2.1 | MIT or Apache-2.0 | used by console on Windows (and already under the hub's tests) |
| libc | 0.2.191 | MIT or Apache-2.0 | used by console on Linux and macOS |
| miniz_oxide | 0.8.9 | MIT or Zlib or Apache-2.0 | inflating the manifest inside an APK |
| adler2 | 2.0.1 | 0BSD or MIT or Apache-2.0 | used by miniz_oxide |
| getrandom | 0.4.3 | MIT or Apache-2.0 | the test family's random password (already under the hub) |
| tempfile | 3.27.0 | MIT or Apache-2.0 | tests only: a folder for the test family |

Everything else under these is listed, with its licence, in the hub's, the
core's and the engine's READMEs; `Cargo.lock` holds the same versions as
theirs. Nothing is GPL or AGPL. The code here was written for LiMo; the way
it runs `adb` (finding it, one phone only, timeouts, starting the server
with no pipes) is ported from LiMo's own fifth proof
([`../../spikes/05-strict-mode/pc`](../../spikes/05-strict-mode/pc/)). The
crate is under the Apache License 2.0, as is all of LiMo.

## Layout

```
phone/devtool/
  Cargo.toml, Cargo.lock, README.md, .gitignore
  src/
    main.rs       limo-phone-dev.exe
    lib.rs        the crate's notes and its modules; the real tool
    cli.rs        the command line and the help
    commands.rs   the eight commands, against the Adb and Ui traits
    adb.rs        the Adb trait; the real adb, every call through one function with a timeout
    phone.rs      every call to the phone, the shell commands, and what they answer
    family.rs     the test family on this PC: made, read, codes, deleted
    pairing.rs    the pairing file's bytes
    status.rs     what the phone holds, read by its own engine
    apk.rs        what an APK says it is: its package, version and debug flag
    places.rs     where the test family lives, and the tool's log
    ui.rs         the terminal, and the PIN typed without it showing
  tests/
    house_rules.rs
    devtool/      main.rs, fake.rs (the fake phone and terminal), phones.rs,
                  check.rs, setup.rs, code_and_status.rs, forget.rs,
                  secrets.rs, apk.rs
```
