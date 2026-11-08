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
