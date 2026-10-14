//! Wildlife: a few creatures living around the shrine, heard (and sometimes
//! seen) when the **Wildlife** setting is on.
//!
//! Which creatures live here comes from the **biome**, and the seed picks two of
//! them and gives each its own voice (pitch) and temper (how often it calls).
//! Every call is an event on a stateless schedule, like the weather's, so the
//! audio instance hears a songbird at the moment the visual instance shows it
//! flit across the sky. The weather decides how much they call: freely in
//! clear air and at dusk, hardly at all in a storm or in snow.

use crate::framebuffer::{Framebuffer, Rgba};
use crate::math::sin_turns;
use crate::plants::Biome;
use crate::rng::{hash3, Rng};
use crate::weather::{WeatherKind, WeatherState};
use crate::{LOGICAL_H, LOGICAL_W};

const STREAM: u64 = 0x5749_4c44; // "WILD"

/// The creatures. Each is synthesised from scratch in
/// [`crate::creatures`]; fireflies are only seen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Creature {
    /// Pulsed, resonant croaks.
    Frog,
    /// A soft two- or three-note call.
    Dove,
    /// Short phrases of quick chirps, and a small bird crossing the sky.
    Songbird,
    /// Bursts of three or four high pulses.
    Cricket,
    /// A long, fast-trembling high trill.
    Cicada,
    /// Low, breathy hoots.
    Owl,
    /// Slow green-gold glows over the plateau.
    Firefly,
}

impl Creature {
    pub fn name(self) -> &'static str {
        match self {
            Creature::Frog => "frogs",
            Creature::Dove => "doves",
            Creature::Songbird => "songbirds",
            Creature::Cricket => "crickets",
            Creature::Cicada => "cicadas",
            Creature::Owl => "owls",
            Creature::Firefly => "fireflies",
        }
    }

    /// (slots, shortest period, longest period): how the calls are scheduled.
    fn schedule(self) -> (u64, f64, f64) {
        match self {
            Creature::Frog => (4, 2.0, 5.0),
            Creature::Dove => (2, 7.0, 14.0),
            Creature::Songbird => (3, 4.0, 9.0),
            Creature::Cricket => (5, 1.2, 3.0),
            Creature::Cicada => (2, 6.0, 12.0),
            Creature::Owl => (1, 10.0, 18.0),
            Creature::Firefly => (8, 1.5, 4.0),
        }
    }

    /// Night creatures swell at dusk; birds settle.
    fn at_dusk(self) -> f64 {
        match self {
            Creature::Cricket | Creature::Cicada | Creature::Frog | Creature::Firefly | Creature::Owl => 1.6,
            Creature::Songbird => 0.5,
            Creature::Dove => 0.8,
        }
    }

    /// How long a call lasts (seconds): for drawing what goes with it.
    pub fn seen_for(self) -> f64 {
        match self {
            Creature::Songbird => 1.8,
            Creature::Firefly => 2.2,
            _ => 0.0,
        }
    }
}

/// The creatures a biome can hold, most characteristic first.
pub fn residents(biome: Biome) -> [Creature; 3] {
    match biome {
        Biome::Moss => [Creature::Frog, Creature::Dove, Creature::Cricket],
        Biome::Verdant => [Creature::Songbird, Creature::Cricket, Creature::Dove],
        Biome::Fungal => [Creature::Cicada, Creature::Owl, Creature::Firefly],
    }
}

/// One creature living here: its kind, voice and temper.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Resident {
    pub creature: Creature,
    /// Pitch multiplier, 0.8..1.25.
    pub pitch: f64,
    /// How readily it calls, 0.6..1.0.
    pub rate: f64,
    stream: u64,
}

/// One call: who, when, and its own random numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Call {
    pub resident: Resident,
    pub at: f64,
    pub hash: u64,
}

impl Call {
    pub fn rng(&self) -> Rng {
        Rng::new(self.hash)
    }

    /// Stereo position in `[-1, 1]`.
    pub fn pan(&self) -> f64 {
        Rng::new(self.hash ^ 0x5041_4e).bipolar() * 0.8
    }

    /// How near it is, `0.35..1`: nearer is louder and brighter.
    pub fn nearness(&self) -> f64 {
        Rng::new(self.hash ^ 0x4e45_4152).range(0.35, 1.0)
    }
}

/// The wildlife of one shrine.
#[derive(Clone, Debug, PartialEq)]
pub struct Wildlife {
    seed: u64,
    pub residents: Vec<Resident>,
}

impl Wildlife {
    /// Two of the biome's creatures, chosen and voiced by the seed.
    pub fn new(seed: u64, biome: Biome) -> Self {
        let mut rng = Rng::stream(seed, STREAM);
        let pool = residents(biome);
        let first = rng.below(pool.len());
        let second = (first + 1 + rng.below(pool.len() - 1)) % pool.len();
        let residents = [pool[first], pool[second]]
            .into_iter()
            .enumerate()
            .map(|(k, creature)| Resident {
                creature,
                pitch: rng.range(0.8, 1.25),
                rate: rng.range(0.6, 1.0),
                stream: STREAM ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9),
            })
            .collect();
        Wildlife { seed, residents }
    }

    /// "frogs and crickets", for the setup screen and the info card.
    pub fn summary(&self) -> String {
        let names: Vec<&str> = self.residents.iter().map(|r| r.creature.name()).collect();
        names.join(" and ")
    }

    /// How much a creature calls in this weather, in `[0, 1]`.
    fn activity(creature: Creature, kind: WeatherKind) -> f64 {
        let base = match kind {
            WeatherKind::Clear => 1.0,
            WeatherKind::Dusk => creature.at_dusk(),
            WeatherKind::Rain => 0.35,
            WeatherKind::Wind => 0.45,
            WeatherKind::Fog => 0.6,
            WeatherKind::Snow => 0.08,
            WeatherKind::Storm => 0.04,
        };
        // Frogs love the rain.
        let base = if creature == Creature::Frog && kind == WeatherKind::Rain { 1.1 } else { base };
        base.clamp(0.0, 1.0)
    }

    /// Every call in `[t0, t1)`. Pure: the same range always gives the same
    /// calls, in the audio and the visuals alike.
    pub fn calls(&self, kind: WeatherKind, t0: f64, t1: f64) -> Vec<Call> {
        let mut out = Vec::new();
        if t1 <= t0 {
            return out;
        }
        for resident in &self.residents {
            let gate = Self::activity(resident.creature, kind) * resident.rate;
            let (slots, lo, hi) = resident.creature.schedule();
            let mut slot_rng = Rng::stream(self.seed, resident.stream);
            for slot in 0..slots {
                let period = slot_rng.range(lo, hi);
                let phase = slot_rng.unit() * period;
                let first = ((t0 + phase) / period).floor() as i64 - 1;
                let last = ((t1 + phase) / period).floor() as i64;
                for n in first..=last {
                    let mut r = Rng::new(hash3(self.seed ^ resident.stream, slot, n as u64));
                    let at = (n as f64 + 0.9 * r.unit()) * period - phase;
                    if at < t0 || at >= t1 || r.unit() >= gate {
                        continue;
                    }
                    out.push(Call { resident: *resident, at, hash: r.next_u64() });
                }
            }
        }
        out
    }

    /// Draw what can be seen of the calls under way at `t`: a songbird crossing
    /// the sky, fireflies glowing over the plateau. Rain thins them like it
    /// thins the calls (they are the same events).
    pub fn draw(&self, fb: &mut Framebuffer, kind: WeatherKind, t: f64, w: &WeatherState, grade: &dyn Fn(Rgba) -> Rgba) {
        let longest = self.residents.iter().map(|r| r.creature.seen_for()).fold(0.0, f64::max);
        if longest <= 0.0 {
            return;
        }
        for call in self.calls(kind, t - longest, t + 1e-9) {
            let span = call.resident.creature.seen_for();
            let age = (t - call.at) / span;
            if !(0.0..1.0).contains(&age) {
                continue;
            }
            let mut r = call.rng();
            match call.resident.creature {
                Creature::Songbird => {
                    // A small dark bird crossing the upper sky, wings beating.
                    let dir = if r.chance(0.5) { -1.0 } else { 1.0 };
                    let x0 = r.range(40.0, LOGICAL_W as f64 - 40.0);
                    let y0 = r.range(20.0, 110.0);
                    let x = x0 + dir * age * r.range(90.0, 160.0);
                    let y = y0 - 10.0 * age + 3.0 * sin_turns(age * 3.0);
                    let colour = grade([40, 36, 44, 255]);
                    let flap = sin_turns(t * 7.0 + r.unit()) > 0.0;
                    let (px, py) = (x as i32, y as i32);
                    fb.put(px, py, colour);
                    let wing = if flap { -1 } else { 0 };
                    fb.put(px - 1, py + wing, colour);
                    fb.put(px + 1, py + wing, colour);
                    fb.put(px - 2, py + wing - (flap as i32), colour);
                    fb.put(px + 2, py + wing - (flap as i32), colour);
                }
                Creature::Firefly => {
                    // A glow that swells and fades as it drifts upward.
                    let x = r.range(30.0, LOGICAL_W as f64 - 30.0) + 6.0 * sin_turns(age + r.unit());
                    let y = r.range(140.0, LOGICAL_H as f64 - 40.0) - 14.0 * age;
                    let glow = sin_turns(0.5 * age) * (1.0 - 0.5 * w.light);
                    let colour = [214, 240, 120, 255];
                    let (px, py) = (x as i32, y as i32);
                    fb.blend_over(px, py, colour, (glow * 255.0) as u32);
                    for (ox, oy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                        fb.blend_over(px + ox, py + oy, colour, (glow * 90.0) as u32);
                    }
                }
                _ => {}
            }
        }
    }
}
