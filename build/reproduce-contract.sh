#!/bin/bash
# Rebuild the courtroom contract from a clean copy of the source and print its fingerprint.
# Plain cargo build for wasm32-unknown-unknown with fixed flags, so it runs the same on any machine with Rust.
set -e
export MSYS_NO_PATHCONV=1 MSYS2_ARG_CONV_EXCL="*"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="${TMPDIR:-/tmp}/enc-repro-build"
rm -rf "$WORK"; mkdir -p "$WORK"
cp -r "$ROOT/contracts" "$WORK/contracts"
rm -rf "$WORK"/contracts/*/target
cd "$WORK/contracts/courtroom"
# On Windows Git Bash the compiler sees C:/ style paths, so remap those spellings too.
w() { if (cd "$1" && pwd -W) >/dev/null 2>&1; then (cd "$1" && pwd -W | tr '/' '\\'); else echo "$1"; fi; }
WORKW="$(w "$WORK")"
CARGOH="${CARGO_HOME:-$HOME/.cargo}"
RUSTUPH="${RUSTUP_HOME:-$HOME/.rustup}"
export RUSTFLAGS="--cfg near -C strip=symbols --remap-path-prefix=$WORK=/src --remap-path-prefix=$WORKW=/src --remap-path-prefix=$CARGOH=/cargo --remap-path-prefix=$(w "$CARGOH")=/cargo --remap-path-prefix=$RUSTUPH=/rustup --remap-path-prefix=$(w "$RUSTUPH")=/rustup"
export CARGO_INCREMENTAL=0
${CARGO:-cargo} ${TOOLCHAIN:+$TOOLCHAIN} build --locked --target wasm32-unknown-unknown --release >"${LOG:-/dev/null}" 2>&1
[ -n "$OUT" ] && cp target/wasm32-unknown-unknown/release/courtroom.wasm "$OUT"
sha256sum target/wasm32-unknown-unknown/release/courtroom.wasm
rm -rf "$WORK"
