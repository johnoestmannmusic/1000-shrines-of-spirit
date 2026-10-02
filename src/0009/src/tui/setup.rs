//! The start screen: an animated title, the version, and the setup questions
//! in a bordered box, styled like the rest of the TUI. Rendering a WAV also
//! happens here, with a progress bar in the same box.

use super::gfx::*;
use crate::{parse_label, parse_mode, Mode, DEFAULT_LABEL};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear};
use ratatui::{DefaultTerminal, Frame};
use shrine0009::cli::{self, format_length, DEFAULT_FILE, DEFAULT_SECONDS, FADE_OUT_SECONDS};
use shrine0009::{Seeds, DEFAULT_SEEDS, SAMPLE_RATE};
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
    Seed(usize),
    File,
    Overwrite,
    Length,
}

const MODES: [(&str, &str); 3] = [
    ("Play live", "endless, with a visual tour of the engine"),
    ("Render to WAV", "as fast as the computer can"),
    ("Play and record", "hear it while it is saved to a WAV file"),
];

struct Setup {
    step: Step,
    back: Vec<Step>,
    mode_sel: usize,
    input: String,
    error: Option<String>,
    mode: Mode,
    label: String,
    seeds: [u64; 4],
    path: String,
    seconds: f64,
}

impl Setup {
    fn new() -> Self {
        let d = DEFAULT_SEEDS;
        Setup {
            step: Step::Mode,
            back: Vec::new(),
            mode_sel: 0,
            input: String::new(),
            error: None,
            mode: Mode::Play,
            label: DEFAULT_LABEL.to_string(),
            seeds: [d.s1, d.s2, d.s3, d.s4],
            path: DEFAULT_FILE.to_string(),
            seconds: DEFAULT_SECONDS,
        }
    }

    fn default_for(&self, step: Step) -> String {
        let d = [DEFAULT_SEEDS.s1, DEFAULT_SEEDS.s2, DEFAULT_SEEDS.s3, DEFAULT_SEEDS.s4];
        match step {
            Step::Mode => "1".into(),
            Step::Label => DEFAULT_LABEL.into(),
            Step::Seed(i) => d[i].to_string(),
            Step::File => DEFAULT_FILE.into(),
            Step::Overwrite => "n".into(),
            Step::Length => format_length(DEFAULT_SECONDS),
        }
    }

    fn question(&self) -> (String, String) {
        const SEED_ROLE: [&str; 4] = ["the frozen chord", "glitch layer 1", "glitch layer 2", "the bass"];
        match self.step {
            Step::Mode => ("What would you like to do?".into(), "↑ ↓ or 1-3 to choose, Enter to confirm.".into()),
            Step::Label => (
                "Title shown in the player".into(),
                "Up to 4 characters. It appears in the header and the explainer ticker.".into(),
            ),
            Step::Seed(i) => (
                format!("Seed {}: shapes {}", i + 1, SEED_ROLE[i]),
                "Decimal or 0x hex. The defaults are the canonical version of the track.".into(),
            ),
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
            Step::Mode => Some(if self.mode == Mode::Render { Step::Seed(0) } else { Step::Label }),
            Step::Label => Some(Step::Seed(0)),
            Step::Seed(i) if i < 3 => Some(Step::Seed(i + 1)),
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
                self.mode = parse_mode(&(self.mode_sel + 1).to_string()).unwrap_or(Mode::Play);
                true
            }
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
                Step::Mode => ("Mode".to_string(), MODES[self.mode_sel].0.to_string()),
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
        let options = if self.step == Step::Mode { MODES.len() as u16 } else { 1 };
        // summary (+ gap), question, options or input, gap, hint, error line, borders
        let height = summary.len() as u16 + (!summary.is_empty()) as u16 + 1 + options + 1 + 1 + 1 + 2;
        let r = screen(f, t, "SETUP", height, "Enter accept · Esc back · Ctrl-C quit");
        let buf = f.buffer_mut();
        let mut y = 0;
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
        if self.step == Step::Mode {
            for (i, (name, desc)) in MODES.iter().enumerate() {
                let sel = i == self.mode_sel;
                let marker = if sel { "›" } else { " " };
                let style = if sel { fg(GLITCH2).add_modifier(Modifier::BOLD) } else { fg(TEXT) };
                put(buf, r, 0, y, &format!("{marker} {}  {name:<16}", i + 1), style);
                put(buf, r, 22, y, desc, fg(if sel { TEXT } else { DIM }));
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

/// Asks the setup questions. `None` if the user quit.
pub fn run(terminal: &mut DefaultTerminal) -> Result<Option<Choices>, String> {
    let start = Instant::now();
    let mut s = Setup::new();
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
            (Step::Mode, KeyCode::Up) => s.mode_sel = (s.mode_sel + MODES.len() - 1) % MODES.len(),
            (Step::Mode, KeyCode::Down) => s.mode_sel = (s.mode_sel + 1) % MODES.len(),
            (Step::Mode, KeyCode::Char(c @ '1'..='3')) => {
                s.mode_sel = c as usize - '1' as usize;
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
            }
            (Step::Mode | Step::Overwrite, _) => {}
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
    Ok(Some(Choices {
        mode: s.mode,
        label: s.label,
        seeds: Seeds { s1: s.seeds[0], s2: s.seeds[1], s3: s.seeds[2], s4: s.seeds[3] },
        path: s.path,
        seconds: s.seconds,
    }))
}

/// One frame of "getting ready" while the engine freezes its chord.
pub fn starting(terminal: &mut DefaultTerminal, label: &str) -> Result<(), String> {
    terminal
        .draw(|f| {
            let r = screen(f, 2.0, "STARTING", 3, label);
            put(f.buffer_mut(), r, 0, 0, "Rendering and freezing the FM chord...", fg(TEXT));
            put(f.buffer_mut(), r, 0, 1, "This takes about a second.", fg(DIM));
        })
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Renders the WAV with a progress bar in the setup box. Returns a summary
/// line to print after the screen closes.
pub fn render(terminal: &mut DefaultTerminal, c: &Choices) -> Result<String, String> {
    let done = Arc::new(AtomicU64::new(0));
    let finished = Arc::new(AtomicBool::new(false));
    let total = (c.seconds * SAMPLE_RATE as f64) as u64;
    let worker = {
        let (done, finished, seeds, path, seconds) = (done.clone(), finished.clone(), c.seeds, c.path.clone(), c.seconds);
        std::thread::spawn(move || {
            let r = cli::render_with_progress(seeds, &path, seconds, |d, _| done.store(d, Ordering::Relaxed));
            finished.store(true, Ordering::Relaxed);
            r
        })
    };
    let start = Instant::now();
    let mut cancelled = false;
    while !finished.load(Ordering::Relaxed) {
        let t = start.elapsed().as_secs_f64();
        let d = done.load(Ordering::Relaxed);
        let frac = d as f64 / total.max(1) as f64;
        let speed = d as f64 / SAMPLE_RATE as f64 / t.max(0.001);
        terminal
            .draw(|f| {
                let r = screen(f, t, "RENDERING", 5, "Ctrl-C cancel");
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
                if k.kind == KeyEventKind::Press && quit_key(k.code, k.modifiers) {
                    cancelled = true;
                    break;
                }
            }
        }
    }
    if cancelled {
        // The render thread is left to stop with the process; the file is incomplete.
        return Ok(format!("Cancelled. {} is incomplete.", c.path));
    }
    worker.join().map_err(|_| "render thread panicked".to_string())?.map_err(|e| format!("{}: {e}", c.path))?;
    Ok(format!("Rendered {} to {}.", format_length(c.seconds), c.path))
}

/// Text previews of the setup screen (for `--frame`): the first question, and
/// a later one with answers filled in.
pub fn preview(width: u16, height: u16) -> Vec<String> {
    use ratatui::backend::TestBackend;
    let mut out = Vec::new();
    let mut s = Setup::new();
    let mut later = Setup::new();
    later.mode_sel = 2;
    for answer in ["", "SOS9", "", "42"] {
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
