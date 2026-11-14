// Lets a visitor send a sentence to the court with no wallet. The site's own test account opens the round,
// bonds the visitor's word and pays the run fee, all in test NEAR. The ruling is real and lands on the chain.
//   GET  /api/write  -> today's count and cap. Also closes one visitor round whose objection window has ended.
//   POST /api/write  {topic, claim, word} -> {round, ...}
import { loadKey, view, call, head } from './_lib/near.js';
import { cleanClaim, cleanTopic, cleanWord, pickTerms, buildRound, countSince, makeLimiter, CAP_PER_DAY, PER_VISITOR_PER_HOUR, DAY_BLOCKS, WIKI, WIKIDATA } from './_lib/rules.js';

const CONTRACT = process.env.TABBACI_CONTRACT || 'finalenc2.testnet';
const ACCOUNT = process.env.RELAY_ACCOUNT || 'tabbaci-visitors.testnet';
const UA = { 'user-agent': 'Tabbaci/1.0 (https://tabbaci.vercel.app)' };
const perVisitor = makeLimiter(PER_VISITOR_PER_HOUR, 3600_000);
let keyCache;
const T0 = { t: 0 };
const step = (name) => { if (process.env.WRITE_DEBUG) console.log(name, Date.now() - T0.t, 'ms'); };
const key = () => (keyCache ||= loadKey(process.env.RELAY_KEY || ''));

// Who opened a round and when never changes, so it is read once and kept while the function stays warm.
const meta = new Map();
async function roundMeta(i) {
  if (meta.has(i)) return meta.get(i);
  const r = await view(CONTRACT, 'get_round', { round: i }).catch(() => null);
  if (!r) return null;
  const m = { id: i, opener: r.opener, opened_block: r.opened_block, status: r.status, role: r.role };
  if (r.status !== 'open') meta.set(i, m);
  return m;
}

// Rounds newest first, going back until rounds are older than `since` or `max` rounds were read.
async function recentRounds(since, max = 120) {
  const n = await view(CONTRACT, 'round_count');
  const out = [];
  for (let top = n; top >= 1 && n - top < max; top -= 25) {
    const ids = []; for (let i = top; i > Math.max(0, top - 25); i--) ids.push(i);
    const batch = (await Promise.all(ids.map(roundMeta))).filter(Boolean);
    out.push(...batch);
    if (batch.every((r) => r.opened_block < since)) break;
  }
  return out;
}

async function status() {
  const h = await head();
  const rounds = await recentRounds(h - DAY_BLOCKS);
  const used = countSince(rounds, ACCOUNT, h - DAY_BLOCKS);
  return { h, rounds, used, cap: CAP_PER_DAY, left: Math.max(0, CAP_PER_DAY - used), account: ACCOUNT, contract: CONTRACT };
}

// Anyone may close a finished case once its objection window ends. Doing it here pays the visitor bonds back.
async function sweep(h, rounds) {
  for (const r of rounds) {
    if (r.opener !== ACCOUNT || r.status === 'open' || r.role !== 'original') continue;
    const c = await view(CONTRACT, 'get_case', { round: r.id }).catch(() => null);
    if (!c || c.finalized || h < c.window_ends) continue;
    try { const t = await call({ account: ACCOUNT, key: key(), contract: CONTRACT, method: 'finalize', args: { round: r.id }, tgas: 50, wait: 'INCLUDED' }); return { finalized: r.id, tx: t.hash }; }
    catch (e) { return { finalize_error: String(e.message).slice(0, 120) }; }
  }
  return null;
}

async function findPage(topic) {
  const title = topic.replace(/ /g, '_');
  const r = await fetch(WIKI + encodeURIComponent(title) + '?redirect=true', { headers: UA, signal: AbortSignal.timeout(10000) });
  if (r.status === 404) return { error: `Wikipedia has no page called "${topic}". Try the exact page name.` };
  if (!r.ok) return { error: 'Wikipedia did not answer. Try again in a minute.' };
  const page = await r.json();
  if (page.type === 'disambiguation') return { error: `"${topic}" can mean several things on Wikipedia. Pick a more exact page name.` };
  const canonical = page.titles && page.titles.canonical;
  const qid = page.wikibase_item;
  if (!canonical || !/^Q\d+$/.test(qid || '')) return { error: 'That Wikipedia page has no Wikidata entry, so the court cannot find a second source for it.' };
  // Read the exact page the court will read, so the key words are chosen from the same text.
  const exact = await fetch(WIKI + encodeURIComponent(canonical), { headers: UA, signal: AbortSignal.timeout(10000) });
  if (!exact.ok) return { error: 'Wikipedia did not answer. Try again in a minute.' };
  const body = await exact.text();
  const wd = await fetch(`${WIKIDATA}${qid}/descriptions/en`, { headers: UA, signal: AbortSignal.timeout(10000) });
  if (!wd.ok) return { error: 'That topic has no English description on Wikidata, so the court has only one source. Try a better known topic.' };
  return { canonical, qid, body, title: page.title };
}

// Start the court on a round the site opened. Refuses any other round, so the site never pays for someone else's.
async function run(round) {
  if (!Number.isInteger(round) || round < 1) return { error: 'No round number.' };
  const r = await view(CONTRACT, 'get_round', { round });
  if (!r || r.opener !== ACCOUNT) return { error: 'The site only starts rulings on sentences it sent.' };
  if (r.status !== 'open') return { round, started: true, ruled: true };
  // A run that came back empty leaves the round open. The contract allows three runs, so start the next one.
  if (r.pending || r.attempts.some((x) => x.result === 'running')) return { round, started: true, already: true };
  if (r.attempts.length >= 3) return { error: 'The court has already tried three times on this round.' };
  const t = await call({ account: ACCOUNT, key: key(), contract: CONTRACT, method: 'attempt', args: { round }, tgas: 300, deposit: '0.02', wait: 'INCLUDED' });
  step('court started');
  return { round, started: true, tx: t.hash };
}

async function readBody(req) {
  if (req.body && typeof req.body === 'object') return req.body;
  if (typeof req.body === 'string') { try { return JSON.parse(req.body); } catch { return {}; } }
  const chunks = []; for await (const c of req) chunks.push(c);
  try { return JSON.parse(Buffer.concat(chunks).toString() || '{}'); } catch { return {}; }
}

export default async function handler(req, res) {
  res.setHeader('cache-control', 'no-store');
  if (!process.env.RELAY_KEY) return res.status(503).json({ error: 'Sending from the site is switched off right now.' });
  try {
    if (req.method === 'GET') {
      const s = await status();
      const swept = await sweep(s.h, s.rounds).catch(() => null);
      return res.status(200).json({ used: s.used, cap: s.cap, left: s.left, account: s.account, contract: s.contract, swept });
    }
    if (req.method !== 'POST') return res.status(405).json({ error: 'Use GET or POST.' });

    T0.t = Date.now();
    const b = await readBody(req);
    if (b.action === 'run') return res.status(200).json(await run(Number(b.round)));

    const c = cleanClaim(b.claim); if (c.error) return res.status(400).json({ error: c.error });
    const t = cleanTopic(b.topic); if (t.error) return res.status(400).json({ error: t.error });
    const w = cleanWord(b.word); if (w.error) return res.status(400).json({ error: w.error });

    const p = await findPage(t.topic); if (p.error) return res.status(400).json({ error: p.error });
    step('page found');
    const terms = pickTerms(c.claim, p.canonical, p.body);
    if (!terms.length) return res.status(400).json({ error: 'The court could not find the article text on that page. Try another topic.' });

    const s = await status();
    step('cap read');
    if (s.left <= 0) return res.status(429).json({ error: `Today's ${s.cap} free sentences are used up. Come back tomorrow, or watch a real ruling.`, limit: 'day' });
    const who = String(req.headers['x-forwarded-for'] || req.socket?.remoteAddress || 'x').split(',')[0].trim();
    if (!perVisitor(who)) return res.status(429).json({ error: `You can send ${PER_VISITOR_PER_HOUR} sentences an hour from here. Watch a real ruling while you wait.`, limit: 'visitor' });

    const k = key();
    const [stake, bal] = await Promise.all([view(CONTRACT, 'stake'), view(CONTRACT, 'balance', { account: ACCOUNT })]);
    if (BigInt(bal) < BigInt(stake)) await call({ account: ACCOUNT, key: k, contract: CONTRACT, method: 'deposit', args: {}, tgas: 30, deposit: '1' });
    step('stake ready');

    const args = buildRound({ claim: c.claim, canonical: p.canonical, qid: p.qid, terms });
    const open = await call({ account: ACCOUNT, key: k, contract: CONTRACT, method: 'open_round', args, tgas: 100 });
    step('opened');
    const round = Number(open.value);
    if (!round) throw new Error('the round did not open');
    const bond = await call({ account: ACCOUNT, key: k, contract: CONTRACT, method: 'bond', args: { round, word: w.word }, tgas: 50 });
    step('bonded');
    return res.status(200).json({
      round, contract: CONTRACT, word: w.word, page: p.title, sources: args.picks.map((x) => x.url), terms,
      tx: { open: open.hash, bond: bond.hash }, left: s.left - 1, cap: s.cap, account: ACCOUNT,
    });
  } catch (e) {
    console.error('write failed:', e && e.message, e && e.cause && (e.cause.code || e.cause.message));
    return res.status(502).json({ error: 'The chain did not take the sentence just now. Try again in a minute.' });
  }
}
