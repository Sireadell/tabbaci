#!/usr/bin/env node
// Turns a re-check report into the two small files the public sees:
//   badge.json   the README sticker (read by shields.io from the "status" branch)
//   status.json  what the site serves at /status.json for AI readers and humans
//   node tools/recheck/status.mjs recheck-report.json out-dir
// Reads the chain for the live numbers (read only). Never changes a result: a mismatch makes the sticker red.
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

export const SITE = 'https://tabbaci.vercel.app';
export const REPO = 'https://github.com/Sireadell/tabbaci';
export const RAW_STATUS = 'https://raw.githubusercontent.com/Sireadell/tabbaci/status/status.json';

export function badgeFor(report) {
  if (!report) return { schemaVersion: 1, label: 'every ruling re-checked', message: 'not run yet', color: 'lightgrey' };
  const bad = report.mismatch > 0;
  const message = bad
    ? `${report.mismatch} MISMATCH in ${report.finished} rounds`
    : `${report.finished} of ${report.finished}, no mismatch (${report.match} fully MATCH)`;
  return { schemaVersion: 1, label: 'every ruling re-checked', message, color: bad ? 'red' : 'brightgreen' };
}

async function rpcView(rpc, contract, method, args = {}) {
  const r = await fetch(rpc, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'query', params: { request_type: 'call_function', finality: 'final', account_id: contract, method_name: method, args_base64: Buffer.from(JSON.stringify(args)).toString('base64') } }) });
  const j = await r.json();
  return JSON.parse(Buffer.from(j.result.result).toString());
}

export const COURT2 = 'tabbaci-court2.testnet';
export const MAINNET_PILOT = '5c807510305d238f1cec81ba0544b08b8f388e807b2d20a514b36e1678ab801e';

async function keyCount(rpc, account) {
  const k = await fetch(rpc, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'query', params: { request_type: 'view_access_key_list', finality: 'final', account_id: account } }) });
  return (await k.json()).result.keys.length;
}

// Court 2 and the mainnet pilot, read from the chain (read only). Either one is null when its node does not answer.
export async function readCourts(testRpc = 'https://test.rpc.fastnear.com', mainRpc = 'https://rpc.mainnet.near.org') {
  let court2 = null, mainnet = null;
  try {
    const rounds = await rpcView(testRpc, COURT2, 'round_count');
    let lastSettled = null, settled = 0;
    for (let i = 1; i <= rounds; i++) {
      const r = await rpcView(testRpc, COURT2, 'get_round', { round: i });
      if (r && r.status === 'settled') { settled++; lastSettled = i; }
    }
    court2 = { rounds, settled, lastSettled, keys: await keyCount(testRpc, COURT2) };
  } catch { /* leave null */ }
  try {
    mainnet = { rounds: await rpcView(mainRpc, MAINNET_PILOT, 'round_count'), keys: await keyCount(mainRpc, MAINNET_PILOT) };
  } catch { /* leave null */ }
  return { court2, mainnet };
}

// The plain re-check headline, built from the counts so every source words it the same way.
export function plainHeadline(r) {
  return `${r.finished - r.mismatch} of ${r.finished} finished rounds: ${r.mismatch} mismatch`;
}

// live: { rounds, keys } read from the chain, or null when the node does not answer.
export async function readLive(contract = 'finalenc2.testnet', rpc = 'https://test.rpc.fastnear.com') {
  try {
    const rounds = await rpcView(rpc, contract, 'round_count');
    const k = await fetch(rpc, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'query', params: { request_type: 'view_access_key_list', finality: 'final', account_id: contract } }) });
    const keys = (await k.json()).result.keys.length;
    return { rounds, keys };
  } catch { return null; }
}

export function statusFor({ report, live, courts = null, contract = 'finalenc2.testnet', network = 'testnet' }) {
  return {
    name: 'Tabbaci',
    what: 'An encyclopedia on NEAR where the writer pays for being wrong. Three sealed AI judges rule on each sentence and anyone can re-check every ruling.',
    contract, network,
    rounds_on_chain: live ? live.rounds : (report ? report.rounds_on_chain : null),
    access_keys_on_contract: live ? live.keys : null,
    last_recheck: report ? {
      checked: report.checked, finished_rounds: report.finished,
      fully_match: report.match, no_mismatch_but_some_lines_need_tools: report.open, mismatch: report.mismatch,
      headline: typeof report.finished === 'number' ? plainHeadline(report) : report.headline,
    } : null,
    court_2: courts && courts.court2 ? {
      contract: COURT2, network: 'testnet', program_fingerprint: '129b36ec',
      access_keys_on_contract: courts.court2.keys, rounds_on_chain: courts.court2.rounds,
      settled_rounds: courts.court2.settled, last_settled_round: courts.court2.lastSettled,
      adds: "The contract itself checks all three judges' signatures and that each judge's quote is really in the page.",
      latest_case: `${SITE}/case.html?round=${courts.court2.lastSettled}&c=${COURT2}`,
    } : null,
    mainnet_pilot: courts && courts.mainnet ? {
      contract: MAINNET_PILOT, network: 'mainnet', bonded_rounds: courts.mainnet.rounds,
      access_keys_on_contract: courts.mainnet.keys, checker_output: `${REPO}/tree/main/proofs/mainnet`,
    } : null,
    one_command: 'node tools/verify/verify.mjs --contract finalenc2.testnet --round 25 --archive archive --network testnet --attested-judges --rpc https://test.rpc.fastnear.com',
    re_check_all: 'node tools/recheck/recheck.mjs --all',
    links: { site: SITE, repo: REPO, judges_guide: `${REPO}/blob/main/JUDGES.md`, llms: `${SITE}/llms.txt`, nightly_status: RAW_STATUS, workflow: `${REPO}/actions/workflows/recheck.yml` },
  };
}

async function main() {
  const [file, out = '.'] = process.argv.slice(2);
  const report = file ? JSON.parse(readFileSync(file, 'utf8')) : null;
  const live = await readLive();
  const courts = await readCourts();
  mkdirSync(out, { recursive: true });
  writeFileSync(join(out, 'badge.json'), JSON.stringify(badgeFor(report), null, 2) + '\n');
  writeFileSync(join(out, 'status.json'), JSON.stringify(statusFor({ report, live, courts }), null, 2) + '\n');
  if (file) writeFileSync(join(out, 'recheck-report.json'), JSON.stringify(report, null, 2) + '\n');
  console.log(badgeFor(report).message);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) main().catch((e) => { console.error(e.message); process.exit(1); });
