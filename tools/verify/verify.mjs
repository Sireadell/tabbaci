#!/usr/bin/env node
// One-command checker for a courtroom ruling.
//   node verify.mjs --contract court2.sireadell.testnet --round 1 [--network testnet] [--archive dir]
//                   [--tx <attempt tx hash>] [--signer <model>=0x..] [--vectors payout-vectors.json]
// Prints one line per check: MATCH, MISMATCH or CANNOT VERIFY. Exit code 0 all match, 1 any mismatch, 2 something could not be checked.
import { readFileSync, existsSync, mkdirSync } from 'node:fs';
import { dirname } from 'node:path';
import { proofPath, sealedBoxCommand } from './proofs.mjs';
import { spawnSync } from 'node:child_process';
import { sha256, sha256hex, b64url, recoverAddress } from './crypto.mjs';
import { net, settle, settleBatchCheck } from './payout.mjs';
import { dataUser, proseUser, requestBody, canon, extract, windowOf, proseOutcome, dataOutcome } from './frames.mjs';

const args = Object.fromEntries(
  process.argv.slice(2).reduce((acc, a, i, all) => (a.startsWith('--') ? [...acc, [a.slice(2), all[i + 1] && !all[i + 1].startsWith('--') ? all[i + 1] : 'true']] : acc), []),
);
const network = args.network || 'testnet';
const RPC = args.rpc || (network === 'mainnet' ? 'https://rpc.mainnet.near.org' : 'https://rpc.testnet.near.org');

const rows = [];
const add = (status, name, detail = '') => rows.push({ status, name, detail });
const M = 'MATCH', X = 'MISMATCH', C = 'CANNOT VERIFY';

async function rpc(method, params) {
  const r = await fetch(RPC, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }) });
  const j = await r.json();
  if (j.error) throw new Error(JSON.stringify(j.error).slice(0, 200));
  return j.result;
}
async function view(contract, method, a = {}) {
  const res = await rpc('query', { request_type: 'call_function', finality: 'final', account_id: contract, method_name: method, args_base64: Buffer.from(JSON.stringify(a)).toString('base64') });
  if (res.error) throw new Error(res.error);
  return JSON.parse(Buffer.from(res.result).toString());
}

// ---- wasm export names, to prove there is no upgrade method ----
function wasmExports(buf) {
  let p = 8;
  const leb = () => { let r = 0, s = 0, b; do { b = buf[p++]; r |= (b & 0x7f) << s; s += 7; } while (b & 0x80); return r >>> 0; };
  while (p < buf.length) {
    const id = buf[p++];
    const size = leb();
    const end = p + size;
    if (id === 7) {
      const n = leb();
      const out = [];
      for (let i = 0; i < n; i++) {
        const len = leb();
        out.push(buf.subarray(p, p + len).toString());
        p += len;
        p++; // kind
        leb(); // index
      }
      return out;
    }
    p = end;
  }
  return [];
}
const FORBIDDEN = /^(upgrade|deploy|migrate|update_contract|set_owner|set_code|replace_code|add_full_access_key|add_key|delete_key|set_dao|set_resolver|set_courtroom|set_models)/i;

async function main() {
  if (args.vectors) {
    const v = JSON.parse(readFileSync(args.vectors, 'utf8'));
    const bad = v.filter((x) => net(BigInt(x.stake), x.p, x.o) !== BigInt(x.net));
    add(bad.length ? X : M, 'payout maths', `${v.length} vectors recomputed in JavaScript, ${bad.length} differ`);
    const sc = settleBatchCheck();
    add(sc.ok ? M : X, 'payout money conservation', sc.detail);
  }
  if (!args.contract || !args.round) {
    if (!args.vectors) { console.error('usage: node verify.mjs --contract <account> --round <id> [--network testnet|mainnet] [--archive dir] [--tx hash] [--signer model=0x..] [--vectors file]'); process.exit(3); }
    return finish();
  }
  const contract = args.contract;
  const round = Number(args.round);

  // 1. the contract cannot be changed
  try {
    const keys = await rpc('query', { request_type: 'view_access_key_list', finality: 'final', account_id: contract });
    add(keys.keys.length === 0 ? M : X, 'contract has no keys', `${keys.keys.length} access key(s)`);
  } catch (e) { add(C, 'contract has no keys', e.message); }
  let code = null;
  try {
    const c = await rpc('query', { request_type: 'view_code', finality: 'final', account_id: contract });
    code = Buffer.from(c.code_base64, 'base64');
    const names = wasmExports(code);
    const bad = names.filter((n) => FORBIDDEN.test(n));
    add(bad.length ? X : M, 'contract has no upgrade method', bad.length ? `found: ${bad.join(', ')}` : `${names.length} exported methods, none can replace code, keys or owners`);
    add(M, 'contract code fingerprint', `sha256 ${sha256hex(code)}`);
  } catch (e) { add(C, 'contract has no upgrade method', e.message); }

  // 2. the courtroom program fingerprint
  let courtroomHash = null;
  try {
    const [url, hash] = await view(contract, 'courtroom');
    courtroomHash = hash;
    const r = await fetch(url);
    if (!r.ok) add(C, 'courtroom program matches its fingerprint', `could not download ${url}`);
    else {
      const got = sha256hex(Buffer.from(await r.arrayBuffer()));
      add(got === hash ? M : X, 'courtroom program matches its fingerprint', `contract says ${hash.slice(0, 12)}..., file is ${got.slice(0, 12)}...`);
    }
  } catch (e) { add(C, 'courtroom program matches its fingerprint', e.message); }

  // 3. the round
  let r;
  try { r = await view(contract, 'get_round', { round }); } catch (e) { add(C, 'read the round', e.message); return finish(); }
  if (!r) { add(X, 'read the round', `no round ${round} on ${contract}`); return finish(); }
  const input = JSON.parse(r.input);
  add(b64url(sha256(Buffer.from(r.input))) === r.ih ? M : X, 'stored request matches its hash', `ih ${r.ih.slice(0, 12)}...`);
  add(sha256hex(Buffer.from(r.claim)) === input.th && r.claim === input.claim ? M : X, 'sentence text matches the stored request');

  // 4. attempts
  const n = r.attempts.length;
  const fails = r.attempts.filter((a) => a.result !== 'ok' && a.result !== 'running' && a.result !== 'SRC_DISAGREE').length;
  let okShape = n <= 3 && (r.status !== 'void' || r.void_reason === 'SRC_DISAGREE' || fails === 3) && (r.status !== 'settled' || r.attempts.some((a) => a.result === 'ok'));
  add(okShape ? M : X, 'attempt list is consistent', `${n} attempt(s) published: ${r.attempts.map((a) => `#${a.n} ${a.result} in ${a.end_block ? a.end_block - a.start_block : '?'} blocks`).join(', ')}; round is ${r.status}${r.void_reason ? ' (' + r.void_reason + ')' : ''}`);

  // bonds: recompute every payout with the fixed rule
  if (r.bonds && r.bonds.length) {
    const WORDS = { is: 9500, 'very likely': 8000, likely: 6500, disputed: 5000 };
    const outcome = r.outcome === 'T' || r.outcome === 'F' ? r.outcome : null;
    if (r.status === 'open') add(C, 'bond payouts follow the rule', 'round is still open');
    else if (r.reserve_before == null) add(C, 'bond payouts follow the rule', 'this contract did not record the reserve at settlement');
    else {
      const entries = r.bonds.map((b) => ({ stake: BigInt(b.stake), chance: WORDS[b.word] }));
      const want = settle(entries, outcome, BigInt(r.reserve_before));
      const got = r.bonds.map((b) => b.payout == null ? null : BigInt(b.payout));
      const same = got.every((g, i) => g === want.payouts[i]);
      add(same ? M : X, 'bond payouts follow the rule', r.bonds.map((b, i) => `${b.word} ${(want.payouts[i] >= entries[i].stake ? '+' : '-')}${(want.payouts[i] > entries[i].stake ? want.payouts[i] - entries[i].stake : entries[i].stake - want.payouts[i]) * 10000n / entries[i].stake}bp`).join(', '));
    }
  }
  if (r.status === 'open') { add(C, 'ruling', 'round is still open, nothing to check yet'); return finish(); }
  if (!r.verdict) { add(r.status === 'void' && r.void_reason === '3_FAILED_RUNS' ? M : X, 'verdict present', r.status === 'void' ? 'void after failed runs, no verdict expected' : 'settled round has no verdict'); return finish(); }

  // 5. the verdict
  let v;
  try { v = JSON.parse(r.verdict); } catch { add(X, 'verdict is valid JSON'); return finish(); }
  add(Buffer.byteLength(r.verdict) <= 900 && v.v === 1 && v.rd === round && v.ih === r.ih ? M : X, 'verdict belongs to this round and request', `${Buffer.byteLength(r.verdict)} bytes, o=${v.o}`);
  add(v.o === r.outcome || (v.o === 'V' && r.status === 'void') ? M : X, 'contract outcome equals the verdict', `verdict ${v.o}, contract ${r.outcome ?? r.void_reason}`);

  // 6. sources
  const archive = args.archive;
  const bodies = [];
  for (let i = 0; i < (v.s || []).length; i++) {
    const s = v.s[i];
    const file = archive ? `${archive}/${Buffer.from(s.h, 'base64url').toString('hex')}` : null;
    if (file && existsSync(file)) {
      const bytes = readFileSync(file);
      bodies.push(bytes);
      add(b64url(sha256(bytes)) === s.h ? M : X, `source ${s.i} bytes match their fingerprint`, `HTTP ${s.c}`);
    } else {
      bodies.push(null);
      add(C, `source ${s.i} bytes match their fingerprint`, archive ? 'archived bytes missing' : 'no --archive folder given');
    }
  }
  let numbersTrue = null;
  if (input.kind === 'data' && bodies.every(Boolean)) {
    const vals = bodies.map((b, i) => extract(b.toString('utf8'), input.src[i].x));
    const nums = vals.map(Number);
    const tol = Math.max(...input.src.map((x) => x.tol));
    const agree = nums.every((x) => Math.abs(x - nums[0]) <= tol);
    const shown = vals.every((val, i) => (val ?? '').slice(0, 24) === v.s[i].x);
    add(shown ? M : X, 'extracted values match the verdict', vals.join(' and '));
    if (v.o === 'V') add(!agree ? M : X, 'sources really disagree', vals.join(' vs '));
    else numbersTrue = Math.abs(nums[0] - Number(input.cv)) <= tol;
    if (v.o !== 'V') add(agree ? M : X, 'sources agree within tolerance', `tolerance ${tol}`);
  }

  // 7. judges: rebuild each request, check each signature
  if ((v.j || []).length) {
    let user = null;
    if (input.kind === 'data') user = dataUser(input.claim, canon(Number(input.cv)));
    else if (bodies.length && bodies.every(Boolean)) {
      const parts = bodies.map((b) => windowOf(b, input.terms || [])).filter(Boolean);
      user = parts.length ? proseUser(parts.join('\n---\n'), input.claim) : null;
    }
    const signers = {};
    for (const [k, val] of Object.entries(Object.fromEntries((args.signer ? [args.signer] : []).map((x) => x.split('='))))) signers[k] = val.toLowerCase();
    for (const j of v.j) {
      const model = input.m[j.m];
      if (j.a === 'X') { add(M, `judge ${j.m} (${model}) was not counted`, 'unsigned or failed reply is treated as UNSURE'); continue; }
      if (!user) { add(C, `judge ${j.m} (${model}) signature`, 'cannot rebuild the question without the archived sources'); continue; }
      const rh = sha256hex(Buffer.from(requestBody(model, user, input.seed)));
      const text = `${model}:${rh}:${Buffer.from(j.h, 'base64url').toString('hex')}`;
      const addr = recoverAddress(text, Buffer.from(j.g, 'base64url'));
      if (!addr) { add(X, `judge ${j.m} (${model}) signature`, 'does not recover to any signer'); continue; }
      const want = signers[model.toLowerCase()];
      if (want) add(addr === want ? M : X, `judge ${j.m} (${model}) signed by the expected key`, addr);
      else add(C, `judge ${j.m} (${model}) signed by the model's attested key`, `valid signature from ${addr}; give --signer "${model}=<attested address>" to compare`);
    }
    // 8. outcome recomputed from the answers
    const answers = v.j.map((j) => (j.a === 'X' ? 'U' : j.a));
    if (input.kind === 'prose') {
      const [o, w] = proseOutcome(answers);
      add(o === v.o && !!w === !!v.w ? M : X, 'outcome follows from the judges', `answers ${answers.join('')} give ${o}${w ? ' (2 of 3)' : ''}`);
    } else if (numbersTrue !== null) {
      const [o, w] = dataOutcome(answers, numbersTrue);
      add(o === v.o && !!w === !!v.w ? M : X, 'outcome follows from the numbers and judges', `answers ${answers.join('')}, numbers say ${numbersTrue} give ${o}`);
    } else add(C, 'outcome follows from the numbers and judges', 'needs the archived sources');
    add(C, 'judge reply words match the answers', 'reply bytes are not published, only their fingerprint');
  }

  // 9. the sealed box run itself
  const savedPath = args.proof || proofPath(args.contract, args.round);
  const cmd = sealedBoxCommand({ tx: args.tx, bundle: savedPath, network, save: args['save-proof'] === 'true', savePath: savedPath });
  if (cmd) {
    if (cmd.args[0] === 'tx' && args['save-proof'] === 'true') mkdirSync(dirname(savedPath), { recursive: true });
    const t = spawnSync('outlayer-verify', cmd.args, { encoding: 'utf8' });
    if (t.error) add(C, 'sealed-box attestation (outlayer-verify)', 'outlayer-verify is not installed on this machine');
    else add(t.status === 0 ? M : t.status === 1 ? X : C, 'sealed-box attestation (outlayer-verify)', `${(t.stdout || t.stderr || '').trim().split('\n').slice(-1)[0]} [${cmd.source}]`);
  } else add(C, 'sealed-box attestation (outlayer-verify)', `no saved proof at ${savedPath}; pass --tx <attempt transaction hash> (add --save-proof to keep it) and install outlayer-verify`);
  return finish();
}

function finish() {
  const w = Math.max(...rows.map((r) => r.name.length));
  for (const r of rows) console.log(`${r.status.padEnd(13)} ${r.name.padEnd(w)}  ${r.detail}`);
  const any = (s) => rows.some((r) => r.status === s);
  const overall = any(X) ? X : any(C) ? C : M;
  console.log(`\nOVERALL: ${overall}`);
  process.exit(overall === M ? 0 : overall === X ? 1 : 2);
}

main().catch((e) => { console.error('checker error:', e.message); process.exit(3); });
