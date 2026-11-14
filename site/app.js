// Shared by every page. Reads the chain straight from the visitor's browser: no server, no account, no key.
// A saved copy of every round (data/rounds.json) fills in when a public node refuses the visitor.
const CFG = window.SITE_CONFIG;
const NODES = CFG.rpcs || [CFG.rpc];
const $ = (id) => document.getElementById(id);
const esc = (s) => String(s ?? '').replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));

// ---------- chain access ----------
async function rpc(method, params) {
  for (let round = 0; round < 2; round++) {
    for (const url of NODES) {
      try {
        const ctl = new AbortController();
        const t = setTimeout(() => ctl.abort(), 7000);
        const r = await fetch(url, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }), signal: ctl.signal });
        clearTimeout(t);
        if (!r.ok) continue;
        const j = await r.json();
        if (j.error) { if (j.error.cause && j.error.cause.name === 'UNKNOWN_BLOCK') continue; throw new Error('rpc error'); }
        return j.result;
      } catch (e) { if (e.message === 'rpc error') throw e; }
    }
    await new Promise((ok) => setTimeout(ok, 700));
  }
  throw new Error('no node answered');
}
async function view(method, args = {}, contract = CFG.contract) {
  const res = await rpc('query', { request_type: 'call_function', finality: 'final', account_id: contract, method_name: method, args_base64: btoa(JSON.stringify(args)) });
  if (res.error) throw new Error(res.error);
  return JSON.parse(new TextDecoder().decode(new Uint8Array(res.result)));
}

// ---------- saved copy ----------
const savedPs = {};
function saved(file = CFG.savedFile || 'data/rounds.json') {
  if (!savedPs[file]) savedPs[file] = fetch(file).then((r) => (r.ok ? r.json() : null)).catch(() => null);
  return savedPs[file];
}
// Returns { rounds (newest first), live, savedAt }. Final rounds come from the saved copy, anything open,
// unpaid or newer than the copy is asked from the chain.
async function allRounds(contract = CFG.contract, file) {
  const sv = await saved(file);
  const have = new Map(((sv && sv.rounds) || []).map((r) => [r.id, r]));
  let live = false;
  try {
    const n = await view('round_count', {}, contract); live = true;
    const todo = [];
    for (let i = 1; i <= n; i++) {
      const r = have.get(i);
      if (!r || r.status === 'open' || (r.bonds || []).some((b) => b.payout == null)) todo.push(i);
    }
    for (let k = 0; k < todo.length; k += 4) {
      await Promise.all(todo.slice(k, k + 4).map((i) => view('get_round', { round: i }, contract).then((r) => { if (r) have.set(i, { ...(have.get(i) || {}), id: i, ...r }); })));
    }
  } catch (e) { /* keep the saved copy */ }
  const tag = contract === CFG.contractV2 ? { _c: contract } : {};
  return { rounds: [...have.values()].map((r) => ({ ...r, ...tag })).sort((a, b) => b.id - a.id), live, savedAt: sv && sv.saved };
}
async function getRound(id) {
  const sv = await saved(); const old = sv && sv.rounds.find((x) => x.id === id);
  try { const r = await view('get_round', { round: id }); return r ? { id, ...(old ? { _saved: old._saved } : {}), ...r, _live: true } : null; }
  catch (e) { return old ? { ...old, _live: false } : null; }
}

// ---------- words and numbers ----------
const near = (y) => (Number(BigInt(y) / 10n ** 18n) / 1e6).toFixed(4);
const pct = (num, den) => (Number((num * 10000n) / den) / 100).toFixed(2) + '%';
const parse = (s) => { try { return JSON.parse(s); } catch { return null; } };
const isLie = (r) => (CFG.plantedLies || []).includes(Number(r.id));
const letterOf = (r) => (r.status === 'open' ? 'OPEN' : r.status === 'void' ? 'VOID' : r.outcome === 'T' ? 'TRUE' : r.outcome === 'F' ? 'FALSE' : r.outcome === 'D' ? 'SPLIT' : 'UNSURE');
const clsOf = (word) => ({ TRUE: 'st-true', FALSE: 'st-false' }[word] || 'st-other');
const stamp = (word, extra = '') => `<span class="stamp ${clsOf(word)} ${extra}">${esc(word)}</span>`;
const plainOutcome = (r) => (r.status === 'open' ? 'waiting for a ruling' : r.status === 'void' ? (r.void_reason === 'SRC_DISAGREE' ? 'cancelled: the two pages disagree' : 'cancelled: three failed runs') : { T: 'the court says it is true', F: 'the court says it is false', D: 'the judges split', U: 'the court could not confirm it' }[r.outcome] || r.outcome);
const modelName = (m) => String(m || '').split('/').pop();
const blocksOf = (r) => (r.attempts || []).filter((a) => a.end_block).map((a) => a.end_block - a.start_block);
const explorer = () => 'https://' + (CFG.network === 'mainnet' ? '' : 'testnet.') + 'nearblocks.io';
const verifyCmd = (id) => `node tools/verify/verify.mjs --contract ${CFG.contract} --round ${id} --network ${CFG.network} --archive archive`;
const fullCheckCmd = (id) => `${verifyCmd(id)} --attested-judges`;
const WORDS = { is: 9500, 'very likely': 8000, likely: 6500, disputed: 5000 };

function bondMoney(b) {
  if (b.payout == null) return { text: 'bond locked, payout not made yet', kind: 'wait' };
  const p = BigInt(b.payout), s = BigInt(b.stake);
  if (p === s) return { text: `bond returned in full, ${near(p)} NEAR`, kind: 'even' };
  return p > s ? { text: `gained ${near(p - s)} NEAR (${pct(p - s, s)} of the bond)`, kind: 'gain' } : { text: `fined ${near(s - p)} NEAR (${pct(s - p, s)} of the bond)`, kind: 'fine' };
}

// ---------- page chrome ----------
// Headings and blocks settle in as they reach the screen. Without JS, or with reduced motion, everything just shows.
function reveal() {
  if (!('IntersectionObserver' in window) || matchMedia('(prefers-reduced-motion: reduce)').matches) return;
  document.documentElement.classList.add('js');
  const io = new IntersectionObserver((es) => es.forEach((e) => { if (e.isIntersecting) { e.target.classList.add('in'); io.unobserve(e.target); } }), { rootMargin: '0px 0px -8% 0px' });
  document.querySelectorAll('.sec-head,.closing .in').forEach((el) => { el.classList.add('rv'); io.observe(el); });
}
function chrome(active) {
  const links = [['index.html', 'Home', 'home'], ['ledger.html', 'Ledger', 'ledger'], ['pages.html', 'Pages', 'pages'], ['try.html', 'Try to lie', 'try']];
  const top = $('top');
  if (top) top.innerHTML = `<div class="wrap bar"><a class="brand" href="index.html"><span class="brand-mark" aria-hidden="true">T</span>Tabbaci</a><nav aria-label="Main">${links.map(([h, t, k]) => `<a href="${h}"${k === active ? ' class="on" aria-current="page"' : ''}>${t}</a>`).join('')}</nav></div>`;
  reveal();
  const foot = $('foot');
  if (foot) foot.innerHTML = `<div class="wrap foot-in"><p><b>Tabbaci</b> is an encyclopedia where every checkable sentence has a money bond. Everything here runs on the NEAR test network, with test money.</p><p class="mute">Read straight from <code>${esc(CFG.contract)}</code>. <a href="ledger.html">Every ruling</a> · <a href="index.html#check">Check one yourself</a> · <a href="index.html#not">What this is not</a> · <a href="https://github.com/Sireadell/tabbaci">Code on GitHub</a></p></div>`;
}

// ---------- ruling rows ----------
function latestItem(r) {
  const b = blocksOf(r);
  return `<li><a class="lrow" href="case.html?round=${r.id}${r._c ? '&c=' + r._c : ''}"><span class="rid">No. ${r.id}</span><span class="claim">${esc(r.claim)}</span>${stamp(letterOf(r), 'small')}<span class="meta">${isLie(r) ? 'a lie, tried on purpose' : r.tag === 'visitor' ? 'sent by a visitor, no wallet' : 'a page sentence'}${b.length ? ` · ruled in ${b[b.length - 1]} blocks` : ''}</span></a></li>`;
}

// ---------- the checks that run in the browser ----------
// Same rules as tools/verify, ported. Anything the browser cannot do (signatures, the sealed-box report) is
// listed as "needs the command line" and never as MATCH.
async function sha256b(bytes) { return new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)); }
const toB64url = (u8) => btoa(String.fromCharCode(...u8)).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
const toHex = (u8) => [...u8].map((x) => x.toString(16).padStart(2, '0')).join('');
const enc = (s) => new TextEncoder().encode(s);

function netOf(stake, chance, outcome) {
  const BPS = 10000n;
  let d = (outcome === 'T' ? BPS : 0n) - BigInt(chance); if (d < 0n) d = -d;
  const num = stake * (25000000n - d * d), den = 100000000n;
  return num >= 0n ? num / den : -((-num + den - 1n) / den);
}
function settleRule(entries, outcome, reserve) {
  if (!outcome) return entries.map((e) => e.stake);
  const nets = entries.map((e) => netOf(e.stake, e.chance, outcome));
  const losses = nets.filter((n) => n < 0n).reduce((a, n) => a - n, 0n);
  const gains = nets.filter((n) => n > 0n).reduce((a, n) => a + n, 0n);
  const pool = losses + reserve;
  return entries.map((e, i) => (nets[i] >= 0n ? e.stake + (gains <= pool ? nets[i] : (nets[i] * pool) / gains) : e.stake + nets[i]));
}
const tally = (a) => { const t = a.filter((x) => x === 'T').length, f = a.filter((x) => x === 'F').length; return [t, f, a.length - t - f]; };
function proseOutcome(a) {
  const [t, f, u] = tally(a);
  if (t === 3) return ['T', false]; if (f === 3) return ['F', false];
  if (t === 2 && u === 1) return ['T', true]; if (f === 2 && u === 1) return ['F', true];
  if ((t === 2 && f === 1) || (t === 1 && f === 2)) return ['D', false];
  return ['U', false];
}
function dataOutcome(a, numbersTrue) {
  const [t, f, u] = tally(a); const dec = numbersTrue ? 'T' : 'F';
  if (t === 3) return [dec, false]; if (t === 2 && u === 1) return [dec, true];
  if ((t === 2 && f === 1) || (t === 1 && f === 2) || f === 3 || (f === 2 && u === 1)) return ['D', false];
  return ['U', false];
}
function wasmExports(buf) {
  let p = 8;
  const leb = () => { let r = 0, s = 0, b; do { b = buf[p++]; r |= (b & 0x7f) << s; s += 7; } while (b & 0x80); return r >>> 0; };
  while (p < buf.length) {
    const id = buf[p++]; const size = leb(); const end = p + size;
    if (id === 7) { const n = leb(); const out = []; for (let i = 0; i < n; i++) { const len = leb(); out.push(new TextDecoder().decode(buf.subarray(p, p + len))); p += len; p++; leb(); } return out; }
    p = end;
  }
  return [];
}
const FORBIDDEN = /^(upgrade|deploy|migrate|update_contract|set_owner|set_code|replace_code|add_full_access_key|add_key|delete_key|set_dao|set_resolver|set_courtroom|set_models)/i;

// Runs every check and calls show(name, status, detail) as each one finishes. status: MATCH, MISMATCH or NEEDS.
async function runChecks(id, show, opts = {}) {
  const out = []; const add = (name, status, detail) => { out.push({ name, status, detail }); show(name, status, detail); };
  const pause = () => new Promise((ok) => setTimeout(ok, opts.pace ?? 260));
  let r;
  try { r = await view('get_round', { round: id }); } catch { add('Read the round from the chain', 'NEEDS', 'no public node answered just now. Try again in a minute, or use the command below.'); return out; }
  if (!r) { add('Read the round from the chain', 'MISMATCH', `no round ${id} on ${CFG.contract}`); return out; }
  add('Read the round from the chain', 'MATCH', `round ${id} on ${CFG.contract}`); await pause();
  const hasSubtle = !!(window.crypto && crypto.subtle);
  try {
    const keys = await rpc('query', { request_type: 'view_access_key_list', finality: 'final', account_id: CFG.contract });
    add('The contract has no access keys', keys.keys.length === 0 ? 'MATCH' : 'MISMATCH', keys.keys.length === 0 ? 'no one can change it' : `${keys.keys.length} key still on it. Its owner keeps it until the final deletion, so this line is red on purpose today.`);
  } catch { add('The contract has no access keys', 'NEEDS', 'the node did not answer'); }
  await pause();
  try {
    const c = await rpc('query', { request_type: 'view_code', finality: 'final', account_id: CFG.contract });
    const names = wasmExports(Uint8Array.from(atob(c.code_base64), (ch) => ch.charCodeAt(0)));
    const bad = names.filter((n) => FORBIDDEN.test(n));
    add('The contract has no way to swap its own code', bad.length ? 'MISMATCH' : 'MATCH', bad.length ? `found: ${bad.join(', ')}` : `${names.length} methods, none can replace code, keys or owners`);
  } catch { add('The contract has no way to swap its own code', 'NEEDS', 'the node did not answer'); }
  await pause();
  const input = parse(r.input);
  if (!input || !hasSubtle) { add('The stored request matches its fingerprint', 'NEEDS', hasSubtle ? 'request unreadable' : 'this browser has no secure hashing here. Use the command.'); return out; }
  const ih = toB64url(await sha256b(enc(r.input)));
  add('The stored request matches its fingerprint', ih === r.ih ? 'MATCH' : 'MISMATCH', 'sha-256 of the request, computed in this browser'); await pause();
  const th = toHex(await sha256b(enc(r.claim)));
  add('The sentence is the one that was sent', th === input.th && r.claim === input.claim ? 'MATCH' : 'MISMATCH', 'its hash equals the one in the request'); await pause();
  if (r.bonds && r.bonds.length) {
    if (r.status === 'open') add('Bond payouts follow the public rule', 'NEEDS', 'the round is still open');
    else if (r.reserve_before == null) add('Bond payouts follow the public rule', 'NEEDS', 'this round did not record the reserve');
    else {
      const outcome = r.outcome === 'T' || r.outcome === 'F' ? r.outcome : null;
      const want = settleRule(r.bonds.map((b) => ({ stake: BigInt(b.stake), chance: WORDS[b.word] })), outcome, BigInt(r.reserve_before));
      const same = r.bonds.every((b, i) => b.payout != null && BigInt(b.payout) === want[i]);
      const unpaid = r.bonds.some((b) => b.payout == null);
      add('Bond payouts follow the public rule', unpaid ? 'NEEDS' : same ? 'MATCH' : 'MISMATCH', unpaid ? 'not paid out yet, the challenge window is still open' : 'recomputed with whole-number maths in this browser');
    }
    await pause();
  }
  if (r.status === 'open') { add('The ruling', 'NEEDS', 'the round is still open, nothing to check yet'); return out; }
  const v = parse(r.verdict);
  if (!v) { add('The ruling is present', r.status === 'void' ? 'MATCH' : 'MISMATCH', r.status === 'void' ? 'cancelled round, no ruling expected' : 'a settled round has no ruling'); return out; }
  add('The ruling belongs to this round and request', (v.v === 1 || v.v === 2) && v.rd === id && v.ih === r.ih ? 'MATCH' : 'MISMATCH', `the sealed program wrote outcome ${v.o}`); await pause();
  add('The contract outcome equals the ruling', v.o === r.outcome || (v.o === 'V' && r.status === 'void') ? 'MATCH' : 'MISMATCH', `ruling ${v.o}, contract ${r.outcome ?? r.void_reason}`); await pause();
  const sv = await saved(); const old = sv && sv.rounds.find((x) => x.id === id);
  const sources = v.s || [];
  if (sources.length) {
    const have = sources.filter((s) => old && old._saved && old._saved[s.i]);
    if (have.length === sources.length) {
      let ok = 0; const details = [];
      for (const s of sources) {
        try { const bytes = new Uint8Array(await (await fetch('data/archive/' + old._saved[s.i].file)).arrayBuffer()); if (toB64url(await sha256b(bytes)) === s.h) ok++; else details.push(`source ${s.i} differs`); } catch { details.push(`source ${s.i} missing`); }
      }
      add('The saved evidence pages match what the sealed run read', ok === sources.length ? 'MATCH' : 'MISMATCH', ok === sources.length ? `${ok} page(s) hashed in this browser equal the fingerprints in the ruling` : details.join(', '));
    } else add('The saved evidence pages match what the sealed run read', 'NEEDS', 'no saved copy of these pages on this site. The command checks them from the archive folder.');
    await pause();
  }
  const answers = (v.j || []).map((j) => (j.a === 'X' ? 'U' : j.a));
  if (answers.length) {
    if (input.kind === 'prose') { const [o, w] = proseOutcome(answers); add('The outcome follows from the three judge answers', o === v.o && !!w === !!v.w ? 'MATCH' : 'MISMATCH', `answers ${answers.join(' ')} give ${o}${w ? ' (two of three)' : ''}`); }
    else {
      const nums = sources.map((s) => Number(s.x)); const tol = Math.max(0, ...(input.src || []).map((x) => x.tol));
      const agree = nums.length && nums.every((x) => Number.isFinite(x) && Math.abs(x - nums[0]) <= tol);
      if (v.o === 'V') add('The sources really disagree', !agree ? 'MATCH' : 'MISMATCH', sources.map((s) => s.x).join(' vs '));
      else { const [o, w] = dataOutcome(answers, Math.abs(nums[0] - Number(input.cv)) <= tol); add('The outcome follows from the numbers and the judges', agree && o === v.o && !!w === !!v.w ? 'MATCH' : 'MISMATCH', `sources read ${sources.map((s) => s.x).join(' and ')}, the sentence says ${input.cv}, judge answers ${answers.join(' ')}`); }
    }
    await pause();
    if (v.v === 2 && window.V2) {
      const pages = [];
      for (const x of sources) { const f = old && old._saved && old._saved[x.i]; try { pages.push(f ? new Uint8Array(await (await fetch('data/archive/' + f.file)).arrayBuffer()) : null); } catch { pages.push(null); } }
      for (const row of await window.V2.check(input, v, r, pages)) add(row.name, row.status, row.detail);
    } else if (window.JudgeSig) {
      let bytes = [];
      if (input.kind !== 'data') {
        for (const x of sources) { const f = old && old._saved && old._saved[x.i]; try { bytes.push(f ? new Uint8Array(await (await fetch('data/archive/' + f.file)).arrayBuffer()) : null); } catch { bytes.push(null); } }
      }
      let keys = null;
      try { keys = (await (await fetch('data/judge-keys.json')).json()).keys; } catch { /* the line then says COMMAND LINE */ }
      const sig = await window.JudgeSig.check(input, v, bytes, keys);
      add('Each judge signed its own answer', sig.state === 'NEEDS' ? 'NEEDS' : sig.state, sig.detail);
    } else add('Each judge signed its own answer', 'NEEDS', 'the signature code did not load. Run the command below.');
    await pause();
  }
  add('The sealed box really ran the fingerprinted program', 'NEEDS', CFG.sealed[id] ? 'a saved proof exists for this round. Check it with the command below.' : 'needs the outlayer-verify tool. Run the command below.');
  return out;
}

// ---------- replay of one real ruling ----------
const modelBrief = (m) => { const n = modelName(m); return n.length > 26 ? n.slice(0, 25) + '...' : n; };
function mountReplay(root, rounds, startId) {
  const byId = new Map(rounds.map((r) => [r.id, r]));
  const picks = CFG.featured.filter((f) => byId.has(f.round));
  if (!picks.length) { root.innerHTML = '<p class="mute">No rulings to replay yet.</p>'; return; }
  let cur = picks.find((p) => p.round === startId) || picks[0];
  let timers = [];
  const reduce = window.matchMedia && matchMedia('(prefers-reduced-motion: reduce)').matches;
  root.innerHTML = `<div class="rp-head"><p class="label">A real ruling, replayed from the chain</p><div class="rp-picks" role="group" aria-label="Pick a case">${picks.map((p) => `<button type="button" class="chip" data-r="${p.round}">${esc(p.title)}</button>`).join('')}</div></div><div class="rp-stage" id="rp-stage"></div><div class="rp-foot"><button type="button" class="btn ghost" id="rp-again">Replay</button><a id="rp-open" href="#">Open the full court record</a></div>`;
  const stage = root.querySelector('#rp-stage');
  const clear = () => { timers.forEach(clearTimeout); timers = []; };
  function play() {
    clear();
    const r = byId.get(cur.round);
    root.querySelectorAll('.chip').forEach((c) => c.setAttribute('aria-pressed', String(Number(c.dataset.r) === cur.round)));
    root.querySelector('#rp-open').href = `case.html?round=${r.id}`;
    const input = parse(r.input) || {}; const v = parse(r.verdict) || {}; const models = input.m || [];
    const split = r.claim.split(/(Judges?:.*)$/i);
    const claimHtml = split.length > 1 ? `${esc(split[0])}<mark>${esc(split[1])}</mark>` : esc(r.claim);
    const b = (r.bonds || [])[0];
    const blocks = blocksOf(r); const last = blocks[blocks.length - 1]; const secs = last ? Math.round(last * 1.1) : null;
    const srcs = (input.src || []).map((s) => { const got = (v.s || []).find((x) => x.i === s.i); return `<li><span class="mono">${esc(s.u.replace(/^https?:\/\//, ''))}</span><span class="mute">${got ? (got.x !== undefined && got.x !== '' ? 'read ' + esc(got.x) : 'fetched, HTTP ' + esc(got.c)) : ''}</span></li>`; }).join('');
    const seats = (v.j || []).map((j) => `<div class="seat"><span class="seat-n">${esc(modelBrief(models[j.m]))}</span>${stamp(input.kind === 'data' ? (j.a === 'T' ? 'MATCHES' : j.a === 'F' ? 'DIFFERS' : 'UNSURE') : (j.a === 'T' ? 'TRUE' : j.a === 'F' ? 'FALSE' : 'UNSURE'), 'seat-stamp')}<span class="mono sig">${j.g ? 'signed ' + esc(j.g.slice(0, 10)) + '...' : 'no signed reply'}</span></div>`).join('');
    let money = '';
    if (b && b.payout != null) {
      const s = BigInt(b.stake), p = BigInt(b.payout); const back = Number((p * 1000n) / s) / 10; const w = Math.max(0, Math.min(100, back));
      money = `<div class="split-bar" aria-hidden="true"><i class="keep" style="width:${w}%"></i><i class="lose" style="width:${100 - w}%"></i></div><p class="split-txt">${p < s ? `Bond ${near(s)} NEAR. <b class="neg">${near(s - p)} lost</b>, ${near(p)} given back. ${pct(s - p, s)} of the bond is gone.` : p > s ? `Bond ${near(s)} NEAR. <b class="pos">${near(p - s)} gained</b>.` : `Bond ${near(s)} NEAR given back in full.`}</p>`;
    } else if (r.challenger) money = `<p class="split-txt">Challenged by <span class="mono">${esc(r.challenger)}</span> with ${near(r.bond)} NEAR. The court ruled again.</p>`;
    else if (b) money = `<p class="split-txt">Bond ${near(b.stake)} NEAR, locked until the challenge window closes.</p>`;
    stage.innerHTML = `
      <div class="beat"><p class="bl">The sentence${b ? ` <span class="tag">${near(b.stake)} NEAR bond · "${esc(b.word)}"</span>` : ''}</p><p class="rp-claim">${claimHtml}</p></div>
      <div class="beat"><p class="bl">The evidence the court reads</p><ul class="srcs">${srcs || '<li class="mute">Two pages from the approved list</li>'}</ul>${split.length > 1 ? '<p class="warn">The sentence carries an order to the judges. They see it only as quoted text.</p>' : ''}</div>
      <div class="beat"><div class="seal"><div class="box" aria-hidden="true"><span class="lid"></span></div><p class="bl">Sealed in an Intel chip${secs ? `, ruled in ${last} blocks (about ${secs} s)` : ''}</p></div></div>
      <div class="beat"><p class="bl">Three judges answer${input.kind === 'data' ? ': does the sentence state the number ' + esc(input.cv) + '?' : ''}</p><div class="seats">${seats || '<p class="mute">No judge answers yet.</p>'}</div></div>
      <div class="beat"><div class="verdict-row">${stamp(letterOf(r), 'big')}<span class="mute">${esc(plainOutcome(r))}${input.kind === 'data' ? '. The numbers the sources returned decide this one.' : ''}</span></div>${money}</div>`;
    const beats = [...stage.querySelectorAll('.beat')];
    const seatEls = [...stage.querySelectorAll('.seat-stamp')];
    if (reduce) { beats.forEach((e) => e.classList.add('in')); stage.querySelector('.box').classList.add('shut'); return; }
    seatEls.forEach((s) => s.classList.add('pending'));
    const at = [150, 1800, 3700, 5700, 9400];
    beats.forEach((e, i) => timers.push(setTimeout(() => e.classList.add('in'), at[i])));
    seatEls.forEach((s, i) => timers.push(setTimeout(() => s.classList.remove('pending'), 6500 + i * 900)));
    timers.push(setTimeout(() => stage.querySelector('.box').classList.add('shut'), 4300));
  }
  root.querySelectorAll('.chip').forEach((c) => c.addEventListener('click', () => { cur = picks.find((p) => p.round === Number(c.dataset.r)); play(); }));
  root.querySelector('#rp-again').addEventListener('click', play);
  play();
}

// Point every read on this page at another court contract that the config lists (court version 2).
function useContract(c) {
  if (c && c === CFG.contractV2) { CFG.contract = c; CFG.savedFile = 'data/rounds-v2.json'; }
}

window.Site = { useContract, CFG, $, esc, rpc, view, allRounds, getRound, near, pct, parse, isLie, letterOf, stamp, plainOutcome, modelName, blocksOf, explorer, verifyCmd, fullCheckCmd, bondMoney, chrome, latestItem, runChecks, mountReplay, saved, WORDS };
