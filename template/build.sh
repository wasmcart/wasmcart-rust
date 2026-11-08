#!/usr/bin/env bash
# Build this cart and pack it into a .wasc.
set -euo pipefail
cd "$(dirname "$0")"

NAME=my_cart   # cdylib output name: the package name with - turned into _

# The cart's version is the crate's: bump `version` in Cargo.toml to release.
VERSION="$(cargo metadata --no-deps --format-version 1 |
  node -e 'let s="";process.stdin.on("data",(d)=>s+=d).on("end",()=>process.stdout.write(JSON.parse(s).packages[0].version))')"

cargo build --release --target wasm32-unknown-unknown
npx wasmcart pack \
  --wasm "target/wasm32-unknown-unknown/release/${NAME}.wasm" \
  --name "${NAME}" --version "${VERSION}" \
  -o "${NAME}.wasc"

echo
echo "run it:  npx wasmcart ${NAME}.wasc"
