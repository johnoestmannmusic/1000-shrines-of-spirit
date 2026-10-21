//! Live audition of the current shrine, for the setup screen.
//!
//! Host-only. A background thread renders the full `Sim` (music + weather) into
//! a small queue; the sound card's callback drains it. Changing a setting just
//! bumps a generation counter and the producer rebuilds. If no device can be
//! opened the caller keeps `None` and setup carries on silently, so this is
//! safe over SSH or on a machine with no sound card.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};
use shrine0016::sim::{Sim, SimConfig};
use shrine0016::SAMPLE_RATE;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex};

/// Frames per block handed to the device.
const BLOCK: usize = 512;
/// Blocks queued ahead of the device.
const QUEUE: usize = 8;

struct Shared {
    config: Mutex<SimConfig>,
    generation: AtomicU64,
    stop: AtomicBool,
}

/// A running live audition. Dropping it stops the sound.
pub struct Audition {
    shared: Arc<Shared>,
    _stream: cpal::Stream,
    producer: Option<std::thread::JoinHandle<()>>,
}

impl Audition {
    pub fn start(config: SimConfig) -> Result<Audition, String> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or("no audio output device found")?;
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let format = supported.sample_format();
        let stream_config: StreamConfig = supported.config();
        let device_rate = stream_config.sample_rate;

        let shared = Arc::new(Shared {
            config: Mutex::new(config),
            generation: AtomicU64::new(0),
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
            .name("shrinegen-audition".into())
            .spawn({
                let shared = shared.clone();
                move || produce(shared, tx)
            })
            .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| format!("could not start audition: {e}"))?;
        Ok(Audition { shared, _stream: stream, producer: Some(producer) })
    }

    /// Swap in a new configuration; the producer rebuilds on its next block.
    pub fn set_config(&self, config: SimConfig) {
        if let Ok(mut current) = self.shared.config.lock() {
            *current = config;
        }
        self.shared.generation.fetch_add(1, Ordering::Relaxed);
    }
}

impl Drop for Audition {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.producer.take() {
            let _ = handle.join();
        }
    }
}

/// The engine thread: rebuilds the `Sim` whenever the configuration changes.
fn produce(shared: Arc<Shared>, tx: SyncSender<Vec<f32>>) {
    let mut generation = u64::MAX;
    let mut sim: Option<Sim> = None;
    loop {
        if shared.stop.load(Ordering::Relaxed) {
            break;
        }
        let gen = shared.generation.load(Ordering::Relaxed);
        if gen != generation {
            generation = gen;
            let config = shared.config.lock().map(|c| c.clone()).ok();
            sim = config.map(Sim::new);
        }
        let Some(sim) = sim.as_mut() else {
            if tx.send(vec![0.0f32; BLOCK * 2]).is_err() {
                break;
            }
            continue;
        };
        let mut block = vec![0.0f32; BLOCK * 2];
        sim.render_audio(&mut block);
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
        .map_err(|e| format!("could not open audition audio: {e}"))
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
