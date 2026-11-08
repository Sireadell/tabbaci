import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { checkReport, roundNonce, expectedText, matchSigner } from "./attest.mjs";

// a real report fetched from the public NEAR AI Cloud report endpoint, kept with the Rust tests
const fx = JSON.parse(readFileSync(new URL("../../programs/court/testdata/model_report.json", import.meta.url)));
const copy = () => JSON.parse(JSON.stringify(fx.report));

test("a real report passes and says the Intel chain was not checked", () => {
  const r = checkReport(copy(), fx.nonce);
  assert.equal(r.ok, true, r.why);
  assert.equal(r.chainChecked, false);
  assert.equal(r.algo, "ed25519");
});

test("wrong nonce, swapped signer, edited config, edited log and debug mode are all refused", () => {
  assert.match(checkReport(copy(), "11".repeat(32)).why, /nonce/);
  const a = copy(); a.signing_address = "ab".repeat(32);
  assert.match(checkReport(a, fx.nonce).why, /bind/);
  const b = copy(); const t = JSON.parse(b.info.tcb_info); t.app_compose += " "; b.info.tcb_info = t;
  assert.match(checkReport(b, fx.nonce).why, /app_compose/);
  const c = copy(); const log = typeof c.event_log === "string" ? JSON.parse(c.event_log) : c.event_log;
  log.find((e) => e.imr === 3 && e.event === "system-preparing").event = "system-preparing-x"; c.event_log = log;
  assert.equal(checkReport(c, fx.nonce).ok, false);
  const d = copy(); const q = Buffer.from(d.intel_quote, "hex"); q[48 + 120] |= 1; d.intel_quote = q.toString("hex");
  assert.match(checkReport(d, fx.nonce).why, /debug/);
});

test("this round's number is different for every round", () => {
  assert.notEqual(roundNonce(Buffer.from("a")), roundNonce(Buffer.from("b")));
  assert.equal(roundNonce(Buffer.from("a")).length, 64);
});

test("signature text and signer matching", () => {
  assert.equal(expectedText("m", Buffer.from("q"), Buffer.from("r")).split(":").length, 3);
  const ok = checkReport(copy(), fx.nonce);
  assert.equal(matchSigner([ok], "ed25519", ok.signer).ok, true);
  assert.equal(matchSigner([ok], "ecdsa", ok.signer).ok, false);
  assert.match(matchSigner([ok, ok], "ed25519", ok.signer).why, /more than one/);
});
