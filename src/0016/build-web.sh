#!/bin/sh
# Builds the artifact WASM and places it next to the web player.
set -e
cd "$(dirname "$0")"
cargo build --release -p shrine-0016-engine --lib --target wasm32-unknown-unknown
mkdir -p web
cp target/wasm32-unknown-unknown/release/shrine0016.wasm web/shrine.wasm
echo "web/shrine.wasm: $(wc -c < web/shrine.wasm) bytes"
