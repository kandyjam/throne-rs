# Upstream tracking — [throneproj/Throne](https://github.com/throneproj/Throne)

Remote: `upstream` → `https://github.com/throneproj/Throne.git`  
Baseline branch: `upstream/dev`  
**Target release (this audit):** tag **`1.4.0-beta.1`** (`50a22014`, 2026-10-04).
**Last audited tip:** `upstream/dev` describe **`1.4.0-beta.1-13-g32138a44`** on 2026-10-09 (13 commits past the pin; not absorbed).
**Product version:** `1.4.0-beta.1` (root [`VERSION`](../VERSION) + workspace Cargo version = this pin).

This pin identifies the core and schema baseline; it does not claim complete product parity. The scanner, Kill Switch, global hotkeys, OS proxy refactor, and historical gaps below remain partial or unported.

Agent skill for ongoing parity work: [`skills/throne-upstream-parity/SKILL.md`](../skills/throne-upstream-parity/SKILL.md).

## How to refresh

```bash
./script/upstream-audit            # current VERSION → newest 1.* tag
./script/upstream-audit 1.4.0-beta.1 # explicit release tag
```

Manual equivalent:

```bash
git fetch upstream --prune --tags
git log --oneline 1.3.2..1.4.0-beta.1
git diff --dirstat=files,0 1.3.2 1.4.0-beta.1
```

## Feature parity matrix

Status legend: ✅ implemented for the stated contract · 🧩 partial · ⏳ planned · ❌ out of scope to rewrite (the Go core is still synced and shipped). Runtime verification limits are listed below.

| Area | Upstream signal (commits / modules) | Rust status | Target crate |
|------|-------------------------------------|-------------|--------------|
| Protocol catalog (SS/VMess/VLESS/Trojan/HY2/TUIC/…) | README + `OutboundFactory` | 🧩 types exist; full beans partial | `throne-domain`, `throne-import` |
| Share-link import | `GroupUpdater::RawUpdater` | ✅ common schemes | `throne-import` |
| Multi-line / base64 subscription body | `RawUpdater::update` | ✅ line + b64 split | `throne-import` |
| Clash YAML sub | `updateClash` | 🧩 proxies list (core types) | `throne-import` |
| Sing-box / Xray JSON sub | `updateSingBox` / `updateXray` | 🧩 outbounds + custom full | `throne-import` |
| Full Xray subscription | `2fc64c51` + 1.2.4 subtype | ✅ import as `xrayfullconfig` | `throne-import` |
| SIP008 / WireGuard file | `updateSIP008` / `updateWireguardFileConfig` | ✅ | `throne-import` |
| `throne://add/` · `throne://route/` · `throne://remoteRoute/` · `throne://addsub/` | `e7eb0438` deeplink schemes | ✅ | `throne-import` |
| SQLite profiles/groups/settings/routes | `*Repo.cpp` | ✅ **wire-compatible** `throne.db`, including `endpoint_json`, subscription option/metadata JSON, `latency_at`, IP-list generations and scan progress; legacy migration and round-trip contracts tested | `throne-storage` |
| Route profiles (raw + remote + auto-update) | `5b1482c3`, `54dde90e`, `4eeef231` | 🧩 **Routes dialog** (Common/Warp/DNS/Route tabs; retired Hijack tab hidden, New/Clone/Export/Import/Edit/Delete/Update, simple+advanced rule editor, raw editor); deep attribute-tab editor later | `throne-domain`, `throne` |
| System proxy / TUN modes | MainWindow + settings | 🧩 system proxy OS; TUN config on Start (needs privileges) | `throne` |
| TUN private-range bypass flag | `3c344f78`, `e37e472a` | ✅ Tun Settings + `route_exclude_address` | `throne-domain` |
| **macOS Tun DNS exclude hole (#1738)** | `f97019d2` `subtractPrefix` | ✅ Darwin punches Tun CIDR out of private excludes | `throne-core-client` |
| Runtime stats UI | `bac76b83` | 🧩 live rates + Connections + **Traffic Graph** (SpeedWidget) | `throne` |
| Traffic stats aggregation | `ff3d3c10`, `11373611` | ✅ QueryStats → minute/hour `throne_stats.db` + Tools → **Traffic Stats** dialog | `throne-storage`, `throne` |
| URL / speed / IP / country tests | core RPC + menus | 🧩 URL/IP/simple Speedtest; stored-profile tests use effective endpoint clones and report resolution errors; full multi-thread speed later | `throne-core-client`, `throne` |
| **Auto Selector (1.2.3)** | `6697ceaf`+ · plan · core · DialogAutoSelector | ✅ create + plan + Start · core RPC · Tools stats | `throne-domain`, `throne-core-client`, `throne`, `core/server` |
| **Auto Selector Xray full (1.2.4)** | `f97019d2` allow `xrayfullconfig`, socks bridge, `xray_full_configs` | ✅ plan + build + LoadConfig 16/17 · core gates | same |
| Multi-file import (1.2.3) | `f989e3e8` | ✅ Program → Import from file(s)… | `throne` |
| Program → New Profile (1.2.3) | `9cb7b2d3` | 🧩 **New Auto Selector** (+ add from input/clipboard); full type picker later | `throne` |
| Sub update keep-running (#1753) | `f97019d2` GroupUpdater | ✅ protect running + `kept_in_use` status text | `throne-domain`, `throne` |
| Xray geo asset download | `5206254a`, `1bd3a321` | ⏳ | `throne` / tools |
| WARP generate | `0957b8d5`, `61ff7a37` | ⏳ | `throne-import` |
| Mieru | `64f74878` | 🧩 type enum | `throne-import` |
| Config security / remove insecure | `cd7cb259`, `50a22014` | ✅ derive labels and cleanup decisions from effective TLS/cipher/private-host/pin/raw-config semantics; stale `insecure` flag is not authoritative; global-only certificate compromise is not removable | `throne-domain`, `throne` |
| **Global skip_cert (1.3.0-beta.3)** | `c42111b6` | ✅ `skip_cert` setting + outbound TLS `insecure` | `throne-domain`, `throne-core-client`, `throne` |
| **Auto Selector outage recovery (1.3.0-beta.3)** | `66c7612b` | ✅ keep unavailable members when they are the whole pool | `throne-domain` |
| **Connections add-to-route** | `5c322220`, `be6206e0`, `bfe5d59d` | ✅ domain levels, process name/path and destination targets; existing-rule/coverage marks avoid counting a target's own line in another list | `throne`, `throne-domain` |
| **Inner hop endpoints (1.3.0-beta.3)** | `8a844c5e` | 🧩 `inner_hop_endpoint_ids` persisted; generate later | `throne-domain`, `throne-storage` |
| Tray profile + route selector | `688d1cb4`, `c632c4e9` | 🧩 tray icon + show/start-stop/quit; profile and route selector later | `throne` |
| Sub update diff popup | `58276dd2`, `a05cc8fd` | 🧩 status bar + kept-in-use line (no modal list yet) | `throne` / `throne-domain` |
| Proxy list search UX | `8e726191` | 🧩 text filter | `throne` |
| Hotkeys | Settings `hk_*`, `c672f85e` | 🧩 dialog + saved labels; upstream global-hotkey rewrite/KDE Wayland support not ported | `throne` |
| i18n | `zh_CN` / `ru_RU` / `fa_IR` | ⏳ | `throne` |
| Profile editor tab order (1.2.4) | `33777e27` | ⏳ GPUI focus order later | `throne` |
| Linux CLI installer | `89f3ec1d` | ⏳ | `script/` / xtask |
| Windows unwritable config dir (1.2.4) | `d8e663cc` | ⏳ NSI; macOS package already uses AppConfig | `script/`, `throne` |
| Core: sing-box / xray / TUN / DNS | Go `core/` (moved out of `core/server` in 1.3.0-beta.4) | ❌ Rust data-plane rewrite remains out of scope; shipping `ThroneCore` **synced to 1.4.0-beta.1** (sing-box `ebe0747b4182`, xray `e662c22ab109`, wireguard-go `3517ea9271e4`) with local Darwin physical-NIC patch retained via `netmon` | `core/` |
| Core IPC Start/Stop | `dispatch.go` + Qt framing | 🧩 Start/Stop/Test/QueryStats/QueryConnections/**CloseConnections**, AutoSelector, scanner/egress RPCs; API service keeps trackers available; Xray strategy reserved field 12 and connection source field 14 retained | `throne-core-client`, `core/` |
| **IP lists and scanner (1.4.0-beta.1)** | `86f196d4` | 🧩 native manual lists and bounded TCP scanning only: 4,096 targets, 256 hosts per CIDR, concurrency 16, batches 64; scan models/storage/RPCs exist, but full probes, resume UI, remote updates and default list seeds are not ported | `throne-domain`, `throne-storage`, `throne-core-client`, `throne` |
| **Profile/group endpoint sources** | `1809cb0e` | 🧩 group/profile inheritance and DB round-trip; profile list assignment and Own/Inherit restoration; Start and stored-profile tests clone server overrides while retaining identity; group/fixed-address editor remains | `throne-domain`, `throne-storage`, `throne` |
| **Experimental Kill Switch** | `866e13a4` | 🧩 `kill_switch` persisted and upstream guard retained in core; **no GUI control or active lifecycle integration**; no runtime protection claim | `throne-domain`, `throne-storage`, `core/`, `throne` |
| **Retired DNS hijack / selectable TUN stack** | `a7534e3d` | ✅ retired standalone controls/RPC removed or hidden; no selectable `stack` emitted; Linux auto-redirect option persists. Mandatory DNS route action remains a separate core behavior | `throne-core-client`, `throne`, `core/` |
| **L3 bridge config / Block final** | target core config contract | ✅ `throne-br` name, direct-rule twins and rule ordering; Block default becomes final reject and cannot acquire a direct bridge bypass; privileged runtime remains untested | `throne-core-client` |
| **Close connections (1.3.0-beta.1)** | `96d079c9` | ✅ Connections tab × + `CloseConnections` RPC | `throne`, `throne-core-client` |
| **Connection source + LAN inbound (1.3.0-beta.2)** | `a1b6ac4b` | ✅ proto `source` + Source column when LAN inbound + wildcard listen shows LAN IP | `throne-core-client`, `throne-domain`, `throne` |
| **WireGuard schemes / INI (1.3.0-beta.2)** | `30c45594`, `2b3e829d` | ✅ `wg://` `wireguard://` `vpn://` + INI PresharedKey | `throne-import` |
| **Xray FinalMask `fm` (1.3.0-beta.2)** | `22de1725` | ✅ parse share-link query + persist in `outbound_json` | `throne-import`, `throne-domain` |
| **URL scheme opt-out (1.3.0-beta.2)** | `064fc284` | ✅ `url_scheme_auto_register` setting + Basic Settings checkbox (Install/Uninstall later) | `throne-domain`, `throne` |
| **Tun private ranges (1.3.0-beta.1)** | `1a512bf3` | ✅ `vpn_private_ranges` + Tun Settings editor | `throne-domain`, `throne` |
| **Snell / OpenVPN / OpenConnect** | `e5f31546`, `3ab8b595` | 🧩 types + Snell share-link import + sing-box outbound types; VPN challenge UI later | `throne-domain`, `throne-import`, `throne-core-client` |
| **Group URL/Speed test** | `09e49039` | ✅ group-tab context menu | `throne` |
| **Route endpoints** | `3ab8b595`, `8a844c5e` | 🧩 `endpoint_profile_ids` + `inner_hop_endpoint_ids` persisted; share/materialize later | `throne-domain`, `throne-storage` |
| **Xray DNS inject (1.3.0-beta.1)** | `a7f20dd1` | ✅ stop sending reserved field 12; core injects direct-dns | `throne-core-client`, `core/server` |
| **macOS Tun DNS addr** | `6e9848c2` | ✅ core uses tun prefix addr (not Next) + local physical iface helper | `core/server` |
| **OTP / dashboard / JSON editor** | 1.3.0-beta.1 | ⏳ dashboard RPC present in core; OTP/dashboard/deep editor GUI remains | `throne` |
| System proxy | `QvProxyConfigurator`, `b19a4b95` | 🧩 existing macOS `networksetup` integration; upstream cross-platform OS proxy refactor not ported | `throne-core-client` |
| GPUI shell | — | ✅ M0 | `throne` |
| Main window layout / ops | `mainwindow.ui` | 🧩 relative toolbar menus, Start/Stop, Tun/Proxy, group tabs, 5-col table, logs/conn/**Traffic Graph**, status version from Cargo; retired standalone DNS control hidden | `throne` |
| Secondary dialogs | BasicSettings / GroupItem / ProfileEdit | 🧩 Basic (Common + **Subscription** + **URL Scheme** auto-register) / Groups/Add/Routing/Tun/Hotkey/**Edit Profile (rename)**; deep ProfileEdit later | `throne` |

## Defaults synced from upstream

| Setting | Upstream default (recent) | Rust default |
|---------|---------------------------|--------------|
| `remote_dns` | Google DoH (`56d0d9fd`) | `https://dns.google/dns-query` |
| `vpn_strict_route` | off by default (`5a7f2c96`), Windows CI may enable | `false` |
| `test_latency_url` | thronged URL in settings | `https://www.gstatic.com/generate_204` |
| `inbound_socks_port` | typical 2080 | `2080` |

## Implementation waves

1. **Wave A** — tracking doc, share-link import, SQLite load/save, settings skeleton, UI Import ✅  
2. **Wave B** — Clash/JSON/SIP008/WG sub, route share import, route_profiles table, UI route cycle ✅  
3. **Wave C** — Start/Stop/URL/IP/Speed(simple)/Stats/Connections ✅; active route + MetaCubeX `srslist` + jsDelivr mirror + adblock ✅  
4. **Wave D** — TUN privilege helper, tray, stats charts / `throne_stats.db`  
5. **Wave B+** — HTTP sub ✅; remote route fetch ✅; full Clash option parity / OS deeplink registration  
6. **Wave 1.2.3** — version pin · Auto Selector create/plan/Start · multi-file import · Program menu · core + stats dialog ✅  
7. **Wave 1.2.4** — version pin · core sync (egress, xray full gates, DNS quiet) · Auto Selector Xray full · Tun exclude hole · sub kept-in-use ✅  
8. **Wave 1.3.0-beta.1** — core sync (Tun/DNS, CloseConnections, VPN RPCs, sing-box 1.14) · Snell import · Tun ranges · group URL test · connection close ✅  
9. **Wave 1.3.0-beta.2** — connection source · LAN inbound label · WG/vpn schemes · FinalMask · url_scheme_auto_register · core sing-box/WG bump ✅  
10. **Wave 1.3.0-beta.3** — skip_cert · Auto Selector empty-pool recovery · connections add-to-route · inner-hop ids · core sing-box/Xray bump ✅  
11. **Wave 1.3.2** — core tree `core/` · DNS cache persist off · Darwin Tun allows local DNS · IPv6 private bypass · empty sub rejected · simple-rule domains lowercased · `reject_method` import ✅  
12. **Wave 1.4.0-beta.1** — tag/core/schema pin · API service and L3/Block config contracts · bounded manual TCP scanner · profile endpoint sources · connection route targets · derived security/cleanup 🧩 (explicit product gaps below).

## Commit triage notes (1.3.2 → 1.4.0-beta.1)

All **23 commits** are accounted for below. Source: local `git log --reverse --format='%h %s' 1.3.2..1.4.0-beta.1`; no changes after the requested tag are included. The audited development tip is 13 commits newer and is not the product pin.

| SHA | Upstream change | Classification and action |
|-----|-----------------|---------------------------|
| `b19a4b95` | Refactor OS proxy installation | **Must port / gap:** ⏳ cross-platform refactor not ported; existing Rust/macOS path retained |
| `c672f85e` | Rewrite global hotkeys; support KDE Wayland | **Must port / gap:** ⏳ native global registration and KDE Wayland support not ported; saved hotkey labels are not equivalent |
| `be6206e0` | Route any domain level and mark existing rules (#1915) | **Must port:** ✅ domain/process/destination target generation and existing/covered rule indication |
| `bfe5d59d` | Avoid coverage by the target's own line in another route list | **Must port:** ✅ coverage logic regression contract |
| `ba87d52f` | Automatically update AUR package on stable release | **Release infrastructure:** ⏳ native rewrite release publishing remains separate |
| `20a36e8d` | Publish RPM repository to GitHub Pages | **Release infrastructure:** ⏳ RPM repository publishing not ported |
| `bb8b3d21` | Publish APT repository to GitHub Pages | **Release infrastructure:** ⏳ APT repository publishing not ported |
| `1e13ca3a` | Fix empty APT indexes; pin publishing actions | **Release infrastructure:** ⏳ depends on the unported APT workflow |
| `a4f601d8` | Align AUR sources and CI build settings | **Release infrastructure:** ⏳ upstream Qt AUR workflow not copied into the native build |
| `53e59b53` | Sign APT and RPM repositories | **Release infrastructure:** ⏳ native repository publishing/signing remains |
| `5df59a1b` | Cross-distro RHEL/SLE dependency expressions | **Packaging:** ⏳ distro package dependency validation remains; host core build is not Linux package validation |
| `af360a14` | Fix SLE dependencies | **Packaging:** ⏳ same distro validation gap |
| `d65f9703` | Group clean-up actions under a submenu (#1940) | **Cosmetic/UX:** 🧩 cleanup actions exist and security semantics updated; nested Clean up submenu not ported |
| `fe44ac03` | Fix #1338 in vendored fkYAML | **Qt/C++ parser:** N/A no fkYAML use in Rust; no blanket claim of YAML option parity |
| `0b429e6d` | Rename release packages to lowercase | **Packaging:** ⏳ upstream package naming workflow not ported |
| `4de37bb9` | Stop attaching Android AAR to releases | **Out of desktop scope:** N/A Android release artifact policy; Go source sync retains upstream layout |
| `86f196d4` | Add IP scanner | **Must port + core:** 🧩 core/proto synced; IP-list/scan models and compatible SQLite storage; manual list + TCP UI subset only. Full ICMP/HTTP/config/WARP probes, resume UI, remote update scheduler and default list seeds remain |
| `1809cb0e` | Integrate IP lists with groups/profiles | **Must port:** 🧩 persisted endpoint sources; profile list assignment and Own/Inherit restoration; effective endpoint used by Start and stored-profile tests; group/fixed-address editor remains |
| `6a030fe3` | Update sing-box | **Core:** ✅ exact target module pins, rebuilt host `ThroneCore`; no Rust data-plane rewrite |
| `a7534e3d` | Remove standalone DNS hijack and selectable TUN stacks | **Must port + core:** ✅ retired controls/RPC handled; generated TUN omits `stack`; physical-interface patch follows the new `netmon` package. DNS protocol route action is still required |
| `866e13a4` | Experimental Kill Switch | **Must port + core:** 🧩 setting persists and guard code is present; GUI/lifecycle integration and privileged runtime verification are absent |
| `ec7a5432` | Fix connection table width | **Qt-only presentation:** N/A Qt table sizing implementation; GPUI layout has separate verification |
| `50a22014` | Improve security advisory | **Must port:** ✅ derived security, private endpoints, raw/custom/Xray forms, pin-aware MASQUE and cleanup rules; global-only skip-cert compromise does not make a profile removable |

### Target contracts implemented during this audit

- Generated configs contain the target's **API service** so stats/connection trackers exist without enabling its HTTP listener (`listen_port = 0`).
- L3 bridge generation uses `bridge_name = "throne-br"`, emits direct-rule twins in order, and places fallback after ordinary/adblock rules. A Block final emits a final reject and never gains a bridge bypass.
- IP-list writes publish entries atomically through upstream `entries_generation`; old entry tables migrate, headers do not erase unloaded entries, and scan seeds/cursors round-trip. Interrupted scan normalization is explicit rather than performed whenever another DB connection opens.
- `endpoint_json`, group subscription metadata/options and `latency_at` survive full state saves. Endpoint clones preserve ports and TLS/transport identity, emit WebSocket Host only in headers, and leave saved beans unchanged. Serverless and Hysteria2 realm profiles are blocked before list lookup/assignment.
- The native TCP scanner is deliberately bounded to **4,096 targets**, **256 hosts per CIDR**, **16 concurrent probes**, and **64 targets per batch**. It is not the complete upstream scanner workflow.
- Core sources and `libcore.proto` match the requested tag except documented local hardening. The Darwin physical-NIC helper now uses `netmon.DefaultInterface()` rather than the retired `boxdns` monitor.

### Verification status (2026-10-09)

| Check | Evidence/status |
|-------|-----------------|
| Domain contract tests after security/cleanup changes | **Executed:** `cargo test -p throne-domain` — 89 passed |
| Actual rebuilt core smoke checks | **Executed:** six checks, including valid WebSocket endpoint config and rejection of invalid WebSocket `transport.host` |
| Go validation | **Executed:** vet across `./...`; race tests for `internal/scan`, `internal/rulesets` and `internal/rpc`; guard/sysdns compiled with the host build tags |
| Host core packaging build | **Executed:** host `ThroneCore` rebuilt with packaging tags |
| Final Rust workspace tests | **Executed:** `cargo test --workspace --locked` — 266 passed across nine suites |
| Desktop compilation | **Executed:** `cargo check -p throne --locked` — no errors; two unused-code warnings for the hidden legacy Hijack tab and its validator |
| Formatting and whitespace | **Executed:** `cargo fmt --all --check` and `git diff --check` passed |
| Native GUI exercise | **Executed in isolated previews:** scanner and routing windows render without clipping at their default sizes; list selection, save-as-new, endpoint-reset callback and routing toggle exercised. Temporary DB and mock endpoint/routing callbacks were used; live MainWindow persistence and privileged runtime were not exercised |
| Semantic review | **Inspected:** final endpoint, scanner error recovery, security cleanup and packaging changes have no open actionable findings; Windows packaging reviewed from source only |
| Strict Clippy | **Not clean:** blocked by pre-existing `auto_selector`/`store` warnings; no strict-lint success claim |
| Windows and privileged TUN/L3/Kill Switch runtime | **Untested:** no cross-platform or privileged-runtime completion claim |

Remaining historical gaps in the matrix and earlier audits remain open unless a later row explicitly closes them. In particular, schema/RPC coverage does not imply a complete scanner, Kill Switch, MASQUE editor or platform UI.

## Commit triage notes (1.3.0-beta.3 → 1.3.2)

76 commits (`1.3.0-beta.4`, `1.3.0`, `1.3.1`, `1.3.2`). Full log: `git log --oneline 1.3.0-beta.3..1.3.2`.

| SHA | Summary | Action |
|-----|---------|--------|
| `e3e2bca4` | core folder `core/server` → `core/` | ✅ vendored tree + `script/build-core` |
| `d7d8692c` | `dns_persist_cache` default off | ✅ `cache_file.store_fakeip/store_dns` |
| `3f2f1880` | drop Darwin Tun local-DNS hard error | ✅ |
| `531fd035` | IPv6 private ranges + marker repo | ✅ ranges (+ old-default upgrade); ⏳ markers |
| `1539f5ee` | lowercase simple domain/suffix/keyword | ✅ regex unchanged |
| `99e67ba6` | reject empty subscription body | ✅ group left unchanged |
| `dc3acf48` | import `reject_method` | ✅ |
| `8d251aeb` | dedupe rule-sets | ✅ already unique in `compile_route_section` |
| `c6bd8e1e` + later go.mod | sing-box / xray / WG pins through 1.3.2 | ✅ |
| `77fad407` | Xray DNS random + interface bind | ✅ core pin |
| `2ce157d7` | MASQUE + `WarpRegister` | 🧩 RPC in core; GUI/import later |
| `ccf3b459` | subscription userinfo card | 🧩 raw header already stored as `group.info`; card UI later |
| `abf9b07d` | Update All External Resources | ⏳ `UpdateRuleSets` RPC present, no Tools action |
| `a1814347` | chain-through-endpoint DNS | ⏳ no multi-hop builder yet |
| `f94aadb0` | IDN domains | ⏳ |
| `4a1005f0` | auto selector (mobile core) | N/A desktop plan |
| `d045fc62` | deprecate hijack-dns UI | ⏳ generator still emits hijack-dns |
| `ed8a7059` | updater exempt from insecure TLS | ⏳ no self-updater |
| i18n / Qt a11y / Android / connections tree / diagnostics / TrustTunnel editor | | ⏳ / N/A |

Local patch at that audit used `boxdns.DefaultInterface()`. On the current 1.4.0-beta.1 pin, `core/internal/sysdns/sysdns_darwin.go` retains the physical-NIC selection through `netmon.DefaultInterface()` so system DNS is not pointed at `utun`.

## Commit triage notes (1.3.0-beta.2 → 1.3.0-beta.3)

17 commits. User-visible / core must-port vs Qt-only:

| SHA | Summary | Action |
|-----|---------|--------|
| `66c7612b` | Auto Selector recovery from outage | ✅ keep unavailable when they are the whole pool |
| `5e009c11` | i18n / translation context | ⏳ |
| `e7a13b5a` | Route dialog width | N/A Qt |
| `8a844c5e` | Inner endpoint hops | 🧩 persist `inner_hop_endpoint_ids`; generate later |
| `25c3dac7` / `8a270ae9` | Connections focus line | N/A Qt |
| `c42111b6` | Global insecure / skip_cert | ✅ outbound TLS `insecure` |
| `d82783a1` | Xray asset downloader history | ⏳ combo history later |
| `1d5bc45b` | Update sing-box | ✅ `go.mod` / `box.go` / `boxmain` |
| `1ff14ded` | Update Xray-core v26.9.9 | ✅ replace pin |
| `61863537` / `2d024adb` | Windows NSI process searcher | ✅ go.mod (Windows core) |
| `c981c732` | Revert Xray pin tweak | ✅ |
| `e00806d1` / `bc13197d` | Runtime stats in main window | 🧩 Traffic Graph already in main window |
| `5c322220` | Connections add dest/domain to route | ✅ right-click menu |
| `848ff42b` | Data view / EndpointHost | 🧩 `endpoint_host` already; Qt HTML N/A |
| Inno installer / MASQUE | release notes | ⏳ Windows packager; MASQUE via sing-box bump |

## Commit triage notes (1.3.0-beta.1 → 1.3.0-beta.2)

29 commits. User-visible / core must-port vs Qt-only:

| SHA | Summary | Action |
|-----|---------|--------|
| `2feab171` | fix constant collision | N/A Qt |
| `a1b6ac4b` | improve LAN support | ✅ LocalNetwork + inbound label + Source column |
| `2b3e829d` | fix wireguard decoding | ✅ INI keys / PresharedKey |
| `339340e4` | improve db error handling | ⏳ rusqlite already surfaces errors |
| `1f9207c0` | upgrade golang to 1.27 | ⏳ CI; core still `go 1.26` in go.mod |
| `1098f990` | fix grpc issues in sing-box | ✅ sing-box replace bump |
| `f2cbd0ca` | minor tray/icon/route refactors | N/A Qt |
| `59475280` | zh_CN i18n | ⏳ |
| `fd305f1f` | hotkey accessible names | N/A Qt |
| `6b54f211` | refactoring tray/theme/routes | N/A Qt |
| `065381c7` | rewrite subscription logic | 🧩 Rust already identity-preserves; no Qt parser split |
| `840bee60` | minor WG/sub improvements | 🧩 covered by WG import |
| `30c45594` | `wireguard://` `vpn://` | ✅ |
| `5363313a` / `44f7a93b` | font/theme | N/A Qt |
| `f5b4c8a5` | improve OpenVPN import | ⏳ ovpn file import later |
| `8c720ff2` | dashboard asset path | ⏳ dashboard GUI |
| `22de1725` | FinalMask (`fm`) | ✅ share-link parse + JSON |
| `4b950612` | test path | N/A Qt |
| `c8af9a2a` | discard stale test poll (#1790) | N/A no ResultPoller; one-shot URL test |
| `48a870ad` | connections tableModel | N/A Qt; GPUI table already |
| `133f8747` | invalid checking + context menu | ⏳ CheckConfig skip later |
| `c453431d` / `064fc284` / `5f048553` | URL scheme | ✅ setting + checkbox |
| `c4095535` | OpenVPN/OpenConnect reconnect | ⏳ OTP GUI |
| `f340000b` | WG widget length | N/A Qt editor |
| `8b1905bb` | update amnezia | 🧩 INI keys stored in raw conf |
| `49cea029` | RPM packaging | ⏳ |

## Commit triage notes (1.2.4 → 1.3.0-beta.1)

49 commits. User-visible / core must-port vs Qt-only:

| SHA | Summary | Action |
|-----|---------|--------|
| `a7f20dd1` | inject sing-box direct-dns into Xray | ✅ proto field 12 reserved + core |
| `b2de0ea3` | Xray DNS + loopback pinning | ✅ core |
| `6e9848c2` | Darwin tun DNS address | ✅ core tunPrefix.Addr() |
| `08f2f501` | Windows extra process | ✅ core |
| `7d96d9e8` / `8fc1b3d6` | Tun loopback | ✅ core |
| `96d079c9` | Close connections | ✅ |
| `09e49039` | Group URL/Speed test | ✅ |
| `1a512bf3` | Alter private ranges | ✅ |
| `e5f31546` | Snell | 🧩 type + share link |
| `3ab8b595` | OpenVPN/OpenConnect + route endpoints | 🧩 types + persist ids |
| `610f572a` | Hysteria Gecko obfs | ⏳ emit from editor later |
| `8b73d132` | TLS spoof | ⏳ |
| `79ee81f9` | sing-box dashboard | ⏳ GUI; core RPC present |
| `2e7182b9` | OTP management | ⏳ |
| `509dbf24` | Preset settings | ⏳ |
| `3dfead38` | Custom JSON editor | ⏳ Qt widget |
| `a6c8274c` | L3 bridge | 🧩 setting stored |
| `6899acb5` | Disable custom dock icon | 🧩 `follow_status_in_taskbar` |
| `12feb429` | Remember operation mode | ✅ already persist-on-toggle |
| `d746dc1c` | Extend DNS settings | ⏳ extra DNS fields |
| `f39a486e` | Connections search/sort persist | ⏳ |
| Qt/i18n/installer | various | ⏳ / N/A |

## Commit triage notes (1.2.4 → tip / pre-1.2.5, superseded)

| SHA | Summary | Action |
|-----|---------|--------|
| `ed2fdde2` | update naiveproxy to v150.0.7871.63-1 (`cronet-go` replace + platform libs) | ✅ `core/server/go.mod` + `go.sum` synced from tip |

No GUI/RPC/proto changes. Re-audit when upstream tags **1.2.5**.

## Commit triage notes (1.2.3 → 1.2.4)

8 commits on the release line (GitHub compare `1.2.3...1.2.4`):

| SHA | Summary | Action |
|-----|---------|--------|
| `0e8aeaca` | fix unix isAdmin crash | Qt-only; Tun privilege already post-core |
| `2408281a` | fix some xray dns issues | core go.mod/xray replace |
| `f97019d2` | Auto Selector dup + Xray full + MacOS Tun DNS | **ported** domain/config/core/sub |
| `18a82022` | xray + auto_redirect | core `egress.go` |
| `fa44e69e` | mid-test query data reclaim | core Reclaim; GUI session gen ⏳ |
| `d8e663cc` | installer unwritable config | ⏳ Windows packaging |
| `7cfc2253` | zh_CN i18n | ⏳ |
| `33777e27` | profile editor tab order | ⏳ GPUI |

## Commit triage notes (1.2.2 → 1.2.3)

24 commits — Auto Selector, core bumps, multi-file import, Program → New Profile, tray, Unix crash re-release. See skill 1.2.3 section; treated as landed baseline.

## Commit triage notes (recent upstream themes)

- **Core upgrades**: sing-box / xray / amnezia bumps dominate release cadence → pin Go module when packaging core binary.  
- **Xray depth**: interface bind, geo assets, full-config test, UDP → core-client exposes `xray_full_configs` + DNS address.  
- **Subscription UX**: identity-preserving apply + kept-in-use; Basic Settings → Subscription ✅; PeriodicRunner for `sub_auto_update` / `route_auto_update` ✅ (poll 60s, min interval 30 min, respects `skip_auto_update` / route `auto_update`).  
- **Routing**: remote route profiles via deeplink, raw routes, warp-bypass outbound.  
- **Platform**: Linux TUN, installer, Wayland hotkey branch (`upstream/dev-wayland-hotkey`).
