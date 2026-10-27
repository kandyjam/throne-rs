# Pinned core dependency patches

`script/build-core` applies `sing-box-reality-mlkem.patch` and
`sing-box-darwin-system-dns.patch` to a private copy of
`github.com/throneproj/sing-box` at `v1.11.16-0.20261019002151-a29d468286bc`.
It verifies the replacement module version and original source SHA256 before
patching, uses a temporary modfile with `-mod=readonly`, and removes the copy
after the build. Committed lockfiles and the shared Go module cache are untouched.
`script/bundle-mac` invokes this same build path. Re-audit or retire the patch
when updating the dependency pin; a changed source fails the build explicitly.

## REALITY hybrid key share compatibility

The pinned client removed X25519MLKEM768 from ClientHello. Recent REALITY servers
require this share before any optional X25519 share and reject the stripped hello.
The patch preserves the selected fingerprint's shares, removes the redundant
rebuild, and derives the authentication key from Ecdhe, falling back to
MlkemEcdhe only when Ecdhe is absent. This matches the pinned Xray client and
server key-selection order. HKDF, AEAD and certificate verification stay intact.

The pinned uTLS `chrome` maps to Chrome 133 and offers the required hybrid share.
`firefox` (120) and `safari` (16) do not; `random` can select these as well. Choose
`chrome` explicitly for servers that require the hybrid share. This patch does
not silently substitute another fingerprint or disable verification.

Executed on 2026-10-27: with identical live node credentials, the original core
failed REALITY verification for Firefox, Chrome and Safari. The patched core
completed an HTTPS URL test with Chrome (873 ms); Firefox and Safari remained
negative controls. No node credentials are stored in this repository.

Re-audited for Throne 1.4.0-beta.4 on 2026-10-27: the new sing-box pin
`a29d468286bc` retains exactly the same `reality_client.go` SHA256
(`cb4fcbd0f76347282cd31e630271c02bb3c5273205b005f47987804e2adc4742`).
The existing patch applies with zero fuzz. The source-hash guard remains
unchanged; the dependency-version guard now names the beta.4 pin.

## Darwin system DNS loop prevention

Beta.4 directs system DNS to Throne's `127.0.0.53` loopback listener. The pinned
sing-box local DNS source excludes its own TUN addresses, but does not recognize
that loopback address as its own. A local DNS transport can consequently query
Throne again instead of the original upstream resolver.

The patch discovers a structural DNS override service registered by `boxmain`.
The service maps the active physical interface to the saved per-service DNS
snapshot. Known static servers replace the alias in their original order; DHCP
is consulted only when the snapshot confirms that no static DNS was configured.
The alias, including IPv4-mapped representations, is never retained as an
upstream. Independent system resolvers keep their order.

A separate unprivileged probe may not be able to read the owner's root-only
record. Inaccessible, corrupt, stale or incomplete ownership information is
explicitly unknown: the resolver removes Throne's alias without guessing static
servers or falling back to DHCP. It fails resolution if no independent server
remains. Missing or invalid interface indices use the same conservative
exclusion. A genuinely absent record with no local owner leaves unmanaged DNS
unchanged. Record permissions are not relaxed.

The build requires the original `source_darwin.go` SHA256
`9a272357e282eb48d9d1e80a589fe0729c422c7d96cb792d3c2049a30704e104`
and applies the patch with zero fuzz. Set `THRONE_CORE_VERIFY_PATCHES=1` when
running `script/build-core` to run the patched systemconfig and core DNS tests
with the race detector before compiling the shipping binary.

Executed on 2026-10-27: the final shipping-path build with
`THRONE_CORE_VERIFY_PATCHES=1` passed the patched systemconfig and sysdns race
tests, including unreadable records and unknown-state filtering. The resulting
core passed all 13 configuration/RPC smoke groups.

These deterministic tests do not mutate system DNS. Privileged alias setup,
restoration, network-service failover and resolver notification timing still
require separate runtime validation. Installed/setuid binaries are not refreshed
by building the audit binary under `target/parity-beta4/`.
