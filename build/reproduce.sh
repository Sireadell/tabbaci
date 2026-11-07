#!/bin/bash
# Rebuild the courtroom program from a clean copy of the source and print its fingerprint.
# Same source, same compiler, same flags give the same file on every machine.
set -e
SRC="$(cd "$(dirname "$0")/.." && pwd)/programs/court"
WORK="$(mktemp -d)"
cp -r "$SRC" "$WORK/court"
rm -rf "$WORK/court/target"
cd "$WORK/court"
export RUSTFLAGS="--remap-path-prefix=$WORK/court=/src --remap-path-prefix=$HOME/.cargo=/cargo --remap-path-prefix=$HOME/.rustup=/rustup"
export CARGO_INCREMENTAL=0
${CARGO:-cargo} ${TOOLCHAIN:+$TOOLCHAIN} build --locked --target wasm32-wasip2 --release ${FEATURES:+--features $FEATURES} >/dev/null 2>&1
sha256sum target/wasm32-wasip2/release/court.wasm
rm -rf "$WORK"
