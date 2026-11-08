# Reproducible build

`./build/reproduce.sh` copies the courtroom program source into a fresh folder, builds it with fixed flags and prints the sha256 of the result. Run it on any machine and compare.

- Compiler used for the recorded result: rustc 1.99.1 (58cf8f9c3 2026-10-13)
- Target: wasm32-wasip2. Flags: --locked (exact dependency versions from Cargo.lock), codegen-units 1, lto, opt-level z, panic abort, strip, no incremental, file paths remapped.
- Result on Windows, two builds from two different folders: identical (`aa5ed46ffc58efc94921daa224f95b88d584599e6a05daf7ee0e427ffc2885f1` for the test build with the stand-in judges).
- Not yet checked: the same build on Linux or macOS. Until it is, treat Windows as the reference build.
- On a machine without the Microsoft linker, set `TOOLCHAIN=+stable-x86_64-pc-windows-gnu`.
- `FEATURES=standin` builds the test version with fake judges. The real courtroom is the build without it.

## Linux check (added, not yet run on Linux)
`.github/workflows/reproducible-build.yml` builds the program and the contract twice on Linux with Rust 1.99.1 and fails if the two fingerprints differ. It is a manual or push-triggered GitHub Actions job; nothing has run it yet, so Linux stays unproven until it has.
- `./build/reproduce-contract.sh` is the contract version of `reproduce.sh` (plain `cargo build --locked` for wasm32-unknown-unknown, symbols stripped). Two local Windows runs: identical (`ceb758ad37675c935f15c80692dba97a1d204c4b6029a9d7c17ed6dccaf1da3d`).
- Found while testing: the contract's function order depends on the name of the build folder, so the script always builds in the same folder name (`enc-repro-build` in the temp directory). Anyone comparing fingerprints should use the script, not their own folder.
- These contract fingerprints are for this script only. The contracts deployed on testnet were built with `cargo near build`, so their code hashes are a different file.
