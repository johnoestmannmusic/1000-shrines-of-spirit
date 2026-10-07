//! The frozen chords: each 3-operator FM chord of the progression is rendered
//! once, analysed into an average magnitude spectrum, then resynthesised
//! forever by random-phase spectral freezing (same magnitudes, fresh random
//! phases every hop, windowed overlap-add). Chord changes are spectral
//! morphs: during a chord's last bar each new frame's power spectrum is
//! blended towards the next chord's.

use crate::fft::Fft;
use crate::harmony::Harmony;
pub use crate::fm::Algo;
use crate::fm::{Fm3, Op};
use crate::math::{cos_turns, midi_hz, sin_turns};
use crate::rng::Rng;
use crate::SAMPLE_RATE;

/// Modulator ratios to choose from. Near-integers keep the chord harmonic
/// with a slight shimmer.
const RATIOS: [f64; 7] = [0.5, 1.0, 1.0, 2.0, 2.0, 3.0, 4.003];
/// Length of the FM chord rendered for analysis.
const CHORD_SECONDS: f64 = 8.0;
/// FFT / window size. 2^15 at 48 kHz ≈ 0.68 s window: a deep, smooth smear.
pub const N: usize = 1 << 15;
/// Hop: quarter window. Hann² over 4 overlapping hops sums to a constant 1.5.
/// Must leave room for all of a frame's work units (checked in `new`).
pub const HOP: usize = N / 4;
/// Output level (RMS per channel).
const DRONE_RMS: f64 = 0.16;
/// Bins below this are left empty (sub rumble is the bass's job).
const LOW_CUT_HZ: f64 = 110.0;

/// Building one frame is split into small work units, one every
/// `UNIT_SPACING` samples through the hop, so no single moment carries a
/// whole 32k-point IFFT (that would overrun a browser audio callback).
/// Scheduled on the sample position, so output never depends on block size.
const FILL_UNITS: usize = 8;
const UNIT_SPACING: usize = 256;

pub struct Drone {
    fft: Fft,
    window: Vec<f64>,
    /// Frozen (left, right) magnitude spectra, one pair per chord.
    mags: Vec<(Vec<f64>, Vec<f64>)>,
    harmony: Harmony,
    /// The frame under construction blends chord `.0` into `.1` by `.2`.
    mix: (usize, usize, f64),
    /// Samples output so far.
    pos: u64,
    rng: Rng,
    /// Work buffers for the frame under construction.
    re: Vec<f64>,
    im: Vec<f64>,
    /// Five windowed frames per channel: four sounding, one being built.
    slots_l: Vec<Vec<f64>>,
    slots_r: Vec<Vec<f64>>,
    /// Slot indices of the sounding frames, newest first.
    sounding: [usize; 4],
    building: usize,
    /// Position within the current hop.
    offset: usize,
    /// Frames that have started sounding so far.
    frames: u64,
    recipes: Vec<Vec<Recipe>>,
}

/// One chord note's FM settings: (wiring, [op2 ratio, op3 ratio, op2 index, op3 index], op3 feedback).
pub type Recipe = (Algo, [f64; 4], f64);

fn render_chord(rng: &mut Rng, voicing: &[f64], recipes: &[Recipe]) -> Vec<f64> {
    let sr = SAMPLE_RATE as f64;
    let len = (CHORD_SECONDS * sr) as usize;
    let mut out = vec![0.0; len];
    for (note, (algo, [r2, r3, i2, i3], fb)) in voicing.iter().zip(recipes) {
        let detune = rng.range(-7.0, 7.0) / 100.0; // ± 7 cents per channel
        let mut voice = Fm3::new(
            midi_hz(note + detune),
            *algo,
            [
                Op::new(1.0, 0.0, rng.unit()),
                Op::new(*r2, *i2, rng.unit()),
                Op::new(*r3, *i3, rng.unit()),
            ],
            *fb,
        );
        // Higher notes quieter.
        let gain = 1.0 / (1.0 + (note - 45.0).max(0.0) / 18.0);
        // Slowly swaying modulation depth enriches the averaged spectrum.
        let sway_hz = rng.range(0.08, 0.3);
        let sway_phase = rng.unit();
        for (i, s) in out.iter_mut().enumerate() {
            let sway = 0.7 + 0.3 * sin_turns(sway_phase + sway_hz * i as f64 / sr);
            *s += gain * voice.next(sr, sway);
        }
    }
    out
}

fn recipes_for(rng: &mut Rng, notes: &[f64]) -> Vec<Recipe> {
    // One FM recipe per chord note, shared by both channels so they're the
    // same chord, but each channel gets its own detune and phases.
    notes
        .iter()
        .map(|_| {
            let algo = if rng.chance(0.5) { Algo::Stack } else { Algo::Pair };
            let r2 = rng.pick(&RATIOS);
            let r3 = rng.pick(&RATIOS);
            (algo, [r2, r3, rng.range(0.3, 2.2), rng.range(0.1, 1.2)], rng.range(0.0, 0.6))
        })
        .collect()
}

impl Drone {
    /// `first` must be the stream chord 1's voicing was drawn from (it goes
    /// on to choose chord 1's FM recipes, exactly as in 0009); `rest` serves
    /// the other chords.
    pub fn new(seed: u64, harmony: Harmony, mut first: Rng, mut rest: Rng) -> Self {
        let fft = Fft::new(N);
        let window: Vec<f64> = (0..N).map(|i| 0.5 - 0.5 * cos_turns(i as f64 / N as f64)).collect();
        let low_bin = (LOW_CUT_HZ * N as f64 / SAMPLE_RATE as f64) as usize;
        let mut mags = Vec::new();
        let mut all_recipes = Vec::new();
        for (i, chord) in harmony.chords.iter().enumerate() {
            let rng = if i == 0 { &mut first } else { &mut rest };
            let recipes = recipes_for(rng, &chord.notes);
            let left = render_chord(rng, &chord.notes, &recipes);
            let right = render_chord(rng, &chord.notes, &recipes);
            let mut mag_l = average_magnitudes(&fft, &window, &left);
            let mut mag_r = average_magnitudes(&fft, &window, &right);
            // Scale so the resynthesis comes out at DRONE_RMS. With an unscaled
            // inverse FFT, each frame's mean power is 2·Σ|m|² (Parseval), and four
            // overlapping Hann-windowed frames of uncorrelated noise sum to 1.5×.
            for m in [&mut mag_l, &mut mag_r] {
                for x in m[..low_bin].iter_mut() {
                    *x = 0.0;
                }
                let power: f64 = m.iter().map(|x| x * x).sum();
                let scale = DRONE_RMS / (3.0 * power).sqrt();
                for x in m.iter_mut() {
                    *x *= scale;
                }
            }
            mags.push((mag_l, mag_r));
            all_recipes.push(recipes);
        }

        assert!((FILL_UNITS + 1 + 15 + 1) * UNIT_SPACING <= HOP);
        let mut drone = Drone {
            fft,
            window,
            mags,
            harmony,
            mix: (0, 0, 0.0),
            pos: 0,
            rng: Rng::stream(seed, 0xD1),
            re: vec![0.0; N],
            im: vec![0.0; N],
            slots_l: vec![vec![0.0; N]; 5],
            slots_r: vec![vec![0.0; N]; 5],
            // Frames go in slots 0, 1, 2 (sounding) and 4 (entering at the
            // first sample); slot 3 is the one recycled first.
            sounding: [2, 1, 0, 3],
            building: 4,
            offset: 0,
            frames: 0,
            recipes: all_recipes,
        };
        // Start in the steady state: three frames already sounding, a fourth
        // built and waiting to enter at the first sample.
        for slot in [0, 1, 2, 4] {
            drone.building = slot;
            for unit in 0..drone.units() {
                drone.work(unit);
            }
        }
        drone
    }

    fn units(&self) -> usize {
        FILL_UNITS + 1 + self.fft.stages() as usize + 1
    }

    /// One slice of building the next random-phase frame for both channels.
    /// Both go through one complex IFFT: Z = X_L + i·X_R, whose real part is
    /// the left signal and imaginary part the right (both spectra Hermitian).
    fn work(&mut self, unit: usize) {
        let (re, im) = (&mut self.re, &mut self.im);
        let stages = self.fft.stages() as usize;
        if unit < FILL_UNITS {
            if unit == 0 {
                // Which chord(s) this frame belongs to, judged at its centre.
                let centre = self.pos + HOP as u64 + N as u64 / 2;
                let p = self.harmony.at(centre);
                self.mix = (p.chord, p.next, p.morph);
            }
            let (a, b, x) = self.mix;
            let per = N / 2 / FILL_UNITS;
            for k in (unit * per).max(1)..(unit + 1) * per {
                let (pl, pr) = (self.rng.unit(), self.rng.unit());
                let (ml, mr) = if x == 0.0 {
                    (self.mags[a].0[k], self.mags[a].1[k])
                } else {
                    // Blend power, not amplitude, so loudness holds steady.
                    let blend = |p: f64, q: f64| ((1.0 - x) * p * p + x * q * q).sqrt();
                    (blend(self.mags[a].0[k], self.mags[b].0[k]), blend(self.mags[a].1[k], self.mags[b].1[k]))
                };
                let (lr, li) = (ml * cos_turns(pl), ml * sin_turns(pl));
                let (rr, ri) = (mr * cos_turns(pr), mr * sin_turns(pr));
                re[k] = lr - ri;
                im[k] = li + rr;
                re[N - k] = lr + ri;
                im[N - k] = rr - li;
            }
        } else if unit == FILL_UNITS {
            re[0] = 0.0;
            im[0] = 0.0;
            re[N / 2] = 0.0;
            im[N / 2] = 0.0;
            self.fft.bit_reverse(re, im);
        } else if unit <= FILL_UNITS + stages {
            self.fft.stage(re, im, (unit - FILL_UNITS - 1) as u32, true);
        } else if unit == FILL_UNITS + stages + 1 {
            let (l, r) = (&mut self.slots_l[self.building], &mut self.slots_r[self.building]);
            for i in 0..N {
                l[i] = re[i] * self.window[i];
                r[i] = im[i] * self.window[i];
            }
        }
    }

    /// Next stereo sample.
    pub fn next(&mut self) -> (f64, f64) {
        if self.offset == 0 {
            // The finished frame starts sounding; the oldest slot is recycled.
            let freed = self.sounding[3];
            self.sounding = [self.building, self.sounding[0], self.sounding[1], self.sounding[2]];
            self.building = freed;
            self.frames += 1;
        }
        if self.offset % UNIT_SPACING == 0 {
            self.work(self.offset / UNIT_SPACING);
        }
        // Overlap-add, oldest frame first.
        let (mut l, mut r) = (0.0, 0.0);
        for age in (0..4).rev() {
            let i = age * HOP + self.offset;
            l += self.slots_l[self.sounding[age]][i];
            r += self.slots_r[self.sounding[age]][i];
        }
        self.offset = (self.offset + 1) % HOP;
        self.pos += 1;
        (l, r)
    }
}

/// Read-only views for visualisation.
impl Drone {
    /// Chord `i`'s frozen magnitude spectra (left, right), bins 0..=N/2.
    pub fn magnitudes(&self, i: usize) -> (&[f64], &[f64]) {
        (&self.mags[i].0, &self.mags[i].1)
    }

    pub fn harmony(&self) -> &Harmony {
        &self.harmony
    }

    /// Chord `i`'s FM recipes, one per note.
    pub fn recipes(&self, i: usize) -> &[Recipe] {
        &self.recipes[i]
    }

    /// Frames that have started sounding so far.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Position within the current hop, 0..HOP.
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// The newest sounding frame (left), as `points` peak values.
    pub fn newest_frame_overview(&self, points: usize) -> Vec<f32> {
        let frame = &self.slots_l[self.sounding[0]];
        frame
            .chunks(N.div_ceil(points))
            .map(|c| c.iter().fold(0.0f64, |a, x| a.max(x.abs())) as f32)
            .collect()
    }
}

/// RMS-averaged magnitude spectrum (bins 0..=N/2) over Hann-windowed frames.
fn average_magnitudes(fft: &Fft, window: &[f64], signal: &[f64]) -> Vec<f64> {
    let mut power = vec![0.0; N / 2 + 1];
    let mut re = vec![0.0; N];
    let mut im = vec![0.0; N];
    let mut frames = 0;
    let mut start = 0;
    while start + N <= signal.len() {
        for i in 0..N {
            re[i] = signal[start + i] * window[i];
            im[i] = 0.0;
        }
        fft.transform(&mut re, &mut im, false);
        for (k, p) in power.iter_mut().enumerate() {
            *p += re[k] * re[k] + im[k] * im[k];
        }
        frames += 1;
        start += HOP;
    }
    power.iter().map(|p| (p / frames as f64).sqrt()).collect()
}
