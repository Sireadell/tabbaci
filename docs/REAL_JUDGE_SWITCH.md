# Switching to the real AI judges, then deleting the keys

Do these in order. Nothing here has been run yet. Each step has a check so you can see it worked before moving on.

## Before you start
- [ ] NEAR AI account has credit, and an API key saved in a file (never paste it into a chat or a commit).
- [ ] Confirm the three judge models are real and run in a secure enclave: call the NEAR AI model list and check each model name the contract holds (`models` view on the contract). If a name is wrong, fix it now: the contract cannot change it later.
- [ ] The five top-rung models are chosen too (`top_models`).

## 1. Build the real courtroom program
- [ ] `FEATURES= OUT=real-court.wasm ./build/reproduce.sh` (no `standin`). Write down the sha256 it prints.
- [ ] Build it a second time from another folder. The two sha256 values must be identical.
- [ ] `outlayer upload real-court.wasm` and note the file address it prints.

## 2. Lock the key to that exact build
- [ ] New owner account (the lock cannot be moved after keys are deleted), for example `key2.sireadell.testnet`.
- [ ] `tools\lockkey\lock-key.ps1 -Owner key2.sireadell.testnet -ProgramHash <sha256> -KeyFile <key file> -Via <final contract> -Create`
- [ ] It must end with `LOCKED ... keys_left=0`.

## 3. Deploy the final contract
- [ ] New contract account, deployed with: the file address and sha256 from step 1, the 3 and 5 model names, `secrets_account` = the owner from step 2, `secrets_profile` = `judge`, the real stake and challenge bond sizes, and window lengths you actually want (hours, not the short test values).
- [ ] The DAO approves the same source list.

## 4. One real round
- [ ] Open a round, bond, run the attempt.
- [ ] The ruling must show 3 judge signatures. Run `node tools/verify/verify.mjs --contract <final> --round <id> --archive archive --tx <attempt tx> --network <net>`. Every line should be MATCH.
- [ ] If the judges time out or refuse, stop. Do not delete keys.

## 5. Test the money on a small scale
- [ ] One bonded sentence, finalize, check the payout line says MATCH.
- [ ] One challenge that adds a source, re-run, finalize.

## 6. Delete the keys (last, cannot be undone)
- [ ] `tools\final\delete-keys.ps1 -Contract <final>` first (dry run lists the keys).
- [ ] `tools\final\delete-keys.ps1 -Contract <final> -Yes`
- [ ] Run the checker again: the line "contract has no keys" must say MATCH.
- [ ] Save the proof bundle under `proofs/<final>/`.

## If something is wrong after step 6
Nothing can be changed. Deploy a new contract and start again from step 3 (keep the same locked key only if the program build is unchanged and the new contract is named in `-Via`; otherwise use a new key owner).
