# LiMo spike: Windows service and screen lock

A small, throwaway proof for the LiMo screen time limiter. It is not the real
product. It exists to confirm three things before the real build starts, and
then it can be removed.

## What it proves

1. A Rust Windows service running as LocalSystem can start a helper process
   inside the logged in user's desktop session.
2. That helper can cover every attached monitor with a lock overlay, above
   other always on top windows, and swallow keyboard input while the lock is
   up.
3. A standard (non administrator) account cannot stop the service, and if the
   helper is ended it is brought back within about a second.

On the real product the service makes the decisions and the helper only draws
the lock. This spike keeps that split so the findings carry over.

## Subcommands

One binary, `limo-spike.exe`:

| Command | Needs admin | What it does |
| --- | --- | --- |
| `monitors` | no | Print each monitor's rectangle and whether it is primary, then a count. Shows no window. |
| `overlay --seconds N` | no | Developer lock on every monitor for N seconds (clamped 1..30). Ends early with the chord Ctrl+Shift+F12 (exit code 3). Prints the helper report line when it ends. It never has UIAccess (see below). |
| `overlay --managed` | no | The mode the service launches. Checks the control file every 250 ms and ends when the deadline passes, is cleared, or breaks the deadline rule below. No escape chord. Hard cap of 120 seconds. Writes the helper report once its windows are up. |
| `install [--helper-as user\|system]` | yes | Copy the binary to Program Files, prepare the ProgramData folder with locked down permissions, write the helper mode, create the `LiMoSpike` service (LocalSystem, automatic, restart on failure after 1 second), and start it. Refuses, changing nothing, if the service already exists. |
| `uninstall` | yes | Stop and delete the service, then remove both folders. Safe to run twice. Refuses, changing nothing, when run from the installed copy, which cannot delete itself; run it from the build folder copy. |
| `lock --seconds N` | yes | Set the lock for N seconds (clamped 5..120) by writing the control file. |
| `unlock` | yes | Clear the lock. |
| `status` | no | Show whether the service exists and its state, the helper mode, the lock deadline and seconds left (or that it was ignored, and why), the last 15 log lines, and the helper report from the data folder, the temp folder, or both, each labelled with its path and time. |
| `bandcheck` | no | Create one topmost 1 by 1 window that is never shown, print one line with this process's UIAccess flag and that window's z order band, then destroy the window. A safe self check of the band reading code. |
| `probe --after N` | no | Wait N seconds (clamped 0..60), then, as the current account, try to: stop the service; end every other process whose image path is exactly the installed exe; overwrite the control file with 0; create a file in the install folder; delete the installed exe. Each attempt is recorded as succeeded, refused (with the Win32 error code and text), or not applicable (nothing there). Prints the report, saves it to `%TEMP%\limo-spike-probe.txt`, and ends with a summary line. |
| `service` | n/a | The service control manager entry point. If run from a terminal it says so and exits. |

Exit codes: 0 success, 1 an operation failed, 2 refused (bad arguments, needs
administrator, already installed, or uninstall run from the installed copy),
3 the developer overlay was ended with the escape chord.

## Why `probe` exists

While the overlay is up the keyboard is swallowed, so a tester cannot type
`sc stop`, use most of Task Manager, or run `unlock`. `probe` makes the attacks
unattended and writes them down. It only ever touches the spike itself: it
opens only processes named `limo-spike.exe`, ends one only after confirming its
image path is exactly the installed exe, never ends itself, never creates the
control file if it is missing, and removes any test file it manages to create.
Run it from a copy outside Program Files (see TESTING.md), otherwise deleting
the installed exe is refused only because the file is in use.

## Files it uses when installed

- `%ProgramFiles%\LiMoSpike\limo-spike.exe` : the installed binary.
- `%ProgramData%\LiMoSpike\config.txt` : the helper mode, `user` or `system`.
- `%ProgramData%\LiMoSpike\control.txt` : one line, the unix time until which the lock holds. `0` means cleared.
- `%ProgramData%\LiMoSpike\spike.log` : timestamped log of service starts and stops, helper launches, exits and relaunches, and errors.
- `%ProgramData%\LiMoSpike\helper-report.txt` : what the last SYSTEM helper got (see below). A user mode helper cannot write here, so its report goes to `%TEMP%\helper-report.txt` of the account it runs as.

The data folder grants SYSTEM and Administrators full control and Users read
only, so a standard account cannot edit the control file.

## How the service works

The service wakes every 100 ms. On each tick it reads the control file, applies
the deadline rule, and while the lock holds makes sure exactly one helper runs
in the active console session. If the helper has died it launches a new one at
once and logs `gap_ms`: the time from noticing the death to the new process
starting. Because a death is noticed within one tick, the real gap is at most
100 ms more than the logged figure, plus the moment the new window takes to
draw.

To keep the log readable at that rate: "no session at the console" and the
active session id are logged only when they change, a failed launch is retried
after 1 second, and a repeated identical launch error is logged once. If the
active session changes (a user switch), the old helper is closed and a new one
started in the new session. If a helper is still running 3 seconds after the
lock ended, the service closes it.

## Helper launch modes

- `user`: the service gets the session's user token (WTSQueryUserToken),
  duplicates it to a primary token, builds the user's environment block, and
  starts the helper on `winsta0\default` with CreateProcessAsUserW. The helper
  runs as the user, so the user can end it, and the service brings it back.
- `system`: the service duplicates its own SYSTEM token, sets the token's
  session id to the active session, then starts the helper the same way. The
  helper runs as SYSTEM inside the user's session, so a standard user cannot
  end it. TESTING.md compares the two.

In both modes the service also sets the UIAccess flag on the duplicated token
before the launch. The next section says why.

## Staying above other always on top windows (UIAccess)

The first run on real hardware found a hole. Another ordinary always on top
app (a desktop ticker that puts itself back on top about once a second) kept
showing over the lock, flickering, because both windows were in the same z
order band and each kept putting itself on top. The mouse still works during a
lock, so anything that shows there can also be clicked.

Windows keeps windows in z order bands. Topmost windows of a process whose
token carries the UIAccess flag go into the UIAccess band, which sits above
every ordinary topmost window. So the service now sets that flag on the
helper's primary token, in both launch modes, before starting the helper:
`SetTokenInformation(TokenUIAccess)` with a 4 byte value of 1. That needs
SeTcbPrivilege, which the LocalSystem service enables at start. No code signing
or manifest is needed when the flag is set on the token this way.

If setting the flag fails, the helper is launched anyway: a lock without
UIAccess is better than no lock. The launch lines in the log end in
`uiaccess=1` or `uiaccess=0` (a relaunch line has `gap_ms` after it), and a
failure's Win32 error is logged once per lock, not on every relaunch.

To show what a helper really got, it writes `helper-report.txt` once its
windows are up: the time, its pid, `managed` or `developer`, its own UIAccess
flag read back from its token, and the z order band of each window, for
example:

```
time=2026-11-17 10:00:00 pid=4242 mode=managed uiaccess=1 windows=2 band[1]=2 (UIAccess band) band[2]=2 (UIAccess band)
```

The band comes from user32's `GetWindowBand`, which is undocumented, so it is
looked up at run time; if it is missing, or the call fails, the band is
reported as unknown. Band numbers are undocumented too, so the raw number is
always printed and only 1 (the normal desktop band) and 2 (the UIAccess band)
are labelled. The SYSTEM helper writes the report to the data folder. The user
mode helper cannot write there, by design, so it writes to its temp folder.
The data folder is never created for this. Writing the report never stops or
delays the lock.

The developer `overlay --seconds N` command cannot get UIAccess: only a helper
started by the service can, because only the service can set the flag. So an
always on top app such as the ticker still shows over a developer overlay.
That is expected; Part U of TESTING.md checks the real lock.

`bandcheck` is a safe self check of the reading code. It creates one hidden 1
by 1 topmost window, never shown, prints the report line for it, destroys it
and exits 0. From a normal terminal it should say `uiaccess=0` and band 1.

## Safety limits (built into the code)

- A lock can never be in force for more than 120 seconds:
  - the `lock` command clamps what it writes to 5..120 seconds ahead;
  - the deadline rule: every reader (the service, the managed overlay and
    `status`) ignores a deadline more than 120 seconds ahead of now and treats
    it as no lock. `lock` never writes one, so such a value can only come from
    a hand edit or a clock change, and the lock then fails open. The service
    logs an ignored deadline once, and `status` says it was ignored and why;
  - the managed overlay caps its own lifetime at 120 seconds whatever the
    control file says.
- The overlay never hides, closes or ends any other program. It does not block
  the mouse, and it cannot block Ctrl+Alt+Del.
- The service never locks the workstation, never signs the user out, and never
  shuts the machine down.
- `uninstall` always removes everything; it retries the folder removal for a
  few seconds while the stopped service process lets go of the exe.

## Build and test

```
cargo build --release
cargo test
```

Produces `target\release\limo-spike.exe`. Built with cargo and rustc 1.98.1
and the Visual Studio 2022 C++ build tools. The unit tests cover the pure
logic: the clamps, the deadline rule, path comparison, Win32 error decoding,
the probe's result classification and formatting, the helper report line and
band labels, the choice of where the report is written, and the launch log
lines.

## Try it

See `TESTING.md` for the checklist: a safe developer check, an administrator
walk through, and the real test from a standard account, in both helper modes.
