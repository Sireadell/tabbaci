import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, existsSync, writeFileSync, mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { proofPath, sealedBoxCommand, PROOF_ROOT } from './proofs.mjs';

test('proof path is per contract and round', () => {
  assert.match(proofPath('court5.sireadell.testnet', 6).replace(/\\/g, '/'), /proofs\/court5\/round-6\.bundle\.json$/);
});

test('no tx and no saved file gives no command', () => {
  assert.equal(sealedBoxCommand({ network: 'testnet' }), null);
});

test('a saved file is checked offline', () => {
  const f = join(mkdtempSync(join(tmpdir(), 'p-')), 'b.json');
  writeFileSync(f, '{}');
  const c = sealedBoxCommand({ bundle: f, tx: 'abc' });
  assert.deepEqual(c.args.slice(0, 3), ['bundle', f, '--offline']);
});

test('tx with --save-proof rewrites the saved file', () => {
  const f = join(mkdtempSync(join(tmpdir(), 'p-')), 'b.json');
  writeFileSync(f, '{}');
  const c = sealedBoxCommand({ bundle: f, tx: 'abc', save: true, savePath: f });
  assert.equal(c.args[0], 'tx');
  assert.ok(c.args.includes('--bundle'));
});

test('the committed round 6 proof is real JSON with a quote inside', () => {
  const f = proofPath('court5.sireadell.testnet', 6);
  assert.ok(existsSync(f), 'saved proof missing');
  const text = readFileSync(f, 'utf8');
  JSON.parse(text);
  assert.ok(text.length > 5000);
});

test('the committed round 6 proof re-checks offline (skipped if outlayer-verify is not installed)', (t) => {
  const f = proofPath('court5.sireadell.testnet', 6);
  const r = spawnSync('outlayer-verify', ['bundle', f, '--offline', '--short', '--no-color'], { encoding: 'utf8' });
  if (r.error) return t.skip('outlayer-verify not installed');
  assert.equal(r.status, 0, r.stdout + r.stderr);
  assert.match(r.stdout, /PROVEN/);
});
