import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';

const src = readFileSync(fileURLToPath(new URL('../../site/v2checks.js', import.meta.url)), 'utf8');
const win = {};
vm.runInNewContext(src, { window: win, TextEncoder, TextDecoder, crypto: globalThis.crypto, atob });
const { readable, quoteIn } = win.V2;

test('readable turns page-data leftovers into plain prose', () => {
  assert.equal(readable('Mount Everest is the highest mountain on Earth above sea level.'), 'Mount Everest is the highest mountain on Earth above sea level.');
  const B = String.fromCharCode(92); // one backslash, as it appears inside page data
  assert.equal(readable('the China' + B + 'u2013Nepal border' + B + 'nand ' + B + '"more' + B + '" &amp; more'), 'the China–Nepal border and "more" & more');
  assert.equal(readable('<b>Bold</b>   text'), 'Bold text');
});

test('a very long quote is shortened at a word, and the full quote still passes the check', () => {
  const long = 'word '.repeat(80).trim();
  const r = readable(long);
  assert.ok(r.length <= 224 && r.endsWith('...'));
  assert.ok(quoteIn(long, long));
});
