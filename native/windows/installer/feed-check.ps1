<#
.SYNOPSIS
  Builds latest-native.json from a local native installer the way the release does, and checks that the app's
  own reader (native/windows/src/update.rs) takes it and verifies the installer against it. Publishes nothing.

.DESCRIPTION
  1. Copies the installer (default: the newest out\*-native-setup.exe) into a temp folder.
  2. Makes a throwaway minisign key (`tauri signer generate`, its own random password) and signs the copy as the release does:
     `tauri signer sign --app-version <version>` (the release uses the real updater key; the shipped app refuses
     this one, which the test checks too).
  3. Writes latest-native.json with latest-native.mjs, the script the release workflow runs.
  4. Runs update.rs's `feed_from_a_local_installer` test on it: the manifest parses for this platform, the URL
     names the installer, the signature names the version, and the bytes verify whole and streamed.
  The temp folder and the key are deleted at the end.

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File native\windows\installer\feed-check.ps1
#>
param(
    [string]$Installer,
    # @tauri-apps/cli's tauri.js; default: node_modules here or in the main checkout
    [string]$TauriCli,
    [switch]$KeepTemp
)
$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
$native = (Resolve-Path "$here\..\..").Path
$repo = (Resolve-Path "$here\..\..\..").Path

if (-not $Installer) {
    $Installer = Get-ChildItem "$here\out\*-native-setup.exe" -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 1 -ExpandProperty FullName
}
if (-not $Installer -or -not (Test-Path $Installer)) { throw 'no native installer: run installer\build.ps1 or pass -Installer' }
$name = Split-Path $Installer -Leaf
if ($name -notmatch '^Q-Calc-(.+?)-(x64|arm64)-native-setup\.exe$') { throw "not a release installer name: $name" }
$version = $Matches[1]
$platform = if ($Matches[2] -eq 'arm64') { 'windows-aarch64' } else { 'windows-x86_64' }

if (-not $TauriCli) {
    $common = (git -C $repo rev-parse --path-format=absolute --git-common-dir 2>$null)
    $roots = @($repo)
    if ($common) { $roots += (Split-Path $common -Parent) }
    foreach ($r in $roots) {
        $c = Join-Path $r 'node_modules\@tauri-apps\cli\tauri.js'
        if (Test-Path $c) { $TauriCli = $c; break }
    }
}
if (-not $TauriCli) { throw 'no @tauri-apps/cli: run npm ci, or pass -TauriCli' }

$tmp = Join-Path ([IO.Path]::GetTempPath()) ('qcalc-feed-check-' + [Guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory -Force $tmp | Out-Null
try {
    Copy-Item $Installer (Join-Path $tmp $name)
    $key = Join-Path $tmp 'throwaway.key'
    # a password of its own (Windows PowerShell drops an empty argument)
    $pw = [Guid]::NewGuid().ToString('N')
    node $TauriCli signer generate --ci -p $pw -w $key -f | Out-Host
    if ($LASTEXITCODE) { throw 'tauri signer generate failed' }
    node $TauriCli signer sign -f $key -p $pw --app-version $version (Join-Path $tmp $name) | Out-Host
    if ($LASTEXITCODE) { throw 'tauri signer sign failed' }
    $base = "https://github.com/maxconine/Q-Calc/releases/download/v$version"
    node (Join-Path $here 'latest-native.mjs') $version $base $tmp "$platform=$name"
    if ($LASTEXITCODE) { throw 'latest-native.mjs failed' }
    Get-Content (Join-Path $tmp 'latest-native.json') | Out-Host

    $env:QCALC_FEED_CHECK = $tmp
    $env:QCALC_FEED_PUBKEY = (Get-Content "$key.pub" -Raw).Trim()
    Push-Location $native
    try {
        cargo test -p qcalc-win --release feed_from_a_local_installer -- --ignored --nocapture
        $code = $LASTEXITCODE
    } finally { Pop-Location }
    if ($code) { throw "update.rs didn't take the feed (cargo test exit $code)" }
    "feed check passed: $name, $platform, $version"
} finally {
    Remove-Item Env:QCALC_FEED_CHECK, Env:QCALC_FEED_PUBKEY -ErrorAction SilentlyContinue
    if (-not $KeepTemp) { Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue }
}
