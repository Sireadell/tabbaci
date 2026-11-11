# Checker run on all 15 rounds (finalenc2.testnet, rounds 2 to 16)

Command, once per round: `node tools/verify/verify.mjs --contract finalenc2.testnet --round N --archive archive --network testnet --attested-judges`

| Result | Rounds |
|---|---|
| Only mismatch is "contract has no keys" (1 key kept on purpose until the owner says go) | all 15 |
| Judge signatures: every judge signed by a key Intel vouches for | all 15 |
| Verdict, request, sentence, attempt list, contract outcome | MATCH in all 15 |
| Not checkable from the chain alone: bond payout rule, judge reply words, sealed-box attestation (outlayer-verify not installed), source bytes of 5 prose rounds | listed as CANNOT VERIFY, never as MATCH |

Time from sending the ruling call to the settled round: min 15 blocks, typical 17, max 46 (about one second per block). Every round needed exactly 1 attempt, none failed.
Outcomes: rounds 2, 4 to 16 true, round 3 (the deliberately false sentence) unsure (two of the three judges returned no signed reply, so the numbers could not be confirmed).
