#!/usr/bin/env node
// One command that checks the privacy claims from public chain data only. No keys, no logins.
//
//   node tools/verify-privacy.mjs --contract privacy.sireadell.testnet --secret-owner seal.sireadell.testnet [--rpc URL]
//
// Each line prints MATCH, MISMATCH or CANNOT VERIFY (never a fake pass).
// Exit code: 0 all MATCH, 1 any MISMATCH, 2 some CANNOT VERIFY and no MISMATCH.
import { createHash } from "node:crypto";

const a = process.argv.slice(2);
const opt = (n, d) => { const i = a.indexOf(n); return i >= 0 ? a[i + 1] : d; };
const contract = opt("--contract");
const owner = opt("--secret-owner");
const rpc = opt("--rpc", "https://rpc.testnet.near.org");
const outlayer = opt("--outlayer", rpc.includes("testnet") ? "outlayer.testnet" : "outlayer.near");
if (!contract || !owner) {
  console.error("usage: node tools/verify-privacy.mjs --contract <account> --secret-owner <account> [--rpc URL]");
  process.exit(2);
}

async function q(params) {
  const r = await fetch(rpc, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: "v", method: "query", params: { finality: "final", ...params } }),
    signal: AbortSignal.timeout(25000),
  });
  if (!r.ok) throw new Error(`RPC HTTP ${r.status}`);
  const j = await r.json();
  if (j.error) throw new Error(`RPC ${j.error.cause?.name || j.error.name || "error"}`);
  if (j.result?.error) throw new Error(String(j.result.error).slice(0, 120));
  return j.result;
}
async function view(account, method, args) {
  const res = await q({ request_type: "call_function", account_id: account, method_name: method, args_base64: Buffer.from(JSON.stringify(args)).toString("base64") });
  return JSON.parse(Buffer.from(res.result).toString());
}

const rows = [];
const add = (name, verdict, detail) => rows.push({ name, verdict, detail });
async function check(name, fn) {
  try { const [v, d] = await fn(); add(name, v, d); }
  catch (e) { add(name, "CANNOT VERIFY", e.message); }
}

// wasm imports: names of the host functions the code is able to call
function wasmImports(buf) {
  if (buf.readUInt32BE(0) !== 0x0061736d) throw new Error("not a wasm file");
  let p = 8;
  const leb = () => { let r = 0, s = 0, b; do { b = buf[p++]; r |= (b & 0x7f) << s; s += 7; } while (b & 0x80); return r >>> 0; };
  const names = [];
  while (p < buf.length) {
    const id = buf[p++]; const len = leb(); const end = p + len;
    if (id === 2) {
      const n = leb();
      for (let i = 0; i < n; i++) {
        const ml = leb(); p += ml; const fl = leb(); names.push(buf.slice(p, p + fl).toString()); p += fl;
        const kind = buf[p++];
        if (kind === 0) leb(); else if (kind === 1) { p++; const f = leb(); leb(); if (f & 1) leb(); } else if (kind === 2) { const f = leb(); leb(); if (f & 1) leb(); } else if (kind === 3) { p += 2; } else throw new Error("odd import");
      }
    }
    p = end;
  }
  return names;
}

// 1. nobody can sign for the contract
await check("contract has no access keys", async () => {
  await q({ request_type: "view_account", account_id: contract });
  const k = (await q({ request_type: "view_access_key_list", account_id: contract })).keys;
  return k.length === 0 ? ["MATCH", `${contract} has 0 access keys`] : ["MISMATCH", `${contract} has ${k.length} access key(s), so its owner can still change or drain it`];
});

// 2. the code cannot upgrade itself or add keys
await check("contract code cannot deploy or add keys", async () => {
  const code = await q({ request_type: "view_code", account_id: contract });
  const buf = Buffer.from(code.code_base64, "base64");
  const hash = createHash("sha256").update(buf).digest("hex");
  const bad = wasmImports(buf).filter((n) => /deploy_contract|add_key|delete_key|stake|delete_account|create_account|transfer/.test(n));
  const bad2 = bad.filter((n) => !/transfer/.test(n));
  if (bad2.length) return ["MISMATCH", `code can call: ${bad2.join(", ")}`];
  return ["MATCH", `no deploy/key calls in code (sha256 ${hash.slice(0, 16)}...)`];
});

// 3. the program the contract runs is exactly the file it names
let programHash = null;
await check("program file matches the hash in the contract", async () => {
  const st = await q({ request_type: "view_state", account_id: contract, prefix_base64: Buffer.from("STATE").toString("base64") });
  const v = Buffer.from(st.values[0].value, "base64");
  let p = 0;
  const str = () => { const l = v.readUInt32LE(p); p += 4; const s = v.slice(p, p + l).toString(); p += l; return s; };
  const url = str(); const hash = str();
  programHash = hash;
  const f = await fetch(url, { signal: AbortSignal.timeout(30000) });
  if (!f.ok) return ["CANNOT VERIFY", `program file not reachable (HTTP ${f.status})`];
  const got = createHash("sha256").update(Buffer.from(await f.arrayBuffer())).digest("hex");
  return got === hash ? ["MATCH", `sha256 ${hash.slice(0, 16)}...`] : ["MISMATCH", `contract names ${hash.slice(0, 16)}... but the file is ${got.slice(0, 16)}...`];
});

// 4. the secret can only be read by that program, called through the contract
await check("secret readable only by that program via the contract", async () => {
  if (!programHash) return ["CANNOT VERIFY", "program hash unknown"];
  const rows2 = await view(outlayer, "list_user_secrets", { account_id: owner });
  const row = rows2.find((r) => r.accessor?.WasmHash?.hash === programHash);
  if (!row) return ["MISMATCH", `${owner} has no secret row for program ${programHash.slice(0, 16)}...`];
  const w = row.access?.Predecessor?.condition?.Whitelist?.accounts;
  if (Array.isArray(w) && w.length === 1 && w[0] === contract) return ["MATCH", `only calls from ${contract} are admitted`];
  return ["MISMATCH", `access rule is ${JSON.stringify(row.access).slice(0, 140)}`];
});

// 5. the secret owner cannot widen that rule later
await check("secret owner has no access keys", async () => {
  await q({ request_type: "view_account", account_id: owner });
  const k = (await q({ request_type: "view_access_key_list", account_id: owner })).keys;
  return k.length === 0 ? ["MATCH", `${owner} has 0 access keys`] : ["MISMATCH", `${owner} has ${k.length} access key(s), so the rule could still be changed`];
});

// 6. things this command cannot check yet
add("judge replies come from the named models", "CANNOT VERIFY", "needs the paid NEAR AI credit and the judge step; not built in this test");

const w = Math.max(...rows.map((r) => r.name.length));
for (const r of rows) console.log(`${r.verdict.padEnd(13)} ${r.name.padEnd(w)}  ${r.detail}`);
const bad = rows.some((r) => r.verdict === "MISMATCH");
const unk = rows.some((r) => r.verdict === "CANNOT VERIFY");
console.log(bad ? "RESULT: MISMATCH" : unk ? "RESULT: some lines CANNOT VERIFY" : "RESULT: MATCH");
process.exit(bad ? 1 : unk ? 2 : 0);
