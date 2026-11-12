#!/usr/bin/env bash
# Public PROF routing contract; only synthetic files, tools and RPC peers.
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
python3 - "${GS3_TEST_HUD:-$repo/gs3/hud/target/release/gs3-hud}" <<'PYTEST'
import hashlib, json, os, pathlib, socket, subprocess, sys, tempfile, threading
hud = str(pathlib.Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory(prefix='prof-') as temp:
    base = pathlib.Path(temp); agent = base/'agent'; routing = base/'routing'
    def write(path, data):
        path.parent.mkdir(parents=True, exist_ok=True); path.write_text(data)
    for sub in ['home', 'xdg', 'project', 'bin', 'routing']:
        (base/sub).mkdir()
    session = agent/'sessions/project/test.jsonl'; write(session, '')
    write(agent/'settings.json', json.dumps({'defaultProvider':'fixture', 'defaultModel':'base-host-987', 'defaultThinkingLevel':'medium'}))
    write(agent/'agents/worker.md', '---\nname: gentle-ai-worker\nmodel: fixture/stale-md-987\neffort: low\n---\n')
    write(agent/'gentle-agents/tasks/one.json', json.dumps({'task':{'agent':'gentle-ai-worker','model':'fixture/stale-task-987','thinking':'low','status':'completed','createdAt':1}}))
    for tool in ['git', 'gh', 'gentle-shell', 'herdr']:
        path = base/'bin'/tool; write(path, '#!/bin/sh\nexit 1\n'); path.chmod(0o700)
    env = {'PATH':str(base/'bin')+':/usr/bin:/bin', 'HOME':str(base/'home'),
           'XDG_CONFIG_HOME':str(base/'xdg'), 'XDG_CACHE_HOME':str(base/'xdg'),
           'GENTLE_PI_CONFIG_HOME':str(routing), 'GENTLE_PI_AGENT_HOME':str(agent),
           'PI_CODING_AGENT_DIR':str(agent), 'ENGRAM_PORT':'0', 'ENGRAM_DB_PATH':str(base/'missing.db'),
           'GS3_SHELL':str(base/'bin/gentle-shell'), 'GS3_HERDR':str(base/'bin/herdr')}
    sock = base/'rpc.sock'; listener = socket.socket(socket.AF_UNIX); listener.bind(str(sock)); listener.listen(4); listener.settimeout(4)
    def snapshot():
        return {str(p.relative_to(base)):(p.stat().st_mode, hashlib.sha256(p.read_bytes()).hexdigest())
                for p in base.rglob('*') if p.is_file()}
    def run(label, expected, marker, forbidden=()):
        requests, errors = [], []
        def serve():
            try:
                for _ in range(2):
                    conn, _ = listener.accept()
                    with conn:
                        req = json.loads(conn.makefile('rb').readline()); requests.append(req['method'])
                        if req['method'] == 'pane.get':
                            result = {'pane':{'pane_id':'fixture','workspace_id':'fixture','agent_session':{'value':str(session)}}}
                        elif req['method'] == 'workspace.get': result = {'workspace':{'label':'fixture'}}
                        else: raise AssertionError(req['method'])
                        conn.sendall((json.dumps({'result':result})+'\n').encode())
            except Exception as e: errors.append(str(e))
        before = snapshot(); worker = threading.Thread(target=serve,daemon=True); worker.start()
        proc = subprocess.run([hud,'--pane','fixture','--socket',str(sock),'--cwd',str(base/'project'),'--once'], env=env,capture_output=True,text=True,timeout=10)
        worker.join(5)
        compact = ''.join(proc.stdout.split()).lower()
        problems = []
        if proc.returncode != 0 or proc.stderr: problems.append('exit=%s stderr=%r' % (proc.returncode,proc.stderr))
        if requests != ['pane.get','workspace.get'] or errors or worker.is_alive(): problems.append('RPC=%r errors=%r' % (requests,errors))
        if before != snapshot(): problems.append('fixture modified')
        for value in [marker,'base-host-987','TaskManager','1/1']+list(expected):
            if ''.join(value.split()).lower() not in compact: problems.append('missing '+value)
        for value in ['stale-md-987','stale-task-987','SENSITIVE_SENTINEL']+list(forbidden):
            if value.lower() in compact: problems.append('unexpected '+value)
        if problems:
            print(label+': FAIL '+ '; '.join(problems),file=sys.stderr); print(proc.stdout,file=sys.stderr); raise SystemExit(1)
        print(label+': PASS exit=0 stderr=empty RPC=2 fixture=unchanged')
        return proc.stdout
    models = routing/'models.json'
    # Focus either new regression for RED without skipping it after another failure.
    selected = os.environ.get('GS3_PROFILE_CASE', 'all')
    assert selected in ('all', 'baseline', 'roles', 'bytes'), 'invalid GS3_PROFILE_CASE'
    count = 0
    if selected in ('all', 'baseline'):
        initial = {'orchestrator':{'model':'fixture/prof-host-987','thinking':'high'},
                   'gentle-ai-worker':{'model':'fixture/prof-worker-987','effort':'xhigh','apiKey':'SENSITIVE_SENTINEL'},
                   'review-refuter':'fixture/prof-refuter-987','review-validator':{}}
        write(models,json.dumps(initial))
        run('canonical', ['prof-host-987','prof-worker-987','prof-refuter-987','review-validator','xhigh'], '/gentle:profiles')
        replacement = routing/'models.tmp'; write(replacement,json.dumps({'gentle-ai-worker':{'model':'fixture/prof-worker-988','thinking':'high'}})); replacement.replace(models)
        run('atomic replacement', ['prof-worker-988'], '/gentle:profiles', ['prof-worker-987','prof-refuter-987','prof-host-987'])
        models.unlink(); run('missing', ['gentle-ai-worker'], 'profiles: no disponible', ['prof-worker-988'])
        write(models,'{not json'); run('invalid', [], 'profiles: JSON inválido', ['prof-worker-988'])
        env.pop('GENTLE_PI_CONFIG_HOME')
        write(base/'home/.pi/gentle-ai/models.json',json.dumps({'review-refuter':'fixture/wrong-home-987'}))
        run('unknown origin', [], 'profiles: ruta desconocida', ['wrong-home-987'])
        count += 5
    env['GENTLE_PI_CONFIG_HOME'] = str(routing)
    if selected in ('all', 'roles'):
        many = {'role-%03d' % i: {'model':'fixture/model-%03d' % i} for i in range(300)}
        many['z-final-role'] = {'model':'fixture/prof-final-987'}
        write(models,json.dumps(many))
        run('more than 256 roles', list(many)+['prof-final-987'], '/gentle:profiles')
        count += 1
    if selected in ('all', 'bytes'):
        write(models,json.dumps({'gentle-ai-worker':{'model':'fixture/prof-large-987','description':'x'*300000}}))
        assert models.stat().st_size > 256*1024
        run('more than 256 KiB', ['prof-large-987'], '/gentle:profiles')
        count += 1
    # B: a bundled prefix derives <prefix>/config/models.json from its <prefix>/agent home.
    # D: without an `orchestrator` entry the host row falls back to the session defaults.
    bundled = base/'bundled'; b_agent = bundled/'agent'; b_session = b_agent/'sessions/project/test.jsonl'
    write(b_session, '')
    write(b_agent/'settings.json', json.dumps({'defaultProvider':'fixture','defaultModel':'base-host-987','defaultThinkingLevel':'medium'}))
    write(b_agent/'agents/worker.md', '---\nname: gentle-ai-worker\nmodel: fixture/stale-md-987\n---\n')
    write(b_agent/'gentle-agents/tasks/one.json', json.dumps({'task':{'agent':'gentle-ai-worker','status':'completed','createdAt':1}}))
    write(bundled/'config/models.json', json.dumps({'review-refuter':'fixture/bundled-refuter-987'}))
    session = b_session
    env = {k: v for k, v in env.items() if k not in ('GENTLE_PI_CONFIG_HOME', 'GENTLE_PI_AGENT_HOME', 'PI_CODING_AGENT_DIR')}
    run('bundled prefix', ['bundled-refuter-987'], '/gentle:profiles · global', ['ruta desconocida', 'stale-md-987'])
    os.rename(bundled/'config/models.json', bundled/'config/real.json'); os.symlink(bundled/'config/real.json', bundled/'config/models.json')
    run('bundled symlink', [], 'profiles: archivo inválido', ['bundled-refuter-987'])
    count += 2
    listener.close()
print('PROF: %s public cases passed; no real profiles or services touched' % count)
PYTEST
