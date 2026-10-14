//! 0011 — an endless Glitch Ambient track with jungle breaks, built as a
//! software artifact.
//!
//! ```text
//! seed1 → chord progression (scale, key) → 3-op FM chords → random-phase FFT freeze + morphs → filter → drone arc ─┐
//! seed2 → glitch artifacts 1 → cyclic repeats 1 ─┐                                                    │
//! seed3 → glitch artifacts 2 → cyclic repeats 2 ─┴→ echo/delay ───────────────────────────────────────┼→ long reverb
//! seed4 → FM bass + sub-bass (follows chord roots) → low-pass ─────────────────────────────────────────┤
//! seed5 → synthesised break → sampler colour → chopper / arranger → drum bus ─────────────────────────┘
//! ```
//!
//! Determinism is the core design constraint: given the same seeds, the
//! output is bit-identical on every platform (native and WASM), at any
//! render block size, and with any future compiler. See `math.rs`.




use crate::atmos::{AtmosLoop, LoopTimbre};
use crate::texture::Grid;
use crate::bass::Bass;
use crate::delay::{PingPong, TapeEcho};
use crate::drone::Drone;
use crate::drums::Drums;
use crate::harmony::{Harmony, Pace, Scale};
use crate::master::{MasterChain, MasterFx};
use crate::overlay::CodeOverlay;
use crate::rng::Rng;
use crate::filter::{DcBlock, OnePole, Svf};
use crate::glitch::History;
use crate::modulate::Mods;
use crate::pattern::Layer;
use crate::reverb::Reverb;
use crate::telemetry::Meters;
use crate::tempo::Tempo;


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seeds {
    pub s1: u64,
    pub s2: u64,
    pub s3: u64,
    pub s4: u64,
    /// Shapes the drum kit's sounds and break (the kit itself is a setting).
    pub s5: u64,
    /// Shapes the atmosphere loops (their timbres are settings).
    pub s6: u64,
    /// Decides the drone's rhythmic amplitude LFO (off by default).
    pub s7: u64,
}

/// The canonical version of the track.
pub const DEFAULT_SEEDS: Seeds = Seeds { s1: 1000, s2: 9, s3: 1009, s4: 2026, s5: 168, s6: 11, s7: 7 };

/// How one atmosphere layer is made: one of the sustained timbres, or an
/// editable event grid of physical-texture grains (fire, water, stones, wood).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopDesign {
    Sustained(LoopTimbre),
    Events(Grid),
}

impl LoopDesign {
    /// No atmosphere on this layer.
    pub const OFF: LoopDesign = LoopDesign::Sustained(LoopTimbre::Off);

    pub fn is_on(&self) -> bool {
        match self {
            LoopDesign::Sustained(t) => *t != LoopTimbre::Off,
            LoopDesign::Events(g) => !g.is_empty(),
        }
    }

    /// The sustained timbre, if this is one.
    pub fn timbre(&self) -> Option<LoopTimbre> {
        match self {
            LoopDesign::Sustained(t) => Some(*t),
            LoopDesign::Events(_) => None,
        }
    }

    /// The recipe token: a timbre code, or `E` + 16 hex digits for a grid.
    pub fn token(&self) -> String {
        match self {
            LoopDesign::Sustained(t) => t.code().to_string(),
            LoopDesign::Events(g) => format!("E{}", g.to_hex()),
        }
    }

    pub fn from_token(s: &str) -> Option<LoopDesign> {
        if let Some(hex) = s.strip_prefix('E').or_else(|| s.strip_prefix('e')) {
            return Grid::from_hex(hex).map(LoopDesign::Events);
        }
        LoopTimbre::from_code(s).map(LoopDesign::Sustained)
    }

    pub fn name(&self) -> String {
        match self {
            LoopDesign::Sustained(t) => t.name().to_string(),
            LoopDesign::Events(g) if g.is_empty() => "Event grid (empty)".to_string(),
            LoopDesign::Events(_) => "Event grid".to_string(),
        }
    }

    /// A very short label for a box or header.
    pub fn short(&self) -> &'static str {
        match self {
            LoopDesign::Sustained(t) => t.code(),
            LoopDesign::Events(_) => "EVT",
        }
    }

    pub fn blurb(&self) -> String {
        match self {
            LoopDesign::Sustained(t) => t.blurb().to_string(),
            LoopDesign::Events(_) => "fire, water, stones, wood: grains on an editable grid".to_string(),
        }
    }
}

/// Where the tonic comes from: the seed (the default, unchanged), or an
/// explicit pitch class the player chooses at setup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyChoice {
    Seed,
    /// Pitch class 0..11 (0 = C).
    Note(u8),
}

/// A repeating entrance/exit arc for the frozen-chord drone: every `cycle_s`
/// seconds the drone swells up for `hold_s` seconds, holds, then fades back to
/// silence until the cycle comes round again. `cycle_s == 0` means always on.
/// Timing is in whole seconds (like the slow form cycles), so changing the BPM
/// moves the rhythm but leaves this long breathing of the piece alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DroneArc {
    pub cycle_s: u16,
    pub hold_s: u16,
}

impl DroneArc {
    /// No arc: the drone plays continuously, exactly as it always has.
    pub const ALWAYS_ON: DroneArc = DroneArc { cycle_s: 0, hold_s: 0 };

    /// Whether the arc gates the drone at all.
    pub fn is_on(&self) -> bool {
        self.cycle_s > 0
    }

    /// (cycle, hold) in samples; the hold is clamped to the cycle.
    pub fn samples(&self) -> (u64, u64) {
        let sr = crate::SAMPLE_RATE as u64;
        let cycle = self.cycle_s as u64 * sr;
        (cycle, (self.hold_s as u64 * sr).min(cycle))
    }

    /// Gain 0..1 at `clock`: a raised-cosine swell at each end of the hold,
    /// flat in between, and silence for the rest of the cycle. Always exactly
    /// 1.0 when the arc is off, so the default sound is untouched.
    pub fn gain(&self, clock: u64) -> f64 {
        let (cycle, hold) = self.samples();
        if cycle == 0 {
            return 1.0;
        }
        let p = clock % cycle;
        if p >= hold {
            return 0.0;
        }
        // A quarter of the hold, no shorter than 2 s and no longer than 15 s
        // (and never more than half the hold, so the plateau survives).
        let sr = crate::SAMPLE_RATE as u64;
        let fade = (hold / 4).clamp(2 * sr, 15 * sr).min(hold / 2).max(1);
        if p < fade {
            0.5 - 0.5 * crate::math::cos_turns(0.5 * p as f64 / fade as f64)
        } else if p + fade > hold {
            0.5 - 0.5 * crate::math::cos_turns(0.5 * (hold - p) as f64 / fade as f64)
        } else {
            1.0
        }
    }
}

/// Musical choices made up front (the seeds fill in the details).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub scale: Scale,
    /// 1 to 4.
    pub chords: u8,
    pub pace: Pace,
    /// The drum kit family (seed 5 shapes it).
    pub kit: crate::kits::Kit,
    /// Where the drums sit in the stereo field.
    pub space: crate::kits::DrumSpace,    /// The drum tempo; every ambient layer moves at exactly half of it.
    pub bpm: u16,
    /// The four atmosphere loop layers (seed 6 shapes the sustained ones).
    pub loops: [LoopDesign; 4],
    /// A fade-in/out arc per atmosphere layer (cycle 0 = always on).
    pub loop_arcs: [DroneArc; 4],
    /// Semitones the atmosphere loops are transposed by (0 = the chord tones
    /// exactly, so they match the chosen root note).
    pub loop_transpose: i8,
    /// Where the tonic comes from (seed 1 picks it by default).
    pub key: KeyChoice,
    /// The repeating entrance/exit arc of the frozen-chord drone.
    pub drone: DroneArc,
    /// An end-of-chain low-pass on the drone, in Hz (0 = off, bypass).
    pub drone_tone: u16,
    /// Whether drum break edits may play slices backwards.
    pub drum_reverse: bool,
    /// End-of-chain master effects.
    pub master: MasterFx,
    /// The browser's source-code scroll overlay.
    pub code: CodeOverlay,
    /// The track's name: shown at the head of the page and used as the WAV
    /// title. Empty falls back to the product name.
    pub name: crate::name::Name,
    /// How the architecture is turned and placed. Identity by default, so the
    /// shrine is exactly the centred 16x16 model (`HC005`).
    pub arch: crate::arch::ArchTransform,
    /// The two glitch layers: which kinds play, and how often. The defaults
    /// reproduce the originally hard-coded layers exactly.
    pub glitch: [crate::glitch_setup::GlitchSetup; 2],
    /// An authored architecture. Empty means the seed's generator decides.
    pub model: crate::shrine_model::ShrineModel,
    /// Constant camera rotation, in degrees per second; the whole viewport
    /// orbits the shrine. Negative goes the other way, 0 holds still.
    pub camera_speed: i16,
    /// Whether the drone's rhythmic amplitude LFO runs. Off by default, so the
    /// drone sound is exactly as it always was.
    pub drone_rhythm: bool,
    /// Whether that same rhythm also shapes the atmosphere loops.
    pub loop_rhythm: bool,
}

pub const DEFAULT_SETTINGS: Settings =
    Settings {
        scale: Scale::Lydian,
        chords: 3,
        pace: Pace::HalfTime,
        kit: crate::kits::Kit::SubClicks,
        space: crate::kits::DrumSpace::Wide,
        bpm: crate::tempo::DEFAULT_BPM,
        loops: [
            LoopDesign::Sustained(LoopTimbre::Voices),
            LoopDesign::Sustained(LoopTimbre::Bells),
            LoopDesign::OFF,
            LoopDesign::OFF,
        ],
        loop_arcs: [DroneArc::ALWAYS_ON; 4],
        loop_transpose: 0,
        key: KeyChoice::Seed,
        drone: DroneArc::ALWAYS_ON,
        drone_tone: 0,
        drum_reverse: true,
        master: MasterFx::OFF,
        code: CodeOverlay::DEFAULT,
        name: crate::name::Name::EMPTY,
        arch: crate::arch::ArchTransform::IDENTITY,
        glitch: crate::glitch_setup::GlitchSetup::DEFAULT,
        model: crate::shrine_model::ShrineModel::EMPTY,
        camera_speed: 0,
        drone_rhythm: false,
        loop_rhythm: false,
    };

/// 0010's sound: the default settings with no atmosphere loops.
pub const NO_LOOPS: [LoopDesign; 4] = [LoopDesign::OFF; 4];

/// The ping-pong echo's delay, in glitch sixteenths (3/16).
const ECHO_SIXTEENTHS: u64 = 3;
/// The drums' tape echo delay, in drum sixteenths.
const TAPE_SIXTEENTHS: u64 = 3;

// Mix. "send" levels feed the shared long reverb.
const DRONE_DRY: f64 = 1.0;
const DRONE_SEND: f64 = 0.35;
const BASS_DRY: f64 = 1.0;
const BASS_SEND: f64 = 0.0;
const GLITCH1_DRY: f64 = 0.8;
const GLITCH1_ECHO: f64 = 0.35;
const GLITCH1_SEND: f64 = 0.3;
const GLITCH2_DRY: f64 = 0.7;
const GLITCH2_ECHO: f64 = 0.6;
const GLITCH2_SEND: f64 = 0.4;
const ECHO_DRY: f64 = 0.6;
const ECHO_SEND: f64 = 0.5;
const ECHO_FEEDBACK: f64 = 0.55;
/// Per atmosphere layer: dry level, reverb send, target length in seconds, and
/// stereo side level (1.0 = full width).
const LOOP_DRY: [f64; 4] = [0.8, 0.7, 0.6, 0.55];
const LOOP_SEND: [f64; 4] = [0.45, 0.5, 0.45, 0.4];
const LOOP_SECONDS: [u64; 4] = [6, 4, 5, 7];
const LOOP_WIDTH: [f64; 4] = [1.0, 0.6, 0.8, 0.7];
const DRUMS_DRY: f64 = 0.85;
const DRUMS_SEND: f64 = 0.1;
/// The drums' tape echo (Wide + tape echo only): send of the snare-type hits,
/// its return in the mix, and a little of it into the reverb.
const TAPE_SEND: f64 = 0.55;
const TAPE_WET: f64 = 0.6;
const TAPE_REVERB: f64 = 0.15;
const TAPE_FEEDBACK: f64 = 0.5;

// Making room for the drums: while they play, the drone dips and darkens.
/// Follower times (seconds) for "drums are playing" and for individual hits.
const PRESENCE_ATTACK: f64 = 0.05;
const PRESENCE_RELEASE: f64 = 1.5;
const PUNCH_ATTACK: f64 = 0.002;
const PUNCH_RELEASE: f64 = 0.06;
/// Drum-bus levels that count as "fully playing" / "a full hit".
const PRESENCE_REF: f64 = 0.3;
const PUNCH_REF: f64 = 0.6;
/// Depths: drone level (≈ −5 dB), per-hit pump (≈ −1 dB, quick), high shelf (−6 dB), glitch/echo dip.
const DUCK_DEPTH: f64 = 0.45;
const PUMP_DEPTH: f64 = 0.12;
const SHELF_DEPTH: f64 = 0.5;
const SHELF_HZ: f64 = 2500.0;
const GLITCH_DUCK: f64 = 0.15;
const REVERB_WET: f64 = 0.55;
const MASTER_GAIN: f64 = 0.9;
const FADE_IN_SECONDS: u64 = 10;

/// Drone filter sweep range.
pub const DRONE_CUTOFF_MIN: f64 = 400.0;
pub const DRONE_CUTOFF_OCTAVES: f64 = 4.2;

// The two glitch layers are built from `glitch_setup::GlitchSetup`, whose
// defaults reproduce the values that used to be hard-coded here.

/// A layer to hear on its own. Soloing changes only the *monitor* output
/// (`render_split`); the mix, and so every recording, is never touched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Solo {
    Drone,
    Glitch1,
    Glitch2,
    Bass,
    Drums,
    /// Atmosphere loop 1.
    Loop1,
    /// Atmosphere loop 2.
    Loop2,
    /// Atmosphere loop 3.
    Loop3,
    /// Atmosphere loop 4.
    Loop4,
    /// The echo and reverb returns only: the room without the instruments.
    Space,
}

/// Monitor-only state: its own echo and reverb (so a soloed layer keeps its
/// space), its own master stage, and a ramp for click-free switching.
pub(crate) struct Monitor {
    /// The echo's delay, to rebuild it fresh on each switch.
    echo_delay: usize,
    target: Option<Solo>,
    current: Option<Solo>,
    amount: f64,
    echo: PingPong,
    reverb: Reverb,
    dc: [DcBlock; 2],
    out: (f64, f64),
}

/// About 20 ms to fade between the mix and a solo.
const SOLO_RAMP: f64 = 1.0 / (0.02 * crate::SAMPLE_RATE as f64);

impl Monitor {
    fn new(echo_delay: usize) -> Self {
        Monitor {
            echo_delay,
            target: None,
            current: None,
            amount: 0.0,
            echo: PingPong::new(echo_delay, ECHO_FEEDBACK),
            reverb: Reverb::new(),
            dc: Default::default(),
            out: (0.0, 0.0),
        }
    }
}

/// Attack/release envelope follower.
pub(crate) struct Follower {
    env: f64,
    attack: f64,
    release: f64,
}

impl Follower {
    fn new(attack_s: f64, release_s: f64) -> Self {
        let sr = crate::SAMPLE_RATE as f64;
        let coef = |t: f64| 1.0 - crate::math::exp(-1.0 / (t * sr));
        Follower { env: 0.0, attack: coef(attack_s), release: coef(release_s) }
    }

    fn process(&mut self, x: f64) -> f64 {
        let a = if x > self.env { self.attack } else { self.release };
        self.env = crate::math::sanitize(self.env + a * (x - self.env));
        self.env
    }
}

pub struct Track {
    pub(crate) seeds: Seeds,
    pub(crate) tempo: Tempo,
    pub(crate) tape: TapeEcho,
    pub(crate) monitor: Monitor,
    pub(crate) presence: Follower,
    pub(crate) punch: Follower,
    pub(crate) shelf: [OnePole; 2],
    /// Drone gain from the drum-aware mix (display only).
    pub(crate) duck_gain: f64,
    pub(crate) settings: Settings,
    pub(crate) master: MasterChain,
    pub(crate) harmony: Harmony,
    pub(crate) drums: Drums,
    pub(crate) clock: u64,
    pub(crate) mods: Mods,
    pub(crate) drone: Drone,
    /// The drone's rhythmic amplitude LFO, if it is switched on.
    drone_rhythm: crate::drone_rhythm::DroneRhythm,
    pub(crate) drone_filter: [Svf; 2],
    /// End-of-chain drone character filter (a chosen low-pass).
    pub(crate) drone_tone_filter: [Svf; 2],
    /// Drone gain from the repeating entrance/exit arc (1 = always on).
    pub(crate) drone_window: f64,
    pub(crate) history: History,
    pub(crate) glitch1: Layer,
    pub(crate) glitch2: Layer,
    pub(crate) loops: [AtmosLoop; 4],
    /// Per-layer gain from each atmosphere layer's own arc.
    pub(crate) loop_windows: [f64; 4],
    pub(crate) echo: PingPong,
    pub(crate) bass: Bass,
    pub(crate) reverb: Reverb,
    pub(crate) dc: [DcBlock; 2],
    // Display-only state: written by `tick`, never read by it.
    pub(crate) meters: Meters,
    pub(crate) drone_cutoff: f64,
    pub(crate) drone_q: f64,
    pub(crate) overview_frame: u64,
}

impl Track {
    pub fn new(seeds: Seeds, settings: Settings) -> Self {
        let sr = crate::SAMPLE_RATE as f64;
        // Chord 1's voicing is the first draw from the drone's stream (as in 0009).
        let mut first = Rng::stream(seeds.s1, 0xD0);
        let mut rest = Rng::stream(seeds.s1, 0xD7);
        let key = match settings.key {
            KeyChoice::Seed => crate::harmony::key_for(seeds.s1),
            KeyChoice::Note(n) => n % 12,
        };
        let tempo = Tempo::new(settings.bpm);
        let settings = Settings { bpm: tempo.bpm, ..settings };
        let chords = settings.chords.clamp(1, 4) as usize;
        let harmony = Harmony::new(settings.scale, chords, settings.pace, tempo.beat(), key, &mut first, &mut rest);
        let echo_delay = (ECHO_SIXTEENTHS * tempo.glitch16()) as usize;
        // Loop lengths: whole beats, 3-8 s, pairwise co-prime so all four drift.
        let mut avoid = 1u64;
        let mut avoids = [0u64; 4];
        for (i, a) in avoids.iter_mut().enumerate() {
            *a = avoid;
            let beats = crate::atmos::loop_beats(tempo.beat(), LOOP_SECONDS[i], (i > 0).then_some(avoid));
            avoid = avoid.wrapping_mul(beats);
        }
        let mut drums = Drums::new(seeds.s5, settings.kit, settings.space, &harmony, tempo);
        drums.set_reverse(settings.drum_reverse);
        Track {
            seeds,
            tempo,
            tape: TapeEcho::new((TAPE_SIXTEENTHS * tempo.drum16) as usize, TAPE_FEEDBACK),
            monitor: Monitor::new(echo_delay),
            presence: Follower::new(PRESENCE_ATTACK, PRESENCE_RELEASE),
            punch: Follower::new(PUNCH_ATTACK, PUNCH_RELEASE),
            shelf: [OnePole::new(SHELF_HZ, sr), OnePole::new(SHELF_HZ, sr)],
            duck_gain: 1.0,
            settings,
            master: MasterChain::new(settings.master, seeds.s1),
            harmony: harmony.clone(),
            drums,
            clock: 0,
            mods: Mods::new(seeds.s1),
            glitch1: Layer::new(settings.glitch[0].layer_config(0, &harmony, tempo), seeds.s2, 0x61),
            glitch2: Layer::new(settings.glitch[1].layer_config(1, &harmony, tempo), seeds.s3, 0x62),
            loops: std::array::from_fn(|i| {
                AtmosLoop::new(
                    settings.loops[i],
                    seeds.s6,
                    0xA1 + i as u64,
                    &harmony,
                    tempo.beat(),
                    LOOP_SECONDS[i],
                    (i > 0).then_some(avoids[i]),
                    settings.loop_transpose,
                )
            }),
            loop_windows: [1.0; 4],
            drone: Drone::new(seeds.s1, harmony, first, rest),
            drone_rhythm: crate::drone_rhythm::DroneRhythm::new(seeds.s7, settings.bpm),
            drone_filter: [Svf::new(1000.0, 0.7, sr), Svf::new(1000.0, 0.7, sr)],
            drone_tone_filter: [Svf::new(8_000.0, 0.7, sr), Svf::new(8_000.0, 0.7, sr)],
            drone_window: 1.0,
            history: History::new(),
            echo: PingPong::new(echo_delay, ECHO_FEEDBACK),
            bass: Bass::new(seeds.s4),
            reverb: Reverb::new(),
            dc: Default::default(),
            meters: Meters::default(),
            drone_cutoff: 0.0,
            drone_q: 0.0,
            overview_frame: u64::MAX,
        }
    }

    /// Fill `out` with interleaved stereo samples. Output depends only on the
    /// seeds and the running sample position, never on how it is chunked.
    pub fn render(&mut self, out: &mut [f32]) {
        for frame in out.chunks_exact_mut(2) {
            let (l, r) = self.tick();
            frame[0] = l as f32;
            frame[1] = r as f32;
        }
    }

    /// The tempo this version plays at.
    pub fn tempo(&self) -> Tempo {
        self.tempo
    }

    /// Choose a layer to hear on its own (or `None` for the full mix). Only
    /// the monitor output of `render_split` changes.
    pub fn set_solo(&mut self, solo: Option<Solo>) {
        self.monitor.target = solo;
    }

    /// The layer currently soloed on the monitor, and how far faded in (0..1).
    pub fn solo(&self) -> (Option<Solo>, f64) {
        (self.monitor.current, self.monitor.amount)
    }

    /// Like `render`, but also fills `monitor` with what to *listen* to: the
    /// mix, or the soloed layer. `mix` is bit-identical to `render`'s output.
    pub fn render_split(&mut self, mix: &mut [f32], monitor: &mut [f32]) {
        for (m, o) in mix.chunks_exact_mut(2).zip(monitor.chunks_exact_mut(2)) {
            let (l, r) = self.tick();
            m[0] = l as f32;
            m[1] = r as f32;
            o[0] = self.monitor.out.0 as f32;
            o[1] = self.monitor.out.1 as f32;
        }
    }

    fn tick(&mut self) -> (f64, f64) {
        let sr = crate::SAMPLE_RATE as f64;
        let clock = self.clock;
        let m = self.mods.at(clock);

        // 1: frozen drone through the slowly cycling filter and the repeating
        // entrance/exit arc.
        if clock % 32 == 0 {
            let cutoff = DRONE_CUTOFF_MIN * crate::math::exp2(m.drone_open * DRONE_CUTOFF_OCTAVES);
            let q = 0.6 + 2.4 * m.drone_reso;
            for f in self.drone_filter.iter_mut() {
                f.set(cutoff, q, sr);
            }
            self.drone_cutoff = cutoff;
            self.drone_q = q;
            // The arc, times the rhythmic LFO if it is switched on. Off, the
            // multiplier is exactly 1.0, so the sound is unchanged.
            let rhythm = if self.settings.drone_rhythm || self.settings.loop_rhythm {
                self.drone_rhythm.level(clock)
            } else {
                1.0
            };
            self.drone_window = self.settings.drone.gain(clock)
                * if self.settings.drone_rhythm { rhythm } else { 1.0 };
            for k in 0..4 {
                // The loops can breathe with the same rhythm, on their own arc.
                self.loop_windows[k] = self.settings.loop_arcs[k].gain(clock)
                    * if self.settings.loop_rhythm { rhythm } else { 1.0 };
            }
            if self.settings.drone_tone > 0 {
                for f in self.drone_tone_filter.iter_mut() {
                    f.set(self.settings.drone_tone as f64, 0.7, sr);
                }
            }
        }
        let window = self.drone_window;
        let (dl, dr) = self.drone.next();
        let dl = self.drone_filter[0].process(dl).low * window;
        let dr = self.drone_filter[1].process(dr).low * window;
        // The chosen drone character filter, at the very end of the drone chain.
        let (dl, dr) = if self.settings.drone_tone > 0 {
            (self.drone_tone_filter[0].process(dl).low, self.drone_tone_filter[1].process(dr).low)
        } else {
            (dl, dr)
        };
        self.history.push(dl, dr);

        // 5 (computed early): jungle drums, and how present they are right now.
        let (d, d_r, d_perc) = self.drums.next(clock, &self.mods);
        let d_abs = d.abs().max(d_r.abs());
        let presence = (self.presence.process(d_abs) / PRESENCE_REF).min(1.0);
        let punch = (self.punch.process(d_abs) / PUNCH_REF).min(1.0);
        // The tape echo only runs in "Wide + tape echo".
        let (tl, tr) = if self.settings.space == crate::kits::DrumSpace::Tape {
            self.tape.process(clock, d_perc * TAPE_SEND)
        } else {
            (0.0, 0.0)
        };
        // Make room: the drone dips, pumps with each hit, and loses some top end.
        // (With no drums, presence and punch stay exactly 0 and nothing changes.)
        let duck = (1.0 - DUCK_DEPTH * presence) * (1.0 - PUMP_DEPTH * punch);
        let shelf = SHELF_DEPTH * presence;
        let dl = (dl - (dl - self.shelf[0].process(dl)) * shelf) * duck;
        let dr = (dr - (dr - self.shelf[1].process(dr)) * shelf) * duck;
        self.duck_gain = duck;
        let others = 1.0 - GLITCH_DUCK * presence;

        // 2 & 3: glitch layers. A muted layer keeps ticking, so its pattern
        // evolution stays deterministic, but its density is zero and it is
        // silent.
        let d1 = if self.settings.glitch[0].enabled { m.density1 } else { 0.0 };
        let d2 = if self.settings.glitch[1].enabled { m.density2 } else { 0.0 };
        let g1 = self.glitch1.next(clock, d1, &self.history);
        let g2 = self.glitch2.next(clock, d2, &self.history);
        let (el, er) = self.echo.process(
            g1.0 * GLITCH1_ECHO + g2.0 * GLITCH2_ECHO,
            g1.1 * GLITCH1_ECHO + g2.1 * GLITCH2_ECHO,
        );

        // The four atmosphere layers: they make room for the drums like the
        // drone, drift gently in level, and each breathes on its own arc.
        let mut a = [(0.0f64, 0.0f64); 4];
        for k in 0..4 {
            if self.loops[k].is_on() {
                let (l, r) = self.loops[k].next(clock, &self.harmony);
                let level = if k % 2 == 0 { m.loop_level } else { m.loop2_level };
                let g = duck * level * self.loop_windows[k];
                a[k] = if LOOP_WIDTH[k] == 1.0 {
                    // Full width: bit-identical to the old un-narrowed mix.
                    (l * g, r * g)
                } else {
                    let (mid, side) = (0.5 * (l + r), 0.5 * (l - r) * LOOP_WIDTH[k]);
                    ((mid + side) * g, (mid - side) * g)
                };
            }
        }

        // 4: bass, gliding to each chord's root during the morph bar. The bass
        // leaves with the drone: one arrangement, one envelope.
        if clock % 32 == 0 {
            let p = self.harmony.at(clock);
            let (a, z) = (self.harmony.chords[p.chord].bass, self.harmony.chords[p.next].bass);
            self.bass.set_root(a + (z - a) * p.morph);
        }
        let b = self.bass.next(clock, &m) * window;

        // Shared long reverb.
        let (rl, rr) = self.reverb.process(
            dl * DRONE_SEND
                + a[0].0 * LOOP_SEND[0]
                + a[1].0 * LOOP_SEND[1]
                + a[2].0 * LOOP_SEND[2]
                + a[3].0 * LOOP_SEND[3]
                + b * BASS_SEND
                + g1.0 * GLITCH1_SEND
                + g2.0 * GLITCH2_SEND
                + el * ECHO_SEND
                + d * DRUMS_SEND
                + tl * TAPE_REVERB,
            dr * DRONE_SEND
                + a[0].1 * LOOP_SEND[0]
                + a[1].1 * LOOP_SEND[1]
                + a[2].1 * LOOP_SEND[2]
                + a[3].1 * LOOP_SEND[3]
                + b * BASS_SEND
                + g1.1 * GLITCH1_SEND
                + g2.1 * GLITCH2_SEND
                + er * ECHO_SEND
                + d_r * DRUMS_SEND
                + tr * TAPE_REVERB,
        );

        let mut l = dl * DRONE_DRY
            + a[0].0 * LOOP_DRY[0]
            + a[1].0 * LOOP_DRY[1]
            + a[2].0 * LOOP_DRY[2]
            + a[3].0 * LOOP_DRY[3]
            + b * BASS_DRY
            + (g1.0 * GLITCH1_DRY + g2.0 * GLITCH2_DRY + el * ECHO_DRY) * others
            + d * DRUMS_DRY
            + rl * REVERB_WET
            + tl * TAPE_WET;
        let mut r = dr * DRONE_DRY
            + a[0].1 * LOOP_DRY[0]
            + a[1].1 * LOOP_DRY[1]
            + a[2].1 * LOOP_DRY[2]
            + a[3].1 * LOOP_DRY[3]
            + b * BASS_DRY
            + (g1.1 * GLITCH1_DRY + g2.1 * GLITCH2_DRY + er * ECHO_DRY) * others
            + d_r * DRUMS_DRY
            + rr * REVERB_WET
            + tr * TAPE_WET;

        let fade_len = FADE_IN_SECONDS * crate::SAMPLE_RATE as u64;
        if clock < fade_len {
            let x = clock as f64 / fade_len as f64;
            l *= x * x;
            r *= x * x;
        }

        self.clock += 1;
        let l = crate::math::tanh(self.dc[0].process(l) * MASTER_GAIN);
        let r = crate::math::tanh(self.dc[1].process(r) * MASTER_GAIN);

        let mt = &mut self.meters;
        mt.drone = mt.drone.max(dl.abs()).max(dr.abs());
        mt.bass = mt.bass.max(b.abs());
        mt.glitch1 = mt.glitch1.max(g1.0.abs()).max(g1.1.abs());
        mt.glitch2 = mt.glitch2.max(g2.0.abs()).max(g2.1.abs());
        mt.echo_l = mt.echo_l.max(el.abs());
        mt.echo_r = mt.echo_r.max(er.abs());
        mt.reverb = mt.reverb.max(rl.abs()).max(rr.abs());
        mt.drums = mt.drums.max(d_abs);
        mt.loop1 = mt.loop1.max(a[0].0.abs()).max(a[0].1.abs());
        mt.loop2 = mt.loop2.max(a[1].0.abs()).max(a[1].1.abs());
        mt.loop3 = mt.loop3.max(a[2].0.abs()).max(a[2].1.abs());
        mt.loop4 = mt.loop4.max(a[3].0.abs()).max(a[3].1.abs());
        mt.tape = mt.tape.max(tl.abs()).max(tr.abs());
        mt.out_l = mt.out_l.max(l.abs());
        mt.out_r = mt.out_r.max(r.abs());

        // The monitor: computed from the same signals, after the mix, never feeding back.
        let mon = &mut self.monitor;
        if mon.current.is_none() && mon.target.is_none() {
            mon.out = (l, r);
        } else {
            if mon.current != mon.target {
                // Fade back to the mix, then switch layer (with fresh echo/reverb).
                mon.amount -= SOLO_RAMP;
                if mon.amount <= 0.0 {
                    mon.amount = 0.0;
                    mon.current = mon.target;
                    mon.echo = PingPong::new(mon.echo_delay, ECHO_FEEDBACK);
                    mon.reverb = Reverb::new();
                    mon.dc = Default::default();
                }
            } else {
                mon.amount = (mon.amount + SOLO_RAMP).min(1.0);
            }
            let fade = if clock < fade_len { let x = clock as f64 / fade_len as f64; x * x } else { 1.0 };
            let (sl, sr) = match mon.current {
                None => (0.0, 0.0),
                Some(Solo::Space) => (
                    el * ECHO_DRY * others + rl * REVERB_WET + tl * TAPE_WET,
                    er * ECHO_DRY * others + rr * REVERB_WET + tr * TAPE_WET,
                ),
                Some(solo) => {
                    // The layer's dry signal, its echo (glitches only) and its own reverb.
                    let (dry, echo_in, send) = match solo {
                        Solo::Drone => ((dl * DRONE_DRY, dr * DRONE_DRY), (0.0, 0.0), (dl * DRONE_SEND, dr * DRONE_SEND)),
                        Solo::Bass => ((b * BASS_DRY, b * BASS_DRY), (0.0, 0.0), (b * BASS_SEND, b * BASS_SEND)),
                        Solo::Loop1 => ((a[0].0 * LOOP_DRY[0], a[0].1 * LOOP_DRY[0]), (0.0, 0.0), (a[0].0 * LOOP_SEND[0], a[0].1 * LOOP_SEND[0])),
                        Solo::Loop2 => ((a[1].0 * LOOP_DRY[1], a[1].1 * LOOP_DRY[1]), (0.0, 0.0), (a[1].0 * LOOP_SEND[1], a[1].1 * LOOP_SEND[1])),
                        Solo::Loop3 => ((a[2].0 * LOOP_DRY[2], a[2].1 * LOOP_DRY[2]), (0.0, 0.0), (a[2].0 * LOOP_SEND[2], a[2].1 * LOOP_SEND[2])),
                        Solo::Loop4 => ((a[3].0 * LOOP_DRY[3], a[3].1 * LOOP_DRY[3]), (0.0, 0.0), (a[3].0 * LOOP_SEND[3], a[3].1 * LOOP_SEND[3])),
                        Solo::Drums => (
                            (d * DRUMS_DRY + tl * TAPE_WET, d_r * DRUMS_DRY + tr * TAPE_WET),
                            (0.0, 0.0),
                            (d * DRUMS_SEND + tl * TAPE_REVERB, d_r * DRUMS_SEND + tr * TAPE_REVERB),
                        ),
                        Solo::Glitch1 => (
                            (g1.0 * GLITCH1_DRY * others, g1.1 * GLITCH1_DRY * others),
                            (g1.0 * GLITCH1_ECHO, g1.1 * GLITCH1_ECHO),
                            (g1.0 * GLITCH1_SEND, g1.1 * GLITCH1_SEND),
                        ),
                        _ => (
                            (g2.0 * GLITCH2_DRY * others, g2.1 * GLITCH2_DRY * others),
                            (g2.0 * GLITCH2_ECHO, g2.1 * GLITCH2_ECHO),
                            (g2.0 * GLITCH2_SEND, g2.1 * GLITCH2_SEND),
                        ),
                    };
                    let (mel, mer) = mon.echo.process(echo_in.0, echo_in.1);
                    let (wl, wr) = mon.reverb.process(send.0 + mel * ECHO_SEND, send.1 + mer * ECHO_SEND);
                    (
                        dry.0 + mel * ECHO_DRY * others + wl * REVERB_WET,
                        dry.1 + mer * ECHO_DRY * others + wr * REVERB_WET,
                    )
                }
            };
            let sl = crate::math::tanh(mon.dc[0].process(sl * fade) * MASTER_GAIN);
            let sr = crate::math::tanh(mon.dc[1].process(sr * fade) * MASTER_GAIN);
            let a = mon.amount;
            mon.out = (l + (sl - l) * a, r + (sr - r) * a);
        }
        self.master.process(l, r)
    }
}
