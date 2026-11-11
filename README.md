# Living Encyclopedia

Living Encyclopedia is an encyclopedia on NEAR where every checkable sentence has money behind it, and a program sealed inside an Intel chip decides whether it is true.

I built it because I wanted to read a fact online and check, myself, who decided it was true and on what evidence, without having to trust them.

> Who decided this sentence is true, and can I check that without trusting them?

Most sites ask you to trust whoever runs them. Here the writer pays when a sentence is wrong, the evidence is fetched by a program nobody can see into or change, three NEAR AI models rule on it, and the ruling sits on chain where anyone can re-check it.

Everything below runs on one contract on NEAR testnet: `finalenc2.testnet`.

## What happened on chain

| Rounds | What they are | Ruling |
|---|---|---|
| 2 to 16 | Three pages of real sentences: NEAR's protocol version, near-sdk and near-api-js releases, facts about Lagos and Abuja | True on 14 of 15. Round 3 is a planted false number (protocol version 87). It came back unsure, because 2 of the 3 judges gave no signed reply, so nothing could be confirmed. |
| 20, 21, 22 | Planted lies: NEAR mainnet runs protocol version 92, 85 and 89. Each one has a writer who staked 0.05 NEAR on the word *is* | False on all three. All 3 judges signed each ruling with a key Intel vouches for. |
| 17, 18, 19 | The same kind of planted lies, opened without stakes by mistake | False on all three. No money behind them. |
| 1 | Opened before the writers had money in the contract | Never ruled. |

Every round that ran needed exactly one attempt. On rounds 2 to 16 and 20 to 22, going from the ruling call to a settled round took 15 to 47 blocks (about one second each).

## Check it yourself in one minute

You need Node 18 or newer and a copy of this repo. No account and no keys.

Install Intel's quote checker, once (a few seconds):

```
npm ci --prefix privacy/tools
```

Check round 20, the planted lie "On 11 November 2026, NEAR mainnet runs protocol version 92":

```
node tools/verify/verify.mjs --contract finalenc2.testnet --round 20 --archive archive --network testnet --attested-judges
```

It took 24 to 31 seconds on my machine. `--attested-judges` asks NEAR AI for a fresh Intel-signed report for each judge model (free, no key) and checks that each judge's signature on round 20 comes from the key in that report.

You will see:

| Check | Result | What it means |
|---|---|---|
| contract has no keys | MISMATCH | The contract still holds 1 access key. I am keeping it on purpose until I give the go to delete it. |
| contract has no upgrade method | MATCH | None of its 28 methods can replace the code, the keys or an owner. |
| contract code fingerprint, courtroom program fingerprint | MATCH | The sealed program's fingerprint on chain is the build in this repo (sha256 `2c8e24a5...`). |
| stored request, sentence, attempt list, verdict, outcome | MATCH | The ruling belongs to round 20 and its exact inputs. One attempt, 32 blocks. Verdict F, and the contract says F. |
| judge 0, 1 and 2 signed by a key Intel vouches for | MATCH | DeepSeek-V4.1-Flash, Qwen3.8-27B and Qwen3.6-35B each signed the answer from inside a sealed chip. |
| bond payouts follow the rule | CANNOT VERIFY | The contract does not record the reserve at settlement, so the payout maths cannot be redone from the chain alone. |
| source bytes, outcome from the numbers | CANNOT VERIFY | The two sources are live NEAR RPC status pages that change every second, so their exact bytes cannot be saved and compared later. |
| judge reply words | CANNOT VERIFY | Only a fingerprint of each reply is published, not its words. |
| sealed-box attestation | CANNOT VERIFY | Needs OutLayer's `outlayer-verify` tool and the attempt transaction (see below). |

The last line reads OVERALL: MISMATCH and the exit code is 1, because of the one key. Exit code 0 means everything matched, 1 means something did not, 2 means nothing mismatched but something could not be checked.

The same command works on any round from 2 to 22. I ran it on rounds 2 to 16: in all 15 the only mismatch is the key, and all 3 judges match an Intel-vouched key (`proofs/finalenc2/checker-all-rounds.md`).

**To check the sealed chip too,** install `outlayer-verify` (it needs Rust and a C compiler: `cargo install --git https://github.com/out-layer/outlayer-verify`) and add `--tx` with the attempt transaction, for example on round 2: `--round 2 --tx BA3GSpSEvX8uEbTkfJFGhke2UiM4kn9Y6ys1ZyexCziS`. I have not run this line on `finalenc2` yet, because the tool is not installed here.

**To see it in a browser,** serve the `site/` folder from any static file server and open `ledger.html`. It reads `finalenc2.testnet` live, with no server in between. `index.html` lets you try to sneak a lie past it.

## Privacy on NEAR

Four pieces keep the ruling private while it is made, and checkable after.

| What | What it does here | How to check it |
|---|---|---|
| Sealed Intel TDX chip on OutLayer | The courtroom program runs inside it, so nobody, including me, can see or change what it reads or how it asks the judges while it runs. | The program's fingerprint on chain matches the build in this repo (checker line "courtroom program matches its fingerprint"). |
| NEAR AI Cloud private inference | All 3 judges run inside their own Intel-sealed chips and sign every reply with a key that Intel vouches for. | `--attested-judges` on the checker fetches a fresh Intel-signed report per model and matches each judge's signature to it. |
| Judge key locked to one program | The NEAR AI key can be read only by this exact program build, only through `finalenc2`. The account that owns the key has 0 keys left, so nobody can change that rule. | The key owner `fkey2.sireadell.testnet` has 0 access keys on chain. The lock run is recorded in `proofs/finalenc2/README.md`. |
| The `privacy/` folder | Judge report checks, a reader that never leaks a data-feed key, a writer whose instructions stay hidden, and a contract for sealed bets that stay unreadable until their batch closes. | Sealed-bet contract: 25 tests. Node tools: 30 tests. The sealed-bet contract is built and tested, not deployed. |

## How it works

1. **A writer puts money on a sentence.** For example: "On 7 November 2026, NEAR mainnet runs protocol version 88." The writer adds one word for how sure they are (*is*, *very likely*, *likely* or *disputed*). The stake is 0.05 NEAR whichever word they pick.
2. **A sealed program checks it.** The contract asks OutLayer to run my courtroom program inside an Intel TDX chip, so nobody can see or change what it does while it runs. The program fetches the evidence itself from two sites run by different owners, both on an approved list of 10. Then three NEAR AI models answer, one after another. The program checks each judge's signature and signs one ruling: true, false, unsure, split or void.
3. **The contract pays out.** It reads the ruling and, once a 36,000 block challenge window (about 12 hours) has closed, anyone can call `finalize` to settle every stake with one fixed public formula (below).

A sentence about a number is decided by the numbers the two sites return, not by the AI. The judges only confirm that the sentence states that number.

The NEAR AI key the program uses is locked to this exact program build and to `finalenc2`. The account that owns the key has 0 keys left, so nobody, including me, can widen who reads it.

## The payout rule

You pick one word. Being right with a sure word earns more, being wrong with it costs more.

| Word you pick | If the sentence is true | If it is false |
|---|---|---|
| is | you gain 24.75% of the stake | you lose 65.25% |
| very likely | you gain 21% | you lose 39% |
| likely | you gain 12.75% | you lose 17.25% |
| disputed | your stake comes back | your stake comes back |

Saying how sure you really are earns the most on average. Claiming more certainty than you have, or hiding behind a vaguer word, earns less. Gains are paid from other writers' losses and a reserve. If a ruling comes back unsure, split or void, everyone gets their stake back in full. The full rule is in `docs/PAYOUT_RULE.md`.

Anyone can challenge a ruling inside the window by staking 0.2 NEAR and adding a source, which starts a re-run. 5% of rounds are picked at random for an audit.

Today a sentence is a bond, not a market: writers stake on their own words. Trading odds on someone else's sentence is not built.

## What runs today

| Part | Where | Proof |
|---|---|---|
| Courtroom program (Rust, wasm32-wasip2) | `programs/court` | 46 tests. Fetches 2 to 3 sources (HTTPS only, 8 s each, 2 MB cap), asks 3 judges with a fixed prompt, checks each signature, stops itself at 75 s. Two clean builds give the same sha256. |
| Contract | `contracts/courtroom`, rules in `contracts/rules` | 18 contract tests, 17 rule tests. Rounds, approved sources from 2 independent groups, one attempt at a time, void after 3 failed runs, challenges, audits, `finalize`. Live as `finalenc2.testnet`. |
| Payout formula | `contracts/payout`, `docs/PAYOUT_RULE.md` | 16 property tests, including exact money conservation over 20,000 random settlements. |
| Checker | `tools/verify` | Node, no packages of its own. Its keccak and signature recovery match the Rust program. `--attested-judges` uses Intel's quote checker in `privacy/tools`. |
| Judges | NEAR AI Cloud | `deepseek-ai/DeepSeek-V4.1-Flash-1017`, `Qwen/Qwen3.8-27B`, `Qwen/Qwen3.6-35B-A3B-FP8`. A full 3-judge ruling took 2.3 to 4.6 s in my timing runs. |
| Accuracy test | `tools/accuracy` | 200 sentences (100 true, 100 false) with truth from Wikidata, run against the 3 real judges on 9 November 2026. The 3-judge ruling was right on 198, wrong on 0, no ruling on 2. Cost: $0.0173 for 600 judge calls. |
| Pages | `content/pages/` | `near-chain` (rounds 2 to 4), `near-tools` (rounds 5 to 10), `lagos-abuja` (rounds 11 to 16), and the planted lies (rounds 20 to 22). |
| Site | `site/` | Try to lie to it, a live ledger of every ruling, and the pages. |

## What this is not

- **Testnet only.** Nothing is deployed on mainnet.
- **The fines on rounds 20 to 22 are not paid yet.** Each of those writers stands to lose 65.25% of 0.05 NEAR. Their challenge windows close at blocks 277,303,311 to 277,303,424. After that anyone can call `finalize`, and the fines move.
- **Finalize has only paid stakes back so far.** 14 rounds (2 and 4 to 16) were finalized, and round 3's stakes came back when it was ruled unsure. That is all 15 page rounds paid. Each writer got 0.05 NEAR back with a bonus of 0, because no wrong writer had paid into the pool yet.
- **A judge that fails counts as unsure.** If a judge gives no signed reply, its answer is not counted. That happened on round 3 (2 of 3 judges). A retry would change the program's fingerprint and break the key lock, so I did not add one.
- **One access key is still on the contract, on purpose.** It stays until I give the go to delete it. Until then I could still redeploy the contract. The checker shows this as its one MISMATCH.
- **Round 1 was never ruled.** It was opened before the writers had money in the contract.
- **No one has challenged a `finalenc2` ruling yet.** A full challenge, re-run and payout ran on an earlier test contract before the real judges were switched on (`proofs/finalenc1`).
- **It does not prove a sentence is true about the world.** It proves which listed sites the evidence came from, that a sealed chip fetched it, and how three models ruled.
- **Three models from one provider are three models, not three independent parties.**
- **Accuracy is measured on simple sentences only** ("The capital of X is Y"). Harder sentences are not measured.
- **It never picks a side when two sources disagree.** The round is void and stakes go back.
- **The pages were written by hand.** No AI writer has published a page.
- **The reproducible build is only compared on Windows.** A Linux check is written (`.github/workflows/reproducible-build.yml`) but has never run.

## Code written before the hackathon, and libraries

NEAR HACKATHON opened on 1 November 2026. The first commit in this repo is from 7 November 2026, and everything here was written after that.

Libraries I did not write:

| Where | Libraries |
|---|---|
| Courtroom program | serde, serde_json, sha2, sha3, k256, ed25519-dalek, x25519-dalek, hkdf, chacha20poly1305, base64, hex, wasi-http-client |
| Contracts | near-sdk 5, base64, hex, sha2, serde_json |
| Sealed bets (`privacy/`) | substrate-bn, getrandom, sha2, sha3, serde, serde_json, wasi-http-client, hex |
| Intel quote checker | @phala/dcap-qvl 0.7.1 (npm) |

Services: NEAR testnet, OutLayer (runs the program in the Intel TDX chip), NEAR AI Cloud (the three judges).

## Which tracks this fits

I could not find NEAR's own prize wording saved anywhere, so these are the track names and the tech each one names.

**Privacy & Private AI** (TEE, NEAR AI Cloud)
- The courtroom program runs inside an Intel TDX chip on OutLayer.
- The judges are NEAR AI Cloud models, and each one signs from inside its own Intel-sealed chip. The checker confirms that against a fresh Intel report.
- The NEAR AI key is locked to one program build and one contract, and its owner has no keys left.

**AI Security & Defense** (TEE attestation, policy engines)
- Anyone with the checker can match every judge signature to a key Intel vouches for.
- Fixed rules decide the outcome, not a person: approved sources only, two independent groups, HTTPS only, numbers decided by the numbers.
- A sentence that hides an order to the judges reaches them as quoted text in a fixed frame.

**Open Innovation** (money, intelligence, ownership)
- Writers lock a stake behind each sentence and a public formula pays or takes it.
- AI judges rule only on evidence the sealed program fetched itself.
- The contract has no method that can replace its code.

## Run the tests

```
cd programs/court;      cargo test --lib
cd contracts/courtroom; cargo test
cd contracts/rules;     cargo test
cd contracts/payout;    cargo test
cd tools/verify;        node --test
```

Run each line from the repo root. On Windows without the Microsoft C++ linker, add `+stable-x86_64-pc-windows-gnu` after `cargo`.

To build the contract, run `cargo near build non-reproducible-wasm --no-abi` in `contracts/courtroom`. To build the courtroom program, run `./build/reproduce.sh`.

## Repository

| Folder | What is in it |
|---|---|
| `programs/court` | The courtroom program that runs in the sealed chip |
| `contracts/courtroom` | The NEAR contract: rounds, sources, bonds, challenges |
| `contracts/rules` | Round rules, tested without a blockchain |
| `contracts/payout` | The payout formula and its tests |
| `tools/verify` | The one-command checker |
| `tools/writer` | Publishes a page: opens rounds and bonds words. `check.mjs` checks a page first |
| `tools/accuracy` | The 200-sentence judge test |
| `privacy` | Judge report checks, and sealed bets that are built and tested but not deployed |
| `content/pages` | The pages |
| `proofs` | Run notes and saved sealed-box proofs (see `docs/SEALED_BOX_PROOF.md`) |
| `docs` | The payout rule, the sealed-box proof, the real-judge checklist |
| `site` | Try to lie, ledger, pages |
| `archive` | Page bytes that past rulings saw |

> Who decided this sentence is true, and can I check that without trusting them?
