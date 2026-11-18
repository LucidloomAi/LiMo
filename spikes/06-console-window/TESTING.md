# Testing LiMo spike 06

Everything here runs on the Windows PC that has this folder. Nothing needs a
phone or a person watching: the self check does the looking. The steps by
hand at the end are optional.

Commands are typed in PowerShell, in this spike folder
(`spikes\06-console-window`), unless a step says otherwise.

Nothing here is elevated, installs anything, or writes outside this folder.
The window it opens is small (420 by 760), does not take the keyboard focus
and closes itself.

## 1. Build

```
cd app
$env:EXPO_NO_TELEMETRY = "1"
npm ci
npm run typecheck
npm run build:console
cd ..
```

Pass: `typecheck` prints nothing; the build ends with
`Built application at: ...\app\src-tauri\target\release\limo-console-proof.exe`.

## 2. One self check

```
$p = Start-Process app\src-tauri\target\release\limo-console-proof.exe -ArgumentList '--self-check','logs\self-check.log' -PassThru -Wait
$p.ExitCode
Get-Content logs\self-check.log
```

Make the `logs` folder first if it does not exist
(`New-Item -ItemType Directory -Force logs`).

Pass:
- the exit code is `0`;
- the log has these lines, in this order, each starting with a time in ms:
  `start` (with the WebView2 runtime version), `window_created`,
  `page_load_started: http://tauri.localhost/`, `page_load_finished`,
  `page js_start`, `page today_rendered` (with `where="Answers from: rust
  (host tauri)"`, Asha Left 75 min, Kabir Left 10 min, `csp_violations=0`),
  `page rust_answer` (the JSON, `"answeredBy":"rust"`, and
  `core_command_probe=refused (Command plugin:app|version not allowed by
  ACL)`), `page give_15` (Left 75 min to Left 90 min), `page child_route`
  (`href=http://tauri.localhost/child/c1`, heading Asha), `page lock_now`
  (Unlocked to Locked), `page reload_requested`, a second
  `page_load_started: http://tauri.localhost/child/c1`, `page after_reload`
  (heading Asha, Left 90 min, Locked, `csp_violations=0`), `done: all 8 steps
  reported, exit code 0`, `event_loop_exit`;
- afterwards no `limo-console-proof.exe` is running:
  `Get-Process limo-console-proof -ErrorAction SilentlyContinue` prints
  nothing.

## 3. Measurements

```
powershell -ExecutionPolicy Bypass -File tools\run-self-check.ps1 -Runs 8 -HoldMs 3000 -FreshProfile
```

It runs the self check 8 times (the first with a new WebView2 data folder),
reads the memory of the exe and its WebView2 processes during the 3 s hold,
notes whether the window ever became the foreground window, and ends any
leftover process of the proof. Results go to `logs\summary.csv` and
`logs\run-<n>.log`.

Pass: every `ExitCode` is `0`, `TookFocus` is `False`, `LeftoversEnded` is
`0`. The README's tables come from this script.

## 4. Plain files over 127.0.0.1

```
node tools\serve-check.mjs
```

Pass: `GET / -> 200`, the script `-> 200 text/javascript`,
`script looks like the app: true`. `GET /child/c1 -> 404` is expected: the
plain server has no fallback to `index.html`.

## 5. The failure paths (optional, needs two rebuilds)

1. In `app\src-tauri\src\self_check.rs` add a step to `STEPS` that the page
   never reports, for example `"never_reported_temporary_test",`.
2. `cd app; npx tauri build --no-bundle; cd ..`
3. Run step 2 above. Pass: after about 20 s the exit code is `1` and the log
   ends with `timeout: missing after 20 s: never_reported_temporary_test;
   exit code 1`.
4. Run it again and close the window after 3 s
   (`$p = Start-Process ... -PassThru; Start-Sleep 3; $p.CloseMainWindow()`).
   Pass: exit code `1`, log line `closed: window closed early, missing: ...`.
5. Remove the step and rebuild.

## 6. By hand (optional)

- In a normal browser: `cd app; $env:BROWSER = "none"; npx expo start --port 8099`,
  open `http://localhost:8099`. Pass: "Answers from: mock (host mock)", the
  buttons and the Child route work. Stop the server with Ctrl+C.
- The window in normal mode:
  `app\src-tauri\target\release\limo-console-proof.exe` with no arguments.
  Pass: "Answers from: rust (host tauri)"; Give and Lock change the figures;
  closing the window ends the process. This was not done in the proof run.

## Results

| Check | Result |
| --- | --- |
| Build | passed, first `tauri build --no-bundle` 1 min 56 s |
| Self check, 24 runs over three builds of the exe | all exit code 0 with all 8 steps (besides the two deliberate failure runs) |
| Window took the focus | never, in 14 runs that checked |
| Leftover processes | none after any run |
| Plain files over 127.0.0.1 | index 200, script 200, `/child/c1` 404 |
| Failure paths | deadline: exit 1 after 20,000 ms; closed early: exit 1 |
| Browser with the mock | passed |
| Normal mode by hand | not done |
