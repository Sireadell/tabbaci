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
- `./build/reproduce-contract.sh` is the contract version of `reproduce.sh` (plain `cargo build --locked` for wasm32-unknown-unknown with the near cfg, symbols stripped). Two local Windows runs: identical (`53c5bd127b4d42f85217ea82dd19d2fb53ea2ce4725bab3bcf7ea43484938615`, testnet build, before wasm-opt).
- Found while testing: the contract's function order depends on the name of the build folder, so the script always builds in the same folder name (`enc-repro-build` in the temp directory). Anyone comparing fingerprints should use the script, not their own folder.
- These contract fingerprints are for this script only. The contracts deployed on testnet were built with `cargo near build`, so their code hashes are a different file.

## Private court (program version 3)
- `./build/reproduce-private.sh` and `./build/reproduce-contract-private.sh` are the same scripts for `programs/court-private` and `contracts/courtroom-private`. Two local Windows runs each, rustc 1.99.1, identical: program `d2d4d644f854bc7fe22047c43b6a6f8d9f757d731fd7b717999642aac83f9c88` (544575 bytes, real judges), stand-in build `317d527821e0d81a056f571d1439370cd79fdd49d1c7f0d1bbc1b78a52c0c62d`, contract `ca2b17b5df920381c0ab57d7b11799b465719b0088daab7e97f826a784f3318d`.
