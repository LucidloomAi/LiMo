# Runs the release exe's self check once, against LiMo for Windows' own
# door (tools\door.ps1), and prints its log and what the door's service
# said.
#
#   powershell -ExecutionPolicy Bypass -File tools\run-self-check.ps1 [-Name first] [-Fresh] [-More "--probe-keys --no-page-guard"]
#
# Run from the console folder, after tools\build-release.ps1 and
# `cargo build --example real_door --no-default-features`. The log is
# target\self-check\<Name>.log; WebView2's data folder for these runs is
# target\self-check\webview-data, made new first with -Fresh (a first start
# on a new PC). -More holds further options for the exe, apart by spaces:
# --probe-keys, --no-page-guard, --keys-on, --size 390x844, --hold-ms 2000,
# --switches ...
#
# The window is a normal one, not focused and not on top, and closes itself:
# the run ends within a minute whatever happens. The exit code is the self
# check's: 0 when every step was seen.

param(
  [string]$Name = 'self-check',
  [switch]$Fresh,
  [string]$More = ''
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'door.ps1')
$console = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $console 'target\release\limo-console.exe'
if (-not (Test-Path $exe)) { throw 'build the exe first: tools\build-release.ps1' }
$dir = Join-Path $console 'target\self-check'
New-Item -ItemType Directory -Force $dir | Out-Null
$data = Join-Path $dir 'webview-data'
if ($Fresh -and (Test-Path $data)) { Remove-Item -Recurse -Force $data }
$log = Join-Path $dir "$Name.log"
if (Test-Path $log) { Remove-Item -Force $log }

$door = Start-RealDoor $dir
try {
  $sw = [Diagnostics.Stopwatch]::StartNew()
  $arguments = @('--self-check', $log, '--webview-data', $data) + (Get-DoorArguments $door) + @($More -split ' ' | Where-Object { $_ })
  $p = Start-Process -FilePath $exe -ArgumentList $arguments -WorkingDirectory $console -PassThru
  $null = $p.Handle
  if (-not $p.WaitForExit(60000)) { $p.Kill(); $p.WaitForExit() }
} finally {
  $said = Stop-RealDoor $door
}
Get-Content $log
$said
"exit code $($p.ExitCode) after $([int]$sw.Elapsed.TotalMilliseconds) ms"
exit $p.ExitCode
