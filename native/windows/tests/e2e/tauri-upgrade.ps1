<#
.SYNOPSIS
  A Tauri install (2.0.15) auto-updates into the native app, through the real Tauri updater: the shipped app's own
  update loop and tauri-plugin-updater 2.12.0 find the new version in a Tauri-format latest.json, download it,
  verify it with minisign and run the native installer exactly as they run a Tauri one. The native installer goes
  over the Tauri install silently, the native app comes back in the tray as the new version with the Tauri app's
  settings and history, owns the "Installed apps" entry, and its own update (latest-native.json) works after.
  Nothing real is touched and nothing is published.

.DESCRIPTION
  1. The Tauri app at -TauriTag (git archive into a temp folder, npm ci, `tauri build --bundles nsis`) under a test
     identity: identifier com.maxconine.qcalc.upgradetest and product name "Q Calc Upgrade Test" (so its own config
     folder, web view data, single-instance lock, temp names and sign-in value), the updater pointed at
     http://127.0.0.1:<Port>/latest.json with a throwaway key (dangerousInsecureTransportProtocol, test only). Three
     patches, all outside the plugin: its log goes to its own folder (crash.rs names com.maxconine.qcalc), and the
     loop's tick and quiet wait (30 s, 5 min hidden and idle) become 2 s and -QuietSecs; the first check stays at
     20 s. The exe is stamped as an NSIS build the way tauri-bundler stamps the copy it packs (the shipped exes
     carry the stamp), which is what makes the plugin look for `windows-x86_64-nsis` before `windows-x86_64`.
  2. The native installers -New and -Next (build.ps1 -Features update-e2e -AppId "Q Calc Upgrade Test"), signed with
     the throwaway key by `tauri signer sign --app-version`, as the release signs them. latest.json is written by the
     release's own src-tauri/release/latest-json.mjs, latest-native.json by installer/latest-native.mjs.
  3. A Tauri install, as the Tauri installer leaves one, under the test app id: qcalc.exe in a temp folder with a
     space in its name, HKCU\Software\maxconine\<app id> (its folder), the Uninstall entry with the Tauri
     installer's values (DisplayVersion 2.0.15), the Run value as tauri-plugin-autostart writes it (unquoted) and a
     Start menu shortcut. (The Tauri NSIS installer itself is not run: when silent it kills every qcalc.exe of this
     user by name, which would take the real Q Calc down with it.) Its data: tests/fixtures/tauri's settings.json in
     its config folder and the fixture LevelDB as its web view's localStorage. That LevelDB keeps the history under
     the origin http://tauri.localhost:58704 (make.ps1's Edge), which the real page never had, so the 2.0.15 page
     starts empty under http://tauri.localhost; it is handed the fixture's four rows over DevTools
     (WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port, cdp-eval.mjs) and reloaded, so the history
     the native app must find is one the 2.0.15 page itself wrote.
  4. The run: the Tauri app starts at "sign-in" (--autostart), checks, downloads, verifies, writes its "updated"
     marker and starts the installer (the command line is read back from the process), and exits; the native
     installer replaces qcalc.exe in the folder the registry names and starts it in the tray. Checks: the version,
     the entry, the folder, the Run value kept, the settings and the migrated history; then latest-native.json
     offers -Next and the native app installs it by itself.
  5. Key priority: the native app is uninstalled, the Tauri install laid down again, and latest.json carries both
     `windows-x86_64-nsis` (pointing at a copy of the native installer named nsis-key-wins.exe, never a Tauri
     installer) and `windows-x86_64`. Which file the updater fetches shows which key wins.
  The QCALC_* variables the native app's test builds read (QCALC_DATA = the Tauri copy's config folder, so both
  apps share it as they do for real; QCALC_MIGRATE = its web view's LevelDB) pass from the Tauri app through the
  installer to the native app. The real install (%LOCALAPPDATA%\Q Calc, its files' hashes, its HKCU entries, Run
  and StartupApproved values, shortcuts, and the data folders' files) is fingerprinted before and after; whatever
  this run made is removed at the end, the test install by its own uninstaller.

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File native\windows\tests\e2e\tauri-upgrade.ps1
  # again, reusing the builds (the Tauri build has the key and port baked in):
  powershell -NoProfile -ExecutionPolicy Bypass -File native\windows\tests\e2e\tauri-upgrade.ps1 -Work C:\t\tu -KeepTemp
  powershell -NoProfile -ExecutionPolicy Bypass -File native\windows\tests\e2e\tauri-upgrade.ps1 -Work C:\t\tu -SkipBuild
#>
param(
    [string]$OutDir = (Join-Path $PSScriptRoot 'e2e-out\tauri-upgrade'),
    [string]$Work,
    [switch]$KeepTemp,
    [switch]$SkipBuild,
    [string]$TauriTag = 'v2.0.15',
    [string]$New = '3.0.0',
    [string]$Next = '3.0.1',
    [int]$Port = 47615,
    [int]$QuietSecs = 3
)
$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
$native = (Resolve-Path "$here\..\..").Path
$repo = (Resolve-Path "$native\..\..").Path
$installerDir = Join-Path $native 'installer'
$fixtures = Join-Path $native 'tests\fixtures\tauri'
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$OutDir = (Resolve-Path $OutDir).Path
Start-Transcript -Path (Join-Path $OutDir 'log.txt') -Force | Out-Null

$Old = $TauriTag.TrimStart('v')
$AppId = 'Q Calc Upgrade Test'
$Identifier = 'com.maxconine.qcalc.upgradetest'
$Instance = 'tauri-upgrade'
$UninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$AppId"
$DirKey = "HKCU:\Software\maxconine\$AppId"
$RunKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$ApprovedKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run'
$Programs = [Environment]::GetFolderPath('Programs')
$Shortcut = Join-Path $Programs "$AppId.lnk"
# the Tauri copy's own folders (tauri's app_config_dir and app_local_data_dir, the web view's data under the latter)
$TauriConfig = Join-Path ([Environment]::GetFolderPath('ApplicationData')) $Identifier
$TauriLocal = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) $Identifier
$LevelDb = Join-Path $TauriLocal 'EBWebView\Default\Local Storage\leveldb'
foreach ($p in $TauriConfig, $TauriLocal) { if ($p -notmatch 'upgradetest$') { throw "refusing: $p" } }

$Root = if ($Work) { $Work } else { Join-Path ([IO.Path]::GetTempPath()) ('qcalc-tauri-upgrade-' + [Guid]::NewGuid().ToString('N').Substring(0, 8)) }
New-Item -ItemType Directory -Force $Root | Out-Null
$Root = (Resolve-Path $Root).Path
$Src = Join-Path $Root 'tauri-src'; $Www = Join-Path $Root 'www'; $Tmp = Join-Path $Root 'tmp'; $Keys = Join-Path $Root 'keys'
$InstallDir = Join-Path $Root $AppId
$Log = Join-Path $Root 'qcalc.log'; $ServerLog = Join-Path $Root 'server.log'
$TauriExe = Join-Path $Root 'tauri-qcalc.exe'
New-Item -ItemType Directory -Force $Www, $Tmp, $Keys | Out-Null
Remove-Item $Log -ErrorAction SilentlyContinue

$script:results = New-Object System.Collections.Generic.List[object]
$script:server = $null
$script:mine = New-Object System.Collections.Generic.List[int]

function Log([string]$msg) { Write-Host ("[{0:HH:mm:ss.fff}] {1}" -f (Get-Date), $msg) }
function Wait-Until([scriptblock]$__until, [int]$__ms = 5000, [int]$__step = 100) {
    $__sw = [Diagnostics.Stopwatch]::StartNew()
    do { if (& $__until) { return $true }; Start-Sleep -Milliseconds $__step } while ($__sw.ElapsedMilliseconds -lt $__ms)
    [bool](& $__until)
}
function Invoke-Check([string]$id, [string]$name, [scriptblock]$body) {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    try { $r = & $body } catch { $r = @{ Pass = $false; Detail = "threw: $($_.Exception.Message) at $($_.InvocationInfo.ScriptLineNumber)" } }
    $ms = $sw.ElapsedMilliseconds
    $script:results.Add([pscustomobject]@{ Id = $id; Name = $name; Pass = [bool]$r.Pass; Detail = $r.Detail; Ms = $ms })
    Log ("{0} {1}: {2} ({3} ms) {4}" -f $id, $name, $(if ($r.Pass) { 'pass' } else { 'FAIL' }), $ms, $r.Detail)
}

# ---- the real install's fingerprint (read only) ----

function Fingerprint {
    $out = New-Object System.Collections.Generic.List[string]
    $real = Join-Path $env:LOCALAPPDATA 'Q Calc'
    if (Test-Path $real) {
        Get-ChildItem -LiteralPath $real -Recurse -Force -File | Sort-Object FullName | ForEach-Object { $out.Add("file $($_.FullName) $($_.Length) $((Get-FileHash -LiteralPath $_.FullName).Hash) $($_.LastWriteTimeUtc.Ticks)") }
    } else { $out.Add("no $real") }
    foreach ($k in 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Q Calc', 'HKCU:\Software\maxconine\Q Calc') {
        $v = Get-ItemProperty $k -ErrorAction SilentlyContinue
        $out.Add("$k=" + $(if ($v) { ($v.PSObject.Properties | Where-Object { $_.Name -notlike 'PS*' } | Sort-Object Name | ForEach-Object { "$($_.Name):$($_.Value)" }) -join ';' } else { 'none' }))
    }
    $out.Add("run=$((Get-ItemProperty $RunKey -ErrorAction SilentlyContinue).'Q Calc')")
    $ap = (Get-ItemProperty $ApprovedKey -ErrorAction SilentlyContinue).'Q Calc'
    $out.Add("approved=$(if ($ap) { [BitConverter]::ToString($ap) } else { 'none' })")
    foreach ($l in (Join-Path $Programs 'Q Calc.lnk'), (Join-Path ([Environment]::GetFolderPath('Desktop')) 'Q Calc.lnk')) {
        $out.Add("lnk $l " + $(if (Test-Path $l) { (Get-FileHash -LiteralPath $l).Hash } else { 'none' }))
    }
    # the data folders: every file by name (the web view's churn included), and the app's own files by hash. The
    # real Q Calc may be running and writes these itself (its log, rates.json), so they are compared apart
    foreach ($d in (Join-Path $env:APPDATA 'com.maxconine.qcalc'), (Join-Path $env:LOCALAPPDATA 'com.maxconine.qcalc')) {
        if (Test-Path $d) {
            Get-ChildItem -LiteralPath $d -Force -File | Sort-Object Name | ForEach-Object { $out.Add("data $($_.FullName) $($_.Length) $((Get-FileHash -LiteralPath $_.FullName).Hash) $($_.LastWriteTimeUtc.Ticks)") }
        } else { $out.Add("no $d") }
    }
    $out
}
function Real-Log { $p = Join-Path $env:LOCALAPPDATA 'com.maxconine.qcalc\qcalc.log'; if (Test-Path $p) { [IO.File]::ReadAllText($p) } else { '' } }
$before = @(Fingerprint)
$realLogBefore = (Real-Log).Length
$realRunning = @(Get-Process qcalc -ErrorAction SilentlyContinue | Where-Object { try { $_.Path -eq (Join-Path $env:LOCALAPPDATA 'Q Calc\qcalc.exe') } catch { $false } } | ForEach-Object Id)
Log "real install fingerprinted ($($before.Count) lines); real Q Calc running: $(if ($realRunning) { "pid $($realRunning -join ',')" } else { 'no' })"

# ---- helpers ----

function Send-Pipe([hashtable]$Request, [int]$TimeoutMs = 3000) {
    $pipe = New-Object System.IO.Pipes.NamedPipeClientStream('.', "qcalc-bench-$Instance", [System.IO.Pipes.PipeDirection]::InOut)
    try {
        $pipe.Connect($TimeoutMs)
        $w = New-Object System.IO.StreamWriter($pipe); $w.AutoFlush = $true
        $w.WriteLine(($Request | ConvertTo-Json -Compress -Depth 6))
        $line = (New-Object System.IO.StreamReader($pipe)).ReadLine()
        if ($line) { return $line | ConvertFrom-Json }
        $null
    } catch { $null } finally { $pipe.Dispose() }
}
function Copies { @(Get-Process qcalc -ErrorAction SilentlyContinue | Where-Object { try { $_.Path -eq (Join-Path $InstallDir 'qcalc.exe') } catch { $false } }) }
function Ours-Under-Root { @(Get-Process -ErrorAction SilentlyContinue | Where-Object { try { $_.Path -and $_.Path.StartsWith($Root, [StringComparison]::OrdinalIgnoreCase) } catch { $false } }) }
function Served([string]$pattern) { @(Get-Content $ServerLog -ErrorAction SilentlyContinue | Where-Object { $_ -match $pattern }) }
function LogText {
    foreach ($i in 1..10) {
        if (-not (Test-Path $Log)) { return '' }
        try {
            $fs = New-Object System.IO.FileStream($Log, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]'ReadWrite, Delete')
            try { return (New-Object System.IO.StreamReader($fs)).ReadToEnd() } finally { $fs.Dispose() }
        } catch { Start-Sleep -Milliseconds 50 }
    }
    ''
}
function Start-Server {
    Remove-Item $ServerLog, "$ServerLog.err" -ErrorAction SilentlyContinue
    $script:server = Start-Process node -ArgumentList @("`"$here\update-server.mjs`"", "`"$Www`"", "$Port") -PassThru -WindowStyle Hidden -RedirectStandardOutput $ServerLog -RedirectStandardError "$ServerLog.err"
    if (-not (Wait-Until { (Test-Path $ServerLog) -and (Get-Content $ServerLog -ErrorAction SilentlyContinue | Select-String "^port $Port") } 10000)) { throw "the feed server didn't start on $Port (in use?): $(Get-Content "$ServerLog.err" -ErrorAction SilentlyContinue)" }
}
function Stop-Server { if ($script:server -and -not $script:server.HasExited) { Stop-Process -Id $script:server.Id -Force; $script:server.WaitForExit(3000) | Out-Null } }
function Tauri-Cli { Join-Path $Src 'node_modules\@tauri-apps\cli\tauri.js' }
function Sign([string]$file, [string]$version) {
    $pw = Get-Content (Join-Path $Keys 'pw.txt') -Raw
    node (Tauri-Cli) signer sign -f (Join-Path $Keys 'trusted.key') -p $pw --app-version $version $file | Out-Null
    if ($LASTEXITCODE) { throw "signing $file failed" }
}
# the Tauri page over DevTools (cdp-eval.mjs). The expression goes through PowerShell 5's argument passing, which
# mangles double quotes, so it uses single quotes only
$CdpPort = $Port + 1
function Cdp([string]$expr) {
    $ErrorActionPreference = 'Continue'
    $out = node "$here\cdp-eval.mjs" $CdpPort 'tauri.localhost' $expr 2>$null
    if ($LASTEXITCODE -eq 0 -and $out) { $out | ConvertFrom-Json }
}
# tests/fixtures/tauri's four history rows (its LevelDB keeps them under http://tauri.localhost:58704, an origin
# the real page never had, so the page itself is given them), as the 2.0.x page stores them
$HistoryJs = "[{id:'mg8k1a-x1',quantity:'26.455471462185 lb',display:'26.4555 lb',expr:'12 kg to lb',n:26.455471462185,at:1792999100000}," +
    "{id:'mg8k2b-x2',quantity:'5 m',display:'5 m',expr:'r = 5 m',n:5,at:1792999400000}," +
    "{id:'mg8k3c-x3',fnBody:'x^2 + 1',fnParams:['x'],kind:'function',display:'f(x) = x^2 + 1',at:1792999700000,expr:'f(x) = x^2 + 1',fnName:'f'}," +
    "{id:'mg8k4d-x4',display:'6.28319',expr:'2\u00d7\u03c0',n:6.283185307179586,exact:'2\u03c0',at:1792999940000}]"
# the Tauri copy's web view processes (their user data folder names the test identifier) go once it has
function Our-WebViews { @(Get-CimInstance Win32_Process -Filter "Name = 'msedgewebview2.exe'" -ErrorAction SilentlyContinue | Where-Object { $_.CommandLine -like "*$Identifier*" }) }
function Wait-WebViews {
    if (-not (Wait-Until { (Our-WebViews).Count -eq 0 } 20000 250)) { Our-WebViews | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue } }
}
function Lnk-Target([string]$p) { if (Test-Path $p) { (New-Object -ComObject WScript.Shell).CreateShortcut($p).TargetPath } }
function Reg([string]$k) { Get-ItemProperty $k -ErrorAction SilentlyContinue }

# a Tauri install as the Tauri NSIS installer (currentUser) and tauri-plugin-autostart leave one
function Lay-Tauri-Install {
    New-Item -ItemType Directory -Force $InstallDir | Out-Null
    Copy-Item $TauriExe (Join-Path $InstallDir 'qcalc.exe') -Force
    New-Item -Path $DirKey -Force | Out-Null
    Set-ItemProperty -Path $DirKey -Name '(default)' -Value $InstallDir
    Set-ItemProperty -Path $DirKey -Name 'Installer Language' -Value '1033'
    New-Item -Path $UninstallKey -Force | Out-Null
    $exe = Join-Path $InstallDir 'qcalc.exe'
    $vals = [ordered]@{ DisplayName = $AppId; DisplayIcon = "`"$exe`""; DisplayVersion = $Old; Publisher = 'maxconine'
        InstallLocation = "`"$InstallDir`""; UninstallString = "`"$(Join-Path $InstallDir 'uninstall.exe')`""; MainBinaryName = 'qcalc.exe'
        HelpLink = 'https://github.com/maxconine/Q-Calc'; URLInfoAbout = 'https://github.com/maxconine/Q-Calc'; URLUpdateInfo = 'https://github.com/maxconine/Q-Calc' }
    foreach ($k in $vals.Keys) { Set-ItemProperty -Path $UninstallKey -Name $k -Value $vals[$k] }
    foreach ($k in 'NoModify', 'NoRepair') { New-ItemProperty -Path $UninstallKey -Name $k -PropertyType DWord -Value 1 -Force | Out-Null }
    New-ItemProperty -Path $UninstallKey -Name EstimatedSize -PropertyType DWord -Value 19629 -Force | Out-Null
    Set-ItemProperty -Path $RunKey -Name $AppId -Value "$exe --autostart"
    $s = (New-Object -ComObject WScript.Shell).CreateShortcut($Shortcut); $s.TargetPath = $exe; $s.WorkingDirectory = $InstallDir; $s.Save()
}

# the Tauri copy's data: the fixture settings (auto update on) and the fixture LevelDB as its web view's storage
function Lay-Tauri-Data {
    foreach ($p in $TauriConfig, $TauriLocal) { if (Test-Path $p) { Remove-Item $p -Recurse -Force } }
    New-Item -ItemType Directory -Force $TauriConfig, $LevelDb | Out-Null
    $s = Get-Content (Join-Path $fixtures 'settings.json') -Raw | ConvertFrom-Json
    $s.autoUpdate = $true
    [IO.File]::WriteAllText((Join-Path $TauriConfig 'settings.json'), ($s | ConvertTo-Json -Depth 8))
    Copy-Item (Join-Path $fixtures 'leveldb\*') $LevelDb
}

# the environment the Tauri copy runs in, and so the installer it starts and the native app that starts
$envNames = 'QCALC_INSTANCE', 'QCALC_DATA', 'QCALC_LOG', 'QCALC_BENCH', 'QCALC_MIGRATE', 'QCALC_NO_OPEN', 'QCALC_UPDATE_FEED', 'QCALC_UPDATE_PUBKEY', 'QCALC_UPDATE_QUIET_SECS', 'TEMP', 'TMP', 'CARGO_TARGET_DIR', 'WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS', 'TAURI_SIGNING_PRIVATE_KEY', 'TAURI_SIGNING_PRIVATE_KEY_PASSWORD'
$saved = @{}; foreach ($k in $envNames) { $saved[$k] = [Environment]::GetEnvironmentVariable($k) }
function Set-AppEnv {
    $env:QCALC_INSTANCE = $Instance; $env:QCALC_DATA = $TauriConfig; $env:QCALC_LOG = $Log; $env:QCALC_BENCH = '1'
    $env:QCALC_MIGRATE = $LevelDb; $env:QCALC_NO_OPEN = '1'
    $env:QCALC_UPDATE_FEED = "http://127.0.0.1:$Port/latest-native.json"
    $env:QCALC_UPDATE_PUBKEY = (Get-Content (Join-Path $Keys 'trusted.key.pub') -Raw).Trim()
    $env:QCALC_UPDATE_QUIET_SECS = "$QuietSecs"
    $env:TEMP = $Tmp; $env:TMP = $Tmp
}
function Restore-Env { foreach ($k in $envNames) { [Environment]::SetEnvironmentVariable($k, $saved[$k]) } }
Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY, Env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD, Env:QCALC_COMPAT_SIMULATE, Env:QCALC_TEST_CRASH -ErrorAction SilentlyContinue

# watches one Tauri -> native upgrade: the feed requests, the installer's command line, the Tauri copy leaving and
# the native copy answering; times in ms from the Tauri copy's start
function Watch-Upgrade($tauri, [string]$wantFile, [int]$ms = 240000) {
    $t0 = [Diagnostics.Stopwatch]::StartNew()
    $ev = [ordered]@{}
    $cmd = $null; $native = $null
    $n0 = @(Get-Content $ServerLog -ErrorAction SilentlyContinue).Count
    function Since([string]$pat) { @(Get-Content $ServerLog -ErrorAction SilentlyContinue | Select-Object -Skip $n0 | Where-Object { $_ -match $pat }) }
    while ($t0.ElapsedMilliseconds -lt $ms) {
        $now = $t0.ElapsedMilliseconds
        if (-not $ev.Contains('feed') -and (Since '^GET /latest.json 200').Count) { $ev['feed'] = $now }
        if (-not $ev.Contains('download') -and (Since "^GET /[^ ]+\.exe 200").Count) { $ev['download'] = $now }
        if (-not $cmd) {
            $p = Get-CimInstance Win32_Process -Filter "Name LIKE '$AppId-%'" -ErrorAction SilentlyContinue | Select-Object -First 1
            if ($p) { $cmd = $p.CommandLine; $ev['installer'] = $now; $script:installerExe = $p.ExecutablePath }
        }
        if (-not $ev.Contains('tauri exit') -and $tauri.HasExited) { $ev['tauri exit'] = $now }
        if (-not $native) {
            $c = Copies | Where-Object { $_.Id -ne $tauri.Id } | Select-Object -First 1
            if ($c) { $native = $c; $ev['native started'] = $now; $script:mine.Add($c.Id) }
        }
        if ($native -and -not $ev.Contains('native answering') -and $null -ne (Send-Pipe @{ cmd = 'update' } 300)) { $ev['native answering'] = $now; break }
        Start-Sleep -Milliseconds 100
    }
    $nativeCmd = if ($native) { (Get-CimInstance Win32_Process -Filter "ProcessId = $($native.Id)" -ErrorAction SilentlyContinue).CommandLine }
    [pscustomobject]@{ Events = $ev; Command = $cmd; Native = $native; NativeCommand = $nativeCmd; ExitCode = $(if ($tauri.HasExited) { $tauri.ExitCode } else { 'running' })
        Timeline = ($ev.Keys | ForEach-Object { "$_ $($ev[$_])" }) -join ', ' }
}

try {

# ---- 1. the Tauri app at $TauriTag, under a test identity ----

$stamp = Join-Path $Root 'build.json'
# bumped when the patches below change, so -SkipBuild rebuilds
$PatchSet = 2
Invoke-Check 'B1' "build the Tauri app $TauriTag as '$AppId' ($Identifier), updater on 127.0.0.1:$Port with a throwaway key" {
    $built = if (Test-Path $stamp) { Get-Content $stamp -Raw | ConvertFrom-Json }
    if ($SkipBuild -and $built -and $built.port -eq $Port -and $built.tag -eq $TauriTag -and $built.patches -eq $PatchSet -and (Test-Path $TauriExe) -and (Test-Path (Join-Path $Keys 'trusted.key'))) {
        return @{ Pass = $true; Detail = "reused ($($built.seconds) s when built); bundle type $($built.bundle)" }
    }
    $sw = [Diagnostics.Stopwatch]::StartNew()
    if (Test-Path $Src) { Remove-Item $Src -Recurse -Force }
    New-Item -ItemType Directory -Force $Src | Out-Null
    git -C $repo archive --format=tar -o (Join-Path $Root 'src.tar') $TauriTag
    if ($LASTEXITCODE) { throw "git archive $TauriTag failed" }
    tar -xf (Join-Path $Root 'src.tar') -C $Src
    Remove-Item (Join-Path $Root 'src.tar')
    # npm and the tauri cli report on stderr, which PowerShell 5 would turn into errors
    $ErrorActionPreference = 'Continue'
    Push-Location $Src
    try { cmd /c 'npm ci --no-audit --no-fund 2>&1' | Out-Null; if ($LASTEXITCODE) { throw 'npm ci failed' } } finally { Pop-Location }
    # the throwaway updater key
    Get-ChildItem $Keys -File | Remove-Item -Force
    $pw = [Guid]::NewGuid().ToString('N'); [IO.File]::WriteAllText((Join-Path $Keys 'pw.txt'), $pw)
    node (Tauri-Cli) signer generate --ci -p $pw -w (Join-Path $Keys 'trusted.key') -f | Out-Null
    if ($LASTEXITCODE) { throw 'tauri signer generate failed' }
    $pub = (Get-Content (Join-Path $Keys 'trusted.key.pub') -Raw).Trim()
    # the patches: outside the plugin, so the updater's own path is untouched
    $patch = {
        param($file, $from, $to)
        $p = Join-Path $Src $file; $t = [IO.File]::ReadAllText($p)
        if (-not $t.Contains($from)) { throw "patch: '$from' not in $file" }
        [IO.File]::WriteAllText($p, $t.Replace($from, $to))
    }
    & $patch 'src-tauri\src\crash.rs' '.join("com.maxconine.qcalc")' ".join(`"$Identifier`")"
    & $patch 'src-tauri\src\update.rs' 'const TICK: Duration = Duration::from_secs(30);' 'const TICK: Duration = Duration::from_secs(2);'
    & $patch 'src-tauri\src\update.rs' 'const IDLE: Duration = Duration::from_secs(5 * 60);' "const IDLE: Duration = Duration::from_secs($QuietSecs);"
    $overlay = [ordered]@{ identifier = $Identifier; productName = $AppId
        plugins = [ordered]@{ updater = [ordered]@{ pubkey = $pub; endpoints = @("http://127.0.0.1:$Port/latest.json"); dangerousInsecureTransportProtocol = $true } } }
    $overlayPath = Join-Path $Root 'test.conf.json'
    [IO.File]::WriteAllText($overlayPath, ($overlay | ConvertTo-Json -Depth 6))
    $env:CARGO_TARGET_DIR = Join-Path $Root 't'
    Push-Location $Src
    try { cmd /c "node `"$(Tauri-Cli)`" build --bundles nsis --config `"$overlayPath`" 2>&1" | Out-Host; if ($LASTEXITCODE) { throw 'tauri build failed' } } finally { Pop-Location; Remove-Item Env:CARGO_TARGET_DIR }
    $ErrorActionPreference = 'Stop'
    # the exe the NSIS installer carries is stamped with its bundle type (tauri-utils' static __TAURI_BUNDLE_TYPE,
    # "__TAURI_BUNDLE_TYPE_VAR_UNK" until tauri-bundler overwrites it with ..._NSS), which is what makes the plugin
    # try windows-x86_64-nsis first; the copy left in target\release is not stamped, so it's stamped here the same
    # way (the shipped exes are: the installed 2.0.x qcalc.exe carries ..._NSS and no ..._UNK)
    $latin = [Text.Encoding]::GetEncoding(28591)
    $text = $latin.GetString([IO.File]::ReadAllBytes((Join-Path $Root 't\release\qcalc.exe')))
    if (([regex]::Matches($text, '__TAURI_BUNDLE_TYPE_VAR_UNK')).Count -ne 1) { throw 'no single bundle type marker in the exe' }
    [IO.File]::WriteAllBytes($TauriExe, $latin.GetBytes($text.Replace('__TAURI_BUNDLE_TYPE_VAR_UNK', '__TAURI_BUNDLE_TYPE_VAR_NSS')))
    $bundle = ([regex]::Matches($latin.GetString([IO.File]::ReadAllBytes($TauriExe)), '__TAURI_BUNDLE_TYPE_VAR_[A-Z]{3}') | ForEach-Object Value) -join ','
    $secs = [Math]::Round($sw.Elapsed.TotalSeconds)
    [IO.File]::WriteAllText($stamp, (@{ tag = $TauriTag; port = $Port; seconds = $secs; bundle = $bundle; patches = $PatchSet } | ConvertTo-Json))
    @{ Pass = (Test-Path $TauriExe) -and $bundle -notmatch 'UNK'; Detail = "$secs s; exe $([Math]::Round((Get-Item $TauriExe).Length / 1MB, 1)) MB, bundle type markers $bundle" }
}

# ---- 2. the native installers ----

$newName = "Q-Calc-$New-x64-native-setup.exe"; $nextName = "Q-Calc-$Next-x64-native-setup.exe"
Invoke-Check 'B2' "build and sign the native installers $New and $Next (update-e2e feature, app id '$AppId')" {
    $detail = @()
    foreach ($v in $New, $Next) {
        $dest = Join-Path $Www "Q-Calc-$v-x64-native-setup.exe"
        if (-not ($SkipBuild -and (Test-Path $dest))) {
            $sw = [Diagnostics.Stopwatch]::StartNew()
            & "$installerDir\build.ps1" -Version $v -Features update-e2e -AppId $AppId | Out-Host
            Move-Item -Force "$installerDir\out\Q-Calc-$v-x64-native-setup.exe" $dest
            $detail += "$v built in $([Math]::Round($sw.Elapsed.TotalSeconds)) s"
        }
        Sign $dest $v
    }
    $sig = (Get-Content (Join-Path $Www "$newName.sig") -Raw).Trim()
    $comment = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($sig)) -split "`n" | Where-Object { $_ -like 'trusted comment:*' }
    @{ Pass = (Test-Path (Join-Path $Www $newName)) -and (Test-Path (Join-Path $Www $nextName)) -and $comment -match "version:$([regex]::Escape($New))"
       Detail = "$($detail -join '; '); $newName $([Math]::Round((Get-Item (Join-Path $Www $newName)).Length / 1MB, 2)) MB, $comment" }
}

# ---- 3. the feed, the Tauri install and its data ----

Invoke-Check 'T1' "latest.json (the release's latest-json.mjs) offers $New's native installer as windows-x86_64; the Tauri install is laid down" {
    Remove-Item (Join-Path $Www 'latest.json'), (Join-Path $Www 'latest-native.json') -ErrorAction SilentlyContinue
    node (Join-Path $repo 'src-tauri\release\latest-json.mjs') $New "http://127.0.0.1:$Port" $Www "windows-x86_64=$newName"
    if ($LASTEXITCODE) { throw 'latest-json.mjs failed' }
    $script:latest = Get-Content (Join-Path $Www 'latest.json') -Raw
    Start-Server
    Lay-Tauri-Install
    Lay-Tauri-Data
    $m = $script:latest | ConvertFrom-Json
    $keysIn = ($m.platforms.PSObject.Properties.Name) -join ','
    @{ Pass = ($m.version -eq $New -and $keysIn -eq 'windows-x86_64' -and (Reg $UninstallKey).DisplayVersion -eq $Old -and (Test-Path (Join-Path $LevelDb 'CURRENT')))
       Detail = "latest.json $($m.version), platforms: $keysIn; install $InstallDir (entry $((Reg $UninstallKey).DisplayVersion), Run '$((Reg $RunKey).$AppId)'); data $TauriConfig" }
}

# ---- 4. the real Tauri updater installs the native app ----

$script:up = $null
Invoke-Check 'H1' "the $Old page holds a history of its own: four rows put in its localStorage over DevTools, then the page reloaded" {
    Set-AppEnv
    $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$CdpPort"
    $script:tauri = Start-Process -FilePath (Join-Path $InstallDir 'qcalc.exe') -ArgumentList '--autostart' -PassThru
    Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS
    $script:mine.Add($script:tauri.Id)
    $t0 = [Diagnostics.Stopwatch]::StartNew()
    if (-not (Wait-Until { $script:o = Cdp 'location.href'; $script:o -and $script:o.value -like 'http://tauri.localhost/*' } 15000 250)) { throw 'the page never showed up on DevTools' }
    $set = Cdp "localStorage.setItem('qcalc-history', JSON.stringify($HistoryJs)); setTimeout(() => location.reload(), 50); 'set'"
    # the page, reloaded, has loaded the rows and written them back the way it keeps them
    Start-Sleep -Milliseconds 1500
    Wait-Until { $script:h = Cdp "document.readyState === 'complete' && JSON.parse(localStorage.getItem('qcalc-history') || '[]').map((r) => r.expr)"; $script:h -and @($script:h.value).Count -eq 4 } 10000 250 | Out-Null
    $script:seededAt = $t0.ElapsedMilliseconds
    @{ Pass = ($set.value -eq 'set' -and @($script:h.value).Count -eq 4 -and -not $script:tauri.HasExited)
       Detail = "page $($script:o.value); history after the reload: $(@($script:h.value) -join ' | ') at $($script:seededAt) ms (the update's first check is at 20 s)" }
}

Invoke-Check 'U1' "the Tauri $Old app, started at sign-in, finds $New, downloads, verifies and runs the native installer" {
    $tauri = $script:tauri
    $script:up = Watch-Upgrade $tauri $newName
    $tauriLog = Join-Path $TauriLocal 'qcalc.log'
    if (Test-Path $tauriLog) { Copy-Item $tauriLog (Join-Path $OutDir 'tauri-app.log') -Force }
    $served = (Get-Content $ServerLog | Where-Object { $_ -notmatch '^port' }) -join '; '
    @{ Pass = ($script:up.Events.Contains('feed') -and (Served "^GET /$([regex]::Escape($newName)) 200").Count -eq 1 -and $script:up.Command -and $script:up.ExitCode -eq 0)
       Detail = "timeline (ms): $($script:up.Timeline); served: $served; installer command line: $($script:up.Command); Tauri exit code $($script:up.ExitCode)" }
}

Invoke-Check 'U2' 'the updater passes /S /UPDATE /R /ARGS <its own args> (quiet mode), from a temp copy named after the app and version' {
    $c = "$($script:up.Command)"
    $args_ = ($c -replace '^"[^"]*"\s*', '')
    @{ Pass = ($args_ -eq '/S /UPDATE /R /ARGS --autostart' -and $script:installerExe -like "$Tmp\$AppId-$New-updater-*\$AppId-$New-installer*.exe")
       Detail = "args '$args_'; exe $($script:installerExe)" }
}

Invoke-Check 'U3' "the native installer replaces qcalc.exe in the folder the Tauri installer recorded and starts $New in the tray" {
    $native = $script:up.Native
    Wait-Until { -not (Get-CimInstance Win32_Process -Filter "Name LIKE '$AppId-%'" -ErrorAction SilentlyContinue) } 30000 | Out-Null
    $u = Send-Pipe @{ cmd = 'update' }
    $s = Send-Pipe @{ cmd = 'state' }
    $file = (Get-Item (Join-Path $InstallDir 'qcalc.exe')).VersionInfo
    $stray = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) $AppId
    @{ Pass = ($native -and $u.version -eq $New -and $file.ProductVersion -eq $New -and $file.FileDescription -notmatch 'Tauri' -and -not $s.visible -and $script:up.NativeCommand -match '--autostart$' -and -not (Test-Path $stray))
       Detail = "running $($u.version) (exe $($file.ProductVersion)) pid $($native.Id), command line $($script:up.NativeCommand); visible $($s.visible); update state $($u.state) '$($u.line)'; nothing in $stray" }
}

Invoke-Check 'U4' "the Installed apps entry, folder key, shortcut and sign-in value now belong to the native app" {
    $r = Reg $UninstallKey; $d = (Reg $DirKey).'(default)'; $run = (Reg $RunKey).$AppId
    $exe = Join-Path $InstallDir 'qcalc.exe'
    $un = Join-Path $InstallDir 'uninstall.exe'
    $unNew = (Test-Path $un) -and (Get-Item $un).LastWriteTime -gt (Get-Item (Join-Path $Www $newName)).LastWriteTime.AddSeconds(-1)
    @{ Pass = ($r.DisplayVersion -eq $New -and $r.QuietUninstallString -eq "`"$un`" /S" -and $r.UninstallString -eq "`"$un`"" -and $d -eq $InstallDir -and $unNew -and
               (Lnk-Target $Shortcut) -eq $exe -and $run -eq "$exe --autostart")
       Detail = "entry $($r.DisplayName) $($r.DisplayVersion), quiet uninstall '$($r.QuietUninstallString)'; folder key '$d'; uninstall.exe written by the native installer: $unNew; shortcut -> $(Lnk-Target $Shortcut); Run kept as the user had it: '$run'" }
}

Invoke-Check 'U5' 'the Tauri settings and the web view history carry over (moved once, into the shared folder)' {
    $st = Send-Pipe @{ cmd = 'state' }
    $e = Send-Pipe @{ cmd = 'e2e' }
    $rows = @($st.rows | ForEach-Object { $_.expr })
    $expected = @('12 kg to lb', 'r = 5 m', 'f(x) = x^2 + 1', "2$([char]0xD7)$([char]0x3C0)")
    $s = Get-Content (Join-Path $TauriConfig 'settings.json') -Raw | ConvertFrom-Json
    $mark = Get-Content (Join-Path $TauriConfig 'tauri-migrated.json') -Raw -ErrorAction SilentlyContinue
    $m = Get-Content (Join-Path $TauriConfig 'messaging.json') -Raw -ErrorAction SilentlyContinue | ConvertFrom-Json
    $migLine = ((LogText) -split "`n" | Where-Object { $_ -match 'migrate:' }) -join ' | '
    @{ Pass = (($rows -join '|') -eq ($expected -join '|') -and $e.theme -eq 'dark' -and $e.hotkey.wanted -eq 'ctrl+alt+shift+f10' -and $s.settings.angleMode -eq 'rad' -and $s.settings.sigFigs -eq 9 -and $s.autoUpdate -eq $true -and $mark -and $m.name -eq 'ada_l')
       Detail = "rows: $($rows -join ' | ') (last row code points $((($rows | Select-Object -Last 1).ToCharArray() | ForEach-Object { '{0:X}' -f [int]$_ }) -join ' ')); theme $($e.theme), hotkey $($e.hotkey.wanted) (active $($e.hotkey.active)); settings.json angle $($s.settings.angleMode) sigFigs $($s.settings.sigFigs) autoUpdate $($s.autoUpdate); marker $mark; messaging name $($m.name); log: $migLine; Tauri's 'updated' marker left: $(Test-Path (Join-Path $TauriConfig 'updated'))" }
}

Invoke-Check 'U6' "the native app's own update: latest-native.json offers $Next and it installs that by itself" {
    node (Join-Path $installerDir 'latest-native.mjs') $Next "http://127.0.0.1:$Port" $Www "windows-x86_64=$nextName" | Out-Null
    if ($LASTEXITCODE) { throw 'latest-native.mjs failed' }
    $old = $script:up.Native
    $t0 = [Diagnostics.Stopwatch]::StartNew()
    Send-Pipe @{ cmd = 'update check' } | Out-Null
    $seen = [ordered]@{}
    while ($t0.Elapsed.TotalSeconds -lt 180 -and -not $old.HasExited) {
        $u = Send-Pipe @{ cmd = 'update' } 500
        if ($u -and -not $seen.Contains($u.state)) { $seen[$u.state] = $t0.ElapsedMilliseconds }
        Start-Sleep -Milliseconds 100
    }
    $quit = $t0.ElapsedMilliseconds
    $back = Wait-Until { (Copies | Where-Object { $_.Id -ne $old.Id }).Count -ge 1 -and $null -ne (Send-Pipe @{ cmd = 'update' } 300) } 60000
    Copies | ForEach-Object { $script:mine.Add($_.Id) }
    $u = Send-Pipe @{ cmd = 'update' }
    $st = Send-Pipe @{ cmd = 'state' }
    $r = Reg $UninstallKey
    $states = ($seen.Keys | ForEach-Object { "$_ $($seen[$_])" }) -join ', '
    @{ Pass = ($old.HasExited -and $back -and $u.version -eq $Next -and $r.DisplayVersion -eq $Next -and @($st.rows).Count -ge 4 -and (Served "^GET /latest-native.json 200").Count -ge 1)
       Detail = "states (ms): $states; $New quit at $quit ms; $($u.version) answering at $($t0.ElapsedMilliseconds) ms, entry $($r.DisplayVersion), rows $(@($st.rows).Count), visible $($st.visible)" }
}

Invoke-Check 'U7' "the native uninstaller removes the upgraded install (files, entry, folder key, shortcut) and leaves the data" {
    foreach ($p in Copies) { Send-Pipe @{ cmd = 'quit' } 2000 | Out-Null; if (-not $p.WaitForExit(5000)) { Stop-Process -Id $p.Id -Force } }
    $p = Start-Process -FilePath (Join-Path $InstallDir 'uninstall.exe') -ArgumentList '/S' -PassThru
    $p.WaitForExit(60000) | Out-Null
    # NSIS runs the uninstaller from a temp copy, so wait for the files to go
    Wait-Until { -not (Test-Path (Join-Path $InstallDir 'qcalc.exe')) -and -not (Test-Path $UninstallKey) } 30000 | Out-Null
    $run = (Reg $RunKey).$AppId
    @{ Pass = (-not (Test-Path (Join-Path $InstallDir 'qcalc.exe')) -and -not (Test-Path $UninstallKey) -and -not (Test-Path $DirKey) -and -not (Test-Path $Shortcut) -and (Test-Path (Join-Path $TauriConfig 'history.json')))
       Detail = "files gone, entry gone, folder key gone, shortcut gone; data kept; Run value (Tauri-written, unquoted, so not the native app's to remove): '$run'" }
}

# ---- 5. which key a Tauri copy takes when latest.json has both ----

Invoke-Check 'P1' 'a latest.json with windows-x86_64-nsis and windows-x86_64: the Tauri updater (an NSIS build) takes -nsis' {
    if (Test-Path $InstallDir) { Remove-Item $InstallDir -Recurse -Force }
    Wait-WebViews
    Lay-Tauri-Install
    Lay-Tauri-Data
    Copy-Item (Join-Path $Www $newName) (Join-Path $Www 'nsis-key-wins.exe') -Force
    $m = $script:latest | ConvertFrom-Json
    $entry = $m.platforms.'windows-x86_64'
    $both = [ordered]@{ version = $m.version; pub_date = $m.pub_date; platforms = [ordered]@{
        'windows-x86_64-nsis' = [ordered]@{ signature = $entry.signature; url = "http://127.0.0.1:$Port/nsis-key-wins.exe" }
        'windows-x86_64' = [ordered]@{ signature = $entry.signature; url = $entry.url } } }
    [IO.File]::WriteAllText((Join-Path $Www 'latest.json'), ($both | ConvertTo-Json -Depth 5))
    Remove-Item (Join-Path $Www 'latest-native.json') -ErrorAction SilentlyContinue
    Remove-Item $Log -ErrorAction SilentlyContinue
    $n = (Get-Content $ServerLog).Count
    Set-AppEnv
    $tauri = Start-Process -FilePath (Join-Path $InstallDir 'qcalc.exe') -ArgumentList '--autostart' -PassThru
    $script:mine.Add($tauri.Id)
    $w = Watch-Upgrade $tauri 'nsis-key-wins.exe'
    $got = @(Get-Content $ServerLog | Select-Object -Skip $n | Where-Object { $_ -match '\.exe' })
    $u = Send-Pipe @{ cmd = 'update' }
    @{ Pass = ($got.Count -eq 1 -and $got[0] -match '^GET /nsis-key-wins\.exe 200' -and $u.version -eq $New)
       Detail = "fetched: $($got -join '; '); timeline (ms): $($w.Timeline); then running $($u.version). So the release's latest.json must not carry windows-x86_64-nsis (latest-json.mjs refuses any key but windows-x86_64 / windows-aarch64)" }
}

} finally {
    # ---- clean up: only what this run made ----
    foreach ($p in Copies) { Send-Pipe @{ cmd = 'quit' } 2000 | Out-Null; if (-not $p.WaitForExit(5000)) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue } }
    foreach ($id in $script:mine) { Stop-Process -Id $id -Force -ErrorAction SilentlyContinue }
    Ours-Under-Root | ForEach-Object { Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue }
    Stop-Server
    $un = Join-Path $InstallDir 'uninstall.exe'
    if (Test-Path $un) {
        $p = Start-Process -FilePath $un -ArgumentList '/S' -PassThru
        $p.WaitForExit(60000) | Out-Null
        Wait-Until { -not (Test-Path (Join-Path $InstallDir 'qcalc.exe')) } 30000 | Out-Null
    }
    Wait-WebViews
    $leftovers = @()
    if (Test-Path $UninstallKey) { $leftovers += 'uninstall entry'; Remove-Item $UninstallKey -Recurse -ErrorAction SilentlyContinue }
    if (Test-Path $DirKey) { $leftovers += 'folder key'; Remove-Item $DirKey -Recurse -ErrorAction SilentlyContinue }
    if ((Reg $RunKey).$AppId) { $leftovers += 'Run value'; Remove-ItemProperty -Path $RunKey -Name $AppId -ErrorAction SilentlyContinue }
    if ((Reg $ApprovedKey).$AppId) { $leftovers += 'StartupApproved value'; Remove-ItemProperty -Path $ApprovedKey -Name $AppId -ErrorAction SilentlyContinue }
    if (Test-Path $Shortcut) { $leftovers += 'shortcut'; Remove-Item $Shortcut -ErrorAction SilentlyContinue }
    foreach ($d in $TauriConfig, $TauriLocal) { if (Test-Path $d) { Remove-Item $d -Recurse -Force -ErrorAction SilentlyContinue } }
    Restore-Env
    if (Test-Path $Log) { Copy-Item $Log (Join-Path $OutDir 'qcalc.log') -Force -ErrorAction SilentlyContinue }
    if (Test-Path $ServerLog) { Copy-Item $ServerLog (Join-Path $OutDir 'server.log') -Force -ErrorAction SilentlyContinue }
    # rd with \\?\: the cargo build's paths can pass MAX_PATH, which Remove-Item can't delete
    if (-not $KeepTemp) { cmd /c "rd /s /q `"\\?\$Root`" 2>nul" }
}

$after = @(Fingerprint)
$diff = @(Compare-Object $before $after | ForEach-Object { "$($_.SideIndicator) $($_.InputObject)" })
# the running Q Calc (or any copy run without QCALC_LOG) may write the shared log and rates.json meanwhile, so those are only checked for lines of this run; anything else changing is a failure
$own = @($diff | Where-Object { $_ -notmatch '^(<=|=>) data .*\\com\.maxconine\.qcalc\\(qcalc\.log|rates\.json) ' })
$realLog = Real-Log
$foreign = $realLog.Length -gt $realLogBefore -and ($realLog.Substring([Math]::Min($realLogBefore, $realLog.Length)) -match [regex]::Escape($Root) -or $realLog.Substring([Math]::Min($realLogBefore, $realLog.Length)) -match 'Upgrade Test')
$script:results.Add([pscustomobject]@{ Id = 'S1'; Name = "the real install's files, entries, Run and StartupApproved values, shortcuts and data are untouched"; Pass = ($own.Count -eq 0 -and -not $foreign)
    Detail = $(if ($diff.Count -eq 0) { "identical ($($before.Count) lines)" } elseif ($own.Count -eq 0) { "identical but for the shared log and rates (the running Q Calc, or any other copy run without QCALC_LOG, writes them; none of this run's lines are in the log): $($diff -join ' ')" } else { "changed: $($own -join ' || ')" }) + "; nothing of this run in the real log: $(-not $foreign)"; Ms = 0 })
$gone = -not (Test-Path $TauriConfig) -and -not (Test-Path $TauriLocal) -and -not (Test-Path $UninstallKey) -and -not (Test-Path $DirKey) -and -not (Test-Path $Shortcut) -and -not (Reg $RunKey).$AppId -and ($KeepTemp -or -not (Test-Path $Root))
$script:results.Add([pscustomobject]@{ Id = 'S2'; Name = 'everything this run made is gone'; Pass = $gone; Detail = "removed by hand: $(if ($leftovers) { $leftovers -join ', ' } else { 'nothing' }); work folder $(if ($KeepTemp) { "kept: $Root" } else { 'removed' })"; Ms = 0 })

$failed = @($script:results | Where-Object { -not $_.Pass })
$md = New-Object System.Text.StringBuilder
[void]$md.AppendLine("## Q Calc: a Tauri $Old install auto-updates into the native app")
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
