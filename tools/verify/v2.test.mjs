import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { norm, quoteIn, parseReply, requestBodyV2, checkV2, ed25519Ok } from './v2.mjs';

const fixture = JSON.parse(readFileSync(fileURLToPath(new URL('../../programs/court-v2/testdata/prose_round.json', import.meta.url)), 'utf8'));
const input = JSON.parse(fixture.input);
const v = JSON.parse(fixture.output);
const pages = [
  Buffer.from('intro text. Near Protocol launched its mainnet in 2020. more text'),
  Buffer.from('history: the network launched in October 2020, said the report'),
];
// what the contract stores for the program's output (the third judge made up its quote)
const checked = v.j.map((j, i) => ({ model: input.m[i], said: j.v, answer: j.a, quote: j.q, quote_ok: !j.qx, sig_ok: true }));
const round = { checked };
const by = (rows, re) => rows.filter((r) => re.test(r.name));

test('request bytes are the same as the program and the contract pin', () => {
  assert.equal(requestBodyV2('m', 's', 'u', 7, 120), '{"model":"m","messages":[{"role":"system","content":"s"},{"role":"user","content":"u"}],"temperature":0,"seed":7,"max_tokens":120,"stream":false,"reasoning_effort":"none"}');
});

test('text rules match the program', () => {
  assert.equal(norm('  The\tMoon \n\n is  Cheese. '), 'the moon is cheese.');
  assert.ok(quoteIn('x The   Moon is\nbig. y', 'the moon IS big.'));
  assert.ok(!quoteIn('the moon', ''));
  assert.deepEqual(parseReply('FALSE\n"A b."'), ['FALSE', 'A b.']);
  assert.deepEqual(parseReply('TRUE'), ['TRUE', '']);
});

test("the program's real output passes every v2 line", () => {
  const rows = checkV2({ r: round, input, v, bodies: pages, attested: null });
  assert.ok(!rows.some((r) => r.status === 'MISMATCH'), JSON.stringify(rows.filter((r) => r.status === 'MISMATCH')));
  assert.equal(by(rows, /signature checked by the contract/).filter((r) => r.status === 'MATCH').length, 3);
  assert.equal(by(rows, /quote found in page/).filter((r) => r.status === 'MATCH').length, 2);
  assert.equal(by(rows, /quote not on the page/).filter((r) => r.status === 'MATCH').length, 1);
  assert.equal(by(rows, /outcome follows/)[0].status, 'MATCH');
});

test('a forged signature is a MISMATCH', () => {
  const bad = JSON.parse(fixture.output);
  const g = Buffer.from(bad.j[0].g, 'base64url');
  g[0] ^= 1;
  bad.j[0].g = g.toString('base64url');
  const rows = checkV2({ r: round, input, v: bad, bodies: pages, attested: null });
  assert.equal(by(rows, /judge 0 .*signature checked by the contract/)[0].status, 'MISMATCH');
});

test('a quote that is not in the saved page bytes is a MISMATCH when the program claimed it counted', () => {
  const rows = checkV2({ r: round, input, v, bodies: [Buffer.from('nothing here'), Buffer.from('nor here')], attested: null });
  assert.ok(rows.some((r) => r.status === 'MISMATCH'));
});

test('a changed page breaks the excerpt line', () => {
  const rows = checkV2({ r: round, input, v, bodies: [Buffer.from('other text, Near Protocol launched its mainnet in 2020.'), pages[1]], attested: null });
  assert.equal(by(rows, /excerpt is cut/)[0].status, 'MISMATCH');
});

test('missing page bytes cannot be verified, never a match', () => {
  const rows = checkV2({ r: round, input, v, bodies: [null, null], attested: null });
  assert.equal(by(rows, /quote found in page/)[0].status, 'CANNOT VERIFY');
});

test('an attested key list is compared', () => {
  const key = Buffer.from(v.j[0].k, 'base64url').toString('hex');
  const ok = checkV2({ r: round, input, v, bodies: pages, attested: { [input.m[0].toLowerCase()]: [key] } });
  assert.equal(by(ok, /judge 0 .*Intel/)[0].status, 'MATCH');
  const no = checkV2({ r: round, input, v, bodies: pages, attested: { [input.m[0].toLowerCase()]: ['00'.repeat(32)] } });
  assert.equal(by(no, /judge 0 .*Intel/)[0].status, 'MISMATCH');
});

test('ed25519Ok refuses a wrong length', () => {
  assert.equal(ed25519Ok('x', Buffer.alloc(10), Buffer.alloc(32)), false);
});
