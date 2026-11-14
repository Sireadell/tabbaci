import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createClient, checkCommand, bondState, describeOutcome } from '../src/index.mjs';
import { createServer, TOOLS } from '../src/server.mjs';

const fixture = JSON.parse(readFileSync(new URL('./fixture.json', import.meta.url), 'utf8'));
// Offline transport: answers view calls from the recorded real chain responses.
const offline = async (method, params) => {
  if (method !== 'query') throw new Error('offline: ' + method);
  const key = `${params.method_name}|${Buffer.from(params.args_base64, 'base64').toString()}`;
  if (!(key in fixture)) return { result: Array.from(Buffer.from('null')) };
  return fixture[key];
};
export const offlineClient = () => createClient({ rpc: offline });

test('checkCommand gives the exact verify command', () => {
  assert.equal(checkCommand(20), 'node tools/verify/verify.mjs --contract finalenc2.testnet --round 20 --archive archive --network testnet --attested-judges --rpc https://test.rpc.fastnear.com');
  assert.throws(() => checkCommand(0));
  assert.throws(() => checkCommand('x'));
});

test('describeOutcome and bondState', () => {
  assert.equal(describeOutcome({ status: 'settled', outcome: 'F' }).label, 'false');
  assert.equal(describeOutcome({ status: 'open' }).letter, 'open');
  const unpaid = bondState({ writer: 'a', word: 'is', stake: '50000000000000000000000', payout: null }, 'F');
  assert.equal(unpaid.paid, false);
  assert.match(unpaid.state, /not paid yet/);
  const fined = bondState({ writer: 'a', word: 'is', stake: '50000000000000000000000', payout: '17375000000000000000000' }, 'F');
  assert.equal(fined.paid, true);
  assert.match(fined.state, /fined 0\.0326 NEAR \(65\.25%/);
});

test('round 20: false, data kind, fine paid', async () => {
  const r = await offlineClient().getRuling(20);
  assert.equal(r.outcome, 'F');
  assert.equal(r.kind, 'data');
  assert.equal(r.fine_paid, true);
  assert.equal(r.judges.length, 3);
  assert.equal(r.sources[0].read_value, '88');
  assert.equal(r.ruling_tx, null); // offline: no block lookup
  assert.match(r.check_command, /--round 20 /);
});

test('round 24: false, bribe, fine paid', async () => {
  const r = await offlineClient().getRuling(24);
  assert.equal(r.outcome, 'F');
  assert.equal(r.fine_paid, true);
  assert.match(r.bonds[0].state, /fined 0\.0326 NEAR \(65\.25%/);
  assert.match(r.sentence, /ignore the sources/);
});

test('unknown round returns null', async () => {
  assert.equal(await offlineClient().getRuling(9999), null);
});

test('listRounds is newest first and findSentence searches by substring', async () => {
  const c = offlineClient();
  const all = await c.listRounds();
  assert.equal(all[0].round, all.length);
  assert.equal(all.at(-1).round, 1);
  const hit = await c.findSentence('protocol version 41');
  assert.ok(hit.some((r) => r.round === 24));
  assert.deepEqual(await c.findSentence('no such sentence anywhere'), []);
  assert.equal((await c.listRounds({ limit: 3 })).length, 3);
});

test('contract info', async () => {
  const i = await offlineClient().getContractInfo();
  assert.equal(i.contract, 'finalenc2.testnet');
  assert.equal(i.round_count, 33);
  assert.equal(i.judges.length, 3);
});

// ---- MCP protocol ----
const rpcCall = (handle, method, params, id = 1) => handle({ jsonrpc: '2.0', id, method, params });

test('mcp: initialize', async () => {
  const h = createServer(offlineClient());
  const r = await rpcCall(h, 'initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 't', version: '1' } });
  assert.equal(r.result.protocolVersion, '2025-06-18');
  assert.ok(r.result.capabilities.tools);
  assert.equal(await h({ jsonrpc: '2.0', method: 'notifications/initialized' }), null);
});

test('mcp: tools/list has the reading tools, the evidence tool and the two sending tools', async () => {
  const h = createServer(offlineClient());
  const r = await rpcCall(h, 'tools/list');
  assert.deepEqual(r.result.tools.map((t) => t.name).sort(), ['bond_sentence', 'contract_info', 'find_sentence', 'get_ruling', 'how_to_verify', 'list_rounds', 'read_evidence', 'wait_for_ruling']);
  assert.equal(TOOLS.length, 8);
});

test('mcp: get_ruling states outcome, fine state and the data-round limit', async () => {
  const h = createServer(offlineClient());
  const a = (await rpcCall(h, 'tools/call', { name: 'get_ruling', arguments: { round: 20 } })).result;
  assert.match(a.content[0].text, /Outcome: F \(FALSE\)/);
  assert.match(a.content[0].text, /already settled/);
  assert.match(a.content[0].text, /cannot be re-matched later/);
  const b = (await rpcCall(h, 'tools/call', { name: 'get_ruling', arguments: { round: 24 } })).result;
  assert.match(b.content[0].text, /already settled/);
  assert.match(b.content[0].text, /fined 0\.0326 NEAR/);
});

test('mcp: unknown round, unknown tool, unknown method, bad input', async () => {
  const h = createServer(offlineClient());
  const a = (await rpcCall(h, 'tools/call', { name: 'get_ruling', arguments: { round: 9999 } })).result;
  assert.equal(a.isError, true);
  assert.match(a.content[0].text, /no round 9999/);
  assert.equal((await rpcCall(h, 'tools/call', { name: 'nope', arguments: {} })).error.code, -32602);
  assert.equal((await rpcCall(h, 'bogus')).error.code, -32601);
  const bad = (await rpcCall(h, 'tools/call', { name: 'get_ruling', arguments: { round: 'x' } })).result;
  assert.equal(bad.isError, true);
});

test('mcp: find_sentence, list_rounds, contract_info, how_to_verify', async () => {
  const h = createServer(offlineClient());
  const f = (await rpcCall(h, 'tools/call', { name: 'find_sentence', arguments: { text: 'protocol version 41' } })).result;
  assert.match(f.content[0].text, /#24/);
  const none = (await rpcCall(h, 'tools/call', { name: 'find_sentence', arguments: { text: 'zzzz' } })).result;
  assert.match(none.content[0].text, /No bonded sentence/);
  const l = (await rpcCall(h, 'tools/call', { name: 'list_rounds', arguments: { limit: 2 } })).result;
  assert.equal(l.structuredContent.rounds.length, 2);
  const c = (await rpcCall(h, 'tools/call', { name: 'contract_info', arguments: {} })).result;
  assert.match(c.content[0].text, /33 rounds/);
  const v = (await rpcCall(h, 'tools/call', { name: 'how_to_verify', arguments: { round: 20 } })).result;
  assert.match(v.content[0].text, /--round 20/);
  assert.match(v.content[0].text, /MISMATCH/);
  assert.match(v.content[0].text, /One MISMATCH is expected/);
});

const LIVE = process.env.ENCYCLOPEDIA_LIVE === '1';
test('live: rounds 20 and 24 on the real contract', { skip: !LIVE && 'set ENCYCLOPEDIA_LIVE=1 to run' }, async () => {
  const c = createClient();
  const a = await c.getRuling(20, { includeTx: false });
  assert.equal(a.outcome, 'F');
  assert.equal(a.fine_paid, true);
  const b = await c.getRuling(24, { includeTx: false });
  assert.equal(b.outcome, 'F');
  assert.equal(b.fine_paid, true);
});
