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
use crate::iso::{to_px_round, Iso, Point, FP};
use crate::math::sin_turns;
use crate::palette::Palette;
use crate::plants::{role, Biome, Plant, Plants};
use crate::raster;
use crate::shrine::{kind, Shrine};
use crate::weather::{EventKind, Weather, WeatherKind, WeatherState};
use crate::light::{self, Rig};
use crate::material::{self, Exposure, Materials};
use crate::weather_fx;
use crate::wildlife::Wildlife;
use crate::{GRID_H, GRID_W, SHRINE_H, SHRINE_W};


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

    /// Fraction of the weather's other particles to draw.
    pub fn particle_fraction(self) -> f64 {
        match self {
            Quality::High => 1.0,
            Quality::Medium => 0.6,
            Quality::Low => 0.3,
        }
    }

    /// Fraction of the rain field to draw.
    pub fn rain_fraction(self) -> f64 {
        match self {
            Quality::High => 1.0,
            Quality::Medium => 0.5,
            Quality::Low => 0.25,
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

/// The whole piece at one seed and setting.
#[derive(Clone, Debug)]
pub struct Scene {
    pub seed: u64,
    pub biome: Biome,
    pub palette: Palette,
    pub shrine: Shrine,
    pub plants: Plants,
    pub weather: Weather,
    /// The creatures that can be seen, when the Wildlife setting is on.
    pub wildlife: Option<Wildlife>,
    /// The built futures' street furniture and machines (none in the wild).
    pub props: crate::props::Props,
    /// The indoor futures' roof.
    pub roof: Option<crate::roof::Roof>,
}

impl Scene {
    pub fn new(seed: u64, biome: Biome, weather_kind: WeatherKind) -> Self {
        let shrine = Shrine::generate(seed);
        let plants = Plants::generate(seed, &shrine, biome);
        let weather = Weather::new(weather_kind, seed);
        let roof = crate::roof::Roof::new(biome, tallest(&shrine));

        Scene {
            seed,
            biome,
            palette: Palette::for_biome(seed, biome),
            shrine,
            plants,
            weather,
            wildlife: None,
            props: crate::props::Props::generate(seed, biome),
            roof,
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
        self.roof = crate::roof::Roof::new(self.biome, tallest(&self.shrine));
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
        let (sky_top, sky_bottom) = self.sky_colors_at(&w);
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

        // Everything the weather draws, and the grade it lights the scene with.
        let lit = |c: Rgba| self.lit(c, &w);
        let fx = weather_fx::Fx {
            seed: self.seed,
            t: state.time,
            w,
            weather: &self.weather,
            palette: &self.palette,
            fraction: quality.particle_fraction(),
            rain_fraction: quality.rain_fraction(),
            grade: &lit,
        };
        // The far layer sits behind the shrine: clouds, shafts of light, the
        // bolt, fog banks and the distant rain and snow.
        fx.far(fb);

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
        // How far a cell descends below the ground (0 for most).
        let depth_at = |gx: i32, gy: i32| -> u32 {
            let inside = gx >= ox && gx < ox + SHRINE_W as i32 && gy >= oy && gy < oy + SHRINE_H as i32;
            if inside {
                self.shrine.depth_at(gx - ox, gy - oy) as u32
            } else {
                0
            }
        };
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
                let depth = depth_at(gx, gy);
                let has_plants = !buckets[(gy * GRID_W as i32 + gx) as usize].is_empty() || self.props.any_in(gx, gy);
                if height == 0 && depth == 0 && !has_plants {
                    continue;
                }
                cells.push((key(gx, gy, height), gx, gy, height, depth));
            }
        }
        cells.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(core::cmp::Ordering::Equal));

        // A whole-cell world offset is just a moved projection origin, so the
        // unrotated path below stays pixel-exact however far the shrine moves.
        // Only the horizontal part goes into the origin: the vertical part is
        // added to the column's levels, so it can be split at the ground plane.
        let shrine_iso = {
            let shift = iso.offset_shift(arch.off_x as i32, arch.off_y as i32, 0);
            Iso { origin_x: iso.origin_x + shift.x, origin_y: iso.origin_y + shift.y, ..iso }
        };
        let lift = arch.off_z as i32;

        let stride = quality.plant_stride();

        // The light for this frame: the weather's sun and sky light, shadows
        // cast from the shrine's heights in scene space (model plus offset,
        // before the orbit), and the altar glowing with the drone's arc.
        let (arch_rot, arch_off) = arch.world(0.0);
        let to_scene = |x: f64, y: f64, z: f64| -> [f64; 3] {
            if arch.is_unrotated() {
                [x + arch_off.0, y + arch_off.1, z + arch_off.2]
            } else {
                let p = crate::iso::to_world(x, y, z, &arch_rot, arch_off);
                [p.0, p.1, p.2]
            }
        };
        let mut tops = vec![0.0f64; (GRID_W * GRID_H) as usize];
        let mut altar: Option<([f64; 3], f64)> = None;
        for &(_, gx, gy, height, _) in &cells {
            if height == 0 {
                continue;
            }
            let top = height as f64 - 1.0;
            let c = to_scene(gx as f64, gy as f64, top);
            let (sx, sy) = (c[0].round() as i32, c[1].round() as i32);
            if sx >= 0 && sy >= 0 && sx < GRID_W as i32 && sy < GRID_H as i32 {
                let t = &mut tops[(sy * GRID_W as i32 + sx) as usize];
                *t = t.max(c[2].max(0.0));
            }
            // An altar sunk below the plateau is hidden by it, light and all.
            if self.shrine.layers.get(gx, gy, height as i32 - 1).kind == kind::ALTAR && c[2] >= 0.0 {
                let a = altar.get_or_insert(([0.0; 3], 0.0));
                for k in 0..3 {
                    a.0[k] += c[k];
                }
                a.1 += 1.0;
            }
        }
        let materials = Materials::new(self.seed, &self.palette, self.biome);
        let rig = Rig::new(light::Setup {
            kind: self.weather.kind,
            biome: self.biome,
            tops,
            cast: arch.is_unrotated(),
            orbit,
            glow_at: altar.map(|(sum, n)| [sum[0] / n, sum[1] / n, sum[2] / n + 0.3]),
            glow: state.coupling.glow.clamp(0.0, 1.0),
            // The altar's light is the green-blue of its ornament (a
            // future's own light colour in the built biomes).
            accent: materials.glow(),
            fog: self.palette.fog,
            fog_level: w.fog,
            flash: w.flash,
            t: state.time,
            wind: w.wind_strength,
        });
        let grade = Grade::new(self, &w);
        // Under a roof nothing settles on the ground or the architecture.
        let exposure = if self.biome.roofed() { Exposure { cover: 0.0, rain: 0.0 } } else { Exposure { cover: w.cover, rain: w.rain } };
        let neon = self.biome.futuristic();
        // A face's normal in scene space: turned with the architecture.
        let normal = |face: raster::Face| -> [f64; 3] {
            let n = face.normal();
            if arch.is_unrotated() {
                return n;
            }
            [0, 1, 2].map(|i| arch_rot[i][0] * n[0] + arch_rot[i][1] * n[1] + arch_rot[i][2] * n[2])
        };

        // A cell's block: the model levels it spans and what it is made of. A
        // raised column runs from the ground up to its top tile; a descending
        // one from its depth up to its cap, which sits at ground level.
        let block = |gx: i32, gy: i32, height: u32, depth: u32| -> Option<((i32, i32), material::Block)> {
            let (range, layer) = if height > 0 {
                ((0, height as i32 - 1), height as i32 - 1)
            } else if depth > 0 {
                ((-(depth as i32), 0), 0)
            } else {
                return None;
            };
            let tile = self.shrine.layers.get(gx, gy, layer);
            Some((range, material::Block { kind: tile.kind, variant: tile.variant, gx, gy, top: range.1 as f64 }))
        };
        // The textured, lit colour of one pixel of a block or of the ground.
        let shade_block = |b: &material::Block, s: &raster::Surface| -> Rgba {
            let alb = materials.albedo(b, s, exposure);
            let c = if b.kind == kind::EMPTY {
                grade.apply(rig.shade(s, [s.x, s.y, 0.0], [0.0, 0.0, 1.0], &alb))
            } else {
                grade.apply(rig.shade(s, to_scene(s.x, s.y, s.z), normal(s.face), &alb))
            };
            if neon && alb.emissive > 0.0 {
                // A future's lights are lit from within: the weather's grade
                // can dim the scene around them, never the lights themselves.
                let e = (0.55 + 0.4 * alb.emissive).min(0.95);
                return [0, 1, 2, 3].map(|k| if k == 3 { 255 } else { c[k].max((alb.colour[k] as f64 * e) as u8) });
            }
            c
        };

        // Shading is the costly part, and in painter's order much of what is
        // drawn is later drawn over. So the geometry goes through twice: first
        // only to find which draw ends up owning each pixel, then for real,
        // shading a pixel only in the draw that keeps it. The second pass runs
        // the same draws in the same order, with the effects and plants in
        // between, so the picture is exactly what painting everything would
        // give.
        let bounds = raster::Bounds::of(fb);
        let mut owner = vec![u32::MAX; (bounds.w * bounds.h) as usize];
        // On the turned path one sort key per column cannot order every
        // overlap correctly (tall and short, tilted blocks), so there the
        // first pass keeps the *nearest* surface at each pixel, by true depth.
        // Upright, the painter's order is exact and is kept, pixel for pixel.
        let mut nearest = if general { vec![f64::NEG_INFINITY; owner.len()] } else { Vec::new() };
        let at = |s: &raster::Surface| (s.py * bounds.w + s.px) as usize;
        // Draw one half of a block, either side of the ground plane (z = 0),
        // in either pass.
        let draw = |pass: &mut Pass, id: u32, b: &material::Block, (lo, hi): (i32, i32), half: raster::Half| {
            let mut sink = |s: &raster::Surface| match pass {
                Pass::Find(o, near) => {
                    let i = at(s);
                    if near.is_empty() {
                        o[i] = id;
                    } else if s.depth >= near[i] - DEPTH_TIE {
                        // On a tie (a shared edge, coplanar tiles) the later
                        // draw wins, as the painter's order would have it.
                        near[i] = s.depth.max(near[i]);
                        o[i] = id;
                    }
                }
                Pass::Paint(fb, o) => {
                    if o[at(s)] == id {
                        fb.put(s.px, s.py, shade_block(b, s));
                    }
                }
            };
            let (gx, gy) = (b.gx, b.gy);
            if b.kind == kind::EMPTY {
                // The plateau: one flat cell, turned only by the camera orbit.
                if general {
                    raster::fill_box_shaded(bounds, &iso, &ground_rot, ground_off, gx, gy, (0.0, 0.0), raster::Half::All, &mut sink);
                } else {
                    let c = iso.project(gx, gy, 0);
                    let centre = (gx as f64, gy as f64, 0.0);
                    raster::fill_diamond_shaded(bounds, c.x, c.y, iso.half_w(), iso.half_h(), centre, raster::Face::Top, &mut sink);
                }
                return;
            }
            if general {
                let range = (lo as f64, hi as f64);
                raster::fill_box_shaded(bounds, &iso, &rot, off, gx, gy, range, half, &mut sink);
                return;
            }
            let (lo, hi) = (lo + lift, hi + lift);
            let (lo, hi) = match half {
                raster::Half::All => (lo, hi),
                // A cap exactly at ground level belongs to the upper half.
                raster::Half::Above => (lo.max(0), hi),
                raster::Half::Below if lo < 0 => (lo, hi.min(0)),
                raster::Half::Below => return,
            };
            raster::fill_column_range_shaded(bounds, &shrine_iso, gx, gy, lo, hi, lift, &mut sink);
        };

        // The plateau, whole: no cell is left out of it, so nothing can show
        // through a hole in the ground. It is earth and grass, lit like
        // everything else, so it takes the shrine's cast shadows and the
        // passing cloud shadows.
        let plateau: Vec<material::Block> = (0..GRID_H as i32)
            .flat_map(|gy| (0..GRID_W as i32).map(move |gx| material::Block { kind: kind::EMPTY, variant: 0, gx, gy, top: 0.0 }))
            .collect();
        let below: Vec<((i32, i32), material::Block)> =
            cells.iter().filter_map(|&(_, gx, gy, height, depth)| block(gx, gy, height, depth)).collect();

        // First pass: who owns each pixel. Everything below the ground plane
        // comes first, then the plateau over it (hiding it wherever the
        // plateau is, and still showing it past the plateau's edge, which has
        // no thickness), then the architecture above the ground.
        let mut id = 0u32;
        {
            let mut find = Pass::Find(&mut owner, &mut nearest);
            for (range, b) in &below {
                id += 1;
                draw(&mut find, id, b, *range, raster::Half::Below);
            }
            for g in &plateau {
                id += 1;
                draw(&mut find, id, g, (0, 0), raster::Half::All);
            }
            for (range, b) in &below {
                id += 1;
                draw(&mut find, id, b, *range, raster::Half::Above);
            }
        }

        // Second pass, for real: the same draws, in the same order.
        let mut id = 0u32;
        for (range, b) in &below {
            id += 1;
            draw(&mut Pass::Paint(fb, &owner), id, b, *range, raster::Half::Below);
        }
        for g in &plateau {
            id += 1;
            draw(&mut Pass::Paint(fb, &owner), id, g, (0, 0), raster::Half::All);
        }

        // A point of the plateau's space at height `z`, on screen: where the
        // props stand and the roof is built.
        let proj3 = |x: f64, y: f64, z: f64| -> Point {
            if general {
                iso.project_rotated(x, y, z, &ground_rot, ground_off)
            } else {
                iso.project_world(x, y, z)
            }
        };
        // The indoor futures' back walls stand behind everything else.
        let roof_cx = self.roof.map(|_| crate::roof::Ctx {
            proj: &proj3,
            lit: &lit,
            seed: self.seed,
            t: state.time,
            w,
            accent: materials.glow(),
            glass: blend(self.palette.sky_bottom, [214, 236, 224, 255], 60, 100),
        });
        if let (Some(r), Some(c)) = (&self.roof, &roof_cx) {
            r.back(fb, c);
        }
        let night = ((1.0 - w.light)
            + if self.weather.kind == WeatherKind::Dusk { 0.5 } else { 0.0 }
            + if self.biome == Biome::ResearchLab { 0.3 } else { 0.0 })
        .clamp(0.0, 1.0);
        let foliage = [self.palette.foliage_dark, self.palette.foliage, self.palette.foliage_light];

        // Where a point of the world lands on screen: on the plateau, or on the
        // architecture at model level `z` (which moves with the transform).
        let on_ground = |x: f64, y: f64| -> Point {
            if general {
                iso.project_rotated(x, y, 0.0, &ground_rot, ground_off)
            } else {
                iso.project_world(x, y, 0.0)
            }
        };
        let on_shrine = |x: f64, y: f64, z: f64| -> Point {
            if general {
                iso.project_rotated(x, y, z, &rot, off)
            } else {
                iso.project_world(x + arch.off_x as f64, y + arch.off_y as f64, z + arch.off_z as f64)
            }
        };
        // Whether a point of the architecture is above the ground plane: a
        // column sunk below it has no top for rain to land on or drip from.
        let above_ground = |x: f64, y: f64, z: f64| -> bool {
            let wz = if general { crate::iso::to_world(x, y, z, &rot, off).2 } else { z + arch.off_z as f64 };
            wz >= 0.0
        };
        // Splashes and drips, each found from the shared schedule so the weather
        // bed plays the very same ones. Those on a column are drawn with it, so
        // nearer columns hide them; those on the ground go on the plateau now.
        let water = self.lit(blend(self.palette.sky_bottom, [240, 245, 255, 255], 85, 100), &w);
        let mut on_cell: Vec<Vec<(Point, f64, f64, bool)>> = vec![Vec::new(); (GRID_W * GRID_H) as usize];
        // Under a roof the rain lands on the glass, not the ground.
        let mut on_roof: Vec<(f64, f64, f64)> = Vec::new();
        for (e, age) in weather_fx::live(&self.weather, EventKind::Splash, state.time, weather_fx::SPLASH_S) {
            let (x, y) = e.spot();
            if self.roof.is_some() {
                on_roof.push((x, y, age));
                continue;
            }
            match self.column_at(x, y).filter(|c| above_ground(x, y, c.2 as f64 - 1.0)) {
                Some((gx, gy, h)) => {
                    let at = on_shrine(x, y, h as f64 - 1.0);
                    on_cell[(gy * GRID_W as i32 + gx) as usize].push((at, age, -1.0, true));
                }
                None => weather_fx::splash(fb, on_ground(x, y), age, false, water),
            }
        }
        let columns = self.columns();
        if !columns.is_empty() && self.roof.is_none() {
            for (e, age) in weather_fx::live(&self.weather, EventKind::Drip, state.time, weather_fx::DRIP_S) {
                let (gx, gy, h) = columns[(e.hash % columns.len() as u64) as usize];
                if !above_ground(gx as f64 + 0.5, gy as f64 + 0.5, h as f64 - 1.0) {
                    continue;
                }
                // From the front corner of the column's top, down its face.
                let at = on_shrine(gx as f64 + 0.5, gy as f64 + 0.5, h as f64 - 1.0);
                let fall = (h as f64 - 1.0) * iso.level_h as f64 / FP as f64;
                on_cell[(gy * GRID_W as i32 + gx) as usize].push((at, age, fall, false));
            }
        }

        for (_, gx, gy, height, depth) in cells {
            // The upper half of each block, over the plateau. For a descending
            // column that is just its cap, flush with the ground.
            if let Some((range, b)) = block(gx, gy, height, depth) {
                id += 1;
                draw(&mut Pass::Paint(fb, &owner), id, &b, range, raster::Half::Above);
            }
            for &(at, age, fall, splash) in &on_cell[(gy * GRID_W as i32 + gx) as usize] {
                if splash {
                    weather_fx::splash(fb, at, age, true, water);
                } else {
                    weather_fx::drip(fb, at, fall, age, water);
                }
            }
            for i in self.props.in_cell(gx, gy) {
                let p = &self.props.items[i];
                let base = proj3(p.x, p.y, 0.0);
                let light = rig.on_plant(p.x, p.y, to_px_round(base.x), to_px_round(base.y));
                let pcx = crate::props::Ctx { proj: &proj3, lit: &lit, t: state.time, wind: w.wind_strength, night, light, roof: self.roof.as_ref() };
                crate::props::draw(fb, &self.props, i, &pcx, self.palette.accent, foliage);
                // A wire is strung when the nearer of its two poles is drawn.
                for &(a, b) in &self.props.wires {
                    let other = if a == i { b } else if b == i { a } else { continue };
                    let o = &self.props.items[other];
                    if (o.gx + o.gy, other) < (p.gx + p.gy, i) {
                        crate::props::wire(fb, &self.props.items[a], &self.props.items[b], &pcx);
                    }
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
                let light = rig.on_plant(plant.gx as f64, plant.gy as f64, to_px_round(base.x), to_px_round(base.y));
                self.draw_plant(fb, plant, state, base, light);
            }
        }

        // A soft, dithered halo of the altar's light over everything near it.
        if let Some((g, k)) = rig.glow() {
            let at = if general { iso.project_rotated(g[0], g[1], g[2], &ground_rot, ground_off) } else { iso.project_world(g[0], g[1], g[2]) };
            let (cx, cy) = (to_px_round(at.x), to_px_round(at.y));
            let halo = blend(self.lit(materials.glow(), &w), [255, 250, 235, 255], 30, 100);
            let r = 11;
            for y in cy - r..=cy + r {
                for x in cx - r..=cx + r {
                    let d = (((x - cx) * (x - cx) + (y - cy) * (y - cy)) as f64).sqrt() / r as f64;
                    if d >= 1.0 {
                        continue;
                    }
                    let a = light::dither(k * (1.0 - d) * (1.0 - d) * 0.55, 4.0, light::bayer(x, y));
                    fb.blend_over(x, y, halo, (a * 85.0) as u32);
                }
            }
        }

        // The roof, over the shrine, with the weather landing on it.
        if let (Some(r), Some(c)) = (&self.roof, &roof_cx) {
            r.draw(fb, c, &on_roof, water);
        }

        // The near layer, over everything: the close rain, snow, leaves,
        // motes, embers and sparks, then the low mist.
        fx.near(fb, &on_ground);
        if let Some(wildlife) = &self.wildlife {
            wildlife.draw(fb, self.weather.kind, state.time, &w, &lit);
        }

        (sky_top, sky_bottom)
    }

    /// Weather lighting, fog, the kind's colour grade and any lightning flash,
    /// applied to one colour. Everything drawn goes through this, the sky
    /// included, so the page's own sky matches.
    fn lit(&self, c: Rgba, w: &WeatherState) -> Rgba {
        self.lit_with(&Profiles::of(self.weather.kind, self.biome), c, w)
    }

    /// [`Scene::lit`] with the weather's grade worked out once (it is asked
    /// for every textured pixel).
    fn lit_with(&self, g: &Profiles, c: Rgba, w: &WeatherState) -> Rgba {
        let p = &g.weather;
        let brightness = (45.0 + 55.0 * self.light_level(w)) as u32;
        let fog = (w.fog * 45.0) as u32;
        // The fog is lit by the same light: in a storm it is grey, not glowing.
        let c = blend(shade(c, brightness, 100), shade(self.palette.fog, 25 + brightness * 3 / 4, 100), fog, 100);
        // The grade shifts the hue toward the tint without lifting a dark
        // palette: each channel is scaled by the tint's own balance.
        let mean = (p.tint[0] as u32 + p.tint[1] as u32 + p.tint[2] as u32).max(1);
        let tone = |k: usize| {
            let f = 100 * 3 * p.tint[k] as u32 / mean; // 100 = unchanged
            let scale = 100 * (100 - p.grade) + f * p.grade; // in 10000ths
            ((c[k] as u32 * scale) / 10_000).min(255) as u8
        };
        let c = [tone(0), tone(1), tone(2), c[3]];
        // The weather's vividness: saturation about the grey, then contrast
        // about the middle, so each weather holds together as one palette.
        let lp = &g.light;
        let mean = (c[0] as f64 + c[1] as f64 + c[2] as f64) / 3.0;
        let grade = |v: u8| {
            let v = mean + (v as f64 - mean) * lp.saturation;
            (128.0 + (v - 128.0) * lp.contrast).clamp(0.0, 255.0) as u8
        };
        let c = [grade(c[0]), grade(c[1]), grade(c[2]), c[3]];
        blend(c, [235, 240, 255, 255], (w.flash * 65.0) as u32, 100)
    }

    /// How much light reaches the scene, 0-1: the weather's own, except under
    /// a roof, where the lab's ceiling lights keep it bright and the
    /// greenhouse's glass loses a little less of it.
    fn light_level(&self, w: &WeatherState) -> f64 {
        match self.biome {
            Biome::ResearchLab => 0.6 + 0.4 * w.light,
            Biome::Greenhouse => 0.2 + 0.8 * w.light,
            _ => w.light,
        }
    }

    /// The two lit sky endpoints for a weather state. Dusk lowers a warm sun
    /// into the bottom of the sky and deepens the top.
    pub fn sky_colors_at(&self, w: &WeatherState) -> (Rgba, Rgba) {
        let (mut top, mut bottom) = (self.palette.sky_top, self.palette.sky_bottom);
        if self.weather.kind == WeatherKind::Dusk {
            top = blend(top, [70, 46, 108, 255], 45, 100);
            bottom = blend(bottom, [255, 150, 92, 255], 58, 100);
        }
        // Cloud cover darkens the sky, the top most.
        let top = shade(top, 100 - (w.cloud * 40.0) as u32, 100);
        let bottom = shade(bottom, 100 - (w.cloud * 22.0) as u32, 100);
        (self.lit(top, w), self.lit(bottom, w))
    }

    /// The column standing at grid point `(x, y)`, if any: its cell and height.
    /// Where the top surface of grid cell `(gx, gy)` is on screen in
    /// `state`, in logical pixels, and its level: the top of the shrine's
    /// column there, or the ground. The same placement `render_state` gives a
    /// rain splash, through the same camera, orbit and architecture
    /// transform, so anything put there (the page's @ orbs) stays on it.
    /// Off the grid: `None`.
    pub fn surface_point(&self, state: &SimState, gx: i32, gy: i32) -> Option<(f64, f64, f64)> {
        if gx < 0 || gy < 0 || gx >= GRID_W as i32 || gy >= GRID_H as i32 {
            return None;
        }
        let mut iso = Iso::grid_24();
        iso.cam_yaw = state.camera.yaw;
        iso.cam_pitch = state.camera.pitch;
        let arch = state.arch;
        let orbit = state.orbit_turns;
        let general = !arch.is_unrotated() || orbit != 0.0;
        let (rot, off) = arch.world(orbit);
        let (ground_rot, ground_off) = ArchTransform::IDENTITY.world(orbit);
        let (x, y) = (gx as f64, gy as f64);
        let above_ground = |z: f64| -> bool {
            let wz = if general { crate::iso::to_world(x, y, z, &rot, off).2 } else { z + arch.off_z as f64 };
            wz >= 0.0
        };
        let (at, level) = match self.column_at(x, y).filter(|c| above_ground(c.2 as f64 - 1.0)) {
            Some((_, _, h)) => {
                let z = h as f64 - 1.0;
                let p = if general {
                    iso.project_rotated(x, y, z, &rot, off)
                } else {
                    iso.project_world(x + arch.off_x as f64, y + arch.off_y as f64, z + arch.off_z as f64)
                };
                (p, h as f64)
            }
            None => {
                let p = if general { iso.project_rotated(x, y, 0.0, &ground_rot, ground_off) } else { iso.project_world(x, y, 0.0) };
                (p, 0.0)
            }
        };
        Some((at.x as f64 / FP as f64, at.y as f64 / FP as f64, level))
    }

    pub fn column_at(&self, x: f64, y: f64) -> Option<(i32, i32, u32)> {
        let (gx, gy) = (x.round() as i32, y.round() as i32);
        let (ox, oy) = self.shrine.origin();
        let inside = gx >= ox && gx < ox + SHRINE_W as i32 && gy >= oy && gy < oy + SHRINE_H as i32;
        if !inside {
            return None;
        }
        let h = self.shrine.height_at(gx - ox, gy - oy) as u32;
        (h > 0).then_some((gx, gy, h))
    }

    /// Every column tall enough to drip from its edge, in scan order.
    pub fn columns(&self) -> Vec<(i32, i32, u32)> {
        let mut out = Vec::new();
        for gy in 0..GRID_H as i32 {
            for gx in 0..GRID_W as i32 {
                if let Some(c) = self.column_at(gx as f64, gy as f64) {
                    if c.2 >= 2 {
                        out.push(c);
                    }
                }
            }
        }
        out
    }

    fn draw_plant(&self, fb: &mut Framebuffer, plant: &Plant, state: &SimState, base: Point, light: f64) {
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
            shade(self.lit(c, &w), (light * 100.0) as u32, 100)
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

/// The tallest column of a shrine, in levels.
fn tallest(shrine: &Shrine) -> f64 {
    let mut h = 0u8;
    for ly in 0..SHRINE_H as i32 {
        for lx in 0..SHRINE_W as i32 {
            h = h.max(shrine.height_at(lx, ly));
        }
    }
    h as f64
}

fn lerp(a: Rgba, b: Rgba, t: f64) -> Rgba {
    let f = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * t).round() as u8;
    [f(a[0], b[0]), f(a[1], b[1]), f(a[2], b[2]), 255]
}

/// A weather's two profiles: its tint and its light's vividness.
struct Profiles {
    weather: crate::weather::Profile,
    light: light::LightProfile,
}

impl Profiles {
    fn of(kind: WeatherKind, biome: Biome) -> Self {
        Profiles { weather: kind.profile(), light: light::profile_in(kind, biome) }
    }
}

/// [`Scene::lit`] for a whole frame, as one affine map of RGB. Every step of
/// the grade — the light level, the lit fog, the tint, the saturation, the
/// contrast and a lightning flash — is linear in the colour, so the chain
/// folds into one 3x3 matrix and an offset, worked out once and applied to
/// every textured pixel in nine multiply-adds.
struct Grade {
    m: [[f64; 3]; 3],
    b: [f64; 3],
}

impl Grade {
    fn new(scene: &Scene, w: &WeatherState) -> Self {
        let p = Profiles::of(scene.weather.kind, scene.biome);
        let mut m = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let mut b = [0.0; 3];
        // out = a * out + c, per channel.
        let diag = |m: &mut [[f64; 3]; 3], b: &mut [f64; 3], a: [f64; 3], c: [f64; 3]| {
            for k in 0..3 {
                for j in 0..3 {
                    m[k][j] *= a[k];
                }
                b[k] = b[k] * a[k] + c[k];
            }
        };
        let brightness = (45.0 + 55.0 * scene.light_level(w)).floor();
        diag(&mut m, &mut b, [brightness / 100.0; 3], [0.0; 3]);
        let fog = (w.fog * 45.0).floor() / 100.0;
        let lit_fog = (25.0 + brightness * 0.75) / 100.0;
        let f = scene.palette.fog;
        diag(&mut m, &mut b, [1.0 - fog; 3], [0, 1, 2].map(|k| f[k] as f64 * lit_fog * fog));
        let pw = &p.weather;
        let mean = (pw.tint[0] as f64 + pw.tint[1] as f64 + pw.tint[2] as f64).max(1.0);
        let g = pw.grade as f64 / 100.0;
        diag(&mut m, &mut b, [0, 1, 2].map(|k| (1.0 - g) + 3.0 * pw.tint[k] as f64 / mean * g), [0.0; 3]);
        // Saturation: out = mean + (out - mean) * s, mixing the channels.
        let sat = p.light.saturation;
        let mix = (1.0 - sat) / 3.0;
        let (m0, b0) = (m, b);
        for k in 0..3 {
            for j in 0..3 {
                m[k][j] = sat * m0[k][j] + mix * (m0[0][j] + m0[1][j] + m0[2][j]);
            }
            b[k] = sat * b0[k] + mix * (b0[0] + b0[1] + b0[2]);
        }
        let con = p.light.contrast;
        diag(&mut m, &mut b, [con; 3], [128.0 * (1.0 - con); 3]);
        let q = (w.flash * 65.0).floor() / 100.0;
        diag(&mut m, &mut b, [1.0 - q; 3], [235.0 * q, 240.0 * q, 255.0 * q]);
        Grade { m, b }
    }

    #[inline]
    fn apply(&self, c: [f64; 3]) -> Rgba {
        let k = |r: usize| (self.m[r][0] * c[0] + self.m[r][1] * c[1] + self.m[r][2] * c[2] + self.b[r]).clamp(0.0, 255.0) as u8;
        [k(0), k(1), k(2), 255]
    }
}

/// One of the two passes over the geometry (see `render_state`): finding
/// which draw owns each pixel, then painting only those.
enum Pass<'p> {
    /// Owners, and (on the turned path) the nearest depth found so far.
    Find(&'p mut [u32], &'p mut [f64]),
    Paint(&'p mut Framebuffer, &'p [u32]),
}

/// Depths closer than this count as the same surface, and the later draw
/// wins. It covers shared edges and coplanar tiles, whose planes disagree by a
/// hair because each face's corners are rounded to the fixed-point grid;
/// about a quarter of a pixel's worth of depth.
const DEPTH_TIE: f64 = 0.02;
