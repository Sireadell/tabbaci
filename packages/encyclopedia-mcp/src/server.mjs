// MCP server over stdio (newline-delimited JSON-RPC 2.0, hand-rolled, no dependencies). Read-only.
import { createInterface } from 'node:readline';
import { createClient } from './index.mjs';

const PROTOCOL = '2025-06-18';
const SUPPORTED = ['2025-06-18', '2025-03-26', '2024-11-05'];

const NO_REMATCH = 'Data-kind rounds (numbers read from live status pages) cannot be re-matched later, because those pages change. For them the stored record on the contract is the proof.';

export const TOOLS = [
  { name: 'get_ruling', description: 'Get one ruling by round number: the sentence, the outcome (T true, F false, U unsure, D judges split), kind, writer, bond, whether the fine is paid, each judge answer, block heights, the ruling transaction if findable, and the exact command to re-check it.',
    inputSchema: { type: 'object', properties: { round: { type: 'integer', minimum: 1, description: 'Round number, 1 or higher.' } }, required: ['round'], additionalProperties: false } },
  { name: 'list_rounds', description: 'List every round, newest first, with sentence, outcome, kind, writer and whether the fine is paid.',
    inputSchema: { type: 'object', properties: { limit: { type: 'integer', minimum: 1, description: 'Only the newest N rounds.' } }, additionalProperties: false } },
  { name: 'find_sentence', description: 'Search the bonded sentences by a piece of text (case-insensitive substring) and return the matching rounds with their outcomes.',
    inputSchema: { type: 'object', properties: { text: { type: 'string', description: 'Text to look for inside a sentence.' } }, required: ['text'], additionalProperties: false } },
  { name: 'contract_info', description: 'Describe the contract: its name, network, number of rounds, the sealed courtroom program fingerprint, the three judge models, the approved sources and the stake size.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false } },
  { name: 'how_to_verify', description: 'Explain how to re-check a ruling yourself: returns the exact command and what MATCH, MISMATCH and CANNOT VERIFY mean. Optionally pass a round number.',
    inputSchema: { type: 'object', properties: { round: { type: 'integer', minimum: 1 } }, additionalProperties: false } },
];

const fineLine = (r) => {
  if (r.fine_paid === null) return 'No money is bonded on this sentence, so there is no fine.';
  if (r.fine_paid) return `Fine or payout is already settled: ${r.bonds.map((b) => `${b.writer} ${b.state}`).join('; ')}.`;
  if (r.outcome === 'F' || r.outcome === 'T') return 'Fine is NOT paid yet. It is paid once the 12 hour challenge window has closed and someone calls finalize.';
  return 'Fine is not paid: the round has no final ruling yet.';
};

export function rulingText(r) {
  const verb = { T: 'TRUE', F: 'FALSE', U: 'UNSURE', D: 'JUDGES SPLIT', open: 'WAITING FOR A RULING', void: 'CANCELLED' }[r.outcome] || r.outcome;
  const lines = [
    `Round ${r.round} on ${r.contract} (${r.network}): "${r.sentence}"`,
    `Outcome: ${r.outcome} (${verb}).`,
    `Kind: ${r.kind}${r.tag ? ` (tag: ${r.tag})` : ''}. Writer: ${r.writer}.`,
    fineLine(r),
    r.judges.length ? `Judges: ${r.judges.map((j) => `${j.model} answered ${j.answer}${j.signed ? '' : ' (no signed reply)'}`).join('; ')}.` : 'No judge answers yet.',
  ];
  if (r.kind === 'data') lines.push(r.judge_note, NO_REMATCH);
  if (r.ruled_in_blocks !== null) lines.push(`Ruled in ${r.ruled_in_blocks} blocks (last block ${r.last_block}).`);
  if (r.ruling_tx_url) lines.push(`Ruling transaction: ${r.ruling_tx_url}`);
  lines.push(`Re-check it: ${r.check_command}`);
  return lines.join('\n');
}

export const HOW_TO = [
  'To re-check a ruling without trusting the people who made it, get a copy of the Tabbaci repo (Node 18 or newer, no account, no keys) and run, from its root:',
  '',
  '{CMD}',
  '',
  'Each line it prints is one of:',
  '- MATCH: that check passed (for example the stored request matches its hash, or the payout follows the rule).',
  '- MISMATCH: that check failed.',
  '- CANNOT VERIFY: the check could not be run (for example an old block is no longer kept by public nodes).',
  'Exit code 0 means everything matched, 1 means a mismatch, 2 means something could not be checked.',
  'One MISMATCH is expected on every round today: the contract still has one access key that its keeper has not deleted yet, so the "contract cannot be changed" check reports MISMATCH on purpose.',
  NO_REMATCH,
].join('\n');

function ok(text, data) { return { content: [{ type: 'text', text }, { type: 'text', text: JSON.stringify(data) }], structuredContent: data }; }
function fail(text) { return { content: [{ type: 'text', text }], isError: true }; }

export async function callTool(client, name, a = {}) {
  try {
    if (name === 'get_ruling') {
      const r = await client.getRuling(a.round);
      if (!r) return fail(`There is no round ${a.round} on ${client.contract}.`);
      return ok(rulingText(r), r);
    }
    if (name === 'list_rounds') {
      const rs = await client.listRounds({ limit: a.limit });
      const text = [`${rs.length} round(s) on ${client.contract}, newest first.`, ...rs.map((r) => `#${r.round} [${r.outcome}] ${r.kind}, fine ${r.fine_paid === null ? 'n/a' : r.fine_paid ? 'paid' : 'not paid yet'}: ${r.sentence}`), NO_REMATCH].join('\n');
      return ok(text, { rounds: rs });
    }
    if (name === 'find_sentence') {
      if (typeof a.text !== 'string' || !a.text.trim()) return fail('Give some text to search for.');
      const rs = await client.findSentence(a.text);
      const text = rs.length ? [`${rs.length} match(es):`, ...rs.map((r) => `#${r.round} [${r.outcome}] ${r.kind}, fine ${r.fine_paid === null ? 'n/a' : r.fine_paid ? 'paid' : 'not paid yet'}: ${r.sentence}`)].join('\n') : `No bonded sentence on ${client.contract} contains "${a.text}".`;
      return ok(text, { matches: rs });
    }
    if (name === 'contract_info') {
      const i = await client.getContractInfo();
      return ok(`${i.contract} on ${i.network}: ${i.round_count} rounds, ${i.judges.length} judges, stake ${i.stake_near} NEAR per sentence. Read-only access.`, i);
    }
    if (name === 'how_to_verify') {
      const cmd = a.round ? client.checkCommand(a.round) : client.checkCommand(1).replace(/--round 1/, '--round <round number>');
      return ok(HOW_TO.replace('{CMD}', cmd), { command: cmd });
    }
    return fail(`Unknown tool: ${name}`);
  } catch (e) {
    return fail(`Could not complete that: ${e.message}`);
  }
}

export function createServer(client = createClient()) {
  // Returns a response object, or null for notifications.
  return async function handle(msg) {
    if (!msg || msg.jsonrpc !== '2.0' || typeof msg.method !== 'string') {
      return { jsonrpc: '2.0', id: msg && msg.id !== undefined ? msg.id : null, error: { code: -32600, message: 'Invalid request' } };
    }
    const isNote = msg.id === undefined || msg.id === null;
    const reply = (result) => (isNote ? null : { jsonrpc: '2.0', id: msg.id, result });
    const err = (code, message) => (isNote ? null : { jsonrpc: '2.0', id: msg.id, error: { code, message } });
    switch (msg.method) {
      case 'initialize': {
        const want = msg.params && msg.params.protocolVersion;
        return reply({ protocolVersion: SUPPORTED.includes(want) ? want : PROTOCOL, capabilities: { tools: {} }, serverInfo: { name: 'tabbaci-mcp', version: '0.1.0' },
          instructions: 'Read-only access to verified rulings on the Tabbaci contract (NEAR testnet). Always report the outcome, whether the fine is paid, and that data-kind rounds cannot be re-matched later.' });
      }
      case 'ping': return reply({});
      case 'tools/list': return reply({ tools: TOOLS });
      case 'tools/call': {
        const p = msg.params || {};
        if (!TOOLS.some((t) => t.name === p.name)) return err(-32602, `Unknown tool: ${p.name}`);
        return reply(await callTool(client, p.name, p.arguments || {}));
      }
      default:
        return isNote ? null : err(-32601, `Method not found: ${msg.method}`);
    }
  };
}

export function runStdio(client) {
  const handle = createServer(client);
  const rl = createInterface({ input: process.stdin });
  let pending = 0, closed = false;
  const done = () => { if (closed && pending === 0) process.exit(0); };
  rl.on('line', async (line) => {
    if (!line.trim()) return;
    pending++;
    let out;
    try { out = await handle(JSON.parse(line)); }
    catch (e) { out = e instanceof SyntaxError ? { jsonrpc: '2.0', id: null, error: { code: -32700, message: 'Parse error' } } : { jsonrpc: '2.0', id: null, error: { code: -32603, message: 'Internal error' } }; }
    if (out) process.stdout.write(JSON.stringify(out) + '\n');
    pending--; done();
  });
  rl.on('close', () => { closed = true; done(); });
}
