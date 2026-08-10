#!/usr/bin/env bash
# Shared helpers for script/bundle-* (Zed-style packaging).
# shellcheck disable=SC2034

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT"

VERSION="$(tr -d '[:space:]' <"$ROOT/VERSION")"
DIST="${DIST:-$ROOT/dist}"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
PROFILE="${CARGO_PROFILE:-release}"
RELEASE_DIR="$TARGET_DIR/$PROFILE"

ARCH_UNAME="$(uname -m)"
case "$ARCH_UNAME" in
  x86_64|amd64) ARCH=x86_64 ;;
  aarch64|arm64) ARCH=aarch64 ;;
  *) ARCH="$ARCH_UNAME" ;;
esac

OS_UNAME="$(uname -s)"
case "$OS_UNAME" in
  Darwin) HOST=macos ;;
  Linux)  HOST=linux ;;
  MINGW*|MSYS*|CYGWIN*) HOST=windows ;;
  *) HOST=unknown ;;
esac

log()  { printf '==> %s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
die()  { printf 'error: %s\n' "$*" >&2; exit 1; }

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "'$1' not found — $2"
}

# Resolve `go` even when it is installed but not on the non-interactive PATH
# (Homebrew keg, official tarball under /usr/local/go, etc.).
find_go() {
  if [[ -n "${GO_BIN:-}" && -x "${GO_BIN}" ]]; then
    echo "$GO_BIN"
    return 0
  fi
  if command -v go >/dev/null 2>&1; then
    command -v go
    return 0
  fi
  local candidate
  for candidate in \
    "${HOMEBREW_PREFIX:-/opt/homebrew}/bin/go" \
    /opt/homebrew/bin/go \
    /usr/local/bin/go \
    /usr/local/go/bin/go \
    "${HOMEBREW_PREFIX:-/opt/homebrew}/opt/go/bin/go" \
    /opt/homebrew/opt/go/bin/go \
    /usr/local/opt/go/bin/go \
    "${HOME}/sdk/go/bin/go" \
    "${HOME}/.local/go/bin/go" \
    "${HOME}/go/bin/go"
  do
    if [[ -x "$candidate" ]]; then
      echo "$candidate"
      return 0
    fi
  done
  return 1
}

# Prefer a prebuilt ThroneCore when Go is unavailable (dev machines often
# already have one from cargo run, THRONE_CORE, or the installed .app).
find_existing_core() {
  local name="ThroneCore"
  if [[ "$HOST" == "windows" ]]; then
    name="ThroneCore.exe"
  fi
  local candidate
  for candidate in \
    "${THRONE_CORE:-}" \
    "$RELEASE_DIR/$name" \
    "$TARGET_DIR/release/$name" \
    "$TARGET_DIR/debug/$name" \
    "$ROOT/target/release/$name" \
    "$ROOT/target/debug/$name" \
    "/Applications/Throne.app/Contents/MacOS/ThroneCore" \
    "${HOME}/Applications/Throne.app/Contents/MacOS/ThroneCore"
  do
    if [[ -n "$candidate" && -f "$candidate" && -x "$candidate" ]]; then
      # Don't return the destination we are about to overwrite as the only source
      # when it is empty/stale — still OK if it already exists and is executable.
      echo "$candidate"
      return 0
    fi
  done
  return 1
}

ensure_core_binary() {
  local core_out="$1"
  mkdir -p "$(dirname "$core_out")"

  local go_bin=""
  if go_bin="$(find_go)"; then
    # Keep in sync with script/build-core / upstream script/build_go.sh.
    local tags="${THRONE_CORE_TAGS:-}"
    if [[ -z "$tags" ]]; then
      tags="with_clash_api,with_gvisor,with_quic,with_wireguard,with_utls,with_dhcp,with_tailscale,badlinkname,tfogo_checklinkname0"
      case "$(uname -s 2>/dev/null || echo unknown)" in
        Darwin|Linux|darwin|linux) tags+=",with_naive_outbound" ;;
      esac
    fi
    log "Building Go ThroneCore with $go_bin → $core_out (tags=$tags)"
    (
      cd "$ROOT/core/server"
      local ver
      ver="$("$go_bin" list -m -f '{{.Version}}' github.com/sagernet/sing-box 2>/dev/null || true)"
      local ldflags="-w -s -X 'internal/godebug.defaultGODEBUG=multipathtcp=0' -checklinkname=0"
      if [[ -n "$ver" ]]; then
        ldflags="-w -s -X 'github.com/sagernet/sing-box/constant.Version=${ver}' -X 'internal/godebug.defaultGODEBUG=multipathtcp=0' -checklinkname=0"
      fi
      if [[ "$(uname -s 2>/dev/null || true)" == "Darwin" ]]; then
        export CGO_ENABLED=1
        export CGO_LDFLAGS="-weak_framework UniformTypeIdentifiers"
      fi
      "$go_bin" build -trimpath -tags "$tags" -ldflags "$ldflags" -o "$core_out" .
    )
    return 0
  fi

  local existing=""
  if existing="$(find_existing_core)"; then
    if [[ "$(cd "$(dirname "$existing")" && pwd)/$(basename "$existing")" == \
          "$(cd "$(dirname "$core_out")" && pwd)/$(basename "$core_out")" ]]; then
      log "Reusing existing ThroneCore at $core_out (go not installed)"
      return 0
    fi
    log "go not on PATH — copying prebuilt ThroneCore"
    log "  from: $existing"
    log "  to:   $core_out"
    warn "Install Go for a from-source core build: brew install go   (or https://go.dev/dl/)"
    cp "$existing" "$core_out"
    chmod +x "$core_out" 2>/dev/null || true
    # Preserve setuid bit if present on the source (Tun privilege on macOS/Linux).
    if [[ -u "$existing" ]]; then
      chmod u+s "$core_out" 2>/dev/null || true
    fi
    return 0
  fi

  die "ThroneCore unavailable: 'go' not found and no prebuilt core located.

Install Go, then re-run:
  brew install go
  # or: https://go.dev/dl/

Or point at an existing core:
  export THRONE_CORE=/path/to/ThroneCore

Searched: THRONE_CORE, target/{release,debug}/ThroneCore, /Applications/Throne.app/..."
}

# Build GUI (Throne) + Go core (ThroneCore) into $RELEASE_DIR, side-by-side.
prepare_binaries() {
  mkdir -p "$RELEASE_DIR" "$DIST"

  need_cmd cargo "install Rust toolchain"

  local core_out="$RELEASE_DIR/ThroneCore"
  local gui_out="$RELEASE_DIR/Throne"
  if [[ "$HOST" == "windows" ]]; then
    core_out="$RELEASE_DIR/ThroneCore.exe"
    gui_out="$RELEASE_DIR/Throne.exe"
  fi

  # When cross-compiling the GUI, place core next to that target triple too.
  if [[ -n "${TARGET_TRIPLE:-}" ]]; then
    RELEASE_DIR="$TARGET_DIR/$TARGET_TRIPLE/$PROFILE"
    mkdir -p "$RELEASE_DIR"
    core_out="$RELEASE_DIR/ThroneCore"
    gui_out="$RELEASE_DIR/Throne"
    if [[ "$HOST" == "windows" || "${TARGET_TRIPLE}" == *-windows-* ]]; then
      core_out="$RELEASE_DIR/ThroneCore.exe"
      gui_out="$RELEASE_DIR/Throne.exe"
    fi
  fi

  ensure_core_binary "$core_out"

  log "Building Rust GUI Throne [$PROFILE]"
  # Avoid empty-array expansion under macOS bash 3.2 + `set -u`.
  if [[ -n "${TARGET_TRIPLE:-}" ]]; then
    cargo build -p throne --profile "$PROFILE" --target "$TARGET_TRIPLE"
  else
    cargo build -p throne --profile "$PROFILE"
  fi

  [[ -f "$gui_out" ]] || die "GUI binary missing: $gui_out"
  [[ -f "$core_out" ]] || die "core binary missing: $core_out"

  # Export for callers
  GUI_BIN="$gui_out"
  CORE_BIN="$core_out"
  export RELEASE_DIR GUI_BIN CORE_BIN

  log "Binaries ready:"
  ls -lh "$GUI_BIN" "$CORE_BIN"
}

# Strip debug symbols when strip(1) exists (keep originals for crash tooling).
maybe_strip() {
  local f="$1"
  if command -v strip >/dev/null 2>&1 && [[ -f "$f" ]]; then
    strip -S "$f" 2>/dev/null || strip "$f" 2>/dev/null || true
  fi
}
