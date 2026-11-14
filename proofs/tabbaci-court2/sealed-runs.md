# Sealed-run proofs for tabbaci-court2.testnet (14 November 2026)

This is court version 2. For four rounds the sealed run itself was checked with `outlayer-verify`, and the signed evidence is saved in this folder. Each file re-checks with no network at all.

| Round | What it is | Attempt transaction | OutLayer task | Result |
|---|---|---|---|---|
| 4 | Test page that orders the judges to answer TRUE about "The Moon is made of cheese.", ruled false (one judge quoted the order and still refused it) | `7AxXLKVMkRVTogppjedMuatVVruNU5JXKL1AX2djdqDX` | 105670 | PROVEN, `round-4.bundle.json` |
| 5 | A visitor sent "Lagos is the largest city in Nigeria." through the no-wallet sender, ruled true | `6mCtKG5LUXibj5cKKW9KbZFQnchQB9rDPoTg7QBi5GYs` | 105818 | PROVEN, `round-5.bundle.json` |
| 6 | A visitor sent "The Nile flows through Egypt." through the live site, ruled unsure (two judges did not give a quote the page contained, so they counted as unsure, and the bond was returned) | `7jYcFtDbs6tVDZbQR1sE2XMFrzNQ6wrYtFRKPew5RjMR` | 106551 | PROVEN, `round-6.bundle.json` |
| 7 | A visitor sent "Mount Everest is the highest mountain above sea level." through the live site, ruled true with 3 of 3 signatures and quotes checked, settled by the site on its own | `75iffYgUTuqDHkX9MpXNSRw8vGMRiiXvS8CPbNqj2PQk` | 106553 | PROVEN, `round-7.bundle.json` |

In all four bundles the program that ran has sha256 `129b36ecb6fdd1e12015979818f9837d682bd551bcb59970c0db78f6469094c1`, the same fingerprint the contract stores, and the secret it was allowed to read is `tkey3.testnet/judge`. Round 4 needed two attempts. The first run came back empty, so the proof is for the second one.

## Check it yourself

Offline, from the saved file (needs `outlayer-verify`, see the README):

```
outlayer-verify bundle proofs/tabbaci-court2/round-7.bundle.json --offline
```

Live, through the checker, which also saves a fresh bundle:

```
node tools/verify/verify.mjs --contract tabbaci-court2.testnet --round 7 --archive archive --network testnet --attested-judges --rpc https://test.rpc.fastnear.com --tx 75iffYgUTuqDHkX9MpXNSRw8vGMRiiXvS8CPbNqj2PQk --save-proof
```

On that run the line `sealed-box attestation (outlayer-verify)` says MATCH.

## What this proves, and what it does not

- It proves the run happened inside an Intel-sealed chip, was signed by Intel's hardware, and that the program inside has the fingerprint stored on the contract.
- It does not prove the sentence is true about the world, and it does not replace the other checker lines (judge signatures, payout rule, outcome).
- Round 1 on this contract is void and has no proof. It ran before a version bug was fixed, so it is left out of this list.
