// Client for the Tabbaci contract. No dependencies, Node 20+.
// Reading is plain NEAR view calls: no key, no account. Sending a sentence goes through the
// site's free sender (tabbaci.vercel.app/api/write), which bonds it with test money on testnet.
// The caller never holds a wallet or a key.

export const DEFAULTS = {
  contract: 'finalenc2.testnet',
  network: 'testnet',
  rpcs: ['https://test.rpc.fastnear.com', 'https://rpc.testnet.near.org'],
  site: 'https://tabbaci.vercel.app',
};

export const WORDS = ['is', 'very likely', 'likely', 'disputed'];

const WORD = { T: 'true', F: 'false', U: 'unsure', D: 'judges split' };

// Same approach as site/app.js: try each node, wait, try again.
export function makeTransport({ rpcs = DEFAULTS.rpcs, fetchImpl = globalThis.fetch, tries = 3 } = {}) {
  return async function rpc(method, params) {
    for (let round = 0; round < tries; round++) {
      for (const url of rpcs) {
        try {
          const r = await fetchImpl(url, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }) });
          if (!r.ok) continue;
          const j = await r.json();
          if (j.error) { const e = new Error('rpc error'); e.rpc = j.error; throw e; }
          return j.result;
        } catch (e) { if (e.message === 'rpc error') throw e; }
      }
      await new Promise((ok) => setTimeout(ok, 800 * (round + 1)));
    }
    throw new Error('no NEAR node answered');
  };
}

export function createClient(opts = {}) {
  const contract = opts.contract || (typeof process !== 'undefined' && process.env.TABBACI_CONTRACT) || DEFAULTS.contract;
  const network = opts.network || DEFAULTS.network;
  const rpc = opts.rpc || makeTransport(opts);
  const explorer = `https://${network === 'mainnet' ? '' : 'testnet.'}nearblocks.io`;

  async function view(method, args = {}) {
    const res = await rpc('query', { request_type: 'call_function', finality: 'final', account_id: contract, method_name: method, args_base64: Buffer.from(JSON.stringify(args)).toString('base64') });
    if (res.error) throw new Error(String(res.error).slice(0, 200));
    return JSON.parse(Buffer.from(res.result).toString());
  }

  // The ruling transaction is not stored on the round. Look for the "attempt" call
  // in the few blocks before the last attempt started. Old blocks may be gone from public nodes.
  async function findRulingTx(startBlock) {
    for (let h = startBlock - 1; h >= startBlock - 3; h--) {
      try {
        const blk = await rpc('block', { block_id: h });
        for (const c of blk.chunks) {
          if (c.tx_root === '11111111111111111111111111111111') continue;
          const ch = await rpc('chunk', { chunk_id: c.chunk_hash });
          const t = ch.transactions.find((x) => x.receiver_id === contract && x.actions.some((a) => a.FunctionCall && a.FunctionCall.method_name === 'attempt'));
          if (t) return t.hash;
        }
      } catch { /* node may not keep this block */ }
    }
    return null;
  }

  const site = (opts.site || DEFAULTS.site).replace(/\/$/, '');
  const writer = opts.writer || makeWriter({ site, fetchImpl: opts.fetchImpl || globalThis.fetch });

  return {
    contract, network, explorer, site,
    view,
    bondSentence: (a) => bondSentence(writer, { site, explorer }, a),
    readEvidence: (page) => readEvidence(page, { fetchImpl: opts.fetchImpl || globalThis.fetch }),
    waitForRuling: (round, o) => waitForRuling({ view, writer, ctx: { contract, network, explorer, site } }, round, { ...opts.wait, ...o }),
    getContractInfo: () => getContractInfo(view, contract, network),
    listRounds: (o) => listRounds(view, contract, network, o),
    getRuling: (round, o = {}) => getRuling(view, findRulingTx, { contract, network, explorer }, round, o),
    findSentence: (text, o) => findSentence(view, contract, network, text, o),
    checkCommand: (round) => checkCommand(round, { contract, network }),
  };
}

export function checkCommand(round, { contract = DEFAULTS.contract, network = DEFAULTS.network } = {}) {
  const n = Number(round);
  if (!Number.isInteger(n) || n < 1) throw new Error('round must be a whole number, 1 or higher');
  return `node tools/verify/verify.mjs --contract ${contract} --round ${n} --archive archive --network ${network} --attested-judges --rpc https://test.rpc.fastnear.com`;
}

const parse = (s) => { try { return JSON.parse(s); } catch { return null; } };
const bigNear = (y) => (Number(BigInt(y) / 10n ** 18n) / 1e6).toFixed(4);

export function describeOutcome(r) {
  if (r.status === 'open') return { letter: 'open', label: 'waiting for a ruling' };
  if (r.status === 'void') return { letter: 'void', label: r.void_reason === 'SRC_DISAGREE' ? 'cancelled: the pages disagree' : 'cancelled: 3 failed runs' };
  return { letter: r.outcome, label: WORD[r.outcome] || r.outcome };
}

// Plain-words money state for one bond. Mirrors site/certificate.html.
export function bondState(b, outcome) {
  const stake = BigInt(b.stake);
  const base = { writer: b.writer, word: b.word, stake_near: bigNear(stake), stake_yocto: String(b.stake) };
  if (!b.payout) {
    const ruled = outcome === 'F' || outcome === 'T';
    return { ...base, paid: false, payout_near: null, state: ruled ? 'not paid yet: paid after the 5.8 hour challenge window closes and someone calls finalize' : 'waiting for a ruling' };
  }
  const p = BigInt(b.payout);
  const pct = (n) => (Number((n * 10000n) / stake) / 100).toFixed(2) + '%';
  const state = p === stake ? 'stake paid back in full, no gain'
    : p < stake ? `fined ${bigNear(stake - p)} NEAR (${pct(stake - p)} of the stake)`
    : `gained ${bigNear(p - stake)} NEAR (${pct(p - stake)} of the stake)`;
  return { ...base, paid: true, payout_near: bigNear(p), payout_yocto: String(b.payout), state };
}

export function parseRound(id, r, { network = DEFAULTS.network, contract = DEFAULTS.contract } = {}) {
  const input = parse(r.input) || {};
  const verdict = parse(r.verdict);
  const models = input.m || [];
  const { letter, label } = describeOutcome(r);
  const kind = input.kind === 'data' ? 'data' : 'prose';
  const checked = r.checked || [];
  const judges = verdict && verdict.j ? verdict.j.map((j) => ({ model: models[j.m] || `judge ${j.m}`, answer: j.a, signed: Boolean(j.g), ...(checked[j.m] ? { quote: checked[j.m].quote || null, quote_in_page_excerpt: checked[j.m].quote_ok, signature_checked_by_contract: checked[j.m].sig_ok } : {}) })) : [];
  const sources = verdict && verdict.s ? verdict.s.map((s) => {
    const src = (input.src || []).find((x) => x.i === s.i);
    return { url: src ? src.u : `source ${s.i}`, http_status: s.c, read_value: s.x !== undefined ? s.x : null };
  }) : [];
  const done = r.attempts.filter((a) => a.end_block);
  const last = r.attempts[r.attempts.length - 1];
  const bonds = (r.bonds || []).map((b) => bondState(b, r.outcome));
  return {
    round: id,
    contract, network,
    sentence: r.claim,
    outcome: letter,
    outcome_words: label,
    status: r.status,
    kind,
    kind_note: kind === 'data'
      ? 'Data round: decided by numbers read from live status pages. Those pages change, so this round cannot be re-matched later; the stored record is the proof.'
      : 'Page round: decided by judges reading archived page text, which can be re-checked later.',
    tag: r.tag,
    writer: r.bonds && r.bonds[0] ? r.bonds[0].writer : r.opener,
    opener: r.opener,
    bonds,
    fine_paid: bonds.length ? bonds.every((b) => b.paid) : null,
    challenger: r.challenger || null,
    judges,
    sources,
    judges_confirm_value: kind === 'data' && input.cv ? input.cv : null,
    judge_note: kind === 'data' ? 'On a data round the sources decide the outcome. A judge answer of T only confirms the sentence states the number in judges_confirm_value; it does not say that number is right.' : null,
    opened_block: r.opened_block,
    attempts: r.attempts.map((a) => ({ n: a.n, start_block: a.start_block, end_block: a.end_block || null, result: a.result })),
    ruled_in_blocks: done.length ? done[done.length - 1].end_block - done[done.length - 1].start_block : null,
    last_block: last ? (last.end_block || last.start_block) : null,
    ruling_tx: null,
    ruling_tx_url: null,
  };
}

export async function getRuling(view, findRulingTx, ctx, round, { includeTx = true } = {}) {
  const n = Number(round);
  if (!Number.isInteger(n) || n < 1) throw new Error('round must be a whole number, 1 or higher');
  const r = await view('get_round', { round: n });
  if (!r) return null;
  const out = parseRound(n, r, ctx);
  out.check_command = checkCommand(n, ctx);
  const last = r.attempts[r.attempts.length - 1];
  if (includeTx && last) {
    const hash = await findRulingTx(last.start_block);
    if (hash) { out.ruling_tx = hash; out.ruling_tx_url = `${ctx.explorer}/txns/${hash}`; }
    else out.ruling_tx_url = `${ctx.explorer}/address/${ctx.contract}`;
  }
  return out;
}

export async function getContractInfo(view, contract, network) {
  const [count, courtroom, models, dao, stake, reserve, sources] = await Promise.all([
    view('round_count'), view('courtroom'), view('models'), view('dao'), view('stake'), view('reserve'), view('sources'),
  ]);
  return {
    contract, network,
    round_count: count,
    courtroom_program: courtroom,
    judges: models,
    dao,
    stake_near: bigNear(stake),
    reserve_near: bigNear(reserve),
    approved_sources: sources,
    read_only: true,
  };
}

export async function listRounds(view, contract, network, { limit } = {}) {
  const n = await view('round_count');
  const out = [];
  for (let hi = n; hi >= 1; hi -= 4) {
    const ids = [hi, hi - 1, hi - 2, hi - 3].filter((i) => i >= 1);
    const rs = await Promise.all(ids.map((i) => view('get_round', { round: i }).then((r) => (r ? { id: i, r } : null))));
    for (const x of rs.filter(Boolean)) {
      const p = parseRound(x.id, x.r, { network, contract });
      out.push({ round: p.round, sentence: p.sentence, outcome: p.outcome, outcome_words: p.outcome_words, kind: p.kind, writer: p.writer, fine_paid: p.fine_paid });
    }
    if (limit && out.length >= limit) break;
  }
  return limit ? out.slice(0, limit) : out;
}

export async function findSentence(view, contract, network, text, o = {}) {
  const q = String(text || '').trim().toLowerCase();
  if (!q) throw new Error('give some text to search for');
  const all = await listRounds(view, contract, network, o);
  return all.filter((r) => r.sentence.toLowerCase().includes(q));
}

// ---- the evidence the court will read ----

const WIKI = 'https://en.wikipedia.org/api/rest_v1/page/summary/';
const WIKIDATA = 'https://www.wikidata.org/w/rest.php/wikibase/v1/entities/items/';
const UA = { 'user-agent': 'tabbaci-mcp (https://tabbaci.vercel.app)' };

// The court reads only the opening summary of the Wikipedia page and the one-line Wikidata description.
// Showing that text first lets a writer bond sentences the court can actually check.
export async function readEvidence(page, { fetchImpl = globalThis.fetch } = {}) {
  const topic = String(page ?? '').trim();
  if (!topic) throw new Error('give the Wikipedia page name, for example Lagos');
  const r = await fetchImpl(WIKI + encodeURIComponent(topic.replace(/ /g, '_')) + '?redirect=true', { headers: UA });
  if (r.status === 404) throw new Error(`Wikipedia has no page called "${topic}". Try the exact page name.`);
  if (!r.ok) throw new Error('Wikipedia did not answer. Try again in a minute.');
  const p = await r.json();
  if (p.type === 'disambiguation') throw new Error(`"${topic}" can mean several things on Wikipedia. Pick a more exact page name.`);
  let wikidata = null;
  if (/^Q\d+$/.test(p.wikibase_item || '')) {
    const w = await fetchImpl(`${WIKIDATA}${p.wikibase_item}/descriptions/en`, { headers: UA }).catch(() => null);
    if (w && w.ok) wikidata = await w.json().catch(() => null);
  }
  return { page: p.title, wikipedia_summary: p.extract || '', wikidata_description: typeof wikidata === 'string' ? wikidata : null, wikidata_id: p.wikibase_item || null };
}

// ---- sending a sentence (play money, testnet, through the site) ----

// Talks to the site's free sender. Every round it opens is tagged 'agent', so it shows as sent by an AI.
export function makeWriter({ site = DEFAULTS.site, fetchImpl = globalThis.fetch } = {}) {
  const api = `${site}/api/write`;
  async function post(body) {
    const r = await fetchImpl(api, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) });
    const j = await r.json().catch(() => ({ error: 'The site did not answer. Try again in a minute.' }));
    if (!r.ok || j.error) { const e = new Error(j.error || `the site answered ${r.status}`); e.limit = j.limit; throw e; }
    return j;
  }
  return {
    send: ({ sentence, topic, word }) => post({ claim: sentence, topic, word, from: 'agent' }),
    run: (round) => post({ action: 'run', round }),
  };
}

export async function bondSentence(writer, ctx, { sentence, wikipedia_page, sureness } = {}) {
  const word = String(sureness ?? '').trim().toLowerCase();
  if (!WORDS.includes(word)) throw new Error('sureness must be one of: is, very likely, likely, disputed');
  if (typeof sentence !== 'string' || !sentence.trim()) throw new Error('give the sentence to bond');
  if (typeof wikipedia_page !== 'string' || !wikipedia_page.trim()) throw new Error('give the Wikipedia page the court should read, for example Lagos');
  const j = await writer.send({ sentence: sentence.trim(), topic: wikipedia_page.trim(), word });
  // Start the court right away, as the try page does. One more try if the first start fails.
  let run = null, runError = null;
  for (let i = 0; i < 2 && !run; i++) {
    try { run = await writer.run(j.round); } catch (e) { runError = e.message; }
  }
  const tx = (h) => (h ? `${ctx.explorer}/txns/${h}` : null);
  return {
    round: j.round, sentence: sentence.trim(), word: j.word, page: j.page, sources: j.sources || [],
    court_started: Boolean(run), court_error: run ? null : runError,
    record_url: `${ctx.site}/case.html?round=${j.round}`,
    open_tx_url: tx(j.tx && j.tx.open), bond_tx_url: tx(j.tx && j.tx.bond), run_tx_url: tx(run && run.tx),
    sent_by: j.account, free_left_today: j.left, cap: j.cap,
    expect: 'The ruling usually lands in about 4 minutes. Call wait_for_ruling with this round number.',
  };
}

// Polls the round until the court rules. If a run came back empty, asks the site for the next one
// (the contract allows three), the same way the try page does.
export async function waitForRuling({ view, writer, ctx }, round, { maxMs = 10 * 60_000, everyMs = 5000, sleep = (ms) => new Promise((ok) => setTimeout(ok, ms)), now = Date.now } = {}) {
  const n = Number(round);
  if (!Number.isInteger(n) || n < 1) throw new Error('round must be a whole number, 1 or higher');
  const t0 = now();
  let retried = 0, first = true;
  for (;;) {
    let r, read = true;
    try { r = await view('get_round', { round: n }); } catch { read = false; }
    if (read && !r && first) throw new Error(`there is no round ${n} on ${ctx.contract}`);
    first = false;
    if (r && r.status !== 'open') return { settled: true, waited_seconds: Math.round((now() - t0) / 1000), ...rulingSummary(parseRound(n, r, ctx), ctx) };
    const runs = (r && r.attempts) || [];
    if (r && writer && runs.length && runs.length < 3 && !r.pending && runs.every((x) => x.result !== 'running') && retried < runs.length) {
      retried = runs.length;
      try { await writer.run(n); } catch { /* the next poll shows whether it started */ }
    }
    if (now() - t0 >= maxMs) {
      return { settled: false, round: n, outcome: 'open', waited_seconds: Math.round((now() - t0) / 1000), record_url: `${ctx.site}/case.html?round=${n}`,
        note: 'No ruling yet. The round stays open on the chain. Call wait_for_ruling again later.' };
    }
    await sleep(everyMs);
  }
}

// What the ruling means for the bond. Money moves when the 5.8 hour challenge window closes.
export function rulingSummary(p, ctx) {
  const b = p.bonds[0] || null;
  const atRisk = b && b.word !== 'disputed' && BigInt(b.stake_yocto) > 0n;
  let money;
  if (!b) money = 'No bond on this round.';
  else if (b.paid) money = b.state;
  else if (!atRisk || p.status === 'void' || !['T', 'F'].includes(p.outcome)) money = 'The bond comes back in full once the 5.8 hour challenge window closes.';
  else if (p.outcome === 'F') money = `Fined: part of the ${b.stake_near} NEAR bond is taken when the 5.8 hour challenge window closes.`;
  else money = `The ${b.stake_near} NEAR bond comes back with a small gain when the 5.8 hour challenge window closes.`;
  return {
    round: p.round, sentence: p.sentence, outcome: p.outcome, outcome_words: p.outcome_words, tag: p.tag,
    word: b ? b.word : null, stake_near: b ? b.stake_near : null,
    fined: Boolean(atRisk && p.status !== 'void' && p.outcome === 'F'),
    fine_paid: b ? b.paid : null, money,
    judges: p.judges.map((j) => `${j.model}: ${j.answer}`),
    record_url: `${ctx.site}/case.html?round=${p.round}`,
  };
}
