//! Renders a WAV file through the SpectralFreeze algorithm for offline
//! listening tests (Phase A - before any plugin/GUI code exists).
//!
//! Note: `clap` here is the Rust argument-parsing crate, unrelated to the
//! CLAP plugin format this project will also target later in freeze_plugin.

use clap::Parser;
use freeze_dsp::render::{render_frozen_loop, DEFAULT_ROOT_NOTE};
use freeze_dsp::voice::VoiceManager;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "freeze_cli")]
struct Args {
    /// Input WAV file to freeze.
    #[arg(long)]
    input: PathBuf,

    /// Output WAV file to write.
    #[arg(long)]
    out: PathBuf,

    /// Freeze point, 0-100 (% into the source sample's usable length).
    #[arg(long, default_value_t = 30.0)]
    freeze_point: f32,

    /// Formant shift in semitones, -12..12.
    #[arg(long, default_value_t = 0.0)]
    formant_shift: f32,

    /// MIDI note to play (60 = middle C = root note, no pitch shift).
    #[arg(long, default_value_t = DEFAULT_ROOT_NOTE)]
    note: u8,

    /// Output length in seconds.
    #[arg(long, default_value_t = 5.0)]
    seconds: f32,

    /// Stereo width, 0-100 (0 = mono/centered - the default - 100 = the
    /// frozen loop's full natural L/R difference).
    #[arg(long, default_value_t = 0.0)]
    stereo_width: f32,
}

fn read_wav_channels(path: &PathBuf) -> (Vec<Vec<f32>>, f32) {
    let mut reader = hound::WavReader::open(path).expect("failed to open input WAV");
    let spec = reader.spec();
    let num_channels = spec.channels as usize;
    let sample_rate = spec.sample_rate as f32;

    let mut channels: Vec<Vec<f32>> = vec![Vec::new(); num_channels];

    match spec.sample_format {
        hound::SampleFormat::Float => {
            for (i, sample) in reader.samples::<f32>().enumerate() {
                let s = sample.expect("failed to read sample");
                channels[i % num_channels].push(s);
            }
        }
        hound::SampleFormat::Int => {
            let max_amplitude = (1i64 << (spec.bits_per_sample - 1)) as f32;
            for (i, sample) in reader.samples::<i32>().enumerate() {
                let s = sample.expect("failed to read sample") as f32 / max_amplitude;
                channels[i % num_channels].push(s);
            }
        }
    }

    (channels, sample_rate)
}

fn write_wav_stereo(path: &PathBuf, sample_rate: f32, left: &[f32], right: &[f32]) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: sample_rate as u32,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec).expect("failed to create output WAV");
    for (&l, &r) in left.iter().zip(right.iter()) {
        writer.write_sample(l).expect("failed to write sample");
        writer.write_sample(r).expect("failed to write sample");
    }
    writer.finalize().expect("failed to finalize output WAV");
}

fn main() {
    let args = Args::parse();

    let (channels, sample_rate) = read_wav_channels(&args.input);
    println!(
        "Loaded {} channel(s) at {} Hz, {} samples ({:.2}s)",
        channels.len(),
        sample_rate,
        channels[0].len(),
        channels[0].len() as f32 / sample_rate
    );

    let loop_buffer = render_frozen_loop(
        &channels,
        sample_rate,
        args.freeze_point,
        args.formant_shift,
        args.stereo_width,
        DEFAULT_ROOT_NOTE,
    );
    println!(
        "Rendered frozen loop: {} samples ({:.2}s) at freeze_point={} formant_shift={}st stereo_width={}%",
        loop_buffer.channels[0].len(),
        loop_buffer.channels[0].len() as f32 / sample_rate,
        args.freeze_point,
        args.formant_shift,
        args.stereo_width,
    );

    let mut voice_manager = VoiceManager::new(sample_rate, DEFAULT_ROOT_NOTE);
    voice_manager.note_on(args.note, 0, 1.0, 0);

    let total_samples = (args.seconds * sample_rate) as usize;
    let block_size = 512;
    let mut output_left = Vec::with_capacity(total_samples);
    let mut output_right = Vec::with_capacity(total_samples);
    let mut block_left = vec![0.0f32; block_size];
    let mut block_right = vec![0.0f32; block_size];

    while output_left.len() < total_samples {
        voice_manager.process_block(&loop_buffer, &mut block_left, &mut block_right);
        output_left.extend_from_slice(&block_left);
        output_right.extend_from_slice(&block_right);
    }
    output_left.truncate(total_samples);
    output_right.truncate(total_samples);

    write_wav_stereo(&args.out, sample_rate, &output_left, &output_right);
    println!(
        "Wrote {} samples ({:.2}s, stereo width={}%) to {}",
        output_left.len(),
        args.seconds,
        args.stereo_width,
        args.out.display()
    );
}
