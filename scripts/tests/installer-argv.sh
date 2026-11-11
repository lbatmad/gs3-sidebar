#!/bin/bash
# Exercise the public installer with isolated, recording helpers.
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
tmp=$(mktemp -d /tmp/gs3-installer-argv.XXXXXX)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/project/scripts" "$tmp/fixture"
cp "$root/install.sh" "$tmp/project/install.sh"
cp "$root/scripts/lib.sh" "$tmp/project/scripts/lib.sh"
printf 'unchanged\n' > "$tmp/fixture/sentinel"
printf 'glob fixture\n' > "$tmp/fixture/match-one"
for helper in check-deps build-hud install-launcher install-extensions doctor; do
  printf '%s\n' '#!/bin/bash' 'printf "%s" "$(basename "$0")" >> "$RECORD"' 'for arg in "$@"; do printf " <%s>" "$arg" >> "$RECORD"; done' 'printf "\n" >> "$RECORD"' > "$tmp/project/scripts/$helper.sh"
  chmod +x "$tmp/project/scripts/$helper.sh"
done
export GS3_PREFIX=default-prefix GS3_CONFIG_DIR=default-config GS3_AGENT_DIR=default-agent GS3_DRY_RUN=0
export RECORD="$tmp/record"
unset XDG_CONFIG_HOME PI_CODING_AGENT_DIR || :
passed=0; failed=0
run_case() {
  shell=$1; name=$2; prefix=$3; config=$4; agent=$5; expected=$6
  shift 6
  : > "$RECORD"
  printf 'gs3-sidebar will install:\n  %s/bin/gs3, %s/bin/gs3-hud\n  %s\n  Pi extensions in %s/extensions\n\n== Verification\n\nDone. Open a terminal inside herdr and run: gs3\n' "$prefix" "$prefix" "$config" "$agent" > "$tmp/expected-out"
  printf '%s\n' "$expected" > "$tmp/expected-record"
  rc=0
  (cd "$tmp/fixture" && "$shell" "$tmp/project/install.sh" "$@") > "$tmp/out" 2> "$tmp/err" || rc=$?
  if [ "$rc" -eq 0 ] && [ ! -s "$tmp/err" ] && cmp -s "$tmp/expected-out" "$tmp/out" && cmp -s "$tmp/expected-record" "$RECORD" && printf 'unchanged\n' | cmp -s - "$tmp/fixture/sentinel" && printf 'glob fixture\n' | cmp -s - "$tmp/fixture/match-one" && [ "$(ls -A "$tmp/fixture" | wc -l | tr -d ' ')" = 2 ]; then
    printf 'PASS %s %s\n' "$shell" "$name"; passed=$((passed + 1))
  else
    printf 'FAIL %s %s exit=%s\nstdout:\n' "$shell" "$name" "$rc"
    /bin/cat "$tmp/out"
    printf 'stderr:\n'; /bin/cat "$tmp/err"
    printf 'expected helper argv:\n'; /bin/cat "$tmp/expected-record"
    printf 'actual helper argv:\n'; /bin/cat "$RECORD"
    failed=$((failed + 1))
  fi
}
run_suite() {
  shell=$1
  run_case "$shell" empty default-prefix default-config default-agent 'check-deps.sh
build-hud.sh
install-launcher.sh
install-extensions.sh
doctor.sh'
  run_case "$shell" plain prefix config agent 'check-deps.sh <--no-build>
build-hud.sh <--binary> <binary> <--prefix> <prefix> <--agent-dir> <agent> <--config-dir> <config>
install-launcher.sh <--prefix> <prefix> <--agent-dir> <agent> <--config-dir> <config>
install-extensions.sh <--prefix> <prefix> <--agent-dir> <agent> <--config-dir> <config>
doctor.sh <--prefix> <prefix> <--agent-dir> <agent> <--config-dir> <config>' --binary binary --prefix prefix --agent-dir agent --config-dir config -y
  run_case "$shell" whitespace 'prefix dir' 'config dir' 'agent dir' 'check-deps.sh <--no-build>
build-hud.sh <--binary> <binary file> <--prefix> <prefix dir> <--agent-dir> <agent dir> <--config-dir> <config dir>
install-launcher.sh <--prefix> <prefix dir> <--agent-dir> <agent dir> <--config-dir> <config dir>
install-extensions.sh <--prefix> <prefix dir> <--agent-dir> <agent dir> <--config-dir> <config dir>
doctor.sh <--prefix> <prefix dir> <--agent-dir> <agent dir> <--config-dir> <config dir>' --binary 'binary file' --prefix 'prefix dir' --agent-dir 'agent dir' --config-dir 'config dir' -y
  run_case "$shell" glob 'match-*' default-config default-agent 'check-deps.sh
build-hud.sh <--prefix> <match-*>
install-launcher.sh <--prefix> <match-*>
install-extensions.sh <--prefix> <match-*>
doctor.sh <--prefix> <match-*>' --prefix 'match-*' -y
}
run_suite /bin/bash
other=$(command -v bash)
if [ "$other" != /bin/bash ] && [ "$("$other" -c 'printf %s "$BASH_VERSION"')" != "$(/bin/bash -c 'printf %s "$BASH_VERSION"')" ]; then run_suite "$other"; fi
printf 'TOTAL pass=%s fail=%s\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
