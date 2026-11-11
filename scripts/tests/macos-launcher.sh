#!/usr/bin/env bash
# Isolated launcher check: nothing is installed and no real herdr pane is touched.
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
mock="$(mktemp -d /tmp/gs3-launcher-test.XXXXXX)"
trap 'rm -rf "$mock"' EXIT
printf '#!/bin/sh\ncase "$1 $2" in "pane split") printf "{\\"result\\":{\\"pane\\":{\\"pane_id\\":\\"hud\\"}}}\\n";; esac\n' > "$mock/herdr"
printf '#!/bin/sh\nprintf "gentle-shell:%%s\\n" "$*"\n' > "$mock/gentle-shell"
printf '#!/bin/sh\nprintf "pi:%%s\\n" "$*"\n' > "$mock/pi"
chmod +x "$mock/herdr" "$mock/gentle-shell" "$mock/pi"
result="$(env -u GS3_SHELL HERDR_ENV=1 HERDR_PANE_ID=main GS3_HUD=/usr/bin/true PATH="$mock:/usr/bin:/bin" /bin/bash "$repo/gs3/bin/gs3" 'two words')"
expected="--extension $repo/extensions/herdr-session/index.ts --gs3-herdr-pane main --gs3-herdr-socket ${HERDR_SOCKET_PATH:-$HOME/.config/herdr/herdr.sock} two words"
[ "$result" = "gentle-shell:$expected" ] || { printf 'wrong foreground application: %s\n' "$result" >&2; exit 1; }
result="$(HERDR_ENV=1 HERDR_PANE_ID=main GS3_HUD=/usr/bin/true GS3_SHELL="$mock/pi" PATH="$mock:/usr/bin:/bin" /bin/bash "$repo/gs3/bin/gs3" 'two words')"
[ "$result" = "pi:$expected" ] || { printf 'explicit shell override changed: %s\n' "$result" >&2; exit 1; }
printf '#!/bin/sh\nprintf "Linux\\n"\n' > "$mock/uname"
printf '#!/bin/sh\nprintf "gentle-shell2:%%s\\n" "$*"\n' > "$mock/gentle-shell2"
chmod +x "$mock/uname" "$mock/gentle-shell2"
result="$(env -u GS3_SHELL HERDR_ENV=1 HERDR_PANE_ID=main GS3_HUD=/usr/bin/true PATH="$mock:/usr/bin:/bin" /bin/bash "$repo/gs3/bin/gs3" 'two words')"
[ "$result" = "gentle-shell2:$expected" ] || { printf 'Linux foreground agent changed: %s\n' "$result" >&2; exit 1; }
printf 'macOS defaults to Gentle Shell; Linux and explicit overrides remain supported\n'
