# Engine parity through the app: a sample of the golden eval corpus (native/core/golden/eval.*.jsonl, the TS engine's
# own outputs) typed into the running native app with `qcalc.exe --bench <spec>` (WM_CHARs posted to the bar, the
# live answer awaited until painted), and what the app showed compared with each fixture's display. It proves the
# app's answer path (input rules, the live request, the engine thread, the slot) is the engine's.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File native\windows\tests\e2e\parity.ps1 -Exe native\target\release\qcalc.exe
#
# The sample is deterministic: lines evaluated with the app's own options (none, or degrees), no clock or randomness,
# spread evenly over each file. Exit 1 when fewer than -MinPercent of them match exactly.
param(
    [string]$Exe = (Join-Path $PSScriptRoot '..\..\..\target\release\qcalc.exe'),
    [string]$OutDir = (Join-Path (Get-Location) 'e2e-out'),
    [string]$Golden = (Join-Path $PSScriptRoot '..\..\..\core\golden'),
    [int]$PerFile = 40,
    [double]$MinPercent = 98
)
$ErrorActionPreference = 'Stop'
$Exe = (Resolve-Path $Exe).Path
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$OutDir = (Resolve-Path $OutDir).Path

# ---- the sample ----

# evaluateLine(text, {}) or (text, {angleMode: "deg"}): degrees as a fresh app has them; the app is set to the
# engine's 12 significant figures (the app's own default is 6), so the displays are comparable
$shape = [regex]'^\{"fn":"evaluateLine","args":\[("(?:[^"\\]|\\.)*"),(\{\}|\{"angleMode":"deg"\})\],"out":'
$skip = [regex]'(?i)\b(now|today|tomorrow|yesterday|time|date|random|rand|ans|last)\b'
$cases = New-Object System.Collections.Generic.List[object]
foreach ($file in Get-ChildItem $Golden -Filter 'eval.*.jsonl' | Sort-Object Name) {
    $lines = [IO.File]::ReadAllLines($file.FullName)
    $fit = New-Object System.Collections.Generic.List[string]
    $seen = @{}
    foreach ($l in $lines) {
        $m = $shape.Match($l)
        if (-not $m.Success) { continue }
        $text = $m.Groups[1].Value | ConvertFrom-Json
        if (-not $text -or $text -ne $text.Trim() -or $text -match '[\r\n\t]' -or $skip.IsMatch($text) -or $seen.ContainsKey($text)) { continue }
        $seen[$text] = $true
        $fit.Add($l)
    }
    if ($fit.Count -eq 0) { continue }
    $take = [Math]::Min($PerFile, $fit.Count)
    for ($k = 0; $k -lt $take; $k++) {
        $rec = $fit[[int][Math]::Floor($k * $fit.Count / $take)] | ConvertFrom-Json
        $out = $rec.out
        if (-not $out.display -or $out.kind -in @('empty', 'error')) { continue }
        $cases.Add([pscustomobject]@{ File = $file.BaseName; Text = [string]$rec.args[0]; Display = [string]$out.display; Kind = [string]$out.kind })
    }
}
Write-Host "sample: $($cases.Count) lines from $(@($cases | Select-Object -ExpandProperty File -Unique).Count) files"

# ---- through the app ----

$root = Join-Path ([IO.Path]::GetTempPath()) ("qcalc-parity-" + [Guid]::NewGuid().ToString('N').Substring(0, 8))
$data = Join-Path $root 'data'
New-Item -ItemType Directory -Force $data | Out-Null
[IO.File]::WriteAllText((Join-Path $data 'settings.json'), (@{ firstRunDone = $true; hotkey = 'ctrl+alt+shift+f10'; autoUpdate = $false; defaults = 2; settings = @{ shareUsage = $false; theme = 'light'; sigFigs = 12 } } | ConvertTo-Json -Depth 4))
$spec = Join-Path $root 'spec.json'
$result = Join-Path $root 'result.json'
[IO.File]::WriteAllText($spec, (@{ inputs = @($cases | ForEach-Object { $_.Text }); whole = $true; answers = $true; out = $result; quit = $true } | ConvertTo-Json -Depth 4))

$env:QCALC_INSTANCE = 'e2e-parity'
$env:QCALC_DATA = $data
$env:QCALC_LOG = Join-Path $root 'qcalc.log'
$sw = [Diagnostics.Stopwatch]::StartNew()
$p = Start-Process -FilePath $Exe -ArgumentList @('--bench', "`"$spec`"") -PassThru
if (-not $p.WaitForExit(600000)) { Stop-Process -Id $p.Id -Force; throw 'the bench run took over 10 minutes' }
if (-not (Test-Path $result)) { throw "no result from the app (log: $(Get-Content (Join-Path $root 'qcalc.log') -Tail 5 -ErrorAction SilentlyContinue))" }
$res = Get-Content -Raw -Encoding UTF8 $result | ConvertFrom-Json
Write-Host ("app ran {0} inputs in {1:n1} s; {2} timeouts" -f $cases.Count, ($sw.ElapsedMilliseconds / 1000), $res.timeouts)

$byInput = @{}
foreach ($a in $res.answers) { $byInput[[int]$a.input] = $a }
$match = 0
$bad = New-Object System.Collections.Generic.List[object]
for ($i = 0; $i -lt $cases.Count; $i++) {
    $c = $cases[$i]
    $a = $byInput[$i]
    $got = if ($a) { [string]$a.display } else { $null }
    if ($got -eq $c.Display) { $match++ }
    else { $bad.Add([pscustomobject]@{ file = $c.File; input = $c.Text; fixture = $c.Display; app = $got; shown = $(if ($a) { [string]$a.shown } else { $null }) }) }
}
$pct = if ($cases.Count) { 100.0 * $match / $cases.Count } else { 0 }
$bad | ConvertTo-Json -Depth 3 | Set-Content -Encoding utf8 (Join-Path $OutDir 'parity-mismatches.json')
Copy-Item (Join-Path $root 'qcalc.log') (Join-Path $OutDir 'parity-qcalc.log') -ErrorAction SilentlyContinue
Remove-Item $root -Recurse -Force -ErrorAction SilentlyContinue

$pass = $pct -ge $MinPercent -and $cases.Count -ge 100
$md = New-Object System.Text.StringBuilder
[void]$md.AppendLine('## Engine parity through the app')
[void]$md.AppendLine()
[void]$md.AppendLine(("**{0}: {1} of {2} golden lines ({3:n1}%) show the fixture's display exactly** (needs {4}%; {5} timeouts)." -f $(if ($pass) { 'pass' } else { 'FAIL' }), $match, $cases.Count, $pct, $MinPercent, $res.timeouts))
if ($bad.Count) {
    [void]$md.AppendLine()
    [void]$md.AppendLine('| file | input | fixture | app |')
    [void]$md.AppendLine('|---|---|---|---|')
    foreach ($b in $bad | Select-Object -First 25) { [void]$md.AppendLine("| $($b.file) | ``$($b.input -replace '\|', '\|')`` | $($b.fixture -replace '\|', '\|') | $($b.app -replace '\|', '\|') |") }
}
[IO.File]::WriteAllText((Join-Path $OutDir 'parity.md'), $md.ToString())
if ($env:GITHUB_STEP_SUMMARY) { [IO.File]::AppendAllText($env:GITHUB_STEP_SUMMARY, $md.ToString()) }
Write-Host $md.ToString()
if (-not $pass) { exit 1 }
exit 0
