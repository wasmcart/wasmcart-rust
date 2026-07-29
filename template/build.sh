#!/usr/bin/env bash
# Build this cart and pack it into a .wasc.
set -euo pipefail
cd "$(dirname "$0")"

NAME=my_cart   # cdylib output name: the package name with - turned into _

cargo build --release --target wasm32-unknown-unknown
npx wasmcart pack \
  --wasm "target/wasm32-unknown-unknown/release/${NAME}.wasm" \
  --name "${NAME}" \
  -o "${NAME}.wasc"

echo
echo "run it:  npx wasmcart ${NAME}.wasc"
