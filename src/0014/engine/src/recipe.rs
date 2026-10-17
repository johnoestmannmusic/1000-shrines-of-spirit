//! A flat, portable encoding of a shrine's settings.
//!
//! The C ABI, the emitted artifact and the web page all speak this: a plain
//! array of `u32` words, versioned so it can never silently drift. Decoding is
//! total and defensive — every index is clamped — so a corrupt recipe can only
//! change the picture, never crash the module.
//!
//! Layout (56 words):
//! `[version][seed hi,lo][s1 hi,lo]…[s6 hi,lo][biome][weather]`
//! `[scale][chords][pace][kit][space][bpm][key][transpose]`
//! `[drone cycle][drone hold][drone tone]`
//! then four layers of `[kind][d0..d3][cycle][hold]`.

use crate::atmos::{LoopTimbre, LOOP_TIMBRES};
use crate::harmony::{Pace, Scale};
use crate::kits::{KITS, SPACES};
use crate::master::MasterFx;
use crate::overlay::CodeOverlay;
use crate::plants::Biome;
use crate::sim::SimConfig;
use crate::texture::Grid;
use crate::track::{DroneArc, KeyChoice, LoopDesign, Seeds, Settings};
use crate::weather::WeatherKind;
use crate::{DEFAULT_SEEDS, DEFAULT_SETTINGS};

pub const VERSION: u32 = 16;

/// Encode a configuration.
pub fn encode(config: &SimConfig) -> Vec<u32> {
    let mut w = Vec::with_capacity(56);
    let s = &config.settings;

    w.push(VERSION);
    w.push((config.seed >> 32) as u32);
    w.push(config.seed as u32);
    for seed in [
        config.seeds.s1,
        config.seeds.s2,
        config.seeds.s3,
        config.seeds.s4,
        config.seeds.s5,
        config.seeds.s6,
        config.seeds.s7,
    ] {
        w.push((seed >> 32) as u32);
        w.push(seed as u32);
    }
    w.push(index_of(&Biome::ALL, config.biome));
    w.push(index_of(&WeatherKind::ALL, config.weather));

    w.push(s.scale.index() as u32);
    w.push(s.chords as u32);
    w.push(s.pace.index());
    w.push(s.kit.index() as u32);
    w.push(s.space.index() as u32);
    w.push(s.bpm as u32);
    w.push(match s.key {
        KeyChoice::Seed => 0,
        KeyChoice::Note(n) => n as u32 + 1,
    });
    w.push((s.loop_transpose as i32 + 12).clamp(0, 24) as u32);
    w.push(s.drone.cycle_s as u32);
    w.push(s.drone.hold_s as u32);
    w.push(s.drone_tone as u32);

    for layer in 0..4 {
        match s.loops[layer] {
            LoopDesign::Sustained(LoopTimbre::Off) => w.extend_from_slice(&[0, 0, 0, 0, 0]),
            LoopDesign::Sustained(t) => w.extend_from_slice(&[1, t.index() as u32, 0, 0, 0]),
            LoopDesign::Events(g) => {
                let g = g.words();
                w.extend_from_slice(&[2, g[0], g[1], g[2], g[3]]);
            }
        }
        let arc = s.loop_arcs[layer];
        w.push(arc.cycle_s as u32);
        w.push(arc.hold_s as u32);
    }
    w.push(if s.drum_reverse { 1 } else { 0 });
    w.push(s.master.bits());
    w.push(s.code.word());
    w.push(s.code.angle_word());
    w.push(s.arch.rot_word());
    w.push(s.arch.off_word());
    for layer in s.glitch {
        w.push(layer.word_a());
        w.push(layer.word_b());
    }
    w.push((s.camera_speed + 180).clamp(0, 360) as u32);
    // Bit 0: the drone. Bit 1: the same rhythm on the atmosphere loops.
    // Bit 2: wildlife (an older recipe has it clear, so it decodes as off).
    w.push(s.drone_rhythm as u32 | ((s.loop_rhythm as u32) << 1) | ((s.wildlife as u32) << 2));
    // Version 16: the 2-step garage setup.
    w.push(s.garage.word());
    for word in s.model.words() {
        w.push(word);
    }
    for word in s.name.words() {
        w.push(word);
    }
    w
}

/// Decode a configuration. Short, empty or corrupt input falls back to the
/// defaults for whatever it cannot supply.
pub fn decode(words: &[u32]) -> SimConfig {
    if words.len() < 17 {
        return SimConfig::default();
    }
    let mut c = Cursor { words, at: 0 };
    let version = c.next();
    let seed = join(c.next(), c.next());
    let mut s: [u64; 7] = [0; 7];
    for (i, v) in s.iter_mut().enumerate() {
        // Version 15 added the drone-rhythm seed; older recipes carry six.
        if i == 6 && version < 15 {
            break;
        }
        *v = join(c.next(), c.next());
    }
    let biome = pick(&Biome::ALL, c.next());
    let weather = pick(&WeatherKind::ALL, c.next());

    let scale = Scale::from_index(c.next() as usize);
    let chords = (c.next() as u8).clamp(1, 4);
    let pace = match c.next() {
        1 => Pace::Jungle,
        2 => Pace::Garage,
        _ => Pace::HalfTime,
    };
    let kit = KITS[(c.next() as usize) % KITS.len()];
    let space = SPACES[(c.next() as usize) % SPACES.len()];
    let bpm = (c.next() as u16).max(1);
    let key = match c.next() {
        0 => KeyChoice::Seed,
        n => KeyChoice::Note(((n - 1) % 12) as u8),
    };
    let loop_transpose = ((c.next() as i32 - 12).clamp(-12, 12)) as i8;
    let drone = DroneArc { cycle_s: c.next() as u16, hold_s: c.next() as u16 };
    let drone_tone = c.next() as u16;

    let mut loops = [LoopDesign::OFF; 4];
    let mut loop_arcs = [DroneArc::ALWAYS_ON; 4];
    for layer in 0..4 {
        let kind = c.next();
        let data = [c.next(), c.next(), c.next(), c.next()];
        loops[layer] = match kind {
            1 => LoopDesign::Sustained(LOOP_TIMBRES[(data[0] as usize) % LOOP_TIMBRES.len()]),
            2 => LoopDesign::Events(Grid::from_words(data)),
            _ => LoopDesign::OFF,
        };
        loop_arcs[layer] = DroneArc { cycle_s: c.next() as u16, hold_s: c.next() as u16 };
    }

    // Version 2 added the drum-reverse switch; older recipes default it to on.
    let drum_reverse = if version < 2 { true } else { c.next() != 0 };
    // Version 3 added the master effects; older recipes have none.
    let master = if version < 3 { MasterFx::OFF } else { MasterFx::from_bits(c.next()) };
    // Version 4 added the source-code overlay; version 5 added its text scale
    // (older words leave the top byte zero and read as the default scale);
    // version 6 added the track name; version 7 added the overlay's tilt;
    // version 8 packed its vertical shift into the same word; version 9 added
    // the architecture transform; version 10 added the glitch layers.
    let mut code = if version < 4 { CodeOverlay::DEFAULT } else { CodeOverlay::from_word(c.next()) };
    if version >= 7 {
        let layout = c.next();
        code.angle = CodeOverlay::angle_from_word(layout);
        if version >= 8 {
            code.offset = CodeOverlay::offset_from_word(layout);
        }
    }
    let arch = if version >= 9 {
        let (rot_x, rot_y, rot_z) = crate::arch::ArchTransform::rot_from_word(c.next());
        let (off_x, off_y, off_z) = crate::arch::ArchTransform::off_from_word(c.next());
        crate::arch::ArchTransform { rot_x, rot_y, rot_z, off_x, off_y, off_z }
    } else {
        crate::arch::ArchTransform::IDENTITY
    };
    let glitch = if version >= 10 {
        core::array::from_fn(|_| {
            let a = c.next();
            let b = c.next();
            crate::glitch_setup::GlitchSetup::from_words(a, b)
        })
    } else {
        crate::glitch_setup::GlitchSetup::DEFAULT
    };
    // Version 11 added the authored architecture: 64 words, absent (and so
    // procedural) in every earlier recipe.
    let camera_speed = if version >= 12 {
        (c.next().min(360) as i16) - 180
    } else {
        0
    };
    let (drone_rhythm, loop_rhythm, wildlife) = if version >= 15 {
        let word = c.next();
        (word & 1 != 0, word & 2 != 0, word & 4 != 0)
    } else {
        (false, false, false)
    };
    // Version 16 added 2-step garage's own settings.
    let garage = if version >= 16 { crate::garage::GarageSetup::from_word(c.next()) } else { crate::garage::GarageSetup::DEFAULT };
    let model = if version >= 14 {
        let words: Vec<u32> = (0..crate::shrine_model::WORDS as u32).map(|_| c.next()).collect();
        crate::shrine_model::ShrineModel::from_words(&words)
    } else if version >= 13 {
        let words: Vec<u32> = (0..(crate::shrine_model::CELLS as u32 / 4 + 1)).map(|_| c.next()).collect();
        crate::shrine_model::ShrineModel::from_v13_words(&words)
    } else if version >= 11 {
        let words: Vec<u32> = (0..crate::shrine_model::CELLS as u32 / 4).map(|_| c.next()).collect();
        crate::shrine_model::ShrineModel::from_legacy_words(&words)
    } else {
        crate::shrine_model::ShrineModel::EMPTY
    };
    let name = if version < 6 {
        crate::name::Name::EMPTY
    } else {
        let mut words = [0u32; crate::name::CAP / 4];
        for w in &mut words {
            *w = c.next();
        }
        crate::name::Name::from_words(&words)
    };

    SimConfig {
        seed,
        seeds: Seeds { s1: s[0], s2: s[1], s3: s[2], s4: s[3], s5: s[4], s6: s[5], s7: s[6] },
        settings: Settings {
            scale,
            chords,
            pace,
            kit,
            space,
            bpm,
            loops,
            loop_arcs,
            loop_transpose,
            key,
            drone: DroneArc { cycle_s: drone.cycle_s, hold_s: drone.hold_s.min(drone.cycle_s) },
            drone_tone,
            drum_reverse,
            master,
            code,
            name,
            arch,
            glitch,
            model,
            camera_speed,
            drone_rhythm,
            loop_rhythm,
            wildlife,
            garage,
        },
        biome,
        weather,
    }
}

fn index_of<T: PartialEq + Copy>(all: &[T], value: T) -> u32 {
    all.iter().position(|v| *v == value).unwrap_or(0) as u32
}

fn pick<T: Copy>(all: &[T], i: u32) -> T {
    all[(i as usize) % all.len()]
}

fn join(hi: u32, lo: u32) -> u64 {
    ((hi as u64) << 32) | lo as u64
}

struct Cursor<'a> {
    words: &'a [u32],
    at: usize,
}

impl Cursor<'_> {
    fn next(&mut self) -> u32 {
        let v = self.words.get(self.at).copied().unwrap_or(0);
        self.at += 1;
        v
    }
}

/// The canonical default recipe, handy for tests and the page fallback.
pub fn default_words() -> Vec<u32> {
    encode(&SimConfig {
        seed: 1000,
        seeds: DEFAULT_SEEDS,
        settings: DEFAULT_SETTINGS,
        biome: Biome::Moss,
        weather: WeatherKind::Rain,
    })
}

impl SimConfig {
    /// A sane default shrine, used when no recipe is supplied.
    pub fn default_config() -> Self {
        SimConfig {
            seed: 1000,
            seeds: DEFAULT_SEEDS,
            settings: DEFAULT_SETTINGS,
            biome: Biome::Moss,
            weather: WeatherKind::Rain,
        }
    }
}

impl Default for SimConfig {
    fn default() -> Self {
        SimConfig::default_config()
    }
}
