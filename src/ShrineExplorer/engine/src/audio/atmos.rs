//! Atmosphere loops: the short, evolving pad loops of 90s sample CDs and
//! game soundtracks, made the way a sampler made them.
//!
//! At start-up each chord of the progression gets its own loop, a few
//! seconds long, synthesised once by one of the timbres (formant voices, an FM
//! bell, a bell pad over a warm swell, a wave-cycle pad, or one of the noise
//! textures: Air, Hush, Gust, Shine, Rill, Veil and Mist), or one of the later
//! family: Drift, Tide, Ember, Ripple, Ebb, Bronze, Frost, Lantern and Loom. Everything that
//! moves inside the loop
//! (the vowel, the wave steps, the swells, the strikes) is timed to repeat
//! exactly once per loop. The loop point is then hidden the way a sampler
//! does it: the last moments are crossfaded into the first, so playback can
//! go round and round for ever without a seam.
//!
//! Playback reads the loop at `clock % length` and, during each chord's
//! morph bar, crossfades (equal power) into the next chord's loop at the
//! same position: the pad changes chord without ever restarting.

use crate::filter::Svf;
use crate::fm::{Algo, Fm3, Op};
use crate::harmony::Harmony;
use crate::math::{cos_turns, decay_coef, exp, exp2, midi_hz, sin_turns};
use crate::rng::Rng;
use crate::LoopDesign;

const SR: f64 = crate::SAMPLE_RATE as f64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopTimbre {
    Voices,
    Bells,
    BellPad,
    WaveCycle,
    Air,
    Hush,
    Gust,
    Shine,
    Rill,
    Veil,
    Mist,
    Drift,
    Tide,
    Ember,
    Ripple,
    Ebb,
    Bronze,
    Frost,
    Lantern,
    Loom,
    Off,
}

pub const LOOP_TIMBRES: [LoopTimbre; 21] = [
    LoopTimbre::Voices,
    LoopTimbre::Bells,
    LoopTimbre::BellPad,
    LoopTimbre::WaveCycle,
    LoopTimbre::Air,
    LoopTimbre::Hush,
    LoopTimbre::Gust,
    LoopTimbre::Shine,
    LoopTimbre::Rill,
    LoopTimbre::Veil,
    LoopTimbre::Mist,
    LoopTimbre::Drift,
    LoopTimbre::Tide,
    LoopTimbre::Ember,
    LoopTimbre::Ripple,
    LoopTimbre::Ebb,
    LoopTimbre::Bronze,
    LoopTimbre::Frost,
    LoopTimbre::Lantern,
    LoopTimbre::Loom,
    LoopTimbre::Off,
];

impl LoopTimbre {
    pub fn name(self) -> &'static str {
        match self {
            LoopTimbre::Voices => "Voices",
            LoopTimbre::Bells => "Bells",
            LoopTimbre::BellPad => "BellPad",
            LoopTimbre::WaveCycle => "Wave sequence",
            LoopTimbre::Air => "Air",
            LoopTimbre::Hush => "Hush",
            LoopTimbre::Gust => "Gust",
            LoopTimbre::Shine => "Shine",
            LoopTimbre::Rill => "Rill",
            LoopTimbre::Veil => "Veil",
            LoopTimbre::Mist => "Mist",
            LoopTimbre::Drift => "Drift",
            LoopTimbre::Tide => "Tide",
            LoopTimbre::Ember => "Ember",
            LoopTimbre::Ripple => "Ripple",
            LoopTimbre::Ebb => "Ebb",
            LoopTimbre::Bronze => "Bronze",
            LoopTimbre::Frost => "Frost",
            LoopTimbre::Lantern => "Lantern",
            LoopTimbre::Loom => "Loom",
            LoopTimbre::Off => "Off",
        }
    }

    /// A one-line sketch, for the setup screen.
    pub fn blurb(self) -> &'static str {
        match self {
            LoopTimbre::Voices => "synth voices whose vowel drifts a → o → u → e",
            LoopTimbre::Bells => "FM glass, struck softly in a slow pattern",
            LoopTimbre::BellPad => "a bright FM bell over a warm, swelling pad",
            LoopTimbre::WaveCycle => "the waveform changes every beat",
            LoopTimbre::Air => "noise tuned to the chord: airy, whispered",
            LoopTimbre::Hush => "noise shaped by gliding vowel formants: a wordless breath",
            LoopTimbre::Gust => "noise through a slowly opening low-pass: gusts and calm",
            LoopTimbre::Shine => "high, drifting resonances: air sparkling in the light",
            LoopTimbre::Rill => "narrow bands fluttering quickly: water over stones",
            LoopTimbre::Veil => "resonant bands sweeping up and back: a curtain of light",
            LoopTimbre::Mist => "a low, slowly breathing noise bed with almost no pitch",
            LoopTimbre::Drift => "detuned pulse-wave strings, slowly chorused and breathing",
            LoopTimbre::Tide => "a bright saw pad through a slowly sweeping notch: a phasing tide",
            LoopTimbre::Ember => "a dark pad pulsing through a soft rhythmic gate, in time",
            LoopTimbre::Ripple => "plucked strings arpeggiating the chord, trailing a soft dotted echo",
            LoopTimbre::Ebb => "reversed swells: each note breathes in backwards, then stops",
            LoopTimbre::Bronze => "inharmonic gong partials swelling and beating slowly",
            LoopTimbre::Frost => "tiny high grains scattered over the chord: crystalline frost",
            LoopTimbre::Lantern => "a breathy flute-organ with a chiff on each note and slow tremolo",
            LoopTimbre::Loom => "a soft square-wave arpeggio stepping through an opening filter",
            LoopTimbre::Off => "no loop layer",
        }
    }

    /// Three-letter code used in recipes.
    pub fn code(self) -> &'static str {
        match self {
            LoopTimbre::Voices => "VOX",
            LoopTimbre::Bells => "BEL",
            LoopTimbre::BellPad => "BLP",
            LoopTimbre::WaveCycle => "WVC",
            LoopTimbre::Air => "AIR",
            LoopTimbre::Hush => "HSH",
            LoopTimbre::Gust => "GST",
            LoopTimbre::Shine => "SHN",
            LoopTimbre::Rill => "RIL",
            LoopTimbre::Veil => "VEL",
            LoopTimbre::Mist => "MST",
            LoopTimbre::Drift => "DRF",
            LoopTimbre::Tide => "TDE",
            LoopTimbre::Ember => "EMB",
            LoopTimbre::Ripple => "RPL",
            LoopTimbre::Ebb => "EBB",
            LoopTimbre::Bronze => "BRZ",
            LoopTimbre::Frost => "FRS",
            LoopTimbre::Lantern => "LNT",
            LoopTimbre::Loom => "LOM",
            LoopTimbre::Off => "OFF",
        }
    }

    pub fn from_code(code: &str) -> Option<LoopTimbre> {
        LOOP_TIMBRES.iter().copied().find(|t| t.code().eq_ignore_ascii_case(code))
    }

    pub fn index(self) -> usize {
        LOOP_TIMBRES.iter().position(|t| *t == self).unwrap_or(0)
    }
}

/// Loops are 3 to 8 seconds long, in whole half-time beats.
pub const MIN_SECONDS: u64 = 3;
pub const MAX_SECONDS: u64 = 8;
/// The loop-point crossfade: 150 ms.
pub const XFADE: usize = 7_200;
/// Each chord's loop is scaled to this RMS (per channel).
const LOOP_RMS: f64 = 0.1;
/// Coefficients of the moving filters are refreshed this often (samples).
const CONTROL: usize = 32;

/// How many beats a loop lasts: as close to `target_s` seconds as whole beats
/// allow, kept within 3-8 s. If `avoid` is given, the count is nudged so it
/// shares no factor with it, and the two loops drift against each other.
pub fn loop_beats(beat: u64, target_s: u64, avoid: Option<u64>) -> u64 {
    let sr = crate::SAMPLE_RATE as u64;
    let lo = (MIN_SECONDS * sr).div_ceil(beat).max(1);
    let hi = (MAX_SECONDS * sr / beat).max(lo);
    let n = ((target_s * sr + beat / 2) / beat).clamp(lo, hi);
    let Some(other) = avoid else { return n };
    let coprime = |m: u64| gcd(m, other) == 1;
    [n, n - 1, n + 1, n - 2, n + 2]
        .into_iter()
        .find(|m| (lo..=hi).contains(m) && coprime(*m))
        .unwrap_or(n)
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// What happens in one chord's loop: chosen once, then played by both channels
/// (each with its own detune and phases), like the drone's chord recipes.
#[derive(Clone, Debug)]
pub struct Performance {
    /// MIDI notes, low to high.
    pub notes: Vec<f64>,
    /// Struck timbres: (note index, start within the loop in samples, velocity).
    pub strikes: Vec<(usize, usize, f64)>,
    /// Wave sequence: one single-cycle wave per beat, as harmonic amplitudes.
    pub waves: Vec<[f64; HARMONICS]>,
    /// Where in its cycle the slow in-loop movement starts (turns).
    pub offset: f64,
    /// Keep the FM bells' overtones whole-number (2-step garage).
    pub harmonic: bool,
    /// 2-step garage: the swung sixteenth grid (length, push), so scattered
    /// grains land on it. (0, 0) leaves them free.
    pub grid: (usize, usize),
}

pub const HARMONICS: usize = 12;
const TABLE: usize = 2048;

/// Chord tones moved into the loop register (MIDI 60-83), none closer than a
/// whole tone to another, `count` of them chosen at random.
fn pick_notes(rng: &mut Rng, chord: &[f64], count: usize) -> Vec<f64> {
    let mut pool: Vec<f64> = chord
        .iter()
        .map(|n| {
            let mut n = *n;
            while n < 60.0 {
                n += 12.0;
            }
            while n >= 84.0 {
                n -= 12.0;
            }
            n
        })
        .collect();
    pool.sort_by(|a, b| a.partial_cmp(b).unwrap());
    pool.dedup();
    let mut out: Vec<f64> = Vec::new();
    while out.len() < count && !pool.is_empty() {
        let n = pool.remove(rng.below(pool.len()));
        if out.iter().all(|m| (m - n).abs() >= 2.0) {
            out.push(n);
        }
    }
    out.sort_by(|a, b| a.partial_cmp(b).unwrap());
    out
}

fn perform(timbre: LoopTimbre, rng: &mut Rng, chord: &[f64], beats: u64, beat: u64) -> Performance {
    let count = match timbre {
        LoopTimbre::BellPad | LoopTimbre::Shine | LoopTimbre::Veil | LoopTimbre::Ripple | LoopTimbre::Loom => 4,
        LoopTimbre::Mist => 1,
        LoopTimbre::Bronze => 2,
        _ => 3,
    };
    let notes = pick_notes(rng, chord, count);
    let mut strikes = Vec::new();
    match timbre {
        LoopTimbre::Bells => {
            // Each note struck once or twice, on beats or half-beats; the loop
            // always opens with a strike.
            for i in 0..notes.len() {
                for _ in 0..rng.int(1, 2) {
                    let at = rng.below(beats as usize * 2) * beat as usize / 2;
                    strikes.push((i, at, rng.range(0.55, 1.0)));
                }
            }
            if !strikes.iter().any(|s| s.1 == 0) {
                strikes[0].1 = 0;
            }
        }
        LoopTimbre::BellPad => {
            // One soft strum of the whole chord at the top of the loop, and a
            // quieter echo of one note half way through.
            for i in 0..notes.len() {
                strikes.push((i, i * 1_440, 0.9 - 0.1 * i as f64));
            }
            let half = (beats as usize / 2) * beat as usize;
            strikes.push((rng.below(notes.len()), half, 0.45));
        }
        LoopTimbre::Ember => {
            // A gate on every eighth: the downbeats always open, the rest by
            // chance, so the pad pulses in a pattern that repeats each loop.
            let step = beat as usize / 2;
            for s in 0..beats as usize * 2 {
                if s == 0 || s % 4 == 0 || rng.chance(0.62) {
                    let vel = if s % 4 == 0 { 1.0 } else { rng.range(0.45, 0.85) };
                    strikes.push((0, s * step, vel));
                }
            }
        }
        LoopTimbre::Ripple | LoopTimbre::Loom => {
            // An arpeggio on eighths, up then down the chord; Loom also leaps
            // an octave (an index past the last note means "an octave up").
            let n = notes.len().max(1);
            let span = if timbre == LoopTimbre::Loom { 2 * n } else { n };
            let mut order: Vec<usize> = (0..span).collect();
            order.extend((1..span.saturating_sub(1)).rev());
            let shift = rng.below(order.len());
            let step = beat as usize / 2;
            for s in 0..beats as usize * 2 {
                if s > 0 && rng.chance(0.18) {
                    continue;
                }
                let idx = order[(s + shift) % order.len()];
                let vel = if s % 2 == 0 { rng.range(0.8, 1.0) } else { rng.range(0.5, 0.8) };
                strikes.push((idx, s * step, vel));
            }
        }
        LoopTimbre::Ebb => {
            // Each note swells in backwards to a cut on a beat.
            for i in 0..notes.len() {
                let end = (1 + rng.below(beats as usize)) * beat as usize;
                strikes.push((i, end, rng.range(0.6, 1.0)));
            }
        }
        LoopTimbre::Lantern => {
            // Each note is re-tongued once or twice per loop, on a beat.
            for i in 0..notes.len() {
                for _ in 0..rng.int(1, 2) {
                    strikes.push((i, rng.below(beats as usize) * beat as usize, rng.range(0.6, 1.0)));
                }
            }
        }
        _ => {}
    }
    let waves = if timbre == LoopTimbre::WaveCycle {
        (0..beats)
            .map(|_| {
                let odd_only = rng.chance(0.3);
                // Roll-off: 1/√n (bright), 1/n (saw-like) or 1/n^1.5 (mellow).
                let tilt = rng.below(3);
                let mut w = [0.0; HARMONICS];
                for (h, a) in w.iter_mut().enumerate() {
                    let n = (h + 1) as f64;
                    if odd_only && (h + 1) % 2 == 0 {
                        continue;
                    }
                    let r = rng.unit();
                    let rolloff = match tilt {
                        0 => 1.0 / n.sqrt(),
                        1 => 1.0 / n,
                        _ => 1.0 / (n * n.sqrt()),
                    };
                    *a = r * r * rolloff;
                }
                w[0] = w[0].max(0.3);
                w
            })
            .collect()
    } else {
        Vec::new()
    };
    Performance { notes, strikes, waves, offset: rng.unit(), harmonic: false, grid: (0, 0) }
}

/// An event layer's per-chord settings: a few chord tones for the Knock grains
/// to strike, and a phase offset. The grid itself is the design.
fn perform_events(rng: &mut Rng, chord: &[f64]) -> Performance {
    let notes = pick_notes(rng, chord, 3);
    Performance { notes, strikes: Vec::new(), waves: Vec::new(), offset: rng.unit(), harmonic: false, grid: (0, 0) }
}

/// Band-limited sawtooth (polyBLEP), phase in turns, `dt` = increment per sample.
fn saw(phase: f64, dt: f64) -> f64 {
    let blep = if phase < dt {
        let x = phase / dt;
        x + x - x * x - 1.0
    } else if phase > 1.0 - dt {
        let x = (phase - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    };
    2.0 * phase - 1.0 - blep
}

/// Band-limited pulse of width `pw` (0..1): the difference of two saws.
fn pulse(phase: f64, pw: f64, dt: f64) -> f64 {
    let other = phase + pw;
    saw(phase, dt) - saw(other - other.floor(), dt)
}

fn advance(phase: &mut f64, dt: f64) {
    *phase += dt;
    *phase -= phase.floor();
}

/// Vowel formants (Hz) for "ah", "oh", "oo", "eh", visited in turn once per loop.
const VOWELS: [[f64; 3]; 4] = [[730.0, 1090.0, 2440.0], [570.0, 840.0, 2410.0], [300.0, 870.0, 2240.0], [530.0, 1840.0, 2480.0]];
const FORMANT_GAIN: [f64; 3] = [1.0, 0.6, 0.35];
const FORMANT_Q: [f64; 3] = [8.0, 10.0, 12.0];

/// The formants at `p` turns through the loop.
fn vowel_at(p: f64) -> [f64; 3] {
    let x = (p - p.floor()) * VOWELS.len() as f64;
    let i = x as usize % VOWELS.len();
    let j = (i + 1) % VOWELS.len();
    let f = x - x.floor();
    let s = 0.5 - 0.5 * cos_turns(0.5 * f); // eased
    [0, 1, 2].map(|k| VOWELS[i][k] + (VOWELS[j][k] - VOWELS[i][k]) * s)
}

/// One channel of one chord's loop, rendered over `total` samples: a full
/// warm-up loop (so tails and filters have settled), then the loop itself,
/// then the crossfade region.
fn render_channel(timbre: LoopTimbre, perf: &Performance, rng: &mut Rng, len: usize, beat: usize, total: usize) -> Vec<f64> {
    let mut out = vec![0.0; total];
    let turns = |i: usize| (i % len) as f64 / len as f64 + perf.offset;
    match timbre {
        LoopTimbre::Voices => {
            let mut src = vec![0.0; total];
            for note in &perf.notes {
                for d in [-7.0, 0.0, 7.0] {
                    let cents = d + rng.range(-2.5, 2.5);
                    let hz = midi_hz(note + cents / 100.0);
                    let mut ph = rng.unit();
                    let (mut vib, vib_hz) = (rng.unit(), rng.range(4.6, 5.6));
                    for s in src.iter_mut() {
                        let dt = hz * (1.0 + 0.0045 * sin_turns(vib)) / SR;
                        *s += saw(ph, dt);
                        advance(&mut ph, dt);
                        advance(&mut vib, vib_hz / SR);
                    }
                }
            }
            let mut noise = Rng::new(rng.next_u64());
            let mut formants = [Svf::default(), Svf::default(), Svf::default()];
            for (i, (o, s)) in out.iter_mut().zip(&src).enumerate() {
                if i % CONTROL == 0 {
                    let f = vowel_at(turns(i));
                    for k in 0..3 {
                        formants[k].set(f[k], FORMANT_Q[k], SR);
                    }
                }
                let x = s + 0.6 * noise.bipolar();
                let mut y = 0.0;
                for k in 0..3 {
                    y += FORMANT_GAIN[k] * formants[k].process(x).band / FORMANT_Q[k];
                }
                *o = y * (0.75 + 0.25 * sin_turns(turns(i)));
            }
        }
        LoopTimbre::Bells | LoopTimbre::BellPad => {
            let glass = timbre == LoopTimbre::Bells;
            let detune: Vec<f64> = perf.notes.iter().map(|_| rng.range(-4.0, 4.0) / 100.0).collect();
            let (r2, r3, decay) = if glass {
                // Glass: inharmonic 3.5 and 7.07, or (in garage, against
                // plain chords) the nearest whole-number ratios.
                let (a, b) = if perf.harmonic { (3.0, 4.0) } else { (3.5, 7.07) };
                (a, b, rng.range(1.6, 2.6))
            } else {
                (3.0, 1.0, rng.range(0.7, 1.1))
            };
            let ring = (4.0 * SR) as usize;
            for &(n, at, vel) in &perf.strikes {
                let hz = midi_hz(perf.notes[n] + detune[n]);
                // The same strike in every pass, so the loop repeats exactly.
                let phases = [rng.unit(), rng.unit(), rng.unit()];
                let mut start = at;
                while start < total {
                    let mut v = Fm3::new(
                        hz,
                        if glass { Algo::Pair } else { Algo::Stack },
                        [Op::new(1.0, 0.0, phases[0]), Op::new(r2, 1.5, phases[1]), Op::new(r3, 0.7, phases[2])],
                        if glass { 0.0 } else { 0.3 },
                    );
                    let (c_amp, c_bright) = (decay_coef(decay, SR), decay_coef(decay * 0.35, SR));
                    let (mut amp, mut bright) = (vel, 1.0);
                    for o in out[start..(start + ring).min(total)].iter_mut() {
                        *o += 0.5 * amp * v.next(SR, 0.25 + 0.75 * bright);
                        amp *= c_amp;
                        bright *= c_bright;
                    }
                    start += len;
                }
            }
            // A quiet sustained bed underneath: sines for glass, a warm saw pad for BellPad.
            let mut bed = vec![0.0; total];
            for (k, note) in perf.notes.iter().enumerate() {
                let voices: &[f64] = if glass { &[0.0] } else { &[-9.0, 9.0] };
                for d in voices {
                    let dt = midi_hz(note + (d + rng.range(-2.0, 2.0)) / 100.0) / SR;
                    let mut ph = rng.unit();
                    let swell_at = k as f64 / perf.notes.len() as f64;
                    for (i, b) in bed.iter_mut().enumerate() {
                        let swell = 0.5 + 0.5 * sin_turns(turns(i) + swell_at);
                        *b += swell * if glass { sin_turns(ph) } else { saw(ph, dt) };
                        advance(&mut ph, dt);
                    }
                }
            }
            let mut lp = Svf::default();
            for (i, (o, b)) in out.iter_mut().zip(&bed).enumerate() {
                if i % CONTROL == 0 {
                    lp.set(1_000.0 + 1_400.0 * (0.5 + 0.5 * sin_turns(turns(i))), 0.8, SR);
                }
                *o += if glass { 0.12 * b } else { 0.35 * lp.process(*b).low };
            }
        }
        LoopTimbre::WaveCycle => {
            // One single-cycle table per beat, played through as a chord.
            let tables: Vec<Vec<f64>> = perf
                .waves
                .iter()
                .map(|w| {
                    let mut t: Vec<f64> = (0..TABLE)
                        .map(|i| {
                            let p = i as f64 / TABLE as f64;
                            w.iter().enumerate().map(|(h, a)| a * sin_turns((h + 1) as f64 * p)).sum::<f64>()
                        })
                        .collect();
                    let peak = t.iter().fold(0.0f64, |m, x| m.max(x.abs())).max(1e-9);
                    t.iter_mut().for_each(|x| *x /= peak);
                    t
                })
                .collect();
            let steps = tables.len();
            let read = |t: &[f64], ph: f64| {
                let x = ph * TABLE as f64;
                let i = x as usize % TABLE;
                let f = x - x.floor();
                t[i] + (t[(i + 1) % TABLE] - t[i]) * f
            };
            let blend = beat / 4;
            for note in &perf.notes {
                let dt = midi_hz(note + rng.range(-5.0, 5.0) / 100.0) / SR;
                let mut ph = rng.unit();
                for (i, o) in out.iter_mut().enumerate() {
                    let pos = i % len;
                    let (step, into) = (pos / beat % steps, pos % beat);
                    let a = read(&tables[step], ph);
                    // Crossfade into the next wave over the last quarter of each beat.
                    let x = if into + blend >= beat {
                        let f = (into + blend - beat) as f64 / blend as f64;
                        let b = read(&tables[(step + 1) % steps], ph);
                        a + (b - a) * (0.5 - 0.5 * cos_turns(0.5 * f))
                    } else {
                        a
                    };
                    // Each step speaks with a soft accent, like a sequenced patch.
                    let accent = 0.75 + 0.25 * exp(-(into as f64) / (0.25 * beat as f64));
                    *o += x * accent;
                    advance(&mut ph, dt);
                }
            }
            let mut lp = Svf::new(5_000.0, 0.7, SR);
            out.iter_mut().for_each(|o| *o = lp.process(*o).low);
        }
        LoopTimbre::Air => {
            let mut noise = Rng::new(rng.next_u64());
            let mut bands: Vec<(Svf, Svf, f64)> = perf
                .notes
                .iter()
                .enumerate()
                .map(|(k, n)| {
                    let hz = midi_hz(*n + rng.range(-3.0, 3.0) / 100.0);
                    (Svf::new(hz, 28.0, SR), Svf::new(2.0 * hz, 22.0, SR), k as f64 / perf.notes.len() as f64)
                })
                .collect();
            let mut air = Svf::new(2_600.0, 1.2, SR);
            for (i, o) in out.iter_mut().enumerate() {
                let x = noise.bipolar();
                let mut y = 0.04 * air.process(x).band;
                for (lo, hi, at) in bands.iter_mut() {
                    let swell = 0.5 + 0.5 * sin_turns(turns(i) + *at);
                    y += swell * (lo.process(x).band + 0.5 * hi.process(x).band) / 28.0;
                }
                *o = y;
            }
        }
        LoopTimbre::Hush => {
            // Noise blown through the same three vocal formants as Voices,
            // but the formants glide through the vowel cycle on their own: a
            // wordless, breathy "shh" rather than a sung pitch.
            let mut noise = Rng::new(rng.next_u64());
            let mut formants = [Svf::default(), Svf::default(), Svf::default()];
            let mut air = Svf::new(3_200.0, 0.9, SR);
            for (i, o) in out.iter_mut().enumerate() {
                if i % CONTROL == 0 {
                    let f = vowel_at(turns(i));
                    for k in 0..3 {
                        formants[k].set(f[k], FORMANT_Q[k], SR);
                    }
                }
                let x = noise.bipolar();
                let mut y = 0.0;
                for k in 0..3 {
                    y += FORMANT_GAIN[k] * formants[k].process(x).band / FORMANT_Q[k];
                }
                y += 0.18 * air.process(x).band;
                *o = y * (0.7 + 0.3 * sin_turns(turns(i)));
            }
        }
        LoopTimbre::Gust => {
            // One wide low-pass gusting open and shut, plus a soft resonance on
            // the lowest chord note so the wind still belongs to the harmony.
            let mut noise = Rng::new(rng.next_u64());
            let mut lp = Svf::default();
            let mut res = Svf::new(perf.notes.first().map_or(220.0, |n| midi_hz(*n)), 3.2, SR);
            for (i, o) in out.iter_mut().enumerate() {
                let p = turns(i);
                if i % CONTROL == 0 {
                    // Two gusts of unequal length per loop, so calm and squall alternate.
                    let gust = 0.5 + 0.5 * sin_turns(p) * sin_turns(2.0 * p + 0.25);
                    lp.set(250.0 + 1_550.0 * gust, 0.8, SR);
                }
                let x = noise.bipolar();
                let air = 0.55 + 0.45 * (0.5 + 0.5 * sin_turns(p));
                *o = (lp.process(x).low + 0.6 * res.process(x).band) * air;
            }
        }
        LoopTimbre::Shine => {
            // Narrow high resonances on chord notes two octaves up, each lit by
            // its own slow tremolo: air catching the light.
            let mut noise = Rng::new(rng.next_u64());
            let mut bands: Vec<(Svf, f64, u64)> = perf
                .notes
                .iter()
                .map(|n| {
                    let hz = midi_hz(*n + 24.0 + rng.range(-5.0, 5.0) / 100.0);
                    (Svf::new(hz, 18.0, SR), rng.unit(), rng.int(1, 3))
                })
                .collect();
            let mut air = Svf::new(5_200.0, 0.7, SR);
            for (i, o) in out.iter_mut().enumerate() {
                let x = noise.bipolar();
                let mut y = 0.25 * air.process(x).band;
                for (f, phase, rate) in bands.iter_mut() {
                    let trem = 0.5 + 0.5 * sin_turns(turns(i) * *rate as f64 + *phase);
                    y += trem * f.process(x).band / 18.0;
                }
                *o = y;
            }
        }
        LoopTimbre::Rill => {
            // Very narrow bands at the chord notes, each fluttering at its own
            // fast but whole-number rate: water running over stones.
            let mut noise = Rng::new(rng.next_u64());
            let mut bands: Vec<(Svf, f64, u64)> = perf
                .notes
                .iter()
                .map(|n| {
                    let hz = midi_hz(*n + rng.range(-6.0, 6.0) / 100.0);
                    (Svf::new(hz, 24.0, SR), rng.unit(), rng.int(6, 11))
                })
                .collect();
            let mut mid = Svf::new(1_500.0, 1.2, SR);
            for (i, o) in out.iter_mut().enumerate() {
                let x = noise.bipolar();
                let mut y = 0.12 * mid.process(x).band;
                for (f, phase, rate) in bands.iter_mut() {
                    let flutter = 0.5 + 0.5 * sin_turns(turns(i) * *rate as f64 + *phase);
                    y += flutter * f.process(x).band / 24.0;
                }
                *o = y;
            }
        }
        LoopTimbre::Veil => {
            // Four resonant bands that sweep a whole octave up and back down
            // across the loop, each at its own phase: a curtain of light that
            // opens and closes, still anchored to the chord.
            let mut noise = Rng::new(rng.next_u64());
            let mut bands: Vec<(Svf, f64, f64)> = perf
                .notes
                .iter()
                .enumerate()
                .map(|(k, n)| (Svf::default(), *n + rng.range(-4.0, 4.0) / 100.0, k as f64 / perf.notes.len().max(1) as f64))
                .collect();
            let mut air = Svf::new(4_000.0, 0.7, SR);
            for (i, o) in out.iter_mut().enumerate() {
                let p = turns(i);
                if i % CONTROL == 0 {
                    for (f, base, at) in bands.iter_mut() {
                        let sweep = 0.5 - 0.5 * cos_turns(p + *at);
                        f.set(midi_hz(*base + 12.0 * sweep), 20.0, SR);
                    }
                }
                let x = noise.bipolar();
                let mut y = 0.22 * air.process(x).band;
                for (f, _, at) in bands.iter_mut() {
                    let sweep = 0.5 - 0.5 * cos_turns(p + *at);
                    y += (0.35 + 0.65 * sweep) * f.process(x).band / 20.0;
                }
                *o = y;
            }
        }
        LoopTimbre::Mist => {
            // A steep low-pass breathing open and shut over one low resonance:
            // a near-pitchless bed, the quietest of the noise family.
            let mut noise = Rng::new(rng.next_u64());
            let mut lp = Svf::default();
            let mut res = Svf::new(perf.notes.first().map_or(110.0, |n| midi_hz(*n)), 1.4, SR);
            for (i, o) in out.iter_mut().enumerate() {
                let p = turns(i);
                let breath = 0.5 - 0.5 * cos_turns(p);
                if i % CONTROL == 0 {
                    lp.set(200.0 + 600.0 * breath, 0.7, SR);
                }
                let x = noise.bipolar();
                *o = (1.4 * lp.process(x).low + 0.8 * res.process(x).band) * (0.5 + 0.5 * breath);
            }
        }
        LoopTimbre::Drift => {
            // Three pulse waves per note, each narrowing and widening at its
            // own whole number of cycles per loop (the "chorus"), through a
            // soft low-pass that breathes once per loop.
            for note in &perf.notes {
                for d in [-9.0, 0.0, 9.0] {
                    let hz = midi_hz(note + (d + rng.range(-2.0, 2.0)) / 100.0);
                    let (mut ph, pwm_at, pwm_rate) = (rng.unit(), rng.unit(), rng.int(1, 3) as f64);
                    let (ens_at, ens_rate) = (rng.unit(), (len as f64 / SR * rng.range(2.2, 3.4)).round().max(1.0));
                    for (i, o) in out.iter_mut().enumerate() {
                        let p = turns(i);
                        let dt = hz * (1.0 + 0.0028 * sin_turns(p * ens_rate + ens_at)) / SR;
                        let pw = 0.5 + 0.38 * sin_turns(p * pwm_rate + pwm_at);
                        *o += pulse(ph, pw, dt);
                        advance(&mut ph, dt);
                    }
                }
            }
            let mut lp = Svf::default();
            for (i, o) in out.iter_mut().enumerate() {
                if i % CONTROL == 0 {
                    lp.set(1_100.0 + 1_300.0 * (0.5 - 0.5 * cos_turns(turns(i))), 0.75, SR);
                }
                *o = lp.process(*o).low;
            }
        }
        LoopTimbre::Tide => {
            // Two bright saws per note through four all-pass stages whose
            // corner sweeps up and back once per loop; mixed with the dry
            // signal, the moving notches roll through the pad like a tide.
            for note in &perf.notes {
                for d in [-6.0, 6.0] {
                    let dt = midi_hz(note + (d + rng.range(-2.0, 2.0)) / 100.0) / SR;
                    let mut ph = rng.unit();
                    for o in out.iter_mut() {
                        *o += saw(ph, dt);
                        advance(&mut ph, dt);
                    }
                }
            }
            let mut stages = [(0.0f64, 0.0f64); 4];
            let (mut coef, mut fb) = (0.0, 0.0);
            let mut lp = Svf::new(4_200.0, 0.7, SR);
            for (i, o) in out.iter_mut().enumerate() {
                if i % CONTROL == 0 {
                    let sweep = 0.5 - 0.5 * cos_turns(turns(i));
                    let g = crate::math::tan_turns(0.5 * 260.0 * exp2(3.6 * sweep) / SR);
                    coef = (g - 1.0) / (g + 1.0);
                }
                let dry = *o;
                let mut x = dry + 0.55 * fb;
                for (x1, y1) in stages.iter_mut() {
                    let y = coef * x + *x1 - coef * *y1;
                    *x1 = x;
                    *y1 = y;
                    x = y;
                }
                fb = x;
                *o = lp.process(0.5 * (dry + x)).low;
            }
        }
        LoopTimbre::Ember => {
            // A dark, slowly opening saw pad, opened and closed by the gate
            // pattern in the strikes. The gate is smoothed so it never clicks.
            let mut pad = vec![0.0; total];
            for note in &perf.notes {
                for d in [-8.0, 8.0] {
                    let dt = midi_hz(note - 12.0 + (d + rng.range(-2.0, 2.0)) / 100.0) / SR;
                    let mut ph = rng.unit();
                    for p in pad.iter_mut() {
                        *p += saw(ph, dt);
                        advance(&mut ph, dt);
                    }
                }
            }
            let mut gate = vec![0.0; len];
            let open = beat / 2 * 7 / 10;
            for &(_, at, vel) in &perf.strikes {
                for g in gate[at..(at + open).min(len)].iter_mut() {
                    *g = vel;
                }
            }
            let (mut env, coef) = (0.0, 1.0 - decay_coef(0.006, SR));
            let mut lp = Svf::default();
            for (i, (o, p)) in out.iter_mut().zip(&pad).enumerate() {
                if i % CONTROL == 0 {
                    lp.set(520.0 + 520.0 * (0.5 - 0.5 * cos_turns(turns(i))) + 600.0 * env, 1.1, SR);
                }
                env += (gate[i % len] - env) * coef;
                *o = lp.process(*p).low * env;
            }
        }
        LoopTimbre::Ripple => {
            // Karplus-Strong plucks: a burst of noise recirculating through a
            // gently damped delay of one period. Each pluck is the same burst
            // in every pass, so the loop repeats exactly.
            let ring = (1.6 * SR) as usize;
            for &(n, at, vel) in &perf.strikes {
                let hz = midi_hz(perf.notes[n % perf.notes.len()] - 12.0 + rng.range(-3.0, 3.0) / 100.0);
                let period = ((SR / hz) - 0.5).round().max(2.0) as usize;
                let burst: Vec<f64> = (0..period).map(|_| rng.bipolar() * vel).collect();
                let mut start = at;
                while start < total {
                    let mut buf = burst.clone();
                    let mut k = 0;
                    for o in out[start..(start + ring).min(total)].iter_mut() {
                        let (a, b) = (buf[k], buf[(k + 1) % period]);
                        buf[k] = 0.5 * (a + b) * 0.996;
                        *o += a;
                        k = (k + 1) % period;
                    }
                    start += len;
                }
            }
            // A dotted-eighth echo, darkened on each repeat.
            let d = beat * 3 / 4;
            let mut damp = crate::filter::OnePole::new(2_400.0, SR);
            for i in d..total {
                let back = damp.process(out[i - d]);
                out[i] += 0.42 * back;
            }
        }
        LoopTimbre::Ebb => {
            // A soft FM tone per note whose envelope is a decay played
            // backwards: it swells in, brightening, and is cut on the beat.
            for &(n, end, vel) in &perf.strikes {
                let hz = midi_hz(perf.notes[n] + rng.range(-4.0, 4.0) / 100.0);
                let length = ((1.5 + 1.5 * vel) * beat as f64) as usize;
                let tau = length as f64 / 4.0;
                let tail = (0.015 * SR) as usize;
                let mut stop = end;
                while stop < total + length {
                    let begin = stop.saturating_sub(length);
                    let mut v = crate::fm::Fm2::new(hz, 2.0);
                    for t in begin..(stop + tail).min(total) {
                        let env = if t < stop {
                            exp(-((stop - t) as f64) / tau)
                        } else {
                            0.5 + 0.5 * cos_turns(0.5 * (t - stop) as f64 / tail as f64)
                        };
                        out[t] += vel * env * v.next(SR, 0.2 + 1.6 * env);
                    }
                    stop += len;
                }
            }
        }
        LoopTimbre::Bronze => {
            // Inharmonic partials of a struck plate, an octave down; each is
            // paired with a twin a whole number of beats-per-loop away, so
            // the pair beats slowly and still repeats with the loop.
            const PARTIALS: [(f64, f64); 6] = [(1.0, 1.0), (1.48, 0.7), (2.09, 0.55), (2.57, 0.4), (3.14, 0.3), (4.21, 0.18)];
            let loop_hz = SR / len as f64;
            for note in &perf.notes {
                let base = midi_hz(note - 12.0);
                for (k, &(ratio, amp)) in PARTIALS.iter().enumerate() {
                    let beat_hz = loop_hz * rng.int(1, 4) as f64;
                    let swell_at = rng.unit();
                    let rate = 1.0 + (k % 2) as f64;
                    for twin in [0.0, beat_hz] {
                        // A rotating phasor: one complex multiply per sample
                        // instead of a sine (only + - *, so still exact).
                        let dt = (base * ratio + twin) / SR;
                        let start = rng.unit();
                        let (mut re, mut im) = (cos_turns(start), sin_turns(start));
                        let (c, sn) = (cos_turns(dt), sin_turns(dt));
                        let mut swell = 0.0;
                        for (i, o) in out.iter_mut().enumerate() {
                            if i % CONTROL == 0 {
                                swell = 0.25 + 0.75 * (0.5 - 0.5 * cos_turns(turns(i) * rate + swell_at));
                            }
                            *o += amp * swell * im;
                            (re, im) = (re * c - im * sn, re * sn + im * c);
                        }
                    }
                }
            }
        }
        LoopTimbre::Frost => {
            // Short windowed sine grains, two and three octaves above the
            // chord, scattered differently in each channel; the same grains
            // in every pass. A faint high air sits under them.
            let grains = (len as f64 / SR * 16.0) as usize;
            for _ in 0..grains {
                let note = perf.notes[rng.below(perf.notes.len())] + if rng.chance(0.6) { 24.0 } else { 36.0 };
                let dt = midi_hz(note.min(108.0)) / SR;
                let dur = (rng.range(0.02, 0.07) * SR) as usize;
                let (amp, phase) = (rng.range(0.3, 1.0), rng.unit());
                let mut start = rng.below(len);
                if perf.grid.0 > 0 {
                    // Garage: onto the swung sixteenth grid.
                    let (six, push) = perf.grid;
                    let k = start / six;
                    start = (k * six + if k % 2 == 1 { push } else { 0 }) % len;
                }
                while start < total {
                    let mut ph = phase;
                    for (k, o) in out[start..(start + dur).min(total)].iter_mut().enumerate() {
                        let w = 0.5 - 0.5 * cos_turns(k as f64 / dur as f64);
                        *o += amp * w * sin_turns(ph);
                        advance(&mut ph, dt);
                    }
                    start += len;
                }
            }
            let mut noise = Rng::new(rng.next_u64());
            let mut air = Svf::new(7_000.0, 0.8, SR);
            for o in out.iter_mut() {
                *o += 0.05 * air.process(noise.bipolar()).band;
            }
        }
        LoopTimbre::Lantern => {
            // A flute-organ: a sine with a little second and third harmonic,
            // breath tuned to the note, a whole-number tremolo, and a chiff
            // (a short bright puff) each time the note is re-tongued.
            let mut noise = Rng::new(rng.next_u64());
            let trem = (len as f64 / SR * 4.8).round().max(1.0);
            for (n, note) in perf.notes.iter().enumerate() {
                let hz = midi_hz(note + rng.range(-3.0, 3.0) / 100.0);
                let dt = hz / SR;
                let mut ph = rng.unit();
                let trem_at = rng.unit();
                let mut breath = Svf::new(2.0 * hz, 6.0, SR);
                let mut chiff = Svf::new(4.0 * hz, 3.0, SR);
                let tongues: Vec<usize> = perf.strikes.iter().filter(|s| s.0 == n).map(|s| s.1).collect();
                for (i, o) in out.iter_mut().enumerate() {
                    let pos = i % len;
                    // Samples since the most recent tonguing, wrapping round the loop.
                    let since = tongues.iter().map(|&at| (pos + len - at) % len).min().unwrap_or(len) as f64;
                    let attack = 1.0 - exp(-since / (0.09 * SR));
                    let puff = exp(-since / (0.03 * SR));
                    let x = noise.bipolar();
                    let tone = sin_turns(ph) + 0.28 * sin_turns(2.0 * ph) + 0.1 * sin_turns(3.0 * ph);
                    let t = 0.82 + 0.18 * sin_turns(turns(i) * trem + trem_at);
                    *o += t * (0.35 + 0.65 * attack) * (tone + 0.9 * breath.process(x).band) + 2.5 * puff * chiff.process(x).band;
                    advance(&mut ph, dt);
                }
            }
        }
        LoopTimbre::Loom => {
            // Square-wave steps, each with its own quick envelope and a
            // resonant low-pass that snaps open with the step and closes.
            let n = perf.notes.len().max(1);
            let step = beat / 2;
            let mut lp = Svf::default();
            let mut voice: Vec<(usize, f64, f64)> = vec![(0, 0.0, 0.0); len];
            for &(idx, at, vel) in &perf.strikes {
                let hz = midi_hz(perf.notes[idx % n] + 12.0 * (idx / n) as f64);
                for (k, v) in voice[at..(at + step).min(len)].iter_mut().enumerate() {
                    *v = (k, hz, vel);
                }
            }
            let mut ph = rng.unit();
            for (i, o) in out.iter_mut().enumerate() {
                let (k, hz, vel) = voice[i % len];
                if hz == 0.0 {
                    lp.process(0.0);
                    continue;
                }
                let since = k as f64;
                let amp = vel * (1.0 - exp(-since / (0.003 * SR))) * exp(-since / (0.22 * step as f64 + 1.0));
                if i % CONTROL == 0 {
                    lp.set(380.0 + 3_000.0 * vel * exp(-since / (0.12 * step as f64 + 1.0)), 3.5, SR);
                }
                let dt = hz / SR;
                *o = lp.process(amp * pulse(ph, 0.5, dt)).low;
                advance(&mut ph, dt);
            }
            let d = beat * 3 / 4;
            for i in d..total {
                let back = out[i - d];
                out[i] += 0.3 * back;
            }
        }
        LoopTimbre::Off => {}
    }
    out
}

/// Turns a warm-up + loop + crossfade render into a seamless loop of `len`:
/// the last `XFADE` samples' continuation is faded (equal power) into the start.
fn close_loop(s: &[f64], len: usize) -> Vec<f64> {
    (0..len)
        .map(|i| {
            if i < XFADE {
                let p = 0.25 * i as f64 / XFADE as f64;
                s[len + i] * sin_turns(p) + s[2 * len + i] * cos_turns(p)
            } else {
                s[len + i]
            }
        })
        .collect()
}

/// Each event layer is scaled to this RMS like the sustained ones, but the
/// scale is capped so a nearly-empty grid can't blow one click up to full level.
const EVENT_MAX_SCALE: f64 = 6.0;

/// One atmosphere loop layer: a loop per chord, crossfaded as the chords change.
pub struct AtmosLoop {
    design: LoopDesign,
    beats: u64,
    len: usize,
    /// Per chord: (left, right), `len` samples each.
    loops: Vec<(Vec<f32>, Vec<f32>)>,
    performances: Vec<Performance>,
    /// Gains of (chord, next chord) and which chords, refreshed every `CONTROL` samples.
    mix: (usize, usize, f64, f64),
}

impl AtmosLoop {
    /// `stream` keeps each layer's randomness separate; `target_s` is its
    /// rough length; `avoid` is the other loop's beat count, if any.
    pub fn new(
        design: LoopDesign,
        seed: u64,
        stream: u64,
        harmony: &Harmony,
        beat: u64,
        target_s: u64,
        avoid: Option<u64>,
        transpose: i8,
    ) -> Self {
        let beats = loop_beats(beat, target_s, avoid);
        let len = (beats * beat) as usize;
        let mut rng = Rng::stream(seed, stream);
        let mut loops = Vec::new();
        let mut performances = Vec::new();
        if design.is_on() {
            for chord in &harmony.chords {
                let mut perf = match design {
                    LoopDesign::Sustained(t) => perform(t, &mut rng, &chord.notes, beats, beat),
                    LoopDesign::Events(_) => perform_events(&mut rng, &chord.notes),
                };
                perf.harmonic = harmony.garage;
                perf.grid = (harmony.sixteenth as usize, harmony.swing_push as usize);
                if transpose != 0 {
                    for n in &mut perf.notes {
                        *n += transpose as f64;
                    }
                }
                let total = 2 * len + XFADE;
                let (l, r) = match design {
                    LoopDesign::Sustained(t) => (
                        close_loop(&render_channel(t, &perf, &mut rng, len, beat as usize, total), len),
                        close_loop(&render_channel(t, &perf, &mut rng, len, beat as usize, total), len),
                    ),
                    LoopDesign::Events(g) => (
                        close_loop(&crate::texture::render_channel(g, &perf.notes, &mut rng, len, total), len),
                        close_loop(&crate::texture::render_channel(g, &perf.notes, &mut rng, len, total), len),
                    ),
                };
                let power: f64 = l.iter().chain(&r).map(|x| x * x).sum::<f64>() / (2 * len) as f64;
                let mut scale = if power > 0.0 { LOOP_RMS / power.sqrt() } else { 0.0 };
                if matches!(design, LoopDesign::Events(_)) {
                    scale = scale.min(EVENT_MAX_SCALE);
                }
                loops.push((l.iter().map(|x| (x * scale) as f32).collect(), r.iter().map(|x| (x * scale) as f32).collect()));
                performances.push(perf);
            }
        }
        AtmosLoop { design, beats, len, loops, performances, mix: (0, 0, 1.0, 0.0) }
    }

    pub fn is_on(&self) -> bool {
        !self.loops.is_empty()
    }

    /// Next stereo sample at `clock`.
    pub fn next(&mut self, clock: u64, harmony: &Harmony) -> (f64, f64) {
        if self.loops.is_empty() {
            return (0.0, 0.0);
        }
        if clock % CONTROL as u64 == 0 {
            let (a, b, morph) = harmony.loop_mix(clock);
            let x = 0.25 * morph;
            self.mix = (a, b, cos_turns(x), sin_turns(x));
        }
        let i = (clock % self.len as u64) as usize;
        let (a, b, ga, gb) = self.mix;
        let (la, ra) = (&self.loops[a].0, &self.loops[a].1);
        let mut l = la[i] as f64 * ga;
        let mut r = ra[i] as f64 * ga;
        if gb > 0.0 {
            l += self.loops[b].0[i] as f64 * gb;
            r += self.loops[b].1[i] as f64 * gb;
        }
        (l, r)
    }
}

/// Read-only views for visualisation.
impl AtmosLoop {
    pub fn design(&self) -> LoopDesign {
        self.design
    }

    /// (beats, length in samples).
    pub fn length(&self) -> (u64, usize) {
        (self.beats, self.len)
    }

    pub fn performances(&self) -> &[Performance] {
        &self.performances
    }

    /// Chord `chord`'s loop (left), as `points` peak values.
    pub fn overview(&self, chord: usize, points: usize) -> Vec<f32> {
        self.loops.get(chord).map_or_else(Vec::new, |(l, _)| {
            l.chunks(self.len.div_ceil(points).max(1)).map(|c| c.iter().fold(0.0f32, |a, x| a.max(x.abs()))).collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_stay_in_range_and_drift() {
        for bpm in crate::tempo::MIN_BPM..=crate::tempo::MAX_BPM {
            let beat = crate::tempo::Tempo::new(bpm).beat();
            let a = loop_beats(beat, 6, None);
            let b = loop_beats(beat, 4, Some(a));
            for n in [a, b] {
                let s = (n * beat) as f64 / SR;
                assert!((MIN_SECONDS as f64 - 1e-9..=MAX_SECONDS as f64 + 1e-9).contains(&s), "{bpm}: {s}");
            }
            assert_eq!(gcd(a, b), 1, "{bpm}: {a} and {b} beats share a factor");
        }
    }

    #[test]
    fn every_timbre_sounds_distinct() {
        let (mut a, mut b) = (Rng::stream(1000, 0xD0), Rng::stream(1000, 0xD7));
        let t = crate::tempo::Tempo::new(168);
        let h = Harmony::new(crate::harmony::Scale::Lydian, 2, crate::harmony::Pace::HalfTime, t.beat(), 2, &mut a, &mut b);
        let mut seen: Vec<(String, u64)> = Vec::new();
        let mut designs: Vec<(String, LoopDesign)> = LOOP_TIMBRES
            .into_iter()
            .filter(|t| *t != LoopTimbre::Off)
            .map(|t| (t.name().to_string(), LoopDesign::Sustained(t)))
            .collect();
        for (name, g) in [("Fire", crate::texture::Grid::fire()), ("Water", crate::texture::Grid::water()), ("Stones", crate::texture::Grid::stones()), ("Wood", crate::texture::Grid::wood())] {
            designs.push((name.to_string(), LoopDesign::Events(g)));
        }
        for (name, design) in designs {
            let lp = AtmosLoop::new(design, 11, 0xA1, &h, t.beat(), 6, None, 0);
            let (l, _) = &lp.loops[0];
            let hash = l.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, x| {
                x.to_bits().to_le_bytes().into_iter().fold(h, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
            });
            if let Some((other, _)) = seen.iter().find(|(_, o)| *o == hash) {
                panic!("{name} sounds identical to {other}");
            }
            seen.push((name, hash));
        }
    }

    #[test]
    fn the_original_timbres_keep_their_recipe_indices() {
        // Recipes store a sustained loop by its index here, so new timbres are
        // only ever appended (before Off, which has its own recipe tag).
        let codes: Vec<&str> = LOOP_TIMBRES[..11].iter().map(|t| t.code()).collect();
        assert_eq!(codes, ["VOX", "BEL", "BLP", "WVC", "AIR", "HSH", "GST", "SHN", "RIL", "VEL", "MST"]);
        assert_eq!(LOOP_TIMBRES.last(), Some(&LoopTimbre::Off));
        let mut all: Vec<&str> = LOOP_TIMBRES.iter().map(|t| t.code()).collect();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), LOOP_TIMBRES.len(), "every code is unique");
    }

    #[test]
    fn the_loop_point_is_seamless() {
        let (mut a, mut b) = (Rng::stream(1000, 0xD0), Rng::stream(1000, 0xD7));
        let t = crate::tempo::Tempo::new(168);
        let h = Harmony::new(crate::harmony::Scale::Lydian, 2, crate::harmony::Pace::HalfTime, t.beat(), 2, &mut a, &mut b);
        let mut designs: Vec<(String, LoopDesign)> = LOOP_TIMBRES
            .into_iter()
            .filter(|t| *t != LoopTimbre::Off)
            .map(|t| (t.name().to_string(), LoopDesign::Sustained(t)))
            .collect();
        for (name, g) in [("Fire", crate::texture::Grid::fire()), ("Water", crate::texture::Grid::water()), ("Stones", crate::texture::Grid::stones()), ("Wood", crate::texture::Grid::wood())] {
            designs.push((name.to_string(), LoopDesign::Events(g)));
        }
        for (name, design) in designs {
            let lp = AtmosLoop::new(design, 11, 0xA1, &h, t.beat(), 6, None, 0);
            let (l, _) = &lp.loops[0];
            let step = |i: usize, j: usize| (l[i] - l[j]).abs();
            // The jump across the loop point is no bigger than ordinary sample-to-sample motion.
            let typical = (1..l.len()).map(|i| step(i, i - 1)).fold(0.0f32, f32::max);
            assert!(step(0, l.len() - 1) <= typical, "{name}");
        }
    }
}
