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

# the block sees the caller's variables through powershell's dynamic scope, so this uses names no caller does
# (it once took $cond, which hid the ui automation conditions the settings and tray checks keep under that name)
function Wait-Until([scriptblock]$__until, [int]$__ms = 2000) {
    $__sw = [Diagnostics.Stopwatch]::StartNew()
    do {
        if (& $__until) { return $true }
        Start-Sleep -Milliseconds 50
    } while ($__sw.ElapsedMilliseconds -lt $__ms)
    return [bool](& $__until)
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
            Click-Other | Out-Null
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

# a real click on another app's window; that's what takes focus away on a desktop. a fresh window can miss the
# first click, so it clicks again (a little lower each time) until that window is in front; returns whether it is
function Click-Other {
    for ($i = 0; $i -lt 4; $i++) {
        $r = $N::Rect($script:otherHwnd)
        $x = [int](($r.Left + $r.Right) / 2); $y = [int](($r.Top + $r.Bottom) / 2) + 20 * $i
        $q = Get-QCalcWindow
        if ($q -ne [IntPtr]::Zero -and $N::IsWindowVisible($q)) {
            $qr = $N::Rect($q)
            if ($x -ge $qr.Left -and $x -le $qr.Right -and $y -ge $qr.Top -and $y -le $qr.Bottom) {
                $y = $r.Bottom - 20
                Log 'click point overlapped the calculator; clicking the bottom of the other window'
            }
        }
        $N::Click($x, $y)
        if (Wait-Until { $N::GetForegroundWindow() -eq $script:otherHwnd } 600) { return $true }
        Log "click $($i + 1) on the other window left $(Describe-Foreground) in front"
    }
    return $false
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
    # an annotation shows on the run's page without signing in, unlike the log
    if (-not $pass) { Write-Host ("::{0} title=check {1} {2}::{3}" -f $(if ($required) { 'error' } else { 'warning' }), $id, $name, ($detail -replace "`r?`n", ' ')) }
    # and what the app saw, which otherwise is only in the artifact
    if (-not $pass -and $required) { Write-Host ("::notice title=check {0}: what the app saw::{1}" -f $id, (Describe-App)) }
}

# whether q calc is still running, where its window is, and the host's last notes, on one line
function Describe-App {
    $pids = Get-QCalcPids
    $h = Get-QCalcWindow
    $win = if ($h -eq [IntPtr]::Zero) { 'no window' } else {
        $r = $N::Rect($h)
        "window $($r.Left),$($r.Top) to $($r.Right),$($r.Bottom), visible $($N::IsWindowVisible($h))"
    }
    $notes = if ($lookNotes -and (Test-Path $lookNotes)) { (Get-Content $lookNotes -Tail 40) -join ' / ' } else { 'no notes' }
    $text = "processes: $($pids.Count); $win; screen $($N::Screen().Width)x$($N::Screen().Height); front: $(Describe-Foreground); notes: $notes"
    $text -replace "`r?`n", ' ' -replace '::', ': :' | ForEach-Object { if ($_.Length -gt 6000) { $_.Substring($_.Length - 6000) } else { $_ } }
}

function Expect-Answer([string]$expr, [string]$pattern) {
    Show-QCalc | Out-Null
    Clear-Input
    $N::Type($expr, 30)
    $a = Read-Answer $pattern
    if ($a.Found) { @{ Pass = $true; Detail = "'$expr' shows '$($a.Text)' (via $($a.Source))" } }
    else { @{ Pass = $false; Detail = "'$expr' never showed /$pattern/; saw: $($a.Text)" } }
}

function Hex([int]$c) { '#{0:x6}' -f $c }

# how far apart two colours are, channel by channel
function Distance([int]$a, [int]$b) {
    $d = 0
    foreach ($shift in 0, 8, 16) { $d = [Math]::Max($d, [Math]::Abs((($a -shr $shift) -band 255) - (($b -shr $shift) -band 255))) }
    $d
}

# the window with an answer over a full-screen backdrop, cropped with room for its shadow, plus pixel probes:
# a rounded corner shows what's outside the window at its corner pixel instead of the hairline the top edge has,
# and a shadow darkens just below the bottom edge
function Save-Look([string]$file, [string]$color, [string]$alt) {
    Hide-QCalc
    $backdrop = Start-Process powershell -PassThru -ArgumentList @(
        '-NoProfile', '-ExecutionPolicy', 'Bypass', '-WindowStyle', 'Hidden',
        '-File', (Join-Path $PSScriptRoot 'backdrop.ps1'), '-Color', $color, '-Alt', $alt)
    try {
        $up = Wait-Until { @($N::Windows([int[]]@($backdrop.Id), 'qcalc-e2e-backdrop')).Count -gt 0 } 15000
        if (-not $up) { throw 'the backdrop never appeared' }
        Start-Sleep -Milliseconds 300
        $h = Show-QCalc
        Clear-Input
        $N::Type('12 kg to lb', 30)
        $a = Read-Answer '^26\.4'
        Start-Sleep -Milliseconds 400
        $r = $N::Rect($h)
        $m = 48
        $N::Screenshot((Join-Path $OutDir $file), $r.Left - $m, $r.Top - $m, $r.Right - $r.Left + 2 * $m, $r.Bottom - $r.Top + 2 * $m)
        $cx = [int](($r.Left + $r.Right) / 2)
        $corner = $N::Pixel($r.Left, $r.Top)
        $edge = $N::Pixel($cx, $r.Top)
        $near = $N::Pixel($cx, $r.Bottom + 3)
        $far = $N::Pixel($cx, $r.Bottom + 40)
        @(
            "$file $($r.Right - $r.Left)x$($r.Bottom - $r.Top) px, answer $(if ($a.Found) { 'shown' } else { 'missing' })",
            "corner pixel $(Hex $corner) vs top edge $(Hex $edge) (rounded: $((Distance $corner $edge) -gt 16))",
            "below the edge $(Hex $near) vs 40 px down $(Hex $far) (shadow: $((Distance $near $far) -gt 6))"
        ) -join '; '
    } finally {
        Stop-Process -Id $backdrop.Id -Force -ErrorAction SilentlyContinue
    }
}

# the tray menu's Settings... opens the same window; a second route, reported rather than required
function Test-TraySettings {
    try {
        Hide-QCalc
        $button = Find-TrayButton
        if (-not $button) { return 'tray icon not found' }
        $box = $button.Current.BoundingRectangle
        $N::RightClick([int]($box.X + $box.Width / 2), [int]($box.Y + $box.Height / 2))
        $script:item = $null
        $cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, "Settings$([char]0x2026)")
        Wait-Until {
            $menu = $N::FindWindowW('#32768', $null)
            if ($menu -ne [IntPtr]::Zero) { $script:item = [System.Windows.Automation.AutomationElement]::FromHandle($menu).FindFirst([System.Windows.Automation.TreeScope]::Descendants, $cond) }
            [bool]$script:item
        } 3000 | Out-Null
        if (-not $script:item) { $N::Chord($N::VK_ESCAPE); return 'no Settings... in the tray menu' }
        $script:item.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
        if (-not (Wait-Until { (Get-SettingsWindow) -ne [IntPtr]::Zero } 5000)) { return 'Settings... did not open the window' }
        $w = Get-SettingsWindow
        [System.Windows.Automation.AutomationElement]::FromHandle($w).GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern).Close()
        'opened the settings window'
    } catch {
        "error: $($_.Exception.Message)"
    }
}

function Get-SettingsWindow {
    $pids = Get-QCalcPids
    if (-not $pids) { return [IntPtr]::Zero }
    $w = @($N::Windows([int[]]$pids, 'Q Calc Settings') | Where-Object { $N::IsWindowVisible($_) })
    if ($w) { return $w[0] }
    [IntPtr]::Zero
}

# picks an appearance in the settings window the way a person would, saves a shot of that window, and closes it
function Set-Appearance([string]$choice, [string]$shot) {
    Show-QCalc | Out-Null
    $N::Chord($N::VK_CONTROL, $N::VK_OEM_COMMA)
    if (-not (Wait-Until { (Get-SettingsWindow) -ne [IntPtr]::Zero } 5000)) { throw 'ctrl+comma never opened the settings window' }
    $w = Get-SettingsWindow
    $root = [System.Windows.Automation.AutomationElement]::FromHandle($w)
    $cond = New-Object System.Windows.Automation.AndCondition(
        (New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, $choice)),
        (New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button)))
    $script:button = $null
    Wait-Until { $script:button = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $cond); [bool]$script:button } 5000 | Out-Null
    if (-not $script:button) { throw "no '$choice' button in the settings window" }
    # the choices carry aria-pressed, so chromium offers them as toggles with no invoke; either one clicks
    $pattern = $null
    if ($script:button.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) { $pattern.Invoke() }
    else { $script:button.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle() }
    Start-Sleep -Milliseconds 500
    $r = $N::Rect($w)
    $N::Screenshot((Join-Path $OutDir $shot), $r.Left, $r.Top, $r.Right - $r.Left, $r.Bottom - $r.Top)
    $root.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern).Close()
    Wait-Until { (Get-SettingsWindow) -eq [IntPtr]::Zero } 2000 | Out-Null
}

# ---------------------------------------------------------------------------

$screen = $N::Screen()
$env_ = [ordered]@{
    'exe' = $Exe
    'install' = $(if ($env:QCALC_INSTALL_MODE) { $env:QCALC_INSTALL_MODE } else { 'unknown' })
    'os' = [Environment]::OSVersion.VersionString
    'edition' = $(try { $o = Get-CimInstance Win32_OperatingSystem; "$($o.Caption) build $($o.BuildNumber)" } catch { "unknown: $($_.Exception.Message)" })
    'display adapter' = $(try { (@(Get-CimInstance Win32_VideoController | ForEach-Object { "$($_.Name) (driver $($_.DriverVersion))" }) -join ', ') } catch { "unknown: $($_.Exception.Message)" })
    'dwm composition' = $N::DwmComposition()
    'transparency effects' = $(try { (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize' -ErrorAction Stop).EnableTransparency } catch { 'unset' })
    'apps use light theme' = $(try { (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize' -ErrorAction Stop).AppsUseLightTheme } catch { 'unset' })
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
$lookNotes = Join-Path $env:TEMP 'qcalc-e2e-look.txt'
Remove-Item $lookNotes -ErrorAction SilentlyContinue
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
Log "opened again pid $($script:proc.Id)"

# opening it from start or a shortcut, with nothing running, shows the bar; only the launch at sign in is quiet
Invoke-Check '0b' 'opening it again shows the bar' $false {
    $shown = Wait-Until { Test-Shown } 30000
    @{ Pass = $shown; Detail = "shown when opened with nothing running: $shown" }
}

Get-Process -Name $ProcName -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 500
Remove-Item $lookNotes -ErrorAction SilentlyContinue
$script:proc = Start-Process -FilePath $Exe -ArgumentList '--autostart' -PassThru
Log "relaunched as at sign in, pid $($script:proc.Id)"

Invoke-Check '1' 'the launch at sign in starts hidden' $true {
    $exists = Wait-Until { (Get-QCalcWindow) -ne [IntPtr]::Zero } 30000
    # let the webview load and anything that would flash on screen do so
    Start-Sleep -Seconds 2
    $alive = -not $script:proc.HasExited
    $visible = @($N::Windows([int[]](Get-QCalcPids), '') | Where-Object { $N::IsWindowVisible($_) -and $N::Title($_) -like 'Q Calc*' } | ForEach-Object { "'$($N::Title($_))'" })
    @{ Pass = ($alive -and $exists -and $visible.Count -eq 0)
       Detail = "process alive: $alive; window created: $exists; visible windows: $(if ($visible) { $visible -join ', ' } else { 'none' })" }
}

# the notification area button named Q Calc, opening the overflow flyout if the icon is tucked in there
# (the flyout stays open when it's found there, so the button can be clicked); $null if there's none
function Find-TrayButton {
    $find = {
        foreach ($cls in 'Shell_TrayWnd', 'NotifyIconOverflowWindow', 'TopLevelWindowForOverflowXamlIsland') {
            # ui automation's desktop, not FindWindowW: that never returns windows 11's overflow flyout
            $w = [System.Windows.Automation.AutomationElement]::RootElement.FindFirst([System.Windows.Automation.TreeScope]::Children,
                (New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ClassNameProperty, $cls)))
            if (-not $w) { continue }
            $buttons = $w.FindAll(
                [System.Windows.Automation.TreeScope]::Descendants,
                (New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button)))
            foreach ($b in $buttons) {
                # not $n: powershell names ignore case, and that would hide $N
                try { $name = $b.Current.Name } catch { continue }
                if ($name -like '*Q Calc*') { return $b }
            }
        }
        return $null
    }
    $button = & $find
    if ($button) { return $button }
    # windows 11 style taskbars tuck new icons into the overflow flyout, which is empty until opened
    $tray = $N::FindWindowW('Shell_TrayWnd', $null)
    if ($tray -eq [IntPtr]::Zero) { return $null }
    $cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, 'Show hidden icons', [System.Windows.Automation.PropertyConditionFlags]::IgnoreCase)
    $chevron = [System.Windows.Automation.AutomationElement]::FromHandle($tray).FindFirst([System.Windows.Automation.TreeScope]::Descendants, $cond)
    if (-not $chevron) { return $null }
    $chevron.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    $script:trayButton = $null
    Wait-Until { $script:trayButton = & $find; [bool]$script:trayButton } 2000 | Out-Null
    if (-not $script:trayButton) { $N::Chord($N::VK_ESCAPE) }
    $script:trayButton
}

Invoke-Check '1b' 'tray icon' $false {
    $button = Find-TrayButton
    if ($button) { $N::Chord($N::VK_ESCAPE) }
    @{ Pass = [bool]$button; Detail = $(if ($button) { "found '$($button.Current.Name)'" } else { 'no notification area item named Q Calc (best effort: shell layouts differ)' }) }
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
    $otherFront = Click-Other
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
    Click-Other | Out-Null
    $hidden = Wait-Until { -not (Test-Shown) } 1500
    @{ Pass = $hidden; Detail = "front after click: $(Describe-Foreground); hidden: $hidden" }
}

# single instance may not exist yet
Invoke-Check '8' 'second launch reuses the first' $false {
    Hide-QCalc
    $second = Start-Process -FilePath $Exe -PassThru
    Wait-Until { $second.HasExited } 5000 | Out-Null
    Wait-Until { Test-Shown } 1000 | Out-Null
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
    Start-Sleep -Milliseconds 400
    $before = Height $h
    Clear-Input
    $N::Type('graph sin(x)', 30)
    $grew = Wait-Until { (Height $h) -gt $before } 4000
    @{ Pass = $grew; Detail = "height $before px before, $(Height $h) px after typing 'graph sin(x)'" }
}

# 420's smoke rises from a room the page opens above the bar; the window has to grow up to hold it, as on the mac
Invoke-Check '9b' 'smoke room grows the window' $false {
    $h = Show-QCalc
    Clear-Input
    Start-Sleep -Milliseconds 300
    $before = Height $h
    $top = $N::Rect($h).Top
    $N::Type('420', 30)
    $grew = Wait-Until { (Height $h) -ge $before + 100 } 2000
    $tall = Height $h
    $moved = $top - $N::Rect($h).Top
    $a = Read-Answer '^420$' 1000
    Clear-Input
    # the smoke clears after about 2.7 s; later shots shouldn't catch it
    $back = Wait-Until { (Height $h) -le $before } 4000
    @{ Pass = $grew; Detail = "answer 420 $(if ($a.Found) { 'shown' } else { 'not seen: ' + $a.Text }); height $before px, then $tall px with the top $moved px higher; back to $(Height $h) px: $back" }
}

function Near([int]$a, [int]$b, [int]$within = 6) { [Math]::Abs($a - $b) -le $within }

# the top edge is the bar's handle, as on the mac: the input lies under it, and mustn't take the press instead
Invoke-Check '9c' 'top edge drags the bar and the spot is kept' $true {
    $h = Show-QCalc
    Clear-Input
    Start-Sleep -Milliseconds 300
    $r = $N::Rect($h)
    $x = [int](($r.Left + $r.Right) / 2); $y = $r.Top + 5
    # toward whichever side has room: a bar pushed off the screen is pulled back on at the next show, so on a small
    # screen (the runner's is 1024x768) a fixed 220 px to the right isn't a spot that can be kept
    $s = $N::Screen()
    $dx = if ($r.Right + 230 -le $s.Width) { 220 } else { -[Math]::Min(220, $r.Left - 10) }
    $dy = if ($r.Bottom + 150 -le $s.Height) { 140 } else { -[Math]::Min(140, $r.Top - 10) }
    $N::Drag($x, $y, $x + $dx, $y + $dy)
    $moved = Wait-Until { $m = $N::Rect($h); (Near $m.Left ($r.Left + $dx)) -and (Near $m.Top ($r.Top + $dy)) } 2000
    $m = $N::Rect($h)
    $stayed = Test-Shown
    Hide-QCalc
    $h = Show-QCalc
    $again = $N::Rect($h)
    $kept = (Near $again.Left $m.Left) -and (Near $again.Top $m.Top)
    @{ Pass = ($moved -and $stayed -and $kept)
       Detail = "dragged $dx, $dy px: moved $($m.Left - $r.Left), $($m.Top - $r.Top) px; still shown: $stayed; same spot after hide and show: $kept" }
}

Invoke-Check '9d' 'double click on the top edge recentres' $true {
    $h = Show-QCalc
    $r = $N::Rect($h)
    $x = [int](($r.Left + $r.Right) / 2); $y = $r.Top + 5
    $N::Click($x, $y)
    $N::Click($x, $y)
    $s = $N::Screen()
    $centred = Wait-Until { $m = $N::Rect($h); Near ([int](($m.Left + $m.Right) / 2)) ([int]($s.Width / 2)) 40 } 2000
    $m = $N::Rect($h)
    @{ Pass = $centred; Detail = "window middle at x $([int](($m.Left + $m.Right) / 2)) on a $($s.Width) px wide screen; shown: $(Test-Shown)" }
}

# a thin strip under the input's text moves it too, as on the mac; recentred after for the checks that follow
Invoke-Check '9d2' 'bottom edge drags the bar' $true {
    $h = Show-QCalc
    $r = $N::Rect($h)
    $x = [int](($r.Left + $r.Right) / 2); $y = $r.Bottom - 2
    $N::Drag($x, $y, $x - 180, $y - 120)
    $moved = Wait-Until { $m = $N::Rect($h); (Near $m.Left ($r.Left - 180)) -and (Near $m.Top ($r.Top - 120)) } 2000
    $m = $N::Rect($h)
    $stayed = Test-Shown
    $cx = [int](($m.Left + $m.Right) / 2)
    $N::Click($cx, $m.Top + 5)
    $N::Click($cx, $m.Top + 5)
    Start-Sleep -Milliseconds 400
    @{ Pass = ($moved -and $stayed); Detail = "dragged -180, -120 px from the bottom edge: moved $($m.Left - $r.Left), $($m.Top - $r.Top) px; still shown: $stayed" }
}

# either side edge makes the bar wider for a long calculation. the left one here: the right has the settings and
# clear buttons at its edge, which keep their clicks. put back after, so the later shots are the usual bar
Invoke-Check '9e' 'side edge widens the bar' $true {
    $h = Show-QCalc
    $r = $N::Rect($h)
    # near the bottom: the top corners, most of the way down a bare bar, move it instead
    $y = $r.Bottom - 10
    $N::Drag($r.Left + 3, $y, $r.Left - 147, $y)
    $wider = Wait-Until { $m = $N::Rect($h); (Near ($m.Right - $m.Left) ($r.Right - $r.Left + 150) 12) -and (Near $m.Right $r.Right 3) } 2000
    $m = $N::Rect($h)
    $grew = ($m.Right - $m.Left) - ($r.Right - $r.Left)
    Start-Sleep -Milliseconds 400
    # the bar itself, not only its window: the page's white reaches the new left edge
    $edge = $N::Pixel($m.Left + 12, $y)
    $inside = $N::Pixel($m.Right - 200, $y)
    $filled = (Distance $edge $inside) -le 12
    $stayed = Test-Shown
    $N::Drag($m.Left + 3, $y, $m.Left + 3 + $grew + 60, $y)
    $back = Wait-Until { $e = $N::Rect($h); Near ($e.Right - $e.Left) ($r.Right - $r.Left) 3 } 2000
    # the left edge's drag is a move too, so the spot it left is forgotten for the checks after
    $e = $N::Rect($h)
    $x = [int](($e.Left + $e.Right) / 2)
    $N::Click($x, $e.Top + 5)
    $N::Click($x, $e.Top + 5)
    Start-Sleep -Milliseconds 400
    @{ Pass = ($wider -and $filled -and $stayed -and $back)
       Detail = "dragged the left edge 150 px: $($r.Right - $r.Left) px wide, then $($m.Right - $m.Left) px, right edge moved $($m.Right - $r.Right) px; bar fills it: $filled ($(Hex $edge) at the new edge, $(Hex $inside) inside); still shown: $stayed; back to the standard width: $back" }
}

# for people to look at: the rounded edge, hairline, shadow and backdrop against light and dark
Invoke-Check '10' 'look in light and dark' $false {
    # the other window stays on top, and would sit in the settings shots
    if ($script:other -and -not $script:other.HasExited) { Stop-Process -Id $script:other.Id -Force }
    $script:other = $null; $script:otherHwnd = [IntPtr]::Zero
    $light = Save-Look 'look-light.png' 'f3f3f3' 'd0d7e2'
    $dark = Save-Look 'look-dark.png' '1c1c1c' '2f3a4c'
    # the runner's system theme is light, so the app's own dark setting is switched on for one shot
    try {
        Set-Appearance 'Dark' 'settings-dark.png'
        $themed = Save-Look 'look-dark-theme.png' '1c1c1c' '2f3a4c'
    } finally {
        try { Set-Appearance 'System' 'settings-system.png' } catch { Log "could not put the appearance back: $($_.Exception.Message)" }
    }
    @{ Pass = $true; Detail = "$light; $dark; $themed; tray route: $(Test-TraySettings)" }
}

# what the app saw when it dressed the window and what the page paints: tells the runner's limits from our bugs
Invoke-Check '11' 'look diagnostics' $false {
    $notes = @(if (Test-Path $lookNotes) { Get-Content $lookNotes })
    $notes | ForEach-Object { Log "look: $_" }
    @{ Pass = ($notes.Count -gt 0); Detail = $(if ($notes) { $notes -join ' / ' } else { "no notes at $lookNotes" }) }
}

# a hotkey the user picked: saved as alt+q, then a launch like the one at sign in has to answer it
Invoke-Check '12' 'a custom hotkey (Alt+Q) works after a launch' $true {
    try { Hide-QCalc } catch { }
    Get-Process -Name $ProcName -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Milliseconds 500
    $file = Join-Path $env:APPDATA 'com.maxconine.qcalc\settings.json'
    $json = if (Test-Path $file) { Get-Content -Raw $file | ConvertFrom-Json } else { New-Object psobject }
    $json | Add-Member -NotePropertyName hotkey -NotePropertyValue 'alt+q' -Force
    [IO.File]::WriteAllText($file, ($json | ConvertTo-Json -Depth 20))
    $script:proc = Start-Process -FilePath $Exe -ArgumentList '--autostart' -PassThru
    if (-not (Wait-Until { (Get-QCalcWindow) -ne [IntPtr]::Zero } 30000)) { return @{ Pass = $false; Detail = 'no window after the launch' } }
    Start-Sleep -Seconds 2
    $N::Chord($N::VK_MENU, $N::VK_Q)
    $shown = Wait-Until { Test-Shown } 3000
    @{ Pass = $shown; Detail = "settings file: $file; shown after Alt+Q: $shown" }
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
