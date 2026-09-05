use freeze_dsp::render::{render_frozen_loop, LoopBufferData, DEFAULT_ROOT_NOTE};
use freeze_dsp::voice::VoiceManager;
use nih_plug::prelude::*;
use std::sync::Arc;

/// The loop buffer is baked from a synthetic source at `initialize()` time
/// (real sample loading is Phase E) and played polyphonically through
/// `VoiceManager`, driven by real MIDI note on/off. Parameter automation
/// (Freeze Point / Formant Shift / Stereo Width) is Phase D.
pub struct FreezePlugin {
    params: Arc<FreezePluginParams>,
    loop_buffer: Option<LoopBufferData>,
    voices: VoiceManager,
}

#[derive(Params)]
struct FreezePluginParams {}

impl Default for FreezePlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(FreezePluginParams::default()),
            loop_buffer: None,
            voices: VoiceManager::new(1.0, DEFAULT_ROOT_NOTE),
        }
    }
}

impl Default for FreezePluginParams {
    fn default() -> Self {
        Self {}
    }
}

/// A short, harmonically rich synthetic tone to freeze - stands in for a
/// user-loaded sample until Phase E adds real file loading.
fn synthetic_source(sample_rate: f32, seconds: f32) -> Vec<f32> {
    let len = (sample_rate * seconds) as usize;
    (0..len)
        .map(|i| {
            let t = i as f32 / sample_rate;
            0.5 * (t * 220.0 * std::f32::consts::TAU).sin()
                + 0.3 * (t * 440.0 * std::f32::consts::TAU).sin()
                + 0.1 * (t * 660.0 * std::f32::consts::TAU).sin()
        })
        .collect()
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

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        let sample_rate = buffer_config.sample_rate;
        let source = synthetic_source(sample_rate, 1.0);
        self.loop_buffer = Some(render_frozen_loop(
            &[source],
            sample_rate,
            50.0,
            0.0,
            30.0,
            DEFAULT_ROOT_NOTE,
        ));
        self.voices = VoiceManager::new(sample_rate, DEFAULT_ROOT_NOTE);

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
        let Some(loop_buffer) = &self.loop_buffer else {
            return ProcessStatus::Normal;
        };

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

        let channels = buffer.as_slice();
        let (left, right) = channels.split_at_mut(1);
        self.voices.process_block(loop_buffer, left[0], right[0]);

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
