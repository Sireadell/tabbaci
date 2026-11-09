// Run: node --test tools/judge-checks.test.mjs
// Uses one real report saved from NEAR AI Cloud. Every tampered copy must be refused.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { generateKeyPairSync, sign } from "node:crypto";
import { checkReportBytes, edVerifyText } from "./judge-checks.mjs";

const { nonce, report } = JSON.parse(readFileSync(new URL("./testdata/judge_report.json", import.meta.url), "utf8"));
const copy = () => JSON.parse(JSON.stringify(report));
const flipHex = (h, at) => h.slice(0, at) + (h[at] === "0" ? "1" : "0") + h.slice(at + 1);

test("the real report passes every offline check", () => {
  const [fail] = checkReportBytes(copy(), nonce);
  assert.equal(fail, null);
});

test("a different nonce is refused", () => {
  const [fail] = checkReportBytes(copy(), "11".repeat(32));
  assert.match(fail, /nonce/);
});

test("a swapped signing key is refused", () => {
  const r = copy(); r.signing_address = flipHex(r.signing_address, 5);
  assert.match(checkReportBytes(r, nonce)[0], /report data/);
});

test("a changed nonce echo is refused", () => {
  const r = copy(); r.request_nonce = "22".repeat(32);
  assert.match(checkReportBytes(r, nonce)[0], /echo/);
});

test("a changed configuration is refused", () => {
  const r = copy();
  if (typeof r.info.tcb_info === "string") r.info.tcb_info = JSON.parse(r.info.tcb_info);
  r.info.tcb_info.app_compose += " ";
  assert.match(checkReportBytes(r, nonce)[0], /config id/);
});

test("an edited event log is refused", () => {
  const r = copy();
  const ev = typeof r.event_log === "string" ? JSON.parse(r.event_log) : r.event_log;
  const i = ev.findIndex((e) => e.imr === 3);
  assert.ok(i >= 0, "fixture has runtime events");
  if (ev[i].digest) ev[i].digest = flipHex(ev[i].digest, 3);
  else ev[i].event += "x"; // empty digest events are hashed from their name, so renaming one must break the replay
  r.event_log = ev;
  assert.match(checkReportBytes(r, nonce)[0], /RTMR3/);
});

test("debug mode switched on is refused", () => {
  const r = copy();
  const q = Buffer.from(r.intel_quote, "hex");
  q[48 + 120] |= 1; // TD attributes, bit 0 is debug
  r.intel_quote = q.toString("hex");
  assert.match(checkReportBytes(r, nonce)[0], /debug/);
});

test("an ed25519 report relabelled as ecdsa is refused", () => {
  const r = copy(); r.signing_algo = "ecdsa";
  assert.match(checkReportBytes(r, nonce)[0], /20 byte ecdsa address/);
});

test("a non-Intel-TDX quote is refused", () => {
  const r = copy(); const q = Buffer.from(r.intel_quote, "hex"); q.writeUInt32LE(0, 4); r.intel_quote = q.toString("hex");
  assert.match(checkReportBytes(r, nonce)[0], /quote/);
});

test("ed25519 signature check accepts a good signature and refuses a bad text or key", () => {
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  const raw = publicKey.export({ format: "der", type: "spki" }).subarray(-32).toString("hex");
  const text = "m/one:" + "aa".repeat(32) + ":" + "bb".repeat(32);
  const sig = sign(null, Buffer.from(text), privateKey).toString("hex");
  assert.equal(edVerifyText(raw, text, sig), true);
  assert.equal(edVerifyText(raw, text + "x", sig), false);
  const other = generateKeyPairSync("ed25519").publicKey.export({ format: "der", type: "spki" }).subarray(-32).toString("hex");
  assert.equal(edVerifyText(other, text, sig), false);
});

// The courtroom asks for ecdsa receipts, so its judges' reports bind a 20 byte address instead of an ed25519 key.
const ec = JSON.parse(readFileSync(new URL("./testdata/judge_report_ecdsa.json", import.meta.url), "utf8"));
const ecCopy = () => JSON.parse(JSON.stringify(ec.report));

test("a real ecdsa report passes every offline check", () => {
  assert.equal(checkReportBytes(ecCopy(), ec.nonce)[0], null);
});

test("an ecdsa report with a swapped address is refused", () => {
  const r = ecCopy(); r.signing_address = flipHex(r.signing_address, 7);
  assert.match(checkReportBytes(r, ec.nonce)[0], /report data/);
});

test("an ecdsa report with a different nonce is refused", () => {
  assert.match(checkReportBytes(ecCopy(), "33".repeat(32))[0], /nonce/);
});

test("an ecdsa report relabelled as ed25519 is refused", () => {
  const r = ecCopy(); r.signing_algo = "ed25519";
  assert.ok(checkReportBytes(r, ec.nonce)[0]);
});
