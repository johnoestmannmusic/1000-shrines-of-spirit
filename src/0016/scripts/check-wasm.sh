#!/bin/sh
# Verifies the artifact WASM is loadable with the plain WebAssembly API:
#   * zero imports (no host functions, no WASI, no glue), and
#   * the expected C-ABI exports present.
# Builds first, so it tests exactly what would ship.
set -eu
cd "$(dirname "$0")/.."
./build-web.sh >/dev/null
node --input-type=module -e '
import { readFile } from "node:fs/promises";
const mod = new WebAssembly.Module(await readFile("web/shrine.wasm"));
const imports = WebAssembly.Module.imports(mod).map((i) => `${i.module}.${i.name}`);
if (imports.length) {
	console.error("check-wasm: unexpected imports:", imports.join(", "));
	process.exit(1);
}
const exports = WebAssembly.Module.exports(mod).map((e) => e.name);
for (const name of ["memory", "shrine_abi_version", "shrine_logical_size"]) {
	if (!exports.includes(name)) {
		console.error("check-wasm: missing export", name, "have:", exports.join(", "));
		process.exit(1);
	}
}
console.log("check-wasm: 0 imports,", exports.length, "exports");
'
