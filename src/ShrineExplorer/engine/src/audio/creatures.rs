//! The voices of the wildlife: every call synthesised from sines, a little
//! breath noise and simple envelopes — no samples. Which creature calls, and
//! when, comes from the shared schedule in [`crate::wildlife`].

use crate::filter::{OnePole, Svf};
use crate::math::{cos_turns, pan_gains, sin_turns};
use crate::rng::Rng;
use crate::wildlife::{Call, Creature};
use crate::SAMPLE_RATE;

const SR: f64 = SAMPLE_RATE as f64;
/// The most calls that can overlap; the oldest give way.
const MAX_VOICES: usize = 12;
/// Wildlife sits well under the weather.
const GAIN: f64 = 0.08;

/// One note of a call.
#[derive(Clone, Copy)]
struct Note {
    start: usize,
    len: usize,
    /// Pitch glides from `f0` to `f1` across the note.
    f0: f64,
    f1: f64,
    amp: f64,
    /// Amplitude trembling: rate (Hz) and depth.
    trem: (f64, f64),
    /// How much of the note is breath rather than tone.
    breath: f64,
    /// A pulse train gating the note (a croak), in Hz; 0 for none.
    pulse: f64,
}

#[derive(Clone)]
struct Voice {
    wait: usize,
    t: usize,
    notes: Vec<Note>,
    phases: Vec<f64>,
    rng: Rng,
    breath: Svf,
    tone_lp: OnePole,
    gain: (f64, f64),
}

impl Voice {
    fn end(&self) -> usize {
        self.notes.iter().map(|n| n.start + n.len).max().unwrap_or(0)
    }

    fn next(&mut self) -> (f64, f64) {
        if self.wait > 0 {
            self.wait -= 1;
            return (0.0, 0.0);
        }
        let t = self.t;
        self.t += 1;
        let noise = self.rng.bipolar();
        let mut y = 0.0;
        for (n, ph) in self.notes.iter().zip(self.phases.iter_mut()) {
            if t < n.start || t >= n.start + n.len {
                continue;
            }
            let p = (t - n.start) as f64 / n.len as f64;
            let env = sin_turns(0.5 * p);
            let hz = n.f0 + (n.f1 - n.f0) * p;
            *ph += hz / SR;
            *ph -= ph.floor();
            let s = t as f64 / SR;
            let trem = 1.0 - n.trem.1 * (0.5 - 0.5 * cos_turns(s * n.trem.0));
            let gate = if n.pulse > 0.0 {
                let g = 0.5 + 0.5 * cos_turns(s * n.pulse);
                g * g * g
            } else {
                1.0
            };
            let tone = sin_turns(*ph) + 0.25 * sin_turns(2.0 * *ph) * n.breath;
            y += n.amp * env * trem * gate * (tone * (1.0 - n.breath) + n.breath * 3.0 * noise);
        }
        let y = self.tone_lp.process(self.breath.process(y).low);
        (y * self.gain.0, y * self.gain.1)
    }
}

/// Every call currently sounding.
#[derive(Clone, Default)]
pub struct Creatures {
    voices: Vec<Voice>,
}

impl Creatures {
    /// Start `call`, `wait` samples from now.
    pub fn trigger(&mut self, call: &Call, wait: usize) {
        let notes = notes(call);
        if notes.is_empty() {
            return;
        }
        if self.voices.len() >= MAX_VOICES {
            self.voices.remove(0);
        }
        let near = call.nearness();
        let (l, r) = pan_gains(call.pan());
        let top = notes.iter().map(|n| n.f0.max(n.f1)).fold(0.0, f64::max);
        self.voices.push(Voice {
            wait,
            t: 0,
            phases: vec![0.0; notes.len()],
            notes,
            rng: Rng::new(call.hash ^ 0x4252_4541),
            // Softens the breath and the edges; farther calls are duller.
            breath: Svf::new((top * 2.5).min(16_000.0), 0.7, SR),
            tone_lp: OnePole::new(2_500.0 + 9_000.0 * near, SR),
            gain: (GAIN * near * l, GAIN * near * r),
        });
    }

    pub fn next(&mut self) -> (f64, f64) {
        let (mut l, mut r) = (0.0, 0.0);
        for v in self.voices.iter_mut() {
            let (a, b) = v.next();
            l += a;
            r += b;
        }
        self.voices.retain(|v| v.wait > 0 || v.t < v.end());
        (l, r)
    }
}

/// The notes of one call, from the creature and the call's own randomness.
fn notes(call: &Call) -> Vec<Note> {
    let mut r = call.rng();
    let pitch = call.resident.pitch;
    let s = |seconds: f64| (seconds * SR) as usize;
    let note = |start, len, f0, f1, amp| Note { start, len, f0, f1, amp, trem: (0.0, 0.0), breath: 0.0, pulse: 0.0 };
    let mut out = Vec::new();
    let mut at = 0;
    match call.resident.creature {
        Creature::Songbird => {
            // A phrase of quick chirps, each sweeping up or down.
            for _ in 0..r.int(3, 6) {
                let len = s(r.range(0.04, 0.12));
                let f0 = r.range(2_600.0, 4_800.0) * pitch;
                out.push(note(at, len, f0, f0 * r.range(0.6, 1.6), r.range(0.7, 1.0)));
                at += len + s(r.range(0.02, 0.08));
            }
        }
        Creature::Dove => {
            // "hoo-HOO-hoo": soft, a little breathy, the middle note higher.
            let base = 420.0 * pitch;
            for (k, rel) in [1.0, 1.12, 0.95].into_iter().enumerate().take(r.int(2, 3) as usize) {
                let len = s(if k == 1 { 0.42 } else { 0.3 });
                out.push(Note { trem: (6.0, 0.15), breath: 0.15, ..note(at, len, base * rel, base * rel * 0.97, 0.9) });
                at += len + s(0.12);
            }
        }
        Creature::Frog => {
            // One to three croaks: a buzzing pulse train on a low tone.
            for _ in 0..r.int(1, 3) {
                let len = s(r.range(0.25, 0.5));
                let f0 = r.range(440.0, 620.0) * pitch;
                out.push(Note { breath: 0.35, pulse: r.range(22.0, 34.0), ..note(at, len, f0, f0 * 0.92, 1.0) });
                at += len + s(r.range(0.12, 0.25));
            }
        }
        Creature::Cricket => {
            // Three or four quick, high pulses.
            let f = r.range(4_300.0, 5_000.0) * pitch;
            for _ in 0..r.int(3, 4) {
                out.push(note(at, s(0.018), f, f, 0.8));
                at += s(0.04);
            }
        }
        Creature::Cicada => {
            // A long trill, trembling fast, swelling and dying away.
            let f = r.range(5_800.0, 7_000.0) * pitch;
            out.push(Note { trem: (r.range(40.0, 56.0), 0.9), ..note(0, s(r.range(1.0, 2.2)), f, f * 0.98, 0.6) });
        }
        Creature::Owl => {
            // Two or three low hoots, the last one dipping.
            let f = r.range(300.0, 380.0) * pitch;
            let n = r.int(2, 3);
            for k in 0..n {
                let dip = if k + 1 == n { 0.86 } else { 0.95 };
                out.push(Note { breath: 0.25, ..note(at, s(0.45), f, f * dip, 1.0) });
                at += s(0.7);
            }
        }
        Creature::Firefly => {}
    }
    out
}
