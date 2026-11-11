# Payout rule for bonded sentences

A writer puts the same fixed stake B on every sentence and picks one word. Each word is a chance that the sentence is true.

| Word | Chance | If the sentence is true | If the sentence is false |
|---|---|---|---|
| is | 95% | +24.75% of B | -65.25% of B |
| very likely | 80% | +21% | -39% |
| likely | 65% | +12.75% | -17.25% |
| disputed | 50% | 0 | 0 |

The gain or loss is `B x (1/4 - (outcome - chance)^2)`, where outcome is 1 for true and 0 for false. That is the stake times the Brier score of the word minus the Brier score of "disputed".

What this gives:

- Stating your honest belief earns the most on average. Writing a surer word than you believe, or a vaguer one, earns less.
- "Disputed" always returns exactly the stake, so hiding earns nothing.
- There is no jump between neighbouring chances, so no word is a sweet spot to aim for.
- The stake is the same for every word, so a bolder word does not cost more to write.
- Gains are paid from the losses of other writers and a reserve fed by failed challenge bonds and funders. If the pool is short, all gains in that settlement are scaled down together.
- Rulings of unsure, disputed by the judges, or void return every stake in full.
- Money is whole numbers. Gains round down and losses round up, so the contract never pays out more than it holds, and no unit is created or lost.
- Which sentences are audited is drawn from a random number, the sentence and the round only. The word is not an input.

Tests: `contracts/payout/tests/properties.rs` (16 tests, including a fine-grid honesty check, no-jump check at every basis point, exact money conservation over 20,000 random settlements, and 1,000 fixed vectors in `tests/vectors.json`).
