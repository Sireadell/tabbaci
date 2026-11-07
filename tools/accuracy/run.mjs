#!/usr/bin/env node
// Ask the judges the same question the courtroom asks, for every sentence in dataset.json, and score the rulings.
//   node run.mjs --judge standin                       checks the plumbing with a fake judge (no network, no cost)
//   node run.mjs --judge near --key-file <path>        real judges on NEAR AI Cloud (needs credit)
// Scores each model alone and the 3-judge ruling. Wrong rulings (says true when false, or false when true) are the number that matters.
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { proseUser, requestBody, proseOutcome } from '../verify/frames.mjs';

const arg = (k, d) => { const i = process.argv.indexOf('--' + k); return i > 0 ? process.argv[i + 1] : d; };
const judge = arg('judge', 'standin');
const models = (arg('models', 'deepseek-ai/DeepSeek-V4.1-Flash-1017,Qwen/Qwen3.8-27B,Qwen/Qwen3.6-35B-A3B-FP8')).split(',');
const limit = Number(arg('limit', '200'));
const items = JSON.parse(readFileSync(new URL('dataset.json', import.meta.url), 'utf8')).slice(0, limit);
const key = judge === 'near' ? readFileSync(arg('key-file'), 'utf8').trim() : null;

async function ask(model, item, i) {
  if (judge === 'standin') return item.excerpt.includes(item.claim.replace(/^.* is /, '').replace(/\.$/, '')) ? 'TRUE' : 'FALSE';
  const body = requestBody(model, proseUser(item.excerpt, item.claim), 1000 + i);
  const r = await fetch('https://cloud-api.near.ai/v1/chat/completions', {
    method: 'POST',
    headers: { authorization: `Bearer ${key}`, 'content-type': 'application/json', 'x-no-aliasing': 'true', 'accept-encoding': 'identity' },
    body,
  });
  if (!r.ok) return 'ERROR';
  const j = await r.json();
  return (j.choices?.[0]?.message?.content || '').trim();
}
const letter = (w) => (w === 'TRUE' ? 'T' : w === 'FALSE' ? 'F' : 'U');

const per = Object.fromEntries(models.map((m) => [m, { right: 0, wrong: 0, unsure: 0 }]));
const three = { right: 0, wrong: 0, none: 0 };
const misses = [];
for (let i = 0; i < items.length; i++) {
  const it = items[i];
  const answers = [];
  for (const m of models) {
    const a = letter(await ask(m, it, i));
    answers.push(a);
    const s = per[m];
    if (a === 'U') s.unsure++; else if (a === it.label) s.right++; else s.wrong++;
  }
  const [o] = proseOutcome(answers);
  if (o === 'D' || o === 'U') three.none++;
  else if (o === it.label) three.right++;
  else { three.wrong++; misses.push({ id: it.id, claim: it.claim, label: it.label, answers: answers.join('') }); }
}
const n = items.length;
const pct = (x) => `${((x / n) * 100).toFixed(1)}%`;
const lines = [`# Accuracy run (${judge}), ${n} sentences`, '', '| Who | Right | Wrong | Unsure |', '|---|---|---|---|'];
for (const m of models) lines.push(`| ${m} | ${pct(per[m].right)} | ${pct(per[m].wrong)} | ${pct(per[m].unsure)} |`);
lines.push(`| 3-judge ruling | ${pct(three.right)} | ${pct(three.wrong)} | ${pct(three.none)} (no ruling) |`, '');
if (misses.length) lines.push('Wrong rulings:', ...misses.map((x) => `- ${x.id} ${x.claim} (truth ${x.label}, answers ${x.answers})`));
mkdirSync(new URL('results/', import.meta.url), { recursive: true });
writeFileSync(new URL(`results/accuracy-${judge}.md`, import.meta.url), lines.join('\n') + '\n');
writeFileSync(new URL(`results/accuracy-${judge}.json`, import.meta.url), JSON.stringify({ judge, n, per, three, misses }, null, 1));
console.log(lines.join('\n'));
