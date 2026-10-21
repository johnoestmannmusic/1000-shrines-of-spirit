#!/bin/sh
# Build the uploadable release: the stored shrine, emitted to a self-contained
# Rust file, compiled with bare rustc, assembled into dist/.
#
#   scripts/build-dist.sh [SEED]
#
# Upload the files in dist/ to any static host.
set -eu
cd "$(dirname "$0")/.."

mkdir -p artifacts dist

seed=""
[ "$#" -gt 0 ] && seed="--seed $1"

echo "emitting the artifact…"
cargo run -q --release -- --emit $seed --out artifacts/ShrineGen.rs

echo "compiling with rustc…"
rustc --edition 2021 --target wasm32-unknown-unknown -O --crate-type cdylib \
	--crate-name shrine_0016 artifacts/ShrineGen.rs -o dist/shrine.wasm

cp web/index.html web/shrine.worklet.js dist/
cp artifacts/ShrineGen.rs dist/shrine.rs

# A rendered WAV is optional: drop one in web/wav/ and the page offers it.
if [ -d web/wav ]; then
	mkdir -p dist/wav
	cp web/wav/*.wav dist/wav/ 2>/dev/null || true
	# A listing, so the page finds the render whatever it is called.
	printf '[' > dist/wav/manifest.json
	first=1
	for f in web/wav/*.wav; do
		[ -e "$f" ] || continue
		[ "$first" -eq 1 ] || printf ',' >> dist/wav/manifest.json
		first=0
		name=$(basename "$f" | sed 's/\\/\\\\/g; s/"/\\"/g')
		printf '"%s"' "$name" >> dist/wav/manifest.json
	done
	printf ']' >> dist/wav/manifest.json
fi

echo "dist/ ready:"
ls -l dist/
