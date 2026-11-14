import test from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
const W = createRequire(import.meta.url)('../../site/wikilink.js');

const LAGOS = 'Lagos, or Lagos City, is a large metropolitan port city in southwestern Nigeria. With a population of about 17 million, it is the largest city in Nigeria and the second largest in Africa. The city is on the Atlantic coast.';

test('empty or missing values give empty text and no crash', () => {
  assert.deepEqual(W.readParams(''), { fromWiki: false, topic: '', claim: '', notes: [], present: false });
  const r = W.readParams('?from=wiki&topic=Lagos&claim=');
  assert.equal(r.claim, ''); assert.ok(r.notes.includes('empty')); assert.equal(r.fromWiki, true);
});

test('only the exact word wiki counts as coming from Wikipedia', () => {
  for (const f of ['wikipedia', 'WIKI', 'agent', '']) assert.equal(W.readParams('?from=' + f + '&topic=x').fromWiki, false, f);
});

test('script tags stay plain text, they are not stripped or run', () => {
  const r = W.readParams('?from=wiki&topic=Lagos&claim=' + encodeURIComponent('<script>alert(1)</script> Lagos is big'));
  assert.equal(r.claim, '<script>alert(1)</script> Lagos is big');
});

test('a 1000 character selection is cut to 280 at a word break', () => {
  const r = W.readParams('?claim=' + encodeURIComponent('word '.repeat(200)));
  assert.ok(r.claim.length <= 280); assert.ok(r.claim.endsWith('word')); assert.ok(r.notes.includes('cut'));
});

test('links are removed so the court will not refuse the sentence', () => {
  const r = W.readParams('?claim=' + encodeURIComponent('Lagos is big https://evil.example/x and old'));
  assert.ok(!/https?:/.test(r.claim)); assert.ok(r.notes.includes('links-removed'));
  assert.equal(r.claim, 'Lagos is big and old');
});

test('footnote marks and hidden characters are removed', () => {
  assert.equal(W.cleanText('Lagos is big[12] and old[citation needed][a].'), 'Lagos is big and old.');
  assert.equal(W.cleanText('La​gos ‮is⁦ big'), 'La gos is big');
});

test('topic: underscores, fragment and length', () => {
  assert.equal(W.cleanTopicParam('Lagos_State#History'), 'Lagos State');
  assert.equal(W.cleanTopicParam('a'.repeat(300)).length, 120);
});

test('bad percent encoding does not crash', () => {
  const r = W.readParams('?topic=%E0%A4%A&claim=%zz');
  assert.equal(typeof r.topic, 'string'); assert.equal(typeof r.claim, 'string');
});

test('summary check: intro sentence passes, a deep sentence does not', () => {
  assert.equal(W.summaryCheck('Lagos is a large metropolitan port city in southwestern Nigeria.', 'Lagos', LAGOS).ok, true);
  const deep = W.summaryCheck('Afrobeats musicians such as Fela Kuti performed at the Afrika Shrine nightclub.', 'Lagos', LAGOS);
  assert.equal(deep.ok, false); assert.ok(deep.missing.includes('afrobeats'));
});

test('intro sentences are split from the summary', () => {
  const s = W.introSentences(LAGOS, 3);
  assert.equal(s.length, 3); assert.ok(s[0].startsWith('Lagos, or Lagos City'));
});

test('bookmark is one line, starts with javascript: and carries the origin', () => {
  const h = W.bookmarkHref('http://localhost:8080/', 'Only English Wikipedia.', 'Open an article first.');
  assert.ok(h.startsWith('javascript:')); assert.ok(h.includes('http://localhost:8080/try.html')); assert.ok(!/[\r\n]/.test(h));
  assert.ok(h.length < 1200);
  assert.doesNotThrow(() => new Function(h.slice('javascript:'.length)));
});
