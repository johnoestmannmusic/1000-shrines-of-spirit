//! `--emit`: write the single, self-contained, human-readable artifact.
//!
//! The output is one `.rs` file: the engine's modules inlined under `pub mod`,
//! plus this shrine's recipe baked in and a tiny C ABI. It compiles with bare
//! `rustc` to a WASM module — no Cargo, no registry, no network (`HC002`,
//! `HC006`). Nothing from the host (terminal, server, PNG/WAV writers) is
//! included, so the artifact is exactly the piece.

use shrine_explorer::recipe;
use shrine_explorer::sim::SimConfig;
use shrine_explorer::track::LoopDesign;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Emit the artifact for `config` to `out`.
pub fn emit(config: &SimConfig, out: &str) -> io::Result<()> {
    let root = PathBuf::from("engine/src");
    if !root.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "run from the repository root (engine/src not found)",
        ));
    }

    let mut a = String::new();
    a.push_str(&header(config));

    // lib.rs, minus its module declarations (those become inline modules below)
    // and minus the wasm module (the artifact generates its own baked ABI).
    let lib = fs::read_to_string(root.join("lib.rs"))?;
    for line in lib.lines() {
        let t = line.trim_start();
        if t.starts_with("pub mod ") && t.ends_with(';') {
            continue;
        }
        a.push_str(line);
        a.push('\n');
    }

    // Root modules, sorted so the output is reproducible.
    a.push_str("\n// ============================================================================\n// Engine modules\n// ============================================================================\n");
    for name in module_names(&root, &["lib", "wasm", "mod"])? {
        let body = strip_tests(&fs::read_to_string(root.join(format!("{name}.rs")))?);
        a.push_str(&format!("\n// ---- src/{name}.rs ----\npub mod {name} {{\n{body}\n}}\n"));
    }

    // The music engine, nested under `audio`.
    let audio_dir = root.join("audio");
    a.push_str("\n// ============================================================================\n// Music engine modules (src/audio/)\n// ============================================================================\n");
    a.push_str("pub mod audio {\n");
    for name in module_names(&audio_dir, &["mod", "wasm"])? {
        let body = strip_tests(&fs::read_to_string(audio_dir.join(format!("{name}.rs")))?);
        a.push_str(&format!("// ---- src/audio/{name}.rs ----\npub mod {name} {{\n{body}\n}}\n"));
    }
    a.push_str("pub use self::track::*;\n}\n");

    // The baked recipe.
    let words = recipe::encode(config);
    a.push_str(
        "\n// ============================================================================\n\
         // Baked recipe — this artifact is this shrine\n\
         // ============================================================================\n",
    );
    let list = words.iter().map(|w| w.to_string()).collect::<Vec<_>>().join(", ");
    a.push_str(&format!("/// `recipe::encode` of the settings the setup menu produced.\nconst RECIPE: [u32; {}] = [{list}];\n", words.len()));

    // Build stamp and the human-readable card shown beside the piece.
    let build: u32 = env!("BUILD_DATE").parse().unwrap_or(0);
    a.push_str(&format!("\n/// Build date as `YYYYMMDD`, stamped when this file was emitted.\nconst BUILD: u32 = {build};\n"));
    a.push_str(&format!(
        "\n/// The source-code overlay word: bit 0 enabled, byte 1 opacity, byte 2 speed, byte 3 text scale.\nconst CODE_OVERLAY: u32 = {};\n",
        config.settings.code.word()
    ));
    a.push_str(&format!(
        "\n/// The overlay's tilt: positive leans the drift to the left.\nconst CODE_ANGLE: i32 = {};\n",
        config.settings.code.angle
    ));
    a.push_str(&format!(
        "\n/// The overlay's vertical shift, percent of the canvas height.\nconst CODE_OFFSET: i32 = {};\n",
        config.settings.code.offset
    ));
    a.push_str(&format!(
        "\n/// Camera rotation speed, degrees per second.\nconst CAMERA_SPEED: i32 = {};\n",
        config.settings.camera_speed
    ));
    a.push_str(&format!(
        "\n/// The track name, shown at the head of the page and used as the WAV title.\nconst NAME: &str = {:?};\n",
        config.settings.name.as_str()
    ));
    a.push_str(&format!("\n/// Everything needed to recreate this shrine.\nconst INFO: &str = {:?};\n", info(config, build)));

    // The C ABI.
    a.push_str(ABI);

    let path = Path::new(out);
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            fs::create_dir_all(dir)?;
        }
    }
    fs::write(path, a)?;
    Ok(())
}

/// Copy the emitted artifact somewhere the page can fetch it (e.g.
/// `web/shrine.rs`), so the source-code overlay has something to scroll.
pub fn publish_source(from: &str, dest: &str) -> io::Result<()> {
    if let Some(dir) = Path::new(dest).parent() {
        if !dir.as_os_str().is_empty() {
            fs::create_dir_all(dir)?;
        }
    }
    fs::copy(from, dest).map(|_| ())
}

fn module_names(dir: &Path, skip: &[&str]) -> io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        if skip.contains(&name.as_str()) {
            continue;
        }
        names.push(name);
    }
    names.sort();
    Ok(names)
}

/// Remove `#[cfg(test)] mod tests { … }` blocks: the artifact ships the piece,
/// not its tests.
fn strip_tests(src: &str) -> String {
    const MARK: &str = "#[cfg(test)]";
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < src.len() {
        if src[i..].starts_with(MARK) {
            match src[i..].find('{') {
                Some(open_rel) => {
                    let open = i + open_rel;
                    let mut depth = 0i32;
                    let mut end = None;
                    for (k, c) in src[open..].char_indices() {
                        match c {
                            '{' => depth += 1,
                            '}' => {
                                depth -= 1;
                                if depth == 0 {
                                    end = Some(open + k + 1);
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    if let Some(end) = end {
                        i = end;
                        continue;
                    }
                }
                None => {}
            }
        }
        let ch = src[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn header(config: &SimConfig) -> String {
    let s = &config.settings;    format!(
        "// ============================================================================\n\
         // ShrineGen — a procedural isometric shrine.\n\
         //\n\
         // GENERATED ARTIFACT — emitted by `ShrineGen --emit`. Do not edit by hand;\n\
         // change the setup and re-emit. This file is the whole piece: the isometric\n\
         // framework, the shrine, the plants, the weather, the music engine, and the\n\
         // recipe below. It is standard-library only and has no external dependencies.\n\
         //\n\
         // Build it with nothing but rustc:\n\
         //\n\
         //   rustc --edition 2021 --target wasm32-unknown-unknown -O \\\n\
         //         --crate-type cdylib {file} -o shrine.wasm\n\
         //\n\
         // The shrine it draws and plays:\n\
         //   seed {seed} · {biome} · {weather}\n\
         //   {scale}, {chords} chord(s), {pace}, {bpm} BPM, root {key}, kit {kit}\n\
         //   drone arc {drone_s}s up {drone_hold}s, tone {drone_tone}\n\
         //\n\
         // Contents: crate docs and constants, then engine modules (iso, grid, render,\n\
         // audio/…, sim), then the baked recipe and the C ABI. Read top to bottom.\n\
         // ============================================================================\n\n",
        file = "shrine.rs",
        seed = config.seed,
        biome = config.biome.name(),
        weather = config.weather.name(),
        scale = s.scale.name(),
        chords = s.chords,
        pace = s.pace.name(),
        bpm = s.bpm,
        key = match s.key {
            shrine_explorer::track::KeyChoice::Seed => "seed picks".to_string(),
            shrine_explorer::track::KeyChoice::Note(n) => shrine_explorer::harmony::key_name(n).to_string(),
        },
        kit = s.kit.name(),
        drone_s = s.drone.cycle_s,
        drone_hold = s.drone.hold_s,
        drone_tone = s.drone_tone,
    )
}

/// The build date stamped by `build.rs`, as `YYYYMMDD`.
pub fn build_date() -> u32 {
    env!("BUILD_DATE").parse().unwrap_or(0)
}

/// The human-readable card baked into the artifact: enough for anyone to
/// recreate the shrine by hand.
pub fn info(config: &SimConfig, build: u32) -> String {
    let s = &config.settings;
    let key = match s.key {
        shrine_explorer::track::KeyChoice::Seed => "seed picks".to_string(),
        shrine_explorer::track::KeyChoice::Note(n) => shrine_explorer::harmony::key_name(n).to_string(),
    };
    let fx = if !s.master.enabled {
        "off".to_string()
    } else {
        let on: Vec<&str> = s.master.labels().iter().filter(|(_, on)| *on).map(|(name, _)| *name).collect();
        if on.is_empty() { "on · none".to_string() } else { on.join(", ") }
    };
    let mut lines = vec![
        format!("ShrineGen  v{build}"),
        String::new(),
        format!("seed       {}", config.seed),
        format!("biome      {}", config.biome.name()),
        format!("weather    {}", config.weather.name()),
        format!(
            "wildlife   {}",
            if config.settings.wildlife {
                shrine_explorer::wildlife::Wildlife::new(config.seed, config.biome).summary()
            } else {
                "off".to_string()
            }
        ),
        String::new(),
        format!("scale      {}", s.scale.name()),
        format!("root note  {key}"),
        format!("chords     {}", s.chords),
        format!(
            "mode       {}",
            match s.pace {
                shrine_explorer::harmony::Pace::Garage => {
                    format!("{} · {} · key lift {}", s.pace.name(), s.garage.progression_name(), s.garage.lift.name())
                }
                shrine_explorer::harmony::Pace::Micro => format!(
                    "{} · pulse {} · grains {} · pips {} · dropouts {} · chords {} · drone shift {}",
                    s.pace.name(),
                    s.micro.pulse.name(),
                    s.micro.grains.name(),
                    s.micro.pips.name(),
                    s.micro.dropouts.name(),
                    s.micro.chords.name(),
                    s.micro.shift.name()
                ),
                _ => s.pace.name().to_string(),
            }
        ),
        format!("tempo      {} BPM", s.bpm),
        format!("start      {}", if s.start_s == 0 { "at the beginning".to_string() } else { format!("{} s in", s.start_s) }),
        format!("map        {}", if s.map == (0, 0) { "home · 0, 0".to_string() } else { format!("{}, {} from home", s.map.0, s.map.1) }),
        format!("kit        {}", s.kit.name()),
        format!("space      {}", s.space.name()),
        format!("drum rev   {}", if s.drum_reverse { "on" } else { "off" }),
        format!("master fx  {fx}"),
        format!("code over  {}", s.code.summary()),
        format!("arch       {}", s.arch.summary()),
        format!(
            "camera     {}",
            if s.camera_speed == 0 { "still".to_string() } else { format!("{:+}°/s", s.camera_speed) }
        ),
        format!("glitch 1   {}", s.glitch[0].summary()),
        format!("glitch 2   {}", s.glitch[1].summary()),
        format!("transpose  {} semitones", s.loop_transpose),
        String::new(),
        format!("drone arc  {}", arc(s.drone)),
        format!("drone tone {}", tone(s.drone_tone)),
    ];
    for i in 0..4 {
        lines.push(format!("layer {}    {}", i + 1, layer(s.loops[i])));
        lines.push(format!("  arc {}    {}", i + 1, arc(s.loop_arcs[i])));
    }
    lines.push(String::new());
    lines.push(format!(
        "seeds      {} {} {} {} {} {} {}",
        config.seeds.s1,
        config.seeds.s2,
        config.seeds.s3,
        config.seeds.s4,
        config.seeds.s5,
        config.seeds.s6,
        config.seeds.s7
    ));
    lines.push("           keys&chords · glitch1 · glitch2 · bass · drums · atmosphere · drone rhythm".to_string());
    lines.push(format!(
        "rhythm     drone {} · loops {}",
        if config.settings.drone_rhythm { "on" } else { "off" },
        if config.settings.loop_rhythm { "on" } else { "off" }
    ));
    lines.push(String::new());
    lines.push("Music License: CC0 / Public Domain".to_string());
    lines.push("Code License: MIT".to_string());
    lines.join("\n")
}

/// The track name, falling back to the product name when none was chosen.
pub fn title(config: &SimConfig) -> String {
    if config.settings.name.is_empty() {
        "ShrineGen".to_string()
    } else {
        config.settings.name.as_str().to_string()
    }
}

fn arc(a: shrine_explorer::track::DroneArc) -> String {
    if a.cycle_s == 0 {
        "always on".to_string()
    } else {
        format!("{}s cycle, {}s up", a.cycle_s, a.hold_s)
    }
}

fn tone(hz: u16) -> String {
    if hz == 0 {
        "off".to_string()
    } else {
        format!("{hz} Hz low-pass")
    }
}

fn layer(d: LoopDesign) -> String {
    match d {
        LoopDesign::Sustained(t) => t.name().to_string(),
        LoopDesign::Events(_) => "event grid".to_string(),
    }
}

/// The baked C ABI. It deliberately matches the engine's signature (`words`,
/// `len`, `mode`) so the same page works against either module, but ignores the
/// caller's words and uses the baked `RECIPE`.
const ABI: &str = r#"
// ============================================================================
// C ABI — plain extern "C", loadable with the standard WebAssembly API
// ============================================================================

/// Build date as `YYYYMMDD` (e.g. 20261011).
#[no_mangle]
pub extern "C" fn shrine_build_date() -> u32 {
    BUILD
}

/// The source-code overlay word: bit 0 enabled, byte 1 opacity, byte 2 speed.
#[no_mangle]
pub extern "C" fn shrine_code_overlay() -> u32 {
    CODE_OVERLAY
}

/// The overlay's tilt in degrees (positive leans the drift to the left).
#[no_mangle]
pub extern "C" fn shrine_code_angle() -> i32 {
    CODE_ANGLE
}

/// The overlay's vertical shift, percent of the canvas height.
#[no_mangle]
pub extern "C" fn shrine_code_offset() -> i32 {
    CODE_OFFSET
}

/// The camera rotation speed in degrees per second, so the page can orbit its
/// own sky in step with the engine.
#[no_mangle]
pub extern "C" fn shrine_camera_speed() -> i32 {
    CAMERA_SPEED
}

/// Pointer to the UTF-8 info card (`shrine_info_len` bytes).
#[no_mangle]
pub extern "C" fn shrine_info_ptr() -> *const u8 {
    INFO.as_ptr()
}

/// Length of the info card in bytes.
#[no_mangle]
pub extern "C" fn shrine_info_len() -> u32 {
    INFO.len() as u32
}

/// Pointer to the UTF-8 track name (`shrine_name_len` bytes).
#[no_mangle]
pub extern "C" fn shrine_name_ptr() -> *const u8 {
    NAME.as_ptr()
}

/// Length of the track name in bytes (0 when none was chosen).
#[no_mangle]
pub extern "C" fn shrine_name_len() -> u32 {
    NAME.len() as u32
}

/// Nudge the camera from the page's pointer, in `[-1, 1]` per axis.
///
/// # Safety
/// `player` must come from `shrine_new`.
#[no_mangle]
pub unsafe extern "C" fn shrine_set_camera(player: *mut crate::player::Player, yaw: f64, pitch: f64) {
    let p = unsafe { &mut *player };
    p.set_camera(yaw, pitch);
}

#[no_mangle]
pub unsafe extern "C" fn shrine_set_sky_transparent(player: *mut crate::player::Player, on: u32) {
    let p = unsafe { &mut *player };
    p.set_sky_transparent(on != 0);
}

#[no_mangle]
pub unsafe extern "C" fn shrine_sky_colors(player: *mut crate::player::Player) -> u64 {
    let p = unsafe { &*player };
    p.sky_colors()
}

/// Build a player. `mode`: 1 = audio, 2 = visuals. The caller's words are
/// ignored: this artifact always uses its baked `RECIPE`.
///
/// # Safety
/// Returns an owned pointer to be released with `shrine_free`.
#[no_mangle]
pub unsafe extern "C" fn shrine_new(_words: *const u32, _words_len: u32, mode: u32) -> *mut crate::player::Player {
    let config = crate::recipe::decode(&RECIPE);
    let sim = if mode == 2 { crate::sim::Sim::new_visual(config) } else { crate::sim::Sim::new(config) };
    Box::into_raw(Box::new(crate::player::Player::new(sim)))
}

/// Render `frames` stereo samples; the pointer is valid until the next call.
///
/// # Safety
/// `player` must come from `shrine_new`.
#[no_mangle]
pub unsafe extern "C" fn shrine_render_audio(player: *mut crate::player::Player, frames: u32) -> *const f32 {
    let p = unsafe { &mut *player };
    p.render_audio(frames as usize).as_ptr()
}

/// Render one 300x300 RGBA frame at `sample_clock`; valid until the next call.
///
/// # Safety
/// `player` must come from `shrine_new`.
#[no_mangle]
pub unsafe extern "C" fn shrine_render_visual(player: *mut crate::player::Player, sample_clock: u64) -> *const u8 {
    let p = unsafe { &mut *player };
    p.render_visual(sample_clock).as_ptr()
}

/// The Learn page's description of this shrine: UTF-8 JSON, `shrine_learn_len` bytes.
///
/// # Safety
/// `player` must come from `shrine_new`.
#[no_mangle]
pub unsafe extern "C" fn shrine_learn_ptr(player: *mut crate::player::Player) -> *const u8 {
    let p = unsafe { &mut *player };
    p.learn_json().as_ptr()
}

/// Length of the description in bytes.
///
/// # Safety
/// `player` must come from `shrine_new`.
#[no_mangle]
pub unsafe extern "C" fn shrine_learn_len(player: *mut crate::player::Player) -> u32 {
    let p = unsafe { &mut *player };
    p.learn_json().len() as u32
}

/// The music's moving parts now: `shrine_learn_state_len` f64s (layout in
/// `learn::STATE_LEN`). Display only.
///
/// # Safety
/// `player` must come from `shrine_new`.
#[no_mangle]
pub unsafe extern "C" fn shrine_learn_state(player: *mut crate::player::Player) -> *const f64 {
    let p = unsafe { &mut *player };
    p.learn_state().as_ptr()
}

#[no_mangle]
pub extern "C" fn shrine_learn_state_len() -> u32 {
    crate::learn::STATE_LEN as u32
}

/// The weather at `sample_clock`: 6 f64s (wind, gust, rain, snow, fog, light).
///
/// # Safety
/// `player` must come from `shrine_new`.
#[no_mangle]
pub unsafe extern "C" fn shrine_learn_weather(player: *mut crate::player::Player, sample_clock: u64) -> *const f64 {
    let p = unsafe { &mut *player };
    p.weather_now(sample_clock).as_ptr()
}

/// Where grid cell `(gx, gy)`'s top surface is on screen at `sample_clock`:
/// 3 f64s (x, y in logical pixels, its level); x = −1 off the grid.
///
/// # Safety
/// `player` must come from `shrine_new`.
#[no_mangle]
pub unsafe extern "C" fn shrine_surface_point(player: *mut crate::player::Player, gx: i32, gy: i32, sample_clock: u64) -> *const f64 {
    let p = unsafe { &mut *player };
    p.surface_point(gx, gy, sample_clock).as_ptr()
}

/// Where side `side` (0 +x, 1 −x, 2 +y, 3 −y) of the shrine is on screen at
/// `sample_clock`: 4 f64s (x, y in logical pixels, then the outward direction).
///
/// # Safety
/// `player` must come from `shrine_new`.
#[no_mangle]
pub unsafe extern "C" fn shrine_side_point(player: *mut crate::player::Player, side: u32, sample_clock: u64) -> *const f64 {
    let p = unsafe { &mut *player };
    p.side_point(side, sample_clock).as_ptr()
}

/// A player for the shrine at `(x, y)` on this artifact's map: its baked
/// home, at those coordinates. `mode`: 1 = audio, 2 = visuals.
#[no_mangle]
pub extern "C" fn shrine_new_at(x: i32, y: i32, mode: u32) -> *mut crate::player::Player {
    let mut config = crate::recipe::decode(&RECIPE);
    config.settings.map = (x, y);
    let sim = if mode == 2 { crate::sim::Sim::new_visual(config) } else { crate::sim::Sim::new(config) };
    Box::into_raw(Box::new(crate::player::Player::new(sim)))
}

/// Where on the map this artifact starts (the setup's Map position).
#[no_mangle]
pub extern "C" fn shrine_map_x() -> i32 {
    crate::recipe::decode(&RECIPE).settings.map.0
}

#[no_mangle]
pub extern "C" fn shrine_map_y() -> i32 {
    crate::recipe::decode(&RECIPE).settings.map.1
}

/// The spot `(x, y)` of this artifact's map at a glance (see `explore::glance`).
#[no_mangle]
pub extern "C" fn shrine_map_glance(x: i32, y: i32) -> u32 {
    crate::explore::glance(&crate::recipe::decode(&RECIPE), x, y)
}

/// Hear one layer alone (1 drone, 2–3 glitch layers, 4 bass, 5 drums, 6–9
/// atmosphere loops, 10 the echo and reverb), or 0 for the full mix.
///
/// # Safety
/// `player` must come from `shrine_new`.
#[no_mangle]
pub unsafe extern "C" fn shrine_set_solo(player: *mut crate::player::Player, layer: u32) {
    let p = unsafe { &mut *player };
    p.set_solo(layer);
}

/// Release a player.
///
/// # Safety
/// `player` must come from `shrine_new` and not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn shrine_free(player: *mut crate::player::Player) {
    if !player.is_null() {
        unsafe { drop(Box::from_raw(player)) };
    }
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_test_blocks() {
        let src = "fn a() {}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn x() {}\n}\nfn b() {}\n";
        let out = strip_tests(src);
        assert!(out.contains("fn a()"));
        assert!(out.contains("fn b()"));
        assert!(!out.contains("mod tests"));
    }

    #[test]
    fn emits_a_self_contained_file() {
        let path = std::env::temp_dir().join("shrine-emit-test.rs");
        let path = path.to_str().unwrap();
        emit(&SimConfig::default(), path).unwrap();
        let text = std::fs::read_to_string(path).unwrap();
        assert!(text.contains("pub mod iso {"), "missing iso module");
        assert!(text.contains("pub mod audio {"), "missing audio module");
        assert!(text.contains("const RECIPE:"), "missing baked recipe");
        assert!(text.contains("shrine_render_visual"), "missing C ABI");
        assert!(text.contains("shrine_set_sky_transparent"), "missing sky toggle");
        assert!(text.contains("shrine_sky_colors"), "missing sky colours");
        assert!(!text.contains("#[cfg(test)]"), "tests were not stripped");
    }

    #[test]
    fn the_track_name_and_licences_are_baked() {
        let mut config = SimConfig::default();
        config.settings.name = shrine_explorer::name::Name::from_str("Dusk Conservatory");
        config.settings.code.scale = 250;
        config.settings.code.angle = -25;
        config.settings.code.offset = 40;
        let path = std::env::temp_dir().join("shrine-emit-name.rs");
        let path = path.to_str().unwrap();
        emit(&config, path).unwrap();
        let text = std::fs::read_to_string(path).unwrap();
        assert!(text.contains("const NAME: &str = \"Dusk Conservatory\";"), "name not baked");
        assert!(text.contains("shrine_name_ptr"), "name ABI missing");
        assert!(text.contains("ShrineGen  v"), "the info card should keep the product name");
        assert!(text.contains("text 250%"), "the info card lacks the overlay scale");
        assert!(text.contains("shrine_code_angle"), "the tilt ABI is missing");
        assert!(text.contains("const CODE_ANGLE: i32 = -25;"), "the tilt was not baked");
        assert!(text.contains("const CODE_OFFSET: i32 = 40;"), "the vertical shift was not baked");
        assert!(text.contains("shrine_code_offset"), "the vertical-shift ABI is missing");
        assert!(text.contains("Music License: CC0 / Public Domain"), "music licence missing");
        assert!(text.contains("Code License: MIT"), "code licence missing");
    }
}
