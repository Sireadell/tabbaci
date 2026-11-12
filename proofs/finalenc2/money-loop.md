# Money loop on finalenc2.testnet (11 November 2026)

| Step | Result |
|---|---|
| Rounds 2 and 4 to 16 finalized | 14 rounds finalized on chain (checked with `get_case`, 11 November 2026). Each writer got the 0.05 NEAR stake back (the bonus is 0 because no wrong writer funded it) |
| Round 3 (planted lie, ruled unsure) | Has no case to finalize, the call panics with "this round has no ruling to finish". Both stakes were returned when it was ruled unsure, so all 15 page rounds have paid out |
| Rounds 20, 21, 22 (planted false protocol versions 92, 85, 89, each bonded "is") | All three ruled F by 3 real judges, every judge signed by an Intel-vouched key. Rulings took 32, 47 and 44 blocks |
| Rounds 17 to 19 | Opened and attempted without bonds by mistake (writers had no ledger balance). They carry no money and prove only the judges |
| Fines on rounds 20 to 22 | Paid on 12 November 2026 after the window closed. Each writer lost 65.25% of 0.05 NEAR. The checker says MATCH on the payout rule for all three |
| Rounds 23 to 31 (12 November 2026) | Nine more planted lies with different tricks, two of them bribes to the judges. All nine ruled F, every ruling signed by 3 Intel-vouched judges (rounds 24 and 25 checked with the checker). Fines are paid after the window closes, around 21:00 UTC on 12 November 2026 |
| Rounds 32 and 33 | True sentence (version 88) ruled T. `sireadell.testnet` challenged it with 0.2 NEAR and a third source (round 33), and the re-run ruled T again, so the challenge bond is lost when the case is finalized |
| Cost | About 11 rulings of 3 judge calls each, a few cents of credit. The first attempt on round 33 used too little gas and returned nothing, so it needed a second attempt |

Open limit: a judge that returns no signed reply counts as unsure. This happened on round 3 (2 of 3 judges) and about 1% of the time in the 200-sentence test. A retry would change the program fingerprint and break the key lock, so it was not added.
