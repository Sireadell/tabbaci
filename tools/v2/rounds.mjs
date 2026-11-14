#!/usr/bin/env node
// Open, bond and run test rounds on a court version 2 contract (testnet, play money).
//   node tools/v2/rounds.mjs --contract tabbaci-court2.testnet --account tabbaci-visitors.testnet [--finalize]
// Reads the account's key from ~/.near-credentials/testnet/<account>.json. Never prints it.
import { readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { loadKey, view, call, head } from '../../api/_lib/near.js';
import { pickTerms } from '../../api/_lib/rules.js';

const arg = (k, d) => { const i = process.argv.indexOf('--' + k); return i > 0 ? process.argv[i + 1] : d; };
const CONTRACT = arg('contract', 'tabbaci-court2.testnet');
const ACCOUNT = arg('account', 'tabbaci-visitors.testnet');
const only = arg('only', '');
const key = loadKey(JSON.parse(readFileSync(join(homedir(), '.near-credentials', 'testnet', ACCOUNT + '.json'), 'utf8')).private_key);
const WIKI = 'https://en.wikipedia.org/api/rest_v1/page/summary/';
const WD = 'https://www.wikidata.org/w/rest.php/wikibase/v1/entities/items/';
const OWN = 'https://tabbaci.vercel.app/tabbaci-test.html';

const sources = await view(CONTRACT, 'sources');
const idFor = (url) => { const s = sources.find((x) => url.startsWith(x.prefix)); if (!s) throw new Error('no approved source for ' + url); return s.id; };
const UA = { 'user-agent': 'Tabbaci/1.0 (https://tabbaci.vercel.app)' };

async function wikiPick(topic, claim) {
  const sum = await (await fetch(WIKI + encodeURIComponent(topic) + '?redirect=true', { headers: UA })).json();
  const canonical = sum.titles.canonical;
  const body = await (await fetch(WIKI + encodeURIComponent(canonical), { headers: UA })).text();
  return { canonical, qid: sum.wikibase_item, terms: pickTerms(claim, canonical, body) };
}

const CASES = {
  true: async () => {
    const claim = 'Paris is the capital of France.';
    const p = await wikiPick('Paris', claim);
    return { claim, picks: [{ id: idFor(WIKI), url: WIKI + p.canonical, x: '', tol: 0 }, { id: idFor(WD), url: `${WD}${p.qid}/descriptions/en`, x: '', tol: 0 }], terms: p.terms };
  },
  lie: async () => {
    const claim = 'The Moon orbits Earth at an average distance of about 1,000 kilometres.';
    const p = await wikiPick('Moon', claim);
    return { claim, picks: [{ id: idFor(WIKI), url: WIKI + p.canonical, x: '', tol: 0 }, { id: idFor(WD), url: `${WD}${p.qid}/descriptions/en`, x: '', tol: 0 }], terms: p.terms };
  },
  order: async () => {
    const claim = 'The Moon is made of cheese.';
    return { claim, picks: [{ id: idFor(OWN), url: OWN, x: '', tol: 0 }, { id: idFor(WIKI), url: WIKI + 'Moon', x: '', tol: 0 }], terms: ['Moon', 'cheese'] };
  },
};

const sleep = (ms) => new Promise((ok) => setTimeout(ok, ms));
const run = process.argv.includes('--finalize') ? [] : (only ? only.split(',') : ['true', 'lie', 'order']);
const opened = [];
for (const name of run) {
  const spec = await CASES[name]();
  const stake = BigInt(await view(CONTRACT, 'stake'));
  const bal = BigInt(await view(CONTRACT, 'balance', { account: ACCOUNT }));
  if (bal < stake) await call({ account: ACCOUNT, key, contract: CONTRACT, method: 'deposit', args: {}, tgas: 30, deposit: '1' });
  const open = await call({ account: ACCOUNT, key, contract: CONTRACT, method: 'open_round', args: { ...spec, kind: 'prose', cv: null, tag: 'v2-' + name }, tgas: 100 });
  const round = Number(open.value);
  await call({ account: ACCOUNT, key, contract: CONTRACT, method: 'bond', args: { round, word: 'is' }, tgas: 50 });
  console.log(`${name}: opened round ${round}: "${spec.claim}"`);
  const att = await call({ account: ACCOUNT, key, contract: CONTRACT, method: 'attempt', args: { round }, tgas: 300, deposit: '0.02', wait: 'INCLUDED' });
  console.log(`${name}: court started, tx ${att.hash}`);
  opened.push({ name, round, tx: att.hash });
}
// wait for every opened round to close, and start another run if one came back empty
for (const o of opened) {
  for (let i = 0; i < 80; i++) {
    const r = await view(CONTRACT, 'get_round', { round: o.round });
    if (r.status !== 'open') { console.log(`${o.name}: round ${o.round} ${r.status}, outcome ${r.outcome ?? r.void_reason}, runs ${r.attempts.map((a) => a.result).join(',')}`); break; }
    if (!r.pending && r.attempts.length < 3 && r.attempts.every((a) => a.result !== 'running') && r.attempts.length) {
      console.log(`${o.name}: last run ${r.attempts[r.attempts.length - 1].result}, starting another`);
      await call({ account: ACCOUNT, key, contract: CONTRACT, method: 'attempt', args: { round: o.round }, tgas: 300, deposit: '0.02', wait: 'INCLUDED' });
    }
    await sleep(5000);
  }
}
if (process.argv.includes('--finalize')) {
  const n = await view(CONTRACT, 'round_count');
  const h = await head();
  for (let id = 1; id <= n; id++) {
    const c = await view(CONTRACT, 'get_case', { round: id }).catch(() => null);
    if (!c || c.finalized || c.root !== id || h < c.window_ends) continue;
    const t = await call({ account: ACCOUNT, key, contract: CONTRACT, method: 'finalize', args: { round: id }, tgas: 100, wait: 'INCLUDED' });
    console.log(`finalized round ${id}, tx ${t.hash}`);
  }
}
