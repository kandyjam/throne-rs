---
name: throne-upstream-parity
description: >-
  Track and close feature gaps between throne-rs (Rust/GPUI rewrite) and
  upstream throneproj/Throne. Use when aligning versions, auditing parity,
  implementing missing upstream features, or refreshing docs/UPSTREAM_TRACKING.md.
  Triggers: 对齐原版, upstream parity, 1.2.x, throneproj/Throne, Auto Selector,
  feature gap, NKR_VERSION, UPSTREAM_TRACKING.
---

# Throne upstream parity skill

Keep **throne-rs** (`rewrite/rust-gpui`) behaviorally aligned with
[throneproj/Throne](https://github.com/throneproj/Throne) without reintroducing Qt.

## Remotes & version source of truth

| Item | Value |
|------|--------|
| Upstream remote | `upstream` → `https://github.com/throneproj/Throne.git` |
| Baseline branch | `upstream/dev` (or a release tag e.g. `1.2.3`) |
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
git log --oneline 1.2.2..1.2.3
```

If `git fetch` fails (network), use GitHub API / release notes:

```bash
curl -sL 'https://api.github.com/repos/throneproj/Throne/releases/tags/1.2.3'
curl -sL 'https://api.github.com/repos/throneproj/Throne/compare/1.2.2...1.2.3'
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

## Where code maps

| Upstream (Qt/C++/Go) | throne-rs |
|---------------------|-----------|
| `Profile` / outbound beans | `throne-domain` `Profile`, `ProfileType`, `ParsedOutbound` |
| Auto Selector (`autoselector`) | `throne-domain` `auto_selector.rs` + `ProfileType::AutoSelector` |
| `PlanAutoSelector` | `plan_auto_selector` / `AppState::resolve_auto_selector_members` |
| `BuildSingBoxConfig` | `throne-core-client` `config_build.rs` (`build_load_config_ex`) |
| `ThroneCore` RPC | `throne-core-client` + vendored `core/server` |
| SQLite repos | `throne-storage` (wire-compatible `throne.db`) |
| Main window / menus | `crates/throne/src/ui/main_window.rs` |
| Dialogs | `crates/throne/src/ui/dialogs.rs` |
| Import / deeplink | `throne-import` |

## Version bump checklist

When moving e.g. `1.2.2` → `1.2.3`:

- [ ] `VERSION`
- [ ] workspace `Cargo.toml` `version` (+ comment about tip tag)
- [ ] `README.md` version line
- [ ] `docs/UPSTREAM_TRACKING.md` product version + audited tip
- [ ] Hardcoded UA fallbacks (e.g. `throne-import` fetch) if any
- [ ] `crates/throne-domain/src/version.rs` docs only (`NKR_VERSION` comes from Cargo)

Packaging scripts already read root `VERSION`.

## 1.2.3 feature notes (reference)

Headline release notes (upstream tag `1.2.3`):

| Feature | Upstream modules | Rust status intent |
|---------|------------------|--------------------|
| **Auto Selector** | `outbounds/autoselector`, `AutoSelectorPlan`, `server_autoselector.go`, stats dialog | Domain + plan + Start via `urltest`/`selector` group; full core `auto-selector` outbound + `QueryAutoSelectors` RPC needs ThroneCore ≥ 1.2.3 |
| Multi-file import | file dialog multi-select | Program → “Import from file(s)…” (`osascript`/`zenity` + clipboard path fallback) |
| New Profile in Program menu | Program dropdown | “New Auto Selector” (+ existing add-from-input/clipboard) |
| Exclude private range (TUN) | Tun settings | Already wired (`disable_private_range_bypass`) |
| Xray / installer / crash fixes | core + packaging | Prefer core binary bump when packaging; not all ported in GUI |

### Auto Selector implementation contract

- Persist as profile `type = autoselector` + outbound JSON keys matching upstream (`gid`, `pool_cap`, `build_limit`, `balance`, …).
- On Start: `resolve_auto_selector_members` → `AutoSelectorBuild` → `start_profile_ex`.
- Config emit strategy (current, 1.2.3-aligned):
  - Members as `p{id}` outbounds + group tagged `proxy` with **`type: "auto-selector"`** (fields: url, intervals, warm, pinned, balance, …).
  - Requires ThroneCore built from `core/server` ≥ 1.2.3 (`./script/build-core`).
  - Client can poll `QueryAutoSelectors` / send `AutoSelectorAction` once core is running that build.
- Do **not** nest Auto Selectors / chains as members (plan skips `MetaType`).

## Core binary policy

- Keep shipping Go `ThroneCore` next to the GUI (`parentcheck` requires parent basename `Throne`).
- When upstream bumps sing-box/xray or adds RPCs, sync `core/server` (proto, go.mod replace, new `server_*.go`) in a dedicated change; do not invent incompatible RPC framing.
- Framing remains length-prefixed method + protobuf (see `throne-core-client`).

## Definition of done for a parity slice

- [ ] User-visible path works without Qt (create → list → Start/Stop or import path)
- [ ] DB round-trip preserves upstream type string / outbound JSON
- [ ] Tests for plan/config pure logic
- [ ] Matrix row updated (✅ / 🧩 / ⏳)
- [ ] No silent version drift (`VERSION` == workspace version == UI status version)

## Anti-patterns

- Bumping version without implementing or documenting remaining gaps
- Copying entire Qt `mainwindow.cpp` into GPUI in one shot
- Breaking `throne.db` schema compatibility for cosmetic renames
- Emitting core types the bundled ThroneCore cannot decode without a core upgrade path

## Quick audit commands

```bash
# Current product version
cat VERSION && rg 'version = "' Cargo.toml | head -1

# Gap search vs matrix
rg -n '⏳|🧩|❌' docs/UPSTREAM_TRACKING.md

# Auto Selector surface
rg -n 'AutoSelector|autoselector' crates --glob '*.rs'

# Upstream tip (when fetch works)
git log --oneline upstream/dev -15
```
