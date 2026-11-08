#!/usr/bin/env node
// Re-checks one sealed batch from public chain data. No keys, no logins.
//
//   node tools/batch-check.mjs --contract <account> --batch <id> --key <hex> [--rpc URL]
//   node tools/batch-check.mjs --file batch-fixture.json
//
// Each line prints MATCH, MISMATCH or CANNOT VERIFY. Exit code 0 all MATCH, 1 any MISMATCH,
// 2 some CANNOT VERIFY and no MISMATCH.
//
// What it does, from the stored sealed orders and the released key alone:
//   1. the key times the curve's base point equals the lock stored when the batch opened
//   2. every sealed order is opened again and equals what the contract recorded (count too)
//   3. the batch is cleared again with the same rules and equals what the contract recorded
//   4. money: matched YES shares equal matched NO shares, every cost is within its deposit
//   5. sealed writer pages and writer instructions open with the same key and match their
//      fingerprints fixed before the key existed
import { readFileSync } from "node:fs";
import { keccak256 } from "../../tools/verify/crypto.mjs";
import { clear } from "./clearing.mjs";

const P = 21888242871839275222246405745257275088696311157297823662689037894645226208583n;
const mod = (a) => ((a % P) + P) % P;
function inv(a) {
  let [r0, r1, s0, s1] = [P, mod(a), 0n, 1n];
  while (r1) { const q = r0 / r1; [r0, r1] = [r1, r0 - q * r1]; [s0, s1] = [s1, s0 - q * s1]; }
  return mod(s0);
}
const add = (p, q) => {
  if (!p) return q; if (!q) return p;
  if (p[0] === q[0]) { if (mod(p[1] + q[1]) === 0n) return null; return dbl(p); }
  const l = mod((q[1] - p[1]) * inv(q[0] - p[0]));
  const x = mod(l * l - p[0] - q[0]);
  return [x, mod(l * (p[0] - x) - p[1])];
};
function dbl(p) {
  if (!p || p[1] === 0n) return null;
  const l = mod(3n * p[0] * p[0] * inv(2n * p[1]));
  const x = mod(l * l - 2n * p[0]);
  return [x, mod(l * (p[0] - x) - p[1])];
}
export function mul(k, p) {
  let r = null, a = p;
  while (k > 0n) { if (k & 1n) r = add(r, a); a = dbl(a); k >>= 1n; }
  return r;
}
const G = [1n, 2n];
const leNum = (b) => b.reduce((acc, x, i) => acc | (BigInt(x) << (8n * BigInt(i))), 0n);
const pointLe = (b) => [leNum(b.slice(0, 32)), leNum(b.slice(32, 64))];
const be32 = (n) => Buffer.from(n.toString(16).padStart(64, "0"), "hex");
const sharedBe = (s) => Buffer.concat([be32(s[0]), be32(s[1])]);
const hex = (b) => Buffer.from(b).toString("hex");
const xor = (a, b) => Buffer.from(a.map((x, i) => x ^ b[i]));

export function openOrder(k, c1, ct) {
  const s = mul(k, pointLe(c1));
  if (!s) return null;
  const pad = keccak256(Buffer.concat([Buffer.from("order-v1"), sharedBe(s)]));
  return xor(Buffer.from(ct), Buffer.from(pad));
}

export function openWords(k, c1, ct) {
  const s = mul(k, pointLe(c1));
  if (!s) return null;
  const sh = sharedBe(s);
  const out = [];
  for (let i = 0; i * 32 < ct.length; i++) {
    const idx = Buffer.alloc(8); idx.writeBigUInt64BE(BigInt(i));
    const pad = keccak256(Buffer.concat([Buffer.from("words-v1"), sh, idx]));
    ct.slice(i * 32, i * 32 + 32).forEach((x, j) => out.push(x ^ pad[j]));
  }
  return Buffer.from(out);
}

function parseOrder(o) {
  if (o[0] !== 1 || o[1] > 1) return null;
  const price = (o[2] << 8) | o[3];
  if (price === 0 || price >= 10000) return null;
  return { side: o[1], price, amount: BigInt("0x" + hex(o.slice(4, 20))) };
}

// fx: { batch, clearing, words, instructions, key } in the shapes the contract's view methods return
export function checkBatch(fx) {
  const rows = [];
  const add2 = (n, v, d) => rows.push([n, v, d]);
  const b = fx.batch;
  if (!b) return [["batch exists", "CANNOT VERIFY", "no batch data"]];
  if (b.state !== "Opened") add2("batch is opened", b.state === "Void" ? "MATCH" : "CANNOT VERIFY", `state is ${b.state}`);
  if (b.state !== "Opened" || !fx.key) return rows;
  const k = leNum(Buffer.from(fx.key, "hex"));
  const lock = mul(k, G);
  const lockOk = lock && lock[0] === pointLe(b.lock)[0] && lock[1] === pointLe(b.lock)[1];
  add2("released key matches the lock", lockOk ? "MATCH" : "MISMATCH", lockOk ? "key times base point equals the stored lock" : "key does not open this lock");
  if (!lockOk) return rows;

  const opened = b.orders.map((o) => openOrder(k, o.c1, o.ct));
  const recs = opened.map((p) => {
    const q = p && parseOrder(p);
    return q;
  });
  const same =
    b.opened.length === b.orders.length &&
    recs.every((q, i) => {
      const r = b.opened[i];
      const dep = BigInt(b.orders[i].deposit);
      const good = q && q.amount <= dep;
      if (!good) return r.status === 1;
      return r.status === 0 && r.side === q.side && r.price_bp === q.price && BigInt(r.amount) === q.amount && r.from === b.orders[i].from;
    });
  add2("every sealed order opens to what the contract recorded", same ? "MATCH" : "MISMATCH",
    same ? `${b.orders.length} sealed, ${b.opened.length} opened` : `${b.orders.length} sealed, ${b.opened.length} recorded, or a line differs`);

  if (b.opened.length !== b.orders.length) return rows;
  if (fx.clearing) {
    const reqs = b.opened.map((r) => ({ side: r.side, limit: r.price_bp, amount: BigInt(r.amount) }));
    const mine = clear(reqs);
    const theirs = fx.clearing;
    const ok =
      mine.price_bp === theirs.price_bp &&
      mine.matched.toString() === theirs.matched &&
      mine.fills.length === theirs.fills.length &&
      mine.fills.every((f, i) => f.shares.toString() === theirs.fills[i][0] && f.cost.toString() === theirs.fills[i][1]);
    add2("clearing re-run gives the contract's price and fills", ok ? "MATCH" : "MISMATCH", ok ? `price ${mine.price_bp} bp, matched ${mine.matched}` : "re-run differs from the stored clearing");
    let yes = 0n, no = 0n, paid = 0n, within = true;
    theirs.fills.forEach((f, i) => {
      if (b.opened[i].side === 0) yes += BigInt(f[0]); else no += BigInt(f[0]);
      paid += BigInt(f[1]);
      if (BigInt(f[1]) > BigInt(b.orders[i].deposit)) within = false;
    });
    add2("YES shares equal NO shares and cost is covered", yes === no && within && paid >= BigInt(theirs.matched) ? "MATCH" : "MISMATCH", `yes ${yes}, no ${no}, collateral ${paid}`);
  } else {
    add2("clearing re-run", "CANNOT VERIFY", "no stored clearing was given");
  }

  for (const [label, list] of [["writer pages", fx.words], ["writer instructions", fx.instructions]]) {
    if (!list) continue;
    let bad = 0;
    for (const w of list) {
      const text = openWords(k, w.c1 ? Buffer.from(w.c1, "hex") : [], Buffer.from(w.ct, "hex"));
      if (!text || hex(keccak256(text)) !== w.text_hash) bad++;
    }
    add2(`${label} open and match their fingerprints`, bad ? "MISMATCH" : "MATCH", `${list.length} sealed, ${bad} wrong`);
  }
  return rows;
}

// K5: does a writer keep one hidden instruction set over time? input: [{batch, from, text_hash}]
export function brainContinuity(records) {
  const byWriter = new Map();
  for (const r of records) { if (!byWriter.has(r.from)) byWriter.set(r.from, []); byWriter.get(r.from).push(r); }
  const out = [];
  for (const [who, list] of byWriter) {
    list.sort((a, b) => a.batch - b.batch);
    const first = list[0].text_hash;
    const change = list.find((r) => r.text_hash !== first);
    out.push({ writer: who, batches: list.length, same: !change, firstChange: change ? change.batch : null, fingerprint: first });
  }
  return out;
}

// K5: the owner later shows the instructions. Does the shown text match the fingerprint fixed before the key existed?
export function checkReveal(textHashHex, shownText) {
  if (shownText == null) return ["CANNOT VERIFY", "instructions not shown by the owner"];
  return hex(keccak256(Buffer.from(shownText))) === textHashHex ? ["MATCH", "shown text has the registered fingerprint"] : ["MISMATCH", "shown text does not match the registered fingerprint"];
}

function wordsJsonFix(list) {
  return list.map((w) => ({ ...w, c1: typeof w.c1 === "string" ? w.c1 : hex(w.c1) }));
}

async function main() {
  const a = process.argv.slice(2);
  const opt = (n, d) => { const i = a.indexOf(n); return i >= 0 ? a[i + 1] : d; };
  let fx;
  if (opt("--file")) {
    fx = JSON.parse(readFileSync(opt("--file"), "utf8"));
  } else {
    const contract = opt("--contract"), id = Number(opt("--batch")), key = opt("--key");
    const rpc = opt("--rpc", "https://rpc.testnet.near.org");
    if (!contract || !id) { console.error("usage: node tools/batch-check.mjs --contract <account> --batch <id> --key <hex>  |  --file <fixture.json>"); process.exit(2); }
    const view = async (m) => {
      const r = await fetch(rpc, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ jsonrpc: "2.0", id: "v", method: "query", params: { request_type: "call_function", finality: "final", account_id: contract, method_name: m, args_base64: Buffer.from(JSON.stringify({ batch_id: id })).toString("base64") } }), signal: AbortSignal.timeout(25000) });
      const j = await r.json();
      if (j.error || j.result?.error) throw new Error(String(j.result?.error || j.error.cause?.name || "rpc error").slice(0, 100));
      return JSON.parse(Buffer.from(j.result.result).toString());
    };
    try {
      fx = { batch: await view("get_batch"), key };
      for (const [f, m] of [["clearing", "get_clearing"], ["words", "get_words"], ["instructions", "get_instructions"]]) {
        try { fx[f] = await view(m); } catch { /* an older contract may not have this view */ }
      }
    } catch (e) { console.log(`CANNOT VERIFY  chain data  ${e.message}`); process.exit(2); }
  }
  if (fx.words) fx.words = wordsJsonFix(fx.words);
  if (fx.instructions) fx.instructions = wordsJsonFix(fx.instructions);
  const rows = checkBatch(fx);
  const w = Math.max(...rows.map((r) => r[0].length));
  for (const [n, v, d] of rows) console.log(`${v.padEnd(13)} ${n.padEnd(w)}  ${d}`);
  const bad = rows.some((r) => r[1] === "MISMATCH"), unk = rows.some((r) => r[1] === "CANNOT VERIFY");
  console.log(bad ? "RESULT: MISMATCH" : unk ? "RESULT: some lines CANNOT VERIFY" : "RESULT: MATCH");
  process.exit(bad ? 1 : unk ? 2 : 0);
}

import { fileURLToPath } from "node:url";
if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) await main();
