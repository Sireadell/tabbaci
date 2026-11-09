# finalenc2.testnet: final contract with the real courtroom program

| Item | Value |
|---|---|
| Program | real judges, sha256 2c8e24a5fa6d2d2b60431207eeabddb3521b286936c78a82ca3bf99878e01f9c (two clean builds match) |
| Deploy | CZvqA9KkDoLbGGPPEqDE2uUECpz1MjXoPdiM8x4Rvhyk |
| Judge key owner | fkey2.sireadell.testnet, profile judge, readable only via finalenc2 and only by this program. The owner has 0 keys left, so nobody can change that rule |
| Settings | stake 0.05 NEAR, first challenge bond 0.2 NEAR, challenge window 36000 blocks (about 12 h), audit rate 5% |
| Sources | the same 10 as finalenc1, same ids |

## First page with real judges: near-chain (rounds 2 to 4)

| Round | Ruling transaction |
|---|---|
| 2 | BA3GSpSEvX8uEbTkfJFGhke2UiM4kn9Y6ys1ZyexCziS (program ran in 4.6 s, ruled T) |
| 3 | CwsYcMTGqbABfpWxy4tHSJ3wuqLUgmkb2HP656dwSHUu |
| 4 | 8BK5mbesQSJ1ML3LfgaAqiEecGTuBsxySTrKvoh6EYN1 |

Round 1 was opened without a bond while the writers were still being funded, and has no ruling.

Checker: `node tools/verify/verify.mjs --contract finalenc2.testnet --round 2 --archive archive --tx BA3GSpSEvX8uEbTkfJFGhke2UiM4kn9Y6ys1ZyexCziS --network testnet --attested-judges`. All 3 judges MATCH: each reply is signed by a key that Intel vouches for. Program fingerprint, request, sentence and outcome also MATCH. The one MISMATCH is "contract has no keys": the key stays until the founder says go.
