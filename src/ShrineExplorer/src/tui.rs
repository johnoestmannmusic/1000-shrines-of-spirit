//! The interactive setup menu: choose every part of a shrine, watch it live,
//! save a still or a WAV. Answers are remembered between runs.
//!
//! Host-only. The settings it produces are exactly the recipe the artifact
//! bakes (`recipe.rs`), so what you see here is what ships.

use crate::{audition::Audition, emit, preview, seed, state, wav};
use shrine_explorer::arch::ArchTransform;
use shrine_explorer::master::MasterFx;
use shrine_explorer::overlay::CodeOverlay;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::{DefaultTerminal, Frame};
use shrine_explorer::atmos::LOOP_TIMBRES;
use shrine_explorer::glitch::KINDS;
use shrine_explorer::shrine::kind as tile_kind;
use shrine_explorer::shrine_model::ShrineModel;
use shrine_explorer::glitch_setup::{GlitchSetup, RATES};
use shrine_explorer::framebuffer::Framebuffer;
use shrine_explorer::harmony::{key_name, Pace, PACES, SCALES};
use shrine_explorer::kits::{KITS, SPACES};
use shrine_explorer::plants::Biome;
use shrine_explorer::scene::Quality;
use shrine_explorer::sim::{Sim, SimConfig};
use shrine_explorer::texture::Grid;
use shrine_explorer::track::{DroneArc, KeyChoice, LoopDesign};
use shrine_explorer::weather::WeatherKind;
use shrine_explorer::SAMPLE_RATE;
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
    Swing,
    GarageBass,
    GarageStabs,
    GarageTone,
    GarageProgression,
    GarageLift,
    MicroPulse,
    MicroGrains,
    MicroPips,
    MicroDropouts,
    MicroChords,
    MicroShift,
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
    Wildlife,
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
    Randomize,
    MapPos,
    MakeHome,
    StartAt,
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
    /// Type a spot on the explorer's map, `x, y`.
    MapPosition,
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
    Randomize,
}

/// The setup is split into tabs; each is one or more of the menu's groups.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tab {
    Track,
    Shrine,
    Music,
    Atmosphere,
    Drone,
    Glitch,
    Look,
    Export,
}

const TABS: [Tab; 8] = [Tab::Track, Tab::Shrine, Tab::Music, Tab::Atmosphere, Tab::Drone, Tab::Glitch, Tab::Look, Tab::Export];

impl Tab {
    fn name(self) -> &'static str {
        match self {
            Tab::Track => "Track",
            Tab::Shrine => "Shrine",
            Tab::Music => "Music",
            Tab::Atmosphere => "Atmosphere",
            Tab::Drone => "Drone & FX",
            Tab::Glitch => "Glitch",
            Tab::Look => "Look",
            Tab::Export => "Export",
        }
    }

    /// The tab a menu group (its header) belongs to.
    fn of_group(header: &str) -> Tab {
        match header {
            "Track" => Tab::Track,
            "Shrine" => Tab::Shrine,
            "Music" => Tab::Music,
            "Atmosphere" => Tab::Atmosphere,
            "Drone" | "Master" => Tab::Drone,
            "Glitches" => Tab::Glitch,
            "Overlay" | "Camera" | "Architecture" => Tab::Look,
            _ => Tab::Export,
        }
    }
}

/// Where the main screen's keys go: the section list, or the settings of
/// the chosen section.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Focus {
    Sections,
    Settings,
}

/// `"12, -3"`, `"12 -3"` or `"12,-3"` → a map position.
fn parse_map(text: &str) -> Option<(i32, i32)> {
    let mut it = text.split(|c: char| c == ',' || c.is_whitespace()).filter(|p| !p.is_empty()).map(|p| p.parse::<i32>());
    match (it.next(), it.next(), it.next()) {
        (Some(Ok(x)), Some(Ok(y)), None) => Some((x, y)),
        _ => None,
    }
}

fn map_status((x, y): (i32, i32)) -> String {
    if (x, y) == (0, 0) {
        "at home · 0, 0".to_string()
    } else {
        format!("exploring {x}, {y} · the rows show the home's settings")
    }
}

/// A section's current values, in a few words, for the sidebar.
fn section_summary(tab: Tab, c: &SimConfig) -> String {
    let s = &c.settings;
    match tab {
        Tab::Track if s.map != (0, 0) => format!("@ {}, {}", s.map.0, s.map.1),
        Tab::Track => if s.name.is_empty() { "unnamed".to_string() } else { s.name.as_str().to_string() },
        Tab::Shrine => format!("{} · {}", c.biome.name(), c.weather.name()),
        Tab::Music => {
            let mode = match s.pace {
                Pace::HalfTime => "half-time",
                Pace::Jungle => "jungle",
                Pace::Garage => "2-step",
                Pace::Micro => "micro",
            };
            format!("{mode} · {}", s.bpm)
        }
        Tab::Atmosphere => {
            let n = s.loops.iter().filter(|l| l.is_on()).count();
            format!("{n} layer{}", if n == 1 { "" } else { "s" })
        }
        Tab::Drone => {
            let arc = if s.drone.cycle_s == 0 { "always on".to_string() } else { format!("{} s arc", s.drone.cycle_s) };
            if s.master.enabled { format!("{arc} · fx") } else { arc }
        }
        Tab::Glitch => format!("{} on", s.glitch.iter().filter(|g| g.enabled).count()),
        Tab::Look => {
            let cam = if s.camera_speed == 0 { "still".to_string() } else { format!("orbit {:+}°/s", s.camera_speed) };
            if s.code.enabled { format!("{cam} · code") } else { cam }
        }
        Tab::Export => "WAV · cover · seed".to_string(),
    }
}

/// The tab of every row.
fn tabs_of(rows: &[Row]) -> Vec<Tab> {
    let mut tab = Tab::Track;
    rows.iter()
        .map(|r| {
            if r.kind == Kind::Header {
                tab = Tab::of_group(&r.label);
            }
            tab
        })
        .collect()
}

/// A popup listing every value of one setting. Moving through it applies
/// each value live (picture and audition); enter keeps it, esc puts back
/// what was there when it opened.
struct Picker {
    kind: Kind,
    title: String,
    /// (label, one-line description, the configuration with that value).
    options: Vec<(String, String, SimConfig)>,
    sel: usize,
    original: SimConfig,
}

/// What a row is for, in a line: shown under the menu for the selected row.
fn row_help(kind: Kind) -> &'static str {
    match kind {
        Kind::Header => "",
        Kind::Name => "The track's name: the page heading and the WAV title. Enter to type it.",
        Kind::Randomize => "Roll any mix of settings at once: just the seed, the drums, the mode… Enter to open.",
        Kind::MapPos => "Where on this home's map: enter types x, y. ←→ step x, shift+←→ step y. The rows below stay the home's.",
        Kind::MakeHome => "Make the shrine at this spot the new home, at 0, 0 (the map re-centres on it).",
        Kind::StartAt => "Join the piece part-way in. The music is fast-forwarded exactly; the visuals start there too.",
        Kind::Seed => "The seed builds the shrine, its plants, palette and weather. ←→ step, shift ±100, enter to type.",
        Kind::Biome => "What grows around the shrine, or which built future surrounds it.",
        Kind::Weather => "The weather: it moves the plants, the light and the sound of the bed.",
        Kind::Wildlife => "Creatures of the biome, heard and seen. Enter to switch on or off.",
        Kind::Audition => "Hear the music live while you choose. Enter to switch on or off.",
        Kind::Scale => "Which notes exist. Every chord, bass note, pip and ping is drawn from it.",
        Kind::Key => "The tonic: seed 1 picks it, or choose one.",
        Kind::Chords => "How many chords the progression cycles through.",
        Kind::Pace => "How the music moves. Each mode brings its own drums, bass and defaults.",
        Kind::Swing => "2-step swing: how late every second sixteenth lands (50 % is straight).",
        Kind::GarageProgression => "The chord loop: the seed's pick, or a named progression.",
        Kind::GarageLift => "Step the whole progression up a whole tone for a while, then home.",
        Kind::GarageBass => "A moving garage bassline, or the sustained bass of the other modes.",
        Kind::GarageStabs => "How often chord stabs play on the pushes.",
        Kind::GarageTone => "The stabs' colour.",
        Kind::MicroPulse => "What carries the microsound beat: a sub kick and pulse, with or without clicks.",
        Kind::MicroGrains => "How many ticks and grains, and how much of the bed becomes a grain cloud.",
        Kind::MicroPips => "High sine pips on chord tones, a few milliseconds each.",
        Kind::MicroDropouts => "Digital failure as edits: steps drop out, skip back or ratchet.",
        Kind::MicroChords => "How often the chords change in microsound.",
        Kind::MicroShift => "Transpose the drone's fragments by a repeating figure of four.",
        Kind::Bpm => "The drum tempo; every ambient layer moves at exactly half of it.",
        Kind::Kit => "How the drums are synthesised. Seed 5 sets each voice within the kit.",
        Kind::Space => "Where the drums sit in the stereo field.",
        Kind::DrumReverse => "Whether break edits may play slices backwards. Enter to switch.",
        Kind::OpenFx => "End-of-chain master effects. Enter to open.",
        Kind::Layer(_) => "One atmosphere loop: a sustained timbre, an event grid, or off.",
        Kind::LayerArc(_) => "How the layer comes and goes: always on, or in and out over a cycle.",
        Kind::LayerHold(_) => "How long the layer stays in, each cycle.",
        Kind::Transpose => "Shift the atmosphere loops by semitones.",
        Kind::DroneArc => "How the drone comes and goes: always on, or in and out over a cycle.",
        Kind::DroneHold => "How long the drone stays in, each cycle.",
        Kind::DroneTone => "A low-pass at the end of the drone's chain (off, or its cutoff).",
        Kind::DroneRhythm => "A rhythmic swell on the drone, shaped by seed 7. Enter to switch.",
        Kind::LoopRhythm => "The drone's rhythm on the atmosphere loops too. Enter to switch.",
        Kind::Glitch(_) => "One glitch layer: which sounds, how often, how loud. Enter to open.",
        Kind::CodeEnabled => "Scroll this shrine's own source code through the sky. Enter to switch.",
        Kind::CodeOpacity => "How strongly the code shows.",
        Kind::CodeSpeed => "How fast the code scrolls.",
        Kind::CodeScale => "The code's text size.",
        Kind::CodeAngle => "Tilt the code's drift.",
        Kind::CodeOffset => "Move the code up or down the sky.",
        Kind::CodePlane => "How the code answers the camera: pinned, sliding, or wrapped round you.",
        Kind::CameraSpeed => "The camera's constant orbit, degrees per second (negative turns the other way).",
        Kind::ArchRotX | Kind::ArchRotY | Kind::ArchRotZ => "Turn the architecture on one axis; the ground stays put.",
        Kind::ArchOffX | Kind::ArchOffY | Kind::ArchOffZ => "Move the architecture, in cells or levels.",
        Kind::EditArch => "Edit the shrine square by square. Enter to open.",
        Kind::SeedN(_) => "One of the music's seeds. ←→ step, shift ±100, enter to type.",
        Kind::SavePng => "Save the picture as it is now.",
        Kind::CoverPng => "Save a 1600×1600 cover: the start of the piece, unturned (as the WAV embeds it).",
        Kind::WavExport => "Render the music to a WAV with metadata and cover art.",
        Kind::ExportSeed => "Write these settings to a .shrineseed file.",
        Kind::ImportSeed => "Load settings from a .shrineseed file.",
        Kind::Reset => "Start a fresh project from the defaults.",
        Kind::Compile => "Build the self-contained web artifact from these settings.",
        Kind::Quit => "Save the answers and leave.",
    }
}

/// The Randomize screen: which groups to roll (remembered for the session),
/// the cursor, and the settings before the last roll (for undo).
struct RandomForm {
    sel: usize,
    chosen: [bool; shrine_explorer::randomize::GROUPS.len()],
    before: Option<SimConfig>,
}

impl RandomForm {
    fn new() -> Self {
        // Everything, to begin with.
        RandomForm { sel: 0, chosen: [true; shrine_explorer::randomize::GROUPS.len()], before: None }
    }

    /// The rows after the groups.
    const ACTIONS: [&'static str; 3] = ["Randomize now", "Undo last roll", "Back"];

    fn len(&self) -> usize {
        self.chosen.len() + Self::ACTIONS.len()
    }
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
            filename: format!("artifacts/ShrineGen-{seed}.wav"),
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
    picker: Option<Picker>,
    rform: RandomForm,
    focus: Focus,
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
            status: "ready — ↑↓ section · → open · esc quit".to_string(),
            job: None,
            prompt: None,
            audition: None,
            screen: Screen::Main,
            wav: WavForm::new(seed),
            gform: GlitchForm::new(),
            edit: EditForm::new(),
            fx: FxForm::new(),
            picker: None,
            rform: RandomForm::new(),
            focus: Focus::Sections,
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
        self.rows_of(&self.config)
    }

    /// The menu for any configuration (a picker labels its options with it).
    fn rows_of(&self, c: &SimConfig) -> Vec<Row> {
        let s = &c.settings;
        let mut rows: Vec<Row> = Vec::new();
        let head = |label: &str, rows: &mut Vec<Row>| {
            rows.push(Row { label: label.to_string(), value: String::new(), kind: Kind::Header });
        };
        let field = |label: String, value: String, kind: Kind, rows: &mut Vec<Row>| {
            rows.push(Row { label, value, kind });
        };

        head("Track", &mut rows);
        field("Randomize…".into(), "choose what to roll".into(), Kind::Randomize, &mut rows);
        field(
            "Map position".into(),
            if s.map == (0, 0) { "home · 0, 0".into() } else { format!("{}, {}", s.map.0, s.map.1) },
            Kind::MapPos,
            &mut rows,
        );
        if s.map != (0, 0) {
            field("Make this shrine home".into(), "re-centre the map here".into(), Kind::MakeHome, &mut rows);
        }
        field(
            "Track name".into(),
            if s.name.is_empty() { "(unnamed)".into() } else { s.name.as_str().to_string() },
            Kind::Name,
            &mut rows,
        );
        field(
            "Start at".into(),
            if s.start_s == 0 { "the beginning".into() } else { format!("{} s · {}:{:02} in", s.start_s, s.start_s / 60, s.start_s % 60) },
            Kind::StartAt,
            &mut rows,
        );

        head("Shrine", &mut rows);
        field("Seed".into(), c.seed.to_string(), Kind::Seed, &mut rows);
        field("Biome".into(), c.biome.name().to_string(), Kind::Biome, &mut rows);
        field("Weather".into(), format!("{} · {}", c.weather.name(), c.weather.blurb()), Kind::Weather, &mut rows);
        field(
            "Wildlife".into(),
            if c.settings.wildlife {
                format!("on · {}", shrine_explorer::wildlife::Wildlife::new(c.seed, c.biome).summary())
            } else {
                "off".into()
            },
            Kind::Wildlife,
            &mut rows,
        );
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
        field("Mode".into(), s.pace.name().to_string(), Kind::Pace, &mut rows);
        if s.pace == Pace::Garage {
            field(
                "Swing".into(),
                format!("{}%{}", s.garage.swing, if s.garage.swing == 50 { " · straight" } else { "" }),
                Kind::Swing,
                &mut rows,
            );
            field(
                "Progression".into(),
                match s.garage.progression as usize {
                    0 => "seed picks".to_string(),
                    i => {
                        let p = &shrine_explorer::harmony::PROGRESSIONS[i - 1];
                        format!("{} · {}", p.name, p.numerals)
                    }
                },
                Kind::GarageProgression,
                &mut rows,
            );
            field("Key lift".into(), s.garage.lift.name().to_string(), Kind::GarageLift, &mut rows);
            field("Bass".into(), s.garage.bass.name().to_string(), Kind::GarageBass, &mut rows);
            field("Stabs".into(), s.garage.stabs.name().to_string(), Kind::GarageStabs, &mut rows);
            if s.garage.stabs != shrine_explorer::garage::Stabs::Off {
                field("Stab tone".into(), s.garage.tone.name().to_string(), Kind::GarageTone, &mut rows);
            }
        }
        if s.pace == Pace::Micro {
            field("Pulse".into(), s.micro.pulse.name().to_string(), Kind::MicroPulse, &mut rows);
            field("Grains".into(), s.micro.grains.name().to_string(), Kind::MicroGrains, &mut rows);
            field("Pips".into(), s.micro.pips.name().to_string(), Kind::MicroPips, &mut rows);
            field("Dropouts".into(), s.micro.dropouts.name().to_string(), Kind::MicroDropouts, &mut rows);
            field("Chord rate".into(), s.micro.chords.name().to_string(), Kind::MicroChords, &mut rows);
            field("Drone shift".into(), s.micro.shift.name().to_string(), Kind::MicroShift, &mut rows);
        }
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
        field("Save PNG".into(), format!("artifacts/shrine-{}.png", c.seed), Kind::SavePng, &mut rows);
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

    /// The next selectable row within the selection's tab (wrapping).
    fn menu_step(&self, from: usize, dir: i32) -> usize {
        let rows = self.rows();
        let tabs = tabs_of(&rows);
        let n = rows.len() as i32;
        let tab = tabs[from.min(rows.len() - 1)];
        let mut i = from as i32;
        for _ in 0..n {
            i = (i + dir).rem_euclid(n);
            if rows[i as usize].kind != Kind::Header && tabs[i as usize] == tab {
                return i as usize;
            }
        }
        from
    }

    /// The tab the selection is in.
    fn tab(&self) -> Tab {
        let rows = self.rows();
        tabs_of(&rows)[self.sel.min(rows.len() - 1)]
    }

    /// Select the first row of `tab`.
    fn goto_tab(&mut self, tab: Tab) {
        let rows = self.rows();
        let tabs = tabs_of(&rows);
        if let Some(i) = (0..rows.len()).find(|&i| tabs[i] == tab && rows[i].kind != Kind::Header) {
            self.sel = i;
        }
    }

    fn switch_tab(&mut self, dir: i32) {
        let i = TABS.iter().position(|t| *t == self.tab()).unwrap_or(0) as i32;
        self.goto_tab(TABS[(i + dir).rem_euclid(TABS.len() as i32) as usize]);
    }

    /// Every value a choice row can take, as whole configurations (in the
    /// order of its list), or `None` for a row that is not a choice.
    fn choices(&self, kind: Kind) -> Option<Vec<SimConfig>> {
        let base = &self.config;
        let each = |n: usize, set: &dyn Fn(&mut SimConfig, usize)| -> Vec<SimConfig> {
            (0..n)
                .map(|i| {
                    let mut c = base.clone();
                    set(&mut c, i);
                    c
                })
                .collect()
        };
        use shrine_explorer::{garage as g, micro as m};
        Some(match kind {
            Kind::Biome => each(Biome::ALL.len(), &|c, i| c.biome = Biome::ALL[i]),
            Kind::Weather => each(WeatherKind::ALL.len(), &|c, i| c.weather = WeatherKind::ALL[i]),
            Kind::Scale => each(SCALES.len(), &|c, i| c.settings.scale = SCALES[i]),
            Kind::Key => each(13, &|c, i| c.settings.key = key_from_index(i)),
            Kind::Chords => each(4, &|c, i| c.settings.chords = i as u8 + 1),
            Kind::Pace => each(PACES.len(), &|c, i| {
                let s = &mut c.settings;
                s.set_mode(PACES[i]);
                if s.pace == Pace::Micro && !s.kit.is_light() && s.kit != shrine_explorer::kits::Kit::Off {
                    s.kit = shrine_explorer::kits::Kit::MicroBlips;
                }
            }),
            Kind::GarageProgression => each(shrine_explorer::harmony::PROGRESSIONS.len() + 1, &|c, i| {
                c.settings.garage.progression = i as u8;
                // A named progression needs all four chords.
                if i > 0 {
                    c.settings.chords = 4;
                }
            }),
            Kind::GarageLift => each(g::KEY_LIFTS.len(), &|c, i| c.settings.garage.lift = g::KEY_LIFTS[i]),
            Kind::GarageBass => each(g::BASS_MODES.len(), &|c, i| c.settings.garage.bass = g::BASS_MODES[i]),
            Kind::GarageStabs => each(g::STABS.len(), &|c, i| c.settings.garage.stabs = g::STABS[i]),
            Kind::GarageTone => each(g::STAB_TONES.len(), &|c, i| c.settings.garage.tone = g::STAB_TONES[i]),
            Kind::MicroPulse => each(m::PULSES.len(), &|c, i| c.settings.micro.pulse = m::PULSES[i]),
            Kind::MicroGrains => each(m::GRAINS.len(), &|c, i| c.settings.micro.grains = m::GRAINS[i]),
            Kind::MicroPips => each(m::PIPS.len(), &|c, i| c.settings.micro.pips = m::PIPS[i]),
            Kind::MicroDropouts => each(m::DROPOUTS.len(), &|c, i| c.settings.micro.dropouts = m::DROPOUTS[i]),
            Kind::MicroChords => each(m::CHORD_RATES.len(), &|c, i| c.settings.micro.chords = m::CHORD_RATES[i]),
            Kind::MicroShift => each(m::DRONE_SHIFTS.len(), &|c, i| c.settings.micro.shift = m::DRONE_SHIFTS[i]),
            Kind::Kit => {
                // Microsound only offers the light, small kits.
                let micro = base.settings.pace == Pace::Micro;
                let kits: Vec<_> = KITS.into_iter().filter(|k| !micro || k.is_light() || *k == shrine_explorer::kits::Kit::Off).collect();
                each(kits.len(), &|c, i| c.settings.kit = kits[i])
            }
            Kind::Space => each(SPACES.len(), &|c, i| c.settings.space = SPACES[i]),
            Kind::Layer(l) => each(LOOP_TIMBRES.len() + 1, &|c, i| {
                // The event grid keeps a grid already drawn on this layer.
                let keep = matches!(c.settings.loops[l], LoopDesign::Events(_)) && i == EVENT_GRID;
                if !keep {
                    c.settings.loops[l] = index_to_design(i);
                }
            }),
            Kind::LayerArc(l) => each(ARCS.len(), &|c, i| c.settings.loop_arcs[l].cycle_s = ARCS[i]),
            Kind::DroneArc => each(ARCS.len(), &|c, i| c.settings.drone.cycle_s = ARCS[i]),
            Kind::DroneTone => each(TONES.len(), &|c, i| c.settings.drone_tone = TONES[i]),
            Kind::CodePlane => each(CodeOverlay::PLANES.len(), &|c, i| c.settings.code.plane = i as u8),
            _ => return None,
        })
    }

    /// A one-line description of `kind`'s value in `c`, for its picker.
    fn option_blurb(kind: Kind, c: &SimConfig) -> String {
        let s = &c.settings;
        match kind {
            Kind::Biome => c.biome.blurb().to_string(),
            Kind::Scale => s.scale.mood().to_string(),
            Kind::Key => match s.key {
                KeyChoice::Seed => "seed 1 chooses the tonic".to_string(),
                KeyChoice::Note(_) => String::new(),
            },
            Kind::Chords => match s.chords {
                1 => "one chord, held".to_string(),
                n => format!("{n} chords in turn"),
            },
            Kind::Pace => s.pace.blurb().to_string(),
            Kind::GarageProgression if s.garage.progression == 0 => "the seed picks one from the scale's family".to_string(),
            Kind::GarageLift => s.garage.lift.blurb().to_string(),
            Kind::GarageBass => s.garage.bass.blurb().to_string(),
            Kind::GarageStabs => s.garage.stabs.blurb().to_string(),
            Kind::GarageTone => s.garage.tone.blurb().to_string(),
            Kind::MicroPulse => s.micro.pulse.blurb().to_string(),
            Kind::MicroGrains => s.micro.grains.blurb().to_string(),
            Kind::MicroPips => s.micro.pips.blurb().to_string(),
            Kind::MicroDropouts => s.micro.dropouts.blurb().to_string(),
            Kind::MicroChords => s.micro.chords.blurb().to_string(),
            Kind::MicroShift => s.micro.shift.blurb().to_string(),
            Kind::Kit => s.kit.blurb().to_string(),
            Kind::Space => s.space.blurb().to_string(),
            Kind::Layer(l) => s.loops[l].blurb(),
            Kind::LayerArc(_) | Kind::DroneArc => "how long one in-and-out cycle lasts".to_string(),
            Kind::DroneTone => "the low-pass cutoff (off leaves the drone open)".to_string(),
            Kind::CodePlane => ["pinned to the frame", "a flat band that slides as the camera turns", "wrapped round the viewer"]
                [(s.code.plane as usize).min(2)]
                .to_string(),
            _ => String::new(),
        }
    }

    /// Open the picker for the selected row, if it is a choice.
    fn open_picker(&mut self) -> bool {
        let row = &self.rows()[self.sel];
        let (kind, title) = (row.kind, row.label.clone());
        let Some(configs) = self.choices(kind) else {
            return false;
        };
        let label = |c: &SimConfig| self.rows_of(c).into_iter().find(|r| r.kind == kind).map(|r| r.value).unwrap_or_default();
        let current = label(&self.config);
        let options: Vec<(String, String, SimConfig)> =
            configs.into_iter().map(|c| (label(&c), App::option_blurb(kind, &c), c)).collect();
        let sel = options.iter().position(|o| o.0 == current).unwrap_or(0);
        self.picker = Some(Picker { kind, title, options, sel, original: self.config.clone() });
        true
    }

    /// Move the picker to option `i` and hear/see it straight away.
    fn picker_to(&mut self, i: usize) {
        let Some(p) = &mut self.picker else { return };
        let i = i.min(p.options.len() - 1);
        if i == p.sel && p.options[i].2.settings == self.config.settings {
            return;
        }
        p.sel = i;
        self.config = p.options[i].2.clone();
        self.rebuild();
    }

    fn handle_picker_key(&mut self, code: KeyCode) {
        let Some(p) = &self.picker else { return };
        let (n, sel) = (p.options.len(), p.sel);
        match code {
            KeyCode::Up | KeyCode::Left => self.picker_to((sel + n - 1) % n),
            KeyCode::Down | KeyCode::Right => self.picker_to((sel + 1) % n),
            KeyCode::PageUp => self.picker_to(sel.saturating_sub(8)),
            KeyCode::PageDown => self.picker_to(sel + 8),
            KeyCode::Home => self.picker_to(0),
            KeyCode::End => self.picker_to(n - 1),
            KeyCode::Enter | KeyCode::Char(' ') => {
                let p = self.picker.take().unwrap();
                self.status = format!("{} · {}", p.title, p.options[p.sel].0);
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                let p = self.picker.take().unwrap();
                self.config = p.original;
                self.rebuild();
            }
            // A letter jumps to the next option starting with it.
            KeyCode::Char(ch) if ch.is_alphanumeric() => {
                let want = ch.to_ascii_lowercase();
                let hit = (1..=n).map(|k| (sel + k) % n).find(|&i| p.options[i].0.chars().next().is_some_and(|c| c.to_ascii_lowercase() == want));
                if let Some(i) = hit {
                    self.picker_to(i);
                }
            }
            _ => {}
        }
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
                // Each mode moves the tempo, scale and chords that were still
                // at the old mode's defaults; anything chosen by hand is kept.
                let s = &mut self.config.settings;
                s.set_mode(cycle(&PACES, s.pace, forward));
                if s.pace == Pace::Micro && !s.kit.is_light() && s.kit != shrine_explorer::kits::Kit::Off {
                    s.kit = shrine_explorer::kits::Kit::MicroBlips;
                }
            }
            Kind::GarageProgression => {
                let g = &mut self.config.settings.garage;
                let n = shrine_explorer::harmony::PROGRESSIONS.len() as u8 + 1;
                g.progression = if forward { (g.progression + 1) % n } else { (g.progression + n - 1) % n };
                // A named progression needs all four chords.
                if g.progression > 0 {
                    self.config.settings.chords = 4;
                }
            }
            Kind::GarageLift => {
                let g = &mut self.config.settings.garage;
                g.lift = cycle(&shrine_explorer::garage::KEY_LIFTS, g.lift, forward);
            }
            Kind::GarageBass => {
                let g = &mut self.config.settings.garage;
                g.bass = cycle(&shrine_explorer::garage::BASS_MODES, g.bass, forward);
            }
            Kind::GarageStabs => {
                let g = &mut self.config.settings.garage;
                g.stabs = cycle(&shrine_explorer::garage::STABS, g.stabs, forward);
            }
            Kind::GarageTone => {
                let g = &mut self.config.settings.garage;
                g.tone = cycle(&shrine_explorer::garage::STAB_TONES, g.tone, forward);
            }
            Kind::Swing => {
                let g = &mut self.config.settings.garage;
                let step = if shift { 4 } else { 1 };
                g.swing = if forward { (g.swing + step).min(shrine_explorer::garage::MAX_SWING) } else { g.swing.saturating_sub(step).max(shrine_explorer::garage::MIN_SWING) };
            }
            Kind::Bpm => {
                let b = self.config.settings.bpm;
                self.config.settings.bpm = if forward {
                    (b + if shift { 10 } else { 1 }).min(180)
                } else {
                    b.saturating_sub(if shift { 10 } else { 1 }).max(70)
                };
            }
            Kind::Kit => {
                // Microsound only makes sense with the light, small kits.
                let s = &mut self.config.settings;
                let kits: Vec<_> = KITS.into_iter().filter(|k| s.pace != Pace::Micro || k.is_light() || *k == shrine_explorer::kits::Kit::Off).collect();
                s.kit = cycle(&kits, s.kit, forward);
            }
            Kind::MicroPulse => {
                let m = &mut self.config.settings.micro;
                m.pulse = cycle(&shrine_explorer::micro::PULSES, m.pulse, forward);
            }
            Kind::MicroGrains => {
                let m = &mut self.config.settings.micro;
                m.grains = cycle(&shrine_explorer::micro::GRAINS, m.grains, forward);
            }
            Kind::MicroPips => {
                let m = &mut self.config.settings.micro;
                m.pips = cycle(&shrine_explorer::micro::PIPS, m.pips, forward);
            }
            Kind::MicroDropouts => {
                let m = &mut self.config.settings.micro;
                m.dropouts = cycle(&shrine_explorer::micro::DROPOUTS, m.dropouts, forward);
            }
            Kind::MicroChords => {
                let m = &mut self.config.settings.micro;
                m.chords = cycle(&shrine_explorer::micro::CHORD_RATES, m.chords, forward);
            }
            Kind::MicroShift => {
                let m = &mut self.config.settings.micro;
                m.shift = cycle(&shrine_explorer::micro::DRONE_SHIFTS, m.shift, forward);
            }
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
            Kind::Wildlife => self.config.settings.wildlife = !self.config.settings.wildlife,
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
            Kind::StartAt => {
                let step = if shift { 30 } else { 5 };
                let v = self.config.settings.start_s as i32 + if forward { step } else { -step };
                self.config.settings.start_s = v.clamp(0, shrine_explorer::track::MAX_START_S as i32) as u16;
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
            Kind::MapPos => {
                // ←→ step x; with shift, y.
                let d = if forward { 1 } else { -1 };
                let m = &mut self.config.settings.map;
                if shift {
                    m.1 = m.1.saturating_add(d);
                } else {
                    m.0 = m.0.saturating_add(d);
                }
                self.status = map_status(*m);
            }
            Kind::MakeHome => return,
            Kind::Randomize => {
                self.screen = Screen::Randomize;
                return;
            }
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
        // Every choice opens its list.
        if self.open_picker() {
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
            Kind::Randomize => {
                self.screen = Screen::Randomize;
            }
            Kind::MapPos => {
                let (x, y) = self.config.settings.map;
                self.prompt = Some(Prompt { label: "Map position (x, y)".into(), value: format!("{x}, {y}"), action: PromptAction::MapPosition });
            }
            Kind::MakeHome => self.make_home(),
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
            Kind::Swing => s.garage.swing.to_string(),
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
            Kind::StartAt => s.start_s.to_string(),
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
            Kind::Swing => c.settings.garage.swing = n.clamp(shrine_explorer::garage::MIN_SWING as i64, shrine_explorer::garage::MAX_SWING as i64) as u8,
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
            Kind::StartAt => c.settings.start_s = n.clamp(0, shrine_explorer::track::MAX_START_S as i64) as u16,
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
            PromptAction::MapPosition => match parse_map(&text) {
                Some(m) => {
                    self.config.settings.map = m;
                    self.rebuild();
                    self.status = map_status(m);
                }
                None => self.status = format!("map position: \"{text}\" is not x, y"),
            },
            PromptAction::Name => {
                self.config.settings.name = shrine_explorer::name::Name::from_str(&text);
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

    /// The shrine at the map position becomes the home, at 0, 0.
    fn make_home(&mut self) {
        let (x, y) = self.config.settings.map;
        if (x, y) == (0, 0) {
            return;
        }
        self.config = shrine_explorer::explore::at(&self.config, x, y);
        // The selection may now point past the removed row.
        self.sel = self.rows().iter().position(|r| r.kind == Kind::MapPos).unwrap_or(self.sel);
        self.rebuild();
        self.status = format!("the shrine at {x}, {y} is home now");
    }

    /// Roll the chosen groups (keeping what was there, for undo).
    fn roll(&mut self) {
        let groups: Vec<shrine_explorer::randomize::Group> =
            shrine_explorer::randomize::GROUPS.iter().zip(self.rform.chosen).filter(|(_, on)| *on).map(|(g, _)| *g).collect();
        if groups.is_empty() {
            self.status = "nothing chosen to randomize".into();
            return;
        }
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
        let mut rng = shrine_explorer::rng::Rng::new(nanos);
        self.rform.before = Some(self.config.clone());
        shrine_explorer::randomize::apply(&mut self.config, &groups, &mut rng);
        self.rebuild();
        self.status = format!("rolled {}", groups.iter().map(|g| g.name().to_lowercase()).collect::<Vec<_>>().join(", "));
    }

    fn handle_random_key(&mut self, code: KeyCode) {
        let n = self.rform.len();
        let groups = self.rform.chosen.len();
        let sel = self.rform.sel;
        match code {
            KeyCode::Esc | KeyCode::Char('q') => self.screen = Screen::Main,
            KeyCode::Up => self.rform.sel = (sel + n - 1) % n,
            KeyCode::Down => self.rform.sel = (sel + 1) % n,
            KeyCode::Char('a') => self.rform.chosen = [true; shrine_explorer::randomize::GROUPS.len()],
            KeyCode::Char('n') => self.rform.chosen = [false; shrine_explorer::randomize::GROUPS.len()],
            KeyCode::Char('r') => self.roll(),
            KeyCode::Char('u') => self.undo_roll(),
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right => match sel {
                i if i < groups => self.rform.chosen[i] = !self.rform.chosen[i],
                i if i == groups => self.roll(),
                i if i == groups + 1 => self.undo_roll(),
                _ => self.screen = Screen::Main,
            },
            _ => {}
        }
    }

    fn undo_roll(&mut self) {
        match self.rform.before.take() {
            Some(c) => {
                self.config = c;
                self.rebuild();
                self.status = "undid the last roll".into();
            }
            None => self.status = "nothing to undo".into(),
        }
    }

    fn draw_random(&self, f: &mut Frame) {
        let area = f.area();
        let buf = f.buffer_mut();
        if area.width < 30 || area.height < 12 {
            put(buf, area, 0, 0, "terminal too small", style(TEXT));
            return;
        }
        let list_w = (area.width / 2).clamp(24, 46);
        let help_y = area.height - 1;
        let about_y = area.height - 2;
        put(buf, area, 0, 0, concat!("ShrineGen · randomize · v", env!("BUILD_DATE")), style(ACCENT).add_modifier(Modifier::BOLD));
        let status_x = area.width.saturating_sub(self.status.chars().count() as u16 + 1);
        put(buf, area, status_x, 0, &self.status, style(DIM));
        put(buf, area, 1, 1, "CHOOSE WHAT TO ROLL", style(DIM).add_modifier(Modifier::BOLD));
        let groups = shrine_explorer::randomize::GROUPS;
        let list_h = about_y.saturating_sub(3) as usize;
        let scroll = menu_scroll(self.rform.sel, self.rform.len(), list_h);
        for i in (0..self.rform.len()).skip(scroll).take(list_h) {
            let y = 2 + (i - scroll) as u16 + if i >= groups.len() { 1 } else { 0 };
            if y >= about_y {
                break;
            }
            let selected = i == self.rform.sel;
            let marker = if selected { "›" } else { " " };
            let st = if selected { style(SELECT).add_modifier(Modifier::BOLD) } else { style(TEXT) };
            if i < groups.len() {
                let mark = if self.rform.chosen[i] { "[x]" } else { "[ ]" };
                put(buf, area, 0, y, &format!("{marker} {mark} {}", groups[i].name()), st);
            } else {
                let label = RandomForm::ACTIONS[i - groups.len()];
                let st = if i == groups.len() { st.fg(color(ACCENT)) } else { st };
                put(buf, area, 0, y, &format!("{marker} {label}"), st);
            }
        }
        let preview = Rect::new(list_w, 1, area.width - list_w, about_y.saturating_sub(1));
        draw_preview(buf, preview, &self.frame);
        let about = match self.rform.sel {
            i if i < groups.len() => groups[i].blurb(),
            i if i == groups.len() => "roll every ticked group now (or press r anywhere here)",
            i if i == groups.len() + 1 => "put back the settings from before the last roll",
            _ => "back to the setup",
        };
        put(buf, area, 0, about_y, &format!(" {about}"), style(DIM));
        put(buf, area, 0, help_y, "↑↓ move · space tick · r roll · u undo · a all · n none · esc back", style(FAINT));
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
        let out = format!("artifacts/shrine-{}.png", self.config.seed);
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
        let out = format!("artifacts/ShrineGen-{}-cover.png", self.config.seed);
        // The same picture the WAV export embeds: the start, unturned.
        self.status = match preview::write_cover(&preview::cover_frame(&self.config), &out, COVER_SIZE) {
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
                self.config.settings.name = shrine_explorer::name::Name::from_str(&text);
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
        if self.picker.is_some() {
            self.handle_picker_key(code);
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
        if self.screen == Screen::Randomize {
            self.handle_random_key(code);
            return self.quit;
        }
        // Keys that work wherever the focus is.
        match code {
            KeyCode::Char('q') => {
                state::save(&self.config);
                return true;
            }
            KeyCode::Char(d @ '1'..='8') => {
                self.goto_tab(TABS[d as usize - '1' as usize]);
                self.focus = Focus::Settings;
                return self.quit;
            }
            KeyCode::Char('r') if self.focus == Focus::Settings => {
                self.randomize();
                return self.quit;
            }
            KeyCode::Char('R') => {
                self.screen = Screen::Randomize;
                return self.quit;
            }
            _ => {}
        }
        match self.focus {
            // The section list: pick one, then step into it.
            Focus::Sections => match code {
                KeyCode::Esc => {
                    state::save(&self.config);
                    return true;
                }
                KeyCode::Up | KeyCode::BackTab => self.switch_tab(-1),
                KeyCode::Down | KeyCode::Tab => self.switch_tab(1),
                KeyCode::Right | KeyCode::Enter | KeyCode::Char(' ') => self.focus = Focus::Settings,
                KeyCode::Char('r') => self.randomize(),
                _ => {}
            },
            // A section's settings.
            Focus::Settings => match code {
                KeyCode::Esc | KeyCode::Backspace | KeyCode::Tab => self.focus = Focus::Sections,
                KeyCode::BackTab => self.switch_tab(-1),
                KeyCode::Up => self.sel = self.menu_step(self.sel, -1),
                KeyCode::Down => self.sel = self.menu_step(self.sel, 1),
                KeyCode::Left => self.adjust(false, mods.contains(KeyModifiers::SHIFT)),
                KeyCode::Right => self.adjust(true, mods.contains(KeyModifiers::SHIFT)),
                KeyCode::Enter | KeyCode::Char(' ') => self.activate(),
                _ => {}
            },
        }
        self.quit
    }

    fn draw(&self, f: &mut Frame) {
        match self.screen {
            Screen::Wav => self.draw_wav(f),
            Screen::Fx => self.draw_fx(f),
            Screen::Glitch => self.draw_glitch(f),
            Screen::Editor => self.draw_editor(f),
            Screen::Randomize => self.draw_random(f),
            Screen::Main => self.draw_main(f),
        }
        // A prompt can be open over any screen, so it is always drawn last.
        if let Some(p) = &self.prompt {
            let area = f.area();
            let buf = f.buffer_mut();
            let w = (p.label.chars().count().max(p.value.chars().count() + 2).max(34) as u16 + 4).min(area.width);
            let rect = Rect::new((area.width - w) / 2, area.height / 2 - 2, w, 5.min(area.height));
            let inner = draw_box(buf, rect, &p.label);
            put(buf, inner, 1, 0, &format!("{}▏", p.value), style(SELECT).bg(color(POPUP)));
            put(buf, inner, 1, 2, "enter confirm · esc cancel", style(FAINT).bg(color(POPUP)));
        }
    }

    fn draw_main(&self, f: &mut Frame) {
        let area = f.area();
        let buf = f.buffer_mut();
        if area.width < 50 || area.height < 12 {
            put(buf, area, 0, 0, "terminal too small", style(TEXT));
            return;
        }
        let help_y = area.height - 1;
        let about_y = area.height - 2;
        let top = 2u16;
        put(buf, area, 0, 0, concat!("ShrineGen · setup · v", env!("BUILD_DATE")), style(ACCENT).add_modifier(Modifier::BOLD));
        let status_x = area.width.saturating_sub(self.status.chars().count() as u16 + 1);
        put(buf, area, status_x, 0, &self.status, style(DIM));

        // Three columns: the sections, the chosen section's settings, the picture.
        let wide = area.width >= 100;
        let side_w: u16 = if wide { 26 } else { 13 };
        let set_w = ((area.width - side_w) / 2).clamp(32, 50).min(area.width - side_w - 4);
        let set_x = side_w + 1;
        let tab = self.tab();
        let in_sections = self.focus == Focus::Sections;

        // The sidebar.
        put(buf, area, 1, 1, "SECTIONS", style(if in_sections { ACCENT } else { DIM }).add_modifier(Modifier::BOLD));
        for (k, t) in TABS.iter().enumerate() {
            let y = top + k as u16;
            if y >= about_y {
                break;
            }
            let on = *t == tab;
            let (name_st, sum_st) = if on && in_sections {
                let hi = Style::new().fg(color((20, 20, 24))).bg(color(SELECT)).add_modifier(Modifier::BOLD);
                (hi, Style::new().fg(color((40, 36, 30))).bg(color(SELECT)))
            } else if on {
                (style(SELECT).add_modifier(Modifier::BOLD), style(DIM))
            } else {
                (style(TEXT), style(FAINT))
            };
            let width = side_w as usize - 1;
            let name = format!("{} {:<11}", if on { "›" } else { " " }, t.name());
            let line = if wide {
                let summary: String = section_summary(*t, &self.config).chars().take(width.saturating_sub(13)).collect();
                format!("{name}{summary:<w$}", w = width - 13)
            } else {
                name.chars().take(width).collect()
            };
            // Name and summary in their own styles, over one highlighted bar.
            put(buf, area, 0, y, &format!("{line:<width$}"), sum_st);
            put(buf, area, 0, y, &name.chars().take(width).collect::<String>(), name_st);
        }
        for y in 1..about_y {
            put_char(buf, area, side_w, y, '│', style(FAINT));
        }

        // The settings of the chosen section. A section's first group heading
        // is the section itself, so only the later ones are shown.
        let rows = self.rows();
        let tabs = tabs_of(&rows);
        let mut shown: Vec<usize> = (0..rows.len()).filter(|&i| tabs[i] == tab).collect();
        if shown.first().is_some_and(|&i| rows[i].kind == Kind::Header) {
            shown.remove(0);
        }
        let col = Rect::new(area.x + set_x, area.y, set_w, area.height);
        put(buf, col, 1, 1, &tab.name().to_uppercase(), style(if in_sections { DIM } else { ACCENT }).add_modifier(Modifier::BOLD));
        let list_h = about_y.saturating_sub(top + 1) as usize;
        let at = shown.iter().position(|&i| i == self.sel).unwrap_or(0);
        let scroll = menu_scroll(at, shown.len(), list_h);
        let label_w = 20usize.min(set_w as usize / 2);
        for (k, &i) in shown.iter().enumerate().skip(scroll).take(list_h) {
            let row = &rows[i];
            let y = top + (k - scroll) as u16;
            let selected = i == self.sel && !in_sections;
            match row.kind {
                Kind::Header => put(buf, col, 1, y, &row.label.to_uppercase(), style(DIM).add_modifier(Modifier::BOLD)),
                _ => {
                    let marker = if selected { "›" } else { " " };
                    let label_style = if selected {
                        style(SELECT).add_modifier(Modifier::BOLD)
                    } else if i == self.sel {
                        style(TEXT).add_modifier(Modifier::BOLD)
                    } else {
                        style(TEXT)
                    };
                    let label: String = row.label.chars().take(label_w).collect();
                    put(buf, col, 0, y, &format!("{marker} {label:<label_w$}"), label_style);
                    let value = if self.choices(row.kind).is_some() { format!("{} ▾", row.value) } else { row.value.clone() };
                    put(buf, col, label_w as u16 + 3, y, &value, style(if selected { SELECT } else { DIM }));
                }
            }
        }

        let px = set_x + set_w + 1;
        let preview = Rect::new(px, 1, area.width.saturating_sub(px), about_y.saturating_sub(1));
        draw_preview(buf, preview, &self.frame);

        // What the focus is on, and the keys for it.
        let about = if in_sections {
            let n = shown.iter().filter(|&&i| rows[i].kind != Kind::Header).count();
            format!(" {} — {} settings · → or enter to open", tab.name(), n)
        } else {
            rows.get(self.sel).map(|r| format!(" {}", row_help(r.kind))).unwrap_or_default()
        };
        put(buf, area, 0, about_y, &about, style(DIM));
        if let Some(job) = &self.job {
            draw_progress(buf, area, help_y, job);
        } else {
            let keys = if in_sections {
                "↑↓ section · → / enter open · 1-8 jump · R randomize… · esc quit"
            } else {
                "↑↓ setting · enter choose · ←→ change (shift = bigger) · r random · esc back to sections"
            };
            put(buf, area, 0, help_y, keys, style(FAINT));
        }
        if let Some(p) = &self.picker {
            draw_picker(buf, area, p, set_x);
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

/// The popups' background.
const POPUP: Rgb = (24, 23, 30);

/// Clear `rect`, frame it with a titled border, and return the inside.
fn draw_box(buf: &mut Buffer, rect: Rect, title: &str) -> Rect {
    let bg = Style::new().bg(color(POPUP));
    let edge = style(ACCENT).bg(color(POPUP));
    for y in 0..rect.height {
        for x in 0..rect.width {
            let ch = match (x, y) {
                (0, 0) => '┌',
                (x, 0) if x == rect.width - 1 => '┐',
                (0, y) if y == rect.height - 1 => '└',
                (x, y) if x == rect.width - 1 && y == rect.height - 1 => '┘',
                (_, 0) => '─',
                (_, y) if y == rect.height - 1 => '─',
                (0, _) => '│',
                (x, _) if x == rect.width - 1 => '│',
                _ => ' ',
            };
            put_char(buf, rect, x, y, ch, if ch == ' ' { bg } else { edge });
        }
    }
    put(buf, rect, 2, 0, &format!(" {title} "), style(ACCENT).bg(color(POPUP)).add_modifier(Modifier::BOLD));
    Rect::new(rect.x + 1, rect.y + 1, rect.width.saturating_sub(2), rect.height.saturating_sub(2))
}

const PICKER_KEYS: &str = "↑↓ choose (heard live) · type to jump · enter keep · esc put back";

/// A setting's list of values: each with its description, the highlighted
/// one's in full underneath.
fn draw_picker(buf: &mut Buffer, area: Rect, p: &Picker, x0: u16) {
    let label_w = p.options.iter().map(|o| o.0.chars().count()).max().unwrap_or(4).min(40);
    let blurb_w = p.options.iter().map(|o| o.1.chars().count()).max().unwrap_or(0).min(56);
    let help_w = row_help(p.kind).chars().count().max(PICKER_KEYS.chars().count()) + 4;
    let w = ((label_w + 4 + blurb_w + 3).max(p.title.chars().count() + 8).max(help_w).max(40) as u16).min(area.width);
    let max_list = area.height.saturating_sub(10) as usize;
    let list_h = p.options.len().min(max_list.max(3));
    let h = (list_h as u16 + 5).min(area.height.saturating_sub(2));
    let x0 = x0.min(area.width.saturating_sub(w));
    let rect = Rect::new(x0, 2.min(area.height), w, h);
    let inner = draw_box(buf, rect, &p.title);
    let bg = color(POPUP);
    let scroll = menu_scroll(p.sel, p.options.len(), list_h);
    for (k, (label, blurb, _)) in p.options.iter().enumerate().skip(scroll).take(list_h) {
        let y = (k - scroll) as u16;
        let on = k == p.sel;
        let st = if on { Style::new().fg(color((20, 20, 24))).bg(color(SELECT)).add_modifier(Modifier::BOLD) } else { style(TEXT).bg(bg) };
        let text: String = label.chars().take(label_w).collect();
        put(buf, inner, 0, y, &format!("{} {:<label_w$} ", if on { "›" } else { " " }, text), st);
        put(buf, inner, label_w as u16 + 3, y, blurb, style(if on { TEXT } else { DIM }).bg(bg));
    }
    if scroll > 0 {
        put_char(buf, inner, inner.width.saturating_sub(1), 0, '▴', style(DIM).bg(bg));
    }
    if scroll + list_h < p.options.len() {
        put_char(buf, inner, inner.width.saturating_sub(1), list_h as u16 - 1, '▾', style(DIM).bg(bg));
    }
    let foot = list_h as u16 + 1;
    put(buf, inner, 1, foot, row_help(p.kind), style(DIM).bg(bg));
    put(buf, inner, 1, foot + 1, PICKER_KEYS, style(FAINT).bg(bg));
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
        .args(["--edition", "2021", "--target", "wasm32-unknown-unknown", "-O", "--crate-type", "cdylib", "--crate-name", "shrine_explorer"])
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
    // The cover shows where the piece starts, seen straight on.
    let cover = preview::cover_png(&preview::cover_frame(&config), COVER_SIZE);

    let _ = tx.send(Progress::Fraction(0.92, "writing WAV + metadata".to_string()));
    let build = emit::build_date();
    let mut comment = emit::info(&config, build);
    comment.push_str("\n\nrecipe=");
    comment.push_str(&shrine_explorer::recipe::encode(&config).iter().map(|w| w.to_string()).collect::<Vec<_>>().join(","));
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

    const DEFAULT_SETTINGS_FOR_TEST: shrine_explorer::Settings = shrine_explorer::DEFAULT_SETTINGS;

    /// An app whose architecture is procedural, with the editor open. Editor
    /// tests must not depend on whatever is stored on this machine.
    fn editor_app() -> App {
        let mut app = App::new();
        // From the defaults, not whatever is saved on this machine (an offset
        // or turned architecture changes what the editor allows).
        app.config = SimConfig::default();
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
            album: "ShrineExplorer".to_string(),
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
    fn a_choice_row_opens_its_list_rather_than_an_editor() {
        let mut app = App::new();
        app.sel = app.rows().iter().position(|r| r.kind == Kind::Biome).unwrap();
        app.activate();
        assert!(app.prompt.is_none(), "biome should not prompt");
        let p = app.picker.as_ref().expect("biome should open its list");
        assert_eq!(p.options.len(), Biome::ALL.len());
        assert_eq!(p.options[p.sel].2.biome, app.config.biome, "the list opens on the current value");
    }

    fn picker_on(app: &mut App, kind: Kind) {
        app.sel = app.rows().iter().position(|r| r.kind == kind).unwrap();
        app.picker = None;
        assert!(app.open_picker(), "{kind:?} has no list");
    }

    #[test]
    fn a_picker_applies_live_and_esc_puts_it_back() {
        let mut app = App::new();
        app.config.weather = WeatherKind::Clear;
        let before = app.config.clone();
        picker_on(&mut app, Kind::Weather);
        app.handle_key(KeyCode::Down, KeyModifiers::NONE);
        app.handle_key(KeyCode::Down, KeyModifiers::NONE);
        assert_ne!(app.config.weather, before.weather, "moving through the list applies it");
        app.handle_key(KeyCode::Esc, KeyModifiers::NONE);
        assert!(app.picker.is_none());
        assert_eq!(shrine_explorer::recipe::encode(&app.config), shrine_explorer::recipe::encode(&before), "esc must put it all back");
    }

    #[test]
    fn a_picker_keeps_the_choice_on_enter() {
        let mut app = App::new();
        app.config.settings.scale = SCALES[0];
        picker_on(&mut app, Kind::Scale);
        app.handle_key(KeyCode::End, KeyModifiers::NONE);
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(app.picker.is_none());
        assert_eq!(app.config.settings.scale, *SCALES.last().unwrap());
    }

    #[test]
    fn choosing_a_mode_in_its_list_moves_the_mode_defaults() {
        let mut app = App::new();
        app.config.settings = DEFAULT_SETTINGS_FOR_TEST;
        picker_on(&mut app, Kind::Pace);
        let micro = PACES.iter().position(|p| *p == Pace::Micro).unwrap();
        app.picker_to(micro);
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(app.config.settings.pace, Pace::Micro);
        assert_eq!(app.config.settings.bpm, shrine_explorer::Settings::mode_defaults(Pace::Micro).0);
    }

    #[test]
    fn every_choice_row_lists_distinct_values_and_says_what_it_is() {
        let mut app = App::new();
        // Show every mode's rows in turn.
        for pace in PACES {
            app.config.settings.set_mode(pace);
            for row in app.rows() {
                if row.kind != Kind::Header {
                    assert!(!row_help(row.kind).is_empty(), "{:?} has no help line", row.kind);
                }
                if app.choices(row.kind).is_none() {
                    continue;
                }
                picker_on(&mut app, row.kind);
                let p = app.picker.take().unwrap();
                assert!(p.options.len() >= 2, "{:?} lists {} value(s)", row.kind, p.options.len());
                let mut labels: Vec<&String> = p.options.iter().map(|o| &o.0).collect();
                labels.dedup();
                assert_eq!(labels.len(), p.options.len(), "{:?} lists a value twice", row.kind);
            }
        }
    }

    #[test]
    fn sections_then_settings_and_back() {
        let mut app = App::new();
        app.config = SimConfig::default();
        app.focus = Focus::Sections;
        app.goto_tab(Tab::Track);
        // ↑↓ in the sidebar change the section (wrapping).
        app.handle_key(KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(app.tab(), Tab::Shrine);
        app.handle_key(KeyCode::Up, KeyModifiers::NONE);
        app.handle_key(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(app.tab(), Tab::Export, "sections wrap");
        // → steps in; ↑↓ then stay inside the section.
        app.handle_key(KeyCode::Char('3'), KeyModifiers::NONE);
        assert_eq!((app.tab(), app.focus), (Tab::Music, Focus::Settings));
        for _ in 0..40 {
            app.handle_key(KeyCode::Down, KeyModifiers::NONE);
            assert_eq!(app.tab(), Tab::Music, "the arrows left the section");
        }
        // Esc goes back to the sidebar, a second Esc saves and quits.
        assert!(!app.handle_key(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.focus, Focus::Sections);
        app.handle_key(KeyCode::Right, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Settings);
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Sections);
        // Every section's rows are reachable and stay in it.
        for tab in TABS {
            app.goto_tab(tab);
            let mut i = app.sel;
            for _ in 0..60 {
                i = app.menu_step(i, 1);
                assert_eq!(tabs_of(&app.rows())[i], tab);
            }
        }
    }

    #[test]
    fn every_section_has_a_short_summary_in_every_mode() {
        for pace in PACES {
            let mut c = SimConfig::default();
            c.settings.set_mode(pace);
            c.settings.name = shrine_explorer::name::Name::from_str("A long track name here");
            for tab in TABS {
                let s = section_summary(tab, &c);
                assert!(!s.is_empty(), "{tab:?}");
                assert!(s.chars().count() <= 22 || tab == Tab::Track, "{tab:?}: {s}");
            }
        }
    }

    #[test]
    fn the_setup_screen_draws_tabs_and_a_picker() {
        use ratatui::backend::TestBackend;
        let mut terminal = ratatui::Terminal::new(TestBackend::new(120, 36)).unwrap();
        let mut app = App::new();
        app.goto_tab(Tab::Music);
        let mut screen = |app: &App| -> String {
            terminal.draw(|f| app.draw(f)).unwrap();
            let text: String = terminal.backend().buffer().content().iter().map(|c| c.symbol()).collect();
            if std::env::var("SHRINE_PRINT_SCREEN").is_ok() {
                for line in text.chars().collect::<Vec<_>>().chunks(120) {
                    println!("{}", line.iter().collect::<String>().trim_end_matches(|c| c == '▀' || c == ' '));
                }
            }
            text
        };
        let plain = screen(&app);
        assert!(plain.contains("Mode"), "the Music tab shows its rows");
        picker_on(&mut app, Kind::Scale);
        let text = screen(&app);
        for t in TABS {
            assert!(text.contains(t.name()), "the sidebar misses {}", t.name());
        }
        assert!(text.contains(SCALES[0].name()));
        assert!(text.contains("enter keep"));
    }

    #[test]
    fn reset_restores_defaults() {
        let mut app = App::new();
        app.config.seed = 42;
        app.config.settings.bpm = 111;
        app.config.settings.arch = ArchTransform { rot_z: 35, off_x: 3, ..ArchTransform::IDENTITY };
        app.config.settings.name = shrine_explorer::name::Name::from_str("Scratch");
        app.sel = app.rows().iter().position(|r| r.kind == Kind::Reset).unwrap();
        app.activate();
        // Compare the recipes: the strongest statement that the reset is exact.
        assert_eq!(
            shrine_explorer::recipe::encode(&app.config),
            shrine_explorer::recipe::encode(&SimConfig::default())
        );
        assert!(app.config.settings.name.is_empty());
    }

    #[test]
    fn the_glitch_screen_edits_the_open_layer() {
        let mut app = App::new();
        // Start from a known layer, whatever the saved settings say.
        app.config.settings.glitch[1].enabled = true;
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
        app.config.settings.code = shrine_explorer::overlay::CodeOverlay::OFF;
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
    fn the_randomize_screen_rolls_only_what_is_ticked_and_can_undo() {
        let mut app = App::new();
        app.config = SimConfig::default();
        let before = app.config.clone();
        app.sel = app.rows().iter().position(|r| r.kind == Kind::Randomize).unwrap();
        app.activate();
        assert_eq!(app.screen, Screen::Randomize);
        // Only the drums.
        app.handle_key(KeyCode::Char('n'), KeyModifiers::NONE);
        let drums = shrine_explorer::randomize::GROUPS.iter().position(|g| *g == shrine_explorer::randomize::Group::Drums).unwrap();
        app.rform.sel = drums;
        app.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
        let mut moved = false;
        for _ in 0..12 {
            app.handle_key(KeyCode::Char('r'), KeyModifiers::NONE);
            let s = &app.config.settings;
            assert_eq!((app.config.seed, s.pace, s.scale, s.bpm), (before.seed, before.settings.pace, before.settings.scale, before.settings.bpm));
            moved |= (s.kit, s.space, s.drum_reverse) != (before.settings.kit, before.settings.space, before.settings.drum_reverse);
        }
        assert!(moved, "rolling the drums never changed them");
        let rolled = app.config.clone();
        app.handle_key(KeyCode::Char('u'), KeyModifiers::NONE);
        assert_ne!(shrine_explorer::recipe::encode(&app.config), shrine_explorer::recipe::encode(&rolled), "undo did nothing");
        app.handle_key(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(app.screen, Screen::Main);
    }

    #[test]
    fn a_map_position_can_be_typed_stepped_and_made_home() {
        let mut app = App::new();
        app.config = SimConfig::default();
        let home = app.config.clone();
        app.sel = app.rows().iter().position(|r| r.kind == Kind::MapPos).unwrap();
        app.activate();
        if let Some(p) = &mut app.prompt {
            p.value = "3, -2".into();
        }
        app.commit_prompt();
        assert_eq!(app.config.settings.map, (3, -2));
        // The rows stay the home's; only the map position moved.
        assert_eq!(shrine_explorer::Settings { map: (0, 0), ..app.config.settings }, home.settings);
        app.adjust(true, false);
        assert_eq!(app.config.settings.map, (4, -2));
        app.adjust(false, true);
        assert_eq!(app.config.settings.map, (4, -3));
        // Garbage is reported, not applied.
        app.activate();
        if let Some(p) = &mut app.prompt {
            p.value = "north".into();
        }
        app.commit_prompt();
        assert_eq!(app.config.settings.map, (4, -3));
        // Make it home: the same shrine, now at 0, 0.
        let there = shrine_explorer::explore::at(&home, 4, -3);
        app.sel = app.rows().iter().position(|r| r.kind == Kind::MakeHome).unwrap();
        app.activate();
        assert_eq!(shrine_explorer::recipe::encode(&app.config), shrine_explorer::recipe::encode(&there));
        assert!(app.rows().iter().all(|r| r.kind != Kind::MakeHome), "at home there is nothing to make home");
        assert_eq!(parse_map("12,-3"), Some((12, -3)));
        assert_eq!(parse_map("12 -3"), Some((12, -3)));
        assert_eq!(parse_map("12"), None);
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
