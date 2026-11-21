# LiMo's installer for Windows

One file, `LiMo_<version>_x64-setup.exe`, made by Tauri's bundler with
NSIS, for every account of a PC (per machine). It copies LiMo's files into
`%ProgramFiles%\LiMo` and calls `limo.exe`, which does and decides
everything else: the data folder, the service, the firewall rules, the
shortcut, the upgrade and the removal
([`../windows/README.md`](../windows/README.md), "Installing, upgrading and
removing").

**What it is now.** Built, looked inside, and never run: no builder runs an
installer. What only a run at a PC shows is the owner's (see [The owner's
run](#the-owners-run)). It is not signed (see [Unsigned: what Windows
shows](#unsigned-what-windows-shows)).

## Contents

- [What it puts on a PC](#what-it-puts-on-a-pc)
- [What it does, in order](#what-it-does-in-order)
- [Building it](#building-it)
- [The phone app](#the-phone-app)
- [The version](#the-version)
- [The template](#the-template)
- [The hooks, as compiled](#the-hooks-as-compiled)
- [The notices](#the-notices)
- [What was built and looked at](#what-was-built-and-looked-at)
- [Unsigned: what Windows shows](#unsigned-what-windows-shows)
- [The owner's run](#the-owners-run)
- [On a PC that never had LiMo](#on-a-pc-that-never-had-limo)
- [Tools, and what the build fetches](#tools-and-what-the-build-fetches)
- [Layout](#layout)
- [Left open](#left-open)

## What it puts on a PC

```
%ProgramFiles%\LiMo\
  limo.exe                         LiMo for Windows: the service, its helpers, and what an installer calls
  limo-console.exe                 the guardian's console, with its page inside
  phone\limo-child.apk             LiMo's app for phones (its child form), as the console puts it on a phone
  notices\LICENSE.txt              LiMo's licence, the Apache License 2.0
  notices\THIRD-PARTY-NOTICES.txt  every third party component, and its licence text
  uninstall.exe                    written by the installer as it runs; Windows' list of apps runs it
```

Nothing else of the installer's own: no Start menu or desktop shortcut (it
is `limo install` that makes "LiMo settings" in the Start menu of every
account), and in the registry only Windows' entry in its list of apps
(`HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall\LiMo`) and the
place it installed to (`HKLM\Software\LucidLoom AI\LiMo`). `%ProgramData%\LiMo`
and LiMo's adb key in the guardian's `%LOCALAPPDATA%\LiMo\` are
`limo.exe`'s and the console's, never the installer's.

## What it does, in order

The installer, as an administrator (Windows asks):

1. **Refuses** to run on 32 bit Windows, or over a newer LiMo (its entry in
   Windows' list of apps says the version), with nothing changed.
2. **Installs WebView2** only on a PC without it: Microsoft's bootstrapper,
   carried in the installer, downloads it from Microsoft (Windows 11 has
   it already).
3. **`limo stop-for-upgrade`**, from a copy of the new `limo.exe` in the
   installer's own temporary folder, before any file is copied: on a PC
   with LiMo, the service stopped and every LiMo program running from the
   program folder ended, so their files can be replaced; on a PC without
   LiMo, it says so. Not 0: nothing is copied, nothing changed.
4. **Copies the files** above into `%ProgramFiles%\LiMo`, always there
   (no folder page; `/D=` is ignored), and writes `uninstall.exe` and its
   entry in Windows' list of apps.
5. **`limo install`**, from `%ProgramFiles%\LiMo\limo.exe`, never with
   `--dev`. 0: installed. 1: installed with a step not done or failed,
   said in a message that points at `%ProgramData%\LiMo\setup.log`. 2:
   refused, nothing else changed; the installer says so and stops (the
   programs stay copied; removing LiMo from Windows' list of apps takes
   them away).
6. **The last page** offers "Open LiMo settings", which starts the console
   in the installing person's own account, not elevated: the first start.

An upgrade, or the same version again, is the same run over what is there:
step 3 stops the old LiMo, step 4 replaces its files, step 5 says what it
is ("This replaces LiMo 0.1.0 with LiMo 0.2.0") and keeps the family's
file. The installer never runs the old version's uninstaller.

The uninstaller (Windows' list of apps, or `uninstall.exe`), as an
administrator:

1. **`limo uninstall`**, in a console window of its own, where the person
   types the guardian password (unless a guardian allowed the removal in
   LiMo's settings in the ten minutes before) and says whether the
   family's file is kept. Not 0: nothing is removed, and the uninstaller
   says so and stops.
2. Only then **removes** the files above, its entry in Windows' list of
   apps, and the console window's own browser data in this account (it
   holds nothing of the family; the page keeps nothing).

Each `limo.exe` command runs in a console window of its own, where Windows
shows its words as `limo.exe` writes them (the installer's own list of
details would garble the apostrophes). The window is `cmd.exe`'s, so that
when the command fails it stays open with "Press any key to continue", and
its last lines can be read; the command's exit code comes back to the
installer through it.

## Building it

Needs, on Windows (x64): Rust 1.89 or later with the MSVC toolchain, Node
22 with npm, 7-Zip (for the look inside the installer; without it that
step is left out and says so), and the internet the first time (the
crates, the npm packages, NSIS and its plugin, the WebView2 bootstrapper).
Nothing is installed on the PC and nothing runs elevated; Tauri's bundler
keeps the NSIS it fetches in `%LOCALAPPDATA%\tauri`.

From the repository's folder, in PowerShell:

```
powershell -ExecutionPolicy Bypass -File installer\build.ps1 -ChildApk <the signed release APK of the child form>
powershell -ExecutionPolicy Bypass -File installer\build.ps1        # the APK's path in LIMO_CHILD_APK
powershell -ExecutionPolicy Bypass -File installer\build.ps1 -Throwaway   # a dry build, with a throwaway phone app
```

It says each step as it goes, and stops at the first that fails with a
sentence saying why:

| Step | What |
| --- | --- |
| 1. the version | from `windows\Cargo.toml`; the console's must be the same (see [The version](#the-version)) |
| 2. the phone app | the APK given, checked before anything long is built (see [The phone app](#the-phone-app)) |
| 3. the tools | node, npm, cargo; Tauri's bundler as `package-lock.json` pins it (`npm ci` here when missing); the app's packages (`npm ci` in `phone\app` when missing); 7-Zip |
| 4. the console's page | `phone\app`'s own `scripts\build-console.mjs` (`npm run build:console`), into `phone\app\dist\console`, which the console embeds |
| 5. the page's source map | the same bundle again, with a map naming its files, into `installer\target\page`, for the notices |
| 6. `limo.exe` | `cargo build --release --locked` of `windows\` |
| 7. `limo-console.exe` | `cargo build --release --locked` of `console\`, the page inside it |
| 8. the notices, and the files staged | `notices.mjs` (see [The notices](#the-notices)); the files as they go on a PC, in `installer\target\stage`; the build PC's names looked for in every one |
| 9. the installer | Tauri's bundler (`tauri bundle --bundles nsis`) with `installer\target\bundle.json`, which the script writes: LiMo's template and hooks, the console as the main program, the staged files as its resources, the version; then the installer copied to `installer\target` with its SHA256 beside it. The bundler rewrites the features of the console's tauri crates in `console\Cargo.toml`; the script puts the file back as it was, byte for byte |
| 10. a look inside | 7-Zip lists and unpacks the installer; its files for the program folder must be exactly the layout; the build PC's names looked for in every file it holds, and in the installer itself |
| 11. the hooks, read back | makensis compiles the bundler's script again with every instruction logged (`/V4`), macros expanded, into `installer\target\installer-compiled.log`; the three commands looked for in their places (see [The hooks, as compiled](#the-hooks-as-compiled)); its second copy of the installer is thrown away |

Steps 6 and 7 build with each of the build PC's folders remapped to a
plain name in what Rust writes into an exe (the home folder to `home`,
Cargo's to `cargo`, rustup's to `rustup`, the repository to `limo`), as
`console\tools\build-release.ps1` does, and into `installer\target\cargo`,
never into another folder's `target`. The names looked for (steps 8 and
10): the build PC's user name, its home folder, its name and the
repository's folder, as ASCII and as UTF-16 in either byte alignment, in
any case, in every file and in every entry inside each APK; only the
counts are said.

Writes `installer\target\LiMo_<version>_x64-setup.exe` and
`LiMo_<version>_x64-setup.exe.sha256`; a dry build writes
`LiMo_<version>_x64-setup-DRY-BUILD-throwaway-phone-app.exe`, never to be
given to anyone. `installer\target` is git ignored.

## The phone app

The installer carries LiMo's child form as a release build signed with
the maker's key, which the lead builds; the build takes its path from
`-ChildApk` or from the environment variable `LIMO_CHILD_APK`. With
neither, it stops before building anything: "LiMo's app for phones is
missing: give the signed release APK of the child form with -ChildApk
<file>, or put its path in the environment variable LIMO_CHILD_APK." The
APK is read as the console reads it before putting LiMo on a phone
(limo-cable's `read_apk`, through `console\examples\app_facts.rs`): it must
be `ai.lucidloom.limo.child` and not a debug build. Its signature is not
checked here: Android checks it on the phone, and refuses a LiMo of another
key over the one there.

`-Throwaway` makes a throwaway APK instead (`console\examples\throwaway_apk.rs`,
the console's tests' own: LiMo's package name, version 0.3.0, a release
build, nothing to run in it), so the whole build can be tried without the
real app.

## The version

One place: **LiMo for Windows' version, in `windows\Cargo.toml`**. It is
the version `limo install` records in `%ProgramData%\LiMo\installed.json`
and compares on an upgrade, so the installer takes it too: its file name,
its version in Windows' list of apps (which the next installer reads to
refuse a downgrade), and its own file version. The console's version
(`console\Cargo.toml`) must be the same, or the build stops and says which
to change. The phone app has its own version, which the console compares
with the one on a phone.

## The template

`installer.nsi` is Tauri's own NSIS template, as tauri-cli 2.12.5 ships it
(`crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi` in the Tauri
repository, MIT OR Apache-2.0), given to the bundler as
`bundle.windows.nsis.template`, with four changes, each marked `LiMo:` in
it. Tauri's template as it is was tried first, with only the hooks, and its
generated script read: it fights LiMo's model in these places.

1. **The folder.** Tauri's offers a folder page and keeps the last place.
   LiMo goes in `%ProgramFiles%\LiMo` only: the console trusts the service
   only as `Program Files\LiMo\limo.exe`, and `limo install` copies itself
   there from anywhere else, so another folder would leave two LiMos.
2. **No "uninstall first".** On an upgrade Tauri's shows a page whose
   first choice, checked, is "Uninstall before installing", which runs the
   old version's uninstaller; that would run `limo uninstall`, which asks
   for the guardian password and offers to delete the family's file. LiMo's
   installs over the old version and lets `limo.exe` upgrade, and refuses a
   newer LiMo before anything is done (Tauri's refused it only when silent).
3. **No shortcut of its own.** Tauri's makes `LiMo.lnk` in the Start menu
   and offers a desktop shortcut; `limo install` makes "LiMo settings" for
   every account, and `limo uninstall` removes it.
4. **No "Delete the application data" box** at removal: a family could read
   it as the family's file, which `limo uninstall` asks about in its own
   words. The console's browser data in this account is removed anyway.

Everything else is Tauri's: the WebView2 check and bootstrapper, the files,
Windows' list of apps, the check that the console is not running, the last
page's run of the console as the person's own account
(`nsis_tauri_utils::RunAsUser`). The hooks are Tauri's mechanism
(`bundle.windows.nsis.installerHooks`), in `hooks.nsh`.

## The hooks, as compiled

`hooks.nsh` holds the three calls; the template puts each where Tauri's
does (`NSIS_HOOK_PREINSTALL` before the files, `NSIS_HOOK_POSTINSTALL`
after them, `NSIS_HOOK_PREUNINSTALL` before anything is removed). Read
back from makensis's own log of the instructions it compiled (step 11),
21 Nov 2026, with the line numbers of that log:

```
589: ExecWait: ""$SYSDIR\cmd.exe" /d /c ""$PLUGINSDIR\limo-new\limo.exe" stop-for-upgrade & if not errorlevel 0 (echo. & pause & exit 3) else if errorlevel 2 (echo. & pause & exit 2) else if errorlevel 1 (echo. & pause & exit 1)"" (->$0)
687: File: "limo-console.exe" 4316672 bytes
695: WriteUninstaller: "$INSTDIR\uninstall.exe"
726: ExecWait: ""$SYSDIR\cmd.exe" /d /c ""$INSTDIR\limo.exe" install & if not errorlevel 0 (echo. & pause & exit 3) else if errorlevel 2 (echo. & pause & exit 2) else if errorlevel 1 (echo. & pause & exit 1)"" (->$0)
825: ExecWait: ""$SYSDIR\cmd.exe" /d /c ""$INSTDIR\limo.exe" uninstall & if not errorlevel 0 (echo. & pause & exit 3) else if errorlevel 2 (echo. & pause & exit 2) else if errorlevel 1 (echo. & pause & exit 1)"" (->$0)
925: Delete: "$INSTDIR\limo-console.exe"
```

So: `stop-for-upgrade`, from the new `limo.exe` the installer carries for
it (`$PLUGINSDIR\limo-new`, copied there just before), comes before the
first program file is copied; `install`, never with `--dev`, after every
file and the uninstaller; `uninstall`, in a console window through
`ExecWait`, before the first file is removed, the removal going on only
when it returns 0. The check also finds no `--dev`, no `nsExec` and no
shortcut made in any instruction, and no name of the build PC in any
instruction: the build PC's folders appear only in the compiler's own
lines (what it reads, includes, defines and writes), which are not in the
installer. The fourth call, `stop-for-upgrade` from a copy of the
installed `limo.exe`, is for an uninstaller run with `/UPDATE`, which
LiMo's installer never does (Tauri's updater would): it never calls `limo
uninstall`.

Each runs through `cmd.exe`'s `if errorlevel`, which reads the exit code
when the command has ended: 0, 1 and 2 come back as they are; a code of 2
or more as 2; a negative one (a crash) as 3, never as 0.

## The notices

`notices.mjs`, a script of this folder (Node, Apache-2.0; no tool of
anyone else's), writes `notices\LICENSE.txt` (the repository's `LICENSE`)
and `notices\THIRD-PARTY-NOTICES.txt` from the packages themselves:

- **the Rust crates in `limo.exe` and `limo-console.exe`**: each program's
  tree from `cargo metadata --locked` for `x86_64-pc-windows-msvc` with its
  default features, following normal dependencies only and leaving out
  procedural macros (they run while compiling and leave nothing in the
  program); LiMo's own crates named once, as LiMo's;
- **the npm packages in the console's page**: those whose files the page's
  bundle holds, from the source map of the same bundle (step 5), so a
  package that is installed but not bundled is not listed;
- **the page's three fonts**, with the OFL beside each;
- **the installer's own parts**: NSIS (its licence kept in
  `licences\NSIS-3.13-COPYING.txt`, since on a clean PC the bundler fetches
  NSIS only at step 9; the build stops if the NSIS it used is another
  version), Tauri's NSIS plugin, and Microsoft's WebView2 bootstrapper.

Each licence text is read from the component's own files (`LICENSE`,
`COPYING`, `NOTICE` and the like) and written once for all the components
that carry it word for word. On 21 Nov 2026: **266 Rust crates, 20 npm
packages, 3 fonts and 3 parts of the installer; 215 distinct licence
texts; 718,581 bytes.** Eight carry no licence file of their own, and are
listed with their authors and their source's address, with the standard
texts of what they name: alloc-stdlib 0.3.0 (BSD-3-Clause), selectors
0.38.0 (MPL-2.0, its source at its repository), webview2-com and
webview2-com-sys 0.39.1, @react-native/js-polyfills 0.86.3 and
@react-native/normalize-colors 0.74.89 (MIT), nsis_tauri_utils (MIT OR
Apache-2.0) and the WebView2 bootstrapper (Microsoft's own terms). The
licences named: MIT, Apache-2.0, either of them, BSD-2-Clause,
BSD-3-Clause, Zlib, Unicode-3.0, 0BSD, CC0-1.0 and MIT-0 as choices,
Unlicense as a choice, MPL-2.0 (cssparser, dtoa-short, option-ext and
selectors, in `limo-console.exe` through Tauri: a file level licence, their
source as published), OFL-1.1 (the fonts), NSIS's zlib, bzip2 and CPL
parts, and Microsoft's terms for the WebView2 bootstrapper. Every one lets
LiMo ship under the Apache License 2.0; none is GPL, LGPL or AGPL. The
file ends with the WebView2 Runtime's notice that it includes Microsoft
Defender SmartScreen and sends information to Microsoft.

**Not in this list: what is inside the phone app.** The APK is carried as
it is; its own third party components (the app's npm packages, Android's
libraries, LiMo's Rust code for the phone) belong in the app's own notices,
which the phone side keeps.

## What was built and looked at

The dry build (`build.ps1 -Throwaway`), 21 Nov 2026, from this checkout,
every step passed:

| | |
| --- | --- |
| The installer | `installer\target\LiMo_0.1.0_x64-setup-DRY-BUILD-throwaway-phone-app.exe`, **7,129,707 bytes (6.80 MiB)**, SHA256 `5f23b8cb3cf0687afd2af5a565f2059741e3befc0dcfc93623d6cff43f01271e` (a build differs by a few hundred bytes from the one before: the bundler stamps it). It carries a 307,914 byte throwaway APK; with the real one (29.2 MiB) it will be about 36 MiB, nearly all of it the APK, which compresses little |
| Inside it, for `%ProgramFiles%\LiMo` (7-Zip) | `limo.exe` (10,929,664 bytes), `limo-console.exe` (4,316,672), `notices\LICENSE.txt`, `notices\THIRD-PARTY-NOTICES.txt`, `phone\limo-child.apk`: **exactly the layout**, nothing more (`uninstall.exe` is written when it runs) |
| Inside it, used while it runs and removed | `$PLUGINSDIR\limo-new\limo.exe` (the new `limo.exe` for the first call), `modern-wizard.bmp`, `nsDialogs.dll`, `nsis_tauri_utils.dll`, `System.dll`; `$TEMP\MicrosoftEdgeWebview2Setup.exe` (1,826,528 bytes, run only on a PC without WebView2) |
| The build PC's names | 0 in each of the 11 files 7-Zip unpacked from it, and in every entry inside the APK; 0 in the installer file itself; 0 in every staged file before bundling; 0 in any instruction the installer runs |
| The hooks | as above |
| Defender on this PC | real time protection on; its history holds no detection of any LiMo file built here. No scan was started by hand: this PC's Defender sends samples to Microsoft by itself (automatic sample submission and advanced cloud reporting are on), and uploading LiMo anywhere needs the owner's word. A scan of the release installer is for the owner to start, or to run with sample submission off |

A build without a phone app stops at step 2 with its sentence; one given
a file that is not an APK stops there too ("The file is not an APK: it is
not even a zip file."). Timings, once the crates are fetched: the page
about a minute, `limo.exe` about a minute from nothing, `limo-console.exe`
about two and a half, the bundle under a minute.

## Unsigned: what Windows shows

The installer is not signed: unsigned for the trial, as the owner decided
on 16 Nov 2026, signing later. **None of this has been tried**: it is what
Microsoft's own pages say, read on 21 Nov 2026, and where they give no
words, what is commonly seen, marked as such.

1. **SmartScreen**, for a file downloaded with a browser (the file then
   carries the mark of the web; a file built or copied on the PC itself
   does not, and meets none of this). Microsoft: SmartScreen checks
   "downloaded files against a list of files that are well known and
   downloaded frequently. If the file isn't on that list, Microsoft
   Defender SmartScreen shows a warning, advising caution", and "If a URL,
   a file, an app, or a certificate has an established reputation, users
   don't see any warnings. If there's no reputation, the item is marked as
   a higher risk and presents a warning to the user"; it "protects against
   malicious files from the internet" and not files "on internal locations
   or network shares"
   ([SmartScreen](https://learn.microsoft.com/en-us/windows/security/operating-system-security/virus-and-threat-protection/microsoft-defender-smartscreen/)).
   A new unsigned installer has no reputation, so a family meets the
   warning. Its words are on no Microsoft page read; the warning commonly
   seen is a blue window, "Windows protected your PC", with a link "More
   info" that shows the app's file name and "Publisher: Unknown publisher"
   and a button "Run anyway" (**not from Microsoft's documentation; to be
   confirmed and written down in the run on a PC that never had LiMo**).
2. **The elevation prompt** (User Account Control), since the installer
   runs as an administrator. Microsoft: an administrator gets "the consent
   prompt", a standard account "the credential prompt" (an administrator's
   name and password); the prompts are shown on the secure desktop; and
   "Yellow background: the application is unsigned or signed but isn't
   trusted"
   ([How UAC works](https://learn.microsoft.com/en-us/windows/security/application-security/application-control/user-account-control/how-it-works)).
   Its question, commonly seen as "Do you want to allow this app from an
   unknown publisher to make changes to your device?", with "Publisher:
   Unknown", is not on that page (**to be confirmed**).
3. **Smart App Control** (Windows 11), where it is on. Microsoft: "If the
   security service is unable to make a confident prediction about the
   app, then Smart App Control checks to see if the app has a valid
   signature ... If the app is unsigned, or the signature is invalid, Smart
   App Control will consider it untrusted and block it for your
   protection", and "There is currently no way to bypass Smart App Control
   protection for individual apps. You can turn Smart App Control off";
   "Recent Windows updates allow Smart App Control to be re-enabled without
   requiring a clean installation"
   ([Smart App Control FAQ](https://support.microsoft.com/en-us/topic/what-is-smart-app-control-285ea03d-fa88-4d56-882e-6698afdb7003)).
   So on a PC where it is on, LiMo's unsigned installer, and its unsigned
   programs, are blocked with no "Run anyway": **each trial PC is checked
   for it first** (Windows Security, App and browser control, Smart App
   Control).
4. **Antivirus.** An unsigned program that installs a SYSTEM service, ends
   other programs, takes the keyboard and covers the screen fits what
   antivirus programs flag. This PC's Defender found nothing in the built
   files as they were written (see [What was built and looked
   at](#what-was-built-and-looked-at)); no file was sent to any scanner.

## The owner's run

For the lead, with the owner, on this PC, over the test install
(`limo install --dev`, `../windows/TESTING.md`) with a family whose
guardian password is known and the child's test account. About 30
minutes. Use an installer built with the real phone app
(`build.ps1 -ChildApk ...`); a dry build does for everything here but Add
a phone. Close LiMo's settings first. A file built on this PC carries no
mark of the web, so SmartScreen does not show here: that is the clean PC's
run.

1. **Installed over the test install.** Double click
   `installer\target\LiMo_0.1.0_x64-setup.exe`. Windows asks for
   elevation (yellow, an unknown publisher): Yes. The welcome page, then
   Install. Look for, in order: a console window for `limo
   stop-for-upgrade` ("Stopped the service LiMo.", perhaps "Ended limo.exe
   (pid P) in session N.", "Nothing of LiMo runs from C:\Program Files\LiMo
   now, so its files can be replaced."), closing by itself; the files
   copied in the installer's list; a second console window for `limo
   install` ("This replaces an earlier LiMo whose version was not recorded
   (a test install made before LiMo kept it) with LiMo 0.1.0.", no "Not in
   the program folder:" line this time, "The test cap of the earlier test
   install is now off.", the two firewall rules, a shortcut made, "LiMo
   0.1.0 is installed."), closing by itself; the last page with "Open LiMo
   settings" ticked. Finish: the console opens in your own account; sign
   in with the family's password. Then check: `C:\Program Files\LiMo`
   holds exactly `limo.exe`, `limo-console.exe`, `phone\limo-child.apk`,
   `notices\` (two files) and `uninstall.exe`; `limo status` says Running,
   Installed: LiMo 0.1.0, Test cap: off; the Start menu has one "LiMo
   settings" and no "LiMo"; no desktop shortcut; Settings, Apps, Installed
   apps lists "LiMo", 0.1.0, LucidLoom AI; part Q's step 3 shows the two
   firewall rules; the child's account is limited again within seconds.
2. **Installed again over itself** (what an upgrade does, the version the
   same: a true upgrade needs the next version). Run the installer again:
   no page offers to uninstall first; the same two console windows; `limo
   install` says "LiMo 0.1.0 is installed: it is installed again over
   itself."; the console signs in with the same password; still two
   firewall rules and one shortcut.
3. **The uninstall refused without the password.** Settings, Apps, LiMo,
   Uninstall; elevation: Yes; the uninstaller's page: Uninstall. A console
   window asks "Guardian password, to remove LiMo (not shown):". Press
   Enter alone: "No password was typed. Nothing was removed. ...", then
   "Press any key to continue"; a key. The uninstaller says "LiMo was not
   removed." and stops. Check: LiMo still in Windows' list of apps, `limo
   status` Running, `C:\Program Files\LiMo` whole.
4. **The uninstall with the password.** The same, and type the guardian
   password: "The guardian password is right.", then "Keep the family's
   file? ... Type keep or delete:"; type `keep`. Its steps, "LiMo is
   removed.", and the window closes by itself. The uninstaller removes the
   files and finishes. Check: no `C:\Program Files\LiMo`; LiMo not in
   Windows' list of apps; no "LiMo settings" in the Start menu;
   `Get-NetFirewallRule -Group LiMo` finds nothing; elevated, `dir
   C:\ProgramData\LiMo` shows `family.limo` and `setup.log` (kept); the
   child's account is not limited.
5. **Back to the test install**, for every later part of
   `../windows/TESTING.md`: elevated, from the build folder, `.\limo
   install --dev`.

Write down: each console window's lines (a photo will do), anything the
installer or the uninstaller said in a box, and how long each run took.

## On a PC that never had LiMo

The real test (Windows 11, no developer tools, no phone maker's driver),
about 90 minutes with Add a phone:

- **Download it as a family would**, with a browser, from where the owner
  put it, and compare its SHA256 with the `.sha256` file
  (`Get-FileHash <file>`).
- **Before running it**: Windows Security, App and browser control, Smart
  App Control: On, Evaluation or Off. Write it down; if On, the installer
  is expected to be blocked with no way past it (see above).
- **SmartScreen**: write down its exact words and buttons, and the steps
  past it.
- **The elevation prompt**: its colour and its publisher line.
- **WebView2**: Windows 11 has it, so nothing is downloaded; on a Windows
  10 PC without it, the installer's list says it is installing it (the
  internet is needed then).
- **`limo stop-for-upgrade`** says there is no LiMo; **`limo install`**
  says it is a new install and makes the data folder (its owner and
  access list, as `../windows/TESTING.md` part Q says to write them down).
- **The first start**: "Open LiMo settings" opens the console in the
  guardian's account, on the first start's welcome.
- **The child's account**: LiMo's settings refuse it; it cannot read
  `C:\ProgramData\LiMo` (`dir` there, from the child's account: access
  denied).
- **Defender**: Windows Security, Protection history: anything about
  `limo.exe`, `limo-console.exe` or the installer.
- **Add a phone** with the Galaxy: the cable's strip on a PC that never had
  Samsung's driver.
- **An upgrade over it** (when the next version exists) and **a removal**,
  as above.

## Tools, and what the build fetches

| What | From | Licence | Ships in the installer |
| --- | --- | --- | --- |
| Tauri's CLI and bundler, `@tauri-apps/cli` 2.12.5 and `@tauri-apps/cli-win32-x64-msvc` 2.12.5 | npm, as `package-lock.json` pins them | Apache-2.0 OR MIT | no: it makes it |
| NSIS 3.13 | fetched by the bundler from Tauri's own copy, `https://github.com/tauri-apps/binary-releases/releases/download/nsis-3.13/nsis-3.13.zip`, its hash checked, kept in `%LOCALAPPDATA%\tauri\NSIS` | zlib/libpng; its bzip2 module under the bzip2 licence; its LZMA module under the CPL 1.0 (its COPYING) | yes: its code is the installer and the uninstaller, with two of its plugins (`System.dll`, `nsDialogs.dll`) and its wizard picture |
| `nsis_tauri_utils.dll` 0.5.3 | fetched by the bundler, `https://github.com/tauri-apps/nsis-tauri-utils/releases/download/nsis_tauri_utils-v0.5.3/nsis_tauri_utils.dll`, its hash checked | MIT OR Apache-2.0 | yes: a plugin the installer calls (the version compare, the check that the console is not running, running it as the person's own account) |
| Microsoft Edge WebView2 bootstrapper, `MicrosoftEdgeWebview2Setup.exe` | fetched by the bundler from Microsoft, `https://go.microsoft.com/fwlink/p/?LinkId=2124703` | Microsoft's own terms, not open source | yes: run only on a PC without the WebView2 Runtime |
| 7-Zip | on the build PC | LGPL-2.1 with an unRAR restriction | no: it only looks inside |
| Node, npm, Rust, Cargo | on the build PC | MIT, Artistic-2.0, MIT OR Apache-2.0 | no |

The first time on a PC, the bundler found an older NSIS (3.11) in
`%LOCALAPPDATA%\tauri`, left there by an earlier use of Tauri's bundler on
this PC, and replaced it with 3.13 ("NSIS.exe is outdated. Recreating
it."). Nothing else on the PC is changed by a build. No new crate was
added for this; the console gained two examples (`app_facts.rs`,
`throwaway_apk.rs`) that use what it already depends on.

## Layout

```
installer/
  README.md
  build.ps1            the one build, step by step
  installer.nsi        Tauri's NSIS template with LiMo's changes
  hooks.nsh            the three calls of limo.exe
  notices.mjs          the third party notices, from the packages themselves
  licences/NSIS-3.13-COPYING.txt   NSIS's licence, for the notices
  package.json, package-lock.json  Tauri's bundler, pinned
  .gitignore           node_modules/, target/
```

and in `../console/examples/`: `app_facts.rs` (what an APK is) and
`throwaway_apk.rs` (a throwaway APK for a dry build).

Made by a build, git ignored: `installer\node_modules` and
`installer\target` (`cargo\` the two programs' builds and the bundler's
output, `stage\` the files as they go on a PC, `page\` the page's bundle
with its map, `look\` the installer unpacked, `bundle.json`,
`installer-compiled.log`, and the installer with its `.sha256`).

## Left open

- **Never run.** Everything a run shows is the owner's: [The owner's
  run](#the-owners-run) on this PC, then [a PC that never had
  LiMo](#on-a-pc-that-never-had-limo).
- **The release installer** needs the signed release APK of the child
  form; built so far only as a dry build.
- **Signing**: the installer, its uninstaller, `limo.exe` and
  `limo-console.exe` are unsigned (the decision of 16 Nov 2026). When
  signing comes, the bundler signs the installer and the uninstaller
  (`bundle.windows.signCommand`), and the two programs are signed before
  they are staged.
- **The words of the warnings** (SmartScreen's, the elevation prompt's) are
  on no Microsoft page read; the clean PC's run writes them down.
- **A true upgrade** (an older LiMo replaced by a newer) and **the refusal
  of a downgrade** need two versions; the run over itself shows the same
  steps.
- **Defender**: a scan of the release installer, and the clean PC's
  protection history (risk of LiMo being taken for malware).
- **Windows 10, and Windows on ARM**: not tried; the installer refuses 32
  bit Windows.
- **The phone app's own notices**, inside the APK: the phone side's.
- **The installer's own words** use a straight apostrophe (NSIS reads a
  script without a byte order mark as the PC's code page, so the files are
  kept plain ASCII); `limo.exe`'s words, in its console windows, are
  typographic.
