# installs the nsis build silently when there is one, else falls back to the bare exe; prints the exe to launch
param(
    [string]$Target = (Join-Path $PSScriptRoot '..\..\target\release')
)
$ErrorActionPreference = 'Stop'

function Find-InstalledExe {
    $keys = @(
        'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
        'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
        'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*'
    )
    $dirs = @()
    $entry = Get-ItemProperty $keys -ErrorAction SilentlyContinue | Where-Object { $_.DisplayName -like 'Q Calc*' } | Select-Object -First 1
    if ($entry) {
        Write-Host "uninstall entry: $($entry.DisplayName) $($entry.DisplayVersion)"
        if ($entry.InstallLocation) { $dirs += $entry.InstallLocation.Trim('"') }
        if ($entry.DisplayIcon) { $dirs += Split-Path ($entry.DisplayIcon.Trim('"') -replace ',\d+$', '') }
    }
    $dirs += @("$env:LOCALAPPDATA\Q Calc", "$env:LOCALAPPDATA\Programs\Q Calc", "$env:ProgramFiles\Q Calc")
    foreach ($dir in $dirs) {
        if (-not (Test-Path $dir)) { continue }
        $exe = Get-ChildItem $dir -Filter *.exe -File | Where-Object { $_.Name -notmatch 'uninst' } |
            Sort-Object { if ($_.BaseName -in @('qcalc', 'Q Calc')) { 0 } else { 1 } } | Select-Object -First 1
        if ($exe) { return $exe.FullName }
    }
    return $null
}

$bundle = Join-Path $Target 'bundle\nsis'
$installer = Get-ChildItem $bundle -Filter *.exe -File -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 1
$exe = $null
$mode = 'bare exe'

if ($installer) {
    Write-Host "installing $($installer.Name) silently"
    # WaitForExit rather than -Wait: -Wait also waits on anything the installer launches
    $p = Start-Process -FilePath $installer.FullName -ArgumentList '/S' -PassThru
    if (-not $p.WaitForExit(300000)) { throw 'installer did not finish within 5 minutes' }
    if ($p.ExitCode -ne 0) { throw "installer exited with $($p.ExitCode)" }
    $exe = Find-InstalledExe
    if ($exe) { $mode = "installed from $($installer.Name)" }
    else { Write-Warning 'installer ran but no installed exe was found; falling back to the built exe' }
}

if (-not $exe) {
    $built = Get-ChildItem $Target -Filter *.exe -File -ErrorAction SilentlyContinue | Where-Object { $_.Name -notmatch 'build-script' } |
        Sort-Object { if ($_.BaseName -in @('qcalc', 'Q Calc')) { 0 } else { 1 } } | Select-Object -First 1
    if (-not $built) { throw "no installer in $bundle and no exe in $Target" }
    $exe = $built.FullName
}

Write-Host "exe: $exe ($mode)"
if ($env:GITHUB_ENV) {
    # AppendAllText writes utf-8 without a bom on both powershells
    [IO.File]::AppendAllText($env:GITHUB_ENV, "QCALC_EXE=$exe`nQCALC_INSTALL_MODE=$mode`n")
}
