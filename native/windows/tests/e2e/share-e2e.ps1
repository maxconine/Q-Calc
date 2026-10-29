# End-to-end messaging and sharing between two copies of the native Windows app, through the relay (relay/src,
# unchanged) run locally by `wrangler dev` (Miniflare: offline, local durable objects and bucket; relay-e2e.toml).
# Never the real relay: both copies get QCALC_RELAY_URL=http://127.0.0.1:<port>, which relay.rs takes only for a
# loopback address, and the names claimed exist only in this run's local state.
#
# Each copy is isolated as smoke.ps1's is (QCALC_INSTANCE, QCALC_DATA, QCALC_LOG, a hotkey nobody uses, and
# QCALC_DOWNLOADS for what Save writes), driven over its bench pipe (`share {…}`, share_debug.rs). Checks: text, a
# clip and files over 1 MiB (several chunks) both ways, byte for byte with SHA-256 and the Mark of the Web; key
# pinning, a changed key held until Trust New Key; an upload that survives the relay going away mid-upload (a part
# tried again on its own, and Retry after the tries run out); the Mac's construction both ways (share-vectors.mjs);
# the thread (lines, a kept reply with the caret after it, sending from it), drag hints, name completion, a copied
# image as clipboard.png, a shortcut sending its target, and the privacy notice.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File native\windows\tests\e2e\share-e2e.ps1 -Exe native\target\debug\qcalc.exe
#
# -Wrangler: wrangler's bin\wrangler.js (else wrangler@4.151.0 is installed once into %TEMP%\qcalc-share-e2e-wrangler).
# Exit code 1 when a check fails.
param(
    [string]$Exe = (Join-Path $PSScriptRoot '..\..\..\target\release\qcalc.exe'),
    [string]$Wrangler,
    [int]$Port = 8787,
    [string]$OutDir = (Join-Path (Get-Location) 'share-e2e-out')
)
$ErrorActionPreference = 'Stop'
$Exe = (Resolve-Path $Exe).Path
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$OutDir = (Resolve-Path $OutDir).Path
Start-Transcript -Path (Join-Path $OutDir 'log.txt') -Force | Out-Null

Add-Type -TypeDefinition (Get-Content -Raw (Join-Path $PSScriptRoot 'Native.cs')) -ReferencedAssemblies System.Drawing, System.Core
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
$N = [QCalcE2E.Native]
# keys only ever go to the app under test (this suite drives the pipe, but the guard stays as smoke.ps1 keeps it)
$N::GuardExe = $Exe

$Root = Join-Path ([IO.Path]::GetTempPath()) ("qcalc-share-e2e-{0}" -f [Guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory -Force $Root | Out-Null
$RelayState = Join-Path $Root 'relay'
$Relay = "http://127.0.0.1:$Port"
$script:results = New-Object System.Collections.Generic.List[object]
$script:mine = New-Object System.Collections.Generic.List[int]
$script:relayProc = $null

function Log([string]$msg) { Write-Host ("[{0:HH:mm:ss.fff}] {1}" -f (Get-Date), $msg) }

function Wait-Until([scriptblock]$__until, [int]$__ms = 5000) {
    $__sw = [Diagnostics.Stopwatch]::StartNew()
    do {
        try { if (& $__until) { return $true } } catch {}
        Start-Sleep -Milliseconds 100
    } while ($__sw.ElapsedMilliseconds -lt $__ms)
    try { return [bool](& $__until) } catch { return $false }
}

function Check([string]$id, [string]$what, [bool]$ok, [string]$detail = '') {
    $script:results.Add([pscustomobject]@{ id = $id; what = $what; ok = $ok; detail = $detail })
    $mark = if ($ok) { 'PASS' } else { 'FAIL' }
    Log "$mark $id $what $detail"
}

# ---- the relay ----

function Find-Wrangler {
    if ($Wrangler) { return (Resolve-Path $Wrangler).Path }
    $dir = Join-Path ([IO.Path]::GetTempPath()) 'qcalc-share-e2e-wrangler'
    $js = Join-Path $dir 'node_modules\wrangler\bin\wrangler.js'
    if (-not (Test-Path $js)) {
        New-Item -ItemType Directory -Force $dir | Out-Null
        Log "installing wrangler@4.151.0 into $dir (once)"
        Push-Location $dir
        # npm's warnings go to stderr, which -ErrorAction Stop would treat as a failure: judge by the file instead
        $eap = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
        try {
            & npm init -y 2>&1 | Out-Null
            & npm install --no-audit --no-fund wrangler@4.151.0 2>&1 | Out-Null
        } finally { $ErrorActionPreference = $eap; Pop-Location }
        if (-not (Test-Path $js)) { throw "wrangler didn't install into $dir" }
    }
    $js
}

function Test-Relay {
    try {
        $r = Invoke-WebRequest -UseBasicParsing -TimeoutSec 3 -Method Post "$Relay/lookup" -Body '{"from":"x","key":"y","name":"z"}'
        return $r.StatusCode -eq 200
    } catch { return $false }
}

function Start-Relay {
    $js = Find-Wrangler
    $cfg = Join-Path $PSScriptRoot 'relay-e2e.toml'
    $env:WRANGLER_SEND_METRICS = 'false'
    $env:CI = '1'
    # node directly (not npx), so the whole tree can be stopped by its pid; state persists across a restart
    # in the run's own folder: wrangler keeps its build (.wrangler) where it starts
    $script:relayProc = Start-Process node -PassThru -WindowStyle Hidden -WorkingDirectory $Root `
        -RedirectStandardOutput (Join-Path $OutDir 'relay.log') -RedirectStandardError (Join-Path $OutDir 'relay.err') `
        -ArgumentList @("`"$js`"", 'dev', '--config', "`"$cfg`"", '--ip', '127.0.0.1', '--port', "$Port", '--persist-to', "`"$RelayState`"")
    if (-not (Wait-Until { Test-Relay } 90000)) { throw "the local relay didn't start (see relay.log)" }
    Log "relay up on $Relay (pid $($script:relayProc.Id))"
}

function Stop-Relay {
    if ($script:relayProc -and -not $script:relayProc.HasExited) {
        & taskkill /T /F /PID $script:relayProc.Id 2>$null | Out-Null
        $script:relayProc.WaitForExit(10000) | Out-Null
    }
    Wait-Until { -not (Test-Relay) } 10000 | Out-Null
    Log 'relay down'
}

# ---- the two copies ----

function New-Copy([string]$Name, [string]$Chord, [int]$SlowMs = 0) {
    $dir = Join-Path $Root $Name
    $c = [pscustomobject]@{
        Name = $Name; Instance = "share-e2e-$Name"; Data = (Join-Path $dir 'data'); Log = (Join-Path $dir 'qcalc.log')
        Downloads = (Join-Path $dir 'downloads'); Chord = $Chord; Slow = $SlowMs; Proc = $null
    }
    New-Item -ItemType Directory -Force $c.Data, $c.Downloads | Out-Null
    # files ask for Save until the auto-accept checks turn it on (`prefs`)
    $store = @{ hotkey = $Chord; firstRunDone = $true; autoUpdate = $false; settings = @{ theme = 'light'; shareUsage = $false; autoAcceptBytes = 0 } }
    [IO.File]::WriteAllText((Join-Path $c.Data 'settings.json'), ($store | ConvertTo-Json -Depth 6))
    $c
}

function Start-Copy($c) {
    $env:QCALC_INSTANCE = $c.Instance
    $env:QCALC_DATA = $c.Data
    $env:QCALC_LOG = $c.Log
    $env:QCALC_DOWNLOADS = $c.Downloads
    $env:QCALC_RELAY_URL = $Relay
    $env:QCALC_BENCH = '1'
    $env:QCALC_E2E_SLOW_UPLOAD_MS = "$($c.Slow)"
    $c.Proc = Start-Process -FilePath $Exe -ArgumentList '--autostart' -PassThru
    $script:mine.Add($c.Proc.Id)
    if (-not (Wait-Until { $null -ne (Pipe $c @{ cmd = 'state' }) } 20000)) { throw "$($c.Name)'s pipe never answered" }
    Log "$($c.Name) up (pid $($c.Proc.Id))"
}

function Stop-Copy($c) {
    if (-not $c.Proc -or $c.Proc.HasExited) { return }
    Pipe $c @{ cmd = 'quit' } | Out-Null
    if (-not $c.Proc.WaitForExit(5000)) { Stop-Process -Id $c.Proc.Id -Force -ErrorAction SilentlyContinue }
}

function Pipe($c, [hashtable]$Request, [int]$TimeoutMs = 3000) {
    $pipe = New-Object System.IO.Pipes.NamedPipeClientStream('.', "qcalc-bench-$($c.Instance)", [System.IO.Pipes.PipeDirection]::InOut)
    try {
        $pipe.Connect($TimeoutMs)
        $w = New-Object System.IO.StreamWriter($pipe)
        $w.AutoFlush = $true
        $w.WriteLine(($Request | ConvertTo-Json -Compress -Depth 8))
        $line = (New-Object System.IO.StreamReader($pipe)).ReadLine()
        if ($line) { return $line | ConvertFrom-Json }
        return $null
    } catch { return $null } finally { $pipe.Dispose() }
}

# `share {…}` on a copy
function Share($c, [hashtable]$Op) { Pipe $c @{ cmd = ('share ' + ($Op | ConvertTo-Json -Compress -Depth 8)) } 10000 }
function St($c) { Share $c @{ op = 'state' } }
function Thread-Of($c, [string]$who) { $s = St $c; if ($s.threads.threads.PSObject.Properties[$who]) { @($s.threads.threads.$who) } else { @() } }
function Transfer-Named($c, [string]$name, [bool]$mine) { @((St $c).share.transfers | Where-Object { $_.name -eq $name -and $_.mine -eq $mine }) | Select-Object -Last 1 }
function Sha([string]$path) { (Get-FileHash -Algorithm SHA256 $path).Hash.ToLower() }

function New-Blob([string]$path, [int]$bytes) {
    $b = New-Object byte[] $bytes
    (New-Object Random 7).NextBytes($b)
    [IO.File]::WriteAllBytes($path, $b)
    $path
}

# a file sent from $from to $to, saved there and compared; the saved path
function Send-And-Save($from, $to, [string]$file, [string]$id) {
    $name = Split-Path $file -Leaf
    Share $from @{ op = 'files'; to = $to.User; paths = @($file) } | Out-Null
    $got = Wait-Until { (Transfer-Named $to $name $false).state -eq 'received' } 30000
    Check $id "$($from.Name) offers $name to $($to.Name)" $got
    if (-not $got) { return $null }
    $t = Transfer-Named $to $name $false
    Share $to @{ op = 'save'; id = $t.id } | Out-Null
    $saved = Wait-Until { (Transfer-Named $to $name $false).state -eq 'saved' } 60000
    $sent = Wait-Until { (Transfer-Named $from $name $true).state -eq 'sent' } 10000
    $path = Join-Path $to.Downloads $name
    $same = $saved -and (Test-Path $path) -and ((Sha $path) -eq (Sha $file)) -and ((Get-Item $path).Length -eq (Get-Item $file).Length)
    $motw = $saved -and ((Get-Content -Path $path -Stream Zone.Identifier -ErrorAction SilentlyContinue) -join "`n") -match 'ZoneId=3'
    Check $id "$name saved byte for byte (sha-256), the sender says sent" ($same -and $sent) "$((Get-Item $file).Length) bytes"
    Check "$id-motw" "$name carries the Mark of the Web" $motw
    $path
}

$A = New-Copy 'a' 'ctrl+alt+shift+f10' 1500
$B = New-Copy 'b' 'ctrl+alt+shift+f11'
$A | Add-Member User 'alice-e2e'
$B | Add-Member User 'bob-e2e'
$clipBefore = $null

try {
    Start-Relay
    Start-Copy $A
    Start-Copy $B

    # ---- names, links ----
    Share $A @{ op = 'name'; name = $A.User } | Out-Null
    Share $B @{ op = 'name'; name = $B.User } | Out-Null
    $ready = Wait-Until { (St $A).messaging.link -eq 'Ready' -and (St $B).messaging.link -eq 'Ready' } 20000
    Check 'N1' 'both copies claim a name on the local relay and their inboxes open' $ready
    if (-not $ready) { throw 'no link' }
    Share $A @{ op = 'allow'; who = $B.User } | Out-Null
    Share $B @{ op = 'allow'; who = $A.User } | Out-Null

    # ---- the privacy notice, completion ----
    Share $B @{ op = 'line'; text = 'message ali' } | Out-Null
    Check 'C1' 'message <name> finishes a name (Tab)' (Wait-Until { (St $B).completion -eq 'ce-e2e' } 5000) "$((St $B).completion)"

    # ---- text both ways ----
    Share $A @{ op = 'text'; to = $B.User; text = 'hello bob · 1' } | Out-Null
    Check 'T1' 'a message goes alice → bob and lands in his history' (Wait-Until { (Thread-Of $B $A.User | Where-Object { $_.dir -eq 'received' -and $_.text -eq 'hello bob · 1' }) } 10000)
    Share $B @{ op = 'text'; to = $A.User; text = 'hi alice · 2' } | Out-Null
    Check 'T2' 'a message goes bob → alice, and the sent one is in his thread' (Wait-Until { (Thread-Of $A $B.User | Where-Object { $_.text -eq 'hi alice · 2' }) -and (Thread-Of $B $A.User | Where-Object { $_.dir -eq 'sent' -and $_.text -eq 'hi alice · 2' }) } 10000)

    foreach ($c in 'paste to ali', 'send file to ali', 'messages ali') {
        Share $B @{ op = 'line'; text = $c } | Out-Null
        Check 'C2' "'$c' finishes the name" (Wait-Until { (St $B).completion -eq 'ce-e2e' } 5000) "$((St $B).completion)"
    }
    Share $B @{ op = 'line'; text = '' } | Out-Null

    # ---- clips both ways ----
    $clipAB = "from alice ✓`nsecond line"
    Share $A @{ op = 'clip'; to = $B.User; text = $clipAB } | Out-Null
    Check 'P1' 'a clip goes alice → bob, sealed, and opens there' (Wait-Until { (Thread-Of $B $A.User | Where-Object { $_.clip -eq $clipAB }) } 15000)
    Check 'P0' 'the privacy notice shows at the first share' ((St $A).threads.notice -eq $true)
    $clipBA = 'from bob · 3×4=12'
    Share $B @{ op = 'clip'; to = $A.User; text = $clipBA } | Out-Null
    Check 'P2' 'a clip goes bob → alice' (Wait-Until { (Thread-Of $A $B.User | Where-Object { $_.clip -eq $clipBA }) } 15000)

    # ---- someone new: their share waits for Allow (here settings' Requests › Allow) ----
    Share $B @{ op = 'forget'; who = $A.User } | Out-Null
    Start-Sleep -Milliseconds 300
    Share $A @{ op = 'clip'; to = $B.User; text = 'while asking' } | Out-Null
    $asking = Wait-Until { $s = St $B; $s.share.held -ge 1 -and (@($s.messaging.asking) -contains $A.User) } 15000
    Check 'A1' "after Remove, alice's clip waits behind her request" ($asking -and -not (Thread-Of $B $A.User | Where-Object { $_.clip -eq 'while asking' }))
    Share $B @{ op = 'allow'; who = $A.User } | Out-Null
    Check 'A2' "Allow in settings shows what she shared while asking" (Wait-Until { (Thread-Of $B $A.User | Where-Object { $_.clip -eq 'while asking' }) -and (St $B).share.held -eq 0 } 10000)

    # ---- files both ways, several chunks ----
    $fa = New-Blob (Join-Path $Root 'alice notes.bin') (3 * 1024 * 1024 + 12345)
    $fb = New-Blob (Join-Path $Root 'bob-photo.raw') (1024 * 1024 + 1)
    Send-And-Save $A $B $fa 'F1' | Out-Null
    Send-And-Save $B $A $fb 'F2' | Out-Null
    Check 'F3' 'both threads have the file lines' (((Thread-Of $B $A.User) | Where-Object { $_.file.name -eq 'alice notes.bin' }) -and ((Thread-Of $A $B.User) | Where-Object { $_.file.name -eq 'bob-photo.raw' }))

    # ---- keys: pinned ----
    $sa = St $A; $sb = St $B
    $pinned = ($sb.keys.pins.($A.User) -eq $sa.keys.pub) -and ($sa.keys.pins.($B.User) -eq $sb.keys.pub) -and $sa.keys.pub -and $sb.keys.pub
    Check 'K1' "each pins the other's published key" $pinned

    # ---- a shortcut sends its target ----
    $lnk = Join-Path $Root 'shortcut to notes.lnk'
    $ws = New-Object -ComObject WScript.Shell
    $sc = $ws.CreateShortcut($lnk); $sc.TargetPath = $fa; $sc.Save()
    Share $A @{ op = 'files'; to = $B.User; paths = @($lnk) } | Out-Null
    $viaLnk = Wait-Until { @((St $B).share.transfers | Where-Object { -not $_.mine -and $_.name -eq 'alice notes.bin' }).Count -ge 2 } 20000
    Check 'L1' 'a .lnk sends the file it points to (its name and size)' ($viaLnk -and (@((St $B).share.transfers | Where-Object { $_.name -like '*.lnk' }).Count -eq 0))
    $link = Join-Path $Root 'symlinked.bin'
    try {
        New-Item -ItemType SymbolicLink -Path $link -Target $fb -ErrorAction Stop | Out-Null
        Share $A @{ op = 'files'; to = $B.User; paths = @($link) } | Out-Null
        Check 'L2' 'a symlink sends its target' (Wait-Until { @((St $B).share.transfers | Where-Object { -not $_.mine -and $_.name -eq 'bob-photo.raw' }).Count -ge 1 } 20000)
    } catch { Log "skipped L2 (no symlink rights: $($_.Exception.Message))" }

    # ---- a copied image goes as clipboard.png ----
    try { $clipBefore = [Windows.Forms.Clipboard]::GetDataObject() } catch {}
    $bmp = New-Object Drawing.Bitmap 40, 30
    $g = [Drawing.Graphics]::FromImage($bmp); $g.Clear([Drawing.Color]::FromArgb(255, 30, 144, 255)); $g.Dispose()
    [Windows.Forms.Clipboard]::SetImage($bmp)
    Share $A @{ op = 'paste'; to = $B.User } | Out-Null
    $png = Wait-Until { (Transfer-Named $B 'clipboard.png' $false).state -eq 'received' } 20000
    if ($png) {
        Share $B @{ op = 'save'; id = (Transfer-Named $B 'clipboard.png' $false).id } | Out-Null
        Wait-Until { (Transfer-Named $B 'clipboard.png' $false).state -eq 'saved' } 20000 | Out-Null
        $p = Join-Path $B.Downloads 'clipboard.png'
        $img = $null
        try { $img = [Drawing.Image]::FromFile($p) } catch {}
        Check 'I1' 'a copied image goes as clipboard.png, a PNG that opens' ($null -ne $img -and $img.Width -eq 40 -and $img.Height -eq 30)
        if ($img) { $img.Dispose() }
    } else { Check 'I1' 'a copied image goes as clipboard.png' $false }

    # ---- the thread: lines, a kept reply, sending from it ----
    Share $B @{ op = 'open'; who = $A.User } | Out-Null
    Wait-Until { (St $B).threads.panel.thread -eq $A.User } 3000 | Out-Null
    Pipe $B @{ cmd = 'type'; text = 'kept reply' } | Out-Null
    Wait-Until { (St $B).threads.draft -eq 'kept reply' } 3000 | Out-Null
    Pipe $B @{ cmd = 'key'; vk = 27 } | Out-Null
    $closed = Wait-Until { $null -eq (St $B).threads.panel } 3000
    Share $B @{ op = 'open'; who = $A.User } | Out-Null
    $back = Wait-Until { $s = St $B; $s.threads.draft -eq 'kept reply' -and $s.threads.caret -eq 10 -and $s.threads.focus } 3000
    Check 'R1' 'Esc closes the thread, and the unsent reply is back with the caret after it' ($closed -and $back)
    Pipe $B @{ cmd = 'key'; vk = 13 } | Out-Null
    Check 'R2' 'Enter in the thread sends the reply to alice' (Wait-Until { (Thread-Of $A $B.User | Where-Object { $_.text -eq 'kept reply' -and $_.dir -eq 'received' }) -and (St $B).threads.draft -eq '' } 10000)
    Share $A @{ op = 'text'; to = $B.User; text = 'into the open thread' } | Out-Null
    $inThread = Wait-Until { (Thread-Of $B $A.User | Where-Object { $_.text -eq 'into the open thread' }) } 10000
    $noCard = -not (@((St $B).messaging.cards) | Where-Object { $_.text -eq 'into the open thread' })
    Check 'R3' "a message from the open thread's person lands in it, not as a card" ($inThread -and $noCard)

    # ---- drag hints ----
    Share $B @{ op = 'drag'; at = 'thread'; paths = @($fb) } | Out-Null
    Check 'D1' 'a drag over the thread says where it goes' (Wait-Until { (St $B).drag.thread -eq "drop to send to $($A.User)" } 3000) "$((St $B).drag | ConvertTo-Json -Compress)"
    Share $B @{ op = 'close' } | Out-Null
    Share $B @{ op = 'line'; text = "message $($A.User) here" } | Out-Null
    Start-Sleep -Milliseconds 300
    Share $B @{ op = 'drag'; at = 'bar'; paths = @($fb) } | Out-Null
    Check 'D2' 'a drag over the bar with a message line says who gets it' (Wait-Until { (St $B).drag.bar -eq "drop to send to $($A.User)" } 3000) "$((St $B).drag | ConvertTo-Json -Compress)"
    Share $B @{ op = 'line'; text = '2+2' } | Out-Null
    Start-Sleep -Milliseconds 300
    Share $B @{ op = 'drag'; at = 'bar'; paths = @($fb) } | Out-Null
    Check 'D3' 'over a calculation it says why not' (Wait-Until { (St $B).drag.bar -like 'clear the line to send files*' } 3000)
    Share $B @{ op = 'drag'; at = 'bar'; paths = @($Root) } | Out-Null
    Check 'D4' 'a folder says to zip it' (Wait-Until { (St $B).drag.bar -like 'a folder can*' } 3000)
    Check 'D5' 'the hint before the drag comes back after it' ((St $B).hint -notlike 'clear the line*')
    Share $B @{ op = 'line'; text = '' } | Out-Null

    # ---- the Mac's construction, both ways ----
    $vec = Join-Path $Root 'vectors'
    $made = & node (Join-Path $PSScriptRoot 'share-vectors.mjs') make $vec | ConvertFrom-Json
    $mac = Get-Content -Raw -Encoding UTF8 (Join-Path $vec 'mac.json') | ConvertFrom-Json
    $op = @{ op = 'vector-open' }
    foreach ($p in $mac.PSObject.Properties) { $op[$p.Name] = $p.Value }
    $opened = Share $B $op
    Check 'M1' "a clip sealed as the Mac seals it opens here" ($opened.clip -eq $mac.clipText)
    $savedMac = $opened.saved
    Check 'M2' "a 2.5 MiB file (3 chunks) and its offer sealed as the Mac seals them open, checked and marked" ($opened.facts.sha256 -eq $mac.plainSha -and $savedMac -and (Sha $savedMac) -eq $mac.plainSha -and $opened.marked)
    $sealedOut = Join-Path $vec 'win-file.sealed'
    $plainIn = Join-Path $vec 'win-plain.bin'
    $w = Share $A @{ op = 'vector-seal'; recipPub = $made.recipPub; from = 'win-user'; to = 'mac-user'; clip = 'hello from windows ✓'; plainFile = $plainIn; sealedFile = $sealedOut }
    $win = @{ senderPub = $w.senderPub; clip = $w.clip; offer = $w.offer; from = 'win-user'; to = 'mac-user'; sealedFile = $sealedOut; plainFile = $plainIn }
    [IO.File]::WriteAllText((Join-Path $vec 'win.json'), ($win | ConvertTo-Json -Depth 6))
    $macOpened = & node (Join-Path $PSScriptRoot 'share-vectors.mjs') open $vec | ConvertFrom-Json
    Check 'M3' 'what this app seals opens with the Mac construction (clip, offer, 3-chunk file)' ($macOpened.clip -eq 'hello from windows ✓' -and $macOpened.matches -and $macOpened.lengthOk) "$($macOpened | ConvertTo-Json -Compress)"

    # ---- a changed key: held until Trust New Key ----
    $oldPub = (St $A).keys.pub
    Stop-Copy $A
    Remove-Item (Join-Path $A.Data 'share-key.bin') -Force
    Start-Copy $A
    Wait-Until { (St $A).messaging.link -eq 'Ready' } 20000 | Out-Null
    Share $A @{ op = 'clip'; to = $B.User; text = 'with my new key' } | Out-Null
    $held = Wait-Until { $s = St $B; $s.share.heldForKey -ge 1 -and $s.keys.changed.($A.User) } 15000
    $newPub = (St $A).keys.pub
    Check 'K2' "a changed key holds what they send (nothing opens with an unpinned key)" ($held -and $newPub -ne $oldPub -and -not (Thread-Of $B $A.User | Where-Object { $_.clip -eq 'with my new key' }))
    Share $B @{ op = 'clip'; to = $A.User; text = 'should not go' } | Out-Null
    Check 'K3' 'nothing is sealed for them while it has changed' (Wait-Until { (St $B).hint -like '*security key changed*' } 10000) "$((St $B).hint)"
    Share $B @{ op = 'open'; who = $A.User } | Out-Null
    Share $B @{ op = 'trust'; who = $A.User } | Out-Null
    $trusted = Wait-Until { $s = St $B; $s.keys.pins.($A.User) -eq $newPub -and (Thread-Of $B $A.User | Where-Object { $_.clip -eq 'with my new key' }) } 15000
    Check 'K4' 'Trust New Key pins the new key and opens what was held' $trusted
    Share $B @{ op = 'close' } | Out-Null
    Share $B @{ op = 'clip'; to = $A.User; text = 'after trust' } | Out-Null
    Check 'K5' 'sending to them works again after trusting' (Wait-Until { (Thread-Of $A $B.User | Where-Object { $_.clip -eq 'after trust' }) } 15000)

    # ---- the relay goes away mid-upload ----
    # alice's copy waits 1.5 s before each part (QCALC_E2E_SLOW_UPLOAD_MS): the window to take the relay down
    $fc = New-Blob (Join-Path $Root 'retry me.bin') (2 * 1024 * 1024 + 77)
    Share $A @{ op = 'files'; to = $B.User; paths = @($fc) } | Out-Null
    Wait-Until { (Transfer-Named $B 'retry me.bin' $false).state -eq 'received' } 20000 | Out-Null
    Share $B @{ op = 'save'; id = (Transfer-Named $B 'retry me.bin' $false).id } | Out-Null
    $up = Wait-Until { (Transfer-Named $A 'retry me.bin' $true).state -eq 'uploading' } 15000
    Stop-Relay
    Start-Sleep -Seconds 2
    Start-Relay
    $auto = Wait-Until { (Transfer-Named $A 'retry me.bin' $true).state -eq 'sent' } 60000
    $savedC = Wait-Until { (Transfer-Named $B 'retry me.bin' $false).state -eq 'saved' } 60000
    $sameC = $savedC -and ((Sha (Join-Path $B.Downloads 'retry me.bin')) -eq (Sha $fc))
    Check 'U1' 'a part cut short by the relay going away goes again on its own, and the file arrives intact' ($up -and $auto -and $sameC) "A: $((Transfer-Named $A 'retry me.bin' $true).state) B: $((Transfer-Named $B 'retry me.bin' $false).state)"

    $fd = New-Blob (Join-Path $Root 'retry by hand.bin') (1024 * 1024 + 5000)
    Wait-Until { (St $A).messaging.link -eq 'Ready' -and (St $B).messaging.link -eq 'Ready' } 30000 | Out-Null
    Share $A @{ op = 'files'; to = $B.User; paths = @($fd) } | Out-Null
    Wait-Until { (Transfer-Named $B 'retry by hand.bin' $false).state -eq 'received' } 20000 | Out-Null
    Share $B @{ op = 'save'; id = (Transfer-Named $B 'retry by hand.bin' $false).id } | Out-Null
    Wait-Until { (Transfer-Named $A 'retry by hand.bin' $true).state -eq 'uploading' } 15000 | Out-Null
    Stop-Relay
    $failed = Wait-Until { $t = Transfer-Named $A 'retry by hand.bin' $true; $t.state -eq 'failed' -and $t.retry } 60000
    Check 'U2' 'past its tries the upload fails and keeps its sealed file for Retry' $failed "$((Transfer-Named $A 'retry by hand.bin' $true) | ConvertTo-Json -Compress)"
    Start-Relay
    # Retry while bob's inbox is down (as if his pc were offline for a moment): the relay's `ready` for the finished
    # upload reaches nobody, and bob has to ask for it once his inbox opens again
    Wait-Until { (St $A).messaging.link -eq 'Ready' -and (St $B).messaging.link -eq 'Ready' } 40000 | Out-Null
    Share $B @{ op = 'inbox'; pause = $true } | Out-Null
    $bobAway = Wait-Until { (St $B).messaging.link -eq 'Offline' } 3000
    Share $A @{ op = 'retry'; id = (Transfer-Named $A 'retry by hand.bin' $true).id } | Out-Null
    $doneA = Wait-Until { (Transfer-Named $A 'retry by hand.bin' $true).state -eq 'sent' } 30000
    Check 'U3a' "alice's Retry finishes while bob's inbox is still down" ($doneA -and $bobAway) "bob away: $bobAway, alice: $((Transfer-Named $A 'retry by hand.bin' $true).state)"
    Share $B @{ op = 'inbox'; pause = $false } | Out-Null
    $savedD = Wait-Until { (Transfer-Named $B 'retry by hand.bin' $false).state -eq 'saved' } 60000
    $sameD = $savedD -and ((Sha (Join-Path $B.Downloads 'retry by hand.bin')) -eq (Sha $fd))
    Check 'U3' 'Retry sends the rest once the relay is back, and the file arrives intact' $sameD "A: $((Transfer-Named $A 'retry by hand.bin' $true).state) B: $((Transfer-Named $B 'retry by hand.bin' $false).state)"

    # ---- messages typed quickly arrive in order ----
    Wait-Until { (St $A).messaging.link -eq 'Ready' -and (St $B).messaging.link -eq 'Ready' } 30000 | Out-Null
    $burst = 1..10 | ForEach-Object { "quick $_" }
    foreach ($m in $burst) { Share $A @{ op = 'text'; to = $B.User; text = $m } | Out-Null }
    $inOrder = Wait-Until { $got = @(Thread-Of $B $A.User | Where-Object { $_.text -like 'quick *' } | ForEach-Object { $_.text }); ($got -join '|') -eq ($burst -join '|') } 15000
    Check 'Q1' 'ten messages sent back to back arrive, in the order they were sent' $inOrder "$(@(Thread-Of $B $A.User | Where-Object { $_.text -like 'quick *' } | ForEach-Object { $_.text }) -join ', ')"

    # ---- streaming, saving without asking, the clipboard ----
    function No-Partials($c) { @(Get-ChildItem -Force -LiteralPath $c.Downloads -Filter '.qcalc-*' -ErrorAction SilentlyContinue).Count -eq 0 }
    Share $B @{ op = 'prefs'; autoAccept = 100000000; clipboard = $true } | Out-Null
    Share $A @{ op = 'prefs'; autoAccept = 100000000; clipboard = $false } | Out-Null
    Start-Sleep -Milliseconds 300
    $settingsB = Get-Content -Raw (Join-Path $B.Data 'settings.json') | ConvertFrom-Json
    Check 'S0' 'Settings › Messages: saving without asking and copying received files are saved' ($settingsB.settings.autoAcceptBytes -eq 100000000 -and $settingsB.settings.receivedToClipboard -eq $true)

    # alice uploads a part every 1.5 s (QCALC_E2E_SLOW_UPLOAD_MS): bob reads each one while the rest are still coming
    $fs = New-Blob (Join-Path $Root 'streamed.bin') (9 * 1024 * 1024 + 321)
    Share $A @{ op = 'files'; to = $B.User; paths = @($fs) } | Out-Null
    $overlap = Wait-Until { $tb = Transfer-Named $B 'streamed.bin' $false; $ta = Transfer-Named $A 'streamed.bin' $true; $tb.state -eq 'downloading' -and $tb.percent -gt 0 -and $ta.state -eq 'uploading' } 30000 50
    $streamedSaved = Wait-Until { (Transfer-Named $B 'streamed.bin' $false).state -eq 'saved' } 60000
    $tsb = Transfer-Named $B 'streamed.bin' $false
    $pathS = Join-Path $B.Downloads 'streamed.bin'
    Check 'S1' "an allowed name's file saves on its own, streamed, byte for byte" ($streamedSaved -and (Sha $pathS) -eq (Sha $fs) -and $tsb.saved -eq $pathS) "$($tsb | ConvertTo-Json -Compress)"
    Check 'S2' 'the recipient reads parts while the sender is still uploading' $overlap
    Check 'S3' 'one file per send in Downloads, nothing partial left' ((No-Partials $B) -and @(Get-ChildItem -LiteralPath $B.Downloads -Filter 'streamed*').Count -eq 1)
    $drop = $null; $effect = $null
    try {
        $drop = [Windows.Forms.Clipboard]::GetFileDropList()
        $ms = [Windows.Forms.Clipboard]::GetData('Preferred DropEffect')
        if ($ms) { $buf = New-Object byte[] 4; $ms.Read($buf, 0, 4) | Out-Null; $effect = [BitConverter]::ToInt32($buf, 0) }
    } catch {}
    Check 'S4' 'the saved file is on the clipboard as a file to paste (copy)' ($drop -and $drop.Contains($pathS) -and $effect -eq 1) "drop: $($drop -join ';') effect: $effect"
    Check 'S5' 'a saved file has Show in Folder' (@($tsb.acts) -contains 'Show in Folder') "$($tsb.acts -join ',')"
    Check 'S6' 'the sender says sent, and the line under the bar follows it' ((Wait-Until { (Transfer-Named $A 'streamed.bin' $true).state -eq 'sent' } 20000) -and ((St $A).hint -like 'streamed.bin*sent to*')) "$((St $A).hint)"

    # several at once
    $many = @((New-Blob (Join-Path $Root 'many-1.bin') 1024), (New-Blob (Join-Path $Root 'many-2.bin') (1024 * 1024)), (New-Blob (Join-Path $Root 'many-3.bin') (2 * 1024 * 1024 + 9)))
    Share $A @{ op = 'files'; to = $B.User; paths = $many } | Out-Null
    $allIn = Wait-Until { @(1..3 | Where-Object { (Transfer-Named $B "many-$_.bin" $false).state -eq 'saved' }).Count -eq 3 } 60000
    $allSame = $allIn -and @(1..3 | Where-Object { (Sha (Join-Path $B.Downloads "many-$_.bin")) -eq (Sha $many[$_ - 1]) }).Count -eq 3
    Check 'S7' 'three files sent at once all save on their own, intact' $allSame

    # over the size it asks
    Share $B @{ op = 'prefs'; autoAccept = 10000000 } | Out-Null
    Start-Sleep -Milliseconds 200
    $fbig = New-Blob (Join-Path $Root 'over ten.bin') (11 * 1000 * 1000)
    Share $A @{ op = 'files'; to = $B.User; paths = @($fbig) } | Out-Null
    $asks = Wait-Until { (Transfer-Named $B 'over ten.bin' $false).state -eq 'received' } 20000
    Start-Sleep -Milliseconds 800
    $tbig = Transfer-Named $B 'over ten.bin' $false
    Check 'S8' 'a file over the size set waits for Save' ($asks -and $tbig.state -eq 'received' -and @($tbig.acts) -contains 'Save') "$($tbig | ConvertTo-Json -Compress)"
    Share $B @{ op = 'save'; id = $tbig.id } | Out-Null
    Check 'S9' 'Save streams it in' ((Wait-Until { (Transfer-Named $B 'over ten.bin' $false).state -eq 'saved' } 60000) -and (Sha (Join-Path $B.Downloads 'over ten.bin')) -eq (Sha $fbig))

    # the recipient cancels mid-way, the sender takes one back mid-way: nothing is left either side
    Share $B @{ op = 'prefs'; autoAccept = 100000000 } | Out-Null
    $fc1 = New-Blob (Join-Path $Root 'cancel me.bin') (12 * 1024 * 1024)
    Share $A @{ op = 'files'; to = $B.User; paths = @($fc1) } | Out-Null
    Wait-Until { (Transfer-Named $B 'cancel me.bin' $false).state -eq 'downloading' } 20000 50 | Out-Null
    Share $B @{ op = 'cancel'; id = (Transfer-Named $B 'cancel me.bin' $false).id } | Out-Null
    $declined = Wait-Until { (Transfer-Named $A 'cancel me.bin' $true).state -eq 'declined' } 20000
    Start-Sleep -Milliseconds 500
    Check 'X1' "Cancel stops the download, the sender hears it was turned down, nothing's left" ($declined -and (Transfer-Named $B 'cancel me.bin' $false).state -eq 'refused' -and (No-Partials $B) -and -not (Test-Path (Join-Path $B.Downloads 'cancel me.bin'))) "A: $((Transfer-Named $A 'cancel me.bin' $true).state) B: $((Transfer-Named $B 'cancel me.bin' $false).state)"
    $fc2 = New-Blob (Join-Path $Root 'take me back.bin') (12 * 1024 * 1024)
    Share $A @{ op = 'files'; to = $B.User; paths = @($fc2) } | Out-Null
    Wait-Until { (Transfer-Named $A 'take me back.bin' $true).state -eq 'uploading' } 20000 50 | Out-Null
    Check 'X2a' 'Take Back shows while it uploads' (@((Transfer-Named $A 'take me back.bin' $true).acts) -contains 'Take Back')
    Share $A @{ op = 'takeback'; id = (Transfer-Named $A 'take me back.bin' $true).id } | Out-Null
    $tookBack = Wait-Until { (Transfer-Named $B 'take me back.bin' $false).state -eq 'cancelled' } 20000
    Start-Sleep -Milliseconds 500
    Check 'X2' "Take Back stops the upload, the recipient hears it, nothing's left" ($tookBack -and (Transfer-Named $A 'take me back.bin' $true).state -eq 'withdrawn' -and (No-Partials $B) -and -not (Test-Path (Join-Path $B.Downloads 'take me back.bin'))) "A: $((Transfer-Named $A 'take me back.bin' $true).state) B: $((Transfer-Named $B 'take me back.bin' $false).state)"

    # a stranger's file always asks, whatever the size setting
    Share $B @{ op = 'forget'; who = $A.User } | Out-Null
    Start-Sleep -Milliseconds 300
    $fst = New-Blob (Join-Path $Root 'from a stranger.bin') 2048
    Share $A @{ op = 'files'; to = $B.User; paths = @($fst) } | Out-Null
    $req = Wait-Until { (Transfer-Named $B 'from a stranger.bin' $false).state -eq 'request' } 20000
    Start-Sleep -Milliseconds 800
    Check 'X3' "a stranger's file waits behind their request, never saved on its own" ($req -and (Transfer-Named $B 'from a stranger.bin' $false).state -eq 'request' -and -not (Test-Path (Join-Path $B.Downloads 'from a stranger.bin')))
    Share $B @{ op = 'allow'; who = $A.User } | Out-Null
    $tst = $null; Wait-Until { $tst = Transfer-Named $B 'from a stranger.bin' $false; $tst.state -eq 'received' } 10000 | Out-Null
    Share $B @{ op = 'save'; id = (Transfer-Named $B 'from a stranger.bin' $false).id } | Out-Null
    Check 'X4' 'once allowed, Save brings it in' (Wait-Until { (Transfer-Named $B 'from a stranger.bin' $false).state -eq 'saved' } 20000)

    # a name with no key yet
    $nk = Invoke-RestMethod -Method Post "$Relay/claim" -Body ('{"name":"nokey-e2e","key":"' + ('c3' * 32) + '"}')
    Share $A @{ op = 'files'; to = 'nokey-e2e'; paths = @($fst) } | Out-Null
    Check 'X5' 'a name with no key yet: it says they need a newer q calc, and nothing is offered' ($nk.status -eq 'ok' -and (Wait-Until { (St $A).hint -like '*needs a newer q calc*' } 15000)) "$((St $A).hint)"

    # the sender quits mid-upload: on its next start the relay lets the file go and the recipient hears
    $fq = New-Blob (Join-Path $Root 'quit mid-way.bin') (12 * 1024 * 1024)
    Share $A @{ op = 'files'; to = $B.User; paths = @($fq) } | Out-Null
    Wait-Until { (Transfer-Named $A 'quit mid-way.bin' $true).state -eq 'uploading' } 20000 50 | Out-Null
    Stop-Process -Id $A.Proc.Id -Force
    Start-Copy $A
    $let = Wait-Until { (Transfer-Named $B 'quit mid-way.bin' $false).state -eq 'cancelled' } 40000
    Start-Sleep -Milliseconds 500
    Check 'X6' "after a restart mid-upload the recipient hears it was taken back, nothing partial is left" ($let -and (No-Partials $B)) "B: $((Transfer-Named $B 'quit mid-way.bin' $false).state)"
} catch {
    Check 'X' 'the suite ran to the end' $false $_.Exception.Message
    Log $_.ScriptStackTrace
} finally {
    if ($clipBefore) { try { [Windows.Forms.Clipboard]::SetDataObject($clipBefore, $true) } catch {} }
    foreach ($c in $A, $B) { Stop-Copy $c }
    foreach ($id in $script:mine) { Stop-Process -Id $id -Force -ErrorAction SilentlyContinue }
    Stop-Relay
    foreach ($c in $A, $B) { if (Test-Path $c.Log) { Copy-Item $c.Log (Join-Path $OutDir "qcalc-$($c.Name).log") -Force } }
}

$failedChecks = @($script:results | Where-Object { -not $_.ok })
$lines = @('| id | check | result |', '|---|---|---|') + ($script:results | ForEach-Object { "| $($_.id) | $($_.what) | $(if ($_.ok) { 'pass' } else { "FAIL $($_.detail)" }) |" })
[IO.File]::WriteAllLines((Join-Path $OutDir 'results.md'), $lines)
Log ("{0} checks, {1} failed" -f $script:results.Count, $failedChecks.Count)
Stop-Transcript | Out-Null
Remove-Item -Recurse -Force $Root -ErrorAction SilentlyContinue
if ($failedChecks.Count) { exit 1 }
exit 0
