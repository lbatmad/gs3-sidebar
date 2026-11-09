import net from 'node:net';
import { lstat } from 'node:fs/promises';
import path from 'node:path';

let sequence = 0;

export function install(pi, env = process.env) {
  pi.registerFlag('gs3-herdr-pane', { type: 'string', description: 'gs3-owned herdr pane ID (paired with --gs3-herdr-socket)' });
  pi.registerFlag('gs3-herdr-socket', { type: 'string', description: 'Absolute local herdr socket for the gs3 session bridge' });
  let queue = Promise.resolve(), active, stopped = false;

  async function publish(event, ctx) {
    if (stopped || ctx.mode !== 'tui') return;
    const pane = pi.getFlag('gs3-herdr-pane'), socket = pi.getFlag('gs3-herdr-socket');
    if (typeof pane !== 'string' || !pane || typeof socket !== 'string' || !path.isAbsolute(socket)) return;
    // Let herdr's official integration own reporting when it is enabled.
    // A different inherited binding must not be silently overridden either.
    if (env.HERDR_ENV === '1' && env.HERDR_PANE_ID && env.HERDR_SOCKET_PATH) return;
    try {
      const session = ctx.sessionManager.getSessionFile();
      if (typeof session !== 'string' || !path.isAbsolute(session)) return;
      const stat = await lstat(socket);
      if (!stat.isSocket() || stat.uid !== process.getuid()) throw new Error('untrusted socket');
      if (stopped) return;
      sequence = Math.max(sequence + 1, Date.now() * 1000);
      const request = { id: `gs3:session:${sequence}`, method: 'pane.report_agent_session', params: {
        pane_id: pane, source: 'herdr:pi', agent: 'pi', seq: sequence,
        ...(event.reason ? { session_start_source: event.reason } : {}), agent_session_path: session,
      } };
      await new Promise((resolve, reject) => {
        const client = net.createConnection(socket); active = client;
        let data = '', finished = false;
        const finish = error => {
          if (finished) return;
          finished = true; clearTimeout(timer); client.destroy(); active = undefined;
          error ? reject(error) : resolve();
        };
        const timer = setTimeout(() => finish(new Error('timeout')), 750);
        client.on('error', finish);
        client.on('close', () => finish(new Error('closed')));
        client.on('connect', () => client.write(JSON.stringify(request) + '\n'));
        client.on('data', chunk => {
          data += chunk;
          if (data.length > 65536) return finish(new Error('oversized reply'));
          if (!data.includes('\n')) return;
          try {
            const reply = JSON.parse(data.split('\n')[0]);
            if (reply.id !== request.id || reply.error || !Object.hasOwn(reply, 'result')) throw new Error('rejected report');
            finish();
          } catch (error) { finish(error); }
        });
      });
    } catch {
      if (!stopped) ctx.ui?.notify?.('gs3 session bridge: unable to report the session to herdr.', 'warning');
    }
  }
  const enqueue = (event, ctx) => (queue = queue.then(() => publish(event, ctx)));
  pi.on('session_start', (event, ctx) => { stopped = false; return enqueue(event, ctx); });
  pi.on('agent_start', (_event, ctx) => enqueue({}, ctx));
  pi.on('session_shutdown', () => { stopped = true; active?.destroy(); });
}
