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

## 1.4.0 feature notes (current pin)

Target `99f7ecd1` (2026-11-05); audited 2026-11-06. All 7 commits since beta.6 are triaged in `docs/UPSTREAM_TRACKING.md`. `upstream/dev` is `1.4.0-2-g1346d9ee`; `5f92cd98` and `1346d9ee` are excluded. This release pin does not assert complete product parity.

- MasterDNS SOCKS user and password stay ignored and no longer produce an import note. Dial-field, REALITY padding, AnyTLS fast-open, Hysteria `pinSHA256`, WireGuard `DNS`/`AllowedIPs`, and OpenVPN redirect-gateway warnings were not emitted.
- Core pins: sing-box `6bc5c846150e`, uTLS `f0ae7792aa1d`, Xray `1c6eb7771c4d`, WireGuard `e11787583013`. The MASQUE change is inside sing-box. Native WARP generation remains unported. Both patches still guard the beta.5/beta.6 source hashes.
- The remote API gained connections, WARP, profile-config, and route-editing endpoints. That API stays unported. Qt title-bar theming and the connections `doItemsLayout` crash fix are N/A.
- Simple interface, web UI, About/donation, log export, and historical headless/API/scanner/Kill Switch/hotkey gaps remain explicit.
- Executed on 2026-11-06: 301 tests in throne-domain, throne-import, and throne-core-client; `cargo check -p throne`. Source hashes for `reality_client.go` and `source_darwin.go` match the beta.6 guards. `THRONE_CORE_VERIFY_PATCHES=1 ./script/build-core target/parity-1.4.0` exited 0 (race tests, 78M audit binary). Full workspace tests, strict Clippy, throne-storage, and the 13 actual-core smoke groups were not re-run. Native GUI, privileged DNS/TUN, and cross-platform runtime remain untested.

## 1.4.0-beta.6 feature notes (historical pin)

Target `4b1904e3` (2026-11-04); audited 2026-11-04. All 15 commits since beta.5 are triaged in `docs/UPSTREAM_TRACKING.md`. `upstream/dev` is `1.4.0-beta.6-1-gb8074099`; `b8074099` (fix rare crash) is excluded. This release pin does not assert complete product parity.

- DNSTT and MasterDNS persist as `dnstt` and `masterdns`. `dnst://`, unencrypted `slipnet://`, `masterdns://` / `stormdns://` yptun, and profile JSON import into ExportToJson. Config build strips `server`, turns resolver lines into sing-box objects or MasterDNS strings, and uses the system resolver when the list is empty. URL tests add 30s per DNSTT and 90s per MasterDNS. The editor edits domain, key, mode, resolvers, and DNSTT backend credentials.
- Encrypted SlipNet links fail visibly. MasterDNS TOML (`v1/`) returns "this MasterDNS link format is not supported." DNSTT resolver ranges are not expanded. Resolver IP lists, the xdns form, and scanner DNS probes stay partial. Chain inner hops stay unwired.
- macOS Start emits DNS type `local` and does not add a `dns-system` inbound. The core publishes a temporary network service and routes `0.0.0.0/8` into the Tun. The Tun-prefix exclude hole stays, because system DNS is the next address inside that prefix. A failed Start does not blank networksetup DNS.
- `SystemDNSOverride` and `sing-box-darwin-system-dns.patch` stay so a leftover `127.0.0.53` record is not used as an upstream. `RestoreLegacy` undoes a dead beta.4/beta.5 networksetup state. Do not replay the old `netmon` follow onto the rewritten `sysdns_darwin.go`.
- Core pins: sing-box `f25a530dc468`, sing-tun `55c7bb51acc7`, Xray `1c6eb7771c4d`, WireGuard `e11787583013`. Both patches apply with zero fuzz. The unpatched REALITY and Darwin source hashes are unchanged from beta.5. Log export, Qt themes, CI, and the shorter upstream README are not ported.
- Simple interface, web UI, About/donation, and historical headless/API/scanner/Kill Switch/hotkey gaps remain explicit.
- Executed on 2026-11-04: 298 tests in throne-domain, throne-import, and throne-core-client; `cargo check -p throne`. Go tests of sysdns, rpc, probe, and scan passed (69). `THRONE_CORE_VERIFY_PATCHES=1 ./script/build-core target/parity-beta6` exited 0 (race tests, 78M audit binary). Full workspace tests, strict Clippy, throne-storage, and the 13 actual-core smoke groups were not re-run. Native GUI, privileged DNS/TUN, and cross-platform runtime remain untested.

## 1.4.0-beta.5 feature notes (historical pin)

Target `ec2420df` (2026-11-02); audited 2026-11-03. All 40 commits since beta.4 are triaged in `docs/UPSTREAM_TRACKING.md`. `upstream/dev` is `1.4.0-beta.5-2-ga76f33cf`; later commits (`a76f33cf` remove custom fund address, `4c5b1e0a` shorter README) are excluded. This release pin does not assert complete product parity.

- Per-group `route_id` and `import_sub_route` persist. Start and domain resolve use that group's route and do not change the global active route. Subscription routing creates or updates the `source_gid`-owned route, including Happ private-IP rules. There is no group-route editor, and chain inner hops are not expanded.
- A running profile deleted by a subscription update reconnects only when exactly one same-type, same-name successor remains. A route change restarts that group's running profile. No successor stops the core and leaves it stopped.
- `core_dns_in_lan` and `core_dns_in_port` (default 5533) add direct `dns-in` and a reject rule on the real Start config. URL tests and ResolveDomains do not. There is no settings control yet.
- macOS Tun and URL tests emit DNS type `underlying` instead of `local` (#2045). Mixed mode and ResolveDomains stay `local`. ECH treats `underlying` as a direct bootstrap.
- Hysteria 2 omits obfs when the type is empty, `none`, or `plain`, or the password is empty. Hysteria 1 writes `up_mbps`/`down_mbps` of 10/50 when unset. Clash bandwidth units match upstream. Hy2 `user:pass` concatenation was not re-ported.
- Auto Selector country exclusion, freshness, start quorum, and unmeasured candidates are pure helpers. Native Start does not call them.
- Core pins: sing-box `af51a7460ccd`, Xray `1c6eb7771c4d`, WireGuard `e11787583013`. Xray reset-on-network and mobile log forwarding are in the Go core. WireGuard DF and GSO are dependency pins only. Xray mux widgets and WARP Amnezia camouflage are not ported.
- Darwin physical-interface hardening and the REALITY ML-KEM patch stay. The Darwin sing-box patch was rebased onto `sing-tun/dnsinfo`; `replaceManagedDNS` is unchanged.
- Simple interface, web UI, About/donation, and historical headless/API/scanner/Kill Switch/hotkey gaps remain explicit.
- Executed on 2026-11-03: 312 tests in throne-domain, throne-import, throne-core-client, and throne-storage; `cargo check -p throne`; formatting of those crates. `THRONE_CORE_VERIFY_PATCHES=1 ./script/build-core target/parity-beta5` exited 0 (systemconfig and sysdns race tests, 77M audit binary). Full workspace tests, strict production Clippy, and the 13 actual-core smoke groups were not re-run. Native GUI, privileged DNS/TUN, and cross-platform runtime remain untested.

## 1.4.0-beta.4 feature notes (historical pin)

Target `dd748826` (2026-10-22); audited 2026-10-27. All 32 commits since beta.3 are triaged in `docs/UPSTREAM_TRACKING.md`. `upstream/dev` is `1.4.0-beta.4-15-g1f30af24`; later commits are excluded. This release pin does not assert complete product parity.

- Route-specific DNS and disabled rules persist through compatible DB/share/edit/remote refresh; Start/probe generation respects effective DNS and skips disabled rules before rule-set fetches.
- Core and additive ResolveDomains/connection-rule RPCs match beta.4. Native selected/group domain resolution preserves identity; matched rules appear in Connections tooltips.
- Native Ctrl/⌘K Command Palette and Core Options expose existing actions, mux presets, mixed-inbound disable and opt-in Xray system CA. Explicit trust choices and REALITY are retained.
- XDrive JSON is losslessly bridged to Xray; unsupported XDrive share links fail visibly. Specialized form/validation/export remains.
- Rebuild with `./script/build-core`; preserve the private source-guarded REALITY ML-KEM patch, Darwin physical-interface handling and managed local-resolver protection. Never overwrite legacy `core/server/` or assume installed privileged binaries refreshed automatically.
- WebDAV, connection-to-specific-server routing, complete field search, full mux/XDrive forms and historical headless/API/scanner/Kill Switch/hotkey gaps remain explicit. See the matrix for executed checks and runtime limits.
- Executed on 2026-10-27: 387 Rust workspace tests, strict production Clippy, desktop compilation, formatting, final host core rebuild with Darwin DNS patch race tests, and 13 actual-core smoke groups. Independent ECH review passed 18 extra config cases. All-target lint remains blocked by pre-existing test-only lints; native GUI, privileged DNS/TUN and cross-platform runtime remain untested.

## 1.4.0-beta.3 feature notes (historical pin)

Verification on 2026-10-18: 310 workspace tests, desktop check, host core rebuild, 10 actual-core smoke groups and Go targeted/race tests passed. Strict Clippy remains blocked by 18 pre-existing domain lints; native desktop and privileged/cross-platform runtime remain untested.

All **16 commits** since beta.2 are triaged in `docs/UPSTREAM_TRACKING.md`; the release pin is not a full-parity claim.

- Multi-group Auto Selector `gids` preserves order, removes duplicates, retains legacy `gid`, tolerates missing groups, and resolves each group's eligibility for Start/rerank. The native profile editor selects tracked groups.
- Group sorting persists upstream `sort_method`/`sort_descending`, re-sorts changed data and restores each group's header state.
- TLS route fragmentation/record fragmentation and fallback delay survive DB/share round-trip, are editable and preserve non-terminal `route-options`.
- Native TLS cipher suites are imported with unsupported values reported/dropped. `fp=unsafe` disables ordinary uTLS; REALITY retains required random uTLS. XHTTP/Xray JSON cipher suites use the full-config bridge. Invalid Clash ports cannot wrap and partial feeds block destructive subscription replacement.
- Connections resolve generated tags to running profile/member names. QUIC I1–I5 generation and capability diagnostics have core/client support; desktop controls remain open.
- Core matches beta.3 except retained Darwin NIC hardening. Rebuild with `./script/build-core`; existing staged setuid binaries require separate refresh.
- Headless engine/service/API, country-provider customization, Clash VLESS engine preference, full validation/editor/export and historical scanner/Kill Switch/hotkey gaps remain explicit. See the matrix for executed evidence and runtime limits.


## Current pin

| Item | Value |
|------|--------|
| Upstream remote | `upstream` → `https://github.com/throneproj/Throne.git` |
| **Pin** | tag **`1.4.0`** (`99f7ecd1`, 2026-11-05) |
| `upstream/dev` at last audit | `1.4.0-2-g1346d9ee` on 2026-11-06 — `5f92cd98` and `1346d9ee` excluded |
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

**Local patch:** keep `core/internal/boxmain/context.go` injecting `sysdns.DNSOverride` and `core/patches/sing-box-darwin-system-dns.patch`. Beta.6 `Apply(tunCIDR)` publishes a temporary network service; do not replay the old `netmon.DefaultInterface()` follow onto `sysdns_darwin.go`. `RestoreLegacy` cleans a dead beta.4/beta.5 `127.0.0.53` networksetup state.

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

## 1.4.0-beta.2 contracts and remaining gaps (historical pin)

All 14 release commits are triaged in `docs/UPSTREAM_TRACKING.md`.

- TLS URI queries preserve literal `+`; ECH shorthand/legacy fields and whole-certificate pins persist. XHTTP VLESS and Xray JSON ECH use the existing `xrayfullconfig` bridge. Dedicated Xray editor/export and full-config batch-test parity remain open.
- Native ECH HTTPS queries use direct/custom resolvers before proxy DNS, with the same ASCII query/match name; GUI resolver metadata is stripped from core TLS. Static configs skip bootstrap.
- WireGuard/Amnezia names follow explicit/adjacent-peer and wrapper priority; `awg://` and obfuscation/peer options persist. DB reload restores display fields, and generation converts legacy beans and `worker_count` to core `workers`. INI imports one coherent last peer; full multi-peer INI remains open.
- FakeIP uses `2001:db8::/32`, respects `fakeip_disable_ipv6`, and no longer overlaps ULA bypass.
- Native log history uses monospace/font/size/max-line settings, a collapsible literal filter and hold/resume while scrolled up. Regex/highlighting remain a gap. Tray Connect/Disconnect transitions refresh independently of window visibility.
- **Remote control API remains unported:** encrypted protocol/router/listener, key/allowlist and Integration settings are separate from the existing core tracker API. Kill Switch/global hotkeys/scanner and historical gaps remain open.
- Core sources match beta.2 except the retained Darwin NIC patch; Go modules and proto did not change. Host core rebuilt; actual-core validation covers ECH, Amnezia endpoints, XHTTP/Xray and FakeIP. Live remote handshakes, privileged traffic and cross-platform GUI remain untested; see matrix for final test totals.
- Verification on 2026-10-14: 290 workspace tests passed (10 suites), desktop check passed with two existing warnings, formatting/diff checks passed, and all seven rebuilt-core smoke groups passed. Regression fixes cover encoded plus/quoted repr preservation, WG peer association and DB reload/override, workers conversion and immediate log trimming.

## 1.4.0-beta.1 contracts and remaining gaps (historical pin)

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
