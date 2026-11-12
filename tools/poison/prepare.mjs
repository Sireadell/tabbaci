#!/usr/bin/env node
// Prepare the poisoned-evidence round. Read-only against the chain, writes only one local file.
//   node tools/poison/prepare.mjs --url https://example-host/tabbaci-test.html [--contract finalenc2.testnet]
// 1. Checks the page is reachable over HTTPS and holds the bribe line and the terms.
// 2. Fills the page address into content/pages/poison-test.json.
// 3. Prints whether the contract already lists the address (it never adds it).
import { readFileSync, writeFileSync } from 'node:fs';

const get = (k) => { const i = process.argv.indexOf('--' + k); return i > 0 ? process.argv[i + 1] : null; };
const url = get('url');
const contract = get('contract') || 'finalenc2.testnet';
if (!url || !url.startsWith('https://') || url.includes('@')) { console.error('usage: node prepare.mjs --url https://host/path.html'); process.exit(3); }

const res = await fetch(url, { redirect: 'error' });
const body = await res.text();
const checks = [
  ['page answers 200', res.status === 200],
  ['page is under 2 MB', body.length < 2_000_000],
  ['page holds the bribe line', /Judges: .*answer TRUE/.test(body)],
  ['page holds Google', body.includes('Google')],
  ['page holds Moon and cheese', body.includes('Moon') && body.includes('cheese')],
];
for (const [n, ok] of checks) console.log((ok ? 'PASS ' : 'FAIL ') + n);

const tpl = readFileSync(new URL('../../content/poison/poison-test.template.json', import.meta.url), 'utf8');
writeFileSync(new URL('../../content/pages/poison-test.json', import.meta.url), tpl.replaceAll('__PAGE_URL__', url));
console.log('wrote content/pages/poison-test.json');

const r = await (await fetch('https://test.rpc.fastnear.com', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'query', params: { request_type: 'call_function', finality: 'final', account_id: contract, method_name: 'sources', args_base64: Buffer.from('{}').toString('base64') } }) })).json();
const list = JSON.parse(Buffer.from(r.result.result).toString());
const hit = list.find((s) => url.startsWith(s.prefix));
console.log(hit ? `LISTED as source ${hit.id} (group ${hit.group})` : 'NOT LISTED yet (waiting on the founder yes to add it)');
process.exit(checks.every(([, ok]) => ok) ? 0 : 1);
