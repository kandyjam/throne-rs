---
name: throne-upstream-parity
description: >-
  Align throne-rs (Rust/GPUI rewrite) with a throneproj/Throne release tag.
  Use when the user says 对齐上游, 对齐原版, upstream parity, bump to a Throne
  version, or refresh docs/UPSTREAM_TRACKING.md. Runs script/upstream-audit,
  ports user-visible and core-contract gaps, and leaves Qt-only work documented.
---

# Throne upstream parity

Keep **throne-rs** (`rewrite/rust-gpui`) behaviorally aligned with
[throneproj/Throne](https://github.com/throneproj/Throne) without reintroducing Qt.

## Current pin

| Item | Value |
|------|--------|
| Upstream remote | `upstream` → `https://github.com/throneproj/Throne.git` |
| **Pin** | tag **`1.3.2`** (`9dd4fe96`, 2026-09-29) |
| `upstream/dev` at last audit | `1.3.2-20-ga7534e3d` — do not absorb until asked |
| Product version | root `VERSION` + workspace `Cargo.toml` `version` |
| Matrix | [`docs/UPSTREAM_TRACKING.md`](../../docs/UPSTREAM_TRACKING.md) |
| Audit command | `./script/upstream-audit [tag]` |

Do not treat historical `4.x` tags as the product line. Do not invent the next version when the tag is missing.

## Align to a tag

1. **Audit** — `./script/upstream-audit <tag>`. Read the commit list, dirstat, proto stat, and `replace` lines. If fetch fails, use `https://api.github.com/repos/throneproj/Throne/compare/<from>...<to>`.
2. **Triage** each commit:
   - **Must port** — profile types, import, Start/TUN/DNS flags, subscription apply, route rules, settings keys that Qt already persists
   - **Core** — replace `core/` from the tag (`git archive <tag> core`), then re-apply the local Darwin DNS patch below
   - **Cosmetic / i18n / Qt-only** — document as ⏳ or N/A
   - **Out of scope** — Android `core/mobile` behavior, do not rewrite the data plane in Rust
3. **Port only gaps.** Skip rows already ✅ unless the UX is wrong.
4. **Tests first for each contract** — the test names the user-visible guarantee (empty sub left unchanged, domain lowercased, cache not persisted).
5. **Pin** — `VERSION`, workspace `version`, `README.md` badge, `crates/throne-domain/src/version.rs` comment, this table, and a triage section in `docs/UPSTREAM_TRACKING.md`.
6. **Verify** — `cargo test -p throne-domain -p throne-core-client -p throne-import -p throne-storage` and `cargo check -p throne`. If `core/` changed, `./script/build-core`.

## Core sync

Upstream 1.3.0-beta.4 moved the module from `core/server` to `core/`. Build from `core/` (`script/build-core`, `script/lib.sh`).

```bash
# after fetching the tag
rm -rf core
git archive <tag> core | tar -x
# re-apply the local patch (do not drop it)
```

**Local patch:** `core/internal/sysdns/sysdns_darwin.go` resolves the NIC through `boxdns.DefaultInterface()` (TUN/loopback excluded) before sing-box's monitor. Upstream still calls `interfaceMonitor.DefaultInterface().Name`, which follows `utun` after `auto_route`.

Proto lives at `core/gen/libcore.proto`. New RPCs since 1.3.0-beta.3: `WarpRegister`, `CaptureDiagnostics`, `StopDiagnostics`, `UpdateRuleSets`. Field 12 `vpn_status_timeout_ms` is reserved. Do not invent framing; the Rust client is length-prefixed method + protobuf.

## Where code maps

| Upstream | throne-rs |
|----------|-----------|
| `Profile` / outbound beans | `throne-domain` `Profile`, `ParsedOutbound` |
| Auto Selector | `throne-domain` `auto_selector.rs` |
| `BuildSingBoxConfig` | `throne-core-client` `config_build.rs` |
| Tun exclude / #1738 | `build_tun_route_exclude_addrs` / `subtract_ip_prefix` |
| `ThroneCore` RPC | `throne-core-client` + vendored `core/` |
| SQLite | `throne-storage` (`throne.db` wire compatible) |
| Main window / dialogs | `crates/throne/src/ui/` |
| Import / deeplink | `throne-import` |

## 1.3.2 contracts already ported

- `dns_persist_cache` defaults off (`experimental.cache_file.store_fakeip` / `store_dns`).
- Darwin Tun accepts local DNS; no "Local override required" error.
- Default private bypass includes `fc00::/7`, `fe80::/10`, `ff00::/8`. An exact old IPv4-only default list is upgraded on DB load. `vpn_ipv6` adds `vpn_tun_ipv6_cidr` to Tun `address` and, on macOS, punches that prefix out of excludes.
- Empty subscription snapshots do not touch the group (`rejected_empty`).
- Simple rules lowercase `domain` / `suffix` / `keyword`; regex stays as written.
- Route import reads `reject_method` and legacy `method`.

Still open (do not claim done): MASQUE editor, subscription info card, `UpdateRuleSets` menu, chain-through-endpoint DNS, IDN, connections tree, diagnostics UI, TrustTunnel/OpenVPN editors, i18n. See the 1.3.2 table in `docs/UPSTREAM_TRACKING.md`.

## Definition of done

- [ ] User path works without Qt, or the gap is an explicit ⏳/N/A row
- [ ] DB round-trip keeps upstream type strings and setting keys
- [ ] New behavior has a failing-then-passing test
- [ ] `VERSION` == workspace version
- [ ] Core rebuilt when Go or proto changed
- [ ] Darwin `sysdns` patch still present after a core checkout

## Anti-patterns

- Bumping `VERSION` without a triage table
- Copying Qt `mainwindow.cpp` into GPUI in one change
- Renaming `throne.db` columns
- Dropping the `sysdns` physical-NIC patch when replacing `core/`
- Absorbing `upstream/dev` when the user named a release tag
