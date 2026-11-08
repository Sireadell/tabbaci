#!/usr/bin/env node
// Save the exact bytes a ruling fetched, named by their sha256, so anyone can re-check them.
//   node archive.mjs --contract <account> --round <id> --out <folder> [--network testnet|mainnet]
// If a page has changed since the ruling, its hash will not match; that is printed as PAGE CHANGED and nothing is saved.
import { mkdirSync, writeFileSync, existsSync } from 'node:fs';
import { createHash } from 'node:crypto';

const a = Object.fromEntries(process.argv.slice(2).reduce((acc, x, i, all) => (x.startsWith('--') ? [...acc, [x.slice(2), all[i + 1]]] : acc), []));
const rpcUrl = a.rpc || (a.network === 'mainnet' ? 'https://rpc.mainnet.near.org' : 'https://rpc.testnet.near.org');
if (!a.contract || !a.round || !a.out) { console.error('usage: node archive.mjs --contract <account> --round <id> --out <folder>'); process.exit(3); }

const view = async (method, args) => {
  const r = await fetch(rpcUrl, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'query', params: { request_type: 'call_function', finality: 'final', account_id: a.contract, method_name: method, args_base64: Buffer.from(JSON.stringify(args)).toString('base64') } }) });
  const j = await r.json();
  return JSON.parse(Buffer.from(j.result.result).toString());
};
const round = await view('get_round', { round: Number(a.round) });
if (!round || !round.verdict) { console.error('no verdict for that round'); process.exit(2); }
const v = JSON.parse(round.verdict);
const input = JSON.parse(round.input);
mkdirSync(a.out, { recursive: true });
let changed = 0;
for (const s of v.s || []) {
  const url = input.src.find((x) => x.i === s.i).u;
  const bytes = Buffer.from(await (await fetch(url, { headers: { 'User-Agent': 'courtroom-fetch/0.1' } })).arrayBuffer());
  const h = createHash('sha256').update(bytes).digest('base64url');
  const hex = createHash('sha256').update(bytes).digest('hex');
  if (h === s.h) {
    if (!existsSync(`${a.out}/${hex}`)) writeFileSync(`${a.out}/${hex}`, bytes);
    console.log(`saved   ${url}  ${hex}`);
  } else {
    changed++;
    console.log(`PAGE CHANGED SINCE RULING  ${url}  (ruling saw ${Buffer.from(s.h, 'base64url').toString('hex').slice(0, 12)}..., now ${hex.slice(0, 12)}...)`);
  }
}
process.exit(changed ? 1 : 0);
