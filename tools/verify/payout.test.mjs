import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { net, settleBatchCheck } from './payout.mjs';

test('JavaScript payout maths equals the Rust vectors', () => {
  const v = JSON.parse(readFileSync(new URL('../../contracts/payout/tests/vectors.json', import.meta.url), 'utf8'));
  assert.equal(v.length, 1000);
  for (const x of v) assert.equal(net(BigInt(x.stake), x.p, x.o), BigInt(x.net));
});

test('money is conserved', () => {
  assert.equal(settleBatchCheck().ok, true);
});
