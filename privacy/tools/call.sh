#!/bin/bash
# usage: call.sh <method> '<json>' [deposit NEAR] [signer]   (calls privacy.sireadell.testnet)
export PATH="$PATH:/c/Users/DELL/near-tools/bin"
m=$1; a=${2:-{\}}; dep=${3:-0}; signer=${4:-privacy.sireadell.testnet}
near contract call-function as-transaction privacy.sireadell.testnet $m json-args "$a" prepaid-gas '300 Tgas' attached-deposit "$dep NEAR" sign-as $signer network-config testnet sign-with-legacy-keychain send 2>&1 | sed 's/\x1b\[[0-9;]*m//g' | grep -E "EVENT_JSON:\{\"standard\":\"sealbox|panicked|ExecutionError|has failed|rror:|Transaction ID|Gas burnt|return value" | cut -c1-400
