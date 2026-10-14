//! Scene assembly: everything needed to draw one frame.
//!
//! A [`Scene`] owns the generated shrine, its plants and its weather, and turns
//! them into pixels in depth order. The *simulation state* it draws from —
//! [`SimState`] — is the same shared truth the music engine uses (see
//! [`crate::sim`]): weather lights and fogs the palette, sways the plants and
//! sets the rain, and the drone's slow arc brightens the altar. The 300x300
//! logical buffer never changes size (`HC003`); under load the [`Quality`] level
//! drops *effects*, not resolution.

use crate::arch::ArchTransform;
use crate::framebuffer::{blend, shade, Framebuffer, Rgba};
use crate::iso::{to_px_round, Iso, Point};
use crate::math::sin_turns;
use crate::palette::Palette;
use crate::plants::{role, Biome, Plant, Plants};
use crate::raster;
use crate::rng::Rng;
use crate::shrine::{kind, Shrine};
use crate::weather::{Weather, WeatherKind, WeatherState};
use crate::{GRID_H, GRID_W, LOGICAL_H, LOGICAL_W, SHRINE_H, SHRINE_W};

const RAIN_STREAM: u64 = 0x5241_494e; // "RAIN"
const MAX_RAIN: usize = 180;

/// How much of the scene to draw. The buffer size is fixed (`HC003`); this only
/// controls effect density.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Quality {
    High,
    Medium,
    Low,
}

impl Quality {
    /// Draw every `stride`-th plant on a cell.
    pub fn plant_stride(self) -> usize {
        match self {
            Quality::Low => 2,
            _ => 1,
        }
    }

    /// Fraction of the rain field to draw.
    pub fn rain_fraction(self) -> f64 {
        match self {
            Quality::High => 1.0,
            Quality::Medium => 0.5,
            Quality::Low => 0.0,
        }
    }
}

/// What the visuals read back from the shared simulation, in `[0, 1]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VisualCoupling {
    /// Foliage sway gain (driven by wind).
    pub foliage_sway: f64,
    /// Rain streak density.
    pub rain_density: f64,
    /// Altar glow (driven by the drone's slow arc).
    pub glow: f64,
}

/// The slow control signals the music is built from, in `[0, 1]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Instruments {
    pub drone: f64,
    pub loops: [f64; 4],
    /// A decaying accent each beat, for pulsing detail.
    pub pulse: f64,
}

/// A bounded camera nudge from the pointer, in `[-1, 1]`.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Camera {
    pub yaw: f64,
    pub pitch: f64,
}

/// The single source of truth for one instant, shared by audio and visuals.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SimState {
    pub time: f64,
    pub weather: WeatherState,
    pub coupling: VisualCoupling,
    pub instruments: Instruments,
    pub camera: Camera,
    /// Whether to fill the sky. The page clears this so its own sky (and the
    /// source waterfall behind the shrine) can show through.
    pub draw_sky: bool,
    /// How the architecture is turned and placed (`arch::ArchTransform`).
    pub arch: ArchTransform,
    /// Camera orbit, in turns: the whole scene turning about the shrine. Driven
    /// by the camera rotation speed and the clock.
    pub orbit_turns: f64,
}

impl SimState {
    /// A state derived only from weather (used by the plain preview, where the
    /// music's arcs are not in play).
    pub fn from_weather(time: f64, weather: WeatherState) -> Self {
        SimState {
            time,
            weather,
            coupling: VisualCoupling {
                foliage_sway: weather.wind_strength,
                rain_density: weather.rain,
                glow: 1.0,
            },
            instruments: Instruments { drone: 1.0, loops: [1.0; 4], pulse: 0.0 },
            camera: Camera::default(),
            draw_sky: true,
            arch: ArchTransform::IDENTITY,
            orbit_turns: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct RainStreak {
    x: i32,
    phase: f64,
    speed: f64,
    len: i32,
}

/// The whole piece at one seed and setting.
#[derive(Clone, Debug)]
pub struct Scene {
    pub seed: u64,
    pub biome: Biome,
    pub palette: Palette,
    pub shrine: Shrine,
    pub plants: Plants,
    pub weather: Weather,
    rain: Vec<RainStreak>,
}

impl Scene {
    pub fn new(seed: u64, biome: Biome, weather_kind: WeatherKind) -> Self {
        let shrine = Shrine::generate(seed);
        let plants = Plants::generate(seed, &shrine, biome);
        let weather = Weather::new(weather_kind, seed);

        let mut rng = Rng::stream(seed, RAIN_STREAM);
        let rain = (0..MAX_RAIN)
            .map(|_| RainStreak {
                x: rng.int(0, (LOGICAL_W - 1) as u64) as i32,
                phase: rng.unit(),
                speed: rng.range(140.0, 260.0),
                len: rng.int(4, 10) as i32,
            })
            .collect();

        Scene {
            seed,
            biome,
            palette: Palette::from_seed(seed),
            shrine,
            plants,
            weather,
            rain,
        }
    }

    /// The palette's sky gradient endpoints, for a page that draws its own sky.
    pub fn sky_colors(&self) -> (Rgba, Rgba) {
        (self.palette.sky_top, self.palette.sky_bottom)
    }

    /// Use an authored architecture instead of the generated one.
    ///
    /// A no-op for an empty model, so the generator stays authoritative by
    /// default and nothing about today's output moves (`HC004`). Plants are
    /// regenerated against the new structure, so they still grow on it.
    pub fn author(&mut self, model: &crate::shrine_model::ShrineModel) {
        if !model.is_authored() {
            return;
        }
        self.shrine = Shrine::from_model(self.seed, model);
        self.plants = Plants::generate(self.seed, &self.shrine, self.biome);
    }

    /// The weather at `t` seconds, from this scene's own weather.
    pub fn weather_state(&self, t: f64) -> WeatherState {
        self.weather.state(t)
    }

    /// Draw the scene at `t` seconds using only its weather (convenience for
    /// previews). The music's arcs are assumed full.
    pub fn render(&self, fb: &mut Framebuffer, t: f64, quality: Quality) {
        self.render_state(fb, &SimState::from_weather(t, self.weather.state(t)), quality);
    }

    /// Draw the scene from a shared [`SimState`]. This is the coupled path the
    /// artifact uses: audio and visuals evaluate the same state.
    ///
    /// Returns the two lit sky endpoints actually used, so a page that draws its
    /// own sky (to put the source waterfall behind the shrine) can match it
    /// exactly, weather lighting included.
    pub fn render_state(&self, fb: &mut Framebuffer, state: &SimState, quality: Quality) -> (Rgba, Rgba) {
        let w = state.weather;
        let mut iso = Iso::grid_24();
        iso.cam_yaw = state.camera.yaw;
        iso.cam_pitch = state.camera.pitch;

        // Sky gradient (drawn only when the scene owns the sky; the page may
        // draw it instead, so the source waterfall shows through behind the
        // shrine). The lit endpoints are returned either way.
        let sky_top = self.lit(self.palette.sky_top, &w);
        let sky_bottom = self.lit(self.palette.sky_bottom, &w);
        if state.draw_sky {
            let last = fb.height() as i32 - 1;
            for y in 0..=last {
                let f = y as f64 / last as f64;
                fb.fill_span(y, 0, fb.width() as i32 - 1, lerp(sky_top, sky_bottom, f));
            }
        } else {
            // The page draws its own sky, so nothing here covers that region:
            // start clear, or the previous frame's shrine lingers as a ghost.
            fb.clear([0, 0, 0, 0]);
        }

        // Camera orbit, if the shrine is being turned under the viewer. The
        // world vertical is untouched, so plants still stand up and only the
        // horizontal plane turns.
        let arch = state.arch;
        let orbit = state.orbit_turns;
        let general = !arch.is_unrotated() || orbit != 0.0;
        let (rot, off) = arch.world(orbit);
        let (ground_rot, ground_off) = ArchTransform::IDENTITY.world(orbit);

        // Ground plane. When the camera orbits, the cells are turned with it, so
        // they go through the general quad path; otherwise they keep the exact
        // axis-aligned diamonds.
        let (ox, oy) = self.shrine.origin();
        // A shrine with nothing descending renders exactly as it always has.
        let descents = self.shrine.has_depths();
        // Where a cell descends, the ground tile is the column's own cap.
        let dug = |gx: i32, gy: i32| -> u32 {
            let inside = gx >= ox && gx < ox + SHRINE_W as i32 && gy >= oy && gy < oy + SHRINE_H as i32;
            if inside {
                self.shrine.depth_at(gx - ox, gy - oy) as u32
            } else {
                0
            }
        };
        let ground = self.lit(self.palette.ground_dark, &w);
        // Cells in depth order: each column, then the plants growing on it.
        let mut buckets: Vec<Vec<usize>> = vec![Vec::new(); (GRID_W * GRID_H) as usize];
        for (i, p) in self.plants.items.iter().enumerate() {
            buckets[(p.gy * GRID_W as i32 + p.gx) as usize].push(i);
        }
        // Painter's key. A turned column can overlap its neighbours, so the
        // order has to follow the *rotated* position, not the grid's.
        let key = |gx: i32, gy: i32, height: u32| -> f64 {
            if !general {
                // Exactly the old `depth_key` order: by (gx + gy) only, with a
                // stable sort keeping the scan order for ties. Adding anything
                // else here would reorder columns within a diagonal.
                return (gx + gy) as f64;
            }
            let (x, y) = (gx as f64, gy as f64);
            let z = height.saturating_sub(1) as f64 / 2.0;
            let wx = rot[0][0] * x + rot[0][1] * y + rot[0][2] * z + off.0;
            let wy = rot[1][0] * x + rot[1][1] * y + rot[1][2] * z + off.1;
            let wz = rot[2][0] * x + rot[2][1] * y + rot[2][2] * z + off.2;
            (wx + wy) * 1024.0 + wz
        };
        let mut cells: Vec<(f64, i32, i32, u32, u32)> = Vec::new();
        for gy in 0..GRID_H as i32 {
            for gx in 0..GRID_W as i32 {
                let inside = gx >= ox && gx < ox + SHRINE_W as i32 && gy >= oy && gy < oy + SHRINE_H as i32;
                let height = if inside { self.shrine.height_at(gx - ox, gy - oy) as u32 } else { 0 };
                let depth = dug(gx, gy);
                let has_plants = !buckets[(gy * GRID_W as i32 + gx) as usize].is_empty();
                if height == 0 && depth == 0 && !has_plants {
                    continue;
                }
                cells.push((key(gx, gy, height), gx, gy, height, depth));
            }
        }
        cells.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(core::cmp::Ordering::Equal));

        // A whole-cell world offset is just a moved projection origin, so the
        // unrotated path below stays pixel-exact however far the shrine moves.
        let shrine_iso = {
            let shift = iso.offset_shift(arch.off_x as i32, arch.off_y as i32, arch.off_z as i32);
            Iso { origin_x: iso.origin_x + shift.x, origin_y: iso.origin_y + shift.y, ..iso }
        };

        // The altar glows with the drone's slow arc — the one instrument the
        // visuals always show.
        let glow = (state.coupling.glow.clamp(0.0, 1.0) * 55.0) as u32;
        let stride = quality.plant_stride();

        // Columns that descend below the ground are drawn *first*. The ground
        // plane — which is at z = 0 — is then painted over everything of theirs
        // that spills outside their own square, so they are visible only through
        // the square they occupy, which the ground pass leaves out.
        if descents {
            let mut below: Vec<(f64, i32, i32, u32)> =
                cells.iter().filter(|c| c.4 > 0).map(|c| (c.0, c.1, c.2, c.4)).collect();
            below.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(core::cmp::Ordering::Equal));
            for (_, gx, gy, depth) in below {
                let tile = self.shrine.layers.get(gx, gy, 0);
                let base = top_colour(tile.kind, &self.palette);
                let base = if tile.kind == kind::ALTAR { blend(base, [255, 255, 255, 255], glow, 100) } else { base };
                let top = self.lit(base, &w);
                let (r, o) = if general { (rot, off) } else { (ArchTransform::IDENTITY.rotation(), (0.0, 0.0, 0.0)) };
                raster::fill_column_down(fb, &iso, &r, o, gx, gy, depth, top, shade(top, 3, 5), shade(top, 2, 5));
            }
        }

        for gy in 0..GRID_H as i32 {
            for gx in 0..GRID_W as i32 {
                if dug(gx, gy) > 0 {
                    continue;
                }
                let colour = ground;
                if general {
                    raster::fill_box(fb, &iso, &ground_rot, ground_off, gx, gy, 1, colour, colour, colour);
                } else {
                    raster::fill_cell(fb, &iso, gx, gy, 0, colour);
                }
            }
        }

        for (_, gx, gy, height, _depth) in cells {
            // Descending columns were already drawn, before the ground plane.
            // Nothing may be drawn for them here, or the plane's own occlusion
            // is undone by the second pass.
            if height > 0 {
                let tile = self.shrine.layers.get(gx, gy, height as i32 - 1);
                let base = top_colour(tile.kind, &self.palette);
                let base = if tile.kind == kind::ALTAR { blend(base, [255, 255, 255, 255], glow, 100) } else { base };
                let top = self.lit(base, &w);
                if general {
                    raster::fill_box(fb, &iso, &rot, off, gx, gy, height, top, shade(top, 3, 5), shade(top, 2, 5));
                } else {
                    raster::fill_column(fb, &shrine_iso, gx, gy, height, top, shade(top, 3, 5), shade(top, 2, 5));
                }
            }
            for (n, &i) in buckets[(gy * GRID_W as i32 + gx) as usize].iter().enumerate() {
                if n % stride != 0 {
                    continue;
                }
                let plant = &self.plants.items[i];
                let base = if general {
                    iso.project_rotated(plant.gx as f64, plant.gy as f64, plant.z as f64, &ground_rot, ground_off)
                } else {
                    iso.project(plant.gx, plant.gy, plant.z)
                };
                self.draw_plant(fb, plant, state, base);
            }
        }

        // Rain.
        let visible = (self.rain.len() as f64 * state.coupling.rain_density * quality.rain_fraction()) as usize;
        if visible > 0 {
            let colour = self.lit(blend(self.palette.sky_bottom, self.palette.fog, 50, 100), &w);
            // Horizontal wind, -1 (left) .. 1 (right): the rain follows it.
            let wind_x = (0.5 - w.wind_dir) * 2.0 * w.wind_strength;
            let span = LOGICAL_H as f64 + 40.0;
            for s in &self.rain[..visible] {
                let y = ((s.phase * span + state.time * s.speed) % span) - 20.0;
                let x = (s.x as f64 + state.time * wind_x * 60.0).rem_euclid(LOGICAL_W as f64);
                let (x, y) = (x as i32, y as i32);
                // Tilt the streak along its fall direction so it reads as wind.
                let dx = (wind_x * s.len as f64 * 0.7).round() as i32;
                raster::line(fb, x, y, x + dx, y + s.len, colour);
            }
        }

        (sky_top, sky_bottom)
    }

    /// Weather lighting and fog, applied to one colour.
    fn lit(&self, c: Rgba, w: &WeatherState) -> Rgba {
        let brightness = (45.0 + 55.0 * w.light) as u32;
        let fog = (w.fog * 45.0) as u32;
        blend(shade(c, brightness, 100), self.palette.fog, fog, 100)
    }

    fn draw_plant(&self, fb: &mut Framebuffer, plant: &Plant, state: &SimState, base: Point) {
        let (bx, by) = (base.x + plant.offset.0, base.y + plant.offset.1);
        let w = state.weather;
        let colour = |r: u8| {
            let c = match r {
                role::DARK => self.palette.foliage_dark,
                role::MID => self.palette.foliage,
                role::LIGHT => self.palette.foliage_light,
                role::ACCENT => self.palette.accent,
                _ => self.palette.foliage,
            };
            self.lit(c, &w)
        };
        let amplitude = plant.sway.amplitude * (0.25 + 0.75 * state.coupling.foliage_sway);
        let sway = |y: i32| -> i32 {
            if plant.height <= 0 {
                return 0;
            }
            let f = y as f64 / plant.height as f64;
            (amplitude * f * sin_turns(state.time + plant.sway.phase + w.wind_dir)).round() as i32
        };
        for stroke in &plant.strokes {
            let mut prev: Option<(i32, i32)> = None;
            for &(x, y) in &stroke.points {
                let px = to_px_round(bx + x + sway(y));
                let py = to_px_round(by - y);
                if let Some((ox, oy)) = prev {
                    let width = stroke.width.max(1) as i32;
                    for k in 0..width {
                        let off = k - width / 2;
                        raster::line(fb, ox, oy + off, px, py + off, colour(stroke.role));
                    }
                }
                prev = Some((px, py));
            }
        }
    }
}

fn top_colour(kind: u8, palette: &Palette) -> Rgba {
    match kind {
        kind::FLOOR => palette.ground,
        kind::WALL => palette.stone,
        kind::PILLAR => palette.stone_light,
        kind::ROOF => palette.stone_dark,
        kind::RUBBLE => palette.ground_dark,
        kind::ALTAR => palette.accent,
        kind::STONE => palette.stone_dark,
        _ => palette.ground,
    }
}

fn lerp(a: Rgba, b: Rgba, t: f64) -> Rgba {
    let f = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * t).round() as u8;
    [f(a[0], b[0]), f(a[1], b[1]), f(a[2], b[2]), 255]
}
