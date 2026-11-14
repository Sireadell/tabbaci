#!/usr/bin/env node
// Re-runs the checker on finished rounds and prints one summary line.
//   node tools/recheck/recheck.mjs --all                       every finished round on the contract
//   node tools/recheck/recheck.mjs --rounds 20,25,33           only these rounds
//   options: --contract finalenc2.testnet  --out report.json  --status site/status.json
// Each round ends in one of three states, taken from the checker's own exit code:
//   MATCH     every line the checker could test matched (exit 0)
//   OPEN      nothing mismatched, but some lines need something this machine lacks (exit 2)
//   MISMATCH  a line failed, or the checker crashed (exit 1 or 3)
// Exit code 0 when no round mismatched. A MISMATCH is never hidden or counted as a match.
import { spawnSync } from 'node:child_process';
import { appendFileSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const args = Object.fromEntries(process.argv.slice(2).reduce((a, x, i, all) => (x.startsWith('--') ? [...a, [x.slice(2), all[i + 1] && !all[i + 1].startsWith('--') ? all[i + 1] : 'true']] : a), []));
const contract = args.contract || 'finalenc2.testnet';
const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const rpc = args.rpc || 'https://test.rpc.fastnear.com';

export function classify(exit) { return exit === 0 ? 'MATCH' : exit === 2 ? 'OPEN' : 'MISMATCH'; }

async function view(method, a = {}) {
  const r = await fetch(rpc, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'query', params: { request_type: 'call_function', finality: 'final', account_id: contract, method_name: method, args_base64: Buffer.from(JSON.stringify(a)).toString('base64') } }) });
  const j = await r.json();
  if (!j.result || j.result.error) throw new Error(j.error ? JSON.stringify(j.error).slice(0, 120) : String(j.result?.error).slice(0, 120));
  return JSON.parse(Buffer.from(j.result.result).toString());
}

async function finishedRounds() {
  const n = await view('round_count');
  const out = [];
  for (let id = 1; id <= n; id += 8) {
    const batch = await Promise.all(Array.from({ length: Math.min(8, n - id + 1) }, (_, k) => view('get_round', { round: id + k }).then((r) => [id + k, r])));
    for (const [rid, r] of batch) if (r && r.status !== 'open') out.push(rid);
  }
  return { total: n, finished: out };
}

async function main() {
  let rounds;
  let total = null;
  if (args.all) { const f = await finishedRounds(); rounds = f.finished; total = f.total; } else rounds = (args.rounds || '20,25,33').split(',').map((s) => s.trim()).filter(Boolean).map(Number);

  // Intel's list of vouched judge keys is gathered once for the whole run.
  const dir = mkdtempSync(join(tmpdir(), 'tabbaci-recheck-'));
  const signersFile = join(dir, 'signers.json');
  let gotSigners = false;
  try {
    const rr = await view('get_round', { round: rounds[0] });
    const models = JSON.parse(rr.input).m;
    const run = spawnSync(process.execPath, [join(root, 'privacy', 'tools', 'verify-judges.mjs'), '--algo', 'ecdsa', '--signers', '--models', models.join(',')], { encoding: 'utf8', timeout: 240000 });
    if (run.status !== null && run.stdout) { writeFileSync(signersFile, run.stdout); gotSigners = true; }
  } catch { /* each round then reports the judge key lines as CANNOT VERIFY */ }

  const results = [];
  for (const round of rounds) {
    const run = spawnSync(process.execPath, [
      join(root, 'tools', 'verify', 'verify.mjs'),
      '--contract', contract, '--round', String(round), '--archive', join(root, 'archive'),
      '--network', 'testnet', '--rpc', rpc, ...(gotSigners ? ['--signers-json', signersFile] : []),
    ], { encoding: 'utf8', cwd: root, timeout: 240000 });
    const lines = (run.stdout || '').split('\n');
    const state = classify(run.status);
    const rec = {
      round, state, exit: run.status,
      matched: lines.filter((l) => l.startsWith('MATCH')).length,
      mismatches: lines.filter((l) => l.startsWith('MISMATCH')).map((l) => l.replace(/\s+/g, ' ').trim()),
      open: lines.filter((l) => l.startsWith('CANNOT VERIFY')).map((l) => l.replace(/\s+/g, ' ').trim().slice(0, 160)),
    };
    if (state === 'MISMATCH' && !rec.mismatches.length) rec.mismatches.push(`checker exited with code ${run.status}: ${(run.stderr || '').trim().slice(0, 120)}`);
    results.push(rec);
    console.log(`round ${round}: ${state}  (${rec.matched} lines matched, ${rec.open.length} need something this machine lacks)`);
    for (const b of rec.mismatches) console.log('   ' + b);
  }

  const count = (s) => results.filter((r) => r.state === s).length;
  const n = results.length;
  const headline = count('MISMATCH') === 0 && count('OPEN') === 0
    ? `${n} of ${n} finished rounds: MATCH`
    : `${n - count('MISMATCH')} of ${n} finished rounds: ${count('MISMATCH')} mismatch (${count('MATCH')} fully MATCH, ${count('OPEN')} with lines this machine could not test)`;
  console.log('\n' + headline);
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, `## ${headline}\n`);
  const report = { checked: new Date().toISOString(), contract, rounds_on_chain: total, finished: n, match: count('MATCH'), open: count('OPEN'), mismatch: count('MISMATCH'), headline, results };
  if (args.out) writeFileSync(args.out, JSON.stringify(report, null, 2));
  process.exit(count('MISMATCH') === 0 ? 0 : 1);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) main().catch((e) => { console.error('recheck error:', e.message); process.exit(1); });
