//! The weather bed: the weather as sound.
//!
//! It plays from the *same* [`Weather`] the visuals use. The continuous layers
//! (wind, the rain's wash, snow hush, fog air, rustle, crackle) follow the
//! shared [`crate::weather::WeatherState`] levels the scene draws its particles
//! from, and every event the scene draws — a splash, a drip, an ember's spark,
//! a lightning strike — is found from the same schedule and heard on the same
//! sample clock (thunder after its flash, by the strike's distance). It is
//! mixed on top of the ported 0011 music, which is left untouched.

use crate::audio::ambience::{Ambience, Hit};
use crate::audio::creatures::Creatures;
use crate::audio::rain::Rain;
use crate::audio::thunder::Thunder;
use crate::audio::wind::Wind;
use crate::filter::OnePole;
use crate::rng::Rng;
use crate::weather::{EventKind, Weather, WeatherState};
use crate::weather_fx::DRIP_S;
use crate::wildlife::Wildlife;
use crate::{GRID_H, GRID_W, SAMPLE_RATE};

const SR: f64 = SAMPLE_RATE as f64;
/// The weather state is sampled this often (samples); it moves over seconds.
const CONTROL: usize = 32;

/// The weather bed sits this far under the music (about -8 dB), so it reads as
/// background rather than a foreground instrument.
const BED_GAIN: f64 = 0.4;

/// Whether each raindrop the scene draws is also heard as its own tap or
/// plink. Off: the rain is heard as its wash.
const RAINDROP_HITS: bool = false;

/// Wind, rain, thunder and the small sounds, mixed into an existing
/// interleaved stereo buffer.
#[derive(Clone)]
pub struct WeatherBed {
    wind: Wind,
    rain: Rain,
    thunder: Thunder,
    ambience: Ambience,
    /// The creatures, when the Wildlife setting is on.
    wildlife: Option<Wildlife>,
    creatures: Creatures,
    /// Which grid cells hold a column (a splash there lands on stone).
    hard: Vec<bool>,
    /// Snow muffles everything in the bed.
    muffle: [OnePole; 2],
    /// Under a roof: the weather is heard through glass, every splash lands
    /// on a pane, and the wind is a muffled pressure outside.
    roofed: bool,
    state: Option<WeatherState>,
    tick: usize,
    /// Sounds found by the schedule, waiting for their exact sample. They
    /// start then (not at the top of the block that found them), so nothing
    /// about the bed depends on how the audio is cut into blocks.
    queue: Vec<(u64, Due)>,
}

/// A scheduled sound, waiting to start.
#[derive(Clone)]
enum Due {
    Hit(Hit),
    Call(crate::wildlife::Call),
    Thunder { strength: f64, distance: f64, pan: f64, hash: u64 },
}

impl WeatherBed {
    /// `hard` is one flag per grid cell, row-major: does a column stand there?
    /// `wildlife` is `Some` when the Wildlife setting is on.
    pub fn new(seed: u64, hard: Vec<bool>, wildlife: Option<Wildlife>) -> Self {
        WeatherBed {
            wind: Wind::new(seed),
            rain: Rain::new(seed ^ 0x9E37_79B9_7F4A_7C15),
            thunder: Thunder::default(),
            ambience: Ambience::new(seed),
            wildlife,
            creatures: Creatures::default(),
            hard,
            muffle: [OnePole::new(9_000.0, SR), OnePole::new(9_000.0, SR)],
            roofed: false,
            state: None,
            tick: 0,
            queue: Vec::new(),
        }
    }

    /// Heard from under a roof (the indoor futures).
    pub fn roofed(mut self, on: bool) -> Self {
        self.roofed = on;
        self
    }

    fn is_hard(&self, x: f64, y: f64) -> bool {
        if self.roofed {
            // Every drop lands on the glass.
            return true;
        }
        let (gx, gy) = (x.round() as i64, y.round() as i64);
        if gx < 0 || gy < 0 || gx >= GRID_W as i64 || gy >= GRID_H as i64 {
            return false;
        }
        self.hard.get((gy * GRID_W as i64 + gx) as usize).copied().unwrap_or(false)
    }

    /// Queue every event that sounds in samples `[s0, s1)`, at its own
    /// sample. Each sound's randomness comes from its event, never from a
    /// stream shared with the bed's continuous noise.
    fn schedule(&mut self, weather: &Weather, s0: u64, s1: u64) {
        // Look a couple of samples wider than the block, then keep exactly
        // the sounds whose own sample falls inside it: each sound belongs to
        // one block by its sample number, never by a rounded time.
        let pad = 2.0 / SR;
        let (t0, t1) = (s0 as f64 / SR - pad, s1 as f64 / SR + pad);
        let at_sample = |at: f64| (at.max(0.0) * SR) as u64;
        let mut due: Vec<(u64, Due)> = Vec::new();
        // Sounds owned by an event in this block but landing after it.
        let mut spill: Vec<(u64, Due)> = Vec::new();
        // Single raindrops (taps, plinks, drips) are seen but no longer heard:
        // as separate pitched hits they bubbled over the music. The rain is
        // heard as its wash alone.
        if RAINDROP_HITS {
            for e in weather.events(EventKind::Splash, t0, t1) {
                let (x, y) = e.spot();
                let (pan, hard) = (e.pan(), self.is_hard(x, y));
                let gain = e.rng().range(0.08, 0.22);
                let mut r = Rng::new(e.hash ^ 0x4849_5453);
                let hit = if hard { Hit::plink(0, &mut r, pan, gain) } else { Hit::tap(0, &mut r, pan, gain * 1.4) };
                due.push((at_sample(e.at), Due::Hit(hit)));
            }
            // A drip is heard when it lands, at the end of its fall.
            for e in weather.events(EventKind::Drip, t0 - DRIP_S, t1 - DRIP_S) {
                let mut r = Rng::new(e.hash ^ 0x4452_4950);
                due.push((at_sample(e.at + DRIP_S), Due::Hit(Hit::drip(0, &mut r, e.pan(), 0.16))));
            }
        }
        for e in weather.events(EventKind::Spark, t0, t1) {
            let pan = e.pan();
            let mut r = e.rng();
            // A spark's crackles belong to the spark, wherever they land.
            if at_sample(e.at) < s0 || at_sample(e.at) >= s1 {
                continue;
            }
            for k in 0..r.int(2, 4) {
                let at = e.at + k as f64 * r.range(0.006, 0.02);
                let mut rr = Rng::new(e.hash ^ 0x5350_4b00 ^ k);
                spill.push((at_sample(at), Due::Hit(Hit::crackle(0, &mut rr, pan, 0.22))));
            }
        }
        if let Some(wildlife) = &self.wildlife {
            for call in wildlife.calls(weather.kind, t0, t1) {
                due.push((at_sample(call.at), Due::Call(call)));
            }
        }
        for e in weather.events(EventKind::Strike, t0 - 3.1, t1) {
            let at = e.thunder_at();
            if at >= t0 && at < t1 {
                let (strength, distance) = e.strike();
                due.push((at_sample(at), Due::Thunder { strength, distance, pan: e.pan(), hash: e.hash }));
            }
        }
        due.retain(|d| d.0 >= s0 && d.0 < s1);
        due.extend(spill);
        // Stable: sounds due on the same sample keep the schedule's order.
        due.sort_by_key(|d| d.0);
        self.queue.extend(due);
        self.queue.sort_by_key(|d| d.0);
    }

    /// Start everything due at sample `s`.
    fn start_due(&mut self, s: u64) {
        while self.queue.first().is_some_and(|d| d.0 <= s) {
            let (_, due) = self.queue.remove(0);
            match due {
                Due::Hit(hit) => self.ambience.start(hit),
                Due::Call(call) => self.creatures.trigger(&call, 0),
                Due::Thunder { strength, distance, pan, hash } => self.thunder.add(0, strength, distance, pan, hash),
            }
        }
    }

    /// Add the bed into `out`, starting at `t0` seconds. `out` must already hold
    /// the music mix; this only adds to it.
    pub fn render(&mut self, out: &mut [f32], t0: f64, weather: &Weather) {
        self.render_from((t0 * SR).round() as u64, out, weather);
    }

    /// [`WeatherBed::render`] from sample `s0`: the exact clock, so the bed
    /// is the same however the audio is cut into blocks.
    pub fn render_from(&mut self, s0: u64, out: &mut [f32], weather: &Weather) {
        let frames = out.len() / 2;
        self.ambience.rustle_on = weather.kind != crate::weather::WeatherKind::Wind;
        self.schedule(weather, s0, s0 + frames as u64);
        for i in 0..frames {
            let s = s0 + i as u64;
            self.start_due(s);
            if self.tick % CONTROL == 0 || self.state.is_none() {
                let st = weather.state(s as f64 / SR);
                for m in self.muffle.iter_mut() {
                    let open = 9_000.0 - 6_800.0 * st.snow;
                    m.set_cutoff(if self.roofed { open * 0.4 } else { open }, SR);
                }
                self.state = Some(st);
            }
            self.tick = self.tick.wrapping_add(1);
            let s = self.state.expect("sampled above");
            let (mut wl, mut wr) = self.wind.next(s.wind_strength, s.gust, s.wind_dir);
            if self.roofed {
                wl *= 0.45;
                wr *= 0.45;
            }
            let (rl, rr) = self.rain.next(s.rain);
            let (tl, tr) = self.thunder.next();
            let (al, ar) = self.ambience.next(&s);
            let (cl, cr) = self.creatures.next();
            let l = self.muffle[0].process(wl + rl + al + cl) + tl;
            let r = self.muffle[1].process(wr + rr + ar + cr) + tr;
            out[2 * i] += (l * BED_GAIN).clamp(-1.0, 1.0) as f32;
            out[2 * i + 1] += (r * BED_GAIN).clamp(-1.0, 1.0) as f32;
        }
    }
}
