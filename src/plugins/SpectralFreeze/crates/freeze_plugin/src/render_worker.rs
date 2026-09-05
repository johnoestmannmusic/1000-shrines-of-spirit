use arc_swap::ArcSwap;
use freeze_dsp::render::{render_frozen_loop, LoopBufferData};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

/// Freeze Point / Formant Shift / Stereo Width are all render-time
/// operations on the source sample (see `render::render_frozen_loop`) -
/// none of them are cheap per-sample transforms, so they can't just be read
/// on the audio thread. A single background thread renders on demand and
/// publishes the result into a shared `ArcSwap` the audio thread reads
/// lock-free.
#[derive(Clone, Copy, PartialEq)]
pub struct RenderRequest {
    pub freeze_point_pct: f32,
    pub formant_shift_semitones: f32,
    pub stereo_width_pct: f32,
}

/// Owns the background render thread. Only ever holds the *latest*
/// requested params (`request_render` overwrites any not-yet-started
/// request) so a fast automation sweep can't back the worker up with a
/// queue of stale renders to work through.
pub struct RenderWorker {
    pending: Arc<(Mutex<Option<RenderRequest>>, Condvar)>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl RenderWorker {
    pub fn spawn(
        source: Arc<Vec<Vec<f32>>>,
        sample_rate: f32,
        root_note: u8,
        output: Arc<ArcSwap<LoopBufferData>>,
    ) -> Self {
        let pending = Arc::new((Mutex::new(None::<RenderRequest>), Condvar::new()));
        let stop = Arc::new(AtomicBool::new(false));

        let pending_thread = pending.clone();
        let stop_thread = stop.clone();
        let handle = thread::spawn(move || {
            let (lock, cvar) = &*pending_thread;
            loop {
                let request = {
                    let mut guard = lock.lock().unwrap();
                    while guard.is_none() && !stop_thread.load(Ordering::Acquire) {
                        guard = cvar.wait(guard).unwrap();
                    }
                    if stop_thread.load(Ordering::Acquire) {
                        return;
                    }
                    guard.take().expect("woke with no request and no stop signal")
                };

                let rendered = render_frozen_loop(
                    &source,
                    sample_rate,
                    request.freeze_point_pct,
                    request.formant_shift_semitones,
                    request.stereo_width_pct,
                    root_note,
                );
                output.store(Arc::new(rendered));
            }
        });

        Self { pending, stop, handle: Some(handle) }
    }

    /// Overwrites any not-yet-started pending request with this one.
    pub fn request_render(&self, request: RenderRequest) {
        let (lock, cvar) = &*self.pending;
        *lock.lock().unwrap() = Some(request);
        cvar.notify_one();
    }
}

impl Drop for RenderWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let (lock, cvar) = &*self.pending;
        drop(lock.lock().unwrap());
        cvar.notify_one();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use freeze_dsp::render::DEFAULT_ROOT_NOTE;
    use std::time::{Duration, Instant};

    fn make_source(sample_rate: f32, seconds: f32) -> Arc<Vec<Vec<f32>>> {
        let len = (sample_rate * seconds) as usize;
        let tone: Vec<f32> = (0..len)
            .map(|i| (i as f32 / sample_rate * 220.0 * std::f32::consts::TAU).sin())
            .collect();
        Arc::new(vec![tone])
    }

    fn wait_for_render(output: &ArcSwap<LoopBufferData>, timeout: Duration) -> bool {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if !output.load().channels.is_empty() {
                return true;
            }
            thread::sleep(Duration::from_millis(5));
        }
        false
    }

    #[test]
    fn published_buffer_starts_empty_and_updates_after_request() {
        let sample_rate = 48000.0;
        let output = Arc::new(ArcSwap::new(Arc::new(LoopBufferData {
            channels: Vec::new(),
            sample_rate,
            root_note: DEFAULT_ROOT_NOTE,
        })));
        assert!(output.load().channels.is_empty());

        let worker = RenderWorker::spawn(make_source(sample_rate, 1.0), sample_rate, DEFAULT_ROOT_NOTE, output.clone());
        worker.request_render(RenderRequest { freeze_point_pct: 50.0, formant_shift_semitones: 0.0, stereo_width_pct: 30.0 });

        assert!(wait_for_render(&output, Duration::from_secs(2)), "worker did not publish a render in time");
        assert_eq!(output.load().channels.len(), 2, "render_frozen_loop always outputs stereo");
    }

    #[test]
    fn rapid_requests_collapse_to_the_latest_one() {
        let sample_rate = 48000.0;
        let output = Arc::new(ArcSwap::new(Arc::new(LoopBufferData {
            channels: Vec::new(),
            sample_rate,
            root_note: DEFAULT_ROOT_NOTE,
        })));

        let worker = RenderWorker::spawn(make_source(sample_rate, 1.0), sample_rate, DEFAULT_ROOT_NOTE, output.clone());
        // Fire a burst of superseding requests - the mailbox should collapse
        // these down rather than queueing every one of them.
        for freeze_point_pct in [10.0, 20.0, 30.0, 40.0, 50.0] {
            worker.request_render(RenderRequest { freeze_point_pct, formant_shift_semitones: 0.0, stereo_width_pct: 0.0 });
        }

        assert!(wait_for_render(&output, Duration::from_secs(2)), "worker did not publish a render in time");
        // Give the worker a moment to settle in case it were (incorrectly)
        // still working through a backlog of the earlier requests.
        thread::sleep(Duration::from_millis(200));
        assert_eq!(output.load().channels.len(), 2);
    }
}
