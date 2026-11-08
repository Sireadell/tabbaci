// Shared by every page. Reads the chain directly: no server, no account, nothing to install.
const CFG = window.SITE_CONFIG;

// Public testnet nodes limit how often one visitor may ask, so try each listed node, then wait and try again.
const NODES = CFG.rpcs || [CFG.rpc];
async function rpc(method, params) {
  for (let round = 0; round < 3; round++) {
    for (const url of NODES) {
      try {
        const r = await fetch(url, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }) });
        if (!r.ok) continue;
        const j = await r.json();
        if (j.error) throw new Error('rpc error');
        return j.result;
      } catch (e) { if (e.message === 'rpc error') throw e; }
    }
    await new Promise((ok) => setTimeout(ok, 800 * (round + 1)));
  }
  throw new Error('no node answered');
}
async function view(method, args = {}) {
  const res = await rpc('query', { request_type: 'call_function', finality: 'final', account_id: CFG.contract, method_name: method, args_base64: btoa(JSON.stringify(args)) });
  return JSON.parse(new TextDecoder().decode(new Uint8Array(res.result)));
}
const esc = (s) => String(s ?? '').replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
const near = (y) => (Number(BigInt(y) / 10n ** 18n) / 1e6).toFixed(4);
const word = { T: 'true', F: 'false', U: 'unsure', D: 'judges split', V: 'sources disagree' };
function outcomeOf(r) {
  if (r.status === 'open') return ['waiting for a ruling', 't-other'];
  if (r.status === 'void') return [r.void_reason === 'SRC_DISAGREE' ? 'cancelled: the pages disagree' : 'cancelled: 3 failed runs', 't-other'];
  return [word[r.outcome] || r.outcome, r.outcome === 'T' ? 't-T' : r.outcome === 'F' ? 't-F' : 't-other'];
}
const verifyCmd = (id) => `node tools/verify/verify.mjs --contract ${CFG.contract} --round ${id} --network ${CFG.network} --archive archive`;
const blocksOf = (r) => r.attempts.filter((a) => a.end_block).map((a) => a.end_block - a.start_block);
const kindOf = (r) => (r.tag === 'lie_attempt' ? 'lie tried' : 'page sentence');

// What one bond did, in words: "very likely: gained 0.0105".
function bondLine(b) {
  if (!b.payout) return `${esc(b.word)}: money locked, waiting`;
  const p = BigInt(b.payout), s = BigInt(b.stake);
  if (p === s) return `${esc(b.word)}: money back, no gain`;
  return p > s ? `${esc(b.word)}: gained ${near(p - s)} NEAR` : `${esc(b.word)}: lost ${near(s - p)} NEAR`;
}

async function allRounds() {
  const n = await view('round_count');
  const out = [];
  // A few at a time, newest first, so a public node does not turn us away.
  for (let hi = n; hi >= 1; hi -= 4) {
    const ids = [hi, hi - 1, hi - 2, hi - 3].filter((i) => i >= 1);
    const rs = await Promise.all(ids.map((i) => view('get_round', { round: i }).then((r) => (r ? { id: i, ...r } : null))));
    out.push(...rs.filter(Boolean));
  }
  return out;
}

// Short list item for the first page.
function rulingItem(r) {
  const [label, cls] = outcomeOf(r);
  const b = blocksOf(r);
  return `<li><span class="rid">#${r.id}</span><span class="claim">${esc(r.claim)}</span><span class="meta"><b class="${cls}">${label}</b> · ${kindOf(r)}${b.length ? ` · ruled in ${b[b.length - 1]} blocks` : ''}</span></li>`;
}

// Full row for the ledger.
function roundRow(r) {
  const [label, cls] = outcomeOf(r);
  const b = blocksOf(r);
  return `<article class="lrow"><div class="lhead"><span class="rid">#${r.id}</span><span class="claim">${esc(r.claim)}</span><b class="verdict ${cls}">${label}</b></div>
  <div class="meta">${kindOf(r)} · ${r.attempts.length} run(s)${b.length ? ` · ${b.join(', ')} blocks` : ''}${r.bonds.length ? ' · ' + r.bonds.map(bondLine).join(' · ') : ''}</div>
  <details><summary>Check this ruling yourself</summary>
  <p class="mute">From a copy of the code, run:</p><pre>${esc(verifyCmd(r.id))}</pre>
  ${r.bonds.length ? `<p class="mute">Money behind it</p><table>${r.bonds.map((x) => `<tr><td>${esc(x.writer)}</td><td>${esc(x.word)}</td><td>${near(x.stake)} in</td><td>${x.payout ? near(x.payout) + ' out' : 'waiting'}</td></tr>`).join('')}</table>` : ''}
  ${r.attempts.map((a) => `<p class="mute">Run ${a.n}: ${esc(a.result)}</p>`).join('')}
  ${r.verdict ? `<p class="mute">The ruling exactly as the sealed program wrote it</p><pre>${esc(r.verdict)}</pre>` : ''}
  </details></article>`;
}

const CFGnear = (y) => near(y);
window.Site = { CFGnear, view, allRounds, roundRow, rulingItem, bondLine, outcomeOf, verifyCmd, esc, CFG };
