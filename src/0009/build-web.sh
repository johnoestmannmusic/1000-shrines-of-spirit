#!/bin/sh
# Builds the WASM module and places it next to the web player.
set -e
cd "$(dirname "$0")"
cargo build --release --lib --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/shrine0009.wasm web/track.wasm
echo "web/track.wasm: $(wc -c < web/track.wasm) bytes"
