# Builds limo-console.exe for release, with the build PC's own folders left
# out of it, and checks that they are.
#
#   powershell -ExecutionPolicy Bypass -File tools\build-release.ps1
#
# Run from the console folder. Steps:
#   1. the console's page, in phone/app (npm run build:console);
#   2. cargo build --release, with each of the build PC's folders that Rust
#      would write into the exe (for panic messages) remapped to a plain
#      name: the user's home folder to "home", Cargo's to "cargo", rustup's
#      to "rustup", the repository to "limo". The flags travel in
#      CARGO_ENCODED_RUSTFLAGS, so a folder with a space in it is one flag;
#   3. a search of the exe for the build PC's user name (as ASCII and as
#      UTF-16, any case), its home folder, its name, and the repository's
#      folder. It prints only how many times each was found, never the names,
#      and fails if any count is not 0.
#
# Writes target\release\limo-console.exe. Nothing is installed, nothing is
# signed, no installer is made.

$ErrorActionPreference = 'Stop'
$console = Split-Path -Parent $PSScriptRoot
$repo = Split-Path -Parent $console
$app = Join-Path $repo 'phone\app'

Write-Host '[release] the console''s page (npm run build:console)'
Push-Location $app
try {
  & node scripts\build-console.mjs
  if ($LASTEXITCODE -ne 0) { throw 'the console''s page did not build' }
} finally { Pop-Location }

$homeDir = $env:USERPROFILE
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $homeDir '.cargo' }
$rustupHome = if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { Join-Path $homeDir '.rustup' }
# When several match, rustc applies the last: the home folder goes first, so
# the folders inside it win.
$flags = @(
  "--remap-path-prefix=$homeDir=home",
  "--remap-path-prefix=$cargoHome=cargo",
  "--remap-path-prefix=$rustupHome=rustup",
  "--remap-path-prefix=$repo=limo"
)
$env:CARGO_ENCODED_RUSTFLAGS = $flags -join [char]0x1f
Write-Host '[release] cargo build --release (paths remapped)'
$sw = [Diagnostics.Stopwatch]::StartNew()
Push-Location $console
try {
  & cargo build --release
  if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
} finally {
  Pop-Location
  Remove-Item Env:\CARGO_ENCODED_RUSTFLAGS -ErrorAction SilentlyContinue
}
Write-Host "[release] built in $([int]$sw.Elapsed.TotalSeconds) s"

$exe = Join-Path $console 'target\release\limo-console.exe'
$bytes = [IO.File]::ReadAllBytes($exe)
$ascii = [Text.Encoding]::GetEncoding(28591).GetString($bytes).ToLowerInvariant()
$utf16 = [Text.Encoding]::Unicode.GetString($bytes).ToLowerInvariant()
$utf16b = [Text.Encoding]::Unicode.GetString($bytes, 1, $bytes.Length - 1).ToLowerInvariant()
function Count([string]$what) {
  $w = $what.ToLowerInvariant()
  $n = 0
  foreach ($text in @($ascii, $utf16, $utf16b)) {
    $at = 0
    while (($at = $text.IndexOf($w, $at, [StringComparison]::Ordinal)) -ge 0) { $n++; $at += $w.Length }
  }
  return $n
}
$checks = [ordered]@{
  'the build PC''s user name' = $env:USERNAME
  'its home folder' = $homeDir
  'the PC''s name' = $env:COMPUTERNAME
  'the repository''s folder' = $repo
}
$bad = 0
foreach ($k in $checks.Keys) {
  $n = Count $checks[$k]
  Write-Host ("[release] {0}: found {1} times" -f $k, $n)
  if ($n -ne 0) { $bad++ }
}
Write-Host ("[release] limo-console.exe: {0:N0} bytes" -f $bytes.Length)
if ($bad -ne 0) { throw 'the exe holds a name or a folder of the build PC' }
Write-Host '[release] the exe holds none of them'
