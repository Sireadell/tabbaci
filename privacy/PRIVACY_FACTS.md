# Privacy facts (working notes for the reader-facing page)

Facts only. Each row says what is built, how a stranger checks it, and what is not proven.
Wording for the public page is written separately.

## What is hidden, from whom, for how long

| Thing | Hidden from | Until | How |
|---|---|---|---|
| Direction, price limit and amount of a bet | everyone, including us | the batch closes | sealed to a lock made inside the locked chip; the chip hands out the key only after the close block |
| A writer's page | everyone | the batch closes | same lock; the page's fingerprint is on chain before the key exists |
| A writer's instructions (sealed on chain) | everyone | the batch closes | same lock, own record |
| A writer's instructions (kept in the chip) | everyone | never, unless the owner shows them | stored as a secret only this program build can read; only fingerprints leave |
| The AI key and any data-feed key | everyone | never | secret locked to one program build and one contract; the owner account has no keys |
| Licensed evidence text | everyone | never | read inside the chip; only status, size, fingerprint and one short value leave |
| What a bet owner shows an auditor | everyone else | owner's choice | a viewing proof for one sealed order or page |

## What is not hidden

Who bets, when, and the size of the deposit. Every order once its batch has cleared. Who funded which writer. Verdicts, judge answers and signatures. Public evidence pages. Who asked for a ruling.

## Checks a stranger can run (Node only, no keys, no logins)

| Claim | Command | Honest failure output |
|---|---|---|
| Nobody can widen who opens the secrets, contract cannot change itself | `node privacy/tools/verify-privacy.mjs --contract C --secret-owner O` | MISMATCH with the number of keys left |
| A batch was opened with the right key, every order opens to what was recorded, and clearing is exact | `node privacy/tools/batch-check.mjs --contract C --batch N --key K` | MISMATCH naming the line, or CANNOT VERIFY if the batch is not opened |
| A judge model's report is fresh and tied to the signer | `node privacy/tools/verify-privacy.mjs ... --report file --nonce hex` | MISMATCH with the failed rule |
| A secret marker never reached public data | `node privacy/tools/verify-privacy.mjs ... --canary M --txs h1,h2` | MISMATCH naming the transaction |
| Writer instructions stayed the same over time | `brainContinuity` in `privacy/tools/batch-check.mjs` over the sealed records | prints the first batch where the fingerprint changed |

## Built and tested (counts are from the last run)

| Keeper | Built | Tests |
|---|---|---|
| K1 keyed evidence | reader that sends the key only over https to a named host, scrubs the key in six encodings, and publishes only status, size, fingerprint, one value | 8 Rust tests |
| K2 attested judges | five report checks, signer matching, signature check, end-to-end encryption, contract-side signature check | 12 report and signature tests plus 3 encryption tests in the court program, 3 in the signature crate, 4 Node tests |
| K3 sealed batches | contract with one-price clearing, refunds, void on withheld key, double-answer guard | 25 Rust tests in the contract (10 on clearing), 2 Node tests against 200 shared vectors |
| K4 sealed forecasts | pages sealed to the batch lock, opened by anyone after close | covered in K3 contract tests and the Node batch checker |
| K5 hidden writer | program that prints only page and fingerprints; blocks a page that repeats the instructions | 7 Rust tests |
| K6 viewing proof | earlier work, kept | 7 Rust tests in the sealing library |
| K7 one check command | extended with batch, report and marker lines | 10 Node tests for the batch checker |

## Not proven

| Item | Why |
|---|---|
| Intel's signature chain on a judge report | needs an extra tool that was not installed; the command prints CANNOT VERIFY |
| A real judge answering with end-to-end encryption, and the signature coming back | no model credit |
| The certificate check against fake HTTPS hosts inside the chip | needs a run on the chip; not done |
| The new batch contract and writer program on the real testnet | built and unit tested only; not deployed |
| Gas for the chip request versus the callback at 100 orders | measured in the unit-test host only (about 155 Tgas for opening and clearing 100 orders) |
| Physical attacks on the chip hardware | out of scope for the design; a second lock was not built |
