# LiMo spike 06: the guardian console in a Tauri window

A small, throwaway proof for the LiMo screen time limiter. It is not the real
product. It answers one question with measurements:

Can the guardian console on the master PC be the Expo app's own screens,
exported for the web and shown in a Tauri v2 window on Windows, calling Rust,
with nothing fetched from the network at run time? And what does that cost?

## The answer

Yes. The same two Expo Router screens run in a normal browser (with a mock),
as a plain web export, and inside a Tauri v2 window where every figure on the
screen comes from Rust. Inside Tauri the page renders, navigates, survives a
reload on a deep route, and the buttons change Rust's state. A warm start
draws the first screen with Rust's data about 0.32 s after the process
starts. The exe is 4.7 MiB, the window and its WebView2 processes use about
185 MB of private memory while open. The page fetched nothing, but the
WebView2 runtime itself opened one HTTPS connection to a Microsoft address
each time it was watched (see "Network").

## What it proves, and what it does not

It proves, on this Windows 11 PC, without anyone watching (`--self-check`):

1. the Expo SDK 55 web export (Expo Router, `single` output) renders inside
   Tauri from files bundled into the exe, with no dev server and no remote
   URL;
2. the screens reach Rust only through one function, `host.request`, which
   uses Tauri's `invoke` inside the window and a mock elsewhere;
3. "Give 15 minutes" and "Lock now" change state held by Rust, and the screen
   shows Rust's new answer;
4. navigation to `/child/c1` works, and a reload on that route under Tauri's
   own address (`http://tauri.localhost/child/c1`) comes back on the Child
   screen with Rust's state intact;
5. the content security policy blocks nothing the app needs (0 violations),
   and a Tauri command the window was not granted is refused by the ACL;
6. the same export served as plain files over `http://127.0.0.1:<port>`
   answers the index and its script.

It does not:

- build for a phone or run the screens on one. The source has nothing web
  only in the screens, and the host falls back to the mock when Tauri is
  absent, but a phone run was not asked for;
- talk to a Windows service. The Rust side holds canned data in memory and
  uses neither `LiMo\core` nor `LiMo\hub`;
- test a real cold start (after a reboot), Windows 10, a PC without WebView2,
  an installer, or the window in normal mode by hand (normal mode was built
  but not opened; the self check runs the same window and code);
- design anything. System fonts, default spacing.

## Layout

```
06-console-window/
  app/                            the Expo app (TypeScript)
    package.json, package-lock.json, app.json, tsconfig.json
    app/_layout.tsx               Expo Router stack; starts the self check if asked
    app/index.tsx                 "Today": two children, time used and left, "Give 15 minutes"
    app/child/[id].tsx            "Child": devices, "Lock now", Back
    src/host.ts                   THE seam: host.request(name, args), and the mock
    src/types.ts                  the shapes Rust and the mock answer with
    src/selfCheck.ts              self check driver (runs only when Rust asks for it)
    src-tauri/                    the Tauri v2 shell (Rust)
      Cargo.toml, Cargo.lock, build.rs, tauri.conf.json
      capabilities/default.json   the window may call the 3 console commands, nothing else
      runtime-capabilities/self-check.json
                                  added at run time, only with --self-check
      icons/icon.ico              made by tools/make-icon.mjs
      src/main.rs                 commands, window, arguments
      src/family.rs               canned family data and its rules
      src/self_check.rs           the self check log, deadline and exit codes
  tools/
    run-self-check.ps1            runs the self check N times, reads memory, checks for leftovers
    serve-check.mjs               serves the export as plain files and fetches it
    make-icon.mjs                 writes the 32 by 32 icon
  README.md, TESTING.md, .gitignore
```

Ignored and made by the build: `app/node_modules`, `app/dist` (the web
export), `app/src-tauri/target` (Rust output, and the proof's WebView2 data
folder), `logs/` (self check logs and measurements).

## How a press travels

1. "Give 15 minutes" calls `host.request('give_minutes', { childId: 'c1', minutes: 15 })`.
2. `src/host.ts` sees `__TAURI_INTERNALS__` on the global object and calls
   `invoke('give_minutes', ...)` from `@tauri-apps/api/core`.
3. Tauri checks the ACL: the window `main` holds the capability `default`,
   which grants `allow-give-minutes`. It maps `childId` to `child_id`.
4. `give_minutes` in `src-tauri/src/main.rs` locks the in-memory `Family`,
   adds 15 minutes, and returns the whole family view as JSON.
5. The screen shows the answer. On this PC that took 11 to 12 ms from the
   click to the new figure in the DOM.

**How the running place is detected.** Tauri v2 puts `__TAURI_INTERNALS__` on
the page's global object before any of the page's scripts run; nothing else
does. Present: Tauri, `invoke`. Absent: the mock in the same file. The screen
says which answered ("Answers from: rust (host tauri)" or "mock (host mock)").
`isTauri()` from `@tauri-apps/api` reads a different global (`isTauri`); the
internals check was chosen because it is what `invoke` itself needs.

## The Tauri shell

- **Bundled files only.** `build.frontendDist` is `../dist`; Tauri embeds the
  export into the exe (brotli compressed) and serves it at
  `http://tauri.localhost/`. No `devUrl`, no remote URL.
- **Window.** Made in code (`main.rs`), 420 by 760, resizable, not maximised,
  not full screen, not always on top, `focused(false)`, titled
  "LiMo console proof". In 14 measured runs the window never became the
  foreground window (sampled every 100 ms).
- **Capabilities.** `build.rs` lists the five app commands in an app
  manifest, so each needs an explicit `allow-<command>` permission. The
  capability `default` grants only `allow-family-view`, `allow-give-minutes`
  and `allow-lock-child`: no `core:default`, no events, no window API. The
  self check proves it: `plugin:app|version` is refused with "Command
  plugin:app|version not allowed by ACL". The two self check commands are
  granted by `runtime-capabilities/self-check.json`, which `main.rs` adds with
  `add_capability` only when started with `--self-check`; it is not in
  `app.security.capabilities`, so it is not loaded otherwise.
- **Content security policy** (`tauri.conf.json`):
  `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline';
  img-src 'self' data:; font-src 'self' data:; connect-src ipc:
  http://ipc.localhost; object-src 'none'; base-uri 'none'; form-action
  'none'; frame-src 'none'`. `'unsafe-inline'` for styles is there because
  React Native Web writes style rules at run time and the export has an inline
  reset `<style>`. No script may come from anywhere but the app. The page
  listens for `securitypolicyviolation` events: 0 on both page loads.
- **Devtools off in release.** The `devtools` Cargo feature is not enabled,
  and Tauri opens devtools only in debug builds without it. Not checked by
  pressing keys.
- **WebView2 data folder.** For the proof it is `webview-data` next to the
  exe, inside the ignored `target` folder, so nothing is written under the
  user's AppData. The product would keep Tauri's default
  (`%LOCALAPPDATA%\<identifier>`). It is 10.8 MB after the runs.

## Expo Router output mode and reload

`app.json` sets `web.output` to `single`: one `index.html` and one script, the
router reads the address on the client. On a reload at
`http://tauri.localhost/child/c1` Tauri's asset resolver looks for
`child/c1`, then `child/c1.html`, then `child/c1/index.html`, then falls back
to `index.html` (tauri 2.12.7, `src/manager/mod.rs`). The router then draws
the Child screen. The self check does exactly this reload and gets the Child
screen back with Rust's state ("Left 90 min", "Locked"), 25 ms page load.

`static` output was exported once for comparison (into `logs/`, not used):
it writes `index.html`, `child/[id].html` (literally, with brackets),
`+not-found.html` and `_sitemap.html`, each pre-rendered. Under Tauri a reload
on `/child/c1` would still fall back to `index.html`, which holds the
pre-rendered Today screen, and React would have to hydrate it as the Child
route. Not run, but there is nothing for the console to gain from it.

## Results on this PC

Windows 11 Home, WebView2 Runtime 157.0.4341.60 (per machine). Rust 1.98.1
(MSVC), Node 22.13.1, npm 11.12.0. Times are from the first line of `main`
(the self check log); the process start before that is a few ms. Other
programs were running.

### Start to first render

From `tools/run-self-check.ps1 -Runs 8 -HoldMs 3000 -FreshProfile`, final exe:

| Run | Window created | Page loaded | First render reported (Rust's data on screen) | All 8 steps done |
| --- | --- | --- | --- | --- |
| 1, new WebView2 data folder | 272 ms | 377 ms | 416 ms | 575 ms |
| 2 to 8, warm | 237 to 253 ms | 277 to 313 ms | 309 to 349 ms, median 318 ms | 405 to 449 ms |

Run 1 starts with no WebView2 data folder, like a first start on a new PC,
but the exe and the WebView2 runtime files were already in the disk cache: it
is not a cold boot. Most of the time is before the window exists (Tauri setup
and starting WebView2's browser process). The page's own script runs about
46 ms after navigation starts.

Clicks, from the click to the new text in the DOM: "Give 15 minutes" 11 to
12 ms, open the Child route 11 to 24 ms, "Lock now" 11 to 12 ms.

### Memory while the window is open

Read during the 3 s hold after the self check finished, over the exe and all
its child processes (7: the exe, and WebView2's browser, GPU, renderer, two
utility and the crash handler processes).

| | Run 1 | Runs 2 to 8 |
| --- | --- | --- |
| Private bytes, all 7 | 194.8 MB | 182.3 to 185.8 MB |
| Working set, all 7 added up | 377.9 MB | 355.8 to 359.3 MB |
| Working set of the exe alone | 22.5 MB | 22.3 to 22.4 MB |

The added up working set counts shared pages (WebView2's DLLs) more than
once, so private bytes is the better figure: about 185 MB while the console
is open, nothing when it is closed.

### Sizes

| What | Size |
| --- | --- |
| `limo-console-proof.exe`, release, no installer | 4,971,008 bytes (4.74 MiB), one file, no DLL beside it |
| Web export `app/dist` | 21 files, 1,122,432 bytes; the script is 1,100,937 bytes (300,026 gzipped) |
| WebView2 data folder after the runs | 10,778,764 bytes |
| `app/node_modules` | 671 packages, 471 MB |
| `app/src-tauri/target` | 1.29 GB |

The release profile uses Tauri's suggested size settings (`lto`,
`codegen-units = 1`, `opt-level = "s"`, `panic = "abort"`, `strip`).

### Build times

| Step | Time |
| --- | --- |
| `npm install` (671 packages) | 31 s |
| `expo export --platform web` | 13 s |
| First `tauri build --no-bundle` (crates fetched before, 15 s) | 1 min 56 s |
| Rebuild after a JavaScript change (export and cargo) | 76 s |
| Rebuild after a Rust change | 63 to 65 s |

A JavaScript change rebuilds the Rust crate too, because the export is
embedded in it, and LTO with one codegen unit relinks everything. Daily work
would use `expo start --web` with the mock, or a dev profile without LTO.

### Plain files over 127.0.0.1

`node tools/serve-check.mjs` serves `app/dist` with Node's `http` module, no
fallback:

```
GET / -> 200 text/html; charset=utf-8 1185 bytes
GET /_expo/static/js/web/entry-<hash>.js -> 200 text/javascript; charset=utf-8 1100798 bytes
script looks like the app: true
GET /child/c1 -> 404 text/plain 9 bytes
```

So the fallback "the service serves the pages to the PC's own browser" stays
open: the export is ordinary static files with absolute `/_expo/...` paths.
The server must answer unknown paths with `index.html`, as Tauri does, or a
reload on a deep link gives 404. The browser would have the mock host unless
`host.ts` learns a third place (HTTP to the service).

### In a normal browser

`expo start` (port 8099, `BROWSER=none`) was run once and the page opened in
a browser: "Answers from: mock (host mock)", "Give 15 minutes" changed Left
75 to 90 min, the Child route opened, "Lock now" showed Locked, Back showed
Locked on Today. The dev server was stopped afterwards and its port checked
free.

### Network

The page fetched nothing: the CSP allows only the app's own files and the IPC
address, there were 0 violations, and the only addresses in the bundle are in
strings (error messages, an SVG namespace). But in two runs watched with
`Get-NetTCPConnection`, WebView2's browser process opened one HTTPS
connection to a Microsoft address (40.99.10.114:443 in one run,
52.98.40.34:443 in the other) about 2.8 s after start. What it carries is not
known. The WebView2 licence says "The software may collect information about
you and your end users' use of the software, and send that to Microsoft."
(section 3(a)). Tauri's default WebView2 arguments already include
`--disable-features=...,msSmartScreenProtection` (seen on the WebView2
processes' command lines). For a product that promises no server, this needs
its own look before release.

## WebView2

- **On this PC:** present, per machine, version 157.0.4341.60, read from
  `HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}`
  value `pv` (read only, nothing changed).
- **How an installer tells** (Microsoft, "Distribute your app and the
  WebView2 Runtime",
  https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution):
  read `pv` (REG_SZ) under
  `HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}`
  (per machine, 64 bit Windows; without `WOW6432Node` on 32 bit) and
  `HKCU\Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}`
  (per user). "If neither regkey exists, or if only one of these regkeys
  exists but its value is `null`, an empty string, or 0.0.0.0, this means
  that the WebView2 Runtime isn't installed on the client."
- **Windows 11:** "The Evergreen Runtime is preinstalled onto all Windows 11
  devices as a part of the Windows 11 operating system."
  (https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/evergreen-vs-fixed-version)
- **Windows 10:** "The vast majority of Windows 10 devices have the WebView2
  Runtime installed already ... A small number of Windows 10 devices don't
  have the WebView2 Runtime installed." So the installer must check.
- **Evergreen Bootstrapper:** "a tiny (approximately 2 MB) installer. The
  WebView2 Runtime Bootstrapper downloads and installs the Evergreen Runtime
  from Microsoft servers". Needs the internet at install time. Kept up to
  date by Microsoft afterwards.
- **Evergreen Standalone Installer:** "a full installer that installs the
  Evergreen WebView2 Runtime in offline environments". Microsoft gives no
  size on the pages read; Tauri says about 127 MB.
- **Fixed Version:** a chosen runtime version packaged with the app, used
  only by it, never updated by Microsoft: "you must periodically update your
  app together with the updated WebView2 Runtime". Microsoft: "over 250 MB";
  Tauri: "around 180MB" added to the installer. The app points at it with the
  `browserExecutableFolder` argument (Tauri: `webviewInstallMode` type
  `fixedRuntime` with a `path`).
- **Tauri's installer options** (https://v2.tauri.app/distribute/windows-installer/):
  `downloadBootstrapper` (default, 0 MB, needs internet), `embedBootstrapper`
  (about 1.8 MB, needs internet), `offlineInstaller` (about 127 MB, offline),
  `fixedVersion` (about 180 MB, offline), `skip`.
- **Licence:** the WebView2 Runtime is Microsoft's software and is not open
  source. The terms quoted here are from "MICROSOFT SOFTWARE LICENSE TERMS,
  MICROSOFT EDGE WEBVIEW2 RUNTIME"
  (https://developer.microsoft.com/microsoft-edge/api/eula/webview2), which
  is UNCONFIRMED as the governing redistribution terms: no WebView2 docs or
  download page linked to it, it was found by web search, and the quotes came
  through a text fetch in short pieces. Microsoft's docs say only "The
  WebView2 Runtime is a redistributable runtime". The page says: "The
  software is licensed, not sold." Section 2,
  Distributable Code: "You may copy and distribute the object code of the
  software", with your applications, if you "add significant primary
  functionality to it in your applications", "require distributors and
  external end users to agree to terms that protect it and Microsoft at least
  as much as this agreement", "acquire all code ... directly from Microsoft",
  and indemnify Microsoft. You may not "provide any Microsoft download sites
  or shortcuts that provides access to this software to a third party".
  Section 9(a): unless SmartScreen is disabled, you "must provide notice to
  all users that your software includes Microsoft Defender SmartScreen and
  collects and sends the end user's information to Microsoft". The terms
  speak of "the software" only and do not name the three installers
  separately; Microsoft's docs tell apps to package each of them.

Unconfirmed: that this EULA page is the one Microsoft's download buttons
show (the download page did not expose its link to the reader); a Microsoft
size for the Standalone Installer; why Microsoft says 250 MB and Tauri 180 MB
for Fixed Version; whether Tauri's `msSmartScreenProtection` switch counts as
"disable SmartScreen as described in Section 3(b)"; whether the "download
sites or shortcuts" clause allows an installer that fetches the bootstrapper
from Microsoft's link (the shipped file options avoid the question). Not
legal advice; read the terms before release.

## Build

Needs, on Windows: Rust stable with the MSVC toolchain (and the Visual Studio
C++ build tools it uses), Node 22, and the WebView2 Runtime to run it. No
global tools: the Tauri command line tool is a dev dependency.

From this spike folder, in PowerShell:

```
cd app
$env:EXPO_NO_TELEMETRY = "1"
npm ci
npm run build:console
```

`build:console` runs `expo export --platform web --output-dir dist --clear`
and then `tauri build --no-bundle`. The exe is
`app\src-tauri\target\release\limo-console-proof.exe`. No installer is made,
so nothing downloads NSIS or WiX. `npm run typecheck` runs `tsc --noEmit`.

## Self check

From this spike folder:

```
app\src-tauri\target\release\limo-console-proof.exe --self-check logs\self-check.log
```

The exe has no console, so PowerShell returns at once; to see the exit code:

```
$p = Start-Process app\src-tauri\target\release\limo-console-proof.exe -ArgumentList '--self-check','logs\self-check.log' -PassThru -Wait
$p.ExitCode
```

The window opens, the page reports eight steps (`js_start`, `today_rendered`,
`rust_answer`, `give_15`, `child_route`, `lock_now`, `reload_requested`,
`after_reload`), Rust writes each with the milliseconds since start, writes
`done` and exits with 0. If a step is missing after 20 s it writes
`timeout: missing after 20 s: <steps>` and exits with 1. If the window is
closed early it writes `closed: ...` and exits with 1. `--hold-ms N` keeps the
window open N ms after `done` (for reading memory) and is refused without
`--self-check`. Both failure paths were tried once with a temporary extra
step that the page never reports (exit 1 after 20,000 ms; and exit 1 after
closing the window), then the step was removed and the exe rebuilt.

Measurements: `powershell -ExecutionPolicy Bypass -File tools\run-self-check.ps1 -Runs 8 -HoldMs 3000 -FreshProfile`.

**Why the self check cannot drive the app in normal mode.** Three locks: the
page's driver starts only if Rust put `__LIMO_SELF_CHECK__` on the page, which
it does only with `--self-check`; the window is granted the two self check
commands only by a capability added at run time with `--self-check`; and the
commands return "not in self check mode" without a `SelfCheck`. In normal
mode the driver code is in the bundle but returns before its first call.

## What this means for the product

- **Works:** one set of Expo screens can be the phone's guardian screens and
  the PC's console. The console is a 4.7 MiB Rust exe; the screens call
  `host.request`, and only `host.ts` knows whether Rust, a mock, or later a
  phone transport answers.
- **Costs:** about 185 MB of private memory while the console is open; a
  WebView2 dependency (present on Windows 11 and most Windows 10 PCs, so the
  installer checks the registry and ships the bootstrapper, about 2 MB, or the
  standalone installer, about 127 MB, for offline PCs); a proprietary
  Microsoft runtime with licence duties (terms for end users, notices); and
  the runtime's own call to Microsoft at start, still to be explained.
- **Does not work as is:** a plain static server without an `index.html`
  fallback breaks reloads on deep links; browser accelerator keys and the
  default WebView2 context menu were not looked at (a guardian could press F5,
  which is harmless here, or open a print dialog).
- **Reaching the Windows service** (not built): the console runs in the
  guardian's desktop session and the service in session 0, so the console's
  Rust commands would forward each `host.request` to the service over a local
  named pipe whose ACL allows only the console's user, and hold no state of
  their own. The page never learns the pipe exists.

## Tools and libraries

What ships in the exe or the page:

| What | Version | Licence | Where |
| --- | --- | --- | --- |
| tauri | 2.12.7 | Apache-2.0 OR MIT | exe |
| tauri-runtime-wry, wry | 2.12.5, 0.57.2 | Apache-2.0 OR MIT | exe |
| tao | 0.37.2 | Apache-2.0 | exe |
| webview2-com, webview2-com-sys | 0.39.1 | MIT | exe (WebView2 bindings; no loader DLL beside the exe) |
| windows crates | 0.62.x | MIT OR Apache-2.0 | exe |
| serde, serde_json | 1.x | MIT OR Apache-2.0 | exe |
| brotli, brotli-decompressor, alloc-stdlib, alloc-no-stdlib | 9.1.3, 6.3.2, ... | BSD-3-Clause (and MIT) | exe, unpacks the embedded export |
| ICU4X crates (icu_*, zerovec, yoke, tinystr, ...) via url and idna | 2.4.0 and others | Unicode-3.0 | exe |
| option-ext (via dirs) | 0.2.0 | **MPL-2.0** (file level copyleft: its source must stay available, it is on crates.io) | exe |
| memchr, aho-corasick, walkdir, same-file, winapi-util | | Unlicense OR MIT | exe |
| dunce | 1.0.5 | CC0-1.0 OR MIT-0 OR Apache-2.0 | exe |
| every other crate in the exe (129 crates in all besides the proof's own, from `cargo tree -e normal,no-proc-macro`; the linker may drop some) | | MIT, Apache-2.0 or both | exe |
| Microsoft Edge WebView2 Runtime | 157.0.4341.60 on this PC | **Microsoft proprietary licence, not open source** | not in the exe; on the PC or shipped by the installer |
| @tauri-apps/api | 2.12.5 | Apache-2.0 OR MIT | page |
| expo, expo-router, expo-constants, expo-linking, expo-modules-core, expo-glass-effect, @expo/metro-runtime, @expo/cli (its `require` shim) | 55.0.x | MIT | page |
| react, react-dom, scheduler | 19.2.0, 0.27.0 | MIT | page |
| react-native-web | 0.21.4 | MIT | page |
| react-native-screens, react-native-safe-area-context | 4.23.0, 5.6.2 | MIT | page |
| @react-navigation/* (native, native-stack, bottom-tabs, elements, core, routers) | 7.x | MIT | page |
| hyphenate-style-name | 1.1.0 | BSD-3-Clause | page |
| the other 34 packages in the bundle (56 in all, from its source map) | | MIT | page |

Build only: @tauri-apps/cli 2.12.5 (Apache-2.0 OR MIT), tauri-build 2.7.6
(Apache-2.0 OR MIT), TypeScript 5.9 (Apache-2.0), Expo's CLI and Metro (MIT),
Rust (MIT or Apache-2.0), the MSVC linker and Windows SDK (Microsoft
licence). No code was copied from GPL or AGPL projects.

## What was installed or written

- npm: 671 packages into `app\node_modules` only. No global installs.
- Cargo: crates into the user's Cargo cache.
- Files: only inside this spike folder (the WebView2 data folder too).
- Nothing elevated, no installers, no registry writes, no services, no
  firewall or system settings changed. WebView2 was only read.

## Rough edges

1. **No real build trouble.** The first `tauri build --no-bundle` worked. The
   longest path under `app\src-tauri\target` is 236 characters, under the 260
   limit even without long paths; no CMake or ninja is involved, which is what
   broke spike 03. `node_modules` reaches 308 characters; npm and Node cope,
   and Windows long paths are on. A folder much deeper than this one could
   push the Rust build over 260.
2. **`expo start` edits `tsconfig.json`** (it rewrote the `include` list).
   Harmless.
3. **PowerShell 5.1 `Set-Content -Encoding utf8` writes a BOM.** It happened
   once to `app.json` during the `static` experiment and was undone.
4. **PowerShell shows npm's and cargo's stderr progress as red
   `NativeCommandError`.** It is not an error.
5. **A JavaScript change rebuilds the exe in about a minute** (see Build
   times).
6. **The release exe holds the build PC's paths.** Rust keeps source file
   paths for panic messages, so the exe contains the Cargo cache path, which
   includes the Windows user name (251 times in this build). The web export
   does not. A product build should strip them, for example with
   `RUSTFLAGS="--remap-path-prefix=<cargo home>=cargo"` on the build server.
7. **tauri-build writes files into the source tree:**
   `app/src-tauri/gen/schemas/` and `app/src-tauri/permissions/autogenerated/`.
   Both are ignored.

## Unknowns

- What the WebView2 browser process sends to Microsoft at start, and whether
  a browser argument or policy turns it off.
- A real cold start after a reboot, and Windows 10.
- Accelerator keys (F5, Ctrl+P, Ctrl+F) and the default context menu in the
  release window.
- The licence points listed as unconfirmed under "WebView2".
