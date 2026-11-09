// Pure checks on a NEAR AI Cloud model report. No network, so each rule has a test (see judge-checks.test.mjs).
import { createHash, createPublicKey, verify as edVerify } from "node:crypto";

const sha256 = (b) => createHash("sha256").update(b).digest();
const sha384 = (b) => createHash("sha384").update(b).digest();
const clean = (h) => String(h).replace(/^0x/i, "");

const HEADER = 48;
export function parseQuote(buf) {
  if (buf.length < HEADER + 584) throw new Error("quote is too short");
  const version = buf.readUInt16LE(0);
  const teeType = buf.readUInt32LE(4);
  if (version !== 4) throw new Error(`quote version ${version}, expected 4`);
  if (teeType !== 0x81) throw new Error(`quote type 0x${teeType.toString(16)} is not Intel TDX`);
  return {
    tdAttributes: buf.subarray(HEADER + 120, HEADER + 128),
    mrConfigId: buf.subarray(HEADER + 184, HEADER + 232),
    rtmr3: buf.subarray(HEADER + 472, HEADER + 520),
    reportData: buf.subarray(HEADER + 520, HEADER + 584),
  };
}

// dstack records each runtime event; events with an empty digest are hashed from their parts.
export function replayRtmr3(events) {
  let rtmr = Buffer.alloc(48);
  for (const e of events) {
    if (e.imr !== 3) continue;
    let d;
    if (e.digest) d = Buffer.from(clean(e.digest), "hex");
    else {
      const t = Buffer.alloc(4); t.writeUInt32LE(e.event_type >>> 0);
      d = sha384(Buffer.concat([t, Buffer.from(":"), Buffer.from(e.event ?? ""), Buffer.from(":"), Buffer.from(clean(e.event_payload ?? ""), "hex")]));
    }
    d = Buffer.concat([d, Buffer.alloc(48)]).subarray(0, 48);
    rtmr = sha384(Buffer.concat([rtmr, d]));
  }
  return rtmr;
}

export function checkReportBytes(report, nonceHex) {
  const fails = [];
  if (String(report.request_nonce ?? "").toLowerCase() !== nonceHex) fails.push("report does not echo our nonce");
  // ed25519: report data is the 32 byte key, then our nonce.
  // ecdsa (what the courtroom uses): the 20 byte address, 12 zero bytes, then our nonce.
  const algo = report.signing_algo;
  if (algo !== "ed25519" && algo !== "ecdsa") fails.push(`signing_algo is ${algo}, not ed25519 or ecdsa`);
  let key = Buffer.from(clean(report.signing_address ?? ""), "hex");
  if (algo === "ed25519" && key.length !== 32) fails.push("signing_address is not a 32 byte ed25519 key");
  if (algo === "ecdsa") {
    if (key.length !== 20) fails.push("signing_address is not a 20 byte ecdsa address");
    key = Buffer.concat([key, Buffer.alloc(12)]);
  }
  let q;
  try { q = parseQuote(Buffer.from(clean(report.intel_quote ?? ""), "hex")); } catch (e) { return [`quote: ${e.message}`, null]; }
  if (q.tdAttributes[0] & 1) fails.push("debug mode is ON in this chip");
  const want = Buffer.concat([key, Buffer.from(nonceHex, "hex")]);
  if (!q.reportData.equals(want)) fails.push("quote report data is not (signing key, our nonce)");
  let compose = report.info?.tcb_info;
  if (typeof compose === "string") { try { compose = JSON.parse(compose); } catch { compose = null; } }
  const raw = compose?.app_compose;
  if (typeof raw !== "string") fails.push("app_compose missing");
  else {
    const cfg = Buffer.concat([Buffer.from([1]), sha256(Buffer.from(raw)), Buffer.alloc(15)]);
    if (!q.mrConfigId.equals(cfg)) fails.push("quote config id is not 01 + sha256(app_compose)");
  }
  let events = report.event_log;
  if (typeof events === "string") { try { events = JSON.parse(events); } catch { events = null; } }
  if (!Array.isArray(events)) fails.push("event_log missing");
  else if (!replayRtmr3(events).equals(q.rtmr3)) fails.push("event log does not replay to the quote's RTMR3");
  return [fails.join("; ") || null, q];
}

export function edVerifyText(signingKeyHex, text, sigHex) {
  const key = createPublicKey({ key: Buffer.concat([Buffer.from("302a300506032b6570032100", "hex"), Buffer.from(clean(signingKeyHex), "hex")]), format: "der", type: "spki" });
  return edVerify(null, Buffer.from(text), key, Buffer.from(clean(sigHex), "hex"));
}

