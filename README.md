# Throne-rs

**Rust + [GPUI](https://github.com/zed-industries/zed) rewrite** of [Throne](https://github.com/throneproj/Throne) (formerly Nekoray) — a cross-platform desktop GUI proxy client powered by Sing-box / Xray.

> Status: **Milestone 0 — UI shell**. Domain models and a GPUI main window run with demo data. The Go core is retained; the local-socket protobuf RPC client is stubbed.

## Architecture

| Layer | Crate / path | Role |
|-------|----------------|------|
| UI | `crates/throne` | GPUI application (main window, lists, actions) |
| Domain | `crates/throne-domain` | Profiles, groups, runtime status, in-memory store |
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

## Roadmap

1. **M0** — GPUI shell, domain models, demo store ✅
2. **M1** — SQLite persistence (profiles / groups / settings)
3. **M2** — `libcore.proto` Rust client + core process lifecycle
4. **M3** — Subscription import, share-link parse, config build
5. **M4** — System proxy / TUN elevation, tray, hotkeys
6. **M5** — Latency / speed tests, traffic charts, connections view
7. **M6** — Feature parity polish + packaging

## License

GPL-3.0-or-later (same family as upstream Throne). See [LICENSE](./LICENSE).

## Credits

- Upstream: [throneproj/Throne](https://github.com/throneproj/Throne)
- [SagerNet/sing-box](https://github.com/SagerNet/sing-box), [XTLS/Xray-core](https://github.com/xtls/xray-core)
- UI framework: [Zed GPUI](https://github.com/zed-industries/zed)
