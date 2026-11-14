# Deploying the private court on NEAR testnet

Windows PowerShell 5.1. Tools in `C:\Users\DELL\near-tools\bin`. Nothing here holds a key or secret.
Run the near and outlayer commands through `Start-Process -Wait -NoNewWindow -RedirectStandardOutput/-RedirectStandardError` so native stderr lines do not raise errors.

| Item | Value |
|---|---|
| Contract account | `court-private.sireadell.testnet` (funder and DAO: `sireadell.testnet`) |
| Program | `build/reproduce-private.sh`, sha256 `d2d4d644f854bc7fe22047c43b6a6f8d9f757d731fd7b717999642aac83f9c88`, 544575 bytes |
| Program URL | `https://test.fastfs.io/sireadell.testnet/outlayer.testnet/d2d4d644f854bc7fe22047c43b6a6f8d9f757d731fd7b717999642aac83f9c88.wasm` |
| Secret owner and profile | `pkey.sireadell.testnet`, `court` (the founder creates the owner and locks the secret) |
| Init arguments | `tools/private-deploy/init-args.json` |

Judge models, signing keys, stake (0.05 NEAR), first challenge bond (0.2 NEAR), window (36000 blocks), run deadline (72000), pending timeout (300) and audit rate (5%) are the live court-v2 values: models and signers read from `tabbaci-court2.testnet` (`signers`, `models`), ladder values from the `finalenc2.testnet` deploy transaction.

## 1. Upload the program
```
outlayer.exe upload court-private.wasm
```
Then fetch the URL above and check the sha256 matches.

## 2. Build the contract
```
cd contracts\courtroom-private
$env:RUSTUP_TOOLCHAIN='stable-x86_64-pc-windows-gnu'
cargo-near.exe near build non-reproducible-wasm --locked --no-abi
```
`--no-abi` is needed on the GNU toolchain (the ABI step links a native binary and fails). Output: `target\near\courtroom_private.wasm`.

## 3. Create the account
```
near.exe account create-account fund-myself court-private.sireadell.testnet 1.5NEAR autogenerate-new-keypair save-to-legacy-keychain sign-as sireadell.testnet network-config testnet sign-with-legacy-keychain send
```
Storing the 336 KB contract costs 0.00001 NEAR per byte, so the account needs about 3.5 NEAR in total (1.5 and 2.1 were both not enough; the error message shows the shortfall, not the total).

## 4. Deploy with the init call
```
near.exe contract deploy court-private.sireadell.testnet use-file contracts\courtroom-private\target\near\courtroom_private.wasm with-init-call new file-args tools\private-deploy\init-args.json prepaid-gas 100Tgas attached-deposit 0NEAR network-config testnet sign-with-legacy-keychain send
```
Check: `near.exe contract call-function as-read-only court-private.sireadell.testnet courtroom json-args {} network-config testnet now` (also `models`, `signers`, `stake`, `dao`).

## 5. Add the private source (DAO only)
```
near.exe contract call-function as-transaction court-private.sireadell.testnet add_private_source json-args '{"prefix":"https://api.github.com/repos/Sireadell/paycheck-sui-frontend/","group":"own-github","key_env":"PRIVATE_SOURCE_KEY"}' prepaid-gas 30Tgas attached-deposit 0NEAR sign-as sireadell.testnet network-config testnet sign-with-legacy-keychain send
```
The first private source gets id 1. `key_env` must be `PRIVATE_` plus capitals, digits or `_`, and the prefix must be a path on api.github.com.

## 6. Rounds (any account; the writer account deposits first)
Money: `deposit` (payable) puts balance in the contract, `bond` takes the 0.05 NEAR stake from it.
```
deposit        {}                                       attach 1 NEAR
open_round     {"claim":"...","kind":"private","cv":"20","picks":[{"id":1,"url":"https://api.github.com/repos/Sireadell/paycheck-sui-frontend/<path>","x":"len:$","tol":0}],"terms":[],"tag":"p1","op":"gt"}
bond           {"round":<id>,"word":"is"}               word: is, very likely, likely or disputed. Only before the first attempt
attempt        {"round":<id>}                           payable, attach 0.02 NEAR, 300 Tgas. Starts the sealed run
get_round      {"round":<id>}                           view: status, attempts, outcome
finalize       {"round":<id>}                           only after the 36000 block window closes; settles the bonds
```
`x` is `json:$.path` or `len:$`; `op` is gt, ge, lt, le or eq; a private round has exactly one source and no excerpt terms. If a run comes back empty, call `attempt` again (up to 3). Challenges use `challenge {"round":<id>,"picks":[...]}` with the same private source. See `tools/v2/rounds.mjs` for a scripted public-round version and `tools/verify/private.mjs` for the checker.

## 7. Keys
Do not delete the contract keys until the rounds are run. Then use `tools\final\delete-keys.ps1 -Contract court-private.sireadell.testnet` (dry run) and `-Yes`.
