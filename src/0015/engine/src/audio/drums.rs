//! Jungle drums, made the way jungle producers worked, but entirely synthesised.
//!
//! 1. A two-bar breakbeat is *played* once at start-up by synthesised drum
//!    voices (from one of the kit families in `kits.rs`) at a funk-record
//!    tempo (125-150 BPM), with human micro-timing and cymbal choke.
//! 2. It is bussed for punch (compression, soft clip), then "sampled": run
//!    through a sampler colour chain (saturation, sample-and-hold, bit depth,
//!    a lowpass, a small room), all set by the kit.
//! 3. It is chopped into 32 sixteenth-note slices and re-sequenced live at
//!    the chosen tempo (168 BPM by default). Playing a slower break at jungle
//!    tempo is the classic move: the break speeds up and its pitch rises with it.
//!
//! Every bar's slice plan is a pure function of seed 5, the bar number and
//! the slow LFOs, so playback never depends on block size or history.
//! The kit "Off" switches the drums off.
//!
//! **2-step garage** plays the same machine differently. The loop is played
//! at the track's own tempo (no pitch-up), shuffled by the swing setting,
//! from rules rather than a record: a kick on the one and a second that
//! *skips* past beat three to land just after it, claps on two and four,
//! rimshots pushing the off-beats, shuffled sixteenth hats with an open hat
//! on each off-beat eighth. Its edits lean toward the glitch: hat stutters,
//! reversed claps sucking into the snare, kicks dropping out, pitched-down
//! tails; and its sections breathe between hats, a kick-less breakdown, the
//! full 2-step, and 2-step with the glitches let loose.
//!
//! **Microsound** records its loop at the track's tempo too, dead straight
//! and with no human timing (exactness is the sound): a sub-sine kick on each
//! beat, clicks of a few samples on the beats and off-beats, ticks and
//! grains scattered over the sixteenths. It skips the sampler colour, so
//! every click stays as sharp as it was made. Its edits are digital
//! failures: a step drops out, skips back to repeat the one before, or
//! ratchets four times in a sixteenth.

use crate::filter::{OnePole, Svf};
use crate::harmony::Harmony;
use crate::kits::{DrumSpace, KitParams};
use crate::math::{sanitize, tanh};
use crate::modulate::Mods;
use crate::rng::Rng;
use crate::tempo::Tempo;

pub const STEPS: usize = 16;
pub const PHRASE_BARS: u64 = 4;
pub const SLICES: usize = 32;
/// Short fades where the slice sequence is discontinuous (in samples).
const DECLICK: u64 = 24;
/// Intro: the first phrases stay ambient.
const SILENT_PHRASES: u64 = 2;
/// Snare-type hits are this much quieter when the drums are spread (Wide, Wide + tape echo).
const SPREAD_SNARE_LEVEL: f64 = 0.8;

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
    /// Silence for the step: a microsound dropout.
    Gap,
    /// Retriggered four times within the step (a 1/64 ratchet).
    Ratchet,
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
    /// 2-step garage: the kick filtered away, claps and hats floating.
    Breakdown,
    /// 2-step garage: the full groove.
    TwoStep,
    /// 2-step garage, with stutters and reversals let loose.
    Shuffle,
    /// Microsound: the clicks and grains without the kick.
    Clicks,
    /// Microsound: the full pulse.
    Pulse,
    /// Microsound: the pulse, failing more often.
    Fracture,
}

impl Section {
    pub fn name(self) -> &'static str {
        match self {
            Section::Out => "out",
            Section::HatsOnly => "hats only",
            Section::HalfTime => "half-time",
            Section::Full => "full break",
            Section::Rolls => "full + rolls",
            Section::Breakdown => "breakdown",
            Section::TwoStep => "2-step",
            Section::Shuffle => "2-step + glitch",
            Section::Clicks => "clicks only",
            Section::Pulse => "micro pulse",
            Section::Fracture => "pulse + dropouts",
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

const SR: f64 = crate::SAMPLE_RATE as f64;

fn add_scaled(buf: &mut [f64], at: usize, sound: &[f64], gain: f64) {
    if gain == 1.0 {
        return add(buf, at, sound);
    }
    for (i, s) in sound.iter().enumerate() {
        if let Some(b) = buf.get_mut(at + i) {
            *b += s * gain;
        }
    }
}

fn add(buf: &mut [f64], at: usize, sound: &[f64]) {
    for (i, s) in sound.iter().enumerate() {
        if let Some(b) = buf.get_mut(at + i) {
            *b += s;
        }
    }
}

pub struct Drums {
    enabled: bool,
    /// Whether steps may play their slice backwards.
    reverse: bool,
    seed: u64,
    /// The kit family and every voice's numbers.
    params: KitParams,
    /// A sixteenth at the break's source tempo, and the playback rate to the track's tempo.
    src16: usize,
    rate: f64,
    /// A sixteenth and a bar at the track's tempo (`Tempo::drum16`), in samples.
    six: u64,
    bar: u64,
    /// The sampled break (stereo), and the snare-type hits alone (mono) for the tape echo.
    brk: Vec<f64>,
    brk_r: Vec<f64>,
    perc: Vec<f64>,
    space: DrumSpace,
    /// (source position, voice) of every hit in the break.
    hits: Vec<(usize, Voice)>,
    pattern: &'static str,
    plans: [Option<Plan>; 2],
    highpass: [Svf; 2],
    hp_mix: f64,
    /// 2-step garage: the swing (the first sixteenth's share of each pair,
    /// 0.5 = straight), or `None` for jungle.
    swing: Option<f64>,
    /// The garage breakdown's low cut, and how far it is in.
    low_cut: [Svf; 2],
    lc_mix: f64,
    /// Microsound: its pulse, grains and dropouts, or `None` for the other modes.
    micro: Option<crate::micro::MicroSetup>,
    // Display-only.
    last_hits: Vec<Voice>,
    hit_count: u64,
    step: usize,
}

impl Drums {
    /// Enable or disable reversed slices in the break edits.
    pub fn set_reverse(&mut self, on: bool) {
        self.reverse = on;
    }

    pub fn new(seed: u64, kit: crate::kits::Kit, space: DrumSpace, harmony: &Harmony, tempo: Tempo) -> Self {
        Drums::styled(seed, kit, space, harmony, tempo, None, None)
    }

    /// 2-step garage drums, shuffled by `swing` (0.5 straight .. about 0.66).
    pub fn garage(seed: u64, kit: crate::kits::Kit, space: DrumSpace, harmony: &Harmony, tempo: Tempo, swing: f64) -> Self {
        Drums::styled(seed, kit, space, harmony, tempo, Some(swing.clamp(0.5, 0.72)), None)
    }

    /// Microsound drums: straight, exact, small.
    pub fn micro(seed: u64, kit: crate::kits::Kit, space: DrumSpace, harmony: &Harmony, tempo: Tempo, setup: crate::micro::MicroSetup) -> Self {
        Drums::styled(seed, kit, space, harmony, tempo, None, Some(setup))
    }

    fn styled(
        seed: u64,
        kit: crate::kits::Kit,
        space: DrumSpace,
        harmony: &Harmony,
        tempo: Tempo,
        swing: Option<f64>,
        micro: Option<crate::micro::MicroSetup>,
    ) -> Self {
        let mut params = KitParams::new(seed, kit, harmony);
        if swing.is_some() {
            // A rounder, shorter kick that leaves room for a moving bassline.
            params.kick_decay = params.kick_decay.min(0.3);
            params.kick_drop *= 0.8;
        }
        let six = tempo.drum16;
        // Garage and microsound play their loop at the track's own tempo, unpitched.
        let src16 = if swing.is_some() || micro.is_some() { six as usize } else { (60.0 / params.source_bpm / 4.0 * SR).round() as usize };
        let mut d = Drums {
            enabled: kit != crate::kits::Kit::Off,
            reverse: true,
            seed,
            rate: src16 as f64 / six as f64,
            src16,
            six,
            bar: STEPS as u64 * six,
            params,
            brk: Vec::new(),
            brk_r: Vec::new(),
            perc: Vec::new(),
            space,
            hits: Vec::new(),
            pattern: "off",
            plans: [None, None],
            highpass: [Svf::new(5000.0, 0.7, SR), Svf::new(5000.0, 0.7, SR)],
            hp_mix: 0.0,
            swing,
            low_cut: [Svf::new(260.0, 0.7, SR), Svf::new(260.0, 0.7, SR)],
            lc_mix: 0.0,
            micro,
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

    /// The jungle break's hits: one of the classic patterns, played with
    /// human timing at the source tempo. (Its draws on `rng` are exactly the
    /// break's original ones, so the break never changes.)
    fn jungle_hits(&mut self, rng: &mut Rng) -> Vec<(usize, Voice, f64)> {
        let (name, kicks, snares, ghosts) = PATTERNS[rng.below(PATTERNS.len())];
        self.pattern = name;
        let p = &self.params;
        let src16 = self.src16;
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
                hits.push((at(s, rng), Voice::Kick, rng.range(0.85, 1.0)));
            }
            if snares.contains(&s) {
                hits.push((at(s, rng), Voice::Snare, rng.range(0.9, 1.0)));
            } else if ghosts.contains(&s) || (s % 2 == 1 && rng.chance(0.12)) {
                hits.push((at(s, rng), Voice::Ghost, rng.range(0.22, 0.38)));
            }
            // The 8ths on the ride or closed hats (accents alternate), some off-16th hats,
            // and an open hat to close bar 2.
            if s % 2 == 0 {
                let accent = if s % 4 == 0 { 0.75 } else { 0.45 } * rng.range(0.85, 1.0);
                let v = if p.ride_on_8ths { Voice::Ride } else { Voice::Hat };
                hits.push((at(s, rng), v, accent));
            } else if rng.chance(p.hat_density) {
                hits.push((at(s, rng), Voice::Hat, rng.range(0.25, 0.45)));
            }
            if s == 30 {
                hits.push((at(s, rng), Voice::OpenHat, 0.5));
            }
        }
        hits.sort_by_key(|h| h.0);
        hits
    }

    /// The 2-step garage loop's hits, at the track's own tempo, from the
    /// genre's rules with seed 5 choosing within them.
    fn garage_hits(&mut self, rng: &mut Rng) -> Vec<(usize, Voice, f64)> {
        let six = self.src16;
        let p = &self.params;
        let swing = self.swing.unwrap_or(0.5);
        // Each odd sixteenth is pushed late by the swing.
        let push = ((2.0 * swing - 1.0) * six as f64) as usize;
        // A hair of human timing, only ever late: every hit begins inside its
        // own slice, so moving or fading a slice never splits a hit.
        let at = |step: usize, rng: &mut Rng| {
            let jitter = (rng.unit() * 24.0) as usize;
            step * six + if step % 2 == 1 { push } else { 0 } + jitter
        };
        // The skip: bar A's second kick lands just past beat three (the "and"
        // or the "a"), bar B's anywhere from just before to well after it.
        let skip_a = rng.pick(&[10usize, 11]);
        let skip_b = rng.pick(&[9usize, 10, 11, 14]);
        let pickup = rng.chance(0.45);
        let ghost_kick = if rng.chance(0.4) { Some(rng.pick(&[3usize, 7])) } else { None };
        let rim_a = rng.pick(&[6usize, 7, 14]);
        let rim_b = rng.pick(&[6usize, 14, 15]);
        // Open hats on the off-beat eighths: always on two of them.
        let open: [bool; 4] = [true, rng.chance(0.7), true, rng.chance(0.7)];
        let name = match (skip_a, pickup) {
            (10, false) => "2-step: skip on the and of 3",
            (10, true) => "2-step: skip on the and of 3, pickup",
            (_, false) => "2-step: skip on the a of 3",
            (_, true) => "2-step: skip on the a of 3, pickup",
        };
        let mut hits: Vec<(usize, Voice, f64)> = Vec::new();
        for s in 0..SLICES {
            let (b, second) = (s % STEPS, s >= STEPS);
            let kick = b == 0 || (!second && b == skip_a) || (second && b == skip_b);
            if kick {
                hits.push((at(s, rng), Voice::Kick, rng.range(0.9, 1.0)));
            } else if second && pickup && b == 15 {
                hits.push((at(s, rng), Voice::Kick, rng.range(0.55, 0.7)));
            } else if !second && ghost_kick == Some(b) {
                hits.push((at(s, rng), Voice::Kick, rng.range(0.4, 0.5)));
            }
            if b == 4 || b == 12 {
                hits.push((at(s, rng), Voice::Snare, rng.range(0.88, 1.0)));
            } else if (!second && b == rim_a) || (second && b == rim_b) || (b % 2 == 1 && rng.chance(0.08)) {
                hits.push((at(s, rng), Voice::Ghost, rng.range(0.25, 0.4)));
            }
            // Shuffled sixteenth hats, an open hat on the off-beat eighths.
            if b % 4 == 2 && open[b / 4] {
                hits.push((at(s, rng), Voice::OpenHat, rng.range(0.42, 0.55)));
            } else if b % 2 == 0 {
                let accent = if b % 4 == 0 { 0.5 } else { 0.4 } * rng.range(0.85, 1.0);
                hits.push((at(s, rng), Voice::Hat, accent));
            } else if rng.chance(0.55 + 0.4 * p.hat_density) {
                hits.push((at(s, rng), Voice::Hat, rng.range(0.2, 0.34)));
            }
        }
        self.pattern = name;
        hits.sort_by_key(|h| h.0);
        hits
    }

    /// The microsound loop's hits: exactly on the sixteenths, from the
    /// pulse and grain settings with seed 5 choosing within them.
    fn micro_hits(&mut self, rng: &mut Rng) -> Vec<(usize, Voice, f64)> {
        let six = self.src16;
        let m = self.micro.unwrap_or(crate::micro::MicroSetup::DEFAULT);
        let g = m.grains.amount();
        let kick = m.pulse != crate::micro::Pulse::Off;
        let click = m.pulse == crate::micro::Pulse::SubClick;
        // Bar B may leave one beat's kick out.
        let skip = if rng.chance(0.5) { Some(rng.pick(&[4usize, 8, 12])) } else { None };
        let mut hits: Vec<(usize, Voice, f64)> = Vec::new();
        for s in 0..SLICES {
            let (b, second) = (s % STEPS, s >= STEPS);
            let at = s * six;
            if kick && b % 4 == 0 && !(second && skip == Some(b)) {
                hits.push((at, Voice::Kick, rng.range(0.85, 1.0)));
            }
            if click {
                if b % 4 == 0 {
                    hits.push((at, Voice::Snare, rng.range(0.35, 0.5)));
                } else if b % 4 == 2 && rng.chance(0.5) {
                    hits.push((at, Voice::Snare, rng.range(0.25, 0.4)));
                }
            }
            // Ticks on the sixteenths, softer off the eighths.
            if rng.chance(0.25 + 0.45 * g) {
                let accent = if b % 2 == 0 { 1.0 } else { 0.75 };
                hits.push((at, Voice::Hat, rng.range(0.12, 0.35) * accent));
            }
            if rng.chance(0.08 + 0.22 * g) {
                hits.push((at, Voice::Ghost, rng.range(0.2, 0.5)));
            }
            if rng.chance(0.03 + 0.04 * g) {
                hits.push((at, Voice::OpenHat, rng.range(0.25, 0.35)));
            }
        }
        self.pattern = match (kick, skip.is_some()) {
            (false, _) => "micro: clicks and grains, no pulse",
            (true, false) => "micro: four-on-the-floor sub",
            (true, true) => "micro: four-on-the-floor sub, a beat left out",
        };
        hits.sort_by_key(|h| h.0);
        hits
    }

    /// Plays and samples the break (once, at start-up).
    fn record_break(&mut self) {
        let garage = self.swing.is_some();
        let micro = self.micro.is_some();
        let mut rng = Rng::stream(self.seed, if micro { 0x31C0 } else if garage { 0x2577 } else { 0xB4EA });
        let hits = if micro {
            self.micro_hits(&mut rng)
        } else if garage {
            self.garage_hits(&mut rng)
        } else {
            self.jungle_hits(&mut rng)
        };
        let click_on_kick = self.micro.is_some_and(|m| m.pulse == crate::micro::Pulse::SubClick);
        let p = self.params.clone();
        let len = self.break_len() + SR as usize;
        let (mut buf, mut buf_r, mut perc) = (vec![0.0; len], vec![0.0; len], vec![0.0; len]);
        // Panning has its own random stream, so it never shifts the break itself.
        let mut pan_rng = Rng::stream(self.seed, 0x9A4);
        let space = self.space;
        for (i, (pos, voice, vel)) in hits.iter().enumerate() {
            let sound = match voice {
                Voice::Kick if micro => {
                    let mut k = crate::kits::sub_kick(&p, *vel);
                    if click_on_kick {
                        for (j, x) in crate::kits::micro_click(&p, 0.5 * vel, false, &mut rng).iter().enumerate() {
                            k[j] += x;
                        }
                    }
                    k
                }
                Voice::Snare if micro => crate::kits::micro_click(&p, *vel, false, &mut rng),
                Voice::Hat if micro => crate::kits::micro_click(&p, *vel, true, &mut rng),
                Voice::Ghost | Voice::OpenHat | Voice::Ride if micro => crate::kits::micro_grain(&p, *vel, &mut rng),
                Voice::Kick => crate::kits::kick(&p, *vel, &mut rng),
                Voice::Snare if garage => {
                    // A clap over the kit's snare, set a little back.
                    let mut c = crate::kits::clap(&p, *vel, &mut rng);
                    let s = crate::kits::snare(&p, *vel * 0.7, false, &mut rng);
                    for (k, x) in s.iter().enumerate() {
                        if let Some(y) = c.get_mut(k) {
                            *y += 0.45 * x;
                        }
                    }
                    c
                }
                Voice::Snare => crate::kits::snare(&p, *vel, false, &mut rng),
                Voice::Ghost => crate::kits::snare(&p, *vel, true, &mut rng),
                v => {
                    // Choke: a cymbal stops when the next cymbal is struck.
                    let next = hits[i + 1..].iter().find(|h| h.1.is_cymbal()).map(|h| h.0);
                    let max_len = next.map_or(usize::MAX, |n| n.saturating_sub(*pos).max(96) + 96);
                    crate::kits::cymbal(&p, *v, *vel, max_len, &mut rng)
                }
            };
            // Where this hit sits: kick centred; snare-type hits 45–65% to a random side,
            // cymbals 50–75% (never near centre, so the clicks always use the space).
            // (2-step garage keeps its claps in the middle with the kick: they
            // are the backbeat. Its rims and ghosts still spread out.)
            // (Microsound places every small sound exactly, anywhere across
            // the field; the kick stays in the middle.)
            let (lo, hi) = match voice {
                Voice::Kick => (0.0, 0.0),
                _ if micro => (0.2, 0.95),
                Voice::Snare if garage => (0.0, 0.0),
                Voice::Snare | Voice::Ghost => (0.45, 0.65),
                _ => (0.5, 0.75),
            };
            let pan = if space == DrumSpace::Centred || hi == 0.0 {
                0.0
            } else {
                let side = if pan_rng.chance(0.5) { -1.0 } else { 1.0 };
                side * pan_rng.range(lo, hi)
            };
            // Panning only turns the far side down, so a panned hit never drives the bus
            // harder than a centred one; snares also sit a little lower when spread.
            let (gl, gr) = if pan == 0.0 {
                (1.0, 1.0)
            } else {
                let (a, b) = crate::math::pan_gains(pan);
                let level = if matches!(voice, Voice::Snare | Voice::Ghost) { SPREAD_SNARE_LEVEL } else { 1.0 };
                (level * a / a.max(b), level * b / a.max(b))
            };
            add_scaled(&mut buf, *pos, &sound, gl);
            add_scaled(&mut buf_r, *pos, &sound, gr);
            if space == DrumSpace::Tape && matches!(voice, Voice::Snare | Voice::Ghost) {
                add(&mut perc, *pos, &sound);
            }
        }
        self.hits = hits.iter().map(|(p, v, _)| (*p, *v)).collect();

        if micro {
            // No bus, no sampler, no room: each click as sharp as it was
            // made. A fixed level, so a loop of only grains stays small.
            let peak = buf.iter().chain(buf_r.iter()).fold(1.0f64, |a, x| a.max(x.abs()));
            for x in buf.iter_mut().chain(buf_r.iter_mut()) {
                *x = sanitize(*x * 0.9 / peak);
            }
            self.brk = buf;
            self.brk_r = buf_r;
            self.perc = perc;
            return;
        }

        // Punch: compress and soft-clip the whole break, as if bussed through hardware.
        crate::kits::punch_bus(&mut buf, &mut buf_r, p.punch);

        // The sampler: saturation, sample-and-hold, bit depth, a lowpass.
        let joint = |l: &[f64], r: &[f64]| l.iter().zip(r).fold(1e-9f64, |a, (x, y)| a.max(x.abs()).max(y.abs()));
        let levels = (1u64 << (p.bits - 1)) as f64;
        let sample = |b: &mut [f64], peak: f64| {
            let mut lp = OnePole::new(p.lp_hz, SR);
            let mut held = 0.0;
            for (i, x) in b.iter_mut().enumerate() {
                let v = tanh(1.4 * *x / peak) / tanh(1.4);
                if i % p.hold == 0 {
                    held = (v * levels).round() / levels;
                }
                *x = lp.process(held);
            }
        };
        let peak = joint(&buf, &buf_r);
        sample(&mut buf, peak);
        sample(&mut buf_r, peak);

        // A small room; wide spaces cross-feed it a little between the channels.
        let taps = [(529usize, 0.16), (811, 0.11), (1103, 0.08)];
        let cross = if space == DrumSpace::Centred { 0.0 } else { 0.15 };
        let (dry_l, dry_r) = (buf.clone(), buf_r.clone());
        let mut room_lp = [OnePole::new(4000.0, SR), OnePole::new(4000.0, SR)];
        for i in 0..len {
            let (mut wl, mut wr) = (0.0, 0.0);
            for (d, g) in taps {
                if i >= d {
                    if cross == 0.0 {
                        wl += dry_l[i - d] * g;
                        wr += dry_r[i - d] * g;
                    } else {
                        wl += (dry_l[i - d] * (1.0 - cross) + dry_r[i - d] * cross) * g;
                        wr += (dry_r[i - d] * (1.0 - cross) + dry_l[i - d] * cross) * g;
                    }
                }
            }
            buf[i] += room_lp[0].process(wl);
            buf_r[i] += room_lp[1].process(wr);
        }
        let peak = joint(&buf, &buf_r);
        for x in buf.iter_mut().chain(buf_r.iter_mut()) {
            *x = sanitize(*x * 0.9 / peak);
        }
        if space == DrumSpace::Tape {
            let peak = perc.iter().fold(1e-9f64, |a, x| a.max(x.abs()));
            sample(&mut perc, peak);
            let peak = perc.iter().fold(1e-9f64, |a, x| a.max(x.abs()));
            for x in perc.iter_mut() {
                *x = sanitize(*x * 0.9 / peak);
            }
        }
        self.brk = buf;
        self.brk_r = buf_r;
        self.perc = perc;
    }

    /// The arrangement section for phrase `p` (pure).
    fn section(&self, p: u64, mods: &Mods) -> Section {
        if p < SILENT_PHRASES {
            return Section::Out;
        }
        if p == SILENT_PHRASES {
            return Section::HatsOnly;
        }
        if self.swing.is_some() {
            return self.garage_section(p, mods);
        }
        if self.micro.is_some() {
            return self.micro_section(p, mods);
        }
        let energy = mods.at(p * PHRASE_BARS * self.bar).drums;
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

    /// A garage phrase's section: hats, the breakdown, the 2-step, or the
    /// 2-step with its glitches, by the same energy LFO as jungle's.
    fn garage_section(&self, p: u64, mods: &Mods) -> Section {
        let energy = mods.at(p * PHRASE_BARS * self.bar).drums;
        let mut rng = Rng::stream(self.seed, 0x2577_5EC0 ^ p);
        let e = (energy + rng.range(-0.12, 0.12)).clamp(0.0, 1.0);
        match e {
            x if x < 0.2 => Section::Out,
            x if x < 0.32 => Section::HatsOnly,
            x if x < 0.46 => Section::Breakdown,
            x if x < 0.8 => Section::TwoStep,
            _ => Section::Shuffle,
        }
    }

    /// A microsound phrase's section: clicks alone, the pulse, or the pulse
    /// failing, by the same energy LFO.
    fn micro_section(&self, p: u64, mods: &Mods) -> Section {
        let energy = mods.at(p * PHRASE_BARS * self.bar).drums;
        let mut rng = Rng::stream(self.seed, 0x31C0_5EC0 ^ p);
        let e = (energy + rng.range(-0.12, 0.12)).clamp(0.0, 1.0);
        match e {
            x if x < 0.2 => Section::Out,
            x if x < 0.36 => Section::Clicks,
            x if x < 0.82 => Section::Pulse,
            _ => Section::Fracture,
        }
    }

    /// The slice plan for a microsound bar (pure): the loop in order, with
    /// digital failures on top — a step dropping out, skipping back to repeat
    /// the one before, or ratcheting.
    fn micro_plan(&self, bar: u64, mods: &Mods) -> Plan {
        let section = self.section(bar / PHRASE_BARS, mods);
        let fill = bar % PHRASE_BARS == PHRASE_BARS - 1;
        let half = (bar % 2) as usize * STEPS;
        let mut steps = [Step { slice: 0, op: Op::Play }; STEPS];
        for (i, s) in steps.iter_mut().enumerate() {
            *s = Step { slice: (half + i) as u8, op: Op::Play };
        }
        let mut rng = Rng::stream(self.seed, 0x31C0_BA40_0000 ^ bar);
        let base = self.micro.map_or(0.0, |m| m.dropouts.chance());
        let p = if section == Section::Out || base == 0.0 {
            0.0
        } else {
            base + if section == Section::Fracture { 0.12 } else { 0.0 } + if fill { 0.1 } else { 0.0 }
        };
        for i in 1..STEPS {
            if rng.chance(p) {
                let here = steps[i].slice;
                steps[i] = match rng.weighted(&[0.3, 0.25, 0.25, 0.2]) {
                    0 => Step { slice: here, op: Op::Gap },
                    1 => Step { slice: steps[i - 1].slice, op: Op::Play },
                    2 => Step { slice: here, op: Op::Ratchet },
                    _ => Step { slice: here, op: Op::Roll },
                };
            }
        }
        let mut join = [false; STEPS];
        for i in 1..STEPS {
            let (a, b) = (steps[i - 1], steps[i]);
            join[i] = a.op == Op::Play && b.op == Op::Play && b.slice == a.slice + 1;
        }
        Plan { bar, section, fill, steps, join }
    }

    /// The slice plan for a garage bar (pure): the loop in order, with
    /// glitch edits on top.
    fn garage_plan(&self, bar: u64, mods: &Mods) -> Plan {
        let section = self.section(bar / PHRASE_BARS, mods);
        let fill = bar % PHRASE_BARS == PHRASE_BARS - 1;
        let half = (bar % 2) as usize * STEPS;
        let mut steps = [Step { slice: 0, op: Op::Play }; STEPS];
        for (i, s) in steps.iter_mut().enumerate() {
            *s = Step { slice: (half + i) as u8, op: Op::Play };
        }
        let mut rng = Rng::stream(self.seed, 0x2577_BA40_0000 ^ bar);
        let p = match section {
            Section::Out => 0.0,
            Section::HatsOnly => 0.06,
            Section::Breakdown => 0.1,
            Section::Shuffle => 0.26,
            _ => 0.08,
        } + if fill && section != Section::Out { 0.15 } else { 0.0 };
        let strong = self.strong_slices();
        // Slices holding only hats and rims: what a stutter or a dropped kick
        // borrows — only from the same place in the swing (an on-beat slice
        // for an on-beat step, a pushed one for a pushed step), so a borrowed
        // hit lands exactly where that step's own would.
        let weak_at = |i: usize| -> Vec<u8> { (0..SLICES as u8).filter(|s| !strong.contains(s) && *s as usize % 2 == i % 2).collect() };
        for (i, s) in steps.iter_mut().enumerate().skip(1) {
            let weak = weak_at(i);
            if rng.chance(p) {
                *s = match rng.weighted(&[0.35, 0.2, 0.2, 0.25]) {
                    // A hat stutter.
                    0 if !weak.is_empty() => Step { slice: rng.pick(&weak), op: Op::Roll },
                    // A clap reversed into the beat after it.
                    1 if self.reverse => Step { slice: if i % 8 == 3 { s.slice + 1 } else { s.slice }, op: Op::Reverse },
                    // The kick drops out for a beat.
                    2 if strong.contains(&s.slice) && !weak.is_empty() => Step { slice: rng.pick(&weak), op: Op::Play },
                    3 => Step { slice: s.slice, op: Op::PitchDown },
                    _ => *s,
                };
            }
            // Fill: a reversed clap and a stutter into the next phrase.
            if fill && section != Section::Out {
                if i == STEPS - 2 && self.reverse && rng.chance(0.6) {
                    *s = Step { slice: (half + 12) as u8, op: Op::Reverse };
                } else if i == STEPS - 1 && rng.chance(0.6) {
                    *s = Step { slice: s.slice, op: Op::Roll };
                }
            }
        }
        let mut join = [false; STEPS];
        for i in 1..STEPS {
            let (a, b) = (steps[i - 1], steps[i]);
            join[i] = a.op == Op::Play && b.op == Op::Play && b.slice == a.slice + 1;
        }
        Plan { bar, section, fill, steps, join }
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
        if self.swing.is_some() {
            return self.garage_plan(bar, mods);
        }
        if self.micro.is_some() {
            return self.micro_plan(bar, mods);
        }
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
            // Garage and microsound sections never reach the jungle plan.
            Section::Breakdown | Section::TwoStep | Section::Shuffle | Section::Clicks | Section::Pulse | Section::Fracture => 0.0,
        } + if fill && section != Section::Out { 0.2 } else { 0.0 };
        let strong = self.strong_slices();
        for (i, s) in steps.iter_mut().enumerate().skip(1) {
            if rng.chance(p) {
                *s = match rng.weighted(&[0.4, 0.2, 0.2, 0.2]) {
                    0 if !strong.is_empty() => Step { slice: rng.pick(&strong), op: Op::Play },
                    1 => Step { slice: s.slice, op: if self.reverse { Op::Reverse } else { Op::Play } },
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

    fn sample_at(&self, pos: f64) -> (f64, f64, f64) {
        let i = pos.floor();
        let f = pos - i;
        let i = i as usize;
        let read = |b: &[f64]| match (b.get(i), b.get(i + 1)) {
            (Some(a), Some(c)) => a + (c - a) * f,
            (Some(a), None) => *a,
            _ => 0.0,
        };
        (read(&self.brk), read(&self.brk_r), if self.perc.is_empty() { 0.0 } else { read(&self.perc) })
    }

    fn current(&mut self, bar: u64, mods: &Mods) -> Plan {
        let fresh = |p: &Option<Plan>| p.as_ref().is_some_and(|p| p.bar == bar);
        if !fresh(&self.plans[0]) {
            self.plans[0] = if fresh(&self.plans[1]) { self.plans[1].take() } else { Some(self.plan(bar, mods)) };
            self.plans[1] = Some(self.plan(bar + 1, mods));
        }
        self.plans[0].clone().unwrap()
    }

    /// Where a stutter retriggers within a step, and where in the slice the
    /// repeat reads from. Halfway, from the slice's start — except a swung
    /// garage off-beat, whose hit sits late in its slice: there the repeat
    /// starts at the hit and lands halfway through the rest, so both strikes
    /// stay on the swung grid.
    fn roll_split(&self, slice: u8) -> (u64, u64) {
        let six = self.six;
        match self.swing {
            Some(swing) if slice % 2 == 1 => {
                let push = ((2.0 * swing - 1.0) * six as f64) as u64;
                (push + (six - push) / 2, push)
            }
            _ => (six / 2, 0),
        }
    }

    /// The source range a step reads, for counting hits.
    fn range(&self, step: Step, i: usize) -> (f64, f64) {
        let base = step.slice as f64 * self.src16 as f64;
        let len = self.src16 as f64;
        match step.op {
            Op::Play | Op::Reverse => (base, base + len),
            Op::Roll => (base, base + len / 2.0),
            Op::Ratchet => (base, base + len / 4.0),
            Op::Gap => (base, base),
            Op::PitchDown => (base, base + len * 0.75),
            Op::Half => {
                let h = if i % 2 == 0 { 0.0 } else { len / 2.0 };
                (base + h, base + h + len / 2.0)
            }
        }
    }

    /// (left, right, snare-type hits for the tape echo)
    pub fn next(&mut self, clock: u64, mods: &Mods) -> (f64, f64, f64) {
        if !self.enabled {
            return (0.0, 0.0, 0.0);
        }
        let six = self.six;
        let bar = clock / self.bar;
        let in_bar = clock % self.bar;
        let i = (in_bar / six) as usize;
        let t = in_bar % six;
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
        let (roll_at, roll_from) = self.roll_split(st.slice);
        let quarter = (six / 4).max(1);
        let retrigger = t == 0 || (st.op == Op::Roll && t == roll_at) || (st.op == Op::Ratchet && t % quarter == 0);
        if retrigger && section != Section::Out {
            self.step = i;
            let (lo, hi) = self.range(st, i);
            self.last_hits = self
                .hits
                .iter()
                .filter(|(p, v)| {
                    (*p as f64) >= lo
                        && (*p as f64) < hi
                        && (section != Section::HatsOnly || v.is_cymbal())
                        && (section != Section::Clicks || *v != Voice::Kick)
                })
                .map(|(_, v)| *v)
                .collect();
            self.hit_count += self.last_hits.len() as u64;
        }

        let base = st.slice as f64 * self.src16 as f64;
        let len = self.src16 as f64;
        let rate = self.rate;
        let tt = t as f64;
        let (pos, local, local_len) = match st.op {
            Op::Play => (base + tt * rate, t, six),
            Op::Reverse => (base + len - 1.0 - tt * rate, t, six),
            Op::Roll => {
                if t < roll_at {
                    (base + tt * rate, t, roll_at)
                } else {
                    let u = t - roll_at;
                    (base + (roll_from + u) as f64 * rate, u, six - roll_at)
                }
            }
            Op::PitchDown => (base + tt * rate * 0.75, t, six),
            Op::Ratchet => {
                let u = t % quarter;
                (base + u as f64 * rate, u, quarter)
            }
            Op::Gap => (base, t, six),
            Op::Half => (base + (tt + (i % 2) as f64 * six as f64) * rate * 0.5, t, six),
        };
        let (mut x, mut xr, mut xp) = if section == Section::Out || st.op == Op::Gap { (0.0, 0.0, 0.0) } else { self.sample_at(pos) };

        // Fade only where the sequence actually jumps.
        let joined_in = joined && !matches!(st.op, Op::Roll | Op::Ratchet);
        let fade_in = if joined_in && local == t { 1.0 } else { (local as f64 / DECLICK as f64).min(1.0) };
        let to_end = local_len - local;
        let fade_out = if next_joins && !matches!(st.op, Op::Roll | Op::Ratchet) { 1.0 } else { (to_end as f64 / DECLICK as f64).min(1.0) };
        x *= fade_in * fade_out;
        xr *= fade_in * fade_out;
        xp *= fade_in * fade_out;

        // Hats-only sections: glide a high-pass in, so only the cymbals remain
        // (and the snare-only tape send fades out with it).
        let target = if matches!(section, Section::HatsOnly | Section::Clicks) { 1.0 } else { 0.0 };
        self.hp_mix += (target - self.hp_mix) * 0.0005;
        let low = self.highpass[0].process(x).low;
        let high = x - low;
        let low_r = self.highpass[1].process(xr).low;
        let high_r = xr - low_r;
        let (mut l, mut r) = (x + (high - x) * self.hp_mix, xr + (high_r - xr) * self.hp_mix);
        if self.swing.is_some() {
            // The garage breakdown: the kick and the bass end of the loop
            // filtered away, so the claps and hats float.
            let target = if section == Section::Breakdown { 1.0 } else { 0.0 };
            self.lc_mix += (target - self.lc_mix) * 0.0004;
            let (ll, lr) = (self.low_cut[0].process(l).low, self.low_cut[1].process(r).low);
            l -= ll * self.lc_mix;
            r -= lr * self.lc_mix;
        }
        (l, r, xp * (1.0 - self.hp_mix))
    }
}

/// Read-only views for visualisation.
impl Drums {
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn space(&self) -> DrumSpace {
        self.space
    }

    pub fn pattern_name(&self) -> &'static str {
        self.pattern
    }

    /// Every hit of the recorded loop: (source position in samples, voice).
    pub fn hits(&self) -> &[(usize, Voice)] {
        &self.hits
    }

    /// A sixteenth and a bar at the track's tempo, in samples.
    pub fn sixteenth(&self) -> u64 {
        self.six
    }

    pub fn bar(&self) -> u64 {
        self.bar
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
        let both: Vec<f64> = self.brk[..n].iter().zip(&self.brk_r[..n]).map(|(l, r)| l.abs().max(r.abs())).collect();
        both.chunks(chunk).map(|c| c.iter().fold(0.0f64, |a, x| a.max(x.abs())) as f32).collect()
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
            play_bpm: 60.0 / (self.six as f64 / SR * 4.0),
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
