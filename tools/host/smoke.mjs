// Smoke test for the hosted copy. Builds dist/, serves it on a free local port,
// checks the ledger and try-to-lie pages, then checks the NEAR testnet RPC the site calls.
// Usage: node tools/host/smoke.mjs
import { createServer } from 'node:http';
import { readFile, stat } from 'node:fs/promises';
import { join, dirname, resolve, extname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const dist = join(root, 'dist');
const types = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.css': 'text/css; charset=utf-8', '.json': 'application/json; charset=utf-8' };

const results = [];
function check(name, ok, detail = '') {
  results.push({ name, ok, detail });
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? '  (' + detail + ')' : ''}`);
}

// 1. Build a fresh dist/.
execFileSync(process.execPath, [join(root, 'tools', 'host', 'build.mjs')], { stdio: 'inherit' });

// 2. Serve dist/ on a free port.
const server = createServer(async (req, res) => {
  const url = new URL(req.url, 'http://localhost');
  let path = decodeURIComponent(url.pathname);
  if (path.endsWith('/')) path += 'index.html';
  const file = join(dist, path);
  if (!file.startsWith(dist)) { res.writeHead(403).end(); return; }
  try {
    const body = await readFile(file);
    res.writeHead(200, { 'content-type': types[extname(file)] || 'application/octet-stream' }).end(body);
  } catch {
    res.writeHead(404).end('not found');
  }
});
await new Promise((ok) => server.listen(0, '127.0.0.1', ok));
const base = `http://127.0.0.1:${server.address().port}`;

// 3. Page checks: status 200 and key elements present.
async function getPage(path) {
  const r = await fetch(base + path);
  return { status: r.status, text: await r.text() };
}
const ledger = await getPage('/ledger.html');
check('ledger.html returns 200', ledger.status === 200, `status ${ledger.status}`);
for (const id of ['id="rows"', 'id="filters"', 'src="config.js"', 'src="app.js"']) {
  check(`ledger.html has ${id}`, ledger.text.includes(id));
}

const lie = await getPage('/try.html');
check('try-to-lie try.html returns 200', lie.status === 200, `status ${lie.status}`);
for (const s of ['id="claim"', 'id="out"', 'data-p="source"', 'data-p="inject"', 'data-p="late"', 'src="config.js"', 'src="app.js"']) {
  check(`try.html has ${s}`, lie.text.includes(s));
}

const home = await getPage('/index.html');
check('index.html returns 200', home.status === 200);
for (const s of ['id="replay"', 'id="chk-run"', 'id="numbody"']) check(`index.html has ${s}`, home.text.includes(s));
const data = await getPage('/data/rounds.json');
check('saved rounds copy is served', data.status === 200 && JSON.parse(data.text).rounds.length > 30);
for (const p of ['/case.html', '/pages.html', '/page.html']) check(`${p} returns 200`, (await getPage(p)).status === 200);

// 4. Testnet RPC: same config file and same call the site makes.
const cfgSrc = await readFile(join(dist, 'config.js'), 'utf8');
const win = {};
new Function('window', cfgSrc)(win);
const CFG = win.SITE_CONFIG;
const nodes = CFG.rpcs || [CFG.rpc];
const body = {
  jsonrpc: '2.0', id: 1, method: 'query',
  params: { request_type: 'call_function', finality: 'final', account_id: CFG.contract, method_name: 'round_count', args_base64: Buffer.from('{}').toString('base64') },
};
let rpcOk = false;
for (const url of nodes) {
  try {
    const r = await fetch(url, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body), signal: AbortSignal.timeout(15000) });
    if (!r.ok) { console.log(`WARN  RPC ${url} gave HTTP ${r.status}, the site will try the next node`); continue; }
    const j = await r.json();
    if (j.error) { console.log(`WARN  RPC ${url} returned a JSON-RPC error, the site will try the next node`); continue; }
    const value = JSON.parse(Buffer.from(j.result.result).toString('utf8'));
    check(`RPC ${url} answers`, true, `round_count on ${CFG.contract} = ${value}`);
    rpcOk = true;
    break;
  } catch (e) {
    console.log(`WARN  RPC ${url} did not answer (${e.name === 'TimeoutError' ? 'timed out' : e.message}), the site will try the next node`);
  }
}
check('at least one testnet RPC node answers the site call', rpcOk);

server.close();
const failed = results.filter((r) => !r.ok).length;
console.log(`\n${results.length - failed} of ${results.length} checks passed.`);
process.exit(failed ? 1 : 0);
