<#
.SYNOPSIS
  Rebuilds the Tauri app's data fixtures (made-up data, nobody's real files) in the Tauri app's exact on-disk shapes:

  - settings.json: src-tauri/src/store.rs `Store`, serialised as it does (serde_json pretty, camelCase), with a
    changed setting of each kind, the shortcut, the bar's spot and width, onboarding and `signIn`.
  - leveldb\: WebView2's localStorage for the page (`%LOCALAPPDATA%\com.maxconine.qcalc\EBWebView\Default\Local
    Storage\leveldb`), written by Chromium itself (Edge, headless, a throwaway profile) for the origin
    http://tauri.localhost, the page's keys and values as src/lib/storage.ts, messaging.ts, nickname.ts and
    gameSaves.ts write them. Two runs, so the database holds a compacted table (.ldb) and a newer write-ahead log
    over it, as a profile that's been used a while does.

  Run it only to change the fixtures; the tests read the committed files.
    powershell -NoProfile -ExecutionPolicy Bypass -File native\windows\tests\fixtures\tauri\make.ps1
#>
param([string]$Edge = "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe")
$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
$now = 1793000000000 # ms, 2026-10-26

# ---- the page's localStorage ----

# src/lib/history.ts HistoryRow[], oldest first, as persistableHistory keeps it
$history = @(
    @{ id = 'mg8k1a-x1'; expr = '12 kg to lb'; display = '26.4555 lb'; n = 26.455471462185; quantity = '26.455471462185 lb'; at = $now - 900000 }
    @{ id = 'mg8k2b-x2'; expr = 'r = 5 m'; display = '5 m'; n = 5; quantity = '5 m'; kind = 'definition'; at = $now - 600000 }
    @{ id = 'mg8k3c-x3'; expr = 'f(x) = x^2 + 1'; display = 'f(x) = x^2 + 1'; kind = 'function'; fnName = 'f'; fnParams = @('x'); fnBody = 'x^2 + 1'; at = $now - 300000 }
    @{ id = 'mg8k4d-x4'; expr = "2$([char]0xD7)$([char]0x3C0)"; display = '6.28319'; exact = "2$([char]0x3C0)"; n = 6.283185307179586; at = $now - 60000 }
)
# an older tape the second run replaces (the table keeps it, the log's newer write wins)
$older = @(@{ id = 'mg7z0a-x0'; expr = '1+1'; display = '2'; n = 2; at = $now - 86400000 })
$messaging = @{ name = 'ada_l'; key = ('5f' * 32); accepted = @('grace_h'); blocked = @('spam_bot'); recent = @('grace_h') }
# the page's own mirror of its settings (the Tauri app's settings.json is what counts)
$pageSettings = @{ angleMode = 'deg'; sigFigs = 6; theme = 'system' }

function J($v) { ConvertTo-Json -InputObject $v -Depth 6 -Compress }

$first = [ordered]@{
    'qcalc-history'    = (J $older)
    'qcalc-settings'   = (J $pageSettings)
    'qcalc-onboarding' = (J @{ opens = 3; commits = 2; hints = 0; done = $false })
    'qcalc-filler'     = ('x' * 1200000)
}
$second = [ordered]@{
    'qcalc-history'   = (J $history)
    'qcalc-messaging' = (J $messaging)
    'qcalc-nickname'  = 'teal otter'
    'qcalc-crack'     = '7'
    'qcalc-equal'     = (J @{ streak = 4; best = 9 })
    'qcalc-draft'     = (J @{ expr = '3x+'; savedAt = $now - 30000 })
}

function Page($set, [string[]]$remove = @()) {
    $lines = foreach ($k in $set.Keys) { "localStorage.setItem($(J $k), $(J $set[$k]));" }
    $lines += foreach ($k in $remove) { "localStorage.removeItem($(J $k));" }
    "<!doctype html><meta charset=utf-8><title>fixture</title><script>$($lines -join "`n")</script>"
}

# serves one page to Edge at http://tauri.localhost:<port>/ (mapped to 127.0.0.1)
function Run-Edge([string]$userData, [string]$html) {
    # one port for both runs: the origin (http://tauri.localhost:<port>) must stay the same, as the app's did
    $listener = New-Object System.Net.Sockets.TcpListener([Net.IPAddress]::Loopback, $script:port)
    $listener.Start()
    $port = $listener.LocalEndpoint.Port
    $script:port = $port
    $body = [Text.Encoding]::UTF8.GetBytes($html)
    $edgeArgs = @('--headless=new', "--user-data-dir=`"$userData`"", '--no-first-run', '--no-default-browser-check', '--disable-sync',
        '--disable-extensions', '--disable-background-networking', '--disable-component-update', '--no-proxy-server',
        "--host-resolver-rules=`"MAP tauri.localhost 127.0.0.1`"", '--virtual-time-budget=3000', '--dump-dom', "http://tauri.localhost:$port/")
    $p = Start-Process -FilePath $Edge -ArgumentList $edgeArgs -PassThru -WindowStyle Hidden -RedirectStandardOutput (Join-Path $userData '..\dom.txt')
    $sw = [Diagnostics.Stopwatch]::StartNew()
    while (-not $p.HasExited -and $sw.ElapsedMilliseconds -lt 30000) {
        if ($listener.Pending()) {
            $c = $listener.AcceptTcpClient()
            $s = $c.GetStream()
            $buf = New-Object byte[] 8192
            [void]$s.Read($buf, 0, $buf.Length)
            $head = [Text.Encoding]::ASCII.GetBytes("HTTP/1.1 200 OK`r`nContent-Type: text/html; charset=utf-8`r`nContent-Length: $($body.Length)`r`nConnection: close`r`n`r`n")
            $s.Write($head, 0, $head.Length); $s.Write($body, 0, $body.Length); $s.Flush(); $c.Close()
        } else { Start-Sleep -Milliseconds 20 }
    }
    $listener.Stop()
    if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force; throw 'edge did not finish' }
}

$script:port = 0
$work = Join-Path ([IO.Path]::GetTempPath()) ('qcalc-fixture-' + [Guid]::NewGuid().ToString('N').Substring(0, 8))
$userData = Join-Path $work 'profile'
New-Item -ItemType Directory -Force $userData | Out-Null
try {
    Run-Edge $userData (Page $first)
    Run-Edge $userData (Page $second @('qcalc-filler'))
    $db = Join-Path $userData 'Default\Local Storage\leveldb'
    if (-not (Test-Path $db)) { throw "no leveldb at $db" }
    $out = Join-Path $here 'leveldb'
    Remove-Item $out -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force $out | Out-Null
    # the database's files; LOCK is empty, and LOG / LOG.old are Chromium's text logs (they name this machine's paths)
    Get-ChildItem $db -File | Where-Object { $_.Name -notin 'LOCK', 'LOG', 'LOG.old' } | Copy-Item -Destination $out
    Get-ChildItem $out | Format-Table Name, Length
} finally {
    Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue
}

# ---- settings.json: src-tauri/src/store.rs Store, field order as it serialises ----
$settings = @'
{
  "settings": {
    "angleMode": "rad",
    "fractionMode": true,
    "sigFigMode": false,
    "rationalize": true,
    "keepWords": false,
    "answerForm": "exact",
    "historyInsert": "answer",
    "historyShow": "always",
    "sigFigs": 9,
    "draftSeconds": 120,
    "defaultUnits": {
      "length": "ft"
    },
    "theme": "dark",
    "typstPreview": true,
    "typstCopy": false,
    "spaceThousands": true,
    "copyUnitless": true,
    "shareUsage": false,
    "keybinds": {
      "angle": "ctrl+alt+r"
    }
  },
  "hotkey": "ctrl+alt+shift+f10",
  "onboarding": {
    "opens": 41,
    "commits": 117,
    "hints": 2047,
    "done": true
  },
  "firstRunDone": true,
  "positions": {
    "\\\\.\\DISPLAY1": [
      412.0,
      180.0
    ]
  },
  "spotSeconds": 0,
  "usedAt": 1792999000,
  "width": 900.0,
  "analytics": null,
  "defaults": 2,
  "autoUpdate": false,
  "signIn": true
}
'@
[IO.File]::WriteAllText((Join-Path $here 'settings.json'), ($settings -replace "`r`n", "`n"))
'settings.json written'
