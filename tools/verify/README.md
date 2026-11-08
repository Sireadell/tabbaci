# Checker

One command that checks a ruling without trusting the people who made it.

```
node verify.mjs --contract <account> --round <id> [--network testnet|mainnet] [--archive <folder>] [--tx <attempt transaction hash>] [--signer "<model>=0x<address>"] [--vectors ../../contracts/payout/tests/vectors.json]
```

Needs only Node 18 or newer. No packages to install. Each line says MATCH, MISMATCH or CANNOT VERIFY.
Exit code 0: everything matched. 1: something did not match. 2: nothing mismatched but something could not be checked.

What it checks: the contract has no keys and no method that can replace its code, the courtroom program matches the fingerprint the contract holds, the stored request matches its hash, the verdict belongs to the round, the attempt list is consistent, archived page bytes match their fingerprints, extracted numbers and the outcome follow from the sources and the judges' answers, every judge signature recovers to a signer (and to the expected model key when you pass `--signer`), the payout maths, and, when `outlayer-verify` is installed and you pass `--tx`, the sealed box attestation.

What it cannot check today: that a judge's reply words equal its recorded answer (only the reply fingerprint is published).

Saved sealed-box proofs: the proof for each round is kept in the repo at `proofs/<contract name>/round-<id>.bundle.json` (round 6 on court5 is included). The checker picks it up on its own and re-checks it with no network and no OutLayer service, so a judge can verify the sealed-box run offline. To save a new one: add `--tx <attempt transaction hash> --save-proof`. To check a bundle by hand: `outlayer-verify bundle <file> --offline`.

Tests: `node --test`.

