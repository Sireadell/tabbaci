#!/usr/bin/env node
// Before a page goes on chain, check that every sentence on it can be checked.
//   node tools/writer/check.mjs content/pages/<page>.json --contract court5.sireadell.testnet
// For each sentence it does what the courtroom does before the judges are asked, with the same rules:
//   every source is HTTPS, under 2 MB, answers 200, and is on the contract's approved list;
//   the sources come from at least two different groups;
//   number sentences: every source gives a number, the numbers agree within the tolerance,
//     and the result (true or false) matches the "expect" field when the page sets one;
//   text sentences: a key word is found in EVERY source (the courtroom only needs one),
//     and the piece of text the judges would read is printed so a person can read it too.
// Exit 0 when every sentence passes, 1 when any fails.
import { readFileSync } from 'node:fs';

const file = process.argv[2];
const get = (k) => { const i = process.argv.indexOf('--' + k); return i > 0 ? process.argv[i + 1] : null; };
const contract = get('contract');
const rpcUrl = get('rpc') || 'https://rpc.testnet.near.org';
if (!file || !contract) { console.error('usage: node check.mjs <page.json> --contract <account>'); process.exit(3); }
const page = JSON.parse(readFileSync(file, 'utf8'));

const MAX_BYTES = 2 * 1024 * 1024;
const UA = 'courtroom-fetch/0.1'; // the courtroom sends this exact header
const view = async (method, args) => {
  const r = await fetch(rpcUrl, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'query', params: { request_type: 'call_function', finality: 'final', account_id: contract, method_name: method, args_base64: Buffer.from(JSON.stringify(args)).toString('base64') } }) });
  const j = await r.json();
  return JSON.parse(Buffer.from(j.result.result).toString());
};
const list = (await view('sources', {})).filter((s) => s.removed_at === null);

// Same extractors as programs/court/src/extract.rs.
function jsonPath(v, path) {
  if (!path.startsWith('$')) return undefined;
  let rest = path.slice(1), cur = v;
  while (rest) {
    if (rest[0] === '.') { const r = rest.slice(1); const end = r.search(/[.[]/) < 0 ? r.length : r.search(/[.[]/); const key = r.slice(0, end); if (!key || cur == null) return undefined; cur = cur[key]; rest = r.slice(end); }
    else if (rest[0] === '[') { const end = rest.indexOf(']'); const idx = Number(rest.slice(1, end)); if (end < 0 || !Array.isArray(cur)) return undefined; cur = cur[idx]; rest = rest.slice(end + 1); }
    else return undefined;
    if (cur === undefined) return undefined;
  }
  return cur;
}
function extract(body, ex) {
  if (ex.startsWith('json:')) {
    let v; try { v = JSON.parse(body); } catch { return null; }
    const got = jsonPath(v, ex.slice(5));
    return typeof got === 'string' || typeof got === 'number' || typeof got === 'boolean' ? String(got) : null;
  }
  if (ex.startsWith('find:')) {
    const rest = ex.slice(5); const bar = rest.indexOf('|'); if (bar < 0) return null;
    const pre = rest.slice(0, bar), suf = rest.slice(bar + 1);
    const a = body.indexOf(pre); if (a < 0) return null;
    const start = a + pre.length; const b = body.indexOf(suf, start); if (b < 0) return null;
    const t = body.slice(start, b).trim();
    return t && t.length <= 200 ? t : null;
  }
  return null;
}
function windowOf(body, terms, width = 300) {
  const lower = body.toLowerCase();
  for (const t of terms) { const pos = lower.indexOf(t.toLowerCase()); if (pos >= 0) return body.slice(Math.max(0, pos - width / 2), pos + t.length + width / 2); }
  return null;
}
const num = (s) => (/^[+-]?\d+(\.\d+)?$/.test(s) ? Number(s) : null);

let failed = 0;
for (const [n, s] of page.sentences.entries()) {
  const problems = [];
  const groups = new Set();
  const got = [];
  for (const p of s.picks) {
    const src = list.find((x) => p.url.startsWith(x.prefix));
    if (!p.url.startsWith('https://')) problems.push(`not HTTPS: ${p.url}`);
    if (!src) { problems.push(`not on the approved list: ${p.url}`); continue; }
    groups.add(src.group);
    let status = 0, body = '';
    try {
      const r = await fetch(p.url, { headers: { 'User-Agent': UA, Accept: 'application/json, text/html, text/plain' }, redirect: 'manual', signal: AbortSignal.timeout(8000) });
      status = r.status; const bytes = Buffer.from(await r.arrayBuffer());
      if (bytes.length > MAX_BYTES) problems.push(`over 2 MB: ${p.url}`);
      body = bytes.toString('utf8');
    } catch (e) { problems.push(`could not fetch ${p.url}: ${e.message}`); continue; }
    if (status !== 200) { problems.push(`answered ${status}: ${p.url}`); continue; }
    if (s.kind === 'data') {
      const v = extract(body, p.x);
      if (v === null || num(v) === null) problems.push(`no number at ${p.x} in ${p.url} (got ${JSON.stringify(v)})`);
      got.push({ group: src.group, v });
    } else {
      const w = windowOf(body, s.terms);
      if (!w) problems.push(`none of ${JSON.stringify(s.terms)} found in ${p.url}`);
      got.push({ group: src.group, w });
    }
  }
  if (groups.size < 2) problems.push(`needs two source groups, has ${groups.size}`);
  let result = '';
  if (s.kind === 'data' && !problems.length) {
    const nums = got.map((g) => num(g.v));
    const tol = Math.max(...s.picks.map((p) => p.tol));
    if (nums.some((x) => Math.abs(x - nums[0]) > tol)) result = 'void: sources disagree';
    else result = Math.abs(nums[0] - num(s.cv)) <= tol ? 'true' : 'false';
    if (s.expect && result !== { T: 'true', F: 'false' }[s.expect]) problems.push(`would rule ${result}, page expects ${s.expect}`);
  }
  if (s.kind === 'prose' && !problems.length) result = 'goes to the judges';
  const ok = problems.length === 0;
  if (!ok) failed++;
  console.log(`\n${ok ? 'PASS' : 'FAIL'}  ${n + 1}. [${s.word}] ${s.claim}`);
  console.log(`      groups: ${[...groups].join(', ')}${result ? '  ->  ' + result : ''}`);
  for (const g of got) console.log(s.kind === 'data' ? `      ${g.group}: ${g.v}` : `      ${g.group}: ...${(g.w || '').replace(/\s+/g, ' ').trim()}...`);
  for (const p of problems) console.log(`      problem: ${p}`);
}
console.log(`\n${page.sentences.length - failed} of ${page.sentences.length} sentences can be checked.`);
process.exit(failed ? 1 : 0);
