// Native <-> WASM parity for ShrineGen.
//
// Loads the compiled module and reproduces the golden hashes that
// `engine/tests/parity_golden.rs` locks in on the native side. Run it against
// the engine build (`build-web.sh`) to prove the browser hears and sees exactly
// what Rust renders.
//
//   node web/verify.mjs [path/to/shrine.wasm]

import { readFile } from "node:fs/promises";

// Re-locked in 0014: the weather bed is now block-independent (see
// AUDIO_GOLDEN in engine/tests/parity_golden.rs).
const AUDIO_GOLDEN = 0x4431b25653cbda5cn;
// Re-locked when `raster::fill_face` was fixed (side faces had been drawn with
// x and y swapped), for the weather rework (FEAT-40), for the textured, lit
// architecture (FEAT-44), for its paler palette (FEAT-45), and for the default
// camera turning at -2°/s (frames 1-4). These mirror
// VISUAL_GOLDEN in engine/tests/parity_golden.rs.
const VISUAL_GOLDEN = [
	0xba5ce76b7435b6f1n,
	0x45d1c475b1940cfcn,
	0xe3f880345775ef2en,
	0x96f34aa3ec60d4fbn,
	0xee7f6469f21fd637n,
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

// 0014: the built futures playing 2-step garage, from the recipes and
// native hashes in parity-futures.json (written by engine/tests/parity_golden.rs).
const fixture = JSON.parse(await readFile(new URL("./parity-futures.json", import.meta.url), "utf8"));
for (const config of fixture.configs) {
	const words = Uint32Array.from(config.words);
	const ptr = exports.shrine_alloc(words.length * 4);
	new Uint32Array(exports.memory.buffer, ptr, words.length).set(words);
	const a = exports.shrine_new(ptr, words.length, 1);
	let h = OFFSET;
	let left = fixture.audio_seconds * SR;
	while (left > 0) {
		const frames = Math.min(4096, left);
		const out = exports.shrine_render_audio(a, frames);
		h = fnv(new Uint8Array(exports.memory.buffer, out, frames * 2 * 4), 0, frames * 2 * 4, h);
		left -= frames;
	}
	exports.shrine_free(a);
	const got = h.toString(16).padStart(16, "0");
	console.log(`${config.name.padEnd(20)} audio ${got}`);
	if (got !== config.audio) {
		console.error(`  MISMATCH — native ${config.audio}`);
		failed = true;
	}
	const v = exports.shrine_new(ptr, words.length, 2);
	fixture.frames.forEach((t, i) => {
		const out = exports.shrine_render_visual(v, BigInt(Math.round(t * SR)));
		const view = new Uint8Array(exports.memory.buffer, out, LOGICAL * LOGICAL * 4);
		const fh = fnv(view, 0, view.length, OFFSET).toString(16).padStart(16, "0");
		console.log(`${config.name.padEnd(20)} frame ${i} ${fh}`);
		if (fh !== config.visual[i]) {
			console.error(`  MISMATCH — native ${config.visual[i]}`);
			failed = true;
		}
	});
	exports.shrine_free(v);
	exports.shrine_dealloc(ptr, words.length * 4);
}

// 0015: the Learn page's exports describe the piece and read its moving parts.
if (typeof exports.shrine_learn_ptr === "function") {
	const v = exports.shrine_new(0, 0, 2);
	const len = exports.shrine_learn_len(v);
	const text = new TextDecoder().decode(new Uint8Array(exports.memory.buffer, exports.shrine_learn_ptr(v), len));
	try {
		const d = JSON.parse(text);
		console.log(`learn          ${len} bytes, ${d.chords.length} chords, ${d.spectra.length} spectra`);
	} catch (e) {
		console.error(`  LEARN JSON INVALID: ${e.message}`);
		failed = true;
	}
	exports.shrine_free(v);
	const a = exports.shrine_new(0, 0, 1);
	exports.shrine_render_audio(a, 4800);
	const st = new Float64Array(exports.memory.buffer, exports.shrine_learn_state(a), exports.shrine_learn_state_len());
	if (st[0] !== 4800) {
		console.error(`  LEARN STATE clock ${st[0]}, expected 4800`);
		failed = true;
	}
	exports.shrine_free(a);
} else {
	console.error("  no Learn exports");
	failed = true;
}

if (failed) {
	console.error("NATIVE/WASM PARITY FAILED");
	process.exit(1);
}
console.log("NATIVE/WASM PARITY OK");
