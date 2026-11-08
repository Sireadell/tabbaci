// Checks on a NEAR AI Cloud model report, the same rules as programs/court/src/attest.rs, so a
// stranger can run them with Node alone. Rules come from docs.near.ai (model attestations and
// quote-nonce-signer pages). Intel's signature chain on the quote is NOT checked here.
import { createHash } from "node:crypto";

const HEADER = 48;
const TD_ATTRIBUTES = HEADER + 120, MRCONFIGID = HEADER + 184, RTMR3 = HEADER + 472, REPORT_DATA = HEADER + 520;
const sha = (alg, ...parts) => { const h = createHash(alg); for (const p of parts) h.update(p); return h.digest(); };
const asJson = (v) => (typeof v === "string" ? JSON.parse(v) : v);

export function roundNonce(inputBytes) {
  return sha("sha256", inputBytes).toString("hex");
}

export function replayRtmr3(eventLog) {
  let rtmr = Buffer.alloc(48);
  for (const e of asJson(eventLog)) {
    if (e.imr !== 3) continue;
    const et = Buffer.alloc(4); et.writeUInt32LE(e.event_type);
    const digest = sha("sha384", et, ":", Buffer.from(e.event, "utf8"), ":", Buffer.from(e.event_payload || "", "hex"));
    if (e.digest && e.digest.toLowerCase() !== digest.toString("hex")) throw new Error(`digest mismatch for event ${e.event}`);
    rtmr = sha("sha384", rtmr, digest);
  }
  return rtmr;
}

// Returns {ok:true, signer, algo, chainChecked:false} or {ok:false, why}
export function checkReport(report, nonceHex) {
  try {
    if (!/^[0-9a-f]{64}$/i.test(nonceHex)) throw new Error("nonce must be 32 bytes of hex");
    const algo = report.signing_algo;
    const addr = Buffer.from(String(report.signing_address).replace(/^0x/, ""), "hex");
    if ((algo === "ecdsa" && addr.length !== 20) || (algo === "ed25519" && addr.length !== 32) || !["ecdsa", "ed25519"].includes(algo)) throw new Error("unsupported signing identity");
    if (String(report.request_nonce).toLowerCase() !== nonceHex.toLowerCase()) throw new Error("echoed nonce is not the one we sent");
    const q = Buffer.from(report.intel_quote, "hex");
    if (q.length < HEADER + 584) throw new Error("quote is too short");
    if (q.readUInt16LE(0) !== 4 || q.readUInt32LE(4) !== 0x81) throw new Error("not a TDX version 4 quote");
    if (q[TD_ATTRIBUTES] & 1) throw new Error("TDX debug mode is on");
    const bound = Buffer.concat([Buffer.concat([addr, Buffer.alloc(32 - addr.length)]), Buffer.from(nonceHex, "hex")]);
    if (!q.subarray(REPORT_DATA, REPORT_DATA + 64).equals(bound)) throw new Error("quote report data does not bind this signer and nonce");
    const compose = asJson(report.info.tcb_info).app_compose;
    const cfg = Buffer.concat([Buffer.from([1]), sha("sha256", Buffer.from(compose, "utf8")), Buffer.alloc(15)]);
    if (!q.subarray(MRCONFIGID, MRCONFIGID + 48).equals(cfg)) throw new Error("quote config id does not match app_compose");
    if (!q.subarray(RTMR3, RTMR3 + 48).equals(replayRtmr3(report.event_log))) throw new Error("event log does not replay to the quote's RTMR3");
    return { ok: true, signer: addr.toString("hex"), algo, chainChecked: false };
  } catch (e) {
    return { ok: false, why: e.message };
  }
}

// The text a provider_tee signature must carry.
export function expectedText(model, requestBytes, responseBytes) {
  return `${model}:${sha("sha256", requestBytes).toString("hex")}:${sha("sha256", responseBytes).toString("hex")}`;
}

// A signature is accepted only when its signer and algorithm match exactly one checked report.
export function matchSigner(checked, algo, address) {
  const a = address.replace(/^0x/, "").toLowerCase();
  const hits = checked.filter((c) => c.algo === algo && c.signer === a);
  return hits.length === 1 ? { ok: true } : { ok: false, why: hits.length ? "signer matches more than one report" : "signer matches no checked report" };
}
