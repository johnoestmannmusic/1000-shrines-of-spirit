use freeze_dsp::render::{render_frozen_loop, LoopBufferData, DEFAULT_ROOT_NOTE};
use nih_plug::prelude::*;
use std::sync::Arc;

/// Phase B: prove the VST3/CLAP/standalone build+load pipeline works. The
/// loop buffer is baked from a synthetic source at `initialize()` time and
/// just plays on repeat, ignoring MIDI/transport entirely - real note
/// on/off, polyphony and parameter automation are Phase C/D.
pub struct FreezePlugin {
    params: Arc<FreezePluginParams>,
    loop_buffer: Option<LoopBufferData>,
    play_pos: usize,
}

#[derive(Params)]
struct FreezePluginParams {}

impl Default for FreezePlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(FreezePluginParams::default()),
            loop_buffer: None,
            play_pos: 0,
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

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
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
        self.play_pos = 0;

        true
    }

    fn reset(&mut self) {
        self.play_pos = 0;
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let Some(loop_buffer) = &self.loop_buffer else {
            return ProcessStatus::Normal;
        };
        let left = &loop_buffer.channels[0];
        let right = &loop_buffer.channels[1];
        let loop_len = left.len();

        for channel_samples in buffer.iter_samples() {
            let pos = self.play_pos % loop_len;
            for (ch_idx, sample) in channel_samples.into_iter().enumerate() {
                *sample = if ch_idx == 0 { left[pos] } else { right[pos] };
            }
            self.play_pos += 1;
        }

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
