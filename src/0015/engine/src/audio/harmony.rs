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
    /// The major pentatonic: bright Japanese pop's "no fourth, no seventh" scale.
    Yonanuki,
    /// The Okinawan scale: major, without the second and sixth.
    Ryukyu,
}

pub const SCALES: [Scale; 14] = [
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
    Scale::Yonanuki,
    Scale::Ryukyu,
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
            Scale::Yonanuki => &[0, 2, 4, 7, 9],
            Scale::Ryukyu => &[0, 4, 5, 7, 11],
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
            Scale::Yonanuki => "Major pentatonic (yonanuki)",
            Scale::Ryukyu => "Ryukyu",
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
            Scale::Yonanuki => "bright and sunny, five notes, no tension",
            Scale::Ryukyu => "island-bright, major with a gentle lift",
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
            Scale::Yonanuki => "YON",
            Scale::Ryukyu => "RYU",
        }
    }

    pub fn from_code(code: &str) -> Option<Scale> {
        SCALES.iter().copied().find(|s| s.code().eq_ignore_ascii_case(code))
    }

    pub fn index(self) -> usize {
        SCALES.iter().position(|s| *s == self).unwrap_or(0)
    }

    /// A major-third scale (else minor), for choosing garage progressions.
    pub fn is_major(self) -> bool {
        self.intervals().contains(&4) && !self.intervals().contains(&3)
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
    /// 2-step garage: chords change every two drum bars (as half-time), and
    /// the drums, bass and stabs play the garage way.
    Garage,
    /// Microsound: clicks, grains and sine pips on a faint pulse; chords
    /// change slowly (as half-time), everything plays small and exact.
    Micro,
}

/// The modes, in setup order.
pub const PACES: [Pace; 4] = [Pace::HalfTime, Pace::Jungle, Pace::Garage, Pace::Micro];

impl Pace {
    pub fn name(self) -> &'static str {
        match self {
            Pace::HalfTime => "half-time",
            Pace::Jungle => "jungle",
            Pace::Garage => "2-step garage",
            Pace::Micro => "microsound",
        }
    }

    /// The mode called `name` (case and spacing ignored; "garage" is enough).
    pub fn from_name(name: &str) -> Option<Pace> {
        let key = |s: &str| s.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_lowercase()).collect::<String>();
        let want = key(name);
        PACES.into_iter().find(|p| key(p.name()) == want || (want == "garage" && *p == Pace::Garage) || (want == "micro" && *p == Pace::Micro))
    }

    /// Recipe value.
    pub fn index(self) -> u32 {
        match self {
            Pace::HalfTime => 0,
            Pace::Jungle => 1,
            Pace::Garage => 2,
            Pace::Micro => 3,
        }
    }

    /// A bar's length, given one half-time beat (`Tempo::beat`) in samples.
    pub fn bar_samples(self, beat: u64) -> u64 {
        match self {
            Pace::HalfTime => 4 * beat,
            Pace::Jungle => 2 * beat,
            Pace::Garage => 4 * beat,
            Pace::Micro => 4 * beat,
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
    /// Its pitch classes (0-11), root first: the chord the stabs voice.
    pub tones: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct Harmony {
    /// How long each change's crossfade lasts (samples): a whole bar for
    /// the slow modes, a single beat in 2-step garage.
    morph_len: u64,
    /// 2-step garage: which chord plays in each two-bar step of the cycle
    /// (the progression, then — with the key lift — its lifted copy). Empty
    /// for the slow modes, which weight their chords' lengths instead.
    sequence: Vec<usize>,
    /// Chords in one pass of the progression (before any lift).
    pub loop_len: usize,
    /// 2-step garage: how late a swung sixteenth is pushed (samples) and how
    /// long a sixteenth is, so the loops and glitches can swing with the
    /// drums. Zero for the slow modes.
    pub swing_push: u64,
    pub sixteenth: u64,
    /// 2-step garage: plain chords, so the timbres keep to whole-number
    /// (harmonic) overtones and the glitch pitches to the safest notes.
    pub garage: bool,
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
            let tones = [0usize, 2, 4, 6].iter().map(|s| ((key as usize + iv[(degree + s) % n as usize] as usize) % 12) as u8).collect();
            out.push(Chord { degree, notes, bass, bars: *bars, tones });
        }
        let bar_samples = pace.bar_samples(beat);
        let cycle = weights.iter().map(|b| *b as u64).sum::<u64>() * bar_samples;
        let loop_len = out.len();
        Harmony { morph_len: bar_samples, sequence: Vec::new(), loop_len, swing_push: 0, sixteenth: 0, garage: false, scale, key, chords: out, bar_samples, cycle }
    }

    /// 2-step garage's drone: the tonic as an open fifth (root, fifth,
    /// octave, fifth above) — no third, no colour notes — so it sits under
    /// every chord of the progression without rubbing against it.
    pub fn pedal(scale: Scale, beat: u64, key: u8) -> Harmony {
        let iv = scale.intervals();
        let fifth = if iv.len() == 7 { iv[4] as f64 } else { 7.0 };
        let root = (48 + key as i32) as f64;
        let notes = vec![root, root + fifth, root + 12.0, root + 12.0 + fifth];
        let mut bass = (36 + key as i32) as f64;
        if bass > 45.0 {
            bass -= 12.0;
        }
        let bar_samples = Pace::Garage.bar_samples(beat);
        Harmony {
            morph_len: bar_samples,
            garage: true,
            scale,
            key,
            chords: vec![Chord { degree: 0, notes, bass, bars: 32, tones: vec![key % 12, (key + fifth as u8) % 12] }],
            sequence: Vec::new(),
            loop_len: 1,
            swing_push: 0,
            sixteenth: 0,
            bar_samples,
            cycle: 32 * bar_samples,
        }
    }

    /// 2-step garage's harmony, each chord two drum bars long and changing
    /// on the downbeat (the loops crossfade over just the last beat).
    ///
    /// With four chords it plays one of the named [`PROGRESSIONS`] — chosen
    /// with `progression` (1-based), or by the seed from those of the scale's
    /// family when it is 0 — with chords borrowed from outside the key where
    /// the progression asks (secondary dominants, borrowed iv, ♭VII). With
    /// fewer chords it keeps to the scale's own. `lift` > 0 steps the whole
    /// progression up a whole tone every `lift` passes, then home again.
    pub fn garage(scale: Scale, chords: usize, beat: u64, key: u8, rng: &mut Rng, progression: usize, lift: usize) -> Harmony {
        let iv = scale.intervals();
        let n = iv.len();
        let major = scale.is_major();
        let k = chords.clamp(1, 4);
        let shape: Vec<(u8, Quality)> = if k == 4 {
            let pick = if progression >= 1 && progression <= PROGRESSIONS.len() {
                &PROGRESSIONS[progression - 1]
            } else {
                let family: Vec<&Progression> = PROGRESSIONS.iter().filter(|p| p.major == major).collect();
                family[rng.below(family.len())]
            };
            pick.chords.to_vec()
        } else {
            // Short loops: the scale's own chords.
            let pool: &[&[usize]] = match (major, k) {
                (_, 1) => &[&[0]],
                (true, 2) => &[&[0, 3], &[0, 4]],
                (true, _) => &[&[0, 3, 4], &[5, 3, 4]],
                (false, 2) => &[&[0, 3], &[0, 6]],
                (false, _) => &[&[0, 3, 6], &[0, 6, 3]],
            };
            let degrees = pool[rng.below(pool.len())];
            degrees.iter().map(|d| diatonic(iv, d % n)).collect()
        };
        let mut out: Vec<Chord> = shape.iter().map(|&(root, q)| garage_chord(iv, key, root, q)).collect();
        let loop_len = out.len();
        let mut sequence: Vec<usize> = (0..loop_len).collect();
        if lift > 0 {
            // The lifted copy, a whole tone up, after `lift` passes at home.
            out.extend(shape.iter().map(|&(root, q)| garage_chord(iv, key, (root + 2) % 12, q)).map(|mut c| {
                // Keep the lifted bass and shells in the same registers.
                if c.bass > 45.0 {
                    c.bass -= 12.0;
                }
                c
            }));
            sequence = (0..lift).flat_map(|_| 0..loop_len).chain((0..lift).flat_map(|_| loop_len..2 * loop_len)).collect();
        }
        let bar_samples = Pace::Garage.bar_samples(beat);
        let cycle = sequence.len() as u64 * bar_samples;
        Harmony { morph_len: bar_samples / 8, sequence, loop_len, swing_push: 0, sixteenth: 0, garage: true, scale, key, chords: out, bar_samples, cycle }
    }

    /// How the atmosphere loops blend at `t`: (from, to, 0..1). The slow
    /// modes crossfade during each chord's last bar, exactly as [`at`]. In
    /// 2-step garage the loops change *on* the downbeat with the bass and
    /// stabs: a quick crossfade over the first sixteenth of the new chord.
    ///
    /// [`at`]: Harmony::at
    pub fn loop_mix(&self, t: u64) -> (usize, usize, f64) {
        if self.sequence.is_empty() {
            let p = self.at(t);
            return (p.chord, p.next, p.morph);
        }
        let step = (t / self.bar_samples) as usize;
        let cur = self.sequence[step % self.sequence.len()];
        let fade = (self.bar_samples / 32).max(1);
        let pos = t % self.bar_samples;
        if step > 0 && pos < fade {
            let prev = self.sequence[(step - 1) % self.sequence.len()];
            if prev != cur {
                let x = pos as f64 / fade as f64;
                return (prev, cur, x * x * (3.0 - 2.0 * x));
            }
        }
        (cur, cur, 0.0)
    }

    /// Whether chord `i` is the first of a pass through the progression.
    pub fn starts_pass(&self, i: usize) -> bool {
        i % self.loop_len.max(1) == 0
    }

    /// Whether chord `i` has a tone a semitone from the tonic or its fifth
    /// (the garage drone's pedal): the drone steps back under it.
    pub fn rubs_pedal(&self, i: usize) -> bool {
        let iv = self.scale.intervals();
        let fifth = if iv.contains(&7) { 7 } else { iv.get(iv.len() / 2).copied().unwrap_or(7) };
        let pedal = [self.key % 12, (self.key + fifth) % 12];
        self.chords[i].tones.iter().any(|t| pedal.iter().any(|p| (12 + *t as i32 - *p as i32) % 12 == 1 || (12 + *p as i32 - *t as i32) % 12 == 1))
    }

    /// The progression at sample `t`. The last bar of each chord morphs into the next.
    pub fn at(&self, t: u64) -> Position {
        if !self.sequence.is_empty() {
            let step = (t / self.bar_samples) as usize % self.sequence.len();
            let (chord, next) = (self.sequence[step], self.sequence[(step + 1) % self.sequence.len()]);
            let pos = t % self.bar_samples;
            let morph_start = self.bar_samples - self.morph_len;
            let morph = if chord != next && pos >= morph_start {
                let x = (pos - morph_start) as f64 / self.morph_len as f64;
                x * x * (3.0 - 2.0 * x)
            } else {
                0.0
            };
            return Position { chord, next, bar: step as u32, morph };
        }
        let n = self.chords.len();
        if n == 1 {
            return Position { chord: 0, next: 0, bar: ((t / self.bar_samples) % 32) as u32, morph: 0.0 };
        }
        let mut pos = t % self.cycle;
        for (i, c) in self.chords.iter().enumerate() {
            let len = c.bars as u64 * self.bar_samples;
            if pos < len {
                let bar = (pos / self.bar_samples) as u32;
                let morph_start = len - self.morph_len;
                let morph = if pos >= morph_start {
                    let x = (pos - morph_start) as f64 / self.morph_len as f64;
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

/// A chord's quality, as its intervals above the root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    Maj7,
    Dom7,
    Min7,
    HalfDim,
}

impl Quality {
    pub fn intervals(self) -> [u8; 4] {
        match self {
            Quality::Maj7 => [0, 4, 7, 11],
            Quality::Dom7 => [0, 4, 7, 10],
            Quality::Min7 => [0, 3, 7, 10],
            Quality::HalfDim => [0, 3, 6, 10],
        }
    }

    pub fn suffix(self) -> &'static str {
        match self {
            Quality::Maj7 => "maj7",
            Quality::Dom7 => "7",
            Quality::Min7 => "m7",
            Quality::HalfDim => "m7b5",
        }
    }
}

/// A named garage progression: four chords as (semitones above the key, quality).
pub struct Progression {
    pub name: &'static str,
    /// How it reads in roman numerals.
    pub numerals: &'static str,
    /// Major-key (else minor-key) family, for the seed's pick.
    pub major: bool,
    pub chords: [(u8, Quality); 4],
}

use Quality::{Dom7 as D7, HalfDim as HD, Maj7 as M7, Min7 as M};

/// The 2-step garage progressions: the bright, upbeat turns of Japanese pop,
/// city pop and game menus, many with chords borrowed from outside the key.
pub const PROGRESSIONS: [Progression; 15] = [
    Progression { name: "Royal road", numerals: "IVmaj7 V7 iii7 vi7", major: true, chords: [(5, M7), (7, D7), (4, M), (9, M)] },
    Progression { name: "Royal road, bright", numerals: "IVmaj7 V7 III7 vi7", major: true, chords: [(5, M7), (7, D7), (4, D7), (9, M)] },
    Progression { name: "Soft turn", numerals: "IVmaj7 III7 vi7 I7", major: true, chords: [(5, M7), (4, D7), (9, M), (0, D7)] },
    Progression { name: "City lights", numerals: "ii7 V7 Imaj7 VI7", major: true, chords: [(2, M), (7, D7), (0, M7), (9, D7)] },
    Progression { name: "Sunrise", numerals: "IVmaj7 V7 vi7 bVIImaj7", major: true, chords: [(5, M7), (7, D7), (9, M), (10, M7)] },
    Progression { name: "Menu loop", numerals: "Imaj7 vi7 ii7 V7", major: true, chords: [(0, M7), (9, M), (2, M), (7, D7)] },
    Progression { name: "Wistful", numerals: "Imaj7 III7 IVmaj7 iv7", major: true, chords: [(0, M7), (4, D7), (5, M7), (5, M)] },
    Progression { name: "Two-five-one", numerals: "ii7 V7 Imaj7 vi7", major: true, chords: [(2, M), (7, D7), (0, M7), (9, M)] },
    Progression { name: "Pop loop", numerals: "vi7 IVmaj7 Imaj7 V7", major: true, chords: [(9, M), (5, M7), (0, M7), (7, D7)] },
    Progression { name: "Daydream", numerals: "Imaj7 II7 IVmaj7 iv7", major: true, chords: [(0, M7), (2, D7), (5, M7), (5, M)] },
    Progression { name: "Night drive", numerals: "i7 iv7 bVII7 bIIImaj7", major: false, chords: [(0, M), (5, M), (10, D7), (3, M7)] },
    Progression { name: "Neon", numerals: "i7 IV7 bVIImaj7 bIIImaj7", major: false, chords: [(0, M), (5, D7), (10, M7), (3, M7)] },
    Progression { name: "Climb", numerals: "bVImaj7 bVII7 i7 V7", major: false, chords: [(8, M7), (10, D7), (0, M), (7, D7)] },
    Progression { name: "Rain city", numerals: "i7 bVImaj7 bIIImaj7 bVII7", major: false, chords: [(0, M), (8, M7), (3, M7), (10, D7)] },
    Progression { name: "Descent", numerals: "ii7b5 V7 i7 bVImaj7", major: false, chords: [(2, HD), (7, D7), (0, M), (8, M7)] },
];

/// The scale's own seventh chord on degree `d`, as (semitones above the key, quality).
fn diatonic(iv: &[u8], d: usize) -> (u8, Quality) {
    let n = iv.len();
    let at = |k: usize| (iv[(d + k) % n] as i32 + 12 * ((d + k) / n) as i32 - iv[d] as i32) as u8;
    let (third, fifth, seventh) = (at(2), at(4), at(6));
    let q = match (third, fifth, seventh) {
        (4, _, 11) => Quality::Maj7,
        (4, _, _) => Quality::Dom7,
        (3, 6, _) => Quality::HalfDim,
        _ => Quality::Min7,
    };
    (iv[d], q)
}

/// A garage chord: its tones, the loops' open shell (root, fifth, tenth,
/// rooted C4–B4 so it opens out above the stabs), and its bass note.
fn garage_chord(iv: &[u8], key: u8, root: u8, q: Quality) -> Chord {
    let qi = q.intervals();
    let r = (key + root) % 12;
    let tones: Vec<u8> = qi.iter().map(|i| (r + i) % 12).collect();
    let mut low = 60.0 + r as f64;
    while low > 71.0 {
        low -= 12.0;
    }
    let notes = vec![low, low + qi[2] as f64, low + qi[1] as f64 + 12.0];
    let mut bass = (36 + key as i32 + root as i32) as f64;
    while bass > 45.0 {
        bass -= 12.0;
    }
    let degree = iv.iter().position(|x| *x == root).unwrap_or(0);
    Chord { degree, notes, bass, bars: 1, tones }
}

pub fn key_name(pc: u8) -> &'static str {
    ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"][pc as usize % 12]
}
