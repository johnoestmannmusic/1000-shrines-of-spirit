//! Weather as pixels: clouds, light shafts, fog, lightning, rain, snow, leaves,
//! motes, embers, splashes and drips.
//!
//! Every effect is *stateless*: a particle's position is its own hashed start
//! plus time times its speed, wrapped, so any frame can be drawn on its own and
//! the page can render at whatever rate it likes (`HC004`). How many of each are
//! drawn comes straight from the shared [`WeatherState`] levels, which the
//! weather bed also plays from — that is what keeps the sound and the picture
//! at the same intensity.
//!
//! Effects that sit in the sky are composited with [`Framebuffer::blend_over`],
//! so they still work when the page paints the sky itself and the frame is
//! transparent there.

use crate::framebuffer::{blend, Framebuffer, Rgba};
use crate::iso::{to_px_round, Point};
use crate::math::{cos_turns, exp2, sin_turns};
use crate::palette::Palette;
use crate::rng::{hash3, Rng};
use crate::weather::{Event, EventKind, Weather, WeatherState};
use crate::{LOGICAL_H, LOGICAL_W};

const W: f64 = LOGICAL_W as f64;
const H: f64 = LOGICAL_H as f64;

// Particle layers, each with its own hash stream.
const FAR_RAIN: u64 = 1;
const NEAR_RAIN: u64 = 2;
const FLAKES: u64 = 3;
const LEAVES: u64 = 4;
const MOTES: u64 = 5;
const EMBERS: u64 = 6;
const CLOUDS: u64 = 7;
const FOG: u64 = 8;
const SHAFTS: u64 = 9;

/// Most particles per layer, at full intensity and full quality.
const MAX_FAR_RAIN: usize = 240;
const MAX_NEAR_RAIN: usize = 130;
const MAX_FLAKES: usize = 420;
const MAX_LEAVES: usize = 90;
const MAX_MOTES: usize = 80;
const MAX_EMBERS: usize = 90;

/// How long a splash lasts, and a drip takes to fall, in seconds.
pub const SPLASH_S: f64 = 0.32;
pub const DRIP_S: f64 = 0.9;
/// How long a spark's flare lasts.
const SPARK_S: f64 = 0.6;
/// How long a butterfly takes to cross the scene.
pub const BUTTERFLY_S: f64 = 18.0;

/// Everything an effect needs to know about the frame.
pub struct Fx<'a> {
    pub seed: u64,
    pub t: f64,
    pub w: WeatherState,
    pub weather: &'a Weather,
    pub palette: &'a Palette,
    /// Fraction of the particles to draw (from the quality level).
    pub fraction: f64,
    /// Fraction of the rain to draw (rain is the heaviest effect).
    pub rain_fraction: f64,
    /// Weather lighting and grading, applied to one colour.
    pub grade: &'a dyn Fn(Rgba) -> Rgba,
}

/// Rain's sideways speed as a share of its fall at full wind: the streaks
/// lean by the same ratio, so they point along their travel.
const RAIN_SLANT: f64 = 0.45;

fn particle(seed: u64, layer: u64, i: usize) -> Rng {
    Rng::new(hash3(seed, layer, i as u64))
}

fn wrap(x: f64, span: f64) -> f64 {
    x.rem_euclid(span)
}

impl Fx<'_> {
    /// The sideways wind now, -1 (left) .. 1 (right).
    fn wind_x(&self) -> f64 {
        self.w.wind_x
    }

    /// How far the wind has carried things, in seconds at full wind: every
    /// particle drifts by a multiple of this, so its motion always matches
    /// the wind of the moment and turns round only when the wind does.
    fn drift(&self) -> f64 {
        self.w.drift
    }

    fn count(&self, max: usize, level: f64, fraction: f64) -> usize {
        (max as f64 * level.clamp(0.0, 1.0) * fraction) as usize
    }

    /// The far layer, behind the architecture: light shafts, clouds, the
    /// lightning bolt, fog banks, and the distant rain and snow.
    pub fn far(&self, fb: &mut Framebuffer) {
        self.shafts(fb);
        self.clouds(fb);
        self.bolt(fb);
        self.fog_banks(fb, 105.0, 200.0, 4, 120);
        self.rain(fb, FAR_RAIN, MAX_FAR_RAIN, (3, 6), (150.0, 210.0), 105);
        self.flakes(fb, true);
    }

    /// The near layer, over everything: the close rain, snow, leaves, motes,
    /// embers and sparks, then the low mist.
    pub fn near(&self, fb: &mut Framebuffer, ground: &dyn Fn(f64, f64) -> Point) {
        self.rain(fb, NEAR_RAIN, MAX_NEAR_RAIN, (9, 17), (230.0, 300.0), 235);
        self.flakes(fb, false);
        self.leaves(fb);
        self.motes(fb);
        self.embers(fb);
        self.sparks(fb, ground);
        self.butterflies(fb);
        self.fog_banks(fb, 205.0, 290.0, 3, 95);
    }

    /// Soft diagonal shafts of light, in clear weather and at dusk.
    fn shafts(&self, fb: &mut Framebuffer) {
        let level = self.w.motes * self.w.light * (1.0 - self.w.cloud);
        if level < 0.05 {
            return;
        }
        let colour = (self.grade)([255, 236, 196, 255]);
        for i in 0..3 {
            let mut r = particle(self.seed, SHAFTS, i);
            let (x0, width, drift) = (r.range(-40.0, 220.0), r.range(14.0, 34.0), r.range(0.02, 0.05));
            // Each shaft slowly brightens and fades on its own.
            let a = level * (0.5 + 0.5 * sin_turns(self.t * drift + r.unit()));
            let alpha = (a * 34.0) as u32;
            for y in 0..(H as i32 * 3 / 4) {
                // Leaning down to the right, fading toward the ground.
                let fade = 1.0 - y as f64 / (H * 0.75);
                let cx = x0 + y as f64 * 0.55;
                for x in (cx - width / 2.0) as i32..=(cx + width / 2.0) as i32 {
                    let edge = 1.0 - ((x as f64 - cx).abs() / (width / 2.0)).min(1.0);
                    fb.blend_over(x, y, colour, (alpha as f64 * fade * edge) as u32);
                }
            }
        }
    }

    /// Clouds drifting across the top of the sky with the wind; darker and
    /// heavier the stormier it is.
    fn clouds(&self, fb: &mut Framebuffer) {
        let level = self.w.cloud;
        if level < 0.05 {
            return;
        }
        let dark = blend(self.palette.fog, [26, 28, 38, 255], (level * 85.0) as u32, 100);
        let lit = blend(dark, [235, 240, 255, 255], (self.w.flash * 80.0) as u32, 100);
        let colour = (self.grade)(lit);
        let speed = 18.0;
        let n = 3 + (level * 6.0) as usize;
        for i in 0..n {
            let mut r = particle(self.seed, CLOUDS, i);
            let (w, h) = (r.range(60.0, 150.0), r.range(9.0, 22.0));
            let span = W + w * 2.0;
            let cx = wrap(r.unit() * span + self.drift() * speed * r.range(0.7, 1.3), span) - w;
            let cy = r.range(6.0, 85.0);
            let alpha = level * r.range(90.0, 170.0);
            for y in (cy - h) as i32..=(cy + h) as i32 {
                let dy = (y as f64 - cy) / h;
                for x in (cx - w) as i32..=(cx + w) as i32 {
                    let dx = (x as f64 - cx) / w;
                    // A soft-edged ellipse with a lumpy top.
                    let lump = 0.12 * sin_turns(dx * 2.5 + r.unit());
                    let d = dx * dx + (dy + lump) * (dy + lump);
                    if d < 1.0 {
                        fb.blend_over(x, y, colour, (alpha * (1.0 - d)) as u32);
                    }
                }
            }
        }
    }

    /// Horizontal fog bands between rows `y0` and `y1`, swelling and drifting.
    fn fog_banks(&self, fb: &mut Framebuffer, y0: f64, y1: f64, bands: u64, max_alpha: u32) {
        let level = self.w.fog;
        if level < 0.12 {
            return;
        }
        let colour = (self.grade)(self.palette.fog);
        // A slow creep of its own, plus the wind.
        let travel = self.drift() * 6.0 + self.t * 2.0;
        for b in 0..bands {
            let mut r = particle(self.seed, FOG, (b + (y0 as u64) * 10) as usize);
            let cy = y0 + (y1 - y0) * (b as f64 + 0.5) / bands as f64;
            let thick = r.range(10.0, 22.0);
            let (f1, f2, ph) = (r.range(1.0, 2.5), r.range(3.0, 5.0), r.unit());
            let a = level * max_alpha as f64 * (0.6 + 0.4 * sin_turns(self.t * 0.03 + ph));
            for x in 0..LOGICAL_W as i32 {
                let u = (x as f64 + travel) / W;
                let edge = cy + 6.0 * sin_turns(u * f1 + ph) + 3.0 * sin_turns(u * f2 - self.t * 0.05);
                for y in (edge - thick) as i32..=(edge + thick) as i32 {
                    let d = ((y as f64 - edge) / thick).abs();
                    fb.blend_over(x, y, colour, (a * (1.0 - d * d)) as u32);
                }
            }
        }
    }

    /// The bolt of any strike in its first quarter-second: a jagged line down
    /// from the top of the sky with a branch or two, and a soft glow.
    fn bolt(&self, fb: &mut Framebuffer) {
        if self.w.flash <= 0.0 {
            return;
        }
        for e in self.weather.events(EventKind::Strike, self.t - 0.25, self.t + 1e-9) {
            let (strength, distance) = e.strike();
            let age = self.t - e.at;
            let a = strength * (1.0 - 0.5 * distance) * exp2(-age / 0.06);
            let mut r = Rng::new(e.hash ^ 0x424f_4c54);
            let x0 = r.range(0.12, 0.88) * W;
            let bottom = r.range(90.0, 150.0) * (1.0 - 0.3 * distance);
            let core = [245, 248, 255, 255];
            let glow = [170, 190, 255, 255];
            let mut path = vec![(x0, 0.0)];
            let steps = 14;
            for k in 1..=steps {
                let (px, _) = path[k - 1];
                path.push((px + r.range(-7.0, 7.0), bottom * k as f64 / steps as f64));
            }
            let mut segments: Vec<((f64, f64), (f64, f64), f64)> = path.windows(2).map(|p| (p[0], p[1], 1.0)).collect();
            for _ in 0..r.int(1, 2) {
                let mut at = path[r.int(3, steps as u64 - 3) as usize];
                let lean = if r.chance(0.5) { -1.0 } else { 1.0 };
                for _ in 0..r.int(3, 6) {
                    let next = (at.0 + lean * r.range(2.0, 8.0), at.1 + r.range(5.0, 10.0));
                    segments.push((at, next, 0.55));
                    at = next;
                }
            }
            for (p, q, weight) in segments {
                let alpha = (a * weight * 255.0) as u32;
                stroke(fb, p, q, glow, alpha / 3, 1);
                stroke(fb, p, q, core, alpha, 0);
            }
        }
    }

    /// One rain layer: streaks tilted by the wind whose heads are bright and
    /// whose tails fade, so they read as motion rather than as lines.
    fn rain(&self, fb: &mut Framebuffer, layer: u64, max: usize, len: (u64, u64), speed: (f64, f64), alpha: u32) {
        let n = self.count(max, self.w.rain, self.rain_fraction);
        if n == 0 {
            return;
        }
        let base = blend(self.palette.sky_bottom, [232, 238, 250, 255], 78, 100);
        let colour = (self.grade)(base);
        let wind_x = self.wind_x();
        let span = H + 40.0;
        for i in 0..n {
            let mut r = particle(self.seed, layer, i);
            let (x, phase, v, l) = (r.unit() * W, r.unit(), r.range(speed.0, speed.1), r.int(len.0, len.1) as i32);
            let y = wrap(phase * span + self.t * v, span) - 20.0;
            let x = wrap(x + self.drift() * v * RAIN_SLANT, W);
            // The streak lies along the drop's travel: across by the wind,
            // down by its fall.
            let dx = wind_x * RAIN_SLANT * l as f64;
            for k in 0..=l {
                let f = k as f64 / l as f64; // 0 at the tail, 1 at the head
                let px = (x + dx * f).round() as i32;
                fb.blend_over(px, y as i32 + k, colour, (alpha as f64 * (0.15 + 0.85 * f * f)) as u32);
            }
        }
    }

    /// Snowflakes in three sizes, swaying and drifting with the wind, some of
    /// them catching the light. The far layer is the small, slow ones.
    fn flakes(&self, fb: &mut Framebuffer, far: bool) {
        let n = self.count(MAX_FLAKES, self.w.snow, self.fraction);
        if n == 0 {
            return;
        }
        let white = (self.grade)([246, 248, 255, 255]);
        let span = H + 20.0;
        for i in 0..n {
            let mut r = particle(self.seed, FLAKES, i);
            let size = if r.chance(0.55) { 0 } else if r.chance(0.7) { 1 } else { 2 };
            if far != (size == 0) {
                continue;
            }
            let (x, phase, v) = (r.unit() * W, r.unit(), r.range(10.0, 22.0) * (1.0 + size as f64 * 0.6));
            let (sway, sway_hz, sway_at) = (r.range(2.0, 7.0), r.range(0.15, 0.4), r.unit());
            let y = wrap(phase * span + self.t * v, span) - 10.0;
            let x = wrap(x + self.drift() * 30.0 + sway * sin_turns(self.t * sway_hz + sway_at), W);
            let twinkle = 0.5 + 0.5 * sin_turns(self.t * r.range(0.5, 2.0) + r.unit());
            let alpha = match size {
                0 => 150,
                1 => 190 + (65.0 * twinkle) as u32,
                _ => 240,
            };
            let (px, py) = (x as i32, y as i32);
            fb.blend_over(px, py, white, alpha);
            if size == 2 {
                for (ox, oy) in [(1, 0), (0, 1), (1, 1)] {
                    fb.blend_over(px + ox, py + oy, white, alpha * 2 / 3);
                }
            }
        }
    }

    /// Leaves and debris blown across the scene, tumbling (their colour
    /// flips as they turn) and fluttering up and down.
    fn leaves(&self, fb: &mut Framebuffer) {
        let n = self.count(MAX_LEAVES, self.w.leaves, self.fraction);
        if n == 0 {
            return;
        }
        let tones = [self.palette.foliage, self.palette.foliage_light, self.palette.accent, self.palette.ground];
        let span = W + 40.0;
        for i in 0..n {
            let mut r = particle(self.seed, LEAVES, i);
            let (x, y0, v) = (r.unit() * span, r.range(40.0, 270.0), r.range(50.0, 130.0));
            let (flutter, f_hz, at) = (r.range(3.0, 12.0), r.range(0.3, 0.9), r.unit());
            let x = wrap(x + self.drift() * v, span) - 20.0;
            let y = y0 + flutter * sin_turns(self.t * f_hz + at) + 4.0 * sin_turns(self.t * f_hz * 3.1 + at);
            let tumble = sin_turns(self.t * r.range(0.8, 2.2) + r.unit());
            // Lifted toward the light, so a leaf reads against the moss.
            let tone = blend(tones[r.below(tones.len())], [255, 236, 170, 255], 30, 100);
            let colour = (self.grade)(if tumble > 0.0 { tone } else { blend(tone, [20, 20, 20, 255], 40, 100) });
            let (px, py) = (x as i32, y as i32);
            fb.put(px, py, colour);
            fb.put(px, py + 1, colour);
            // Face-on it shows its width; edge-on, only a sliver.
            if tumble.abs() > 0.3 {
                fb.put(px + 1, py, colour);
                fb.put(px + 1, py + 1, colour);
            }
            if tumble.abs() > 0.7 {
                fb.put(px + 2, py + 1, colour);
            }
        }
    }

    /// Dust motes drifting slowly in warm light, glinting as they turn.
    fn motes(&self, fb: &mut Framebuffer) {
        let n = self.count(MAX_MOTES, self.w.motes, self.fraction);
        if n == 0 {
            return;
        }
        let colour = (self.grade)([255, 244, 214, 255]);
        for i in 0..n {
            let mut r = particle(self.seed, MOTES, i);
            let (x0, y0) = (r.unit() * W, r.range(30.0, 260.0));
            let (ax, ay, hz, at) = (r.range(6.0, 20.0), r.range(4.0, 12.0), r.range(0.03, 0.09), r.unit());
            let x = wrap(x0 + ax * sin_turns(self.t * hz + at) + self.drift() * 8.0, W);
            let y = y0 + ay * sin_turns(self.t * hz * 1.3 + at * 2.0);
            let glint = 0.5 + 0.5 * sin_turns(self.t * r.range(0.2, 0.7) + r.unit());
            fb.blend_over(x as i32, y as i32, colour, (40.0 + 200.0 * glint * glint * self.w.light) as u32);
        }
    }

    /// Embers rising from the ground, swaying, cooling from yellow through
    /// orange to a dull red as they climb, with a faint glow around the hot
    /// ones.
    fn embers(&self, fb: &mut Framebuffer) {
        let n = self.count(MAX_EMBERS, self.w.embers, self.fraction);
        if n == 0 {
            return;
        }
        let span = 240.0;
        for i in 0..n {
            let mut r = particle(self.seed, EMBERS, i);
            let (x0, phase, v) = (r.unit() * W, r.unit(), r.range(8.0, 26.0));
            let (sway, hz, at) = (r.range(3.0, 10.0), r.range(0.1, 0.35), r.unit());
            let climb = wrap(phase * span + self.t * v, span);
            let life = climb / span; // 0 just lifted, 1 burnt out
            let y = 290.0 - climb;
            let x = wrap(x0 + sway * sin_turns(self.t * hz + at) + self.drift() * 10.0, W);
            let hot = [255, 214, 120, 255];
            let cool = [170, 50, 36, 255];
            let colour = (self.grade)(blend(hot, cool, (life * 100.0) as u32, 100));
            let flicker = 0.7 + 0.3 * sin_turns(self.t * r.range(3.0, 7.0) + r.unit());
            let alpha = ((1.0 - life) * 255.0 * flicker) as u32;
            let (px, py) = (x as i32, y as i32);
            fb.blend_over(px, py, colour, alpha);
            if life < 0.6 {
                for (ox, oy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                    fb.blend_over(px + ox, py + oy, colour, alpha / 3);
                }
            }
            if life < 0.25 {
                fb.blend_over(px, py - 1, [255, 240, 200, 255], alpha / 2);
            }
        }
    }

    /// A rare butterfly drifting slowly across the scene: a steady, gently
    /// wavering path rather than a bounce, its wings opening and closing.
    fn butterflies(&self, fb: &mut Framebuffer) {
        for e in self.weather.events(EventKind::Butterfly, self.t - BUTTERFLY_S, self.t + 1e-9) {
            let age = (self.t - e.at) / BUTTERFLY_S;
            let mut r = Rng::new(e.hash ^ 0x4246_4c59);
            let dir = if r.chance(0.5) { -1.0 } else { 1.0 };
            let (y0, rise) = (r.range(110.0, 220.0), r.range(-25.0, 25.0));
            let (sway_at, flap_hz) = (r.unit(), r.range(2.5, 3.5));
            let x = if dir > 0.0 { -6.0 + age * (W + 12.0) } else { W + 6.0 - age * (W + 12.0) };
            // A slow drift up or down over the whole crossing, and a waver of
            // a pixel or so — never a bounce.
            let y = y0 + rise * age + 1.2 * sin_turns(age * 3.0 + sway_at);
            let colour = (self.grade)(blend(self.palette.accent, [255, 244, 210, 255], 35, 100));
            let (px, py) = (x as i32, y as i32);
            fb.put(px, py, colour);
            // Wings: open, a pixel either side; closed, folded up into one.
            if sin_turns(self.t * flap_hz + sway_at) > 0.0 {
                fb.put(px - 1, py - 1, colour);
                fb.put(px + 1, py - 1, colour);
            } else {
                fb.put(px, py - 1, colour);
            }
        }
    }

    /// A spark's flare: a burst of three to five hot points thrown up from
    /// where it popped, falling back as they fade. The weather bed plays a
    /// crackle at exactly `e.at`.
    fn sparks(&self, fb: &mut Framebuffer, ground: &dyn Fn(f64, f64) -> Point) {
        if self.w.embers <= 0.0 {
            return;
        }
        for e in self.weather.events(EventKind::Spark, self.t - SPARK_S, self.t + 1e-9) {
            let age = (self.t - e.at) / SPARK_S;
            let (gx, gy) = e.spot();
            let base = ground(gx, gy);
            let (bx, by) = (to_px_round(base.x) as f64, to_px_round(base.y) as f64);
            let mut r = Rng::new(e.hash ^ 0x5350_524b);
            let colour = (self.grade)([255, 200, 110, 255]);
            for _ in 0..r.int(3, 5) {
                let (vx, vy) = (r.range(-14.0, 14.0), r.range(18.0, 34.0));
                let s = age * SPARK_S;
                let x = bx + vx * s;
                let y = by - vy * s + 40.0 * s * s;
                fb.blend_over(x as i32, y as i32, colour, ((1.0 - age) * 255.0) as u32);
            }
        }
    }
}

/// The splash of a raindrop landing at plane point `at` (fixed point), `age`
/// in `[0, 1]`: a flattened ring spreading out, and a crown of droplets thrown
/// up and falling back. Larger on stone (`hard`) than in the moss.
pub fn splash(fb: &mut Framebuffer, at: Point, age: f64, hard: bool, colour: Rgba) {
    let (cx, cy) = (to_px_round(at.x) as f64, to_px_round(at.y) as f64);
    let fade = 1.0 - age;
    // The moment of impact: a bright point.
    if age < 0.2 {
        fb.blend_over(cx as i32, cy as i32, colour, 255);
    }
    let ring = (1.5 + if hard { 4.5 } else { 3.0 } * age, 0.5);
    let steps = 14;
    for k in 0..steps {
        let a = k as f64 / steps as f64;
        let x = cx + ring.0 * cos_turns(a);
        let y = cy + ring.0 * ring.1 * sin_turns(a);
        fb.blend_over(x.round() as i32, y.round() as i32, colour, (fade * 215.0) as u32);
    }
    if hard {
        for (dir, lift) in [(-1.0, 1.0), (1.0, 0.8), (-0.4, 1.3), (0.5, 1.2)] {
            let x = cx + dir * 4.0 * age;
            let y = cy - lift * 6.0 * age + 9.0 * age * age;
            fb.blend_over(x.round() as i32, y.round() as i32, colour, (fade * 230.0) as u32);
        }
    }
}

/// A drip at `age` in `[0, 1]`: a bead swelling on the edge at `at`, then
/// falling `fall` logical pixels, glinting.
pub fn drip(fb: &mut Framebuffer, at: Point, fall: f64, age: f64, colour: Rgba) {
    let (cx, cy) = (to_px_round(at.x), to_px_round(at.y) as f64);
    if age < 0.4 {
        // Swelling on the edge.
        fb.blend_over(cx, cy as i32, colour, (age / 0.4 * 220.0) as u32);
        return;
    }
    let f = (age - 0.4) / 0.6;
    let y = cy + fall * f * f;
    fb.blend_over(cx, y as i32, colour, 235);
    fb.blend_over(cx, y as i32 - 1, colour, 90);
}

/// A thick-ish line from `p` to `q` in logical pixels, composited.
fn stroke(fb: &mut Framebuffer, p: (f64, f64), q: (f64, f64), colour: Rgba, alpha: u32, radius: i32) {
    let n = ((q.0 - p.0).abs().max((q.1 - p.1).abs()) as i32).max(1);
    for k in 0..=n {
        let f = k as f64 / n as f64;
        let (x, y) = ((p.0 + (q.0 - p.0) * f) as i32, (p.1 + (q.1 - p.1) * f) as i32);
        for ox in -radius..=radius {
            fb.blend_over(x + ox, y, colour, alpha);
        }
    }
}

/// Events of `kind` whose effect is visible at `t` (begun within `span`).
pub fn live(weather: &Weather, kind: EventKind, t: f64, span: f64) -> Vec<(Event, f64)> {
    weather
        .events(kind, t - span, t + 1e-9)
        .into_iter()
        .map(|e| {
            let age = ((t - e.at) / span).clamp(0.0, 1.0);
            (e, age)
        })
        .collect()
}
