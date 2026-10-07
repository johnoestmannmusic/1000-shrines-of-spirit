#!/bin/sh
# Builds the WASM module and places it next to the web player.
set -e
cd "$(dirname "$0")"
cargo build --release -p shrine-0011-engine --lib --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/shrine0011.wasm web/track.wasm
echo "web/track.wasm: $(wc -c < web/track.wasm) bytes"
