# Builds LiMo's installer for Windows from a checkout, in order, saying each
# step as it goes (README.md in this folder says how and why):
#
#   powershell -ExecutionPolicy Bypass -File installer\build.ps1 -ChildApk <the signed release APK>
#   powershell -ExecutionPolicy Bypass -File installer\build.ps1            (the APK's path in LIMO_CHILD_APK)
#   powershell -ExecutionPolicy Bypass -File installer\build.ps1 -Throwaway (a dry build, with a throwaway phone app)
#
# Steps: the version, from one place; the phone app, checked; the tools;
# the console's page (the web export); the page's source map, for the
# notices; limo.exe and limo-console.exe for release, with the build PC's
# folders left out of them; the third party notices; the files staged as
# they go on a PC, and the build PC's names looked for in every one; the
# installer, by Tauri's bundler with LiMo's template and hooks; and a look
# inside it: its files against the layout, the names looked for in every
# file it holds, and the hooks read back from the script as compiled.
#
# Writes installer\target\LiMo_<version>_x64-setup.exe and its .sha256.
# Everything it makes is under installer\target (git ignores it); it never
# writes into another folder's target. It runs nothing it builds, installs
# nothing, elevates nothing. A dry build's installer carries a throwaway
# phone app and must never be given to anyone: its name says so.

param(
  [string]$ChildApk = $env:LIMO_CHILD_APK,
  [switch]$Throwaway
)

$ErrorActionPreference = 'Stop'
$installer = $PSScriptRoot
$repo = Split-Path -Parent $installer
$target = Join-Path $installer 'target'
$stage = Join-Path $target 'stage'
$cargoTarget = Join-Path $target 'cargo'
$sevenZip = Join-Path $env:ProgramFiles '7-Zip\7z.exe'
$script:step = 0
$steps = 11

function Step([string]$what) { $script:step++; Write-Host ''; Write-Host ("[installer] {0} of {1}: {2}" -f $script:step, $steps, $what) }
function Say([string]$what) { Write-Host "[installer]   $what" }
function Stop-Build([string]$why) { Write-Host ''; Write-Host "[installer] STOPPED: $why"; exit 1 }
function Invoke-In([string]$dir, [string]$exe, [string[]]$arguments) {
  Push-Location $dir
  try {
    & $exe @arguments
    if ($LASTEXITCODE -ne 0) { Stop-Build "$exe $($arguments -join ' ') failed (exit code $LASTEXITCODE)" }
  } finally { Pop-Location }
}
function Get-Version([string]$manifest) {
  $text = Get-Content $manifest -Raw
  $m = [regex]::Match($text, '(?ms)^\[package\].*?^version\s*=\s*"([^"]+)"')
  if (-not $m.Success) { Stop-Build "no version in $manifest" }
  return $m.Groups[1].Value
}

# The build PC's names, looked for in every file that ships: as ASCII and
# as UTF-16 in either byte alignment, in any case. Only the counts are said.
$homeDir = $env:USERPROFILE
$names = [ordered]@{
  'the build PC''s user name' = $env:USERNAME
  'its home folder' = $homeDir
  'the PC''s name' = $env:COMPUTERNAME
  'the repository''s folder' = $repo
}
function Measure-Names([byte[]]$bytes) {
  $texts = @(
    [Text.Encoding]::GetEncoding(28591).GetString($bytes).ToLowerInvariant(),
    [Text.Encoding]::Unicode.GetString($bytes).ToLowerInvariant()
  )
  if ($bytes.Length -gt 1) { $texts += [Text.Encoding]::Unicode.GetString($bytes, 1, $bytes.Length - 1).ToLowerInvariant() }
  $found = [ordered]@{}
  foreach ($k in $names.Keys) {
    $w = $names[$k].ToLowerInvariant()
    $n = 0
    foreach ($t in $texts) { $at = 0; while (($at = $t.IndexOf($w, $at, [StringComparison]::Ordinal)) -ge 0) { $n++; $at += $w.Length } }
    $found[$k] = $n
  }
  return $found
}
# Every file under a folder, and every entry inside each APK (a zip, whose
# entries are compressed): the counts, file by file; true when all are 0.
function Test-Names([string]$folder, [string]$label) {
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  $bad = 0
  $files = Get-ChildItem $folder -Recurse -File
  foreach ($f in $files) {
    $parts = @(@{ Name = $f.FullName.Substring($folder.Length + 1); Bytes = [IO.File]::ReadAllBytes($f.FullName) })
    if ($f.Extension -eq '.apk') {
      $zip = [IO.Compression.ZipFile]::OpenRead($f.FullName)
      try {
        foreach ($e in $zip.Entries) {
          if ($e.Length -eq 0) { continue }
          $s = $e.Open(); $ms = New-Object IO.MemoryStream; $s.CopyTo($ms); $s.Dispose()
          $parts += @{ Name = "$($f.Name) : $($e.FullName)"; Bytes = $ms.ToArray() }
        }
      } finally { $zip.Dispose() }
    }
    foreach ($p in $parts) {
      $found = Measure-Names $p.Bytes
      $total = ($found.Values | Measure-Object -Sum).Sum
      if ($total -ne 0) {
        $bad++
        Say ("{0}: {1}" -f $p.Name, (($found.GetEnumerator() | Where-Object { $_.Value -ne 0 } | ForEach-Object { "$($_.Key) $($_.Value) times" }) -join ', '))
      }
    }
  }
  Say ("{0}: {1} files, and the entries inside each APK, looked through; {2} holding a name or a folder of the build PC" -f $label, $files.Count, $bad)
  return ($bad -eq 0)
}

Write-Host '[installer] LiMo''s installer for Windows'

# 1. The version, from one place.
Step 'the version'
$version = Get-Version (Join-Path $repo 'windows\Cargo.toml')
$consoleVersion = Get-Version (Join-Path $repo 'console\Cargo.toml')
if ($consoleVersion -ne $version) {
  Stop-Build "LiMo for Windows is $version and the console $consoleVersion. The version is LiMo for Windows' (windows\Cargo.toml); give the console the same (console\Cargo.toml), then build again."
}
Say "LiMo $version, from windows\Cargo.toml (the version limo install records and compares); the console has the same"

# 2. The phone app, checked before anything long is built.
Step 'the phone app'
$env:CARGO_TARGET_DIR = $cargoTarget
try {
  if ($Throwaway) {
    $throwDir = Join-Path $target 'throwaway'
    if (Test-Path $throwDir) { Remove-Item -Recurse -Force $throwDir }
    Invoke-In $repo 'cargo' @('run', '--quiet', '--manifest-path', 'console\Cargo.toml', '--no-default-features', '--example', 'throwaway_apk', '--', $throwDir)
    $ChildApk = Join-Path $throwDir 'phone\limo-child.apk'
    Say 'a throwaway phone app (a dry build): this installer must never be given to anyone'
  } elseif (-not $ChildApk) {
    Stop-Build 'LiMo''s app for phones is missing: give the signed release APK of the child form with -ChildApk <file>, or put its path in the environment variable LIMO_CHILD_APK. (A dry build with a throwaway app: -Throwaway.)'
  } elseif (-not (Test-Path $ChildApk -PathType Leaf)) {
    Stop-Build "LiMo's app for phones is missing: there is no file at $ChildApk."
  }
  $facts = & cargo run --quiet --manifest-path (Join-Path $repo 'console\Cargo.toml') --no-default-features --example app_facts -- $ChildApk
  if ($LASTEXITCODE -ne 0) { Stop-Build "the phone app at $ChildApk is not an APK: $facts" }
  $facts = $facts | ConvertFrom-Json
  if ($facts.package -ne 'ai.lucidloom.limo.child') { Stop-Build "the phone app is $($facts.package), not LiMo's child form (ai.lucidloom.limo.child)." }
  if ($facts.debuggable) { Stop-Build 'the phone app is a debug build: the installer carries the release build, signed with the maker''s key.' }
  Say "LiMo's child form $($facts.version), a release build ($((Get-Item $ChildApk).Length) bytes)"
} finally { Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue }

# 3. The tools.
Step 'the tools'
foreach ($tool in 'node', 'npm', 'cargo') {
  if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { Stop-Build "$tool is not on this PC (README.md says what the build needs)" }
}
Say "node $(node --version), $(cargo --version)"
if (-not (Test-Path (Join-Path $installer 'node_modules\@tauri-apps\cli'))) {
  Say 'Tauri''s bundler, as package-lock.json pins it (npm ci)'
  Invoke-In $installer 'npm' @('ci', '--no-audit', '--no-fund')
}
$tauri = Join-Path $installer 'node_modules\@tauri-apps\cli\tauri.js'
Say "Tauri's bundler: $(node $tauri --version)"
$app = Join-Path $repo 'phone\app'
if (-not (Test-Path (Join-Path $app 'node_modules'))) {
  Say 'the app''s packages, as its package-lock.json pins them (npm ci in phone\app)'
  Invoke-In $app 'npm' @('ci', '--no-audit', '--no-fund')
}
$haveSevenZip = Test-Path $sevenZip
Say ($(if ($haveSevenZip) { '7-Zip, for the look inside the installer' } else { '7-Zip is not on this PC: the look inside the installer is left out' }))

# 4. The console's page.
Step 'the console''s page (the web export, npm run build:console)'
Invoke-In $app 'node' @('scripts\build-console.mjs')

# 5. The page's source map, for the notices: the same bundle again, with a
#    map naming the files in it. It goes under installer\target only.
Step 'the page''s source map, for the notices'
$page = Join-Path $target 'page'
if (Test-Path $page) { Remove-Item -Recurse -Force $page }
New-Item -ItemType Directory -Force $page | Out-Null
# The same settings as build-console.mjs gives its own run.
$saved = @{ LIMO_FORM = $env:LIMO_FORM; EXPO_NO_TELEMETRY = $env:EXPO_NO_TELEMETRY; CI = $env:CI }
$env:LIMO_FORM = 'guardian'; $env:EXPO_NO_TELEMETRY = '1'; $env:CI = '1'
try {
  Invoke-In $app 'npx.cmd' @('expo', 'export:embed', '--platform', 'web', '--dev', 'false', '--minify', 'true',
    '--entry-file', 'console.tsx', '--bundle-output', (Join-Path $page 'console.js'), '--assets-dest', (Join-Path $page 'assets'),
    '--sourcemap-output', (Join-Path $page 'console.js.map'))
} finally {
  foreach ($k in $saved.Keys) {
    if ($null -eq $saved[$k]) { Remove-Item "Env:\$k" -ErrorAction SilentlyContinue } else { Set-Item "Env:\$k" -Value $saved[$k] }
  }
}
$same = (Get-FileHash (Join-Path $page 'console.js')).Hash -eq (Get-FileHash (Join-Path $app 'dist\console\console.js')).Hash
Say ($(if ($same) { 'the same bundle as the page, byte for byte' } else { 'the bundle differs from the page''s in its bytes (the map''s reference): its files are the same' }))

# 6 and 7. The two programs for release, with each of the build PC's
#    folders remapped to a plain name in what Rust writes into an exe (its
#    panic messages), as console\tools\build-release.ps1 does.
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $homeDir '.cargo' }
$rustupHome = if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { Join-Path $homeDir '.rustup' }
$flags = @("--remap-path-prefix=$homeDir=home", "--remap-path-prefix=$cargoHome=cargo", "--remap-path-prefix=$rustupHome=rustup", "--remap-path-prefix=$repo=limo")
$env:CARGO_ENCODED_RUSTFLAGS = $flags -join [char]0x1f
$env:CARGO_TARGET_DIR = $cargoTarget
try {
  Step 'limo.exe for release (LiMo for Windows)'
  $sw = [Diagnostics.Stopwatch]::StartNew()
  Invoke-In $repo 'cargo' @('build', '--release', '--locked', '--manifest-path', 'windows\Cargo.toml', '--bin', 'limo')
  Say "built in $([int]$sw.Elapsed.TotalSeconds) s"
  Step 'limo-console.exe for release (the console, with its page inside)'
  $sw = [Diagnostics.Stopwatch]::StartNew()
  Invoke-In $repo 'cargo' @('build', '--release', '--locked', '--manifest-path', 'console\Cargo.toml', '--bin', 'limo-console')
  Say "built in $([int]$sw.Elapsed.TotalSeconds) s"
} finally {
  Remove-Item Env:\CARGO_ENCODED_RUSTFLAGS -ErrorAction SilentlyContinue
  Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue
}
$limoExe = Join-Path $cargoTarget 'release\limo.exe'
$consoleExe = Join-Path $cargoTarget 'release\limo-console.exe'

# 8. The notices, then the files staged as they go on a PC.
Step 'the third party notices, and the files staged'
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force (Join-Path $stage 'phone') | Out-Null
Invoke-In $installer 'node' @('notices.mjs', (Join-Path $stage 'notices'), (Join-Path $page 'console.js.map'))
Copy-Item $limoExe (Join-Path $stage 'limo.exe')
Copy-Item $ChildApk (Join-Path $stage 'phone\limo-child.apk')
$consoleCopy = Join-Path $target 'console-checked'
if (Test-Path $consoleCopy) { Remove-Item -Recurse -Force $consoleCopy }
New-Item -ItemType Directory -Force $consoleCopy | Out-Null
Copy-Item $consoleExe (Join-Path $consoleCopy 'limo-console.exe')
$staged = Get-ChildItem $stage -Recurse -File | ForEach-Object { $_.FullName.Substring($stage.Length + 1) }
Say "staged: limo-console.exe (from the build), $($staged -join ', ')"
$clean = (Test-Names $stage 'the staged files') -and (Test-Names $consoleCopy 'limo-console.exe')
if (-not $clean) { Stop-Build 'a file that would ship holds a name or a folder of the build PC (above)' }

# 9. The installer, by Tauri's bundler: LiMo's template and hooks, the
#    console as its main program, the staged files as its resources.
Step 'the installer (Tauri''s bundler, NSIS)'
$resources = [ordered]@{}
foreach ($rel in $staged) { $resources[(Join-Path $stage $rel)] = ($rel -replace '\\', '/') }
$config = [ordered]@{
  version = $version
  bundle = [ordered]@{
    active = $true
    targets = @('nsis')
    publisher = 'LucidLoom AI'
    copyright = 'LucidLoom AI. LiMo is free software under the Apache License 2.0.'
    homepage = 'https://lucidloom.ai'
    shortDescription = 'LiMo, a screen time limiter for children'
    resources = $resources
    windows = [ordered]@{
      allowDowngrades = $false
      webviewInstallMode = [ordered]@{ type = 'embedBootstrapper'; silent = $true }
      nsis = [ordered]@{
        template = (Join-Path $installer 'installer.nsi')
        installerHooks = (Join-Path $installer 'hooks.nsh')
        installMode = 'perMachine'
        languages = @('English')
        displayLanguageSelector = $false
        compression = 'lzma'
      }
    }
  }
}
$configFile = Join-Path $target 'bundle.json'
[IO.File]::WriteAllText($configFile, ($config | ConvertTo-Json -Depth 10))
# Tauri's CLI rewrites the features of the console's tauri crates in its
# Cargo.toml to match its configuration; the file is put back as it was,
# byte for byte, so a build never leaves the checkout changed.
$consoleManifest = Join-Path $repo 'console\Cargo.toml'
$manifestBytes = [IO.File]::ReadAllBytes($consoleManifest)
$env:CARGO_TARGET_DIR = $cargoTarget
try {
  Invoke-In (Join-Path $repo 'console') 'node' @($tauri, 'bundle', '--bundles', 'nsis', '--config', $configFile)
} finally {
  Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue
  [IO.File]::WriteAllBytes($consoleManifest, $manifestBytes)
}
$built = Join-Path $cargoTarget "release\bundle\nsis\LiMo_${version}_x64-setup.exe"
if (-not (Test-Path $built)) { Stop-Build "the bundler wrote no installer at $built" }
$outName = if ($Throwaway) { "LiMo_${version}_x64-setup-DRY-BUILD-throwaway-phone-app.exe" } else { "LiMo_${version}_x64-setup.exe" }
$out = Join-Path $target $outName
Copy-Item $built $out -Force
$hash = (Get-FileHash $out -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText("$out.sha256", "$hash  $outName`n")
$nsisVersion = (& (Join-Path $env:LOCALAPPDATA 'tauri\NSIS\makensis.exe') /VERSION).Trim().TrimStart('v')
$keptVersion = [regex]::Match((Get-Content (Join-Path $installer 'notices.mjs') -Raw), "NSIS_VERSION = '([^']+)'").Groups[1].Value
if ($nsisVersion -ne $keptVersion) { Stop-Build "the bundler used NSIS $nsisVersion, and the notices carry NSIS $keptVersion's licence: put its COPYING in installer\licences and its version in notices.mjs" }
Say "$outName, $((Get-Item $out).Length) bytes, SHA256 $hash; NSIS $nsisVersion"

# 10. A look inside it: its files against the layout, and the names.
Step 'a look inside the installer'
$expected = @('limo-console.exe') + $staged | Sort-Object
if ($haveSevenZip) {
  $look = Join-Path $target 'look'
  if (Test-Path $look) { Remove-Item -Recurse -Force $look }
  $listing = & $sevenZip l -slt $out
  if ($LASTEXITCODE -ne 0) { Stop-Build '7-Zip could not read the installer' }
  $paths = $listing | Where-Object { $_ -like 'Path = *' } | Select-Object -Skip 1 | ForEach-Object { $_.Substring(7) }
  $program = $paths | Where-Object { -not $_.StartsWith('$') } | Sort-Object
  $others = $paths | Where-Object { $_.StartsWith('$') } | Sort-Object
  Say "what goes in %ProgramFiles%\LiMo: $($program -join ', ')"
  Say "what the installer uses while it runs, and removes: $($others -join ', ')"
  $diff = Compare-Object $expected $program
  if ($diff) { Stop-Build "the installer's files are not the layout: $(($diff | ForEach-Object { "$($_.SideIndicator) $($_.InputObject)" }) -join '; ')" }
  Say 'the files for the program folder are exactly the layout'
  & $sevenZip x -y "-o$look" $out | Out-Null
  if ($LASTEXITCODE -ne 0) { Stop-Build '7-Zip could not unpack the installer' }
  if (-not (Test-Names $look 'every file inside the installer')) { Stop-Build 'a file inside the installer holds a name or a folder of the build PC (above)' }
} else {
  Say 'no 7-Zip: the files are the bundler''s own resources, as staged above'
}
$outFound = Measure-Names ([IO.File]::ReadAllBytes($out))
if ((($outFound.Values | Measure-Object -Sum).Sum) -ne 0) { Stop-Build "the installer file itself holds a name of the build PC: $(($outFound.GetEnumerator() | ForEach-Object { "$($_.Key) $($_.Value)" }) -join ', ')" }
Say 'the installer file itself holds none of the build PC''s names'

# 11. The hooks, read back from the script as compiled: makensis compiles
#    the bundler's script again with every instruction logged (/V4), macros
#    expanded, and the three commands are looked for in their places. Its
#    own output, a second copy of the installer, is thrown away.
Step 'the hooks, read back'
$nsisDir = Join-Path $cargoTarget 'release\nsis\x64'
Push-Location $nsisDir
try { $compiled = & (Join-Path $env:LOCALAPPDATA 'tauri\NSIS\makensis.exe') '/V4' '/INPUTCHARSET' 'UTF8' 'installer.nsi' } finally { Pop-Location }
$compiledOk = $LASTEXITCODE -eq 0
Remove-Item (Join-Path $nsisDir 'nsis-output.exe') -ErrorAction SilentlyContinue
if (-not $compiledOk) { Stop-Build 'makensis could not compile the generated script again' }
$lines = [string[]]($compiled | ForEach-Object { "$_" })
[IO.File]::WriteAllLines((Join-Path $target 'installer-compiled.log'), $lines)
function Find-Line([string]$pattern, [int]$from = 0) { for ($i = $from; $i -lt $lines.Length; $i++) { if ($lines[$i] -match $pattern) { return $i } } return -1 }
$install = Find-Line '^Section: "Install"'
$uninstall = Find-Line '^Section: "Uninstall"'
$stop = Find-Line '^ExecWait: .*limo-new\\limo\.exe" stop-for-upgrade &' $install
$firstFile = Find-Line '^File: "limo-console\.exe"' $install
$writeUninstaller = Find-Line '^WriteUninstaller:' $install
$set = Find-Line '^ExecWait: .*\$INSTDIR\\limo\.exe" install &' $install
$installEnd = Find-Line '^SectionEnd' $install
$remove = Find-Line '^ExecWait: .*\$INSTDIR\\limo\.exe" uninstall &' $uninstall
$firstDelete = Find-Line '^Delete: "\$INSTDIR\\' $uninstall
$problems = @()
if (-not ($install -ge 0 -and $stop -gt $install -and $firstFile -gt $stop)) { $problems += 'limo stop-for-upgrade is not before the first file is copied' }
if (-not ($set -gt $writeUninstaller -and $set -lt $installEnd)) { $problems += 'limo install is not after the files are copied' }
if (-not ($uninstall -ge 0 -and $remove -gt $uninstall -and $firstDelete -gt $remove)) { $problems += 'limo uninstall is not before the first file is removed' }
# Instructions only: the plugins makensis lists as available (" + ...") are not calls.
$instructions = $lines | Where-Object { $_ -notmatch '^\s\+ ' }
if ($instructions | Where-Object { $_ -match '--dev' }) { $problems += 'the installer passes --dev' }
if ($instructions | Where-Object { $_ -match 'nsExec::' }) { $problems += 'the installer runs something through nsExec' }
if ($instructions | Where-Object { $_ -match '^CreateShortcut' }) { $problems += 'the installer makes a shortcut of its own' }
# No name of the build PC in anything the installer does: only the
# compiler's own lines (what it reads, includes, defines and writes) name
# where things are on the build PC, and none of that is in the installer.
$compilerOnly = '^(Processing |Changing directory|!include|!define|!addplugindir|Output:|NSIS Modern User Interface)'
$runtime = $instructions | Where-Object { $_ -notmatch $compilerOnly } | Where-Object {
  $l = $_.ToLowerInvariant(); @($names.Values | Where-Object { $l.Contains($_.ToLowerInvariant()) }).Count -gt 0
}
if ($runtime) { $problems += "an instruction of the installer names the build PC ($(@($runtime).Count) lines)" }
if ($problems) { Stop-Build ($problems -join '; ') }
foreach ($i in $stop, $firstFile, $writeUninstaller, $set, $remove, $firstDelete) { Say ("{0}: {1}" -f ($i + 1), $lines[$i]) }
Say 'stop-for-upgrade, from the new limo.exe, before the first file; install after the files and the uninstaller; uninstall before anything is removed; no --dev, no nsExec, no shortcut of its own, and no name of the build PC in any instruction'
Write-Host ''
Write-Host "[installer] done: installer\target\$outName"
