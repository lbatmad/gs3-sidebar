#!/usr/bin/env bash
# Check the public launcher passes the selected tools literally to the sidecar.
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
mock="$(mktemp -d /tmp/gs3-stack-test.XXXXXX)"
mkdir -p "$mock/tools with spaces"
trap 'rm -rf "$mock"' EXIT
cat > "$mock/tools with spaces/herdr" <<'MOCK'
#!/bin/sh
case "$1 $2" in
  'pane split') printf '{"result":{"pane":{"pane_id":"hud"}}}\n' ;;
  'pane run') printf '%s\n' "$4" > "$GS3_TEST_COMMAND" ;;
esac
MOCK
printf '#!/bin/sh\nexit 0\n' > "$mock/tools with spaces/shell"
chmod +x "$mock/tools with spaces/herdr" "$mock/tools with spaces/shell"
export GS3_TEST_COMMAND="$mock/command"
HERDR_ENV=1 HERDR_PANE_ID=main GS3_HUD=/usr/bin/true \
  GS3_SHELL="$mock/tools with spaces/shell" GS3_HERDR="$mock/tools with spaces/herdr" \
  env -u GENTLE_PI_CONFIG_HOME -u GENTLE_SHELL_CONFIG_HOME -u GENTLE_PI_AGENT_HOME -u PI_CODING_AGENT_DIR \
  /bin/bash "$repo/gs3/bin/gs3"
command_text="$(< "$mock/command")"
expected="GS3_SHELL=$(printf '%q' "$mock/tools with spaces/shell") GS3_HERDR=$(printf '%q' "$mock/tools with spaces/herdr") exec "
[[ "$command_text" == "$expected"* ]] || { printf 'selected tool paths missing or incorrectly quoted: %s\n' "$command_text" >&2; exit 1; }
# Exercise shell parsing without launching a real HUD or tools.
command_text="${command_text%%exec *}/usr/bin/env"
result="$(/bin/bash -c "$command_text")"
[[ "$result" == *"GS3_SHELL=$mock/tools with spaces/shell"* ]]
[[ "$result" == *"GS3_HERDR=$mock/tools with spaces/herdr"* ]]
# A forwarded routing home comes first and survives shell parsing untouched.
HERDR_ENV=1 HERDR_PANE_ID=main GS3_HUD=/usr/bin/true \
  GENTLE_PI_CONFIG_HOME="$mock/tools with spaces/config" \
  GS3_SHELL="$mock/tools with spaces/shell" GS3_HERDR="$mock/tools with spaces/herdr" \
  /bin/bash "$repo/gs3/bin/gs3"
command_text="$(< "$mock/command")"
[[ "$command_text" == "GENTLE_PI_CONFIG_HOME=$(printf '%q' "$mock/tools with spaces/config") "* ]] || { printf 'routing home lost or unquoted: %s\n' "$command_text" >&2; exit 1; }
command_text="${command_text%%exec *}/usr/bin/env"
result="$(/bin/bash -c "$command_text")"
[[ "$result" == *"GENTLE_PI_CONFIG_HOME=$mock/tools with spaces/config"* ]] || { printf 'routing home mangled by the shell: %s\n' "$result" >&2; exit 1; }
printf 'Stack launcher paths are explicit and shell-safe\n'

# Run the public HUD against a wholly synthetic installation and Unix socket.
# GS3_TEST_HUD permits comparing another binary without embedding a local path.
python3 - "$mock" "${GS3_TEST_HUD:-$repo/gs3/hud/target/release/gs3-hud}" <<'PY'
import hashlib, json, os, pathlib, socket, subprocess, sys, threading

base, hud = map(pathlib.Path, sys.argv[1:])
root = base / 'fixture'
agent = root / 'agent'
legacy = root / 'prefix/lib/node_modules/gentle-pi'
managed = agent / 'npm/node_modules/gentle-pi'

def write(path, text, executable=False):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)
    if executable:
        path.chmod(0o700)

def manifest(path, name, version):
    write(path / 'package.json', json.dumps({'name': name, 'version': version}))

def binary(path, version):
    write(path, '#!/bin/sh\n[ "$1" = "--version" ] || [ "$1" = "version" ] || exit 1\nprintf "%s\\n" "' + version + '"\n', True)

(agent / 'sessions/project').mkdir(parents=True)
(root / 'home').mkdir()
(root / 'project').mkdir()
(root / 'xdg').mkdir()
write(agent / 'sessions/project/session.jsonl', '')
write(root / 'installation.json', json.dumps({'Schema': 'gentle-shell-user-install/v1', 'Prefix': str(root / 'prefix'), 'Agent': str(agent)}))
manifest(root / 'prefix/lib/node_modules/@earendil-works/pi-coding-agent', '@earendil-works/pi-coding-agent', '1.5.0')
manifest(legacy, 'gentle-pi', '4.5.1')
manifest(managed, 'gentle-pi', '4.7.0')
for package, pin in [(legacy, '4.3.1'), (managed, '4.4.0')]:
    write(package / 'scripts/gentle-ai-installer.mjs', 'export const INSTALLER_VERSION = "' + pin + '";\n')
    binary(package / ('.gentle-ai/v' + pin) / 'gentle-ai', 'gentle-ai ' + pin)
binary(agent / 'bin/engram', 'engram 3.7.0')
binary(root / 'bin/herdr', 'herdr 0.9.3')
binary(root / 'bin/gentle-shell', 'pi 99.9.9')
# Avoid invoking real Git, gh, or any PATH-selected tool during the HUD run.
write(root / 'bin/git', '#!/bin/sh\nexit 1\n', True)
write(root / 'bin/gh', '#!/bin/sh\nexit 1\n', True)
settings = agent / 'settings.json'
disabled = {'source': str(legacy), 'extensions': [], 'skills': [], 'prompts': [], 'themes': []}
sockpath = str(base / 'mock.sock')
listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
listener.bind(sockpath)
listener.listen(4)
listener.settimeout(5)

def snapshot():
    return {str(p.relative_to(root)): (p.stat().st_mode, hashlib.sha256(p.read_bytes()).hexdigest()) if p.is_file() else ('dir',)
            for p in root.rglob('*')}

def run_case(label, packages, shell_version, ai_version):
    write(settings, json.dumps({'packages': packages}))
    before = snapshot()
    requests = []
    errors = []
    def serve():
        try:
            for _ in range(2):
                conn, _ = listener.accept()
                with conn:
                    f = conn.makefile('rb')
                    req = json.loads(f.readline())
                    requests.append(req['method'])
                    if req['method'] == 'pane.get':
                        result = {'pane': {'pane_id': 'fixture', 'workspace_id': 'fixture', 'agent_session': {'value': str(agent / 'sessions/project/session.jsonl')}}}
                    elif req['method'] == 'workspace.get':
                        result = {'workspace': {'label': 'fixture'}}
                    else:
                        raise AssertionError('unexpected method: ' + req['method'])
                    conn.sendall((json.dumps({'result': result}) + '\n').encode())
        except Exception as e:
            errors.append(str(e))
    worker = threading.Thread(target=serve, daemon=True)
    worker.start()
    env = {'PATH': str(root / 'bin') + ':/usr/bin:/bin', 'HOME': str(root / 'home'),
           'XDG_CONFIG_HOME': str(root / 'xdg'), 'XDG_CACHE_HOME': str(root / 'xdg'),
           'ENGRAM_PORT': '0', 'ENGRAM_DB_PATH': str(root / 'absent.db'),
           'GS3_SHELL': str(root / 'bin/gentle-shell'), 'GS3_HERDR': str(root / 'bin/herdr')}
    proc = subprocess.run([str(hud), '--pane', 'fixture', '--socket', sockpath, '--cwd', str(root / 'project'), '--once'],
                          env=env, capture_output=True, text=True, timeout=12)
    worker.join(6)
    lines = proc.stdout.splitlines()
    stack = [line.strip() for line in lines if any(name in line for name in ('● pi ', '● gs3-hud ', 'gentle-shell ', 'gentle-ai ', '● engram ', '● herdr '))]
    expected = ['● pi v1.5.0', '● gs3-hud v0.1.0', shell_version, ai_version, '● engram v3.7.0', '● herdr v0.9.3']
    issues = []
    if proc.returncode != 0 or proc.stderr: issues.append('exit=%s stderr=%r' % (proc.returncode, proc.stderr))
    if requests != ['pane.get', 'workspace.get'] or errors or worker.is_alive(): issues.append('socket requests=%r errors=%r' % (requests, errors))
    if snapshot() != before: issues.append('fixture changed')
    if not all(any(item in line for line in stack) for item in expected): issues.append('expected=%r observed=%r' % (expected, stack))
    if issues:
        print(label + ': FAIL ' + '; '.join(issues), file=sys.stderr)
        print('HUD stdout:\n' + proc.stdout, file=sys.stderr)
        raise SystemExit(1)
    print(label + ': exit=0 stderr=empty fixture=unchanged; ' + '; '.join(expected))

run_case('managed', [disabled, 'npm:gentle-pi@4.7.0'], '● gentle-shell v4.7.0', '● gentle-ai v4.4.0')
run_case('legacy', [], '● gentle-shell v4.5.1', '● gentle-ai v4.3.1')
run_case('ambiguous', [str(legacy), 'npm:gentle-pi@4.7.0'], '○ gentle-shell · no verificada', '○ gentle-ai · no verificada')
run_case('missing', [disabled, 'npm:gentle-pi@9.9.9'], '○ gentle-shell · no verificada', '○ gentle-ai · no verificada')
listener.close()
PY
