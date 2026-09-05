mod render_worker;

use arc_swap::ArcSwap;
use freeze_dsp::render::{render_frozen_loop, LoopBufferData, DEFAULT_ROOT_NOTE};
use freeze_dsp::resample::resample_linear;
use freeze_dsp::voice::VoiceManager;
use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui, widgets, EguiState};
use render_worker::{RenderRequest, RenderWorker};
use std::path::Path;
use std::sync::Arc;

/// The loop buffer is baked from a source sample (the built-in placeholder
/// tone until a real file is loaded via the editor's "Load Sample" button)
/// and played polyphonically through `VoiceManager`, driven by real MIDI
/// note on/off. Freeze Point / Formant Shift / Stereo Width are real
/// automatable params, but re-rendering the frozen loop on every change is
/// too expensive for the audio thread - `process()` only *notices* a param
/// change and hands it to a background `RenderWorker`, which publishes the
/// new loop into `loop_buffer` (an `ArcSwap` so the audio thread can read
/// the latest version lock-free without ever blocking on the render).
pub struct FreezePlugin {
    params: Arc<FreezePluginParams>,
    /// The audio being frozen. An `ArcSwap` (not a plain `Arc`) so the
    /// editor's "Load Sample" button can swap in newly loaded audio without
    /// touching the audio thread - `RenderWorker` always reads whatever is
    /// current at the moment it renders.
    source: Arc<ArcSwap<Vec<Vec<f32>>>>,
    loop_buffer: Arc<ArcSwap<LoopBufferData>>,
    worker: Option<RenderWorker>,
    voices: VoiceManager,
    sample_rate: f32,
    /// The params a render has already been requested for (or that were
    /// used for the initial synchronous render). Compared against the
    /// current param values each block to detect a change at all.
    last_requested: RenderRequest,
    /// A detected change waiting for the throttle window to open. Always
    /// overwritten with the latest observed value (never queued), so once
    /// the throttle allows a send, it's always the most current params -
    /// never a stale intermediate value from partway through a drag.
    pending_request: Option<RenderRequest>,
    /// Counts down in samples; a new render is only actually requested once
    /// this reaches zero. See `RENDER_THROTTLE_MS` for why this exists.
    throttle_countdown: i64,
}

/// Minimum spacing between actual render requests, regardless of how often
/// the params themselves change.
///
/// Without this, a host automating (or a human dragging) Freeze Point
/// smoothly triggers a fresh render on nearly every processed block - each
/// landing mid-crossfade from the *previous* one, so the crossfades never
/// get to finish and instead pile up into a continuous warble. This isn't a
/// bug in the crossfade itself: two different freeze-point positions are
/// genuinely different, unrelated spectral snapshots (not points on a smooth
/// continuum), so some character change when scrubbing is inherent to this
/// DSP approach. But letting each committed change's `BUFFER_CROSSFADE_MS`
/// window actually complete before the next one starts turns "constant
/// stutter" into "occasional smooth morph" - throttling to roughly
/// `BUFFER_CROSSFADE_MS` or a bit more keeps them from overlapping.
const RENDER_THROTTLE_MS: f32 = 100.0;

#[derive(Params)]
struct FreezePluginParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[id = "freeze_point"]
    pub freeze_point: FloatParam,

    #[id = "formant_shift"]
    pub formant_shift: FloatParam,

    #[id = "stereo_width"]
    pub stereo_width: FloatParam,
}

fn silent_loop_buffer() -> LoopBufferData {
    LoopBufferData { channels: Vec::new(), sample_rate: 1.0, root_note: DEFAULT_ROOT_NOTE }
}

impl Default for FreezePlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(FreezePluginParams::default()),
            source: Arc::new(ArcSwap::new(Arc::new(Vec::new()))),
            loop_buffer: Arc::new(ArcSwap::new(Arc::new(silent_loop_buffer()))),
            worker: None,
            voices: VoiceManager::new(1.0, DEFAULT_ROOT_NOTE),
            sample_rate: 1.0,
            last_requested: RenderRequest { freeze_point_pct: 50.0, formant_shift_semitones: 0.0, stereo_width_pct: 30.0 },
            pending_request: None,
            throttle_countdown: 0,
        }
    }
}

impl Default for FreezePluginParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(320, 240),
            freeze_point: FloatParam::new("Freeze Point", 50.0, FloatRange::Linear { min: 0.0, max: 100.0 })
                .with_unit(" %"),
            formant_shift: FloatParam::new(
                "Formant Shift",
                0.0,
                FloatRange::Linear { min: -12.0, max: 12.0 },
            )
            .with_unit(" st"),
            stereo_width: FloatParam::new("Stereo Width", 30.0, FloatRange::Linear { min: 0.0, max: 100.0 })
                .with_unit(" %"),
        }
    }
}

/// A short synthetic tone to freeze - the built-in placeholder until a real
/// file is loaded via the editor. Brightness (the balance between the
/// fundamental and its upper harmonics) sweeps over the tone's duration,
/// specifically so that different Freeze Point values capture genuinely
/// different-sounding moments - a *constant* tone would give every Freeze
/// Point nearly identical magnitude content differing only in essentially
/// arbitrary starting phase, which is a poor demonstration of what Freeze
/// Point is for.
fn synthetic_source(sample_rate: f32, seconds: f32) -> Vec<f32> {
    let len = (sample_rate * seconds) as usize;
    (0..len)
        .map(|i| {
            let t = i as f32 / sample_rate;
            let brightness = i as f32 / len as f32; // 0.0 at start, 1.0 at end
            0.5 * (t * 220.0 * std::f32::consts::TAU).sin()
                + (0.05 + 0.5 * brightness) * (t * 440.0 * std::f32::consts::TAU).sin()
                + (0.5 * brightness) * (t * 660.0 * std::f32::consts::TAU).sin()
        })
        .collect()
}

/// Decodes a WAV file into one `Vec<f32>` per channel (interleaved samples
/// deinterleaved), normalizing integer formats to `[-1.0, 1.0]`. Returns the
/// file's own sample rate alongside - the caller is responsible for
/// resampling to the plugin's operating rate if they differ.
fn load_wav_channels(path: &Path) -> Result<(Vec<Vec<f32>>, f32), String> {
    let mut reader = hound::WavReader::open(path).map_err(|e| format!("couldn't open WAV: {e}"))?;
    let spec = reader.spec();
    let num_channels = (spec.channels as usize).max(1);
    let sample_rate = spec.sample_rate as f32;

    let mut channels: Vec<Vec<f32>> = vec![Vec::new(); num_channels];
    match spec.sample_format {
        hound::SampleFormat::Float => {
            for (i, sample) in reader.samples::<f32>().enumerate() {
                let s = sample.map_err(|e| format!("error reading sample: {e}"))?;
                channels[i % num_channels].push(s);
            }
        }
        hound::SampleFormat::Int => {
            let max_amplitude = (1i64 << (spec.bits_per_sample - 1)) as f32;
            for (i, sample) in reader.samples::<i32>().enumerate() {
                let s = sample.map_err(|e| format!("error reading sample: {e}"))? as f32 / max_amplitude;
                channels[i % num_channels].push(s);
            }
        }
    }

    if channels.iter().all(|c| c.is_empty()) {
        return Err("WAV file contains no audio samples".to_string());
    }

    Ok((channels, sample_rate))
}

/// Brings a loaded file's audio to the plugin's current operating rate -
/// without this, a file whose native rate differs from the host's would
/// play back pitch/speed-shifted, since `VoiceManager` reads the frozen
/// loop assuming it's already at the plugin's operating rate.
fn prepare_source_for_plugin_rate(channels: Vec<Vec<f32>>, file_rate: f32, plugin_rate: f32) -> Vec<Vec<f32>> {
    if (file_rate - plugin_rate).abs() < 0.5 {
        channels
    } else {
        channels.into_iter().map(|c| resample_linear(&c, file_rate, plugin_rate)).collect()
    }
}

/// GUI-thread-only state for the editor (which file is loaded, any load
/// error) - not shared with the audio thread and not persisted.
#[derive(Default)]
struct FreezeEditorState {
    filename: Option<String>,
    error: Option<String>,
}

impl Plugin for FreezePlugin {
    const NAME: &'static str = "SpectralFreeze";
    const VENDOR: &'static str = "John Oestmann";
    const URL: &'static str = "https://johnoestmannmusic.com";
    const EMAIL: &'static str = "contact@johnoestmannmusic.com";

    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let source = self.source.clone();
        let loop_buffer = self.loop_buffer.clone();
        let trigger = self.worker.as_ref().map(|worker| worker.trigger());

        create_egui_editor(
            self.params.editor_state.clone(),
            FreezeEditorState::default(),
            |_, _| {},
            move |egui_ctx, setter, state| {
                egui::CentralPanel::default().show(egui_ctx, |ui| {
                    ui.heading("SpectralFreeze");

                    ui.add_space(8.0);
                    ui.label("Freeze Point");
                    ui.add(widgets::ParamSlider::for_param(&params.freeze_point, setter));

                    ui.label("Formant Shift");
                    ui.add(widgets::ParamSlider::for_param(&params.formant_shift, setter));

                    ui.label("Stereo Width");
                    ui.add(widgets::ParamSlider::for_param(&params.stereo_width, setter));

                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(8.0);

                    if ui.button("Load Sample...").clicked() {
                        if let Some(path) = rfd::FileDialog::new().add_filter("WAV", &["wav", "WAV"]).pick_file() {
                            match load_wav_channels(&path) {
                                Ok((channels, file_rate)) => {
                                    // Read the plugin's current operating rate from
                                    // the loop buffer it last rendered at, rather
                                    // than a value captured once when the editor
                                    // was created (which could be stale if the
                                    // editor is opened unusually early).
                                    let plugin_rate = loop_buffer.load().sample_rate;
                                    let prepared = prepare_source_for_plugin_rate(channels, file_rate, plugin_rate);
                                    source.store(Arc::new(prepared));
                                    if let Some(trigger) = &trigger {
                                        trigger.request_render(RenderRequest {
                                            freeze_point_pct: params.freeze_point.value(),
                                            formant_shift_semitones: params.formant_shift.value(),
                                            stereo_width_pct: params.stereo_width.value(),
                                        });
                                    }
                                    state.filename = path.file_name().map(|n| n.to_string_lossy().into_owned());
                                    state.error = None;
                                }
                                Err(e) => state.error = Some(e),
                            }
                        }
                    }

                    ui.add_space(4.0);
                    match &state.filename {
                        Some(name) => {
                            ui.label(format!("Loaded: {name}"));
                        }
                        None => {
                            ui.label("Using built-in placeholder tone");
                        }
                    }
                    if let Some(error) = &state.error {
                        ui.colored_label(egui::Color32::from_rgb(220, 80, 80), error);
                    }
                });
            },
        )
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        let sample_rate = buffer_config.sample_rate;
        self.source.store(Arc::new(vec![synthetic_source(sample_rate, 1.0)]));

        let request = RenderRequest {
            freeze_point_pct: self.params.freeze_point.value(),
            formant_shift_semitones: self.params.formant_shift.value(),
            stereo_width_pct: self.params.stereo_width.value(),
        };
        // First render happens synchronously here (initialize() runs before
        // playback starts, so blocking is fine) so process() never sees the
        // placeholder silent buffer once the host actually starts playing.
        self.loop_buffer.store(Arc::new(render_frozen_loop(
            &self.source.load(),
            sample_rate,
            request.freeze_point_pct,
            request.formant_shift_semitones,
            request.stereo_width_pct,
            DEFAULT_ROOT_NOTE,
        )));
        self.last_requested = request;

        self.worker =
            Some(RenderWorker::spawn(self.source.clone(), sample_rate, DEFAULT_ROOT_NOTE, self.loop_buffer.clone()));
        self.voices = VoiceManager::new(sample_rate, DEFAULT_ROOT_NOTE);
        self.sample_rate = sample_rate;
        self.pending_request = None;
        self.throttle_countdown = 0;

        true
    }

    fn reset(&mut self) {
        self.voices.choke_all();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Freeze Point / Formant Shift / Stereo Width are checked once per
        // block (not per-sample - they're not audio-rate, and re-rendering
        // the frozen loop is too expensive to consider on every sample
        // anyway). A detected change is only actually sent to the worker
        // once every RENDER_THROTTLE_MS, so a smooth drag/automation sweep
        // can't land a new buffer swap before the previous one's crossfade
        // has finished (see RENDER_THROTTLE_MS for why that matters).
        let current_request = RenderRequest {
            freeze_point_pct: self.params.freeze_point.value(),
            formant_shift_semitones: self.params.formant_shift.value(),
            stereo_width_pct: self.params.stereo_width.value(),
        };
        if current_request != self.last_requested {
            self.pending_request = Some(current_request);
        }

        self.throttle_countdown -= buffer.samples() as i64;
        if self.throttle_countdown <= 0 {
            if let Some(request) = self.pending_request.take() {
                if let Some(worker) = &self.worker {
                    worker.request_render(request);
                }
                self.last_requested = request;
            }
            self.throttle_countdown = ((RENDER_THROTTLE_MS / 1000.0) * self.sample_rate) as i64;
        }

        // Block-level MIDI handling: every event pending for this buffer is
        // applied before rendering, rather than split at the exact sample it
        // arrived on. Good enough for proving polyphony/pitch/voice-stealing
        // here; sample-accurate note timing can be revisited later if a
        // fast arpeggio/chord attack audibly needs it.
        while let Some(event) = context.next_event() {
            match event {
                NoteEvent::NoteOn { note, channel, velocity, voice_id, .. } => {
                    self.voices.note_on(note, channel, velocity, voice_id.unwrap_or(note as i32));
                }
                NoteEvent::NoteOff { note, channel, .. } => {
                    self.voices.note_off(note, channel);
                }
                NoteEvent::Choke { .. } => {
                    self.voices.choke_all();
                }
                _ => (),
            }
        }

        let loop_buffer = self.loop_buffer.load();
        let channels = buffer.as_slice();
        let (left, right) = channels.split_at_mut(1);
        self.voices.process_block(&loop_buffer, left[0], right[0]);

        ProcessStatus::Normal
    }
}

impl ClapPlugin for FreezePlugin {
    const CLAP_ID: &'static str = "com.johnoestmannmusic.spectral-freeze";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Freezes a spectral snapshot of a sample into a sustained pad/drone");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for FreezePlugin {
    const VST3_CLASS_ID: [u8; 16] = *b"SpectralFreeze01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Synth];
}

nih_export_clap!(FreezePlugin);
nih_export_vst3!(FreezePlugin);
