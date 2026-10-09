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
| **Pin** | tag **`1.4.0-beta.1`** (`50a22014`, 2026-10-04) |
| `upstream/dev` at last audit | `1.4.0-beta.1-13-g32138a44` on 2026-10-09 — do not absorb until asked |
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

**Local patch:** `core/internal/sysdns/sysdns_darwin.go` resolves the physical NIC through `netmon.DefaultInterface()` (TUN/loopback excluded) before sing-box's monitor. The 1.4.0-beta.1 removal of the `boxdns` monitor must not drop this hardening; sing-box's default interface can follow `utun` after `auto_route`.

Proto lives at `core/gen/libcore.proto`. New contracts since 1.3.0-beta.3 include `WarpRegister`, `CaptureDiagnostics`, `StopDiagnostics`, `UpdateRuleSets`, scanner/egress RPCs and the target's guard support. Retired standalone DNS-hijack RPCs are removed. Field 12 `vpn_status_timeout_ms` is reserved. Do not invent framing; the Rust client is length-prefixed method + protobuf.

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

## 1.4.0-beta.1 contracts and remaining gaps (current pin)

The 23 commits from `1.3.2` to `1.4.0-beta.1` are individually triaged in `docs/UPSTREAM_TRACKING.md`. This is a core/schema baseline with partial product coverage, not a full-parity claim.

- **Core/API/config:** target `core/` and proto synced; host core rebuilt. Emit the API service with its HTTP listener disabled so stats/connection trackers exist. L3 bridge uses `throne-br`, ordered direct-rule twins and fallback; Block final rejects rather than bypassing. Retired DNS-hijack controls/selectable TUN stack removed or hidden; the mandatory DNS route action remains.
- **IP lists/scanner:** upstream-compatible models, atomic entry generations, migration, scan progress/seed storage and RPCs. Native UI supports manual lists and **TCP only**, capped at **4,096 targets**, **256 hosts per CIDR**, **16 concurrent probes**, **64 targets per batch**. Full probes, resumable GUI sessions, remote list updates and default list seeds remain open.
- **Endpoint sources:** profile/group inheritance and JSON/DB compatibility; profile list assignment plus **Own/Inherit** restoration. Start and stored-profile tests use transient endpoint clones, preserve TLS/SNI/transport hosts and ports, and block serverless/realm overrides before list lookup. Group/fixed-address editor remains open.
- **Security/cleanup:** derived classification covers raw/custom/Xray, private hosts, pins and custom MASQUE's mandatory TLS. Cleanup honors the security-display gate and never removes a profile solely because global skip-cert compromised it.
- **Connection routing:** domain levels, process name/path, destination targets and existing/coverage marks; a target's own identical line in another list does not count as covering it.
- **Kill Switch:** setting persisted and upstream guard code retained in core; **no GUI or lifecycle integration** and no privileged runtime verification. Do not claim user protection is active.
- **Platform gaps:** KDE/global-hotkey rewrite and OS proxy refactor are not ported. APT/RPM/AUR release automation and historical matrix gaps remain separate work.

Verification recorded during the audit: domain **89 tests passed**, six actual-core smoke checks (including positive/negative WebSocket config cases), earlier Go vet/race checks and host packaging build. Final workspace checks and native GUI exercise are **pending**; Windows and privileged TUN/L3/Kill Switch runtime are **untested**. Refresh this record only from executed results.

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
