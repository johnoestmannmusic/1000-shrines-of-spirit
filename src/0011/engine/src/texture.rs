//! Physical texture loops: an editable event grid.
//!
//! The sustained timbres in `atmos` are pads; this module makes the *foley*
//! kind of atmosphere — fire crackling, water running, stones knocking, wood
//! creaking, rocks and logs struck, cave drips and natural chimes — as short
//! synthesised grains placed on an 8 x 16 grid (eight event rows, sixteen
//! steps). Nothing is sampled: each grain is built from noise, clicks and
//! resonances, then wrapped circularly into the loop the same way the sustained
//! timbres are, so the seam is hidden and playback can go round for ever.
//!
//! The grid is the design. The Fire / Water / Stones / Wood presets are just
//! starting grids; every bit is editable in setup and stored in the recipe.

use crate::filter::Svf;
use crate::math::{cos_turns, exp, midi_hz, sin_turns};
use crate::rng::Rng;
use crate::SAMPLE_RATE;

const SR: f64 = SAMPLE_RATE as f64;

/// Event rows.
pub const ROWS: usize = 8;
/// Steps per grid row.
pub const STEPS: usize = 16;
/// The rows, top to bottom, for the editor and the recipe.
pub const EVENT_NAMES: [&str; ROWS] = ["Crackle", "Knock", "Creak", "Hiss", "Rock", "Log", "Drip", "Chime"];

/// An 8 x 16 on/off grid: one bitmask per event row (row 0 in the low bits).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grid {
    pub rows: [u16; ROWS],
}

impl Grid {
    pub const EMPTY: Grid = Grid { rows: [0; ROWS] };

    pub fn is_empty(&self) -> bool {
        self.rows.iter().all(|r| *r == 0)
    }

    pub fn get(&self, row: usize, step: usize) -> bool {
        self.rows[row] & (1 << step) != 0
    }

    pub fn set(&mut self, row: usize, step: usize, on: bool) {
        if on {
            self.rows[row] |= 1 << step;
        } else {
            self.rows[row] &= !(1 << step);
        }
    }

    pub fn toggle(&mut self, row: usize, step: usize) {
        self.rows[row] ^= 1 << step;
    }

    /// The whole grid as 128 bits (row 0 in the low 16 bits).
    pub fn bits(&self) -> u128 {
        self.rows.iter().enumerate().fold(0u128, |b, (i, r)| b | ((*r as u128) << (16 * i)))
    }

    pub fn from_bits(bits: u128) -> Grid {
        let mut rows = [0u16; ROWS];
        for (i, r) in rows.iter_mut().enumerate() {
            *r = (bits >> (16 * i)) as u16;
        }
        Grid { rows }
    }

    /// Four 32-bit words, for the WASM ABI.
    pub fn words(&self) -> [u32; 4] {
        let b = self.bits();
        [b as u32, (b >> 32) as u32, (b >> 64) as u32, (b >> 96) as u32]
    }

    pub fn from_words(w: [u32; 4]) -> Grid {
        let b = (w[0] as u128) | ((w[1] as u128) << 32) | ((w[2] as u128) << 64) | ((w[3] as u128) << 96);
        Grid::from_bits(b)
    }

    /// 32 lowercase hex digits, used as the recipe token.
    pub fn to_hex(&self) -> String {
        format!("{:032x}", self.bits())
    }

    pub fn from_hex(s: &str) -> Option<Grid> {
        // The old 4-row form (16 digits) is the low half of the new one.
        let padded;
        let s = match s.len() {
            32 => s,
            16 => {
                padded = format!("{}{s}", "0".repeat(16));
                &padded
            }
            _ => return None,
        };
        u128::from_str_radix(s, 16).ok().map(Grid::from_bits)
    }

    pub fn fire() -> Grid {
        Grid { rows: [0b1110_1110_1110_1110, 0b0000_0010_0000_0010, 0, 0b1000_0000_1000_0000, 0, 0, 0, 0] }
    }

    pub fn water() -> Grid {
        Grid { rows: [0, 0b0001_0001_0001_0001, 0, 0b1111_1111_1111_1111, 0, 0, 0, 0] }
    }

    pub fn stones() -> Grid {
        Grid { rows: [0b0001_0000_0100_0000, 0b1010_1010_1010_1010, 0, 0, 0, 0, 0, 0] }
    }

    pub fn wood() -> Grid {
        Grid { rows: [0, 0b0000_0010_0000_0010, 0b1010_1010_1010_1010, 0, 0, 0, 0, 0] }
    }
}

/// Render one channel of an event-grid loop over `total` samples, for a loop of
/// `len`. Grains are laid down for three passes (warm-up, loop, crossfade), so
/// tails from the end of one cycle are already present at the start of the next
/// and `close_loop` has a real continuation to fade in.
pub fn render_channel(grid: Grid, notes: &[f64], rng: &mut Rng, len: usize, total: usize) -> Vec<f64> {
    let mut out = vec![0.0; total];
    let mut noise = Rng::new(rng.next_u64());
    let step_len = (len / STEPS).max(1);
    for pass in 0..3usize {
        let base = pass * len;
        for row in 0..ROWS {
            let mut bits = grid.rows[row];
            while bits != 0 {
                let step = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                let start = base + step * step_len;
                if start < total {
                    grain(row, &mut out, start, notes, &mut noise, rng);
                }
            }
        }
    }
    out
}

fn add(out: &mut [f64], i: usize, x: f64) {
    if let Some(o) = out.get_mut(i) {
        *o += x;
    }
}

/// One synthesised event, written into `out` at `start`.
fn grain(kind: usize, out: &mut [f64], start: usize, notes: &[f64], noise: &mut Rng, rng: &mut Rng) {
    match kind {
        0 => crackle(out, start, noise, rng),
        1 => knock(out, start, notes, rng),
        2 => creak(out, start, noise, rng),
        3 => hiss(out, start, noise, rng),
        4 => rock(out, start, noise, rng),
        5 => log(out, start, notes, rng),
        6 => drip(out, start, notes, noise, rng),
        _ => chime(out, start, notes, rng),
    }
}

/// A crackle: a tiny burst of noise through a narrow band-pass, gone in tens
/// of milliseconds. A marked step spawns a small cluster, so a sparse grid
/// still reads as fire rather than one tick.
fn crackle(out: &mut [f64], start: usize, noise: &mut Rng, rng: &mut Rng) {
    let pops = rng.int(1, 3);
    let mut at = start;
    for _ in 0..pops {
        let hz = rng.range(1_200.0, 4_800.0);
        let q = rng.range(1.5, 4.0);
        let tau = rng.range(0.006, 0.03);
        let dur = (tau * 7.0 * SR) as usize;
        let amp = rng.range(0.5, 1.0);
        let mut f = Svf::new(hz, q, SR);
        let c = exp(-1.0 / (tau * SR));
        let mut env = amp;
        for i in 0..dur {
            let click = if i == 0 { noise.bipolar() } else { 0.0 };
            let y = f.process(noise.bipolar()).band;
            add(out, at + i, (y + 0.6 * click) * env);
            env *= c;
        }
        at += (rng.range(0.004, 0.02) * SR) as usize;
    }
}

/// A knock: an impulse struck into three inharmonic resonances, tuned to a
/// chord tone so it belongs to the harmony. Medium decay.
fn knock(out: &mut [f64], start: usize, notes: &[f64], rng: &mut Rng) {
    let base = if notes.is_empty() { 60.0 } else { notes[rng.below(notes.len())] };
    let hz = midi_hz(base + rng.range(-2.0, 2.0));
    let modes = [1.0, rng.range(2.3, 3.1), rng.range(4.5, 6.5)];
    let taus = [rng.range(0.09, 0.26), rng.range(0.05, 0.16), rng.range(0.02, 0.08)];
    let gains = [1.0, rng.range(0.4, 0.75), rng.range(0.15, 0.4)];
    partials(out, start, hz, &modes, &taus, &gains, 0.6);
}

/// A rock struck: a low, dull thud — a pitch-diving sine under a short burst of
/// low-passed noise. Untuned, like a stone on stone.
fn rock(out: &mut [f64], start: usize, noise: &mut Rng, rng: &mut Rng) {
    let hz0 = rng.range(150.0, 320.0);
    let hz1 = hz0 * rng.range(0.45, 0.7);
    let tau = rng.range(0.06, 0.16);
    let dur = (tau * 7.0 * SR) as usize;
    let mut f = Svf::new(rng.range(600.0, 1_400.0), 0.8, SR);
    let c = exp(-1.0 / (tau * SR));
    let mut env = 1.0;
    let mut ph = 0.0;
    for i in 0..dur {
        let u = i as f64 / dur as f64;
        let hz = hz0 + (hz1 - hz0) * u;
        ph += hz / SR;
        ph -= ph.floor();
        let noise_low = f.process(noise.bipolar()).low;
        add(out, start + i, (0.9 * sin_turns(ph) + 0.5 * noise_low) * env);
        env *= c;
    }
}

/// A hollow log hit: a click into a woody mid resonance with two inharmonic
/// partners. Short and dry, a little lower than a chime.
fn log(out: &mut [f64], start: usize, notes: &[f64], rng: &mut Rng) {
    let base = if notes.is_empty() { 55.0 } else { notes[rng.below(notes.len())] - 7.0 };
    let hz = midi_hz(base + rng.range(-1.0, 1.0));
    let modes = [1.0, rng.range(1.7, 2.2), rng.range(2.6, 3.4)];
    let taus = [rng.range(0.08, 0.18), rng.range(0.05, 0.12), rng.range(0.03, 0.08)];
    let gains = [1.0, rng.range(0.5, 0.85), rng.range(0.2, 0.5)];
    partials(out, start, hz, &modes, &taus, &gains, 0.7);
}

/// A cave/water drip: a short pitched blip whose pitch rises, with a whisper of
/// noise at the impact. Tuned two octaves up so it glints above the mix.
fn drip(out: &mut [f64], start: usize, notes: &[f64], noise: &mut Rng, rng: &mut Rng) {
    let base = if notes.is_empty() { 72.0 } else { notes[rng.below(notes.len())] + 12.0 };
    let hz0 = midi_hz(base) * rng.range(0.8, 1.0);
    let hz1 = hz0 * rng.range(1.5, 2.4);
    let tau = rng.range(0.04, 0.1);
    let dur = (tau * 8.0 * SR) as usize;
    let c = exp(-1.0 / (tau * SR));
    let mut env = 1.0;
    let mut ph = 0.0;
    for i in 0..dur {
        let u = i as f64 / dur as f64;
        let hz = hz0 + (hz1 - hz0) * u;
        ph += hz / SR;
        ph -= ph.floor();
        let impact = if i < 12 { 0.25 * noise.bipolar() } else { 0.0 };
        add(out, start + i, (sin_turns(ph) + impact) * env * 0.7);
        env *= c;
    }
}

/// A natural chime/bell: four inharmonic partials with a long decay, tuned to a
/// chord tone an octave up. The brightest and longest of the hits.
fn chime(out: &mut [f64], start: usize, notes: &[f64], rng: &mut Rng) {
    let base = if notes.is_empty() { 79.0 } else { notes[rng.below(notes.len())] + 12.0 };
    let hz = midi_hz(base + rng.range(-1.0, 1.0));
    let modes = [1.0, rng.range(2.7, 3.3), rng.range(5.1, 6.2), rng.range(8.2, 10.5)];
    let taus = [rng.range(0.4, 0.9), rng.range(0.3, 0.7), rng.range(0.15, 0.4), rng.range(0.08, 0.25)];
    let gains = [1.0, rng.range(0.5, 0.8), rng.range(0.25, 0.5), rng.range(0.1, 0.3)];
    partials(out, start, hz, &modes, &taus, &gains, 0.45);
}

/// A struck resonator: `hz` plus inharmonic modes, each an exponentially
/// decaying sine. Shared by the knock, log and chime.
fn partials(out: &mut [f64], start: usize, hz: f64, modes: &[f64], taus: &[f64], gains: &[f64], amp: f64) {
    let dur = (taus[0] * 8.0 * SR) as usize;
    for i in 0..dur {
        let t = i as f64 / SR;
        let mut y = 0.0;
        for m in 0..modes.len() {
            y += gains[m] * sin_turns(modes[m] * hz * t) * exp(-t / taus[m]);
        }
        add(out, start + i, y * amp);
    }
}

/// A creak: band-passed noise whose centre bends upwards, with a rough
/// stick-slip envelope (the sound of wood under load).
fn creak(out: &mut [f64], start: usize, noise: &mut Rng, rng: &mut Rng) {
    let f0 = rng.range(180.0, 700.0);
    let f1 = f0 * rng.range(1.3, 2.6);
    let q = rng.range(4.0, 10.0);
    let tau = rng.range(0.15, 0.5);
    let dur = (tau * 7.0 * SR) as usize;
    let rate = rng.int(5, 11) as f64;
    let phase = rng.unit();
    let mut f = Svf::default();
    for i in 0..dur {
        let u = i as f64 / dur.max(1) as f64;
        if i % 32 == 0 {
            f.set(f0 + (f1 - f0) * u, q, SR);
        }
        // Slow swell, roughened by an irregular flutter.
        let env = (0.5 - 0.5 * cos_turns(0.5 * u)) * (0.6 + 0.4 * sin_turns(rate * u + phase));
        add(out, start + i, f.process(noise.bipolar()).band * env);
    }
}

/// Hiss / flow: noise through a slowly moving band-pass with a soft swell and
/// fall. The running-water bed.
fn hiss(out: &mut [f64], start: usize, noise: &mut Rng, rng: &mut Rng) {
    let f0 = rng.range(400.0, 2_600.0);
    let f1 = f0 * rng.range(0.55, 1.7);
    let q = rng.range(0.8, 2.5);
    let tau = rng.range(0.3, 0.9);
    let dur = (tau * 4.0 * SR) as usize;
    let mut f = Svf::new(f0, q, SR);
    for i in 0..dur {
        let u = i as f64 / dur.max(1) as f64;
        if i % 32 == 0 {
            f.set(f0 + (f1 - f0) * u, q, SR);
        }
        let env = 0.5 - 0.5 * cos_turns(u);
        add(out, start + i, f.process(noise.bipolar()).band * env * 0.5);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_and_hex_round_trip() {
        for grid in [Grid::EMPTY, Grid::fire(), Grid::water(), Grid::stones(), Grid::wood()] {
            assert_eq!(Grid::from_bits(grid.bits()), grid);
            assert_eq!(Grid::from_hex(&grid.to_hex()), Some(grid));
            assert_eq!(Grid::from_words(grid.words()), grid);
        }
        // The old 16-digit form is the low half.
        assert_eq!(Grid::from_hex("808000000202eeee"), Some(Grid::from_bits(0x8080_0000_0202_EEEE)));
        assert_eq!(Grid::from_hex("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"), None);
        assert_eq!(Grid::from_hex("abc"), None);
    }

    #[test]
    fn presets_are_distinct_and_nonempty() {
        let all = [Grid::fire(), Grid::water(), Grid::stones(), Grid::wood()];
        for (i, g) in all.iter().enumerate() {
            assert!(!g.is_empty(), "preset {i} is empty");
            for other in &all[i + 1..] {
                assert_ne!(g, other, "presets {i} collide");
            }
        }
    }

    #[test]
    fn set_and_toggle_work() {
        let mut g = Grid::EMPTY;
        assert!(!g.get(1, 3));
        g.toggle(1, 3);
        assert!(g.get(1, 3));
        g.set(1, 3, false);
        assert!(!g.get(1, 3));
        // The new rows are addressable too.
        g.toggle(7, 15);
        assert!(g.get(7, 15));
    }

    #[test]
    fn rendering_is_finite_and_distinct() {
        let len = 4 * SAMPLE_RATE as usize;
        let total = 2 * len + 4800;
        let mut hashes = Vec::new();
        // One row lit at a time, so every event kind is exercised.
        for row in 0..ROWS {
            let mut grid = Grid::EMPTY;
            grid.set(row, 3, true);
            let mut rng = Rng::stream(11, 0xA1);
            let s = render_channel(grid, &[62.0, 66.0, 69.0], &mut rng, len, total);
            assert!(s.iter().all(|x| x.is_finite()), "row {row} non-finite");
            assert!(s.iter().any(|x| x.abs() > 1e-6), "row {row} produced silence");
            let h = s.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, x| {
                x.to_bits().to_le_bytes().into_iter().fold(h, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
            });
            assert!(!hashes.contains(&h), "two event rows rendered identically");
            hashes.push(h);
        }
    }
}
