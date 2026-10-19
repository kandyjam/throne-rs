---
name: throne-upstream-parity
description: >-
  Track and close feature gaps between throne-rs (Rust/GPUI rewrite) and
  upstream throneproj/Throne. Use when aligning versions, auditing parity,
  implementing missing upstream features, or refreshing docs/UPSTREAM_TRACKING.md.
  Triggers: 对齐原版, upstream parity, 1.2.x, 1.2.4, 1.2.5, throneproj/Throne, Auto Selector,
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
| **Current pin** | tag **`1.4.0-beta.3`** (`01086027`, 2026-10-17) |
| **Tip tracked** | `upstream/dev` describe `1.4.0-beta.3-6-gcb26b89e` on 2026-10-18; six later commits excluded |
| **1.2.5** | Never tagged; line jumped to **1.3.0-beta.1**. |
| Product version | root [`VERSION`](../../VERSION) + workspace `Cargo.toml` `version` (= requested audited release tag / `NKR_VERSION`) |
| Parity matrix | [`docs/UPSTREAM_TRACKING.md`](../../docs/UPSTREAM_TRACKING.md) |
| This rewrite | branch `rewrite/rust-gpui`, crates under `crates/` |

**Do not** treat historical `4.x` tags as current product line; they are ancestors.
**Do not** invent the next release number when the tag is missing — port tip commits under the current pin and re-pin when the tag appears.

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
   - **Core binary** — Go `core/` (moved from `core/server` in 1.3.0-beta.4), sing-box/xray bumps, new RPCs (`libcore.proto`)
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
| `ThroneCore` RPC | `throne-core-client` + vendored `core/` |
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

## Pre-1.2.5 tip (not a release)

`1.2.4..upstream/dev` as of audit:

| SHA | Summary | Status |
|-----|---------|--------|
| `ed2fdde2` | naiveproxy v150 / `cronet-go` replace bump | ✅ `core/server/go.mod` + `go.sum` |

When **1.2.5** is tagged: `git log --oneline 1.2.4..1.2.5` → triage → bump `VERSION` + skill pin + matrix.

## 1.4.0-beta.3 feature notes (current pin)

Verification on 2026-10-18: 310 workspace tests, desktop check, host core rebuild, 10 actual-core smoke groups and Go targeted/race tests passed. Strict Clippy remains blocked by 18 pre-existing domain lints; native desktop and privileged/cross-platform runtime remain untested.

All **16 commits** since beta.2 are triaged in `docs/UPSTREAM_TRACKING.md`; the release pin is not a full-parity claim.

- Multi-group Auto Selector `gids` preserves order, removes duplicates, retains legacy `gid`, tolerates missing groups, and resolves each group's eligibility for Start/rerank. The native profile editor selects tracked groups.
- Group sorting persists upstream `sort_method`/`sort_descending`, re-sorts changed data and restores each group's header state.
- TLS route fragmentation/record fragmentation and fallback delay survive DB/share round-trip, are editable and preserve non-terminal `route-options`.
- Native TLS cipher suites are imported with unsupported values reported/dropped. `fp=unsafe` disables ordinary uTLS; REALITY retains required random uTLS. XHTTP/Xray JSON cipher suites use the full-config bridge. Invalid Clash ports cannot wrap and partial feeds block destructive subscription replacement.
- Connections resolve generated tags to running profile/member names. QUIC I1–I5 generation and capability diagnostics have core/client support; desktop controls remain open.
- Core matches beta.3 except retained Darwin NIC hardening. Rebuild with `./script/build-core`; existing staged setuid binaries require separate refresh.
- Headless engine/service/API, country-provider customization, Clash VLESS engine preference, full validation/editor/export and historical scanner/Kill Switch/hotkey gaps remain explicit. See the matrix for executed evidence and runtime limits.



## 1.4.0-beta.2 feature notes (historical pin)

All **14 commits** are triaged in `docs/UPSTREAM_TRACKING.md`. This is an audited release pin with explicit remaining gaps.

- ECH shorthand/legacy fields and SHA256 certificate pins survive import; URI `+` stays literal. XHTTP VLESS and Xray JSON ECH use the full-config bridge. Dedicated Xray editor/export and full-config batch tests remain open.
- Native ECH bootstrap routes HTTPS over direct/custom DNS before proxy rules, uses matching ASCII query/rule names, skips static configs and strips GUI resolver metadata.
- WireGuard/Amnezia names, `awg://` and obfuscation/peer parameters persist; DB reload restores display fields, generation converts legacy beans and `worker_count` to core `workers`. INI imports one coherent last peer; full multi-peer INI remains open.
- IPv6 FakeIP uses `2001:db8::/32`, honors `fakeip_disable_ipv6`, and is validated against the rebuilt core.
- Log font/history preferences, collapsible literal filter and hold/resume are implemented; regex/highlighting remain. Tray connection transitions refresh while the main window is hidden.
- **New remote control API is not ported.** Custom encrypted protocol/router/listener and Integration settings remain distinct from the core API tracker service. Historical scanner/Kill Switch/global hotkey gaps remain.
- Core matches beta.2 except retained Darwin `netmon` NIC hardening. Modules/proto unchanged; host rebuild and actual-core config/scanner smoke checks executed. Live remote handshakes, GUI and privileged/cross-platform runtime require separate executed evidence.
- Verification on 2026-10-14: 290 workspace tests passed (10 suites), desktop check passed with two existing warnings, formatting/diff checks passed, and all seven rebuilt-core smoke groups passed. Regression fixes cover encoded plus/quoted repr preservation, WG peer association and DB reload/override, workers conversion and immediate log trimming.

## 1.4.0-beta.1 feature notes (historical pin)

All **23 commits** from `1.3.2` to this tag are triaged in `docs/UPSTREAM_TRACKING.md`. Pinning the target does not close the remaining product or historical gaps.

| Theme | Upstream | Rust / core status |
|-------|----------|-------------------|
| Core/proto + API service | `86f196d4`, `6a030fe3`, target config contract | ✅ target core synced and host rebuilt; API service supplies stats/connection trackers with HTTP listener disabled |
| IP-list/scanner | `86f196d4` | 🧩 models/storage/RPC plus manual TCP UI only: 4,096 targets, 256 hosts/CIDR, concurrency 16, batches 64; full probes/resume UI/remote updates/default seeds remain |
| Group/profile endpoints | `1809cb0e` | 🧩 DB inheritance + profile list assignment + Own/Inherit restoration; Start/tests clone endpoint while preserving TLS identity/port; group/fixed-address editor remains |
| L3/Block config contracts | target core | ✅ `throne-br`, direct-rule twins/order and final reject; no Block-to-direct bypass; privileged runtime untested |
| DNS hijack/TUN stack retirement | `a7534e3d` | ✅ retired controls/RPC removed or hidden; no selectable stack emitted; mandatory DNS routing action retained |
| Experimental Kill Switch | `866e13a4` | 🧩 persisted setting + core guard only; **no GUI/lifecycle integration**, no protection claim |
| Derived security/cleanup | `50a22014` | ✅ private/pinned/raw/custom/Xray/MASQUE semantics; global-only compromise is not removable |
| Connection route targets | `be6206e0`, `bfe5d59d` | ✅ domain levels/process/destination plus accurate existing/coverage marks |
| Global hotkeys/KDE + OS proxy refactor | `c672f85e`, `b19a4b95` | ⏳ not ported |
| Packaging/Qt cosmetics | remaining release commits | ⏳ native distro/release automation; Qt-specific implementation N/A; see full triage |

SQLite compatibility includes endpoint JSON, group subscription metadata/options, `latency_at`, atomic IP-list generations and resumable scan records. Scan records do not imply a complete resumable UI.

Requires ThroneCore built from **`core/` at 1.4.0-beta.1** (`./script/build-core`). Preserve the Darwin physical-NIC patch through **`netmon.DefaultInterface()`**, which replaces the historical `boxdns` monitor reference.

Audit verification: **89 domain tests passed**, six actual-core smoke checks including valid/invalid WebSocket configs, earlier Go vet/race checks, and host packaging build. Final workspace checks and native GUI exercise remain **pending**; Windows and privileged TUN/L3/Kill Switch runtime are **untested** until executed evidence updates the matrix.

## 1.3.0-beta.3 feature notes (historical pin)

Headline commits `1.3.0-beta.2...1.3.0-beta.3` (17 commits):

| Theme | Upstream | Rust / core status |
|-------|----------|-------------------|
| **skip_cert → outbound TLS insecure** | `c42111b6` TLS.cpp | ✅ `AppSettings.skip_cert` + `apply_skip_cert_to_outbounds` |
| **Auto Selector outage recovery** | `66c7612b` keep failed members if pool empty | ✅ `kept_unavailable` |
| **Connections add-to-route** | `5c322220` | ✅ right-click dest/domain → simple rules |
| **Inner hop endpoints** | `8a844c5e` | 🧩 persist ids; generate later |
| **sing-box / Xray** | `1d5bc45b` `1ff14ded` | ✅ `core/server` go.mod + box.go/boxmain |
| Runtime stats in main window | `e00806d1` | 🧩 Traffic Graph already in main window |
| Xray geo downloader history | `d82783a1` | ⏳ |
| i18n / Inno / Qt cosmetics | various | ⏳ / N/A |

Requires ThroneCore rebuilt from `core/server` ≥ **1.3.0-beta.3** (`./script/build-core`).

## 1.2.4 feature notes

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
- When upstream bumps sing-box/xray or adds RPCs, sync `core/` (proto, go.mod replace, RPC handlers) in a dedicated change; do not invent incompatible RPC framing. Historical notes referring to `core/server` describe the pre-1.3.0-beta.4 layout.
- Framing remains length-prefixed method + protobuf (see `throne-core-client`).
- Prefer merging local hardening (`sysdns_darwin` physical iface via `netmon.DefaultInterface()`, previously `boxdns`) **with** upstream quiet-networksetup fixes — don’t drop either.

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
