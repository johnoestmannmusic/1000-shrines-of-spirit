//! The start screen: an animated title, the version, and the setup questions
//! in a bordered box, styled like the rest of the TUI. Rendering a WAV also
//! happens here, with a progress bar in the same box.

use super::gfx::*;
use crate::{parse_label, parse_mode, Mode};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear};
use ratatui::{DefaultTerminal, Frame};
use shrine0011::cli::{self, format_length, FADE_OUT_SECONDS};
use shrine0011::atmos::{LoopTimbre, LOOP_TIMBRES};
use shrine0011::harmony::{Pace, SCALES};
use shrine0011::tempo::{Tempo, MAX_BPM, MIN_BPM};
use shrine0011::texture::{Grid, EVENT_NAMES, ROWS, STEPS};
use shrine0011::{KeyChoice, LoopDesign, Seeds, Settings, DEFAULT_SEEDS, SAMPLE_RATE};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The build date, stamped by build.rs.
pub const VERSION: &str = concat!("v", env!("BUILD_DATE"));
const SUBTITLE: &str = "an endless glitch ambient generator";

const NAME: &str = "GlitchAmbiToolkit";
const TITLE: [&str; 5] = [
    " ▄████▄ ██ ▄▄  ██        ██    ▄████▄         ██    ▄▄ ██████             ██ ██    ▄▄  ██ ",
    "██      ██    ████       ██    ██  ██         ██         ██               ██ ██       ████",
    "██  ▄▄▄ ██ ██  ██  ▄███▄ ██▀█▄ ██████ ██▀█▀█▄ ██▀█▄ ██   ██   ▄███▄ ▄███▄ ██ ██ ▄█ ██  ██ ",
    "██   ██ ██ ██  ██  ██    ██ ██ ██  ██ ██ █ ██ ██ ██ ██   ██   ██ ██ ██ ██ ██ ████  ██  ██ ",
    " ▀████▀ ██ ██  ▀██ ▀███▀ ██ ██ ██  ██ ██ █ ██ ██▄█▀ ██   ██   ▀███▀ ▀███▀ ██ ██ ▀█ ██  ▀██",
];
const TITLE_W: u16 = 90;
/// The colours the title slowly cycles through.
const SWEEP: [Rgb; 5] = [DRONE, CHORD, REVERB, GLITCH1, GLITCH2];
const BOX_W: u16 = 76;
const FRAME: Duration = Duration::from_millis(33);

pub struct Choices {
    pub mode: Mode,
    pub label: String,
    pub seeds: Seeds,
    pub settings: Settings,
    pub path: String,
    pub seconds: f64,
}

fn sweep_color(p: f64) -> Rgb {
    let p = p.rem_euclid(1.0) * SWEEP.len() as f64;
    let i = p as usize;
    mix(SWEEP[i % SWEEP.len()], SWEEP[(i + 1) % SWEEP.len()], p - i as f64)
}

/// Draws the title, version and an empty box; returns the box's inner area.
fn screen(f: &mut Frame, t: f64, box_title: &str, box_height: u16, footer: &str) -> Rect {
    let area = f.area();
    f.render_widget(Clear, area);
    let buf = f.buffer_mut();
    let big = area.width >= TITLE_W + 4 && area.height >= 22;
    let title_h = if big { 5 } else { 1 };
    let used = title_h + 1 + 1 + 2 + box_height;
    let top = area.height.saturating_sub(used) / 3;

    if big {
        let x0 = area.x + (area.width - TITLE_W) / 2;
        // Every few seconds one row glitches sideways for a moment.
        let cycle = (t / 3.7) as u64;
        let glitching = t % 3.7 < 0.12 && t > 1.0;
        let glitch_row = (cycle * 2_654_435_761 % 5) as usize;
        let shift: i32 = if cycle % 2 == 0 { 2 } else { -2 };
        for (row, line) in TITLE.iter().enumerate() {
            let dx = if glitching && row == glitch_row { shift } else { 0 };
            for (col, ch) in line.chars().enumerate() {
                if ch == ' ' {
                    continue;
                }
                let mut c = sweep_color(col as f64 / 110.0 - t * 0.06);
                // A soft highlight drifting across the letters.
                let glow = (-(((col as f64 / TITLE_W as f64) - (t * 0.12) % 1.6 + 0.3).powi(2)) * 30.0).exp();
                c = mix(c, WHITE, glow * 0.45);
                if glitching && row == glitch_row {
                    c = mix(c, WHITE, 0.6);
                }
                let x = (x0 as i32 + col as i32 + dx).max(0) as u16;
                if let Some(cell) = buf.cell_mut((x, area.y + top + row as u16)) {
                    cell.set_char(ch).set_style(fg(c));
                }
            }
        }
    } else {
        let w = NAME.len() as u16;
        let st = fg(sweep_color(-t * 0.06)).add_modifier(Modifier::BOLD);
        put(buf, area, (area.width.saturating_sub(w)) / 2, top, NAME, st);
    }

    let vy = top + title_h + 1;
    let version = format!("{VERSION} · {SUBTITLE}");
    let vx = area.width.saturating_sub(version.chars().count() as u16) / 2;
    put(buf, area, vx, vy, VERSION, fg(TEXT));
    put(buf, area, vx + VERSION.len() as u16, vy, &format!(" · {SUBTITLE}"), fg(DIM));

    let bw = BOX_W.min(area.width.saturating_sub(2));
    // Never let the box run off the bottom: keep room for the title and version.
    let room = area.height.saturating_sub(title_h + 3).max(3);
    let bh = box_height.min(room);
    let rect = Rect::new(
        area.x + (area.width - bw) / 2,
        area.y + (vy + 2).min(area.height.saturating_sub(bh)),
        bw,
        bh,
    );
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(fg(mix(FAINT, DIM, 0.6)))
        .title(Span::styled(format!(" {box_title} "), fg(TEXT).add_modifier(Modifier::BOLD)))
        .title_bottom(Line::from(Span::styled(format!(" {footer} "), fg(DIM))));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    Rect::new(inner.x + 1, inner.y, inner.width.saturating_sub(2), inner.height)
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Step {
    Menu,
    Mode,
    Label,
    Bpm,
    Scale,
    Chords,
    Pace,
    Key,
    Kit,
    Space,
    Loop1,
    Loop2,
    Loop3,
    Loop4,
    LoopArc(usize),
    LoopHold(usize),
    DroneCycle,
    DroneHold,
    Transpose,
    DroneTone,
    Seed(usize),
    File,
    Overwrite,
    Length,
}

const MODES: [(&str, &str); 4] = [
    ("Play live", "endless, with a visual tour of the engine"),
    ("Render to WAV", "as fast as the computer can"),
    ("Play and record", "hear it while it is saved to a WAV file"),
    ("Exit", "close GlitchAmbiToolkit"),
];

/// The outcome of the last round, shown at the top of the menu.
pub struct Notice {
    pub text: String,
    pub error: bool,
}

impl Notice {
    pub fn ok(text: impl Into<String>) -> Self {
        Notice { text: text.into(), error: false }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Notice { text: text.into(), error: true }
    }
}

/// The list questions, and which entry of `Setup::sel` each one uses.
fn loop_layer(step: Step) -> Option<usize> {
    match step {
        Step::Loop1 => Some(0),
        Step::Loop2 => Some(1),
        Step::Loop3 => Some(2),
        Step::Loop4 => Some(3),
        _ => None,
    }
}

fn list_id(step: Step) -> Option<usize> {
    match step {
        Step::Mode => Some(0),
        Step::Scale => Some(1),
        Step::Chords => Some(2),
        Step::Pace => Some(3),
        Step::Kit => Some(4),
        Step::Space => Some(5),
        Step::Loop1 => Some(6),
        Step::Loop2 => Some(7),
        Step::Loop3 => Some(8),
        Step::Loop4 => Some(9),
        Step::Key => Some(10),
        _ => None,
    }
}

/// One row of the settings menu.
enum MenuRow {
    Start,
    Exit,
    Header(&'static str),
    Field { label: String, value: String, step: Step },
}

/// The atmosphere-loop choices: None first (the entry numbered 0), then every
/// timbre in `LOOP_TIMBRES`, so the "no atmosphere" choice is up front.
/// The sustained timbres in list order (Off excluded).
fn loop_timbres() -> Vec<LoopTimbre> {
    LOOP_TIMBRES.iter().copied().filter(|t| *t != LoopTimbre::Off).collect()
}

/// The atmosphere list entries: 0 = None, 1 = Event grid, 2.. = sustained timbres.
const LOOP_GRID: usize = 1;

fn loop_entry_count() -> usize {
    2 + loop_timbres().len()
}

fn loop_entry_name(i: usize) -> String {
    match i {
        0 => LoopTimbre::Off.name().to_string(),
        1 => "Event grid".to_string(),
        n => loop_timbres().get(n - 2).map_or("Off".into(), |t| t.name().to_string()),
    }
}

fn loop_entry_blurb(i: usize) -> String {
    match i {
        0 => LoopTimbre::Off.blurb().to_string(),
        1 => "fire, water, stones, wood: press Enter to edit the 8 x 16 grid".to_string(),
        n => loop_timbres().get(n - 2).map_or(String::new(), |t| t.blurb().to_string()),
    }
}

fn entry_to_design(i: usize, grid: Grid) -> LoopDesign {
    match i {
        0 => LoopDesign::OFF,
        1 => LoopDesign::Events(grid),
        n => LoopDesign::Sustained(loop_timbres()[n - 2]),
    }
}

fn design_to_entry(d: LoopDesign) -> usize {
    match d {
        LoopDesign::Sustained(LoopTimbre::Off) => 0,
        LoopDesign::Events(_) => LOOP_GRID,
        LoopDesign::Sustained(t) => 2 + loop_timbres().iter().position(|x| *x == t).unwrap_or(0),
    }
}

const CHORD_CHOICES: [(&str, &str); 4] = [
    ("1 chord", "32 bars: one frozen chord, as in 0009"),
    ("2 chords", "20 / 12 bars"),
    ("3 chords", "16 / 12 / 4 bars"),
    ("4 chords", "14 / 10 / 4 / 4 bars"),
];
const PACE_NAMES: [&str; 2] = ["Half-time", "Jungle"];

/// The chord-pace choices, with their bar lengths at this tempo.
fn pace_choices(bpm: u16) -> Vec<(String, String)> {
    let t = Tempo::new(bpm);
    let secs = |p: Pace| p.bar_samples(t.beat()) as f64 / SAMPLE_RATE as f64;
    vec![
        (
            PACE_NAMES[0].into(),
            format!("bars ≈ {:.1} s at {:.0} BPM: slow, drone-like changes", secs(Pace::HalfTime), t.half_bpm()),
        ),
        (
            PACE_NAMES[1].into(),
            format!("bars ≈ {:.1} s at {} BPM: changes move with the drums", secs(Pace::Jungle), t.bpm),
        ),
    ]
}

fn options(step: Step, bpm: u16) -> Vec<(String, String)> {
    let own = |v: &[(&str, &str)]| v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    match step {
        Step::Mode => own(&MODES),
        Step::Scale => SCALES.iter().map(|s| (s.name().to_string(), s.mood().to_string())).collect(),
        Step::Chords => own(&CHORD_CHOICES),
        Step::Pace => pace_choices(bpm),
        Step::Kit => shrine0011::kits::KITS.iter().map(|k| (k.name().to_string(), k.blurb().to_string())).collect(),
        Step::Space => shrine0011::kits::SPACES.iter().map(|k| (k.name().to_string(), k.blurb().to_string())).collect(),
        Step::Loop1 | Step::Loop2 | Step::Loop3 | Step::Loop4 => (0..loop_entry_count()).map(|i| (loop_entry_name(i), loop_entry_blurb(i))).collect(),
        Step::Key => {
            let mut v = vec![("Seed picks".to_string(), "seed 1 chooses the tonic, as in every earlier recipe".to_string())];
            for pc in 0..12u8 {
                v.push((shrine0011::harmony::key_name(pc).to_string(), "fix the tonic here".to_string()));
            }
            v
        }
        _ => Vec::new(),
    }
}

struct Setup {
    /// Last session's answers: the defaults for this one.
    last: crate::state::Remembered,
    notice: Option<Notice>,
    exit: bool,
    step: Step,
    back: Vec<Step>,
    /// Selected entry of each list question (mode, scale, chords, pace, kit, space, loop 1..4, key).
    sel: [usize; 11],
    input: String,
    error: Option<String>,
    mode: Mode,
    label: String,
    bpm: u16,
    seeds: [u64; 6],
    path: String,
    seconds: f64,
    drone_cycle: u16,
    drone_hold: u16,
    loop_transpose: i8,
    /// The event grid being edited for each layer.
    grids: [Grid; 4],
    /// Each atmosphere layer's fade-in/out arc.
    loop_arcs: [shrine0011::DroneArc; 4],
    drone_tone: u16,
    grid_cursor: (usize, usize),
    /// Which layer's grid is open in the editor (None = the list is shown).
    grid_edit: Option<usize>,
    /// Highlighted row of the settings menu.
    menu_sel: usize,
    /// Live audition while the atmosphere questions are open (None if no device).
    audition: Option<crate::audition::Audition>,
    /// The design currently handed to the audition, so it only rebuilds on change.
    last_design: Option<LoopDesign>,
}

impl Setup {
    fn new(notice: Option<Notice>) -> Self {
        let last = crate::state::load();
        let d = last.seeds;
        let set = last.settings;
        Setup {
            notice,
            exit: false,
            step: Step::Menu,
            back: Vec::new(),
            sel: [
                0,
                set.scale.index(),
                set.chords as usize - 1,
                if set.pace == Pace::Jungle { 1 } else { 0 },
                set.kit.index(),
                set.space.index(),
                design_to_entry(set.loops[0]),
                design_to_entry(set.loops[1]),
                design_to_entry(set.loops[2]),
                design_to_entry(set.loops[3]),
                match set.key {
                    KeyChoice::Seed => 0,
                    KeyChoice::Note(n) => n as usize + 1,
                },
            ],
            input: String::new(),
            error: None,
            mode: Mode::Play,
            label: last.label.clone(),
            bpm: set.bpm,
            seeds: [d.s1, d.s2, d.s3, d.s4, d.s5, d.s6],
            path: last.path.clone(),
            seconds: last.seconds,
            drone_cycle: set.drone.cycle_s,
            drone_hold: set.drone.hold_s,
            loop_transpose: set.loop_transpose,
            grids: std::array::from_fn(|i| match set.loops[i] {
                LoopDesign::Events(g) => g,
                _ => Grid::fire(),
            }),
            loop_arcs: set.loop_arcs,
            drone_tone: set.drone_tone,
            grid_cursor: (0, 0),
            grid_edit: None,
            menu_sel: 0,
            audition: None,
            last_design: None,
            last,
        }
    }

    /// Keep the live audition in step with the atmosphere question on screen.
    fn sync_audition(&mut self) {
        let layer = match self.step {
            Step::Loop1 => Some(0),
            Step::Loop2 => Some(1),
            Step::Loop3 => Some(2),
            Step::Loop4 => Some(3),
            _ => None,
        };
        let Some(layer) = layer else {
            self.audition = None;
            self.last_design = None;
            return;
        };
        let design = entry_to_design(self.sel[6 + layer], self.grids[layer]);
        if self.audition.is_none() {
            self.audition = crate::audition::Audition::start(self.seeds(), self.settings()).ok();
            self.last_design = None;
        }
        if self.last_design != Some(design) {
            if let Some(a) = &self.audition {
                a.set_design(design);
            }
            self.last_design = Some(design);
        }
    }

    /// Every selectable row of the settings menu, in order.
    fn menu_rows(&self) -> Vec<MenuRow> {
        let field = |label: &str, value: String, step: Step| MenuRow::Field { label: label.to_string(), value, step };
        let arc = |i: usize| {
            let a = self.loop_arcs[i];
            if a.cycle_s == 0 { "always on".to_string() } else { format!("{} s cycle, {} s up", a.cycle_s, a.hold_s) }
        };
        let mut r = vec![MenuRow::Start, field("Mode", MODES[self.sel[0]].0.to_string(), Step::Mode)];
        r.push(MenuRow::Header("Music"));
        r.push(field("Scale", SCALES[self.sel[1]].name().to_string(), Step::Scale));
        r.push(field(
            "Root note",
            if self.sel[10] == 0 { "seed picks".into() } else { shrine0011::harmony::key_name((self.sel[10] - 1) as u8).to_string() },
            Step::Key,
        ));
        r.push(field("Chords", CHORD_CHOICES[self.sel[2]].0.to_string(), Step::Chords));
        r.push(field("Pace", PACE_NAMES[self.sel[3]].to_string(), Step::Pace));
        r.push(field("Tempo", format!("{} BPM", self.bpm), Step::Bpm));
        r.push(MenuRow::Header("Drums"));
        r.push(field("Kit", shrine0011::kits::KITS[self.sel[4]].name().to_string(), Step::Kit));
        r.push(field("Space", shrine0011::kits::SPACES[self.sel[5]].name().to_string(), Step::Space));
        r.push(MenuRow::Header("Atmosphere"));
        let loop_step = [Step::Loop1, Step::Loop2, Step::Loop3, Step::Loop4];
        for i in 0..4 {
            r.push(field(&format!("Layer {}", i + 1), loop_entry_name(self.sel[6 + i]), loop_step[i]));
        }
        r.push(field("Transpose", format!("{} semitones", self.loop_transpose), Step::Transpose));
        for i in 0..4 {
            r.push(field(&format!("Layer {} arc", i + 1), arc(i), Step::LoopArc(i)));
        }
        r.push(MenuRow::Header("Drone & form"));
        r.push(field(
            "Drone arc",
            if self.drone_cycle == 0 { "always on".into() } else { format!("{} s cycle, {} s up", self.drone_cycle, self.drone_hold) },
            Step::DroneCycle,
        ));
        r.push(field("Drone tone", if self.drone_tone == 0 { "off".into() } else { format!("{} Hz low-pass", self.drone_tone) }, Step::DroneTone));
        r.push(MenuRow::Header("Seeds"));
        for i in 0..6 {
            r.push(field(&format!("Seed {}", i + 1), self.seeds[i].to_string(), Step::Seed(i)));
        }
        r.push(MenuRow::Header("Output"));
        r.push(field("Title", self.label.clone(), Step::Label));
        if self.mode != Mode::Play {
            r.push(field("File", self.path.clone(), Step::File));
            r.push(field("Length", format_length(self.seconds), Step::Length));
        }
        r.push(MenuRow::Exit);
        r
    }

    /// Move the menu highlight by `dir`, skipping section headers.
    fn menu_step(&self, from: usize, dir: i32) -> usize {
        let rows = self.menu_rows();
        let n = rows.len() as i32;
        let mut i = from as i32;
        for _ in 0..n {
            i = (i + dir).rem_euclid(n);
            if !matches!(rows[i as usize], MenuRow::Header(_)) {
                return i as usize;
            }
        }
        from
    }

    fn settings(&self) -> Settings {
        Settings {
            scale: SCALES[self.sel[1]],
            chords: self.sel[2] as u8 + 1,
            pace: if self.sel[3] == 1 { Pace::Jungle } else { Pace::HalfTime },
            kit: shrine0011::kits::KITS[self.sel[4]],
            space: shrine0011::kits::SPACES[self.sel[5]],
            bpm: self.bpm,
            loops: [
                entry_to_design(self.sel[6], self.grids[0]),
                entry_to_design(self.sel[7], self.grids[1]),
                entry_to_design(self.sel[8], self.grids[2]),
                entry_to_design(self.sel[9], self.grids[3]),
            ],
            loop_arcs: self.loop_arcs,
            loop_transpose: self.loop_transpose,
            key: if self.sel[10] == 0 { KeyChoice::Seed } else { KeyChoice::Note((self.sel[10] - 1) as u8) },
            drone: shrine0011::DroneArc { cycle_s: self.drone_cycle, hold_s: self.drone_hold },
            drone_tone: self.drone_tone,
        }
    }

    fn seeds(&self) -> Seeds {
        let s = self.seeds;
        Seeds { s1: s[0], s2: s[1], s3: s[2], s4: s[3], s5: s[4], s6: s[5] }
    }

    fn default_for(&self, step: Step) -> String {
        let l = &self.last;
        let d = [l.seeds.s1, l.seeds.s2, l.seeds.s3, l.seeds.s4, l.seeds.s5, l.seeds.s6];
        match step {
            Step::Mode | Step::Scale | Step::Chords | Step::Pace | Step::Kit | Step::Space | Step::Loop1 | Step::Loop2 | Step::Loop3 | Step::Loop4 | Step::Key => "1".into(),
            Step::Menu => String::new(),
            Step::DroneCycle => l.settings.drone.cycle_s.to_string(),
            Step::DroneHold => (self.drone_cycle / 3).max(1).to_string(),
            Step::LoopArc(i) => self.loop_arcs[i].cycle_s.to_string(),
            Step::LoopHold(i) => self.loop_arcs[i].hold_s.max(1).to_string(),
            Step::DroneTone => self.last.settings.drone_tone.to_string(),
            Step::Transpose => self.last.settings.loop_transpose.to_string(),
            Step::Label => l.label.clone(),
            Step::Bpm => l.settings.bpm.to_string(),
            Step::Seed(i) => d[i].to_string(),
            Step::File => l.path.clone(),
            Step::Overwrite => "n".into(),
            Step::Length => format_length(l.seconds),
        }
    }

    fn question(&self) -> (String, String) {
        const SEED_ROLE: [&str; 6] = [
            "the key and the chords",
            "glitch layer 1",
            "glitch layer 2",
            "the bass",
            "the drum kit's sounds and break",
            "the atmosphere loops (notes, strikes, waves)",
        ];
        let list_hint = "↑ ↓ (or a number) to choose, Enter to confirm.".to_string();
        match self.step {
            Step::Menu => (String::new(), String::new()),
            Step::Mode => ("What would you like to do?".into(), list_hint),
            Step::Bpm => (
                format!("Tempo (BPM, {MIN_BPM}-{MAX_BPM})"),
                "The drums play at this tempo; glitches, chord bars and echoes move at exactly half. \
                 Below ~140 the break plays slower than it was recorded, so it drops in pitch."
                    .into(),
            ),
            Step::Scale => ("Scale".into(), "The seed picks the key and builds the chords from this scale.".into()),
            Step::Key => (
                "Root note".into(),
                "Seed picks the tonic from seed 1, as in every earlier recipe; or fix it here. The scale still supplies the intervals.".into(),
            ),
            Step::Chords => (
                "How many chords?".into(),
                "Chords 1 and 2 carry most of the 32-bar cycle; each change is a slow spectral morph.".into(),
            ),
            Step::Pace => ("Chord pace".into(), list_hint),
            Step::Kit => (
                "Drum kit".into(),
                "The next seed shapes this kit's sounds and its break. Choose Off for no drums.".into(),
            ),
            Step::Space => ("Drum space".into(), "Where the drum hits sit between the speakers.".into()),
            Step::Loop1 => (
                "Atmosphere layer 1".into(),
                "90s sample-CD pad or physical-texture event grid. 0 leaves it out.".into(),
            ),
            Step::Loop2 => (
                "Atmosphere layer 2".into(),
                "A shorter layer that drifts against layer 1. 0 leaves it out.".into(),
            ),
            Step::Loop3 => ("Atmosphere layer 3".into(), "A third layer, off by default. 0 leaves it out.".into()),
            Step::Loop4 => ("Atmosphere layer 4".into(), "A fourth layer, off by default. 0 leaves it out.".into()),
            Step::LoopArc(i) => (
                format!("Layer {} cycle seconds (0 = always on)", i + 1),
                "Each layer can swell in and out on its own slow repeat, like the drone. 0 keeps it playing all the time.".into(),
            ),
            Step::LoopHold(i) => (
                format!("Layer {} seconds up each cycle (1-{})", i + 1, self.loop_arcs[i].cycle_s),
                "A raised-cosine swell fades it in and out automatically at each end of the hold.".into(),
            ),
            Step::DroneTone => (
                "Drone character low-pass Hz (0 = off)".into(),
                "An end-of-chain low-pass on the drone: lower values make it darker and further back. 0 bypasses it.".into(),
            ),
            Step::DroneCycle => (
                "Drone cycle in seconds (0 = always on)".into(),
                "The frozen drone can swell in and out on a slow repeat: every cycle it fades up, holds, then leaves. 0 keeps it playing all the time.".into(),
            ),
            Step::DroneHold => (
                format!("Seconds the drone stays up each cycle (1-{})", self.drone_cycle),
                "A raised-cosine swell fades it in and out automatically at each end of the hold.".into(),
            ),
            Step::Transpose => (
                "Atmosphere transpose in semitones (-12 to 12)".into(),
                "Shift both atmosphere loops relative to the chord. 0 keeps them exactly on the chord tones, matching the chosen root note.".into(),
            ),
            Step::Label => (
                "Title shown in the player".into(),
                "Up to 4 characters. It appears in the header and the explainer ticker.".into(),
            ),
            Step::Seed(i) => {
                let canonical =
                    [DEFAULT_SEEDS.s1, DEFAULT_SEEDS.s2, DEFAULT_SEEDS.s3, DEFAULT_SEEDS.s4, DEFAULT_SEEDS.s5, DEFAULT_SEEDS.s6][i];
                (
                    format!("Seed {}: shapes {}", i + 1, SEED_ROLE[i]),
                    format!(
                        "Decimal or 0x hex · r rolls a random seed · canonical: {canonical}{}",
                        ""
                    ),
                )
            }
            Step::File => ("Output file".into(), ".wav is added if it is missing.".into()),
            Step::Overwrite => (format!("{} already exists. Overwrite it? (y/n)", self.path), String::new()),
            Step::Length => (
                "Length".into(),
                format!("Seconds, m:ss or h:mm:ss, up to 4:00:00. Ends with a {FADE_OUT_SECONDS:.0} s fade-out."),
            ),
        }
    }

    /// Accepts the current answer, then returns to the menu (or on to the next
    /// half of a two-part field). Returns true only when the whole setup is done.
    fn submit(&mut self) -> bool {
        let raw = self.input.trim().to_string();
        let answer = if raw.is_empty() { self.default_for(self.step) } else { raw };
        let ok = match self.step {
            Step::Mode => {
                if self.sel[0] == MODES.len() - 1 {
                    self.exit = true;
                    return true;
                }
                self.mode = parse_mode(&(self.sel[0] + 1).to_string()).unwrap_or(Mode::Play);
                true
            }
            Step::Scale | Step::Chords | Step::Pace | Step::Kit | Step::Space | Step::Loop1 | Step::Loop2 | Step::Loop3 | Step::Loop4 | Step::Key | Step::Menu => true,
            Step::Label => match parse_label(&answer) {
                Some(l) => {
                    self.label = l;
                    true
                }
                None => {
                    self.error = Some("Enter 1 to 4 characters.".into());
                    false
                }
            },
            Step::Bpm => match cli::parse_bpm(&answer) {
                Some(b) => {
                    self.bpm = b;
                    true
                }
                None => {
                    self.error = Some(format!("Enter a whole number from {MIN_BPM} to {MAX_BPM}."));
                    false
                }
            },
            Step::Seed(i) => match cli::parse_u64(&answer) {
                Some(v) => {
                    self.seeds[i] = v;
                    true
                }
                None => {
                    self.error = Some("Enter a whole number from 0 to 18446744073709551615, or 0x hex.".into());
                    false
                }
            },
            Step::DroneCycle => match cli::parse_drone_cycle(&answer) {
                Some(c) => {
                    self.drone_cycle = c;
                    if c == 0 {
                        self.drone_hold = 0;
                    }
                    true
                }
                None => {
                    self.error = Some("Enter 0, or a whole number of seconds up to 3600.".into());
                    false
                }
            },
            Step::DroneHold => match cli::parse_drone_hold(&answer, self.drone_cycle) {
                Some(h) => {
                    self.drone_hold = h;
                    true
                }
                None => {
                    self.error = Some(format!("Enter a whole number of seconds from 1 to {}.", self.drone_cycle));
                    false
                }
            },
            Step::LoopArc(i) => match cli::parse_drone_cycle(&answer) {
                Some(c) => {
                    self.loop_arcs[i].cycle_s = c;
                    if c == 0 {
                        self.loop_arcs[i].hold_s = 0;
                    }
                    true
                }
                None => {
                    self.error = Some("Enter 0, or a whole number of seconds up to 3600.".into());
                    false
                }
            },
            Step::LoopHold(i) => match cli::parse_drone_hold(&answer, self.loop_arcs[i].cycle_s) {
                Some(h) => {
                    self.loop_arcs[i].hold_s = h;
                    true
                }
                None => {
                    self.error = Some(format!("Enter a whole number of seconds from 1 to {}.", self.loop_arcs[i].cycle_s));
                    false
                }
            },
            Step::DroneTone => match cli::parse_drone_tone(&answer) {
                Some(hz) => {
                    self.drone_tone = hz;
                    true
                }
                None => {
                    self.error = Some("Enter 0 (off), or 100 to 16000 Hz.".into());
                    false
                }
            },
            Step::Transpose => match cli::parse_transpose(&answer) {
                Some(t) => {
                    self.loop_transpose = t;
                    true
                }
                None => {
                    self.error = Some("Enter a whole number from -12 to 12.".into());
                    false
                }
            },
            Step::File => {
                let mut p = answer;
                if !p.to_ascii_lowercase().ends_with(".wav") {
                    p.push_str(".wav");
                }
                self.path = p;
                true
            }
            Step::Overwrite => true,
            Step::Length => match cli::parse_length(&answer) {
                Some(s) => {
                    self.seconds = s;
                    true
                }
                None => {
                    self.error = Some("Enter a length above 0 and up to 4:00:00, e.g. 600, 10:00 or 1:00:00.".into());
                    false
                }
            },
        };
        if !ok {
            return false;
        }
        self.error = None;
        self.input.clear();
        self.step = match self.step {
            Step::LoopArc(i) if self.loop_arcs[i].cycle_s > 0 => Step::LoopHold(i),
            Step::DroneCycle if self.drone_cycle > 0 => Step::DroneHold,
            _ => Step::Menu,
        };
        false
    }

    /// The answers given so far, for the summary at the top of the box.
    fn summary(&self) -> Vec<(String, String)> {
        let mut rows = Vec::new();
        for step in &self.back {
            let row = match step {
                Step::Menu => ("Menu".into(), String::new()),
                Step::Mode => ("Mode".to_string(), MODES[self.sel[0]].0.to_string()),
                Step::Scale => ("Scale".into(), SCALES[self.sel[1]].name().to_string()),
                Step::Key => (
                    "Root note".into(),
                    if self.sel[8] == 0 { "seed picks".into() } else { shrine0011::harmony::key_name((self.sel[8] - 1) as u8).to_string() },
                ),
                Step::Chords => ("Chords".into(), format!("{} · {}", CHORD_CHOICES[self.sel[2]].0, CHORD_CHOICES[self.sel[2]].1)),
                Step::Pace => ("Pace".into(), PACE_NAMES[self.sel[3]].to_string()),
                Step::Bpm => ("Tempo".into(), format!("{} BPM · ambient layers at {:.0}", self.bpm, Tempo::new(self.bpm).half_bpm())),
                Step::Kit => ("Kit".into(), shrine0011::kits::KITS[self.sel[4]].name().to_string()),
                Step::Space => ("Space".into(), shrine0011::kits::SPACES[self.sel[5]].name().to_string()),
            Step::Loop1 => ("Loop 1".into(), loop_entry_name(self.sel[6])),
            Step::Loop2 => ("Loop 2".into(), loop_entry_name(self.sel[7])),
            Step::Loop3 => ("Loop 3".into(), loop_entry_name(self.sel[8])),
            Step::Loop4 => ("Loop 4".into(), loop_entry_name(self.sel[9])),
            Step::LoopArc(i) => (format!("Loop {} arc", i + 1), if self.loop_arcs[*i].cycle_s == 0 { "always on".into() } else { format!("{} s", self.loop_arcs[*i].cycle_s) }),
            Step::LoopHold(i) => (format!("Loop {} up", i + 1), format!("{} s each cycle", self.loop_arcs[*i].hold_s)),
                Step::DroneCycle => (
                    "Drone cycle".into(),
                    if self.drone_cycle == 0 { "always on".into() } else { format!("{} s", self.drone_cycle) },
                ),
                Step::DroneHold => ("Drone up".into(), format!("{} s each cycle", self.drone_hold)),
                Step::Transpose => ("Atmos transpose".into(), format!("{} semitones", self.loop_transpose)),
                Step::DroneTone => ("Drone tone".into(), if self.drone_tone == 0 { "off".into() } else { format!("{} Hz low-pass", self.drone_tone) }),
                Step::Label => ("Title".into(), self.label.clone()),
                Step::Seed(i) => (format!("Seed {}", i + 1), self.seeds[*i].to_string()),
                Step::File => ("File".into(), self.path.clone()),
                Step::Overwrite => ("Overwrite".into(), "yes".into()),
                Step::Length => ("Length".into(), format_length(self.seconds)),
            };
            rows.push(row);
        }
        rows
    }

    fn draw_menu(&self, f: &mut Frame, t: f64) {
        let rows = self.menu_rows();
        let footer = "↑ ↓ choose · Enter edit · Esc exit · Ctrl-C exit";
        let r = screen(f, t, "SETUP", rows.len() as u16 + 4, footer);
        let buf = f.buffer_mut();
        let vis = r.height.saturating_sub(2) as usize;
        let sel = self.menu_sel.min(rows.len().saturating_sub(1));
        let scroll = if vis > 0 && sel >= vis { sel + 1 - vis } else { 0 };
        let mut y = 0;
        put(buf, r, 0, y, "Choose a setting, or Start to play/render", fg(TEXT).add_modifier(Modifier::BOLD));
        y += 1;
        for (i, row) in rows.iter().enumerate().skip(scroll).take(vis) {
            let selected = i == sel;
            let marker = if selected { "›" } else { " " };
            let st = if selected { fg(GLITCH2).add_modifier(Modifier::BOLD) } else { fg(TEXT) };
            match row {
                MenuRow::Header(name) => put(buf, r, 2, y, name, fg(DIM).add_modifier(Modifier::BOLD)),
                MenuRow::Start => put(buf, r, 0, y, &format!("{marker} ▶ Start"), st),
                MenuRow::Exit => put(buf, r, 0, y, &format!("{marker} Exit"), st),
                MenuRow::Field { label, value, .. } => {
                    put(buf, r, 0, y, &format!("{marker} {label:<16}"), st);
                    put(buf, r, 18, y, value, fg(if selected { TEXT } else { DIM }));
                }
            }
            y += 1;
        }
    }

    fn draw(&self, f: &mut Frame, t: f64) {
        if self.step == Step::Menu {
            self.draw_menu(f, t);
            return;
        }
        let summary = self.summary();
        let opts = options(self.step, self.bpm);
        let text_w = BOX_W.min(f.area().width.saturating_sub(2)).saturating_sub(6) as usize;
        let notice_lines = self.notice.as_ref().map(|n| super::ticker::wrap(&n.text, text_w)).unwrap_or_default();
        let notice_rows = if notice_lines.is_empty() { 0 } else { notice_lines.len() as u16 + 1 };
        // The answers so far are condensed into a wrapped breadcrumb instead of
        // one row each, so the screen stays short enough for a laptop.
        let summary_text = summary.iter().map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>().join("  ·  ");
        let mut summary_lines: Vec<String> =
            if summary.is_empty() { Vec::new() } else { super::ticker::wrap(&summary_text, text_w) };
        let summary_cut = summary_lines.len() > 3;
        summary_lines.truncate(3);
        if summary_cut {
            if let Some(last) = summary_lines.last_mut() {
                last.push_str(" …");
            }
        }
        let id = list_id(self.step);
        let editing = self.grid_edit.is_some();
        // Lists longer than six entries (atmosphere, scale, kit) use a compact
        // grid of names, with only the highlighted entry described.
        let grid = !editing && id.is_some() && opts.len() > 6;
        let cols = if grid { (text_w / 20).clamp(1, 4) } else { 1 };
        let list_rows = if editing {
            ROWS as u16
        } else if opts.is_empty() {
            1
        } else if grid {
            opts.len().div_ceil(cols) as u16
        } else {
            opts.len() as u16
        };
        let extra = if editing { 2 } else { grid as u16 * 2 };
        let anim = self.audition.is_some() && matches!(self.step, Step::Loop1 | Step::Loop2 | Step::Loop3 | Step::Loop4);
        // notice, summary (+ gap), question, options, [description], [preview], gap, hint, error line, borders
        let height = notice_rows + summary_lines.len() as u16 + (!summary_lines.is_empty()) as u16 + 1 + list_rows + extra + anim as u16 + 1 + 1 + 1 + 2;
        let footer = "Enter accept · Esc menu · Ctrl-C exit";
        let r = screen(f, t, "SETUP", height, footer);
        let buf = f.buffer_mut();
        let mut y = 0;
        if let Some(n) = &self.notice {
            let (mark, color) = if n.error { ("!", (255, 95, 85)) } else { ("✓", BASS) };
            put(buf, r, 0, y, mark, fg(color).add_modifier(Modifier::BOLD));
            for line in &notice_lines {
                put(buf, r, 2, y, line, fg(if n.error { color } else { TEXT }));
                y += 1;
            }
            y += 1;
        }
        for line in &summary_lines {
            put(buf, r, 0, y, "✓", fg(BASS));
            put(buf, r, 2, y, line, fg(TEXT));
            y += 1;
        }
        if !summary_lines.is_empty() {
            y += 1;
        }
        let (q, hint) = if editing {
            (
                "Event grid — space toggles a cell, arrows move".to_string(),
                "F Fire · W Water · S Stones · O Wood · C clear · Enter accept · Esc back to list".to_string(),
            )
        } else {
            self.question()
        };
        put(buf, r, 0, y, &q, fg(TEXT).add_modifier(Modifier::BOLD));
        y += 1;
        let cursor = if (t * 2.0) as u64 % 2 == 0 { "▏" } else { " " };
        if let Some(layer) = self.grid_edit {
            for row in 0..ROWS {
                put(buf, r, 0, y + row as u16, &format!("{:<8}", EVENT_NAMES[row]), fg(DIM));
                for step in 0..STEPS {
                    let on = self.grids[layer].get(row, step);
                    let is_cursor = self.grid_cursor == (row, step);
                    let cell = if on { "██" } else { "··" };
                    let style = if is_cursor {
                        Style::new().bg(rgb(GLITCH2)).fg(rgb((20, 20, 20)))
                    } else if on {
                        fg(GLITCH2)
                    } else {
                        fg(FAINT)
                    };
                    put(buf, r, 9 + (step * 2) as u16, y + row as u16, cell, style);
                }
            }
            y += ROWS as u16 + 1;
        } else if let Some(id) = id {
            let selected = self.sel[id];
            if grid {
                let cell = (r.width as usize).saturating_sub(2) / cols;
                for (i, (name, _)) in opts.iter().enumerate() {
                    let (row, col) = (i / cols, i % cols);
                    let n = if matches!(self.step, Step::Loop1 | Step::Loop2 | Step::Loop3 | Step::Loop4) { i } else { i + 1 };
                    let style = if i == selected { fg(GLITCH2).add_modifier(Modifier::BOLD) } else { fg(TEXT) };
                    put(buf, r, (col * cell) as u16, y + row as u16, &format!("{n:>2} {name}"), style);
                }
                y += list_rows as u16 + 1;
                if let Some((_, desc)) = opts.get(selected) {
                    put(buf, r, 2, y, desc, fg(TEXT));
                }
                y += 1;
            } else {
                let name_w = opts.iter().map(|o| o.0.chars().count()).max().unwrap_or(0).max(12);
                for (i, (name, desc)) in opts.iter().enumerate() {
                    let sel = i == selected;
                    let marker = if sel { "›" } else { " " };
                    let style = if sel { fg(GLITCH2).add_modifier(Modifier::BOLD) } else { fg(TEXT) };
                    // The atmosphere questions put None first and number it 0.
                    let n = if matches!(self.step, Step::Loop1 | Step::Loop2 | Step::Loop3 | Step::Loop4) { i } else { i + 1 };
                    put(buf, r, 0, y, &format!("{marker} {n:>2}  {name:<name_w$}"), style);
                    put(buf, r, 7 + name_w as u16, y, desc, fg(if sel { TEXT } else { DIM }));
                    y += 1;
                }
            }
        } else {
            let default = self.default_for(self.step);
            put(buf, r, 0, y, "›", fg(GLITCH2).add_modifier(Modifier::BOLD));
            if self.input.is_empty() {
                put(buf, r, 2, y, cursor, fg(GLITCH2));
                put(buf, r, 3, y, &default, fg(mix(FAINT, DIM, 0.7)));
                put(buf, r, 4 + default.chars().count() as u16, y, "(Enter keeps this)", fg(FAINT));
            } else {
                // Long input scrolls so its end (where you type) stays visible.
                let room = r.width.saturating_sub(4) as usize;
                let n = self.input.chars().count();
                let shown: String = if n > room {
                    std::iter::once('…').chain(self.input.chars().skip(n - room + 1)).collect()
                } else {
                    self.input.clone()
                };
                put(buf, r, 2, y, &shown, fg(WHITE).add_modifier(Modifier::BOLD));
                put(buf, r, 2 + shown.chars().count() as u16, y, cursor, fg(GLITCH2));
            }
            y += 1;
        }
        if anim {
            if let Some(a) = &self.audition {
                let ov = a.overview();
                let peak = ov.iter().cloned().fold(1e-9f32, f32::max);
                let w = r.width.saturating_sub(9) as usize;
                put(buf, r, 0, y, "preview ", fg(DIM));
                let bars: Vec<char> = "▁▂▃▄▅▆▇█".chars().collect();
                for i in 0..w {
                    let v = ov.get(i * ov.len() / w.max(1)).copied().unwrap_or(0.0) / peak;
                    let k = ((v.clamp(0.0, 1.0)) * 7.0).round() as usize;
                    put(buf, r, 8 + i as u16, y, &bars[k.min(7)].to_string(), fg(mix(DIM, GLITCH2, 0.7)));
                }
                let p = (a.playhead() * w.saturating_sub(1) as f64).round() as usize;
                put_char(buf, r, 8 + p as u16, y, '●', fg(WHITE));
            }
            y += 1;
        }
        y += 1;
        put(buf, r, 0, y, &hint, fg(DIM));
        y += 1;
        if let Some(e) = &self.error {
            put(buf, r, 0, y, e, Style::new().fg(rgb((255, 95, 85))));
        }
    }
}

fn quit_key(code: KeyCode, mods: KeyModifiers) -> bool {
    code == KeyCode::Char('c') && mods.contains(KeyModifiers::CONTROL)
}

/// Asks the setup questions, showing `notice` (the last round's outcome) on
/// top. `None` if the user chose Exit or quit.
pub fn run(terminal: &mut DefaultTerminal, notice: Option<Notice>) -> Result<Option<Choices>, String> {
    let start = Instant::now();
    let mut s = Setup::new(notice);
    loop {
        s.sync_audition();
        let t = start.elapsed().as_secs_f64();
        terminal.draw(|f| s.draw(f, t)).map_err(|e| e.to_string())?;
        if !event::poll(FRAME).map_err(|e| e.to_string())? {
            continue;
        }
        let Event::Key(k) = event::read().map_err(|e| e.to_string())? else { continue };
        if k.kind != KeyEventKind::Press {
            continue;
        }
        if quit_key(k.code, k.modifiers) {
            return Ok(None);
        }
        // The event-grid editor takes over the keys while it is open.
        if let Some(layer) = s.grid_edit {
            match k.code {
                KeyCode::Esc => s.grid_edit = None,
                KeyCode::Up => s.grid_cursor.0 = (s.grid_cursor.0 + ROWS - 1) % ROWS,
                KeyCode::Down => s.grid_cursor.0 = (s.grid_cursor.0 + 1) % ROWS,
                KeyCode::Left => s.grid_cursor.1 = (s.grid_cursor.1 + STEPS - 1) % STEPS,
                KeyCode::Right => s.grid_cursor.1 = (s.grid_cursor.1 + 1) % STEPS,
                KeyCode::Char(' ') => {
                    let (r, c) = s.grid_cursor;
                    s.grids[layer].toggle(r, c);
                }
                KeyCode::Char('f' | 'F') => s.grids[layer] = Grid::fire(),
                KeyCode::Char('w' | 'W') => s.grids[layer] = Grid::water(),
                KeyCode::Char('s' | 'S') => s.grids[layer] = Grid::stones(),
                KeyCode::Char('o' | 'O') => s.grids[layer] = Grid::wood(),
                KeyCode::Char('c' | 'C') => s.grids[layer] = Grid::EMPTY,
                KeyCode::Enter => {
                    s.grid_edit = None;
                    if s.submit() {
                        break;
                    }
                    s.notice = None;
                }
                _ => {}
            }
            continue;
        }
        if s.step == Step::Menu {
            match k.code {
                KeyCode::Esc => return Ok(None),
                KeyCode::Up => s.menu_sel = s.menu_step(s.menu_sel, -1),
                KeyCode::Down => s.menu_sel = s.menu_step(s.menu_sel, 1),
                KeyCode::Enter => {
                    let rows = s.menu_rows();
                    match rows.get(s.menu_sel.min(rows.len().saturating_sub(1))) {
                        Some(MenuRow::Start) => {
                            if s.mode != Mode::Play && Path::new(&s.path).exists() {
                                s.step = Step::Overwrite;
                            } else {
                                break;
                            }
                        }
                        Some(MenuRow::Exit) => return Ok(None),
                        Some(MenuRow::Field { step, .. }) => {
                            s.step = *step;
                            s.input.clear();
                            s.error = None;
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
            continue;
        }
        match (s.step, k.code) {
            (_, KeyCode::Esc) => {
                s.step = Step::Menu;
                s.input.clear();
                s.error = None;
            }
            (step, KeyCode::Up | KeyCode::Down) if list_id(step).is_some() => {
                let (id, n) = (list_id(step).unwrap(), options(step, s.bpm).len());
                s.sel[id] = if k.code == KeyCode::Up { (s.sel[id] + n - 1) % n } else { (s.sel[id] + 1) % n };
            }
            (step @ (Step::Loop1 | Step::Loop2 | Step::Loop3 | Step::Loop4), KeyCode::Char(c @ '0'..='9')) => {
                let (id, n) = (list_id(step).unwrap(), options(step, s.bpm).len());
                let i = c as usize - '0' as usize;
                if i < n {
                    s.sel[id] = i;
                }
            }
            (step, KeyCode::Char(c @ '1'..='9')) if list_id(step).is_some() => {
                let (id, n) = (list_id(step).unwrap(), options(step, s.bpm).len());
                let i = c as usize - '1' as usize;
                if i < n {
                    s.sel[id] = i;
                }
            }
            (Step::Seed(i), KeyCode::Char('r' | 'R')) => {
                // A random seed, from the clock (seed 5 never rolls 0: that means "no drums").
                let nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos() as u64)
                    .unwrap_or(1);
                let v = shrine0011::rng::Rng::new(nanos ^ (i as u64) << 56).next_u64() % 100_000;
                s.input = (if i == 4 { v.max(1) } else { v }).to_string();
                s.error = None;
            }
            (Step::Overwrite, KeyCode::Char(c)) if "yYnN".contains(c) => {
                if c == 'y' || c == 'Y' {
                    break;
                }
                s.step = Step::Menu;
            }
            (step @ (Step::Loop1 | Step::Loop2 | Step::Loop3 | Step::Loop4), KeyCode::Enter) if s.sel[list_id(step).unwrap()] == LOOP_GRID => {
                s.grid_edit = loop_layer(step);
                s.grid_cursor = (0, 0);
            }
            (step @ (Step::Loop1 | Step::Loop2 | Step::Loop3 | Step::Loop4), KeyCode::Char('g' | 'G')) => {
                s.grid_edit = loop_layer(step);
                s.grid_cursor = (0, 0);
            }
            (_, KeyCode::Enter) => {
                if s.submit() {
                    break;
                }
                // Any new answer replaces the last round's message.
                s.notice = None;
            }
            (Step::Mode | Step::Scale | Step::Chords | Step::Pace | Step::Key | Step::Kit | Step::Space | Step::Loop1 | Step::Loop2 | Step::Loop3 | Step::Loop4 | Step::Overwrite, _) => {}
            (_, KeyCode::Backspace) => {
                s.input.pop();
            }
            (_, KeyCode::Char(c)) if s.input.chars().count() < 1024 && !c.is_control() => {
                s.input.push(c);
                s.error = None;
            }
            _ => {}
        }
    }
    if s.exit {
        return Ok(None);
    }
    // Remember these answers as next time's defaults.
    crate::state::save(&crate::state::Remembered {
        label: s.label.clone(),
        seeds: s.seeds(),
        settings: s.settings(),
        path: if s.mode == Mode::Play { s.last.path.clone() } else { s.path.clone() },
        seconds: if s.mode == Mode::Play { s.last.seconds } else { s.seconds },
    });
    Ok(Some(Choices {
        mode: s.mode,
        label: s.label.clone(),
        seeds: s.seeds(),
        settings: s.settings(),
        path: s.path,
        seconds: s.seconds,
    }))
}

/// One frame of "getting ready" while the engine freezes its chord.
pub fn starting(terminal: &mut DefaultTerminal, label: &str) -> Result<(), String> {
    terminal
        .draw(|f| {
            let r = screen(f, 2.0, "STARTING", 3, label);
            put(f.buffer_mut(), r, 0, 0, "Freezing the chords, recording the break, rendering the loops...", fg(TEXT));
            put(f.buffer_mut(), r, 0, 1, "This takes a second or two.", fg(DIM));
        })
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Renders the WAV with a progress bar in the setup box. Esc cancels back to
/// the menu, Ctrl-C cancels and exits. Returns (outcome, exit the app).
pub fn render(terminal: &mut DefaultTerminal, c: &Choices) -> Result<(Notice, bool), String> {
    let done = Arc::new(AtomicU64::new(0));
    let finished = Arc::new(AtomicBool::new(false));
    let cancel = Arc::new(AtomicBool::new(false));
    let total = (c.seconds * SAMPLE_RATE as f64) as u64;
    let worker = {
        let (done, finished, cancel) = (done.clone(), finished.clone(), cancel.clone());
        let (seeds, settings, path, seconds) = (c.seeds, c.settings, c.path.clone(), c.seconds);
        let info = cli::wav_info(&c.label, &format!("GlitchAmbiToolkit {VERSION}"), seeds, settings);
        std::thread::spawn(move || {
            let r = cli::render_with_progress(seeds, settings, &path, seconds, &info, |d, _| {
                done.store(d, Ordering::Relaxed);
                !cancel.load(Ordering::Relaxed)
            });
            finished.store(true, Ordering::Relaxed);
            r
        })
    };
    let start = Instant::now();
    let mut exit_app = false;
    while !finished.load(Ordering::Relaxed) {
        let t = start.elapsed().as_secs_f64();
        let d = done.load(Ordering::Relaxed);
        let frac = d as f64 / total.max(1) as f64;
        let speed = d as f64 / SAMPLE_RATE as f64 / t.max(0.001);
        terminal
            .draw(|f| {
                let r = screen(f, t, "RENDERING", 5, "Esc cancel · Ctrl-C cancel and exit");
                let buf = f.buffer_mut();
                put(buf, r, 0, 0, &format!("{} → {}", format_length(c.seconds), c.path), fg(TEXT));
                let bar_w = r.width.saturating_sub(8) as usize;
                put(buf, r, 0, 2, &hbar(bar_w, frac), fg(sweep_color(t * 0.05)));
                put(buf, r, bar_w as u16 + 1, 2, &format!("{:>5.1}%", frac * 100.0), fg(TEXT));
                let detail = format!(
                    "{} of {} rendered · {speed:.0}x real time",
                    format_length(d as f64 / SAMPLE_RATE as f64),
                    format_length(c.seconds)
                );
                put(buf, r, 0, 4, &detail, fg(DIM));
            })
            .map_err(|e| e.to_string())?;
        if event::poll(FRAME).map_err(|e| e.to_string())? {
            if let Event::Key(k) = event::read().map_err(|e| e.to_string())? {
                if k.kind == KeyEventKind::Press && (quit_key(k.code, k.modifiers) || k.code == KeyCode::Esc) {
                    exit_app = k.code != KeyCode::Esc;
                    cancel.store(true, Ordering::Relaxed);
                }
            }
        }
    }
    let notice = match worker.join() {
        Err(_) => Notice::error("The render stopped unexpectedly."),
        Ok(Err(e)) => Notice::error(format!("Could not write {}: {e}", c.path)),
        Ok(Ok(true)) => Notice::ok(format!(
            "Rendered {} to {} · recipe {}",
            format_length(c.seconds),
            c.path,
            cli::recipe(c.seeds, c.settings)
        )),
        Ok(Ok(false)) => {
            let got = done.load(Ordering::Relaxed) as f64 / SAMPLE_RATE as f64;
            Notice::ok(format!("Render cancelled: {} holds the first {} (no fade-out).", c.path, format_length(got)))
        }
    };
    Ok((notice, exit_app))
}

/// Text previews of the setup screen (for `--frame`): the first question, and
/// a later one with answers filled in.
pub fn preview(width: u16, height: u16) -> Vec<String> {
    use ratatui::backend::TestBackend;
    let mut out = Vec::new();
    let menu = Setup::new(Some(Notice::ok("Rendered 10:00 to 0011.wav.")));
    let mut field = Setup::new(None);
    field.step = Step::Loop1;
    for (setup, t) in [(&menu, 1.7), (&field, 3.71)] {
        let mut terminal = match ratatui::Terminal::new(TestBackend::new(width, height)) {
            Ok(t) => t,
            Err(_) => return out,
        };
        let _ = terminal.draw(|f| setup.draw(f, t));
        let screen = terminal.backend().buffer();
        for y in 0..height {
            out.push((0..width).map(|x| screen[(x, y)].symbol().to_string()).collect::<String>().trim_end().to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The setup box must fit a laptop terminal: the current question, its
    /// options and the hint stay on screen at 80x24 and smaller.
    #[test]
    fn setup_fits_small_terminals() {
        use ratatui::backend::TestBackend;
        for (w, h) in [(80u16, 24u16), (100, 30), (70, 22)] {
            // A long list (all atmosphere timbres) is the worst case for height.
            for step in [Step::Menu, Step::Mode, Step::Scale, Step::Loop1, Step::Loop2, Step::DroneCycle] {
                let mut s = Setup::new(None);
                s.step = step;
                let mut terminal = ratatui::Terminal::new(TestBackend::new(w, h)).unwrap();
                terminal.draw(|f| s.draw(f, 1.0)).unwrap();
                let buf = terminal.backend().buffer();
                let text: String = (0..h)
                    .map(|y| (0..w).map(|x| buf[(x, y)].symbol()).collect::<String>())
                    .collect::<Vec<_>>()
                    .join("\n");
                let (q, _) = s.question();
                if step == Step::Menu {
                    assert!(text.contains("Start") && text.contains("Atmosphere"), "{w}x{h}: menu missing rows\n{text}");
                    continue;
                }
                assert!(text.contains(q.trim()), "{w}x{h} {step:?}: lost question {q:?}\n{text}");
                assert!(text.contains("Enter"), "{w}x{h} {step:?}: lost hint\n{text}");
                // The event-grid editor must also fit and show its rows/hint.
                if step == Step::Loop1 {
                    let mut g = Setup::new(None);
                    g.step = Step::Loop1;
                    g.grid_edit = Some(0);
                    let mut terminal = ratatui::Terminal::new(TestBackend::new(w, h)).unwrap();
                    terminal.draw(|f| g.draw(f, 1.0)).unwrap();
                    let buf = terminal.backend().buffer();
                    let text: String = (0..h)
                        .map(|y| (0..w).map(|x| buf[(x, y)].symbol()).collect::<String>())
                        .collect::<Vec<_>>()
                        .join("\n");
                    assert!(text.contains("Event grid"), "{w}x{h}: grid question missing\n{text}");
                    assert!(text.contains("Crackle") && text.contains("Hiss"), "{w}x{h}: grid rows missing\n{text}");
                    assert!(text.contains("Fire"), "{w}x{h}: preset hint missing\n{text}");
                }
            }
        }
    }
}
