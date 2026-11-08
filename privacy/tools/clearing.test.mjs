import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { clear } from "./clearing.mjs";

const vectors = JSON.parse(readFileSync(new URL("./clearing-vectors.json", import.meta.url)));

test("Node clearing equals the Rust contract's clearing on every shared vector", () => {
  assert.ok(vectors.length >= 200);
  let traded = 0;
  for (const [i, v] of vectors.entries()) {
    const c = clear(v.orders.map(([s, l, a]) => ({ side: s, limit: l, amount: BigInt(a) })));
    assert.equal(c.price_bp, v.price_bp, `vector ${i} price`);
    assert.equal(c.matched.toString(), v.matched, `vector ${i} matched`);
    assert.deepEqual(c.fills.map((f) => [f.shares.toString(), f.cost.toString()]), v.fills, `vector ${i} fills`);
    if (v.price_bp) traded++;
  }
  assert.ok(traded > 20, `only ${traded} vectors actually traded`);
});

test("orders that never meet do not trade", () => {
  const c = clear([{ side: 0, limit: 3000, amount: 10n ** 24n }, { side: 1, limit: 3000, amount: 10n ** 24n }]);
  assert.equal(c.price_bp, 0);
});
