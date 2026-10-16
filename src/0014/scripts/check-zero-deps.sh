#!/bin/sh
# Fails if the engine crate declares any external dependency (HC001).
# The artifact must remain buildable with the standard library alone, forever.
set -eu

file=engine/Cargo.toml
[ -f "$file" ] || { echo "check-zero-deps: missing $file" >&2; exit 1; }

deps=$(sed -n '/^\[dependencies\]/,/^\[/p' "$file" | grep -vE '^[[:space:]]*(\[|#|$)' || true)
if [ -n "$deps" ]; then
	echo "check-zero-deps: engine declares external dependencies:" >&2
	echo "$deps" >&2
	exit 1
fi
echo "check-zero-deps: engine is dependency-free"
