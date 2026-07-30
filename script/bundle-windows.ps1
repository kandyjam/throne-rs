# Windows packaging — mirrors zed-industries/zed `script/bundle-windows.ps1`.
#
# Builds Throne.exe + ThroneCore.exe and produces an Inno Setup installer.
#
# Prerequisites:
#   - Rust + Go
#   - Inno Setup 6 (ISCC.exe on PATH or default install dir)
#
# Usage (from repo root, PowerShell):
#   .\script\bundle-windows.ps1
#   .\script\bundle-windows.ps1 -Architecture x86_64
#   .\script\bundle-windows.ps1 -Architecture aarch64

param(
    [ValidateSet("x86_64", "aarch64")]
    [string]$Architecture = "x86_64",
    [string]$Configuration = "release"
)

$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $Root

$Version = (Get-Content -Raw (Join-Path $Root "VERSION")).Trim()
$Dist = Join-Path $Root "dist"
$TargetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $Root "target" }
$ReleaseDir = Join-Path $TargetDir $Configuration

New-Item -ItemType Directory -Force -Path $Dist | Out-Null
New-Item -ItemType Directory -Force -Path $ReleaseDir | Out-Null

function Write-Step($msg) { Write-Host "==> $msg" }

# Map arch → rustc target (host build by default)
$RustTarget = $null
switch ($Architecture) {
    "x86_64"  { if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { $RustTarget = "x86_64-pc-windows-msvc" } }
    "aarch64" { if ($env:PROCESSOR_ARCHITECTURE -ne "ARM64") { $RustTarget = "aarch64-pc-windows-msvc" } }
}

function Find-Go {
    if ($env:GO_BIN -and (Test-Path $env:GO_BIN)) { return $env:GO_BIN }
    $cmd = Get-Command go -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    foreach ($c in @(
        "C:\Program Files\Go\bin\go.exe",
        "$env:USERPROFILE\sdk\go\bin\go.exe",
        "$env:LOCALAPPDATA\Programs\Go\bin\go.exe"
    )) {
        if (Test-Path $c) { return $c }
    }
    return $null
}

function Find-ExistingCore([string]$PreferDir) {
    if ($env:THRONE_CORE -and (Test-Path $env:THRONE_CORE)) { return $env:THRONE_CORE }
    foreach ($c in @(
        (Join-Path $PreferDir "ThroneCore.exe"),
        (Join-Path $TargetDir "release\ThroneCore.exe"),
        (Join-Path $TargetDir "debug\ThroneCore.exe"),
        (Join-Path $Root "target\release\ThroneCore.exe"),
        (Join-Path $Root "target\debug\ThroneCore.exe")
    )) {
        if (Test-Path $c) { return $c }
    }
    return $null
}

function Ensure-Core([string]$CoreOut) {
    $dir = Split-Path -Parent $CoreOut
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $go = Find-Go
    if ($go) {
        Write-Step "Building Go ThroneCore with $go"
        Push-Location (Join-Path $Root "core\server")
        & $go build -trimpath -ldflags="-s -w" -o $CoreOut .
        $code = $LASTEXITCODE
        Pop-Location
        if ($code -ne 0) { throw "go build failed" }
        return
    }
    $existing = Find-ExistingCore (Split-Path -Parent $CoreOut)
    if ($existing) {
        Write-Step "go not on PATH — copying prebuilt ThroneCore from $existing"
        Copy-Item -Force $existing $CoreOut
        return
    }
    throw @"
ThroneCore unavailable: 'go' not found and no prebuilt core located.

Install Go from https://go.dev/dl/ (or: winget install GoLang.Go)
Or set THRONE_CORE to an existing ThroneCore.exe
"@
}

if ($RustTarget) {
    $ReleaseDir = Join-Path $TargetDir (Join-Path $RustTarget $Configuration)
    New-Item -ItemType Directory -Force -Path $ReleaseDir | Out-Null
}

$coreOut = Join-Path $ReleaseDir "ThroneCore.exe"
Ensure-Core $coreOut

Write-Step "Building Rust Throne ($Configuration)"
$cargoArgs = @("build", "-p", "throne", "--profile", $Configuration)
if ($RustTarget) {
    $cargoArgs += @("--target", $RustTarget)
}
& cargo @cargoArgs
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$guiOut = Join-Path $ReleaseDir "Throne.exe"
if (-not (Test-Path $guiOut)) { throw "missing $guiOut" }
if (-not (Test-Path $coreOut)) { throw "missing $coreOut" }

Write-Step "Binaries ready"
Get-Item $guiOut, $coreOut | Format-Table Name, Length

# Locate Inno Setup compiler (Zed uses ISCC similarly)
$Iscc = $null
$candidates = @(
    "ISCC.exe",
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
    "${env:LOCALAPPDATA}\Programs\Inno Setup 6\ISCC.exe"
)
foreach ($c in $candidates) {
    if ($c -eq "ISCC.exe") {
        $cmd = Get-Command ISCC.exe -ErrorAction SilentlyContinue
        if ($cmd) { $Iscc = $cmd.Source; break }
    } elseif (Test-Path $c) {
        $Iscc = $c
        break
    }
}
if (-not $Iscc) {
    throw "Inno Setup 6 not found. Install from https://jrsoftware.org/isinfo.php"
}

$Iss = Join-Path $Root "crates\throne\resources\windows\throne.iss"
$OutName = "Throne-$Architecture"
Write-Step "Inno Setup → $Dist\$OutName.exe"

& $Iscc `
    "/DAppVersion=$Version" `
    "/DArch=$Architecture" `
    "/DSourceDir=$ReleaseDir" `
    "/DOutputDir=$Dist" `
    "/DOutputBaseFilename=$OutName" `
    $Iss

if ($LASTEXITCODE -ne 0) { throw "ISCC failed" }

# Also publish portable zip (handy for CI / winget-less installs)
$ZipName = "throne-windows-$Architecture.zip"
$ZipPath = Join-Path $Dist $ZipName
if (Test-Path $ZipPath) { Remove-Item $ZipPath -Force }
Write-Step "Portable zip → $ZipPath"
Compress-Archive -Path $guiOut, $coreOut -DestinationPath $ZipPath -Force

Write-Step "Windows bundle complete"
Get-ChildItem $Dist -Filter "Throne*" | Format-Table Name, Length
