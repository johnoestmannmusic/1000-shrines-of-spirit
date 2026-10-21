//! The plant library: a set of generators (grass, fern, vine, fungus) that
//! share one output shape, plus the rules that scatter them around the shrine.
//!
//! A plant is stored as a handful of **strokes** — polylines in a local frame
//! anchored to a grid cell, `x` to the right and `y` up, in fixed-point units.
//! A [`Sway`] describes how it bends in the weather. Nothing here draws; the
//! scene assembly (`FEAT-10`) turns strokes into pixels, so the same plant data
//! can drive the editor preview and the final render.
//!
//! A biome biases which generators are used; the default set is chosen at
//! setup. Everything is seeded per cell, so adding a plant family later cannot
//! shift another cell's plant.

use crate::math::sin_turns;
use crate::rng::Rng;
use crate::shrine::Shrine;
use crate::{GRID_H, GRID_W};

/// Stream id, so plants never share a sequence with another generator.
const STREAM: u64 = 0x504c_4e54; // "PLNT"

/// The most plants the scene will ever hold, for a bounded frame budget.
pub const MAX_PLANTS: usize = 260;

/// Plant families.
pub mod kind {
    pub const GRASS: u8 = 0;
    pub const FERN: u8 = 1;
    pub const VINE: u8 = 2;
    pub const FUNGUS: u8 = 3;
}

/// Which palette role a stroke draws with.
pub mod role {
    pub const DARK: u8 = 0;
    pub const MID: u8 = 1;
    pub const LIGHT: u8 = 2;
    pub const ACCENT: u8 = 3;
}

/// A biome biases the mixture of plant families. The first three are the
/// ruin's own wild places; the last four are futures built around it, each
/// with its own palette, light, ground, props and (for the roofed two) a
/// ceiling the weather lands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Biome {
    Moss,
    Verdant,
    Fungal,
    /// A bright near-future city block: roads, sidewalks, signage.
    NearFuture,
    /// Steel, glass, blue-green neon and screens, under a panelled ceiling.
    ResearchLab,
    /// Parkland and white titanium machines.
    Ecofuture,
    /// A sunlit glass atrium full of plants, on minimal steel scaffolding.
    Greenhouse,
}

impl Biome {
    pub const ALL: [Biome; 7] = [
        Biome::Moss,
        Biome::Verdant,
        Biome::Fungal,
        Biome::NearFuture,
        Biome::ResearchLab,
        Biome::Ecofuture,
        Biome::Greenhouse,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Biome::Moss => "Moss",
            Biome::Verdant => "Verdant",
            Biome::Fungal => "Fungal",
            Biome::NearFuture => "Near Future",
            Biome::ResearchLab => "Research Lab",
            Biome::Ecofuture => "Ecofuture",
            Biome::Greenhouse => "Greenhouse",
        }
    }

    /// A one-line sketch, for the setup screen and the Learn page.
    pub fn blurb(self) -> &'static str {
        match self {
            Biome::Moss => "an old ruin softened by moss, ferns and grass",
            Biome::Verdant => "a lush ruin, overgrown with leaves and flowers",
            Biome::Fungal => "a damp ruin of mushrooms and glowing caps",
            Biome::NearFuture => "a bright city block: roads, signs, a vending machine",
            Biome::ResearchLab => "steel, glass and neon screens, under a ceiling",
            Biome::Ecofuture => "parkland with white machines and solar tiles",
            Biome::Greenhouse => "a glass atrium of plants on a steel frame",
        }
    }

    /// The biome called `name`, ignoring case, spaces, `-` and `_`
    /// ("near-future", "NearFuture" and "Near Future" all match).
    pub fn from_name(name: &str) -> Option<Biome> {
        let key = |s: &str| s.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_lowercase()).collect::<String>();
        let want = key(name);
        Biome::ALL.into_iter().find(|b| key(b.name()) == want)
    }

    /// One of the four built futures (not the ruin's wild biomes).
    pub fn futuristic(self) -> bool {
        matches!(self, Biome::NearFuture | Biome::ResearchLab | Biome::Ecofuture | Biome::Greenhouse)
    }

    /// Under a ceiling: rain and snow land on the roof, not the ground.
    pub fn roofed(self) -> bool {
        matches!(self, Biome::ResearchLab | Biome::Greenhouse)
    }

    /// Relative weights for grass, fern, vine, fungus.
    fn weights(self) -> [f64; 4] {
        match self {
            Biome::Moss => [3.0, 1.0, 2.0, 1.5],
            Biome::Verdant => [2.0, 3.0, 1.0, 0.4],
            Biome::Fungal => [1.0, 1.0, 2.5, 4.0],
            // Verge grass and the odd weed through the paving.
            Biome::NearFuture => [4.0, 0.6, 1.0, 0.6],
            // Potted grasses and ferns; a little lab culture.
            Biome::ResearchLab => [2.0, 2.0, 0.5, 0.6],
            Biome::Ecofuture => [3.0, 2.5, 1.5, 0.3],
            Biome::Greenhouse => [1.5, 3.5, 3.0, 0.5],
        }
    }

    /// Overall plant density on open ground.
    fn ground_density(self) -> f64 {
        match self {
            Biome::Moss => 0.55,
            Biome::Verdant => 0.65,
            Biome::Fungal => 0.40,
            Biome::NearFuture => 0.14,
            Biome::ResearchLab => 0.12,
            Biome::Ecofuture => 0.62,
            Biome::Greenhouse => 0.78,
        }
    }
}

/// How a plant bends: `phase` in turns, `amplitude` in fixed-point units at the
/// tip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sway {
    pub phase: f64,
    pub amplitude: f64,
}

/// A polyline in the plant's local frame (`y` up, fixed-point units).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stroke {
    pub points: Vec<(i32, i32)>,
    pub width: u8,
    pub role: u8,
}

/// One generated plant.
#[derive(Clone, Debug, PartialEq)]
pub struct Plant {
    pub kind: u8,
    pub gx: i32,
    pub gy: i32,
    /// Stack level the plant grows from (a shrine top, or 0 on the ground).
    pub z: i32,
    /// Sub-cell placement, fixed-point units.
    pub offset: (i32, i32),
    /// Nominal height in fixed-point units.
    pub height: i32,
    pub sway: Sway,
    pub strokes: Vec<Stroke>,
}

/// All the plants in a scene.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plants {
    pub items: Vec<Plant>,
}

impl Plants {
    /// Scatter plants around `shrine` for `seed` in `biome`.
    pub fn generate(seed: u64, shrine: &Shrine, biome: Biome) -> Self {
        let (ox, oy) = shrine.origin();
        let mut items = Vec::new();
        // The futures grow plants only where their layout leaves soil.
        let plan = biome.futuristic().then(|| crate::layout::Plan::new(seed, biome));

        for gy in 0..GRID_H as i32 {
            if items.len() >= MAX_PLANTS {
                break;
            }
            for gx in 0..GRID_W as i32 {
                if items.len() >= MAX_PLANTS {
                    break;
                }
                let inside = gx >= ox && gx < ox + crate::SHRINE_W as i32
                    && gy >= oy && gy < oy + crate::SHRINE_H as i32;
                if !inside && plan.is_some_and(|p| !p.cell(gx, gy).grows()) {
                    continue;
                }
                let density = if inside { biome.ground_density() * 0.18 } else { biome.ground_density() };
                let mut rng = Rng::stream(seed, STREAM ^ cell(gx, gy));
                if !rng.chance(density) {
                    continue;
                }
                let z = if inside { shrine.height_at(gx - ox, gy - oy) as i32 - 1 } else { 0 };
                if z < 0 {
                    continue;
                }
                items.push(grow(&mut rng, biome, gx, gy, z));
            }
        }

        Plants { items }
    }

    pub fn count_of(&self, kind: u8) -> usize {
        self.items.iter().filter(|p| p.kind == kind).count()
    }

    /// A stable hash of every plant, for golden tests.
    pub fn hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        mix(self.items.len() as u64, &mut h);
        for p in &self.items {
            mix(p.kind as u64, &mut h);
            mix(p.gx as u64, &mut h);
            mix(p.gy as u64, &mut h);
            mix(p.z as u64, &mut h);
            mix(p.strokes.len() as u64, &mut h);
            for s in &p.strokes {
                mix(s.width as u64, &mut h);
                mix(s.role as u64, &mut h);
                for &(x, y) in &s.points {
                    mix(x as u64, &mut h);
                    mix(y as u64, &mut h);
                }
            }
        }
        h
    }
}

/// FNV-1a step, shared by the hashes.
fn mix(v: u64, h: &mut u64) {
    *h ^= v;
    *h = h.wrapping_mul(0x0000_0100_0000_01b3);
}

fn cell(gx: i32, gy: i32) -> u64 {
    ((gx as u64) << 32) | (gy as u32 as u64)
}

/// Build one plant, choosing a family by the biome's weights.
fn grow(rng: &mut Rng, biome: Biome, gx: i32, gy: i32, z: i32) -> Plant {
    let weights = biome.weights();
    let family = rng.weighted(&weights) as u8;
    let scale = rng.range(0.8, 1.25);
    let offset = (rng.range(-28.0, 28.0) as i32, rng.range(-14.0, 14.0) as i32);
    let (strokes, height) = match family {
        kind::GRASS => grass(rng, scale),
        kind::FERN => fern(rng, scale),
        kind::VINE => vine(rng, scale),
        _ => fungus(rng, scale),
    };
    Plant {
        kind: family,
        gx,
        gy,
        z,
        offset,
        height,
        sway: Sway { phase: rng.unit(), amplitude: rng.range(6.0, 22.0) },
        strokes,
    }
}

fn grass(rng: &mut Rng, scale: f64) -> (Vec<Stroke>, i32) {
    let blades = rng.int(3, 6);
    let height = rng.range(26.0, 54.0) * scale;
    let mut strokes = Vec::with_capacity(blades as usize);
    for _ in 0..blades {
        let dx = rng.range(-16.0, 16.0);
        let tip = height * rng.range(0.7, 1.1);
        let role = if rng.chance(0.5) { role::MID } else { role::LIGHT };
        strokes.push(Stroke {
            points: vec![(0, 0), ((dx * 0.6) as i32, (tip * 0.6) as i32), ((dx * 1.3) as i32, tip as i32)],
            width: 1,
            role,
        });
    }
    (strokes, height as i32)
}

fn fern(rng: &mut Rng, scale: f64) -> (Vec<Stroke>, i32) {
    let height = rng.range(70.0, 150.0) * scale;
    let mut strokes = vec![Stroke {
        points: vec![(0, 0), (0, (height * 0.5) as i32), (rng.range(-6.0, 6.0) as i32, height as i32)],
        width: 1,
        role: role::DARK,
    }];
    let pairs = rng.int(3, 5);
    for i in 0..pairs {
        let y = height * (0.28 + 0.15 * i as f64);
        let len = rng.range(22.0, 42.0);
        let droop = rng.range(-6.0, 10.0);
        let side = if i % 2 == 0 { 1.0 } else { -1.0 };
        strokes.push(Stroke {
            points: vec![(0, y as i32), ((side * len) as i32, (y + droop) as i32)],
            width: 1,
            role: if i % 2 == 0 { role::MID } else { role::LIGHT },
        });
    }
    (strokes, height as i32)
}

fn vine(rng: &mut Rng, scale: f64) -> (Vec<Stroke>, i32) {
    let height = rng.range(130.0, 300.0) * scale;
    let amplitude = rng.range(8.0, 20.0);
    let waves = rng.range(0.75, 1.6);
    let phase = rng.unit();
    // Anchor the base at (0, 0) by removing the wave's value at t = 0.
    let base = sin_turns(phase);
    let xs = |t: f64| amplitude * (sin_turns(t * waves + phase) - base);
    let steps = 9;
    let mut points = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let t = i as f64 / steps as f64;
        points.push((xs(t) as i32, (t * height) as i32));
    }
    let mut strokes = vec![Stroke { points, width: 1, role: role::DARK }];
    let leaves = rng.int(3, 6);
    for _ in 0..leaves {
        let t = rng.range(0.2, 0.95);
        let y = (t * height) as i32;
        let x = xs(t) as i32;
        let len = rng.range(12.0, 26.0);
        let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
        strokes.push(Stroke {
            points: vec![(x, y), (x + (side * len) as i32, y + rng.range(-6.0, 6.0) as i32)],
            width: 1,
            role: if rng.chance(0.5) { role::MID } else { role::LIGHT },
        });
    }
    (strokes, height as i32)
}

fn fungus(rng: &mut Rng, scale: f64) -> (Vec<Stroke>, i32) {
    let height = rng.range(30.0, 64.0) * scale;
    let cap = rng.range(10.0, 22.0);
    let mut strokes = vec![Stroke {
        points: vec![(0, 0), (0, (height * 0.7) as i32), (0, height as i32)],
        width: 1,
        role: role::DARK,
    }];
    strokes.push(Stroke {
        points: vec![(-cap as i32, height as i32), (cap as i32, height as i32)],
        width: 3,
        role: role::ACCENT,
    });
    (strokes, height as i32)
}
