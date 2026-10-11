//! `.shrineseed` files: a shrine's settings as human-readable `key=value` text.
//!
//! A seed can be written down, hand-edited, shared and committed. It is a
//! friendlier face of the same data the compiled artifact bakes; importing one
//! simply sets the stored settings.
//!
//! Host-only. The artifact itself carries the recipe as words (`recipe.rs`).

use shrine0012::atmos::LOOP_TIMBRES;
use shrine0012::harmony::{key_name, Pace, SCALES};
use shrine0012::kits::{KITS, SPACES};
use shrine0012::plants::Biome;
use shrine0012::sim::SimConfig;
use shrine0012::texture::Grid;
use shrine0012::track::{KeyChoice, LoopDesign, Seeds};
use shrine0012::weather::WeatherKind;
use std::fs;
use std::io;
use std::path::Path;

fn key_label(key: KeyChoice) -> String {
    match key {
        KeyChoice::Seed => "seed".to_string(),
        KeyChoice::Note(n) => key_name(n).to_string(),
    }
}

fn layer_label(design: LoopDesign) -> String {
    match design {
        LoopDesign::Sustained(t) => t.name().to_string(),
        LoopDesign::Events(_) => "event grid".to_string(),
    }
}

fn eq(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// The settings as readable text.
pub fn to_text(config: &SimConfig) -> String {
    let s = &config.settings;
    let mut out = String::new();
    out.push_str("# ShrineGen shrine seed — human-readable settings. Edit and re-import.\n");
    out.push_str(&format!("# build v{}\n", env!("BUILD_DATE")));
    out.push_str(&format!("seed={}\n", config.seed));
    out.push_str(&format!("name={}\n", s.name.as_str()));
    out.push_str(&format!(
        "seeds={},{},{},{},{},{}\n",
        config.seeds.s1, config.seeds.s2, config.seeds.s3, config.seeds.s4, config.seeds.s5, config.seeds.s6
    ));
    out.push_str("# seeds shape: 1 keys & chords, 2 glitch 1, 3 glitch 2, 4 bass, 5 drums, 6 atmosphere\n");
    out.push_str(&format!("biome={}\n", config.biome.name()));
    out.push_str(&format!("weather={}\n", config.weather.name()));
    out.push_str(&format!("scale={}\n", s.scale.name()));
    out.push_str(&format!("key={}\n", key_label(s.key)));
    out.push_str(&format!("chords={}\n", s.chords));
    out.push_str(&format!("pace={}\n", if s.pace == Pace::Jungle { "jungle" } else { "half-time" }));
    out.push_str(&format!("kit={}\n", s.kit.name()));
    out.push_str(&format!("space={}\n", s.space.name()));
    out.push_str(&format!("drum_reverse={}\n", if s.drum_reverse { "on" } else { "off" }));
    out.push_str(&format!("master_fx={}\n", if s.master.enabled { "on" } else { "off" }));
    out.push_str(&format!("code_overlay={}\n", if s.code.enabled { "on" } else { "off" }));
    out.push_str(&format!("code_opacity={}\n", s.code.opacity));
    out.push_str(&format!("code_speed={}\n", s.code.speed));
    out.push_str(&format!("code_scale={}\n", s.code.scale));
    out.push_str(&format!("code_angle={}\n", s.code.angle));
    out.push_str(&format!("code_offset={}\n", s.code.offset));
    for (name, on) in [
        ("fx_tape", s.master.tape),
        ("fx_cd", s.master.cd),
        ("fx_pitch", s.master.pitch),
        ("fx_radio", s.master.radio),
        ("fx_crush", s.master.crush),
        ("fx_dropout", s.master.dropout),
    ] {
        out.push_str(&format!("{name}={}\n", if on { "on" } else { "off" }));
    }
    out.push_str(&format!("bpm={}\n", s.bpm));
    out.push_str(&format!("transpose={}\n", s.loop_transpose));
    out.push_str(&format!("drone_cycle={}\n", s.drone.cycle_s));
    out.push_str(&format!("drone_hold={}\n", s.drone.hold_s));
    out.push_str(&format!("drone_tone={}\n", s.drone_tone));
    for i in 0..4 {
        out.push_str(&format!("layer{}={}\n", i + 1, layer_label(s.loops[i])));
        if let LoopDesign::Events(grid) = s.loops[i] {
            out.push_str(&format!("layer{}_grid={}\n", i + 1, grid_token(grid)));
        }
        out.push_str(&format!("layer{}_cycle={}\n", i + 1, s.loop_arcs[i].cycle_s));
        out.push_str(&format!("layer{}_hold={}\n", i + 1, s.loop_arcs[i].hold_s));
    }
    out
}

/// Parse text back into a config. Missing keys keep the defaults; returns
/// `None` only when nothing recognisable was found.
pub fn from_text(text: &str) -> Option<SimConfig> {
    let mut config = SimConfig::default();
    let mut found = false;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((raw_key, raw_value)) = line.split_once('=') else {
            continue;
        };
        let key = raw_key.trim().to_ascii_lowercase();
        let value = raw_value.trim();

        match key.as_str() {
            "seed" => {
                if let Ok(n) = value.parse() {
                    config.seed = n;
                    found = true;
                }
            }
            "name" => {
                config.settings.name = shrine0012::name::Name::from_str(value);
                found = true;
            }
            "seeds" => {
                let nums: Vec<u64> = value.split(',').filter_map(|p| p.trim().parse().ok()).collect();
                if nums.len() == 6 {
                    config.seeds = Seeds { s1: nums[0], s2: nums[1], s3: nums[2], s4: nums[3], s5: nums[4], s6: nums[5] };
                    found = true;
                }
            }
            "biome" => {
                if let Some(b) = [Biome::Moss, Biome::Verdant, Biome::Fungal].into_iter().find(|b| eq(b.name(), value)) {
                    config.biome = b;
                    found = true;
                }
            }
            "weather" => {
                if let Some(w) = WeatherKind::ALL.into_iter().find(|w| eq(w.name(), value)) {
                    config.weather = w;
                    found = true;
                }
            }
            "scale" => {
                if let Some(sc) = SCALES.into_iter().find(|sc| eq(sc.name(), value)) {
                    config.settings.scale = sc;
                    found = true;
                }
            }
            "key" => {
                if let Some(k) = parse_key(value) {
                    config.settings.key = k;
                    found = true;
                }
            }
            "chords" => {
                if let Ok(n) = value.parse::<u8>() {
                    config.settings.chords = n.clamp(1, 4);
                    found = true;
                }
            }
            "pace" => {
                config.settings.pace = if eq(value, "jungle") { Pace::Jungle } else { Pace::HalfTime };
                found = true;
            }
            "kit" => {
                if let Some(k) = KITS.into_iter().find(|k| eq(k.name(), value)) {
                    config.settings.kit = k;
                    found = true;
                }
            }
            "space" => {
                if let Some(sp) = SPACES.into_iter().find(|sp| eq(sp.name(), value)) {
                    config.settings.space = sp;
                    found = true;
                }
            }
            "drum_reverse" => {
                config.settings.drum_reverse = parse_bool(value);
                found = true;
            }
            "master_fx" => {
                config.settings.master.enabled = parse_bool(value);
                found = true;
            }
            "code_overlay" => {
                config.settings.code.enabled = parse_bool(value);
                found = true;
            }
            "code_opacity" => {
                if let Ok(n) = value.parse::<u8>() {
                    config.settings.code.opacity = n.min(100);
                    found = true;
                }
            }
            "code_speed" => {
                if let Ok(n) = value.parse::<u8>() {
                    config.settings.code.speed = n.min(100);
                    found = true;
                }
            }
            "code_scale" => {
                if let Ok(n) = value.parse::<u8>() {
                    config.settings.code.scale = n.clamp(
                        shrine0012::overlay::CodeOverlay::MIN_SCALE,
                        shrine0012::overlay::CodeOverlay::MAX_SCALE,
                    );
                    found = true;
                }
            }
            "code_angle" => {
                if let Ok(n) = value.parse::<i16>() {
                    config.settings.code.angle = n.clamp(
                        shrine0012::overlay::CodeOverlay::MIN_ANGLE,
                        shrine0012::overlay::CodeOverlay::MAX_ANGLE,
                    );
                    found = true;
                }
            }
            "code_offset" => {
                if let Ok(n) = value.parse::<i16>() {
                    config.settings.code.offset = n.clamp(
                        shrine0012::overlay::CodeOverlay::MIN_OFFSET,
                        shrine0012::overlay::CodeOverlay::MAX_OFFSET,
                    );
                    found = true;
                }
            }
            "fx_tape" => {
                config.settings.master.tape = parse_bool(value);
                found = true;
            }
            "fx_cd" => {
                config.settings.master.cd = parse_bool(value);
                found = true;
            }
            "fx_pitch" => {
                config.settings.master.pitch = parse_bool(value);
                found = true;
            }
            "fx_radio" => {
                config.settings.master.radio = parse_bool(value);
                found = true;
            }
            "fx_crush" => {
                config.settings.master.crush = parse_bool(value);
                found = true;
            }
            "fx_dropout" => {
                config.settings.master.dropout = parse_bool(value);
                found = true;
            }
            "bpm" => {
                if let Ok(n) = value.parse::<u16>() {
                    config.settings.bpm = n.max(1);
                    found = true;
                }
            }
            "transpose" => {
                if let Ok(n) = value.parse::<i8>() {
                    config.settings.loop_transpose = n.clamp(-12, 12);
                    found = true;
                }
            }
            "drone_cycle" => {
                if let Ok(n) = value.parse::<u16>() {
                    config.settings.drone.cycle_s = n;
                    found = true;
                }
            }
            "drone_hold" => {
                if let Ok(n) = value.parse::<u16>() {
                    config.settings.drone.hold_s = n;
                    found = true;
                }
            }
            "drone_tone" => {
                if let Ok(n) = value.parse::<u16>() {
                    config.settings.drone_tone = n;
                    found = true;
                }
            }
            _ => {
                if let Some((index, field)) = parse_layer_key(&key) {
                    match field {
                        0 | 1 => {
                            if let Ok(n) = value.parse::<u16>() {
                                if field == 0 {
                                    config.settings.loop_arcs[index].cycle_s = n;
                                } else {
                                    config.settings.loop_arcs[index].hold_s = n;
                                }
                                found = true;
                            }
                        }
                        2 => {
                            if let Some(d) = parse_layer(value) {
                                config.settings.loops[index] = d;
                                found = true;
                            }
                        }
                        _ => {
                            if let Some(grid) = parse_grid(value) {
                                config.settings.loops[index] = LoopDesign::Events(grid);
                                found = true;
                            }
                        }
                    }
                }
            }
        }
    }

    // Keep the arcs consistent.
    for arc in config.settings.loop_arcs.iter_mut() {
        if arc.cycle_s == 0 {
            arc.hold_s = 0;
        } else {
            arc.hold_s = arc.hold_s.min(arc.cycle_s);
        }
    }
    if config.settings.drone.cycle_s == 0 {
        config.settings.drone.hold_s = 0;
    } else {
        config.settings.drone.hold_s = config.settings.drone.hold_s.min(config.settings.drone.cycle_s);
    }

    found.then_some(config)
}

fn parse_bool(value: &str) -> bool {
    matches!(value.trim().to_ascii_lowercase().as_str(), "on" | "true" | "yes" | "1")
}

fn parse_key(value: &str) -> Option<KeyChoice> {    if eq(value, "seed") || eq(value, "auto") || value.is_empty() {
        return Some(KeyChoice::Seed);
    }
    (0..12u8).find(|pc| eq(key_name(*pc), value)).map(KeyChoice::Note)
}

fn parse_layer(value: &str) -> Option<LoopDesign> {
    if eq(value, "event grid") || eq(value, "events") || eq(value, "grid") {
        return Some(LoopDesign::Events(Grid::fire()));
    }
    if eq(value, "off") || eq(value, "none") {
        return Some(LoopDesign::OFF);
    }
    LOOP_TIMBRES.into_iter().find(|t| eq(t.name(), value)).map(LoopDesign::Sustained)
}

/// `layer1` -> `(0, 2 layers)`, `layer1_cycle` -> `(0, 0)`, `layer1_hold` -> `(0, 1)`,
/// `layer1_grid` -> `(0, 3)`.
fn parse_layer_key(key: &str) -> Option<(usize, u8)> {
    let rest = key.strip_prefix("layer")?;
    let (num, field) = match rest.split_once('_') {
        Some((num, "cycle")) => (num, 0),
        Some((num, "hold")) => (num, 1),
        Some((num, "grid")) => (num, 3),
        Some(_) => return None,
        None => (rest, 2),
    };
    let index: usize = num.parse().ok()?;
    (1..=4).contains(&index).then_some((index - 1, field))
}

/// A named preset grid, or the raw bits as 32 hex digits.
fn grid_token(grid: Grid) -> String {
    let words = grid.words();
    if words == Grid::fire().words() {
        "fire".to_string()
    } else if words == Grid::water().words() {
        "water".to_string()
    } else if words == Grid::stones().words() {
        "stones".to_string()
    } else if words == Grid::wood().words() {
        "wood".to_string()
    } else {
        words.iter().map(|w| format!("{w:08x}")).collect()
    }
}

fn parse_grid(token: &str) -> Option<Grid> {
    let t = token.trim().to_ascii_lowercase();
    match t.as_str() {
        "fire" => return Some(Grid::fire()),
        "water" => return Some(Grid::water()),
        "stones" => return Some(Grid::stones()),
        "wood" => return Some(Grid::wood()),
        _ => {}
    }
    if t.len() != 32 {
        return None;
    }
    let mut words = [0u32; 4];
    for (i, word) in words.iter_mut().enumerate() {
        *word = u32::from_str_radix(&t[i * 8..i * 8 + 8], 16).ok()?;
    }
    Some(Grid::from_words(words))
}

pub fn write(config: &SimConfig, path: &str) -> io::Result<()> {
    if let Some(dir) = Path::new(path).parent() {
        if !dir.as_os_str().is_empty() {
            fs::create_dir_all(dir)?;
        }
    }
    fs::write(path, to_text(config))
}

pub fn read(path: &str) -> io::Result<SimConfig> {
    let text = fs::read_to_string(path)?;
    from_text(&text).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no settings found in the file"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use shrine0012::atmos::LoopTimbre;
    use shrine0012::track::{DroneArc, KeyChoice};

    fn custom() -> SimConfig {
        let mut c = SimConfig {
            seed: 57306,
            seeds: Seeds { s1: 1, s2: 2, s3: 3, s4: 4, s5: 5, s6: 6 },
            biome: Biome::Fungal,
            weather: WeatherKind::Storm,
            ..SimConfig::default()
        };
        c.settings.chords = 2;
        c.settings.bpm = 99;
        c.settings.drum_reverse = false;
        c.settings.key = KeyChoice::Note(5);
        c.settings.loop_transpose = -7;
        c.settings.drone = DroneArc { cycle_s: 120, hold_s: 40 };
        c.settings.loops = [
            LoopDesign::Events(Grid::wood()),
            LoopDesign::Sustained(LoopTimbre::Veil),
            LoopDesign::OFF,
            LoopDesign::Sustained(LoopTimbre::Gust),
        ];
        c.settings.loop_arcs = [
            DroneArc::ALWAYS_ON,
            DroneArc { cycle_s: 60, hold_s: 15 },
            DroneArc::ALWAYS_ON,
            DroneArc::ALWAYS_ON,
        ];
        c
    }

    #[test]
    fn round_trips_exactly() {
        for config in [SimConfig::default(), custom()] {
            let text = to_text(&config);
            let back = from_text(&text).expect("should parse");
            assert_eq!(back.seed, config.seed);
            assert_eq!(back.seeds, config.seeds);
            assert_eq!(back.biome, config.biome);
            assert_eq!(back.weather, config.weather);
            assert_eq!(back.settings.scale, config.settings.scale);
            assert_eq!(back.settings.key, config.settings.key);
            assert_eq!(back.settings.chords, config.settings.chords);
            assert_eq!(back.settings.pace, config.settings.pace);
            assert_eq!(back.settings.kit, config.settings.kit);
            assert_eq!(back.settings.space, config.settings.space);
            assert_eq!(back.settings.bpm, config.settings.bpm);
            assert_eq!(back.settings.drum_reverse, config.settings.drum_reverse);
            assert_eq!(back.settings.loop_transpose, config.settings.loop_transpose);
            assert_eq!(back.settings.drone.cycle_s, config.settings.drone.cycle_s);
            assert_eq!(back.settings.loops, config.settings.loops);
        }
    }

    #[test]
    fn text_is_human_readable() {
        let text = to_text(&SimConfig::default());
        assert!(text.contains("scale=Lydian"));
        assert!(text.contains("seeds=1000,9,1009,2026,168,11"));
        assert!(text.contains("kit=Sub & clicks"));
        assert!(text.contains("layer1=Voices"));
    }

    #[test]
    fn missing_or_empty_is_none() {
        assert!(from_text("").is_none());
        assert!(from_text("# only a comment").is_none());
    }
}
