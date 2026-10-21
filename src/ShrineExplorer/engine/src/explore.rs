//! The explorer's map: an endless grid of shrines around a home.
//!
//! Every shrine has integer coordinates `(x, y)`; the home is `(0, 0)`. The
//! shrine at any spot is a pure function of the home and the coordinates, so
//! a place found while exploring can be typed back in and is exactly the
//! same, natively and in the browser.
//!
//! Neighbours are slight mutations of each other:
//! - every spot has its own architecture (a new shrine seed);
//! - every other group of settings (those in [`crate::randomize::GROUPS`])
//!   changes only across the borders of its own square regions, each group
//!   with its own region size and offset, so a step usually crosses one to
//!   three borders and the rest carries over;
//! - the region around the home keeps the home's values, so the map grows
//!   out of the home.
//!
//! The master effects are always the home's, and every shrine away from home
//! holds still (no camera turn, so the map keeps its directions on screen)
//! and starts at the beginning.

use crate::randomize::{apply, Group, GROUPS};
use crate::rng::{hash3, Rng};
use crate::sim::SimConfig;

/// How large every group's regions are, as a multiple of its own size: above
/// 1 the map changes more gently from step to step, below 1 more busily.
pub const MAP_REGION_SCALE: f64 = 0.5;

/// The camera turn of every shrine away from home, degrees per second: none,
/// so the map's directions always look the same way on screen.
pub const MAP_CAMERA_SPEED: i16 = 0;

/// A group's region size in cells (before [`MAP_REGION_SCALE`]): large for
/// the settings that set a place's character, small for the details. A step
/// crosses a group's border about once in its size; at the explorer's scale
/// of 0.5 a step changes about four groups, on top of the new architecture.
fn region_size(g: Group) -> f64 {
    match g {
        Group::Seed => 1.0,
        // The music changes with the biome (see `MUSIC`), so these four
        // sizes are not used.
        Group::MusicSeeds | Group::Mode | Group::Harmony | Group::Tempo => 12.0,
        Group::Biome => 12.0,
        Group::Weather => 4.0,
        Group::Drums | Group::ModeDetails | Group::Atmosphere | Group::Drone => 12.0,
        Group::Glitches => 2.0,
        Group::Wildlife => 6.0,
        Group::CodeOverlay => 6.0,
        Group::Architecture => 4.0,
    }
}

/// The world a home belongs to: a different home is a different map.
pub fn world(home: &SimConfig) -> u64 {
    hash3(home.seed, home.seeds.s1 ^ home.seeds.s5.rotate_left(17), home.seeds.s6 ^ 0x6D61_70)
}

/// The music's groups: they change together, at the biome's borders, so each
/// biome is a musical world of its own; within one, a shrine's music differs
/// only in its glitches and a nudge of tempo ([`TEMPO_NUDGE`]).
const MUSIC: [Group; 8] = [
    Group::MusicSeeds,
    Group::Mode,
    Group::Harmony,
    Group::Tempo,
    Group::Drums,
    Group::ModeDetails,
    Group::Atmosphere,
    Group::Drone,
];

/// How far each shrine nudges its biome's tempo, either way, in BPM.
const TEMPO_NUDGE: i64 = 3;

fn pack(a: i64, b: i64) -> u64 {
    (a as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (b as u64).rotate_left(29)
}

/// Group `k`'s region at `(x, y)`, and whether it is the home's region.
fn region(world: u64, k: usize, g: Group, x: i32, y: i32) -> ((i64, i64), bool) {
    let size = (region_size(g) * MAP_REGION_SCALE).round().max(1.0) as i64;
    let mut o = Rng::new(hash3(world, 0x0FF5_E700, k as u64));
    let (ox, oy) = (o.below(size as usize) as i64, o.below(size as usize) as i64);
    let at = |v: i32, off: i64| (v as i64 + off).div_euclid(size);
    let here = (at(x, ox), at(y, oy));
    (here, here == (at(0, ox), at(0, oy)))
}

/// The shrine at `(x, y)` on `home`'s map. `(0, 0)` is the home itself. The
/// result is a complete, ordinary configuration (its own map position is
/// `(0, 0)`), so it can be played, exported or made a new home.
pub fn at(home: &SimConfig, x: i32, y: i32) -> SimConfig {
    let mut c = home.clone();
    c.settings.map = (0, 0);
    if (x, y) == (0, 0) {
        return c;
    }
    let w = world(home);
    for (k, g) in GROUPS.iter().enumerate() {
        if *g == Group::Seed {
            // A new architecture at every spot.
            c.seed = hash3(w, 0x5EED, pack(x as i64, y as i64)) % 100_000;
            continue;
        }
        // The music follows the biome's regions (its offsets and size).
        let ((rx, ry), home_region) = if MUSIC.contains(g) {
            let biome = GROUPS.iter().position(|b| *b == Group::Biome).unwrap_or(k);
            region(w, biome, Group::Biome, x, y)
        } else {
            region(w, k, *g, x, y)
        };
        if home_region {
            continue;
        }
        let mut rng = Rng::new(hash3(w, 0x6E00 + k as u64, pack(rx, ry)));
        apply(&mut c, &[*g], &mut rng);
    }
    // Each shrine nudges the tempo a little, so neighbours in one biome are
    // close but never identical.
    let mut n = Rng::new(hash3(w, 0x7E4E0, pack(x as i64, y as i64)));
    let nudge = n.below((2 * TEMPO_NUDGE + 1) as usize) as i64 - TEMPO_NUDGE;
    c.settings.bpm = (c.settings.bpm as i64 + nudge).clamp(70, 180) as u16;
    c.settings.camera_speed = MAP_CAMERA_SPEED;
    c.settings.master = home.settings.master;
    c.settings.start_s = 0;
    c
}

/// The configuration a sim plays: the shrine at the config's map position
/// (itself, at home).
pub fn resolve(config: SimConfig) -> SimConfig {
    let (x, y) = config.settings.map;
    if (x, y) == (0, 0) {
        config
    } else {
        at(&config, x, y)
    }
}

/// Which groups differ between two shrines (for display and tests).
pub fn changed_groups(a: &SimConfig, b: &SimConfig) -> Vec<Group> {
    let s = (&a.settings, &b.settings);
    let mut out = Vec::new();
    let mut check = |g: Group, differs: bool| {
        if differs {
            out.push(g);
        }
    };
    check(Group::Seed, a.seed != b.seed);
    check(Group::MusicSeeds, a.seeds != b.seeds);
    check(Group::Biome, a.biome != b.biome);
    check(Group::Weather, a.weather != b.weather);
    check(Group::Mode, s.0.pace != s.1.pace);
    check(Group::Harmony, (s.0.scale, s.0.key, s.0.chords) != (s.1.scale, s.1.key, s.1.chords));
    check(Group::Tempo, s.0.bpm != s.1.bpm);
    check(Group::Drums, (s.0.kit, s.0.space, s.0.drum_reverse) != (s.1.kit, s.1.space, s.1.drum_reverse));
    check(Group::ModeDetails, (s.0.garage, s.0.micro) != (s.1.garage, s.1.micro));
    check(Group::Atmosphere, (s.0.loops, s.0.loop_arcs, s.0.loop_transpose) != (s.1.loops, s.1.loop_arcs, s.1.loop_transpose));
    check(Group::Drone, (s.0.drone, s.0.drone_tone, s.0.drone_rhythm, s.0.loop_rhythm) != (s.1.drone, s.1.drone_tone, s.1.drone_rhythm, s.1.loop_rhythm));
    check(Group::Glitches, s.0.glitch != s.1.glitch);
    check(Group::Wildlife, s.0.wildlife != s.1.wildlife);
    check(Group::CodeOverlay, s.0.code != s.1.code);
    check(Group::Architecture, s.0.arch != s.1.arch);
    out
}

/// A spot at a glance, for the page's minimap: the biome index in bits 0–3,
/// the weather index in bits 4–7, the mode in bits 8–9.
pub fn glance(home: &SimConfig, x: i32, y: i32) -> u32 {
    let c = at(home, x, y);
    let biome = crate::plants::Biome::ALL.iter().position(|b| *b == c.biome).unwrap_or(0) as u32;
    let weather = crate::weather::WeatherKind::ALL.iter().position(|w| *w == c.weather).unwrap_or(0) as u32;
    biome | weather << 4 | c.settings.pace.index() << 8
}
