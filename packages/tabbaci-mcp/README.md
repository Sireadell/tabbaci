# tabbaci-mcp

Status: version 0.2.0. Version 0.1.0 is on npm and only reads. 0.2.0 adds three tools that let an AI bond its own sentences.

Tabbaci is a court on NEAR testnet. Writers put money on sentences, a program sealed in an Intel chip fetches the evidence, three NEAR AI models rule, and wrong writers are fined. I built this package so other AI agents and developers can read those rulings, and so an AI can put play money behind its own words and get fined when it is wrong.

It's a tool an AI agent plugs into, not an agent. It works as a small JavaScript library and as an MCP server (MCP is the common way AI assistants connect to outside tools). Ask it "what did the court rule on this sentence?" and you get the outcome, whether the money has been paid out, and the command to check it yourself. Or let the AI write a sentence, bond it, and wait for the court.

No keys, no account, no dependencies. The AI never holds a wallet: sending goes through the Tabbaci site's free sender, and the site's own test account pays. Node 20 or newer. MIT license.

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

From npm:

```json
{
  "mcpServers": {
    "tabbaci": { "command": "npx", "args": ["-y", "tabbaci-mcp"] }
  }
}
```

In Claude Code, one line does the same, once 0.2.0 is on npm:

```
claude mcp add tabbaci -- npx -y tabbaci-mcp
```

| Tool | What it does |
|---|---|
| `get_ruling` (`round`) | One ruling: sentence, outcome, kind, writer, bond, fine state, judge answers, blocks, ruling transaction, check command. |
| `list_rounds` (`limit` optional) | Every round, newest first. |
| `find_sentence` (`text`) | Bonded sentences containing the text (case doesn't matter). |
| `contract_info` | Contract, round count, sealed program fingerprint, judge models, approved sources, stake. |
| `how_to_verify` (`round` optional) | The exact `node tools/verify/verify.mjs ...` command and what MATCH, MISMATCH and CANNOT VERIFY mean. |
| `read_evidence` (`wikipedia_page`) | The exact text the court will read: only the opening summary of that Wikipedia page plus its one-line Wikidata description. Anything not stated there comes back unsure. Free, reads Wikipedia only. |
| `bond_sentence` (`sentence`, `wikipedia_page`, `sureness`) | Sends the sentence to the site's free sender, which bonds it with play money and starts the court. `sureness` is *is*, *very likely*, *likely* or *disputed*. Returns the round number and links. The ruling takes about a minute or a few minutes. |
| `wait_for_ruling` (`round`) | Waits until the court rules, checking every 5 seconds for up to about 10 minutes, and asks for another run if one comes back empty. Returns the outcome, each judge's answer, and whether the sentence was fined. |

An unknown round returns a plain "there is no round N" message with the error flag set.

The check command runs from a copy of the Tabbaci repo. Add `--rpc https://test.rpc.fastnear.com` to it, because the default testnet node starts refusing after a few calls.

## Let an AI bond its own sentences

I installed the add-on in Claude Code (Sonnet model) and typed one prompt:

> Write 5 short facts about Lagos, one of them wrong on purpose, and bond each on Tabbaci with how sure you are.

Claude called `read_evidence` on Lagos first, bonded 5 sentences, then waited for each ruling. All 3 judges agreed on every round.

| Round | Sentence | Word | Ruling |
|---|---|---|---|
| [46](https://tabbaci.vercel.app/case.html?round=46) | Lagos is a large port city in southwestern Nigeria. | is | True |
| [47](https://tabbaci.vercel.app/case.html?round=47) | Lagos is the largest city in Nigeria. | is | True |
| [48](https://tabbaci.vercel.app/case.html?round=48) | Lagos is the most populous urban area on the African continent. | very likely | True |
| [49](https://tabbaci.vercel.app/case.html?round=49) | Lagos is a former capital of Nigeria. | is | Unsure, bond paid back in full |
| [50](https://tabbaci.vercel.app/case.html?round=50) | Lagos is the current capital of Nigeria. (the planted wrong one) | very likely | False, fined 39% of 0.05 NEAR, taken when the objection window closes |

Round 49 is true in the world, but "former capital" appears only in the one-line Wikidata description, not in the Wikipedia summary, so the judges did not confirm it.

What each word wins or loses, paid when the 5.8 hour objection window closes:

| Word | Ruled true | Ruled false |
|---|---|---|
| is | +24.75% | -65.25% |
| very likely | +21% | -39% |
| likely | +12.75% | -17.25% |
| disputed | 0 | 0 |

*Disputed* puts nothing at risk, so it can never be fined.

Limits:

- Testnet play money only. The site's test account, `tabbaci-visitors.testnet`, opens the round and bonds 0.05 test NEAR on the AI's behalf, through the free sender at `https://tabbaci.vercel.app/api/write`.
- An AI can send 6 sentences an hour from one place. All senders, people and AIs, share the same 30 a day.
- On the chain these rounds carry the tag `agent`, and the site shows them as "sent by an AI agent".

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

console.log(await enc.readEvidence('Lagos'));  // the exact text the court will read
const { round } = await enc.bondSentence({
  sentence: 'Lagos is the largest city in Nigeria.',
  wikipedia_page: 'Lagos',
  sureness: 'is',
});
console.log(await enc.waitForRuling(round));   // outcome, judge answers, fined or not
```

`getRuling(round, { includeTx: false })` skips looking up the ruling transaction. That look-up scans a few blocks before the last run started, and old blocks may be gone from public nodes. When they are, you get a link to the contract's transactions instead.

Options for `createClient`: `contract`, `network`, `rpcs` (list of node URLs), your own `rpc(method, params)` function, or `site` (the Tabbaci site that sends bonds, default `https://tabbaci.vercel.app`).

## Tests

28 offline tests.

```
node --test                     # offline, uses a recording of real chain responses
ENCYCLOPEDIA_LIVE=1 node --test # also runs the live check of rounds 20 and 24
node scripts/record-fixture.mjs # re-record test/fixture.json from the live contract
```

The live test expects rounds 20 and 24 to be false with their fines paid.
