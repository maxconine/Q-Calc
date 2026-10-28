<#
.SYNOPSIS
  Builds the native app's installer and, when the updater key is at hand, its update feed entry.

.DESCRIPTION
  1. cargo build --release -p qcalc-win with QCALC_VERSION set (the version the app compares in its update checks:
     package.json's, the release tag's).
  2. makensis qcalc.nsi -> out\Q-Calc-<version>-<arch>-native-setup.exe (NSIS from PATH, or the copy the Tauri
     CLI keeps in %LOCALAPPDATA%\tauri\NSIS).
  3. With TAURI_SIGNING_PRIVATE_KEY (and _PASSWORD) set, as in .github/workflows/windows.yml: signs the installer
     with the Tauri updater's minisign key (`tauri signer sign`, the same key the app verifies, src-tauri's pubkey)
     and writes out\latest-native.json, the feed update.rs reads:
       { "version", "pub_date", "platforms": { "windows-x86_64": { "url", "signature" } } }
     Upload the installer first and latest-native.json last to the release, so the feed never points at a file
     that isn't there yet. Authenticode signing, if any, must happen before step 3.

.EXAMPLE
  .\build.ps1                       # version from package.json, x64
  .\build.ps1 -Version 2.1.0 -Arch arm64 -BaseUrl https://github.com/maxconine/Q-Calc/releases/download/v2.1.0
#>
param(
  [string]$Version,
  [ValidateSet('x64', 'arm64')][string]$Arch = 'x64',
  [string]$BaseUrl,
  [switch]$SkipBuild,
  # Authenticode: a command that signs the file named by its last argument (e.g. `signtool sign /fd sha256 /tr
  # http://timestamp.digicert.com /td sha256 /a`). makensis runs it on uninstall.exe as it's written and on the
  # installer at the end (qcalc.nsi SIGN). scripts\sign-check.ps1 runs this path with a throwaway certificate
  [string]$SignCommand
)
$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
$repo = Resolve-Path "$here\..\..\.."
$native = Resolve-Path "$here\..\.."
if (-not $Version) { $Version = (Get-Content "$repo\package.json" -Raw | ConvertFrom-Json).version }
if (-not $BaseUrl) { $BaseUrl = "https://github.com/maxconine/Q-Calc/releases/download/v$Version" }
$target = if ($Arch -eq 'arm64') { 'aarch64-pc-windows-msvc' } else { 'x86_64-pc-windows-msvc' }
$platform = if ($Arch -eq 'arm64') { 'windows-aarch64' } else { 'windows-x86_64' }

if (-not $SkipBuild) {
  $env:QCALC_VERSION = $Version
  Push-Location $native
  try { cargo build --release -p qcalc-win --target $target; if ($LASTEXITCODE) { throw "cargo build failed" } }
  finally { Pop-Location; Remove-Item Env:QCALC_VERSION -ErrorAction SilentlyContinue }
}
$exe = "$native\target\$target\release\qcalc.exe"
if (-not (Test-Path $exe)) { throw "no $exe" }

$makensis = (Get-Command makensis -ErrorAction SilentlyContinue).Source
if (-not $makensis) { $makensis = "$env:LOCALAPPDATA\tauri\NSIS\makensis.exe" }
if (-not (Test-Path $makensis)) { throw "makensis not found: install NSIS 3 or run a Tauri build once" }

$out = "$here\out"
New-Item -ItemType Directory -Force $out | Out-Null
$setup = "$out\Q-Calc-$Version-$Arch-native-setup.exe"
# the numeric part for the version resource (a pre-release "2.1.0-beta.1" is 2.1.0)
$versionNum = ($Version -split '[-+]')[0]
if ($versionNum -notmatch '^\d+\.\d+\.\d+$') { throw "not a version: $Version" }
$nsisArgs = @('/V2', "/DVERSION=$Version", "/DVERSION_NUM=$versionNum", "/DEXE=$exe", "/DOUT=$setup", "/DOUTNAME=$(Split-Path $setup -Leaf)")
if ($SignCommand) { $nsisArgs += "/DSIGN=$SignCommand" }
& $makensis @nsisArgs "$here\qcalc.nsi"
if ($LASTEXITCODE) { throw "makensis failed" }
"installer: $setup ($([math]::Round((Get-Item $setup).Length / 1MB, 2)) MB)"

if (-not $env:TAURI_SIGNING_PRIVATE_KEY) {
  'no TAURI_SIGNING_PRIVATE_KEY: not signed, no latest-native.json'
  return
}
Push-Location $repo
try { npx tauri signer sign --app-version $Version $setup; if ($LASTEXITCODE) { throw "signing failed" } }
finally { Pop-Location }
$sig = (Get-Content "$setup.sig" -Raw).Trim()
# the signature must name this version (update.rs refuses one that doesn't)
$text = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($sig))
if ($text -notmatch "version:$([regex]::Escape($Version))") { throw "the signature's trusted comment doesn't name $Version" }

$feed = "$out\latest-native.json"
$manifest = if (Test-Path $feed) { Get-Content $feed -Raw | ConvertFrom-Json } else { $null }
if (-not $manifest -or $manifest.version -ne $Version) {
  $manifest = [pscustomobject]@{ version = $Version; pub_date = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ'); platforms = [pscustomobject]@{} }
}
$entry = [pscustomobject]@{ signature = $sig; url = "$($BaseUrl.TrimEnd('/'))/$([uri]::EscapeDataString((Split-Path $setup -Leaf)))" }
$manifest.platforms | Add-Member -NotePropertyName $platform -NotePropertyValue $entry -Force
# UTF-8 without a byte-order mark (PowerShell 5's Set-Content -Encoding utf8 writes one)
[IO.File]::WriteAllText($feed, ($manifest | ConvertTo-Json -Depth 5))
"feed: $feed ($platform)"
