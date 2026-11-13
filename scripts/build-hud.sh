#!/usr/bin/env bash
# Compiles gs3-hud and installs it to $PREFIX/bin. Use --binary <file> to skip compiling.
# Usage: scripts/build-hud.sh [--binary <prebuilt>] [--prefix <dir>] [--dry-run]
set -euo pipefail
. "$(dirname "$(realpath "${BASH_SOURCE[0]}")")/lib.sh"

BINARY=""
while [ $# -gt 0 ]; do
  case "$1" in
    --binary) BINARY="$2"; shift 2 ;;
    *) parse_common_flag "$@" && rc=0 || rc=$?
       [ "$rc" = 0 ] && die "unknown option: $1"; shift "$rc" ;;
  esac
done

step "gs3-hud binary"
if [ -z "$BINARY" ]; then
  have cargo || die "cargo not found. Install Rust (https://rustup.rs) or pass --binary <file>."
  export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
  run cargo build --release --manifest-path "$ROOT/gs3/hud/Cargo.toml"
  BINARY="$CARGO_TARGET_DIR/release/gs3-hud"
fi
[ "$DRY_RUN" = 1 ] || [ -x "$BINARY" ] || die "binary not found or not executable: $BINARY"

run install -d "$PREFIX/bin"
run install -m 0755 "$BINARY" "$PREFIX/bin/gs3-hud"
ok "installed $PREFIX/bin/gs3-hud"
# The rail's own text window (a commit and its diff). Optional: without swiftc the rail keeps working and
# simply shows that detail inside its card.
if have swiftc; then
  run swiftc -O -framework AppKit "$ROOT/gs3/panel/Panel.swift" -o "$CARGO_TARGET_DIR/release/gs3-panel"
  run install -m 0755 "$CARGO_TARGET_DIR/release/gs3-panel" "$PREFIX/bin/gs3-panel"
  ok "installed $PREFIX/bin/gs3-panel"
else
  warn "swiftc not found: the native panel stays unavailable and details show inside the rail"
fi
