//! The frozen chord: a 3-operator FM chord is rendered once, analysed into an
//! average magnitude spectrum, then resynthesised forever by random-phase
//! spectral freezing (same magnitudes, fresh random phases every hop,
//! windowed overlap-add).

use crate::fft::Fft;
pub use crate::fm::Algo;
use crate::fm::{Fm3, Op};
use crate::math::{cos_turns, midi_hz, sin_turns};
use crate::rng::Rng;
use crate::SAMPLE_RATE;

/// Chord voicings (MIDI notes), all in D lydian. Seed 1 picks one.
pub const VOICINGS: [&[f64]; 4] = [
    &[50.0, 57.0, 64.0, 66.0, 73.0, 80.0, 83.0],
    &[50.0, 57.0, 61.0, 64.0, 68.0, 71.0, 78.0],
    &[45.0, 52.0, 57.0, 64.0, 66.0, 71.0, 73.0, 80.0],
    &[50.0, 54.0, 61.0, 64.0, 68.0, 73.0, 76.0, 85.0],
];
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
    mag_l: Vec<f64>,
    mag_r: Vec<f64>,
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
    voicing: &'static [f64],
    recipes: Vec<Recipe>,
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

impl Drone {
    pub fn new(seed: u64) -> Self {
        let mut rng = Rng::stream(seed, 0xD0);
        let voicing = VOICINGS[rng.below(VOICINGS.len())];
        // One FM recipe per chord note, shared by both channels so they're
        // the same chord, but each channel gets its own detune and phases.
        let recipes: Vec<Recipe> = voicing
            .iter()
            .map(|_| {
                let algo = if rng.chance(0.5) { Algo::Stack } else { Algo::Pair };
                let r2 = rng.pick(&RATIOS);
                let r3 = rng.pick(&RATIOS);
                (algo, [r2, r3, rng.range(0.3, 2.2), rng.range(0.1, 1.2)], rng.range(0.0, 0.6))
            })
            .collect();
        let left = render_chord(&mut rng, voicing, &recipes);
        let right = render_chord(&mut rng, voicing, &recipes);

        let fft = Fft::new(N);
        let window: Vec<f64> =
            (0..N).map(|i| 0.5 - 0.5 * cos_turns(i as f64 / N as f64)).collect();
        let mut mag_l = average_magnitudes(&fft, &window, &left);
        let mut mag_r = average_magnitudes(&fft, &window, &right);

        // Scale so the resynthesis comes out at DRONE_RMS. With an unscaled
        // inverse FFT, each frame's mean power is 2·Σ|m|² (Parseval), and four
        // overlapping Hann-windowed frames of uncorrelated noise sum to 1.5×.
        let low_bin = (LOW_CUT_HZ * N as f64 / SAMPLE_RATE as f64) as usize;
        for mags in [&mut mag_l, &mut mag_r] {
            for m in mags[..low_bin].iter_mut() {
                *m = 0.0;
            }
            let power: f64 = mags.iter().map(|m| m * m).sum();
            let scale = DRONE_RMS / (3.0 * power).sqrt();
            for m in mags.iter_mut() {
                *m *= scale;
            }
        }

        assert!((FILL_UNITS + 1 + 15 + 1) * UNIT_SPACING <= HOP);
        let mut drone = Drone {
            fft,
            window,
            mag_l,
            mag_r,
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
            voicing,
            recipes,
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
            let per = N / 2 / FILL_UNITS;
            for k in (unit * per).max(1)..(unit + 1) * per {
                let (pl, pr) = (self.rng.unit(), self.rng.unit());
                let (lr, li) = (self.mag_l[k] * cos_turns(pl), self.mag_l[k] * sin_turns(pl));
                let (rr, ri) = (self.mag_r[k] * cos_turns(pr), self.mag_r[k] * sin_turns(pr));
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
        (l, r)
    }
}

/// Read-only views for visualisation.
impl Drone {
    /// Frozen magnitude spectra (left, right), bins 0..=N/2.
    pub fn magnitudes(&self) -> (&[f64], &[f64]) {
        (&self.mag_l, &self.mag_r)
    }

    pub fn voicing(&self) -> &[f64] {
        self.voicing
    }

    pub fn recipes(&self) -> &[Recipe] {
        &self.recipes
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
