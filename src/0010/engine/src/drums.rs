//! Jungle drums, made the way jungle producers worked, but entirely synthesised.
//!
//! 1. A two-bar breakbeat is *played* once at start-up by synthesised drum
//!    voices at a funk-record tempo (~137 BPM), with human micro-timing.
//! 2. It is "sampled": run through a vintage-sampler colour chain (gentle
//!    saturation, sample-and-hold, 12-bit quantising, a lowpass, a small room).
//! 3. It is chopped into 32 sixteenth-note slices and re-sequenced live at
//!    168 BPM. Playing a 137 BPM break at 168 BPM is the classic jungle move:
//!    the break speeds up and its pitch rises with it.
//!
//! Every bar's slice plan is a pure function of seed 5, the bar number and
//! the slow LFOs, so playback never depends on block size or history.
//! Seed 5 = 0 switches the drums off.

use crate::filter::{OnePole, Svf};
use crate::math::{decay_coef, sanitize, sin_turns, tanh};
use crate::modulate::Mods;
use crate::rng::Rng;
use crate::SAMPLE_RATE;

/// A sixteenth note at 168 BPM: exactly half the glitch layers' 84 BPM sixteenth.
pub const SIXTEENTH: u64 = 4284;
pub const STEPS: usize = 16;
pub const BAR: u64 = STEPS as u64 * SIXTEENTH;
pub const PHRASE_BARS: u64 = 4;
/// A sixteenth at the break's original tempo (~137 BPM).
pub const SRC_SIXTEENTH: usize = 5256;
pub const SLICES: usize = 32;
const BREAK_LEN: usize = SLICES * SRC_SIXTEENTH;
/// Sampler playback rate: 137 → 168 BPM, so the pitch rises ~3.5 semitones.
pub const RATE: f64 = SRC_SIXTEENTH as f64 / SIXTEENTH as f64;
/// Short fades where the slice sequence is discontinuous (in samples).
const DECLICK: u64 = 24;
/// Intro: the first phrases stay ambient.
const SILENT_PHRASES: u64 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Voice {
    Kick,
    Snare,
    Ghost,
    Hat,
    OpenHat,
    Ride,
}

impl Voice {
    pub fn letter(self) -> char {
        match self {
            Voice::Kick => 'K',
            Voice::Snare => 'S',
            Voice::Ghost => 'g',
            Voice::Hat => 'h',
            Voice::OpenHat => 'O',
            Voice::Ride => 'R',
        }
    }

    pub fn is_cymbal(self) -> bool {
        matches!(self, Voice::Hat | Voice::OpenHat | Voice::Ride)
    }
}

/// How a step plays its slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Play,
    Reverse,
    /// Retriggered twice within the step (a 1/32 stutter roll).
    Roll,
    /// Played slower, so lower and shorter.
    PitchDown,
    /// Half speed across two steps: the "half-time" drag.
    Half,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Out,
    /// Only the high end of the break: hats and ride.
    HatsOnly,
    HalfTime,
    Full,
    /// Full, with many more edits and rolls.
    Rolls,
}

impl Section {
    pub fn name(self) -> &'static str {
        match self {
            Section::Out => "out",
            Section::HatsOnly => "hats only",
            Section::HalfTime => "half-time",
            Section::Full => "full break",
            Section::Rolls => "full + rolls",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    pub slice: u8,
    pub op: Op,
}

#[derive(Clone, Debug)]
pub struct Plan {
    pub bar: u64,
    pub section: Section,
    pub fill: bool,
    pub steps: [Step; STEPS],
    /// Step `i` continues seamlessly from step `i - 1` (no fade needed).
    join: [bool; STEPS],
}

/// The break's original patterns, at 32 sixteenths over two bars:
/// (name, kicks, snares, ghosts). Hats and ride are added on top.
const PATTERNS: [(&str, &[usize], &[usize], &[usize]); 3] = [
    ("Amen-style", &[0, 2, 10, 11, 16, 18, 26], &[4, 12, 20, 28], &[7, 9, 15, 23, 25, 31]),
    ("Think-style", &[0, 6, 10, 16, 23, 26], &[4, 12, 20, 28], &[14, 30]),
    ("Apache-style", &[0, 3, 8, 10, 16, 19, 24], &[4, 12, 20, 28], &[13, 29, 31]),
];

// Synthesis constants.
const KICK_DROP_SECONDS: f64 = 0.028;
const KICK_DECAY: f64 = 0.33;
const SNARE_BODY: [f64; 2] = [182.0, 331.0];
/// The classic 808-style metallic oscillator bank (Hz).
pub const METAL: [f64; 6] = [205.3, 304.4, 369.6, 522.7, 540.0, 800.0];
const SR: f64 = SAMPLE_RATE as f64;

fn add(buf: &mut [f64], at: usize, sound: &[f64]) {
    for (i, s) in sound.iter().enumerate() {
        if let Some(b) = buf.get_mut(at + i) {
            *b += s;
        }
    }
}

/// Kick: a sine whose pitch drops fast from ~170 to ~48 Hz, a noise click, a soft clip.
fn kick(vel: f64, rng: &mut Rng) -> Vec<f64> {
    let len = (0.42 * SR) as usize;
    let drop = decay_coef(KICK_DROP_SECONDS * 6.9, SR); // e^(-t/τ) per sample
    let amp_k = decay_coef(KICK_DECAY, SR);
    let (mut sweep, mut amp, mut phase) = (1.0, 1.0, 0.0);
    (0..len)
        .map(|i| {
            let f = 48.0 + 122.0 * sweep;
            sweep *= drop;
            let attack = (i as f64 / 48.0).min(1.0);
            let s = sin_turns(phase) * amp * attack;
            phase += f / SR;
            phase -= phase.floor();
            amp *= amp_k;
            let click = if i < 96 { rng.bipolar() * (1.0 - i as f64 / 96.0) * 0.45 } else { 0.0 };
            tanh(1.7 * (s + click)) * vel
        })
        .collect()
}

/// Snare: two tuned body modes + band-passed noise with its own decay + a crack.
fn snare(vel: f64, ghost: bool, rng: &mut Rng) -> Vec<f64> {
    let len = (0.32 * SR) as usize;
    let mut bp = Svf::new(2100.0, 0.75, SR);
    let noise_k = decay_coef(if ghost { 0.07 } else { 0.19 }, SR);
    let body_k = [decay_coef(0.09, SR), decay_coef(0.06, SR)];
    let (mut ne, mut be, mut ph) = (1.0, [1.0, 1.0], [0.0, 0.25]);
    (0..len)
        .map(|i| {
            let mut body = 0.0;
            for m in 0..2 {
                let bend = 1.0 + 0.15 * be[m];
                body += sin_turns(ph[m]) * be[m];
                ph[m] += SNARE_BODY[m] * bend / SR;
                ph[m] -= ph[m].floor();
                be[m] *= body_k[m];
            }
            let n = rng.bipolar();
            let band = bp.process(n).band;
            let noise = (band * 1.6 + n * 0.35) * ne;
            ne *= noise_k;
            let crack = if i < 144 { n * (1.0 - i as f64 / 144.0) } else { 0.0 };
            let attack = (i as f64 / 24.0).min(1.0);
            tanh(1.3 * (0.55 * body + 0.9 * noise + 0.8 * crack) * attack) * vel
        })
        .collect()
}

/// Cymbals: six square waves at inharmonic ratios (the metallic part) plus
/// band-passed noise (the wash of a real cymbal), then high-passed.
fn metal(vel: f64, voice: Voice, rng: &mut Rng) -> Vec<f64> {
    let (decay, scale, centre): (f64, f64, f64) = match voice {
        Voice::Hat => (0.045, 1.0, 8500.0),
        Voice::OpenHat => (0.3, 1.0, 8000.0),
        _ => (1.1, 1.37, 5200.0),
    };
    let len = ((decay * 4.0).min(1.6) * SR) as usize;
    let mut bp = Svf::new(centre, 1.1, SR);
    let mut noise_bp = Svf::new(centre * 1.1, 0.6, SR);
    let mut lp = OnePole::new(5500.0, SR);
    // Rides wash more than hats.
    let noise_amt = if voice == Voice::Ride { 0.75 } else { 0.5 };
    let k = decay_coef(decay * 6.9, SR); // e^(-t/decay)
    let bell_k = decay_coef(0.6, SR);
    let mut phases: [f64; 6] = [0.0; 6];
    for p in phases.iter_mut() {
        *p = rng.unit();
    }
    let (mut env, mut bell_env, mut bell_ph) = (1.0, 1.0, 0.0);
    (0..len)
        .map(|i| {
            let mut sq = 0.0;
            for (p, f) in phases.iter_mut().zip(METAL) {
                sq += if *p < 0.5 { 1.0 } else { -1.0 };
                *p += f * scale / SR;
                *p -= p.floor();
            }
            let wash = noise_bp.process(rng.bipolar()).band;
            let band = bp.process(sq / 6.0).band * (1.0 - noise_amt) + wash * noise_amt * 1.4;
            let high = band - lp.process(band);
            let mut s = high * env * 2.2;
            if voice == Voice::Ride {
                s += 0.12 * sin_turns(bell_ph) * bell_env;
                bell_ph += 2930.0 / SR;
                bell_ph -= bell_ph.floor();
                bell_env *= bell_k;
            }
            env *= k;
            let attack = (i as f64 / 12.0).min(1.0);
            s * attack * vel
        })
        .collect()
}

pub struct Drums {
    enabled: bool,
    seed: u64,
    /// The sampled break.
    brk: Vec<f64>,
    /// (source position, voice) of every hit in the break.
    hits: Vec<(usize, Voice)>,
    pattern: &'static str,
    plans: [Option<Plan>; 2],
    highpass: Svf,
    hp_mix: f64,
    // Display-only.
    last_hits: Vec<Voice>,
    hit_count: u64,
    step: usize,
}

impl Drums {
    pub fn new(seed: u64) -> Self {
        let mut d = Drums {
            enabled: seed != 0,
            seed,
            brk: Vec::new(),
            hits: Vec::new(),
            pattern: "off",
            plans: [None, None],
            highpass: Svf::new(5000.0, 0.7, SR),
            hp_mix: 0.0,
            last_hits: Vec::new(),
            hit_count: 0,
            step: 0,
        };
        if d.enabled {
            d.record_break();
        }
        d
    }

    /// Plays and samples the break (once, at start-up).
    fn record_break(&mut self) {
        let mut rng = Rng::stream(self.seed, 0xB4EA);
        let (name, kicks, snares, ghosts) = PATTERNS[rng.below(PATTERNS.len())];
        self.pattern = name;
        let mut buf = vec![0.0; BREAK_LEN + SR as usize];
        let swing = (SRC_SIXTEENTH as f64 * 0.05) as usize;
        let mut hits: Vec<(usize, Voice, f64)> = Vec::new();
        let at = |step: usize, rng: &mut Rng| {
            // Human timing: ±1.2 ms push/pull, plus a little swing on off-beats.
            let jitter = (rng.bipolar() * 58.0) as i64;
            let base = step * SRC_SIXTEENTH + if step % 2 == 1 { swing } else { 0 };
            (base as i64 + jitter).max(0) as usize
        };
        for s in 0..SLICES {
            if kicks.contains(&s) || (s % 8 == 6 && rng.chance(0.08)) {
                hits.push((at(s, &mut rng), Voice::Kick, rng.range(0.85, 1.0)));
            }
            if snares.contains(&s) {
                hits.push((at(s, &mut rng), Voice::Snare, rng.range(0.9, 1.0)));
            } else if ghosts.contains(&s) || (s % 2 == 1 && rng.chance(0.12)) {
                hits.push((at(s, &mut rng), Voice::Ghost, rng.range(0.22, 0.38)));
            }
            // Ride on the 8ths (accented on beats), hats on some off 16ths, an open hat to finish bar 2.
            if s % 2 == 0 {
                let accent = if s % 4 == 0 { 0.7 } else { 0.5 };
                hits.push((at(s, &mut rng), Voice::Ride, accent * rng.range(0.85, 1.0)));
            } else if rng.chance(0.35) {
                hits.push((at(s, &mut rng), Voice::Hat, rng.range(0.25, 0.45)));
            }
            if s == 30 {
                hits.push((at(s, &mut rng), Voice::OpenHat, 0.5));
            }
        }
        for (pos, voice, vel) in &hits {
            let sound = match voice {
                Voice::Kick => kick(*vel, &mut rng),
                Voice::Snare => snare(*vel, false, &mut rng),
                Voice::Ghost => snare(*vel, true, &mut rng),
                v => metal(*vel, *v, &mut rng),
            };
            add(&mut buf, *pos, &sound);
        }
        self.hits = hits.iter().map(|(p, v, _)| (*p, *v)).collect();
        self.hits.sort_by_key(|h| h.0);

        // The sampler: saturation, 24 kHz sample-and-hold, 12-bit, a lowpass, a small room.
        let peak = buf.iter().fold(1e-9f64, |a, x| a.max(x.abs()));
        let mut lp = OnePole::new(11_000.0, SR);
        let mut held = 0.0;
        for (i, x) in buf.iter_mut().enumerate() {
            let v = tanh(1.4 * *x / peak) / tanh(1.4);
            if i % 2 == 0 {
                held = (v * 2048.0).round() / 2048.0;
            }
            *x = lp.process(held);
        }
        let taps = [(529usize, 0.16), (811, 0.11), (1103, 0.08)];
        let dry = buf.clone();
        let mut room_lp = OnePole::new(4000.0, SR);
        for i in 0..buf.len() {
            let mut wet = 0.0;
            for (d, g) in taps {
                if i >= d {
                    wet += dry[i - d] * g;
                }
            }
            buf[i] += room_lp.process(wet);
        }
        let peak = buf.iter().fold(1e-9f64, |a, x| a.max(x.abs()));
        for x in buf.iter_mut() {
            *x = sanitize(*x * 0.9 / peak);
        }
        self.brk = buf;
    }

    /// The arrangement section for phrase `p` (pure).
    fn section(&self, p: u64, mods: &Mods) -> Section {
        if p < SILENT_PHRASES {
            return Section::Out;
        }
        if p == SILENT_PHRASES {
            return Section::HatsOnly;
        }
        let energy = mods.at(p * PHRASE_BARS * BAR).drums;
        let mut rng = Rng::stream(self.seed, 0x5EC0 ^ p);
        let e = (energy + rng.range(-0.12, 0.12)).clamp(0.0, 1.0);
        match e {
            x if x < 0.22 => Section::Out,
            x if x < 0.34 => Section::HatsOnly,
            x if x < 0.48 => Section::HalfTime,
            x if x < 0.8 => Section::Full,
            _ => Section::Rolls,
        }
    }

    /// Slices containing a kick or a snare: what jungle edits love to move around.
    fn strong_slices(&self) -> Vec<u8> {
        let mut v: Vec<u8> = self
            .hits
            .iter()
            .filter(|(_, voice)| matches!(voice, Voice::Kick | Voice::Snare))
            .map(|(p, _)| ((p + SRC_SIXTEENTH / 2) / SRC_SIXTEENTH).min(SLICES - 1) as u8)
            .collect();
        v.dedup();
        v
    }

    /// The slice plan for bar `bar` (pure).
    pub fn plan(&self, bar: u64, mods: &Mods) -> Plan {
        let section = self.section(bar / PHRASE_BARS, mods);
        let fill = bar % PHRASE_BARS == PHRASE_BARS - 1;
        let half = (bar % 2) as usize * STEPS;
        let mut steps = [Step { slice: 0, op: Op::Play }; STEPS];
        for (i, s) in steps.iter_mut().enumerate() {
            *s = if section == Section::HalfTime {
                Step { slice: (half + i / 2) as u8, op: Op::Half }
            } else {
                Step { slice: (half + i) as u8, op: Op::Play }
            };
        }
        let mut rng = Rng::stream(self.seed, 0xBA40_0000 ^ bar);
        let p = match section {
            Section::Out | Section::HalfTime => 0.0,
            Section::HatsOnly => 0.08,
            Section::Full => 0.14,
            Section::Rolls => 0.32,
        } + if fill && section != Section::Out { 0.2 } else { 0.0 };
        let strong = self.strong_slices();
        for (i, s) in steps.iter_mut().enumerate().skip(1) {
            if rng.chance(p) {
                *s = match rng.weighted(&[0.4, 0.2, 0.2, 0.2]) {
                    0 if !strong.is_empty() => Step { slice: rng.pick(&strong), op: Op::Play },
                    1 => Step { slice: s.slice, op: Op::Reverse },
                    2 => Step { slice: s.slice, op: Op::Roll },
                    _ => Step { slice: s.slice, op: Op::PitchDown },
                };
            }
            // Fill: a snare roll into the next phrase.
            if fill && i >= STEPS - 3 && section != Section::HalfTime && rng.chance(0.6) {
                let snare = strong.iter().copied().find(|x| *x as usize % 8 == 4).unwrap_or(4);
                *s = Step { slice: snare, op: Op::Roll };
            }
        }
        let mut join = [false; STEPS];
        for i in 1..STEPS {
            let (a, b) = (steps[i - 1], steps[i]);
            join[i] = match (a.op, b.op) {
                (Op::Play, Op::Play) => b.slice == a.slice + 1,
                (Op::Half, Op::Half) => b.slice == a.slice || b.slice == a.slice + 1,
                _ => false,
            };
        }
        Plan { bar, section, fill, steps, join }
    }

    fn sample_at(&self, pos: f64) -> f64 {
        let i = pos.floor();
        let f = pos - i;
        let i = i as usize;
        match (self.brk.get(i), self.brk.get(i + 1)) {
            (Some(a), Some(b)) => a + (b - a) * f,
            (Some(a), None) => *a,
            _ => 0.0,
        }
    }

    fn current(&mut self, bar: u64, mods: &Mods) -> Plan {
        let fresh = |p: &Option<Plan>| p.as_ref().is_some_and(|p| p.bar == bar);
        if !fresh(&self.plans[0]) {
            self.plans[0] = if fresh(&self.plans[1]) { self.plans[1].take() } else { Some(self.plan(bar, mods)) };
            self.plans[1] = Some(self.plan(bar + 1, mods));
        }
        self.plans[0].clone().unwrap()
    }

    /// The source range a step reads, for counting hits.
    fn range(step: Step, i: usize) -> (f64, f64) {
        let base = step.slice as f64 * SRC_SIXTEENTH as f64;
        let len = SRC_SIXTEENTH as f64;
        match step.op {
            Op::Play | Op::Reverse => (base, base + len),
            Op::Roll => (base, base + len / 2.0),
            Op::PitchDown => (base, base + len * 0.75),
            Op::Half => {
                let h = if i % 2 == 0 { 0.0 } else { len / 2.0 };
                (base + h, base + h + len / 2.0)
            }
        }
    }

    pub fn next(&mut self, clock: u64, mods: &Mods) -> f64 {
        if !self.enabled {
            return 0.0;
        }
        let bar = clock / BAR;
        let in_bar = clock % BAR;
        let i = (in_bar / SIXTEENTH) as usize;
        let t = in_bar % SIXTEENTH;
        if in_bar == 0 || self.plans[0].is_none() {
            self.current(bar, mods);
        }
        let (st, section, joined, next_joins) = {
            let plan = self.plans[0].as_ref().unwrap();
            let st = plan.steps[i];
            let next_joins = if i + 1 < STEPS {
                plan.join[i + 1]
            } else {
                self.plans[1].as_ref().is_some_and(|n| {
                    n.section == plan.section
                        && matches!((st.op, n.steps[0].op), (Op::Play, Op::Play))
                        && n.steps[0].slice as usize == (st.slice as usize + 1) % SLICES
                })
            };
            (st, plan.section, plan.join[i], next_joins)
        };

        // Retriggers: count which drum hits this step plays (display only).
        let retrigger = t == 0 || (st.op == Op::Roll && t == SIXTEENTH / 2);
        if retrigger && section != Section::Out {
            self.step = i;
            let (lo, hi) = Self::range(st, i);
            self.last_hits = self
                .hits
                .iter()
                .filter(|(p, v)| (*p as f64) >= lo && (*p as f64) < hi && (section != Section::HatsOnly || v.is_cymbal()))
                .map(|(_, v)| *v)
                .collect();
            self.hit_count += self.last_hits.len() as u64;
        }

        let base = st.slice as f64 * SRC_SIXTEENTH as f64;
        let len = SRC_SIXTEENTH as f64;
        let tt = t as f64;
        let (pos, local, local_len) = match st.op {
            Op::Play => (base + tt * RATE, t, SIXTEENTH),
            Op::Reverse => (base + len - 1.0 - tt * RATE, t, SIXTEENTH),
            Op::Roll => {
                let h = SIXTEENTH / 2;
                (base + (t % h) as f64 * RATE, t % h, h)
            }
            Op::PitchDown => (base + tt * RATE * 0.75, t, SIXTEENTH),
            Op::Half => (base + (tt + (i % 2) as f64 * SIXTEENTH as f64) * RATE * 0.5, t, SIXTEENTH),
        };
        let mut x = if section == Section::Out { 0.0 } else { self.sample_at(pos) };

        // Fade only where the sequence actually jumps.
        let joined_in = joined && st.op != Op::Roll;
        let fade_in = if joined_in && local == t { 1.0 } else { (local as f64 / DECLICK as f64).min(1.0) };
        let to_end = local_len - local;
        let fade_out = if next_joins && st.op != Op::Roll { 1.0 } else { (to_end as f64 / DECLICK as f64).min(1.0) };
        x *= fade_in * fade_out;

        // Hats-only sections: glide a high-pass in, so only the cymbals remain.
        let target = if section == Section::HatsOnly { 1.0 } else { 0.0 };
        self.hp_mix += (target - self.hp_mix) * 0.0005;
        let low = self.highpass.process(x).low;
        let high = x - low;
        x + (high - x) * self.hp_mix
    }
}

/// Read-only views for visualisation.
impl Drums {
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn pattern_name(&self) -> &'static str {
        self.pattern
    }

    /// The current bar's plan, if playing.
    pub fn plan_now(&self) -> Option<&Plan> {
        self.plans[0].as_ref()
    }

    pub fn step(&self) -> usize {
        self.step
    }

    pub fn last_hits(&self) -> &[Voice] {
        &self.last_hits
    }

    pub fn hit_count(&self) -> u64 {
        self.hit_count
    }

    /// The sampled break as `points` peak values, and where each slice's hits are.
    pub fn overview(&self, points: usize) -> Vec<f32> {
        if self.brk.is_empty() {
            return Vec::new();
        }
        let chunk = BREAK_LEN.div_ceil(points);
        self.brk[..BREAK_LEN].chunks(chunk).map(|c| c.iter().fold(0.0f64, |a, x| a.max(x.abs())) as f32).collect()
    }

    /// The main voice in each of the 32 slices (for labelling the chop grid).
    pub fn slice_voices(&self) -> Vec<Option<Voice>> {
        (0..SLICES)
            .map(|s| {
                let (lo, hi) = (s * SRC_SIXTEENTH, (s + 1) * SRC_SIXTEENTH);
                let mut best: Option<Voice> = None;
                for (p, v) in &self.hits {
                    if *p >= lo.saturating_sub(SRC_SIXTEENTH / 4) && *p < hi.saturating_sub(SRC_SIXTEENTH / 4) {
                        let rank = |v: Voice| match v {
                            Voice::Kick => 5,
                            Voice::Snare => 4,
                            Voice::Ghost => 3,
                            Voice::OpenHat => 2,
                            Voice::Ride => 1,
                            Voice::Hat => 0,
                        };
                        if best.is_none_or(|b| rank(*v) > rank(b)) {
                            best = Some(*v);
                        }
                    }
                }
                best
            })
            .collect()
    }
}

/// Constants shown in the TUI's drum-voice panel.
pub struct VoiceInfo {
    pub kick_from_hz: f64,
    pub kick_to_hz: f64,
    pub kick_drop_ms: f64,
    pub snare_body_hz: [f64; 2],
    pub metal_hz: [f64; 6],
    pub source_bpm: f64,
    pub play_bpm: f64,
    pub semitones_up: f64,
}

pub fn voice_info() -> VoiceInfo {
    let sixteenth_s = |n: f64| n / SR;
    VoiceInfo {
        kick_from_hz: 170.0,
        kick_to_hz: 48.0,
        kick_drop_ms: KICK_DROP_SECONDS * 1000.0,
        snare_body_hz: SNARE_BODY,
        metal_hz: METAL,
        source_bpm: 60.0 / (sixteenth_s(SRC_SIXTEENTH as f64) * 4.0),
        play_bpm: 60.0 / (sixteenth_s(SIXTEENTH as f64) * 4.0),
        semitones_up: 12.0 * log2(RATE),
    }
}

/// log2 by bisection on the deterministic `exp2` (display only).
fn log2(r: f64) -> f64 {
    let (mut lo, mut hi) = (-8.0f64, 8.0f64);
    for _ in 0..60 {
        let mid = 0.5 * (lo + hi);
        if crate::math::exp2(mid) < r {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}
