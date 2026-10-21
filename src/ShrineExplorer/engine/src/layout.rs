//! The ground plan of the four built futures.
//!
//! The wild biomes are earth and grass everywhere. The futures lay something
//! out around the shrine instead: a city block with roads and sidewalks, a
//! lab floor with light strips, parkland with curving paths and solar
//! tiles, a greenhouse with planting beds. Each is a pure function of the
//! seed and the point on the plateau, so the materials, the plants and the
//! props all agree on where things are without storing a map.
//!
//! The shrine's 16x16 footprint sits centred on the 24x24 plateau, leaving a
//! ring four cells deep around it; `ring` counts outwards through it (1 is
//! the cells touching the footprint, 4 the plateau's edge).

use crate::math::{cos_turns, sin_turns};
use crate::plants::Biome;
use crate::rng::Rng;
use crate::{GRID_H, GRID_W, SHRINE_H, SHRINE_W};

/// Stream id for the layout's few seeded choices.
const STREAM: u64 = 0x4c41_594f; // "LAYO"

/// Where the footprint starts on the plateau (the same for every shrine).
pub const ORIGIN: (i32, i32) = ((GRID_W as i32 - SHRINE_W as i32) / 2, (GRID_H as i32 - SHRINE_H as i32) / 2);

/// The four sides of the footprint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// Low x (the far left on screen).
    West,
    /// High x.
    East,
    /// Low y (the far right on screen).
    North,
    /// High y.
    South,
}

impl Side {
    const ALL: [Side; 4] = [Side::West, Side::East, Side::North, Side::South];

    /// Whether roads on this side run along the y axis.
    pub fn along_y(self) -> bool {
        matches!(self, Side::West | Side::East)
    }
}

/// What a point of the plateau is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ground {
    /// The wild biomes' earth and grass.
    Wild,
    /// Asphalt. `lane` marks the texel edge carrying the centre line,
    /// `crossing` a zebra crossing across it, `along_y` the direction it runs.
    Road { along_y: bool, lane: bool, crossing: bool },
    /// Where two roads meet: plain asphalt with stop lines.
    Junction,
    /// Paving slabs; `curb` is the side facing the road, if any.
    Sidewalk { curb: Option<Side> },
    /// Pale paving inside the shrine's footprint.
    Plaza,
    /// Mown grass (a city verge, or the park lawn).
    Lawn,
    /// A curving white path through the park.
    Path,
    /// A solar tile.
    Solar,
    /// Lab floor panels; `strip` carries a glowing light line.
    Floor { strip: bool },
    /// A planting bed of dark soil.
    Bed,
    /// Glazed terracotta tiles.
    Tile,
    /// Raked gravel.
    Gravel,
}

impl Ground {
    /// Whether plants may grow here.
    pub fn grows(self) -> bool {
        matches!(self, Ground::Wild | Ground::Lawn | Ground::Bed)
    }

    /// Hard, open floor where a prop can stand.
    pub fn standable(self) -> bool {
        matches!(self, Ground::Sidewalk { .. } | Ground::Plaza | Ground::Floor { .. } | Ground::Tile | Ground::Gravel | Ground::Lawn)
    }
}

/// The ring a cell sits in: 0 inside the footprint, 1..4 outward.
pub fn ring(gx: i32, gy: i32) -> i32 {
    let (ox, oy) = ORIGIN;
    let (ex, ey) = (ox + SHRINE_W as i32 - 1, oy + SHRINE_H as i32 - 1);
    (ox - gx).max(gx - ex).max(oy - gy).max(gy - ey).max(0)
}

/// Which sides of the footprint a cell lies beyond (two at a corner).
fn sides(gx: i32, gy: i32) -> (Option<Side>, Option<Side>) {
    let (ox, oy) = ORIGIN;
    let (ex, ey) = (ox + SHRINE_W as i32 - 1, oy + SHRINE_H as i32 - 1);
    let sx = if gx < ox { Some(Side::West) } else if gx > ex { Some(Side::East) } else { None };
    let sy = if gy < oy { Some(Side::North) } else if gy > ey { Some(Side::South) } else { None };
    (sx, sy)
}

/// The seed's layout choices for a biome.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plan {
    biome: Biome,
    /// A bit per side (in `Side::ALL` order): the city's roads, or the
    /// greenhouse's planting beds.
    sides: u8,
    /// Where along each side the zebra crossings are (a cell index 0..15).
    crossing: u8,
    /// The park's path: the radius of its loop and the angle of its spur.
    loop_r: f64,
    spur: f64,
    seed: u64,
}

impl Plan {
    pub fn new(seed: u64, biome: Biome) -> Self {
        let mut rng = Rng::stream(seed, STREAM);
        // Always at least two sides of road (or bed), one of them in front.
        let mut sides = rng.below(16) as u8 | 0b1000;
        if sides.count_ones() < 2 {
            sides |= 0b0010;
        }
        Plan {
            biome,
            sides,
            crossing: 4 + rng.below(8) as u8,
            loop_r: rng.range(9.6, 10.6),
            spur: rng.range(0.0, 1.0),
            seed,
        }
    }

    fn has(&self, side: Side) -> bool {
        let i = Side::ALL.iter().position(|s| *s == side).unwrap();
        self.sides >> i & 1 == 1
    }

    /// The ground of cell `(gx, gy)` (the cell's centre, for the per-cell
    /// layouts; the park's paths are worked out per point by [`Plan::at`]).
    pub fn cell(&self, gx: i32, gy: i32) -> Ground {
        self.at(gx as f64, gy as f64)
    }

    /// The ground at plateau point `(x, y)` (cell centres are whole numbers).
    pub fn at(&self, x: f64, y: f64) -> Ground {
        let (gx, gy) = (x.round() as i32, y.round() as i32);
        let r = ring(gx, gy);
        let (sx, sy) = sides(gx, gy);
        match self.biome {
            Biome::NearFuture => self.city(gx, gy, r, sx, sy),
            Biome::ResearchLab => {
                // A light strip round the footprint, and across the floor
                // every six cells.
                let strip = r == 1 || (r > 1 && (gx % 6 == 0 || gy % 6 == 0) && self.seed & 1 == 0);
                if r >= 3 && sx.is_some() && sy.is_some() {
                    // Planters in the corners.
                    Ground::Bed
                } else {
                    Ground::Floor { strip }
                }
            }
            Biome::Ecofuture => {
                if r == 0 {
                    return Ground::Lawn;
                }
                // A loop path round the shrine, gently wavering, and one spur
                // meandering out from it to the plateau's edge.
                let (cx, cy) = (GRID_W as f64 / 2.0 - 0.5, GRID_H as f64 / 2.0 - 0.5);
                let (dx, dy) = (x - cx, y - cy);
                let d = (dx * dx + dy * dy).sqrt();
                let wobble = 0.3 * sin_turns(self.spur + 0.09 * x) * cos_turns(0.07 * y - self.spur);
                if (d - self.loop_r - wobble).abs() < 0.42 {
                    return Ground::Path;
                }
                if d > self.loop_r {
                    // Out through the front: along +y or +x, by the seed.
                    let (along, across, centre) = if self.sides & 1 == 0 { (y, x, cx) } else { (x, y, cy) };
                    let meander = centre + 2.2 * sin_turns(0.06 * along + self.spur);
                    if along > centre && (across - meander).abs() < 0.42 {
                        return Ground::Path;
                    }
                }
                if r >= 3 && cell_hash(self.seed, gx, gy) % 5 == 0 {
                    return Ground::Solar;
                }
                Ground::Lawn
            }
            Biome::Greenhouse => {
                if r == 0 {
                    return Ground::Tile;
                }
                if r == 1 {
                    return Ground::Gravel;
                }
                // Beds on the seed's sides, two cells deep; tiles between.
                let bed = |s: Option<Side>| s.is_some_and(|s| self.has(s));
                if (2..=3).contains(&r) && (bed(sx) || bed(sy)) {
                    Ground::Bed
                } else if r == 4 && (gx + gy) % 5 == 0 {
                    Ground::Bed
                } else {
                    Ground::Tile
                }
            }
            _ => Ground::Wild,
        }
    }

    /// A city block: the shrine on its own plot, paved, sidewalks round it,
    /// roads on the seed's sides, grass verges on the rest.
    fn city(&self, gx: i32, gy: i32, r: i32, sx: Option<Side>, sy: Option<Side>) -> Ground {
        if r == 0 {
            return Ground::Plaza;
        }
        let road_x = sx.is_some_and(|s| self.has(s));
        let road_y = sy.is_some_and(|s| self.has(s));
        match r {
            1 => {
                // The sidewalk, its curb toward the road beyond it.
                let curb = match (sx, sy) {
                    (Some(s), None) if road_x => Some(s),
                    (None, Some(s)) if road_y => Some(s),
                    _ => None,
                };
                Ground::Sidewalk { curb }
            }
            2 | 3 => {
                if sx.is_some() && sy.is_some() {
                    // A corner: a junction if both roads come here.
                    return match (road_x, road_y) {
                        (true, true) => Ground::Junction,
                        (true, false) => Ground::Road { along_y: true, lane: r == 2, crossing: false },
                        (false, true) => Ground::Road { along_y: false, lane: r == 2, crossing: false },
                        _ => Ground::Lawn,
                    };
                }
                if let Some(side) = sx.or(sy) {
                    if self.has(side) {
                        // The crossing sits at one place along each side.
                        let along = if side.along_y() { gy - ORIGIN.1 } else { gx - ORIGIN.0 };
                        let crossing = along == self.crossing as i32 || along == self.crossing as i32 + 1;
                        return Ground::Road { along_y: side.along_y(), lane: r == 2, crossing };
                    }
                }
                Ground::Lawn
            }
            _ => {
                // The far sidewalk, along the plateau's edge.
                Ground::Sidewalk { curb: None }
            }
        }
    }
}

/// A small per-cell hash, for scattering things across the plan.
pub fn cell_hash(seed: u64, gx: i32, gy: i32) -> u64 {
    crate::rng::hash3(seed ^ STREAM, gx as u64 & 0xff, gy as u64 & 0xff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rings_count_out_from_the_footprint() {
        assert_eq!(ring(ORIGIN.0, ORIGIN.1), 0);
        assert_eq!(ring(ORIGIN.0 - 1, ORIGIN.1 + 3), 1);
        assert_eq!(ring(0, 0), 4);
        assert_eq!(ring(GRID_W as i32 - 1, 10), 4);
    }

    #[test]
    fn the_city_has_roads_and_the_wild_has_none() {
        for seed in 0..20 {
            let city = Plan::new(seed, Biome::NearFuture);
            let roads = (0..GRID_H as i32)
                .flat_map(|y| (0..GRID_W as i32).map(move |x| (x, y)))
                .filter(|&(x, y)| matches!(city.cell(x, y), Ground::Road { .. }))
                .count();
            assert!(roads >= 2 * 2 * 16, "seed {seed}: only {roads} road cells");
            let wild = Plan::new(seed, Biome::Moss);
            assert_eq!(wild.cell(1, 1), Ground::Wild);
        }
    }

    #[test]
    fn every_future_leaves_room_to_grow() {
        for biome in Biome::ALL.into_iter().filter(|b| b.futuristic()) {
            let plan = Plan::new(7, biome);
            let soft = (0..GRID_H as i32)
                .flat_map(|y| (0..GRID_W as i32).map(move |x| (x, y)))
                .filter(|&(x, y)| ring(x, y) > 0 && plan.cell(x, y).grows())
                .count();
            assert!(soft >= 4, "{biome:?}: nowhere for plants");
        }
    }
}
