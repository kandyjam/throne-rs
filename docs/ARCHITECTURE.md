# Throne-rs architecture

## Goals

- Replace the Qt/C++ GUI with **Rust + GPUI** while keeping the battle-tested **Go core**.
- Keep domain logic UI-agnostic and testable without a window.
- Match upstream Throne user workflows (profiles, groups, subscriptions, system proxy, TUN).

## Process model

Unchanged from upstream:

1. GUI starts (or attaches to) a privileged/unprivileged **Core** binary.
2. Control plane is a **local socket** carrying **protobuf** messages defined in `core/server/gen/libcore.proto`.
3. Data plane (proxy, TUN, DNS) lives entirely inside the Go core / sing-box / xray.

## Crate boundaries

### `throne-domain`

- Pure data: `Profile`, `Group`, `CoreStatus`, `SystemMode`, `TrafficSnapshot`.
- `AppState` is the single mutable store the UI renders from.
- No GPUI, no tokio, no filesystem (persistence will be a separate crate later).

### `throne-core-client`

- Owns `CoreSession` lifecycle (spawn, RPC, shutdown).
- Today: stubs returning `CoreError::NotImplemented`.
- Tomorrow: framing compatible with the Qt `API::Client` local-socket codec.

### `throne-import`

- Share-link parsers aligned with upstream `GroupUpdater` / `RawUpdater`.
- `throne://add|route|remoteRoute|addsub/` deep links (`e7eb0438`).

### `throne-storage`

- SQLite tables matching upstream `profiles` / `groups` / `groups_order` / `settings`.
- Default path under the OS data directory (`throne-rs/throne.db`).

### `throne` (binary)

- GPUI `Application` + `MainWindow`.
- Maps user actions → `AppState` mutations (and later async core commands).
- Theme tokens in `theme.rs`.
- Import (clipboard / `THRONE_IMPORT`) + Save DB shortcuts.

See [UPSTREAM_TRACKING.md](./UPSTREAM_TRACKING.md) for the live parity matrix against `throneproj/Throne`.

## UI map (M0)

| Region | Behavior |
|--------|----------|
| Toolbar | Start/Stop, system mode cycle, text filter |
| Group tabs | Switch active group |
| Profile table | `uniform_list`, select, double-click start |
| Sidebar | Selection + core notes + shortcuts |
| Status bar | Core label, demo rates, status message |

## Non-goals (near term)

- Reimplementing sing-box in Rust
- Pixel-perfect clone of every Qt dialog on day one
- Shipping unsigned macOS privilege helpers without a dedicated security review
