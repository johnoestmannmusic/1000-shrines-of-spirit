//! The interactive setup menu: choose every part of a shrine, watch it live,
//! save a still or a WAV. Answers are remembered between runs.
//!
//! Host-only. The settings it produces are exactly the recipe the artifact
//! bakes (`recipe.rs`), so what you see here is what ships.

use crate::{audition::Audition, emit, preview, seed, state, wav};
use shrine0012::arch::ArchTransform;
use shrine0012::master::MasterFx;
use shrine0012::overlay::CodeOverlay;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::{DefaultTerminal, Frame};
use shrine0012::atmos::LOOP_TIMBRES;
use shrine0012::glitch::KINDS;
use shrine0012::shrine::kind as tile_kind;
use shrine0012::shrine_model::ShrineModel;
use shrine0012::glitch_setup::{GlitchSetup, RATES};
use shrine0012::framebuffer::Framebuffer;
use shrine0012::harmony::{key_name, Pace, SCALES};
use shrine0012::kits::{KITS, SPACES};
use shrine0012::plants::Biome;
use shrine0012::scene::Quality;
use shrine0012::sim::{Sim, SimConfig};
use shrine0012::texture::Grid;
use shrine0012::track::{DroneArc, KeyChoice, LoopDesign};
use shrine0012::weather::WeatherKind;
use shrine0012::SAMPLE_RATE;
use std::process::Command;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

const EVENT_GRID: usize = LOOP_TIMBRES.len(); // the extra layer option
const ARCS: [u16; 7] = [0, 30, 60, 120, 180, 300, 600];
const TONES: [u16; 8] = [0, 200, 400, 800, 1500, 3000, 6000, 12000];
const WAV_SECONDS: usize = 20;
const COVER_SIZE: u32 = 1600;
/// What each of the six music seeds shapes.
const SEED_ROLES: [&str; 7] =
    ["keys & chords", "glitch 1", "glitch 2", "bass", "drums", "atmosphere", "drone rhythm"];
const ANIM_MS: u64 = 40;

type Rgb = (u8, u8, u8);
const TEXT: Rgb = (206, 204, 209);
const DIM: Rgb = (140, 137, 148);
const FAINT: Rgb = (70, 67, 78);
const ACCENT: Rgb = (150, 220, 180);
const SELECT: Rgb = (245, 205, 120);

fn color(c: Rgb) -> Color {
    Color::Rgb(c.0, c.1, c.2)
}
fn style(c: Rgb) -> Style {
    Style::new().fg(color(c))
}
fn put(buf: &mut Buffer, area: Rect, x: u16, y: u16, s: &str, st: Style) {
    if x < area.width && y < area.height {
        buf.set_stringn(area.x + x, area.y + y, s, (area.width - x) as usize, st);
    }
}
fn put_char(buf: &mut Buffer, area: Rect, x: u16, y: u16, ch: char, st: Style) {
    if x < area.width && y < area.height {
        if let Some(cell) = buf.cell_mut((area.x + x, area.y + y)) {
            cell.set_char(ch).set_style(st);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Header,
    Seed,
    Biome,
    Weather,
    Audition,
    Scale,
    Key,
    Chords,
    Pace,
    Bpm,
    Kit,
    Space,
    DrumReverse,
    OpenFx,
    Layer(usize),
    LayerArc(usize),
    LayerHold(usize),
    Transpose,
    DroneArc,
    DroneHold,
    DroneTone,
    DroneRhythm,
    LoopRhythm,
    CodeEnabled,
    CodeOpacity,
    CodeSpeed,
    CodeScale,
    CodeAngle,
    CodeOffset,
    CodePlane,
    CameraSpeed,
    ArchRotX,
    ArchRotY,
    ArchRotZ,
    ArchOffX,
    ArchOffY,
    ArchOffZ,
    Name,
    SeedN(usize),
    SavePng,
    CoverPng,
    WavExport,
    ExportSeed,
    ImportSeed,
    Reset,
    Glitch(usize),
    EditArch,
    Compile,
    Quit,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PromptAction {
    ImportSeed,
    ExportSeed,
    Name,
    /// Type an exact value for a numeric row.
    Number(Kind),
    /// Type an exact value on a glitch layer screen.
    GlitchNumber(GlitchRow),
    /// Type an exact height for the architecture editor's cursor cell.
    EditorHeight,
}

struct Prompt {
    label: String,
    value: String,
    action: PromptAction,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Screen {
    Main,
    Wav,
    Fx,
    Glitch,
    Editor,
}

/// The master-effects toggles live in the settings; this only tracks the cursor.
struct FxForm {
    sel: usize,
}

impl FxForm {
    fn new() -> Self {
        FxForm { sel: 0 }
    }

    fn rows(&self, master: MasterFx) -> Vec<(String, String)> {
        let onoff = |on: bool| if on { "on".to_string() } else { "off".to_string() };
        let mut rows = vec![("Master FX".to_string(), onoff(master.enabled))];
        for (name, on) in master.labels() {
            rows.push((name.to_string(), onoff(on)));
        }
        rows.push(("Back".to_string(), String::new()));
        rows
    }
}

/// Which row the menu window starts at: it keeps [`AHEAD`] rows visible below
/// the selection, so you can see what is coming instead of chasing the bottom.
const AHEAD: usize = 8;

fn menu_scroll(sel: usize, rows: usize, list_h: usize) -> usize {
    let lead = list_h.saturating_sub(1).saturating_sub(AHEAD);
    sel.saturating_sub(lead).min(rows.saturating_sub(list_h))
}

fn fx_summary(master: MasterFx) -> String {
    if !master.enabled {
        return "off".to_string();
    }
    let on: Vec<&str> = master.labels().iter().filter(|(_, on)| *on).map(|(name, _)| *name).collect();
    if on.is_empty() {
        "on · none".to_string()
    } else {
        on.join(", ")
    }
}

struct WavForm {
    length_s: f64,
    fade_s: f64,
    filename: String,
    artist: String,
    album: String,
    normalize: bool,
    sel: usize,
    editing: Option<String>,
}

impl WavForm {
    fn new(seed: u64) -> Self {
        WavForm {
            length_s: WAV_SECONDS as f64,
            fade_s: 3.0,
            filename: format!("previews/ShrineGen-{seed}.wav"),
            artist: String::new(),
            album: "ShrineGen".to_string(),
            normalize: false,
            sel: 0,
            editing: None,
        }
    }

    fn rows(&self, title: &str) -> Vec<(String, String)> {
        vec![
            ("Export length".to_string(), format!("{:.1} s", self.length_s)),
            ("Fade out".to_string(), format!("{:.1} s", self.fade_s)),
            ("Filename".to_string(), self.filename.clone()),
            ("Track name".to_string(), title.to_string()),
            ("Artist".to_string(), self.artist.clone()),
            ("Album".to_string(), self.album.clone()),
            ("Normalize".to_string(), if self.normalize { "on".to_string() } else { "off".to_string() }),
            ("▶ Export".to_string(), String::new()),
            ("Back".to_string(), String::new()),
        ]
    }
}

fn parse_seconds(text: &str) -> Option<f64> {
    let mut total = 0.0;
    for part in text.split(':') {
        let value: f64 = part.trim().parse().ok()?;
        if value < 0.0 {
            return None;
        }
        total = total * 60.0 + value;
    }
    Some(total)
}

/// The rows of a glitch layer screen, in order: what plays, then how often.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum GlitchRow {
    Enabled,
    Kind(usize),
    Fill,
    Rate,
    Gain,
    Steps,
    RepeatsMin,
    RepeatsMax,
    Ratchet,
    Back,
}

const GLITCH_ROWS: [GlitchRow; 15] = [
    GlitchRow::Enabled,
    GlitchRow::Kind(0),
    GlitchRow::Kind(1),
    GlitchRow::Kind(2),
    GlitchRow::Kind(3),
    GlitchRow::Kind(4),
    GlitchRow::Kind(5),
    GlitchRow::Fill,
    GlitchRow::Rate,
    GlitchRow::Gain,
    GlitchRow::Steps,
    GlitchRow::RepeatsMin,
    GlitchRow::RepeatsMax,
    GlitchRow::Ratchet,
    GlitchRow::Back,
];

/// The architecture editor: where the cursor is, the working model, and a
/// small undo stack. The working copy starts from whatever the seed produced,
/// so editing continues from the generated shrine.
struct EditForm {
    cx: usize,
    cy: usize,
    /// What the seed's generator produces: the model returns to procedural
    /// whenever the working copy matches it, so the recipe stays lean.
    baseline: ShrineModel,
    working: ShrineModel,
    undo: Vec<ShrineModel>,
}

impl EditForm {
    fn new() -> Self {
        EditForm { cx: 8, cy: 8, baseline: ShrineModel::EMPTY, working: ShrineModel::EMPTY, undo: Vec::new() }
    }
}

/// Which glitch layer the screen is editing, and where the cursor is.
struct GlitchForm {
    layer: usize,
    sel: usize,
}

impl GlitchForm {
    fn new() -> Self {
        GlitchForm { layer: 0, sel: 0 }
    }
}

struct Row {
    label: String,
    value: String,
    kind: Kind,
}

/// Progress reported by the background compile job.
enum Progress {
    Step(usize, String),
    Fraction(f64, String),
    Done(Result<String, String>),
}

/// A running compile: emit the artifact, build it with rustc, check it.
struct Job {
    rx: Receiver<Progress>,
    step: usize,
    total: usize,
    text: String,
    fraction: Option<f64>,
    started: Instant,
}

const COMPILE_STEPS: usize = 3;

fn cycle<T: PartialEq + Copy>(all: &[T], current: T, forward: bool) -> T {
    let i = all.iter().position(|&x| x == current).unwrap_or(0);
    let n = all.len();
    all[if forward { (i + 1) % n } else { (i + n - 1) % n }]
}

fn step_u16(values: &[u16], current: u16, forward: bool) -> u16 {
    match values.iter().position(|&v| v == current) {
        Some(i) if forward => values[(i + 1) % values.len()],
        Some(i) => values[(i + values.len() - 1) % values.len()],
        None => values[0],
    }
}

/// A colour per tile kind, so the plan grid reads at a glance.
fn kind_colour(kind: u8) -> Rgb {
    match kind {
        tile_kind::FLOOR => (120, 120, 130),
        tile_kind::WALL => (170, 170, 180),
        tile_kind::PILLAR => (220, 220, 230),
        tile_kind::ROOF => (140, 180, 220),
        tile_kind::RUBBLE => (200, 150, 140),
        tile_kind::ALTAR => (245, 205, 120),
        tile_kind::STONE => (90, 90, 100),
        _ => FAINT,
    }
}

/// A cell whose blocks descend reads as a colder, deeper stone.
fn below_colour() -> Rgb {
    (74, 92, 120)
}

fn tile_name(kind: u8) -> &'static str {
    match kind {
        tile_kind::FLOOR => "floor",
        tile_kind::WALL => "wall",
        tile_kind::PILLAR => "pillar",
        tile_kind::ROOF => "roof",
        tile_kind::RUBBLE => "rubble",
        tile_kind::ALTAR => "altar",
        tile_kind::STONE => "stone",
        _ => "empty",
    }
}

/// The seed number shown beside the drone rhythm, so it is obvious which one
/// shapes it.
fn config_seed_label(config: &SimConfig, i: usize) -> u64 {
    seed_value(config, i)
}

fn on_off(on: bool) -> String {
    if on {
        "on".to_string()
    } else {
        "off".to_string()
    }
}

/// Typed values are clamped to exactly the range the arrow keys nudge within.
fn clamp_arc(n: i64) -> u16 {
    n.clamp(ARCS[0] as i64, ARCS[ARCS.len() - 1] as i64) as u16
}

fn clamp_rot(n: i64) -> i16 {
    n.clamp(ArchTransform::MIN_ROT as i64, ArchTransform::MAX_ROT as i64) as i16
}

fn clamp_off(n: i64) -> i16 {
    n.clamp(ArchTransform::MIN_OFF as i64, ArchTransform::MAX_OFF as i64) as i16
}

fn key_label(key: KeyChoice) -> String {    match key {
        KeyChoice::Seed => "seed picks".to_string(),
        KeyChoice::Note(n) => key_name(n).to_string(),
    }
}
fn key_index(key: KeyChoice) -> usize {
    match key {
        KeyChoice::Seed => 0,
        KeyChoice::Note(n) => n as usize + 1,
    }
}
fn key_from_index(i: usize) -> KeyChoice {
    if i == 0 {
        KeyChoice::Seed
    } else {
        KeyChoice::Note(((i - 1) % 12) as u8)
    }
}
fn layer_name(i: usize) -> String {
    if i < LOOP_TIMBRES.len() {
        LOOP_TIMBRES[i].name().to_string()
    } else {
        "Event grid".to_string()
    }
}
fn design_to_index(d: LoopDesign) -> usize {
    match d {
        LoopDesign::Sustained(t) => LOOP_TIMBRES.iter().position(|x| *x == t).unwrap_or(0),
        LoopDesign::Events(_) => EVENT_GRID,
    }
}
fn index_to_design(i: usize) -> LoopDesign {
    if i < LOOP_TIMBRES.len() {
        LoopDesign::Sustained(LOOP_TIMBRES[i % LOOP_TIMBRES.len()])
    } else {
        LoopDesign::Events(Grid::fire())
    }
}
fn arc_label(arc: DroneArc) -> String {
    if arc.cycle_s == 0 {
        "always on".to_string()
    } else {
        format!("{} s cycle, {} s up", arc.cycle_s, arc.hold_s)
    }
}
fn seed_value(config: &SimConfig, i: usize) -> u64 {
    [
        config.seeds.s1,
        config.seeds.s2,
        config.seeds.s3,
        config.seeds.s4,
        config.seeds.s5,
        config.seeds.s6,
        config.seeds.s7,
    ][i.min(6)]
}
fn set_seed(config: &mut SimConfig, i: usize, value: u64) {
    match i {
        0 => config.seeds.s1 = value,
        1 => config.seeds.s2 = value,
        2 => config.seeds.s3 = value,
        3 => config.seeds.s4 = value,
        4 => config.seeds.s5 = value,
        5 => config.seeds.s6 = value,
        _ => config.seeds.s7 = value,
    }
}

struct App {
    config: SimConfig,
    sim: Sim,
    frame: Framebuffer,
    time: f64,
    animate: bool,
    sel: usize,
    status: String,
    job: Option<Job>,
    prompt: Option<Prompt>,
    audition: Option<Audition>,
    screen: Screen,
    wav: WavForm,
    fx: FxForm,
    gform: GlitchForm,
    edit: EditForm,
    quit: bool,
}

impl App {
    fn new() -> Self {
        let config = state::load();
        let sim = Sim::new_visual(config.clone());
        let mut frame = Framebuffer::logical();
        sim.render_visual(&mut frame, 0.0, Quality::High);
        let seed = config.seed;
        App {
            config,
            sim,
            frame,
            time: 0.0,
            animate: true,
            sel: 1,
            status: "ready — ←→ change · enter act · r random · q quit".to_string(),
            job: None,
            prompt: None,
            audition: None,
            screen: Screen::Main,
            wav: WavForm::new(seed),
            gform: GlitchForm::new(),
            edit: EditForm::new(),
            fx: FxForm::new(),
            quit: false,
        }
    }

    fn rebuild(&mut self) {
        self.sim = Sim::new_visual(self.config.clone());
        self.redraw();
        state::save(&self.config);
        if let Some(audition) = &self.audition {
            audition.set_config(self.config.clone());
        }
    }

    fn redraw(&mut self) {
        self.sim.render_visual(&mut self.frame, self.time, Quality::High);
    }

    fn rows(&self) -> Vec<Row> {
        let c = &self.config;
        let s = &c.settings;
        let mut rows: Vec<Row> = Vec::new();
        let head = |label: &str, rows: &mut Vec<Row>| {
            rows.push(Row { label: label.to_string(), value: String::new(), kind: Kind::Header });
        };
        let field = |label: String, value: String, kind: Kind, rows: &mut Vec<Row>| {
            rows.push(Row { label, value, kind });
        };

        head("Track", &mut rows);
        field(
            "Track name".into(),
            if s.name.is_empty() { "(unnamed)".into() } else { s.name.as_str().to_string() },
            Kind::Name,
            &mut rows,
        );

        head("Shrine", &mut rows);
        field("Seed".into(), c.seed.to_string(), Kind::Seed, &mut rows);
        field("Biome".into(), c.biome.name().to_string(), Kind::Biome, &mut rows);
        field("Weather".into(), c.weather.name().to_string(), Kind::Weather, &mut rows);
        field(
            "Audition".into(),
            if self.audition.is_some() { "on".into() } else { "off".into() },
            Kind::Audition,
            &mut rows,
        );

        head("Music", &mut rows);
        field("Scale".into(), s.scale.name().to_string(), Kind::Scale, &mut rows);
        field("Root note".into(), key_label(s.key), Kind::Key, &mut rows);
        field("Chords".into(), s.chords.to_string(), Kind::Chords, &mut rows);
        field("Pace".into(), if s.pace == Pace::Jungle { "jungle".into() } else { "half-time".into() }, Kind::Pace, &mut rows);
        field("Tempo".into(), format!("{} BPM", s.bpm), Kind::Bpm, &mut rows);
        field("Kit".into(), s.kit.name().to_string(), Kind::Kit, &mut rows);
        field("Space".into(), s.space.name().to_string(), Kind::Space, &mut rows);
        field("Drum reverse".into(), if s.drum_reverse { "on".into() } else { "off".into() }, Kind::DrumReverse, &mut rows);

        head("Atmosphere", &mut rows);
        for i in 0..4 {
            field(format!("Layer {}", i + 1), layer_name(design_to_index(s.loops[i])), Kind::Layer(i), &mut rows);
        }
        for i in 0..4 {
            field(format!("Layer {} arc", i + 1), arc_label(s.loop_arcs[i]), Kind::LayerArc(i), &mut rows);
            field(format!("Layer {} hold", i + 1), format!("{} s", s.loop_arcs[i].hold_s), Kind::LayerHold(i), &mut rows);
        }
        field("Transpose".into(), format!("{} st", s.loop_transpose), Kind::Transpose, &mut rows);
        field(
            "Loop rhythm".into(),
            if s.loop_rhythm { "on · the drone\u{2019}s rhythm".into() } else { "off".into() },
            Kind::LoopRhythm,
            &mut rows,
        );

        head("Drone", &mut rows);
        field("Drone arc".into(), arc_label(s.drone), Kind::DroneArc, &mut rows);
        field("Drone hold".into(), format!("{} s", s.drone.hold_s), Kind::DroneHold, &mut rows);
        field("Drone tone".into(), if s.drone_tone == 0 { "off".into() } else { format!("{} Hz", s.drone_tone) }, Kind::DroneTone, &mut rows);
        field(
            "Drone rhythm".into(),
            if s.drone_rhythm { format!("on · seed {}", config_seed_label(c, 6)) } else { "off".into() },
            Kind::DroneRhythm,
            &mut rows,
        );

        head("Master", &mut rows);
        field("Master FX".into(), fx_summary(s.master), Kind::OpenFx, &mut rows);

        head("Glitches", &mut rows);
        for i in 0..2 {
            field(format!("Glitch layer {}", i + 1), s.glitch[i].summary(), Kind::Glitch(i), &mut rows);
        }

        head("Overlay", &mut rows);
        field("Code overlay".into(), if s.code.enabled { "on".into() } else { "off".into() }, Kind::CodeEnabled, &mut rows);
        field("Code opacity".into(), format!("{}%", s.code.opacity), Kind::CodeOpacity, &mut rows);
        field("Code speed".into(), s.code.speed.to_string(), Kind::CodeSpeed, &mut rows);
        field("Code text size".into(), format!("{}%", s.code.scale), Kind::CodeScale, &mut rows);
        field("Code angle".into(), format!("{}°", s.code.angle), Kind::CodeAngle, &mut rows);
        field("Code vertical".into(), format!("{}%", s.code.offset), Kind::CodeOffset, &mut rows);
        field("Code plane".into(), CodeOverlay::PLANES[(s.code.plane as usize).min(2)].to_string(), Kind::CodePlane, &mut rows);

        head("Camera", &mut rows);
        field(
            "Rotation speed".into(),
            if s.camera_speed == 0 { "still".into() } else { format!("{:+}°/s", s.camera_speed) },
            Kind::CameraSpeed,
            &mut rows,
        );

        head("Architecture", &mut rows);
        field("Rotate X".into(), format!("{}°", s.arch.rot_x), Kind::ArchRotX, &mut rows);
        field("Rotate Y".into(), format!("{}°", s.arch.rot_y), Kind::ArchRotY, &mut rows);
        field("Rotate Z".into(), format!("{}°", s.arch.rot_z), Kind::ArchRotZ, &mut rows);
        field("Offset X".into(), format!("{:+} cells", s.arch.off_x), Kind::ArchOffX, &mut rows);
        field("Offset Y".into(), format!("{:+} cells", s.arch.off_y), Kind::ArchOffY, &mut rows);
        field("Offset Z".into(), format!("{:+} levels", s.arch.off_z), Kind::ArchOffZ, &mut rows);

        head("Seeds", &mut rows);
        for i in 0..SEED_ROLES.len() {
            field(format!("Seed {} · {}", i + 1, SEED_ROLES[i]), seed_value(c, i).to_string(), Kind::SeedN(i), &mut rows);
        }

        head("Do", &mut rows);
        field("Save PNG".into(), format!("previews/shrine-{}.png", c.seed), Kind::SavePng, &mut rows);
        field("Cover PNG".into(), "1600×1600 export".into(), Kind::CoverPng, &mut rows);
        field("Export WAV".into(), "metadata + cover art".into(), Kind::WavExport, &mut rows);
        field("Export seed".into(), format!("artifacts/ShrineGen-{}.shrineseed", c.seed), Kind::ExportSeed, &mut rows);
        field("Import seed".into(), "load a .shrineseed file".into(), Kind::ImportSeed, &mut rows);
        field("Reset to defaults".into(), "a fresh project".into(), Kind::Reset, &mut rows);
        field(
            "Architecture editor".into(),
            if s.model.is_authored() { "hand-authored".into() } else { "from the seed".into() },
            Kind::EditArch,
            &mut rows,
        );
        field("Compile WASM".into(), "emit → rustc → web/shrine.wasm".into(), Kind::Compile, &mut rows);
        field("Quit".into(), String::new(), Kind::Quit, &mut rows);
        rows
    }

    fn menu_step(&self, from: usize, dir: i32) -> usize {
        let rows = self.rows();
        let n = rows.len() as i32;
        let mut i = from as i32;
        for _ in 0..n {
            i = (i + dir).rem_euclid(n);
            if rows[i as usize].kind != Kind::Header {
                return i as usize;
            }
        }
        from
    }

    fn adjust(&mut self, forward: bool, shift: bool) {
        let kind = self.rows()[self.sel].kind;
        match kind {
            Kind::Seed => {
                let step = if shift { 100 } else { 1 };
                self.config.seed = if forward { self.config.seed.wrapping_add(step) } else { self.config.seed.wrapping_sub(step) };
            }
            Kind::Biome => self.config.biome = cycle(&Biome::ALL, self.config.biome, forward),
            Kind::Name => {}
            Kind::Weather => self.config.weather = cycle(&WeatherKind::ALL, self.config.weather, forward),
            Kind::Audition => self.toggle_audition(),
            Kind::Scale => self.config.settings.scale = cycle(&SCALES, self.config.settings.scale, forward),
            Kind::Key => {
                let i = (key_index(self.config.settings.key) + if forward { 1 } else { 12 }) % 13;
                self.config.settings.key = key_from_index(i);
            }
            Kind::Chords => {
                let c = self.config.settings.chords;
                self.config.settings.chords = if forward { (c % 4) + 1 } else { ((c + 2) % 4) + 1 };
            }
            Kind::Pace => {
                self.config.settings.pace = if self.config.settings.pace == Pace::Jungle { Pace::HalfTime } else { Pace::Jungle };
            }
            Kind::Bpm => {
                let b = self.config.settings.bpm;
                self.config.settings.bpm = if forward {
                    (b + if shift { 10 } else { 1 }).min(180)
                } else {
                    b.saturating_sub(if shift { 10 } else { 1 }).max(70)
                };
            }
            Kind::Kit => self.config.settings.kit = cycle(&KITS, self.config.settings.kit, forward),
            Kind::Space => self.config.settings.space = cycle(&SPACES, self.config.settings.space, forward),
            Kind::DrumReverse => self.config.settings.drum_reverse = !self.config.settings.drum_reverse,
            Kind::OpenFx => {
                self.screen = Screen::Fx;
                return;
            }
            Kind::Layer(i) => {
                let n = LOOP_TIMBRES.len() + 1;
                let idx = design_to_index(self.config.settings.loops[i]);
                self.config.settings.loops[i] = index_to_design(if forward { (idx + 1) % n } else { (idx + n - 1) % n });
            }
            Kind::LayerArc(i) => {
                self.config.settings.loop_arcs[i].cycle_s = step_u16(&ARCS, self.config.settings.loop_arcs[i].cycle_s, forward);
            }
            Kind::LayerHold(i) => {
                let cycle = self.config.settings.loop_arcs[i].cycle_s;
                let hold = self.config.settings.loop_arcs[i].hold_s.clamp(0, cycle);
                self.config.settings.loop_arcs[i].hold_s = if forward { (hold + 1).min(cycle) } else { hold.saturating_sub(1) };
            }
            Kind::Transpose => {
                let t = self.config.settings.loop_transpose;
                self.config.settings.loop_transpose = if forward { (t + 1).min(12) } else { (t - 1).max(-12) };
            }
            Kind::DroneArc => self.config.settings.drone.cycle_s = step_u16(&ARCS, self.config.settings.drone.cycle_s, forward),
            Kind::DroneHold => {
                let cycle = self.config.settings.drone.cycle_s;
                let hold = self.config.settings.drone.hold_s.clamp(0, cycle);
                self.config.settings.drone.hold_s = if forward { (hold + 1).min(cycle) } else { hold.saturating_sub(1) };
            }
            Kind::DroneTone => self.config.settings.drone_tone = step_u16(&TONES, self.config.settings.drone_tone, forward),
            Kind::DroneRhythm => self.config.settings.drone_rhythm = !self.config.settings.drone_rhythm,
            Kind::LoopRhythm => self.config.settings.loop_rhythm = !self.config.settings.loop_rhythm,
            Kind::CodeEnabled => self.config.settings.code.enabled = !self.config.settings.code.enabled,
            Kind::CodeOpacity => {
                let step = if shift { 10 } else { 5 };
                let v = self.config.settings.code.opacity as i32 + if forward { step } else { -step };
                self.config.settings.code.opacity = v.clamp(0, 100) as u8;
            }
            Kind::CodeSpeed => {
                let step = if shift { 10 } else { 5 };
                let v = self.config.settings.code.speed as i32 + if forward { step } else { -step };
                self.config.settings.code.speed = v.clamp(0, 100) as u8;
            }
            Kind::CodeScale => {
                let step = if shift { 50 } else { 25 };
                let v = self.config.settings.code.scale as i32 + if forward { step } else { -step };
                self.config.settings.code.scale =
                    v.clamp(CodeOverlay::MIN_SCALE as i32, CodeOverlay::MAX_SCALE as i32) as u8;
            }
            Kind::CodeAngle => {
                let step = if shift { 15 } else { 5 };
                let v = self.config.settings.code.angle as i32 + if forward { step } else { -step };
                self.config.settings.code.angle =
                    v.clamp(CodeOverlay::MIN_ANGLE as i32, CodeOverlay::MAX_ANGLE as i32) as i16;
            }
            Kind::CodeOffset => {
                let step = if shift { 25 } else { 5 };
                let v = self.config.settings.code.offset as i32 + if forward { step } else { -step };
                self.config.settings.code.offset =
                    v.clamp(CodeOverlay::MIN_OFFSET as i32, CodeOverlay::MAX_OFFSET as i32) as i16;
            }
            Kind::CodePlane => {
                let n = CodeOverlay::PLANES.len() as i32;
                let i = self.config.settings.code.plane as i32 + if forward { 1 } else { -1 };
                self.config.settings.code.plane = i.rem_euclid(n) as u8;
            }
            Kind::CameraSpeed => {
                let step = if shift { 15 } else { 5 };
                let v = self.config.settings.camera_speed as i32 + if forward { step } else { -step };
                self.config.settings.camera_speed = v.clamp(-180, 180) as i16;
            }
            Kind::ArchRotX | Kind::ArchRotY | Kind::ArchRotZ => {
                let step = if shift { 15 } else { 5 };
                let d = if forward { step } else { -step };
                let rot = &mut self.config.settings.arch;
                let field = match kind {
                    Kind::ArchRotX => &mut rot.rot_x,
                    Kind::ArchRotY => &mut rot.rot_y,
                    _ => &mut rot.rot_z,
                };
                let v = *field as i32 + d;
                *field = v.clamp(ArchTransform::MIN_ROT as i32, ArchTransform::MAX_ROT as i32) as i16;
            }
            Kind::ArchOffX | Kind::ArchOffY | Kind::ArchOffZ => {
                let step = if shift { 4 } else { 1 };
                let d = if forward { step } else { -step };
                let arch = &mut self.config.settings.arch;
                let field = match kind {
                    Kind::ArchOffX => &mut arch.off_x,
                    Kind::ArchOffY => &mut arch.off_y,
                    _ => &mut arch.off_z,
                };
                let v = *field as i32 + d;
                *field = v.clamp(ArchTransform::MIN_OFF as i32, ArchTransform::MAX_OFF as i32) as i16;
            }
            Kind::SeedN(i) => {
                let step = if shift { 100 } else { 1 };
                let v = seed_value(&self.config, i);
                set_seed(&mut self.config, i, if forward { v.wrapping_add(step) } else { v.wrapping_sub(step) });
            }
            Kind::Header => return,
            Kind::Reset => {}
            Kind::EditArch => {}
            Kind::Glitch(i) => {
                self.gform.layer = i.min(1);
                self.screen = Screen::Glitch;
                return;
            }
            Kind::SavePng | Kind::CoverPng | Kind::WavExport | Kind::ExportSeed | Kind::ImportSeed | Kind::Compile | Kind::Quit => {
                self.activate();
                return;
            }
        }
        self.rebuild();
    }

    fn activate(&mut self) {
        let (kind, label) = {
            let row = &self.rows()[self.sel];
            (row.kind, row.label.clone())
        };
        // Every plain number can be typed exactly, as well as nudged with the
        // arrows; both go through the same clamp, so they can never disagree.
        if let Some(value) = self.number_text(kind) {
            self.prompt = Some(Prompt { label, value, action: PromptAction::Number(kind) });
            return;
        }
        match kind {
            Kind::SavePng => self.save_png(),
            Kind::CoverPng => self.cover_png(),
            Kind::WavExport => {
                self.screen = Screen::Wav;
            }
            Kind::ExportSeed => {
                self.prompt = Some(Prompt {
                    label: "Export .shrineseed to".into(),
                    value: format!("artifacts/ShrineGen-{}.shrineseed", self.config.seed),
                    action: PromptAction::ExportSeed,
                });
            }
            Kind::ImportSeed => {
                self.prompt = Some(Prompt {
                    label: "Import .shrineseed from".into(),
                    value: format!("artifacts/ShrineGen-{}.shrineseed", self.config.seed),
                    action: PromptAction::ImportSeed,
                });
            }
            Kind::Compile => self.start_compile(),
            Kind::Reset => self.reset_to_defaults(),
            Kind::Glitch(i) => {
                self.gform.layer = i.min(1);
                self.gform.sel = 0;
                self.screen = Screen::Glitch;
            }
            Kind::EditArch => self.open_editor(),
            Kind::Name => {
                self.prompt = Some(Prompt {
                    label: "Track name".into(),
                    value: self.config.settings.name.as_str().to_string(),
                    action: PromptAction::Name,
                });
            }
            Kind::Quit => self.quit = true,
            Kind::Header => {}
            _ => self.adjust(true, false),
        }
    }

    /// The current value of a numeric row, for pre-filling the editor. `None`
    /// means the row is not a plain number and keeps its cycle/toggle action.
    fn number_text(&self, kind: Kind) -> Option<String> {
        let s = &self.config.settings;
        Some(match kind {
            Kind::Seed => self.config.seed.to_string(),
            Kind::SeedN(i) => seed_value(&self.config, i).to_string(),
            Kind::Chords => s.chords.to_string(),
            Kind::Bpm => s.bpm.to_string(),
            Kind::Transpose => s.loop_transpose.to_string(),
            Kind::LayerArc(i) => s.loop_arcs[i].cycle_s.to_string(),
            Kind::LayerHold(i) => s.loop_arcs[i].hold_s.to_string(),
            Kind::DroneArc => s.drone.cycle_s.to_string(),
            Kind::DroneHold => s.drone.hold_s.to_string(),
            Kind::DroneTone => s.drone_tone.to_string(),
            Kind::CodeOpacity => s.code.opacity.to_string(),
            Kind::CodeSpeed => s.code.speed.to_string(),
            Kind::CodeScale => s.code.scale.to_string(),
            Kind::CodeAngle => s.code.angle.to_string(),
            Kind::CodeOffset => s.code.offset.to_string(),
            Kind::CameraSpeed => s.camera_speed.to_string(),
            Kind::ArchRotX => s.arch.rot_x.to_string(),
            Kind::ArchRotY => s.arch.rot_y.to_string(),
            Kind::ArchRotZ => s.arch.rot_z.to_string(),
            Kind::ArchOffX => s.arch.off_x.to_string(),
            Kind::ArchOffY => s.arch.off_y.to_string(),
            Kind::ArchOffZ => s.arch.off_z.to_string(),
            _ => return None,
        })
    }

    /// Apply a typed value to a numeric row. A trailing degree or percent sign
    /// is accepted, and every limit matches the arrow-key nudge exactly.
    fn set_number(&mut self, kind: Kind, label: &str, text: &str) -> Result<(), String> {
        let t = text.trim();
        let digits = t.trim_end_matches(['°', '%']).trim();
        let n: i64 = digits.parse().map_err(|_| format!("{label}: \"{t}\" is not a number"))?;
        let c = &mut self.config;
        match kind {
            Kind::Seed => c.seed = n.max(0) as u64,
            Kind::SeedN(i) => set_seed(c, i, n.max(0) as u64),
            Kind::Chords => c.settings.chords = n.clamp(1, 4) as u8,
            Kind::Bpm => c.settings.bpm = n.clamp(70, 180) as u16,
            Kind::Transpose => c.settings.loop_transpose = n.clamp(-12, 12) as i8,
            Kind::LayerArc(i) => c.settings.loop_arcs[i].cycle_s = clamp_arc(n),
            Kind::LayerHold(i) => {
                let cycle = c.settings.loop_arcs[i].cycle_s;
                c.settings.loop_arcs[i].hold_s = n.clamp(0, cycle as i64) as u16;
            }
            Kind::DroneArc => c.settings.drone.cycle_s = clamp_arc(n),
            Kind::DroneHold => {
                let cycle = c.settings.drone.cycle_s;
                c.settings.drone.hold_s = n.clamp(0, cycle as i64) as u16;
            }
            Kind::DroneTone => c.settings.drone_tone = n.clamp(0, TONES[TONES.len() - 1] as i64) as u16,
            Kind::CodeOpacity => c.settings.code.opacity = n.clamp(0, 100) as u8,
            Kind::CodeSpeed => c.settings.code.speed = n.clamp(0, 100) as u8,
            Kind::CodeScale => {
                c.settings.code.scale =
                    n.clamp(CodeOverlay::MIN_SCALE as i64, CodeOverlay::MAX_SCALE as i64) as u8;
            }
            Kind::CodeAngle => {
                c.settings.code.angle =
                    n.clamp(CodeOverlay::MIN_ANGLE as i64, CodeOverlay::MAX_ANGLE as i64) as i16;
            }
            Kind::CodeOffset => {
                c.settings.code.offset =
                    n.clamp(CodeOverlay::MIN_OFFSET as i64, CodeOverlay::MAX_OFFSET as i64) as i16;
            }
            Kind::CameraSpeed => c.settings.camera_speed = n.clamp(-180, 180) as i16,
            Kind::ArchRotX => c.settings.arch.rot_x = clamp_rot(n),
            Kind::ArchRotY => c.settings.arch.rot_y = clamp_rot(n),
            Kind::ArchRotZ => c.settings.arch.rot_z = clamp_rot(n),
            Kind::ArchOffX => c.settings.arch.off_x = clamp_off(n),
            Kind::ArchOffY => c.settings.arch.off_y = clamp_off(n),
            Kind::ArchOffZ => c.settings.arch.off_z = clamp_off(n),
            _ => return Err(format!("{label}: not a number")),
        }
        self.rebuild();
        Ok(())
    }

    fn commit_prompt(&mut self) {
        let Some(prompt) = self.prompt.take() else {
            return;
        };
        let text = prompt.value.trim().to_string();
        match prompt.action {
            PromptAction::Name => {
                self.config.settings.name = shrine0012::name::Name::from_str(&text);
                state::save(&self.config);
                self.status = format!("track name: {}", emit::title(&self.config));
            }
            PromptAction::ImportSeed => {
                if text.is_empty() {
                    self.status = "no path given".to_string();
                    return;
                }
                match seed::read(&text) {
                    Ok(config) => {
                        self.config = config;
                        state::save(&self.config);
                        self.rebuild();
                        self.status = format!("imported {text}");
                    }
                    Err(e) => self.status = format!("import failed: {e}"),
                }
            }
            PromptAction::EditorHeight => {
                let t = text.trim();
                match t.parse::<i8>() {
                    Ok(v) => {
                        let v = v.clamp(ShrineModel::MIN_HEIGHT, ShrineModel::MAX);
                        let (x, y) = (self.edit.cx, self.edit.cy);
                        let m = self.edit.working;
                        self.mutate_cell(|mo| mo.set(x, y, v, m.kind(x, y).max(1), m.variant(x, y)));
                        self.status = format!("height at {x},{y}: {v}");
                    }
                    Err(_) => self.status = format!("height: \"{t}\" is not a number"),
                }
            }
            PromptAction::GlitchNumber(row) => match self.set_glitch_number(row, &prompt.label, &text) {
                Ok(()) => self.status = format!("{}: {}", prompt.label, text),
                Err(e) => self.status = e,
            },
            PromptAction::Number(kind) => match self.set_number(kind, &prompt.label, &text) {
                Ok(()) => self.status = format!("{}: {}", prompt.label, text),
                Err(e) => self.status = e,
            },
            PromptAction::ExportSeed => {
                if text.is_empty() {
                    self.status = "no path given".to_string();
                    return;
                }
                match seed::write(&self.config, &text) {
                    Ok(()) => self.status = format!("wrote {text}"),
                    Err(e) => self.status = format!("export failed: {e}"),
                }
            }
        }
    }

    fn toggle_audition(&mut self) {
        if let Some(audition) = self.audition.take() {
            drop(audition);
            self.status = "audition off".to_string();
            return;
        }
        match Audition::start(self.config.clone()) {
            Ok(audition) => {
                self.audition = Some(audition);
                self.status = "audition on".to_string();
            }
            Err(e) => self.status = format!("audition: {e}"),
        }
    }

    fn handle_fx_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Esc => self.screen = Screen::Main,
            KeyCode::Up => self.fx.sel = if self.fx.sel == 0 { 7 } else { self.fx.sel - 1 },
            KeyCode::Down => self.fx.sel = (self.fx.sel + 1) % 8,
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right => self.toggle_fx(),
            _ => {}
        }
        false
    }

    fn toggle_fx(&mut self) {
        let master = &mut self.config.settings.master;
        match self.fx.sel {
            0 => master.enabled = !master.enabled,
            1 => master.tape = !master.tape,
            2 => master.cd = !master.cd,
            3 => master.pitch = !master.pitch,
            4 => master.radio = !master.radio,
            5 => master.crush = !master.crush,
            6 => master.dropout = !master.dropout,
            _ => {
                self.screen = Screen::Main;
                return;
            }
        }
        self.rebuild();
        self.status = format!("master FX: {}", fx_summary(self.config.settings.master));
    }

    fn draw_fx(&self, f: &mut Frame) {
        let area = f.area();
        let buf = f.buffer_mut();
        if area.width < 30 || area.height < 12 {
            put(buf, area, 0, 0, "terminal too small", style(TEXT));
            return;
        }
        let list_w = (area.width / 2).clamp(24, 46);
        let help_y = area.height - 1;
        put(buf, area, 0, 0, concat!("ShrineGen · master FX · v", env!("BUILD_DATE")), style(ACCENT).add_modifier(Modifier::BOLD));
        let status_x = area.width.saturating_sub(self.status.chars().count() as u16 + 1);
        put(buf, area, status_x, 0, &self.status, style(DIM));

        let rows = self.fx.rows(self.config.settings.master);
        for (i, (label, value)) in rows.iter().enumerate() {
            let y = 1 + i as u16;
            if y >= help_y {
                break;
            }
            let selected = i == self.fx.sel;
            let marker = if selected { "›" } else { " " };
            let label_style = if selected { style(SELECT).add_modifier(Modifier::BOLD) } else { style(TEXT) };
            put(buf, area, 0, y, &format!("{marker} {label:<16}"), label_style);
            put(buf, area, 18, y, value, style(if selected { SELECT } else { TEXT }));
        }

        let preview = Rect::new(list_w, 0, area.width - list_w, help_y);
        draw_preview(buf, preview, &self.frame);
        put(buf, area, 0, help_y, "↑↓ move · enter toggle · esc back   (any combination)", style(FAINT));
    }

    fn gsetup(&self) -> GlitchSetup {
        self.config.settings.glitch[self.gform.layer.min(1)]
    }

    fn gsetup_mut(&mut self) -> &mut GlitchSetup {
        let layer = self.gform.layer.min(1);
        &mut self.config.settings.glitch[layer]
    }

    /// Every row of the open glitch layer, in order.
    fn glitch_rows(&self) -> Vec<(String, String)> {
        let s = self.gsetup();
        GLITCH_ROWS
            .iter()
            .map(|row| match row {
                GlitchRow::Enabled => ("Layer".to_string(), on_off(s.enabled)),
                GlitchRow::Kind(i) => (format!("  {}", KINDS[*i].name()), on_off(s.kinds[*i])),
                GlitchRow::Fill => ("How often".to_string(), format!("{}%", s.fill)),
                GlitchRow::Rate => ("Step rate".to_string(), format!("×{}", RATES[(s.rate as usize).min(RATES.len() - 1)])),
                GlitchRow::Gain => ("Level".to_string(), format!("{}%", s.gain)),
                GlitchRow::Steps => ("Pattern steps".to_string(), s.steps.to_string()),
                GlitchRow::RepeatsMin => ("Loops before a change".to_string(), s.repeats_min.to_string()),
                GlitchRow::RepeatsMax => ("  … at most".to_string(), s.repeats_max.to_string()),
                GlitchRow::Ratchet => ("Max retriggers".to_string(), s.ratchet.to_string()),
                GlitchRow::Back => ("Back".to_string(), String::new()),
            })
            .collect()
    }

    fn adjust_glitch(&mut self, forward: bool, shift: bool) {
        let row = GLITCH_ROWS[self.gform.sel.min(GLITCH_ROWS.len() - 1)];
        let dir = if forward { 1 } else { -1 };
        match row {
            GlitchRow::Enabled => {
                let s = self.gsetup_mut();
                s.enabled = !s.enabled;
            }
            GlitchRow::Kind(i) => {
                let s = self.gsetup_mut();
                s.kinds[i] = !s.kinds[i];
            }
            GlitchRow::Fill => {
                let step = if shift { 10 } else { 5 };
                let s = self.gsetup_mut();
                s.fill = (s.fill as i32 + dir * step).clamp(0, 100) as u8;
            }
            GlitchRow::Rate => {
                let n = RATES.len() as i32;
                let s = self.gsetup_mut();
                s.rate = (s.rate as i32 + dir).rem_euclid(n) as u8;
            }
            GlitchRow::Gain => {
                let step = if shift { 25 } else { 10 };
                let s = self.gsetup_mut();
                s.gain = (s.gain as i32 + dir * step).clamp(0, 200) as u8;
            }
            GlitchRow::Steps => {
                let step = if shift { 4 } else { 1 };
                let s = self.gsetup_mut();
                s.steps = (s.steps as i32 + dir * step).clamp(1, 24) as u8;
            }
            GlitchRow::RepeatsMin | GlitchRow::RepeatsMax => {
                let step = if shift { 4 } else { 1 };
                let s = self.gsetup_mut();
                if row == GlitchRow::RepeatsMin {
                    s.repeats_min = ((s.repeats_min as i32 + dir * step).clamp(1, 24) as u8).min(s.repeats_max);
                } else {
                    s.repeats_max = ((s.repeats_max as i32 + dir * step).clamp(1, 24) as u8).max(s.repeats_min);
                }
            }
            GlitchRow::Ratchet => {
                let s = self.gsetup_mut();
                s.ratchet = (s.ratchet as i32 + dir).clamp(1, 8) as u8;
            }
            GlitchRow::Back => {}
        }
        self.rebuild();
    }

    fn activate_glitch(&mut self) {
        let idx = self.gform.sel.min(GLITCH_ROWS.len() - 1);
        let row = GLITCH_ROWS[idx];
        if let Some(value) = self.glitch_number(row) {
            let label = self.glitch_rows()[idx].0.trim().to_string();
            self.prompt = Some(Prompt { label, value, action: PromptAction::GlitchNumber(row) });
            return;
        }
        match row {
            GlitchRow::Back => self.screen = Screen::Main,
            _ => self.adjust_glitch(true, false),
        }
    }

    fn glitch_number(&self, row: GlitchRow) -> Option<String> {
        let s = self.gsetup();
        Some(match row {
            GlitchRow::Fill => s.fill.to_string(),
            GlitchRow::Gain => s.gain.to_string(),
            GlitchRow::Steps => s.steps.to_string(),
            GlitchRow::RepeatsMin => s.repeats_min.to_string(),
            GlitchRow::RepeatsMax => s.repeats_max.to_string(),
            GlitchRow::Ratchet => s.ratchet.to_string(),
            _ => return None,
        })
    }

    fn set_glitch_number(&mut self, row: GlitchRow, label: &str, text: &str) -> Result<(), String> {
        let t = text.trim();
        let digits = t.trim_end_matches(['°', '%', '×', 'x']).trim();
        let n: i64 = digits.parse().map_err(|_| format!("{label}: \"{t}\" is not a number"))?;
        let s = self.gsetup_mut();
        match row {
            GlitchRow::Fill => s.fill = n.clamp(0, 100) as u8,
            GlitchRow::Gain => s.gain = n.clamp(0, 200) as u8,
            GlitchRow::Steps => s.steps = n.clamp(1, 24) as u8,
            GlitchRow::RepeatsMin => s.repeats_min = (n.clamp(1, 24) as u8).min(s.repeats_max),
            GlitchRow::RepeatsMax => s.repeats_max = (n.clamp(1, 24) as u8).max(s.repeats_min),
            GlitchRow::Ratchet => s.ratchet = n.clamp(1, 8) as u8,
            _ => return Err(format!("{label}: not a number")),
        }
        self.rebuild();
        Ok(())
    }

    fn handle_glitch_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Esc => self.screen = Screen::Main,
            KeyCode::Up => {
                self.gform.sel = if self.gform.sel == 0 { GLITCH_ROWS.len() - 1 } else { self.gform.sel - 1 };
            }
            KeyCode::Down => self.gform.sel = (self.gform.sel + 1) % GLITCH_ROWS.len(),
            KeyCode::Left => self.adjust_glitch(false, false),
            KeyCode::Right => self.adjust_glitch(true, false),
            KeyCode::Enter | KeyCode::Char(' ') => self.activate_glitch(),
            _ => {}
        }
        false
    }

    fn draw_glitch(&self, f: &mut Frame) {
        let area = f.area();
        let buf = f.buffer_mut();
        if area.width < 30 || area.height < 12 {
            put(buf, area, 0, 0, "terminal too small", style(TEXT));
            return;
        }
        let list_w = (area.width / 2).clamp(24, 46);
        let help_y = area.height - 1;
        let head = format!("ShrineGen · glitches · layer {} · v{}", self.gform.layer.min(1) + 1, env!("BUILD_DATE"));
        put(buf, area, 0, 0, &head, style(ACCENT).add_modifier(Modifier::BOLD));
        let status_x = area.width.saturating_sub(self.status.chars().count() as u16 + 1);
        put(buf, area, status_x, 0, &self.status, style(DIM));

        let rows = self.glitch_rows();
        for (i, (label, value)) in rows.iter().enumerate() {
            let y = 1 + i as u16;
            if y >= help_y {
                break;
            }
            let selected = i == self.gform.sel;
            let marker = if selected { "›" } else { " " };
            let label_style = if selected { style(SELECT).add_modifier(Modifier::BOLD) } else { style(TEXT) };
            put(buf, area, 0, y, &format!("{marker} {label:<22}"), label_style);
            put(buf, area, 24, y, value, style(if selected { SELECT } else { TEXT }));
        }

        let preview = Rect::new(list_w, 0, area.width - list_w, help_y);
        draw_preview(buf, preview, &self.frame);
        put(buf, area, 0, help_y, "↑↓ move · ←→ change · enter type · esc back", style(FAINT));
    }

    /// Open the architecture editor, seeded from whatever the generator made.
    fn open_editor(&mut self) {
        // The baseline is what the generator makes with no model at all.
        let mut bare = self.config.clone();
        bare.settings.model = ShrineModel::EMPTY;
        self.edit.baseline = ShrineModel::from_shrine(&Sim::new_visual(bare).scene.shrine);
        // Always start from what is actually loaded. Seeding from the baseline
        // instead would show the generator's tiles — every square at least one
        // high — for a shrine whose architecture was authored, so ground-level
        // and descending squares would look like they had been reset.
        self.edit.working = self.config.settings.model;
        if !self.edit.working.is_authored() {
            self.edit.working = self.edit.baseline;
        }
        self.edit.cx = self.edit.cx.min(15);
        self.edit.cy = self.edit.cy.min(15);
        self.edit.undo.clear();
        self.screen = Screen::Editor;
        self.status = "editing the architecture".to_string();
    }

    /// Change one cell, with undo. The first real edit is what makes the model
    /// authored; until then the recipe stays procedural.
    fn mutate_cell(&mut self, f: impl FnOnce(&mut ShrineModel)) {
        let mut model = self.edit.working;
        f(&mut model);
        if model == self.edit.working {
            return;
        }
        self.edit.undo.push(self.edit.working);
        if self.edit.undo.len() > 64 {
            self.edit.undo.remove(0);
        }
        self.edit.working = model;
        self.config.settings.model = if model == self.edit.baseline { ShrineModel::EMPTY } else { model };
        self.rebuild();
    }

    fn handle_editor_key(&mut self, code: KeyCode) -> bool {
        let (x, y) = (self.edit.cx, self.edit.cy);
        let m = self.edit.working;
        match code {
            KeyCode::Esc => self.screen = Screen::Main,
            KeyCode::Up | KeyCode::Char('k') => self.edit.cy = if y == 0 { 15 } else { y - 1 },
            KeyCode::Down | KeyCode::Char('j') => self.edit.cy = (y + 1) % 16,
            KeyCode::Left | KeyCode::Char('h') => self.edit.cx = if x == 0 { 15 } else { x - 1 },
            KeyCode::Right | KeyCode::Char('l') => self.edit.cx = (x + 1) % 16,
            KeyCode::Char('+') | KeyCode::Char('=') => {
                let h = (m.height(x, y) + 1).min(ShrineModel::MAX);
                self.mutate_cell(|mo| mo.set(x, y, h, m.kind(x, y), m.variant(x, y)));
            }
            KeyCode::Char('-') | KeyCode::Char('_') => {
                // Lower the column; at zero it stops being a column at all.
                let h = (m.height(x, y) - 1).max(ShrineModel::MIN_HEIGHT);
                self.mutate_cell(|mo| mo.set(x, y, h, m.kind(x, y).max(1), m.variant(x, y)));
            }
            // A digit sets the height of the square under the cursor, and 0
            // removes the column entirely so the ground shows through.
            KeyCode::Char(c) if c.is_ascii_digit() && c <= '6' => {
                let h = (c as u8 - b'0') as i8;
                self.mutate_cell(|mo| mo.set(x, y, h, m.kind(x, y).max(1), m.variant(x, y)));
            }
            // Back to the ground plane on this square.
            KeyCode::Backspace | KeyCode::Delete => {
                self.mutate_cell(|mo| mo.set(x, y, 0, m.kind(x, y).max(1), m.variant(x, y)));
            }
            // Type an exact height, negative to dig down.
            KeyCode::Enter => {
                self.prompt = Some(Prompt {
                    label: format!("Height at {x},{y} ({}..{})", ShrineModel::MIN_HEIGHT, ShrineModel::MAX),
                    value: m.height(x, y).to_string(),
                    action: PromptAction::EditorHeight,
                });
            }
            KeyCode::Char(' ') | KeyCode::Char(']') => {
                let k = (m.kind(x, y) % 7) + 1;
                self.mutate_cell(|mo| mo.set(x, y, m.height(x, y), k, m.variant(x, y)));
            }
            KeyCode::Char('[') => {
                let k = ((m.kind(x, y) + 5) % 7) + 1;
                self.mutate_cell(|mo| mo.set(x, y, m.height(x, y), k, m.variant(x, y)));
            }
            KeyCode::Char('v') => {
                let v = (m.variant(x, y) + 1) % 4;
                self.mutate_cell(|mo| mo.set(x, y, m.height(x, y), m.kind(x, y), v));
            }
            KeyCode::Char('u') => {
                if let Some(prev) = self.edit.undo.pop() {
                    self.edit.working = prev;
                    self.config.settings.model = if prev == self.edit.baseline { ShrineModel::EMPTY } else { prev };
                    self.rebuild();
                    self.status = "undo".to_string();
                }
            }
            KeyCode::Char('r') => {
                // Back to the generator: the recipe carries no model again.
                self.edit.working = self.edit.baseline;
                self.config.settings.model = ShrineModel::EMPTY;
                self.rebuild();
                self.status = "regenerated from the seed".to_string();
            }
            KeyCode::Char('c') => {
                // Blank the plan: every square becomes a hole. `r` brings the
                // seed's shrine back.
                self.mutate_cell(|mo| {
                    for cy in 0..16 {
                        for cx in 0..16 {
                            mo.set(cx, cy, 0, 1, 0);
                        }
                    }
                });
                self.status = "cleared — every square is a hole (r regenerates)".to_string();
            }
            _ => {}
        }
        false
    }

    fn draw_editor(&self, f: &mut Frame) {
        let area = f.area();
        let buf = f.buffer_mut();
        if area.width < 98 || area.height < 22 {
            put(buf, area, 0, 0, "terminal too small for the editor (need 98x22)", style(TEXT));
            return;
        }
        let help_y = area.height - 1;
        put(buf, area, 0, 0, concat!("ShrineGen · architecture · v", env!("BUILD_DATE")), style(ACCENT).add_modifier(Modifier::BOLD));
        put(buf, area, 48, 0, "type/±/0-6 height · space/[ ] kind · v variant · del ground · u undo · r regen · c blank", style(DIM));

        // Plan grid: one glyph per cell, coloured by kind, reversed on the cursor.
        let m = &self.edit.working;
        for y in 0..16u16 {
            for x in 0..16usize {
                let gx = x as u16;
                let gy = y;
                let h = m.height(x, y as usize);
                let k = m.kind(x, y as usize);
                let glyph = format!("{h:>2} ");
                let base = if h < 0 { below_colour() } else if h == 0 { FAINT } else { kind_colour(k) };
                let selected = x == self.edit.cx && y as usize == self.edit.cy;
                let mut st = style(if selected { SELECT } else { base });
                if selected {
                    st = st.add_modifier(Modifier::REVERSED).add_modifier(Modifier::BOLD);
                }
                put(buf, area, gx * 3, 1 + gy, &glyph, st);
            }
        }
        put(buf, area, 0, 18, "negative stacks downwards · floor wall pillar roof rubble altar", style(FAINT));

        let preview = Rect::new(49, 1, area.width - 49, help_y - 1);
        draw_preview(buf, preview, &self.frame);
        let y = self.edit.cy;
        let h = m.height(self.edit.cx, y);
        let what = if h < 0 { "blocks below" } else if h == 0 { "ground" } else { "column" };
        put(buf, area, 0, help_y, &format!("cell {},{y}  ·  {} (h {})  ·  kind {}  variant {}  ·  esc back", self.edit.cx, what, h, tile_name(m.kind(self.edit.cx, y)), m.variant(self.edit.cx, y)), style(FAINT));
    }

    fn randomize(&mut self) {        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1);
        let value = nanos % 100_000;
        match self.rows()[self.sel].kind {
            Kind::SeedN(i) => set_seed(&mut self.config, i, if i == 4 { value.max(1) } else { value }),
            _ => self.config.seed = value,
        }
        self.rebuild();
    }

    fn start_compile(&mut self) {
        let config = self.config.clone();
        let rs = format!("artifacts/ShrineGen-{}.rs", config.seed);
        let seed_path = format!("artifacts/ShrineGen-{}.shrineseed", config.seed);
        let wasm = "web/shrine.wasm".to_string();
        let (tx, rx) = channel();
        let (thread_rs, thread_seed, thread_wasm) = (rs.clone(), seed_path.clone(), wasm.clone());
        std::thread::spawn(move || run_compile(config, thread_rs, thread_seed, thread_wasm, tx));
        self.job = Some(Job { rx, step: 0, total: COMPILE_STEPS, text: "starting…".to_string(), fraction: None, started: Instant::now() });
        self.status = format!("compiling → {wasm}");
    }

    /// Back to a fresh project: every choice, including the track name, the
    /// overlay and the architecture transform. Deliberately not randomised —
    /// `r` already does that — so a reset is always the same, testable state.
    fn reset_to_defaults(&mut self) {
        self.config = SimConfig::default();
        state::save(&self.config);
        self.rebuild();
        self.status = "reset to defaults".to_string();
    }

    fn save_png(&mut self) {
        let out = format!("previews/shrine-{}.png", self.config.seed);
        let caption = format!(
            "seed {} · {} · {} · 300×300 logical → 1200×1200 nearest",
            self.config.seed,
            self.config.biome.name(),
            self.config.weather.name()
        );
        self.status = match preview::write_frame(&self.frame, &caption, &out) {
            Ok(()) => format!("saved {out} (+ .html)"),
            Err(e) => format!("save failed: {e}"),
        };
    }

    fn cover_png(&mut self) {
        let out = format!("previews/ShrineGen-{}-cover.png", self.config.seed);
        self.status = match preview::write_cover(&self.frame, &out, COVER_SIZE) {
            Ok(()) => format!("saved {out} ({COVER_SIZE}×{COVER_SIZE})"),
            Err(e) => format!("cover failed: {e}"),
        };
    }

    fn handle_wav_key(&mut self, code: KeyCode) -> bool {
        if self.wav.editing.is_some() {
            match code {
                KeyCode::Esc => self.wav.editing = None,
                KeyCode::Enter => self.commit_wav_field(),
                KeyCode::Backspace => {
                    if let Some(edit) = &mut self.wav.editing {
                        edit.pop();
                    }
                }
                KeyCode::Char(c) if !c.is_control() => {
                    if let Some(edit) = &mut self.wav.editing {
                        edit.push(c);
                    }
                }
                _ => {}
            }
            return false;
        }
        match code {
            KeyCode::Esc => self.screen = Screen::Main,
            KeyCode::Up => self.wav.sel = if self.wav.sel == 0 { 8 } else { self.wav.sel - 1 },
            KeyCode::Down => self.wav.sel = (self.wav.sel + 1) % 9,
            KeyCode::Left => self.adjust_wav(-1.0),
            KeyCode::Right => self.adjust_wav(1.0),
            KeyCode::Enter | KeyCode::Char(' ') => self.activate_wav(),
            _ => {}
        }
        false
    }

    fn adjust_wav(&mut self, dir: f64) {
        match self.wav.sel {
            0 => self.wav.length_s = (self.wav.length_s + dir * 5.0).clamp(1.0, 3600.0),
            1 => self.wav.fade_s = (self.wav.fade_s + dir).clamp(0.0, 120.0),
            6 => self.wav.normalize = !self.wav.normalize,
            _ => {}
        }
    }

    fn activate_wav(&mut self) {
        match self.wav.sel {
            0 => self.wav.editing = Some(format!("{}", self.wav.length_s)),
            1 => self.wav.editing = Some(format!("{}", self.wav.fade_s)),
            2 => self.wav.editing = Some(self.wav.filename.clone()),
            3 => self.wav.editing = Some(self.config.settings.name.as_str().to_string()),
            4 => self.wav.editing = Some(self.wav.artist.clone()),
            5 => self.wav.editing = Some(self.wav.album.clone()),
            6 => self.wav.normalize = !self.wav.normalize,
            7 => self.start_wav_export(),
            _ => self.screen = Screen::Main,
        }
    }

    fn commit_wav_field(&mut self) {
        let Some(text) = self.wav.editing.take() else {
            return;
        };
        let text = text.trim().to_string();
        match self.wav.sel {
            0 => match parse_seconds(&text) {
                Some(v) => self.wav.length_s = v.clamp(1.0, 3600.0),
                None => self.status = "length: enter seconds or m:ss".to_string(),
            },
            1 => match parse_seconds(&text) {
                Some(v) => self.wav.fade_s = v.clamp(0.0, 600.0),
                None => self.status = "fade: enter seconds or m:ss".to_string(),
            },
            2 if !text.is_empty() => self.wav.filename = text,
            3 => {
                self.config.settings.name = shrine0012::name::Name::from_str(&text);
                state::save(&self.config);
            }
            4 => self.wav.artist = text,
            5 => self.wav.album = text,
            _ => {}
        }
    }

    fn start_wav_export(&mut self) {
        let config = self.config.clone();
        let opts = wav::WavOptions {
            length_s: self.wav.length_s,
            fade_s: self.wav.fade_s,
            filename: self.wav.filename.clone(),
            title: emit::title(&self.config),
            artist: self.wav.artist.clone(),
            album: self.wav.album.clone(),
            normalize: self.wav.normalize,
        };
        let (tx, rx) = channel();
        std::thread::spawn(move || run_wav_export(config, opts, tx));
        self.job = Some(Job { rx, step: 0, total: 3, text: "rendering…".to_string(), fraction: Some(0.0), started: Instant::now() });
        self.status = "exporting WAV…".to_string();
    }

    /// Returns true to quit.
    fn handle_key(&mut self, code: KeyCode, mods: KeyModifiers) -> bool {
        if self.job.is_some() {
            // Ignore input while a compile is running.
            return false;
        }
        // A prompt can be open over any screen, so it always gets the keys first.
        if self.prompt.is_some() {
            match code {
                KeyCode::Esc => self.prompt = None,
                KeyCode::Enter => self.commit_prompt(),
                KeyCode::Backspace => {
                    if let Some(p) = &mut self.prompt {
                        p.value.pop();
                    }
                }
                KeyCode::Char(c) if !c.is_control() => {
                    if let Some(p) = &mut self.prompt {
                        p.value.push(c);
                    }
                }
                _ => {}
            }
            return self.quit;
        }
        if self.screen == Screen::Wav {
            return self.handle_wav_key(code);
        }
        if self.screen == Screen::Fx {
            return self.handle_fx_key(code);
        }
        if self.screen == Screen::Glitch {
            return self.handle_glitch_key(code);
        }
        if self.screen == Screen::Editor {
            return self.handle_editor_key(code);
        }
        match code {
            KeyCode::Char('q') | KeyCode::Esc => {
                state::save(&self.config);
                return true;
            }
            KeyCode::Up => self.sel = self.menu_step(self.sel, -1),
            KeyCode::Down => self.sel = self.menu_step(self.sel, 1),
            KeyCode::Left => self.adjust(false, mods.contains(KeyModifiers::SHIFT)),
            KeyCode::Right => self.adjust(true, mods.contains(KeyModifiers::SHIFT)),
            KeyCode::Enter | KeyCode::Char(' ') => self.activate(),
            KeyCode::Char('r') => self.randomize(),
            _ => {}
        }
        self.quit
    }

    fn draw(&self, f: &mut Frame) {
        match self.screen {
            Screen::Wav => self.draw_wav(f),
            Screen::Fx => self.draw_fx(f),
            Screen::Glitch => self.draw_glitch(f),
            Screen::Editor => self.draw_editor(f),
            Screen::Main => self.draw_main(f),
        }
        // A prompt can be open over any screen, so it is always drawn last.
        if let Some(p) = &self.prompt {
            let area = f.area();
            let buf = f.buffer_mut();
            let y = area.height.saturating_sub(1);
            put(buf, area, 0, y, &format!("{}: {}▏  (enter confirm · esc cancel)", p.label, p.value), style(ACCENT));
        }
    }

    fn draw_main(&self, f: &mut Frame) {
        let area = f.area();
        let buf = f.buffer_mut();
        if area.width < 30 || area.height < 10 {
            put(buf, area, 0, 0, "terminal too small", style(TEXT));
            return;
        }

        let list_w = (area.width / 2).clamp(24, 46);
        let help_y = area.height - 1;

        put(buf, area, 0, 0, concat!("ShrineGen · setup · v", env!("BUILD_DATE")), style(ACCENT).add_modifier(Modifier::BOLD));
        let status_x = area.width.saturating_sub(self.status.chars().count() as u16 + 1);
        put(buf, area, status_x, 0, &self.status, style(DIM));

        let rows = self.rows();
        let list_h = (help_y.saturating_sub(1)) as usize;
        let scroll = menu_scroll(self.sel, rows.len(), list_h);
        for (i, row) in rows.iter().enumerate().skip(scroll).take(list_h) {
            let y = 1 + (i - scroll) as u16;
            let selected = i == self.sel;
            let marker = if selected { "›" } else { " " };
            match row.kind {
                Kind::Header => put(buf, area, 1, y, &row.label.to_uppercase(), style(DIM).add_modifier(Modifier::BOLD)),
                _ => {
                    let label_style = if selected { style(SELECT).add_modifier(Modifier::BOLD) } else { style(TEXT) };
                    put(buf, area, 0, y, &format!("{marker} {:<22}", row.label), label_style);
                    let vx = 24.min(list_w.saturating_sub(2));
                    put(buf, area, vx, y, &row.value, style(if selected { SELECT } else { TEXT }));
                }
            }
        }

        let preview = Rect::new(list_w, 0, area.width - list_w, help_y);
        draw_preview(buf, preview, &self.frame);
        if let Some(job) = &self.job {
            draw_progress(buf, area, help_y, job);
        } else {
            put(buf, area, 0, help_y, "↑↓ row · ←→ change (shift = bigger) · enter act · r random · q quit", style(FAINT));
        }
    }

    fn draw_wav(&self, f: &mut Frame) {
        let area = f.area();
        let buf = f.buffer_mut();
        if area.width < 30 || area.height < 12 {
            put(buf, area, 0, 0, "terminal too small", style(TEXT));
            return;
        }
        let list_w = (area.width / 2).clamp(24, 46);
        let help_y = area.height - 1;
        put(buf, area, 0, 0, concat!("ShrineGen · export WAV · v", env!("BUILD_DATE")), style(ACCENT).add_modifier(Modifier::BOLD));
        let status_x = area.width.saturating_sub(self.status.chars().count() as u16 + 1);
        put(buf, area, status_x, 0, &self.status, style(DIM));

        let rows = self.wav.rows(&emit::title(&self.config));
        for (i, (label, value)) in rows.iter().enumerate() {
            let y = 1 + i as u16;
            if y >= help_y {
                break;
            }
            let selected = i == self.wav.sel;
            let marker = if selected { "›" } else { " " };
            let label_style = if selected { style(SELECT).add_modifier(Modifier::BOLD) } else { style(TEXT) };
            put(buf, area, 0, y, &format!("{marker} {label:<14}"), label_style);
            put(buf, area, 16, y, value, style(if selected { SELECT } else { TEXT }));
        }

        let preview = Rect::new(list_w, 0, area.width - list_w, help_y);
        draw_preview(buf, preview, &self.frame);

        if let Some(edit) = &self.wav.editing {
            put(buf, area, 0, help_y, &format!("{}: {}▏  (enter confirm · esc cancel)", rows[self.wav.sel].0, edit), style(ACCENT));
        } else if let Some(job) = &self.job {
            draw_progress(buf, area, help_y, job);
        } else {
            put(buf, area, 0, help_y, "↑↓ row · enter edit/act · ←→ adjust · esc back", style(FAINT));
        }
    }
}

/// Show the logical buffer with upper half-blocks (2 pixels per cell), truecolor.
fn draw_preview(buf: &mut Buffer, area: Rect, fb: &Framebuffer) {
    let cols = area.width as usize;
    let rows = area.height as usize;
    if cols == 0 || rows == 0 {
        return;
    }
    let (pw, ph) = (fb.width() as usize, fb.height() as usize);
    let vrows = rows * 2;
    for cy in 0..rows {
        for cx in 0..cols {
            let x0 = cx * pw / cols;
            let x1 = ((cx + 1) * pw / cols).max(x0 + 1).min(pw);
            let top = sample(fb, x0, x1, cy * 2, vrows, ph);
            let bottom = sample(fb, x0, x1, cy * 2 + 1, vrows, ph);
            let st = Style::new().fg(color(top)).bg(color(bottom));
            put_char(buf, area, cx as u16, cy as u16, '▀', st);
        }
    }
}

fn sample(fb: &Framebuffer, x0: usize, x1: usize, vrow: usize, vrows: usize, ph: usize) -> Rgb {
    let y0 = vrow * ph / vrows;
    let y1 = ((vrow + 1) * ph / vrows).max(y0 + 1).min(ph);
    let (mut r, mut g, mut b, mut n) = (0u32, 0u32, 0u32, 0u32);
    for y in y0..y1 {
        for x in x0..x1 {
            let p = fb.get(x as i32, y as i32);
            r += p[0] as u32;
            g += p[1] as u32;
            b += p[2] as u32;
            n += 1;
        }
    }
    let n = n.max(1);
    ((r / n) as u8, (g / n) as u8, (b / n) as u8)
}

pub fn run() -> Result<(), String> {
    let mut terminal = ratatui::init();
    let mut app = App::new();
    let result = event_loop(&mut terminal, &mut app);
    ratatui::restore();
    if app.quit {
        state::save(&app.config);
    }
    result
}

fn event_loop(terminal: &mut DefaultTerminal, app: &mut App) -> Result<(), String> {
    let mut last = Instant::now();
    loop {
        let now = Instant::now();
        let dt = now.duration_since(last).as_secs_f64();
        last = now;
        poll_job(app);
        if app.animate {
            app.time += dt;
            app.redraw();
        }
        terminal.draw(|f| app.draw(f)).map_err(|e| e.to_string())?;

        let timeout = if app.animate || app.job.is_some() { Duration::from_millis(ANIM_MS) } else { Duration::from_millis(250) };
        if !event::poll(timeout).map_err(|e| e.to_string())? {
            continue;
        }
        let Event::Key(k) = event::read().map_err(|e| e.to_string())? else {
            continue;
        };
        if k.kind != KeyEventKind::Press {
            continue;
        }
        if app.handle_key(k.code, k.modifiers) {
            return Ok(());
        }
    }
}

/// Drain any messages from the background compile job.
fn poll_job(app: &mut App) {
    let mut finished: Option<Result<String, String>> = None;
    loop {
        let message = match &app.job {
            Some(job) => job.rx.try_recv(),
            None => break,
        };
        match message {
            Ok(Progress::Step(step, text)) => {
                if let Some(job) = &mut app.job {
                    job.step = step;
                    job.fraction = None;
                    job.text = text;
                }
            }
            Ok(Progress::Fraction(fraction, text)) => {
                if let Some(job) = &mut app.job {
                    job.fraction = Some(fraction);
                    job.text = text;
                }
            }
            Ok(Progress::Done(result)) => finished = Some(result),
            Err(_) => break,
        }
    }
    if let Some(result) = finished {
        app.status = match result {
            Ok(message) => format!("✓ {message}"),
            Err(message) => format!("✗ {message}"),
        };
        app.job = None;
    }
}

/// The background compile: emit, rustc, check. Runs off the UI thread.
fn run_compile(config: SimConfig, rs: String, seed_path: String, wasm: String, tx: Sender<Progress>) {
    let _ = tx.send(Progress::Step(0, "writing the artifact + seed".to_string()));
    if let Err(e) = crate::emit::emit(&config, &rs) {
        let _ = tx.send(Progress::Done(Err(format!("emit: {e}"))));
        return;
    }
    if let Err(e) = crate::seed::write(&config, &seed_path) {
        let _ = tx.send(Progress::Done(Err(format!("seed file: {e}"))));
        return;
    }
    let _ = crate::emit::publish_source(&rs, "web/shrine.rs");
    let _ = crate::serve::write_wav_manifest(std::path::Path::new("web"));

    let _ = std::fs::create_dir_all("web");
    let _ = tx.send(Progress::Step(1, "compiling with rustc".to_string()));
    let output = Command::new("rustc")
        .args(["--edition", "2021", "--target", "wasm32-unknown-unknown", "-O", "--crate-type", "cdylib", "--crate-name", "shrine_0012"])
        .arg(&rs)
        .arg("-o")
        .arg(&wasm)
        .output();
    match output {
        Ok(o) if o.status.success() => {}
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            let line = stderr.lines().find(|l| l.contains("error")).unwrap_or("rustc failed").trim().to_string();
            let _ = tx.send(Progress::Done(Err(format!("rustc: {line}"))));
            return;
        }
        Err(e) => {
            let _ = tx.send(Progress::Done(Err(format!("could not run rustc: {e}"))));
            return;
        }
    }

    let _ = tx.send(Progress::Step(2, "checking the wasm module".to_string()));
    match std::fs::read(&wasm) {
        Ok(bytes) if bytes.starts_with(b"\0asm") => {
            let _ = tx.send(Progress::Done(Ok(format!("{wasm} ready ({} bytes) · {seed_path}", bytes.len()))));
        }
        Ok(_) => {
            let _ = tx.send(Progress::Done(Err("the wasm module is malformed".to_string())));
        }
        Err(e) => {
            let _ = tx.send(Progress::Done(Err(format!("{wasm}: {e}"))));
        }
    }
}

/// The background WAV export: render, shape, cover, metadata, write.
fn run_wav_export(config: SimConfig, opts: wav::WavOptions, tx: Sender<Progress>) {
    let sample_rate = SAMPLE_RATE;
    let frames = (opts.length_s * sample_rate as f64).max(1.0) as usize;
    let mut sim = Sim::new(config.clone());
    let mut buf = vec![0f32; frames * 2];
    let chunk = 4096usize;
    let mut done = 0usize;
    while done < frames {
        let n = chunk.min(frames - done);
        sim.render_audio(&mut buf[done * 2..(done + n) * 2]);
        done += n;
        let fraction = done as f64 / frames as f64;
        let _ = tx.send(Progress::Fraction(fraction * 0.8, format!("rendering audio  {:.0}%", fraction * 100.0)));
    }

    wav::fade_in(&mut buf, sample_rate, 0.03);
    wav::fade_out(&mut buf, sample_rate, opts.fade_s);
    if opts.normalize {
        wav::normalize(&mut buf, 0.99);
    }

    let _ = tx.send(Progress::Fraction(0.85, "rendering cover".to_string()));
    let mut fb = Framebuffer::logical();
    let state = sim.state(0.0);
    sim.scene.render_state(&mut fb, &state, Quality::High);
    let cover = preview::cover_png(&fb, COVER_SIZE);

    let _ = tx.send(Progress::Fraction(0.92, "writing WAV + metadata".to_string()));
    let build = emit::build_date();
    let mut comment = emit::info(&config, build);
    comment.push_str("\n\nrecipe=");
    comment.push_str(&shrine0012::recipe::encode(&config).iter().map(|w| w.to_string()).collect::<Vec<_>>().join(","));
    let meta = wav::WavMeta {
        sample_rate,
        title: opts.title.clone(),
        artist: opts.artist.clone(),
        album: opts.album.clone(),
        comment,
        software: format!("ShrineGen v{build}"),
        cover_png: cover,
    };
    match wav::write_export(&opts.filename, &buf, &meta) {
        Ok(()) => {
            let _ = tx.send(Progress::Done(Ok(format!("{} ({:.0}s · cover + metadata)", opts.filename, opts.length_s))));
        }
        Err(e) => {
            let _ = tx.send(Progress::Done(Err(format!("{e}"))));
        }
    }
}

/// A bar plus a spinner, the current step and the elapsed time.
fn draw_progress(buf: &mut Buffer, area: Rect, y: u16, job: &Job) {
    let total = job.total.max(1);
    let frac = job.fraction.unwrap_or_else(|| (job.step as f64 + 0.35) / total as f64).clamp(0.0, 1.0);
    let width = (area.width as usize).saturating_sub(46).clamp(10, 44);
    let filled = (frac * width as f64).round() as usize;
    let bar: String = "█".repeat(filled) + &"░".repeat(width - filled);
    let frames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    let frame = frames[(job.started.elapsed().as_millis() / 80) as usize % frames.len()];
    let text = format!(
        "[{bar}] {}/{} {frame} {} ({:.1}s)",
        job.step + 1,
        total,
        job.text,
        job.started.elapsed().as_secs_f64()
    );
    put(buf, area, 0, y, &text, style(ACCENT));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An app whose architecture is procedural, with the editor open. Editor
    /// tests must not depend on whatever is stored on this machine.
    fn editor_app() -> App {
        let mut app = App::new();
        app.config.settings.model = ShrineModel::EMPTY;
        app.open_editor();
        app
    }

    #[test]
    fn defaults_and_rows_are_sane() {
        let app = App::new();
        assert_eq!((app.frame.width(), app.frame.height()), (300, 300));
        let rows = app.rows();
        assert!(rows.len() > 30);
        assert!(!rows[app.sel].label.is_empty());
        assert_ne!(rows[app.sel].kind, Kind::Header);
    }

    #[test]
    fn menu_step_skips_headers() {
        let app = App::new();
        let rows = app.rows();
        let mut i = app.sel;
        for _ in 0..rows.len() {
            i = app.menu_step(i, 1);
            assert_ne!(rows[i].kind, Kind::Header);
        }
    }

    #[test]
    fn values_cycle() {
        let mut app = App::new();
        let before = app.config.biome;
        app.sel = app.rows().iter().position(|r| r.kind == Kind::Biome).unwrap();
        app.adjust(true, false);
        assert_ne!(app.config.biome, before);
    }

    #[test]
    fn seeds_shift_and_randomise() {
        let mut app = App::new();
        app.sel = app.rows().iter().position(|r| r.kind == Kind::Seed).unwrap();
        let seed = app.config.seed;
        app.adjust(true, false);
        assert_eq!(app.config.seed, seed + 1);
        app.adjust(true, true);
        assert_eq!(app.config.seed, seed + 101);
        app.randomize();
        assert!(app.config.seed < 100_000);
    }

    #[test]
    fn preview_paints_every_cell() {
        let app = App::new();
        let area = Rect::new(0, 0, 30, 20);
        let mut buf = Buffer::empty(area);
        draw_preview(&mut buf, area, &app.frame);
        let painted = buf.content().iter().filter(|c| c.symbol() == "▀").count();
        assert_eq!(painted, 30 * 20);
    }

    #[test]
    fn quit_key_is_reported() {
        let mut app = App::new();
        assert!(app.handle_key(KeyCode::Char('q'), KeyModifiers::NONE));
    }

    #[test]
    fn the_master_fx_screen_toggles_combinations() {
        let mut app = App::new();
        app.config.settings.master = MasterFx::OFF;
        app.screen = Screen::Fx;
        app.fx.sel = 0;
        app.toggle_fx();
        assert!(app.config.settings.master.enabled);
        app.fx.sel = 1; // Tape
        app.toggle_fx();
        app.fx.sel = 4; // Radio
        app.toggle_fx();
        assert!(app.config.settings.master.tape && app.config.settings.master.radio);
        assert!(!app.config.settings.master.cd);
        assert_eq!(app.fx.rows(app.config.settings.master).len(), 8);
        app.fx.sel = 7; // Back
        app.toggle_fx();
        assert_eq!(app.screen, Screen::Main);
    }

    #[test]
    fn the_menu_offers_a_compile_action() {
        let app = App::new();
        assert!(app.rows().iter().any(|r| r.kind == Kind::Compile));
    }

    #[test]
    fn wav_export_writes_audio_metadata_and_cover() {
        let path = "/tmp/shrinegen-export-test.wav";
        let opts = wav::WavOptions {
            length_s: 1.0,
            fade_s: 0.2,
            filename: path.to_string(),
            title: "Test Shrine".to_string(),
            artist: "ShrineGen".to_string(),
            album: "0012".to_string(),
            normalize: true,
        };
        let (tx, rx) = channel();
        run_wav_export(SimConfig::default(), opts, tx);
        let mut done = None;
        while let Ok(message) = rx.try_recv() {
            if let Progress::Done(result) = message {
                done = Some(result);
            }
        }
        assert!(matches!(done, Some(Ok(_))), "export did not finish cleanly: {done:?}");
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert!(bytes.windows(4).any(|w| w == b"INFO"));
        assert!(bytes.windows(4).any(|w| w == b"APIC"), "no cover art");
        assert!(bytes.windows(3).any(|w| w == b"ID3"), "no ID3 tag");
    }

    #[test]
    fn fx_keys_toggle_through_the_screen() {
        let mut app = App::new();
        app.config.settings.master = MasterFx::OFF;
        app.screen = Screen::Fx;
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(app.config.settings.master.enabled);
        app.handle_key(KeyCode::Down, KeyModifiers::NONE);
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(app.config.settings.master.tape);
    }

    #[test]
    fn every_numeric_row_accepts_a_typed_value_and_clamps() {
        let mut app = App::new();
        let kinds: Vec<Kind> = app.rows().iter().map(|r| r.kind).collect();
        let mut checked = 0;
        for kind in kinds {
            // A fresh app per row: some rows legitimately constrain each other
            // (a hold cannot exceed its cycle), so reusing one would make this
            // test depend on the order the rows are visited.
            app = App::new();
            let Some(current) = app.number_text(kind) else { continue };
            // Re-entering the value that is already there must not change it.
            app.set_number(kind, "field", &current).unwrap_or_else(|e| panic!("{kind:?}: {e}"));
            assert_eq!(app.number_text(kind).unwrap(), current, "{kind:?} changed on re-entry");
            // Out-of-range and suffixed input must clamp, never fail or panic.
            app.set_number(kind, "field", "99999").unwrap_or_else(|e| panic!("{kind:?} high: {e}"));
            app.set_number(kind, "field", "-99999").unwrap_or_else(|e| panic!("{kind:?} low: {e}"));
            app.set_number(kind, "field", "5°").unwrap_or_else(|e| panic!("{kind:?} deg: {e}"));
            app.set_number(kind, "field", "5%").unwrap_or_else(|e| panic!("{kind:?} pct: {e}"));
            checked += 1;
        }
        assert!(checked >= 20, "expected many numeric rows, saw {checked}");
    }

    #[test]
    fn garbage_input_is_reported_not_swallowed() {
        let mut app = App::new();
        assert!(app.set_number(Kind::Bpm, "Tempo", "fast").is_err());
        assert!(app.set_number(Kind::Bpm, "Tempo", "").is_err());
        app.set_number(Kind::Bpm, "Tempo", "123").unwrap();
        assert_eq!(app.config.settings.bpm, 123);
    }

    #[test]
    fn enter_opens_a_typed_editor_on_a_numeric_row() {
        let mut app = App::new();
        app.sel = app.rows().iter().position(|r| r.kind == Kind::Bpm).unwrap();
        app.activate();
        assert!(app.prompt.is_some(), "a numeric row should open an editor");
        if let Some(prompt) = &mut app.prompt {
            prompt.value = "150".to_string();
        }
        app.commit_prompt();
        assert_eq!(app.config.settings.bpm, 150);
    }

    #[test]
    fn a_non_numeric_row_still_cycles_rather_than_opening_an_editor() {
        let mut app = App::new();
        app.sel = app.rows().iter().position(|r| r.kind == Kind::Biome).unwrap();
        app.activate();
        assert!(app.prompt.is_none(), "biome should cycle, not prompt");
    }

    #[test]
    fn reset_restores_defaults() {
        let mut app = App::new();
        app.config.seed = 42;
        app.config.settings.bpm = 111;
        app.config.settings.arch = ArchTransform { rot_z: 35, off_x: 3, ..ArchTransform::IDENTITY };
        app.config.settings.name = shrine0012::name::Name::from_str("Scratch");
        app.sel = app.rows().iter().position(|r| r.kind == Kind::Reset).unwrap();
        app.activate();
        // Compare the recipes: the strongest statement that the reset is exact.
        assert_eq!(
            shrine0012::recipe::encode(&app.config),
            shrine0012::recipe::encode(&SimConfig::default())
        );
        assert!(app.config.settings.name.is_empty());
    }

    #[test]
    fn the_glitch_screen_edits_the_open_layer() {
        let mut app = App::new();
        app.sel = app.rows().iter().position(|r| r.kind == Kind::Glitch(1)).unwrap();
        app.activate();
        assert_eq!(app.screen, Screen::Glitch);
        assert_eq!(app.gform.layer, 1);
        // Row 1 is the first kind.
        app.gform.sel = 1;
        let before = app.config.settings.glitch[1].kinds[0];
        app.adjust_glitch(true, false);
        assert_ne!(app.config.settings.glitch[1].kinds[0], before);
        // Row 0 mutes the layer.
        app.gform.sel = 0;
        app.adjust_glitch(true, false);
        assert!(!app.config.settings.glitch[1].enabled);
        // Back leaves the screen.
        app.gform.sel = GLITCH_ROWS.len() - 1;
        app.activate_glitch();
        assert_eq!(app.screen, Screen::Main);
    }

    #[test]
    fn glitch_numbers_can_be_typed_and_clamp() {
        let mut app = App::new();
        app.gform.layer = 0;
        app.set_glitch_number(GlitchRow::Fill, "How often", "80").unwrap();
        assert_eq!(app.config.settings.glitch[0].fill, 80);
        app.set_glitch_number(GlitchRow::Fill, "How often", "500").unwrap();
        assert_eq!(app.config.settings.glitch[0].fill, 100);
        app.set_glitch_number(GlitchRow::Fill, "How often", "35%").unwrap();
        assert_eq!(app.config.settings.glitch[0].fill, 35);
        assert!(app.set_glitch_number(GlitchRow::Fill, "How often", "loud").is_err());
        app.set_glitch_number(GlitchRow::Ratchet, "Max retriggers", "3").unwrap();
        assert_eq!(app.config.settings.glitch[0].ratchet, 3);
    }

    #[test]
    fn the_glitch_repeats_range_cannot_invert() {
        let mut app = App::new();
        app.gform.layer = 0;
        app.config.settings.glitch[0].repeats_min = 4;
        app.config.settings.glitch[0].repeats_max = 9;
        // Index 11 is "loops before a change": push it well past the maximum.
        app.gform.sel = 11;
        for _ in 0..20 {
            app.adjust_glitch(true, false);
        }
        let g = app.config.settings.glitch[0];
        assert!(g.repeats_min <= g.repeats_max, "range inverted: {g:?}");
        // And the same from the other end.
        app.gform.sel = 12;
        for _ in 0..30 {
            app.adjust_glitch(false, false);
        }
        let g = app.config.settings.glitch[0];
        assert!(g.repeats_min <= g.repeats_max, "range inverted: {g:?}");
    }

    #[test]
    fn every_glitch_row_is_reachable_and_reports_a_value() {
        let mut app = App::new();
        for (i, row) in GLITCH_ROWS.iter().enumerate() {
            app.gform.sel = i;
            let rows = app.glitch_rows();
            assert_eq!(rows.len(), GLITCH_ROWS.len());
            let numeric = app.glitch_number(*row);
            if *row == GlitchRow::Back || matches!(row, GlitchRow::Enabled | GlitchRow::Kind(_) | GlitchRow::Rate) {
                assert!(numeric.is_none(), "{row:?} should not be typed");
            } else {
                assert!(numeric.is_some(), "{row:?} should be typable");
            }
            // Every row must survive an adjust without panicking.
            app.adjust_glitch(true, true);
            app.adjust_glitch(false, true);
        }
    }

    #[test]
    fn the_editor_starts_from_the_generated_shrine() {
        let mut app = App::new();
        app.config.settings.model = ShrineModel::EMPTY;
        app.sel = app.rows().iter().position(|r| r.kind == Kind::EditArch).unwrap();
        app.activate();
        assert_eq!(app.screen, Screen::Editor);
        assert!(app.edit.working.is_authored(), "the working copy shows the shrine");
        assert!(!app.config.settings.model.is_authored(), "opening must not author the model");
    }

    #[test]
    fn editing_a_cell_authors_the_model_and_can_be_undone() {
        let mut app = editor_app();
        let (x, y) = (3usize, 4usize);
        app.edit.cx = x;
        app.edit.cy = y;
        let before = app.edit.working.height(x, y);
        app.handle_editor_key(KeyCode::Char('+'));
        assert_eq!(app.edit.working.height(x, y), (before + 1).min(6));
        assert!(app.config.settings.model.is_authored(), "an edit must author the model");
        app.handle_editor_key(KeyCode::Char('u'));
        assert_eq!(app.edit.working.height(x, y), before);
        assert!(!app.config.settings.model.is_authored(), "undoing the only edit returns to procedural");
    }

    #[test]
    fn regenerating_hands_the_architecture_back_to_the_seed() {
        let mut app = editor_app();
        app.edit.cx = 5;
        app.edit.cy = 5;
        app.handle_editor_key(KeyCode::Char('+'));
        assert!(app.config.settings.model.is_authored());
        app.handle_editor_key(KeyCode::Char('r'));
        assert!(!app.config.settings.model.is_authored(), "r must clear the authored model");
        assert!(app.edit.working.is_authored(), "the working copy still shows the shrine");
    }

    #[test]
    fn reopening_the_editor_keeps_the_authored_architecture() {
        let mut app = editor_app();
        // Ground one square and stack another downwards.
        app.edit.cx = 4;
        app.edit.cy = 4;
        app.handle_editor_key(KeyCode::Char('0'));
        app.edit.cx = 5;
        app.handle_editor_key(KeyCode::Char('0'));
        app.handle_editor_key(KeyCode::Char('-'));
        app.handle_editor_key(KeyCode::Char('-'));
        let authored = app.config.settings.model;
        assert!(authored.is_authored());
        assert_eq!(authored.height(4, 4), 0);
        assert_eq!(authored.height(5, 4), -2);

        // Reopen, as a restart would: the same settings, a fresh app.
        let mut again = App::new();
        again.config = app.config.clone();
        again.open_editor();
        assert_eq!(again.edit.working, authored, "the editor must show what is loaded");
        assert_eq!(again.edit.working.height(4, 4), 0, "a ground square stays ground");
        assert_eq!(again.edit.working.height(5, 4), -2, "blocks below stay below");
    }

    #[test]
    fn a_digit_sets_the_height_of_that_square() {
        let mut app = editor_app();
        app.edit.cx = 2;
        app.edit.cy = 2;
        for (c, want) in [('3', 3i8), ('6', 6), ('0', 0), ('1', 1)] {
            app.handle_editor_key(KeyCode::Char(c));
            assert_eq!(app.edit.working.height(2, 2), want, "key {c}");
        }
        // A hole survives: pressing 0 removes the base, not the whole model.
        app.handle_editor_key(KeyCode::Char('0'));
        assert!(app.config.settings.model.is_authored());
        assert_eq!(app.edit.working.height(2, 2), 0);
    }

    #[test]
    fn heights_can_go_negative_and_be_typed() {
        let mut app = editor_app();
        app.edit.cx = 6;
        app.edit.cy = 6;
        // Minus digs below the ground plane, and stops at the deepest pit.
        for _ in 0..10 {
            app.handle_editor_key(KeyCode::Char('-'));
        }
        assert_eq!(app.edit.working.height(6, 6), ShrineModel::MIN_HEIGHT);
        // Plus brings it back up through zero and to the tallest column.
        for _ in 0..20 {
            app.handle_editor_key(KeyCode::Char('+'));
        }
        assert_eq!(app.edit.working.height(6, 6), ShrineModel::MAX);
        // Typing an exact negative height.
        app.prompt = Some(Prompt {
            label: "Height".into(),
            value: "-4".into(),
            action: PromptAction::EditorHeight,
        });
        app.commit_prompt();
        assert_eq!(app.edit.working.height(6, 6), -4);
        assert!(app.config.settings.model.is_authored());
        // Rubbish is reported, not applied, and the height stays put.
        app.prompt = Some(Prompt { label: "Height".into(), value: "deep".into(), action: PromptAction::EditorHeight });
        app.commit_prompt();
        assert_eq!(app.edit.working.height(6, 6), -4);
    }

    #[test]
    fn delete_and_zero_both_remove_a_base() {
        let mut app = editor_app();
        app.edit.cx = 7;
        app.edit.cy = 9;
        app.handle_editor_key(KeyCode::Char('4'));
        assert_eq!(app.edit.working.height(7, 9), 4);
        app.handle_editor_key(KeyCode::Backspace);
        assert_eq!(app.edit.working.height(7, 9), 0, "backspace should return it to ground");
        app.handle_editor_key(KeyCode::Char('5'));
        app.handle_editor_key(KeyCode::Delete);
        assert_eq!(app.edit.working.height(7, 9), 0, "delete should remove the base");
        // The rest of the shrine is untouched.
        assert!(app.edit.working.height(0, 0) > 0);
    }

    #[test]
    fn kind_cycling_keeps_a_valid_kind() {
        let mut app = editor_app();
        app.edit.cx = 1;
        app.edit.cy = 1;
        for _ in 0..9 {
            app.handle_editor_key(KeyCode::Char(' '));
            let k = app.edit.working.kind(1, 1);
            assert!((1..=7).contains(&k), "space produced kind {k}");
        }
        app.handle_editor_key(KeyCode::Char('['));
        assert!((1..=7).contains(&app.edit.working.kind(1, 1)));
    }

    #[test]
    fn clear_blanks_every_square_and_regenerate_brings_it_back() {
        let mut app = editor_app();
        app.handle_editor_key(KeyCode::Char('c'));
        assert!(app.config.settings.model.is_authored(), "a blanked plan is still authored");
        for y in 0..16 {
            for x in 0..16 {
                assert_eq!(app.edit.working.height(x, y), 0, "square {x},{y} should be a hole");
            }
        }
        app.handle_editor_key(KeyCode::Char('r'));
        assert!(!app.config.settings.model.is_authored(), "r returns to the generator");
        assert!(app.edit.working.height(0, 0) > 0, "the seed's shrine is back");
    }

    #[test]
    fn heights_stay_in_range_under_repeated_presses() {
        let mut app = editor_app();
        app.edit.cx = 3;
        app.edit.cy = 3;
        for _ in 0..12 {
            app.handle_editor_key(KeyCode::Char('+'));
        }
        assert!(app.edit.working.height(3, 3) <= 6);
        for _ in 0..12 {
            app.handle_editor_key(KeyCode::Char('-'));
        }
        assert_eq!(
            app.edit.working.height(3, 3),
            ShrineModel::MIN_HEIGHT,
            "minus digs down to the deepest pit"
        );
    }

    #[test]
    fn the_editor_cursor_wraps_and_editing_it_does_nothing() {
        let mut app = editor_app();
        app.edit.cx = 0;
        app.edit.cy = 0;
        app.handle_editor_key(KeyCode::Left);
        assert_eq!(app.edit.cx, 15, "the cursor should wrap");
        app.handle_editor_key(KeyCode::Up);
        assert_eq!(app.edit.cy, 15);
        app.handle_editor_key(KeyCode::Esc);
        assert_eq!(app.screen, Screen::Main);
    }

    #[test]
    fn the_track_name_can_be_set_and_reaches_the_seed_file() {
        let mut app = App::new();
        app.sel = app.rows().iter().position(|r| r.kind == Kind::Name).unwrap();
        app.activate();
        assert!(app.prompt.is_some(), "the name row should open an editor");
        if let Some(prompt) = &mut app.prompt {
            prompt.value = "Dusk Conservatory".to_string();
        }
        app.commit_prompt();
        assert_eq!(app.config.settings.name.as_str(), "Dusk Conservatory");
        assert!(crate::seed::to_text(&app.config).contains("name=Dusk Conservatory"));
    }

    #[test]
    fn the_code_overlay_toggles() {
        let mut app = App::new();
        app.config.settings.code = shrine0012::overlay::CodeOverlay::OFF;
        app.sel = app.rows().iter().position(|r| r.kind == Kind::CodeEnabled).unwrap();
        app.adjust(true, false);
        assert!(app.config.settings.code.enabled);
        app.sel = app.rows().iter().position(|r| r.kind == Kind::CodeOpacity).unwrap();
        app.adjust(true, false);
        assert_eq!(app.config.settings.code.opacity, 5);
        app.sel = app.rows().iter().position(|r| r.kind == Kind::CodeScale).unwrap();
        app.adjust(true, false);
        assert_eq!(app.config.settings.code.scale, 225);
    }

    #[test]
    fn the_menu_scrolls_with_lookahead() {
        // Nothing to scroll when the list fits.
        assert_eq!(menu_scroll(0, 10, 20), 0);
        assert_eq!(menu_scroll(9, 10, 20), 0);
        // 40 rows, 24 visible: the selection keeps 8 rows below it.
        assert_eq!(menu_scroll(0, 40, 24), 0);
        assert_eq!(menu_scroll(15, 40, 24), 0);
        assert_eq!(menu_scroll(16, 40, 24), 1);
        assert_eq!(menu_scroll(39, 40, 24), 16); // 40 - 24, so the end shows
    }
}
