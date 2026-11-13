import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { findOutlayerVerify, platformKey, sha256 } from './outlayer.mjs';
import { classify } from '../recheck/recheck.mjs';

const verify = fileURLToPath(new URL('./verify.mjs', import.meta.url));
const vectors = fileURLToPath(new URL('../../contracts/payout/tests/vectors.json', import.meta.url));

test('untouched payout vectors end in OVERALL: MATCH with exit 0', () => {
  const r = spawnSync(process.execPath, [verify, '--vectors', vectors], { encoding: 'utf8' });
  assert.match(r.stdout, /OVERALL: MATCH/);
  assert.equal(r.status, 0);
});

test('a tampered vector still ends in MISMATCH with exit 1', () => {
  const v = JSON.parse(readFileSync(vectors, 'utf8'));
  v[0].net = String(BigInt(v[0].net) + 1n);
  const f = join(mkdtempSync(join(tmpdir(), 'tamper-')), 'v.json');
  writeFileSync(f, JSON.stringify(v));
  const r = spawnSync(process.execPath, [verify, '--vectors', f], { encoding: 'utf8' });
  assert.match(r.stdout, /OVERALL: MISMATCH/);
  assert.equal(r.status, 1);
});

test('the re-check tool maps exit codes to MATCH, OPEN and MISMATCH', () => {
  assert.deepEqual([0, 2, 1, 3].map(classify), ['MATCH', 'OPEN', 'MISMATCH', 'MISMATCH']);
});

test('a downloaded sealed-box checker with the wrong fingerprint is refused', async () => {
  const key = platformKey();
  const sums = { release: 'https://example.invalid/dl', files: { [key]: { name: 'outlayer-verify-test', sha256: sha256(Buffer.from('the real one')) } } };
  const fetchImpl = async () => ({ ok: true, status: 200, arrayBuffer: async () => Buffer.from('a swapped file') });
  const got = await findOutlayerVerify({ fetchImpl, sums, key, toolsDir: mkdtempSync(join(tmpdir(), 'olv-')) });
  if (got.cmd === 'outlayer-verify') return; // a real copy is installed here, nothing to refuse
  assert.ok(got.mismatch, 'expected a mismatch');
});

test('with no fingerprint written in the repo the checker is not downloaded', async () => {
  const got = await findOutlayerVerify({ sums: null, key: platformKey(), toolsDir: mkdtempSync(join(tmpdir(), 'olv-')) });
  if (got.cmd === 'outlayer-verify') return;
  assert.ok(got.missing);
});
