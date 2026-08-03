# Packaging (Zed-style)

Packaging follows the same layout and flow as
[zed-industries/zed](https://github.com/zed-industries/zed):

| Concern | Zed | Throne-rs |
|---------|-----|-----------|
| macOS `.app` | `cargo-bundle` + `script/bundle-mac` | same |
| macOS DMG | `hdiutil` in `bundle-mac` | same |
| Linux primary | portable **tar.gz** app dir | same (`throne-linux-<arch>.tar.gz`) |
| Linux deb/rpm | community / optional | optional `--deb` / `--rpm` |
| Windows installer | **Inno Setup** (`bundle-windows.ps1`) | same (not NSIS) |
| Bundle metadata | `crates/zed` `[package.metadata.bundle]` | `crates/throne` same table |
| Resources | `crates/zed/resources/` | `crates/throne/resources/` |
| Scripts | `script/bundle-*` | `script/bundle-*` |

## Artifacts

| Platform | Command | Output (`dist/`) |
|----------|---------|------------------|
| macOS | `./script/bundle-mac` | `ThroneRs-<ver>-<arch>.dmg`, `ThroneRs.app` |
| Linux | `./script/bundle-linux` | `throne-linux-<arch>.tar.gz` |
| Linux DEB | `./script/bundle-linux --deb` | `throne_<ver>_<arch>.deb` |
| Linux RPM | `./script/bundle-linux --rpm` | `throne-<ver>-1.<arch>.rpm` |
| Windows | `.\script\bundle-windows.ps1` | `ThroneRs-<arch>.exe` (Inno), portable `.zip` |

Display name is **ThroneRs**; every package still places the **`Throne`** binary and
**`ThroneCore` in the same directory** (upstream Go `parentcheck`). Shared DB
paths remain under upstream `Throne` locations.

## Quick start

```bash
# Prerequisites
#   cargo  — Rust toolchain
#   go     — optional if a prebuilt ThroneCore already exists (see below)
#   cargo-bundle (macOS), Inno Setup 6 (Windows)

# macOS
cargo install cargo-bundle --locked   # once
./script/bundle-mac
# optional signing:
# SIGN_IDENTITY="Developer ID Application: You (TEAMID)" ./script/bundle-mac

# Linux
./script/bundle-linux
./script/bundle-linux --deb
./script/bundle-linux --deb --rpm     # RPM needs `fpm` (gem install fpm)

# Windows (PowerShell)
# Install Inno Setup 6: https://jrsoftware.org/isinfo.php
.\script\bundle-windows.ps1
.\script\bundle-windows.ps1 -Architecture aarch64

# Host auto-detect
./script/package
```

### Go / ThroneCore

`script/lib.sh` builds `ThroneCore` from `core/server` when `go` is available
(`PATH`, Homebrew, `/usr/local/go`, or `GO_BIN`).

If Go is **not** installed, the scripts reuse a prebuilt core from (first hit wins):

1. `$THRONE_CORE`
2. `target/release/ThroneCore` / `target/debug/ThroneCore`
3. `/Applications/Throne.app/Contents/MacOS/ThroneCore` (macOS)

```bash
brew install go                          # recommended for release builds
# or reuse an existing binary:
export THRONE_CORE=/path/to/ThroneCore
./script/package
```

## Layout

```
script/
  lib.sh                 # shared version/arch/build helpers
  bundle-mac             # cargo-bundle → .app → DMG
  bundle-linux           # tar.gz (+ optional deb/rpm)
  bundle-windows.ps1     # Inno Setup + zip
  package                # dispatcher
crates/throne/
  Cargo.toml             # [package.metadata.bundle]  (cargo-bundle)
  resources/
    app-icon.png         # + @2x / size variants / .icns / .ico
    throne.desktop.in    # envsubst-style ${EXEC_PATH} ${ICON_PATH}
    throne.entitlements  # hardened runtime (macOS codesign)
    windows/throne.iss   # Inno Setup script
```

## cargo-bundle metadata

```toml
# crates/throne/Cargo.toml
[package.metadata.bundle]
name = "Throne"
identifier = "app.throne.desktop"
icon = ["resources/app-icon.png", "resources/app-icon@2x.png", ...]
osx_minimum_system_version = "11.0"
osx_url_schemes = ["throne"]
category = "Utility"
```

This is the same table Zed uses (`bundle` / channel-specific `bundle-stable` …).
Throne keeps a single `bundle` section (no release channels yet).

## Linux tarball layout

```
throne-linux-aarch64/
  Throne                 # GUI (required name)
  ThroneCore             # Go core (same directory)
  Throne.desktop
  Throne.png
  bin/throne             # thin launcher
  icons/hicolor/…/Throne.png
  LICENSE
```

DEB/RPM install to `/opt/Throne/{Throne,ThroneCore}` with `/usr/bin/Throne` wrapper
(aligned with upstream Qt `pack_debian.sh`).

## Windows (Inno Setup)

`crates/throne/resources/windows/throne.iss` is compiled by
`script/bundle-windows.ps1` with `/DAppVersion=…`, `/DSourceDir=…`, etc. —
same pattern as Zed’s `zed.iss` + `bundle-windows.ps1`.

Registers `throne://` URL protocol for the current user.

## CI

[`.github/workflows/package.yml`](../.github/workflows/package.yml) runs the
three bundle scripts on tag `v*` or `workflow_dispatch`:

- `macos-14` / `macos-13` → DMG  
- `ubuntu-22.04` (+ arm) → tar.gz + deb  
- `windows-latest` (+ arm) → Inno `.exe` + zip  

## Version

`VERSION` + workspace `version` drive artifact names. Bump both when cutting a
release.

## Why not cargo-packager / NSIS?

Earlier drafts used cargo-packager (dmg/deb/nsis). Packaging was switched to
**match Zed** because this app is also GPUI-based and Zed’s scripts are
battle-tested for:

- `.app` generation via **cargo-bundle**
- **DMG** creation/signing hooks
- Linux **portable tarball** as the first-class artifact
- Windows **Inno Setup** (not NSIS)

Optional DEB/RPM remain available for distro installs but are not the primary
Linux ship format (same as Zed).
