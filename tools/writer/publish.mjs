#!/usr/bin/env node
// Publish a page: open one round per sentence, then let the writers bond their words.
//   node publish.mjs content/pages/near-chain.json --contract court4.sireadell.testnet [--attempt]
// Uses the near CLI and the accounts already saved on this machine. Writes <page>.published.json next to the page.
import { readFileSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';

const file = process.argv[2];
const get = (k) => { const i = process.argv.indexOf('--' + k); return i > 0 ? process.argv[i + 1] : null; };
const contract = get('contract');
const doAttempt = process.argv.includes('--attempt');
if (!file || !contract) { console.error('usage: node publish.mjs <page.json> --contract <account> [--attempt]'); process.exit(3); }
const page = JSON.parse(readFileSync(file, 'utf8'));

function call(method, args, signer, deposit = '0', gas = '300 Tgas') {
  const r = spawnSync('near', ['contract', 'call-function', 'as-transaction', contract, method, 'json-args', JSON.stringify(args), 'prepaid-gas', gas, 'attached-deposit', `${deposit} NEAR`, 'sign-as', signer, 'network-config', 'testnet', 'sign-with-legacy-keychain', 'send'], { encoding: 'utf8' });
  const out = (r.stdout || '') + (r.stderr || '');
  return { ok: r.status === 0, out: out.replace(/\x1b\[[0-9;]*m/g, '') };
}

const published = [];
for (const s of page.sentences) {
  const open = call('open_round', { claim: s.claim, kind: s.kind, cv: s.cv, picks: s.picks, terms: s.terms, tag: 'page' }, s.writer, '0', '100 Tgas');
  const m = open.out.match(/"round":(\d+)/);
  if (!open.ok || !m) { console.error('open failed:', open.out.slice(-300)); process.exit(1); }
  const round = Number(m[1]);
  const bonds = [{ writer: s.writer, word: s.word }, ...(s.also_bond || [])];
  for (const b of bonds) {
    const r = call('bond', { round, word: b.word }, b.writer, '0', '50 Tgas');
    console.log(`round ${round}: ${b.writer} bonds "${b.word}" ${r.ok ? 'ok' : 'FAILED ' + r.out.slice(-160)}`);
  }
  published.push({ ...s, round });
}
writeFileSync(file.replace(/\.json$/, '.published.json'), JSON.stringify({ slug: page.slug, contract, rounds: published.map((p) => ({ round: p.round, claim: p.claim })) }, null, 2));
if (doAttempt) {
  for (const p of published) {
    const r = call('attempt', { round: p.round }, 'sireadell.testnet', '0.02');
    console.log(`round ${p.round}: attempt ${r.ok ? 'sent' : 'FAILED ' + r.out.slice(-200)}`);
  }
}
