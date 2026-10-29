# Crash reports end to end (crash.rs): a copy that panics on its UI thread, and one that takes an access
# violation, each leave a report (and the second a minidump); the next launch offers it once ("Report it" opens a
# short pre-filled issue and the folder: logged, not opened, under QCALC_NO_OPEN); the tray's Report a Problem…
# (through the bench pipe's `report`) makes the folder with the log.
#
# Everything is its own: QCALC_INSTANCE, QCALC_DATA, QCALC_LOG in a temp folder, a hotkey nobody uses, no SendInput,
# and only processes it started are stopped.
#   powershell -NoProfile -ExecutionPolicy Bypass -File native\windows\tests\e2e\crash.ps1 -Exe native\target\release\qcalc.exe
param(
    [string]$Exe = (Join-Path $PSScriptRoot '..\..\..\target\release\qcalc.exe'),
    [string]$Instance = 'crash-e2e',
    [switch]$Keep
)
$ErrorActionPreference = 'Stop'
$Exe = (Resolve-Path $Exe).Path
$Root = Join-Path ([IO.Path]::GetTempPath()) ("qcalc-crash-{0}-{1}" -f $Instance, [Guid]::NewGuid().ToString('N').Substring(0, 8))
$Data = Join-Path $Root 'data'
$Log = Join-Path $Root 'qcalc.log'
New-Item -ItemType Directory -Force $Data | Out-Null
$env:QCALC_INSTANCE = $Instance
$env:QCALC_DATA = $Data
$env:QCALC_LOG = $Log
$env:QCALC_BENCH = '1'
$env:QCALC_NO_OPEN = '1'
Remove-Item Env:QCALC_TEST_CRASH, Env:QCALC_CRASH_CARD, Env:QCALC_MIGRATE -ErrorAction SilentlyContinue
# a chord nobody uses, never Alt+Space; no update checks, no usage posts
$store = @{ hotkey = 'ctrl+alt+shift+f11'; firstRunDone = $true; autoUpdate = $false; settings = @{ shareUsage = $false } }
[IO.File]::WriteAllText((Join-Path $Data 'settings.json'), ($store | ConvertTo-Json -Depth 4))

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
function Send-Pipe([hashtable]$req, [int]$timeout = 3000) {
    $pipe = New-Object System.IO.Pipes.NamedPipeClientStream('.', "qcalc-bench-$Instance", [System.IO.Pipes.PipeDirection]::InOut)
    try {
        $pipe.Connect($timeout)
        $w = New-Object System.IO.StreamWriter($pipe); $w.AutoFlush = $true
        $w.WriteLine(($req | ConvertTo-Json -Compress))
        $line = (New-Object System.IO.StreamReader($pipe)).ReadLine()
        if ($line) { $line | ConvertFrom-Json } else { $null }
    } catch { $null } finally { $pipe.Dispose() }
}
function Start-Copy {
    $p = Start-Process -FilePath $Exe -ArgumentList '--autostart' -PassThru
    $script:mine.Add($p)
    Log "started pid $($p.Id)"
    $p
}
function Stop-Copy($p) {
    if (-not $p -or $p.HasExited) { return }
    Send-Pipe @{ cmd = 'quit' } 2000 | Out-Null
    if (-not $p.WaitForExit(5000)) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
}
function LogText {
    # the app appends while this reads: share it both ways, and try again on a sharing clash
    foreach ($i in 1..10) {
        if (-not (Test-Path $Log)) { return '' }
        try {
            $fs = New-Object System.IO.FileStream($Log, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]'ReadWrite, Delete')
            try { return (New-Object System.IO.StreamReader($fs)).ReadToEnd() } finally { $fs.Dispose() }
        } catch { Start-Sleep -Milliseconds 50 }
    }
    ''
}
function Reports { @(Get-ChildItem (Join-Path $Data 'crashes') -Filter 'crash-*.txt' -ErrorAction SilentlyContinue | Sort-Object Name) }

try {
    # ---- a panic on the UI thread ----
    $env:QCALC_TEST_CRASH = 'panic'
    $p = Start-Copy
    $ended = $p.WaitForExit(20000)
    Remove-Item Env:QCALC_TEST_CRASH
    $r = Reports
    $text = if ($r.Count) { [IO.File]::ReadAllText($r[-1].FullName) } else { '' }
    Check 'C1' 'a UI-thread panic writes a report and the process ends' ($ended -and $r.Count -eq 1 -and $text -match '(?m)^summary: panic at crash\.rs:\d+$' -and $text -match '(?m)^version: ' -and $text -match '(?m)^os: Windows' -and $text -match 'a test crash' -and $text -match 'log \(last' -and $text -match 'qcalc\.exe\+0x') `
        "ended: $ended (exit $(if ($ended) { '0x{0:X}' -f $p.ExitCode })); reports: $($r.Count); summary: $(($text -split "`n" | Where-Object { $_ -like 'summary:*' }) -join '')"

    # ---- the next launch offers it, once ----
    $env:QCALC_CRASH_CARD = 'report'
    $p = Start-Copy
    $up = Wait-Until { $null -ne (Send-Pipe @{ cmd = 'state' } 1000) } 15000
    $offered = Wait-Until { (LogText) -match 'would show .*crash-\d+\.txt' } 10000
    $logText = LogText
    $url = ([regex]::Match($logText, 'report: would open (\S+)')).Groups[1].Value
    $seen = @(Get-ChildItem (Join-Path $Data 'crashes') -Filter '*.seen' -ErrorAction SilentlyContinue).Count
    $clean = $url -and $url.Length -lt 1500 -and $url -notmatch 'Users|%5C|AppData|test%20crash' -and $url -match '^https://github\.com/maxconine/Q-Calc/issues/new\?title=Windows%20native%20[^&]+panic%20at%20crash\.rs' -and $url -match 'crash-\d+\.txt'
    Check 'C2' 'the next launch offers it; Report it opens a short issue with no personal data, and the folder' ($up -and $offered -and $seen -eq 1 -and $clean) `
        "up: $up; card and folder: $offered; marked offered: $seen; url ($($url.Length) chars): $url"
    Stop-Copy $p
    $p = Start-Copy
    $up = Wait-Until { $null -ne (Send-Pipe @{ cmd = 'state' } 1000) } 15000
    Start-Sleep -Milliseconds 1500
    $cards = ([regex]::Matches((LogText), 'crash: card shown')).Count
    Check 'C3' 'it is offered only once' ($up -and $cards -eq 1) "cards shown over two launches: $cards"

    # ---- the tray's Report a Problem… ----
    $reply = Send-Pipe @{ cmd = 'report' }
    $folder = Join-Path $Data 'report'
    $made = Wait-Until { (Test-Path (Join-Path $folder 'qcalc.log')) -and ((LogText) -match 'report: would show .*report\\qcalc\.log') } 10000
    $copied = @(Get-ChildItem $folder -Filter 'crash-*.txt' -ErrorAction SilentlyContinue).Count
    $turl = ([regex]::Matches((LogText), 'report: would open (\S+)') | Select-Object -Last 1).Groups[1].Value
    Check 'C4' 'Report a Problem makes a folder with the log and the newest report, and opens the form' ($reply.ok -and $made -and $copied -eq 1 -and $turl -match 'reported%20from%20the%20tray' -and $turl.Length -lt 1500) `
        "folder: $made; crash report copied: $copied; url: $($turl.Length) chars"
    Stop-Copy $p
    Remove-Item Env:QCALC_CRASH_CARD

    # ---- an access violation: the filter's report and a minidump ----
    $env:QCALC_TEST_CRASH = 'access'
    $p = Start-Copy
    $ended = $p.WaitForExit(30000)
    Remove-Item Env:QCALC_TEST_CRASH
    $r = Reports
    $text = if ($r.Count -ge 2) { [IO.File]::ReadAllText($r[-1].FullName) } else { '' }
    $dmp = ([regex]::Match($text, '(?m)^minidump: (\S+)$')).Groups[1].Value
    $dmpLen = if ($dmp -and (Test-Path (Join-Path $Data "crashes\$dmp"))) { (Get-Item (Join-Path $Data "crashes\$dmp")).Length } else { 0 }
    Check 'C5' 'an access violation writes a report and a minidump' ($ended -and $text -match '(?m)^summary: exception 0xC0000005 \(access violation\) at qcalc\.exe\+0x[0-9a-f]+$' -and $dmpLen -gt 1000) `
        "ended: $ended (exit $(if ($ended) { '0x{0:X}' -f $p.ExitCode })); summary: $(($text -split "`n" | Where-Object { $_ -like 'summary:*' }) -join ''); minidump: $dmp ($dmpLen bytes)"
} finally {
    foreach ($p in $script:mine) { if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue } }
    Remove-Item Env:QCALC_INSTANCE, Env:QCALC_DATA, Env:QCALC_LOG, Env:QCALC_BENCH, Env:QCALC_NO_OPEN, Env:QCALC_TEST_CRASH, Env:QCALC_CRASH_CARD -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 300
    if ($Keep) { "kept $Root" } else { Remove-Item $Root -Recurse -Force -ErrorAction SilentlyContinue }
}
$failed = @($script:results | Where-Object { -not $_.Pass })
"{0} of {1} crash checks passed" -f ($script:results.Count - $failed.Count), $script:results.Count
if ($failed.Count) { exit 1 }
