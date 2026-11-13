#!/usr/bin/env node
// Re-runs the checker on a list of finished rounds and prints "N of N re-verified".
// A round counts as re-verified when the checker finds no MISMATCH. Lines the checker
// cannot test on this machine stay CANNOT VERIFY and are listed, never counted as matches.
//   node tools/recheck/recheck.mjs [--rounds 20,25,33] [--contract finalenc2.testnet]
// Exit code 0 when every round is re-verified, 1 when any round shows a mismatch or the checker fails.
import { spawnSync } from 'node:child_process';
import { appendFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const args = Object.fromEntries(process.argv.slice(2).reduce((a, x, i, all) => (x.startsWith('--') ? [...a, [x.slice(2), all[i + 1]]] : a), []));
const contract = args.contract || 'finalenc2.testnet';
const rounds = (args.rounds || '20,25,33').split(',').map((s) => s.trim()).filter(Boolean);
const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const rpc = 'https://test.rpc.fastnear.com';

const results = [];
for (const round of rounds) {
  const run = spawnSync(process.execPath, [
    join(root, 'tools', 'verify', 'verify.mjs'),
    '--contract', contract, '--round', round, '--archive', join(root, 'archive'),
    '--network', 'testnet', '--attested-judges', '--rpc', rpc,
  ], { encoding: 'utf8', cwd: root, timeout: 180000 });
  const lines = (run.stdout || '').split('\n');
  const bad = lines.filter((l) => l.startsWith('MISMATCH'));
  const open = lines.filter((l) => l.startsWith('CANNOT VERIFY'));
  const ok = run.status === 0 || run.status === 2;
  results.push({ round, ok: ok && bad.length === 0, bad, open: open.length, exit: run.status });
  console.log(`round ${round}: ${ok && bad.length === 0 ? 'no mismatch' : 'PROBLEM'}  (exit ${run.status}, ${lines.filter((l) => l.startsWith('MATCH')).length} matched, ${open.length} could not be checked here)`);
  for (const b of bad) console.log('   ' + b.trim());
}

const good = results.filter((r) => r.ok).length;
const headline = `${good} of ${results.length} rounds re-verified (no mismatch), ${new Date().toISOString().slice(0, 10)}`;
console.log('\n' + headline);
const note = 'Judge signatures and the sealed-box report need the full command in the README.';
console.log(note);
if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, `## ${headline}\n\n${note}\n`);
if (args.out) writeFileSync(args.out, JSON.stringify({ checked: new Date().toISOString(), contract, good, total: results.length, results }, null, 2));
process.exit(good === results.length ? 0 : 1);
