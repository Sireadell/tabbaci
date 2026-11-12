# tabbaci-mcp

Read-only access to Tabbaci's rulings on `finalenc2.testnet` (NEAR testnet), as a small JavaScript library and an MCP server. I built it so other AI agents and developers can ask "what did the court rule on this sentence?" and get the outcome, the money state and the command to check it themselves.

Nothing here writes to the chain. No keys, no account, no dependencies. Node 20 or newer.

Status: not published. `package.json` is marked `"private": true` and the name is a placeholder. I flip that and pick the final name before any publish.

TODO (founder): choose and approve a license. None is set in this package yet.

## What an answer always says

- The outcome: T true, F false, U unsure, D judges split.
- Whether the fine is paid yet. A fine is paid only after the 12 hour challenge window closes and someone calls finalize.
- That a data round (numbers read from live status pages) cannot be re-matched later, because those pages change. The stored record on the contract is the proof.

## Use it as an MCP server

From a local copy of this repo:

```json
{
  "mcpServers": {
    "tabbaci": {
      "command": "node",
      "args": ["C:/path/to/encyclopedia/packages/encyclopedia-mcp/bin/tabbaci-mcp.mjs"]
    }
  }
}
```

Once the package is published under its final name, the config will look like this (the name below is the placeholder):

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
| `find_sentence` (`text`) | Bonded sentences containing the text (case-insensitive). |
| `contract_info` | Contract, round count, sealed program fingerprint, judge models, approved sources, stake. |
| `how_to_verify` (`round` optional) | The exact `node tools/verify/verify.mjs ...` command and what MATCH, MISMATCH and CANNOT VERIFY mean. One MISMATCH is expected today: the contract still has one access key I have not deleted. |

An unknown round returns a plain "there is no round N" message with the error flag set.

## Use it as a library

```js
import { createClient } from './src/index.mjs';

const enc = createClient();                 // finalenc2.testnet by default
const r = await enc.getRuling(24);          // r.outcome === 'F', r.fine_paid === false
console.log(enc.checkCommand(24));          // the verify command
console.log(await enc.findSentence('protocol version 41'));
console.log((await enc.listRounds({ limit: 5 })).map((x) => x.round));
console.log(await enc.getContractInfo());
```

`getRuling(round, { includeTx: false })` skips the look-up of the ruling transaction (it scans a few blocks before the last run started, and old blocks may be gone from public nodes, in which case a link to the contract's transactions is returned instead).

Options for `createClient`: `contract`, `network`, `rpcs` (list of node URLs), or your own `rpc(method, params)` function.

## Tests

```
node --test                     # offline, uses a recording of real chain responses
ENCYCLOPEDIA_LIVE=1 node --test # also runs the live check of rounds 20 and 24
node scripts/record-fixture.mjs # re-record test/fixture.json from the live contract
```

The live test expects round 24's fine to be unpaid. Once someone finalizes round 24 it will need updating.
