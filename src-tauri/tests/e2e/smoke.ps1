# end-to-end smoke test for the windows app; needs an interactive desktop (github's windows runners have one)
# runs under Windows PowerShell 5.1 so UI Automation and C# 5 Add-Type are there without extra installs
param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [string]$OutDir = (Join-Path (Get-Location) 'e2e-out')
)
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
Start-Transcript -Path (Join-Path $OutDir 'log.txt') -Force | Out-Null

Add-Type -TypeDefinition (Get-Content -Raw (Join-Path $PSScriptRoot 'Native.cs')) -ReferencedAssemblies System.Drawing, System.Core
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$N = [QCalcE2E.Native]
$N::SetProcessDPIAware() | Out-Null

$ProcName = [IO.Path]::GetFileNameWithoutExtension($Exe)
$script:results = New-Object System.Collections.Generic.List[object]
$script:proc = $null
$script:hotkey = $null
$script:other = $null
$script:otherHwnd = [IntPtr]::Zero

function Log([string]$msg) { Write-Host ("[{0:HH:mm:ss.fff}] {1}" -f (Get-Date), $msg) }

function Wait-Until([scriptblock]$cond, [int]$ms = 2000) {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    do {
        if (& $cond) { return $true }
        Start-Sleep -Milliseconds 50
    } while ($sw.ElapsedMilliseconds -lt $ms)
    return [bool](& $cond)
}

function Get-QCalcPids { @(Get-Process -Name $ProcName -ErrorAction SilentlyContinue | ForEach-Object { $_.Id }) }

# the calculator window: titled "Q Calc", or "Q Calc = <answer>" when the e2e hook is in
function Get-QCalcWindow {
    $pids = Get-QCalcPids
    if (-not $pids) { return [IntPtr]::Zero }
    $wins = @($N::Windows([int[]]$pids, 'Q Calc') | Where-Object { $t = $N::Title($_); $t -eq 'Q Calc' -or $t -like 'Q Calc = *' })
    if ($script:proc) {
        $own = @($wins | Where-Object { $N::ProcessOf($_) -eq $script:proc.Id })
        if ($own) { return $own[0] }
    }
    if ($wins) { return $wins[0] }
    return [IntPtr]::Zero
}

function Test-Shown { $h = Get-QCalcWindow; $h -ne [IntPtr]::Zero -and $N::IsWindowVisible($h) }
function Test-Front { $h = Get-QCalcWindow; $h -ne [IntPtr]::Zero -and $N::GetForegroundWindow() -eq $h }

function Describe-Foreground {
    $f = $N::GetForegroundWindow()
    if ($f -eq [IntPtr]::Zero) { return 'none' }
    $p = Get-Process -Id ($N::ProcessOf($f)) -ErrorAction SilentlyContinue
    "'{0}' class {1} ({2})" -f $N::Title($f), $N::ClassName($f), $(if ($p) { $p.ProcessName } else { '?' })
}

function Height($h) { $r = $N::Rect($h); $r.Bottom - $r.Top }

function Send-Hotkey([string]$which) {
    if ($which -eq 'Alt+Space') { $N::Chord($N::VK_MENU, $N::VK_SPACE) }
    else { $N::Chord($N::VK_CONTROL, $N::VK_MENU, $N::VK_SPACE) }
}

# the default hotkey first, the fallback only if nothing showed; remembers which one worked
function Invoke-Hotkey([int]$ms = 2000) {
    $order = if ($script:hotkey) { @($script:hotkey) } else { @('Alt+Space', 'Ctrl+Alt+Space') }
    foreach ($k in $order) {
        Log "sending $k"
        Send-Hotkey $k
        if (Wait-Until { Test-Shown } $ms) {
            $script:hotkey = $k
            return $k
        }
        # an unregistered alt+space opens some other window's system menu; close it before the fallback
        $N::Chord($N::VK_ESCAPE)
        Start-Sleep -Milliseconds 200
    }
    return $null
}

# the composer sits at the top of a freshly shown window
function Click-Input {
    $h = Get-QCalcWindow
    $r = $N::Rect($h)
    $N::Click($r.Left + 120, $r.Top + 36)
}

# shows and focuses the window for checks that only care about what happens after that
function Show-QCalc {
    if (-not (Test-Shown)) {
        if (-not (Invoke-Hotkey)) { throw 'could not show the window with either hotkey' }
    }
    if (-not (Wait-Until { Test-Front } 1500)) {
        Log "shown but not in front (front: $(Describe-Foreground)); clicking the input"
        Click-Input
        if (-not (Wait-Until { Test-Front } 1500)) { throw "window is shown but never came to the front; front is $(Describe-Foreground)" }
    }
    Start-Sleep -Milliseconds 300
    Get-QCalcWindow
}

function Hide-QCalc {
    if (Test-Shown) {
        if (Test-Front) { $N::Chord($N::VK_ESCAPE) }
        if (-not (Wait-Until { -not (Test-Shown) } 1500)) {
            Open-Other
            Click-Other
            Wait-Until { -not (Test-Shown) } 1500 | Out-Null
        }
    }
}

function Clear-Input {
    $N::Chord($N::VK_CONTROL, $N::VK_A)
    $N::Chord($N::VK_BACK)
    Start-Sleep -Milliseconds 200
}

function Open-Other {
    if ($script:other -and -not $script:other.HasExited -and $script:otherHwnd -ne [IntPtr]::Zero) { return }
    $s = $N::Screen()
    $y = [Math]::Max(0, $s.Height - 300)
    $script:other = Start-Process powershell -PassThru -ArgumentList @(
        '-NoProfile', '-ExecutionPolicy', 'Bypass', '-WindowStyle', 'Hidden',
        '-File', (Join-Path $PSScriptRoot 'other-window.ps1'), '-X', '20', '-Y', "$y")
    $found = Wait-Until { $w = @($N::Windows([int[]]@($script:other.Id), 'qcalc-e2e-other')); $w.Count -gt 0 } 15000
    if (-not $found) { throw 'the other window never appeared' }
    $script:otherHwnd = @($N::Windows([int[]]@($script:other.Id), 'qcalc-e2e-other'))[0]
}

# a real click on another app's window; that's what takes focus away on a desktop
function Click-Other {
    $r = $N::Rect($script:otherHwnd)
    $x = [int](($r.Left + $r.Right) / 2); $y = [int](($r.Top + $r.Bottom) / 2)
    $q = Get-QCalcWindow
    if ($q -ne [IntPtr]::Zero -and $N::IsWindowVisible($q)) {
        $qr = $N::Rect($q)
        if ($x -ge $qr.Left -and $x -le $qr.Right -and $y -ge $qr.Top -and $y -le $qr.Bottom) {
            $y = $r.Bottom - 20
            Log 'click point overlapped the calculator; clicking the bottom of the other window'
        }
    }
    $N::Click($x, $y)
    Start-Sleep -Milliseconds 300
}

function Normalize([string]$s) {
    (($s -replace '[\u2009\u202F\u00A0\u200B,]', '') -replace '\u2212', '-').Trim()
}

function Get-UiaNames($hwnd) {
    $root = [System.Windows.Automation.AutomationElement]::FromHandle($hwnd)
    $all = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
    foreach ($e in $all) {
        try { $n = $e.Current.Name; if ($n) { $n } } catch { }
    }
}

# the hook's window title first (exact), then webview2's accessibility tree; chromium builds that tree lazily,
# so the first queries can come back empty and this polls
function Read-Answer([string]$pattern, [int]$ms = 6000) {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $names = @()
    do {
        $h = Get-QCalcWindow
        # FromHandle(0) would walk the whole desktop
        if ($h -eq [IntPtr]::Zero) { Start-Sleep -Milliseconds 250; continue }
        $title = $N::Title($h)
        if ($title -like 'Q Calc = *') {
            $a = Normalize $title.Substring(9)
            if ($a -match $pattern) { return @{ Found = $true; Text = $a; Source = 'hook title' } }
        }
        $names = @(Get-UiaNames $h | ForEach-Object { Normalize $_ } | Where-Object { $_ })
        $hit = @($names | Where-Object { $_ -match $pattern })
        if ($hit) { return @{ Found = $true; Text = $hit[0]; Source = 'ui automation' } }
        Start-Sleep -Milliseconds 250
    } while ($sw.ElapsedMilliseconds -lt $ms)
    $seen = ($names | Select-Object -First 25) -join ' | '
    if ($title -like 'Q Calc = *') { $seen = "title '$title'; uia: $seen" }
    return @{ Found = $false; Text = $seen; Source = '' }
}

function Invoke-Check([string]$id, [string]$name, [bool]$required, [scriptblock]$body) {
    Log "--- $id $name"
    $pass = $false; $detail = ''
    try {
        $r = @(& $body)[-1]
        $pass = [bool]$r.Pass; $detail = [string]$r.Detail
    } catch {
        $detail = "error: $($_.Exception.Message)"
    }
    $shot = "$id-$(($name -replace '[^A-Za-z0-9]+', '-').Trim('-').ToLower()).png"
    try { $N::Screenshot((Join-Path $OutDir $shot)) } catch { $shot = "(screenshot failed: $($_.Exception.Message))" }
    $script:results.Add([pscustomobject]@{ Id = $id; Name = $name; Required = $required; Pass = $pass; Detail = $detail; Shot = $shot })
    Log ("{0} {1}: {2}" -f $(if ($pass) { 'PASS' } else { 'FAIL' }), $id, $detail)
}

function Expect-Answer([string]$expr, [string]$pattern) {
    Show-QCalc | Out-Null
    Clear-Input
    $N::Type($expr, 30)
    $a = Read-Answer $pattern
    if ($a.Found) { @{ Pass = $true; Detail = "'$expr' shows '$($a.Text)' (via $($a.Source))" } }
    else { @{ Pass = $false; Detail = "'$expr' never showed /$pattern/; saw: $($a.Text)" } }
}

# ---------------------------------------------------------------------------

$screen = $N::Screen()
$env_ = [ordered]@{
    'exe' = $Exe
    'install' = $(if ($env:QCALC_INSTALL_MODE) { $env:QCALC_INSTALL_MODE } else { 'unknown' })
    'os' = [Environment]::OSVersion.VersionString
    'session' = "$($N::Session()) (console session $($N::ConsoleSession()))"
    'interactive' = [Environment]::UserInteractive
    'input desktop' = $N::InputDesktop()
    'screen' = "$($screen.Width)x$($screen.Height)"
    'foreground lock timeout' = "$($N::ForegroundLockTimeout()) ms"
    'foreground at start' = Describe-Foreground
}
$env_.GetEnumerator() | ForEach-Object { Log "$($_.Key): $($_.Value)" }

Get-Process -Name $ProcName -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 500
$env:QCALC_E2E = '1'
$script:proc = Start-Process -FilePath $Exe -PassThru
Log "started pid $($script:proc.Id)"

# the very first launch shows the window once so people know it's there
Invoke-Check '0' 'first launch shows once' $false {
    $shown = Wait-Until { Test-Shown } 30000
    @{ Pass = $shown; Detail = "shown on first launch: $shown" }
}

Get-Process -Name $ProcName -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 500
$script:proc = Start-Process -FilePath $Exe -PassThru
Log "relaunched pid $($script:proc.Id)"

Invoke-Check '1' 'later launches start hidden' $true {
    $exists = Wait-Until { (Get-QCalcWindow) -ne [IntPtr]::Zero } 30000
    # let the webview load and anything that would flash on screen do so
    Start-Sleep -Seconds 4
    $alive = -not $script:proc.HasExited
    $visible = @($N::Windows([int[]](Get-QCalcPids), '') | Where-Object { $N::IsWindowVisible($_) -and $N::Title($_) -like 'Q Calc*' } | ForEach-Object { "'$($N::Title($_))'" })
    @{ Pass = ($alive -and $exists -and $visible.Count -eq 0)
       Detail = "process alive: $alive; window created: $exists; visible windows: $(if ($visible) { $visible -join ', ' } else { 'none' })" }
}

Invoke-Check '1b' 'tray icon' $false {
    $find = {
        foreach ($cls in 'Shell_TrayWnd', 'NotifyIconOverflowWindow', 'TopLevelWindowForOverflowXamlIsland') {
            $h = $N::FindWindowW($cls, $null)
            if ($h -eq [IntPtr]::Zero) { continue }
            $hit = @(Get-UiaNames $h | Where-Object { $_ -like '*Q Calc*' })
            if ($hit) { return "'$($hit[0])' in $cls" }
        }
        return $null
    }
    $where = & $find
    if (-not $where) {
        # windows 11 style taskbars tuck new icons into the overflow flyout, which is empty until opened
        $tray = $N::FindWindowW('Shell_TrayWnd', $null)
        if ($tray -ne [IntPtr]::Zero) {
            $root = [System.Windows.Automation.AutomationElement]::FromHandle($tray)
            $cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, 'Show hidden icons', [System.Windows.Automation.PropertyConditionFlags]::IgnoreCase)
            $chevron = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $cond)
            if ($chevron) {
                $chevron.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
                Start-Sleep -Milliseconds 800
                $where = & $find
                $N::Chord($N::VK_ESCAPE)
            }
        }
    }
    @{ Pass = [bool]$where; Detail = $(if ($where) { "found $where" } else { 'no notification area item named Q Calc (best effort: shell layouts differ)' }) }
}

Invoke-Check '2' 'hotkey shows and focuses' $true {
    Hide-QCalc
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $k = Invoke-Hotkey
    if (-not $k) { return @{ Pass = $false; Detail = "neither Alt+Space nor Ctrl+Alt+Space showed the window; front: $(Describe-Foreground)" } }
    $front = Wait-Until { Test-Front } ([Math]::Max(200, 2000 - $sw.ElapsedMilliseconds))
    @{ Pass = $front; Detail = "$k showed it in $($sw.ElapsedMilliseconds) ms; foreground: $(if ($front) { 'yes' } else { 'no, ' + (Describe-Foreground) })" }
}

Invoke-Check '3' 'arithmetic answer' $true { Expect-Answer '2+3*4' '^14$' }

Invoke-Check '4a' 'unit conversion' $true { Expect-Answer '12 kg to lb' '^26\.4[56]\d*\s*(lb|lbs|pounds?)$' }

# the percent phrases arrive with the words branch
Invoke-Check '4b' 'percent phrase' $false { Expect-Answer '20% off 80' '^64$' }

Invoke-Check '5' 'esc hides' $true {
    Show-QCalc | Out-Null
    $N::Chord($N::VK_ESCAPE)
    $hidden = Wait-Until { -not (Test-Shown) } 1500
    @{ Pass = $hidden; Detail = $(if ($hidden) { 'hidden after Esc' } else { 'still visible 1.5 s after Esc' }) }
}

# the case windows' focus-stealing rules are about: another app is in front when the hotkey arrives
Invoke-Check '6' 'hotkey refocuses over another app' $true {
    Hide-QCalc
    Open-Other
    Click-Other
    $otherFront = Wait-Until { $N::GetForegroundWindow() -eq $script:otherHwnd } 1500
    if (-not $otherFront) { Log "other window did not take the foreground; front: $(Describe-Foreground)" }
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $k = Invoke-Hotkey
    if (-not $k) { return @{ Pass = $false; Detail = 'hotkey did not show the window the second time' } }
    $front = Wait-Until { Test-Front } ([Math]::Max(200, 2000 - $sw.ElapsedMilliseconds))
    @{ Pass = ($front -and $otherFront)
       Detail = "other app in front first: $otherFront; shown in $($sw.ElapsedMilliseconds) ms; foreground: $(if ($front) { 'yes' } else { 'no, ' + (Describe-Foreground) })" }
}

Invoke-Check '7' 'click elsewhere hides' $true {
    Show-QCalc | Out-Null
    Open-Other
    Click-Other
    $hidden = Wait-Until { -not (Test-Shown) } 1500
    @{ Pass = $hidden; Detail = "front after click: $(Describe-Foreground); hidden: $hidden" }
}

# single instance may not exist yet
Invoke-Check '8' 'second launch reuses the first' $false {
    Hide-QCalc
    $second = Start-Process -FilePath $Exe -PassThru
    Start-Sleep -Seconds 5
    $pids = Get-QCalcPids
    $shown = Test-Shown
    @{ Pass = ($pids.Count -eq 1 -and $shown)
       Detail = "processes: $($pids.Count) (second launch pid $($second.Id), exited: $($second.HasExited)); window shown: $shown" }
}
Get-Process -Name $ProcName -ErrorAction SilentlyContinue | Where-Object { $_.Id -ne $script:proc.Id } | Stop-Process -Force

Invoke-Check '9' 'window grows with content' $true {
    $h = Show-QCalc
    Clear-Input
    $N::Type('1', 30)
    Start-Sleep -Milliseconds 800
    $before = Height $h
    Clear-Input
    $N::Type('graph sin(x)', 30)
    $grew = Wait-Until { (Height $h) -gt $before } 4000
    @{ Pass = $grew; Detail = "height $before px before, $(Height $h) px after typing 'graph sin(x)'" }
}

# ---------------------------------------------------------------------------

try { Hide-QCalc } catch { }
Get-Process -Name $ProcName -ErrorAction SilentlyContinue | Stop-Process -Force
if ($script:other -and -not $script:other.HasExited) { Stop-Process -Id $script:other.Id -Force }

$failedRequired = @($script:results | Where-Object { $_.Required -and -not $_.Pass })
$md = New-Object System.Text.StringBuilder
[void]$md.AppendLine('## Q Calc windows smoke test')
[void]$md.AppendLine()
[void]$md.AppendLine($(if ($failedRequired) { "**$($failedRequired.Count) required check(s) failed.**" } else { '**All required checks passed.**' }))
[void]$md.AppendLine()
[void]$md.AppendLine('| # | check | kind | result | detail | screenshot |')
[void]$md.AppendLine('|---|---|---|---|---|---|')
foreach ($r in $script:results) {
    $kind = if ($r.Required) { 'required' } else { 'informational' }
    $res = if ($r.Pass) { 'pass' } else { 'FAIL' }
    $detail = ($r.Detail -replace '\|', '\|' -replace "`r?`n", ' ')
    [void]$md.AppendLine("| $($r.Id) | $($r.Name) | $kind | $res | $detail | $($r.Shot) |")
}
[void]$md.AppendLine()
[void]$md.AppendLine('<details><summary>runner</summary>')
[void]$md.AppendLine()
$env_.GetEnumerator() | ForEach-Object { [void]$md.AppendLine("- $($_.Key): $($_.Value)") }
[void]$md.AppendLine("- hotkey that worked: $(if ($script:hotkey) { $script:hotkey } else { 'none' })")
[void]$md.AppendLine()
[void]$md.AppendLine('</details>')

[IO.File]::WriteAllText((Join-Path $OutDir 'results.md'), $md.ToString())
if ($env:GITHUB_STEP_SUMMARY) { [IO.File]::AppendAllText($env:GITHUB_STEP_SUMMARY, $md.ToString()) }
Write-Host $md.ToString()
Stop-Transcript | Out-Null
if ($failedRequired) { exit 1 }
exit 0
