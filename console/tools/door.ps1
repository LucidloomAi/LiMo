# Starts and stops LiMo for Windows' own door for a self check: the example
# examples\real_door.rs, in the builder's own account, at pipe names of its
# own (never LiMo's door), with real family files in temporary folders and
# a new password each time, written to a file, never on a command line.
# Dot sourced by run-self-check.ps1 and picture.ps1.
#
# Build it first, once: cargo build --example real_door --no-default-features

function Start-RealDoor([string]$Dir) {
  $console = Split-Path -Parent $PSScriptRoot
  $exe = Join-Path $console 'target\debug\examples\real_door.exe'
  if (-not (Test-Path $exe)) { throw 'build the door first: cargo build --example real_door --no-default-features' }
  New-Item -ItemType Directory -Force $Dir | Out-Null
  $base = "\\.\pipe\limo.console.self-check.$PID.$(Get-Random)"
  $password = Join-Path $Dir 'door-password.txt'
  $out = Join-Path $Dir 'door-out.txt'
  foreach ($f in @($password, "$password.stop", $out)) { if (Test-Path $f) { Remove-Item -Force $f } }
  # Its own console window stays hidden; it stops by itself after 58 s.
  $p = Start-Process -FilePath $exe -ArgumentList @($base, $password, '58') -RedirectStandardOutput $out -WindowStyle Hidden -PassThru
  $null = $p.Handle
  $sw = [Diagnostics.Stopwatch]::StartNew()
  while (-not ((Test-Path $out) -and (Select-String -Path $out -Pattern '^ready' -Quiet))) {
    if ($p.HasExited -or $sw.Elapsed.TotalSeconds -gt 30) { throw 'the real door did not start' }
    Start-Sleep -Milliseconds 50
  }
  return [pscustomobject]@{ Process = $p; Base = $base; PasswordFile = $password; Server = (Resolve-Path $exe).Path; Out = $out }
}

function Stop-RealDoor($Door) {
  New-Item -ItemType File -Force "$($Door.PasswordFile).stop" | Out-Null
  if (-not $Door.Process.WaitForExit(15000)) { $Door.Process.Kill() }
  Get-Content $Door.Out | Where-Object { $_ -like 'door:*' }
}

function Get-DoorArguments($Door) {
  return @('--door-base', $Door.Base, '--door-server', $Door.Server, '--password-file', $Door.PasswordFile)
}
