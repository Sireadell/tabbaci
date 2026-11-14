import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, mkdtempSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { spawn } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { checkPrivate, privateShapeOk, privateUser } from './private.mjs';

// written by the private court program's own code (stand-in judges): programs/court-private/testdata/private_round.json
const fixture = JSON.parse(readFileSync(fileURLToPath(new URL('../../programs/court-private/testdata/private_round.json', import.meta.url)), 'utf8'));
const input = JSON.parse(fixture.input);
const v = JSON.parse(fixture.output);
const signer = fixture.signer;
const attested = Object.fromEntries(input.m.map((m) => [m.toLowerCase(), [signer]]));
// what the contract stores when it accepts this output (contracts/courtroom-private on_attempt)
const checked = v.j.map((j, i) => ({ model: input.m[i], said: j.v, answer: j.a, quote: '', quote_ok: false, sig_ok: true, sig: j.g, key: j.k }));
const round = { checked, fingerprint: v.s[0].h };
const bad = (rows) => rows.filter((r) => r.status === 'MISMATCH');
const by = (rows, re) => rows.filter((r) => re.test(r.name));
const clone = (x) => JSON.parse(JSON.stringify(x));

test('the question text is the same as the program and the contract build', () => {
  assert.equal(
    privateUser('S', 'https://api.github.com/x', 'more than', '20'),
    'Sentence: <untrusted>S</untrusted>\nSource: <untrusted>https://api.github.com/x</untrusted>\nQuestion: does this sentence say that the number read from this source is more than 20? Reply TRUE, FALSE or UNSURE.',
  );
});

test("the program's real private output passes every line", () => {
  const rows = checkPrivate({ r: round, input, v, page: null, attested });
  assert.deepEqual(bad(rows), []);
  assert.ok(!rows.some((r) => r.status === 'CANNOT VERIFY'), JSON.stringify(rows.filter((r) => r.status === 'CANNOT VERIFY')));
  assert.equal(by(rows, /fingerprint recorded in the verdict/)[0].status, 'MATCH');
  assert.equal(by(rows, /contract record matches the fingerprint/)[0].status, 'MATCH');
  assert.equal(by(rows, /signature verifies/).filter((r) => r.status === 'MATCH').length, 3);
  assert.equal(by(rows, /contract checked the same signature/).filter((r) => r.status === 'MATCH').length, 3);
  assert.equal(by(rows, /outcome follows/)[0].status, 'MATCH');
  assert.equal(by(rows, /no page text/)[0].status, 'MATCH');
});

test('the person who holds the page can match it to the fingerprint', () => {
  const page = Buffer.from('a page that is not the one');
  const rows = checkPrivate({ r: round, input, v, page, attested });
  assert.equal(by(rows, /page you hold/)[0].status, 'MISMATCH');
  assert.equal(createHash('sha256').update(page).digest('hex') === fixture.page_sha256_hex, false);
});

test('a different fingerprint on chain is a MISMATCH', () => {
  const rows = checkPrivate({ r: { ...round, fingerprint: 'A'.repeat(43) }, input, v, page: null, attested });
  assert.equal(by(rows, /contract record matches/)[0].status, 'MISMATCH');
});

test('a verdict that carries page text, a value or a quote is a MISMATCH', () => {
  for (const change of [(x) => { x.ex = 'page text'; }, (x) => { x.s[0].x = '37'; }, (x) => { x.j[0].q = 'page text'; }, (x) => { x.s[0].n = 1; }]) {
    const w = clone(v);
    change(w);
    assert.equal(privateShapeOk(w), false);
    assert.equal(by(checkPrivate({ r: round, input, v: w, page: null, attested }), /no page text/)[0].status, 'MISMATCH');
  }
});

test('a forged signature, a flipped bit, or a contract record of another signature is a MISMATCH', () => {
  const w = clone(v);
  const g = Buffer.from(w.j[0].g, 'base64url'); g[0] ^= 1; w.j[0].g = g.toString('base64url');
  assert.ok(bad(checkPrivate({ r: round, input, v: w, page: null, attested })).some((r) => /judge 0 .*signature verifies/.test(r.name)));
  const f = clone(v); f.s[0].y = !f.s[0].y;
  assert.equal(by(checkPrivate({ r: round, input, v: f, page: null, attested }), /outcome follows/)[0].status, 'MISMATCH');
  const r2 = clone(round); r2.checked[1].sig = r2.checked[0].sig.replace(/^./, (c) => (c === 'A' ? 'B' : 'A'));
  assert.ok(bad(checkPrivate({ r: r2, input, v, page: null, attested })).some((r) => /judge 1 .*contract checked/.test(r.name)));
});

test('judges asked about another source do not verify', () => {
  const i2 = clone(input); i2.src[0].u = i2.src[0].u.replace('2026-11-01', '2026-10-01');
  assert.equal(bad(checkPrivate({ r: round, input: i2, v, page: null, attested })).filter((r) => /signature verifies/.test(r.name)).length, 3);
});

// ---------- the whole checker against a stand-in chain ----------

function wasmWithExports(names) {
  const body = [names.length];
  for (const n of names) body.push(n.length, ...Buffer.from(n), 0, 0);
  return Buffer.from([0, 0x61, 0x73, 0x6d, 1, 0, 0, 0, 7, body.length, ...body]);
}

async function standInChain(roundOverride) {
  const program = Buffer.from('stand-in courtroom program bytes');
  const programHash = createHash('sha256').update(program).digest('hex');
  const code = wasmWithExports(['open_round', 'attempt', 'get_round']);
  const r = {
    input: fixture.input, ih: v.ih, claim: input.claim, status: 'settled', outcome: v.o, void_reason: null, bonds: [],
    attempts: [{ n: 1, start_block: 10, end_block: 14, result: 'ok', output: fixture.output }],
    verdict: fixture.output, checked, fingerprint: v.s[0].h, ...roundOverride,
  };
  const server = createServer((req, res) => {
    if (req.method === 'GET') { res.end(program); return; }
    let body = '';
    req.on('data', (c) => { body += c; });
    req.on('end', () => {
      const p = JSON.parse(body).params;
      let result;
      if (p.request_type === 'view_access_key_list') result = { keys: [] };
      else if (p.request_type === 'view_code') result = { code_base64: code.toString('base64') };
      else if (p.method_name === 'courtroom') result = { result: [...Buffer.from(JSON.stringify([`http://127.0.0.1:${server.address().port}/court-private.wasm`, programHash]))] };
      else if (p.method_name === 'get_round') result = { result: [...Buffer.from(JSON.stringify(r))] };
      res.setHeader('content-type', 'application/json');
      res.end(JSON.stringify(result ? { jsonrpc: '2.0', id: 1, result } : { jsonrpc: '2.0', id: 1, error: { message: 'unknown' } }));
    });
  });
  await new Promise((ok) => server.listen(0, '127.0.0.1', ok));
  return server;
}

function runChecker(port, extra = []) {
  const dir = mkdtempSync(join(tmpdir(), 'priv-'));
  const signers = join(dir, 'signers.json');
  writeFileSync(signers, JSON.stringify(attested));
  const verify = fileURLToPath(new URL('./verify.mjs', import.meta.url));
  return new Promise((ok) => {
    const c = spawn(process.execPath, [verify, '--contract', 'court-private.t', '--round', String(v.rd), '--rpc', `http://127.0.0.1:${port}`, '--signers-json', signers, '--proof', join(dir, 'none.json'), ...extra]);
    let out = '';
    c.stdout.on('data', (d) => { out += d; });
    c.on('close', (code) => ok({ out, code }));
  });
}

test('the full checker on a private round: every line matches except the sealed-box proof, which is not saved here', async () => {
  const server = await standInChain({});
  try {
    const { out } = await runChecker(server.address().port);
    const lines = out.split('\n').filter((l) => /^(MATCH|MISMATCH|CANNOT VERIFY)/.test(l));
    const notMatch = lines.filter((l) => !l.startsWith('MATCH'));
    assert.equal(notMatch.length, 1, notMatch.join('\n'));
    assert.match(notMatch[0], /^CANNOT VERIFY\s+sealed-box attestation/);
    for (const want of [/fingerprint recorded in the verdict/, /contract record matches the fingerprint/, /judge 2 .*contract checked the same signature/, /outcome follows from the signed answers and the page bit/, /verdict carries no page text/]) {
      assert.ok(lines.some((l) => l.startsWith('MATCH') && want.test(l)), `missing MATCH line ${want}`);
    }
  } finally { server.close(); }
});

test('the full checker says MISMATCH when the contract recorded another fingerprint', async () => {
  const server = await standInChain({ fingerprint: 'B'.repeat(43) });
  try {
    const { out, code } = await runChecker(server.address().port);
    assert.match(out, /MISMATCH\s+contract record matches the fingerprint/);
    assert.match(out, /OVERALL: MISMATCH/);
    assert.equal(code, 1);
  } finally { server.close(); }
});
