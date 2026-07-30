# Throne-rs

**Rust + [GPUI](https://github.com/zed-industries/zed) rewrite** of [Throne](https://github.com/throneproj/Throne) (formerly Nekoray) — a cross-platform desktop GUI proxy client powered by Sing-box / Xray.

> Status: **Wave B** — Clash/JSON/SIP008/WG subscription import + route profiles.  
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
             │ LoadConfig / Stats RPC (TODO)
┌────────────▼─────────────┐
│  throne-core-client      │
└────────────┬─────────────┘
             │ local socket + protobuf
┌────────────▼─────────────┐
│  core/server (Go)        │
│  sing-box · xray · TUN   │
└──────────────────────────┘
```

Legacy Qt/C++ GUI was removed on branch `rewrite/rust-gpui`. History remains on `dev`.

## Requirements

- Rust 1.85+ (edition 2021)
- macOS / Linux / Windows (GPUI)
- Optional: Go 1.26+ to build `core/server`

## Build & run

```bash
# GUI (demo data)
cargo run -p throne

# Unit tests
cargo test --workspace
```

Build the Go core (unchanged upstream flow; scripts were removed and will return as `cargo xtask`):

```bash
cd core/server
go build -o ../../target/Core .
```

## Upstream sync

```bash
git fetch upstream --prune
git log --oneline upstream/dev -30
# refresh parity notes after reading commits
```

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
4. **Wave C** — `libcore.proto` RPC Start/Stop/QueryStats + URL test
5. **Wave D** — System proxy / TUN, tray, traffic stats UI
6. **Wave E** — Feature parity polish + packaging

## License

GPL-3.0-or-later (same family as upstream Throne). See [LICENSE](./LICENSE).

## Credits

- Upstream: [throneproj/Throne](https://github.com/throneproj/Throne)
- [SagerNet/sing-box](https://github.com/SagerNet/sing-box), [XTLS/Xray-core](https://github.com/xtls/xray-core)
- UI framework: [Zed GPUI](https://github.com/zed-industries/zed)
