# Takes a picture of the console's own window during a self check, at a
# size, to compare with the drawings: the sign in as the self check leaves
# it, with the words for a service that is not running under the field.
#
#   powershell -ExecutionPolicy Bypass -File tools\picture.ps1 [-Size 1100x760] [-Out target\self-check\sign-in-1100.png]
#
# Run from the console folder, after tools\build-release.ps1 and
# `cargo build --example real_door --no-default-features`. Against LiMo for
# Windows' own door (tools\door.ps1), it starts the
# self check with --hold-ms, waits until the self check is done,
# and asks Windows to draw the console's window, and only that window, into
# a picture (PrintWindow with its full content): nothing else on the screen
# is read. The window is not focused and closes itself within seconds.

param(
  [string]$Size = '1100x760',
  [string]$Out = ''
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'door.ps1')
$console = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $console 'target\release\limo-console.exe'
$dir = Join-Path $console 'target\self-check'
New-Item -ItemType Directory -Force $dir | Out-Null
if (-not $Out) { $Out = Join-Path $dir "sign-in-$Size.png" }
$log = Join-Path $dir "picture-$Size.log"

Add-Type -AssemblyName System.Drawing
Add-Type -Namespace LiMoPicture -Name User32 -MemberDefinition @'
[DllImport("user32.dll")] public static extern bool PrintWindow(System.IntPtr hwnd, System.IntPtr hdc, uint flags);
[DllImport("user32.dll")] public static extern bool GetWindowRect(System.IntPtr hwnd, out RECT rect);
[StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
'@

if (Test-Path $log) { Remove-Item -Force $log }
$door = Start-RealDoor $dir
$p = Start-Process -FilePath $exe -ArgumentList (@('--self-check', $log, '--hold-ms', '3000', '--size', $Size, '--webview-data', (Join-Path $dir 'webview-data')) + (Get-DoorArguments $door)) -PassThru
$null = $p.Handle
$sw = [Diagnostics.Stopwatch]::StartNew()
while (-not $p.HasExited -and $sw.Elapsed.TotalSeconds -lt 20) {
  if ((Test-Path $log) -and (Select-String -Path $log -Pattern '  done:' -Quiet)) { break }
  Start-Sleep -Milliseconds 20
}
Start-Sleep -Milliseconds 200
$p.Refresh()
$hwnd = $p.MainWindowHandle
if ($hwnd -eq [IntPtr]::Zero) { throw 'the console''s window was not found' }
$r = New-Object LiMoPicture.User32+RECT
[void][LiMoPicture.User32]::GetWindowRect($hwnd, [ref]$r)
$bmp = New-Object System.Drawing.Bitmap ($r.Right - $r.Left), ($r.Bottom - $r.Top)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$hdc = $g.GetHdc()
# 2: PW_RENDERFULLCONTENT, which draws what WebView2 composes too.
[void][LiMoPicture.User32]::PrintWindow($hwnd, $hdc, 2)
$g.ReleaseHdc($hdc)
$g.Dispose()
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
$p.WaitForExit(30000) | Out-Null
$null = Stop-RealDoor $door
"picture: $Out ($($r.Right - $r.Left) by $($r.Bottom - $r.Top) with the window's frame), self check exit $($p.ExitCode)"
