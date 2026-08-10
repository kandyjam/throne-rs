---
name: throne-upstream-parity
description: >-
  Track and close feature gaps between throne-rs (Rust/GPUI rewrite) and
  upstream throneproj/Throne. Use when aligning versions, auditing parity,
  implementing missing upstream features, or refreshing docs/UPSTREAM_TRACKING.md.
  Triggers: 对齐原版, upstream parity, 1.2.x, 1.2.4, throneproj/Throne, Auto Selector,
  feature gap, NKR_VERSION, UPSTREAM_TRACKING, Xray full config, Tun DNS.
---

# Throne upstream parity skill

Keep **throne-rs** (`rewrite/rust-gpui`) behaviorally aligned with
[throneproj/Throne](https://github.com/throneproj/Throne) without reintroducing Qt.

## Remotes & version source of truth

| Item | Value |
|------|--------|
| Upstream remote | `upstream` → `https://github.com/throneproj/Throne.git` |
| Baseline branch | `upstream/dev` (or a release tag e.g. `1.2.4`) |
| **Current pin** | tag **`1.2.4`** (`33777e27`) |
| Product version | root [`VERSION`](../../VERSION) + workspace `Cargo.toml` `version` (= upstream `NKR_VERSION` / release tag) |
| Parity matrix | [`docs/UPSTREAM_TRACKING.md`](../../docs/UPSTREAM_TRACKING.md) |
| This rewrite | branch `rewrite/rust-gpui`, crates under `crates/` |

**Do not** treat historical `4.x` tags as current product line; they are ancestors.

### Refresh upstream

```bash
git fetch upstream --tags --prune
git describe upstream/dev --tags
git log --oneline <last-audited>..upstream/dev
# or vs a release:
git log --oneline 1.2.3..1.2.4
```

If `git fetch` fails (network), use GitHub API / release notes:

```bash
curl -sL 'https://api.github.com/repos/throneproj/Throne/releases/tags/1.2.4'
curl -sL 'https://api.github.com/repos/throneproj/Throne/compare/1.2.3...1.2.4'
```

## Parity workflow (every alignment task)

1. **Pin target** — release tag or `upstream/dev` tip; write it into `VERSION` + workspace version + README badge line.
2. **Diff** — commits + file list between previous pin and target (compare API or `git log`).
3. **Triage** into:
   - **Must port (user-visible)** — new profile types, menus, dialogs, import, Start path, TUN flags
   - **Core binary** — Go `core/server`, sing-box/xray bumps, new RPCs (`libcore.proto`)
   - **Cosmetic / i18n** — lower priority unless UX is worse than Qt
   - **Out of scope** — keep Go core as binary; do not rewrite data plane in Rust
4. **Implement only gaps** — skip features already ✅ on this branch unless UX is clearly worse.
5. **Update matrix** — edit `docs/UPSTREAM_TRACKING.md` status column + “Last audited tip”.
6. **Verify** — `cargo test --workspace` (or at least domain + core-client + `cargo check -p throne`).
7. **Rebuild core** when Go sources / proto change: `./script/build-core`.

## Where code maps

| Upstream (Qt/C++/Go) | throne-rs |
|---------------------|-----------|
| `Profile` / outbound beans | `throne-domain` `Profile`, `ProfileType`, `ParsedOutbound` |
| Auto Selector (`autoselector`) | `throne-domain` `auto_selector.rs` + `ProfileType::AutoSelector` |
| `PlanAutoSelector` / skip reasons | `plan_auto_selector`, `AutoSelectorSkip`, `classify_custom_member` |
| `BuildSingBoxConfig` | `throne-core-client` `config_build.rs` (`build_load_config_ex`) |
| Xray full config bridges | `build_xray_full_member` + `LoadConfigExtras.xray_full_configs` |
| Tun `route_exclude` / #1738 | `build_tun_route_exclude_addrs` / `subtract_ipv4_prefix` |
| `ThroneCore` RPC | `throne-core-client` + vendored `core/server` |
| SQLite repos | `throne-storage` (wire-compatible `throne.db`) |
| Main window / menus | `crates/throne/src/ui/main_window.rs` |
| Dialogs | `crates/throne/src/ui/dialogs.rs` |
| Import / deeplink | `throne-import` |

## Version bump checklist

When moving e.g. `1.2.3` → `1.2.4`:

- [ ] `VERSION`
- [ ] workspace `Cargo.toml` `version` (+ comment about tip tag)
- [ ] `README.md` version line
- [ ] `docs/UPSTREAM_TRACKING.md` product version + audited tip
- [ ] Hardcoded UA fallbacks (e.g. `throne-import` fetch) if any
- [ ] `crates/throne-domain/src/version.rs` docs only (`NKR_VERSION` comes from Cargo)
- [ ] `script/build-core` comment if core tag baseline changes
- [ ] This skill’s “Current pin” row + feature notes section

Packaging scripts already read root `VERSION`.

## 1.2.4 feature notes (current pin)

Headline commits `1.2.3...1.2.4` (8 commits):

| Theme | Upstream | Rust / core status |
|-------|----------|-------------------|
| **Auto Selector + Xray full** | `f97019d2` plan allows `CustomXrayFullConfig`; generate emits socks bridge + `xray_full_configs` | ✅ plan (`XrayFullChained` / no longer skip Xray full as FullConfig) · ✅ build bridge · ✅ LoadConfig field 16/17 |
| **macOS Tun DNS black-hole** | `f97019d2` `subtractPrefix` on Darwin `route_exclude_address` (#1738) | ✅ `build_tun_route_exclude_addrs` |
| **Subscription keep-running** | `f97019d2` GroupUpdater #1753 — survivors not re-added as duplicates | ✅ `apply_subscription_snapshot` protect + `kept_in_use` notice |
| **Core Xray full gates** | `server.go` `xrayFullGates`, lazy idle | ✅ core/server synced from 1.2.4 |
| **auto_redirect egress mark** | `18a82022` `egress.go` Linux SO_MARK | ✅ `core/server/egress.go` |
| **Xray DNS / gate sweep** | `2408281a`, `f97019d2` gate idle sweep | ✅ go.mod + gate.go |
| **Unix isAdmin crash** | `0e8aeaca` don’t call IsPrivileged before core | N/A Qt; Rust checks privilege after core up for Tun |
| **Test poll isolation** | `fa44e69e` sessionGen / buffer Reclaim | 🧩 core `Reclaim` present; GUI poll gen later |
| **Installer unwritable config** | `d8e663cc` AppConfig fallback | ⏳ Windows NSI / appdata already default on macOS package |
| **Profile editor tab order** | `33777e27` | ⏳ GPUI dialogs; low priority |
| **zh_CN i18n** | `7cfc2253` | ⏳ |

### Auto Selector implementation contract (1.2.4)

- Persist as profile `type = autoselector` + outbound JSON keys matching upstream (`gid`, `pool_cap`, `build_limit`, `balance`, …).
- Custom members: `subtype` `xrayfullconfig` allowed; `fullconfig` (sing-box) still **FullConfig** skip.
- On Start: `resolve_auto_selector_members` → `AutoSelectorBuild` → `start_profile_ex`.
- Config emit:
  - Members as `p{id}` outbounds + group tagged `proxy` with **`type: "auto-selector"`**.
  - Xray full members → socks → `127.0.0.1:bridge` + `LoadConfigReq.xray_full_configs`.
  - Lazy sidecar: `xray_lazy_start` + `xray_idle_seconds = max(120, interval*2)`; `xray_full_idle_seconds = 0`.
- Requires ThroneCore built from `core/server` ≥ **1.2.4** (`./script/build-core`).

## 1.2.3 feature notes (baseline already landed)

| Feature | Upstream modules | Rust status |
|---------|------------------|-------------|
| **Auto Selector** | `outbounds/autoselector`, plan, stats dialog | ✅ create + plan + Start + Tools stats |
| Multi-file import | file dialog multi-select | ✅ Program → Import from file(s)… |
| New Profile in Program menu | Program dropdown | 🧩 New Auto Selector (+ input/clipboard) |
| Exclude private range (TUN) | Tun settings | ✅ + 1.2.4 Darwin hole punch |

## Core binary policy

- Keep shipping Go `ThroneCore` next to the GUI (`parentcheck` requires parent basename `Throne`).
- When upstream bumps sing-box/xray or adds RPCs, sync `core/server` (proto, go.mod replace, new `server_*.go`) in a dedicated change; do not invent incompatible RPC framing.
- Framing remains length-prefixed method + protobuf (see `throne-core-client`).
- Prefer merging local hardening (e.g. `set_dns_darwin` physical iface via boxdns) **with** upstream quiet-networksetup fixes — don’t drop either.

## Definition of done for a parity slice

- [ ] User-visible path works without Qt (create → list → Start/Stop or import path)
- [ ] DB round-trip preserves upstream type string / outbound JSON
- [ ] Tests for plan/config pure logic
- [ ] Matrix row updated (✅ / 🧩 / ⏳)
- [ ] No silent version drift (`VERSION` == workspace version == UI status version)
- [ ] Core rebuilt if Go/proto changed

## Anti-patterns

- Bumping version without implementing or documenting remaining gaps
- Copying entire Qt `mainwindow.cpp` into GPUI in one shot
- Breaking `throne.db` schema compatibility for cosmetic renames
- Emitting core types the bundled ThroneCore cannot decode without a core upgrade path
- Overwriting local core improvements when syncing a tag (diff first)

## Quick audit commands

```bash
# Current product version
cat VERSION && rg 'version = "' Cargo.toml | head -1

# Gap search vs matrix
rg -n '⏳|🧩|❌' docs/UPSTREAM_TRACKING.md

# Auto Selector + Xray full surface
rg -n 'AutoSelector|xrayfull|xray_full|XrayFull' crates --glob '*.rs'

# Core 1.2.4 symbols
rg -n 'xrayFullGates|autoRedirectMark|GetXrayFullIdle' core/server

# Upstream tip (when fetch works)
git log --oneline upstream/dev -15
git log --oneline 1.2.3..1.2.4
```
