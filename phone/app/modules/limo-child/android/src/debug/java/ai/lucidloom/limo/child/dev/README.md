# dev

A test family over the USB cable, with no family PC: the app's side of
the development tool on the PC (`phone/devtool`), which pairs a debug build
without the link (a phone paired this way has no kept pairing, so its link
opens no session; the family's PC pairs a phone through the cable door,
link/README.md); and the probe that
learns the real names of the pages LiMo guards. **Debug builds only**:
everything here is in `src/debug`, which a release build never compiles,
and `npm run inspect` checks that the release APK holds neither the
receiver, nor its class, nor anything of the probe.

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
adb shell am broadcast -a ai.lucidloom.limo.child.dev.PROBE  -n ai.lucidloom.limo.child/ai.lucidloom.limo.child.dev.DevReceiver --ez on true
```

| Action | What the app does |
| --- | --- |
| `ai.lucidloom.limo.child.dev.PAIR` | Reads the pairing file and deletes it at once, whatever it held (to try again, push it again: it is never read twice). Makes the engine from it through the host (`PhoneHost.pair`, with the child's name), which keeps the new state before anything else, then the name, replacing any state and name kept before; fills the welcome and the secret with zeros; starts the keeper. |
| `ai.lucidloom.limo.child.dev.FORGET` | Deletes the kept state (`files/limo/state`) and the child's name (`files/limo/child.json`), takes any cover away, stops the keeper. |
| `ai.lucidloom.limo.child.dev.PROBE`, with the boolean extra `on` | Switches the page probe on (true) or off (false) (below); switched on, it first logs the package manager's answers the watch holds for the guard. With no extra, off. |

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
| `dev probe on`, `dev probe off` | The page probe was switched. |

## The page probe

The guard covers pages that could switch LiMo off or remove it
(watch/README.md, "The guard"). Inside Android's Settings, the only thing
that tells such a page apart is its class: the class of its window, and
the class of the activity Android's usage record shows resumed. The watch
knows the device admin's page by the package manager's answer on the phone
(the activity that handles `ACTION_ADD_DEVICE_ADMIN`) and by a fixed list
read in Android's source (watch/Guarded.kt). The probe is how a real phone
is checked against both, once.

**What it does.** Switched on, it first logs the package manager's answers
the watch holds for its three intents (`device_admin`, `accessibility`,
`settings`), tag `LiMoWatch`: `opens=` the activity each opens, as its
package, its class, an alias's `target=` and whether it is preinstalled,
then a `handler=` line for every activity that handles it (`unread` for an
intent never answered). Then, while it is on, each window event of the
packages the guard reads windows of (Settings, the installer and the
package the answer names for the device admin's page) logs one line, and
each resumed activity in Android's usage record of the packages it reads
that record of (Settings and that package) one line, once for each class
since the probe was switched on:

```
probe answered intent=device_admin opens=com.android.settings/com.android.settings.applications.specialaccess.deviceadmin.DeviceAdminAdd preinstalled
probe window package=com.android.settings class=com.android.settings.applications.specialaccess.deviceadmin.DeviceAdminAdd
probe resumed package=com.android.settings class=com.android.settings.applications.specialaccess.deviceadmin.DeviceAdminAdd
```

It sees exactly what a release build reads and matches, nothing more:
never another app's class name, never any text or anything on the screen.
The watch's `WindowPage` hands the names over in a debug build only:
`src/debug/watch` holds the debug copy, which calls `PageProbe.window` and
`PageProbe.resumed` and then matches as a release build does, and
`src/release/java` the release copy, which only matches (the module's
`build.gradle` adds `src/debug/watch` to the debug source set); the
receiver asks the watch for its answers when the probe is switched on. A
class name is cut at 200 characters and keeps only letters, digits, dots,
underscores and dollar signs.

**Off by default**, and off again whenever LiMo's process starts: the
switch is held in memory only. The watch's own lines (`guard now ...`,
`front now ...`) go on as in any build; they name a page's kind and a
package, never a class.

**For the next run on the phone**, a person at the PC, with the debug
build installed and paired (phone/TESTING.md, parts C and D), and setup
finished on the phone:

1. In one terminal, the log: `adb logcat -s LiMo:V LiMoWatch:V`.
2. In another, the probe on; the log says `probe on`, `dev probe on`, and
   the `probe answered` lines:

   ```
   adb shell am broadcast -a ai.lucidloom.limo.child.dev.PROBE -n ai.lucidloom.limo.child/ai.lucidloom.limo.child.dev.DevReceiver --ez on true
   ```

3. With the phone open (no lock on it, time left), walk each page once,
   slowly, a few seconds on each, and say aloud which page it is, so that
   the log can be matched to it afterwards: Settings' first page; Security
   and privacy, Other security settings, Device admin apps; LiMo's row
   there (the device admin's page); its Deactivate, then Cancel; Back to
   the list; Apps, LiMo (App info); its Uninstall (on Android 10 to 15 it
   opens the device admin's page, ready to switch it off and remove LiMo),
   then Back; its Force stop, and Storage with Clear storage (note whether
   each is greyed out; tap neither); Accessibility (One UI opens a package
   of its own); from the home screen, a long press on LiMo's icon and
   Uninstall (the installer's question), then Cancel. Where the guard
   covers a page, that is LiMo working: type the PIN, which opens these
   settings for five minutes, and carry on.
4. The probe off:

   ```
   adb shell am broadcast -a ai.lucidloom.limo.child.dev.PROBE -n ai.lucidloom.limo.child/ai.lucidloom.limo.child.dev.DevReceiver --ez on false
   ```

5. Copy the log. The `probe answered` lines give what the package manager
   named; each `probe window` and `probe resumed` line a page's real
   package and class; each `front now app package=` line the package in
   front, the installer's among them; each `guard now` line what was
   guarded, and how. The device admin's page, reached from its row in the
   list and from App info's Uninstall, should show the class the answer
   named for `device_admin`, in its window and in its resume. A way to it
   that shows another class, a page that switches LiMo off whose class is
   neither the answer's nor in watch/Guarded.kt, or the installer under
   another package than the two in watch/Guarded.kt, is what to add.

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
40 characters). `PageProbeTest`: off until switched on; then the windows
of Settings, the installer and a maker's package the answer named, and
never another app's, whatever its class; resumed activities of Settings
and that package only, each once, and once more after the probe is
switched on again; the package manager's answers as the watch holds them;
a name cut and kept to what a class name can hold; nothing once off. The
debug copy of the watch's `WindowPage` is not compiled into
those tests (only `src/debug/java` is); the watch's house rules check its
text (watch/README.md, "Tests").

## Layout

```
src/debug/
  AndroidManifest.xml    the receiver, guarded by DUMP
  java/ai/lucidloom/limo/child/dev/
    README.md
    DevReceiver.kt       the three broadcasts
    DevPairing.kt        the pairing file, taken once, and a small strict JSON reader for it
    PageProbe.kt         the page probe: its switch, the package manager's answers, and a line for each window and resumed activity the guard reads
  watch/ai/lucidloom/limo/child/watch/
    WindowPage.kt        the watch's debug copy: hands the class name to the probe, then matches it
src/test/java/ai/lucidloom/limo/child/dev/DevPairingTest.kt, PageProbeTest.kt
```
