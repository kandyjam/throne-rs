# Upstream tracking — [throneproj/Throne](https://github.com/throneproj/Throne)

Remote: `upstream` → `https://github.com/throneproj/Throne.git`  
Baseline branch: `upstream/dev`  
**Target release (this audit):** tag **`1.2.3`** (`3d1b4e86` / re-release fix for Unix startup crash)  
Last local `upstream/dev` tip known at fork time: `fb68b742` (*update xray core*) — still **1.2.2** describe; **1.2.3 / 1.2.4** exist on GitHub and should be fetched when network allows.  
**Product version:** `1.2.3` (root [`VERSION`](../VERSION) + workspace Cargo version = upstream release tag / `NKR_VERSION`)

Agent skill for ongoing parity work: [`skills/throne-upstream-parity/SKILL.md`](../skills/throne-upstream-parity/SKILL.md).

## How to refresh

```bash
git fetch upstream --prune --tags
git log --oneline upstream/dev -50
# optional: commits since last audit
git log --oneline <last-audited>..upstream/dev
# vs release
git log --oneline 1.2.2..1.2.3
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
| Full Xray subscription | `2fc64c51` | 🧩 config array → custom | `throne-import` |
| SIP008 / WireGuard file | `updateSIP008` / `updateWireguardFileConfig` | ✅ | `throne-import` |
| `throne://add/` · `throne://route/` · `throne://remoteRoute/` · `throne://addsub/` | `e7eb0438` deeplink schemes | ✅ | `throne-import` |
| SQLite profiles/groups/settings/routes | `*Repo.cpp` | ✅ **wire-compatible** `throne.db` (incl. `route_rules`, path discovery) | `throne-storage` |
| Route profiles (raw + remote + auto-update) | `5b1482c3`, `54dde90e`, `4eeef231` | 🧩 **Routes dialog** (Common/Hijack/Warp/DNS/Route tabs, New/Clone/Export/Import/Edit/Delete/Update, simple+advanced rule editor, raw editor); deep attribute-tab editor later | `throne-domain`, `throne` |
| System proxy / TUN modes | MainWindow + settings | 🧩 system proxy OS; TUN config on Start (needs privileges) | `throne` |
| TUN private-range bypass flag | `3c344f78`, `e37e472a` (1.2.3 fix) | ✅ Tun Settings dialog + `route_exclude_address` when bypass enabled | `throne-domain` |
| Runtime stats UI | `bac76b83` | 🧩 live rates + Connections tab | `throne` |
| Traffic stats aggregation | `ff3d3c10`, `11373611` | 🧩 QueryStats rates (no throne_stats.db yet) | `throne-core-client` |
| URL / speed / IP / country tests | core RPC + menus | 🧩 URL/IP/simple Speedtest; full multi-thread speed later | `throne-core-client` |
| **Auto Selector (1.2.3)** | `6697ceaf`+ · `autoselector` · `AutoSelectorPlan` · `server_autoselector.go` · `DialogAutoSelector` | ✅ create + plan + Start (native outbound) · core RPC · **Tools → Auto Selector Stats** (live table, recheck/pin/release, only-problems filter) | `throne-domain`, `throne-core-client`, `throne`, `core/server` |
| Multi-file import (1.2.3) | `f989e3e8` | ✅ Program → Import from file(s)… (native multi-select + path clipboard fallback) | `throne` |
| Program → New Profile (1.2.3) | `9cb7b2d3` | 🧩 **New Auto Selector** (+ add from input/clipboard); full type picker later | `throne` |
| Xray geo asset download | `5206254a`, `1bd3a321` | ⏳ | `throne` / tools |
| WARP generate | `0957b8d5`, `61ff7a37` | ⏳ | `throne-import` |
| Mieru | `64f74878` | 🧩 type enum | `throne-import` |
| Config security / remove insecure | `cd7cb259` | 🧩 `security` field | `throne-domain` |
| Tray profile + route selector | `688d1cb4`, `c632c4e9` | 🧩 tray icon + show/start-stop/quit; profile and route selector later | `throne` |
| Sub update diff popup | `58276dd2`, `a05cc8fd` | 🧩 status bar `+added −removed · kept` (no modal list yet) | `throne` / `throne-domain` |
| Proxy list search UX | `8e726191` | 🧩 text filter | `throne` |
| Hotkeys | Settings `hk_*` | 🧩 dialog + saved labels; global rebind later | `throne` |
| i18n | `zh_CN` / `ru_RU` / `fa_IR` | ⏳ | `throne` |
| Linux CLI installer | `89f3ec1d` | ⏳ | `script/` / xtask |
| Core: sing-box / xray / TUN / DNS | Go `core/server` | ❌ keep Go binary (`ThroneCore`) | `core/server` |
| Core IPC Start/Stop | `dispatch.go` + Qt `RPC.cpp` framing | 🧩 Start/Stop/Test/QueryStats/QueryConnections + **QueryAutoSelectors / AutoSelectorAction** (1.2.3 dispatch table); route compile + **srslist + jsDelivr + adblock**; full BuildSingBoxConfig DNS/xray parity still open | `throne-core-client`, `core/server` |
| System proxy | `QvProxyConfigurator` | 🧩 macOS `networksetup` on Start when checkbox on | `throne-core-client` |
| GPUI shell | — | ✅ M0 | `throne` |
| Main window layout / ops | `mainwindow.ui` | 🧩 relative toolbar menus (under each btn), Start/Stop, Tun/DNS/Proxy, group tabs, 5-col table, logs/conn, status + **v1.2.3** | `throne` |
| Secondary dialogs | BasicSettings / GroupItem / ProfileEdit | 🧩 Basic/Groups/Add/Routing/Tun/Hotkey/**Edit Profile (rename)** modals; deep ProfileEdit (outbound JSON) later | `throne` |

## Defaults synced from upstream

| Setting | Upstream default (recent) | Rust default |
|---------|---------------------------|--------------|
| `remote_dns` | Google DoH (`56d0d9fd`) | `https://dns.google/dns-query` |
| `vpn_strict_route` | off by default (`5a7f2c96`), Windows CI may enable | `false` |
| `test_latency_url` | thronged URL in settings | `https://www.gstatic.com/generate_204` |
| `inbound_socks_port` | typical 2080 | `2080` |

## Implementation waves

1. **Wave A** — tracking doc, share-link import, SQLite load/save, settings skeleton, UI Import ✅  
2. **Wave B (this drop)** — Clash/JSON/SIP008/WG sub, route share import, route_profiles table, UI route cycle ✅  
3. **Wave C** — Start/Stop/URL/IP/Speed(simple)/Stats/Connections ✅; active route + MetaCubeX `srslist` + jsDelivr mirror + adblock ✅; full BuildSingBoxConfig DNS/xray parity still open  
4. **Wave D** — TUN privilege helper, tray, stats charts / `throne_stats.db`  
5. **Wave B+** — HTTP sub ✅; remote route fetch ✅; full Clash option parity / OS deeplink registration  
6. **Wave 1.2.3** — version pin · Auto Selector create/plan/Start (native outbound) · multi-file import · Program menu · core sources + RPC · **stats dialog** ✅

## Commit triage notes (1.2.2 → 1.2.3)

24 commits on the release line (see GitHub compare `1.2.2...1.2.3`). Themes:

- **Auto Selector** — new profile type, plan/ranking, core group outbound, monitor dialog, tray/start integration  
- **Core** — sing-box/deps bump, xray gate, dispatch cleanup, memory panic fix under load  
- **UX** — multi-file import, Program → New Profile, tray click, menus, sub diff  
- **Platform** — Windows installer / loopback, Unix crash re-release, exclude private ranges  
- **Hardening** — clash YAML parse, quirc hang, crash/db handling  

Later **1.2.4** (not yet version-pinned here): Xray DNS, Auto Selector bugfixes, macOS/Linux Tun DNS — re-audit after 1.2.3 gaps close.

## Commit triage notes (recent upstream themes pre-1.2.3)

- **Core upgrades**: sing-box / xray / amnezia bumps dominate release cadence → pin Go module when packaging core binary.  
- **Xray depth**: interface bind, geo assets, full-config test, UDP → core-client must expose `need_xray` + asset paths.  
- **Subscription UX**: ✅ group cards and metadata, manual diff popup, serialized update-all, identity-preserving apply, and explicit local-proxy routing. Remaining gaps: scheduled auto-update, `sub_clear`, HWID/custom device headers, and custom-config subscription parity.
- **Routing**: remote route profiles via deeplink, raw routes, warp-bypass outbound.  
- **Platform**: Linux TUN on new kernels, installer, Wayland hotkey branch (`upstream/dev-wayland-hotkey`).
