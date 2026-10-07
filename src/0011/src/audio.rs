//! Live playback.
//!
//! A producer thread runs the engine a little ahead of the sound card,
//! optionally recording to WAV, and hands audio to the device callback in
//! small blocks. Alongside each block it publishes an engine snapshot,
//! stamped with the block's sample position, so the TUI can show exactly what
//! is being heard right now rather than what was just computed.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};
use shrine0011::cli::{Fade, WavWriter};
use shrine0011::telemetry::{Description, Snapshot};
use shrine0011::{Seeds, Settings, Track, SAMPLE_RATE};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// Frames per block handed to the device (~10.7 ms).
const BLOCK: usize = 512;
/// Blocks queued ahead of the device (~170 ms): room for the terminal or
/// system to hiccup without the sound card running dry.
const QUEUE: usize = 16;
/// Fade when quitting early.
pub const STOP_FADE_SECONDS: f64 = 1.5;
/// Snapshots kept waiting for the UI before old ones are dropped.
const MAX_PENDING: usize = 256;

/// The layers that can be soloed, in `Shared::solo` order.
pub const SOLOS: [shrine0011::Solo; 8] = [
    shrine0011::Solo::Drone,
    shrine0011::Solo::Glitch1,
    shrine0011::Solo::Glitch2,
    shrine0011::Solo::Bass,
    shrine0011::Solo::Drums,
    shrine0011::Solo::Loop1,
    shrine0011::Solo::Loop2,
    shrine0011::Solo::Space,
];

/// One rendered block and the engine state at its end.
pub struct Published {
    pub snapshot: Snapshot,
    /// The block's interleaved stereo samples, as played.
    pub samples: Vec<f32>,
}

/// State shared between the producer, the audio callback and the UI.
#[derive(Default)]
pub struct Shared {
    /// Engine frames handed to the sound card so far.
    pub played: AtomicU64,
    pub paused: AtomicBool,
    /// Consume audio as normal but output silence (diagnostics).
    pub mute: AtomicBool,
    /// The layer to hear on its own (0 = none, else 1 + index in `SOLOS`).
    /// Only what you hear changes; recordings always get the full mix.
    pub solo: std::sync::atomic::AtomicU8,
    /// Times playback ran dry after it had started (each is an audible gap).
    pub underruns: AtomicU64,
    /// Asks the producer to fade out and finish.
    pub stop: AtomicBool,
    /// The producer has rendered its last block.
    pub finished: AtomicBool,
    /// Blocks rendered but not yet taken by the audio callback.
    pub queued: AtomicUsize,
    /// Frames that will have been played when everything finishes (valid once `finished`).
    pub end: AtomicU64,
    pub published: Mutex<VecDeque<Published>>,
}

pub struct Recording {
    pub path: String,
    pub seconds: f64,
    /// Metadata tags for the WAV.
    pub info: shrine0011::cli::WavInfo,
}

pub struct Live {
    pub shared: Arc<Shared>,
    pub description: Description,
    pub device: String,
    /// Device sample rate (resampled from 48 kHz when different).
    pub device_rate: u32,
    _stream: cpal::Stream,
    producer: Option<JoinHandle<std::io::Result<Option<u64>>>>,
}

impl Live {
    pub fn start(seeds: Seeds, settings: Settings, recording: Option<Recording>) -> Result<Live, String> {
        let track = Track::new(seeds, settings);
        let description = track.describe();

        let host = cpal::default_host();
        let device = host.default_output_device().ok_or("no audio output device found")?;
        let device_name =
            device.description().map(|d| d.to_string()).unwrap_or_else(|_| "default output".into());
        let config = choose_config(&device)?;
        let sample_format = config.sample_format();
        let stream_config: StreamConfig = config.config();
        let device_rate = stream_config.sample_rate;

        let shared = Arc::new(Shared { end: AtomicU64::new(u64::MAX), ..Default::default() });
        let (tx, rx) = sync_channel::<Vec<f32>>(QUEUE);
        let (recycle_tx, recycle_rx) = sync_channel::<Vec<f32>>(QUEUE + 2);

        let source = Source::new(rx, recycle_tx, shared.clone(), device_rate);
        let stream = match sample_format {
            SampleFormat::F32 => build::<f32>(&device, &stream_config, source),
            SampleFormat::F64 => build::<f64>(&device, &stream_config, source),
            SampleFormat::I16 => build::<i16>(&device, &stream_config, source),
            SampleFormat::I32 => build::<i32>(&device, &stream_config, source),
            SampleFormat::U16 => build::<u16>(&device, &stream_config, source),
            other => return Err(format!("unsupported device sample format {other}")),
        }?;

        let producer_shared = shared.clone();
        let producer = std::thread::Builder::new()
            .name("0011-engine".into())
            .spawn(move || produce(track, recording, producer_shared, tx, recycle_rx))
            .map_err(|e| e.to_string())?;

        // Fill the queue before the sound card starts pulling, so start-up
        // (terminal setup, first frames) can never starve it.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while shared.queued.load(Ordering::Relaxed) < QUEUE && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        stream.play().map_err(|e| format!("could not start audio: {e}"))?;
        Ok(Live {
            shared,
            description,
            device: device_name,
            device_rate,
            _stream: stream,
            producer: Some(producer),
        })
    }

    /// Waits for the producer; returns the number of frames recorded, if recording.
    pub fn join(&mut self) -> std::io::Result<Option<u64>> {
        match self.producer.take() {
            Some(h) => h.join().unwrap_or_else(|_| Err(std::io::Error::other("engine thread panicked"))),
            None => Ok(None),
        }
    }
}

/// Prefer a stereo f32 configuration that runs at 48 kHz natively.
fn choose_config(device: &cpal::Device) -> Result<cpal::SupportedStreamConfig, String> {
    let mut best: Option<(u32, cpal::SupportedStreamConfigRange)> = None;
    if let Ok(configs) = device.supported_output_configs() {
        for c in configs {
            if c.min_sample_rate() <= SAMPLE_RATE && c.max_sample_rate() >= SAMPLE_RATE {
                let score = 2 * (c.sample_format() == SampleFormat::F32) as u32 + (c.channels() == 2) as u32;
                if best.as_ref().is_none_or(|(s, _)| score > *s) {
                    best = Some((score, c));
                }
            }
        }
    }
    match best {
        Some((_, c)) => Ok(c.with_sample_rate(SAMPLE_RATE)),
        None => device.default_output_config().map_err(|e| format!("no usable audio configuration: {e}")),
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
        .map_err(|e| format!("could not open audio output: {e}"))
}

/// The audio-callback side: pulls blocks, resamples if the device isn't at 48 kHz.
struct Source {
    rx: Receiver<Vec<f32>>,
    recycle: SyncSender<Vec<f32>>,
    shared: Arc<Shared>,
    block: Vec<f32>,
    pos: usize,
    /// Audio has started arriving; a starving callback before that is just start-up.
    primed: bool,
    starving: bool,
    /// Linear interpolation between `prev` and `cur` at `frac`; `step` = 48k / device rate.
    /// At 48 kHz, step is exactly 1 and frac stays 0, so samples pass through bit-exact.
    prev: (f32, f32),
    cur: (f32, f32),
    frac: f64,
    step: f64,
}

impl Source {
    fn new(rx: Receiver<Vec<f32>>, recycle: SyncSender<Vec<f32>>, shared: Arc<Shared>, rate: u32) -> Self {
        Source {
            rx,
            recycle,
            shared,
            block: Vec::new(),
            pos: 0,
            primed: false,
            starving: false,
            prev: (0.0, 0.0),
            cur: (0.0, 0.0),
            frac: 1.0,
            step: SAMPLE_RATE as f64 / rate as f64,
        }
    }

    fn pull(&mut self) -> (f32, f32) {
        if self.pos >= self.block.len() {
            match self.rx.try_recv() {
                Ok(next) => {
                    let old = std::mem::replace(&mut self.block, next);
                    let _ = self.recycle.try_send(old);
                    self.shared.queued.fetch_sub(1, Ordering::Relaxed);
                    self.pos = 0;
                    self.primed = true;
                    self.starving = false;
                }
                Err(TryRecvError::Empty) => {
                    if self.primed && !self.starving && !self.shared.finished.load(Ordering::Relaxed) {
                        self.shared.underruns.fetch_add(1, Ordering::Relaxed);
                    }
                    self.starving = true;
                    return (0.0, 0.0);
                }
                Err(TryRecvError::Disconnected) => return (0.0, 0.0),
            }
        }
        let s = (self.block[self.pos], self.block[self.pos + 1]);
        self.pos += 2;
        self.shared.played.fetch_add(1, Ordering::Relaxed);
        s
    }

    fn fill<T: SizedSample + FromSample<f32>>(&mut self, data: &mut [T], channels: usize) {
        let silence = T::from_sample(0.0f32);
        if self.shared.paused.load(Ordering::Relaxed) {
            data.fill(silence);
            return;
        }
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
            let (l, r) = if self.shared.mute.load(Ordering::Relaxed) { (0.0, 0.0) } else { (l, r) };
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

/// The engine thread.
fn produce(
    mut track: Track,
    recording: Option<Recording>,
    shared: Arc<Shared>,
    tx: SyncSender<Vec<f32>>,
    recycle: Receiver<Vec<f32>>,
) -> std::io::Result<Option<u64>> {
    let mut wav = match &recording {
        Some(r) => Some(WavWriter::create(&r.path, &r.info)?),
        None => None,
    };
    let mut fade = recording.as_ref().map(|r| Fade::new((r.seconds * SAMPLE_RATE as f64) as u64));
    let mut result = Ok(());
    let mut mix: Vec<f32> = Vec::new();

    loop {
        let pos = track.clock();
        if shared.stop.load(Ordering::Relaxed) {
            let early = Fade::early(pos, STOP_FADE_SECONDS);
            if fade.is_none_or(|f| f.total() > early.total()) {
                fade = Some(early);
            }
        }
        let frames = match fade {
            Some(f) if pos >= f.total() => break,
            Some(f) => BLOCK.min((f.total() - pos) as usize),
            None => BLOCK,
        };
        let solo = match shared.solo.load(Ordering::Relaxed) {
            0 => None,
            n => SOLOS.get(n as usize - 1).copied(),
        };
        track.set_solo(solo);
        // `mix` is the piece itself (it goes to the recording); `buf` is what
        // you hear (the mix, or a soloed layer).
        let mut buf = recycle.try_recv().unwrap_or_default();
        buf.resize(frames * 2, 0.0);
        mix.resize(frames * 2, 0.0);
        track.render_split(&mut mix, &mut buf);

        if let Some(w) = wav.as_mut() {
            if result.is_ok() {
                let f = fade;
                result = w.write(&mix, |p| f.map_or(1.0, |f| f.gain(p)));
            }
        }
        if let Some(f) = fade {
            for (i, s) in buf.iter_mut().enumerate() {
                *s = (*s as f64 * f.gain(pos + (i / 2) as u64)) as f32;
            }
        }

        let snapshot = track.snapshot();
        if let Ok(mut q) = shared.published.lock() {
            if q.len() >= MAX_PENDING {
                q.pop_front();
            }
            q.push_back(Published { snapshot, samples: buf.clone() });
        }
        // Count first: the callback may take the block the moment it is sent.
        shared.queued.fetch_add(1, Ordering::Relaxed);
        if tx.send(buf).is_err() {
            break;
        }
    }

    shared.end.store(track.clock(), Ordering::Relaxed);
    shared.finished.store(true, Ordering::Relaxed);
    result?;
    match wav {
        Some(w) => w.finish().map(Some),
        None => Ok(None),
    }
}

/// Diagnostic: opens the default output with the same settings as playback,
/// plays silence for a few seconds and reports how the device asks for audio.
pub fn check() -> Result<String, String> {
    use std::time::{Duration, Instant};
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or("no audio output device found")?;
    let name = device.description().map(|d| d.to_string()).unwrap_or_default();
    let config = choose_config(&device)?;
    let stream_config: StreamConfig = config.config();
    let channels = stream_config.channels as usize;
    let log: Arc<Mutex<Vec<(Instant, usize)>>> = Arc::new(Mutex::new(Vec::with_capacity(4096)));
    let l = log.clone();
    let stream = device
        .build_output_stream(
            stream_config.clone(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                data.fill(0.0);
                if let Ok(mut v) = l.try_lock() {
                    v.push((Instant::now(), data.len() / channels));
                }
            },
            |_err| {},
            None,
        )
        .map_err(|e| format!("could not open audio output: {e}"))?;
    stream.play().map_err(|e| e.to_string())?;
    std::thread::sleep(Duration::from_secs(4));
    drop(stream);

    // The real playback pipeline, muted: how often does it run dry?
    let mut live = Live::start(shrine0011::DEFAULT_SEEDS, shrine0011::DEFAULT_SETTINGS, None)?;
    live.shared.mute.store(true, Ordering::Relaxed);
    let mut timeline = String::new();
    for s in 1..=15 {
        std::thread::sleep(Duration::from_secs(1));
        let u = live.shared.underruns.load(Ordering::Relaxed);
        timeline += &format!(" {s}s:{u}");
    }
    live.shared.stop.store(true, Ordering::Relaxed);
    std::thread::sleep(Duration::from_secs(2));
    let _ = live.join();
    let v = log.lock().map_err(|e| e.to_string())?;
    let sizes: Vec<usize> = v.iter().map(|x| x.1).collect();
    let gaps: Vec<f64> = v.windows(2).map(|w| (w[1].0 - w[0].0).as_secs_f64() * 1000.0).collect();
    let max_gap = gaps.iter().cloned().fold(0.0, f64::max);
    Ok(format!(
        "muted playback underruns (cumulative):{timeline}\ndevice: {name}\nconfig: {:?} {} ch @ {} Hz, buffer {:?}\ncallbacks: {} in 4 s, frames per callback min {} / max {} / mean {:.0}\nlongest gap between callbacks: {max_gap:.1} ms\nqueue ahead of device: {} frames",
        config.sample_format(),
        channels,
        stream_config.sample_rate,
        stream_config.buffer_size,
        sizes.len(),
        sizes.iter().min().unwrap_or(&0),
        sizes.iter().max().unwrap_or(&0),
        sizes.iter().sum::<usize>() as f64 / sizes.len().max(1) as f64,
        BLOCK * QUEUE,
    ))
}
