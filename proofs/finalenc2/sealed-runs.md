# Sealed-run proofs for finalenc2.testnet (12 November 2026)

For three rounds the sealed run itself was checked with `outlayer-verify`, and the signed evidence is saved in this folder. Each file re-checks with no network at all.

| Round | What it is | Attempt transaction | OutLayer task | Result |
|---|---|---|---|---|
| 20 | Planted lie, protocol version 92, ruled false | `gwYoRaCAJan5xLzX6bXUKZ83TSAmSZt8JZj4WEz27fj` | 64242 | PROVEN, `round-20.bundle.json` |
| 25 | Moon bribe round (the sentence carried an order to answer TRUE), ruled false | `92jLEthC7ctF62yon8gL3DiSxkh3Anyk43p7FY3uW2ZZ` | 72832 | PROVEN, `round-25.bundle.json` |
| 33 | My 0.2 NEAR challenge of round 32, ruled true again | `Db3MShVkpnZqefBqvwUQay6rSZVeb5zqDPNkHNfoEKkm` | 72986 | PROVEN, `round-33.bundle.json` |

In all three bundles the program that ran has sha256 `2c8e24a5fa6d2d2b60431207eeabddb3521b286936c78a82ca3bf99878e01f9c`, the same fingerprint the contract stores, and the secret it was allowed to read is `fkey2.sireadell.testnet/judge`. Round 33 needed two attempts. The first used too little gas and returned nothing, so the proof is for the second one.

## Check it yourself

Offline, from the saved file (needs `outlayer-verify`, see the README):

```
outlayer-verify bundle proofs/finalenc2/round-25.bundle.json --offline
```

Live, through the checker, which also saves a fresh bundle:

```
node tools/verify/verify.mjs --contract finalenc2.testnet --round 25 --archive archive --network testnet --attested-judges --rpc https://test.rpc.fastnear.com --tx 92jLEthC7ctF62yon8gL3DiSxkh3Anyk43p7FY3uW2ZZ --save-proof
```

On that run the line `sealed-box attestation (outlayer-verify)` says MATCH.

## What this proves, and what it does not

- It proves the run happened inside an Intel-sealed chip, was signed by Intel's hardware, and that the program inside has the fingerprint stored on the contract. Intel's data is read from the chain, not from OutLayer's server.
- It does not prove the sentence is true about the world, and it does not replace the other checker lines (judge signatures, payout rule, outcome).
- On the first attempt for round 33 the OutLayer web service gave an error once, and the same command worked on the next try. The older `testnet-api.outlayer.fastnear.com` address now answers "this API address is retired" and the live address is `testnet-api.outlayer.ai`.
