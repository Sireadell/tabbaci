#!/usr/bin/env node
// Merge content/pages/*.json with their *.published.json into site/pages/*.json and an index.
import { readdirSync, readFileSync, writeFileSync, existsSync } from 'node:fs';
const dir = new URL('../../content/pages/', import.meta.url);
const out = new URL('../../site/pages/', import.meta.url);
const index = [];
for (const f of readdirSync(dir).filter((x) => x.endsWith('.json') && !x.endsWith('.published.json'))) {
  const page = JSON.parse(readFileSync(new URL(f, dir), 'utf8'));
  const pubFile = new URL(f.replace(/\.json$/, '.published.json'), dir);
  if (!existsSync(pubFile)) continue;
  const pub = JSON.parse(readFileSync(pubFile, 'utf8'));
  const sentences = page.sentences.map((s, i) => ({ claim: s.claim, word: s.word, writer: s.writer, kind: s.kind, round: pub.rounds[i].round }));
  writeFileSync(new URL(`${page.slug}.json`, out), JSON.stringify({ slug: page.slug, title: page.title, intro: page.intro, written_by: page.written_by, contract: pub.contract, sentences }, null, 2));
  index.push({ slug: page.slug, title: page.title, count: sentences.length });
}
writeFileSync(new URL('index.json', out), JSON.stringify(index, null, 2));
console.log('pages:', index.map((p) => p.slug).join(', '));
