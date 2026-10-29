<#
.SYNOPSIS
  The native app's self-update end to end, on this machine, against a loopback stand-in for GitHub: an "old" install
  finds a "new" signed installer, downloads and verifies it, waits for a quiet moment, installs it silently and comes
  back in the tray as the new version with its settings and history. Plus the ways a check must fail safely.
  Publishes nothing.

.DESCRIPTION
  1. build.ps1 builds two installers, 2.0.98 and 2.0.99, from a release exe with the `update-e2e` cargo feature (the
     only build that takes QCALC_UPDATE_FEED / _PUBKEY / _QUIET_SECS; a shipping build ignores them, see update.rs's
     tests) and the app id "Q Calc Update E2E" (qcalc.nsi APP_ID: its own uninstall entry, Run value, shortcut and
     folder key, never the real install's).
  2. A throwaway minisign key (`tauri signer generate`) signs 2.0.99 as the release does (`tauri signer sign
     --app-version`); latest-native.mjs writes the feed; update-server.mjs serves it on 127.0.0.1.
  3. 2.0.98 is installed silently into a temp folder (/D=) and started with its own instance, data, log and %TEMP%.
  4. Failure paths, each refused with the reason settings shows: no feed (404, today's state: quiet, "up to date"),
     an older version on the feed, a signature made for another version, a signature from another key, a truncated
     file, a download cut off, and no server at all.
  5. The real loop: the good feed, auto update on. It checks, downloads, verifies ("ready"), waits until the bar has
     been put away and there's been no input for QCALC_UPDATE_QUIET_SECS, runs the installer (/S /R=hidden) and
     quits; the installer replaces the exe and starts it in the tray. The new copy reports 2.0.99, has the history row
     and the setting made before, and finds itself up to date.
  The real install (%LOCALAPPDATA%\Q Calc, its HKCU entries, Run value and shortcut) is fingerprinted before and
  checked after. Everything made is removed at the end (the test install is uninstalled with its own uninstaller).

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File native\windows\tests\e2e\update-e2e.ps1
#>
param(
    [string]$OutDir = (Join-Path (Get-Location) 'e2e-out\update'),
    # the quiet wait (bar put away and no input), the first check and the retry, seconds
    [int]$QuietSecs = 3,
    # reuse the installers a previous run left in -Work (with -KeepTemp)
    [switch]$SkipBuild,
    [string]$Work,
    [switch]$KeepTemp,
    [string]$TauriCli,
    # the installed version and the one the feed offers (a release's dry run: -New 3.0.0); Old must be above 2.0.97,
    # the "older version" the feed offers in a failure case
    [string]$Old = '2.0.98',
    [string]$New = '2.0.99'
)
$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
$native = (Resolve-Path "$here\..\..\..").Path
$repo = (Resolve-Path "$native\..").Path
$installerDir = (Resolve-Path "$here\..\..\installer").Path
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$OutDir = (Resolve-Path $OutDir).Path
Start-Transcript -Path (Join-Path $OutDir 'log.txt') -Force | Out-Null

$AppId = 'Q Calc Update E2E'
$Instance = 'update-e2e'
$Chord = 'ctrl+alt+shift+f8'
$UninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$AppId"
$DirKey = "HKCU:\Software\maxconine\$AppId"
$RunKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$Shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) "$AppId.lnk"

$Root = if ($Work) { $Work } else { Join-Path ([IO.Path]::GetTempPath()) ('qcalc-update-e2e-' + [Guid]::NewGuid().ToString('N').Substring(0, 8)) }
$Www = Join-Path $Root 'www'; $InstallDir = Join-Path $Root 'install'; $Data = Join-Path $Root 'data'
$Tmp = Join-Path $Root 'tmp'; $Log = Join-Path $Root 'qcalc.log'; $ServerLog = Join-Path $Root 'server.log'
New-Item -ItemType Directory -Force $Root, $Www, $Data, $Tmp | Out-Null

$script:results = New-Object System.Collections.Generic.List[object]
$script:mine = New-Object System.Collections.Generic.List[int]
$script:server = $null
$script:proc = $null

function Log([string]$msg) { Write-Host ("[{0:HH:mm:ss.fff}] {1}" -f (Get-Date), $msg) }

function Wait-Until([scriptblock]$__until, [int]$__ms = 5000, [int]$__step = 100) {
    $__sw = [Diagnostics.Stopwatch]::StartNew()
    do {
        if (& $__until) { return $true }
        Start-Sleep -Milliseconds $__step
    } while ($__sw.ElapsedMilliseconds -lt $__ms)
    return [bool](& $__until)
}

function Invoke-Check([string]$id, [string]$name, [scriptblock]$body) {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    try { $r = & $body } catch { $r = @{ Pass = $false; Detail = "threw: $($_.Exception.Message)" } }
    $ms = $sw.ElapsedMilliseconds
    $script:results.Add([pscustomobject]@{ Id = $id; Name = $name; Pass = [bool]$r.Pass; Detail = $r.Detail; Ms = $ms })
    Log ("{0} {1}: {2} ({3} ms) {4}" -f $id, $name, $(if ($r.Pass) { 'pass' } else { 'FAIL' }), $ms, $r.Detail)
}

# ---- the real install's fingerprint (read only) ----

function Fingerprint {
    $dir = Join-Path $env:LOCALAPPDATA 'Q Calc'
    $files = foreach ($f in 'qcalc.exe', 'uninstall.exe') {
        $p = Join-Path $dir $f
        if (Test-Path $p) { "$f=$((Get-FileHash $p -Algorithm SHA256).Hash) $((Get-Item $p).LastWriteTimeUtc.Ticks)" } else { "$f=none" }
    }
    $reg = foreach ($k in 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Q Calc', 'HKCU:\Software\maxconine\Q Calc') {
        $v = Get-ItemProperty $k -ErrorAction SilentlyContinue
        if ($v) { "$k=" + (($v.PSObject.Properties | Where-Object { $_.Name -notlike 'PS*' } | ForEach-Object { "$($_.Name):$($_.Value)" }) -join ';') } else { "$k=none" }
    }
    $run = "run=$((Get-ItemProperty $RunKey -ErrorAction SilentlyContinue).'Q Calc')"
    $lnk = Join-Path ([Environment]::GetFolderPath('Programs')) 'Q Calc.lnk'
    $lnkState = if (Test-Path $lnk) { "lnk=$((Get-Item $lnk).LastWriteTimeUtc.Ticks)" } else { 'lnk=none' }
    (@($files) + @($reg) + $run + $lnkState) -join "`n"
}
$before = Fingerprint

# ---- the app ----

function Send-Pipe([hashtable]$Request, [int]$TimeoutMs = 3000) {
    $pipe = New-Object System.IO.Pipes.NamedPipeClientStream('.', "qcalc-bench-$Instance", [System.IO.Pipes.PipeDirection]::InOut)
    try {
        $pipe.Connect($TimeoutMs)
        $writer = New-Object System.IO.StreamWriter($pipe)
        $writer.AutoFlush = $true
        $writer.WriteLine(($Request | ConvertTo-Json -Compress -Depth 6))
        $line = (New-Object System.IO.StreamReader($pipe)).ReadLine()
        if ($line) { return $line | ConvertFrom-Json }
        return $null
    } catch { return $null } finally { $pipe.Dispose() }
}
function Update-State { Send-Pipe @{ cmd = 'update' } }

function Write-Store([bool]$auto) {
    $path = Join-Path $Data 'settings.json'
    $store = if (Test-Path $path) { Get-Content $path -Raw | ConvertFrom-Json } else { $null }
    if (-not $store) {
        $store = [pscustomobject]@{ hotkey = $Chord; firstRunDone = $true; settings = [pscustomobject]@{ theme = 'dark'; shareUsage = $false; sigFigs = 7 } }
    }
    $store | Add-Member -NotePropertyName autoUpdate -NotePropertyValue $auto -Force
    [IO.File]::WriteAllText($path, ($store | ConvertTo-Json -Depth 8))
}

# the environment the installed copy runs in, and so the installer it starts and the copy that starts
function Set-AppEnv([int]$port) {
    $env:QCALC_INSTANCE = $Instance; $env:QCALC_DATA = $Data; $env:QCALC_LOG = $Log; $env:QCALC_BENCH = '1'
    $env:QCALC_UPDATE_FEED = "http://127.0.0.1:$port/latest-native.json"
    $env:QCALC_UPDATE_PUBKEY = $script:pub
    $env:QCALC_UPDATE_QUIET_SECS = "$QuietSecs"
    $env:TEMP = $Tmp; $env:TMP = $Tmp
}

function Installed-Copies { @(Get-Process qcalc -ErrorAction SilentlyContinue | Where-Object { try { $_.Path -eq (Join-Path $InstallDir 'qcalc.exe') } catch { $false } }) }

function Start-Installed {
    $p = Start-Process -FilePath (Join-Path $InstallDir 'qcalc.exe') -ArgumentList '--autostart' -PassThru
    $script:mine.Add($p.Id)
    if (-not (Wait-Until { $null -ne (Update-State) } 15000)) { throw "the installed copy's pipe never answered (pid $($p.Id))" }
    $p
}

function Stop-Installed {
    foreach ($p in Installed-Copies) {
        Send-Pipe @{ cmd = 'quit' } 2000 | Out-Null
        if (-not $p.WaitForExit(5000)) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
    }
}

function Log-Lines { if (Test-Path $Log) { @(Get-Content $Log) } else { @() } }
function Served([string]$pattern) { @(Get-Content $ServerLog -ErrorAction SilentlyContinue | Where-Object { $_ -match $pattern }) }

# asks for a check and waits for the loop to log what it found; the state settings would show then
function Check-Now([string]$expect, [int]$ms = 20000) {
    $n = (Log-Lines).Count
    $sw = [Diagnostics.Stopwatch]::StartNew()
    Send-Pipe @{ cmd = 'update check' } | Out-Null
    $hit = Wait-Until { @(Log-Lines | Select-Object -Skip $n | Where-Object { $_ -match $expect }).Count -gt 0 } $ms 50
    $line = @(Log-Lines | Select-Object -Skip $n | Where-Object { $_ -match 'updates:' }) -join ' | '
    $st = $null
    Wait-Until { $script:st = Update-State; $script:st -and $script:st.state -notin 'checking', 'downloading' } 5000 50 | Out-Null
    [pscustomobject]@{ Hit = $hit; Ms = $sw.ElapsedMilliseconds; Log = $line; State = $script:st }
}

function Write-Feed([string]$version, [string]$file, [string]$sig) {
    $m = [ordered]@{ version = $version; pub_date = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
        platforms = [ordered]@{ 'windows-x86_64' = [ordered]@{ signature = $sig; url = "http://127.0.0.1:$($script:port)/$file" } } }
    [IO.File]::WriteAllText((Join-Path $Www 'latest-native.json'), ($m | ConvertTo-Json -Depth 5))
}

function Start-Server([int]$port = 0) {
    Remove-Item $ServerLog -ErrorAction SilentlyContinue
    $script:server = Start-Process node -ArgumentList @("`"$here\update-server.mjs`"", "`"$Www`"", "$port") -PassThru -WindowStyle Hidden -RedirectStandardOutput $ServerLog -RedirectStandardError "$ServerLog.err"
    if (-not (Wait-Until { (Test-Path $ServerLog) -and (Get-Content $ServerLog -ErrorAction SilentlyContinue | Select-String '^port (\d+)') } 10000)) { throw 'the feed server never started' }
    [int]((Get-Content $ServerLog | Select-String '^port (\d+)').Matches[0].Groups[1].Value)
}
function Stop-Server { if ($script:server -and -not $script:server.HasExited) { Stop-Process -Id $script:server.Id -Force; $script:server.WaitForExit(3000) | Out-Null } }

$saved = @{}
foreach ($k in 'QCALC_INSTANCE', 'QCALC_DATA', 'QCALC_LOG', 'QCALC_BENCH', 'QCALC_UPDATE_FEED', 'QCALC_UPDATE_PUBKEY', 'QCALC_UPDATE_QUIET_SECS', 'TEMP', 'TMP', 'TAURI_SIGNING_PRIVATE_KEY', 'TAURI_SIGNING_PRIVATE_KEY_PASSWORD') {
    $saved[$k] = [Environment]::GetEnvironmentVariable($k)
}
Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY, Env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD, Env:QCALC_COMPAT_SIMULATE -ErrorAction SilentlyContinue

try {

# ---- 1. two installers ----

$oldSetup = Join-Path $Root "Q-Calc-$Old-x64-native-setup.exe"
$newName = "Q-Calc-$New-x64-native-setup.exe"
$newSetup = Join-Path $Www $newName
Invoke-Check 'U1' "build $Old and $New installers (build.ps1, update-e2e feature, app id '$AppId')" {
    if (-not ($SkipBuild -and (Test-Path $oldSetup) -and (Test-Path $newSetup))) {
        foreach ($v in $Old, $New) {
            $sw = [Diagnostics.Stopwatch]::StartNew()
            & "$installerDir\build.ps1" -Version $v -Features update-e2e -AppId $AppId | Out-Host
            Log "built $v in $([Math]::Round($sw.Elapsed.TotalSeconds)) s"
            $dest = if ($v -eq $Old) { $oldSetup } else { $newSetup }
            Move-Item -Force "$installerDir\out\Q-Calc-$v-x64-native-setup.exe" $dest
        }
    }
    @{ Pass = (Test-Path $oldSetup) -and (Test-Path $newSetup); Detail = "$([Math]::Round((Get-Item $newSetup).Length / 1MB, 2)) MB each" }
}

# ---- 2. keys, signatures, the feed server ----

if (-not $TauriCli) {
    $common = (git -C $repo rev-parse --path-format=absolute --git-common-dir 2>$null)
    foreach ($r in @($repo) + @($(if ($common) { Split-Path $common -Parent }))) {
        $c = Join-Path $r 'node_modules\@tauri-apps\cli\tauri.js'
        if (Test-Path $c) { $TauriCli = $c; break }
    }
}
if (-not $TauriCli) { throw 'no @tauri-apps/cli: run npm ci, or pass -TauriCli' }
$keys = Join-Path $Root 'keys'; New-Item -ItemType Directory -Force $keys | Out-Null
$pw = [Guid]::NewGuid().ToString('N')
function Sign([string]$key, [string]$file, [string]$version) {
    node $TauriCli signer sign -f $key -p $pw --app-version $version $file | Out-Null
    if ($LASTEXITCODE) { throw "signing $file failed" }
    (Get-Content "$file.sig" -Raw).Trim()
}
Invoke-Check 'U2' 'throwaway keys sign the new installer; latest-native.mjs writes the feed' {
    foreach ($k in 'trusted', 'other') {
        node $TauriCli signer generate --ci -p $pw -w (Join-Path $keys "$k.key") -f | Out-Null
        if ($LASTEXITCODE) { throw 'tauri signer generate failed' }
    }
    $script:pub = (Get-Content (Join-Path $keys 'trusted.key.pub') -Raw).Trim()
    $script:goodSig = Sign (Join-Path $keys 'trusted.key') $newSetup $New
    # the same file signed for the old version (an old signed installer passed off as new), and by another key
    $copy = Join-Path $keys $newName
    Copy-Item $newSetup $copy
    $script:wrongVersionSig = Sign (Join-Path $keys 'trusted.key') $copy $Old
    $script:otherKeySig = Sign (Join-Path $keys 'other.key') $copy $New
    # half the installer under its own name, the good signature on it
    $bytes = [IO.File]::ReadAllBytes($newSetup)
    [IO.File]::WriteAllBytes((Join-Path $Www 'truncated.exe'), $bytes[0..([int]($bytes.Length / 2))])
    $script:port = Start-Server
    node (Join-Path $installerDir 'latest-native.mjs') $New "http://127.0.0.1:$($script:port)" $Www "windows-x86_64=$newName" | Out-Host
    $script:goodFeed = Get-Content (Join-Path $Www 'latest-native.json') -Raw
    Remove-Item (Join-Path $Www 'latest-native.json')
    @{ Pass = [bool]$script:pub -and $script:goodFeed -match [regex]::Escape($newName); Detail = "server on 127.0.0.1:$($script:port)" }
}

# ---- 3. the old version, installed into a temp folder ----

Invoke-Check 'U3' "install $Old silently into a temp folder and start it in the tray" {
    $p = Start-Process -FilePath $oldSetup -ArgumentList @('/S', '/NORELAUNCH', "/D=$InstallDir") -PassThru
    if (-not $p.WaitForExit(120000)) { throw 'the installer never finished' }
    $reg = Get-ItemProperty $UninstallKey -ErrorAction SilentlyContinue
    $dir = (Get-ItemProperty $DirKey -ErrorAction SilentlyContinue).'(default)'
    Write-Store $false
    Set-AppEnv $script:port
    $script:proc = Start-Installed
    # a history row and a setting to find after the update
    Send-Pipe @{ inputs = @('6*7'); enter = $true } 20000 | Out-Null
    Send-Pipe @{ cmd = 'hide' } | Out-Null
    $e = Send-Pipe @{ cmd = 'e2e' }
    $u = Update-State
    @{ Pass = ($p.ExitCode -eq 0 -and $reg.DisplayVersion -eq $Old -and $dir -eq $InstallDir -and $u.version -eq $Old -and $e.rows -ge 1)
       Detail = "exit $($p.ExitCode); entry $($reg.DisplayVersion); folder key '$dir'; running $($u.version), state $($u.state): '$($u.line)' [$($u.button)]; history rows $($e.rows)" }
}

# ---- 4. what must be refused ----

Invoke-Check 'F1' 'no feed (HTTP 404, as today): quiet, up to date, again later' {
    $r = Check-Now 'no feed yet'
    @{ Pass = ($r.Hit -and $r.State.state -eq 'current' -and -not $r.State.error -and @(Served '^GET /latest-native.json 404').Count -ge 1)
       Detail = "$($r.Ms) ms; $($r.State.state): '$($r.State.line)' [$($r.State.button)]; log: $($r.Log)" }
}

Invoke-Check 'F2' 'an older version on the feed is left alone' {
    Write-Feed '2.0.97' $newName $script:goodSig
    $before_ = @(Served "GET /$newName").Count
    $r = Check-Now 'older than'
    @{ Pass = ($r.Hit -and $r.State.state -eq 'current' -and @(Served "GET /$newName").Count -eq $before_)
       Detail = "$($r.Ms) ms; $($r.State.state): '$($r.State.line)'; nothing downloaded; log: $($r.Log)" }
}

Invoke-Check 'F3' "a signature made for another version ($Old) is refused before downloading" {
    Write-Feed $New $newName $script:wrongVersionSig
    $before_ = @(Served "GET /$newName").Count
    $r = Check-Now 'check failed'
    @{ Pass = ($r.Hit -and $r.State.state -eq 'failed' -and $r.State.error -match "isn't signed for $New" -and @(Served "GET /$newName").Count -eq $before_)
       Detail = "$($r.Ms) ms; $($r.State.state): '$($r.State.line)'; nothing downloaded" }
}

Invoke-Check 'F4' 'a signature from another key is refused and the download thrown away' {
    Write-Feed $New $newName $script:otherKeySig
    $r = Check-Now 'check failed' 60000
    $left = Test-Path (Join-Path $Tmp 'qcalc-update')
    @{ Pass = ($r.Hit -and $r.State.state -eq 'failed' -and $r.State.error -match 'signature' -and -not $left)
       Detail = "$($r.Ms) ms; '$($r.State.line)'; download kept: $left" }
}

Invoke-Check 'F5' 'a truncated installer is refused' {
    Write-Feed $New 'truncated.exe' $script:goodSig
    $r = Check-Now 'check failed' 60000
    @{ Pass = ($r.Hit -and $r.State.state -eq 'failed' -and -not (Test-Path (Join-Path $Tmp 'qcalc-update')) -and @(Served 'GET /truncated.exe 200').Count -ge 1)
       Detail = "$($r.Ms) ms; '$($r.State.line)'" }
}

Invoke-Check 'F6' 'a download cut off halfway is refused' {
    Write-Feed $New "drop/$newName" $script:goodSig
    $r = Check-Now 'check failed' 60000
    @{ Pass = ($r.Hit -and $r.State.state -eq 'failed' -and -not (Test-Path (Join-Path $Tmp 'qcalc-update')))
       Detail = "$($r.Ms) ms; '$($r.State.line)'; log: $($r.Log)" }
}

Invoke-Check 'F7' 'no server at all (no network): a sentence, not a crash' {
    Stop-Server
    $r = Check-Now 'check failed' 60000
    $script:port = Start-Server $script:port
    $alive = $null -ne (Send-Pipe @{ cmd = 'state' })
    @{ Pass = ($r.Hit -and $r.State.state -eq 'failed' -and $r.State.error -match 'no connection' -and $alive)
       Detail = "$($r.Ms) ms; '$($r.State.line)'; app answering: $alive" }
}

# ---- 5. the real thing ----

Invoke-Check 'U4' "auto update: $Old checks, downloads, verifies, waits for a quiet moment, installs $New and restarts in the tray" {
    [IO.File]::WriteAllText((Join-Path $Www 'latest-native.json'), $script:goodFeed)
    Stop-Installed
    Write-Store $true
    $script:proc = Start-Installed
    $old = $script:proc
    $t0 = [Diagnostics.Stopwatch]::StartNew()
    $seen = [ordered]@{}
    $lines = [ordered]@{}
    $gone = $false
    while ($t0.Elapsed.TotalSeconds -lt 240) {
        if ($old.HasExited) { $gone = $true; break }
        $u = Update-State
        if ($u) {
            if (-not $seen.Contains($u.state)) { $seen[$u.state] = $t0.ElapsedMilliseconds; $lines[$u.state] = "'$($u.line)' [$($u.button)]" }
        } elseif ($old.HasExited) { $gone = $true; break }
        Start-Sleep -Milliseconds 50
    }
    $quitAt = $t0.ElapsedMilliseconds
    $script:mine.Remove($old.Id) | Out-Null
    $relaunched = Wait-Until { (Installed-Copies).Count -ge 1 -and $null -ne (Update-State) } 60000
    $upAt = $t0.ElapsedMilliseconds
    Installed-Copies | ForEach-Object { $script:mine.Add($_.Id) }
    $u = Update-State
    $s = Send-Pipe @{ cmd = 'state' }
    $e = Send-Pipe @{ cmd = 'e2e' }
    $reg = Get-ItemProperty $UninstallKey -ErrorAction SilentlyContinue
    $store = Get-Content (Join-Path $Data 'settings.json') -Raw | ConvertFrom-Json
    $fileVersion = (Get-Item (Join-Path $InstallDir 'qcalc.exe')).VersionInfo.ProductVersion
    $script:timeline = ($seen.Keys | ForEach-Object { "$_ at $($seen[$_]) ms $($lines[$_])" }) -join '; '
    @{ Pass = ($gone -and $relaunched -and $seen.Contains('ready') -and $u.version -eq $New -and $fileVersion -eq $New -and $reg.DisplayVersion -eq $New -and
               -not $s.visible -and $e.rows -ge 1 -and $store.settings.theme -eq 'dark' -and $store.settings.sigFigs -eq 7)
       Detail = "states: $($script:timeline); old copy quit at $quitAt ms; new copy answering at $upAt ms: $($u.version) (exe $fileVersion, entry $($reg.DisplayVersion)), in the tray: $(-not $s.visible); history rows $($e.rows); settings kept: theme $($store.settings.theme), sigFigs $($store.settings.sigFigs)" }
}

Invoke-Check 'U5' "$New finds itself up to date" {
    $r = Check-Now 'updates: (no feed|the feed)|"current"|' 10000
    Wait-Until { $script:u5 = Update-State; $script:u5.state -eq 'current' } 20000 | Out-Null
    $logged = @(Log-Lines | Where-Object { $_ -match 'updates: installer started|installing' }) -join ' | '
    @{ Pass = ($script:u5.state -eq 'current' -and $script:u5.version -eq $New)
       Detail = "$($script:u5.state): '$($script:u5.line)' [$($script:u5.button)]; earlier: $logged" }
}

} finally {
    # ---- clean up: only what this run made ----
    try { Stop-Installed } catch { }
    foreach ($id in $script:mine) { Stop-Process -Id $id -Force -ErrorAction SilentlyContinue }
    Stop-Server
    $un = Join-Path $InstallDir 'uninstall.exe'
    if (Test-Path $un) {
        $p = Start-Process -FilePath $un -ArgumentList '/S' -PassThru
        $p.WaitForExit(60000) | Out-Null
        Wait-Until { -not (Test-Path (Join-Path $InstallDir 'qcalc.exe')) } 30000 | Out-Null
    }
    $leftovers = @()
    if (Test-Path $UninstallKey) { $leftovers += 'uninstall entry'; Remove-Item $UninstallKey -Recurse -ErrorAction SilentlyContinue }
    if (Test-Path $DirKey) { $leftovers += 'folder key'; Remove-Item $DirKey -Recurse -ErrorAction SilentlyContinue }
    if ((Get-ItemProperty $RunKey -ErrorAction SilentlyContinue).$AppId) { $leftovers += 'Run value'; Remove-ItemProperty -Path $RunKey -Name $AppId -ErrorAction SilentlyContinue }
    if (Test-Path $Shortcut) { $leftovers += 'shortcut'; Remove-Item $Shortcut -ErrorAction SilentlyContinue }
    foreach ($k in $saved.Keys) { [Environment]::SetEnvironmentVariable($k, $saved[$k]) }
    Remove-Item Env:QCALC_INSTANCE, Env:QCALC_DATA, Env:QCALC_LOG, Env:QCALC_BENCH, Env:QCALC_UPDATE_FEED, Env:QCALC_UPDATE_PUBKEY, Env:QCALC_UPDATE_QUIET_SECS -ErrorAction SilentlyContinue
    foreach ($k in $saved.Keys) { if ($null -ne $saved[$k]) { [Environment]::SetEnvironmentVariable($k, $saved[$k]) } }
    if (Test-Path $Log) { Copy-Item $Log (Join-Path $OutDir 'qcalc.log') -ErrorAction SilentlyContinue }
    if (Test-Path $ServerLog) { Copy-Item $ServerLog (Join-Path $OutDir 'server.log') -ErrorAction SilentlyContinue }
    if (-not $KeepTemp) { Remove-Item $Root -Recurse -Force -ErrorAction SilentlyContinue } else { Remove-Item $keys -Recurse -Force -ErrorAction SilentlyContinue }
}

$after = Fingerprint
$script:results.Add([pscustomobject]@{ Id = 'S1'; Name = 'the real install, its entries, Run value and shortcut are untouched'; Pass = ($before -eq $after); Detail = $(if ($before -eq $after) { 'same before and after' } else { "changed:`n$before`n--`n$after" }); Ms = 0 })
$script:results.Add([pscustomobject]@{ Id = 'S2'; Name = 'the test install is gone'; Pass = ($leftovers.Count -eq 0 -and -not (Test-Path (Join-Path $InstallDir 'qcalc.exe'))); Detail = $(if ($leftovers) { "removed by hand: $($leftovers -join ', ')" } else { 'its uninstaller removed the files, entry, folder key, Run value and shortcut' }); Ms = 0 })

$failed = @($script:results | Where-Object { -not $_.Pass })
$md = New-Object System.Text.StringBuilder
[void]$md.AppendLine('## Q Calc native self-update, end to end')
[void]$md.AppendLine()
[void]$md.AppendLine($(if ($failed) { "**$($failed.Count) check(s) failed.**" } else { '**All checks passed.**' }))
[void]$md.AppendLine()
[void]$md.AppendLine('| # | check | result | ms | detail |')
[void]$md.AppendLine('|---|---|---|---|---|')
foreach ($r in $script:results) { [void]$md.AppendLine("| $($r.Id) | $($r.Name) | $(if ($r.Pass) { 'pass' } else { 'FAIL' }) | $($r.Ms) | $(($r.Detail -replace '\|', '\|' -replace "`r?`n", ' ')) |") }
[IO.File]::WriteAllText((Join-Path $OutDir 'results.md'), $md.ToString())
Write-Host $md.ToString()
Stop-Transcript | Out-Null
if ($failed) { exit 1 }
