# Upstream tracking — [throneproj/Throne](https://github.com/throneproj/Throne)

Remote: `upstream` → `https://github.com/throneproj/Throne.git`  
Baseline branch: `upstream/dev`  
**Target release (this audit):** tag **`1.4.0`** (`99f7ecd1`, 2026-11-05).
**Last audited tip:** `upstream/dev` describe **`1.4.0-2-g1346d9ee`** on 2026-11-06 (2 commits past the pin: `5f92cd98` telegram README link, `1346d9ee` Homebrew postflight; not absorbed).
**Product version:** `1.4.0` (root [`VERSION`](../VERSION) + workspace Cargo version = this pin).

This pin identifies the audited release baseline; it does not claim complete product parity. The new remote control API, full scanner, Kill Switch, global hotkeys, OS proxy refactor, and historical gaps below remain partial or unported.

Agent skill for ongoing parity work: [`skills/throne-upstream-parity/SKILL.md`](../skills/throne-upstream-parity/SKILL.md).

## How to refresh

```bash
./script/upstream-audit            # current VERSION → newest 1.* tag
./script/upstream-audit 1.4.0 # explicit release tag
```

Manual equivalent:

```bash
git fetch upstream --prune --tags
git log --oneline 1.4.0-beta.4..1.4.0-beta.5
git diff --dirstat=files,0 1.4.0-beta.4 1.4.0-beta.5
```

## 1.4.0 audit (2026-11-06)

**Pinned release:** `99f7ecd1e7175d0b7073333b3f4e9fc2137b918e` (2026-11-05), **7 commits** after beta.6 (`4b1904e3`). `upstream/dev` is `1.4.0-2-g1346d9ee`; `5f92cd98` (telegram README link) and `1346d9ee` (Homebrew `postflight_steps`) are excluded. This audit does not claim complete product parity.

| Commit | Upstream change | Disposition in this rewrite |
|--------|-----------------|-----------------------------|
| `b8074099` | Connections tree `doItemsLayout` after a rare crash | N/A Qt widget layout |
| `46e7dff3` | Native title bar follows the Qt color scheme | N/A Qt `QStyleHints` |
| `7ce0953f` | Drop noisy import warnings | ✅ MasterDNS SOCKS listener credentials stay ignored and no longer produce a note. Dial-field, REALITY padding, AnyTLS fast-open, Hysteria `pinSHA256`, WireGuard `DNS`/`AllowedIPs`, and OpenVPN `bypass-dhcp`/`bypass-dns` warnings were not emitted here |
| `a973efea` | uTLS secondary classical key share | ✅ `replace github.com/metacubex/utls => github.com/throneproj/utls v1.8.8-0.20261105093534-f0ae7792aa1d`. No Rust fingerprint field |
| `39955b7d` | API for connections, WARP, profile config, and route editing | ⏳ Remote control API remains unported. `WarpConfigured` is the same missing-config check, extracted for that API. `NewSimpleRule` and `action_scope` are API helpers |
| `cb89a14f` | Improve MASQUE | ✅ sing-box pin `6bc5c846150e`. The commit changes `protocol/warpmasque` and `transport/warpmasque` only. Native WARP generation remains ⏳ |
| `99f7ecd1` | Bump to 1.4.0 | ✅ `VERSION`, workspace version, and README. Upstream changelog is not copied |

Upstream `core/` between beta.6 and 1.4.0 changes only `go.mod` and `go.sum`. The local Darwin `sysdns` rewrite stays. Xray stays `1c6eb7771c4d`. WireGuard stays `e11787583013`. The downloaded sing-box module still requires sing-tun `55c7bb51acc7`. Unpatched `reality_client.go` is still `38a5d553fffd108afca51bde0b0888512d2f4633daa608e03767838b8fdcdbc0`. Unpatched `source_darwin.go` is still `72d4037c73ef1a71d0e56d25cbed3ba7eb79b3785b316eececbae6dc86ad2248`. Both patches applied with zero fuzz. `SystemDNSOverride` and `RestoreLegacy` stay. The remote API, simple interface, web UI, log export, and historical scanner/Kill Switch/hotkey gaps remain explicit.

Executed on 2026-11-06: 301 tests in throne-domain, throne-import, and throne-core-client (15 suites); `cargo check -p throne`. `THRONE_CORE_VERIFY_PATCHES=1 ./script/build-core target/parity-1.4.0` exited 0 (systemconfig and sysdns race tests, 78M audit binary). Linker warned about duplicate `-lobjc` and `-lresolv`. Full workspace tests, strict Clippy, throne-storage, and the 13 actual-core smoke groups were not re-run. Native GUI, privileged DNS/TUN, and cross-platform runtime remain untested.

## 1.4.0-beta.6 audit (2026-11-04)

**Pinned release:** `4b1904e3252e4a99c25a91dbfc15b9c10fe77f1f` (2026-11-04), **15 commits** after beta.5 (`ec2420df`). `upstream/dev` is `1.4.0-beta.6-1-gb8074099`; `b8074099` (fix rare crash) is excluded. This audit does not claim complete product parity.

| Commit | Upstream change | Disposition in this rewrite |
|--------|-----------------|-----------------------------|
| `4c5b1e0a` | Shorter README | N/A. This repo keeps the rewrite README |
| `a76f33cf` | Remove custom fund address | N/A. No `.github/FUNDING.yml` here |
| `9f75d82b` | Fix WireGuard stall | ✅ Inside the sing-box pin. No Rust DF/GSO flag |
| `f14b86b3` | Reword macOS local DNS | ✅ Core temporary service + `0.0.0.0/8` route. Rust Start emits `local` and no `dns-system` inbound. GUI no longer blanks networksetup DNS |
| `8327f362` | DNS tunnels | ✅ `dnstt` and `masterdns` persist, import, and build. Scanner DNS UI, xdns form, TOML links, and resolver IP lists stay partial |
| `77aa05b1` | Fix CI | N/A |
| `87585b70` | Qt themes | N/A |
| `cc83f348` | Unify build env | N/A |
| `32167664` | Update sing-box | ✅ Pin `f25a530dc468`, sing-tun `55c7bb51acc7`. Xray `1c6eb7771c4d` and WireGuard `e11787583013` unchanged |
| `b71819c0` | manylinux ninja | N/A |
| `d5efd9ca` | manylinux libxkbcommon | N/A |
| `dae587ad` | arm64 deb smoke | N/A |
| `60aae49f` | Included DNS test lists | 🧩 No Rust loader for `res/scanner/iran-resolvers.txt` or `russia-resolvers.txt` |
| `f048c9cc` | Route editor freeze (#2064) | N/A Qt widget |
| `4b1904e3` | Log export | ⏳ No native log-export surface |

macOS Tun DNS is no longer a `127.0.0.53` direct inbound. The core publishes `State:/Network/Service/Throne/` after the box is up and routes `0.0.0.0/8` into the utun that holds the Tun address. Rust keeps the Tun-prefix `route_exclude` hole because that DNS server is the next address inside the prefix. `SystemDNSOverride` and `sing-box-darwin-system-dns.patch` stay, so a leftover beta.4/beta.5 alias is not used as an upstream. `RestoreLegacy` undoes a dead core's networksetup state. The obsolete `netmon` follow inside `sysdns_darwin.go` is not replayed. A failed Start does not call `networksetup -setdnsservers Empty`.

DNSTT (`dnst://`, unencrypted `slipnet://`) and MasterDNS (`masterdns://`, `stormdns://`, yptun `v1?`, profile JSON) store ExportToJson and build sing-box outbounds with no server. Empty resolvers become `{type:system}` or the string `system`. URL tests add 30s per DNSTT profile and 90s per MasterDNS profile. The editor covers domain, key, mode, resolvers, and the DNSTT backend credentials. Encrypted SlipNet links fail with the upstream message. MasterDNS TOML (`v1/`) returns "this MasterDNS link format is not supported." DNSTT resolver ranges are rejected at config build instead of expanded. Chain inner hops are still not expanded. Subscription identity uses type, mode, domain, key, and backend, not the resolver list.

`ScanDnsOptions` is appended to `libcore.proto`. The hand-written Rust scan encoder omits it, so old probes stay wire-compatible. The scanner UI does not expose DNS probes. Xray `xdns` stream settings still ride the full-config JSON bridge when the document already contains them. There is no xdns form.

Both patches apply to `f25a530dc468` with zero fuzz. Unpatched `reality_client.go` is still `38a5d553fffd108afca51bde0b0888512d2f4633daa608e03767838b8fdcdbc0`. Unpatched `source_darwin.go` is still `72d4037c73ef1a71d0e56d25cbed3ba7eb79b3785b316eececbae6dc86ad2248`. `replaceManagedDNS` is unchanged.

Executed on 2026-11-04: 298 tests in throne-domain, throne-import, and throne-core-client; `cargo check -p throne`. `go test` of `sysdns`, `rpc`, `probe`, and `scan` passed (69 tests) after regenerating `libcore.pb.go`, which stays gitignored. `THRONE_CORE_VERIFY_PATCHES=1 ./script/build-core target/parity-beta6` exited 0 (systemconfig and sysdns race tests, 78M audit binary). Linker warned about duplicate `-lobjc` and `-lresolv`. Full workspace tests, strict Clippy, throne-storage, and the 13 actual-core smoke groups were not re-run. Native GUI, privileged DNS/TUN, and cross-platform runtime remain untested.

## 1.4.0-beta.5 audit (2026-11-03)

**Pinned release:** `ec2420df924fcb80793b29704156d2934b856567` (2026-11-02), **40 commits** after beta.4 (`dd748826`). `upstream/dev` is `1.4.0-beta.5-2-ga76f33cf`; `a76f33cf` (remove custom fund address) and `4c5b1e0a` (shorter README) are excluded. This audit does not claim complete product parity.

| Commit | Upstream change | Disposition in this rewrite |
|--------|-----------------|-----------------------------|
| `83438b6d` | macOS minimum version | N/A Qt/CMake deployment target |
| `86c9f825` | Donation info | ⏳ About/donation UI not ported |
| `993ba8ee` | About page; check-for-update moved | ⏳ |
| `e65de361` | Changelog method | ⏳ |
| `43eb0b69` | Command palette with other windows (#2031) | N/A single native window |
| `8e201ea6` | Proxy table sort indicator | ⏳ |
| `97099f72` | Extend Xray mux | 🧩 Core dependency synced. Rust multiplex remains sing-box presets, not the Xray mux form |
| `38f7e830` | Simple interface mode | ⏳ |
| `68be8768` | Web UI | ⏳ |
| `e30fc1a6` | Menu changes | ⏳ Qt menus |
| `dc4c7d11` | API reference bump | N/A C++ reference |
| `3cf5d622` | Simple interface follow-up | ⏳ |
| `01567519` | Update sing-box | ✅ Pin `af51a7460ccd`; `ConnectionManager` takes `ctx` |
| `cd41d2f6` | Android utils | N/A desktop. `core/mobile` sources are in the synced tree |
| `1f30af24` | Android log and CA | N/A desktop. Mobile sources synced |
| `a0e22d53` | Skip identical tray text | ⏳ |
| `e1251769` | Xray xhttp leak | ✅ Xray pin `1c6eb7771c4d` |
| `c852fe31` | macOS DNS `underlying` (#2045) | ✅ Darwin Tun and URL tests use `underlying`. Mixed mode and ResolveDomains stay `local`. ECH accepts `underlying` as a direct bootstrap. This is not the local sysdns patch |
| `8b6445b6` | Do not set DF on WireGuard | ✅ `wireguard-go` pin `e11787583013` only. No Rust DF flag |
| `bf44c995` | Qt cold-start freeze (#2036) | N/A Qt popup measurement |
| `2c68c283` | Subscription remaining-time precision | ⏳ Native card still shows a fixed expiry |
| `911deaf4` | WARP Amnezia camouflage | ⏳ No WARP generator. `jc`/`jmin`/`jmax` and `GenerateWarpCamouflage` are not ported. Proto unchanged |
| `605641a4` | Tun link replaced by core restart | ⏳ Qt page control |
| `51274c94` | Relax import checks; Hysteria v1 defaults | 🧩 Clash bandwidth units and v1 `up_mbps`/`down_mbps` defaults 10/50. Other protocol import relaxations not ported. Hy2 `user:pass` concatenation not re-ported |
| `01b96d49` | Auto Selector name-filter tooltip | ⏳ |
| `1af312fb` | Exclude countries in Auto Selector | 🧩 Include/exclude helpers skip unknown countries. Native Start does not call them |
| `6891043a` | Start Auto Selector once enough profiles work | 🧩 `start_quorum` / `member_works` / `selector_build_changed` / `unmeasured_candidates` are tested helpers. Native Start does not wait for a quorum |
| `617f8baa` | Serve DNS to other devices | 🧩 `core_dns_in_lan` and `core_dns_in_port` (default 5533) emit direct `dns-in` plus a reject rule on the real Start config. URL tests and ResolveDomains do not. No settings control |
| `f6d42fa7` | Reconnect after subscription replaces the running profile | ✅ Exact same-type, same-name successor only. No successor stops the core and leaves it stopped. A route change restarts that group's running profile. A stopped core is not started |
| `8e1644ee` | Data view two lines and tooltip | ⏳ |
| `3c7cab39` | Per-group routing and subscription route import | 🧩 `route_id`, `import_sub_route`, and `source_gid` persist. Start and domain resolve use the group's route without changing the global active route. Subscription apply preserves type-13 endpoint rules and disabled-by-name. Happ `geoip:private` survives empty-rule filtering. No group-route editor. Chain inner hops are not expanded |
| `c99415c4` | API reference bump | N/A C++ reference |
| `0aefa6cf` | Reset Xray on network changes | ✅ Go Xray gate and mobile sources synced. Rust does not own the gate |
| `851d9386` | Forward Xray logs to the mobile sink | ✅ `core/mobile/log.go` synced. Desktop has no mobile sink |
| `36b6bc4a` | WireGuard GSO | ✅ Dependency pin only |
| `4512278c` | Deeplink stuck on start args | N/A Qt restart argv. The Rust deeplink parser is unchanged |
| `32ee312f` | Update Xray | ✅ Core pin synced. Qt Xray stream form not ported |
| `591bb264` | Allow empty Hysteria 2 obfs | ✅ Obfs is omitted when type is empty/`none`/`plain` or the password is empty. Gecko sizes pass through. Unknown types fall back to salamander |
| `ecd1f1d9` | Changelogs for beta.5 | ⏳ Not a product surface in this rewrite |
| `ec2420df` | Lint | N/A Qt lint. This commit is the tag |

### Core baseline and verification boundaries

Active `core/` matches beta.5 except retained local files: `core/internal/sysdns/*`, `core/internal/boxmain/context.go`, and `core/patches/`. Upstream did not touch those Darwin sysdns sources. Dependency pins: sing-box `af51a7460ccd`, Xray `1c6eb7771c4d`, WireGuard `e11787583013`. `libcore.proto` is unchanged. `ResolveDomains` still answers through `dns-direct`; the Go implementation moved to `core/internal/resolve`. Legacy untracked `core/server/` is not a build input.

`reality_client.go` changed only in fallback HTTP host formatting (IPv6 brackets). The ML-KEM patch still applies with zero fuzz. Upstream moved Darwin dnsinfo into `sing-tun/dnsinfo`, so `sing-box-darwin-system-dns.patch` was rebased onto the new `Configuration()` path. `replaceManagedDNS` is the same filter. The unpatched source SHA256 is `72d4037c73ef1a71d0e56d25cbed3ba7eb79b3785b316eececbae6dc86ad2248`.

The host audit binary is `target/parity-beta5/ThroneCore`. Packaging must rebuild through `script/build-core`. Schema acceptance does not prove privileged macOS DNS restoration, TUN/L3/Kill Switch protection, or Windows/Linux runtime.

### Verification evidence

- **Executed:** 312 tests across throne-domain, throne-import, throne-core-client, and throne-storage (19 suites), plus `cargo check -p throne` and `cargo fmt` of those five crates. Contracts covered include Darwin `underlying` DNS, ECH bootstrap of `underlying`, Happ private-IP rules, `dns-in` listen/reject, URL-test inbounds staying empty, and Hysteria v1 defaults / obfs-active.
- **Not re-run:** the previous 387-test workspace bar, strict production Clippy, and the 13 actual-core smoke groups. All-target lint remains blocked by pre-existing test-only lints. Native GUI, privileged DNS/TUN, and cross-platform runtime remain untested.
- **Executed:** `THRONE_CORE_VERIFY_PATCHES=1 ./script/build-core target/parity-beta5` exited 0. The patched sing-box `systemconfig` race test and the local `sysdns` race test passed. `boxmain` has no test files. The audit binary is `target/parity-beta5/ThroneCore` (77M). That build does not refresh an installed or setuid copy, and it does not repeat the 13 configuration/RPC smoke groups.

## 1.4.0-beta.4 audit (2026-10-27)

**Pinned release:** `dd7488268d560b6924de9d787098a9c9c0e5082f` (2026-10-22), **32 commits** after beta.3 (`01086027`). `upstream/dev` is `1.4.0-beta.4-15-g1f30af24`; those 15 subsequent commits are excluded. This audit closes the contracts below and explicitly retains product gaps.

| Commit | Upstream change | Disposition in this rewrite |
|--------|-----------------|-----------------------------|
| `e7362114` | README refresh | ✅ Native README/version and parity scope refreshed |
| `ebe68079` | Release download table | ⏳ Native release automation/download table not ported |
| `464db7fb` | False-positive ERROR highlighting | N/A to current literal log view; full severity highlighting remains a documented gap |
| `2169bae5` | Dock reopen | ✅ Existing GPUI reopen handler restores the retained main session; source and tests inspected, live Dock behavior not re-exercised |
| `c164ad0c` | Large route-editor freezes | N/A Qt widget/model implementation; native editor uses separate drafts/inputs. Large-subscription performance is not benchmarked |
| `cb26b89e` | Russian translation | ⏳ Native i18n remains unported |
| `de65b898` | Subscription fetch with mixed inbound disabled | ✅ Persisted setting/native control, no mixed listener emitted; proxy fetch uses active TUN or reports an unavailable proxy. Conflicting System Proxy activation fails before core/OS changes |
| `23b5bfcb` | Disable rules and route-editor shortcut | 🧩 Disabled state round-trips DB/share/editor, survives Basic saves and remote refresh, and is excluded from routes/DNS/rule-set downloads. Routing menu right-click shortcut remains |
| `bd2a713d` | Connections column sizing | 🧩 Existing native fixed/flexible column layout retained; upstream persistent sizing parity not claimed |
| `67261f7d` | Per-route DNS | ✅ Own/Global DNS for structured/raw profiles, native editor, compatible `dns_json`/`custom_dns`, validation/share/remote-refresh semantics and effective Start/probe config |
| `464265bf` | Homebrew packaging | ⏳ Upstream Qt formula cannot ship the Rust GUI unchanged; native formula/release artifacts remain |
| `4a90f3d4` | WebDAV backup/sync | ⏳ Remote backup, credentials, conflict/restore workflow and native integration remain unported |
| `3ecda025` | Homebrew docs | ⏳ No unsupported native Homebrew install claim added |
| `e5734625` | Qt 6.12 | N/A native GPUI GUI |
| `d1a7521a` | Go 1.27.2 CI | ✅ Target core metadata synced and host rebuild uses Go 1.27.2; upstream CI execution not claimed |
| `73cc65a1` | quirc dependency | N/A Qt QR implementation; native QR import remains a separate gap |
| `8afd25ff` | fkYAML dependency | N/A C++ parser; Rust YAML importer retained with its documented partial format coverage |
| `8caa1cbc` | simple-protobuf 1.0.1 | ✅ Target proto synced; Rust length-prefixed protobuf fields/methods tested. C++ dependency itself is N/A |
| `56bc0d36` | MSVC x86 protobuf fix | N/A C++ library; Windows runtime/build not exercised on this host |
| `72c70df4` | Chain errors, proxy-user parsing, subscription IDs | 🧩 Native plain HTTP/SOCKS names already avoid accidental base64 reinterpretation; full encoded-credential compatibility, descriptive missing-hop and VPN override behavior remain |
| `a2180b7b` | Route connection to a specific server | ⏳ Native context menu still supports Proxy/Direct/Block; referenced-profile materialization and server-selection workflow remain |
| `b83d367e` | Show matched connection rule | ✅ Core metadata, additive fields 15/16, Rust decoding and native outbound tooltip with rule/item; old-core empty metadata remains compatible |
| `ba9ff85f` | Ctrl+K command palette | 🧩 Native Ctrl/⌘K search, case-insensitive tokens, ranked results, Enter/Escape and existing action/settings navigation; exhaustive per-setting field index remains |
| `4a469df5` | Multiplex presets/customization | 🧩 Persisted global preset and Core Options, On/Off/Unspecified/custom JSON for supported native leaves and URL tests. Specialized per-node mux form and share export remain |
| `580c6c59` | TUN leftovers policy | 🧩 Core lifecycle synced; native interactive leftover/retry policy and privileged cross-platform execution remain |
| `d07ea291` | Subscription expiry countdown | N/A five-minute countdown threshold: native usage card displays a fixed expiry timestamp |
| `0ca06f23` | Native file pickers | ✅ Existing native OS file selection retained; live multi-platform picker exercise not repeated |
| `9be37cf0` | macOS DNS loopback lifecycle | 🧩 Target core DNS snapshot/alias/restoration and physical-interface hardening; Rust dns-system inbound/leading hijack rule. Local-resolver loop fix and runtime limits below |
| `b646dc66` | macOS DNS failure handling | 🧩 Target rollback/recovery synced; unit/build evidence only, no privileged DNS changes performed |
| `0a9479d0` | Xray update and XDrive | 🧩 Core dependency synced; XDrive Xray JSON stays lossless through full-config bridge; unsupported XDrive URI rejected. Opt-in system CA preserves explicit false/REALITY and XHTTP extra precedence. Dedicated XDrive wizard/validator/export remains |
| `f491e877` | Resolve domains through core | ✅ Typed ordered per-domain RPC, active/fallback dns-direct config, selected/group native actions preserving transport identity and rejecting stale edits; no OS resolver fallback |
| `dd748826` | Rename search settings | 🧩 Native Command Palette naming and Settings entry; complete upstream field catalog remains |

### Core baseline and verification boundaries

Active `core/` is based on beta.4, with local Darwin physical-interface selection and saved-resolver hardening retained. Dependency pins: sing-box `a29d468286bc`, Xray `ace47b20f7fe`, WireGuard `de3e4a28dbfe`, gRPC `v1.84.0`. The build applies source-hash-guarded patches to a private sing-box copy, preserving the pre-existing REALITY ML-KEM fix. Legacy untracked `core/server/` is not a build input.

Darwin local DNS resolves through the saved per-service static servers or a confirmed DHCP lease. Unreadable/corrupt/stale ownership data causes the local transport to exclude its own alias without guessing DNS settings; independent system servers remain usable, and an alias-only probe fails resolution instead of recursing. No state-file permissions are loosened. See `core/patches/README.md` for the source guards and limits.

The host audit binary is `target/parity-beta4/ThroneCore`. Packaging must rebuild through `script/build-core`; installed/setuid copies require a separate runtime refresh. Schema acceptance does not prove privileged macOS DNS restoration, TUN/L3/Kill Switch protection, remote XDrive/WebDAV handshakes or Windows/Linux runtime. No production network state is changed by this audit.

### Verification evidence

- **Executed:** 387 Rust workspace tests across 20 suites, desktop compilation, strict production Clippy (`cargo clippy --offline --workspace --lib --bins -- -D warnings`), formatting and diff checks. The final test run used the standard compiler with both Rust wrappers disabled. Tests cover route DNS/rule migration/share/update, compatible RPC framing, resolver contracts, mux/trust behavior, native draft/search behavior and persistence.
- **Executed:** final rebuilt-core configuration and RPC smoke passed 13 groups, including deterministic hosts-backed ResolveDomains ordering/errors/duplicates, beta.4 DNS/mux settings and custom raw-DNS ECH bootstrap with proxy-default DNS. These checks never start TUN or change the OS proxy.
- **Executed:** all nine targeted Go packages and RPC/Xray/Xray-DNS race checks passed on the release sync. The final shipping-path rebuild with `THRONE_CORE_VERIFY_PATCHES=1` passed the patched Darwin systemconfig and sysdns race tests, including unreadable ownership records, unknown saved DNS, static/DHCP separation, alias filtering and invalid interface indices. The patched core was then used for the 13 smoke groups above.
- **Executed / inspected:** independent ECH review passed 18 additional generated-configuration cases covering safe bootstrap, proxy dependency, resolver cycles, object-form dependencies and tag selection; no new blocking findings. This does not establish a live ECH handshake.
- **Executed limitation:** strict all-target Clippy still encounters pre-existing test-only `field_reassign_with_default` lints in domain/import. The production-target strict command above passes.
- **Not executed:** live native desktop operation, privileged DNS alias/setup/restoration or service-switch recovery, remote handshakes, Windows/Linux runtime. WebDAV, specific-server routing, full setting search and historical product gaps above remain open.

## 1.4.0-beta.3 audit (2026-10-18)

**Pinned release:** `010860272538fd0955f40dd87e4b200d6fee2adc` (2026-10-17), **16 commits** after beta.2 (`9172ec37`). The fetched `upstream/dev` is `1.4.0-beta.3-6-gcb26b89e`; those six subsequent commits are excluded. This is an audited baseline, not a claim of complete upstream parity.

| Commit | Upstream change | Disposition in this rewrite |
|--------|-----------------|-----------------------------|
| `2bfc5885` | x/net update | ✅ Target `go.mod`/`go.sum` synced |
| `053888ea` | Linux FakeIP CI fixes | 🧩 Core synced; upstream Linux environment/CI execution not reproduced on this Darwin host |
| `5cb7fe16` | Automatic Amnezia I1–I5 QUIC signatures | 🧩 Native generator, RPC and Rust client implemented; profile-editor generation control not ported |
| `6906912f` | Retain group sorting | ✅ Upstream `sort_method`/`sort_descending` DB columns, per-group header state and stable re-sort after import, subscription, rename, traffic and test updates; raw order stays raw |
| `11c80fd4` | Connection profile names | ✅ Running profile and Auto Selector member tags resolve to captured names in Connections; raw outbound tag retained as fallback |
| `a6d042c2` | TLS fragmentation route options | ✅ Models, compatible DB migration, share import/export, route editor switches/delay and core JSON; `route-options` stays non-terminal without an outbound |
| `39d6b620` | WireGuard startup memory fix | ✅ Target wireguard-go dependency synced |
| `cb823588` | Clash VLESS engine preference | ⏳ Engine preference and general native Xray-VLESS editor remain unported; XHTTP share-link/full-config bridge is separate |
| `a1b2d3e5` | TLS cipher suites and unsafe fingerprint | 🧩 Native share-link suites retained with unsupported suites filtered/reported; ordinary TLS disables unsafe uTLS, REALITY uses required random fallback; XHTTP and Xray JSON cipher suites retained through full-config bridge. Dedicated suite editor/export remains |
| `c33ef779` | Remove forced Xray deprecations | ✅ Target Xray dependency synced |
| `bf89f273` | Config export improvements | ⏳ Full native profile/core export workflow remains; route share round-trip now retains the new fragment and override fields |
| `c3c2778b` | Live graph refactor | 🧩 Existing GPUI traffic graph retained; upstream graph controls, sampling and persistence parity not claimed |
| `a8af8ca8` | Structured input validation | 🧩 Invalid Clash ports cannot wrap; partial Clash feeds produce errors so subscription replacement is refused; native cipher diagnostics added. Full upstream per-field validators and editor diagnostics remain |
| `568ce9d3` | Xray ports / mobile signatures / diagnostics | 🧩 Xray reservation/retry/cooldown core changes and capability-aware privilege decoding synced; mobile/native diagnostics UI not ported |
| `725db1e6` | Headless engine, service and packaging | ⏳ Rust headless orchestration, CLI/service/API surface and packaging not ported; shared core changes synced. This is a separate product surface, not Qt-only N/A |
| `01086027` | Multiple Auto Selector groups / country provider | 🧩 Ordered deduplicated `gids` with legacy `gid` compatibility, missing-group handling, per-group chain eligibility, Start/rerank and native tracked-group editor implemented. Country lookup provider customization and related runtime-stat UI remain |

### Core baseline and limits

`core/` matches beta.3 sources except the retained Darwin physical-NIC hardening through `netmon.DefaultInterface()`. Dependency pins: sing-box `8b3b24f856bd`, Xray `0858933635b5`, wireguard-go `98b1170bc9f0`; protobuf adds `GenQuicSignature` and optional privilege capability fields without changing RPC framing. The pre-existing legacy `core/server/` is not the active build source.

The host binary is rebuilt at `target/parity-beta3/ThroneCore`. Packaging still uses the build script and root `VERSION`. A previously staged setuid core is deliberately not overwritten by the existing privilege setup path: an installed/elevated runtime must be refreshed separately before assuming beta.3 RPC support. No privileged network state was changed for this audit.

### Verification evidence

- **Executed:** host Go core rebuild; targeted Go tests for `capcheck`, `diagnostics`, `guard`, `probe`, `xray`; Xray race test.
- **Executed:** `cargo test --workspace` passed **310 tests across 14 suites** (20 more than the beta.2 audit), including multi-group Start/rerank, route JSON, SQLite reopen, persisted sorting, TLS suites/REALITY, partial subscription rejection and additive protobuf contracts.
- **Executed:** `cargo check -p throne` passed with two existing warnings (`RoutingTab::Hijack`, `validate_dns_rules`); `cargo fmt --all` and `git diff --check` passed.
- **Executed:** **10 rebuilt-core smoke groups** passed: API-service SOCKS config; WebSocket endpoint/SNI/Host with negative control; ECH/Amnezia/XHTTP/FakeIP configs; multi-group selector and route-options fragmentation with conflicting-mode rejection; REALITY unsafe fallback and filtered cipher suites; privilege/QUIC-signature RPCs and invalid-IP rejection; scanner count, loopback TCP/progress, stopped-session rejection; rule-set parsing. No profile was started and no system proxy/TUN mode was changed.
- **Executed failure, inspected provenance:** `cargo clippy --workspace --all-targets -- -D warnings` remains blocked by **18 existing domain lint errors** (manual clamps/default reassignment, manual filter, argument counts and a stray doc comment). The corresponding patterns are present in the pre-task snapshot; this is not a clean strict-lint result.
- **Executed regression proof:** both route Basic/Advanced save-loss tests failed before repair and passed afterward. Fragment options now survive simple-rule rebuilds; clearing a simple bucket still removes it, and unconditional route-options payloads are retained.
- **Inspected:** independent core security and Rust reviews found no remaining high-confidence blockers after REALITY, unsupported-suite and route-save repairs. Source review is not desktop interaction or CI evidence.
- **Not executed:** native desktop interactions, live remote handshakes, Linux/Windows runtime, elevated TUN/L3/Kill Switch paths, full headless product workflows. Existing remote control API, scanner/resume UI, global-hotkey and historical editor gaps remain.

## Feature parity matrix

### Node editor slice (2026-10-27)

The native **Edit Profile** dialog now edits name, server/port, protocol authentication, transport, and TLS/REALITY through General / Transport / TLS / JSON tabs. Supported native outbound shapes have structured fields; complex Xray, Custom, WireGuard and other layouts use the complete outbound JSON editor. This closes the former name-only editing limitation without claiming every upstream protocol-specific form or core validator. This earlier editor slice was built against `1.4.0-beta.3` without core/schema changes; the beta.4 audit above supersedes that release pin.

Changed fields patch the original JSON, retaining unknown settings, ECH, certificate pins, multiplex and custom full documents. Save validates common endpoint/auth and Vision/REALITY/transport contracts, refreshes cached address/auth fields, preserves runtime measurements, rejects a draft if subscription/configuration changed meanwhile, and restores state on persistence failure. Cancel discards the draft. Editing the directly running profile restarts that profile; edits during its Start queue a reload. Other profiles apply on their next Start (automatic reload of indirect chain/selector members is not claimed).

Regression coverage includes port changes, optional-field removal, raw/form round trips, protocol identity protection, custom/WireGuard preservation, transport transitions, REALITY/uTLS, stale drafts and SQLite reopen. Native desktop evidence and final verification totals are recorded after the checks below.

Status legend: ✅ implemented for the stated contract · 🧩 partial · ⏳ planned · ❌ out of scope to rewrite (the Go core is still synced and shipped). Runtime verification limits are listed below.

| Area | Upstream signal (commits / modules) | Rust status | Target crate |
|------|-------------------------------------|-------------|--------------|
| **Route DNS and disabled rules (beta.4)** | `67261f7d`, `23b5bfcb` | ✅ Compatible model/DB/share/native editing, remote-refresh preservation and effective generated config; privileged DNS runtime untested | domain, storage, import, core-client, throne |
| **Core domain resolution / matched connection rules** | `f491e877`, `b83d367e` | ✅ Ordered additive RPC and native selected/group actions; matched rule tooltip; deterministic actual-core smoke | core, core-client, throne |
| **Mux / Xray CA / XDrive** | `4a469df5`, `0a9479d0` | 🧩 Core Options and preset generation, lossless Xray JSON + explicit CA choices; specialized forms and export remain | domain, storage, import, core-client, throne |
| **Command Palette** | `ba9ff85f`, `dd748826` | 🧩 Ctrl/⌘K search and existing actions/settings; full field index remains | throne |
| **WebDAV / specific-server connection routing** | `4a90f3d4`, `a2180b7b` | ⏳ Native workflows not implemented | throne, domain, core-client |
| **Native node editing** | `DialogEditProfile`, protocol editors, beta.3 validators | 🧩 Structured common/auth/transport/TLS/REALITY forms plus lossless complete JSON editing; validation, cancel/save, stale-draft guard, database reopen and direct active-profile reload. Specialized Xray/WireGuard/other protocol forms and full upstream validation remain | `throne-domain`, `throne-storage`, `throne` |
| **Multiple Auto Selector groups (1.4.0-beta.3)** | `01086027` | ✅ Ordered/deduplicated `gids`, legacy `gid`, missing-group tolerance, per-group chain eligibility, DB round-trip, Start/rerank and native group checklist; generated config checked by actual core | `throne-domain`, `throne`, `throne-core-client` |
| **Retained group sorting (1.4.0-beta.3)** | `6906912f` | ✅ DB-compatible mode/direction, per-group header state, stable ties, raw-order preservation and re-sort on data updates/reload; tested | `throne-domain`, `throne-storage`, `throne` |
| **Route TLS fragmentation (1.4.0-beta.3)** | `a6d042c2` | ✅ Fragment/record flags and delay in model, DB, shares and editor; Basic/Advanced round trips preserve options; generated route-options checked by actual core | `throne-domain`, `throne-import`, `throne-storage`, `throne-core-client`, `throne` |
| **Core capability / QUIC signature RPCs** | `5cb7fe16`, `568ce9d3` | 🧩 Core/client implemented and live RPC smoke passed; native generation and capability controls remain | `core`, `throne-core-client` |
| **Headless / country providers / full import validation** | `725db1e6`, `01086027`, `a8af8ca8` | ⏳ Headless service/API/CLI and country-provider UI unported; validation is partial as detailed in beta.3 triage | `throne`, `throne-domain`, `throne-import` |
| **TLS/ECH/XHTTP import (1.4.0-beta.2)** | `8e484f36`, `37ef15ff`, `aa0f9de8` | 🧩 ECH shorthand/legacy options and whole-certificate pins preserved; XHTTP VLESS and Xray JSON ECH use the full-config bridge; strict JSON precedes Python/form fallback. Dedicated Xray editor/type, share export and full-config batch tests remain gaps | `throne-import`, `throne-core-client` |
| **ECH DNS bootstrap** | `37ef15ff` | ✅ native TLS HTTPS queries use direct/custom resolvers before proxy rules; static configs skip bootstrap; GUI resolver metadata stripped; canonical query/rule names match. Xray configs retain their own ECH settings | `throne-core-client` |
| **WireGuard / Amnezia names and options** | `a2fbf55b` | ✅ `awg://`, INI explicit/adjacent-peer names, wrapper fragment/description precedence, container disambiguation; options and display fields survive DB reload, endpoint override preserves keys/port, and legacy `worker_count` becomes core `workers`. INI retains one coherent last peer; full multi-peer INI import remains a gap | `throne-import`, `throne-domain`, `throne-storage`, `throne-core-client` |
| **IPv6 FakeIP** | `03cba7a5` | ✅ `2001:db8::/32`, outside private ULA bypass; respects persisted `fakeip_disable_ipv6`; config validated with actual core, privileged traffic untested | `throne-core-client`, `throne-storage` |
| **Log view** | `b5c47542`, `ad53134b`, `32138a44` | 🧩 monospace font/size/history settings, collapsible case-insensitive literal filter, frozen history while scrolled up and Resume; full history shown. Regex/highlighting and upstream include/exclude filter settings remain | `throne`, `throne-domain`, `throne-storage` |
| **Tray connect/disconnect** | `0616d343` | ✅ connection action, disabled transition labels and template-safe connecting glyph; refreshes from native pump while window hidden; live cross-platform GUI untested | `throne` |
| **Throne remote control API** | `79461307` | ⏳ custom encrypted wire protocol, command router, listener, key/LAN allowlist and Integration settings **not ported**; existing core API tracker service is unrelated. Unknown DB settings remain preserved | `throne` |
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
| Core: sing-box / xray / TUN / DNS | Go `core/` (moved out of `core/server` in 1.3.0-beta.4) | ❌ Rust data-plane rewrite remains out of scope; shipping `ThroneCore` **synced to 1.4.0** (sing-box `6bc5c846150e`, sing-tun `55c7bb51acc7`, Xray `1c6eb7771c4d`, WireGuard `e11787583013`, uTLS `f0ae7792aa1d`) with retained Darwin override and REALITY hardening | `core/` |
| **Per-group routing (1.4.0-beta.5)** | `3c7cab39` | 🧩 `route_id` / `import_sub_route` / `source_gid`; Start and domain resolve use the group route; subscription route import. No editor. Chain inner hops remain | `throne-domain`, `throne-storage`, `throne-import`, `throne` |
| **DNS for other devices (1.4.0-beta.5)** | `617f8baa` | 🧩 direct `dns-in` on Start, loopback unless `core_dns_in_lan`, port default 5533, reject rule. No settings control. URL tests stay without it | `throne-domain`, `throne-core-client` |
| **Hysteria obfs and v1 bandwidth (1.4.0-beta.5)** | `591bb264`, `51274c94` | ✅ empty/none/plain obfs off; v1 defaults 10/50 at config build; Clash unit parser. Hy2 `user:pass` not re-ported | `throne-import`, `throne-core-client` |
| **Subscription successor reconnect (1.4.0-beta.5)** | `f6d42fa7` | ✅ one same-type same-name successor; otherwise stop and stay stopped | `throne-domain`, `throne` |
| **Auto Selector country and quorum (1.4.0-beta.5)** | `1af312fb`, `6891043a` | 🧩 pure helpers only; Start does not wait for a quorum | `throne-domain` |
| **macOS DNS underlying (1.4.0-beta.5)** | `c852fe31` #2045 | Historical for that pin. Beta.6 emits `local` and drops `dns-system`; see the beta.6 row | `throne-core-client` |
| **macOS temporary DNS service (1.4.0-beta.6)** | `f14b86b3` | ✅ Start emits `local` and no `127.0.0.53` inbound. Core publishes the temporary service and the `0.0.0.0/8` route. GUI does not blank networksetup DNS | `core/`, `throne-core-client` |
| **DNS tunnels (1.4.0-beta.6)** | `8327f362` | ✅ `dnstt` / `masterdns` import, editor fields, sing-box build, and URL-test allowance. TOML links, resolver ranges, resolver IP lists, xdns form, and scanner DNS probes remain 🧩 | `throne-domain`, `throne-import`, `throne-core-client` |
| **Log export (1.4.0-beta.6)** | `4b1904e3` | ⏳ Qt Diagnostics tab only. No native export action | `throne` |
| **Import warning quieting (1.4.0)** | `7ce0953f` | ✅ MasterDNS SOCKS listener credentials are ignored with no note. The other dropped warnings were not emitted | `throne-import` |
| **MASQUE and uTLS pins (1.4.0)** | `cb89a14f`, `a973efea` | ✅ sing-box `6bc5c846150e` and uTLS `f0ae7792aa1d`. Native WARP generator remains ⏳ | `core/` |
| **Remote API expansion (1.4.0)** | `39955b7d` | ⏳ Connections, WARP, profile-config, and route-editing endpoints stay unported | `throne` |
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
| **macOS Tun DNS addr** | `9be37cf0`, `b646dc66` | 🧩 beta.4 loopback alias `127.0.0.53`, DNS snapshot/restore and physical-interface selection; saved-resolver hardening and privileged limits above supersede historical TUN-prefix address | `core/` |
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
13. **Wave 1.4.0-beta.2** — ECH/TLS/XHTTP import · WireGuard/Amnezia import and persistence · DNS bootstrap/IPv6 FakeIP · log settings/filter/hold · tray connection states 🧩 (remote control API and documented runtime/product gaps remain).

## Commit triage notes (1.4.0-beta.1 → 1.4.0-beta.2)

All **14 commits** in `git log --reverse 1.4.0-beta.1..1.4.0-beta.2` are accounted for. Tag `9172ec37` is the exact target; the four later development commits are excluded.

| SHA | Upstream change | Classification and action |
|-----|-----------------|---------------------------|
| `ad73538e` | Avoid disconnected Unix RPC calls | **Inspected/N/A:** Rust uses `ensure_connected` before calls; no matching eager Qt call. Remote IP-list updater remains unported |
| `8e484f36` | TLS/XHTTP import consistency | **Must port:** certificate SHA256 pins and literal `+` preserved; XHTTP uses strict JSON before legacy fallback and full Xray bridge; certificate pins feed security classification |
| `a2fbf55b` | WireGuard/Amnezia naming | **Must port:** explicit/adjacent-peer names, `vpn://` fragment and description priority, multi-container suffixes, `awg://`; obfuscation and peer options persist, modern/legacy WireGuard generates endpoints |
| `37ef15ff` | Xray ECH and bootstrap DNS loop | **Must port:** Xray ECH retained in full configs; native ECH HTTPS direct rules/custom resolver and canonical query names; no GUI resolver field emitted into core TLS |
| `b5c47542` | Monospace font, live filter, hold log view | **Must port/partial:** font/history settings, native literal filter, hold/resume and full retained history; highlighting/regex remain |
| `03cba7a5` | IPv6 FakeIP documentation range | **Must port:** generated `2001:db8::/32` plus disable-IPv6 preference and core validation |
| `aa0f9de8` | ECH share-link shorthand | **Must port:** flags, base64 PEM wrapping, query-name/resolver targets and legacy override precedence |
| `0616d343` | Tray connect/disconnect and connecting icon | **Must port:** action/transition state/glyph plus independent hidden-window state pump |
| `79461307` | Throne remote API and Integration dialog | **Must port/gap:** ⏳ new service, encrypted protocol/router and Integration dialog not implemented; no listener or remote-control claim |
| `41a0137a` | Kill Switch Qt lifetime and UAC cancellation | **Qt-specific fix/N/A; product gap:** no equivalent Qt lifetime in Rust; Kill Switch GUI/lifecycle and Windows elevation parity remain unported/unverified |
| `a343636e` | MetaCubeX Go for legacy OS builds | **Packaging/gap:** ⏳ legacy Windows/macOS toolchain builds not ported; modern host build retains existing tags |
| `ad53134b` | Collapsible filter and font labels | **Must port:** Filter toggle and Basic Settings log font controls |
| `32138a44` | Refine log filter behavior | **Partial:** case-insensitive literal matching and hide-to-disable; Qt highlighting/filter restoration details not fully ported |
| `9172ec37` | Android test log isolation | **Core sync:** exact `core/mobile/log.go` and `test.go` delta; desktop rebuilt. Android runtime outside desktop validation |

### Verification status (2026-10-14)

- **Executed:** `cargo test --workspace --locked` — **290 passed across 10 suites**. Initial six import contracts and six later regression cases were observed failing before their respective repairs; the final suite passes.
- **Executed:** `cargo check -p throne --locked` — zero errors, two existing dead-code warnings (`RoutingTab::Hijack`, `RoutingDraft::validate_dns_rules`). Formatting and `git diff --check` also pass.
- **Executed:** host `./script/build-core target/parity-beta2`; exact target core differs only by the existing Darwin `netmon` physical-interface patch.
- **Executed:** Rust smoke harness against rebuilt core — **all seven groups pass**: SOCKS/API tracker config; positive/negative WebSocket; imported ECH, Amnezia endpoint with `workers=2`, XHTTP/Xray and IPv6 FakeIP config checks; loopback scanner count/probe/progress/stop; rule-set parsing. Validation does not establish remote-server handshakes or privileged traffic behavior.
- **Inspected and corrected:** security case normalization, canonical ECH bootstrap name, quoted/form-encoded XHTTP values, coherent WireGuard peer options and DB hydration, core workers key, immediate log trimming, hidden-window tray refresh, separate scroll handles and filter focus. Security and Rust re-reviews found no remaining blocking issue in these repairs; full multi-peer and legacy-hydration fixture coverage remain advisory improvements.
- **Untested:** live native GUI interactions, Windows/Linux desktop and packaging, remote ECH/Amnezia/XHTTP handshake, Android runtime and privileged TUN/L3/Kill Switch.

## Commit triage notes (1.3.2 → 1.4.0-beta.1)

All **23 commits** were accounted for in the previous audit. Source: local `git log --reverse --format='%h %s' 1.3.2..1.4.0-beta.1`. The following section is historical; the beta.2 delta is recorded above.

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

Local patch at that audit used `boxdns.DefaultInterface()`. On the current 1.4.0-beta.2 pin, `core/internal/sysdns/sysdns_darwin.go` retains the physical-NIC selection through `netmon.DefaultInterface()` so system DNS is not pointed at `utun`.

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
