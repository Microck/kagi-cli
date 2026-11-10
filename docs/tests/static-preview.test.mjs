import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createServer } from 'node:net';
import test from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';
import config from '../e2e.config.ts';

test('browser preview serves static export routes, search and assets without an SPA fallback', async (t) => {
  const root = await mkdtemp(join(tmpdir(), 'kagi-docs-preview-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  await mkdir(join(root, 'commands', 'assistant'), { recursive: true });
  await mkdir(join(root, 'api'));
  await mkdir(join(root, '_next', 'static'), { recursive: true });
  await writeFile(join(root, 'index.html'), '<h1>Home</h1>');
  await writeFile(join(root, 'commands', 'assistant', 'index.html'), '<h1>Assistant</h1>');
  await writeFile(join(root, 'api', 'search'), JSON.stringify({ indexes: ['Assistant'] }));
  await writeFile(join(root, '_next', 'static', 'app.js'), 'console.log("docs");');

  const { command } = config.targets[0].app;
  assert.equal(command.executable, 'node');
  assert.deepEqual(command.args, [
    'node_modules/sirv-cli/bin.js', 'out', '--host', '127.0.0.1', '--port', '4182', '--quiet',
  ]);
  const reservation = createServer();
  reservation.listen(0, '127.0.0.1');
  await once(reservation, 'listening');
  const port = reservation.address().port;
  await new Promise((resolve, reject) => reservation.close((error) => error ? reject(error) : resolve()));
  const args = [...command.args];
  args[1] = root;
  args[5] = String(port);
  const { HOST, PORT, ...env } = process.env;
  const server = spawn(process.execPath, args, {
    cwd: new URL('..', import.meta.url), env, stdio: ['ignore', 'pipe', 'pipe'],
  });
  t.after(async () => {
    if (server.exitCode === null && server.signalCode === null) {
      const exited = once(server, 'exit');
      server.kill();
      await exited;
    }
  });

  let output = '';
  let startupError;
  server.once('error', (error) => { startupError = error; });
  server.stderr.on('data', (chunk) => { output += chunk; });
  const origin = `http://127.0.0.1:${port}`;
  const deadline = Date.now() + 10_000;
  while (true) {
    if (startupError) throw startupError;
    assert.equal(server.exitCode, null, `Preview exited: ${output}`);
    try {
      const response = await fetch(origin, { signal: AbortSignal.timeout(500) });
      if (response.ok) break;
    } catch {}
    assert.ok(Date.now() < deadline, `Preview did not start: ${output}`);
    await delay(50);
  }

  for (const [path, body] of [
    ['/', '<h1>Home</h1>'],
    ['/commands/assistant', '<h1>Assistant</h1>'],
    ['/commands/assistant/', '<h1>Assistant</h1>'],
    ['/_next/static/app.js', 'console.log("docs");'],
  ]) {
    const response = await fetch(origin + path);
    assert.equal(response.status, 200, path);
    assert.equal(await response.text(), body, path);
  }
  const search = await fetch(origin + '/api/search');
  assert.equal(search.status, 200);
  assert.deepEqual(await search.json(), { indexes: ['Assistant'] });
  assert.equal((await fetch(origin + '/missing-page')).status, 404);
});
