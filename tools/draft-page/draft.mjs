#!/usr/bin/env node
// Have one NEAR AI model draft a short page from the sentences the court ruled true.
//   node tools/draft-page/draft.mjs --key-file <path> [--model Qwen/Qwen3.8-27B] [--stop-usd 6.9]
// The model only sees the ruled sentences. The script rejects a draft that contains a number or a
// round reference that is not in those sentences, then saves content/drafted/near-sentences.json.
// The key is read from the file and never printed. Spend is metered by tools/accuracy/nearai.mjs.
import { writeFileSync, mkdirSync } from 'node:fs';
import { createClient } from '../../packages/tabbaci-mcp/src/index.mjs';
import { BASE, loadKey, Spend } from '../accuracy/nearai.mjs';

const arg = (k, d) => { const i = process.argv.indexOf('--' + k); return i > 0 ? process.argv[i + 1] : d; };
const MODEL = arg('model', 'Qwen/Qwen3.8-27B');
const KEY = loadKey(arg('key-file', null));
const STOP = Number(arg('stop-usd', '6.9'));

const client = createClient();
const all = await client.listRounds();
// Finalized true rulings only. Round 33 is the challenge re-run of case 32, so it is left out.
const rows = [];
for (const r of all.sort((a, b) => a.round - b.round)) {
  if (r.outcome !== 'T' || r.round === 33) continue;
  const full = await client.getRuling(r.round, { includeTx: false });
  if (full.fine_paid !== true) continue;
  rows.push({ round: r.round, sentence: full.sentence });
}
if (rows.length < 5) throw new Error(`only ${rows.length} finalized true sentences found`);

const list = rows.map((r) => `[round ${r.round}] ${r.sentence}`).join('\n');
const system = 'You write short, plain, factual pages. You use only the sentences you are given. You never add a fact, a number, a name or a date that is not in them.';
const user = `Below are sentences that an on-chain court ruled TRUE. Write a short page for a curious reader.

Rules:
- Start with one line: the page title, prefixed "TITLE: ".
- Then 3 short paragraphs, 120 to 180 words in total, plain words, no hype.
- Use only facts from the sentences. Copy every number exactly as written.
- After each fact you use, add its round in brackets, like [round 5].
- Do not mention the court, money, AI or models. Do not say what is not covered.

Sentences:
${list}`;

const spend = new Spend(KEY, STOP);
const allowedNumbers = new Set((list.match(/\d+(?:\.\d+)?/g) || []));
const allowedRounds = new Set(rows.map((r) => String(r.round)));

function problems(text) {
  const out = [];
  const body = text.replace(/\[round \d+\]/g, '');
  for (const n of body.match(/\d+(?:\.\d+)?/g) || []) if (!allowedNumbers.has(n)) out.push(`number ${n} is not in the sentences`);
  for (const m of text.matchAll(/\[round (\d+)\]/g)) if (!allowedRounds.has(m[1])) out.push(`round ${m[1]} is not one of the sentences`);
  if (!/^TITLE: .{5,120}$/m.test(text)) out.push('no TITLE line');
  const words = text.replace(/^TITLE: .*$/m, '').trim().split(/\s+/).length;
  if (words < 90 || words > 260) out.push(`${words} words`);
  return out;
}

let draft = null;
let lastProblems = [];
for (let attempt = 1; attempt <= 3 && !draft; attempt++) {
  spend.check();
  let res;
  try { res = await fetch(`${BASE}/v1/chat/completions`, {
    method: 'POST',
    headers: { authorization: `Bearer ${KEY}`, 'content-type': 'application/json', 'x-no-aliasing': 'true', 'accept-encoding': 'identity' },
    body: JSON.stringify({ model: MODEL, messages: [{ role: 'system', content: system }, { role: 'user', content: user }], max_tokens: 1500, temperature: 0.2, reasoning_effort: 'none' }),
    signal: AbortSignal.timeout(90000),
  }); } catch (e) { console.error(`attempt ${attempt}: ${e.name}`); continue; }
  spend.add(res.headers.get('inference-id'));
  if (!res.ok) { console.error(`attempt ${attempt}: HTTP ${res.status}`); continue; }
  const j = await res.json();
  const text = String(j.choices?.[0]?.message?.content || '').replace(/<think>[\s\S]*?<\/think>/g, '').trim();
  lastProblems = problems(text);
  if (lastProblems.length) console.log('finish:', j.choices?.[0]?.finish_reason, 'chars:', text.length);
  console.log(`attempt ${attempt}: ${lastProblems.length ? 'rejected: ' + lastProblems.join('; ') : 'accepted'}`);
  if (!lastProblems.length) draft = text;
}
await spend.settle();
console.log(`spent so far on this ledger: $${spend.usd.toFixed(4)}`);
if (!draft) { console.error('no acceptable draft'); process.exit(1); }

const title = draft.match(/^TITLE: (.*)$/m)[1].trim();
const text = draft.replace(/^TITLE: .*$/m, '').trim();
mkdirSync(new URL('../../content/drafted/', import.meta.url), { recursive: true });
writeFileSync(new URL('../../content/drafted/near-sentences.json', import.meta.url), JSON.stringify({
  slug: 'near-sentences',
  title,
  text,
  drafted_by: MODEL,
  drafted_on: new Date().toISOString().slice(0, 10),
  label: `This page was drafted by the NEAR AI model ${MODEL} from the sentences below. Every sentence is one the court ruled true, and the script that made it rejects a draft that holds a number or a round that is not in them. Read it beside the rulings, not instead of them: the draft leaves out some dates that the rulings state, and I did not edit its wording.`,
  from_rounds: rows.map((r) => r.round),
  sentences: rows,
}, null, 2) + '\n');
console.log('saved content/drafted/near-sentences.json');
