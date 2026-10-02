//! Jungle drums, made the way jungle producers worked, but entirely synthesised.
//!
//! 1. A two-bar breakbeat is *played* once at start-up by synthesised drum
//!    voices (from one of the kit families in `kits.rs`) at a funk-record
//!    tempo (125-150 BPM), with human micro-timing and cymbal choke.
//! 2. It is bussed for punch (compression, soft clip), then "sampled": run
//!    through a sampler colour chain (saturation, sample-and-hold, bit depth,
//!    a lowpass, a small room), all set by the kit.
//! 3. It is chopped into 32 sixteenth-note slices and re-sequenced live at
//!    168 BPM. Playing a slower break at 168 BPM is the classic jungle move:
//!    the break speeds up and its pitch rises with it.
//!
//! Every bar's slice plan is a pure function of seed 5, the bar number and
//! the slow LFOs, so playback never depends on block size or history.
//! Seed 5 = 0 switches the drums off.

use crate::filter::{OnePole, Svf};
use crate::harmony::Harmony;
use crate::kits::{self, KitParams};
use crate::math::{sanitize, tanh};
use crate::modulate::Mods;
use crate::rng::Rng;
use crate::SAMPLE_RATE;

/// A sixteenth note at 168 BPM: exactly half the glitch layers' 84 BPM sixteenth.
pub const SIXTEENTH: u64 = 4284;
pub const STEPS: usize = 16;
pub const BAR: u64 = STEPS as u64 * SIXTEENTH;
pub const PHRASE_BARS: u64 = 4;
pub const SLICES: usize = 32;
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

const SR: f64 = SAMPLE_RATE as f64;

fn add(buf: &mut [f64], at: usize, sound: &[f64]) {
    for (i, s) in sound.iter().enumerate() {
        if let Some(b) = buf.get_mut(at + i) {
            *b += s;
        }
    }
}

pub struct Drums {
    enabled: bool,
    seed: u64,
    /// The kit family and every voice's numbers.
    params: KitParams,
    /// A sixteenth at the break's source tempo, and the playback rate to 168 BPM.
    src16: usize,
    rate: f64,
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
    pub fn new(seed: u64, kit: crate::kits::Kit, harmony: &Harmony) -> Self {
        let params = KitParams::new(seed, kit, harmony);
        let src16 = (60.0 / params.source_bpm / 4.0 * SR).round() as usize;
        let mut d = Drums {
            enabled: seed != 0,
            seed,
            rate: src16 as f64 / SIXTEENTH as f64,
            src16,
            params,
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

    fn break_len(&self) -> usize {
        SLICES * self.src16
    }

    /// Plays and samples the break (once, at start-up).
    fn record_break(&mut self) {
        let mut rng = Rng::stream(self.seed, 0xB4EA);
        let (name, kicks, snares, ghosts) = PATTERNS[rng.below(PATTERNS.len())];
        self.pattern = name;
        let p = self.params.clone();
        let src16 = self.src16;
        let mut buf = vec![0.0; self.break_len() + SR as usize];
        let swing = (src16 as f64 * 0.05) as usize;
        let mut hits: Vec<(usize, Voice, f64)> = Vec::new();
        let at = |step: usize, rng: &mut Rng| {
            // Human timing: ±1.2 ms push/pull, plus a little swing on off-beats.
            let jitter = (rng.bipolar() * 58.0) as i64;
            let base = step * src16 + if step % 2 == 1 { swing } else { 0 };
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
            // The 8ths on the ride or closed hats (accents alternate), some off-16th hats,
            // and an open hat to close bar 2.
            if s % 2 == 0 {
                let accent = if s % 4 == 0 { 0.75 } else { 0.45 } * rng.range(0.85, 1.0);
                let v = if p.ride_on_8ths { Voice::Ride } else { Voice::Hat };
                hits.push((at(s, &mut rng), v, accent));
            } else if rng.chance(p.hat_density) {
                hits.push((at(s, &mut rng), Voice::Hat, rng.range(0.25, 0.45)));
            }
            if s == 30 {
                hits.push((at(s, &mut rng), Voice::OpenHat, 0.5));
            }
        }
        hits.sort_by_key(|h| h.0);
        for (i, (pos, voice, vel)) in hits.iter().enumerate() {
            let sound = match voice {
                Voice::Kick => kits::kick(&p, *vel, &mut rng),
                Voice::Snare => kits::snare(&p, *vel, false, &mut rng),
                Voice::Ghost => kits::snare(&p, *vel, true, &mut rng),
                v => {
                    // Choke: a cymbal stops when the next cymbal is struck.
                    let next = hits[i + 1..].iter().find(|h| h.1.is_cymbal()).map(|h| h.0);
                    let max_len = next.map_or(usize::MAX, |n| n.saturating_sub(*pos).max(96) + 96);
                    kits::cymbal(&p, *v, *vel, max_len, &mut rng)
                }
            };
            add(&mut buf, *pos, &sound);
        }
        self.hits = hits.iter().map(|(p, v, _)| (*p, *v)).collect();

        // Punch: compress and soft-clip the whole break, as if bussed through hardware.
        kits::punch_bus(&mut buf);

        // The sampler: saturation, sample-and-hold, bit depth, a lowpass, a small room.
        let peak = buf.iter().fold(1e-9f64, |a, x| a.max(x.abs()));
        let mut lp = OnePole::new(p.lp_hz, SR);
        let levels = (1u64 << (p.bits - 1)) as f64;
        let mut held = 0.0;
        for (i, x) in buf.iter_mut().enumerate() {
            let v = tanh(1.4 * *x / peak) / tanh(1.4);
            if i % p.hold == 0 {
                held = (v * levels).round() / levels;
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
            .map(|(p, _)| ((p + self.src16 / 2) / self.src16).min(SLICES - 1) as u8)
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
    fn range(&self, step: Step, i: usize) -> (f64, f64) {
        let base = step.slice as f64 * self.src16 as f64;
        let len = self.src16 as f64;
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
            let (lo, hi) = self.range(st, i);
            self.last_hits = self
                .hits
                .iter()
                .filter(|(p, v)| (*p as f64) >= lo && (*p as f64) < hi && (section != Section::HatsOnly || v.is_cymbal()))
                .map(|(_, v)| *v)
                .collect();
            self.hit_count += self.last_hits.len() as u64;
        }

        let base = st.slice as f64 * self.src16 as f64;
        let len = self.src16 as f64;
        let rate = self.rate;
        let tt = t as f64;
        let (pos, local, local_len) = match st.op {
            Op::Play => (base + tt * rate, t, SIXTEENTH),
            Op::Reverse => (base + len - 1.0 - tt * rate, t, SIXTEENTH),
            Op::Roll => {
                let h = SIXTEENTH / 2;
                (base + (t % h) as f64 * rate, t % h, h)
            }
            Op::PitchDown => (base + tt * rate * 0.75, t, SIXTEENTH),
            Op::Half => (base + (tt + (i % 2) as f64 * SIXTEENTH as f64) * rate * 0.5, t, SIXTEENTH),
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
        let n = self.break_len();
        let chunk = n.div_ceil(points);
        self.brk[..n].chunks(chunk).map(|c| c.iter().fold(0.0f64, |a, x| a.max(x.abs())) as f32).collect()
    }

    /// The main voice in each of the 32 slices (for labelling the chop grid).
    pub fn slice_voices(&self) -> Vec<Option<Voice>> {
        (0..SLICES)
            .map(|s| {
                let src = self.src16;
                let (lo, hi) = (s * src, (s + 1) * src);
                let mut best: Option<Voice> = None;
                for (p, v) in &self.hits {
                    if *p >= lo.saturating_sub(src / 4) && *p < hi.saturating_sub(src / 4) {
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

/// What the TUI shows about the kit.
#[derive(Clone, Debug)]
pub struct VoiceInfo {
    pub kit: &'static str,
    /// One line each for kick, snare and cymbals.
    pub lines: [String; 3],
    pub kick_from_hz: f64,
    pub kick_to_hz: f64,
    pub kick_drop_ms: f64,
    pub snare_hz: [f64; 2],
    pub cymbal_hz: Vec<f64>,
    pub source_bpm: f64,
    pub play_bpm: f64,
    pub semitones_up: f64,
    pub bits: u32,
    pub hold_khz: f64,
}

impl Drums {
    pub fn voice_info(&self) -> VoiceInfo {
        let p = &self.params;
        VoiceInfo {
            kit: if self.enabled { p.kit.name() } else { "off" },
            lines: p.describe(),
            kick_from_hz: p.kick_from,
            kick_to_hz: p.kick_to,
            kick_drop_ms: p.kick_drop * 1000.0,
            snare_hz: p.snare_tones,
            cymbal_hz: p.cymbal_freqs(),
            source_bpm: 60.0 / (self.src16 as f64 / SR * 4.0),
            play_bpm: 60.0 / (SIXTEENTH as f64 / SR * 4.0),
            semitones_up: 12.0 * log2(self.rate),
            bits: p.bits,
            hold_khz: SR / p.hold as f64 / 1000.0,
        }
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
