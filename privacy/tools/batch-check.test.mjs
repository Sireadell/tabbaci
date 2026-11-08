import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { checkBatch, brainContinuity, checkReveal } from "./batch-check.mjs";
import { keccak256 } from "../../tools/verify/crypto.mjs";

const load = () => JSON.parse(readFileSync(new URL("./batch-fixture.json", import.meta.url)));
const verdicts = (fx) => checkBatch(fx).map((r) => r[1]);
const hex = (b) => Buffer.from(b).toString("hex");

test("a batch written by the Rust contract passes every line", () => {
  const v = verdicts(load());
  assert.equal(v.length, 6);
  assert.ok(v.every((x) => x === "MATCH"), v.join(","));
});

test("the wrong key is caught at the first line", () => {
  const fx = load();
  fx.key = "01" + fx.key.slice(2);
  const rows = checkBatch(fx);
  assert.equal(rows[0][1], "MISMATCH");
  assert.equal(rows.length, 1);
});

test("a recorded order that differs from what the sealed copy opens to is caught", () => {
  const fx = load();
  fx.batch.opened[0].price_bp = 9000;
  assert.ok(verdicts(fx).includes("MISMATCH"));
});

test("a missing opened order (41 sealed, 40 opened) is caught", () => {
  const fx = load();
  fx.batch.opened.pop();
  const rows = checkBatch(fx);
  assert.equal(rows[1][1], "MISMATCH");
  assert.match(rows[1][2], /4 sealed, 3 recorded/);
});

test("a stored clearing that favours someone is caught", () => {
  const fx = load();
  fx.clearing.fills[0][1] = (BigInt(fx.clearing.fills[0][1]) - 1n).toString();
  assert.ok(verdicts(fx).includes("MISMATCH"));
  const fx2 = load();
  fx2.clearing.price_bp += 1;
  assert.ok(verdicts(fx2).includes("MISMATCH"));
});

test("a changed sealed page is caught by its fingerprint", () => {
  const fx = load();
  fx.words[0].ct = (fx.words[0].ct.startsWith("00") ? "01" : "00") + fx.words[0].ct.slice(2);
  assert.ok(verdicts(fx).includes("MISMATCH"));
});

test("a voided batch is reported, not faked as opened", () => {
  const fx = load();
  fx.batch.state = "Void";
  const rows = checkBatch(fx);
  assert.equal(rows.length, 1);
  assert.equal(rows[0][1], "MATCH");
});

test("an open batch with no key cannot be verified", () => {
  const fx = load();
  fx.batch.state = "Open";
  assert.equal(checkBatch(fx)[0][1], "CANNOT VERIFY");
});

test("writer instructions: same fingerprint across batches means one unchanged hidden strategy", () => {
  const h = (s) => hex(keccak256(Buffer.from(s)));
  const r = brainContinuity([
    { batch: 1, from: "w1.testnet", text_hash: h("be careful") },
    { batch: 2, from: "w1.testnet", text_hash: h("be careful") },
    { batch: 3, from: "w1.testnet", text_hash: h("be careful") },
    { batch: 1, from: "w2.testnet", text_hash: h("a") },
    { batch: 2, from: "w2.testnet", text_hash: h("b") },
  ]);
  const w1 = r.find((x) => x.writer === "w1.testnet");
  const w2 = r.find((x) => x.writer === "w2.testnet");
  assert.equal(w1.same, true);
  assert.equal(w1.batches, 3);
  assert.equal(w2.same, false);
  assert.equal(w2.firstChange, 2);
});

test("revealing the instructions later: matches, mismatches, or cannot verify", () => {
  const text = "You are a careful writer.";
  const fp = hex(keccak256(Buffer.from(text)));
  assert.equal(checkReveal(fp, text)[0], "MATCH");
  assert.equal(checkReveal(fp, text + " ")[0], "MISMATCH");
  assert.equal(checkReveal(fp, null)[0], "CANNOT VERIFY");
});
