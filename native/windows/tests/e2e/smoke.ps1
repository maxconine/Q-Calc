# End-to-end smoke test for the native Windows app (qcalc-win). Real input through SendInput (Native.cs), state over
# the bench pipe (bench.rs: state, e2e, settings-controls, mem), paint checks from screenshots. The Tauri app's
# src-tauri/tests/e2e/smoke.ps1 ported, plus what the native app does that it didn't (single-instance hand-off,
# the held-hotkey retry, history and the draft across a restart, copy, graph / sys panels, a game, the installer,
# and the performance targets as soft checks).
#
# Needs an interactive desktop (GitHub's windows runners have one). Runs under Windows PowerShell 5.1 (C# 5 Add-Type,
# UI Automation). Everything it starts is its own: QCALC_INSTANCE (mutex, pipe, host window), QCALC_DATA (settings,
# history, draft), QCALC_LOG, a hotkey nobody else uses, and for the installer an app id of its own in a temp folder.
# It never touches the installed Q Calc, its data, its Run entry or its shortcut, and only stops processes it started.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File native\windows\tests\e2e\smoke.ps1 -Exe native\target\release\qcalc.exe
#
# Exit code 1 when a required check fails. Targets (memory, launch, keystroke) are soft: a warning, and a table in
# results.md / the job summary.
param(
    [string]$Exe = (Join-Path $PSScriptRoot '..\..\..\target\release\qcalc.exe'),
    [string]$OutDir = (Join-Path (Get-Location) 'e2e-out'),
    [string]$Instance = 'e2e',
    # Ctrl+Alt+Shift+F9: free on any desktop, so neither the installed app nor a launcher is in the way
    [string]$Chord = 'ctrl+alt+shift+f9',
    [switch]$SkipInstaller,
    [string]$Makensis,
    # an older Windows 10 build for compat.rs (QCALC_COMPAT_SIMULATE: 1507, 1607, 1709, ... or a build number): the
    # functions it lacks are reported missing, so the fallbacks run here. Every copy the suite starts inherits it
    [string]$Simulate,
    # only these checks (ids, e.g. B4,B5), for chasing one; the others are skipped and the suite doesn't judge
    [string]$Only
)
$ErrorActionPreference = 'Stop'
$Exe = (Resolve-Path $Exe).Path
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$OutDir = (Resolve-Path $OutDir).Path
Start-Transcript -Path (Join-Path $OutDir 'log.txt') -Force | Out-Null

Add-Type -TypeDefinition (Get-Content -Raw (Join-Path $PSScriptRoot 'Native.cs')) -ReferencedAssemblies System.Drawing, System.Core
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$N = [QCalcE2E.Native]
# keys only ever go to the app under test or the suite's own windows, never the person's app in front
$N::GuardExe = $Exe
$N::SetProcessDPIAware() | Out-Null

$Root = Join-Path ([IO.Path]::GetTempPath()) ("qcalc-e2e-{0}-{1}" -f $Instance, [Guid]::NewGuid().ToString('N').Substring(0, 8))
$Data = Join-Path $Root 'data'
$Log = Join-Path $Root 'qcalc.log'
New-Item -ItemType Directory -Force $Data | Out-Null
$env:QCALC_INSTANCE = $Instance
$env:QCALC_DATA = $Data
$env:QCALC_LOG = $Log
$env:QCALC_BENCH = '1'
Remove-Item Env:QCALC_E2E -ErrorAction SilentlyContinue
if ($Simulate) { $env:QCALC_COMPAT_SIMULATE = $Simulate } else { Remove-Item Env:QCALC_COMPAT_SIMULATE -ErrorAction SilentlyContinue }

$script:results = New-Object System.Collections.Generic.List[object]
$script:targets = New-Object System.Collections.Generic.List[object]
$script:proc = $null
$script:mine = New-Object System.Collections.Generic.List[int]
$script:other = $null
$script:otherHwnd = [IntPtr]::Zero
$script:clipBefore = $null

function Log([string]$msg) { Write-Host ("[{0:HH:mm:ss.fff}] {1}" -f (Get-Date), $msg) }

# the block sees the caller's variables through dynamic scope, so this uses names no caller does
function Wait-Until([scriptblock]$__until, [int]$__ms = 2000) {
    $__sw = [Diagnostics.Stopwatch]::StartNew()
    do {
        if (& $__until) { return $true }
        Start-Sleep -Milliseconds 50
    } while ($__sw.ElapsedMilliseconds -lt $__ms)
    return [bool](& $__until)
}

# ---- the app ----

# one request, one reply over \\.\pipe\qcalc-bench-<instance>; $null when nothing answers
function Send-Pipe([hashtable]$Request, [int]$TimeoutMs = 3000, [string]$Name = $Instance) {
    $pipe = New-Object System.IO.Pipes.NamedPipeClientStream('.', "qcalc-bench-$Name", [System.IO.Pipes.PipeDirection]::InOut)
    try {
        $pipe.Connect($TimeoutMs)
        $writer = New-Object System.IO.StreamWriter($pipe)
        $writer.AutoFlush = $true
        $writer.WriteLine(($Request | ConvertTo-Json -Compress -Depth 6))
        $reader = New-Object System.IO.StreamReader($pipe)
        $line = $reader.ReadLine()
        if ($line) { return $line | ConvertFrom-Json }
        return $null
    } catch {
        return $null
    } finally {
        $pipe.Dispose()
    }
}

function State { Send-Pipe @{ cmd = 'state' } }
function E2E { Send-Pipe @{ cmd = 'e2e' } }

function Write-Store([hashtable]$Extra = @{}, [string]$Dir = $Data) {
    $store = @{
        hotkey = $Chord
        firstRunDone = $true
        settings = @{ theme = 'light'; shareUsage = $false; draftSeconds = 60 }
        autoUpdate = $false
    }
    foreach ($k in $Extra.Keys) { $store[$k] = $Extra[$k] }
    New-Item -ItemType Directory -Force $Dir | Out-Null
    [IO.File]::WriteAllText((Join-Path $Dir 'settings.json'), ($store | ConvertTo-Json -Depth 6))
}

function Start-QCalc([string[]]$ArgList = @(), [string]$Path = $Exe) {
    $p = if ($ArgList.Count) { Start-Process -FilePath $Path -ArgumentList $ArgList -PassThru } else { Start-Process -FilePath $Path -PassThru }
    $script:mine.Add($p.Id)
    Log "started pid $($p.Id) $($ArgList -join ' ')"
    $p
}

# the pipe's quit (saves and leaves as the app does), else by PID; only processes this script started
function Stop-QCalc($p, [string]$Name = $Instance) {
    if (-not $p -or $p.HasExited) { return }
    Send-Pipe @{ cmd = 'quit' } 2000 $Name | Out-Null
    if (-not $p.WaitForExit(5000)) {
        Log "pid $($p.Id) didn't quit; stopping it"
        Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
        $p.WaitForExit(3000) | Out-Null
    }
}

function Stop-Mine {
    foreach ($id in $script:mine) { Stop-Process -Id $id -Force -ErrorAction SilentlyContinue }
}

# a fresh copy: launched, its pipe answering
function Restart-QCalc([string[]]$ArgList = @()) {
    Stop-QCalc $script:proc
    $script:proc = Start-QCalc $ArgList
    if (-not (Wait-Until { $null -ne (State) } 15000)) { throw "the app's pipe never answered (pid $($script:proc.Id))" }
}

function Get-Bar {
    if (-not $script:proc -or $script:proc.HasExited) { return [IntPtr]::Zero }
    $w = @($N::WindowsOfClass([int[]]@($script:proc.Id), 'QCalcBar'))
    if ($w) { return $w[0] }
    [IntPtr]::Zero
}

function Test-Shown { $h = Get-Bar; $h -ne [IntPtr]::Zero -and $N::IsWindowVisible($h) }
function Test-Front { $h = Get-Bar; $h -ne [IntPtr]::Zero -and $N::GetForegroundWindow() -eq $h }

function Describe-Foreground {
    $f = $N::GetForegroundWindow()
    if ($f -eq [IntPtr]::Zero) { return 'none' }
    $p = Get-Process -Id ($N::ProcessOf($f)) -ErrorAction SilentlyContinue
    "'{0}' class {1} ({2})" -f $N::Title($f), $N::ClassName($f), $(if ($p) { $p.ProcessName } else { '?' })
}

function Height($h) { $r = $N::Rect($h); $r.Bottom - $r.Top }

function Send-Hotkey { $N::Chord($N::VK_CONTROL, $N::VK_MENU, $N::VK_SHIFT, $N::VK_F9) }

function Click-Input {
    $e = E2E
    if (-not $e) { return }
    $f = $e.field
    $N::Click([int]($f[0] + [Math]::Min(60, $f[2] / 2)), [int]($f[1] + $f[3] / 2))
}

function Show-QCalc {
    if (-not (Test-Shown)) {
        Send-Hotkey
        if (-not (Wait-Until { Test-Shown } 3000)) { throw "the hotkey didn't show the bar (front: $(Describe-Foreground))" }
    }
    if (-not (Wait-Until { Test-Front } 1500)) {
        Log "shown but not in front (front: $(Describe-Foreground)); clicking the input"
        Click-Input
        if (-not (Wait-Until { Test-Front } 1500)) { throw "the bar is shown but never came to the front; front is $(Describe-Foreground)" }
    }
    Start-Sleep -Milliseconds 200
    Get-Bar
}

function Hide-QCalc {
    if (Test-Shown) {
        if (Test-Front) { $N::Chord($N::VK_ESCAPE) }
        if (-not (Wait-Until { -not (Test-Shown) } 1500)) {
            Send-Pipe @{ cmd = 'hide' } | Out-Null
            Wait-Until { -not (Test-Shown) } 1500 | Out-Null
        }
    }
}

function Clear-Input {
    $N::Chord($N::VK_CONTROL, $N::VK_A)
    $N::Chord($N::VK_BACK)
    Wait-Until { $s = State; $s -and $s.text -eq '' } 1500 | Out-Null
}

function Normalize([string]$s) {
    (($s -replace '[\u2009\u202F\u00A0\u200B,]', '') -replace '\u2212', '-').Trim()
}

# what the bar answers, over the pipe, once it matches (or the time's up)
function Read-Answer([string]$pattern, [int]$ms = 6000) {
    $script:seen = ''
    $ok = Wait-Until {
        $s = State
        if ($s -and $s.liveFor -eq $s.text) { $script:seen = Normalize $s.answer }
        $script:seen -match $pattern
    } $ms
    @{ Found = $ok; Text = $script:seen }
}

# real keystrokes into the bar; typed again (once) if they didn't all land there (focus stolen mid-word on a shared
# desktop). The line can differ from the keys by the typing rules (pi -> π), so only "nothing landed" retries
function Type-Text([string]$text) {
    $N::Type($text, 25)
    if (Wait-Until { $s = State; $s -and $s.text.Length -gt 0 } 1000) { return }
    Log "typing '$text' left the line empty (front: $(Describe-Foreground)); again"
    Show-QCalc | Out-Null
    $N::Type($text, 25)
}

# the answer slot's pixels: painted ink there, not a blank bar
function Get-AnswerInk {
    $e = E2E
    $a = $e.answer
    if (-not $a -or $a[2] -le 0) { $c = $e.composer; $a = @($c[0] + $c[2] / 2, $c[1], $c[2] / 2, $c[3]) }
    $st = $N::Stats([int]$a[0], [int]$a[1], [int]$a[2], [int]$a[3])
    [pscustomobject]@{ Colors = $st[0]; Luma = [Math]::Round($st[1]); Ink = [Math]::Round($st[2], 4) }
}

function Expect-Answer([string]$expr, [string]$pattern) {
    Show-QCalc | Out-Null
    Clear-Input
    Type-Text $expr
    $a = Read-Answer $pattern
    if (-not $a.Found) { return @{ Pass = $false; Detail = "'$expr' never showed /$pattern/; saw '$($a.Text)'" } }
    Start-Sleep -Milliseconds 150
    $ink = Get-AnswerInk
    @{ Pass = ($ink.Ink -gt 0.004); Detail = "'$expr' shows '$($a.Text)'; answer slot painted: $($ink.Ink * 100)% ink, $($ink.Colors) colours" }
}

function Open-Other {
    if ($script:other -and -not $script:other.HasExited -and $script:otherHwnd -ne [IntPtr]::Zero) { return }
    $s = $N::Screen()
    $y = [Math]::Max(0, $s.Height - 300)
    $script:other = Start-Process powershell -PassThru -ArgumentList @(
        '-NoProfile', '-ExecutionPolicy', 'Bypass', '-WindowStyle', 'Hidden',
        '-File', (Join-Path $PSScriptRoot 'other-window.ps1'), '-X', '20', '-Y', "$y")
    $found = Wait-Until { @($N::Windows([int[]]@($script:other.Id), 'qcalc-e2e-other')).Count -gt 0 } 15000
    if (-not $found) { throw 'the other window never appeared' }
    $script:otherHwnd = @($N::Windows([int[]]@($script:other.Id), 'qcalc-e2e-other'))[0]
}

# a real click on another app's window; a fresh window can miss the first click, so it clicks again until in front
function Click-Other {
    for ($i = 0; $i -lt 4; $i++) {
        $r = $N::Rect($script:otherHwnd)
        $x = [int](($r.Left + $r.Right) / 2); $y = [int](($r.Top + $r.Bottom) / 2) + 20 * $i
        $q = Get-Bar
        if ($q -ne [IntPtr]::Zero -and $N::IsWindowVisible($q)) {
            $qr = $N::Rect($q)
            if ($x -ge $qr.Left -and $x -le $qr.Right -and $y -ge $qr.Top -and $y -le $qr.Bottom) { $y = $r.Bottom - 20 }
        }
        $N::Click($x, $y)
        if (Wait-Until { $N::GetForegroundWindow() -eq $script:otherHwnd } 600) { return $true }
        Log "click $($i + 1) on the other window left $(Describe-Foreground) in front"
    }
    return $false
}

function Describe-App {
    $h = Get-Bar
    $win = if ($h -eq [IntPtr]::Zero) { 'no bar' } else { $r = $N::Rect($h); "bar $($r.Left),$($r.Top) to $($r.Right),$($r.Bottom), visible $($N::IsWindowVisible($h))" }
    $alive = $script:proc -and -not $script:proc.HasExited
    $tail = if (Test-Path $Log) { (Get-Content $Log -Tail 6) -join ' / ' } else { 'no log' }
    "alive: $alive; $win; front: $(Describe-Foreground); log: $tail" -replace "`r?`n", ' ' -replace '::', ': :'
}

# kind: required, informational, or target (a soft check against AGENTS.md's targets)
function Invoke-Check([string]$id, [string]$name, [string]$kind, [scriptblock]$body) {
    if ($Only -and ($Only -split '[,\s]+') -notcontains $id) { return }
    Log "--- $id $name"
    $pass = $false; $detail = ''
    # a required check gets a second go: the desktop is shared (CI's isn't, a dev machine's is), and a window or
    # keystroke from elsewhere can land mid-check; the first attempt's failure stays in the detail
    $tries = if ($kind -eq 'required') { 2 } else { 1 }
    for ($t = 1; $t -le $tries -and -not $pass; $t++) {
        $first = $detail
        try {
            $r = @(& $body)[-1]
            $pass = [bool]$r.Pass; $detail = [string]$r.Detail
        } catch {
            $detail = "error: $($_.Exception.Message)"
        }
        if ($t -gt 1) { $detail = "$detail (second attempt; the first: $first)" }
        if (-not $pass -and $t -lt $tries) { Log "FIRST ATTEMPT FAILED $($id): $detail; trying again"; Start-Sleep -Milliseconds 800 }
    }
    $shot = "$id-$(($name -replace '[^A-Za-z0-9]+', '-').Trim('-').ToLower()).png"
    try { $N::Screenshot((Join-Path $OutDir $shot)) } catch { $shot = "(screenshot failed: $($_.Exception.Message))" }
    $script:results.Add([pscustomobject]@{ Id = $id; Name = $name; Kind = $kind; Pass = $pass; Detail = $detail; Shot = $shot })
    Log ("{0} {1}: {2}" -f $(if ($pass) { 'PASS' } else { 'FAIL' }), $id, $detail)
    if (-not $pass) {
        $level = if ($kind -eq 'required') { 'error' } else { 'warning' }
        Write-Host ("::{0} title=check {1} {2}::{3}" -f $level, $id, $name, ($detail -replace "`r?`n", ' '))
        if ($kind -eq 'required') { Write-Host ("::notice title=check {0}: what the app saw::{1}" -f $id, (Describe-App)) }
    }
}

function Target([string]$what, [double]$value, [double]$limit, [string]$unit) {
    $script:targets.Add([pscustomobject]@{ What = $what; Value = [Math]::Round($value, 3); Limit = $limit; Unit = $unit; Met = ($value -lt $limit) })
    @{ Pass = ($value -lt $limit); Detail = "$what $([Math]::Round($value, 3)) $unit (target < $limit $unit)" }
}

function Get-Clip { try { Get-Clipboard -Raw -ErrorAction Stop } catch { $null } }

# ---------------------------------------------------------------------------

$screen = $N::Screen()
$env_ = [ordered]@{
    'exe' = $Exe
    'instance' = $Instance
    'data' = $Data
    'os' = [Environment]::OSVersion.VersionString
    'session' = "$($N::Session()) (console session $($N::ConsoleSession()))"
    'input desktop' = $N::InputDesktop()
    'screen' = "$($screen.Width)x$($screen.Height)"
    'apps use light theme' = $(try { (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize' -ErrorAction Stop).AppsUseLightTheme } catch { 'unset' })
    'foreground lock timeout' = "$($N::ForegroundLockTimeout()) ms"
    'foreground at start' = Describe-Foreground
    'simulated Windows build' = $(if ($Simulate) { $Simulate } else { 'none' })
}
$env_.GetEnumerator() | ForEach-Object { Log "$($_.Key): $($_.Value)" }
$script:clipBefore = Get-Clip

try {

# ---- launch ----

# a fresh install's first launch shows the bar once, even from sign-in, so people know it's there
Write-Store @{ firstRunDone = $false }
$script:proc = Start-QCalc @('--autostart')
Invoke-Check 'L1' 'first launch shows the bar' 'required' {
    $shown = Wait-Until { Test-Shown } 15000
    @{ Pass = $shown; Detail = "shown on the first launch (even with --autostart): $shown" }
}

if ($Simulate) {
    Invoke-Check 'O1' "runs as Windows 10 $Simulate would (compat.rs fallbacks)" 'required' {
        $script:compatLine = $null
        Wait-Until { $script:compatLine = Select-String -Path $Log -Pattern 'compat: build \d+ \(simulated\).*' -ErrorAction SilentlyContinue | Select-Object -First 1; [bool]$script:compatLine } 5000 | Out-Null
        @{ Pass = [bool]$script:compatLine; Detail = $(if ($script:compatLine) { $script:compatLine.Matches[0].Value } else { 'no compat line in the log' }) }
    }
}


Invoke-Check 'L2' 'opening it with nothing running shows the bar' 'required' {
    Restart-QCalc
    $shown = Wait-Until { Test-Shown } 10000
    $front = Wait-Until { Test-Front } 2000
    @{ Pass = ($shown -and $front); Detail = "shown: $shown; in front: $front ($(Describe-Foreground))" }
}

Invoke-Check 'L3' 'the launch at sign in (--autostart) starts hidden' 'required' {
    Restart-QCalc @('--autostart')
    Start-Sleep -Seconds 1
    $s = State
    $bar = Get-Bar
    $visible = @($N::WindowsOfClass([int[]]@($script:proc.Id), 'QCalcBar') + $N::WindowsOfClass([int[]]@($script:proc.Id), 'QCalcSettings') | Where-Object { $N::IsWindowVisible($_) })
    @{ Pass = (-not $script:proc.HasExited -and $bar -ne [IntPtr]::Zero -and $visible.Count -eq 0 -and -not $s.visible)
       Detail = "alive: $(-not $script:proc.HasExited); bar created: $($bar -ne [IntPtr]::Zero); visible windows: $($visible.Count); pipe visible: $($s.visible)" }
}

# the notification area button named Q Calc; other copies may have one too, so this only reports
function Find-TrayButton {
    $find = {
        foreach ($cls in 'Shell_TrayWnd', 'NotifyIconOverflowWindow', 'TopLevelWindowForOverflowXamlIsland') {
            $w = [System.Windows.Automation.AutomationElement]::RootElement.FindFirst([System.Windows.Automation.TreeScope]::Children,
                (New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ClassNameProperty, $cls)))
            if (-not $w) { continue }
            $buttons = $w.FindAll([System.Windows.Automation.TreeScope]::Descendants,
                (New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button)))
            foreach ($b in $buttons) {
                try { $name = $b.Current.Name } catch { continue }
                if ($name -like '*Q Calc*') { return $b }
            }
        }
        return $null
    }
    $button = & $find
    if ($button) { return $button }
    # Windows 11 tucks new icons into the overflow flyout, which is empty until opened
    $tray = $N::FindWindowW('Shell_TrayWnd', $null)
    if ($tray -eq [IntPtr]::Zero) { return $null }
    $cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, 'Show hidden icons', [System.Windows.Automation.PropertyConditionFlags]::IgnoreCase)
    $chevron = [System.Windows.Automation.AutomationElement]::FromHandle($tray).FindFirst([System.Windows.Automation.TreeScope]::Descendants, $cond)
    if (-not $chevron) { return $null }
    $chevron.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    $script:trayButton = $null
    Wait-Until { $script:trayButton = & $find; [bool]$script:trayButton } 2000 | Out-Null
    $N::Chord($N::VK_ESCAPE)
    $script:trayButton
}

# a notification area item named Q Calc; other copies may have one too, so this only reports
Invoke-Check 'L4' 'tray icon' 'informational' {
    $b = Find-TrayButton
    @{ Pass = [bool]$b; Detail = $(if ($b) { "found '$($b.Current.Name)'" } else { 'no notification area item named Q Calc (best effort: shell layouts differ)' }) }
}

# ---- the hotkey ----

Invoke-Check 'H1' 'hotkey shows and focuses' 'required' {
    Hide-QCalc
    $hk = (E2E).hotkey
    $sw = [Diagnostics.Stopwatch]::StartNew()
    Send-Hotkey
    $shown = Wait-Until { Test-Shown } 3000
    $front = Wait-Until { Test-Front } 2000
    @{ Pass = ($shown -and $front); Detail = "registered: $($hk.active) (wanted $($hk.wanted)); shown in $($sw.ElapsedMilliseconds) ms: $shown; in front: $front ($(Describe-Foreground))" }
}

Invoke-Check 'H2' 'hotkey again hides (toggle)' 'required' {
    Show-QCalc | Out-Null
    Send-Hotkey
    $hidden = Wait-Until { -not (Test-Shown) } 2000
    @{ Pass = $hidden; Detail = "hidden by the second press: $hidden" }
}

# the case Windows' focus rules are about: another app is in front when the hotkey arrives
Invoke-Check 'H3' 'hotkey refocuses over another app' 'required' {
    Hide-QCalc
    Open-Other
    $otherFront = Click-Other
    Send-Hotkey
    $shown = Wait-Until { Test-Shown } 3000
    $front = Wait-Until { Test-Front } 2000
    @{ Pass = ($otherFront -and $shown -and $front); Detail = "other app in front first: $otherFront; shown: $shown; in front: $front ($(Describe-Foreground))" }
}

# ---- hiding ----

Invoke-Check 'E1' 'esc hides' 'required' {
    Show-QCalc | Out-Null
    $N::Chord($N::VK_ESCAPE)
    $hidden = Wait-Until { -not (Test-Shown) } 1500
    @{ Pass = $hidden; Detail = $(if ($hidden) { 'hidden after Esc' } else { 'still visible 1.5 s after Esc' }) }
}

Invoke-Check 'E2' 'click elsewhere hides' 'required' {
    Show-QCalc | Out-Null
    Open-Other
    Click-Other | Out-Null
    $hidden = Wait-Until { -not (Test-Shown) } 1500
    @{ Pass = $hidden; Detail = "front after the click: $(Describe-Foreground); hidden: $hidden" }
}

# focus taken without a click: Alt+Tab to the other window
Invoke-Check 'E3' 'focus loss hides' 'required' {
    Open-Other
    Show-QCalc | Out-Null
    $N::Chord($N::VK_MENU, $N::VK_TAB)
    $hidden = Wait-Until { -not (Test-Shown) } 2000
    @{ Pass = $hidden; Detail = "front after Alt+Tab: $(Describe-Foreground); hidden: $hidden" }
}

# ---- single instance ----

Invoke-Check 'S1' 'a second launch hands off to the first' 'required' {
    Hide-QCalc
    $second = Start-QCalc
    $exited = $second.WaitForExit(5000)
    $shown = Wait-Until { Test-Shown } 2000
    $alive = @($script:mine | Where-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue })
    @{ Pass = ($exited -and $shown -and -not $script:proc.HasExited)
       Detail = "second launch exited: $exited (code $(if ($exited) { $second.ExitCode } else { '-' })); first copy showed the bar: $shown; my processes alive: $($alive.Count)" }
}

Invoke-Check 'S2' 'a second launch with --autostart stays quiet' 'required' {
    Hide-QCalc
    $second = Start-QCalc @('--autostart')
    $exited = $second.WaitForExit(5000)
    Start-Sleep -Milliseconds 500
    @{ Pass = ($exited -and -not (Test-Shown)); Detail = "second launch exited: $exited; bar stayed hidden: $(-not (Test-Shown))" }
}

# ---- typing ----

Invoke-Check 'T1' 'arithmetic answer' 'required' { Expect-Answer '2+3*4' '^14$' }
Invoke-Check 'T2' 'unit conversion' 'required' { Expect-Answer '12 kg to lb' '^26\.4[56]\d*\s*(lb|lbs|pounds?)$' }
Invoke-Check 'T3' 'percent phrase' 'informational' { Expect-Answer '20% off 80' '^64$' }
Invoke-Check 'T4' 'solve' 'informational' { Expect-Answer '3x+2=11' '^x\s*=\s*3$' }

Invoke-Check 'T5' 'enter commits to the tape' 'required' {
    Show-QCalc | Out-Null
    Clear-Input
    $before = @((State).rows).Count
    Type-Text '19*3'
    Read-Answer '^57$' | Out-Null
    $N::Chord($N::VK_RETURN)
    $ok = Wait-Until { $s = State; @($s.rows).Count -gt $before } 3000
    $s = State
    $last = @($s.rows)[-1]
    @{ Pass = ($ok -and $last.expr -match '19' -and (Normalize $last.display) -eq '57'); Detail = "rows $before -> $(@($s.rows).Count); last '$($last.expr)' = '$($last.display)'; line now '$($s.text)'" }
}

Invoke-Check 'C1' 'ctrl+c copies the answer' 'required' {
    Show-QCalc | Out-Null
    Clear-Input
    Set-Clipboard -Value 'e2e-before' -ErrorAction SilentlyContinue
    Type-Text '6*7'
    Read-Answer '^42$' | Out-Null
    $N::Chord($N::VK_CONTROL, $N::VK_C)
    $ok = Wait-Until { (Normalize (Get-Clip)) -eq '42' } 2000
    @{ Pass = $ok; Detail = "clipboard after Ctrl+C: '$(Get-Clip)'" }
}

# the pointer over the answer (hover.rs): the hand, and after a rest the system tooltip saying what a click does
Invoke-Check 'U1' 'the answer shows the hand and a tooltip under the pointer' 'required' {
    Show-QCalc | Out-Null
    Clear-Input
    Type-Text '6*7'
    Read-Answer '^42$' | Out-Null
    $h = Get-Bar
    $r = $N::Rect($h)
    $s = $N::GetDpiForWindow($h) / 96.0
    # the answer sits at the composer's right (16 in), the composer the bar's bottom 72 DIPs
    $N::MoveTo([int]($r.Right - 30 * $s), [int]($r.Bottom - 36 * $s))
    # with no cursor on screen (a CI runner has no mouse) the system can't show the hand: the bar's own choice
    # in WM_SETCURSOR is what's checked then
    $showing = $N::CursorShowing()
    $hand = if ($showing) { Wait-Until { $N::CursorIsHand() } 1500 } else { Wait-Until { (E2E).cursor -eq 'hand' } 1500 }
    $how = if ($showing) { 'on screen' } else { "no cursor on screen, the bar set '$((E2E).cursor)'" }
    $tip = Wait-Until { @($N::WindowsOfClass([int[]]@($script:proc.Id), 'tooltips_class32') | Where-Object { $N::IsWindowVisible($_) }).Count -gt 0 } 2500
    $N::MoveTo([int]($r.Left + 40 * $s), [int]($r.Bottom - 36 * $s))
    $gone = Wait-Until { @($N::WindowsOfClass([int[]]@($script:proc.Id), 'tooltips_class32') | Where-Object { $N::IsWindowVisible($_) }).Count -eq 0 } 1500
    @{ Pass = ($hand -and $tip -and $gone); Detail = "hand over the answer: $hand ($how); tooltip after a rest: $tip; gone when the pointer left: $gone" }
}

# the draft: an unfinished line outlives a hide and a restart (draftSeconds 60)
Invoke-Check 'T6' 'history and the draft survive a restart' 'required' {
    Show-QCalc | Out-Null
    Clear-Input
    Type-Text '41+1'
    Read-Answer '^42$' | Out-Null
    $N::Chord($N::VK_RETURN)
    Wait-Until { (State).text -ne '41+1' } 2000 | Out-Null
    Clear-Input
    Type-Text '100/8'
    Read-Answer '^12\.5$' | Out-Null
    Hide-QCalc
    Start-Sleep -Milliseconds 300
    $rows = @((State).rows)
    Restart-QCalc @('--autostart')
    Wait-Until { @((State).rows).Count -ge $rows.Count } 5000 | Out-Null
    $after = @((State).rows)
    $kept = @($after | Where-Object { $_.expr -match '41' }).Count -gt 0
    Show-QCalc | Out-Null
    $draft = Wait-Until { (State).text -eq '100/8' } 2000
    @{ Pass = ($kept -and $after.Count -eq $rows.Count -and $draft); Detail = "rows before $($rows.Count), after the restart $($after.Count), 41+1 kept: $kept; draft back: $draft (line '$((State).text)')" }
}

# ---- panels and the window ----

Invoke-Check 'P1' 'graph panel opens and the bar grows' 'required' {
    $h = Show-QCalc
    Clear-Input
    Start-Sleep -Milliseconds 200
    $before = Height $h
    Type-Text 'graph sin(x)'
    $grew = Wait-Until { (Height $h) -gt $before + 40 } 4000
    $ok = Wait-Until { $s = State; $s.graph -and $s.graph.points -gt 0 } 4000
    $g = (State).graph
    @{ Pass = ($grew -and $ok); Detail = "height $before -> $(Height $h) px; graph '$($g.expression)' with $($g.points) points" }
}

Invoke-Check 'P2' 'sys panel opens' 'required' {
    $h = Show-QCalc
    Clear-Input
    Start-Sleep -Milliseconds 200
    $before = Height $h
    # sys2 opens two rows as soon as the 2 is typed, the first focused (native only); Enter moves to the next,
    # Tab and Shift+Tab go round them
    Type-Text 'sys2'
    $ok = Wait-Until { $s = State; $null -ne $s.systems -and $s.systems.focus -eq 0 } 4000
    $grew = Wait-Until { (Height $h) -gt $before + 20 } 3000
    $open = Height $h
    $N::Chord($N::VK_TAB)
    $tab = Wait-Until { (State).systems.focus -eq 1 } 1500
    $N::Chord($N::VK_TAB)
    $round = Wait-Until { (State).systems.focus -eq 0 } 1500
    Type-Text 'x+y=3'
    $N::Chord($N::VK_RETURN)
    Start-Sleep -Milliseconds 150
    Type-Text 'x-y=1'
    $a = Read-Answer '^x\s*=\s*2.*y\s*=\s*1$' 4000
    $N::Chord($N::VK_ESCAPE)
    Wait-Until { $null -eq (State).systems } 1500 | Out-Null
    @{ Pass = ($ok -and $grew -and $tab -and $round -and $a.Found); Detail = "height $before -> $open px; open on typing the count, first row focused: $ok; Tab to the second: $tab, round to the first: $round; x+y=3, x-y=1 solved: '$($a.Text)'" }
}

# Tab turns the grey parens into typed ones where they stand, the caret kept by the same character (native only)
Invoke-Check 'B1' 'tab types in the grey parens' 'required' {
    Show-QCalc | Out-Null
    Clear-Input
    Type-Text '5+3)*2'
    Read-Answer '^16$' | Out-Null
    # the caret after the 3: three to the left of the end
    foreach ($i in 1..3) { $N::Chord([uint16]0x25) }
    Wait-Until { (State).caret -eq 3 } 1500 | Out-Null
    $N::Chord($N::VK_TAB)
    $mid = Wait-Until { $s = State; $s.text -eq '(5+3)*2' -and $s.caret -eq 4 } 2000
    $s1 = State
    Clear-Input
    Type-Text '(2+3'
    Read-Answer '^5$' | Out-Null
    $N::Chord($N::VK_TAB)
    $end = Wait-Until { $s = State; $s.text -eq '(2+3)' -and $s.caret -eq 5 } 2000
    $s2 = State
    @{ Pass = ($mid -and $end); Detail = "mid-line: '$($s1.text)' caret $($s1.caret) (want '(5+3)*2' at 4); at the end: '$($s2.text)' caret $($s2.caret) (want '(2+3)' at 5)" }
}

# Ctrl+Shift+E cycles the answer's forms (Tab on the page); Tab no longer does, and Ctrl+E is the exponent form (B3)
Invoke-Check 'B2' 'ctrl+shift+e cycles answer forms' 'required' {
    Show-QCalc | Out-Null
    Clear-Input
    Type-Text '0.75'
    $a = Read-Answer '^0\.75$'
    $N::Chord($N::VK_TAB)
    Start-Sleep -Milliseconds 300
    $tabbed = Normalize (State).shown
    $N::Chord($N::VK_CONTROL, $N::VK_SHIFT, [uint16]0x45)
    $changed = Wait-Until { $x = Normalize (State).shown; $x -ne '' -and $x -ne '0.75' } 2500
    $form = Normalize (State).shown
    $mode = (Send-Pipe @{ cmd = 'exponent' }).model
    @{ Pass = ($a.Found -and $tabbed -eq '0.75' -and $changed -and -not $mode); Detail = "0.75 answers '$($a.Text)'; after Tab '$tabbed' (unchanged); after Ctrl+Shift+E '$form'; exponent form left off: $(-not $mode)" }
}

# Ctrl+E: a lasting exponent-form mode (3,000 -> 3 x 10^3), saying how to turn it off, remembered across a restart
# (exponent.rs); Ctrl+E again turns it off
$script:expo = '^3\s*' + [char]0x00D7 + '\s*10' + [char]0x00B3 + '$'
Invoke-Check 'B3' 'ctrl+e turns on exponent form, remembered' 'required' {
    Show-QCalc | Out-Null
    Clear-Input
    Type-Text '3000'
    $a = Read-Answer '^3000$'
    $N::Chord($N::VK_CONTROL, [uint16]0x45)
    $on = Wait-Until { $x = Send-Pipe @{ cmd = 'exponent' }; $x.model -and (Normalize $x.shown) -match $script:expo } 2500
    $e = Send-Pipe @{ cmd = 'exponent' }
    $hint = [string]$e.hint
    $saidOff = $hint -match 'exponent form on .* Ctrl\+E turns it off'
    Hide-QCalc
    Start-Sleep -Milliseconds 300
    Restart-QCalc @('--autostart')
    Show-QCalc | Out-Null
    Clear-Input
    Type-Text '3000'
    $kept = Wait-Until { $x = Send-Pipe @{ cmd = 'exponent' }; $x.model -and (Normalize $x.shown) -match $script:expo } 4000
    $after = Normalize (Send-Pipe @{ cmd = 'exponent' }).shown
    $N::Chord($N::VK_CONTROL, [uint16]0x45)
    $off = Wait-Until { $x = Send-Pipe @{ cmd = 'exponent' }; -not $x.model -and (Normalize $x.shown) -eq '3000' -and -not $x.hint } 2500
    @{ Pass = ($a.Found -and $on -and $saidOff -and $kept -and $off); Detail = "3000 answers '$($a.Text)'; Ctrl+E: exponent form '$(Normalize $e.shown)': $on, hint '$hint'; after a restart still on: $kept ('$after'); Ctrl+E again off: $off" }
}

# a chained line's faint `ans` (`*2` continues from the last answer): Tab types it in where it's drawn (line.rs)
Invoke-Check 'B4' 'tab types in the faint ans' 'required' {
    Show-QCalc | Out-Null
    Clear-Input
    Type-Text '6'
    Read-Answer '^6$' | Out-Null
    $N::Chord($N::VK_RETURN)
    Wait-Until { (State).text -ne '6' } 2000 | Out-Null
    Clear-Input
    Type-Text '*2'
    $a = Read-Answer '^12$'
    $faint = Wait-Until { $null -ne (Send-Pipe @{ cmd = 'wordTip' }).ans } 1500
    $N::Chord($N::VK_TAB)
    $typed = Wait-Until { $s = State; $s.text -eq 'ans*2' -and $s.caret -eq 5 } 2000
    $b = Read-Answer '^12$'
    $gone = $null -eq (Send-Pipe @{ cmd = 'wordTip' }).ans
    $s = State
    @{ Pass = ($a.Found -and $faint -and $typed -and $b.Found -and $gone); Detail = "*2 after 6 answers '$($a.Text)', faint ans drawn: $faint; Tab: '$($s.text)' caret $($s.caret) (want 'ans*2' at 5), answer '$($b.Text)', faint ans gone: $gone" }
}

# the pointer over a word of the line names it over the word (word_tip.rs): a unit, `ans` with its value, the faint ans
function Hover-Word([string]$name, [int]$ms = 3000) {
    $script:hw = $null
    $found = Wait-Until {
        $d = Send-Pipe @{ cmd = 'wordTip' }
        $w = @($d.words | Where-Object { $_.name -eq $name })
        if ($w.Count -gt 0) { $script:hw = $w[0].at }
        $null -ne $script:hw
    } $ms
    if (-not $found) { return $false }
    $N::MoveTo([int](($script:hw[0] + $script:hw[1]) / 2), [int]$script:hw[2])
    Wait-Until { $d = Send-Pipe @{ cmd = 'wordTip' }; $d.shown -eq $name -and $d.tipUp } $ms
}
function Tip-Visible { @($N::WindowsOfClass([int[]]@($script:proc.Id), 'tooltips_class32') | Where-Object { $N::IsWindowVisible($_) }).Count -gt 0 }
Invoke-Check 'B5' 'hovering a word in the line names it' 'required' {
    Show-QCalc | Out-Null
    Clear-Input
    Type-Text '5 kPa'
    Read-Answer 'kPa' | Out-Null
    $f = (E2E).field
    # onto the line, so its names are asked for
    $N::MoveTo([int]($f[0] + 4), [int]($f[1] + $f[3] / 2))
    $unit = Hover-Word 'kilopascal'
    $unitTip = Tip-Visible
    Clear-Input
    Type-Text 'ans+1'
    Read-Answer '^7$' | Out-Null
    $N::MoveTo([int]($f[0] + $f[2] - 4), [int]($f[1] + $f[3] / 2))
    $ans = Hover-Word 'last answer = 6'
    $ansWhy = if ($ans) { '' } else { " (state $((Send-Pipe @{ cmd = 'wordTip' }) | ConvertTo-Json -Compress -Depth 4); line '$((State).text)'; log $((Get-Content $Log -Tail 10) -join ' | '))" }
    Clear-Input
    Type-Text '*3'
    Read-Answer '^18$' | Out-Null
    # a few px from where the last step left the pointer: a move to the same spot sends no WM_MOUSEMOVE
    $N::MoveTo([int]($f[0] + $f[2] - 10), [int]($f[1] + $f[3] / 2))
    $faint = $false
    $at = $null
    if (Wait-Until { $d = Send-Pipe @{ cmd = 'wordTip' }; $script:fa = $d.ans; $null -ne $d.ans } 2000) {
        $N::MoveTo([int](($script:fa[0] + $script:fa[1]) / 2), [int]$script:fa[2])
        $faint = Wait-Until { $d = Send-Pipe @{ cmd = 'wordTip' }; $script:fd = $d; $d.shown -eq 'last answer = 6' -and $d.tipUp } 3000
    }
    $faintWhy = if ($faint) { '' } else { " (faint ans at $($script:fa -join ','); state $(($script:fd | ConvertTo-Json -Compress -Depth 4)); log $((Get-Content $Log -Tail 10) -join ' | '))" }
    $r = $N::Rect((Get-Bar))
    $N::MoveTo($r.Left + 20, [Math]::Max(0, $r.Top - 120))
    $gone = Wait-Until { $d = Send-Pipe @{ cmd = 'wordTip' }; -not $d.shown -and -not (Tip-Visible) } 2000
    @{ Pass = ($unit -and $unitTip -and $ans -and $faint -and $gone); Detail = "kPa named kilopascal: $unit (tooltip up: $unitTip); typed ans named 'last answer = 6': $ans$ansWhy; faint ans named: $faint$faintWhy; gone when the pointer left: $gone" }
}

function Near([int]$a, [int]$b, [int]$within = 6) { [Math]::Abs($a - $b) -le $within }

# the composer's background drags the bar (HTCAPTION); the spot is remembered for the next showing
Invoke-Check 'W1' 'the bar drags and the spot is kept' 'required' {
    $h = Show-QCalc
    Clear-Input
    Start-Sleep -Milliseconds 300
    $c = (E2E).composer
    $r = $N::Rect($h)
    $x = [int]($c[0] + $c[2] / 2); $y = [int]$c[1] + 4
    $s = $N::Screen()
    $dx = if ($r.Right + 230 -le $s.Width) { 200 } else { -[Math]::Min(200, $r.Left - 10) }
    $dy = if ($r.Bottom + 150 -le $s.Height) { 120 } else { -[Math]::Min(120, $r.Top - 10) }
    $N::Drag($x, $y, $x + $dx, $y + $dy)
    $moved = Wait-Until { $m = $N::Rect($h); (Near $m.Left ($r.Left + $dx)) -and (Near $m.Top ($r.Top + $dy)) } 2000
    $m = $N::Rect($h)
    $stayed = Test-Shown
    Hide-QCalc
    $h = Show-QCalc
    $again = $N::Rect($h)
    $kept = (Near $again.Left $m.Left) -and (Near $again.Top $m.Top)
    @{ Pass = ($moved -and $stayed -and $kept); Detail = "dragged $dx, $dy px: moved $($m.Left - $r.Left), $($m.Top - $r.Top) px; still shown: $stayed; same spot after hide and show: $kept" }
}

Invoke-Check 'W2' 'the left edge widens the bar' 'required' {
    $h = Show-QCalc
    $c = (E2E).composer
    $r = $N::Rect($h)
    # W1 can leave the bar at the screen's left (a 1024 px runner): move it right to make room to widen into
    if ($r.Left -lt 160) {
        $s = $N::Screen()
        $dx = [Math]::Min(170 - $r.Left, $s.Width - $r.Right)
        $x = [int]($c[0] + $c[2] / 2); $cy = [int]$c[1] + 4
        $N::Drag($x, $cy, $x + $dx, $cy)
        Wait-Until { Near ($N::Rect($h).Left) ($r.Left + $dx) } 2000 | Out-Null
        $c = (E2E).composer
        $r = $N::Rect($h)
    }
    $grow = [Math]::Min(150, $r.Left - 10)
    $y = [int]($c[1] + $c[3] / 2)
    $N::Drag($r.Left + 2, $y, $r.Left + 2 - $grow, $y)
    $wider = Wait-Until { $m = $N::Rect($h); (($m.Right - $m.Left) - ($r.Right - $r.Left)) -gt 100 -and (Near $m.Right $r.Right 3) } 2000
    $m = $N::Rect($h)
    Start-Sleep -Milliseconds 300
    # the bar paints into the new width: the far left looks like the middle
    $edge = $N::Pixel($m.Left + 12, $c[1] + 6)
    $inside = $N::Pixel([int](($m.Left + $m.Right) / 2), $c[1] + 6)
    $filled = $true
    foreach ($sh in 0, 8, 16) { if ([Math]::Abs((($edge -shr $sh) -band 255) - (($inside -shr $sh) -band 255)) -gt 16) { $filled = $false } }
    $N::Drag($m.Left + 2, $y, $m.Left + 2 + ($m.Right - $m.Left) - ($r.Right - $r.Left) + 60, $y)
    $back = Wait-Until { $e = $N::Rect($h); Near ($e.Right - $e.Left) ($r.Right - $r.Left) 4 } 2000
    @{ Pass = ($wider -and $filled -and $back); Detail = "$($r.Right - $r.Left) px wide, then $($m.Right - $m.Left) px; painted to the new edge: $filled ('{0:x6}' vs '{1:x6}'); back to the standard width: $back" -f $edge, $inside }
}

Invoke-Check 'W3' 'double click on the background recentres' 'informational' {
    $h = Show-QCalc
    $c = (E2E).composer
    $x = [int]($c[0] + $c[2] / 2); $y = [int]$c[1] + 4
    $N::Click($x, $y); $N::Click($x, $y)
    $s = $N::Screen()
    $centred = Wait-Until { $m = $N::Rect($h); Near ([int](($m.Left + $m.Right) / 2)) ([int]($s.Width / 2)) 40 } 2000
    $m = $N::Rect($h)
    @{ Pass = $centred; Detail = "bar middle at x $([int](($m.Left + $m.Right) / 2)) on a $($s.Width) px wide screen" }
}

# the drag test's spot would stay for the later shots; a fresh store puts the bar back where it starts
Stop-QCalc $script:proc
Write-Store
$script:proc = Start-QCalc @('--autostart')
Wait-Until { $null -ne (State) } 15000 | Out-Null

# ---- settings ----

function Get-SettingsWindow {
    $w = @($N::WindowsOfClass([int[]]@($script:proc.Id), 'QCalcSettings') | Where-Object { $N::IsWindowVisible($_) })
    if ($w) { return $w[0] }
    [IntPtr]::Zero
}

# clicks a settings control for real: the row's part whose choice is $option, from the window's own layout
function Click-Setting([string]$id, [string]$option) {
    $script:ctl = $null
    Wait-Until {
        $d = Send-Pipe @{ cmd = 'settings-controls' }
        $script:ctl = @($d.controls | Where-Object { $_.id -eq $id -and $_.option -eq $option -and $_.visible })[0]
        [bool]$script:ctl
    } 4000 | Out-Null
    if (-not $script:ctl) { throw "no visible '$id' = '$option' control in the settings window" }
    $r = $script:ctl.rect
    $x = [int]($r[0] + $r[2] / 2); $y = [int]($r[1] + $r[3] / 2)
    # on a shared desktop another app's always-on-top window can sit over the control; a click there isn't ours
    if (-not (Wait-Until { $N::OwnerAt($x, $y) -eq $script:proc.Id } 3000)) {
        $o = Get-Process -Id ($N::OwnerAt($x, $y)) -ErrorAction SilentlyContinue
        throw "the '$option' control is covered by another window ($(if ($o) { "$($o.ProcessName) pid $($o.Id)" } else { '?' }))"
    }
    $N::Click($x, $y)
}

Invoke-Check 'G1' 'ctrl+comma opens settings' 'required' {
    Show-QCalc | Out-Null
    $N::Chord($N::VK_CONTROL, $N::VK_OEM_COMMA)
    $open = Wait-Until { (Get-SettingsWindow) -ne [IntPtr]::Zero } 5000
    $front = Wait-Until { $N::GetForegroundWindow() -eq (Get-SettingsWindow) } 2000
    $w = Get-SettingsWindow
    $st = if ($w -ne [IntPtr]::Zero) { $r = $N::Rect($w); $N::Stats($r.Left + 8, $r.Top + 40, $r.Right - $r.Left - 16, $r.Bottom - $r.Top - 48) } else { @(0, 0, 0) }
    if ($w -ne [IntPtr]::Zero) { $r = $N::Rect($w); $N::Screenshot((Join-Path $OutDir 'settings-light.png'), $r.Left, $r.Top, $r.Right - $r.Left, $r.Bottom - $r.Top) }
    @{ Pass = ($open -and $front -and $st[2] -gt 0.01); Detail = "settings window open: $open, in front: $front; painted: $([Math]::Round($st[2] * 100, 1))% ink, $($st[0]) colours" }
}

Invoke-Check 'G2' 'a change applies live (appearance: dark)' 'required' {
    if ((Get-SettingsWindow) -eq [IntPtr]::Zero) { $N::Chord($N::VK_CONTROL, $N::VK_OEM_COMMA); Show-QCalc | Out-Null; $N::Chord($N::VK_CONTROL, $N::VK_OEM_COMMA) }
    Wait-Until { (Get-SettingsWindow) -ne [IntPtr]::Zero } 4000 | Out-Null
    $w = Get-SettingsWindow
    $r = $N::Rect($w)
    $lightLuma = $N::Stats($r.Left + 8, $r.Top + 40, $r.Right - $r.Left - 16, $r.Bottom - $r.Top - 48)[1]
    Click-Setting 'theme' 'dark'
    $applied = Wait-Until { $e = E2E; $e.theme -eq 'dark' -and $e.dark } 3000
    Start-Sleep -Milliseconds 300
    $darkLuma = $N::Stats($r.Left + 8, $r.Top + 40, $r.Right - $r.Left - 16, $r.Bottom - $r.Top - 48)[1]
    $N::Screenshot((Join-Path $OutDir 'settings-dark.png'), $r.Left, $r.Top, $r.Right - $r.Left, $r.Bottom - $r.Top)
    $saved = (Get-Content -Raw (Join-Path $Data 'settings.json') | ConvertFrom-Json).settings.theme
    # the bar too, the next time it shows
    $N::Chord($N::VK_ESCAPE)
    Wait-Until { (Get-SettingsWindow) -eq [IntPtr]::Zero } 2000 | Out-Null
    $h = Show-QCalc
    Clear-Input
    Start-Sleep -Milliseconds 300
    $c = (E2E).composer
    $barLuma = $N::Stats($c[0] + 4, $c[1] + 4, $c[2] - 8, $c[3] - 8)[1]
    $br = $N::Rect($h)
    $N::Screenshot((Join-Path $OutDir 'bar-dark.png'), $br.Left - 24, $br.Top - 24, $br.Right - $br.Left + 48, $br.Bottom - $br.Top + 48)
    @{ Pass = ($applied -and $saved -eq 'dark' -and $darkLuma -lt $lightLuma - 40 -and $barLuma -lt 90)
       Detail = "theme now '$((E2E).theme)', saved '$saved'; settings window luminance $([Math]::Round($lightLuma)) -> $([Math]::Round($darkLuma)); bar luminance $([Math]::Round($barLuma))" }
}

Invoke-Check 'G3' 'esc closes settings (and light comes back)' 'required' {
    Hide-QCalc
    Send-Pipe @{ cmd = 'settings' } | Out-Null
    Wait-Until { (Get-SettingsWindow) -ne [IntPtr]::Zero } 4000 | Out-Null
    Wait-Until { $N::GetForegroundWindow() -eq (Get-SettingsWindow) } 2000 | Out-Null
    Click-Setting 'theme' 'light'
    $light = Wait-Until { (E2E).theme -eq 'light' } 3000
    $N::Chord($N::VK_ESCAPE)
    $closed = Wait-Until { (Get-SettingsWindow) -eq [IntPtr]::Zero -and -not (E2E).settingsOpen } 3000
    $h = Show-QCalc
    Clear-Input
    Start-Sleep -Milliseconds 300
    $c = (E2E).composer
    $barLuma = $N::Stats($c[0] + 4, $c[1] + 4, $c[2] - 8, $c[3] - 8)[1]
    $br = $N::Rect($h)
    $N::Screenshot((Join-Path $OutDir 'bar-light.png'), $br.Left - 24, $br.Top - 24, $br.Right - $br.Left + 48, $br.Bottom - $br.Top + 48)
    @{ Pass = ($light -and $closed -and $barLuma -gt 170); Detail = "back to light: $light; closed by Esc: $closed; bar luminance $([Math]::Round($barLuma))" }
}

function Distance([int]$a, [int]$b) {
    $d = 0
    foreach ($shift in 0, 8, 16) { $d = [Math]::Max($d, [Math]::Abs((($a -shr $shift) -band 255) - (($b -shr $shift) -band 255))) }
    $d
}

# the bar with an answer over a full-screen backdrop, with probes: a rounded corner shows the backdrop at the corner
# pixel, and the shadow darkens just below the bottom edge
function Save-Look([string]$file, [string]$color, [string]$alt) {
    Hide-QCalc
    $backdrop = Start-Process powershell -PassThru -ArgumentList @(
        '-NoProfile', '-ExecutionPolicy', 'Bypass', '-WindowStyle', 'Hidden',
        '-File', (Join-Path $PSScriptRoot 'backdrop.ps1'), '-Color', $color, '-Alt', $alt)
    try {
        if (-not (Wait-Until { @($N::Windows([int[]]@($backdrop.Id), 'qcalc-e2e-backdrop')).Count -gt 0 } 15000)) { throw 'the backdrop never appeared' }
        Start-Sleep -Milliseconds 300
        $h = Show-QCalc
        Clear-Input
        Type-Text '12 kg to lb'
        $a = Read-Answer '^26\.4'
        Start-Sleep -Milliseconds 300
        $r = $N::Rect($h)
        $m = 48
        $N::Screenshot((Join-Path $OutDir $file), $r.Left - $m, $r.Top - $m, $r.Right - $r.Left + 2 * $m, $r.Bottom - $r.Top + 2 * $m)
        $cx = [int](($r.Left + $r.Right) / 2)
        $corner = $N::Pixel($r.Left, $r.Top); $edge = $N::Pixel($cx, $r.Top + 1)
        $near = $N::Pixel($cx, $r.Bottom + 3); $far = $N::Pixel($cx, $r.Bottom + 40)
        @{ Rounded = ((Distance $corner $edge) -gt 16); Shadow = ((Distance $near $far) -gt 6)
           Text = "$file $($r.Right - $r.Left)x$($r.Bottom - $r.Top) px, answer $(if ($a.Found) { 'shown' } else { 'missing' }); corner {0:x6} vs edge {1:x6}; below {2:x6} vs 40 px down {3:x6}" -f $corner, $edge, $near, $far }
    } finally {
        Stop-Process -Id $backdrop.Id -Force -ErrorAction SilentlyContinue
    }
}

Invoke-Check 'V1' 'look over light and dark (rounded corners, shadow)' 'informational' {
    if ($script:other -and -not $script:other.HasExited) { Stop-Process -Id $script:other.Id -Force }
    $script:other = $null; $script:otherHwnd = [IntPtr]::Zero
    $light = Save-Look 'look-light.png' 'f3f3f3' 'd0d7e2'
    $dark = Save-Look 'look-dark.png' '1c1c1c' '2f3a4c'
    @{ Pass = ($light.Rounded -and $dark.Rounded -and $dark.Shadow); Detail = "$($light.Text); rounded $($light.Rounded), shadow $($light.Shadow) / $($dark.Text); rounded $($dark.Rounded), shadow $($dark.Shadow)" }
}

# 420's smoke rises from a room above the bar; the window grows up to hold it, as on the Mac (effects.rs)
Invoke-Check 'V2' 'smoke room grows the window' 'informational' {
    $h = Show-QCalc
    Clear-Input
    Start-Sleep -Milliseconds 300
    $before = Height $h
    $top = $N::Rect($h).Top
    Type-Text '420'
    $grew = Wait-Until { (Height $h) -ge $before + 100 -or @($N::WindowsOfClass([int[]]@($script:proc.Id), 'QCalcEffect') | Where-Object { $N::IsWindowVisible($_) }).Count -gt 0 } 2500
    $tall = Height $h
    Clear-Input
    Start-Sleep -Seconds 3
    @{ Pass = $grew; Detail = "height $before px, then $tall px (top moved $($top - $N::Rect($h).Top) px), or an effect window: $grew" }
}

# ---- a game ----

# a game runs in a process of its own (qcalc.exe --game <kind>), started by the bar's
function Game-Pids { [int[]](@($script:proc.Id) + @(Get-CimInstance Win32_Process -Filter "ParentProcessId=$($script:proc.Id)" | ForEach-Object { [int]$_.ProcessId })) }

# the game takes the bar's place (games/mod.rs): the bar's left and width, hanging from the composer's top, as tall as
# the game, with no title bar; esc from its lobby closes it back to the bar
Invoke-Check 'M1' 'a game opens in the bar''s place and closes back to the bar' 'required' {
    Show-QCalc | Out-Null
    Clear-Input
    Type-Text 'pong'
    Wait-Until { $s = State; $s.hint } 2000 | Out-Null
    $e = E2E
    $bar = $e.frame
    $composerTop = $e.composer[1]
    $N::Chord($N::VK_RETURN)
    $script:game = [IntPtr]::Zero
    $open = Wait-Until { $g = @($N::WindowsOfClassPrefix((Game-Pids), 'QCalcGame') | Where-Object { $N::IsWindowVisible($_) }); if ($g) { $script:game = $g[0] }; [bool]$g } 5000
    $title = if ($open) { $N::Title($script:game) } else { '' }
    $painted = 0
    $placed = $false
    $where = ''
    if ($open) {
        Start-Sleep -Milliseconds 600
        $r = $N::Rect($script:game)
        $where = "game $($r.Left),$($r.Top) $($r.Right - $r.Left)x$($r.Bottom - $r.Top); bar left $($bar[0]) width $($bar[2] - $bar[0]), composer top $composerTop"
        # the bar's left and width, its top at the composer's (moved up only when it wouldn't fit below)
        $placed = (Near $r.Left $bar[0] 3) -and (Near ($r.Right - $r.Left) ($bar[2] - $bar[0]) 3) -and ($r.Top -le $composerTop + 3) -and (($r.Bottom - $r.Top) -lt 0.9 * $N::Screen().Height)
        $painted = $N::Stats($r.Left + 10, $r.Top + 10, $r.Right - $r.Left - 20, $r.Bottom - $r.Top - 20)[2]
        $N::Screenshot((Join-Path $OutDir 'game.png'), $r.Left, $r.Top, $r.Right - $r.Left, $r.Bottom - $r.Top)
        $how = 'esc (the lobby''s first screen)'
        if (-not (Wait-Until { $N::GetForegroundWindow() -eq $script:game } 1500)) {
            $N::Click([int](($r.Left + $r.Right) / 2), $r.Top + 12)
            Wait-Until { $N::GetForegroundWindow() -eq $script:game } 1500 | Out-Null
        }
        # keys only to the game
        if ($N::GetForegroundWindow() -eq $script:game) { $N::Chord($N::VK_ESCAPE) }
        if (-not (Wait-Until { @($N::WindowsOfClassPrefix((Game-Pids), 'QCalcGame')).Count -eq 0 } 2500)) {
            $how = "WM_CLOSE (esc didn't close it; front was $(Describe-Foreground))"; $N::Close($script:game) | Out-Null
        }
    }
    $gamePid = if ($open) { [int]$N::ProcessOf($script:game) } else { 0 }
    $closed = Wait-Until { -not (E2E).gameOpen -and @($N::WindowsOfClassPrefix((Game-Pids), 'QCalcGame')).Count -eq 0 -and -not ($gamePid -and (Get-Process -Id $gamePid -ErrorAction SilentlyContinue)) } 4000
    $back = Wait-Until { Test-Shown } 2500
    @{ Pass = ($open -and $placed -and $closed -and $back -and $painted -gt 0.005 -and -not $script:proc.HasExited)
       Detail = "game window '$title' opened: $open, in the bar's place: $placed ($where), painted $([Math]::Round($painted * 100, 1))% ink; closed by $($how): $closed; the bar back: $back; app alive: $(-not $script:proc.HasExited)" }
}

# ---- the held-hotkey retry ----

# a launcher held the chord at sign-in: registration fails, then the retries (1 s apart) take it once it's free
Invoke-Check 'K1' 'hotkey works after another process held it at launch' 'required' {
    Stop-QCalc $script:proc
    $held = $N::HoldHotkey(0x5e2e, 0x7, $N::VK_F9)
    if (-not $held) { return @{ Pass = $false; Detail = "couldn't hold $Chord for the test (someone else has it)" } }
    try {
        $script:proc = Start-QCalc @('--autostart')
        Wait-Until { $null -ne (State) } 15000 | Out-Null
        Start-Sleep -Milliseconds 1500
        $during = (E2E).hotkey
    } finally {
        $N::ReleaseHotkey(0x5e2e) | Out-Null
    }
    $took = Wait-Until { (E2E).hotkey.active } 5000
    Send-Hotkey
    $shown = Wait-Until { Test-Shown } 3000
    $attempts = @(if (Test-Path $Log) { Select-String -Path $Log -Pattern 'hotkey' | Select-Object -Last 4 | ForEach-Object { $_.Line -replace '^\d+ ', '' } })
    @{ Pass = ($held -and -not $during.active -and $took -and $shown)
       Detail = "held by the test at launch: $held; registered while held: $([bool]$during.active); taken after release: $took; shown by the chord: $shown; log: $($attempts -join ' / ')" }
}

# ---- what a screen reader sees ----

# UI Automation's tree (uia.rs, a11y.rs bar_tree, settings_window.rs uia_tree), read as Narrator reads it
function Uia-Children($e) {
    @($e.FindAll([System.Windows.Automation.TreeScope]::Children, [System.Windows.Automation.Condition]::TrueCondition))
}
function Uia-Describe($e) {
    $c = $e.Current
    $v = ''
    try { $v = '=' + $e.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value } catch {}
    "$($c.ControlType.ProgrammaticName -replace 'ControlType\.', '') '$($c.Name)'$v$(if ($c.HasKeyboardFocus) { ' (focus)' })"
}

Invoke-Check 'A1' 'ui automation: the bar, the answer, the tape and settings are named parts' 'required' {
    $AE = [System.Windows.Automation.AutomationElement]
    Show-QCalc | Out-Null
    Clear-Input
    Type-Text '6*7'
    Read-Answer '^42$' | Out-Null
    $N::Chord($N::VK_RETURN)
    Wait-Until { $s = State; $s -and $s.text -eq '' } 2000 | Out-Null
    Type-Text '2+2'
    Read-Answer '^4$' | Out-Null
    # the line has the focus; Up picks the last row in the tape, which takes it
    $bar = $AE::FromHandle((Get-Bar))
    $kids = Uia-Children $bar
    $edit = $kids | Where-Object { $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Edit } | Select-Object -First 1
    $value = if ($edit) { $edit.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value } else { '' }
    $answer = $kids | Where-Object { $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Text -and $_.Current.Name -eq '4' }
    $focused = $edit -and $edit.Current.HasKeyboardFocus
    $N::Chord($N::VK_UP)
    Start-Sleep -Milliseconds 300
    $rows = @(Uia-Children $bar | Where-Object { $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::ListItem })
    $row = $rows | Where-Object { $_.Current.Name -like '6*7 = 42' -or $_.Current.Name -like '6×7 = 42' } | Select-Object -First 1
    $rowFocus = $row -and $row.Current.HasKeyboardFocus
    $seen = (Uia-Children $bar | ForEach-Object { Uia-Describe $_ }) -join '; '
    $N::Chord($N::VK_ESCAPE)
    Hide-QCalc
    Send-Pipe @{ cmd = 'settings' } | Out-Null
    Wait-Until { (Get-SettingsWindow) -ne [IntPtr]::Zero } 4000 | Out-Null
    Start-Sleep -Milliseconds 300
    $sw = Get-SettingsWindow
    $parts = if ($sw -ne [IntPtr]::Zero) { Uia-Children ($AE::FromHandle($sw)) } else { @() }
    $tabs = @($parts | Where-Object { $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::TabItem }).Count
    $checks = @($parts | Where-Object { $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::CheckBox }).Count
    $radios = @($parts | Where-Object { $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::RadioButton }).Count
    $settingsName = if ($sw -ne [IntPtr]::Zero) { ($AE::FromHandle($sw)).Current.Name } else { '' }
    if ($sw -ne [IntPtr]::Zero) { $N::Close($sw) | Out-Null }
    Wait-Until { (Get-SettingsWindow) -eq [IntPtr]::Zero } 2000 | Out-Null
    @{ Pass = ($edit -and $value -eq '2+2' -and $answer -and $focused -and $row -and $rowFocus -and $tabs -ge 5 -and $checks -ge 1 -and $radios -ge 2)
       Detail = "bar: $seen; line focused before Up: $focused, row focused after: $rowFocus; settings '$settingsName': $tabs tabs, $checks check boxes, $radios radio buttons" }
}

# ---- targets (soft) ----

Invoke-Check 'Z1' 'idle memory after use, hidden and trimmed' 'target' {
    Show-QCalc | Out-Null
    foreach ($e in '2+2', 'sin(90)', '12 kg to lb') { Clear-Input; Type-Text $e; Start-Sleep -Milliseconds 150 }
    Clear-Input
    Hide-QCalc
    # what 30 s hidden gives back (app.rs release_hidden: the regexes, the device, the layouts), asked for now so
    # the suite needn't wait; a hide shorter than that keeps them, so a quick reopen's first key compiles nothing
    Send-Pipe @{ cmd = 'release' } | Out-Null
    Start-Sleep -Milliseconds 800
    # Task Manager's "Memory": the private working set, hidden after use and released, then after the working-set trim
    $private = {
        $p = Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter "IDProcess = $($script:proc.Id)"
        if ($p) { $p.WorkingSetPrivate / 1MB } else { (Get-Process -Id $script:proc.Id).PrivateMemorySize64 / 1MB }
    }
    $hidden = & $private
    $t = Send-Pipe @{ cmd = 'trim' }
    Start-Sleep -Milliseconds 800
    $trimmed = & $private
    $r = Target 'private working set, hidden after use' $hidden 12.5 'MB'
    $r.Detail += " (after the trim $([Math]::Round($trimmed, 1)) MB; private bytes $([Math]::Round($t.private / 1MB, 1)) MB)"
    $r
}

Invoke-Check 'Z2' 'keystroke to painted answer' 'target' {
    $inputs = @('2+2', 'sin(90)', '72 f to c', '1/3 + 1/6', 'sqrt(2)*pi', '5 km in miles', '3x+2=11', '(1+2)*(3+4)/5^2', '20% of 80', 'e^(i pi)+1')
    # after Z1's release (as after 30 s hidden): the bench shows the bar and types at once, ahead of the warm-up
    $cold = Send-Pipe @{ inputs = $inputs } 120000
    Hide-QCalc
    Start-Sleep -Milliseconds 300
    # a quick hide and show keeps what the keystroke path compiled: the same run again, the target's
    $res = Send-Pipe @{ inputs = $inputs } 120000
    if (-not $res -or -not $cold) { return @{ Pass = $false; Detail = 'the bench run never replied' } }
    Hide-QCalc
    $r = Target 'keystroke -> answer painted, median' $res.median 0.8 'ms'
    $r.Detail += " (n $($res.n), p90 $([Math]::Round($res.p90, 3)), p99 $([Math]::Round($res.p99, 3)), timeouts $($res.timeouts); after a long hide: median $([Math]::Round($cold.median, 3)), p99 $([Math]::Round($cold.p99, 3)))"
    $r
}

Invoke-Check 'Z3' 'warm launch to ready' 'target' {
    Stop-QCalc $script:proc
    $times = @()
    for ($i = 0; $i -lt 6; $i++) {
        Remove-Item $Log -ErrorAction SilentlyContinue
        $p = Start-QCalc @('--autostart')
        $script:ready = $null
        Wait-Until { if (Test-Path $Log) { $script:ready = Select-String -Path $Log -Pattern 'ready \(.*\) at \+([\d.]+) ms' | Select-Object -First 1 }; [bool]$script:ready } 10000 | Out-Null
        # the first is cold-ish (the exe and DLLs paged in); the rest are warm
        if ($script:ready -and $i -gt 0) { $times += [double]$script:ready.Matches[0].Groups[1].Value }
        Wait-Until { $null -ne (State) } 5000 | Out-Null
        Stop-QCalc $p
    }
    if (-not $times) { return @{ Pass = $false; Detail = 'no ready line in the log' } }
    $sorted = @($times | Sort-Object)
    $median = $sorted[[int][Math]::Floor($sorted.Count / 2)]
    $r = Target 'warm launch -> ready (in the tray)' $median 75 'ms'
    $r.Detail += " (runs: $($times -join ', '))"
    $r
}

# ---- the installer ----

function Find-Makensis {
    if ($Makensis -and (Test-Path $Makensis)) { return $Makensis }
    $c = (Get-Command makensis -ErrorAction SilentlyContinue).Source
    if ($c) { return $c }
    foreach ($p in "$env:LOCALAPPDATA\tauri\NSIS\makensis.exe", "${env:ProgramFiles(x86)}\NSIS\makensis.exe", "$env:ProgramFiles\NSIS\makensis.exe") {
        if ($p -and (Test-Path $p)) { return $p }
    }
    $null
}

$AppId = "Q Calc E2E $Instance"
$UninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$AppId"
$RunKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$InstallDir = Join-Path $Root 'install'
$Shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) "$AppId.lnk"

function Build-Installer([string]$version) {
    $out = Join-Path $Root "setup-$version.exe"
    $nsi = (Resolve-Path (Join-Path $PSScriptRoot '..\..\installer\qcalc.nsi')).Path
    & $script:makensis /V2 "/DVERSION=$version" "/DEXE=$Exe" "/DOUT=$out" "/DAPP_ID=$AppId" $nsi | Out-Host
    if ($LASTEXITCODE) { throw "makensis failed for $version" }
    $out
}

function Run-Setup([string]$setup, [string[]]$extra) {
    # /D= must be last and unquoted
    $p = Start-Process -FilePath $setup -ArgumentList (@('/S') + $extra + @("/D=$InstallDir")) -PassThru
    if (-not $p.WaitForExit(120000)) { throw "$setup didn't finish" }
    $p.ExitCode
}

function Installed-Copies { @(Get-Process qcalc -ErrorAction SilentlyContinue | Where-Object { try { $_.Path -eq (Join-Path $InstallDir 'qcalc.exe') } catch { $false } }) }

$script:makensis = if ($SkipInstaller) { $null } else { Find-Makensis }
if (-not $script:makensis) {
    Log "installer checks skipped: $(if ($SkipInstaller) { '-SkipInstaller' } else { 'no makensis (NSIS 3) found' })"
} else {
    $InstInstance = "$Instance-installed"
    $savedInstance = $env:QCALC_INSTANCE
    Write-Store @{} (Join-Path $Root 'data-installed')

    Invoke-Check 'I1' 'install (per user, own app id, temp folder)' 'required' {
        $v1 = Build-Installer '0.0.1'
        $script:v2 = Build-Installer '0.0.2'
        $code = Run-Setup $v1 @('/NORELAUNCH')
        $exe = Join-Path $InstallDir 'qcalc.exe'
        $reg = Get-ItemProperty $UninstallKey -ErrorAction SilentlyContinue
        $run = (Get-ItemProperty $RunKey -ErrorAction SilentlyContinue).$AppId
        $ok = $code -eq 0 -and (Test-Path $exe) -and (Test-Path (Join-Path $InstallDir 'uninstall.exe')) -and $reg.DisplayVersion -eq '0.0.1' -and
            $run -eq "`"$exe`" --autostart" -and (Test-Path $Shortcut)
        # the installed copy runs, under its own instance and data
        $env:QCALC_INSTANCE = $InstInstance; $env:QCALC_DATA = Join-Path $Root 'data-installed'
        try {
            $script:inst = Start-QCalc @('--autostart') $exe
            $alive = Wait-Until { $null -ne (Send-Pipe @{ cmd = 'state' } 1000 $InstInstance) } 15000
        } finally { $env:QCALC_INSTANCE = $savedInstance; $env:QCALC_DATA = $Data }
        @{ Pass = ($ok -and $alive); Detail = "exit $code; exe: $(Test-Path $exe); uninstall entry $($reg.DisplayVersion); Run value: '$run'; Start menu shortcut: $(Test-Path $Shortcut); installed copy answers its pipe: $alive" }
    }

    Invoke-Check 'I2' 'upgrade in place (closes the running copy, keeps the sign-in choice, relaunches)' 'required' {
        # the user turned start-at-sign-in off; an upgrade must not turn it back on
        Remove-ItemProperty -Path $RunKey -Name $AppId -ErrorAction SilentlyContinue
        $old = $script:inst
        $env:QCALC_INSTANCE = $InstInstance; $env:QCALC_DATA = Join-Path $Root 'data-installed'
        try { $code = Run-Setup $script:v2 @() } finally { $env:QCALC_INSTANCE = $savedInstance; $env:QCALC_DATA = $Data }
        $closed = $old.WaitForExit(5000)
        $reg = Get-ItemProperty $UninstallKey -ErrorAction SilentlyContinue
        $run = (Get-ItemProperty $RunKey -ErrorAction SilentlyContinue).$AppId
        $relaunched = Wait-Until { (Installed-Copies).Count -ge 1 } 10000
        Installed-Copies | ForEach-Object { $script:mine.Add($_.Id) }
        $s = $null
        Wait-Until { $script:s = Send-Pipe @{ cmd = 'state' } 1000 $InstInstance; $null -ne $script:s } 10000 | Out-Null
        @{ Pass = ($code -eq 0 -and $closed -and $reg.DisplayVersion -eq '0.0.2' -and -not $run -and $relaunched -and $script:s -and -not $script:s.visible)
           Detail = "exit $code; old copy closed: $closed; version $($reg.DisplayVersion); Run value stayed off: $(-not $run); relaunched: $relaunched, in the tray: $($script:s -and -not $script:s.visible)" }
    }

    Invoke-Check 'I3' 'uninstall (closes it, removes files, entry, shortcut, its own Run value)' 'required' {
        $exe = Join-Path $InstallDir 'qcalc.exe'
        Set-ItemProperty -Path $RunKey -Name $AppId -Value "`"$exe`" --autostart"
        $running = Installed-Copies
        $p = Start-Process -FilePath (Join-Path $InstallDir 'uninstall.exe') -ArgumentList '/S' -PassThru
        $p.WaitForExit(60000) | Out-Null
        # the uninstaller runs a copy of itself from %TEMP%; wait for its work
        # mid-delete the exe can answer Access denied, which Stop turns into a throw: that is "not gone yet"
        $gone = Wait-Until { try { -not (Test-Path $exe) -and -not (Test-Path $UninstallKey) } catch { $false } } 30000
        $closed = @($running | Where-Object { -not $_.HasExited }).Count -eq 0
        $run = (Get-ItemProperty $RunKey -ErrorAction SilentlyContinue).$AppId
        $data = Test-Path (Join-Path $Root 'data-installed\settings.json')
        @{ Pass = ($gone -and $closed -and -not $run -and -not (Test-Path $Shortcut) -and $data)
           Detail = "files and entry gone: $gone; running copy closed: $closed; Run value removed: $(-not $run); shortcut removed: $(-not (Test-Path $Shortcut)); data kept: $data" }
    }
}

} finally {
    # ---- clean up: only what this run made ----
    try { if ($script:proc) { Stop-QCalc $script:proc } } catch { }
    Stop-Mine
    if ($script:other -and -not $script:other.HasExited) { Stop-Process -Id $script:other.Id -Force -ErrorAction SilentlyContinue }
    if ($script:makensis) {
        if (Test-Path (Join-Path $InstallDir 'uninstall.exe')) { Start-Process (Join-Path $InstallDir 'uninstall.exe') -ArgumentList '/S' -Wait -ErrorAction SilentlyContinue }
        Remove-Item $UninstallKey -Recurse -ErrorAction SilentlyContinue
        Remove-Item "HKCU:\Software\maxconine\$AppId" -Recurse -ErrorAction SilentlyContinue
        Remove-ItemProperty -Path $RunKey -Name $AppId -ErrorAction SilentlyContinue
        Remove-Item $Shortcut -ErrorAction SilentlyContinue
    }
    if ($null -ne $script:clipBefore) { try { Set-Clipboard -Value $script:clipBefore } catch { } }
    if (Test-Path $Log) { Copy-Item $Log (Join-Path $OutDir 'qcalc.log') -ErrorAction SilentlyContinue }
    Remove-Item $Root -Recurse -Force -ErrorAction SilentlyContinue
}

# ---------------------------------------------------------------------------

$failedRequired = @($script:results | Where-Object { $_.Kind -eq 'required' -and -not $_.Pass })
$md = New-Object System.Text.StringBuilder
[void]$md.AppendLine('## Q Calc native Windows smoke test')
[void]$md.AppendLine()
[void]$md.AppendLine($(if ($failedRequired) { "**$($failedRequired.Count) required check(s) failed.**" } else { '**All required checks passed.**' }))
[void]$md.AppendLine()
[void]$md.AppendLine('| # | check | kind | result | detail | screenshot |')
[void]$md.AppendLine('|---|---|---|---|---|---|')
foreach ($r in $script:results) {
    $res = if ($r.Pass) { 'pass' } elseif ($r.Kind -eq 'required') { 'FAIL' } else { 'warn' }
    [void]$md.AppendLine("| $($r.Id) | $($r.Name) | $($r.Kind) | $res | $(($r.Detail -replace '\|', '\|' -replace "`r?`n", ' ')) | $($r.Shot) |")
}
if ($script:targets.Count) {
    [void]$md.AppendLine()
    [void]$md.AppendLine('### Targets (AGENTS.md; soft)')
    [void]$md.AppendLine()
    [void]$md.AppendLine('| what | measured | target | met |')
    [void]$md.AppendLine('|---|---|---|---|')
    foreach ($t in $script:targets) { [void]$md.AppendLine("| $($t.What) | $($t.Value) $($t.Unit) | < $($t.Limit) $($t.Unit) | $(if ($t.Met) { 'yes' } else { 'no' }) |") }
}
[void]$md.AppendLine()
[void]$md.AppendLine('<details><summary>runner</summary>')
[void]$md.AppendLine()
$env_.GetEnumerator() | ForEach-Object { [void]$md.AppendLine("- $($_.Key): $($_.Value)") }
[void]$md.AppendLine()
[void]$md.AppendLine('</details>')

[IO.File]::WriteAllText((Join-Path $OutDir 'results.md'), $md.ToString())
if ($env:GITHUB_STEP_SUMMARY) { [IO.File]::AppendAllText($env:GITHUB_STEP_SUMMARY, $md.ToString()) }
Write-Host $md.ToString()
Stop-Transcript | Out-Null
if ($failedRequired) { exit 1 }
exit 0
