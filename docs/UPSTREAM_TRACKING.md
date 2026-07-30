# Upstream tracking — [throneproj/Throne](https://github.com/throneproj/Throne)

Remote: `upstream` → `https://github.com/throneproj/Throne.git`  
Baseline branch: `upstream/dev`  
Last audited tip: `fb68b742` (*update xray core*) — same as local `dev` tip at fork time.  
**Product version:** `1.2.2` (= `git describe upstream/dev --tags`; tip commit is tagged `1.2.2`. Older `4.x` tags are ancestors, not the current product line.)

## How to refresh

```bash
git fetch upstream --prune
git log --oneline upstream/dev -50
# optional: commits since last audit
git log --oneline <last-audited>..upstream/dev
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
| TUN private-range bypass flag | `3c344f78` | ✅ Tun Settings dialog + config | `throne-domain` |
| Runtime stats UI | `bac76b83` | 🧩 live rates + Connections tab | `throne` |
| Traffic stats aggregation | `ff3d3c10`, `11373611` | 🧩 QueryStats rates (no throne_stats.db yet) | `throne-core-client` |
| URL / speed / IP / country tests | core RPC + menus | 🧩 URL/IP/simple Speedtest; full multi-thread speed later | `throne-core-client` |
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
| Core IPC Start/Stop | `dispatch.go` + Qt `RPC.cpp` framing | 🧩 Start/Stop/Test/QueryStats/QueryConnections; route compile + **srslist (2178) + jsDelivr mirror + optional adblock**; full BuildSingBoxConfig DNS/xray parity still open | `throne-core-client` |
| System proxy | `QvProxyConfigurator` | 🧩 macOS `networksetup` on Start when checkbox on | `throne-core-client` |
| GPUI shell | — | ✅ M0 | `throne` |
| Main window layout / ops | `mainwindow.ui` | 🧩 relative toolbar menus (under each btn), Start/Stop, Tun/DNS/Proxy, group tabs, 5-col table, logs/conn, status + **v1.2.2** | `throne` |
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

## Commit triage notes (recent upstream themes)

- **Core upgrades**: sing-box / xray / amnezia bumps dominate release cadence → pin Go module when packaging core binary.  
- **Xray depth**: interface bind, geo assets, full-config test, UDP → core-client must expose `need_xray` + asset paths.  
- **Subscription UX**: diff popup, custom config sub, group context menu → import result should return added/removed sets.  
- **Routing**: remote route profiles via deeplink, raw routes, warp-bypass outbound.  
- **Platform**: Linux TUN on new kernels, installer, Wayland hotkey branch (`upstream/dev-wayland-hotkey`).
