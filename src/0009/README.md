# 0009
02 OCT 26

An endless Glitch Ambient track built as a software artifact. A frozen, complex chord and a bass drone hold still underneath, while two cycling layers of glitch artifacts loop, mutate and drift out of phase above them.

The piece is a Rust program with zero dependencies. It compiles two ways:

- **Native**: renders any length of the piece to a 24-bit / 48 kHz WAV file.
- **WASM**: plays the piece live and forever in a browser.

Both produce **bit-identical audio**, and are designed to keep doing so in 20 years (see *Determinism*).

## Signal flow

```
seed 1 → 3-op FM chord → Paulstretch-like FFT freeze → cyclic long filter mod ─┐
seed 2 → glitch artifacts 1 → cyclic repeats 1 ─┐                              │
seed 3 → glitch artifacts 2 → cyclic repeats 2 ─┴→ echo / delay ───────────────┼→ long reverb
seed 4 → FM bass + sub-bass → low-pass ────────────────────────────────────────┘
```

| Stage | File |
|---|---|
| FM chord, spectral freeze (random-phase resynthesis, 32k FFT, ¼ hop) | `src/drone.rs`, `src/fm.rs`, `src/fft.rs` |
| Slow cyclic modulation (LFO periods of 41–307 s, mutually prime) | `src/modulate.rs` |
| Glitch voices: crushed FM blips, noise ticks, drone stutters, bell pings, square dropouts, crushed noise | `src/glitch.rs` |
| Cyclic repeats: 7-step and 11-step loops that repeat, then mutate one step | `src/pattern.rs` |
| Ping-pong echo | `src/delay.rs` |
| FM bass + sine sub through a low-pass | `src/bass.rs` |
| 8-line FDN reverb, ~18 s tail | `src/reverb.rs` |
| Mix and master | `src/lib.rs` |

The musical constants (chord voicings, tempo, layer configs, mix levels) are named `const`s at the top of each file.

## Render a WAV

```
cargo run --release -- --seconds 600 --out 0009.wav
```

Options: `--seconds N` (default 600), `--out FILE`, `--seed1 N` … `--seed4 N` (decimal or `0x` hex). The default seeds are the canonical version of the track (`DEFAULT_SEEDS` in `src/lib.rs`: 1000, 9, 1009, 2026). Any other seeds give an alternate rendition. A 12 s fade-out is applied to the end of the file only; the piece itself never ends.

## Play in a browser

```
./build-web.sh          # builds web/track.wasm
cd web && python3 -m http.server
```

Then open http://localhost:8000 and press Play. Alternate renditions take URL parameters: `?s1=1&s2=2&s3=3&s4=4`. The player is three plain files (`index.html`, `worklet.js`, `track.wasm`), with no build step and no JS libraries.

## Determinism

Given the same seeds, every build produces the same samples, bit for bit, at any block size:

- **No crates.** All math, the RNG (SplitMix64), the FFT and every DSP block are in this folder.
- **No platform math.** `sin`, `exp`, `tanh` and similar come from the platform's libm, which differs between systems. `src/math.rs` rebuilds them from `+ − × ÷`, `floor` and `sqrt`, which IEEE-754 guarantees exactly on every CPU and in WASM.
- **Block-size independence.** All timing runs off a single integer sample clock. Even the drone's FFT work is scheduled by sample position, spread across each hop.
- **Locked by tests.** `cargo test` checks a hash of the first 30 s against `GOLDEN_HASH` in `tests/determinism.rs`. `node web/verify.mjs` computes the same hash from the WASM build.
- **Pinned toolchain.** Rust 1.98.1 is pinned in `rust-toolchain.toml`.

```
cargo test                              # unit + determinism tests
cargo test --release -- --ignored       # 2-hour stability test
./build-web.sh && node web/verify.mjs   # WASM hash must equal GOLDEN_HASH
```

If the composition is changed on purpose, the golden hash changes with it. Update the constant, because that is a new version of the piece.
