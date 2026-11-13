# tabbaci-mcp

Status: published on npm as tabbaci-mcp.

Tabbaci is a court on NEAR testnet. Writers put money on sentences, a program sealed in an Intel chip fetches the evidence, three NEAR AI models rule, and wrong writers are fined. I built this package so other AI agents and developers can read those rulings.

It's a tool an AI agent plugs into, not an agent. It works as a small JavaScript library and as an MCP server (MCP is the common way AI assistants connect to outside tools). Ask it "what did the court rule on this sentence?" and you get the outcome, whether the money has been paid out, and the command to check it yourself.

It only reads from the chain. No keys, no account, no dependencies. Node 20 or newer. MIT license.

## What an answer always says

- The outcome: T true, F false, U unsure, D judges split.
- Whether the fine or payout has been made. That happens only after the challenge window closes (36,000 blocks, about 5.8 hours) and someone calls `finalize`.
- On a data round (numbers read from live status pages), that the pages can't be matched again later because they change. The record stored on the contract is the proof.

Two real examples from `finalenc2.testnet`:

| Round | What it says |
|---|---|
| 24 | Ruled false (F). `fine_paid` is `true`: the writer was fined 0.032625 NEAR, 65.25% of the 0.05 NEAR stake, paid on 12 November 2026. |
| 34 | Ruled true (T), not finalized yet. `fine_paid` is `false`, so the payout hasn't been made yet. |

## Use it as an MCP server

From a local copy of the repo:

```json
{
  "mcpServers": {
    "tabbaci": {
      "command": "node",
      "args": ["C:/path/to/encyclopedia/packages/tabbaci-mcp/bin/tabbaci-mcp.mjs"]
    }
  }
}
```

After it is published to npm:

```json
{
  "mcpServers": {
    "tabbaci": { "command": "npx", "args": ["-y", "tabbaci-mcp"] }
  }
}
```

| Tool | What it does |
|---|---|
| `get_ruling` (`round`) | One ruling: sentence, outcome, kind, writer, bond, fine state, judge answers, blocks, ruling transaction, check command. |
| `list_rounds` (`limit` optional) | Every round, newest first. |
| `find_sentence` (`text`) | Bonded sentences containing the text (case doesn't matter). |
| `contract_info` | Contract, round count, sealed program fingerprint, judge models, approved sources, stake. |
| `how_to_verify` (`round` optional) | The exact `node tools/verify/verify.mjs ...` command and what MATCH, MISMATCH and CANNOT VERIFY mean. One MISMATCH is expected today: the contract still has one access key I haven't deleted. |

An unknown round returns a plain "there is no round N" message with the error flag set.

The check command runs from a copy of the Tabbaci repo. Add `--rpc https://test.rpc.fastnear.com` to it, because the default testnet node starts refusing after a few calls.

## Use it as a library

```js
import { createClient } from './src/index.mjs';

const enc = createClient();                 // finalenc2.testnet by default
const a = await enc.getRuling(24);          // a.outcome === 'F', a.fine_paid === true
const b = await enc.getRuling(34);          // b.outcome === 'T', b.fine_paid === false until finalized
console.log(enc.checkCommand(24));          // the verify command
console.log(await enc.findSentence('protocol version 92'));
console.log((await enc.listRounds({ limit: 5 })).map((x) => x.round));
console.log(await enc.getContractInfo());
```

`getRuling(round, { includeTx: false })` skips looking up the ruling transaction. That look-up scans a few blocks before the last run started, and old blocks may be gone from public nodes. When they are, you get a link to the contract's transactions instead.

Options for `createClient`: `contract`, `network`, `rpcs` (list of node URLs), or your own `rpc(method, params)` function.

## Tests

```
node --test                     # offline, uses a recording of real chain responses
ENCYCLOPEDIA_LIVE=1 node --test # also runs the live check of rounds 20 and 24
node scripts/record-fixture.mjs # re-record test/fixture.json from the live contract
```

The live test expects rounds 20 and 24 to be false with their fines paid.
