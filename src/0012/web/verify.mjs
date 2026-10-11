// Native <-> WASM parity for ShrineGen.
//
// Loads the compiled module and reproduces the golden hashes that
// `engine/tests/parity_golden.rs` locks in on the native side. Run it against
// the engine build (`build-web.sh`) to prove the browser hears and sees exactly
// what Rust renders.
//
//   node web/verify.mjs [path/to/shrine.wasm]

import { readFile } from "node:fs/promises";

const AUDIO_GOLDEN = 0xddb4728052516eb7n;
const VISUAL_GOLDEN = [
	16372517786461568741n,
	9300958290172192542n,
	3436581339193562279n,
	3770431886702531045n,
	7701800233315402307n,
];
const SR = 48000;
const LOGICAL = 300;
const OFFSET = 0xcbf29ce484222325n;
const PRIME = 0x100000001b3n;
const MASK = 0xffffffffffffffffn;

function fnv(bytes, start, len, seed) {
	let h = seed;
	for (let i = start; i < start + len; i++) {
		h = ((h ^ BigInt(bytes[i])) * PRIME) & MASK;
	}
	return h;
}

const path = process.argv[2] ?? "web/shrine.wasm";
const bytes = await readFile(path);
const { instance } = await WebAssembly.instantiate(bytes, {});
const exports = instance.exports;

let failed = false;

// The audio half: 5 seconds of the coupled mix.
const audio = exports.shrine_new(0, 0, 1);
let hash = OFFSET;
let remaining = 5 * SR;
while (remaining > 0) {
	const frames = Math.min(4096, remaining);
	const ptr = exports.shrine_render_audio(audio, frames);
	hash = fnv(new Uint8Array(exports.memory.buffer, ptr, frames * 2 * 4), 0, frames * 2 * 4, hash);
	remaining -= frames;
}
exports.shrine_free(audio);
console.log(`coupled audio  ${hash.toString(16)}`);
if (hash !== AUDIO_GOLDEN) {
	console.error(`  MISMATCH — native ${AUDIO_GOLDEN.toString(16)}`);
	failed = true;
}

// The visual half: five frames.
const visual = exports.shrine_new(0, 0, 2);
for (let i = 0; i < VISUAL_GOLDEN.length; i++) {
	const ptr = exports.shrine_render_visual(visual, BigInt(i * SR));
	const view = new Uint8Array(exports.memory.buffer, ptr, LOGICAL * LOGICAL * 4);
	const frameHash = fnv(view, 0, view.length, OFFSET);
	console.log(`frame ${i}        ${frameHash.toString(16)}`);
	if (frameHash !== VISUAL_GOLDEN[i]) {
		console.error(`  MISMATCH — native ${VISUAL_GOLDEN[i].toString(16)}`);
		failed = true;
	}
}
exports.shrine_free(visual);

if (failed) {
	console.error("NATIVE/WASM PARITY FAILED");
	process.exit(1);
}
console.log("NATIVE/WASM PARITY OK");
