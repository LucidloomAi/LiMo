# Runs the release exe in self check mode several times and measures it.
#
#   powershell -ExecutionPolicy Bypass -File tools\run-self-check.ps1 [-Runs 5] [-HoldMs 3000] [-FreshProfile]
#
# Run from the spike folder. For each run it starts the exe with
# --self-check logs\run-<n>.log --hold-ms <HoldMs>, reads the memory of the
# exe and all its child processes (the WebView2 processes) once the page has
# reported everything, waits for the exit code, then checks that no process
# of this proof is left. -FreshProfile deletes the proof's WebView2 data folder
# before the first run, so that run starts like a first start on a new PC.
# It also notes whether the proof's window was ever the foreground window
# (sampled every 100 ms). Writes logs\summary.csv. No paths in the output.

param(
  [int]$Runs = 5,
  [int]$HoldMs = 3000,
  [switch]$FreshProfile
)

$ErrorActionPreference = 'Stop'
$spike = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $spike 'app\src-tauri\target\release\limo-console-proof.exe'
$dataDir = Join-Path $spike 'app\src-tauri\target\release\webview-data'
$logs = Join-Path $spike 'logs'
New-Item -ItemType Directory -Force $logs | Out-Null

if (-not (Test-Path $exe)) { throw 'build the exe first (see README.md)' }

function Get-Tree([int]$rootId) {
  $all = Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId, Name, WorkingSetSize, PrivatePageCount
  $ids = New-Object System.Collections.Generic.HashSet[int]
  [void]$ids.Add($rootId)
  $grew = $true
  while ($grew) {
    $grew = $false
    foreach ($p in $all) {
      if ($ids.Contains([int]$p.ParentProcessId) -and -not $ids.Contains([int]$p.ProcessId)) {
        [void]$ids.Add([int]$p.ProcessId); $grew = $true
      }
    }
  }
  $all | Where-Object { $ids.Contains([int]$_.ProcessId) }
}

Add-Type -Namespace LiMoProof -Name User32 -MemberDefinition @'
[DllImport("user32.dll")] public static extern System.IntPtr GetForegroundWindow();
[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr hWnd, out uint processId);
'@

function Get-ForegroundPid {
  $h = [LiMoProof.User32]::GetForegroundWindow()
  $fgPid = [uint32]0
  [void][LiMoProof.User32]::GetWindowThreadProcessId($h, [ref]$fgPid)
  return [int]$fgPid
}

function Get-Leftovers {
  # Our exe by name, and WebView2 processes whose command line names our data folder.
  $mine = @(Get-CimInstance Win32_Process -Filter "Name = 'limo-console-proof.exe'")
  $wv = @(Get-CimInstance Win32_Process -Filter "Name = 'msedgewebview2.exe'" |
      Where-Object { $_.CommandLine -and $_.CommandLine.IndexOf($dataDir, [StringComparison]::OrdinalIgnoreCase) -ge 0 })
  return @($mine + $wv)
}

if ($FreshProfile -and (Test-Path $dataDir)) { Remove-Item -Recurse -Force $dataDir }

$before = @(Get-Leftovers)
if ($before.Count -gt 0) { throw "processes of this proof are already running: $($before.Count)" }

$rows = @()
for ($n = 1; $n -le $Runs; $n++) {
  $logRel = "logs\run-$n.log"
  $log = Join-Path $spike $logRel
  if (Test-Path $log) { Remove-Item -Force $log }
  $sw = [Diagnostics.Stopwatch]::StartNew()
  $p = Start-Process -FilePath $exe -ArgumentList @('--self-check', $logRel, '--hold-ms', $HoldMs) -WorkingDirectory $spike -PassThru
  $null = $p.Handle  # keeps the handle so ExitCode can be read later
  $mem = $null
  $tookFocus = $false
  while (-not $p.HasExited -and $sw.Elapsed.TotalSeconds -lt 30) {
    if ((Get-ForegroundPid) -eq $p.Id) { $tookFocus = $true }
    if ($null -eq $mem -and (Test-Path $log) -and (Select-String -Path $log -Pattern '^\s*\d+ ms  done:' -Quiet)) {
      Start-Sleep -Milliseconds 300
      $tree = @(Get-Tree $p.Id)
      $mem = [pscustomobject]@{
        Processes = $tree.Count
        WorkingSetMB = [math]::Round((($tree | Measure-Object WorkingSetSize -Sum).Sum) / 1MB, 1)
        PrivateMB = [math]::Round((($tree | Measure-Object PrivatePageCount -Sum).Sum) / 1MB, 1)
        ExeWorkingSetMB = [math]::Round((($tree | Where-Object Name -eq 'limo-console-proof.exe' | Measure-Object WorkingSetSize -Sum).Sum) / 1MB, 1)
      }
    }
    Start-Sleep -Milliseconds 100
  }
  if (-not $p.HasExited) { $p.Kill(); $p.WaitForExit() }
  $p.WaitForExit()
  $wall = [math]::Round($sw.Elapsed.TotalMilliseconds)
  Start-Sleep -Milliseconds 1500
  $left = @(Get-Leftovers)
  foreach ($l in $left) { Stop-Process -Id $l.ProcessId -Force -ErrorAction SilentlyContinue }

  $text = if (Test-Path $log) { Get-Content $log } else { @() }
  function Ms([string]$key) {
    $line = $text | Where-Object { $_ -match "^\s*(\d+) ms  $([regex]::Escape($key)):" } | Select-Object -First 1
    if ($line -match '^\s*(\d+) ms') { [int]$Matches[1] } else { $null }
  }
  $rows += [pscustomobject]@{
    Run = $n
    ExitCode = $p.ExitCode
    WindowMs = Ms 'window_created'
    FirstPageLoadMs = Ms 'page_load_finished'
    FirstRenderMs = Ms 'page today_rendered'
    DoneMs = Ms 'done'
    WallMs = $wall
    Processes = if ($mem) { $mem.Processes } else { $null }
    WorkingSetMB = if ($mem) { $mem.WorkingSetMB } else { $null }
    PrivateMB = if ($mem) { $mem.PrivateMB } else { $null }
    ExeWorkingSetMB = if ($mem) { $mem.ExeWorkingSetMB } else { $null }
    TookFocus = $tookFocus
    LeftoversEnded = $left.Count
  }
}

$csv = ($rows | ConvertTo-Csv -NoTypeInformation) -join "`r`n"
$note = "HoldMs=$HoldMs FreshProfile=$FreshProfile (WallMs includes the hold)"
Set-Content -Path (Join-Path $logs 'summary.csv') -Value ($note + "`r`n" + $csv) -Encoding utf8
$note
$csv
