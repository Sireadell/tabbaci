#!/usr/bin/env node
// Saves every round of the contract into site/data/rounds.json, so the site still shows real rulings
// when the public chain node refuses a visitor. Read only. Usage: node tools/site/snapshot.mjs
import { writeFileSync } from 'node:fs';
const argv = (k, d) => { const i = process.argv.indexOf('--' + k); return i > 0 ? process.argv[i + 1] : d; };
const CONTRACT = argv('contract', 'finalenc2.testnet');
const V2 = CONTRACT !== 'finalenc2.testnet'; // another contract saves to its own file and only adds to the shared archive copy
const OUTNAME = V2 ? 'rounds-v2.json' : 'rounds.json';
const NODES = ['https://test.rpc.fastnear.com', 'https://rpc.testnet.near.org'];
async function view(method, args = {}) {
  for (let t = 0; t < 4; t++) for (const url of NODES) {
    try {
      const r = await fetch(url, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'query', params: { request_type: 'call_function', finality: 'final', account_id: CONTRACT, method_name: method, args_base64: Buffer.from(JSON.stringify(args)).toString('base64') } }) });
      const j = await r.json();
      if (j.result && j.result.result) return JSON.parse(Buffer.from(j.result.result).toString());
    } catch {}
    await new Promise((ok) => setTimeout(ok, 700 * (t + 1)));
  }
  throw new Error('no node answered');
}
const n = await view('round_count');
const rounds = [];
for (let i = 1; i <= n; i++) { const r = await view('get_round', { round: i }); if (r) rounds.push({ id: i, ...r }); }
writeFileSync(new URL('../../site/data/' + OUTNAME, import.meta.url), JSON.stringify({ contract: CONTRACT, network: 'testnet', saved: new Date().toISOString(), count: n, rounds }));
console.log('saved', rounds.length, 'rounds');

// Saved copies of the evidence pages (public web pages) for rounds whose bytes are in archive/, plus a short quoted excerpt.
import { existsSync, readFileSync, mkdirSync, copyFileSync, readdirSync, rmSync } from 'node:fs';
import { windowOf } from '../verify/frames.mjs';
const out = new URL('../../site/data/' + OUTNAME, import.meta.url);
const saved = JSON.parse(readFileSync(out, 'utf8'));
const arch = new URL('../../archive/', import.meta.url);
const dest = new URL('../../site/data/archive/', import.meta.url);
if (!V2) rmSync(dest, { recursive: true, force: true });
mkdirSync(dest, { recursive: true });
let copied = 0;
for (const r of saved.rounds) {
  if (!r.verdict) continue;
  let v, input; try { v = JSON.parse(r.verdict); input = JSON.parse(r.input); } catch { continue; }
  r._saved = {};
  for (const s of v.s || []) {
    const hex = Buffer.from(s.h, 'base64url').toString('hex');
    const f = new URL(hex, arch);
    if (!existsSync(f)) continue;
    copyFileSync(f, new URL(hex, dest)); copied++;
    const bytes = readFileSync(f);
    r._saved[s.i] = { file: hex, excerpt: input.kind === 'prose' ? windowOf(bytes, input.terms || [], 260) : null };
  }
}
writeFileSync(out, JSON.stringify(saved));
console.log('saved archive copies:', copied);
