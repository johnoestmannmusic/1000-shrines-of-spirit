//! Read-only views into the running engine, for visualisation.
//!
//! Nothing here feeds back into the sound: `tick` only ever *writes* the
//! display state, so the golden-hash test is unaffected by any of it.

use crate::drone::{Recipe, HOP, N};
use crate::glitch::Kind;
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
    pub out_l: f64,
    pub out_r: f64,
}

/// Everything about the track that never changes while it plays.
#[derive(Clone, Debug)]
pub struct Description {
    pub seeds: Seeds,
    pub sample_rate: u32,
    /// Chord notes (MIDI) and each note's FM recipe.
    pub voicing: Vec<f64>,
    pub recipes: Vec<Recipe>,
    /// The frozen left-channel magnitude spectrum, bins 0..=fft_size/2.
    pub magnitudes: Vec<f64>,
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
        Description {
            seeds: self.seeds,
            sample_rate: SAMPLE_RATE,
            voicing: self.drone.voicing().to_vec(),
            recipes: self.drone.recipes().to_vec(),
            magnitudes: self.drone.magnitudes().0.to_vec(),
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
        }
    }

    /// Samples rendered so far.
    pub fn clock(&self) -> u64 {
        self.clock
    }
}
