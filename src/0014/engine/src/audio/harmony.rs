//! Harmony: the scale, the key, and a weighted progression of 1–4 complex,
//! drone-friendly chords, all chosen from the settings and seed 1.

use crate::rng::Rng;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    Lydian,
    Ionian,
    Dorian,
    Phrygian,
    Aeolian,
    Mixolydian,
    HarmonicMinor,
    MelodicMinor,
    WholeTone,
    Hirajoshi,
    PentatonicMinor,
    DoubleHarmonic,
}

pub const SCALES: [Scale; 12] = [
    Scale::Lydian,
    Scale::Ionian,
    Scale::Dorian,
    Scale::Phrygian,
    Scale::Aeolian,
    Scale::Mixolydian,
    Scale::HarmonicMinor,
    Scale::MelodicMinor,
    Scale::WholeTone,
    Scale::Hirajoshi,
    Scale::PentatonicMinor,
    Scale::DoubleHarmonic,
];

impl Scale {
    pub fn intervals(self) -> &'static [u8] {
        match self {
            Scale::Lydian => &[0, 2, 4, 6, 7, 9, 11],
            Scale::Ionian => &[0, 2, 4, 5, 7, 9, 11],
            Scale::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            Scale::Phrygian => &[0, 1, 3, 5, 7, 8, 10],
            Scale::Aeolian => &[0, 2, 3, 5, 7, 8, 10],
            Scale::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
            Scale::HarmonicMinor => &[0, 2, 3, 5, 7, 8, 11],
            Scale::MelodicMinor => &[0, 2, 3, 5, 7, 9, 11],
            Scale::WholeTone => &[0, 2, 4, 6, 8, 10],
            Scale::Hirajoshi => &[0, 2, 3, 7, 8],
            Scale::PentatonicMinor => &[0, 3, 5, 7, 10],
            Scale::DoubleHarmonic => &[0, 1, 4, 5, 7, 8, 11],
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Scale::Lydian => "Lydian",
            Scale::Ionian => "Ionian (major)",
            Scale::Dorian => "Dorian",
            Scale::Phrygian => "Phrygian",
            Scale::Aeolian => "Aeolian (natural minor)",
            Scale::Mixolydian => "Mixolydian",
            Scale::HarmonicMinor => "Harmonic minor",
            Scale::MelodicMinor => "Melodic minor",
            Scale::WholeTone => "Whole tone",
            Scale::Hirajoshi => "Hirajoshi",
            Scale::PentatonicMinor => "Pentatonic minor",
            Scale::DoubleHarmonic => "Double harmonic",
        }
    }

    /// A one-line character sketch, for the setup screen.
    pub fn mood(self) -> &'static str {
        match self {
            Scale::Lydian => "bright, floating, unresolved (raised 4th)",
            Scale::Ionian => "open and consonant",
            Scale::Dorian => "minor but warm, a hopeful 6th",
            Scale::Phrygian => "dark, with a tense flat 2nd",
            Scale::Aeolian => "plain melancholy minor",
            Scale::Mixolydian => "major with a relaxed flat 7th",
            Scale::HarmonicMinor => "minor with a dramatic leading tone",
            Scale::MelodicMinor => "minor below, major above: misty",
            Scale::WholeTone => "dreamlike, no gravity at all",
            Scale::Hirajoshi => "Japanese pentatonic, sparse and stark",
            Scale::PentatonicMinor => "five notes, nothing can clash",
            Scale::DoubleHarmonic => "exotic, two augmented seconds",
        }
    }

    /// Three-letter code used in recipes.
    pub fn code(self) -> &'static str {
        match self {
            Scale::Lydian => "LYD",
            Scale::Ionian => "ION",
            Scale::Dorian => "DOR",
            Scale::Phrygian => "PHR",
            Scale::Aeolian => "AEO",
            Scale::Mixolydian => "MIX",
            Scale::HarmonicMinor => "HMI",
            Scale::MelodicMinor => "MMI",
            Scale::WholeTone => "WHO",
            Scale::Hirajoshi => "HIR",
            Scale::PentatonicMinor => "PMI",
            Scale::DoubleHarmonic => "DBH",
        }
    }

    pub fn from_code(code: &str) -> Option<Scale> {
        SCALES.iter().copied().find(|s| s.code().eq_ignore_ascii_case(code))
    }

    pub fn index(self) -> usize {
        SCALES.iter().position(|s| *s == self).unwrap_or(0)
    }

    pub fn from_index(i: usize) -> Scale {
        SCALES[i % SCALES.len()]
    }
}

/// How long a bar is, for chord changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pace {
    /// Four half-time beats per bar (≈2.86 s at the default 168 BPM): slow, drone-like changes.
    HalfTime,
    /// Bars at the drum tempo (≈1.43 s at 168 BPM), the drums' own bar.
    Jungle,
}

impl Pace {
    /// A bar's length, given one half-time beat (`Tempo::beat`) in samples.
    pub fn bar_samples(self, beat: u64) -> u64 {
        match self {
            Pace::HalfTime => 4 * beat,
            Pace::Jungle => 2 * beat,
        }
    }
}

/// Bars per chord, by how many chords there are. Chords 1 and 2 carry most of
/// the 32-bar cycle.
pub fn bar_weights(chords: usize) -> &'static [u32] {
    match chords {
        0 | 1 => &[32],
        2 => &[20, 12],
        3 => &[16, 12, 4],
        _ => &[14, 10, 4, 4],
    }
}

/// Voicing shapes, in scale steps above the chord's degree (7-note scales).
/// The first four are the four 0009 voicings of D lydian, so the default
/// settings reproduce that chord exactly; the last two are a stacked-fourths
/// (quartal) cloud and a wide open-thirds spread.
const SHAPES: [&[i32]; 6] = [
    &[0, 4, 8, 9, 13, 17, 19],
    &[0, 4, 6, 8, 10, 12, 16],
    &[-3, 1, 4, 8, 9, 12, 13, 17],
    &[0, 2, 6, 8, 10, 13, 15, 20],
    &[0, 3, 6, 9, 12, 15, 18],
    &[0, 4, 9, 11, 13, 16, 18, 22],
];

/// Seed 1 picks the key; the mapping is rotated so the canonical seed is D.
pub fn key_for(seed: u64) -> u8 {
    let pick = |s: u64| (Rng::stream(s, 0x4B).next_u64() % 12) as i64;
    let d = 2i64;
    (d + pick(seed) - pick(crate::DEFAULT_SEEDS.s1)).rem_euclid(12) as u8
}

#[derive(Clone, Debug)]
pub struct Chord {
    /// Scale degree (0 = tonic).
    pub degree: usize,
    /// MIDI notes, low to high.
    pub notes: Vec<f64>,
    /// The bass note (MIDI) for this chord.
    pub bass: f64,
    pub bars: u32,
}

#[derive(Clone, Debug)]
pub struct Harmony {
    pub scale: Scale,
    /// Pitch class of the tonic, 0 = C.
    pub key: u8,
    pub chords: Vec<Chord>,
    pub bar_samples: u64,
    /// Samples in one full trip through every chord.
    pub cycle: u64,
}

/// Where the progression is at one moment.
#[derive(Clone, Copy, Debug, Default)]
pub struct Position {
    pub chord: usize,
    pub next: usize,
    /// Bar within the current chord (0-based).
    pub bar: u32,
    /// 0 → fully on `chord`, 1 → fully on `next` (during the last bar only).
    pub morph: f64,
}

impl Harmony {
    /// `first` is the drone's seed-1 stream: chord 1's voicing is the first
    /// draw from it, exactly as in 0009, so the default reproduces that chord.
    /// Everything else comes from `rest`, leaving `first` for chord 1's FM recipes.
    pub fn new(
        scale: Scale,
        chords: usize,
        pace: Pace,
        beat: u64,
        key: u8,
        first: &mut Rng,
        rest: &mut Rng,
    ) -> Harmony {
        let iv = scale.intervals();
        let n = iv.len() as i32;
        let base = 48 + key as i32; // tonic in octave 3 (D3 = 50)
        let note = |step: i32| -> f64 {
            let oct = step.div_euclid(n);
            (base + 12 * oct + iv[step.rem_euclid(n) as usize] as i32) as f64
        };
        let weights = bar_weights(chords);
        // Degrees for chords after the first: strong ones are more likely.
        let pool: Vec<(usize, f64)> = if n == 7 {
            vec![(3, 3.0), (4, 3.0), (5, 3.0), (1, 2.0), (2, 1.0), (6, 1.0)]
        } else {
            (1..n as usize).map(|d| (d, 1.0)).collect()
        };
        let mut out: Vec<Chord> = Vec::new();
        for (i, bars) in weights.iter().enumerate() {
            let shape = if i == 0 { SHAPES[first.below(4)] } else { SHAPES[rest.below(SHAPES.len())] };
            let degree = if i == 0 {
                0
            } else {
                let prev = out[i - 1].degree;
                let choices: Vec<(usize, f64)> = pool.iter().copied().filter(|(d, _)| *d != prev).collect();
                let w: Vec<f64> = choices.iter().map(|c| c.1).collect();
                choices[rest.weighted(&w)].0
            };
            // Shapes are written for 7-note scales; stretch steps for others.
            let mut notes: Vec<f64> = shape
                .iter()
                .map(|s| {
                    let s = if n == 7 { *s } else { (*s as f64 * n as f64 / 7.0).round() as i32 };
                    note(degree as i32 + s)
                })
                .collect();
            // Later chords sit a little lower if their degree pushes them up.
            let shift = if notes[0] > 55.0 { -12.0 } else { 0.0 };
            for x in notes.iter_mut() {
                *x += shift;
                while *x > 90.0 {
                    *x -= 12.0;
                }
            }
            notes.sort_by(|a, b| a.partial_cmp(b).unwrap());
            notes.dedup();
            let mut bass = (36 + key as i32 + iv[degree] as i32) as f64;
            if bass > 45.0 {
                bass -= 12.0;
            }
            out.push(Chord { degree, notes, bass, bars: *bars });
        }
        let bar_samples = pace.bar_samples(beat);
        let cycle = weights.iter().map(|b| *b as u64).sum::<u64>() * bar_samples;
        Harmony { scale, key, chords: out, bar_samples, cycle }
    }

    /// The progression at sample `t`. The last bar of each chord morphs into the next.
    pub fn at(&self, t: u64) -> Position {
        let n = self.chords.len();
        if n == 1 {
            return Position { chord: 0, next: 0, bar: ((t / self.bar_samples) % 32) as u32, morph: 0.0 };
        }
        let mut pos = t % self.cycle;
        for (i, c) in self.chords.iter().enumerate() {
            let len = c.bars as u64 * self.bar_samples;
            if pos < len {
                let bar = (pos / self.bar_samples) as u32;
                let morph_start = len - self.bar_samples;
                let morph = if pos >= morph_start {
                    let x = (pos - morph_start) as f64 / self.bar_samples as f64;
                    x * x * (3.0 - 2.0 * x) // smoothstep
                } else {
                    0.0
                };
                return Position { chord: i, next: (i + 1) % n, bar, morph };
            }
            pos -= len;
        }
        Position::default()
    }

    /// Every scale tone (MIDI) from `lo` to `hi` inclusive.
    pub fn scale_notes(&self, lo: f64, hi: f64) -> Vec<f64> {
        let iv = self.scale.intervals();
        (0..128)
            .filter(|m| iv.contains(&(((m - self.key as i32).rem_euclid(12)) as u8)))
            .map(|m| m as f64)
            .filter(|m| *m >= lo && *m <= hi)
            .collect()
    }
}

pub fn key_name(pc: u8) -> &'static str {
    ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"][pc as usize % 12]
}
