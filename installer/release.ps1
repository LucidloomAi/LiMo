# Publishes LiMo's installer on the download service of thedumbstuff.page,
# which keeps LiMo's installers; the LucidLoom website's LiMo page then
# offers it by itself. For the person who publishes (README.md in this
# folder, "Publishing", says the order):
#
#   powershell -ExecutionPolicy Bypass -File installer\release.ps1 publish          what it would send, and where; sends nothing
#   powershell -ExecutionPolicy Bypass -File installer\release.ps1 publish --yes    sends it, publishes it, reads it back
#   powershell -ExecutionPolicy Bypass -File installer\release.ps1 check            reads what the site offers, and checks it
#
# Options:
#   --file <installer>     the installer (default installer\target\LiMo_<version>_x64-setup.exe,
#                          the version windows\Cargo.toml's), as build.ps1 writes it
#   --site <address>       the site (default https://thedumbstuff.page, or LIMO_DOWNLOADS_SITE)
#   --token-file <file>    a one line file holding the upload token
#
# Publishing takes only what build.ps1 wrote for release: never a dry build
# (its name says so), the SHA-256 beside it matching, a Windows program, and
# inside it LiMo's child form as a release build (the check build.ps1 makes)
# signed with a key of its own (an APK signing block, not Android's debug
# certificate). It says what it will send (the file, its size, its SHA-256,
# the version, where to), and sends nothing without --yes. With --yes it
# uploads the file with the token (PUT /api/downloads/limo/<version>/<file>),
# publishes it (POST /api/downloads/limo/<version>/publish), then reads
# /downloads/limo/current/release.json back and checks the size and the
# SHA-256, and every field as the LucidLoom page checks it.
#
# The upload token: DOWNLOADS_UPLOAD_TOKEN in the environment, or the one line
# file --token-file names (the downloads site's own docs/installer.md, "The
# upload token", says where the publishing PC keeps it). It is read only with
# --yes, after every check, and sent only in the Authorization header of the
# upload and the publish, to the site named. It is never printed, never
# written anywhere, never put in an error or on a command line.

$ErrorActionPreference = 'Stop'
$installer = $PSScriptRoot
$repo = Split-Path -Parent $installer
$target = Join-Path $installer 'target'
$sevenZip = Join-Path $env:ProgramFiles '7-Zip\7z.exe'
$product = 'limo'
$defaultSite = 'https://thedumbstuff.page'
$stableName = 'LiMo-setup.exe'
$maxSize = 100 * 1024 * 1024   # the site's limit for LiMo: one request (Cloudflare's 100 MB)

function Say([string]$what) { Write-Host "[release] $what" }
function Stop-Release([string]$why) { Write-Host ''; Write-Host "[release] STOPPED: $why"; exit 1 }
function Get-Version([string]$manifest) {
  $text = Get-Content $manifest -Raw
  $m = [regex]::Match($text, '(?ms)^\[package\].*?^version\s*=\s*"([^"]+)"')
  if (-not $m.Success) { Stop-Release "no version in $manifest" }
  return $m.Groups[1].Value
}
function Get-Shown([string]$path) {
  # A path inside the checkout is said relative to it; any other, by its name only.
  $full = [IO.Path]::GetFullPath($path)
  if ($full.StartsWith($repo + '\', [StringComparison]::OrdinalIgnoreCase)) { return $full.Substring($repo.Length + 1) }
  return [IO.Path]::GetFileName($full)
}

# ---------------------------------------------------------------- the command
$command = $null; $yes = $false; $file = $null; $tokenFile = $null
$site = if ($env:LIMO_DOWNLOADS_SITE) { $env:LIMO_DOWNLOADS_SITE } else { $defaultSite }
$usage = 'Usage: release.ps1 publish [--yes] [--file <installer>] [--site <address>] [--token-file <file>] | release.ps1 check [--site <address>]'
for ($i = 0; $i -lt $args.Count; $i++) {
  $a = [string]$args[$i]
  switch -CaseSensitive ($a) {
    'publish' { if ($command) { Write-Host $usage; exit 2 }; $command = 'publish' }
    'check' { if ($command) { Write-Host $usage; exit 2 }; $command = 'check' }
    '--yes' { $yes = $true }
    '--file' { $i++; if ($i -ge $args.Count) { Write-Host $usage; exit 2 }; $file = [string]$args[$i] }
    '--site' { $i++; if ($i -ge $args.Count) { Write-Host $usage; exit 2 }; $site = [string]$args[$i] }
    '--token-file' { $i++; if ($i -ge $args.Count) { Write-Host $usage; exit 2 }; $tokenFile = [string]$args[$i] }
    default { Write-Host $usage; exit 2 }
  }
}
if (-not $command) { Write-Host $usage; exit 2 }
if ($command -eq 'check' -and ($yes -or $file -or $tokenFile)) { Write-Host $usage; exit 2 }
$site = $site.TrimEnd('/')
if ($site -notmatch '^https?://[A-Za-z0-9.\-]+(:\d{1,5})?$') { Stop-Release "--site is an address such as $defaultSite, with nothing after the host" }
if ($site -like 'http://*' -and $site -notmatch '^http://(127\.0\.0\.1|localhost)(:\d+)?$') { Stop-Release 'a site other than this PC is reached over https only' }

# ---------------------------------------------------------------- the site
Add-Type -AssemblyName System.Net.Http
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
[Net.ServicePointManager]::Expect100Continue = $false
$client = New-Object System.Net.Http.HttpClient
$client.Timeout = [TimeSpan]::FromMinutes(30)
$client.DefaultRequestHeaders.UserAgent.ParseAdd('limo-release')

# One request; the answer's status and text. A failure to reach the site
# stops with its own words (never a header of the request).
function Send-Request([string]$method, [string]$path, $content, [string]$token) {
  $req = New-Object System.Net.Http.HttpRequestMessage((New-Object System.Net.Http.HttpMethod($method)), ($site + $path))
  $req.Headers.ExpectContinue = $false
  if ($method -eq 'GET') { $req.Headers.CacheControl = New-Object System.Net.Http.Headers.CacheControlHeaderValue -Property @{ NoCache = $true } }
  if ($token) { $req.Headers.Authorization = New-Object System.Net.Http.Headers.AuthenticationHeaderValue('Bearer', $token) }
  if ($null -ne $content) { $req.Content = $content }
  try {
    $resp = $client.SendAsync($req).GetAwaiter().GetResult()
    $text = $resp.Content.ReadAsStringAsync().GetAwaiter().GetResult()
    return @{ Status = [int]$resp.StatusCode; Text = $text }
  } catch {
    $e = $_.Exception
    while ($e.InnerException) { $e = $e.InnerException }
    Stop-Release "the site did not answer at $site$path ($($e.Message.Trim().TrimEnd('.'))). Nothing more was sent."
  } finally { $req.Dispose() }
}
function Read-Json([string]$text) { try { return ($text | ConvertFrom-Json) } catch { return $null } }

# What the site offers: $null when nothing is published (404), else release.json.
function Get-Release {
  $r = Send-Request 'GET' "/downloads/$product/current/release.json" $null $null
  if ($r.Status -eq 404) { return $null }
  if ($r.Status -ne 200) { Stop-Release "the site answered $($r.Status) for /downloads/$product/current/release.json" }
  $json = Read-Json $r.Text
  if ($null -eq $json) { Stop-Release '/downloads/limo/current/release.json is not JSON' }
  return $json
}

# The fields the LucidLoom website's LiMo page takes (its server.js): the
# Windows entry only; what is wrong with it, or nothing.
function Test-PageTakes($release) {
  $win = $null
  if ($release -and $release.systems) { $win = $release.systems.windows }
  if (-not $win) { return @('there is no Windows entry (systems.windows)') }
  $wrong = @()
  if (-not ($win.file -is [string] -and $win.file -cmatch '^/downloads/limo/[A-Za-z0-9._-]+(/[A-Za-z0-9._-]+)*$' -and -not $win.file.Contains('..'))) { $wrong += "file is not a plain path under /downloads/limo/ ($($win.file))" }
  if (-not ($win.sha256 -is [string] -and $win.sha256 -match '^[0-9a-fA-F]{64}$')) { $wrong += 'sha256 is not 64 hex characters' }
  if (-not (($win.size -is [int] -or $win.size -is [long]) -and $win.size -gt 0)) { $wrong += "size is not a positive whole number ($($win.size))" }
  if (-not ($win.version -is [string] -and $win.version -cmatch '^\d{1,4}\.\d{1,4}\.\d{1,6}([-+][0-9A-Za-z.]{1,32})?$')) { $wrong += "version is not like 1.2.3 ($($win.version))" }
  if ($win.label -and -not ($win.label -is [string] -and $win.label.Trim().Length -gt 0 -and $win.label.Length -le 100)) { $wrong += 'label is longer than 100 characters' }
  return $wrong
}
function Show-Release($win) {
  Say ("  LiMo {0} for Windows, {1}" -f $win.version, $win.label)
  Say ("  published {0}" -f $win.date)
  Say ("  {0}, {1:N0} bytes, SHA-256 {2}" -f $win.file, $win.size, $win.sha256)
}

# The words for a refusal from the downloads service ({error, message}).
function Get-Refusal($r, [string]$version, [string]$what) {
  $json = Read-Json $r.Text
  if (-not ($json -and $json.error)) {
    return "the site answered $($r.Status) to $what, and not as the downloads service does (Cloudflare may have refused it: the downloads site's docs/installer.md, 'If Cloudflare refuses the upload'). Nothing was published."
  }
  switch ($json.error) {
    'unauthorised' { return 'the site refused the token: it is not the upload token the site holds. Ten wrong ones lock this PC''s address out for ten minutes. Nothing was published.' }
    'not_switched_on' { return 'uploads are not switched on at the site (it holds no upload token). Nothing was published.' }
    'asleep' { return 'the downloads service at the site is asleep. Try again in a minute. Nothing was published.' }
    'not_found' { return 'the site does not take LiMo yet: its downloads service has no row for LiMo. Deploy the site with that row first (README.md, Publishing). Nothing was published.' }
    'published' { return "LiMo $version is published already, with other bytes, and the site refused it: a version, once published, is never replaced. A fix is a new version (windows\Cargo.toml and console\Cargo.toml), built again." }
    'older' { return "the site refused it: $($json.message) A newer LiMo is offered already. Nothing was published." }
    default { return "the site refused $what ($($r.Status), $($json.error)): $($json.message) Nothing was published." }
  }
}

if ($command -eq 'check') {
  Say "what $site offers for LiMo (/downloads/limo/current/release.json)"
  $release = Get-Release
  if ($null -eq $release) { Stop-Release 'the site offers no LiMo yet: nothing is published (404). The LucidLoom page says there is no installer yet.' }
  $wrong = Test-PageTakes $release
  if ($release.systems -and $release.systems.windows) { Show-Release $release.systems.windows }
  if ($wrong) { Stop-Release ("the LucidLoom page would not offer it: " + ($wrong -join '; ')) }
  Say '  every field as the LucidLoom page checks it: it offers this download'
  exit 0
}

# ---------------------------------------------------------------- publish: the file
Write-Host "[release] LiMo's installer, for the download service of $site"
if ($site -ne $defaultSite) { Say "  (not ${defaultSite}: a preview or a test)" }
$version = Get-Version (Join-Path $repo 'windows\Cargo.toml')
if (-not $file) { $file = Join-Path $target "LiMo_${version}_x64-setup.exe" }
$name = [IO.Path]::GetFileName($file)
if ($name -match '(?i)dry-build|throwaway') {
  Stop-Release "$name is a dry build, with a throwaway phone app: it is never published. Build for release: build.ps1 -ChildApk <the signed release APK>."
}
$m = [regex]::Match($name, '^LiMo_(\d{1,4}\.\d{1,4}\.\d{1,6})_x64-setup\.exe$')
if (-not $m.Success) { Stop-Release "$name is not an installer build.ps1 writes for release (LiMo_<version>_x64-setup.exe)" }
if ($m.Groups[1].Value -ne $version) {
  Stop-Release "$name is LiMo $($m.Groups[1].Value), and this checkout is LiMo $version (windows\Cargo.toml): build it again, or publish from the checkout it was built from"
}
if (-not (Test-Path $file -PathType Leaf)) { Stop-Release "there is no installer at $(Get-Shown $file): build it first (build.ps1)" }
$size = (Get-Item $file).Length
if ($size -le 0 -or $size -gt $maxSize) { Stop-Release "$name is $size bytes; the site takes LiMo's installer up to $maxSize bytes, in one request" }
$fs = [IO.File]::OpenRead($file)
try { $head = New-Object byte[] 2; $null = $fs.Read($head, 0, 2) } finally { $fs.Dispose() }
if ($head[0] -ne 0x4D -or $head[1] -ne 0x5A) { Stop-Release "$name is not a Windows program (it does not start with MZ)" }
$hash = (Get-FileHash $file -Algorithm SHA256).Hash.ToLowerInvariant()
$sumFile = "$file.sha256"
if (-not (Test-Path $sumFile -PathType Leaf)) { Stop-Release "$name has no $name.sha256 beside it, as build.ps1 writes: build it again" }
$sum = ([IO.File]::ReadAllText($sumFile)).Trim()
if ($sum -cne "$hash  $name") { Stop-Release "$name is not the file build.ps1 wrote: its SHA-256 is not the one in $name.sha256 (changed since, or another file). Build it again." }

# The phone app inside: LiMo's child form, a release build (build.ps1's
# check), signed with a key of its own (not Android's debug certificate).
function Test-Signed([string]$apk) {
  $b = [IO.File]::ReadAllBytes($apk)
  $eocd = -1
  for ($i = $b.Length - 22; $i -ge [Math]::Max(0, $b.Length - 65557); $i--) {
    if ($b[$i] -eq 0x50 -and $b[$i + 1] -eq 0x4B -and $b[$i + 2] -eq 0x05 -and $b[$i + 3] -eq 0x06) { $eocd = $i; break }
  }
  if ($eocd -lt 0) { return 'it is not a zip file' }
  $cd = [long][BitConverter]::ToUInt32($b, $eocd + 16)
  if ($cd -lt 32 -or $cd -gt $eocd) { return 'it is not a zip file' }
  if ([Text.Encoding]::ASCII.GetString($b, [int]($cd - 16), 16) -cne 'APK Sig Block 42') { return 'it is not signed (no APK signing block): a dry build''s throwaway app, or an unsigned build' }
  $blockSize = [BitConverter]::ToUInt64($b, [int]($cd - 24))
  if ($blockSize -lt 24 -or $blockSize -gt ($cd - 8)) { return 'its signing block is damaged' }
  $block = [Text.Encoding]::GetEncoding(28591).GetString($b, [int]($cd - 8 - $blockSize), [int]($blockSize + 8))
  if ($block.Contains('Android Debug')) { return 'it is signed with Android''s debug certificate, not a release key' }
  return $null
}
if (-not (Test-Path $sevenZip)) { Stop-Release "7-Zip is not on this PC: it is needed to look at the phone app inside the installer (as build.ps1 looks inside it)" }
$look = Join-Path $target 'release-look'
if (Test-Path $look) { Remove-Item -Recurse -Force $look }
try {
  & $sevenZip x -y "-o$look" $file 'phone\limo-child.apk' | Out-Null
  $apk = Join-Path $look 'phone\limo-child.apk'
  if ($LASTEXITCODE -ne 0 -or -not (Test-Path $apk -PathType Leaf)) { Stop-Release "$name holds no phone app (phone\limo-child.apk): it is not LiMo's installer" }
  $env:CARGO_TARGET_DIR = Join-Path $target 'cargo'
  try {
    $facts = & cargo run --quiet --manifest-path (Join-Path $repo 'console\Cargo.toml') --no-default-features --example app_facts -- $apk
    $factsOk = $LASTEXITCODE -eq 0
  } finally { Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue }
  if (-not $factsOk) { Stop-Release "the phone app inside $name is not an APK: $facts" }
  $facts = $facts | ConvertFrom-Json
  if ($facts.package -ne 'ai.lucidloom.limo.child') { Stop-Release "the phone app inside is $($facts.package), not LiMo's child form (ai.lucidloom.limo.child)." }
  if ($facts.debuggable) { Stop-Release 'the phone app inside is a debug build: the installer carries the release build, signed with the maker''s key.' }
  $unsigned = Test-Signed $apk
  if ($unsigned) { Stop-Release "the phone app inside $name is not release signed: $unsigned. It is never published." }
  $apkVersion = $facts.version
} finally { if (Test-Path $look) { Remove-Item -Recurse -Force $look } }

$putPath = "/api/downloads/$product/$version/$name"
$publishPath = "/api/downloads/$product/$version/publish"
Say "what would be sent:"
Say "  the file:     $(Get-Shown $file)"
Say ("  its size:     {0:N0} bytes ({1:N1} MB)" -f $size, ($size / 1MB))
Say "  its SHA-256:  $hash (the same as in $name.sha256)"
Say "  the version:  LiMo $version for Windows; inside, LiMo's child form $apkVersion, a release build, signed with a key of its own"
Say "  where to:     $site"
Say "                PUT  $putPath   (the upload token)"
Say "                POST $publishPath   (it becomes the current download)"
Say "  then:         $site/downloads/$product/current/$stableName, which the LucidLoom website's LiMo page offers"

# What the site offers now: a version once published is never replaced.
$release = Get-Release
$now = $null
if ($release -and $release.systems) { $now = $release.systems.windows }
if ($now) {
  Say "the site offers now: LiMo $($now.version)"
  if ($now.version -eq $version) {
    $same = if ($now.sha256 -eq $hash) { 'this same file' } else { 'another file' }
    Stop-Release "LiMo $version is published already ($same): a version, once published, is never replaced. A fix is a new version (windows\Cargo.toml and console\Cargo.toml), built again. Nothing was sent."
  }
  $older = $false
  $a = $version.Split('.') | ForEach-Object { [int]$_ }; $c = ([string]$now.version).Split('.-+')[0..2] | ForEach-Object { [int]$_ }
  for ($k = 0; $k -lt 3; $k++) { if ($a[$k] -ne $c[$k]) { $older = $a[$k] -lt $c[$k]; break } }
  if ($older) { Stop-Release "the site offers LiMo $($now.version), newer than ${version}: an older version is never published over it. Nothing was sent." }
} else {
  Say 'the site offers no LiMo yet: this will be the first'
}
if (-not $yes) {
  Write-Host ''
  Say 'Nothing was sent. To send it and publish it: release.ps1 publish --yes'
  exit 0
}

# ---------------------------------------------------------------- publish: --yes
$token = $env:DOWNLOADS_UPLOAD_TOKEN
if ($tokenFile) {
  if (-not (Test-Path $tokenFile -PathType Leaf)) { Stop-Release '--token-file names no file. Nothing was sent.' }
  $token = ([IO.File]::ReadAllLines($tokenFile) | Where-Object { $_.Trim() } | Select-Object -First 1)
  if ($token) { $token = $token.Trim() }
}
if (-not $token) { Stop-Release 'no upload token: put it in DOWNLOADS_UPLOAD_TOKEN, or give its one line file with --token-file (the downloads site''s docs/installer.md, "The upload token", says where it is kept). Nothing was sent.' }
if ($token.Length -lt 32) { Stop-Release 'the upload token is shorter than the 32 characters the site takes: it is not the token. Nothing was sent.' }

Write-Host ''
Say "uploading $name ..."
$fs = [IO.File]::OpenRead($file)
try {
  $content = New-Object System.Net.Http.StreamContent($fs)
  $content.Headers.ContentType = New-Object System.Net.Http.Headers.MediaTypeHeaderValue('application/octet-stream')
  $r = Send-Request 'PUT' $putPath $content $token
} finally { $fs.Dispose() }
if ($r.Status -ne 200) { $token = $null; Stop-Release (Get-Refusal $r $version 'the upload') }
$got = Read-Json $r.Text
if (-not $got -or $got.sha256 -ne $hash -or [long]$got.size -ne $size) {
  $token = $null
  Stop-Release "the site holds another file than the one sent (its size $($got.size), SHA-256 $($got.sha256)). Nothing was published."
}
if ($got.same) { Say '  the site held this same file already: kept' } else { Say "  uploaded: $($got.size) bytes, SHA-256 $($got.sha256), the same as sent" }

Say "publishing LiMo $version ..."
$body = New-Object System.Net.Http.StringContent('{"notes": ""}', [Text.Encoding]::UTF8, 'application/json')
$r = Send-Request 'POST' $publishPath $body $token
$token = $null
if ($r.Status -ne 200) { Stop-Release (Get-Refusal $r $version 'publishing') }
$done = Read-Json $r.Text
if (-not $done -or $done.version -ne $version -or @($done.systems) -notcontains 'windows') { Stop-Release "the site answered the publish, but not with LiMo $version for Windows: $($r.Text)" }
Say "  published LiMo $version for Windows"

# Read back: what the site now offers, as the LucidLoom page reads it.
Say "reading back /downloads/$product/current/release.json ..."
$release = Get-Release
if ($null -eq $release) { Stop-Release 'the site offers nothing after the publish (404): look at the site (check).' }
$win = $null
if ($release.systems) { $win = $release.systems.windows }
if (-not $win) { Stop-Release 'release.json lists no Windows installer after the publish: look at the site (check).' }
Show-Release $win
$wrong = @()
if ($win.version -ne $version) { $wrong += "its version is $($win.version), not $version" }
if ([long]$win.size -ne $size) { $wrong += "its size is $($win.size), not $size" }
if ($win.sha256 -ne $hash) { $wrong += "its SHA-256 is $($win.sha256), not the file's $hash" }
if ($win.file -ne "/downloads/$product/current/$stableName") { $wrong += "it names $($win.file), not /downloads/$product/current/$stableName" }
$pageWrong = Test-PageTakes $release
if ($pageWrong) { $wrong += $pageWrong }
if ($wrong) { Stop-Release ("what the site offers is not what was sent: " + ($wrong -join '; ')) }
Write-Host ''
Say "done: $site offers LiMo $version for Windows, the size and SHA-256 of the file sent; the LucidLoom page takes every field, and shows it within five minutes (it keeps its answer that long)"
exit 0
