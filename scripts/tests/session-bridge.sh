#!/usr/bin/env bash
# Public launcher and copied-installation checks; no real herdr or Pi is started.
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
mock="$(mktemp -d "$repo/target/gs3-session-bridge-XXXXXX")"
trap 'rm -rf "$mock"' EXIT
mkdir -p "$mock/bin" "$mock/home"
printf '#!/bin/sh\nprintf "%%s\\n" "$@"\n' > "$mock/bin/agent"
printf '#!/bin/sh\nprintf "Linux\\n"\n' > "$mock/bin/uname"
printf '#!/bin/sh\nprintf "called\\n" >> "$CALLS"\ncase "$1 $2" in "pane split") printf '\''{"result":{"pane":{"pane_id":"hud"}}}\\n'\'';; esac\n' > "$mock/bin/herdr"
chmod +x "$mock/bin/"*
export PATH="$mock/bin:/usr/bin:/bin" HOME="$mock/home" CALLS="$mock/calls"
export GS3_SHELL="$mock/bin/agent" GS3_HERDR="$mock/bin/herdr" GS3_HUD=/usr/bin/true
export HERDR_ENV=1 HERDR_PANE_ID=main HERDR_SOCKET_PATH="$mock/herdr.sock"
check() {
  local launcher="$1" extension="$2" actual expected
  actual="$(/bin/bash "$launcher" -- 'two words' '-prompt' 2>"$mock/err")"
  expected="$(printf '%s\n' --extension "$extension" --gs3-herdr-pane main --gs3-herdr-socket "$HERDR_SOCKET_PATH" -- 'two words' '-prompt')"
  [ "$actual" = "$expected" ] && [ ! -s "$mock/err" ] || { printf 'incorrect bridge argv: %s\n' "$actual" >&2; exit 1; }
}
check "$repo/gs3/bin/gs3" "$repo/extensions/herdr-session/index.ts"
# Reserved pre-delimiter flags cannot redirect the binding or create panes.
before="$(wc -l < "$CALLS")"
if /bin/bash "$repo/gs3/bin/gs3" --gs3-herdr-pane other >"$mock/out" 2>"$mock/err"; then exit 1; else [ "$?" = 1 ]; fi
[ ! -s "$mock/out" ] && [ "$(cat "$mock/err")" = 'gs3: --gs3-herdr-pane is reserved for the gs3 session bridge' ]
[ "$(wc -l < "$CALLS")" = "$before" ]
if env -u HERDR_ENV /bin/bash "$repo/gs3/bin/gs3" --gs3-herdr-socket=other >"$mock/out" 2>"$mock/err"; then exit 1; else [ "$?" = 1 ]; fi
[ ! -s "$mock/out" ] && [ "$(cat "$mock/err")" = 'gs3: --gs3-herdr-socket is reserved for the gs3 session bridge' ]
[ "$(wc -l < "$CALLS")" = "$before" ]
# Installation must carry the fixed extension; no checkout-relative lookup.
bash "$repo/scripts/install-launcher.sh" --prefix "$mock/copied prefix" --config-dir "$mock/config" >"$mock/install-out" 2>"$mock/install-err"
[ ! -s "$mock/install-err" ]
check "$mock/copied prefix/bin/gs3" "$mock/copied prefix/share/gs3/herdr-session/index.ts"
cmp "$repo/extensions/herdr-session/core.mjs" "$mock/copied prefix/share/gs3/herdr-session/core.mjs"
bash "$repo/uninstall.sh" --prefix "$mock/copied prefix" --agent-dir "$mock/agent" --config-dir "$mock/config" >"$mock/uninstall-out" 2>"$mock/uninstall-err"
[ ! -s "$mock/uninstall-err" ] && [ ! -e "$mock/copied prefix/share/gs3/herdr-session/core.mjs" ]
printf 'session bridge argv, rejection, copied installation and removal: PASS\n'
