//! Read-only views into the running engine, for visualisation.
//!
//! Nothing here feeds back into the sound: `tick` only ever *writes* the
//! display state, so the golden-hash test is unaffected by any of it.

use crate::drone::{Recipe, HOP, N};
use crate::drums::{Plan, Voice};
use crate::glitch::Kind;
use crate::harmony::{Harmony, Position};
use crate::modulate::ModState;
use crate::pattern::{Layer, Mutation};
use crate::{Seeds, Track, SAMPLE_RATE};

/// Peak absolute level of each component since the last snapshot.
#[derive(Clone, Copy, Debug, Default)]
pub struct Meters {
    pub drone: f64,
    pub bass: f64,
    pub glitch1: f64,
    pub glitch2: f64,
    pub echo_l: f64,
    pub echo_r: f64,
    pub reverb: f64,
    pub drums: f64,
    /// The drums' tape echo return.
    pub tape: f64,
    pub out_l: f64,
    pub out_r: f64,
}

/// Everything about the track that never changes while it plays.
#[derive(Clone, Debug)]
pub struct Description {
    pub seeds: Seeds,
    pub settings: crate::Settings,
    /// The tempo (drums at `bpm`, ambient layers at half).
    pub tempo: crate::tempo::Tempo,
    pub sample_rate: u32,
    /// Scale, key and the chords (notes, bass, bars).
    pub harmony: Harmony,
    /// Each chord's FM recipes, one per note.
    pub recipes: Vec<Vec<Recipe>>,
    /// Each chord's frozen left-channel magnitude spectrum, bins 0..=fft_size/2.
    pub magnitudes: Vec<Vec<f64>>,
    /// Drums: on/off, the break pattern's name, its waveform (peaks), the main voice per slice.
    pub drums_on: bool,
    pub break_pattern: &'static str,
    pub break_overview: Vec<f32>,
    pub slice_voices: Vec<Option<Voice>>,
    /// The drum kit family and how each voice is made.
    pub drum_voices: crate::drums::VoiceInfo,
    /// The drum section of each of the first 128 four-bar phrases (all "out"
    /// when the kit is Off): the arrangement, known in advance.
    pub sections: Vec<crate::drums::Section>,
    pub fft_size: usize,
    pub hop: usize,
    /// Per glitch layer: (steps, step length in samples).
    pub layers: [(usize, u64); 2],
    pub echo_delay: usize,
    pub echo_feedback: f64,
    pub reverb_lines: [usize; 8],
    pub reverb_decay: f64,
    pub bass_root: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct StepInfo {
    pub kind: Kind,
    pub freq: f64,
    pub ratchet: u32,
    /// The step is silent while the layer's density is below this.
    pub threshold: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct VoiceInfo {
    pub kind: Kind,
    pub progress: f64,
    pub pan: f64,
}

#[derive(Clone, Debug)]
pub struct LayerSnapshot {
    pub steps: Vec<Option<StepInfo>>,
    /// Current step and how far through it (0..1).
    pub step: usize,
    pub step_phase: f64,
    pub loops: u64,
    pub loops_until_mutation: u64,
    pub density: f64,
    pub voices: Vec<VoiceInfo>,
    /// (clock, step, what) of the latest mutation.
    pub last_mutation: Option<(u64, usize, Mutation)>,
    pub triggers: u64,
}

/// The engine's moving parts at one moment.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub clock: u64,
    pub mods: ModState,
    /// (period in seconds, value -1..1) per slow LFO.
    pub lfos: [(u64, f64); 7],
    pub drone_cutoff: f64,
    pub drone_q: f64,
    pub drone_frames: u64,
    pub drone_offset: usize,
    /// Peak envelope of the newest drone frame, when a new one started sounding.
    pub new_frame: Option<Vec<f32>>,
    pub layers: [LayerSnapshot; 2],
    /// (FM ratio, modulation index, low-pass cutoff Hz).
    pub bass: (f64, f64, f64),
    pub reverb_lines: [f64; 8],
    pub meters: Meters,
    /// The layer soloed on the monitor (if any) and how far faded in.
    pub solo: Option<crate::Solo>,
    pub solo_amount: f64,
    /// The drone's gain from the drum-aware mix (1 = untouched).
    pub duck_gain: f64,
    /// The bass's current (gliding) root, MIDI.
    pub bass_root: f64,
    /// Where the chord progression is.
    pub harmony: Position,
    /// The bar being played by the drums, the step within it, the hits on the
    /// latest step, and a running hit count.
    pub drum_plan: Option<Plan>,
    pub drum_step: usize,
    pub drum_hits: Vec<Voice>,
    pub drum_hit_count: u64,
}

fn layer_snapshot(layer: &Layer, clock: u64, density: f64) -> LayerSnapshot {
    let cfg = layer.config();
    let (loops, loops_until_mutation) = layer.loops();
    // The step that sounded last (clock has already advanced past it).
    let at = clock.saturating_sub(1);
    LayerSnapshot {
        steps: layer
            .pattern()
            .iter()
            .map(|s| {
                s.map(|e| StepInfo { kind: e.kind, freq: e.freq, ratchet: e.ratchet, threshold: e.threshold })
            })
            .collect(),
        step: ((at / cfg.step_len) % cfg.steps as u64) as usize,
        step_phase: (at % cfg.step_len) as f64 / cfg.step_len as f64,
        loops,
        loops_until_mutation,
        density,
        voices: layer
            .voices()
            .map(|v| VoiceInfo { kind: v.event().kind, progress: v.progress(), pan: v.event().pan })
            .collect(),
        last_mutation: layer.last_mutation(),
        triggers: layer.triggers(),
    }
}

impl Track {
    pub fn describe(&self) -> Description {
        let (delay, feedback) = self.echo.settings();
        let chords = self.harmony.chords.len();
        Description {
            seeds: self.seeds,
            settings: self.settings,
            tempo: self.tempo,
            sample_rate: SAMPLE_RATE,
            harmony: self.harmony.clone(),
            recipes: (0..chords).map(|i| self.drone.recipes(i).to_vec()).collect(),
            magnitudes: (0..chords).map(|i| self.drone.magnitudes(i).0.to_vec()).collect(),
            drums_on: self.drums.enabled(),
            break_pattern: self.drums.pattern_name(),
            break_overview: self.drums.overview(512),
            slice_voices: self.drums.slice_voices(),
            drum_voices: self.drums.voice_info(),
            sections: (0..128u64)
                .map(|p| {
                    if self.drums.enabled() {
                        self.drums.plan(p * crate::drums::PHRASE_BARS, &self.mods).section
                    } else {
                        crate::drums::Section::Out
                    }
                })
                .collect(),
            fft_size: N,
            hop: HOP,
            layers: [
                (self.glitch1.config().steps, self.glitch1.config().step_len),
                (self.glitch2.config().steps, self.glitch2.config().step_len),
            ],
            echo_delay: delay,
            echo_feedback: feedback,
            reverb_lines: crate::reverb::LINES,
            reverb_decay: crate::reverb::DECAY_SECONDS,
            bass_root: crate::bass::ROOT,
        }
    }

    /// The current state; also resets the peak meters.
    pub fn snapshot(&mut self) -> Snapshot {
        let clock = self.clock;
        let mods = self.mods.at(clock.saturating_sub(1));
        let frames = self.drone.frames();
        let new_frame = (frames != self.overview_frame).then(|| {
            self.overview_frame = frames;
            self.drone.newest_frame_overview(256)
        });
        Snapshot {
            clock,
            mods,
            lfos: self.mods.lfos(clock),
            drone_cutoff: self.drone_cutoff,
            drone_q: self.drone_q,
            drone_frames: frames,
            drone_offset: self.drone.offset(),
            new_frame,
            layers: [
                layer_snapshot(&self.glitch1, clock, mods.density1),
                layer_snapshot(&self.glitch2, clock, mods.density2),
            ],
            bass: self.bass.state(),
            reverb_lines: self.reverb.take_peaks(),
            meters: core::mem::take(&mut self.meters),
            bass_root: self.bass.root(),
            duck_gain: self.duck_gain,
            solo: self.solo().0,
            solo_amount: self.solo().1,
            harmony: self.harmony.at(clock.saturating_sub(1)),
            drum_plan: self.drums.plan_now().cloned(),
            drum_step: self.drums.step(),
            drum_hits: self.drums.last_hits().to_vec(),
            drum_hit_count: self.drums.hit_count(),
        }
    }

    /// Samples rendered so far.
    pub fn clock(&self) -> u64 {
        self.clock
    }
}
