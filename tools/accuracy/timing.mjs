#!/usr/bin/env node
// Time one full 3-judge ruling with real NEAR AI judges, asked one after another the way the courtroom asks them
// (chat call, then the signed receipt). Target 60 s, hard cap 90 s.
//   node timing.mjs --key-file <file> [--stop-usd 6.5] [--runs 3]
import { writeFileSync, mkdirSync, readFileSync } from 'node:fs';
import { proseUser, requestBody, proseOutcome } from '../verify/frames.mjs';
import { ask, loadKey, Spend } from './nearai.mjs';

const arg = (k, d) => { const i = process.argv.indexOf('--' + k); return i > 0 ? process.argv[i + 1] : d; };
const models = arg('models', 'deepseek-ai/DeepSeek-V4.1-Flash-1017,Qwen/Qwen3.8-27B,Qwen/Qwen3.6-35B-A3B-FP8').split(',');
const runs = Number(arg('runs', '3'));
const key = loadKey(arg('key-file'));
const spend = new Spend(key, Number(arg('stop-usd', '6.5')));
const items = JSON.parse(readFileSync(new URL('dataset.json', import.meta.url), 'utf8'));

const out = [];
for (let n = 0; n < runs; n++) {
  const it = items[n * 2];
  const t0 = Date.now();
  const per = [];
  for (const [i, m] of models.entries()) {
    const r = await ask(key, spend, requestBody(m, proseUser(it.excerpt, it.claim), 5000 + n * 10 + i));
    per.push({ model: m, ...r });
  }
  const total = Date.now() - t0;
  const letters = per.map((p) => (p.content === 'TRUE' ? 'T' : p.content === 'FALSE' ? 'F' : 'U'));
  out.push({ claim: it.claim, truth: it.label, ruling: proseOutcome(letters)[0], totalMs: total, per });
  await spend.settle();
}
const lines = ['# Real 3-judge timing', '', `Judges asked one after another, as the courtroom does. Target 60 s, cap 90 s. Spent so far: $${spend.usd.toFixed(4)}.`, '',
  '| Run | Sentence | Ruling | Total | Each judge (answer, signed receipt) |', '|---|---|---|---|---|'];
out.forEach((o, i) => lines.push(`| ${i + 1} | ${o.claim} | ${o.ruling} (truth ${o.truth}) | ${(o.totalMs / 1000).toFixed(1)} s | ${o.per.map((p) => `${p.model.split('/')[1]}: ${p.content ?? 'HTTP ' + p.status}, ${(p.totalMs / 1000).toFixed(1)} s, ${p.signed ? (p.signedTextMatches ? 'signed, matches' : 'signed, TEXT MISMATCH') : 'no receipt'}`).join('; ')} |`));
const worst = Math.max(...out.map((o) => o.totalMs));
lines.push('', `Slowest run ${(worst / 1000).toFixed(1)} s: ${worst <= 60000 ? 'inside the 60 s target' : worst <= 90000 ? 'over 60 s target, inside 90 s cap' : 'OVER the 90 s cap'}.`);
mkdirSync(new URL('results/', import.meta.url), { recursive: true });
writeFileSync(new URL('results/timing-near.md', import.meta.url), lines.join('\n') + '\n');
writeFileSync(new URL('results/timing-near.json', import.meta.url), JSON.stringify(out.map((o) => ({ ...o, per: o.per.map(({ content, ...p }) => ({ ...p, answer: content })) })), null, 1));
console.log(lines.join('\n'));
