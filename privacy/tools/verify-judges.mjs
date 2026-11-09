#!/usr/bin/env node
// Checks that the AI judges really run inside sealed, Intel-signed chips, from public data.
//
//   node tools/verify-judges.mjs [--models a,b,c] [--call] [--key-file PATH] [--json]
//
// Run `npm install --prefix tools` once first (it fetches the Intel quote checker).
// Each line prints MATCH, MISMATCH or CANNOT VERIFY (never a fake pass).
// Exit code: 0 all MATCH, 1 any MISMATCH, 2 some CANNOT VERIFY and no MISMATCH.
//
// For every model it asks NEAR AI Cloud for a fresh attestation report with a random 32 byte number
// (a nonce), then checks, on every candidate report:
//   1. the report echoes our nonce
//   2. the quote is an Intel TDX version 4 quote and debug mode is off
//   3. the quote's report data is the model's ed25519 signing key, then our nonce
//   4. the quote's config id is 01, sha256(app_compose), 15 zero bytes
//   5. replaying the runtime event log gives the quote's RTMR3
//   6. Intel's signature chain on the quote is valid and the chip's security level is acceptable
// With --call it also sends one tiny question to each model and checks the signature on the reply:
//   7. the signature is ed25519 over "<model>:<sha256 of request bytes>:<sha256 of reply bytes>"
//   8. the signer key is the key of exactly one verified report from step 1-6
// Needs a NEAR AI Cloud key only for --call (env NEARAI_API_KEY, or --key-file). The key is never printed.
import { createHash, randomBytes } from "node:crypto";
import { readFileSync } from "node:fs";
import { checkReportBytes, edVerifyText } from "./judge-checks.mjs";

const args = process.argv.slice(2);
const opt = (n, d) => { const i = args.indexOf(n); return i >= 0 ? args[i + 1] : d; };
const flag = (n) => args.includes(n);
const BASE = opt("--base", "https://cloud-api.near.ai");
const MODELS = (opt("--models", "deepseek-ai/DeepSeek-V4.1-Flash-1017,Qwen/Qwen3.8-27B,Qwen/Qwen3.6-35B-A3B-FP8")).split(",").map((s) => s.trim()).filter(Boolean);
const CALL = flag("--call");
const ALGO = opt("--algo", "ed25519"); // the courtroom signs with ecdsa: pass --algo ecdsa to print the addresses its rulings should recover to
const ACCEPTED_TCB = ["UpToDate", "SWHardeningNeeded"]; // Intel's own words for "fully patched" and "patched, the app must also harden"

const sha256 = (b) => createHash("sha256").update(b).digest();
const hex = (b) => Buffer.from(b).toString("hex");
const clean = (h) => String(h).replace(/^0x/i, "");

function apiKey() {
  if (process.env.NEARAI_API_KEY) return process.env.NEARAI_API_KEY.trim();
  const f = opt("--key-file");
  if (f) return readFileSync(f, "utf8").trim();
  return null;
}

// ---- network ------------------------------------------------------------------------------------
async function getJson(url, headers = {}) {
  const r = await fetch(url, { headers: { Accept: "application/json", ...headers }, signal: AbortSignal.timeout(60000) });
  const body = await r.text();
  if (!r.ok) throw new Error(`HTTP ${r.status} from ${new URL(url).pathname}`);
  return JSON.parse(body.replace(/^﻿/, ""));
}

let dcap = null;
async function loadDcap() {
  if (dcap !== null) return dcap;
  try { dcap = (await import("@phala/dcap-qvl")).default ?? (await import("@phala/dcap-qvl")); }
  catch { dcap = false; }
  return dcap;
}
async function intelChain(quoteHex) {
  const d = await loadDcap();
  if (!d) return ["CANNOT VERIFY", "Intel quote checker not installed (run: npm install --prefix tools)"];
  try {
    const res = await d.getCollateralAndVerify(Buffer.from(clean(quoteHex), "hex"));
    return ACCEPTED_TCB.includes(res.status)
      ? ["MATCH", `Intel signature chain valid, chip security level ${res.status}`]
      : ["MISMATCH", `Intel signature chain valid but chip security level is ${res.status} (advisories: ${(res.advisory_ids || []).join(", ") || "none"})`];
  } catch (e) {
    const m = String(e.message || e);
    return /fetch|network|ENOTFOUND|ECONN|timeout|HTTP|abort/i.test(m)
      ? ["CANNOT VERIFY", `could not fetch Intel's certificates: ${m.slice(0, 100)}`]
      : ["MISMATCH", `Intel signature chain check failed: ${m.slice(0, 140)}`];
  }
}

const rows = [];
const add = (name, verdict, detail) => rows.push({ name, verdict, detail });
async function check(name, fn) {
  try { const [v, d] = await fn(); add(name, v, d); } catch (e) { add(name, "CANNOT VERIFY", String(e.message).slice(0, 160)); }
}

const key = apiKey();
const verifiedByModel = new Map(); // model -> [{address, algo}] reports that passed every check above

for (const model of MODELS) {
  const nonce = randomBytes(32).toString("hex");
  let reports = null;
  await check(`${model}: fresh report with our nonce`, async () => {
    const u = `${BASE}/v1/attestation/report?${new URLSearchParams({ model, provider: "near", include_tls_fingerprint: "false", signing_algo: ALGO, nonce })}`;
    const j = await getJson(u, { "x-no-aliasing": "true", ...(key ? { Authorization: `Bearer ${key}` } : {}) });
    reports = (j.model_attestations || []).filter((r) => !r.model_name || r.model_name === model);
    if (!reports.length) return ["MISMATCH", "the service returned no report for this exact model name"];
    return ["MATCH", `${reports.length} report(s), nonce ${nonce.slice(0, 12)}...`];
  });
  if (!reports || !reports.length) continue;

  const good = [];
  for (const [i, rep] of reports.entries()) {
    const tag = `${model}${reports.length > 1 ? ` #${i + 1}` : ""}`;
    const [fail, q] = checkReportBytes(rep, nonce);
    add(`${tag}: key and nonce bound in the quote, config and log match`, fail ? "MISMATCH" : "MATCH",
      fail ?? `signer ${clean(rep.signing_address).slice(0, 16)}..., debug off, RTMR3 replays`);
    let chainOk = false;
    if (q) {
      const [v, d] = await intelChain(rep.intel_quote);
      add(`${tag}: Intel signed this quote`, v, d);
      chainOk = v === "MATCH";
    }
    if (!fail && chainOk) good.push({ address: clean(rep.signing_address).toLowerCase(), algo: rep.signing_algo });
  }
  verifiedByModel.set(model, good);
}

if (CALL) {
  if (!key) add("signed reply check", "CANNOT VERIFY", "needs a NEAR AI Cloud key (env NEARAI_API_KEY or --key-file) to send one question");
  else for (const model of MODELS) {
    await check(`${model}: reply signature matches a verified chip key`, async () => {
      const good = verifiedByModel.get(model) || [];
      if (!good.length) return ["CANNOT VERIFY", "no report passed every check above, so there is no key to compare with"];
      const reqBytes = Buffer.from(JSON.stringify({ model, messages: [{ role: "user", content: "Reply with the single word: ok" }], max_tokens: 8, temperature: 0 }));
      const r = await fetch(`${BASE}/v1/chat/completions`, {
        method: "POST", headers: { "content-type": "application/json", Authorization: `Bearer ${key}`, "accept-encoding": "identity" },
        body: reqBytes, signal: AbortSignal.timeout(120000),
      });
      const resBytes = Buffer.from(await r.arrayBuffer());
      if (!r.ok) return ["CANNOT VERIFY", `chat call failed with HTTP ${r.status}`];
      const id = JSON.parse(resBytes.toString()).id;
      const sig = await getJson(`${BASE}/v1/signature/${encodeURIComponent(id)}?${new URLSearchParams({ model, signing_algo: "ed25519" })}`, { Authorization: `Bearer ${key}` });
      if (sig.error_code) return ["CANNOT VERIFY", `signature not available: ${String(sig.message).slice(0, 100)}`];
      const reqH = hex(sha256(reqBytes)), resH = hex(sha256(resBytes));
      const want = sig.signature_kind === "gateway" ? `${reqH}:${resH}` : `${model}:${reqH}:${resH}`;
      if (sig.text !== want) return ["MISMATCH", "signed text is not the hash of the exact request and reply bytes"];
      if (sig.signing_algo !== "ed25519") return ["MISMATCH", `signature algorithm is ${sig.signing_algo}, not ed25519`];
      if (!edVerifyText(sig.signing_address, sig.text, sig.signature)) return ["MISMATCH", "ed25519 signature does not verify"];
      const hits = good.filter((g) => g.address === clean(sig.signing_address).toLowerCase() && g.algo === sig.signing_algo);
      if (hits.length !== 1) return ["MISMATCH", `signer matches ${hits.length} verified reports, must be exactly 1`];
      return ["MATCH", `${sig.signature_kind} signature verifies; signer is the attested key`];
    });
  }
} else {
  add("signed replies", "CANNOT VERIFY", "not run: add --call to send one tiny question per model and check the reply signature");
}

if (flag("--signers")) { console.log(JSON.stringify(Object.fromEntries([...verifiedByModel].map(([m, g]) => [m, g.map((x) => "0x" + x.address)])))); process.exit(0); }
if (flag("--json")) console.log(JSON.stringify(rows));
else {
  const w = Math.min(86, Math.max(...rows.map((r) => r.name.length)));
  for (const r of rows) console.log(`${r.verdict.padEnd(13)} ${r.name.padEnd(w)}  ${r.detail}`);
  const bad0 = rows.some((r) => r.verdict === "MISMATCH");
  const unk0 = rows.some((r) => r.verdict === "CANNOT VERIFY");
  console.log(bad0 ? "RESULT: MISMATCH" : unk0 ? "RESULT: some lines CANNOT VERIFY" : "RESULT: MATCH");
}
const bad = rows.some((r) => r.verdict === "MISMATCH");
const unk = rows.some((r) => r.verdict === "CANNOT VERIFY");
process.exit(bad ? 1 : unk ? 2 : 0);
