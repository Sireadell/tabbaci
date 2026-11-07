// Shared by every page. Reads the chain directly: no server, no account, nothing to install.
const CFG = window.SITE_CONFIG;

async function rpc(method, params) {
  const r = await fetch(CFG.rpc, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }) });
  const j = await r.json();
  if (j.error) throw new Error('rpc error');
  return j.result;
}
async function view(method, args = {}) {
  const res = await rpc('query', { request_type: 'call_function', finality: 'final', account_id: CFG.contract, method_name: method, args_base64: btoa(JSON.stringify(args)) });
  return JSON.parse(new TextDecoder().decode(new Uint8Array(res.result)));
}
const esc = (s) => String(s ?? '').replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
const near = (y) => (Number(BigInt(y) / 10n ** 18n) / 1e6).toFixed(4);
const word = { T: 'true', F: 'false', U: 'unsure', D: 'judges split', V: 'sources disagree' };
function outcomeOf(r) {
  if (r.status === 'open') return ['open', 't-other'];
  if (r.status === 'void') return [r.void_reason === 'SRC_DISAGREE' ? 'void: sources disagree' : 'void: 3 failed runs', 't-other'];
  return [word[r.outcome] || r.outcome, r.outcome === 'T' ? 't-T' : r.outcome === 'F' ? 't-F' : 't-other'];
}
const verifyCmd = (id) => `node tools/verify/verify.mjs --contract ${CFG.contract} --round ${id} --network ${CFG.network} --archive archive`;

async function allRounds() {
  const n = await view('round_count');
  const out = [];
  for (let i = n; i >= 1; i--) {
    const r = await view('get_round', { round: i });
    if (r) out.push({ id: i, ...r });
  }
  return out;
}

function roundRow(r) {
  const [label, cls] = outcomeOf(r);
  const blocks = r.attempts.filter((a) => a.end_block).map((a) => a.end_block - a.start_block).join(', ') || '-';
  return `<tr><td>${r.id}</td><td>${esc(r.claim)}<br><span class="mute">${esc(r.tag || 'page')} · ${r.attempts.length} attempt(s) · ${blocks} blocks</span></td><td class="${cls}">${label}</td>
  <td><details><summary>details</summary>
  <p class="mute">Check it yourself:</p><pre>${esc(verifyCmd(r.id))}</pre>
  ${r.bonds.length ? `<p class="mute">Bonds</p><table>${r.bonds.map((b) => `<tr><td>${esc(b.writer)}</td><td>${esc(b.word)}</td><td>${near(b.stake)} in</td><td>${b.payout ? near(b.payout) + ' out' : 'waiting'}</td></tr>`).join('')}</table>` : ''}
  ${r.attempts.map((a) => `<p class="mute">Attempt ${a.n}: ${esc(a.result)}</p>`).join('')}
  ${r.verdict ? `<p class="mute">Ruling written by the sealed box</p><pre>${esc(r.verdict)}</pre>` : ''}
  </details></td></tr>`;
}

const CFGnear = (y) => near(y);
window.Site = { CFGnear, view, allRounds, roundRow, outcomeOf, verifyCmd, esc, CFG };
