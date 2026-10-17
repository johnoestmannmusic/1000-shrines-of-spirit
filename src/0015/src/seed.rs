//! `.shrineseed` files: a shrine's settings as human-readable `key=value` text.
//!
//! A seed can be written down, hand-edited, shared and committed. It is a
//! friendlier face of the same data the compiled artifact bakes; importing one
//! simply sets the stored settings.
//!
//! Host-only. The artifact itself carries the recipe as words (`recipe.rs`).

use shrine0015::atmos::LOOP_TIMBRES;
use shrine0015::garage::{BASS_MODES, KEY_LIFTS, MAX_SWING, MIN_SWING, STABS, STAB_TONES};
use shrine0015::harmony::{key_name, Pace, SCALES};
use shrine0015::kits::{KITS, SPACES};
use shrine0015::plants::Biome;
use shrine0015::sim::SimConfig;
use shrine0015::texture::Grid;
use shrine0015::track::{KeyChoice, LoopDesign, Seeds};
use shrine0015::weather::WeatherKind;
use std::fs;
use std::io;
use std::path::Path;

/// `glitch<layer>_<field>`, one line per control. `kinds` is a six-digit 1/0
/// mask in `glitch::KINDS` order; `repeats` is `min,max`.
fn parse_glitch(config: &mut SimConfig, key: &str, value: &str) -> bool {
    let Some(rest) = key.strip_prefix("glitch") else {
        return false;
    };
    let Some((layer, field)) = rest.split_once('_') else {
        return false;
    };
    let Ok(layer) = layer.parse::<usize>() else {
        return false;
    };
    if layer < 1 || layer > config.settings.glitch.len() {
        return false;
    }
    let g = &mut config.settings.glitch[layer - 1];
    match field {
        "enabled" => {
            g.enabled = parse_bool(value);
            true
        }
        "kinds" => {
            let bits: Vec<bool> = value.chars().filter_map(|c| match c {
                '0' => Some(false),
                '1' => Some(true),
                _ => None,
            }).collect();
            if bits.len() != g.kinds.len() {
                return false;
            }
            g.kinds.copy_from_slice(&bits);
            true
        }
        "fill" => value.parse::<u8>().map(|v| g.fill = v.min(100)).is_ok(),
        "rate" => value.parse::<u8>().map(|v| g.rate = v.min(3)).is_ok(),
        "gain" => value.parse::<u8>().map(|v| g.gain = v.min(200)).is_ok(),
        "steps" => value.parse::<u8>().map(|v| g.steps = v.clamp(1, 24)).is_ok(),
        "ratchet" => value.parse::<u8>().map(|v| g.ratchet = v.clamp(1, 8)).is_ok(),
        "repeats" => {
            let nums: Vec<u8> = value.split(',').filter_map(|p| p.trim().parse().ok()).collect();
            if nums.len() != 2 {
                return false;
            }
            let (a, b) = (nums[0].clamp(1, 24), nums[1].clamp(1, 24));
            g.repeats_min = a.min(b);
            g.repeats_max = a.max(b);
            true
        }
        _ => false,
    }
}

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
        "seeds={},{},{},{},{},{},{}\n",
        config.seeds.s1, config.seeds.s2, config.seeds.s3, config.seeds.s4, config.seeds.s5, config.seeds.s6, config.seeds.s7
    ));
    out.push_str("# seeds: 1 keys & chords, 2 glitch 1, 3 glitch 2, 4 bass, 5 drums, 6 atmosphere, 7 drone rhythm\n");
    out.push_str(&format!("biome={}\n", config.biome.name()));
    out.push_str(&format!("weather={}\n", config.weather.name()));
    out.push_str(&format!("scale={}\n", s.scale.name()));
    out.push_str(&format!("key={}\n", key_label(s.key)));
    out.push_str(&format!("chords={}\n", s.chords));
    out.push_str(&format!("pace={}\n", s.pace.name()));
    out.push_str("# 2-step garage only: swing 50-66 %, bass drone|rhythmic, stabs off|sparse|busy, stab tone dark|bright\n");
    out.push_str(&format!("garage_swing={}\n", s.garage.swing));
    out.push_str(&format!("garage_bass={}\n", s.garage.bass.name()));
    out.push_str(&format!("garage_stabs={}\n", s.garage.stabs.name()));
    out.push_str(&format!("garage_stab_tone={}\n", s.garage.tone.name()));
    out.push_str("# progression: seed picks, or one of the named ones; key lift: off|every 4 loops|every 8 loops\n");
    out.push_str(&format!("garage_progression={}\n", s.garage.progression_name()));
    out.push_str(&format!("garage_key_lift={}\n", s.garage.lift.name()));
    out.push_str("# microsound only: pulse off|sub|sub + click, grains sparse|medium|dense, pips off|sparse|busy, dropouts off|some|many,\n#   chord rate slow|every 4 bars|every 2 bars|every bar, drone shift off|octaves|fifths|scale steps\n");
    out.push_str(&format!("micro_pulse={}\n", s.micro.pulse.name()));
    out.push_str(&format!("micro_grains={}\n", s.micro.grains.name()));
    out.push_str(&format!("micro_pips={}\n", s.micro.pips.name()));
    out.push_str(&format!("micro_dropouts={}\n", s.micro.dropouts.name()));
    out.push_str(&format!("micro_chord_rate={}\n", s.micro.chords.name()));
    out.push_str(&format!("micro_drone_shift={}\n", s.micro.shift.name()));
    out.push_str(&format!("kit={}\n", s.kit.name()));
    out.push_str(&format!("space={}\n", s.space.name()));
    out.push_str(&format!("drum_reverse={}\n", if s.drum_reverse { "on" } else { "off" }));
    out.push_str(&format!("master_fx={}\n", if s.master.enabled { "on" } else { "off" }));
    out.push_str(&format!("code_overlay={}\n", if s.code.enabled { "on" } else { "off" }));
    out.push_str(&format!("code_opacity={}\n", s.code.opacity));
    out.push_str(&format!("code_speed={}\n", s.code.speed));
    out.push_str(&format!("code_scale={}\n", s.code.scale));
    out.push_str(&format!("camera_speed={}\n", s.camera_speed));
    out.push_str(&format!("drone_rhythm={}\n", if s.drone_rhythm { "on" } else { "off" }));
    out.push_str(&format!("loop_rhythm={}\n", if s.loop_rhythm { "on" } else { "off" }));
    out.push_str(&format!("wildlife={}\n", if s.wildlife { "on" } else { "off" }));
    out.push_str("# where the piece starts, in seconds into the music (0-600)\n");
    out.push_str(&format!("start_s={}\n", s.start_s));
    out.push_str(&format!("code_plane={}\n", shrine0015::overlay::CodeOverlay::PLANES[(s.code.plane as usize).min(2)]));
    out.push_str(&format!("code_angle={}\n", s.code.angle));
    out.push_str(&format!("code_offset={}\n", s.code.offset));
    out.push_str(&format!("arch_rot_x={}\n", s.arch.rot_x));
    out.push_str(&format!("arch_rot_y={}\n", s.arch.rot_y));
    out.push_str(&format!("arch_rot_z={}\n", s.arch.rot_z));
    out.push_str(&format!("arch_off_x={}\n", s.arch.off_x));
    out.push_str(&format!("arch_off_y={}\n", s.arch.off_y));
    out.push_str(&format!("arch_off_z={}\n", s.arch.off_z));
    // An authored architecture is 256 bytes of nothing-but-source; write it as
    // hex so a shared seed carries the whole shrine.
    if s.model.is_authored() {
        out.push_str("# model: an authored 16x16 architecture (hex: a flag word, then nine bits a cell)\n");
        out.push_str(&format!("model={}\n", s.model.to_hex()));
    }
    for (i, g) in s.glitch.iter().enumerate() {
        let n = i + 1;
        out.push_str(&format!("glitch{n}_enabled={}\n", if g.enabled { "on" } else { "off" }));
        out.push_str(&format!("glitch{n}_kinds={}\n", g.kinds.iter().map(|k| if *k { '1' } else { '0' }).collect::<String>()));
        out.push_str(&format!("glitch{n}_fill={}\n", g.fill));
        out.push_str(&format!("glitch{n}_rate={}\n", g.rate));
        out.push_str(&format!("glitch{n}_gain={}\n", g.gain));
        out.push_str(&format!("glitch{n}_steps={}\n", g.steps));
        out.push_str(&format!("glitch{n}_repeats={},{}\n", g.repeats_min, g.repeats_max));
        out.push_str(&format!("glitch{n}_ratchet={}\n", g.ratchet));
    }
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
                config.settings.name = shrine0015::name::Name::from_str(value);
                found = true;
            }
            "seeds" => {
                let nums: Vec<u64> = value.split(',').filter_map(|p| p.trim().parse().ok()).collect();
                // Six values is the older shape; the seventh is the drone rhythm.
                if nums.len() == 6 || nums.len() == 7 {
                    config.seeds = Seeds {
                        s1: nums[0],
                        s2: nums[1],
                        s3: nums[2],
                        s4: nums[3],
                        s5: nums[4],
                        s6: nums[5],
                        s7: nums.get(6).copied().unwrap_or(config.seeds.s7),
                    };
                    found = true;
                }
            }
            "biome" => {
                if let Some(b) = Biome::from_name(value) {
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
                config.settings.pace = Pace::from_name(value).unwrap_or(Pace::HalfTime);
                found = true;
            }
            "garage_swing" => {
                if let Ok(n) = value.trim_end_matches('%').trim().parse::<u8>() {
                    config.settings.garage.swing = n.clamp(MIN_SWING, MAX_SWING);
                    found = true;
                }
            }
            "garage_bass" => {
                if let Some(b) = BASS_MODES.into_iter().find(|b| eq(b.name(), value)) {
                    config.settings.garage.bass = b;
                    found = true;
                }
            }
            "garage_stabs" => {
                if let Some(b) = STABS.into_iter().find(|b| eq(b.name(), value)) {
                    config.settings.garage.stabs = b;
                    found = true;
                }
            }
            "garage_progression" => {
                if eq(value, "seed picks") || eq(value, "seed") {
                    config.settings.garage.progression = 0;
                    found = true;
                } else if let Some(i) = shrine0015::harmony::PROGRESSIONS.iter().position(|p| eq(p.name, value)) {
                    config.settings.garage.progression = i as u8 + 1;
                    found = true;
                }
            }
            "garage_key_lift" => {
                if let Some(l) = KEY_LIFTS.into_iter().find(|l| eq(l.name(), value)) {
                    config.settings.garage.lift = l;
                    found = true;
                }
            }
            "garage_stab_tone" => {
                if let Some(b) = STAB_TONES.into_iter().find(|b| eq(b.name(), value)) {
                    config.settings.garage.tone = b;
                    found = true;
                }
            }
            "micro_pulse" => {
                if let Some(v) = shrine0015::micro::PULSES.into_iter().find(|v| eq(v.name(), value)) {
                    config.settings.micro.pulse = v;
                    found = true;
                }
            }
            "micro_grains" => {
                if let Some(v) = shrine0015::micro::GRAINS.into_iter().find(|v| eq(v.name(), value)) {
                    config.settings.micro.grains = v;
                    found = true;
                }
            }
            "micro_pips" => {
                if let Some(v) = shrine0015::micro::PIPS.into_iter().find(|v| eq(v.name(), value)) {
                    config.settings.micro.pips = v;
                    found = true;
                }
            }
            "micro_dropouts" => {
                if let Some(v) = shrine0015::micro::DROPOUTS.into_iter().find(|v| eq(v.name(), value)) {
                    config.settings.micro.dropouts = v;
                    found = true;
                }
            }
            "micro_chord_rate" => {
                if let Some(v) = shrine0015::micro::CHORD_RATES.into_iter().find(|v| eq(v.name(), value)) {
                    config.settings.micro.chords = v;
                    found = true;
                }
            }
            "micro_drone_shift" => {
                if let Some(v) = shrine0015::micro::DRONE_SHIFTS.into_iter().find(|v| eq(v.name(), value)) {
                    config.settings.micro.shift = v;
                    found = true;
                }
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
                        shrine0015::overlay::CodeOverlay::MIN_SCALE,
                        shrine0015::overlay::CodeOverlay::MAX_SCALE,
                    );
                    found = true;
                }
            }
            "drone_rhythm" => {
                config.settings.drone_rhythm = parse_bool(value);
                found = true;
            }
            "loop_rhythm" => {
                config.settings.loop_rhythm = parse_bool(value);
                found = true;
            }
            "wildlife" => {
                config.settings.wildlife = parse_bool(value);
                found = true;
            }
            "start_s" => {
                if let Ok(n) = value.trim_end_matches('s').trim().parse::<u16>() {
                    config.settings.start_s = n.min(shrine0015::track::MAX_START_S);
                    found = true;
                }
            }
            "camera_speed" => {
                if let Ok(n) = value.parse::<i16>() {
                    config.settings.camera_speed = n.clamp(-180, 180);
                    found = true;
                }
            }
            "code_plane" => {
                if let Some(i) = shrine0015::overlay::CodeOverlay::PLANES.iter().position(|p| eq(p, value)) {
                    config.settings.code.plane = i as u8;
                    found = true;
                }
            }
            "code_angle" => {
                if let Ok(n) = value.parse::<i16>() {
                    config.settings.code.angle = n.clamp(
                        shrine0015::overlay::CodeOverlay::MIN_ANGLE,
                        shrine0015::overlay::CodeOverlay::MAX_ANGLE,
                    );
                    found = true;
                }
            }
            "model" => match shrine0015::shrine_model::ShrineModel::from_hex(value) {
                Some(m) => {
                    config.settings.model = m;
                    found = true;
                }
                None => {}
            },
            "code_offset" => {
                if let Ok(n) = value.parse::<i16>() {
                    config.settings.code.offset = n.clamp(
                        shrine0015::overlay::CodeOverlay::MIN_OFFSET,
                        shrine0015::overlay::CodeOverlay::MAX_OFFSET,
                    );
                    found = true;
                }
            }
            "arch_rot_x" | "arch_rot_y" | "arch_rot_z" | "arch_off_x" | "arch_off_y" | "arch_off_z" => {
                if let Ok(n) = value.parse::<i16>() {
                    let arch = &mut config.settings.arch;
                    let (lo, hi) = if key.starts_with("arch_rot") {
                        (shrine0015::arch::ArchTransform::MIN_ROT, shrine0015::arch::ArchTransform::MAX_ROT)
                    } else {
                        (shrine0015::arch::ArchTransform::MIN_OFF, shrine0015::arch::ArchTransform::MAX_OFF)
                    };
                    let v = n.clamp(lo, hi);
                    match key.as_str() {
                        "arch_rot_x" => arch.rot_x = v,
                        "arch_rot_y" => arch.rot_y = v,
                        "arch_rot_z" => arch.rot_z = v,
                        "arch_off_x" => arch.off_x = v,
                        "arch_off_y" => arch.off_y = v,
                        _ => arch.off_z = v,
                    }
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
            k if k.starts_with("glitch") => {
                if parse_glitch(&mut config, k, value) {
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
    use shrine0015::atmos::LoopTimbre;
    use shrine0015::track::{DroneArc, KeyChoice};

    fn custom() -> SimConfig {
        let mut c = SimConfig {
            seed: 57306,
            seeds: Seeds { s1: 1, s2: 2, s3: 3, s4: 4, s5: 5, s6: 6, s7: 7 },
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
        assert!(text.contains("seeds=1000,9,1009,2026,168,11,7"));
        assert!(text.contains("kit=Sub & clicks"));
        assert!(text.contains("layer1=Voices"));
    }

    #[test]
    fn missing_or_empty_is_none() {
        assert!(from_text("").is_none());
        assert!(from_text("# only a comment").is_none());
    }

    #[test]
    fn glitch_settings_round_trip_through_text() {
        let mut config = SimConfig::default();
        config.settings.glitch[0].fill = 20;
        config.settings.glitch[0].kinds = [true, false, true, false, true, false];
        config.settings.glitch[0].rate = 3;
        config.settings.glitch[0].gain = 140;
        config.settings.glitch[0].steps = 5;
        config.settings.glitch[0].repeats_min = 2;
        config.settings.glitch[0].repeats_max = 5;
        config.settings.glitch[0].ratchet = 6;
        config.settings.glitch[1].enabled = false;
        let text = to_text(&config);
        assert!(text.contains("glitch1_kinds=101010"), "kinds mask missing");
        let back = from_text(&text).unwrap();
        assert_eq!(back.settings.glitch, config.settings.glitch);
    }

    #[test]
    fn an_authored_architecture_survives_export_and_import() {
        let mut config = SimConfig::default();
        config.settings.model.set(4, 6, 5, 3, 2);
        let text = to_text(&config);
        assert!(text.contains("model="), "the model line is missing");
        let back = from_text(&text).unwrap();
        assert_eq!(back.settings.model, config.settings.model);
        // A procedural shrine writes no model line at all.
        assert!(!to_text(&SimConfig::default()).contains("\nmodel="));
    }

    #[test]
    fn corrupt_glitch_lines_are_ignored_not_fatal() {
        let mut config = SimConfig::default();
        config.settings.glitch[0].fill = 55;
        let text = to_text(&config).replace("glitch1_fill=55", "glitch1_fill=loud\nglitch7_fill=9\nglitch1_kinds=111");
        let back = from_text(&text).unwrap();
        assert_eq!(back.settings.glitch[0].fill, 55, "a bad value must not change anything");
        assert_eq!(back.settings.glitch[0].kinds, [true; 6], "a short mask is ignored");
    }

    // ---------------------------------------------------------------------
    // Every parameter, exported and imported. These build configurations with
    // *every* field set at random within its valid range and compare the
    // whole `Settings` struct after a round trip, so a setting added later
    // without a seed line fails here rather than going missing in a file.
    // ---------------------------------------------------------------------

    use shrine0015::arch::ArchTransform;
    use shrine0015::glitch_setup::GlitchSetup;
    use shrine0015::overlay::CodeOverlay;
    use shrine0015::rng::Rng;
    use shrine0015::shrine_model::ShrineModel;
    use shrine0015::tempo::{MAX_BPM, MIN_BPM};

    fn signed(r: &mut Rng, lo: i64, hi: i64) -> i64 {
        lo + r.int(0, (hi - lo) as u64) as i64
    }

    fn arc(r: &mut Rng) -> DroneArc {
        let cycle = if r.chance(0.3) { 0 } else { r.int(1, 900) as u16 };
        let hold = if cycle == 0 { 0 } else { r.int(0, cycle as u64) as u16 };
        DroneArc { cycle_s: cycle, hold_s: hold }
    }

    /// A configuration with every field chosen at random from its range.
    fn random_config(r: &mut Rng) -> SimConfig {
        let mut c = SimConfig {
            seed: r.next_u64(),
            seeds: Seeds { s1: r.next_u64(), s2: r.next_u64(), s3: r.next_u64(), s4: r.next_u64(), s5: r.next_u64(), s6: r.next_u64(), s7: r.next_u64() },
            biome: r.pick(&Biome::ALL),
            weather: r.pick(&WeatherKind::ALL),
            ..SimConfig::default()
        };
        let s = &mut c.settings;
        s.scale = r.pick(&SCALES);
        s.chords = r.int(1, 4) as u8;
        s.pace = r.pick(&shrine0015::harmony::PACES);
        s.kit = r.pick(&KITS);
        s.space = r.pick(&SPACES);
        s.bpm = r.int(MIN_BPM as u64, MAX_BPM as u64) as u16;
        for i in 0..4 {
            s.loops[i] = if r.chance(0.3) {
                let words = [r.next_u64() as u32, r.next_u64() as u32, r.next_u64() as u32, r.next_u64() as u32];
                LoopDesign::Events(if r.chance(0.5) { Grid::from_words(words) } else { r.pick(&[Grid::fire(), Grid::water(), Grid::stones(), Grid::wood()]) })
            } else {
                LoopDesign::Sustained(r.pick(&LOOP_TIMBRES))
            };
            s.loop_arcs[i] = arc(r);
        }
        s.loop_transpose = signed(r, -12, 12) as i8;
        s.key = if r.chance(0.2) { KeyChoice::Seed } else { KeyChoice::Note(r.int(0, 11) as u8) };
        s.drone = arc(r);
        s.drone_tone = r.int(0, 2_000) as u16;
        s.drum_reverse = r.chance(0.5);
        s.master.enabled = r.chance(0.5);
        s.master.tape = r.chance(0.5);
        s.master.cd = r.chance(0.5);
        s.master.pitch = r.chance(0.5);
        s.master.radio = r.chance(0.5);
        s.master.crush = r.chance(0.5);
        s.master.dropout = r.chance(0.5);
        s.code = CodeOverlay {
            enabled: r.chance(0.5),
            opacity: r.int(0, 100) as u8,
            speed: r.int(0, 100) as u8,
            scale: r.int(CodeOverlay::MIN_SCALE as u64, CodeOverlay::MAX_SCALE as u64) as u8,
            angle: signed(r, CodeOverlay::MIN_ANGLE as i64, CodeOverlay::MAX_ANGLE as i64) as i16,
            offset: signed(r, CodeOverlay::MIN_OFFSET as i64, CodeOverlay::MAX_OFFSET as i64) as i16,
            plane: r.int(0, 2) as u8,
        };
        // Any printable name up to the cap, without spaces at its ends (a seed
        // line is trimmed, so those are not kept).
        let len = r.int(0, 32) as usize;
        let mut name: String = (0..len).map(|_| (b' ' + r.int(0, 94) as u8) as char).collect();
        name = name.trim().to_string();
        s.name = shrine0015::name::Name::from_str(&name);
        let rot = |r: &mut Rng| signed(r, ArchTransform::MIN_ROT as i64, ArchTransform::MAX_ROT as i64) as i16;
        let off = |r: &mut Rng| signed(r, ArchTransform::MIN_OFF as i64, ArchTransform::MAX_OFF as i64) as i16;
        s.arch = ArchTransform { rot_x: rot(r), rot_y: rot(r), rot_z: rot(r), off_x: off(r), off_y: off(r), off_z: off(r) };
        for g in s.glitch.iter_mut() {
            let (a, b) = (r.int(1, 24) as u8, r.int(1, 24) as u8);
            *g = GlitchSetup {
                enabled: r.chance(0.5),
                kinds: [0; 6].map(|_| r.chance(0.5)),
                fill: r.int(0, 100) as u8,
                rate: r.int(0, 3) as u8,
                gain: r.int(0, 200) as u8,
                steps: r.int(1, 24) as u8,
                repeats_min: a.min(b),
                repeats_max: a.max(b),
                ratchet: r.int(1, 8) as u8,
            };
        }
        s.model = ShrineModel::EMPTY;
        if r.chance(0.6) {
            for _ in 0..r.int(1, 60) {
                let (x, y) = (r.int(0, 15) as usize, r.int(0, 15) as usize);
                let h = signed(r, ShrineModel::MIN_HEIGHT as i64, ShrineModel::MAX as i64) as i8;
                s.model.set(x, y, h, r.int(1, 7) as u8, r.int(0, 3) as u8);
            }
        }
        s.camera_speed = signed(r, -180, 180) as i16;
        s.drone_rhythm = r.chance(0.5);
        s.loop_rhythm = r.chance(0.5);
        s.wildlife = r.chance(0.5);
        s.start_s = if r.chance(0.5) { 0 } else { r.int(0, shrine0015::track::MAX_START_S as u64) as u16 };
        s.garage = shrine0015::garage::GarageSetup {
            swing: r.int(MIN_SWING as u64, MAX_SWING as u64) as u8,
            bass: r.pick(&BASS_MODES),
            stabs: r.pick(&STABS),
            tone: r.pick(&STAB_TONES),
            progression: r.int(0, shrine0015::harmony::PROGRESSIONS.len() as u64) as u8,
            lift: r.pick(&KEY_LIFTS),
        };
        s.micro = shrine0015::micro::MicroSetup {
            pulse: r.pick(&shrine0015::micro::PULSES),
            grains: r.pick(&shrine0015::micro::GRAINS),
            pips: r.pick(&shrine0015::micro::PIPS),
            dropouts: r.pick(&shrine0015::micro::DROPOUTS),
            chords: r.pick(&shrine0015::micro::CHORD_RATES),
            shift: r.pick(&shrine0015::micro::DRONE_SHIFTS),
        };
        c
    }

    fn assert_same(back: &SimConfig, config: &SimConfig, label: &str) {
        assert_eq!(back.seed, config.seed, "{label}: seed");
        assert_eq!(back.seeds, config.seeds, "{label}: seeds");
        assert_eq!(back.biome, config.biome, "{label}: biome");
        assert_eq!(back.weather, config.weather, "{label}: weather");
        // The whole struct, so no setting can be left out.
        assert_eq!(back.settings, config.settings, "{label}: settings\n{}", to_text(config));
    }

    #[test]
    fn every_parameter_round_trips_through_a_seed() {
        let mut r = Rng::new(0x5EED_F11E);
        for i in 0..400 {
            let config = random_config(&mut r);
            let back = from_text(&to_text(&config)).expect("should parse");
            assert_same(&back, &config, &format!("config {i}"));
        }
    }

    #[test]
    fn every_weather_biome_and_loop_timbre_round_trips() {
        for weather in WeatherKind::ALL {
            for biome in Biome::ALL {
                let mut c = SimConfig { weather, biome, ..SimConfig::default() };
                c.settings.wildlife = true;
                let back = from_text(&to_text(&c)).unwrap();
                assert_eq!((back.weather, back.biome), (weather, biome));
                assert!(back.settings.wildlife);
            }
        }
        for t in LOOP_TIMBRES {
            let mut c = SimConfig::default();
            c.settings.loops = [LoopDesign::Sustained(t); 4];
            assert_eq!(from_text(&to_text(&c)).unwrap().settings.loops, c.settings.loops, "{}", t.name());
        }
    }

    #[test]
    fn every_line_a_seed_writes_is_one_it_can_read() {
        // A key the writer emits but the reader ignores would silently lose
        // that setting on import. Each line must be recognised on its own.
        let mut r = Rng::new(7);
        for _ in 0..40 {
            let config = random_config(&mut r);
            for line in to_text(&config).lines() {
                if line.trim().is_empty() || line.starts_with('#') {
                    continue;
                }
                assert!(from_text(line).is_some(), "the reader does not recognise: {line}");
            }
        }
    }

    #[test]
    fn a_seed_from_before_nine_bit_cells_keeps_its_architecture() {
        // Earlier seeds wrote the model as one byte a cell: height 0..6, kind
        // and variation (the version-13 layout), 512 hex digits.
        let mut bytes = [0u8; 256];
        bytes[4 + 6 * 16] = 5 | (3 << 3) | (2 << 6); // (4, 6): height 5, kind 3, variant 2
        bytes[9 + 9 * 16] = 0 | (1 << 3); // (9, 9): ground
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let back = from_text(&format!("seed=5\nmodel={hex}\n")).unwrap();
        let m = back.settings.model;
        assert!(m.is_authored(), "an old model must stay authored");
        assert_eq!((m.height(4, 6), m.kind(4, 6), m.variant(4, 6)), (5, 3, 2));
        assert_eq!(m.height(9, 9), 0);
        // And it writes back out in today's form, which reads back the same.
        let again = from_text(&to_text(&back)).unwrap();
        assert_eq!(again.settings.model, m);
    }

    #[test]
    fn export_and_import_through_a_file() {
        let mut r = Rng::new(99);
        let config = random_config(&mut r);
        let dir = std::env::temp_dir().join(format!("shrineseed-test-{}", std::process::id()));
        let path = dir.join("nested").join("ShrineGen-test.shrineseed");
        let path = path.to_str().unwrap();
        write(&config, path).expect("export");
        let back = read(path).expect("import");
        assert_same(&back, &config, "file");
        let _ = fs::remove_dir_all(&dir);
        assert!(read("/nonexistent/x.shrineseed").is_err());
    }
}

