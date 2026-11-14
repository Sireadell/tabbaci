import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const site = join(here, '..', '..', 'site');
const require = createRequire(import.meta.url);
const IC = require(join(site, 'intelcheck.js'));
const qvl = await import(pathToFileURL(join(site, 'vendor', 'dcap-qvl-web-0.3.3', 'dcap-qvl-web.js')).href);
qvl.initSync({ module: readFileSync(join(site, 'vendor', 'dcap-qvl-web-0.3.3', 'dcap-qvl-web_bg.wasm')) });

const load = (dir, n) => JSON.parse(readFileSync(join(here, '..', '..', 'proofs', dir, `round-${n}.bundle.json`), 'utf8'));
// A stand-in for the chain that answers what the real contract would.
const chain = (b, over = {}) => async (contract, method) => {
  if (method === 'courtroom') return ['https://x', over.fp ?? b.attestation.wasm_hash];
  if (method === 'is_measurements_approved') return over.approved ?? true;
  throw new Error('unexpected ' + method);
};
const run = (b, over = {}, round) => IC.checkBundle(b, { qvl, view: chain(b, over), contract: 'c.testnet', round: round === undefined ? { verdict: JSON.parse(b.output_raw) } : round });
const status = (rows) => Object.fromEntries(rows.map((r) => [r.key, r.status]));
const clone = (b) => JSON.parse(JSON.stringify(b));

for (const [dir, n] of [['finalenc2', 20], ['finalenc2', 25], ['finalenc2', 33], ['tabbaci-court2', 4], ['tabbaci-court2', 5], ['tabbaci-court2', 6], ['tabbaci-court2', 7], ['finalenc1', 2], ['court5', 6]]) {
  test(`${dir} round ${n}: the saved Intel report passes`, async () => {
    const rows = await run(load(dir, n));
    assert.deepEqual(status(rows), { sig: 'MATCH', bind: 'MATCH', prog: 'MATCH', build: 'MATCH' }, JSON.stringify(rows));
  });
}

test('a tampered quote shows FAIL', async () => {
  const b = clone(load('finalenc2', 25));
  const q = Buffer.from(b.attestation.tdx_quote, 'base64'); q[650] ^= 1; // inside Intel's signature over the report
  b.attestation.tdx_quote = q.toString('base64');
  const s = status(await run(b));
  assert.equal(s.sig, 'MISMATCH'); assert.equal(s.bind, 'NEEDS');
});
test('a flipped byte in the signed header shows FAIL', async () => {
  const b = clone(load('finalenc2', 25));
  const q = Buffer.from(b.attestation.tdx_quote, 'base64'); q[100] ^= 1;
  b.attestation.tdx_quote = q.toString('base64');
  assert.equal(status(await run(b)).sig, 'MISMATCH');
});
test('a report whose program fingerprint was edited shows FAIL', async () => {
  const b = clone(load('finalenc2', 25));
  b.attestation.wasm_hash = 'f'.repeat(64);
  const s = status(await run(b, { fp: 'f'.repeat(64) }));
  assert.equal(s.sig, 'MATCH'); assert.equal(s.bind, 'MISMATCH');
});
test('a program fingerprint that differs from the contract shows FAIL', async () => {
  const b = load('finalenc2', 25);
  const s = status(await run(b, { fp: 'a'.repeat(64) }));
  assert.equal(s.prog, 'MISMATCH'); assert.equal(s.sig, 'MATCH');
});
test('an edited ruling text shows FAIL', async () => {
  const b = clone(load('finalenc2', 25));
  b.output_raw = b.output_raw.replace('o\\":\\"F', 'o\\":\\"T');
  assert.notEqual(b.output_raw, load('finalenc2', 25).output_raw);
  assert.equal(status(await run(b, {}, { verdict: JSON.parse(b.output_raw) })).bind, 'MISMATCH');
});
test('a ruling that is not the one the contract stored shows FAIL', async () => {
  const b = load('finalenc2', 25);
  assert.equal(status(await run(b, {}, { verdict: '{"other":1}' })).bind, 'MISMATCH');
});
test('a build that is not on the approved list shows FAIL', async () => {
  assert.equal(status(await run(load('finalenc2', 25), { approved: false })).build, 'MISMATCH');
});
test('an unreachable chain is never shown as a pass', async () => {
  const b = load('finalenc2', 25);
  const rows = await IC.checkBundle(b, { qvl, view: async () => { throw new Error('down'); }, contract: 'c', round: null });
  const s = status(rows);
  assert.equal(s.sig, 'MATCH'); assert.equal(s.prog, 'NEEDS'); assert.equal(s.build, 'NEEDS'); assert.equal(s.bind, 'NEEDS');
});
test('an empty bundle is not available, not a pass', async () => {
  assert.equal(status(await IC.checkBundle({}, { qvl }))['sig'], 'NEEDS');
});
test('the checker files match the fingerprints written in the repo', async () => {
  const sums = JSON.parse(readFileSync(join(site, 'vendor', 'dcap-qvl-web-0.3.3', 'SHA256SUMS.json'), 'utf8')).files;
  assert.deepEqual(sums, IC.CHECKER_SUMS);
  const { createHash } = await import('node:crypto');
  for (const [f, h] of Object.entries(sums)) assert.equal(createHash('sha256').update(readFileSync(join(site, 'vendor', 'dcap-qvl-web-0.3.3', f))).digest('hex'), h);
});
