# finalenc1.testnet: full appeal ladder run with the stand-in judge

Testnet. The judges here are the stand-in (fake) judges, so these rulings show the machinery, not real AI judgement.

| Step | Transaction |
|---|---|
| Round 2 opened | B1BAb4V9ruAuSHhz5huF1sbWubypVZdZgQFFe2qkAmqN |
| Bonds: w1 "is", w2 "likely", w3 "disputed" | Ak8si3WweXDQZffDyEXGaur2hvtKZCXV5GoeyK48Adep, 6bAsvYB3va5DoCSDvNbAhXng6nJ5eztynvohqmdZ5q8h, 7Wune8XRGegYfZ7i5tD8qjzFTpkUftv73TweSkDktfio |
| Ruling (attempt) | BzCzdvNn1tXbAUyRKBDzh9QAMJXpRV9J22B4Z6UAvn2D |
| Challenge adding a third source (round 3, bond 0.2 NEAR) | 7s7HJH5avDxAf5o2UtHz1DrRpprUmuMWAiNuY1CPAsGw |
| Re-run, upheld | 2p5g4FYJhadXV9GuBAVcSPt5qbRH8XQZP3aKSQqxSxBT |
| Finalize and payouts | E17oW9rJjNvwTvGZRbix9UiAGbN7VXDH639LbZtE1E6J |

Checker: `node tools/verify/verify.mjs --contract finalenc1.testnet --round 2 --archive archive --tx BzCzdvNn1tXbAUyRKBDzh9QAMJXpRV9J22B4Z6UAvn2D --network testnet`. Every check that could run says MATCH (payouts, outcome, request, program fingerprint, sealed-box attestation), except "contract has no keys": the contract still has its key on purpose. The live network pages change every second, so their saved bytes cannot be compared later.
