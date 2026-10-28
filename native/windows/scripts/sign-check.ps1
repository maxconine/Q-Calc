<#
.SYNOPSIS
  A mock code-signing pass over the native app's shipped files: everything real signing and SmartScreen look at,
  checked with a throwaway certificate, so a real certificate can be dropped in later without surprises.

.DESCRIPTION
  Nothing here is published and nothing on the machine changes for good:
  - a self-signed code-signing certificate is made in Cert:\CurrentUser\My only, used, and deleted at the end;
  - only copies are signed (in -OutDir), never the build's own files.

  The checks, each pass / warn / fail:
  1. Coverage: every binary the installer ships (qcalc.exe, the installer, its uninstall.exe) and every DLL the exe
     imports that Windows doesn't provide (a DLL that isn't shipped can't start the app on a PC that lacks it).
  2. Version resource of each PE: CompanyName, ProductName, FileVersion = the release's version, OriginalFilename,
     FileDescription.
  3. Embedded manifest: asInvoker (no UAC prompt), DPI awareness, supportedOS (Windows 10/11).
  4. Signing: each copy signed (SHA-256, an RFC 3161 timestamp when the server answers, else noted); the signature's
     structure read back (signer, digest, timestamp, intact hash); a tampered copy must read as a hash mismatch.
  5. The uninstaller: the installer is rebuilt with this certificate as its signing command (qcalc.nsi's SIGN
     define: !finalize and !uninstfinalize), and the uninstall.exe NSIS signed on the way is checked too.
  6. Hardening flags SmartScreen and Smart App Control reviewers look for (ASLR, high-entropy ASLR, DEP, CFG).

  The mock signature ends in an untrusted root, as a self-signed one must: that's reported as expected. What
  matters is that nothing else is wrong.

  Exit code 1 when a check fails (CI runs it as a non-blocking step).

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File native\windows\scripts\sign-check.ps1
  powershell -File sign-check.ps1 -Exe native\target\release\qcalc.exe -Installer native\windows\installer\out\Q-Calc-2.0.15-x64-native-setup.exe
#>
param(
    [string]$Exe,
    [string]$Installer,
    # the version the binaries must carry; default: QCALC_VERSION, else the installer's file name, else Cargo's
    [string]$Version,
    [string]$OutDir = (Join-Path ([IO.Path]::GetTempPath()) ('qcalc-sign-check-' + [Guid]::NewGuid().ToString('N').Substring(0, 8))),
    [string]$TimestampUrl = 'http://timestamp.digicert.com',
    [switch]$NoTimestamp,
    # rebuild the installer with the mock certificate as its signing command (needs makensis), to check uninstall.exe
    [switch]$SkipUninstaller,
    [string]$Makensis,
    # internal: makensis's !finalize / !uninstfinalize call this to sign one file
    [string]$SignOne,
    [string]$Thumbprint,
    [string]$KeepCopy
)
$ErrorActionPreference = 'Stop'

# ---- one file, for makensis (sign-check.ps1 -SignOne <file> -Thumbprint <t> [-KeepCopy <dir>]) ----
if ($SignOne) {
    $cert = Get-Item "Cert:\CurrentUser\My\$Thumbprint"
    $args2 = @{ FilePath = $SignOne; Certificate = $cert; HashAlgorithm = 'SHA256' }
    if ($TimestampUrl -and -not $NoTimestamp) { $args2.TimestampServer = $TimestampUrl }
    $sig = Set-AuthenticodeSignature @args2
    if ($sig.Status -ne 'Valid' -and $sig.Status -ne 'UnknownError') { Write-Error "signing $SignOne failed: $($sig.Status) $($sig.StatusMessage)"; exit 1 }
    if ($KeepCopy) {
        New-Item -ItemType Directory -Force $KeepCopy | Out-Null
        Copy-Item $SignOne (Join-Path $KeepCopy ([IO.Path]::GetFileName($SignOne) + '.signed')) -Force
    }
    exit 0
}

$here = $PSScriptRoot
$win = (Resolve-Path "$here\..").Path
$native = (Resolve-Path "$here\..\..").Path
$repo = (Resolve-Path "$here\..\..\..").Path

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

namespace QCalcSign {
public class PeInfo {
    public ushort Machine; public bool Plus; public ushort DllCharacteristics; public ushort Subsystem;
    public uint SecuritySize; public List<string> Imports = new List<string>(); public List<string> DelayImports = new List<string>();
    public string Manifest;
}
public static class Pe {
    [DllImport("kernel32", SetLastError = true, CharSet = CharSet.Unicode)] static extern IntPtr LoadLibraryExW(string f, IntPtr h, uint flags);
    [DllImport("kernel32", SetLastError = true)] static extern IntPtr FindResourceW(IntPtr m, IntPtr name, IntPtr type);
    [DllImport("kernel32", SetLastError = true)] static extern uint SizeofResource(IntPtr m, IntPtr r);
    [DllImport("kernel32", SetLastError = true)] static extern IntPtr LoadResource(IntPtr m, IntPtr r);
    [DllImport("kernel32")] static extern IntPtr LockResource(IntPtr r);
    [DllImport("kernel32")] static extern bool FreeLibrary(IntPtr m);

    static uint U32(byte[] b, long o) { return BitConverter.ToUInt32(b, (int)o); }
    static ushort U16(byte[] b, long o) { return BitConverter.ToUInt16(b, (int)o); }

    public static PeInfo Read(string path) {
        byte[] b = File.ReadAllBytes(path);
        var info = new PeInfo();
        uint pe = U32(b, 0x3c);
        if (U32(b, pe) != 0x4550) throw new Exception("not a PE file: " + path);
        info.Machine = U16(b, pe + 4);
        ushort nsec = U16(b, pe + 6);
        ushort optSize = U16(b, pe + 20);
        long opt = pe + 24;
        info.Plus = U16(b, opt) == 0x20b;
        info.Subsystem = U16(b, opt + 68);
        info.DllCharacteristics = U16(b, opt + 70);
        long dirs = opt + (info.Plus ? 112 : 96);
        long secs = opt + optSize;
        Func<uint, long> off = rva => {
            for (int i = 0; i < nsec; i++) {
                long s = secs + 40 * i;
                uint va = U32(b, s + 12), vsz = U32(b, s + 8), raw = U32(b, s + 20), rsz = U32(b, s + 16);
                if (rva >= va && rva < va + Math.Max(vsz, rsz)) return raw + (rva - va);
            }
            return -1;
        };
        Func<long, string> cstr = o => { int e = (int)o; while (e < b.Length && b[e] != 0) e++; return Encoding.ASCII.GetString(b, (int)o, e - (int)o); };
        info.SecuritySize = U32(b, dirs + 4 * 8 + 4);
        uint imp = U32(b, dirs + 1 * 8);
        if (imp != 0) {
            long d = off(imp);
            for (; d >= 0 && d + 20 <= b.Length; d += 20) {
                uint name = U32(b, d + 12);
                if (name == 0) break;
                long n = off(name); if (n >= 0) info.Imports.Add(cstr(n));
            }
        }
        uint delay = U32(b, dirs + 13 * 8);
        if (delay != 0) {
            long d = off(delay);
            for (; d >= 0 && d + 32 <= b.Length; d += 32) {
                uint name = U32(b, d + 4);
                if (name == 0) break;
                long n = off(name); if (n >= 0) info.DelayImports.Add(cstr(n));
            }
        }
        info.Manifest = ManifestOf(path);
        return info;
    }

    // RT_MANIFEST (24), id 1 (an exe's) or 2 (a DLL's)
    public static string ManifestOf(string path) {
        IntPtr m = LoadLibraryExW(path, IntPtr.Zero, 0x2 | 0x20);
        if (m == IntPtr.Zero) return null;
        try {
            foreach (int id in new[] { 1, 2 }) {
                IntPtr r = FindResourceW(m, (IntPtr)id, (IntPtr)24);
                if (r == IntPtr.Zero) continue;
                uint size = SizeofResource(m, r);
                IntPtr p = LockResource(LoadResource(m, r));
                byte[] data = new byte[size];
                Marshal.Copy(p, data, 0, (int)size);
                string text = Encoding.UTF8.GetString(data);
                return text.TrimStart((char)0xFEFF);
            }
            return null;
        } finally { FreeLibrary(m); }
    }
}
}
'@

$script:rows = New-Object System.Collections.Generic.List[object]
function Add-Row([string]$file, [string]$check, [string]$result, [string]$detail) {
    $script:rows.Add([pscustomobject]@{ File = $file; Check = $check; Result = $result; Detail = $detail })
    $color = switch ($result) { 'pass' { 'Green' } 'warn' { 'Yellow' } 'fail' { 'Red' } default { 'Gray' } }
    Write-Host ("[{0}] {1}: {2} - {3}" -f $result.ToUpper(), $file, $check, $detail) -ForegroundColor $color
}

# ---- what to check ----

if (-not $Exe) {
    foreach ($c in "$native\target\x86_64-pc-windows-msvc\release\qcalc.exe", "$native\target\release\qcalc.exe", "$native\target\aarch64-pc-windows-msvc\release\qcalc.exe") {
        if (Test-Path $c) { $Exe = $c; break }
    }
}
if (-not $Exe -or -not (Test-Path $Exe)) { throw "no qcalc.exe: build it (cargo build --release -p qcalc-win) or pass -Exe" }
$Exe = (Resolve-Path $Exe).Path
if (-not $Installer) {
    $Installer = Get-ChildItem "$win\installer\out\*-native-setup.exe" -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 1 -ExpandProperty FullName
}
if ($Installer) { $Installer = (Resolve-Path $Installer).Path }

$cargoVersion = $null
$m = Select-String -Path "$native\Cargo.toml" -Pattern '^\s*version\s*=\s*"([^"]+)"' | Select-Object -First 1
if ($m) { $cargoVersion = $m.Matches[0].Groups[1].Value }
$pkgVersion = (Get-Content "$repo\package.json" -Raw | ConvertFrom-Json).version
if (-not $Version) {
    if ($env:QCALC_VERSION) { $Version = $env:QCALC_VERSION }
    elseif ($Installer -and ((Split-Path $Installer -Leaf) -match '^Q-Calc-(.+?)-(x64|arm64)-native-setup\.exe$')) { $Version = $Matches[1] }
    else { $Version = $cargoVersion }
}
New-Item -ItemType Directory -Force $OutDir | Out-Null
$OutDir = (Resolve-Path $OutDir).Path
Write-Host "exe:       $Exe"
Write-Host "installer: $(if ($Installer) { $Installer } else { '(none: build it with native\windows\installer\build.ps1)' })"
Write-Host "version:   $Version (Cargo $cargoVersion, package.json $pkgVersion)"
Write-Host "out:       $OutDir"

if ($cargoVersion -ne $pkgVersion) {
    Add-Row 'build' 'version source' 'info' "Cargo's version ($cargoVersion) isn't package.json's ($pkgVersion): a release build must set QCALC_VERSION (installer\build.ps1 does), or the exe reports $cargoVersion and the update check offers every release again"
}

# ---- 1. coverage ----

# DLLs Windows itself provides (System32, the API sets); anything else must ship beside qcalc.exe and be signed
function Test-SystemDll([string]$name) {
    $n = $name.ToLowerInvariant()
    if ($n -like 'api-ms-win-*' -or $n -like 'ext-ms-*') {
        # the universal CRT's API sets are system DLLs on Windows 10+, the rest always
        return $true
    }
    if ($n -match '^(vcruntime|msvcp|concrt|vccorlib)\d') { return $false }
    return (Test-Path (Join-Path $env:WINDIR "System32\$n"))
}

$exeInfo = [QCalcSign.Pe]::Read($Exe)
$foreign = @(($exeInfo.Imports + $exeInfo.DelayImports) | Sort-Object -Unique | Where-Object { -not (Test-SystemDll $_) })
$nsi = Get-Content "$win\installer\qcalc.nsi" -Raw
$shipped = @([regex]::Matches($nsi, '(?m)^\s*File\s+(?:"/oname=([^"]+)"|/oname=(\S+))') | ForEach-Object { if ($_.Groups[1].Value) { $_.Groups[1].Value } else { $_.Groups[2].Value } })
$shippedDlls = @($shipped | Where-Object { $_ -like '*.dll' })
if ($foreign.Count -eq 0) {
    Add-Row 'qcalc.exe' 'imports' 'pass' "every DLL it loads is Windows' own ($((($exeInfo.Imports + $exeInfo.DelayImports) | Sort-Object -Unique) -join ', '))"
} else {
    foreach ($dll in $foreign) {
        if ($shippedDlls -contains $dll) { Add-Row 'qcalc.exe' "imports $dll" 'pass' 'shipped by the installer (sign it too)' }
        else { Add-Row 'qcalc.exe' "imports $dll" 'fail' "not a Windows DLL and not shipped: a PC without it can't start Q Calc (link the CRT statically, native\.cargo\config.toml)" }
    }
}
$hasUninst = $nsi -match 'WriteUninstaller'
Add-Row 'installer' 'shipped binaries' 'info' ("the installer writes: " + (@($shipped) + $(if ($hasUninst) { @('uninstall.exe (WriteUninstaller)') } else { @() }) -join ', '))
foreach ($f in $shipped) { if ($f -notin @('qcalc.exe') -and $f -notlike '*.dll') { Add-Row 'installer' "ships $f" 'warn' 'not covered by this check' } }
if ($hasUninst) {
    if ($nsi -match '!uninstfinalize') { Add-Row 'uninstall.exe' 'signing hook' 'pass' 'qcalc.nsi signs it with !uninstfinalize when SIGN is defined' }
    else { Add-Row 'uninstall.exe' 'signing hook' 'fail' "qcalc.nsi has no !uninstfinalize: the uninstaller NSIS writes would ship unsigned inside a signed installer" }
}
if ($nsi -notmatch '!finalize') { Add-Row 'installer' 'signing hook' 'warn' 'no !finalize: the installer must be signed after makensis (fine for SignPath, not for a sign command)' }

# ---- 2. version resource ----

function Test-VersionInfo([string]$label, [string]$path, [string]$original) {
    $vi = [Diagnostics.FileVersionInfo]::GetVersionInfo($path)
    $want = @{ CompanyName = 'maxconine'; ProductName = 'Q Calc' }
    foreach ($k in $want.Keys) {
        $v = $vi.$k
        if ($v -eq $want[$k]) { Add-Row $label $k 'pass' $v }
        elseif ($v) { Add-Row $label $k 'warn' "'$v' (expected '$($want[$k])')" }
        else { Add-Row $label $k 'fail' 'missing: signing services and SmartScreen show the publisher and product from here' }
    }
    $fv = "$($vi.FileVersion)".Trim()
    $num = ($Version -split '[-+]')[0]
    $fixed = '{0}.{1}.{2}' -f $vi.FileMajorPart, $vi.FileMinorPart, $vi.FileBuildPart
    if (-not $fv) { Add-Row $label 'FileVersion' 'fail' 'missing' }
    elseif ($fv -eq $Version -or $fv -eq "$Version.0") {
        if ($fixed -eq $num) { Add-Row $label 'FileVersion' 'pass' "$fv (fixed $fixed.$($vi.FilePrivatePart))" }
        else { Add-Row $label 'FileVersion' 'fail' "string $fv but the fixed version is $fixed" }
    } else { Add-Row $label 'FileVersion' 'fail' "'$fv', expected $Version" }
    if ($vi.OriginalFilename -eq $original) { Add-Row $label 'OriginalFilename' 'pass' $vi.OriginalFilename }
    elseif ($vi.OriginalFilename) { Add-Row $label 'OriginalFilename' 'warn' "'$($vi.OriginalFilename)', expected '$original'" }
    else { Add-Row $label 'OriginalFilename' 'fail' 'missing: Smart App Control and WDAC rules key on it' }
    if ($vi.FileDescription) { Add-Row $label 'FileDescription' 'pass' $vi.FileDescription } else { Add-Row $label 'FileDescription' 'fail' "missing: Task Manager and the UAC/SmartScreen dialogs show it" }
    if ($vi.LegalCopyright) { Add-Row $label 'LegalCopyright' 'pass' $vi.LegalCopyright } else { Add-Row $label 'LegalCopyright' 'warn' 'missing' }
}

# ---- 3. manifest ----

function Test-Manifest([string]$label, [string]$text, [bool]$needPmV2) {
    if (-not $text) { Add-Row $label 'manifest' 'fail' 'no embedded manifest: Windows guesses (virtualisation, UAC installer detection, bitmap-scaled DPI)'; return }
    try { [xml]$x = $text } catch { Add-Row $label 'manifest' 'fail' "not XML: $_"; return }
    $level = [regex]::Match($text, 'requestedExecutionLevel[^>]*level="([^"]+)"').Groups[1].Value
    $ui = [regex]::Match($text, 'requestedExecutionLevel[^>]*uiAccess="([^"]+)"').Groups[1].Value
    if ($level -eq 'asInvoker' -and $ui -ne 'true') { Add-Row $label 'execution level' 'pass' "asInvoker (uiAccess $(if ($ui) { $ui } else { 'unset' }))" }
    elseif ($level) { Add-Row $label 'execution level' 'fail' "$level / uiAccess ${ui}: a per-user app must run asInvoker" }
    else { Add-Row $label 'execution level' 'fail' "no requestedExecutionLevel: Windows' installer detection may ask for admin" }
    $aw = [regex]::Match($text, '<(?:\w+:)?dpiAwareness[^>]*>([^<]+)<').Groups[1].Value
    $old = [regex]::Match($text, '<(?:\w+:)?dpiAware[^>]*>([^<]+)<').Groups[1].Value
    if ($needPmV2) {
        if ($aw -match 'PerMonitorV2') { Add-Row $label 'DPI awareness' 'pass' "dpiAwareness $aw, dpiAware $old" }
        else { Add-Row $label 'DPI awareness' 'fail' "dpiAwareness '$aw' (expected PerMonitorV2)" }
    } elseif ($aw -or $old) { Add-Row $label 'DPI awareness' 'pass' "dpiAware $old $aw" }
    else { Add-Row $label 'DPI awareness' 'warn' 'not DPI aware: blurry on a scaled display' }
    $os = @([regex]::Matches($text, 'supportedOS\b[^>]*\bId="\{([0-9a-fA-F-]+)\}"') | ForEach-Object { $_.Groups[1].Value.ToLowerInvariant() })
    if ($os -contains '8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a') { Add-Row $label 'supportedOS' 'pass' "Windows 10/11 GUID present ($($os.Count) entries)" }
    else { Add-Row $label 'supportedOS' 'fail' "no Windows 10/11 supportedOS: GetVersionEx lies and some APIs run in compatibility mode" }
}

# ---- 6. hardening ----

function Test-Hardening([string]$label, $info, [bool]$strict) {
    $flags = [int]$info.DllCharacteristics
    $want = [ordered]@{ 'ASLR (DYNAMIC_BASE)' = 0x40; 'DEP (NX_COMPAT)' = 0x100 }
    if ($info.Plus) { $want['high-entropy ASLR'] = 0x20 }
    if ($strict) { $want['Control Flow Guard'] = 0x4000 }
    $missing = @($want.Keys | Where-Object { -not ($flags -band $want[$_]) })
    if ($missing.Count -eq 0) { Add-Row $label 'hardening' 'pass' (($want.Keys) -join ', ') }
    else { Add-Row $label 'hardening' 'warn' "missing: $($missing -join ', ') (flags 0x$('{0:x}' -f $flags))" }
}

Test-VersionInfo 'qcalc.exe' $Exe 'qcalc.exe'
Test-Manifest 'qcalc.exe' $exeInfo.Manifest $true
Test-Hardening 'qcalc.exe' $exeInfo $true
$instInfo = $null
if ($Installer) {
    $instInfo = [QCalcSign.Pe]::Read($Installer)
    Test-VersionInfo 'installer' $Installer (Split-Path $Installer -Leaf)
    Test-Manifest 'installer' $instInfo.Manifest $false
    Test-Hardening 'installer' $instInfo $false
} else {
    Add-Row 'installer' 'present' 'warn' 'no installer to check (native\windows\installer\build.ps1 makes one)'
}

# ---- 4. sign copies with a throwaway certificate ----

function Test-Signed([string]$label, [string]$path) {
    $s = Get-AuthenticodeSignature $path
    $c = $s.SignerCertificate
    if (-not $c) { Add-Row $label 'signature' 'fail' "not signed ($($s.Status) $($s.StatusMessage))"; return }
    if ($c.Thumbprint -ne $cert.Thumbprint) { Add-Row $label 'signature' 'fail' "signed by $($c.Subject), not the mock certificate"; return }
    # a self-signed root can't be trusted: UnknownError with the untrusted-root message is the structure being right
    if ($s.Status -eq 'Valid' -or ($s.Status -eq 'UnknownError' -and $s.StatusMessage -match 'root')) {
        Add-Row $label 'signature' 'pass' "Authenticode $($s.SignatureType), hash intact, ends in the mock root as expected ($($s.Status))"
    } else { Add-Row $label 'signature' 'fail' "$($s.Status): $($s.StatusMessage)" }
    $digest = $null
    try {
        Add-Type -AssemblyName System.Security
        $cms = New-Object System.Security.Cryptography.Pkcs.SignedCms
        $raw = [IO.File]::ReadAllBytes($path)
        $info = [QCalcSign.Pe]::Read($path)
        $peOff = [BitConverter]::ToUInt32($raw, 0x3c)
        $dirs = $peOff + 24 + $(if ($info.Plus) { 112 } else { 96 })
        $at = [BitConverter]::ToUInt32($raw, $dirs + 32); $len = [BitConverter]::ToUInt32($raw, $dirs + 36)
        # WIN_CERTIFICATE: dwLength, wRevision, wCertificateType, then the PKCS#7
        $pkcs = New-Object byte[] ([BitConverter]::ToUInt32($raw, $at) - 8)
        [Array]::Copy($raw, $at + 8, $pkcs, 0, $pkcs.Length)
        $cms.Decode($pkcs)
        $digest = $cms.SignerInfos[0].DigestAlgorithm.FriendlyName
        if ($at + $len -ne $raw.Length) { Add-Row $label 'certificate table' 'warn' "data after the signature ($($raw.Length - $at - $len) bytes): signtool verify /pa rejects that" }
    } catch { $digest = "unreadable ($($_.Exception.Message))" }
    if ($digest -match 'sha256') { Add-Row $label 'digest' 'pass' $digest } else { Add-Row $label 'digest' 'fail' "$digest (SHA-256 required)" }
    if ($s.TimeStamperCertificate) { Add-Row $label 'timestamp' 'pass' "RFC 3161 by $($s.TimeStamperCertificate.Subject)" }
    else { Add-Row $label 'timestamp' 'warn' 'none: a real release must be timestamped' }
}


function New-MockCert([string]$subject) {
    Add-Type -AssemblyName System.Security
    $X = 'System.Security.Cryptography.X509Certificates'
    $rsa = New-Object System.Security.Cryptography.RSACng 3072
    $req = New-Object "$X.CertificateRequest" ($subject, $rsa, [Security.Cryptography.HashAlgorithmName]::SHA256, [Security.Cryptography.RSASignaturePadding]::Pkcs1)
    $oids = New-Object System.Security.Cryptography.OidCollection
    [void]$oids.Add((New-Object System.Security.Cryptography.Oid '1.3.6.1.5.5.7.3.3'))
    $req.CertificateExtensions.Add((New-Object "$X.X509EnhancedKeyUsageExtension" ($oids, $false)))
    $req.CertificateExtensions.Add((New-Object "$X.X509KeyUsageExtension" ([Security.Cryptography.X509Certificates.X509KeyUsageFlags]::DigitalSignature, $true)))
    $req.CertificateExtensions.Add((New-Object "$X.X509BasicConstraintsExtension" ($false, $false, 0, $true)))
    $made = $req.CreateSelfSigned([DateTimeOffset]::Now.AddHours(-1), [DateTimeOffset]::Now.AddDays(1))
    $pw = [Guid]::NewGuid().ToString('N')
    $flags = [Security.Cryptography.X509Certificates.X509KeyStorageFlags]'UserKeySet, PersistKeySet'
    $kept = New-Object "$X.X509Certificate2" ($made.Export([Security.Cryptography.X509Certificates.X509ContentType]::Pfx, $pw), $pw, $flags)
    $store = New-Object "$X.X509Store" ('My', 'CurrentUser')
    $store.Open('ReadWrite')
    try { $store.Add($kept) } finally { $store.Close() }
    Get-Item "Cert:\CurrentUser\My\$($kept.Thumbprint)"
}

$cert = $null
$stores = 'Cert:\CurrentUser\My', 'Cert:\CurrentUser\CA', 'Cert:\CurrentUser\Root', 'Cert:\CurrentUser\TrustedPublisher'
try {
    # made in memory and added to CurrentUser\My alone (New-SelfSignedCertificate would also drop a copy in
    # CurrentUser\CA): RSA 3072, code signing only, a day long
    $cert = New-MockCert "CN=Q Calc mock signing $([Guid]::NewGuid().ToString('N').Substring(0, 8)) (throwaway)"
    Add-Row 'certificate' 'throwaway' 'info' "$($cert.Subject), $($cert.Thumbprint), Cert:\CurrentUser\My only, deleted at the end"
    $elsewhere = @($stores | Select-Object -Skip 1 | Where-Object { Test-Path (Join-Path $_ $cert.Thumbprint) })
    if ($elsewhere.Count) { Add-Row 'certificate' 'stores' 'warn' "also appeared in $($elsewhere -join ', ')" }

    $tsa = $null
    if (-not $NoTimestamp) {
        try {
            $r = Invoke-WebRequest -Uri $TimestampUrl -Method Head -UseBasicParsing -TimeoutSec 10 -ErrorAction Stop
            $tsa = $TimestampUrl
        } catch {
            # a TSA answers GET/HEAD with an error status, which still means it's reachable
            if ($_.Exception.Response) { $tsa = $TimestampUrl } else { Add-Row 'timestamp' 'reachable' 'warn' "$TimestampUrl didn't answer ($($_.Exception.Message)): signed without a timestamp, which a real release must not do (the signature would expire with the certificate)" }
        }
    } else { Add-Row 'timestamp' 'reachable' 'warn' '-NoTimestamp: signed without one' }

    $targets = @(@{ Label = 'qcalc.exe'; Path = $Exe })
    if ($Installer) { $targets += @{ Label = 'installer'; Path = $Installer } }
    foreach ($t in $targets) {
        $copy = Join-Path $OutDir ('signed-' + (Split-Path $t.Path -Leaf))
        Copy-Item $t.Path $copy -Force
        $before = (Get-AuthenticodeSignature $t.Path).Status
        if ($before -ne 'NotSigned') { Add-Row $t.Label 'unsigned before' 'warn' "already $before" }
        $p = @{ FilePath = $copy; Certificate = $cert; HashAlgorithm = 'SHA256' }
        if ($tsa) { $p.TimestampServer = $tsa }
        $s = Set-AuthenticodeSignature @p
        if ($tsa -and -not $s.TimeStamperCertificate) {
            # one retry without: a flaky TSA shouldn't hide the rest
            Add-Row $t.Label 'timestamp' 'warn' "the TSA didn't stamp it ($($s.StatusMessage)); signing again without"
            Copy-Item $t.Path $copy -Force
            $p.Remove('TimestampServer')
            $s = Set-AuthenticodeSignature @p
        }
        Test-Signed $t.Label $copy
        # a byte changed after signing must break the hash
        $tampered = Join-Path $OutDir ('tampered-' + (Split-Path $t.Path -Leaf))
        $bytes = [IO.File]::ReadAllBytes($copy)
        $bytes[0x400] = $bytes[0x400] -bxor 0xff
        [IO.File]::WriteAllBytes($tampered, $bytes)
        $ts = (Get-AuthenticodeSignature $tampered).Status
        if ($ts -eq 'HashMismatch') { Add-Row $t.Label 'tamper check' 'pass' 'a changed byte reads as HashMismatch' }
        else { Add-Row $t.Label 'tamper check' 'fail' "a changed byte reads as $ts" }
    }

    # ---- 5. the uninstaller, signed by makensis on the way ----
    if ($Installer -and -not $SkipUninstaller) {
        if (-not $Makensis) {
            $Makensis = (Get-Command makensis -ErrorAction SilentlyContinue).Source
            foreach ($c in "$env:LOCALAPPDATA\tauri\NSIS\makensis.exe", "${env:ProgramFiles(x86)}\NSIS\makensis.exe", "$env:ProgramFiles\NSIS\makensis.exe") {
                if (-not $Makensis -and $c -and (Test-Path $c)) { $Makensis = $c }
            }
        }
        if (-not $Makensis) { Add-Row 'uninstall.exe' 'signed by makensis' 'warn' 'no makensis: not checked' }
        else {
            $keep = Join-Path $OutDir 'nsis-signed'
            $setup = Join-Path $OutDir 'mock-signed-setup.exe'
            $ps = Join-Path $env:WINDIR 'System32\WindowsPowerShell\v1.0\powershell.exe'
            $ts = if ($tsa) { "-TimestampUrl $tsa" } else { '-NoTimestamp' }
            $sign = "`"$ps`" -NoProfile -ExecutionPolicy Bypass -File `"$PSCommandPath`" -Thumbprint $($cert.Thumbprint) $ts -KeepCopy `"$keep`" -SignOne"
            $num = ($Version -split '[-+]')[0]
            & $Makensis /V2 "/DVERSION=$Version" "/DVERSION_NUM=$num" "/DEXE=$Exe" "/DOUT=$setup" "/DSIGN=$sign" "$win\installer\qcalc.nsi" | Out-Host
            if ($LASTEXITCODE) { Add-Row 'uninstall.exe' 'signed by makensis' 'fail' "makensis with SIGN failed ($LASTEXITCODE)" }
            else {
                $un = Get-ChildItem $keep -Filter '*.signed' -ErrorAction SilentlyContinue |
                    Where-Object { $_.Name -ne ((Split-Path $setup -Leaf) + '.signed') } | Sort-Object LastWriteTime | Select-Object -First 1
                if ($un) { Test-Signed 'uninstall.exe' $un.FullName } else { Add-Row 'uninstall.exe' 'signed by makensis' 'fail' '!uninstfinalize never called the sign command' }
                Test-Signed 'installer (makensis !finalize)' $setup
            }
        }
    }
} finally {
    if ($cert) {
        foreach ($st in $stores) {
            $item = Join-Path $st $cert.Thumbprint
            if ($st -eq 'Cert:\CurrentUser\My' -and (Test-Path $item)) { Remove-Item $item -DeleteKey -ErrorAction SilentlyContinue; if (Test-Path $item) { Remove-Item $item -ErrorAction SilentlyContinue } }
        }
        $left = @($stores | Where-Object { Test-Path (Join-Path $_ $cert.Thumbprint) })
        if ($left.Count) { Add-Row 'certificate' 'deleted' 'fail' "still in $($left -join ', ')" } else { Add-Row 'certificate' 'deleted' 'pass' 'gone from every CurrentUser store' }
    }
}

# ---- report ----

Add-Row 'SmartScreen' 'reputation' 'info' "decided by Microsoft per certificate and file hash; a new certificate starts with none, so the first downloads still warn. An EV certificate or Azure Trusted/Artifact Signing gets it faster"
Add-Row 'Smart App Control' 'enforcement' 'info' 'blocks unsigned or untrusted-signed apps when on; only a real (CA-chained) signature lets qcalc.exe run there'

$fails = @($script:rows | Where-Object { $_.Result -eq 'fail' })
$warns = @($script:rows | Where-Object { $_.Result -eq 'warn' })
$md = New-Object System.Text.StringBuilder
[void]$md.AppendLine('## Mock signing check (native Windows)')
[void]$md.AppendLine()
[void]$md.AppendLine("Version $Version. $($fails.Count) failure(s), $($warns.Count) warning(s).")
[void]$md.AppendLine()
[void]$md.AppendLine('| file | check | result | detail |')
[void]$md.AppendLine('|---|---|---|---|')
foreach ($r in $script:rows) { [void]$md.AppendLine("| $($r.File) | $($r.Check) | $($r.Result) | $(($r.Detail -replace '\|', '/') -replace "`r?`n", ' ') |") }
$report = Join-Path $OutDir 'sign-check.md'
[IO.File]::WriteAllText($report, $md.ToString())
if ($env:GITHUB_STEP_SUMMARY) { Add-Content -Path $env:GITHUB_STEP_SUMMARY -Value $md.ToString() }
Write-Host ''
Write-Host "report: $report"
Write-Host ("{0} failure(s), {1} warning(s)" -f $fails.Count, $warns.Count)
if ($fails.Count) { exit 1 }
exit 0
