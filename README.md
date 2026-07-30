# Throne-rs

**Rust + [GPUI](https://github.com/zed-industries/zed) rewrite** of [Throne](https://github.com/throneproj/Throne) (formerly Nekoray) — a cross-platform desktop GUI proxy client powered by Sing-box / Xray.

**Version:** `1.2.2` (aligned with `upstream/dev` tip tag / `NKR_VERSION` — `git describe upstream/dev --tags`; see root [`VERSION`](./VERSION))

> Status: **Wave C+** — Start/Stop with **route + MetaCubeX srslist + jsDelivr mirror + optional adblock** + URL Test/Stats/Connections + HTTP sub diff + macOS system proxy + Edit Profile rename.  
> Upstream remote: `upstream` → [throneproj/Throne](https://github.com/throneproj/Throne) (`dev`).  
> Parity matrix: [docs/UPSTREAM_TRACKING.md](./docs/UPSTREAM_TRACKING.md).

## Architecture

| Layer | Crate / path | Role |
|-------|----------------|------|
| UI | `crates/throne` | GPUI application (main window, lists, actions) |
| Domain | `crates/throne-domain` | Profiles, groups, settings, runtime status |
| Import | `crates/throne-import` | Share links + `throne://` deeplinks (GroupUpdater-aligned) |
| Storage | `crates/throne-storage` | SQLite schema compatible with upstream repos |
| Core client | `crates/throne-core-client` | Async API toward the Go `ThroneCore` process |
| Data plane | `core/server` | Existing Go core (sing-box / xray, TUN, DNS, tests) |

```
┌──────────────────────────┐
│  throne (GPUI)           │
│  profiles · groups · TUN │
└────────────┬─────────────┘
             │ domain state
┌────────────▼─────────────┐
│  throne-domain           │
└────────────┬─────────────┘
             │ build config + Start/Stop
┌────────────▼─────────────┐
│  throne-core-client      │
└────────────┬─────────────┘
             │ unix socket + length-prefixed protobuf
┌────────────▼─────────────┐
│  ThroneCore (Go)         │
│  sing-box · xray · TUN   │
└──────────────────────────┘
```

Legacy Qt/C++ GUI was removed on branch `rewrite/rust-gpui`. History remains on `dev`.

## Requirements

- Rust 1.85+ (edition 2021)
- macOS / Linux / Windows (GPUI)
- **Go core binary** for real proxy: `ThroneCore` (or `Core`) next to the GUI, on `PATH`, or `THRONE_CORE=/path/to/ThroneCore`

## Build & run

```bash
# 1) Build Go core (required for Start to work)
cd core/server
go build -o ../../target/debug/ThroneCore .
cd ../..

# 2) GUI (binary is named `Throne` — required by ThroneCore parentcheck)
cargo run -p throne
# On Start, ThroneCore is auto-copied next to target/debug/Throne from
# /Applications/Throne.app or THRONE_CORE=...

# Unit tests
cargo test --workspace
```

## Packaging (Zed-style)

Same approach as [Zed](https://github.com/zed-industries/zed): `cargo-bundle` + `script/bundle-*`.

| Platform | Command | Artifact |
|----------|---------|----------|
| macOS | `./script/bundle-mac` | `dist/Throne-*.dmg` |
| Linux | `./script/bundle-linux` `[--deb] [--rpm]` | `dist/throne-linux-*.tar.gz` (+ deb/rpm) |
| Windows | `.\script\bundle-windows.ps1` | Inno Setup `dist/Throne-*.exe` |

```bash
# one-time (macOS)
cargo install cargo-bundle --locked

./script/package                 # auto-detect host
./script/bundle-mac
./script/bundle-linux --deb
```

Details: [`docs/PACKAGING.md`](./docs/PACKAGING.md) · CI: [`.github/workflows/package.yml`](./.github/workflows/package.yml).

On **Start**:
1. Builds a minimal sing-box config (mixed inbound + selected outbound)
2. Spawns `ThroneCore` with `THRONE_CORE_SOCKET`
3. Sends `Start` RPC (same framing as Qt Throne)
4. If **System Proxy** is checked, enables macOS HTTP/HTTPS/SOCKS via `networksetup`

Without a core binary, Start shows a clear error instead of a fake “Running” state.

## Upstream sync

```bash
git fetch upstream --prune
git log --oneline upstream/dev -30
# refresh parity notes after reading commits
```

### Use an existing Throne database

The client opens **original** `throne.db` when found (same layout as Qt Throne):

| Priority | Path |
|----------|------|
| 1 | `$THRONE_DB` |
| 2 | Discovered: `<app>/config/throne.db`, `~/Library/Application Support/Throne/config/throne.db`, `~/.config/Throne/config/throne.db`, … |
| 3 | New: OS data dir `…/throne-rs/throne.db` |

```bash
# Point at a portable install
export THRONE_DB="/path/to/Throne/config/throne.db"
cargo run -p throne
```

Schema is wire-compatible (`profiles` / `groups` / `groups_order` / `route_profiles` + `route_rules` / `settings` / `entity_ids`). Unknown settings keys are preserved on save. Traffic stats stay in sibling `throne_stats.db` (not rewritten yet).

Import share links (clipboard or env):

```bash
export THRONE_IMPORT='vless://uuid@host:443?security=tls&type=ws&path=%2F#demo'
cargo run -p throne
# then press Import / ⌘V
```

## Roadmap

1. **M0** — GPUI shell, domain models, demo store ✅
2. **Wave A** — SQLite + share-link import + upstream tracking doc ✅
3. **Wave B** — Clash/JSON/SIP008/WG sub + route share import ✅
4. **Wave C** — `libcore.proto` RPC Start/Stop/QueryStats + URL test + route rules/rule_set compile ✅
5. **Wave D** — System proxy ✅ / TUN (needs privileges) · tray · traffic stats UI
6. **Wave E** — Feature parity polish + packaging

## License

GPL-3.0-or-later (same family as upstream Throne). See [LICENSE](./LICENSE).

## Credits

- Upstream: [throneproj/Throne](https://github.com/throneproj/Throne)
- [SagerNet/sing-box](https://github.com/SagerNet/sing-box), [XTLS/Xray-core](https://github.com/xtls/xray-core)
- UI framework: [Zed GPUI](https://github.com/zed-industries/zed)
