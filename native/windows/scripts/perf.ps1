<#
.SYNOPSIS
  The three targets for one or more builds, interleaved (A B A B ...), as native/windows/PERFORMANCE.md measures
  them. Each run copies a returning user's data folder, then:
    1. launches with --autostart and waits 10 s: memory never shown;
    2. shows the bar and types the standard inputs (bench.rs) -Rounds times, then the 269 golden inputs
       (native/bench/inputs.json), each a key at a time: keystroke -> answer painted, median / p90 / p99;
    3. uses the bar as a person does (each standard input typed, a 0.9 s pause, Enter), hides it, and reads memory
       1 s and 10 s later (before the 30 s trim);
    4. quits and launches -Launches times: warm launch to the first frame (the log's ready line).
  Memory is the private working set (Task Manager's "Memory") and private bytes (commit), from
  GetProcessMemoryInfo. Every copy runs as its own QCALC_INSTANCE with its own QCALC_DATA and log.

.EXAMPLE
  .\perf.ps1 -Exes ..\..\target\release\qcalc.exe
  .\perf.ps1 -Exes base.exe,new.exe -Runs 3 -NoGolden
#>
param(
  [string[]]$Exes = @("$PSScriptRoot\..\..\target\release\qcalc.exe"),
  [int]$Runs = 1,
  [int]$Rounds = 3,
  [int]$Launches = 3,
  [switch]$NoGolden,
  [string]$Work = (Join-Path $env:TEMP 'qcalc-perf')
)
$ErrorActionPreference = 'Stop'
$Std = @('2+2', 'sin(90)', '72 f to c', '1/3 + 1/6', 'sqrt(2)*pi', '5 km in miles', '3x+2=11', '(1+2)*(3+4)/5^2',
  '20% of 80', 'e^(i pi)+1')
$Golden = Get-Content -Raw "$PSScriptRoot\..\..\bench\inputs.json" | ConvertFrom-Json

if (-not ('QCalcPerfMem' -as [type])) {
  Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices;
public static class QCalcPerfMem {
  [StructLayout(LayoutKind.Sequential)] struct C { public uint cb, faults; public UIntPtr peakWs, ws, a, b, c, d, pagefile, peakPagefile, priv, privWs, shared; }
  [DllImport("psapi.dll")] static extern bool GetProcessMemoryInfo(IntPtr h, ref C c, uint cb);
  [DllImport("kernel32.dll")] static extern IntPtr OpenProcess(uint a, bool i, int pid);
  [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
  // MB: private working set, private bytes
  public static double[] Get(int pid) {
    IntPtr h = OpenProcess(0x1010, false, pid); var c = new C(); c.cb = (uint)Marshal.SizeOf(typeof(C));
    bool ok = GetProcessMemoryInfo(h, ref c, c.cb); CloseHandle(h);
    return ok ? new[] { c.privWs.ToUInt64() / 1048576.0, c.priv.ToUInt64() / 1048576.0 } : new[] { 0.0, 0.0 };
  }
}
'@
}

function Send-Q([string]$Inst, [hashtable]$Req) {
  $p = New-Object System.IO.Pipes.NamedPipeClientStream('.', "qcalc-bench-$Inst", [System.IO.Pipes.PipeDirection]::InOut)
  $p.Connect(10000)
  $w = New-Object System.IO.StreamWriter($p); $w.AutoFlush = $true
  $w.WriteLine(($Req | ConvertTo-Json -Compress -Depth 5))
  $line = (New-Object System.IO.StreamReader($p)).ReadLine(); $p.Dispose()
  $line | ConvertFrom-Json
}

function Start-Q([string]$Exe, [string]$Inst, [string]$Data, [string[]]$ArgList) {
  for ($try = 0; $try -lt 6; $try++) {
    $log = Join-Path $Work "$Inst.log"; Remove-Item $log -ErrorAction SilentlyContinue
    $env:QCALC_INSTANCE = $Inst; $env:QCALC_DATA = $Data; $env:QCALC_LOG = $log; $env:QCALC_BENCH = '1'
    $p = if ($ArgList) { Start-Process $Exe -ArgumentList $ArgList -PassThru } else { Start-Process $Exe -PassThru }
    Remove-Item Env:QCALC_INSTANCE, Env:QCALC_DATA, Env:QCALC_LOG, Env:QCALC_BENCH
    $deadline = (Get-Date).AddSeconds(8); $ready = $null
    while (-not $ready -and (Get-Date) -lt $deadline) {
      Start-Sleep -Milliseconds 50
      if (Test-Path $log) { $ready = Select-String -Path $log -Pattern 'ready \(.*\) at \+([\d.]+) ms' | Select-Object -First 1 }
    }
    if ($ready) { return [pscustomobject]@{ p = $p; inst = $Inst; ready = [double]$ready.Matches[0].Groups[1].Value } }
    # the last copy of this instance was still on its way out, and its mutex handed this one off
    $p.WaitForExit(2000) | Out-Null; Start-Sleep -Seconds 1
  }
  throw "$Exe didn't start"
}

function Stop-Q($q) {
  try { Send-Q $q.inst @{ cmd = 'quit' } | Out-Null } catch {}
  if (-not $q.p.WaitForExit(3000)) { Stop-Process -Id $q.p.Id -Force -ErrorAction SilentlyContinue }
}

function Pct($v, $p) { $s = @($v | Sort-Object); if (-not $s.Count) { return $null }; $s[[math]::Max(0, [math]::Ceiling($p / 100 * $s.Count) - 1)] }
function Mem($q) { [QCalcPerfMem]::Get($q.p.Id) }

# a returning user: past the tour, with twenty rows of history
New-Item -ItemType Directory -Force $Work | Out-Null
$fixture = Join-Path $Work 'fixture'
if (-not (Test-Path "$fixture\history.json")) {
  New-Item -ItemType Directory -Force $fixture | Out-Null
  $store = @{ hotkey = 'ctrl+alt+shift+f11'; firstRunDone = $true; autoUpdate = $false
    settings = @{ theme = 'light'; shareUsage = $false; draftSeconds = 60 }
    onboarding = @{ opens = 40; commits = 40; hints = 65535; done = $true } }
  [IO.File]::WriteAllText("$fixture\settings.json", ($store | ConvertTo-Json -Depth 6))
  $q = Start-Q $Exes[0] 'perf-fixture' $fixture @()
  Send-Q $q.inst @{ inputs = @('2+2', 'x = 5', 'x^2 + 3', 'sin(30)', '72 f to c', 'f(t) = t^2 + 1', 'f(3)', '1/3 + 1/6',
    'sqrt(2)*pi', '5 km in miles', '20% of 80', 'ans * 2', '3x+2=11', 'y = 12 kg', 'y to lb', '(1+2)*(3+4)/5^2',
    'e^(i pi)+1', '6!', 'log(1000)', '0.1 + 0.2'); enter = $true; whole = $true } | Out-Null
  Stop-Q $q
}

$rows = @()
for ($run = 0; $run -lt $Runs; $run++) {
  foreach ($exe in $Exes) {
    $name = [IO.Path]::GetFileNameWithoutExtension($exe)
    $inst = "perf-$name"
    $data = Join-Path $Work "data-$name"
    Remove-Item -Recurse -Force $data -ErrorAction SilentlyContinue
    Copy-Item -Recurse $fixture $data
    $o = [ordered]@{ exe = $name }
    $q = Start-Q $exe $inst $data @('--autostart')
    Start-Sleep -Seconds 10
    $m = Mem $q; $o.neverShownPws = $m[0]; $o.neverShownPriv = $m[1]
    Send-Q $q.inst @{ cmd = 'show' } | Out-Null; Start-Sleep -Milliseconds 500
    $ms = @(); for ($r = 0; $r -lt $Rounds; $r++) { $ms += (Send-Q $q.inst @{ inputs = $Std }).ms }
    $o.stdMed = Pct $ms 50; $o.stdP90 = Pct $ms 90; $o.stdP99 = Pct $ms 99
    if (-not $NoGolden) {
      $g = (Send-Q $q.inst @{ inputs = @($Golden) }).ms
      $o.goldMed = Pct $g 50; $o.goldP90 = Pct $g 90; $o.goldP99 = Pct $g 99
    }
    foreach ($t in $Std) {
      Send-Q $q.inst @{ cmd = 'type'; text = $t } | Out-Null; Start-Sleep -Milliseconds 900
      Send-Q $q.inst @{ cmd = 'key'; vk = 13 } | Out-Null; Start-Sleep -Milliseconds 300
    }
    Send-Q $q.inst @{ cmd = 'hide' } | Out-Null
    Start-Sleep -Seconds 1; $m = Mem $q; $o.hidden1sPws = $m[0]; $o.hidden1sPriv = $m[1]
    Start-Sleep -Seconds 9; $m = Mem $q; $o.hidden10sPws = $m[0]; $o.hidden10sPriv = $m[1]
    Stop-Q $q
    $l = @(); for ($i = 0; $i -lt $Launches; $i++) { $q = Start-Q $exe "$inst-l" $data @(); $l += $q.ready; Start-Sleep -Milliseconds 300; Stop-Q $q }
    $o.launchMed = Pct $l 50
    foreach ($k in @($o.Keys)) { if ($o[$k] -is [double]) { $o[$k] = [math]::Round($o[$k], 3) } }
    $row = [pscustomobject]$o; $rows += $row
    $row | ConvertTo-Json -Compress
  }
}
''
foreach ($exe in $Exes) {
  $name = [IO.Path]::GetFileNameWithoutExtension($exe)
  $r = $rows | Where-Object { $_.exe -eq $name }
  $line = "${name}:"
  foreach ($k in 'stdMed', 'stdP90', 'stdP99', 'goldMed', 'goldP90', 'goldP99', 'neverShownPws', 'neverShownPriv', 'hidden10sPws', 'hidden10sPriv', 'launchMed') {
    $v = @($r | ForEach-Object { $_.$k } | Where-Object { $null -ne $_ })
    if ($v.Count) { $line += " $k=$([math]::Round((Pct $v 50), 3))" }
  }
  $line
}
