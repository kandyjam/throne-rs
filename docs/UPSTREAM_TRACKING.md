# Upstream tracking — [throneproj/Throne](https://github.com/throneproj/Throne)

Remote: `upstream` → `https://github.com/throneproj/Throne.git`  
Baseline branch: `upstream/dev`  
**Target release (this audit):** tag **`1.3.0-beta.2`** (`2feab171`, prerelease 2026-09-05).  
Last local `upstream/dev` tip at audit: describe `1.3.0-beta.2-1-g…`.  
**Product version:** `1.3.0-beta.2` (root [`VERSION`](../VERSION) + workspace Cargo version = this pin).

Agent skill for ongoing parity work: [`skills/throne-upstream-parity/SKILL.md`](../skills/throne-upstream-parity/SKILL.md).

## How to refresh

```bash
git fetch upstream --prune --tags
git log --oneline upstream/dev -50
# optional: commits since last audit
git log --oneline <last-audited>..upstream/dev
# vs release
git log --oneline 1.2.3..1.2.4
# when 1.2.5 lands:
git log --oneline 1.2.4..1.2.5
```

## Feature parity matrix

Status legend: ✅ done · 🧩 partial · ⏳ planned · ❌ out of scope (Go core)

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
| SQLite profiles/groups/settings/routes | `*Repo.cpp` | ✅ **wire-compatible** `throne.db` (incl. `route_rules`, path discovery) | `throne-storage` |
| Route profiles (raw + remote + auto-update) | `5b1482c3`, `54dde90e`, `4eeef231` | 🧩 **Routes dialog** (Common/Hijack/Warp/DNS/Route tabs, New/Clone/Export/Import/Edit/Delete/Update, simple+advanced rule editor, raw editor); deep attribute-tab editor later | `throne-domain`, `throne` |
| System proxy / TUN modes | MainWindow + settings | 🧩 system proxy OS; TUN config on Start (needs privileges) | `throne` |
| TUN private-range bypass flag | `3c344f78`, `e37e472a` | ✅ Tun Settings + `route_exclude_address` | `throne-domain` |
| **macOS Tun DNS exclude hole (#1738)** | `f97019d2` `subtractPrefix` | ✅ Darwin punches Tun CIDR out of private excludes | `throne-core-client` |
| Runtime stats UI | `bac76b83` | 🧩 live rates + Connections + **Traffic Graph** (SpeedWidget) | `throne` |
| Traffic stats aggregation | `ff3d3c10`, `11373611` | ✅ QueryStats → minute/hour `throne_stats.db` + Tools → **Traffic Stats** dialog | `throne-storage`, `throne` |
| URL / speed / IP / country tests | core RPC + menus | 🧩 URL/IP/simple Speedtest; full multi-thread speed later | `throne-core-client` |
| **Auto Selector (1.2.3)** | `6697ceaf`+ · plan · core · DialogAutoSelector | ✅ create + plan + Start · core RPC · Tools stats | `throne-domain`, `throne-core-client`, `throne`, `core/server` |
| **Auto Selector Xray full (1.2.4)** | `f97019d2` allow `xrayfullconfig`, socks bridge, `xray_full_configs` | ✅ plan + build + LoadConfig 16/17 · core gates | same |
| Multi-file import (1.2.3) | `f989e3e8` | ✅ Program → Import from file(s)… | `throne` |
| Program → New Profile (1.2.3) | `9cb7b2d3` | 🧩 **New Auto Selector** (+ add from input/clipboard); full type picker later | `throne` |
| Sub update keep-running (#1753) | `f97019d2` GroupUpdater | ✅ protect running + `kept_in_use` status text | `throne-domain`, `throne` |
| Xray geo asset download | `5206254a`, `1bd3a321` | ⏳ | `throne` / tools |
| WARP generate | `0957b8d5`, `61ff7a37` | ⏳ | `throne-import` |
| Mieru | `64f74878` | 🧩 type enum | `throne-import` |
| Config security / remove insecure | `cd7cb259` | 🧩 `security` field | `throne-domain` |
| Tray profile + route selector | `688d1cb4`, `c632c4e9` | 🧩 tray icon + show/start-stop/quit; profile and route selector later | `throne` |
| Sub update diff popup | `58276dd2`, `a05cc8fd` | 🧩 status bar + kept-in-use line (no modal list yet) | `throne` / `throne-domain` |
| Proxy list search UX | `8e726191` | 🧩 text filter | `throne` |
| Hotkeys | Settings `hk_*` | 🧩 dialog + saved labels; global rebind later | `throne` |
| i18n | `zh_CN` / `ru_RU` / `fa_IR` | ⏳ | `throne` |
| Profile editor tab order (1.2.4) | `33777e27` | ⏳ GPUI focus order later | `throne` |
| Linux CLI installer | `89f3ec1d` | ⏳ | `script/` / xtask |
| Windows unwritable config dir (1.2.4) | `d8e663cc` | ⏳ NSI; macOS package already uses AppConfig | `script/`, `throne` |
| Core: sing-box / xray / TUN / DNS | Go `core/server` | ❌ keep Go binary (`ThroneCore`) — **synced to 1.3.0-beta.2 sources** (sing-box replace `4db236a56cd3`, wireguard-go `cdac29e5a25e`) | `core/server` |
| Core IPC Start/Stop | `dispatch.go` + Qt framing | 🧩 Start/Stop/Test/QueryStats/QueryConnections/**CloseConnections** + AutoSelector RPC + Xray strategy (field 12 reserved) + **connection source field 14** | `throne-core-client`, `core/server` |
| **Close connections (1.3.0-beta.1)** | `96d079c9` | ✅ Connections tab × + `CloseConnections` RPC | `throne`, `throne-core-client` |
| **Connection source + LAN inbound (1.3.0-beta.2)** | `a1b6ac4b` | ✅ proto `source` + Source column when LAN inbound + wildcard listen shows LAN IP | `throne-core-client`, `throne-domain`, `throne` |
| **WireGuard schemes / INI (1.3.0-beta.2)** | `30c45594`, `2b3e829d` | ✅ `wg://` `wireguard://` `vpn://` + INI PresharedKey | `throne-import` |
| **Xray FinalMask `fm` (1.3.0-beta.2)** | `22de1725` | ✅ parse share-link query + persist in `outbound_json` | `throne-import`, `throne-domain` |
| **URL scheme opt-out (1.3.0-beta.2)** | `064fc284` | ✅ `url_scheme_auto_register` setting + Basic Settings checkbox (Install/Uninstall later) | `throne-domain`, `throne` |
| **Tun private ranges (1.3.0-beta.1)** | `1a512bf3` | ✅ `vpn_private_ranges` + Tun Settings editor | `throne-domain`, `throne` |
| **Snell / OpenVPN / OpenConnect** | `e5f31546`, `3ab8b595` | 🧩 types + Snell share-link import + sing-box outbound types; VPN challenge UI later | `throne-domain`, `throne-import`, `throne-core-client` |
| **Group URL/Speed test** | `09e49039` | ✅ group-tab context menu | `throne` |
| **Route endpoints** | `3ab8b595` | 🧩 `endpoint_profile_ids` persisted; share/materialize later | `throne-domain`, `throne-storage` |
| **Xray DNS inject (1.3.0-beta.1)** | `a7f20dd1` | ✅ stop sending reserved field 12; core injects direct-dns | `throne-core-client`, `core/server` |
| **macOS Tun DNS addr** | `6e9848c2` | ✅ core uses tun prefix addr (not Next) + local physical iface helper | `core/server` |
| **OTP / dashboard / JSON editor / L3 twins** | 1.3.0-beta.1 | ⏳ / 🧩 `vpn_l3_bridge` setting stored; dashboard RPC present in core, GUI later | `throne` |
| System proxy | `QvProxyConfigurator` | 🧩 macOS `networksetup` on Start when checkbox on | `throne-core-client` |
| GPUI shell | — | ✅ M0 | `throne` |
| Main window layout / ops | `mainwindow.ui` | 🧩 relative toolbar menus, Start/Stop, Tun/DNS/Proxy, group tabs, 5-col table, logs/conn/**Traffic Graph**, status + **v1.2.4** | `throne` |
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
