//! Live audition of one atmosphere layer, for the setup screen.
//!
//! This is deliberately *not* `audio::Live`: it builds only a `Harmony` and a
//! single `AtmosLoop` (no drone, no drums, no FFT), so a design change rebuilds
//! in milliseconds rather than a second or two. It is solo and dry — you hear
//! only the loop you are designing — and it is dropped the moment setup ends,
//! freeing the device for real playback.
//!
//! If no device can be opened the caller keeps the `None` and setup carries on
//! silently, so this is safe over SSH or on a machine with no sound card.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};
use shrine0011::atmos::AtmosLoop;
use shrine0011::harmony::Harmony;
use shrine0011::rng::Rng;
use shrine0011::tempo::Tempo;
use shrine0011::{KeyChoice, LoopDesign, Seeds, Settings, SAMPLE_RATE};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex};

/// Frames per block handed to the device.
const BLOCK: usize = 256;
/// Blocks queued ahead of the device.
const QUEUE: usize = 8;
/// The preview loop length, in seconds (as loop 1 in the real mix).
const PREVIEW_SECONDS: u64 = 6;
const STREAM: u64 = 0xA1;

struct Shared {
    /// The design to render. `generation` bumps whenever it changes.
    design: Mutex<LoopDesign>,
    generation: AtomicU64,
    /// Peaks of the current loop, for the animation.
    overview: Mutex<Vec<f32>>,
    /// Samples rendered (monotonic); the loop position is `% len`.
    playhead: AtomicU64,
    loop_len: AtomicU64,
    stop: AtomicBool,
}

pub struct Audition {
    shared: Arc<Shared>,
    _stream: cpal::Stream,
    producer: Option<std::thread::JoinHandle<()>>,
}

impl Audition {
    pub fn start(seeds: Seeds, settings: Settings) -> Result<Audition, String> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or("no audio output device found")?;
        let config = crate::audio::choose_config(&device)?;
        let format = config.sample_format();
        let stream_config: StreamConfig = config.config();
        let device_rate = stream_config.sample_rate;

        let shared = Arc::new(Shared {
            design: Mutex::new(settings.loops[0]),
            generation: AtomicU64::new(0),
            overview: Mutex::new(Vec::new()),
            playhead: AtomicU64::new(0),
            loop_len: AtomicU64::new(1),
            stop: AtomicBool::new(false),
        });
        let (tx, rx) = sync_channel::<Vec<f32>>(QUEUE);
        let source = Source::new(rx, device_rate);
        let stream = match format {
            SampleFormat::F32 => build::<f32>(&device, &stream_config, source),
            SampleFormat::F64 => build::<f64>(&device, &stream_config, source),
            SampleFormat::I16 => build::<i16>(&device, &stream_config, source),
            SampleFormat::I32 => build::<i32>(&device, &stream_config, source),
            SampleFormat::U16 => build::<u16>(&device, &stream_config, source),
            other => return Err(format!("unsupported device sample format {other}")),
        }?;

        let producer = std::thread::Builder::new()
            .name("0011-audition".into())
            .spawn({
                let shared = shared.clone();
                move || produce(seeds, settings, shared, tx)
            })
            .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| format!("could not start preview audio: {e}"))?;
        Ok(Audition { shared, _stream: stream, producer: Some(producer) })
    }

    /// Swap in a new design; the producer rebuilds on its next block.
    pub fn set_design(&self, design: LoopDesign) {
        if let Ok(mut d) = self.shared.design.lock() {
            *d = design;
        }
        self.shared.generation.fetch_add(1, Ordering::Relaxed);
    }

    /// Peaks of the current loop, for a sparkline.
    pub fn overview(&self) -> Vec<f32> {
        self.shared.overview.lock().map(|o| o.clone()).unwrap_or_default()
    }

    /// Position within the loop, 0..1.
    pub fn playhead(&self) -> f64 {
        let len = self.shared.loop_len.load(Ordering::Relaxed).max(1);
        (self.shared.playhead.load(Ordering::Relaxed) % len) as f64 / len as f64
    }
}

impl Drop for Audition {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.producer.take() {
            let _ = h.join();
        }
    }
}

/// The engine thread: builds the harmony once, then rebuilds the loop whenever
/// the design generation changes.
fn produce(seeds: Seeds, settings: Settings, shared: Arc<Shared>, tx: SyncSender<Vec<f32>>) {
    let tempo = Tempo::new(settings.bpm);
    let key = match settings.key {
        KeyChoice::Seed => shrine0011::harmony::key_for(seeds.s1),
        KeyChoice::Note(n) => n % 12,
    };
    let (mut first, mut rest) = (Rng::stream(seeds.s1, 0xD0), Rng::stream(seeds.s1, 0xD7));
    let harmony = Harmony::new(
        settings.scale,
        settings.chords.clamp(1, 4) as usize,
        settings.pace,
        tempo.beat(),
        key,
        &mut first,
        &mut rest,
    );
    let mut generation = u64::MAX;
    let mut atmos: Option<AtmosLoop> = None;
    let mut clock: u64 = 0;
    loop {
        if shared.stop.load(Ordering::Relaxed) {
            break;
        }
        let gen = shared.generation.load(Ordering::Relaxed);
        if gen != generation {
            generation = gen;
            let design = shared.design.lock().map(|d| *d).unwrap_or(LoopDesign::OFF);
            let a = AtmosLoop::new(design, seeds.s6, STREAM, &harmony, tempo.beat(), PREVIEW_SECONDS, None, settings.loop_transpose);
            let (_, len) = a.length();
            shared.loop_len.store(len as u64, Ordering::Relaxed);
            if let Ok(mut o) = shared.overview.lock() {
                *o = a.overview(0, 160);
            }
            atmos = Some(a);
            clock = 0;
        }
        let Some(a) = atmos.as_mut() else {
            // No design yet: send silence so the callback never starves.
            if tx.send(vec![0.0f32; BLOCK * 2]).is_err() {
                break;
            }
            continue;
        };
        let mut block = vec![0.0f32; BLOCK * 2];
        for f in block.chunks_exact_mut(2) {
            let (l, r) = a.next(clock, &harmony);
            f[0] = (l * 0.9) as f32;
            f[1] = (r * 0.9) as f32;
            clock += 1;
        }
        shared.playhead.store(clock, Ordering::Relaxed);
        if tx.send(block).is_err() {
            break;
        }
    }
}

fn build<T>(device: &cpal::Device, config: &StreamConfig, mut source: Source) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    device
        .build_output_stream(
            config.clone(),
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| source.fill(data, channels),
            |_err| {},
            None,
        )
        .map_err(|e| format!("could not open preview audio: {e}"))
}

/// The audio-callback side. Linear interpolation from 48 kHz when the device
/// runs at another rate; at 48 kHz it is an exact pass-through.
struct Source {
    rx: Receiver<Vec<f32>>,
    block: Vec<f32>,
    pos: usize,
    prev: (f32, f32),
    cur: (f32, f32),
    frac: f64,
    step: f64,
}

impl Source {
    fn new(rx: Receiver<Vec<f32>>, rate: u32) -> Self {
        Source { rx, block: Vec::new(), pos: 0, prev: (0.0, 0.0), cur: (0.0, 0.0), frac: 1.0, step: SAMPLE_RATE as f64 / rate as f64 }
    }

    fn pull(&mut self) -> (f32, f32) {
        if self.pos >= self.block.len() {
            match self.rx.try_recv() {
                Ok(next) => {
                    self.block = next;
                    self.pos = 0;
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => return (0.0, 0.0),
            }
        }
        let s = (self.block[self.pos], self.block[self.pos + 1]);
        self.pos += 2;
        s
    }

    fn fill<T: SizedSample + FromSample<f32>>(&mut self, data: &mut [T], channels: usize) {
        let silence = T::from_sample(0.0f32);
        for frame in data.chunks_mut(channels) {
            while self.frac >= 1.0 {
                self.prev = self.cur;
                self.cur = self.pull();
                self.frac -= 1.0;
            }
            let t = self.frac as f32;
            let l = self.prev.0 + (self.cur.0 - self.prev.0) * t;
            let r = self.prev.1 + (self.cur.1 - self.prev.1) * t;
            self.frac += self.step;
            match frame {
                [mono] => *mono = T::from_sample(0.5 * (l + r)),
                [a, b, rest @ ..] => {
                    *a = T::from_sample(l);
                    *b = T::from_sample(r);
                    rest.fill(silence);
                }
                [] => {}
            }
        }
    }
}
