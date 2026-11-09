import { test } from 'node:test';
import assert from 'node:assert/strict';

const root = process.env.PI_PKG_ROOT;
test('installed Pi loader registers explicit bridge flags and session lifecycle hooks', { skip: !root && 'Set PI_PKG_ROOT to the installed Pi package' }, async () => {
  const { loadExtensions } = await import(root + '/dist/core/extensions/loader.js');
  const loaded = await loadExtensions([new URL('./index.ts', import.meta.url).pathname], process.cwd());
  assert.deepEqual(loaded.errors, []);
  const extension = loaded.extensions[0];
  for (const flag of ['gs3-herdr-pane', 'gs3-herdr-socket']) assert.equal(extension.flags.get(flag).type, 'string');
  for (const event of ['session_start', 'agent_start', 'session_shutdown']) assert.ok(extension.handlers.has(event));
});
