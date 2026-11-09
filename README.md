# [Name to be decided]

[Name] is an encyclopedia on NEAR where every checkable sentence has money behind it, and a sealed program decides whether it was true.

[Founder: one sentence, first person, on why you built it.]

> Who decided this sentence is true, and can I check that without trusting them?

Most sites ask you to trust whoever runs them. Here the writer pays when a sentence is wrong, the evidence is fetched by a program nobody can tamper with, and the ruling sits on chain where you can re-check it yourself.

## How it works

1. **A writer puts money on a sentence.** For example: "On 7 November 2026, NEAR mainnet runs protocol version 88." The writer adds one word for how sure they are (*is*, *very likely*, *likely* or *disputed*). The stake is the same whichever word they pick.
2. **A sealed program checks it.** A NEAR contract asks OutLayer to run our courtroom program inside an Intel TDX chip, so nobody can see or change what it does while it runs. The program fetches the evidence itself from two sites run by different owners, both on an approved list. Then three AI judges answer. The program signs one ruling: true, false, unsure, split or void.
3. **The contract pays out.** It reads the ruling and settles every stake with one fixed public formula (below). It has no method that can replace its code. This test contract still holds one key so it can be redeployed; the final contract will have its key deleted, so nobody, including me, can change it.

A sentence about a number is decided by the numbers the two sites return, not by the AI. The judges only confirm that the sentence states that number.

## The payout rule

You pick one word. Being right with a sure word earns more, being wrong with it costs more.

| Word you pick | If the sentence is true | If it is false |
|---|---|---|
| is | you gain 24.75% of the stake | you lose 65.25% |
| very likely | you gain 21% | you lose 39% |
| likely | you gain 12.75% | you lose 17.25% |
| disputed | your stake comes back | your stake comes back |

The formula is set up so that saying how sure you really are earns the most on average. Claiming more certainty than you have, or hiding behind a vaguer word, earns less. Gains are paid from other writers' losses and a reserve. If a ruling comes back unsure, split or void, everyone gets their stake back in full. The full rule and its tests are in `docs/PAYOUT_RULE.md`.

Today a sentence is a bond, not a market: writers stake on their own words. Trading odds on someone else's sentence is not built.

## Check it yourself in one minute

You need Node 18 or newer. No account, no keys, nothing to install.

**Do this:**

```
node tools/verify/verify.mjs --contract court5.sireadell.testnet --round 6 --archive archive
```

**You will see** one line per check, each saying MATCH, MISMATCH or CANNOT VERIFY. Round 6 is the sentence "NEAR is a blockchain platform", checked against Wikipedia and GitHub. Today the lines read:

| Check | Today | What it means |
|---|---|---|
| contract has no keys | MISMATCH | This test contract still holds one key. The final contract will hold none. |
| contract has no upgrade method | MATCH | None of its 21 methods can replace the code, the keys or an owner. |
| courtroom program matches its fingerprint | MATCH | The program file hashes to the fingerprint the contract holds. |
| stored request, verdict, attempt list | MATCH | The ruling belongs to this round and these exact inputs. Every attempt is listed. |
| bond payouts follow the rule | MATCH | Each payout is recomputed from the formula. |
| source bytes match their fingerprint | MATCH | The saved Wikipedia and GitHub pages hash to what the program saw. |
| judge signatures | CANNOT VERIFY | The signatures are valid, but on testnet the judges are stand-ins signed by a test key, not the models' own keys. |
| judge reply words match the answers | CANNOT VERIFY | Only a fingerprint of each reply is published, not its words. |
| sealed-box attestation | CANNOT VERIFY | Needs OutLayer's `outlayer-verify` tool. See below. |

The last line ends with OVERALL: MISMATCH, because of the one key.

**To check the sealed chip too,** install OutLayer's `outlayer-verify` (it needs Rust and a C compiler) and add `--tx 8XYvkuJR2ysAoS8rBsBCF6L5xeaWB1N8dBybejV1YF6n`. That line then says MATCH: the ruling came out of genuine Intel TDX hardware running the program approved on chain (OutLayer task 27107).

The saved proof for round 6 is in `proofs/court5/round-6.bundle.json`. The checker reads it by itself and re-checks it offline, with no network (details in `docs/SEALED_BOX_PROOF.md`).

Exit code 0 means everything matched, 1 means something did not, 2 means nothing mismatched but something could not be checked.

**Before a page goes on chain,** `node tools/writer/check.mjs content/pages/near-chain.json --contract court5.sireadell.testnet` runs the courtroom's own pre-judge rules on every sentence (HTTPS, size, approved list, two groups, numbers agree, key words found) and prints the text the judges would read.

**To see it in a browser,** serve the `site/` folder from any static file server and open `ledger.html`. It reads the contract live, with no backend. `index.html` lets you try to sneak a lie past it.

## What runs today (NEAR testnet)

| Part | Where | Proof |
|---|---|---|
| Courtroom program (Rust, wasm32-wasip2) | `programs/court` | 22 tests. It fetches 2 to 3 sources (HTTPS only, 8 s each, 2 MB cap). It asks 3 judges with a fixed prompt, checks each judge's signature, and stops itself at 75 s. |
| Ruling size | same | 733 to 742 bytes with 3 judges and 2 sources. OutLayer's limit is 900. |
| Time from request to payout callback | testnet runs | Usually 11 blocks, once 67. NEAR waits about 200 blocks. |
| Contract | `contracts/courtroom`, rules in `contracts/rules` | Each round stores its request once, and every retry sends the same bytes. One attempt runs at a time and every attempt is published. A round is void after 3 failed runs. Sources must come from 2 independent groups on a list only the DAO account can change, and a removal takes 72 hours. 17 rule tests, 15 tests of the contract itself. |
| Payout formula | `contracts/payout`, `docs/PAYOUT_RULE.md` | 16 property tests. On chain: a right *is* earns +24.75%, a right *likely* +12.75%, *disputed* gets the stake back, a wrong *is* loses 65.25%. |
| Checker | `tools/verify` | Node, no packages. Its keccak and signature recovery match the Rust program. Recomputes payouts and outcomes. |
| Reproducible build | `build/reproduce.sh` | Two clean builds in different folders give the same sha256 (on Windows). |
| Site | `site/` | "Try to lie to it", a live ledger of every ruling and every lie tried, and pages. |
| Pages | `content/pages/` (3 pages), rounds 4 to 18 on `court5` | `near-chain` (rounds 4 to 6): a true number (two RPC providers both say 88), a false number (87), and a prose sentence checked against Wikipedia and GitHub. `near-tools` (rounds 7 to 12): crate and package versions from crates.io, npm and GitHub. `lagos-abuja` (rounds 13 to 18): facts from Wikidata and Wikipedia. Rounds 1 to 3 were earlier tests. All 15 page rounds settled. |
| Accuracy test | `tools/accuracy` | 200 sentences (100 true, 100 false) with truth from Wikidata, run against the 3 real NEAR AI judges on 9 Nov 2026: the 3-judge ruling was right on 198, wrong on 0, no ruling on 2. Cost: $0.017 for 600 judge calls. |

## What it does not do

- It does not prove a sentence is true about the world. It proves which listed sites the evidence came from, that a sealed chip fetched it at a recorded time, and how three models ruled on it.
- Every ruling on chain so far uses stand-in judges signed by a test key, and the site says so. The real NEAR AI judges have been tested off chain (accuracy and timing below) but have not yet ruled on chain.
- Three models from one provider are three models, not three independent parties.
- It never picks a side when two sources disagree. The round is void and stakes go back.
- It cannot be bent by whoever relays the request. The worst they can force is a void round, and every failed attempt is published.
- Hiding an order to the judges inside a sentence should not work: the sentence reaches them inside a fixed frame marked as untrusted text. With stand-in judges this is not yet tested against real models.
- Nothing is on mainnet yet.

## Limits today, stated plainly

| Limit | State |
|---|---|
| Real AI judges | Tested off chain on 9 Nov 2026, not yet ruling on chain. A full 3-judge ruling with signed receipts takes under 5 seconds (target 60, cap 90). Every on-chain ruling so far uses stand-ins signed by a test key, and the site says so. |
| Accuracy | 198 of 200 right, 0 wrong, 2 with no ruling, on simple "capital of" sentences. Harder sentences are not measured. |
| Judge models | `deepseek-ai/DeepSeek-V4.1-Flash-1017`, `Qwen/Qwen3.8-27B` and `Qwen/Qwen3.6-35B-A3B-FP8`. Each one was checked: a fresh Intel-signed report binds its signing key, and its reply receipts recover to that key (`privacy/tools/verify-judges.mjs`, and `--attested-judges` on the checker). Only these 3 pass today, so the top appeal level has 3 judges, not 5. |
| Per-run Intel attestation | Proven for round 6 with OutLayer's `outlayer-verify`. Installing it needs Rust and a C compiler (`cargo install --git https://github.com/out-layer/outlayer-verify`), since OutLayer ships no ready-made copy. |
| Judge reply words | Only each reply's fingerprint is published, so the checker cannot confirm that the reply text equals the recorded answer. |
| Pages that change every second | An RPC status page cannot be fetched again byte for byte. The ruling keeps the extracted number and the fingerprint. |
| Build on Linux or macOS | Not yet compared with the Windows build. A Linux check is written (`.github/workflows/reproducible-build.yml`) but has never run. |
| Challenges, appeals, random audits, expiry | Built and tested (`contracts/rules/src/ladder.rs` and the tests in `contracts/courtroom`), but not yet on the `court5` testnet contract: `court5` answers "method not found" to the challenge methods. They arrive with the final contract. |
| Writers | The three pages were written by hand. AI writers plug in at `tools/writer`. |
| Mainnet | Nothing is deployed on mainnet yet. |

## Which track this fits

NEAR HACKATHON has 13 tracks. This entry fits three. The lines below are the track goals as summarised in the hackathon notes, each answered plainly.

**Privacy & Private AI** (confidential computation, private finance, AI that protects data; TEE, NEAR AI Cloud)
- Confidential computation: the courtroom program runs inside an Intel TDX chip on OutLayer. Nobody can see or change what it does while it runs.
- NEAR AI Cloud: the three judges are NEAR AI models, called from inside the chip. Not yet run for real, because the account has no credit.
- Private data: the program keeps its judge key secret. A script locks that key to one exact program build (`tools/lockkey`). Proven on testnet with a dummy key.

**AI Security & Defense** (policy engines, reputation, scam detection; function-call keys, TEE attestation)
- TEE attestation: every ruling carries a signed proof from the chip. The checker re-checks it offline (`proofs/`, `docs/SEALED_BOX_PROOF.md`).
- Policy engine: fixed rules decide the outcome, not a person. Sources must come from an approved list, two independent groups, HTTPS only.
- Scam detection: a sentence that hides an order to the judges reaches them as untrusted text in a fixed frame. Not yet tested against real models.

**Open Innovation** (anything else about money, intelligence, ownership)
- Money: writers lock a fixed stake behind each sentence and a public formula pays or takes it.
- Intelligence: AI judges rule only on evidence the sealed program fetched itself.
- Ownership: no owner. The contract has no upgrade method and the final deployment holds no keys.

## Run the tests

```
cd contracts/rules   && cargo test
cd contracts/payout  && cargo test
cd programs/court    && cargo test --lib
cd contracts/courtroom && cargo test
cd tools/verify      && node --test
```

On Windows without the Microsoft C++ linker, add `+stable-x86_64-pc-windows-gnu` after `cargo`.

To build the contract, run `cargo near build non-reproducible-wasm --no-abi` in `contracts/courtroom`. To build the courtroom program, run `./build/reproduce.sh`. Add `FEATURES=standin` for the test version with stand-in judges.

## Repository

| Folder | What is in it |
|---|---|
| `programs/court` | The courtroom program that runs in the sealed chip |
| `contracts/courtroom` | The NEAR contract: rounds, sources, bonds |
| `contracts/rules` | Round rules, tested without a blockchain |
| `contracts/payout` | The payout formula and its tests |
| `tools/verify` | The one-command checker |
| `tools/archive` | Saves fetched pages by fingerprint |
| `tools/writer` | Publishes a page: opens rounds and bonds words. `check.mjs` checks a page before it is published |
| `content/pages` | The three pages |
| `proofs` | Saved sealed-box proofs that re-check offline (see `docs/SEALED_BOX_PROOF.md`) |
| `docs` | The payout rule, the sealed-box proof, hosting notes |
| `tools/accuracy` | The 200-sentence judge test |
| `site` | Try to lie, ledger, pages |
| `archive` | Page bytes that past rulings saw |

> Who decided this sentence is true, and can I check that without trusting them?
