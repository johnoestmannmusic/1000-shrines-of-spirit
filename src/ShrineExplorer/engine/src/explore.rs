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
//! turns at −2°/s and starts at the beginning.

use crate::randomize::{apply, Group, GROUPS};
use crate::rng::{hash3, Rng};
use crate::sim::SimConfig;

/// How large every group's regions are, as a multiple of its own size: above
/// 1 the map changes more gently from step to step, below 1 more busily.
pub const MAP_REGION_SCALE: f64 = 1.0;

/// The camera turn of every shrine away from home, degrees per second.
pub const MAP_CAMERA_SPEED: i16 = -2;

/// A group's region size in cells (before [`MAP_REGION_SCALE`]): large for
/// the settings that set a place's character, small for the details. A step
/// crosses a group's border about once in its size, so these give about two
/// changed groups per step, on top of the new architecture.
fn region_size(g: Group) -> f64 {
    match g {
        Group::Seed => 1.0,
        Group::MusicSeeds => 4.0,
        Group::Biome => 12.0,
        Group::Weather => 8.0,
        Group::Mode => 14.0,
        Group::Harmony => 8.0,
        Group::Tempo => 6.0,
        Group::Drums => 6.0,
        Group::ModeDetails => 6.0,
        Group::Atmosphere => 6.0,
        Group::Drone => 6.0,
        Group::Glitches => 4.0,
        Group::Wildlife => 10.0,
        Group::CodeOverlay => 10.0,
        Group::Architecture => 8.0,
    }
}

/// The world a home belongs to: a different home is a different map.
pub fn world(home: &SimConfig) -> u64 {
    hash3(home.seed, home.seeds.s1 ^ home.seeds.s5.rotate_left(17), home.seeds.s6 ^ 0x6D61_70)
}

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
        let ((rx, ry), home_region) = region(w, k, *g, x, y);
        if home_region {
            continue;
        }
        let mut rng = Rng::new(hash3(w, 0x6E00 + k as u64, pack(rx, ry)));
        apply(&mut c, &[*g], &mut rng);
    }
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
