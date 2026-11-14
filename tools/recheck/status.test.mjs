import test from 'node:test';
import assert from 'node:assert/strict';
import { badgeFor, statusFor } from './status.mjs';

const report = { checked: '2026-11-14T06:17:00Z', rounds_on_chain: 40, finished: 39, match: 3, open: 36, mismatch: 0, headline: 'x' };

test('a clean report gives a green sticker with honest counts', () => {
  const b = badgeFor(report);
  assert.equal(b.color, 'brightgreen');
  assert.match(b.message, /39 of 39, no mismatch \(3 fully MATCH\)/);
});

test('one mismatch turns the sticker red and says so', () => {
  const b = badgeFor({ ...report, mismatch: 1 });
  assert.equal(b.color, 'red');
  assert.match(b.message, /1 MISMATCH/);
});

test('status carries the contract, key count and last result', () => {
  const s = statusFor({ report, live: { rounds: 40, keys: 0 } });
  assert.equal(s.access_keys_on_contract, 0);
  assert.equal(s.rounds_on_chain, 40);
  assert.equal(s.last_recheck.mismatch, 0);
});

test('status lists court 2 and the mainnet pilot, and the headline is plain', () => {
  const courts = { court2: { rounds: 7, settled: 6, lastSettled: 7, keys: 0 }, mainnet: { rounds: 3, keys: 1 } };
  const s = statusFor({ report, live: null, courts });
  assert.equal(s.court_2.access_keys_on_contract, 0);
  assert.equal(s.court_2.last_settled_round, 7);
  assert.equal(s.mainnet_pilot.bonded_rounds, 3);
  assert.equal(s.last_recheck.headline, '39 of 39 finished rounds: 0 mismatch');
});
