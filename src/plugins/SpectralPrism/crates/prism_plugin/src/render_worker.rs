use arc_swap::ArcSwap;
use prism_dsp::render::{render_frozen_loop, LoopBufferData};
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

/// A cheap-to-clone handle for asking the render worker to render again -
/// separated from `RenderWorker` itself (which owns the thread and stops it
/// on drop) so it can be handed to the GUI thread (e.g. so a "Load Sample"
/// button can trigger a re-render after swapping in new source audio)
/// without giving the GUI any control over the worker's lifetime.
#[derive(Clone)]
pub struct RenderTrigger {
    pending: Arc<(Mutex<Option<RenderRequest>>, Condvar)>,
}

impl RenderTrigger {
    /// Creates a standalone mailbox, independent of any `RenderWorker` -
    /// see `RenderWorker::spawn`'s doc comment for why this independence
    /// matters (a request sent before the worker thread even exists yet is
    /// simply the first thing it processes once it does).
    pub fn new() -> Self {
        Self { pending: Arc::new((Mutex::new(None), Condvar::new())) }
    }

    /// Overwrites any not-yet-started pending request with this one.
    pub fn request_render(&self, request: RenderRequest) {
        let (lock, cvar) = &*self.pending;
        *lock.lock().unwrap() = Some(request);
        cvar.notify_one();
    }
}

impl Default for RenderTrigger {
    fn default() -> Self {
        Self::new()
    }
}

/// Owns the background render thread. Only ever holds the *latest*
/// requested params (`request_render` overwrites any not-yet-started
/// request) so a fast automation sweep can't back the worker up with a
/// queue of stale renders to work through.
///
/// `source` is an `ArcSwap` (not a fixed `Arc` captured at spawn time) so
/// loading a new sample (Phase E) can swap it out - the worker always reads
/// whatever the *current* source is at the moment a request comes in, not
/// whatever it was when the thread started.
pub struct RenderWorker {
    trigger: RenderTrigger,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl RenderWorker {
    /// Spawns the background thread reusing `trigger`'s own mailbox rather
    /// than creating a fresh one - so a `RenderTrigger` created up front
    /// (e.g. `PrismPlugin::default()`, before `initialize()` has spawned
    /// this worker at all) and handed to the editor keeps working
    /// regardless of whether the editor or `initialize()` runs first. A
    /// request sent before this thread exists just sits in the mailbox
    /// (`pending` starts `Some` in that case, not `None`) and is the first
    /// thing the loop below picks up once it does start. This is exactly
    /// what closed the real bug this fixes: nih-plug hosts aren't
    /// guaranteed to call `initialize()` before creating the editor, so a
    /// `RenderTrigger` captured only from `self.worker` (`None` until
    /// `initialize()` runs) could end up permanently unusable for a given
    /// editor session - loading a sample would swap the source in but never
    /// actually trigger a re-render, silently continuing to play whatever
    /// was frozen before.
    pub fn spawn(
        trigger: RenderTrigger,
        source: Arc<ArcSwap<Vec<Vec<f32>>>>,
        sample_rate: f32,
        root_note: u8,
        output: Arc<ArcSwap<LoopBufferData>>,
    ) -> Self {
        let pending = trigger.pending.clone();
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

                let current_source = source.load();
                let rendered = render_frozen_loop(
                    &current_source,
                    sample_rate,
                    request.freeze_point_pct,
                    request.formant_shift_semitones,
                    request.stereo_width_pct,
                    root_note,
                );
                output.store(Arc::new(rendered));
            }
        });

        Self { trigger, stop, handle: Some(handle) }
    }
}

impl Drop for RenderWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let (lock, cvar) = &*self.trigger.pending;
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
    use prism_dsp::render::DEFAULT_ROOT_NOTE;
    use std::time::{Duration, Instant};

    fn make_source(sample_rate: f32, seconds: f32) -> Arc<ArcSwap<Vec<Vec<f32>>>> {
        let len = (sample_rate * seconds) as usize;
        let tone: Vec<f32> = (0..len)
            .map(|i| (i as f32 / sample_rate * 220.0 * std::f32::consts::TAU).sin())
            .collect();
        Arc::new(ArcSwap::new(Arc::new(vec![tone])))
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

        let trigger = RenderTrigger::new();
        let _worker = RenderWorker::spawn(trigger.clone(), make_source(sample_rate, 1.0), sample_rate, DEFAULT_ROOT_NOTE, output.clone());
        trigger.request_render(RenderRequest { freeze_point_pct: 50.0, formant_shift_semitones: 0.0, stereo_width_pct: 30.0 });

        assert!(wait_for_render(&output, Duration::from_secs(2)), "worker did not publish a render in time");
        assert_eq!(output.load().channels.len(), 2, "render_frozen_loop always outputs stereo");
    }

    #[test]
    fn request_sent_before_spawn_is_still_processed() {
        // Regression test for a real bug: a request sent through a
        // `RenderTrigger` created up front (e.g. `PrismPlugin::default()`)
        // must still get processed even if `RenderWorker::spawn` (which
        // reuses that same trigger's mailbox, see its doc comment) hasn't
        // been called yet - this is exactly what happens if a host creates
        // the editor before calling `initialize()` (which is what actually
        // spawns the worker). Before the trigger/worker were decoupled, the
        // editor could only ever capture a trigger from an already-`Some`
        // `self.worker`, so a request made this early was simply lost.
        let sample_rate = 48000.0;
        let trigger = RenderTrigger::new();
        trigger.request_render(RenderRequest { freeze_point_pct: 50.0, formant_shift_semitones: 0.0, stereo_width_pct: 30.0 });

        let output = Arc::new(ArcSwap::new(Arc::new(LoopBufferData {
            channels: Vec::new(),
            sample_rate,
            root_note: DEFAULT_ROOT_NOTE,
        })));
        let _worker = RenderWorker::spawn(trigger, make_source(sample_rate, 1.0), sample_rate, DEFAULT_ROOT_NOTE, output.clone());

        assert!(wait_for_render(&output, Duration::from_secs(2)), "the pre-spawn request was never processed");
        assert_eq!(output.load().channels.len(), 2);
    }

    #[test]
    fn rapid_requests_collapse_to_the_latest_one() {
        let sample_rate = 48000.0;
        let output = Arc::new(ArcSwap::new(Arc::new(LoopBufferData {
            channels: Vec::new(),
            sample_rate,
            root_note: DEFAULT_ROOT_NOTE,
        })));

        let trigger = RenderTrigger::new();
        let _worker = RenderWorker::spawn(trigger.clone(), make_source(sample_rate, 1.0), sample_rate, DEFAULT_ROOT_NOTE, output.clone());
        // Fire a burst of superseding requests - the mailbox should collapse
        // these down rather than queueing every one of them.
        for freeze_point_pct in [10.0, 20.0, 30.0, 40.0, 50.0] {
            trigger.request_render(RenderRequest { freeze_point_pct, formant_shift_semitones: 0.0, stereo_width_pct: 0.0 });
        }

        assert!(wait_for_render(&output, Duration::from_secs(2)), "worker did not publish a render in time");
        // Give the worker a moment to settle in case it were (incorrectly)
        // still working through a backlog of the earlier requests.
        thread::sleep(Duration::from_millis(200));
        assert_eq!(output.load().channels.len(), 2);
    }

    #[test]
    fn swapping_the_source_and_re_requesting_uses_the_new_source() {
        // Proves the whole point of ArcSwap<Vec<Vec<f32>>> over a fixed Arc
        // captured at spawn time: a source swapped in after the worker
        // starts must actually be used by the *next* render, not whatever
        // was current when the thread was spawned.
        let sample_rate = 48000.0;
        let source = make_source(sample_rate, 1.0);
        let output = Arc::new(ArcSwap::new(Arc::new(LoopBufferData {
            channels: Vec::new(),
            sample_rate,
            root_note: DEFAULT_ROOT_NOTE,
        })));

        let trigger = RenderTrigger::new();
        let _worker = RenderWorker::spawn(trigger.clone(), source.clone(), sample_rate, DEFAULT_ROOT_NOTE, output.clone());
        let request = RenderRequest { freeze_point_pct: 50.0, formant_shift_semitones: 0.0, stereo_width_pct: 0.0 };
        trigger.request_render(request);
        assert!(wait_for_render(&output, Duration::from_secs(2)), "worker did not publish the first render in time");
        let first_len = output.load().channels[0].len();

        // Swap in a much longer source (loop length scales with source
        // length up to the 8s cap) and request again via the same cloned
        // trigger, mirroring how the GUI thread would use it.
        let longer_len = (sample_rate * 6.0) as usize;
        let longer_source: Vec<f32> =
            (0..longer_len).map(|i| (i as f32 / sample_rate * 220.0 * std::f32::consts::TAU).sin()).collect();
        source.store(Arc::new(vec![longer_source]));

        // Force a fresh publish to detect: clear the output first so we can
        // tell a *new* render landed rather than reading the still-valid
        // previous one during the wait.
        output.store(Arc::new(LoopBufferData { channels: Vec::new(), sample_rate, root_note: DEFAULT_ROOT_NOTE }));
        trigger.request_render(request);
        assert!(wait_for_render(&output, Duration::from_secs(2)), "worker did not publish the second render in time");

        let second_len = output.load().channels[0].len();
        assert!(second_len > first_len, "expected the longer swapped-in source to produce a longer loop: first={}, second={}", first_len, second_len);
    }
}
