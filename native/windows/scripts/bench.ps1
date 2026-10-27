<#
.SYNOPSIS
  Drives the native Windows app over its bench pipe (\.\pipe\qcalc-bench) and prints the numbers the targets
  are about. The app must run with --bench-pipe (or QCALC_BENCH=1); -Launch starts it itself.

.EXAMPLE
  .\bench.ps1                                  # keystroke -> painted answer over the default inputs
  .\bench.ps1 -Inputs '2+2','sin(90)' -Enter   # type, commit each
  .\bench.ps1 -Mem                             # memory after a hide and the trim
  .\bench.ps1 -Launch -Exe ..\..\target\release\qcalc.exe   # warm launch (5 runs) from the log's ready line
  .\bench.ps1 -Cmd state                       # any pipe command: show, hide, state, mem, trim, quit
#>
param(
  [string[]]$Inputs = @('2+2', 'sin(90)', '72 f to c', '1/3 + 1/6', 'sqrt(2)*pi', '5 km in miles', '3x+2=11',
                        '(1+2)*(3+4)/5^2', '20% of 80', 'e^(i pi)+1'),
  [int]$Rounds = 3,
  [switch]$Enter,
  [switch]$Mem,
  [switch]$Launch,
  [string]$Exe = "$PSScriptRoot\..\..\target\release\qcalc.exe",
  [string]$Cmd,
  [string]$Text
)

function Send-QCalc([hashtable]$Request, [int]$TimeoutMs = 120000) {
  $pipe = New-Object System.IO.Pipes.NamedPipeClientStream('.', 'qcalc-bench', [System.IO.Pipes.PipeDirection]::InOut)
  $pipe.Connect(5000)
  $writer = New-Object System.IO.StreamWriter($pipe)
  $writer.AutoFlush = $true
  $writer.WriteLine(($Request | ConvertTo-Json -Compress -Depth 5))
  $reader = New-Object System.IO.StreamReader($pipe)
  $line = $reader.ReadLine()
  $pipe.Dispose()
  return $line | ConvertFrom-Json
}

function Private-WorkingSetMB([int]$ProcessId) {
  $p = Get-CimInstance Win32_PerfFormattedData_PerfProc_Process | Where-Object { $_.IDProcess -eq $ProcessId }
  if ($p) { return [math]::Round($p.WorkingSetPrivate / 1MB, 1) }
  return $null
}

if ($Cmd) {
  $req = @{ cmd = $Cmd }
  if ($Text) { $req.text = $Text }
  Send-QCalc $req | ConvertTo-Json -Depth 5
  return
}

if ($Launch) {
  $log = Join-Path $env:TEMP 'qcalc-launch.log'
  $times = @()
  for ($i = 0; $i -lt 5; $i++) {
    Remove-Item $log -ErrorAction SilentlyContinue
    $env:QCALC_LOG = $log
    $p = Start-Process -FilePath $Exe -PassThru
    $deadline = (Get-Date).AddSeconds(10)
    $ready = $null
    while (-not $ready -and (Get-Date) -lt $deadline) {
      Start-Sleep -Milliseconds 100
      if (Test-Path $log) { $ready = Select-String -Path $log -Pattern 'ready \(.*\) at \+([\d.]+) ms' | Select-Object -First 1 }
    }
    if ($ready) { $times += [double]$ready.Matches[0].Groups[1].Value }
    Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 300
  }
  Remove-Item Env:QCALC_LOG
  $sorted = $times | Sort-Object
  "warm launch to ready (ms since process creation): runs $($times -join ', ') ; median $($sorted[[int][math]::Floor($sorted.Count / 2)])"
  return
}

if ($Mem) {
  $state = Send-QCalc @{ cmd = 'state' }
  $proc = Get-Process qcalc | Where-Object { $_.Path -like '*target*' -or $_.Path -like '*Q Calc*' } | Select-Object -First 1
  Send-QCalc @{ cmd = 'hide' } | Out-Null
  Start-Sleep -Milliseconds 500
  $before = Private-WorkingSetMB $proc.Id
  $t = Send-QCalc @{ cmd = 'trim' }
  Start-Sleep -Milliseconds 500
  $after = Private-WorkingSetMB $proc.Id
  "private working set (Task Manager 'Memory'): hidden $before MB, after the trim $after MB; private bytes $([math]::Round($t.private / 1MB, 1)) MB"
  return
}

$all = @()
for ($r = 0; $r -lt $Rounds; $r++) {
  $res = Send-QCalc @{ inputs = $Inputs; enter = [bool]$Enter }
  $all += $res.ms
  "round $($r + 1): n $($res.n) median $([math]::Round($res.median, 3)) ms p90 $([math]::Round($res.p90, 3)) ms p99 $([math]::Round($res.p99, 3)) ms timeouts $($res.timeouts)"
}
$s = $all | Sort-Object
$pct = { param($p) $s[[math]::Max(0, [math]::Ceiling($p / 100 * $s.Count) - 1)] }
"all: n $($s.Count) median $([math]::Round((& $pct 50), 3)) ms p90 $([math]::Round((& $pct 90), 3)) ms p99 $([math]::Round((& $pct 99), 3)) ms"
