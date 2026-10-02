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
use shrine0010::cli::{self, format_length, FADE_OUT_SECONDS};
use shrine0010::harmony::{Pace, SCALES};
use shrine0010::{Seeds, Settings, DEFAULT_SEEDS, SAMPLE_RATE};
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
    let big = area.width >= TITLE_W + 4;
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
    let rect = Rect::new(
        area.x + (area.width - bw) / 2,
        area.y + (vy + 2).min(area.height.saturating_sub(box_height)),
        bw,
        box_height.min(area.height),
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
    Mode,
    Label,
    Scale,
    Chords,
    Pace,
    Kit,
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
fn list_id(step: Step) -> Option<usize> {
    match step {
        Step::Mode => Some(0),
        Step::Scale => Some(1),
        Step::Chords => Some(2),
        Step::Pace => Some(3),
        Step::Kit => Some(4),
        _ => None,
    }
}

const CHORD_CHOICES: [(&str, &str); 4] = [
    ("1 chord", "32 bars: one frozen chord, as in 0009"),
    ("2 chords", "20 / 12 bars"),
    ("3 chords", "16 / 12 / 4 bars"),
    ("4 chords", "14 / 10 / 4 / 4 bars"),
];
const PACE_CHOICES: [(&str, &str); 2] = [
    ("Half-time", "bars ≈ 2.9 s at 84 BPM: slow, drone-like changes"),
    ("Jungle", "bars ≈ 1.4 s at 168 BPM: changes move with the drums"),
];

fn options(step: Step) -> Vec<(String, String)> {
    let own = |v: &[(&str, &str)]| v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    match step {
        Step::Mode => own(&MODES),
        Step::Scale => SCALES.iter().map(|s| (s.name().to_string(), s.mood().to_string())).collect(),
        Step::Chords => own(&CHORD_CHOICES),
        Step::Pace => own(&PACE_CHOICES),
        Step::Kit => shrine0010::kits::KITS.iter().map(|k| (k.name().to_string(), k.blurb().to_string())).collect(),
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
    /// Selected entry of each list question (mode, scale, chords, pace, kit).
    sel: [usize; 5],
    input: String,
    error: Option<String>,
    mode: Mode,
    label: String,
    seeds: [u64; 5],
    path: String,
    seconds: f64,
}

impl Setup {
    fn new(notice: Option<Notice>) -> Self {
        let last = crate::state::load();
        let d = last.seeds;
        let set = last.settings;
        Setup {
            notice,
            exit: false,
            step: Step::Mode,
            back: Vec::new(),
            sel: [
                0,
                set.scale.index(),
                set.chords as usize - 1,
                if set.pace == Pace::Jungle { 1 } else { 0 },
                set.kit.index(),
            ],
            input: String::new(),
            error: None,
            mode: Mode::Play,
            label: last.label.clone(),
            seeds: [d.s1, d.s2, d.s3, d.s4, d.s5],
            path: last.path.clone(),
            seconds: last.seconds,
            last,
        }
    }

    fn settings(&self) -> Settings {
        Settings {
            scale: SCALES[self.sel[1]],
            chords: self.sel[2] as u8 + 1,
            pace: if self.sel[3] == 1 { Pace::Jungle } else { Pace::HalfTime },
            kit: shrine0010::kits::KITS[self.sel[4]],
        }
    }

    fn seeds(&self) -> Seeds {
        let s = self.seeds;
        Seeds { s1: s[0], s2: s[1], s3: s[2], s4: s[3], s5: s[4] }
    }

    fn default_for(&self, step: Step) -> String {
        let l = &self.last;
        let d = [l.seeds.s1, l.seeds.s2, l.seeds.s3, l.seeds.s4, l.seeds.s5];
        match step {
            Step::Mode | Step::Scale | Step::Chords | Step::Pace | Step::Kit => "1".into(),
            Step::Label => l.label.clone(),
            Step::Seed(i) => d[i].to_string(),
            Step::File => l.path.clone(),
            Step::Overwrite => "n".into(),
            Step::Length => format_length(l.seconds),
        }
    }

    fn question(&self) -> (String, String) {
        const SEED_ROLE: [&str; 5] =
            ["the key and the chords", "glitch layer 1", "glitch layer 2", "the bass", "the drum kit's sounds and break"];
        let list_hint = "↑ ↓ (or a number) to choose, Enter to confirm.".to_string();
        match self.step {
            Step::Mode => ("What would you like to do?".into(), list_hint),
            Step::Scale => ("Scale".into(), "The seed picks the key and builds the chords from this scale.".into()),
            Step::Chords => (
                "How many chords?".into(),
                "Chords 1 and 2 carry most of the 32-bar cycle; each change is a slow spectral morph.".into(),
            ),
            Step::Pace => ("Chord pace".into(), list_hint),
            Step::Kit => (
                "Drum kit".into(),
                "The next seed shapes this kit's sounds and its break. Choose Off for no drums.".into(),
            ),
            Step::Label => (
                "Title shown in the player".into(),
                "Up to 4 characters. It appears in the header and the explainer ticker.".into(),
            ),
            Step::Seed(i) => {
                let canonical = [DEFAULT_SEEDS.s1, DEFAULT_SEEDS.s2, DEFAULT_SEEDS.s3, DEFAULT_SEEDS.s4, DEFAULT_SEEDS.s5][i];
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

    fn next_step(&self) -> Option<Step> {
        let play_only = self.mode == Mode::Play;
        match self.step {
            Step::Mode => Some(if self.mode == Mode::Render { Step::Scale } else { Step::Label }),
            Step::Label => Some(Step::Scale),
            Step::Scale => Some(Step::Chords),
            Step::Chords => Some(Step::Pace),
            Step::Pace => Some(Step::Seed(0)),
            Step::Seed(3) => Some(Step::Kit),
            // No drums: nothing for seed 5 to shape.
            Step::Kit if shrine0010::kits::KITS[self.sel[4]] == shrine0010::kits::Kit::Off => {
                (!play_only).then_some(Step::File)
            }
            Step::Kit => Some(Step::Seed(4)),
            Step::Seed(i) if i < 4 => Some(Step::Seed(i + 1)),
            Step::Seed(_) => (!play_only).then_some(Step::File),
            Step::File => Some(if Path::new(&self.path).exists() { Step::Overwrite } else { Step::Length }),
            Step::Overwrite => Some(Step::Length),
            Step::Length => None,
        }
    }

    /// Accepts the current answer. Returns true when every question is answered.
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
            Step::Scale | Step::Chords | Step::Pace | Step::Kit => true,
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
            Step::File => {
                let mut p = answer;
                if !p.to_ascii_lowercase().ends_with(".wav") {
                    p.push_str(".wav");
                }
                self.path = p;
                true
            }
            Step::Overwrite => {
                if answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes") {
                    true
                } else {
                    // Back to choosing a name.
                    self.input.clear();
                    self.step = Step::File;
                    self.back.pop();
                    return false;
                }
            }
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
        match self.next_step() {
            Some(next) => {
                self.back.push(self.step);
                self.step = next;
                false
            }
            None => true,
        }
    }

    fn go_back(&mut self) -> bool {
        match self.back.pop() {
            Some(prev) => {
                self.step = prev;
                self.input.clear();
                self.error = None;
                true
            }
            None => false,
        }
    }

    /// The answers given so far, for the summary at the top of the box.
    fn summary(&self) -> Vec<(String, String)> {
        let mut rows = Vec::new();
        for step in &self.back {
            let row = match step {
                Step::Mode => ("Mode".to_string(), MODES[self.sel[0]].0.to_string()),
                Step::Scale => ("Scale".into(), SCALES[self.sel[1]].name().to_string()),
                Step::Chords => ("Chords".into(), format!("{} · {}", CHORD_CHOICES[self.sel[2]].0, CHORD_CHOICES[self.sel[2]].1)),
                Step::Pace => ("Pace".into(), PACE_CHOICES[self.sel[3]].0.to_string()),
                Step::Kit => ("Kit".into(), shrine0010::kits::KITS[self.sel[4]].name().to_string()),
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

    fn draw(&self, f: &mut Frame, t: f64) {
        let summary = self.summary();
        let opts = options(self.step);
        let options = if opts.is_empty() { 1 } else { opts.len() as u16 };
        let text_w = BOX_W.min(f.area().width.saturating_sub(2)).saturating_sub(6) as usize;
        let notice_lines = self.notice.as_ref().map(|n| super::ticker::wrap(&n.text, text_w)).unwrap_or_default();
        let notice_rows = if notice_lines.is_empty() { 0 } else { notice_lines.len() as u16 + 1 };
        // notice, summary (+ gap), question, options or input, gap, hint, error line, borders
        let height =
            notice_rows + summary.len() as u16 + (!summary.is_empty()) as u16 + 1 + options + 1 + 1 + 1 + 2;
        let footer = if self.step == Step::Mode { "Enter accept · Esc exit" } else { "Enter accept · Esc back · Ctrl-C exit" };
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
        for (k, v) in &summary {
            put(buf, r, 0, y, "✓", fg(BASS));
            put(buf, r, 2, y, &format!("{k:<10}"), fg(DIM));
            put(buf, r, 13, y, v, fg(TEXT));
            y += 1;
        }
        if !summary.is_empty() {
            y += 1;
        }
        let (q, hint) = self.question();
        put(buf, r, 0, y, &q, fg(TEXT).add_modifier(Modifier::BOLD));
        y += 1;
        let cursor = if (t * 2.0) as u64 % 2 == 0 { "▏" } else { " " };
        if let Some(id) = list_id(self.step) {
            let name_w = opts.iter().map(|o| o.0.chars().count()).max().unwrap_or(0).max(12);
            for (i, (name, desc)) in opts.iter().enumerate() {
                let sel = i == self.sel[id];
                let marker = if sel { "›" } else { " " };
                let style = if sel { fg(GLITCH2).add_modifier(Modifier::BOLD) } else { fg(TEXT) };
                put(buf, r, 0, y, &format!("{marker} {:>2}  {name:<name_w$}", i + 1), style);
                put(buf, r, 7 + name_w as u16, y, desc, fg(if sel { TEXT } else { DIM }));
                y += 1;
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
        match (s.step, k.code) {
            (_, KeyCode::Esc) => {
                if !s.go_back() {
                    return Ok(None);
                }
            }
            (step, KeyCode::Up | KeyCode::Down) if list_id(step).is_some() => {
                let (id, n) = (list_id(step).unwrap(), options(step).len());
                s.sel[id] = if k.code == KeyCode::Up { (s.sel[id] + n - 1) % n } else { (s.sel[id] + 1) % n };
            }
            (step, KeyCode::Char(c @ '1'..='9')) if list_id(step).is_some() => {
                let (id, n) = (list_id(step).unwrap(), options(step).len());
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
                let v = shrine0010::rng::Rng::new(nanos ^ (i as u64) << 56).next_u64() % 100_000;
                s.input = (if i == 4 { v.max(1) } else { v }).to_string();
                s.error = None;
            }
            (Step::Overwrite, KeyCode::Char(c)) if "yYnN".contains(c) => {
                s.input = c.to_string();
                if s.submit() {
                    break;
                }
            }
            (_, KeyCode::Enter) => {
                if s.submit() {
                    break;
                }
                // Any new answer replaces the last round's message.
                s.notice = None;
            }
            (Step::Mode | Step::Scale | Step::Chords | Step::Pace | Step::Kit | Step::Overwrite, _) => {}
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
            put(f.buffer_mut(), r, 0, 0, "Freezing the chords and recording the break...", fg(TEXT));
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
    let mut s = Setup::new(None);
    let mut later = Setup::new(Some(Notice::ok("Rendered 10:00 to 0010.wav.")));
    later.sel[0] = 2;
    for answer in ["", "SOS9", "", "", "", "", "42"] {
        later.input = answer.to_string();
        later.submit();
    }
    later.input = "0x1F".into();
    for (setup, t) in [(&mut s, 1.7), (&mut later, 3.71)] {
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
