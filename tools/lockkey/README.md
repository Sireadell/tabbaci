# Lock the AI key to our program

`lock-key.ps1` stores the NEAR AI key as an OutLayer secret owned by its own testnet account, readable only by one exact program build, and then deletes every key on that account.

What this gives:
- Only the program whose fingerprint (sha256) equals `-ProgramHash` can read the key. Any other program that names the same secret gets nothing back.
- With `-Via <contract>`, only calls that come through that contract can read it, so nobody can run our program directly and spend our credit.
- The owner account has no keys left, so nobody (including us) can later widen who may read it. The contract already has `secrets_account` and `secrets_profile`; set them to `-Owner` and `-Profile`.

Switch for the real run once credit exists: run `lock-key.ps1` with the final program hash and final contract, deploy the final contract with those two fields, done. If the program changes later, use a new owner account, because the old one can no longer move its lock.

Proven on testnet with a dummy key and `programs/keyprobe` (account `keyprobe.sireadell.testnet`, 0 keys):

| Check | Result |
|---|---|
| Exact build reads the secret | key visible, length 28, fingerprint matches |
| A different build, same secret named | key_visible false |
| Owner keys deleted | 0 keys on chain |
| Owner tries to widen access with no keys | refused (access key not found) |
| Exact build still reads after the keys are gone | yes |

Not proven: the `-Via` rule (it needs the final contract to call through), and the script run end to end in one go (the steps above were run by hand with the same commands; testnet funds were too low for another throwaway account).

`run-and-read.ps1` runs a program on OutLayer testnet and prints the reply from the chain.

## The private court: two secrets in one profile

OutLayer gives one run one secrets profile of one owner account, so the private court's NEAR AI key and the writer's read-only GitHub token must sit in the same secret. `lock-private.ps1` stores both in one go and is run by the person who owns the GitHub token, on their own machine:

```
.\lock-private.ps1 -Owner <new owner account> -ProgramHash <sha256 of the real court-private build> -Via <courtroom-private contract> -Create -Repo <owner/repo>
```

| Step | What happens |
|---|---|
| AI key | read by the script from `C:\Users\DELL\.near-secrets\nearai-api-key.txt`, never shown |
| GitHub token | typed into a hidden prompt, held in memory only, never written to disk |
| `-Repo` | optional: one request to api.github.com checks the token can read that repo |
| Store | one secret `{"NEAR_AI_KEY":..., "PRIVATE_SOURCE_KEY":...}` locked with `--wasm-hash` and `--via` |
| Lock | every key on the owner account deleted, checked at 0 |
| Output | lengths and one `LOCKED ...` line, never a value |

Then deploy `contracts/courtroom-private` with `secrets_account` = `-Owner` and `secrets_profile` = `-Profile` (default `court`).

OutLayer also has `outlayer secrets update` (merges new variables into a stored row). It was not used, because the owner account must keep its key between the two people's turns; one run of `lock-private.ps1` needs no hand-over.
