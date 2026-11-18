#!/usr/bin/env bash
# Isolated launcher check: nothing is installed and no real herdr pane is touched.
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
mock="$(mktemp -d /tmp/gs3-launcher-test.XXXXXX)"
trap 'rm -rf "$mock"' EXIT
cat > "$mock/herdr" <<HDR
#!/bin/sh
case "\$1 \$2" in
  "pane split") printf '{"result":{"pane":{"pane_id":"hud"}}}\n' ;;
  "pane run") printf '%s\n' "\$*" >> "$mock/panerun" ;;
  "pane list") printf '{"result":{"panes":[{"pane_id":"main"},{"pane_id":"stale"},{"pane_id":"multi"},{"pane_id":"argvrail"},{"pane_id":"decoy"},{"pane_id":"echoes"},{"pane_id":"lookalike"},{"pane_id":"argvnosh"},{"pane_id":"other"}]}}\n' ;;
  "pane process-info")
    case "\$*" in
      *"--pane stale"*) printf '{"result":{"process_info":{"foreground_processes":[{"cmdline":"/x/bin/gs3-hud --pane main --cwd /x"}]}}}\n' ;;
      *"--pane multi"*) printf '{"result":{"process_info":{"foreground_processes":[{"cmdline":"/bin/zsh"},{"cmdline":"/x/bin/gs3-hud --pane main --cwd /x"}]}}}\n' ;;
      *"--pane argvrail"*) printf '{"result":{"process_info":{"foreground_processes":[{"argv":["/x/bin/gs3-hud","--pane","main","--cwd","/x"]}]}}}\n' ;;
      # Both strings, wrong order: a command that merely mentions them is not a rail.
      *"--pane decoy"*) printf '{"result":{"process_info":{"foreground_processes":[{"cmdline":"/bin/echo --pane main gs3-hud"}]}}}\n' ;;
      # Right order, but the strings are arguments of another program, not the executable.
      *"--pane echoes"*) printf '{"result":{"process_info":{"foreground_processes":[{"cmdline":"/bin/echo gs3-hud --pane main"}]}}}\n' ;;
      *"--pane lookalike"*) printf '{"result":{"process_info":{"foreground_processes":[{"cmdline":"/bin/not-gs3-hud --pane main"}]}}}\n' ;;
      # herdr reported an argv without argv0; the command line still proves the rail.
      *"--pane argvnosh"*) printf '{"result":{"process_info":{"foreground_processes":[{"argv":["--pane","main"],"cmdline":"/x/bin/gs3-hud --pane main --cwd /x"}]}}}\n' ;;
      *) printf '{"result":{"process_info":{"foreground_processes":[{"cmdline":"/x/bin/pi --resume"}]}}}\n' ;;
    esac ;;
  "pane close") printf '%s\n' "\$*" >> "$mock/paneclose" ;;
  *) exit 0 ;;
esac
HDR
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
# A: Gentle Shell rebuilds Pi's environment, so the sidecar pane inherits no routing
# home; gs3 forwards the explicit absolute ones and never invents or forwards a relative one.
pane_run() { cat "$mock/panerun" 2>/dev/null || true; }
rm -f "$mock/panerun"
env -u GS3_SHELL HERDR_ENV=1 HERDR_PANE_ID=main GS3_HUD=/usr/bin/true PATH="$mock:/usr/bin:/bin" \
  GENTLE_PI_CONFIG_HOME=/fixture/config GENTLE_SHELL_CONFIG_HOME=/fixture/shell-config \
  GENTLE_PI_AGENT_HOME=/fixture/agent PI_CODING_AGENT_DIR=/fixture/agent \
  /bin/bash "$repo/gs3/bin/gs3" >/dev/null
for want in 'GENTLE_PI_CONFIG_HOME=/fixture/config' 'GENTLE_SHELL_CONFIG_HOME=/fixture/shell-config' 'GENTLE_PI_AGENT_HOME=/fixture/agent' 'PI_CODING_AGENT_DIR=/fixture/agent'; do
  case "$(pane_run)" in *"$want"*) ;; *) printf 'sidecar missed routing home: %s\n' "$want" >&2; exit 1 ;; esac
done
rm -f "$mock/panerun"
env -u GS3_SHELL HERDR_ENV=1 HERDR_PANE_ID=main GS3_HUD=/usr/bin/true PATH="$mock:/usr/bin:/bin" \
  GENTLE_SHELL_CONFIG_HOME=relative/home /bin/bash "$repo/gs3/bin/gs3" >/dev/null
case "$(pane_run)" in *relative/home*) printf 'sidecar accepted a relative routing home\n' >&2; exit 1 ;; esac
rm -f "$mock/panerun"
env -u GS3_SHELL -u GENTLE_PI_CONFIG_HOME -u GENTLE_SHELL_CONFIG_HOME -u GENTLE_PI_AGENT_HOME -u PI_CODING_AGENT_DIR \
  HERDR_ENV=1 HERDR_PANE_ID=main GS3_HUD=/usr/bin/true PATH="$mock:/usr/bin:/bin" /bin/bash "$repo/gs3/bin/gs3" >/dev/null
case "$(pane_run)" in *GENTLE_PI_CONFIG_HOME*|*GENTLE_SHELL_CONFIG_HOME*|*GENTLE_PI_AGENT_HOME*|*PI_CODING_AGENT_DIR*) printf 'sidecar invented a routing home\n' >&2; exit 1 ;; esac
printf 'macOS defaults to Gentle Shell; Linux and explicit overrides remain supported; routing homes forwarded to the sidecar\n'
# A: the rail is a pane, and a rail re-created outside this run (a manual restart, a second gs3)
# must not survive the cleanup: closing only the pane id captured at startup is how /quit leaves
# a panel hooked to the side, where no shell reads `q` any more.
rm -f "$mock/paneclose"
env -u GS3_SHELL HERDR_ENV=1 HERDR_PANE_ID=main GS3_HUD=/usr/bin/true PATH="$mock:/usr/bin:/bin" \
  /bin/bash "$repo/gs3/bin/gs3" >/dev/null
closed="$(cat "$mock/paneclose" 2>/dev/null || true)"
case "$closed" in *"pane close hud"*) ;; *) printf 'own rail survived the cleanup: %s\n' "$closed" >&2; exit 1 ;; esac
case "$closed" in *"pane close stale"*) ;; *) printf 're-created rail survived the cleanup: %s\n' "$closed" >&2; exit 1 ;; esac
# A rail is the pane's second foreground process here, and herdr reports argv instead of a
# command line: both are still rails bound to this pane.
case "$closed" in *"pane close multi"*) ;; *) printf 'a rail behind another foreground process survived: %s\n' "$closed" >&2; exit 1 ;; esac
case "$closed" in *"pane close argvrail"*) ;; *) printf 'a rail found only in argv survived: %s\n' "$closed" >&2; exit 1 ;; esac
case "$closed" in *"pane close argvnosh"*) ;; *) printf 'a rail whose argv lacked argv0 survived: %s\n' "$closed" >&2; exit 1 ;; esac
case "$closed" in *"pane close other"*|*"pane close decoy"*|*"pane close echoes"*|*"pane close lookalike"*) printf 'closed a pane that is not a rail: %s\n' "$closed" >&2; exit 1 ;; esac
printf 'every rail bound to the pane is closed on exit, and nothing else\n'
