# Moving in from the Tauri app, end to end: a fresh QCALC_DATA holding the Tauri app's settings.json and, as its
# web view's storage, the fixture LevelDB (tests/fixtures/tauri: Chromium's own files, made-up data). An isolated
# copy must start with the Tauri history on its tape (a migrated definition still answers), the Tauri settings
# (radians, dark, the shortcut, the bar's width), the messaging name and the game saves, and move them only once.
#
# With -Makensis (NSIS 3) it also installs an older native build into a temp folder under an app id of its own
# (registry names, shortcut), uses it, upgrades it in place, and checks the data is all still there. The installer
# never touches the data folder, and a temp folder isn't an "installed" place (autostart.rs), so neither copy writes
# a sign-in entry or checks for updates; the test's own registry entries and shortcut are removed at the end.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File native\windows\tests\e2e\migrate.ps1 -Exe native\target\release\qcalc.exe [-Makensis <makensis.exe>]
param(
    [string]$Exe = (Join-Path $PSScriptRoot '..\..\..\target\release\qcalc.exe'),
    [string]$Instance = 'migrate-e2e',
    [string]$Makensis
)
$ErrorActionPreference = 'Stop'
$Exe = (Resolve-Path $Exe).Path
$fixtures = (Resolve-Path (Join-Path $PSScriptRoot '..\fixtures\tauri')).Path
$Root = Join-Path ([IO.Path]::GetTempPath()) ("qcalc-migrate-{0}-{1}" -f $Instance, [Guid]::NewGuid().ToString('N').Substring(0, 8))
$Data = Join-Path $Root 'data'
$Web = Join-Path $Root 'EBWebView\Default\Local Storage\leveldb'
$Log = Join-Path $Root 'qcalc.log'
New-Item -ItemType Directory -Force $Data, $Web | Out-Null
Copy-Item (Join-Path $fixtures 'settings.json') $Data
Copy-Item (Join-Path $fixtures 'leveldb\*') $Web
$env:QCALC_INSTANCE = $Instance
$env:QCALC_DATA = $Data
$env:QCALC_LOG = $Log
$env:QCALC_BENCH = '1'
$env:QCALC_MIGRATE = $Web
$env:QCALC_NO_OPEN = '1'
Remove-Item Env:QCALC_TEST_CRASH, Env:QCALC_CRASH_CARD -ErrorAction SilentlyContinue

$script:results = New-Object System.Collections.Generic.List[object]
$script:mine = New-Object System.Collections.Generic.List[object]
function Log([string]$m) { Write-Host ("[{0:HH:mm:ss.fff}] {1}" -f (Get-Date), $m) }
function Check([string]$id, [string]$name, [bool]$pass, [string]$detail) {
    $script:results.Add([pscustomobject]@{ Id = $id; Name = $name; Pass = $pass; Detail = $detail })
    Write-Host ("{0} {1} {2}: {3}" -f $(if ($pass) { 'PASS' } else { 'FAIL' }), $id, $name, $detail) -ForegroundColor $(if ($pass) { 'Green' } else { 'Red' })
}
function Wait-Until([scriptblock]$cond, [int]$ms = 10000) {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    do { if (& $cond) { return $true }; Start-Sleep -Milliseconds 100 } while ($sw.ElapsedMilliseconds -lt $ms)
    [bool](& $cond)
}
function Send-Pipe([hashtable]$req, [int]$timeout = 3000, [string]$name = $Instance) {
    $pipe = New-Object System.IO.Pipes.NamedPipeClientStream('.', "qcalc-bench-$name", [System.IO.Pipes.PipeDirection]::InOut)
    try {
        $pipe.Connect($timeout)
        $w = New-Object System.IO.StreamWriter($pipe); $w.AutoFlush = $true
        $w.WriteLine(($req | ConvertTo-Json -Compress))
        $line = (New-Object System.IO.StreamReader($pipe)).ReadLine()
        if ($line) { $line | ConvertFrom-Json } else { $null }
    } catch { $null } finally { $pipe.Dispose() }
}
function Start-Copy([string]$path = $Exe) {
    $p = Start-Process -FilePath $path -ArgumentList '--autostart' -PassThru
    $script:mine.Add($p)
    Log "started pid $($p.Id) $path"
    $p
}
function Stop-Copy($p, [string]$name = $Instance) {
    if (-not $p -or $p.HasExited) { return }
    Send-Pipe @{ cmd = 'quit' } 2000 $name | Out-Null
    if (-not $p.WaitForExit(5000)) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
}
function Answer([string]$text, [string]$name = $Instance) {
    Send-Pipe @{ cmd = 'type'; text = "" } 2000 $name | Out-Null
    Send-Pipe @{ cmd = 'key'; vk = 27 } 2000 $name | Out-Null
    Send-Pipe @{ cmd = 'show' } 2000 $name | Out-Null
    $st = Send-Pipe @{ cmd = 'state' } 2000 $name
    # clear the line, type, and wait for this text's answer
    foreach ($i in 1..40) { if (-not $st -or -not $st.text) { break }; Send-Pipe @{ cmd = 'key'; vk = 8 } 1000 $name | Out-Null; $st = Send-Pipe @{ cmd = 'state' } 1000 $name }
    Send-Pipe @{ cmd = 'type'; text = $text } 2000 $name | Out-Null
    $script:ans = $null
    Wait-Until { $s = Send-Pipe @{ cmd = 'state' } 1000 $name; if ($s -and $s.liveFor -eq $text -and $s.answer) { $script:ans = $s.answer; $true } else { $false } } 8000 | Out-Null
    $script:ans
}
function LogText { if (Test-Path $Log) { [IO.File]::ReadAllText($Log) } else { '' } }
$expected = @('12 kg to lb', 'r = 5 m', 'f(x) = x^2 + 1', "2$([char]0xD7)$([char]0x3C0)")

$AppId = "Q Calc Migrate E2E $Instance"
$UninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$AppId"
$RunKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$InstallDir = Join-Path $Root 'install'
$Shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) "$AppId.lnk"

try {
    $p = Start-Copy
    $up = Wait-Until { $null -ne (Send-Pipe @{ cmd = 'state' } 1000) } 20000
    $st = Send-Pipe @{ cmd = 'state' }
    $rows = @($st.rows | ForEach-Object { $_.expr })
    Check 'MG1' 'the Tauri history is on the tape' ($up -and (($rows -join '|') -eq ($expected -join '|')) -and (Test-Path (Join-Path $Data 'history.json'))) "rows: $($rows -join ' | ')"

    $e = Send-Pipe @{ cmd = 'e2e' }
    Check 'MG2' "the Tauri settings apply (dark, the shortcut, the bar's width)" ($e.theme -eq 'dark' -and $e.dark -and $e.hotkey.wanted -eq 'ctrl+alt+shift+f10' -and ($e.frame[2] - $e.frame[0]) -ge [int](880 * $e.scale)) `
        "theme $($e.theme) dark $($e.dark); hotkey $($e.hotkey.wanted) active $($e.hotkey.active); bar width $($e.frame[2] - $e.frame[0]) px at scale $($e.scale)"

    $a1 = Answer 'r*2'
    $a2 = Answer 'sin(pi/2)'
    $a3 = Answer 'f(3)'
    Check 'MG3' 'a migrated definition and function still answer, in radians' ($a1 -match '^10\s*m$' -and $a2 -eq '1' -and $a3 -eq '10') "r*2 = $a1; sin(pi/2) = $a2; f(3) = $a3"
    Send-Pipe @{ cmd = 'hide' } | Out-Null

    $m = Get-Content (Join-Path $Data 'messaging.json') -Raw -ErrorAction SilentlyContinue | ConvertFrom-Json
    $g = Get-Content (Join-Path $Data 'games.json') -Raw -ErrorAction SilentlyContinue | ConvertFrom-Json
    Check 'MG4' 'the messaging name and key, the lists, the game saves and nickname move' ($m.name -eq 'ada_l' -and $m.key.Length -eq 64 -and $m.accepted -contains 'grace_h' -and $m.blocked -contains 'spam_bot' -and $g.nickname -eq 'teal otter' -and $g.gameSaves.'qcalc-crack' -eq '7') `
        "name $($m.name) key $($m.key.Length) hex, accepted $($m.accepted -join ','), blocked $($m.blocked -join ','); nickname $($g.nickname), crack $($g.gameSaves.'qcalc-crack')"

    Stop-Copy $p
    $s = Get-Content (Join-Path $Data 'settings.json') -Raw | ConvertFrom-Json
    Check 'MG5' "the native app's save keeps the Tauri settings" ($s.settings.angleMode -eq 'rad' -and $s.settings.sigFigs -eq 9 -and $s.hotkey -eq 'ctrl+alt+shift+f10' -and $s.width -eq 900 -and $s.onboarding.done -and $s.signIn -eq $true) `
        "angle $($s.settings.angleMode), sigFigs $($s.settings.sigFigs), hotkey $($s.hotkey), width $($s.width), signIn kept: $($null -ne $s.signIn)"

    # once: a later launch doesn't read the Tauri storage again, and a later Tauri write can't come back over the native app's files
    $p = Start-Copy
    $up = Wait-Until { $null -ne (Send-Pipe @{ cmd = 'state' } 1000) } 20000
    $st = Send-Pipe @{ cmd = 'state' }
    $moves = ([regex]::Matches((LogText), 'migrate: from the Tauri app')).Count
    Check 'MG6' 'the move happens once (the marker), and the tape survives a restart' ($up -and $moves -eq 1 -and (Test-Path (Join-Path $Data 'tauri-migrated.json')) -and @($st.rows).Count -eq 4) "moves logged: $moves; rows after restart: $(@($st.rows).Count)"
    Stop-Copy $p

    # ---- the installer: an older native install, upgraded in place, keeps the data ----
    if (-not $Makensis) {
        foreach ($c in "$env:LOCALAPPDATA\tauri\NSIS\makensis.exe", "${env:ProgramFiles(x86)}\NSIS\makensis.exe") { if (-not $Makensis -and (Test-Path $c)) { $Makensis = $c } }
    }
    if (-not $Makensis) { Log 'installer upgrade check skipped: no makensis' }
    else {
        $nsi = (Resolve-Path (Join-Path $PSScriptRoot '..\..\installer\qcalc.nsi')).Path
        $setups = foreach ($v in '0.0.1', '0.0.2') {
            $out = Join-Path $Root "setup-$v.exe"
            & $Makensis /V2 "/DVERSION=$v" "/DEXE=$Exe" "/DOUT=$out" "/DAPP_ID=$AppId" $nsi | Out-Host
            if ($LASTEXITCODE) { throw "makensis failed for $v" }
            $out
        }
        $installed = Join-Path $InstallDir 'qcalc.exe'
        $run = { param($setup) $q = Start-Process -FilePath $setup -ArgumentList @('/S', '/NORELAUNCH', "/D=$InstallDir") -PassThru; if (-not $q.WaitForExit(120000)) { throw 'setup hung' }; $q.ExitCode }
        $c1 = & $run $setups[0]
        $inst = "$Instance-inst"
        $env:QCALC_INSTANCE = $inst
        try {
            $p = Start-Copy $installed
            $up1 = Wait-Until { $null -ne (Send-Pipe @{ cmd = 'state' } 1000 $inst) } 20000
            $a = Answer '7*6' $inst
            Send-Pipe @{ cmd = 'key'; vk = 13 } 2000 $inst | Out-Null
            Wait-Until { @((Send-Pipe @{ cmd = 'state' } 1000 $inst).rows).Count -ge 5 } 5000 | Out-Null
            $before = @((Send-Pipe @{ cmd = 'state' } 1000 $inst).rows | ForEach-Object { $_.expr })
            # the upgrade closes the running copy itself (its exact path) before replacing it
            $c2 = & $run $setups[1]
            $closed = $p.WaitForExit(10000)
            $ver = (Get-ItemProperty $UninstallKey -ErrorAction SilentlyContinue).DisplayVersion
            $p = Start-Copy $installed
            $up2 = Wait-Until { $null -ne (Send-Pipe @{ cmd = 'state' } 1000 $inst) } 20000
            $after = @((Send-Pipe @{ cmd = 'state' } 1000 $inst).rows | ForEach-Object { $_.expr })
            $e = Send-Pipe @{ cmd = 'e2e' } 2000 $inst
            $runValue = (Get-ItemProperty $RunKey -ErrorAction SilentlyContinue).$AppId
            Stop-Copy $p $inst
            $msg = Get-Content (Join-Path $Data 'messaging.json') -Raw | ConvertFrom-Json
            Check 'MG7' 'an older native install upgraded in place keeps the tape, the settings and the name' ($c1 -eq 0 -and $c2 -eq 0 -and $up1 -and $closed -and $ver -eq '0.0.2' -and $up2 -and $a -eq '42' -and (($after -join '|') -eq ($before -join '|')) -and $after -contains '7*6' -and $e.theme -eq 'dark' -and $msg.name -eq 'ada_l') `
                "setup exits $c1/$c2; old copy closed: $closed; version $ver; rows before $($before.Count) after $($after.Count); theme $($e.theme); name $($msg.name); test Run value (fresh install): $([bool]$runValue)"
        } finally { $env:QCALC_INSTANCE = $Instance }
    }
} finally {
    foreach ($p in $script:mine) { if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue } }
    if (Test-Path (Join-Path $InstallDir 'uninstall.exe')) {
        $u = Start-Process (Join-Path $InstallDir 'uninstall.exe') -ArgumentList '/S' -PassThru
        $u.WaitForExit(60000) | Out-Null
        Start-Sleep -Seconds 2
    }
    # only this test's own entries
    Remove-Item $UninstallKey -Recurse -ErrorAction SilentlyContinue
    Remove-ItemProperty -Path $RunKey -Name $AppId -ErrorAction SilentlyContinue
    Remove-Item $Shortcut -ErrorAction SilentlyContinue
    Remove-Item Env:QCALC_INSTANCE, Env:QCALC_DATA, Env:QCALC_LOG, Env:QCALC_BENCH, Env:QCALC_MIGRATE, Env:QCALC_NO_OPEN -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 300
    Remove-Item $Root -Recurse -Force -ErrorAction SilentlyContinue
}
$failed = @($script:results | Where-Object { -not $_.Pass })
"{0} of {1} migration checks passed" -f ($script:results.Count - $failed.Count), $script:results.Count
if ($failed.Count) { exit 1 }
