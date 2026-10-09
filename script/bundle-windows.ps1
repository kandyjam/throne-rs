# Windows packaging — mirrors zed-industries/zed `script/bundle-windows.ps1`.
#
# Builds Throne.exe + ThroneCore.exe and produces an Inno Setup installer.
#
# Prerequisites:
#   - Rust + Go 1.26+ + protoc (protobuf)
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
    $CoreOut = [System.IO.Path]::GetFullPath($CoreOut)
    $dir = Split-Path -Parent $CoreOut
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $go = Find-Go
    if ($go) {
        Write-Step "Building Go ThroneCore with $go"
        if (-not (Get-Command protoc -ErrorAction SilentlyContinue)) {
            throw "protoc not found; install Protocol Buffers before building ThroneCore"
        }
        $savedPath = $env:PATH
        $savedGoOS = $env:GOOS
        $savedGoArch = $env:GOARCH
        $savedCgo = $env:CGO_ENABLED
        Push-Location (Join-Path $Root "core")
        try {
            $pluginDir = ([string](& $go env GOBIN)).Trim()
            if ($LASTEXITCODE -ne 0) { throw "go env GOBIN failed" }
            if (-not $pluginDir) {
                $goPath = (& $go env GOPATH).Trim()
                if ($LASTEXITCODE -ne 0) { throw "go env GOPATH failed" }
                $pluginDir = Join-Path ($goPath.Split(';')[0]) "bin"
            }
            $env:PATH = "$pluginDir;$savedPath"
            # Install generator executables for the host before setting the core target.
            $env:GOOS = $null
            $env:GOARCH = $null
            if (-not (Get-Command protoc-gen-go -ErrorAction SilentlyContinue)) {
                & $go install google.golang.org/protobuf/cmd/protoc-gen-go@v1.36.11
                if ($LASTEXITCODE -ne 0) { throw "install protoc-gen-go failed" }
            }
            if (-not (Get-Command protoc-gen-go-grpc -ErrorAction SilentlyContinue)) {
                & $go install google.golang.org/grpc/cmd/protoc-gen-go-grpc@v1.6.2
                if ($LASTEXITCODE -ne 0) { throw "install protoc-gen-go-grpc failed" }
            }
            & protoc -I gen --go_out=gen --go-grpc_out=gen `
                --go_opt=paths=source_relative --go-grpc_opt=paths=source_relative gen/libcore.proto
            if ($LASTEXITCODE -ne 0) { throw "generate core protobuf failed" }

            # Match upstream 1.4.0-beta.1 modern Windows builds.
            $env:GOOS = "windows"
            $env:GOARCH = if ($Architecture -eq "aarch64") { "arm64" } else { "amd64" }
            $env:CGO_ENABLED = "0"
            $tags = if ($env:THRONE_CORE_TAGS) { $env:THRONE_CORE_TAGS } else {
                "with_clash_api,with_quic,with_wireguard,with_utls,with_dhcp,with_tailscale,with_openvpn,with_openconnect,badlinkname,tfogo_checklinkname0,with_purego,with_naive_outbound"
            }
            if (($tags -split ',') -contains "with_naive_outbound") {
                $cronetModule = "github.com/sagernet/cronet-go/lib/windows_$($env:GOARCH)"
                & $go mod download $cronetModule
                if ($LASTEXITCODE -ne 0) { throw "download Cronet library failed" }
                $cronetDir = (& $go list -m -f '{{.Dir}}' $cronetModule).Trim()
                if ($LASTEXITCODE -ne 0) { throw "locate Cronet library failed" }
                Copy-Item -Force (Join-Path $cronetDir "libcronet.dll") (Join-Path $dir "libcronet.dll")
            }
            $singboxVersion = (& $go list -m -f '{{.Version}}' github.com/sagernet/sing-box).Trim()
            if ($LASTEXITCODE -ne 0) { throw "read sing-box version failed" }
            $ldflags = "-s -w -X 'github.com/sagernet/sing-box/constant.Version=$singboxVersion' -X 'internal/godebug.defaultGODEBUG=multipathtcp=0' -checklinkname=0"
            & $go build -trimpath -tags $tags -ldflags $ldflags -o $CoreOut .
            if ($LASTEXITCODE -ne 0) { throw "go build failed" }
        } finally {
            Pop-Location
            $env:PATH = $savedPath
            $env:GOOS = $savedGoOS
            $env:GOARCH = $savedGoArch
            $env:CGO_ENABLED = $savedCgo
        }
        return
    }
    $existing = Find-ExistingCore (Split-Path -Parent $CoreOut)
    if ($existing) {
        Write-Step "go not on PATH — copying prebuilt ThroneCore from $existing"
        if ([System.IO.Path]::GetFullPath($existing) -ne $CoreOut) {
            Copy-Item -Force $existing $CoreOut
            $existingCronet = Join-Path (Split-Path -Parent $existing) "libcronet.dll"
            if (Test-Path $existingCronet) {
                Copy-Item -Force $existingCronet (Join-Path $dir "libcronet.dll")
            }
        }
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
$OutName = "ThroneRs-$Architecture"
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
$portableFiles = @($guiOut, $coreOut)
$cronetDll = Join-Path $ReleaseDir "libcronet.dll"
if (Test-Path $cronetDll) { $portableFiles += $cronetDll }
Compress-Archive -Path $portableFiles -DestinationPath $ZipPath -Force

Write-Step "Windows bundle complete"
Get-ChildItem $Dist -Filter "ThroneRs*" | Format-Table Name, Length
