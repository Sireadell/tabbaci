#!/usr/bin/env node
// Build the 200-sentence accuracy set: 100 true and 100 false "capital of" sentences.
// Truth comes from Wikidata (current sovereign states with exactly one capital). The evidence the judges see is a 300-character window of the country's
// Wikipedia summary around the word "capital", cut by the same code the courtroom uses.
// A sentence is only kept when the summary actually names the capital, so every true one can be checked from its excerpt.
import { writeFileSync } from 'node:fs';
import { sha256hex } from '../verify/crypto.mjs';
import { windowOf } from '../verify/frames.mjs';

const UA = { 'user-agent': 'courtroom-fetch/0.1' };
const sparql = `SELECT ?cLabel ?capLabel WHERE { ?c wdt:P31 wd:Q3624078; wdt:P36 ?cap. FILTER NOT EXISTS { ?c wdt:P576 ?end } SERVICE wikibase:label { bd:serviceParam wikibase:language "en". } }`;
const wd = await (await fetch("https://query.wikidata.org/sparql?format=json&query=" + encodeURIComponent(sparql), { headers: { ...UA, accept: "application/sparql-results+json" } })).json();
const byCountry = {};
for (const b of wd.results.bindings) (byCountry[b.cLabel.value] ||= new Set()).add(b.capLabel.value);
const countries = Object.entries(byCountry).filter(([, v]) => v.size === 1).map(([k, v]) => ({ name: k, capital: [...v][0] }));
const pool = countries;
pool.sort((a, b) => a.name.localeCompare(b.name));
const kept = [];
for (const c of pool) {
  if (kept.length >= 100) break;
  try {
    const r = await fetch(`https://en.wikipedia.org/api/rest_v1/page/summary/${encodeURIComponent(c.name.replace(/ /g, '_'))}`, { headers: UA });
    if (!r.ok) continue;
    const j = await r.json();
    const extract = j.extract || '';
    const bytes = Buffer.from(extract, 'utf8');
    const w = windowOf(bytes, ['capital'], 300);
    if (!w || !w.includes(c.capital)) continue;
    kept.push({ ...c, excerpt: w, source_hash: sha256hex(bytes) });
  } catch { /* skip */ }
}
const items = [];
kept.forEach((c, i) => {
  const wrong = kept[(i + 37) % kept.length].capital; // a capital from another country
  if (wrong === c.capital || c.excerpt.includes(wrong)) return;
  items.push({ id: `T${i}`, label: 'T', claim: `The capital of ${c.name} is ${c.capital}.`, excerpt: c.excerpt });
  items.push({ id: `F${i}`, label: 'F', claim: `The capital of ${c.name} is ${wrong}.`, excerpt: c.excerpt });
});
writeFileSync(new URL('dataset.json', import.meta.url), JSON.stringify(items, null, 1));
console.log(`countries kept ${kept.length}, sentences ${items.length} (${items.filter((x) => x.label === 'T').length} true, ${items.filter((x) => x.label === 'F').length} false)`);
