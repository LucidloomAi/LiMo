# dev

A test family over the USB cable, before the link exists: the app's side
of the development tool on the PC (`phone/devtool`). **Debug builds only**:
everything here is in `src/debug`, which a release build never compiles,
and `npm run inspect` checks that the release APK holds neither the
receiver nor its class.

## How the tool pairs the phone

1. The tool makes a test family on the PC (a child, rules, a PIN) and a
   welcome for this phone, as the master would.
2. It writes the pairing file into the app's private files through
   `run-as` (which works only on a debuggable build):
   `files/limo-dev/pairing.json`.
3. It sends a broadcast to `DevReceiver`.

```
{ "welcome": { ...the core's Grant... }, "code_secret_hex": "<64 hex digits>", "made_at": 1795000000, "child_name": "Asha" }
```

| Field | What |
| --- | --- |
| `welcome` | The master's welcome grant, a JSON object, handed to the engine as the very text it is in the file. |
| `code_secret_hex` | The phone's 32 byte code secret, as 64 hex digits. |
| `made_at` | The PC's clock when it wrote the file, unix seconds. A file further than an hour from the phone's clock is refused: left over, not just pushed. |
| `child_name` | The child's name, for the child's screens and the lock: the welcome grant holds no name. Cleaned as the host keeps it (control characters and direction marks dropped, trimmed, cut at 40 characters, as the tool cleans it too); a file without it, or with one that is not text or is nothing once cleaned, is refused (`no_child_name`). The host keeps it beside the engine's state (host/README.md, "The child's name"). |

## The broadcasts

```
adb shell am broadcast -a ai.lucidloom.limo.child.dev.PAIR   -n ai.lucidloom.limo.child/ai.lucidloom.limo.child.dev.DevReceiver
adb shell am broadcast -a ai.lucidloom.limo.child.dev.FORGET -n ai.lucidloom.limo.child/ai.lucidloom.limo.child.dev.DevReceiver
```

| Action | What the app does |
| --- | --- |
| `ai.lucidloom.limo.child.dev.PAIR` | Reads the pairing file and deletes it at once, whatever it held (to try again, push it again: it is never read twice). Makes the engine from it through the host (`PhoneHost.pair`, with the child's name), which keeps the new state before anything else, then the name, replacing any state and name kept before; fills the welcome and the secret with zeros; starts the keeper. |
| `ai.lucidloom.limo.child.dev.FORGET` | Deletes the kept state (`files/limo/state`) and the child's name (`files/limo/child.json`), takes any cover away, stops the keeper. |

The receiver is exported so that adb can reach it, and guarded by
`android.permission.DUMP`, which only the adb shell and the system hold: no
app on the phone can send to it.

What happened is in the log (`adb logcat -s LiMo`), in a word, never what
the file held:

| Line | When |
| --- | --- |
| `dev pair taken` | Paired; the state is kept. |
| `dev pair no_file` | No pairing file was there. |
| `dev pair refused why=<word>` | `not_json`, `no_welcome`, `no_code_secret`, `no_made_at`, `old_file`, `no_child_name`, `code_secret_not_hex`, `code_secret_not_32_bytes`, `unreadable`, `not_deleted`, or `engine` (the engine refused the welcome or the secret, or the state could not be written). |
| `dev forget done` | The state was deleted. |

## The debug build

`npm run build:child:debug` (in the app) makes `out/limo-child-debug.apk`,
beside the release files: the child form as a debug build, debuggable (so
`run-as` works) and with this receiver. `scripts/debug-bundle.gradle` makes
it carry its JavaScript inside, as a release build does, so it shows its
screens without a development server (which it could not reach: LiMo holds
no INTERNET). It has the same package name as the release build, so one
replaces the other. Never give a debug build to a family: anyone with a
cable can change what it keeps.

## Tests

`npm run test:child` compiles this folder into the release unit tests too
(the module's `build.gradle`, source set `testRelease`), so
`DevPairingTest` runs with them: a pairing file as the tool writes it,
taken once and deleted, pairing the phone on the real engine through the
host, both secrets wiped, and a second broadcast finding no file and
changing nothing, and the child's name kept by the host; fourteen files
that are not right, each refused by its word, deleted, and never quoted,
among them one with no name, a name that is not text, and names that are
nothing once cleaned; strings with escapes and a nested welcome read as
the very text they are; a name cleaned (a direction mark dropped, cut at
40 characters).

## Layout

```
src/debug/
  AndroidManifest.xml    the receiver, guarded by DUMP
  java/ai/lucidloom/limo/child/dev/
    README.md
    DevReceiver.kt       the two broadcasts
    DevPairing.kt        the pairing file, taken once, and a small strict JSON reader for it
src/test/java/ai/lucidloom/limo/child/dev/DevPairingTest.kt
```
