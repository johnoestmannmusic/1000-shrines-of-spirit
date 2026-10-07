//! Physical texture loops: an editable event grid.
//!
//! The sustained timbres in `atmos` are pads; this module makes the *foley*
//! kind of atmosphere — fire crackling, water running, stones knocking, wood
//! creaking, rocks and logs struck, cave drips and natural chimes — as short
//! synthesised grains placed on an 8 x 16 grid (eight event rows, sixteen
//! steps). Nothing is sampled and nothing is a pure tone: each hit is a short
//! burst of noise exciting a few heavily damped, inharmonic resonances, all
//! rolled off through a low-pass. That is what keeps the hits dull and woody
//! rather than ringing. The grains are wrapped circularly into the loop the
//! same way the sustained timbres are, so the seam is hidden.
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
        Grid { rows: [0b0001_0000_0100_0000, 0b1010_1010_1010_1010, 0, 0, 0b1000_0010_0000_1000, 0, 0, 0] }
    }

    pub fn wood() -> Grid {
        Grid { rows: [0, 0b0000_0010_0000_0010, 0b1010_1010_1010_1010, 0, 0, 0b1010_1010_1010_1010, 0, 0] }
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
        1 => knock(out, start, notes, noise, rng),
        2 => creak(out, start, noise, rng),
        3 => hiss(out, start, noise, rng),
        4 => rock(out, start, noise, rng),
        5 => log(out, start, notes, noise, rng),
        6 => drip(out, start, notes, noise, rng),
        _ => chime(out, start, notes, noise, rng),
    }
}

/// Noise-excited damped modal synthesis: a short burst of noise drives a bank
/// of band-passes, each damped fast. Low Q and short taus keep it woody and
/// dull rather than ringing, and the inharmonic centres keep it unpitched. This
/// is the organic core every struck hit is built on.
fn modes(out: &mut [f64], start: usize, list: &[(f64, f64, f64, f64)], excite_ms: f64, lp_hz: f64, noise: &mut Rng) {
    let tau = list.iter().map(|m| m.2).fold(0.0, f64::max);
    let dur = ((tau * 6.0 * SR) as usize).max(1);
    let excite = ((excite_ms * 0.001 * SR) as usize).min(dur);
    let mut fs: Vec<Svf> = list.iter().map(|m| Svf::new(m.0, m.1, SR)).collect();
    let cs: Vec<f64> = list.iter().map(|m| exp(-1.0 / (m.2 * SR))).collect();
    let mut envs: Vec<f64> = list.iter().map(|m| m.3).collect();
    let mut lp = Svf::new(lp_hz, 0.7, SR);
    for i in 0..dur {
        let x = if i < excite { noise.bipolar() } else { 0.0 };
        let mut y = 0.0;
        for k in 0..list.len() {
            y += envs[k] * fs[k].process(x).band;
            envs[k] *= cs[k];
        }
        add(out, start + i, lp.process(y).low);
    }
}

/// A short burst of low-passed noise: the dull direct impact of one object on
/// another, with no resonance of its own.
fn thud(out: &mut [f64], start: usize, dur_ms: f64, lp_hz: f64, amp: f64, noise: &mut Rng, rng: &mut Rng) {
    let dur = ((dur_ms * 0.001 * SR) as usize).max(1);
    let tau = (dur as f64 / SR) * rng.range(0.25, 0.5);
    let mut f = Svf::new(lp_hz, 0.8, SR);
    let c = exp(-1.0 / (tau * SR));
    let mut env = amp;
    for i in 0..dur {
        add(out, start + i, f.process(noise.bipolar()).low * env);
        env *= c;
    }
}

/// A short burst of band-passed noise: the bright tick at the moment of
/// contact. Low Q, so it is a tick rather than a ping.
fn click(out: &mut [f64], start: usize, dur_ms: f64, hz: f64, q: f64, amp: f64, noise: &mut Rng, rng: &mut Rng) {
    let dur = ((dur_ms * 0.001 * SR) as usize).max(1);
    let tau = (dur as f64 / SR) * rng.range(0.2, 0.4);
    let mut f = Svf::new(hz, q, SR);
    let c = exp(-1.0 / (tau * SR));
    let mut env = amp;
    for i in 0..dur {
        add(out, start + i, f.process(noise.bipolar()).band * env);
        env *= c;
    }
}

/// A crackle: a small cluster of tiny ticks, gone in tens of milliseconds, so
/// a sparse grid still hisses like fire rather than clicking once.
fn crackle(out: &mut [f64], start: usize, noise: &mut Rng, rng: &mut Rng) {
    let pops = rng.int(2, 4);
    let mut at = start;
    for _ in 0..pops {
        click(out, at, rng.range(2.0, 7.0), rng.range(1_600.0, 5_200.0), rng.range(1.0, 2.2), rng.range(0.5, 1.0), noise, rng);
        at += (rng.range(0.003, 0.018) * SR) as usize;
    }
}

/// A knock: knuckle on wood. A dry click plus two or three short, inharmonic,
/// heavily damped modes — no sustained ring.
fn knock(out: &mut [f64], start: usize, notes: &[f64], noise: &mut Rng, rng: &mut Rng) {
    let base = if notes.is_empty() { 62.0 } else { notes[rng.below(notes.len())] };
    let hz = midi_hz(base + rng.range(-3.0, 3.0)) * rng.range(0.7, 1.0);
    click(out, start, rng.range(1.0, 2.5), hz * 6.0, 0.9, 0.5, noise, rng);
    let m = [
        (hz, rng.range(3.0, 6.0), rng.range(0.015, 0.04), 1.0),
        (hz * rng.range(2.2, 2.8), rng.range(2.5, 5.0), rng.range(0.01, 0.03), rng.range(0.4, 0.7)),
        (hz * rng.range(3.7, 4.8), rng.range(2.0, 4.0), rng.range(0.008, 0.02), rng.range(0.2, 0.4)),
    ];
    modes(out, start, &m, rng.range(0.4, 1.0), 3_200.0, noise);
}

/// A creak: band-passed noise whose centre bends upward with a rough,
/// stick-slip envelope. Left almost unpitched and rolled off, so it reads as
/// wood under load rather than a resonant sweep.
fn creak(out: &mut [f64], start: usize, noise: &mut Rng, rng: &mut Rng) {
    let f0 = rng.range(160.0, 560.0);
    let f1 = f0 * rng.range(1.2, 2.0);
    let q = rng.range(2.0, 4.5);
    let tau = rng.range(0.1, 0.32);
    let dur = (tau * 6.0 * SR) as usize;
    let rate = rng.int(4, 9) as f64;
    let phase = rng.unit();
    let mut f = Svf::default();
    let mut lp = Svf::new(rng.range(1_800.0, 3_500.0), 0.7, SR);
    for i in 0..dur {
        let u = i as f64 / dur.max(1) as f64;
        if i % 32 == 0 {
            f.set(f0 + (f1 - f0) * u, q, SR);
        }
        let rough = 0.5 + 0.5 * sin_turns(rate * u + phase);
        let env = (0.5 - 0.5 * cos_turns(0.5 * u)) * rough * rough;
        add(out, start + i, lp.process(f.process(noise.bipolar()).band).low * env * 0.8);
    }
}

/// Hiss / flow: noise through a slowly moving, low-Q band-pass, rolled off.
/// The running-water bed.
fn hiss(out: &mut [f64], start: usize, noise: &mut Rng, rng: &mut Rng) {
    let f0 = rng.range(400.0, 2_400.0);
    let f1 = f0 * rng.range(0.6, 1.6);
    let q = rng.range(0.7, 1.8);
    let tau = rng.range(0.25, 0.7);
    let dur = (tau * 4.0 * SR) as usize;
    let mut f = Svf::new(f0, q, SR);
    let mut lp = Svf::new(3_000.0, 0.7, SR);
    for i in 0..dur {
        let u = i as f64 / dur.max(1) as f64;
        if i % 32 == 0 {
            f.set(f0 + (f1 - f0) * u, q, SR);
        }
        let env = 0.5 - 0.5 * cos_turns(u);
        add(out, start + i, lp.process(f.process(noise.bipolar()).band).low * env * 0.6);
    }
}

/// A rock struck: mostly dull noise, with only one short low mode for body.
/// Unpitched and over quickly, like a stone landing on stone.
fn rock(out: &mut [f64], start: usize, noise: &mut Rng, rng: &mut Rng) {
    thud(out, start, rng.range(8.0, 22.0), rng.range(600.0, 1_400.0), 1.0, noise, rng);
    click(out, start, rng.range(1.0, 2.5), rng.range(800.0, 2_000.0), rng.range(0.8, 1.4), rng.range(0.4, 0.8), noise, rng);
    let m = [(rng.range(80.0, 160.0), rng.range(2.0, 4.0), rng.range(0.02, 0.05), rng.range(0.4, 0.8))];
    modes(out, start, &m, rng.range(0.4, 1.0), 1_800.0, noise);
}

/// A hollow log hit: dull and low, with a hollow low mode and one inharmonic
/// partner. Woodier and lower than a knock.
fn log(out: &mut [f64], start: usize, notes: &[f64], noise: &mut Rng, rng: &mut Rng) {
    let base = if notes.is_empty() { 55.0 } else { notes[rng.below(notes.len())] - 5.0 };
    let hz = midi_hz(base + rng.range(-1.0, 1.0)) * rng.range(0.5, 0.8);
    click(out, start, rng.range(1.0, 2.5), hz * 7.0, 1.0, 0.5, noise, rng);
    let m = [
        (hz, rng.range(3.5, 6.0), rng.range(0.03, 0.07), 1.0),
        (hz * rng.range(1.9, 2.5), rng.range(2.5, 4.5), rng.range(0.015, 0.04), rng.range(0.3, 0.6)),
    ];
    modes(out, start, &m, rng.range(0.4, 1.0), 2_200.0, noise);
}

/// A cave/water drip: a very short plink that bends upward, with a tick at the
/// impact. Kept brief and rolled off so it does not ring.
fn drip(out: &mut [f64], start: usize, notes: &[f64], noise: &mut Rng, rng: &mut Rng) {
    let base = if notes.is_empty() { 72.0 } else { notes[rng.below(notes.len())] + 12.0 };
    let hz0 = midi_hz(base) * rng.range(0.9, 1.1);
    let hz1 = hz0 * rng.range(1.4, 2.0);
    let q = rng.range(3.0, 6.0);
    let tau = rng.range(0.02, 0.045);
    let dur = (tau * 7.0 * SR) as usize;
    click(out, start, 1.0, hz1 * 2.0, 1.0, 0.2, noise, rng);
    let mut f = Svf::default();
    let mut lp = Svf::new(4_500.0, 0.7, SR);
    for i in 0..dur {
        let u = i as f64 / dur.max(1) as f64;
        if i % 16 == 0 {
            f.set(hz0 + (hz1 - hz0) * u, q, SR);
        }
        add(out, start + i, lp.process(f.process(noise.bipolar()).band).low);
    }
}

/// A natural chime: four inharmonic, damped modes — brighter and a little
/// longer than the other hits, but still short and rolled off rather than a
/// ringing bell.
fn chime(out: &mut [f64], start: usize, notes: &[f64], noise: &mut Rng, rng: &mut Rng) {
    let base = if notes.is_empty() { 79.0 } else { notes[rng.below(notes.len())] + 12.0 };
    let hz = midi_hz(base + rng.range(-1.0, 1.0));
    click(out, start, rng.range(0.8, 1.8), hz * 4.0, 1.0, 0.3, noise, rng);
    let m = [
        (hz, rng.range(4.0, 7.0), rng.range(0.09, 0.2), 1.0),
        (hz * rng.range(2.6, 3.2), rng.range(3.5, 6.0), rng.range(0.07, 0.16), rng.range(0.5, 0.8)),
        (hz * rng.range(5.0, 6.0), rng.range(3.0, 5.0), rng.range(0.05, 0.11), rng.range(0.25, 0.5)),
        (hz * rng.range(8.0, 9.8), rng.range(2.5, 4.5), rng.range(0.03, 0.08), rng.range(0.1, 0.3)),
    ];
    modes(out, start, &m, rng.range(0.4, 0.9), 4_500.0, noise);
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
