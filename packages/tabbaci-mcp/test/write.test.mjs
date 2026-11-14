import test from 'node:test';
import assert from 'node:assert/strict';
import { createClient, makeWriter } from '../src/index.mjs';
import { createServer } from '../src/server.mjs';

const STAKE = '50000000000000000000000';

// A fake site: records every POST and answers like api/write.js does.
function fakeSite({ sendError, runFailsFirst = 0 } = {}) {
  const calls = [];
  let runFails = runFailsFirst;
  const fetchImpl = async (url, init) => {
    const body = JSON.parse(init.body);
    calls.push({ url, body });
    const res = (status, j) => ({ ok: status < 400, status, json: async () => j });
    if (body.action === 'run') {
      if (runFails > 0) { runFails--; return res(502, { error: 'The chain did not take the sentence just now. Try again in a minute.' }); }
      return res(200, { round: body.round, started: true, tx: 'RUNTX' });
    }
    if (sendError) return res(429, { error: sendError, limit: 'agent' });
    return res(200, { round: 41, word: body.word, page: body.topic, sources: ['https://en.wikipedia.org/x', 'https://www.wikidata.org/y'], terms: ['city'], tx: { open: 'OPENTX', bond: 'BONDTX' }, left: 24, cap: 30, account: 'tabbaci-visitors.testnet', from: 'agent' });
  };
  return { calls, fetchImpl };
}

// A fake chain: get_round answers from a list, one entry per poll (the last one repeats).
function fakeChain(states) {
  let i = 0;
  const reads = [];
  return {
    reads,
    rpc: async (method, params) => {
      const args = JSON.parse(Buffer.from(params.args_base64, 'base64').toString());
      reads.push(params.method_name);
      const r = states[Math.min(i++, states.length - 1)];
      return { result: Array.from(Buffer.from(JSON.stringify(r === undefined ? null : typeof r === 'function' ? r(args) : r))) };
    },
  };
}

const round = (over) => ({
  claim: 'Lagos is the capital of Nigeria.', tag: 'agent', status: 'open', outcome: null, opener: 'tabbaci-visitors.testnet', opened_block: 100,
  input: JSON.stringify({ kind: 'prose', m: ['judge-a', 'judge-b', 'judge-c'] }), verdict: null, pending: false, attempts: [],
  bonds: [{ writer: 'tabbaci-visitors.testnet', word: 'is', stake: STAKE, payout: null }], ...over,
});
const ruled = (outcome, word = 'is') => round({ status: 'settled', outcome, verdict: JSON.stringify({ j: [{ m: 0, a: outcome, g: 'sig' }, { m: 1, a: outcome, g: 'sig' }, { m: 2, a: outcome, g: 'sig' }] }),
  attempts: [{ n: 1, start_block: 110, end_block: 130, result: 'done' }], bonds: [{ writer: 'tabbaci-visitors.testnet', word, stake: STAKE, payout: null }] });
const fast = { sleep: async () => {}, everyMs: 0 };

test('the writer marks every sentence as sent by an AI agent', async () => {
  const s = fakeSite();
  const w = makeWriter({ site: 'https://example.test', fetchImpl: s.fetchImpl });
  await w.send({ sentence: 'Lagos is big.', topic: 'Lagos', word: 'likely' });
  assert.equal(s.calls[0].url, 'https://example.test/api/write');
  assert.deepEqual(s.calls[0].body, { claim: 'Lagos is big.', topic: 'Lagos', word: 'likely', from: 'agent' });
});

test('bond_sentence sends, starts the court, and returns the round and links', async () => {
  const s = fakeSite();
  const c = createClient({ fetchImpl: s.fetchImpl, rpc: fakeChain([]).rpc });
  const b = await c.bondSentence({ sentence: ' Lagos is the largest city in Nigeria. ', wikipedia_page: 'Lagos', sureness: 'Very Likely' });
  assert.equal(b.round, 41);
  assert.equal(b.word, 'very likely');
  assert.equal(b.sentence, 'Lagos is the largest city in Nigeria.');
  assert.equal(b.court_started, true);
  assert.equal(b.record_url, 'https://tabbaci.vercel.app/case.html?round=41');
  assert.equal(b.bond_tx_url, 'https://testnet.nearblocks.io/txns/BONDTX');
  assert.equal(b.run_tx_url, 'https://testnet.nearblocks.io/txns/RUNTX');
  assert.deepEqual(s.calls.map((x) => x.body.action || 'send'), ['send', 'run']);
  assert.deepEqual(s.calls[1].body, { action: 'run', round: 41 });
});

test('bond_sentence tries the court start a second time, then reports it did not start', async () => {
  const once = fakeSite({ runFailsFirst: 1 });
  const a = await createClient({ fetchImpl: once.fetchImpl }).bondSentence({ sentence: 'Lagos is in Nigeria.', wikipedia_page: 'Lagos', sureness: 'is' });
  assert.equal(a.court_started, true);
  assert.equal(once.calls.filter((x) => x.body.action === 'run').length, 2);
  const twice = fakeSite({ runFailsFirst: 2 });
  const b = await createClient({ fetchImpl: twice.fetchImpl }).bondSentence({ sentence: 'Lagos is in Nigeria.', wikipedia_page: 'Lagos', sureness: 'is' });
  assert.equal(b.court_started, false);
  assert.match(b.court_error, /did not take/);
});

test('bond_sentence checks its input before calling the site', async () => {
  const s = fakeSite();
  const c = createClient({ fetchImpl: s.fetchImpl });
  await assert.rejects(c.bondSentence({ sentence: 'Lagos is in Nigeria.', wikipedia_page: 'Lagos', sureness: 'certainly' }), /sureness/);
  await assert.rejects(c.bondSentence({ sentence: '', wikipedia_page: 'Lagos', sureness: 'is' }), /sentence/);
  await assert.rejects(c.bondSentence({ sentence: 'Lagos is in Nigeria.', wikipedia_page: ' ', sureness: 'is' }), /Wikipedia/);
  assert.equal(s.calls.length, 0);
});

test('mcp: bond_sentence passes on the site limit message', async () => {
  const s = fakeSite({ sendError: 'An AI can send 6 sentences an hour from here. Wait for the rulings, then send more.' });
  const h = createServer(createClient({ fetchImpl: s.fetchImpl }));
  const r = (await h({ jsonrpc: '2.0', id: 1, method: 'tools/call', params: { name: 'bond_sentence', arguments: { sentence: 'Lagos is in Nigeria.', wikipedia_page: 'Lagos', sureness: 'is' } } })).result;
  assert.equal(r.isError, true);
  assert.match(r.content[0].text, /6 sentences an hour/);
});

test('mcp: bond_sentence text says round, play money and the wait', async () => {
  const s = fakeSite();
  const h = createServer(createClient({ fetchImpl: s.fetchImpl }));
  const r = (await h({ jsonrpc: '2.0', id: 1, method: 'tools/call', params: { name: 'bond_sentence', arguments: { sentence: 'Lagos is in Nigeria.', wikipedia_page: 'Lagos', sureness: 'is' } } })).result;
  assert.match(r.content[0].text, /Round 41/);
  assert.match(r.content[0].text, /play money/);
  assert.match(r.content[0].text, /about 4 minutes/);
  assert.equal(r.structuredContent.round, 41);
});

test('wait_for_ruling polls until the court rules, and a false sentence said with "is" is fined', async () => {
  const chain = fakeChain([round({ pending: true }), round({ attempts: [{ n: 1, start_block: 110, result: 'running' }] }), ruled('F')]);
  const c = createClient({ rpc: chain.rpc, fetchImpl: fakeSite().fetchImpl, wait: fast });
  const w = await c.waitForRuling(41);
  assert.equal(w.settled, true);
  assert.equal(w.outcome, 'F');
  assert.equal(w.fined, true);
  assert.equal(w.fine_paid, false);
  assert.match(w.money, /Fined/);
  assert.equal(w.judges.length, 3);
  assert.equal(chain.reads.length, 3);
});

test('wait_for_ruling: true is not fined, and "disputed" is never fined', async () => {
  const t = await createClient({ rpc: fakeChain([ruled('T')]).rpc, wait: fast }).waitForRuling(41);
  assert.equal(t.fined, false);
  assert.match(t.money, /small gain/);
  const d = await createClient({ rpc: fakeChain([ruled('F', 'disputed')]).rpc, wait: fast }).waitForRuling(41);
  assert.equal(d.fined, false);
  assert.match(d.money, /back in full/);
});

test('wait_for_ruling starts the next run when one came back empty, once per empty run', async () => {
  const empty = round({ attempts: [{ n: 1, start_block: 110, end_block: 120, result: 'empty' }] });
  const s = fakeSite();
  const c = createClient({ rpc: fakeChain([empty, empty, empty, ruled('T')]).rpc, fetchImpl: s.fetchImpl, wait: fast });
  const w = await c.waitForRuling(41);
  assert.equal(w.outcome, 'T');
  assert.equal(s.calls.filter((x) => x.body.action === 'run').length, 1);
});

test('wait_for_ruling gives up after the time cap and says the round stays open', async () => {
  let t = 0;
  const c = createClient({ rpc: fakeChain([round({ pending: true })]).rpc, wait: { sleep: async () => {}, now: () => (t += 60_000), maxMs: 10 * 60_000 } });
  const w = await c.waitForRuling(41);
  assert.equal(w.settled, false);
  assert.equal(w.outcome, 'open');
  assert.match(w.note, /stays open/);
});

test('wait_for_ruling on a round that does not exist', async () => {
  const c = createClient({ rpc: fakeChain([null]).rpc, wait: fast });
  await assert.rejects(c.waitForRuling(9999), /no round 9999/);
  await assert.rejects(c.waitForRuling(0), /whole number/);
});

test('mcp: wait_for_ruling text', async () => {
  const h = createServer(createClient({ rpc: fakeChain([ruled('F')]).rpc, wait: fast }));
  const r = (await h({ jsonrpc: '2.0', id: 1, method: 'tools/call', params: { name: 'wait_for_ruling', arguments: { round: 41 } } })).result;
  assert.match(r.content[0].text, /Round 41: FALSE/);
  assert.match(r.content[0].text, /Fined: it was said with "is"/);
  assert.match(r.content[0].text, /case\.html\?round=41/);
});

// A fake Wikipedia and Wikidata for read_evidence.
const fakeWiki = (summary = { type: 'standard', title: 'Lagos', extract: 'Lagos is the largest city in Nigeria.', wikibase_item: 'Q8673' }, status = 200) => {
  const urls = [];
  const fetchImpl = async (url) => {
    urls.push(url);
    if (url.includes('wikidata')) return { ok: true, status: 200, json: async () => 'largest city and former capital of Nigeria' };
    return { ok: status < 400, status, json: async () => summary };
  };
  return { urls, fetchImpl };
};

test('read_evidence returns the summary and Wikidata line the court reads', async () => {
  const w = fakeWiki();
  const e = await createClient({ fetchImpl: w.fetchImpl }).readEvidence(' Lagos State ');
  assert.equal(e.wikipedia_summary, 'Lagos is the largest city in Nigeria.');
  assert.equal(e.wikidata_description, 'largest city and former capital of Nigeria');
  assert.equal(w.urls[0], 'https://en.wikipedia.org/api/rest_v1/page/summary/Lagos_State?redirect=true');
  assert.equal(w.urls[1], 'https://www.wikidata.org/w/rest.php/wikibase/v1/entities/items/Q8673/descriptions/en');
});

test('read_evidence: missing page, ambiguous page, empty name', async () => {
  await assert.rejects(createClient({ fetchImpl: fakeWiki({}, 404).fetchImpl }).readEvidence('Nowhere'), /no page called/);
  await assert.rejects(createClient({ fetchImpl: fakeWiki({ type: 'disambiguation', title: 'X' }).fetchImpl }).readEvidence('X'), /several things/);
  await assert.rejects(createClient({ fetchImpl: fakeWiki().fetchImpl }).readEvidence(''), /page name/);
});

test('mcp: read_evidence text warns that anything else comes back unsure', async () => {
  const h = createServer(createClient({ fetchImpl: fakeWiki().fetchImpl }));
  const r = (await h({ jsonrpc: '2.0', id: 1, method: 'tools/call', params: { name: 'read_evidence', arguments: { wikipedia_page: 'Lagos' } } })).result;
  assert.match(r.content[0].text, /largest city in Nigeria/);
  assert.match(r.content[0].text, /former capital/);
  assert.match(r.content[0].text, /comes back UNSURE/);
});

test('bond_sentence says "disputed" can never be fined', async () => {
  const { TOOLS } = await import('../src/server.mjs');
  assert.match(TOOLS.find((t) => t.name === 'bond_sentence').description, /can never be fined/);
});
