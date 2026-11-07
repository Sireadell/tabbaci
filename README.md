# [Name to be decided]

[Name] is an encyclopedia where every checkable sentence has money behind it, and a sealed courtroom on NEAR decides whether it was true.

[Founder: one sentence, first person, on why you built it.]

> Who decided this sentence is true, and can I check that without trusting them?

## How it works

1. **A writer puts money on a sentence.** For example: "On 7 November 2026, NEAR mainnet runs protocol version 88." The writer adds one word saying how sure they are (*is*, *very likely*, *likely* or *disputed*) and locks the same fixed stake behind it, whatever word they pick.
2. **A sealed program checks it.** The program runs inside an Intel TDX chip on OutLayer, so nobody can see or change what it does. It fetches the evidence itself from two independent sites on an approved list, then asks three AI models. It signs a ruling (true, false, unsure, split, or void) in under 900 bytes.
3. **A NEAR contract pays out.** The contract reads the ruling and settles every stake with one fixed public formula. Nobody, including us, can change the contract. It has no upgrade method, and the final deployment has no keys.

Anyone can then re-check the whole ruling with one command.

## Check it yourself in one minute

You need Node 18 or newer. There is nothing to install.

```
node tools/verify/verify.mjs --contract court5.sireadell.testnet --round 6 --archive archive
```

Each line prints MATCH, MISMATCH or CANNOT VERIFY. Today you should see:

| Line | Today | Why |
|---|---|---|
| contract has no keys | MISMATCH | This test contract still holds one key so it can be redeployed. The final contract has its key deleted. |
| contract has no upgrade method | MATCH | 21 exported methods. None can replace the code, the keys or an owner. |
| courtroom program matches its fingerprint | MATCH | The file at the contract's URL hashes to the sha256 the contract holds. |
| stored request, verdict, attempt list | MATCH | The ruling belongs to this round and these exact inputs. Every attempt is listed. |
| bond payouts follow the rule | MATCH | Every payout is recomputed from the formula. |
| source bytes match their fingerprint | MATCH | The archived Wikipedia and GitHub pages hash to what the box saw. |
| judge signatures | CANNOT VERIFY | The signatures are valid. The judges on testnet are stand-ins signed by a test key, not the models' own keys (see limits). |
| judge reply words match the answers | CANNOT VERIFY | Only each reply's fingerprint is published, not its words. |
| sealed-box attestation | CANNOT VERIFY | Needs OutLayer's `outlayer-verify` tool and the attempt transaction hash. |

Exit code 0 means everything matched, 1 means something did not match, and 2 means nothing mismatched but something could not be checked.

You can also see the rulings in a browser. Open `site/ledger.html` from any static file server. It reads the contract live, with no backend.

## What runs today (NEAR testnet)

| Part | Where | Proof |
|---|---|---|
| Courtroom program (Rust, wasm32-wasip2) | `programs/court` | 22 tests. It fetches 2 to 3 sources (HTTPS only, 8 s each, 2 MB cap). It asks 3 judges with a fixed prompt, checks each judge's signature, and stops itself at 75 s. |
| Ruling size | same | 733 to 742 bytes with 3 judges and 2 sources. OutLayer's limit is 900. |
| Time from request to payout callback | testnet runs | Usually 11 blocks, once 67. NEAR waits about 200 blocks. |
| Contract | `contracts/courtroom`, rules in `contracts/rules` | Each round stores its request once, and every retry sends the same bytes. One attempt runs at a time and every attempt is published. A round is void after 3 failed runs. Sources must come from 2 independent groups on a list only the DAO account can change, and a removal takes 72 hours. 9 rule tests. |
| Payout formula | `contracts/payout`, `docs/PAYOUT_RULE.md` | 15 property tests. On chain: a right *is* earns +24.75%, a right *likely* +12.75%, *disputed* gets the stake back, a wrong *is* loses 65.25%. |
| Checker | `tools/verify` | Node, no packages. Its keccak and signature recovery match the Rust program. Recomputes payouts and outcomes. |
| Reproducible build | `build/reproduce.sh` | Two clean builds in different folders give the same sha256 (on Windows). |
| Site | `site/` | "Try to lie to it", a live ledger of every ruling and every lie tried, and pages. |
| Seed page | `content/pages/near-chain.json`, rounds 4 to 6 | A true number (two RPC providers both say 88), a false number (87), and a prose sentence checked against Wikipedia and GitHub. All three settled. |
| Accuracy test | `tools/accuracy` | 200 sentences (100 true, 100 false) with truth from Wikidata. Ready to run against real judges. |

## What it does not do

- It does not prove a sentence is true about the world. It proves that the evidence came from listed sites through a sealed chip, at a recorded time, and that three models ruled on that evidence.
- Three models served by one provider are three models, not three independent parties.
- Number sentences are decided by the numbers, not by AI. The judges only confirm that the sentence states that number. Free text reaches the judges inside a fixed frame marked as untrusted data, never as instructions.
- If two sources disagree, the round is void. It never picks a side.
- If someone in the middle misbehaves, the worst they can force is a void round. They can never flip a ruling, and every failed attempt is published.

## Limits today, stated plainly

| Limit | State |
|---|---|
| Real AI judges | Not running yet. The NEAR AI calls are written from the published docs but have not been run, because the account has no credit. Every ruling so far uses stand-in judges signed by a fixed test key, and the site says so. |
| Accuracy | Not measured. The 200-sentence test only proves the plumbing until it runs against real judges. |
| Judge models | `deepseek-ai/DeepSeek-V4.1-Flash-1017`, `Qwen/Qwen3.8-27B` and `Qwen/Qwen3.6-35B-A3B-FP8`. It is not yet confirmed that each one signs inside its own sealed box. |
| Per-run Intel attestation | The checker calls OutLayer's `outlayer-verify` when it is installed. It has not been installed or run here yet. |
| Judge reply words | Only each reply's fingerprint is published, so the checker cannot confirm that the reply text equals the recorded answer. |
| Pages that change every second | An RPC status page cannot be fetched again byte for byte. The ruling keeps the extracted number and the fingerprint. |
| Build on Linux or macOS | Not yet compared with the Windows build. |
| Challenges, random audits, appeals | Designed, not built. |
| Writers | The seed page was written by hand. AI writers plug in at `tools/writer`. |
| Mainnet | Nothing is deployed on mainnet yet. |

## Run the tests

```
cd contracts/rules   && cargo test
cd contracts/payout  && cargo test
cd programs/court    && cargo test --lib
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
| `tools/writer` | Publishes a page: opens rounds and bonds words |
| `tools/accuracy` | The 200-sentence judge test |
| `site` | Try to lie, ledger, pages |
| `archive` | Page bytes that past rulings saw |

> Who decided this sentence is true, and can I check that without trusting them?
