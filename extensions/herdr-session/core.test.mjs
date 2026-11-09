import { test } from 'node:test';
import assert from 'node:assert/strict';
import net from 'node:net';
import fs from 'node:fs';
import path from 'node:path';
import { install } from './core.mjs';

async function fixture(t, reply = 'ok') {
  fs.mkdirSync('target', { recursive: true });
  const socket = path.resolve(`target/s${Math.random().toString(36).slice(2, 6)}`);
  const reports = [], clients = new Set();
  let inFlight = 0, maxInFlight = 0;
  const server = net.createServer(client => {
    clients.add(client); client.on('close', () => clients.delete(client));
    let data = '';
    client.on('data', chunk => {
      data += chunk;
      if (!data.includes('\n')) return;
      const request = JSON.parse(data.split('\n')[0]); reports.push(request);
      inFlight++; maxInFlight = Math.max(maxInFlight, inFlight);
      const respond = () => { inFlight--; client.end(JSON.stringify({ id: request.id, ...(['ok', 'delayed'].includes(reply) ? { result: {} } : { error: { code: 'refused' } }) }) + '\n'); };
      if (reply === 'delayed') setTimeout(respond, 20);
      else if (reply !== 'silent') respond();
    });
  });
  await new Promise(resolve => server.listen(socket, resolve));
  t.after(async () => { for (const client of clients) client.destroy(); await new Promise(resolve => server.close(resolve)); });
  const flags = new Map([['gs3-herdr-pane', 'w2:p1J'], ['gs3-herdr-socket', socket]]);
  function host(env = {}) {
    const handlers = new Map(), registered = new Map(), warnings = [];
    install({ registerFlag: (k, v) => registered.set(k, v), getFlag: k => flags.get(k), on: (k, v) => handlers.set(k, v) }, env);
    const context = (file = '/fixture/current.jsonl', mode = 'tui') => ({ mode, sessionManager: { getSessionFile: () => file }, ui: { notify: text => warnings.push(text) } });
    return { registered, warnings, fire: async (name, ctx = context(), event = {}) => handlers.get(name)?.(event, ctx), context };
  }
  return { socket, flags, reports, host, maxInFlight: () => maxInFlight };
}

test('startup publishes only the exact current session reference without HERDR env', async t => {
  const f = await fixture(t), h = f.host();
  assert.equal(h.registered.get('gs3-herdr-pane')?.type, 'string');
  await h.fire('session_start', h.context(), { reason: 'startup' });
  assert.equal(f.reports.length, 1);
  const r = f.reports[0];
  assert.equal(r.method, 'pane.report_agent_session');
  assert.deepEqual(r.params, { pane_id: 'w2:p1J', source: 'herdr:pi', agent: 'pi', seq: r.params.seq, session_start_source: 'startup', agent_session_path: '/fixture/current.jsonl' });
  assert.deepEqual(h.warnings, []);
});

test('new/resume, agent_start and reload use fresh context, retained flags and increasing sequence', async t => {
  const f = await fixture(t); let h = f.host();
  await h.fire('session_start', h.context('/fixture/a.jsonl'), { reason: 'new' });
  await h.fire('agent_start', h.context('/fixture/b.jsonl'));
  h = f.host(); // Like Pi reload: new factory, same runtime flag values.
  await h.fire('session_start', h.context('/fixture/c.jsonl'), { reason: 'reload' });
  assert.deepEqual(f.reports.map(r => r.params.agent_session_path), ['/fixture/a.jsonl', '/fixture/b.jsonl', '/fixture/c.jsonl']);
  assert.ok(f.reports.every((r, i) => i === 0 || r.params.seq > f.reports[i - 1].params.seq));
});

test('overlapping lifecycle reports are serialized in session order', async t => {
  const f = await fixture(t, 'delayed'), h = f.host();
  await Promise.all([h.fire('session_start', h.context('/fixture/a.jsonl')), h.fire('agent_start', h.context('/fixture/b.jsonl'))]);
  assert.deepEqual(f.reports.map(r => r.params.agent_session_path), ['/fixture/a.jsonl', '/fixture/b.jsonl']);
  assert.equal(f.maxInFlight(), 1);
});

test('multiple bound panes keep their own targets', async t => {
  const f = await fixture(t), a = f.host();
  await a.fire('session_start', a.context('/fixture/a.jsonl'));
  f.flags.set('gs3-herdr-pane', 'w3:p2'); const b = f.host();
  await b.fire('session_start', b.context('/fixture/b.jsonl'));
  assert.deepEqual(f.reports.map(r => [r.params.pane_id, r.params.agent_session_path]), [['w2:p1J', '/fixture/a.jsonl'], ['w3:p2', '/fixture/b.jsonl']]);
});

test('missing/partial bindings, non-TUI, and absent/relative sessions send nothing', async t => {
  const f = await fixture(t), h = f.host();
  await h.fire('session_start', h.context(undefined, 'rpc'));
  await h.fire('session_start', h.context(null));
  await h.fire('session_start', h.context('relative.jsonl'));
  f.flags.delete('gs3-herdr-pane'); await h.fire('session_start');
  f.flags.clear(); await h.fire('session_start');
  assert.deepEqual(f.reports, []);
});

test('matching official integration owns reporting; conflicting env binding never redirects', async t => {
  const f = await fixture(t);
  for (const pane of ['w2:p1J', 'other-pane']) {
    const h = f.host({ HERDR_ENV: '1', HERDR_PANE_ID: pane, HERDR_SOCKET_PATH: f.socket });
    await h.fire('session_start');
  }
  assert.deepEqual(f.reports, []);
});

test('regular files and symlink sockets are rejected before any report', async t => {
  const f = await fixture(t), h = f.host(), link = `${f.socket}l`;
  fs.symlinkSync(f.socket, link); t.after(() => fs.unlinkSync(link));
  f.flags.set('gs3-herdr-socket', link); await h.fire('session_start');
  f.flags.set('gs3-herdr-socket', path.resolve('extensions/herdr-session/core.test.mjs')); await h.fire('session_start');
  assert.deepEqual(f.reports, []);
});

for (const reply of ['error', 'silent']) test(`API ${reply} is bounded and warns without throwing`, async t => {
  const f = await fixture(t, reply), h = f.host();
  await h.fire('session_start');
  assert.equal(f.reports.length, 1);
  assert.deepEqual(h.warnings, ['gs3 session bridge: unable to report the session to herdr.']);
});
