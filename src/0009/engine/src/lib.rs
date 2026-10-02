//! 0009 — an endless Glitch Ambient track, built as a software artifact.
//!
//! ```text
//! seed1 → 3-op FM chord → random-phase FFT freeze → cyclic long filter mod ─┐
//! seed2 → glitch artifacts 1 → cyclic repeats 1 ─┐                              │
//! seed3 → glitch artifacts 2 → cyclic repeats 2 ─┴→ echo/delay ─────────────────┼→ long reverb
//! seed4 → FM bass + sub-bass → low-pass ─────────────────────────────────────────┘
//! ```
//!
//! Determinism is the core design constraint: given the same seeds, the
//! output is bit-identical on every platform (native and WASM), at any
//! render block size, and with any future compiler. See `math.rs`.

pub mod bass;
pub mod delay;
pub mod drone;
pub mod fft;
pub mod filter;
pub mod fm;
pub mod glitch;
pub mod math;
pub mod modulate;
pub mod pattern;
pub mod reverb;
pub mod rng;
pub mod telemetry;

#[cfg(not(target_arch = "wasm32"))]
pub mod cli;

#[cfg(target_arch = "wasm32")]
mod wasm;

use bass::Bass;
use delay::PingPong;
use drone::Drone;
use filter::{DcBlock, Svf};
use glitch::History;
use modulate::Mods;
use pattern::{Layer, LayerConfig};
use reverb::Reverb;
use telemetry::Meters;

pub const SAMPLE_RATE: u32 = 48_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seeds {
    pub s1: u64,
    pub s2: u64,
    pub s3: u64,
    pub s4: u64,
}

/// The canonical version of the track.
pub const DEFAULT_SEEDS: Seeds = Seeds { s1: 1000, s2: 9, s3: 1009, s4: 2026 };

// Sixteenth note ≈ 84 BPM, divisible by 1-4 for ratchets.
const SIXTEENTH: u64 = 8568;

// Mix. "send" levels feed the shared long reverb.
const DRONE_DRY: f64 = 1.0;
const DRONE_SEND: f64 = 0.35;
const BASS_DRY: f64 = 1.0;
const BASS_SEND: f64 = 0.0;
const GLITCH1_DRY: f64 = 0.8;
const GLITCH1_ECHO: f64 = 0.35;
const GLITCH1_SEND: f64 = 0.3;
const GLITCH2_DRY: f64 = 0.7;
const GLITCH2_ECHO: f64 = 0.6;
const GLITCH2_SEND: f64 = 0.4;
const ECHO_DRY: f64 = 0.6;
const ECHO_SEND: f64 = 0.5;
const ECHO_FEEDBACK: f64 = 0.55;
const REVERB_WET: f64 = 0.55;
const MASTER_GAIN: f64 = 0.9;
const FADE_IN_SECONDS: u64 = 10;

/// Drone filter sweep range.
const DRONE_CUTOFF_MIN: f64 = 400.0;
const DRONE_CUTOFF_OCTAVES: f64 = 4.2;

pub fn layer1() -> LayerConfig {
    LayerConfig {
        steps: 7,
        step_len: SIXTEENTH,
        //        Blip Tick Stut Ping Click Crush
        weights: [1.0, 3.0, 2.0, 0.3, 2.0, 1.5],
        notes: &[86.0, 88.0, 90.0, 92.0, 93.0, 95.0, 97.0],
        fill: 0.55,
        gain: (0.25, 0.6),
        pan_width: 0.9,
        repeats: (4, 9),
        max_ratchet: 4,
    }
}

pub fn layer2() -> LayerConfig {
    LayerConfig {
        steps: 11,
        step_len: SIXTEENTH * 2,
        weights: [3.0, 0.5, 2.0, 2.5, 0.5, 0.5],
        notes: &[62.0, 64.0, 66.0, 68.0, 69.0, 71.0, 73.0, 74.0, 76.0, 78.0, 81.0],
        fill: 0.45,
        gain: (0.18, 0.45),
        pan_width: 0.6,
        repeats: (3, 7),
        max_ratchet: 3,
    }
}

pub struct Track {
    seeds: Seeds,
    clock: u64,
    mods: Mods,
    drone: Drone,
    drone_filter: [Svf; 2],
    history: History,
    glitch1: Layer,
    glitch2: Layer,
    echo: PingPong,
    bass: Bass,
    reverb: Reverb,
    dc: [DcBlock; 2],
    // Display-only state: written by `tick`, never read by it.
    meters: Meters,
    drone_cutoff: f64,
    drone_q: f64,
    overview_frame: u64,
}

impl Track {
    pub fn new(seeds: Seeds) -> Self {
        let sr = SAMPLE_RATE as f64;
        Track {
            seeds,
            clock: 0,
            mods: Mods::new(seeds.s1),
            drone: Drone::new(seeds.s1),
            drone_filter: [Svf::new(1000.0, 0.7, sr), Svf::new(1000.0, 0.7, sr)],
            history: History::new(),
            glitch1: Layer::new(layer1(), seeds.s2, 0x61),
            glitch2: Layer::new(layer2(), seeds.s3, 0x62),
            echo: PingPong::new(3 * SIXTEENTH as usize, ECHO_FEEDBACK),
            bass: Bass::new(seeds.s4),
            reverb: Reverb::new(),
            dc: Default::default(),
            meters: Meters::default(),
            drone_cutoff: 0.0,
            drone_q: 0.0,
            overview_frame: u64::MAX,
        }
    }

    /// Fill `out` with interleaved stereo samples. Output depends only on the
    /// seeds and the running sample position, never on how it is chunked.
    pub fn render(&mut self, out: &mut [f32]) {
        for frame in out.chunks_exact_mut(2) {
            let (l, r) = self.tick();
            frame[0] = l as f32;
            frame[1] = r as f32;
        }
    }

    fn tick(&mut self) -> (f64, f64) {
        let sr = SAMPLE_RATE as f64;
        let clock = self.clock;
        let m = self.mods.at(clock);

        // 1: frozen drone through the slowly cycling filter.
        if clock % 32 == 0 {
            let cutoff = DRONE_CUTOFF_MIN * math::exp2(m.drone_open * DRONE_CUTOFF_OCTAVES);
            let q = 0.6 + 2.4 * m.drone_reso;
            for f in self.drone_filter.iter_mut() {
                f.set(cutoff, q, sr);
            }
            self.drone_cutoff = cutoff;
            self.drone_q = q;
        }
        let (dl, dr) = self.drone.next();
        let dl = self.drone_filter[0].process(dl).low;
        let dr = self.drone_filter[1].process(dr).low;
        self.history.push(dl, dr);

        // 2 & 3: glitch layers.
        let g1 = self.glitch1.next(clock, m.density1, &self.history);
        let g2 = self.glitch2.next(clock, m.density2, &self.history);
        let (el, er) = self.echo.process(
            g1.0 * GLITCH1_ECHO + g2.0 * GLITCH2_ECHO,
            g1.1 * GLITCH1_ECHO + g2.1 * GLITCH2_ECHO,
        );

        // 4: bass.
        let b = self.bass.next(clock, &m);

        // Shared long reverb.
        let (rl, rr) = self.reverb.process(
            dl * DRONE_SEND + b * BASS_SEND + g1.0 * GLITCH1_SEND + g2.0 * GLITCH2_SEND + el * ECHO_SEND,
            dr * DRONE_SEND + b * BASS_SEND + g1.1 * GLITCH1_SEND + g2.1 * GLITCH2_SEND + er * ECHO_SEND,
        );

        let mut l = dl * DRONE_DRY + b * BASS_DRY + g1.0 * GLITCH1_DRY + g2.0 * GLITCH2_DRY
            + el * ECHO_DRY
            + rl * REVERB_WET;
        let mut r = dr * DRONE_DRY + b * BASS_DRY + g1.1 * GLITCH1_DRY + g2.1 * GLITCH2_DRY
            + er * ECHO_DRY
            + rr * REVERB_WET;

        let fade_len = FADE_IN_SECONDS * SAMPLE_RATE as u64;
        if clock < fade_len {
            let x = clock as f64 / fade_len as f64;
            l *= x * x;
            r *= x * x;
        }

        self.clock += 1;
        let l = math::tanh(self.dc[0].process(l) * MASTER_GAIN);
        let r = math::tanh(self.dc[1].process(r) * MASTER_GAIN);

        let mt = &mut self.meters;
        mt.drone = mt.drone.max(dl.abs()).max(dr.abs());
        mt.bass = mt.bass.max(b.abs());
        mt.glitch1 = mt.glitch1.max(g1.0.abs()).max(g1.1.abs());
        mt.glitch2 = mt.glitch2.max(g2.0.abs()).max(g2.1.abs());
        mt.echo_l = mt.echo_l.max(el.abs());
        mt.echo_r = mt.echo_r.max(er.abs());
        mt.reverb = mt.reverb.max(rl.abs()).max(rr.abs());
        mt.out_l = mt.out_l.max(l.abs());
        mt.out_r = mt.out_r.max(r.abs());
        (l, r)
    }
}
