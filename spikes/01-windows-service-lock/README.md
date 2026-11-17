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
| `overlay --seconds N` | no | Developer lock on every monitor for N seconds (clamped 1..30). Ends early with the chord Ctrl+Shift+F12 (exit code 3). Prints the helper report (two lines) when it ends. It never has UIAccess (see below). |
| `overlay --managed` | no | The mode the service launches. Checks the control file every 250 ms and ends when the deadline passes, is cleared, or breaks the deadline rule below. No escape chord. Hard cap of 120 seconds. Writes the helper report once its windows are up. |
| `install [--helper-as user\|system\|guarded]` | yes | Copy the binary to Program Files, prepare the ProgramData folder with locked down permissions, write the helper mode, create the `LiMoSpike` service (LocalSystem, automatic, restart on failure after 1 second), and start it. Refuses, changing nothing, if the service already exists. |
| `uninstall` | yes | Stop and delete the service, then remove both folders. Safe to run twice. Refuses, changing nothing, when run from the installed copy, which cannot delete itself; run it from the build folder copy. |
| `lock --seconds N` | yes | Set the lock for N seconds (clamped 5..120) by writing the control file. |
| `unlock` | yes | Clear the lock. |
| `status` | no | Show whether the service exists and its state, the helper mode, the lock deadline and seconds left (or that it was ignored, and why), the last 15 log lines, and the helper report from the data folder, the temp folder, or both, each labelled with its path and time. |
| `bandcheck` | no | Create one topmost 1 by 1 window that is never shown, print the two report lines for this process (its UIAccess flag, integrity level and that window's z order band, then its process security descriptor), then destroy the window. A safe self check of the reading code. |
| `probe --after N` | no | Wait N seconds (clamped 0..60), then, as the current account, try to: stop the service; end every other process whose image path is exactly the installed exe, and for each one it is refused permission to end, open it with the right to rewrite its access list (opened and closed at once, nothing changed); overwrite the control file with 0; create a file in the install folder; delete the installed exe. Each attempt is recorded as succeeded, refused (with the Win32 error code and text), or not applicable (nothing there). Prints the report, saves it to `%TEMP%\limo-spike-probe.txt`, and ends with a summary line. |
| `service` | n/a | The service control manager entry point. If run from a terminal it says so and exits. |

Exit codes: 0 success, 1 an operation failed, 2 refused (bad arguments, needs
administrator, already installed, or uninstall run from the installed copy),
3 the developer overlay was ended with the escape chord.

## Why `probe` exists

While the overlay is up the keyboard is swallowed, so a tester cannot type
`sc stop`, use most of Task Manager, or run `unlock`. `probe` makes the attacks
unattended and writes them down. It only ever touches the spike itself: it
opens only processes named `limo-spike.exe`, ends one only after confirming its
image path is exactly the installed exe, never ends itself, never changes a
process's access list (it only checks whether it could), never creates the
control file if it is missing, and removes any test file it manages to create.
Run it from a copy outside Program Files (see TESTING.md), otherwise deleting
the installed exe is refused only because the file is in use.

## Files it uses when installed

- `%ProgramFiles%\LiMoSpike\limo-spike.exe` : the installed binary.
- `%ProgramData%\LiMoSpike\config.txt` : the helper mode, `user`, `system` or `guarded`.
- `%ProgramData%\LiMoSpike\control.txt` : one line, the unix time until which the lock holds. `0` means cleared.
- `%ProgramData%\LiMoSpike\spike.log` : timestamped log of service starts and stops, helper launches, exits and relaunches, and errors.
- `%ProgramData%\LiMoSpike\helper-report.txt` : what the last SYSTEM helper got (see below). A helper that runs as the user (`user` or `guarded` mode) cannot write here, so its report goes to `%TEMP%\helper-report.txt` of the account it runs as.

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
- `guarded`: the service gets the user's token exactly as in `user` mode,
  raises the token's integrity level to medium plus, and starts the helper
  with a protective access list on its process object. The helper runs as the
  user, one integrity step above the user's ordinary programs, and the user
  may not end it. The next section says why.

In every mode the service also sets the UIAccess flag on the duplicated token
before the launch. The UIAccess section below says why.

## Why the guarded mode exists

The first two modes were tried on real hardware on 17 November 2026:

- `user` mode: a standard account can end the helper (the probe of that day,
  before it gained its access list attempt, said "5 refused, 1 succeeded").
  The service brings it back, but a child could end it over and over in a
  loop.
- `system` mode: works, and the lock windows sit in the UIAccess band. But a
  SYSTEM process drawing UI in a user's session is the thing Windows guidance
  warns against: any bug in the lock screen's input handling (it will later
  take a guardian PIN) becomes a path to SYSTEM.
- UIAccess worked in both modes: `uiaccess=1`, band 2 on both monitors.

`guarded` follows how Windows itself runs assistive technology for a standard
user: the process runs as the user, with UIAccess, at an integrity level
slightly above medium, so a bug in it yields nothing more than the user's own
rights. Two layers then protect it from the user's own ordinary programs, and
they divide the work:

- The integrity level (medium plus) blocks the user's ordinary medium
  integrity programs from injecting code into the helper, posting messages to
  its windows, tampering with its threads (ending or suspending a thread is a
  write right), and rewriting its access list. Windows' integrity rules block
  writes from below; they do not block ending a process, because Windows counts
  that right among the execute rights, which pass from below.
- The access list on the helper's process object is what takes away the right
  to end it. Without it, the process gets Windows' default access list, which
  comes from the user's own token and grants the user everything, including
  ending it. `bandcheck` shows that default: owner the user, full access
  (0x1fffff) for the user.

How it is done:

1. After setting UIAccess, the service calls
   `SetTokenInformation(TokenIntegrityLevel)` with the label SID `S-1-16-8448`,
   that is 0x2100, winnt.h's medium plus (`SECURITY_MANDATORY_MEDIUM_PLUS_RID`,
   medium plus 0x100). Raising a level needs SeTcbPrivilege, which the service
   enables at start.
2. It builds a security descriptor for the helper's process object, from
   SDDL, with the user's SID read from the helper's own token, and passes it
   to CreateProcessAsUserW as the process attributes. The process object then
   has it from the moment it exists, with no gap between launch and
   protection. Threads are left with their defaults: the integrity level
   already blocks tampering with them from below.
3. There are two variants of that descriptor, tried in this order; the first
   that launches is kept:
   - `admin-owner`:
     ```
     O:BAG:SYD:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x101000;;;<user SID>)S:(ML;;NWNR;;;S-1-16-8448)
     ```
     The owner is Administrators, so the user is not the owner and gets no
     implied right to rewrite the access list. Windows may refuse this owner
     for a process started with the user's token (Win32 error 1307, invalid
     owner), because Administrators is not a group the user's token may name
     as owner.
   - `owner-rights`:
     ```
     D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x101000;;;<user SID>)(A;;0x101000;;;OW)S:(ML;;NWNR;;;S-1-16-8448)
     ```
     No owner or group is named, so Windows picks them, and may make the user
     the owner. The OWNER RIGHTS entry (`OW`) then caps whatever the owner may
     do to the same rights the user gets, so being the owner gives no right to
     rewrite the access list.

   In both, SYSTEM and Administrators get full access, and the user gets only
   0x101000, that is SYNCHRONIZE and PROCESS_QUERY_LIMITED_INFORMATION: the
   user can see the process and wait for it, and nothing else. The label
   matches the level the token really has: medium plus, or medium if raising
   it failed.

Every step that fails still ends in a launch, because a weaker lock is better
than no lock. If raising the level fails, the helper runs at the user's own
level. If a variant cannot be built, or the launch with it fails for any
reason, the next variant is tried; if both fail, the helper is launched with
no descriptor, so a mistake in the descriptors can never mean no lock. The
launch line says which access list the helper got: `dacl=admin-owner`,
`dacl=owner-rights`, or `dacl=failed`. The Win32 error of every variant that
failed is logged once per lock, naming the variant, so one hardware run shows
which variant Windows accepts. A refused `admin-owner` followed by
`dacl=owner-rights` is a finding, not a failure.

What is and is not proven:

- Proven by unit tests: the mode is accepted, stored and shown; the launch
  line and the helper report carry the new fields; both descriptor texts are
  the ones above, and both parse, for the account running the tests, into
  descriptors that read back the same; the variants are tried in order and
  any failure moves on to the next.
- Not yet proven on real hardware: that the integrity call succeeds, which
  variant Windows accepts at launch, that the helper then reports 0x2100 with
  UIAccess, band 2 and that descriptor, and above all that a normal (not
  elevated) program of the same account is refused both when it tries to end
  the helper and when it tries to open it to rewrite its access list. Part G
  of TESTING.md checks all of this from the owner's own account.
- Not something this mode can stop: an elevated program sits above medium
  plus and holds the Administrators group, so it can still end the helper. On
  an administrator account Windows usually runs Task Manager elevated, so End
  task may work there; that does not count against the mode. A child's
  account is standard, where neither applies.

## Staying above other always on top windows (UIAccess)

The first run on real hardware found a hole. Another ordinary always on top
app (a desktop ticker that puts itself back on top about once a second) kept
showing over the lock, flickering, because both windows were in the same z
order band and each kept putting itself on top. The mouse still works during a
lock, so anything that shows there can also be clicked.

Windows keeps windows in z order bands. Topmost windows of a process whose
token carries the UIAccess flag go into the UIAccess band, which sits above
every ordinary topmost window. So the service now sets that flag on the
helper's primary token, in every launch mode, before starting the helper:
`SetTokenInformation(TokenUIAccess)` with a 4 byte value of 1. That needs
SeTcbPrivilege, which the LocalSystem service enables at start. No code signing
or manifest is needed when the flag is set on the token this way.

If setting the flag fails, the helper is launched anyway: a lock without
UIAccess is better than no lock. The launch lines in the log carry
`uiaccess=1` or `uiaccess=0`, then `integrity=` (`set` or `failed` in guarded
mode) and `dacl=` (`admin-owner`, `owner-rights` or `failed` in guarded mode),
both `unchanged` in the other two modes (a relaunch line has `gap_ms` after
that), and a failure's Win32 error is logged once per lock, not on every
relaunch.

To show what a helper really got, it writes `helper-report.txt` once its
windows are up. The first line has the time, its pid, `managed` or
`developer`, its own UIAccess flag and integrity level read back from its
token, and the z order band of each window. The second line has its own
process security descriptor read back with GetSecurityInfo (owner, access
list and, when it can be read, the integrity label), as SDDL, so it shows
which owner and access list Windows really applied. For a guarded helper
launched with `admin-owner`, for example (user SID shortened):

```
time=2026-11-17 10:00:00 pid=4242 mode=managed uiaccess=1 integrity=0x2100 (medium plus) windows=2 band[1]=2 (UIAccess band) band[2]=2 (UIAccess band)
process_sddl=O:BAD:P(A;;0x1fffff;;;SY)(A;;0x1fffff;;;BA)(A;;0x101000;;;S-1-5-21-...-1001)S:(ML;;NWNR;;;MP)
```

With `owner-rights`, the owner is whatever Windows picked (likely the user's
own SID) and the access list ends with `(A;;0x101000;;;OW)`. That second line
is a forecast: how Windows prints the applied descriptor (for example 0x1fffff
for full access, `MP` for the medium plus label) is only seen on real
hardware. A failed read prints `process_sddl=unknown (Win32 error N)`.

The integrity level is printed as hex, named only for the known values: 0x1000
low, 0x2000 medium, 0x2100 medium plus, 0x3000 high, 0x4000 system. Any other
value is printed as the raw hex. A `user` helper should say medium, a
`guarded` helper medium plus, and a `system` helper system.

The band comes from user32's `GetWindowBand`, which is undocumented, so it is
looked up at run time; if it is missing, or the call fails, the band is
reported as unknown. Band numbers are undocumented too, so the raw number is
always printed and only 1 (the normal desktop band) and 2 (the UIAccess band)
are labelled. The SYSTEM helper writes the report to the data folder. A helper
that runs as the user (`user` or `guarded` mode) cannot write there, by
design, so it writes to its temp folder.
The data folder is never created for this. Writing the report never stops or
delays the lock.

The developer `overlay --seconds N` command cannot get UIAccess: only a helper
started by the service can, because only the service can set the flag. So an
always on top app such as the ticker still shows over a developer overlay.
That is expected; Part U of TESTING.md checks the real lock.

`bandcheck` is a safe self check of the reading code. It creates one hidden 1
by 1 topmost window, never shown, prints the two report lines for it,
destroys it and exits 0. From a normal terminal the first line should say
`uiaccess=0`, `integrity=0x2000 (medium)` and band 1, and the second should
show an ordinary process's default descriptor: owner your own SID, full
access (0x1fffff) for you and for SYSTEM, and the label
`S:AI(ML;;NWNR;;;ME)` (medium).

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
the helper mode names, the probe's result classification and formatting, its
access list attempt and the summaries TESTING.md expects, the helper report
lines with their band and integrity labels, the choice of where the report is
written, the medium plus label SID, both guarded security descriptors (their
text, and that they parse for the account running the tests), the order the
variants are tried in, and the launch log lines.

## Try it

See `TESTING.md` for the checklist: a safe developer check, an administrator
walk through, and the real test from a standard account, in the `user` and
`system` helper modes, then a check of the `guarded` mode in your own account.
