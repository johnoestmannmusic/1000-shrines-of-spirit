// Proves the WASM build renders exactly what the native build renders:
// prints the FNV-1a hash of the first 30 s with default seeds, which must
// equal GOLDEN_HASH in tests/determinism.rs. Also reports render speed.
//
//   node web/verify.mjs [path/to/track.wasm]
import { readFileSync } from "node:fs";

const path = process.argv[2] ?? new URL("./track.wasm", import.meta.url);
const { instance } = await WebAssembly.instantiate(readFileSync(path), {});
const { memory, sos_new, sos_render } = instance.exports;

const SEEDS = [1000n, 9n, 1009n, 2026n, 168n]; // DEFAULT_SEEDS in engine/src/lib.rs
// DEFAULT_SETTINGS: scale 0 (Lydian), 3 chords, pace 0 (half-time), kit 0 (Acoustic).
const args = [...SEEDS.flatMap((s) => [Number(s >> 32n), Number(s & 0xffffffffn)]), 0, 3, 0, 0];

let t0 = performance.now();
const player = sos_new(...args);
const initMs = performance.now() - t0;

const QUANTUM = 128;
const total = 30 * 48000;
let h = 0xcbf29ce484222325n;
const MASK = 0xffffffffffffffffn;
let worst = 0;
t0 = performance.now();
for (let done = 0; done < total; done += QUANTUM) {
  const q0 = performance.now();
  const ptr = sos_render(player, QUANTUM);
  worst = Math.max(worst, performance.now() - q0);
  const bytes = new Uint8Array(memory.buffer, ptr, QUANTUM * 2 * 4);
  for (const b of bytes) {
    h = ((h ^ BigInt(b)) * 0x100000001b3n) & MASK;
  }
}
const renderMs = performance.now() - t0;

console.log(`hash:  0x${h.toString(16).padStart(16, "0")}`);
console.log(`init:  ${initMs.toFixed(0)} ms`);
console.log(`worst 128-frame quantum: ${worst.toFixed(2)} ms (budget 2.67 ms)`);
console.log(`render incl. hashing: ${(renderMs / 1000).toFixed(1)} s for 30 s of audio`);
