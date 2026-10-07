//! The live TUI. Two views:
//! - Pipeline: the signal chains drawn large, with the sound shown as
//!   particles travelling through them, plus a log of what is changing.
//! - Engine: one panel per part of the engine, each an audio visualisation
//!   that also shows how that part works.
//!
//! Along the bottom, a ticker shows explainers one at a time.

mod draw;
mod explain;
mod gfx;
mod learn;
mod pipeline;
mod rhythm;
pub mod setup;
mod ticker;

use crate::audio::{Live, Published, Recording};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::Frame;
use shrine0011::glitch::Kind;
use shrine0011::pattern::Mutation;
use shrine0011::telemetry::{Description, Snapshot};
use shrine0011::{Seeds, Settings, SAMPLE_RATE};
use std::collections::VecDeque;
use std::io::IsTerminal;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Panel {
    General,
    Flow,
    Spectrum,
    Ola,
    Chord,
    Glitch,
    Cycles,
    Bass,
    Echo,
    Reverb,
    Output,
    Harmony,
    Break,
    Drums,
    /// The atmosphere loops.
    Loops,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    Pipeline,
    Engine,
    Rhythm,
    Learn,
}

/// Recent output frames kept for the oscilloscope.
const SCOPE_FRAMES: usize = 4096;
/// Echo level history (one entry per ~10.7 ms block).
const ECHO_HISTORY: usize = 400;
const MAX_EVENTS: usize = 40;
const FRAME_TIME: Duration = Duration::from_millis(33);

/// Peak levels with a falling release, so meters read smoothly at 30 fps.
#[derive(Default, Clone, Copy)]
pub struct Levels {
    pub drone: f64,
    pub bass: f64,
    pub glitch1: f64,
    pub glitch2: f64,
    pub echo: f64,
    pub reverb: f64,
    pub drums: f64,
    pub loop1: f64,
    pub loop2: f64,
    pub loop3: f64,
    pub loop4: f64,
    pub out_l: f64,
    pub out_r: f64,
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a.max(1) } else { gcd(b, a % b) }
}

/// "m:ss.s" for event timestamps.
pub fn clock(seconds: f64) -> String {
    format!("{}:{:04.1}", (seconds / 60.0) as u64, seconds % 60.0)
}

pub struct Ui {
    pub label: String,
    pub view: View,
    pub desc: Description,
    pub snap: Option<Snapshot>,
    pub scope: VecDeque<(f32, f32)>,
    pub echo_history: VecDeque<(f64, f64)>,
    pub levels: Levels,
    pub reverb_levels: [f64; 8],
    pub frame_overview: Vec<f32>,
    /// Seconds since the newest drone frame started sounding.
    pub frame_age: f64,
    pub flashes: [Vec<f64>; 2],
    pub last_hit: [Option<Kind>; 2],
    pub pipe: pipeline::Pipeline,
    /// (audio time, text, colour) of notable changes, newest last.
    pub events: VecDeque<(f64, String, gfx::Rgb)>,
    last_triggers: [u64; 2],
    /// Drum hits: flash per voice (K S g h O R) and the last seen count.
    pub drum_flash: [f64; 6],
    last_drum_hits: u64,
    /// The Learn view's chosen lesson (it stays put until the user changes it).
    pub lesson: usize,
    /// Instrument lessons solo their layer unless `s` switched back to the mix.
    pub solo_listen: bool,
    /// Spectrum of what you are hearing (for the instrument lessons).
    pub heard_spectrum: Vec<f64>,
    fft: Option<shrine0011::fft::Fft>,
    /// (drum level, drone duck gain) per block, for the "making room" lesson.
    pub duck_history: VecDeque<(f64, f64)>,
    /// The slow cycles, rebuilt here so the Learn view can draw the future
    /// (they are a pure function of the clock).
    pub mods: shrine0011::modulate::Mods,
    /// (phrase, section) for recent phrases, oldest first.
    pub sections: VecDeque<(u64, shrine0011::drums::Section)>,
    last_mutation: [Option<u64>; 2],
    last_audible: [Option<usize>; 2],
    last_update: Instant,
    /// Seconds since start (animation clock).
    pub t: f64,
    pub played: u64,
    pub paused: bool,
    pub quitting: bool,
    pub recording: Option<(String, f64)>,
    pub device: String,
    pub device_rate: u32,
    pub underruns: u64,
    ticker: ticker::Ticker,
}

impl Ui {
    fn new(desc: Description, device: String, device_rate: u32, recording: Option<(String, f64)>, label: String) -> Self {
        let (s1, s2) = (desc.layers[0].0, desc.layers[1].0);
        let pipe = pipeline::Pipeline::new(desc.drums_on, desc.loops.iter().any(|l| l.on()));
        let mods = shrine0011::modulate::Mods::new(desc.seeds.s1);
        let (kit, space, loops) = (desc.settings.kit, desc.settings.space, desc.settings.loops);
        let drone = desc.settings.drone;
        let n = desc.harmony.chords.len();
        let vars = explain::Vars {
            kit: kit.name().to_string(),
            key: shrine0011::harmony::key_name(desc.harmony.key).to_string(),
            key_source: match desc.settings.key {
                shrine0011::KeyChoice::Seed => format!("seed 1 picks it ({} for this seed)", shrine0011::harmony::key_name(desc.harmony.key)),
                shrine0011::KeyChoice::Note(_) => format!("fixed at {} by the player", shrine0011::harmony::key_name(desc.harmony.key)),
            },
            scale: desc.harmony.scale.name().to_string(),
            chords: format!("{n} chord{}", if n == 1 { "" } else { "s" }),
            recipe: shrine0011::cli::recipe(desc.seeds, desc.settings),
            tempo: desc.tempo,
            loop1: desc.loops[0].design.name(),
            loop2: desc.loops[1].design.name(),
            loop1_len: explain::loop_len(&desc.loops[0], desc.sample_rate),
            loop2_len: explain::loop_len(&desc.loops[1], desc.sample_rate),
            loops: {
                let on: Vec<String> = (0..4)
                    .filter(|&k| desc.loops[k].on())
                    .map(|k| format!("loop {} {}", k + 1, desc.loops[k].design.name()))
                    .collect();
                on.join(" and ")
            },
            loops_len: {
                let on: Vec<String> = (0..4)
                    .filter(|&k| desc.loops[k].on())
                    .map(|k| format!("loop {} {}", k + 1, explain::loop_len(&desc.loops[k], desc.sample_rate)))
                    .collect();
                on.join("; ")
            },
            loops_meet: {
                let (a, b) = (desc.loops[0].beats, desc.loops[1].beats);
                format!("{} beats", a * b / gcd(a, b))
            },
            drone_arc: if drone.is_on() {
                format!(
                    "every {} s the drone swells up for {} s and then leaves for {} s",
                    drone.cycle_s,
                    drone.hold_s,
                    drone.cycle_s - drone.hold_s
                )
            } else {
                "the drone plays continuously".to_string()
            },
            drone_cycle: drone.cycle_s.to_string(),
            drone_hold: drone.hold_s.to_string(),
        };
        Ui {
            label,
            view: View::Pipeline,
            desc,
            snap: None,
            scope: VecDeque::with_capacity(SCOPE_FRAMES),
            echo_history: VecDeque::with_capacity(ECHO_HISTORY),
            levels: Levels::default(),
            reverb_levels: [0.0; 8],
            frame_overview: Vec::new(),
            frame_age: 1.0,
            flashes: [vec![0.0; s1], vec![0.0; s2]],
            last_hit: [None; 2],
            pipe,
            events: VecDeque::new(),
            last_triggers: [0; 2],
            drum_flash: [0.0; 6],
            last_drum_hits: 0,
            sections: VecDeque::new(),
            lesson: 0,
            solo_listen: true,
            heard_spectrum: Vec::new(),
            fft: None,
            duck_history: VecDeque::new(),
            mods,
            last_mutation: [None; 2],
            last_audible: [None; 2],
            last_update: Instant::now(),
            t: 0.0,
            played: 0,
            paused: false,
            quitting: false,
            recording,
            device,
            device_rate,
            underruns: 0,
            ticker: ticker::Ticker::new(kit, space, loops, drone, vars),
        }
    }

    /// Takes in every published block whose audio has reached the sound card.
    fn update(&mut self, live: &Live) {
        let now = Instant::now();
        let dt = (now - self.last_update).as_secs_f64();
        self.last_update = now;
        let shared = &live.shared;
        self.played = shared.played.load(Ordering::Relaxed);
        self.underruns = shared.underruns.load(Ordering::Relaxed);
        self.advance(dt);

        let ready: Vec<Published> = match shared.published.lock() {
            Ok(mut q) => {
                let mut out = Vec::new();
                while let Some(front) = q.front() {
                    let frames = (front.samples.len() / 2) as u64;
                    if front.snapshot.clock - frames <= self.played {
                        out.extend(q.pop_front());
                    } else {
                        break;
                    }
                }
                out
            }
            Err(_) => Vec::new(),
        };
        for p in ready {
            self.absorb(p);
        }

        // Instrument lessons solo their layer (only what you hear; recordings get the mix).
        let solo = self.wanted_solo();
        let code = solo.and_then(|s| crate::audio::SOLOS.iter().position(|x| *x == s)).map_or(0, |i| i as u8 + 1);
        shared.solo.store(code, Ordering::Relaxed);
        if solo.is_some() || learn::is_instrument(self.lesson) && self.view == View::Learn {
            self.update_spectrum();
        }
    }

    /// The layer the Learn view wants to hear on its own right now.
    pub fn wanted_solo(&self) -> Option<shrine0011::Solo> {
        if self.view != View::Learn || !self.solo_listen || self.quitting {
            return None;
        }
        learn::solo_for(self.lesson).filter(|s| match s {
            shrine0011::Solo::Drums => self.desc.drums_on,
            shrine0011::Solo::Loop1 => self.desc.loops[0].on(),
            shrine0011::Solo::Loop2 => self.desc.loops[1].on(),
            _ => true,
        })
    }

    /// A 2048-point spectrum of the most recent audio you heard.
    fn update_spectrum(&mut self) {
        const N: usize = 2048;
        if self.scope.len() < N {
            return;
        }
        let fft = self.fft.get_or_insert_with(|| shrine0011::fft::Fft::new(N));
        let mut re: Vec<f64> = self.scope.iter().rev().take(N).rev().map(|(l, r)| 0.5 * (*l as f64 + *r as f64)).collect();
        for (i, x) in re.iter_mut().enumerate() {
            *x *= 0.5 - 0.5 * (i as f64 / N as f64 * std::f64::consts::TAU).cos();
        }
        let mut im = vec![0.0; N];
        fft.transform(&mut re, &mut im, false);
        let fresh: Vec<f64> = (0..N / 2).map(|k| (re[k] * re[k] + im[k] * im[k]).sqrt()).collect();
        // Smooth over frames so the bars read calmly.
        if self.heard_spectrum.len() != fresh.len() {
            self.heard_spectrum = fresh;
        } else {
            for (s, f) in self.heard_spectrum.iter_mut().zip(fresh) {
                *s = s.max(f) * 0.85 + f * 0.15;
            }
        }
    }

    /// Moves animations forward by `dt` seconds.
    fn advance(&mut self, dt: f64) {
        self.t += dt;
        self.ticker.advance(self.t);
        let release = (-dt * 5.0).exp();
        let l = &mut self.levels;
        for v in [
            &mut l.drone,
            &mut l.bass,
            &mut l.glitch1,
            &mut l.glitch2,
            &mut l.echo,
            &mut l.reverb,
            &mut l.drums,
            &mut l.loop1,
            &mut l.loop2,
            &mut l.loop3,
            &mut l.loop4,
            &mut l.out_l,
            &mut l.out_r,
        ] {
            *v *= release;
        }
        for v in self.reverb_levels.iter_mut() {
            *v *= release;
        }
        let flash_decay = (-dt * 7.0).exp();
        for layer in self.flashes.iter_mut() {
            for f in layer.iter_mut() {
                *f *= flash_decay;
            }
        }
        for f in self.drum_flash.iter_mut() {
            *f *= (-dt * 9.0).exp();
        }
        if !self.paused {
            self.frame_age += dt;
            let sr = self.desc.sample_rate as f64;
            let echo_s = self.desc.echo_delay as f64 / sr;
            self.pipe.advance(&self.levels, echo_s, self.desc.echo_feedback, self.t, dt);
        }
    }

    fn event(&mut self, at: f64, text: String, color: gfx::Rgb) {
        if self.events.len() >= MAX_EVENTS {
            self.events.pop_front();
        }
        self.events.push_back((at, text, color));
    }

    fn absorb(&mut self, p: Published) {
        let s = p.snapshot;
        let m = &s.meters;
        let l = &mut self.levels;
        l.drums = l.drums.max(m.drums);
        l.loop1 = l.loop1.max(m.loop1);
        l.loop2 = l.loop2.max(m.loop2);
        l.loop3 = l.loop3.max(m.loop3);
        l.loop4 = l.loop4.max(m.loop4);
        l.drone = l.drone.max(m.drone);
        l.bass = l.bass.max(m.bass);
        l.glitch1 = l.glitch1.max(m.glitch1);
        l.glitch2 = l.glitch2.max(m.glitch2);
        l.echo = l.echo.max(m.echo_l).max(m.echo_r);
        l.reverb = l.reverb.max(m.reverb);
        l.out_l = l.out_l.max(m.out_l);
        l.out_r = l.out_r.max(m.out_r);
        for (v, p) in self.reverb_levels.iter_mut().zip(s.reverb_lines) {
            *v = v.max(p);
        }
        if self.echo_history.len() >= ECHO_HISTORY {
            self.echo_history.pop_front();
        }
        if self.duck_history.len() >= ECHO_HISTORY {
            self.duck_history.pop_front();
        }
        self.duck_history.push_back((m.drums, s.duck_gain));
        self.echo_history.push_back((m.echo_l, m.echo_r));
        for frame in p.samples.chunks_exact(2) {
            if self.scope.len() >= SCOPE_FRAMES {
                self.scope.pop_front();
            }
            self.scope.push_back((frame[0], frame[1]));
        }
        if let Some(overview) = &s.new_frame {
            self.frame_overview = overview.clone();
            self.frame_age = 0.0;
            self.pipe.drone_grain(gfx::level_frac(self.levels.drone, 40.0));
        }

        let now = s.clock as f64 / SAMPLE_RATE as f64;
        let colors = [gfx::GLITCH1, gfx::GLITCH2];
        for (i, layer) in s.layers.iter().enumerate() {
            if layer.triggers != self.last_triggers[i] {
                let hits = layer.triggers.saturating_sub(self.last_triggers[i]).min(4);
                self.last_triggers[i] = layer.triggers;
                if let Some(f) = self.flashes[i].get_mut(layer.step) {
                    *f = 1.0;
                }
                if let Some(Some(step)) = layer.steps.get(layer.step) {
                    self.last_hit[i] = Some(step.kind);
                    for _ in 0..hits {
                        self.pipe.glitch_hit(i, step.kind);
                    }
                }
            }
            // Narrate mutations and the layer thinning out / filling in.
            if let Some((clock, step, what)) = layer.last_mutation {
                if self.last_mutation[i] != Some(clock) {
                    self.last_mutation[i] = Some(clock);
                    let change = match what {
                        Mutation::Replace(k) => format!("step {} became a new {}", step + 1, draw::kind_name(k)),
                        Mutation::Clear => format!("step {} was cleared", step + 1),
                        Mutation::Rotate => "the whole loop rotated one step".to_string(),
                        Mutation::Nudge => format!("step {} was re-pitched", step + 1),
                    };
                    let text = format!(
                        "Glitch layer {} mutated after {} loops: {change}. The new pattern now repeats.",
                        i + 1,
                        layer.loops_until_mutation.max(1)
                    );
                    self.event(now, text, colors[i]);
                }
            }
            let audible = layer.steps.iter().flatten().filter(|st| st.threshold < layer.density).count();
            if self.last_audible[i] != Some(audible) {
                if let Some(before) = self.last_audible[i] {
                    let verb = if audible > before { "fills in" } else { "thins out" };
                    let text = format!(
                        "Glitch layer {} {verb}: {audible} of {} steps now sound (slow density LFOs at {:.2}).",
                        i + 1,
                        layer.steps.len(),
                        layer.density
                    );
                    self.event(now, text, gfx::mix(colors[i], gfx::DIM, 0.4));
                }
                self.last_audible[i] = Some(audible);
            }
        }
        // Drum hits: flash each voice and send particles down the drum lane.
        if s.drum_hit_count != self.last_drum_hits {
            self.last_drum_hits = s.drum_hit_count;
            for v in &s.drum_hits {
                let i = rhythm::voice_index(*v);
                self.drum_flash[i] = 1.0;
                self.pipe.drum_hit(*v);
            }
        }
        if let Some(plan) = &s.drum_plan {
            let phrase = plan.bar / shrine0011::drums::PHRASE_BARS;
            if self.sections.back().is_none_or(|(p, _)| *p != phrase) {
                let changed = self.sections.back().is_some_and(|(_, s)| *s != plan.section);
                if self.sections.len() >= 24 {
                    self.sections.pop_front();
                }
                self.sections.push_back((phrase, plan.section));
                if changed {
                    let text = format!("Drums move to {} at phrase {} (every phrase is 4 bars).", plan.section.name(), phrase + 1);
                    self.event(now, text, gfx::mix(gfx::DRUMS, gfx::DIM, 0.3));
                }
            }
        }
        // Each time an atmosphere loop goes round, a pulse leaves it in the pipeline.
        if let Some(prev) = &self.snap {
            for (k, level) in [self.levels.loop1, self.levels.loop2, self.levels.loop3, self.levels.loop4].into_iter().enumerate() {
                let len = self.desc.loops[k].len as u64;
                if self.desc.loops[k].on() && prev.clock / len != s.clock / len {
                    self.pipe.loop_wrap(gfx::level_frac(level, 40.0));
                }
            }
        }
        if let Some(prev) = &self.snap {
            if prev.harmony.chord != s.harmony.chord {
                let c = &self.desc.harmony.chords[s.harmony.chord];
                let text = format!(
                    "Chord {} of {} begins (degree {}, {} bars); the drone has finished morphing into it.",
                    s.harmony.chord + 1,
                    self.desc.harmony.chords.len(),
                    c.degree + 1,
                    c.bars
                );
                self.event(now, text, gfx::CHORD);
            }
        }
        self.snap = Some(s);
    }

    /// Seconds of audio heard so far.
    pub fn elapsed(&self) -> f64 {
        self.played as f64 / SAMPLE_RATE as f64
    }

    fn draw(&self, f: &mut Frame) {
        let area = f.area();
        let ticker_rows = if area.height >= 24 { 2 } else { 1 };
        let [body, ticker] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(ticker_rows)]).areas(area);
        let explaining = self.ticker.panel();
        self.ticker.draw(f.buffer_mut(), ticker, self.t);

        if area.width < 80 || area.height < 22 {
            draw::too_small(f.buffer_mut(), body);
            return;
        }
        let [header, main] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(body);
        draw::header(self, f.buffer_mut(), header);
        match self.view {
            View::Pipeline => pipeline::draw(self, f, main, explaining),
            View::Engine => self.draw_engine(f, main, explaining),
            View::Rhythm => rhythm::draw(self, f, main, explaining),
            View::Learn => learn::draw(self, f, main, explaining),
        }
    }

    fn draw_engine(&self, f: &mut Frame, grid: Rect, explaining: Panel) {
        let [row1, row2, row3] = Layout::vertical([
            Constraint::Percentage(31),
            Constraint::Percentage(38),
            Constraint::Percentage(31),
        ])
        .areas(grid);
        let wide = [Constraint::Percentage(46), Constraint::Percentage(27), Constraint::Percentage(27)];
        let [spectrum, ola, chord] = Layout::horizontal(wide).areas(row1);
        let [glitch, cycles, bass] = Layout::horizontal(wide).areas(row2);
        let [echo, reverb, loops, output] = Layout::horizontal([
            Constraint::Percentage(21),
            Constraint::Percentage(21),
            Constraint::Percentage(28),
            Constraint::Percentage(30),
        ])
        .areas(row3);

        let panels: [(Panel, Rect); 10] = [
            (Panel::Spectrum, spectrum),
            (Panel::Ola, ola),
            (Panel::Chord, chord),
            (Panel::Glitch, glitch),
            (Panel::Cycles, cycles),
            (Panel::Bass, bass),
            (Panel::Echo, echo),
            (Panel::Reverb, reverb),
            (Panel::Loops, loops),
            (Panel::Output, output),
        ];
        for (panel, rect) in panels {
            draw::panel(self, f, panel, rect, explaining == panel);
        }
    }

    fn key(&mut self, code: KeyCode, live: &Live) {
        match code {
            KeyCode::Tab => {
                self.view = match self.view {
                    View::Pipeline => View::Engine,
                    View::Engine => View::Rhythm,
                    View::Rhythm => View::Learn,
                    View::Learn => View::Pipeline,
                };
            }
            KeyCode::BackTab => {
                self.view = match self.view {
                    View::Pipeline => View::Learn,
                    View::Engine => View::Pipeline,
                    View::Rhythm => View::Engine,
                    View::Learn => View::Rhythm,
                };
            }
            KeyCode::Char('4') => {
                self.view = View::Learn;
            }
            KeyCode::Char('s') if self.view == View::Learn && learn::is_instrument(self.lesson) => {
                self.solo_listen = !self.solo_listen;
            }
            KeyCode::Up | KeyCode::Char('[') if self.view == View::Learn => {
                self.lesson = (self.lesson + learn::LESSONS.len() - 1) % learn::LESSONS.len();
            }
            KeyCode::Down | KeyCode::Char(']') if self.view == View::Learn => {
                self.lesson = (self.lesson + 1) % learn::LESSONS.len();
            }
            KeyCode::Char('1') => self.view = View::Pipeline,
            KeyCode::Char('2') => self.view = View::Engine,
            KeyCode::Char('3') => self.view = View::Rhythm,
            KeyCode::Right => self.ticker.next(self.t),
            KeyCode::Left => self.ticker.prev(self.t),
            KeyCode::Char(' ') if !self.quitting => {
                self.paused = !self.paused;
                live.shared.paused.store(self.paused, Ordering::Relaxed);
            }
            _ => {}
        }
    }
}

/// Plays the track live in the TUI, optionally recording it (for flag-driven runs).
pub fn run(seeds: Seeds, settings: Settings, recording: Option<Recording>, label: String) -> Result<(), String> {
    if !std::io::stdout().is_terminal() {
        return Err("live playback needs an interactive terminal".into());
    }
    let mut terminal = ratatui::init();
    let _ = setup::starting(&mut terminal, &label);
    let result = play(&mut terminal, seeds, settings, recording, label);
    ratatui::restore();
    let (notice, _) = result?;
    if notice.error {
        return Err(notice.text);
    }
    println!("{}", notice.text);
    Ok(())
}

/// Plays in an already-open TUI terminal until the user quits or a recording
/// ends. Returns (what happened, whether the user asked to exit the app).
pub fn play(
    terminal: &mut ratatui::DefaultTerminal,
    seeds: Seeds,
    settings: Settings,
    recording: Option<Recording>,
    label: String,
) -> Result<(setup::Notice, bool), String> {
    let rec_info = recording.as_ref().map(|r| (r.path.clone(), r.seconds));
    let mut live = match Live::start(seeds, settings, recording) {
        Ok(live) => live,
        Err(e) => return Ok((setup::Notice::error(format!("Could not start playback: {e}")), false)),
    };
    // Diagnostics: SHRINE_MUTE=1 runs everything but outputs silence.
    if std::env::var_os("SHRINE_MUTE").is_some() {
        live.shared.mute.store(true, Ordering::Relaxed);
    }

    let mut ui = Ui::new(live.description.clone(), live.device.clone(), live.device_rate, rec_info.clone(), label);
    let mut quit_at: Option<Instant> = None;
    let mut exit_app = false;
    let result = loop {
        ui.update(&live);
        if let Err(e) = terminal.draw(|f| ui.draw(f)) {
            break Err(e.to_string());
        }
        match event::poll(FRAME_TIME) {
            Ok(true) => {
                if let Ok(Event::Key(k)) = event::read() {
                    if k.kind == KeyEventKind::Press {
                        let ctrl_c = k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL);
                        if ctrl_c || matches!(k.code, KeyCode::Char('q') | KeyCode::Esc) {
                            exit_app |= ctrl_c;
                            quit(&live, &mut ui, &mut quit_at);
                        } else {
                            ui.key(k.code, &live);
                        }
                    }
                }
            }
            Ok(false) => {}
            Err(e) => break Err(e.to_string()),
        }
        let shared = &live.shared;
        let done = shared.finished.load(Ordering::Relaxed)
            && shared.played.load(Ordering::Relaxed) >= shared.end.load(Ordering::Relaxed);
        let timed_out = quit_at.is_some_and(|t| t.elapsed() > Duration::from_secs(5));
        if done || timed_out {
            break Ok(());
        }
    };
    result?;

    let heard = shrine0011::cli::format_length(live.shared.played.load(Ordering::Relaxed) as f64 / SAMPLE_RATE as f64);
    let notice = match live.join() {
        Ok(Some(frames)) => {
            let (path, _) = rec_info.unwrap_or_default();
            let secs = frames as f64 / SAMPLE_RATE as f64;
            let mut msg = format!(
                "Recorded {} to {path} · recipe {}.",
                shrine0011::cli::format_length(secs),
                shrine0011::cli::recipe(seeds, settings)
            );
            if quit_at.is_some() {
                msg += " Stopped early, so it ends with a short fade where you quit.";
            }
            setup::Notice::ok(msg)
        }
        Ok(None) => setup::Notice::ok(format!(
            "Played {heard} live · recipe {}",
            shrine0011::cli::recipe(seeds, settings)
        )),
        Err(e) => setup::Notice::error(format!("Recording failed: {e}")),
    };
    Ok((notice, exit_app))
}

fn quit(live: &Live, ui: &mut Ui, quit_at: &mut Option<Instant>) {
    if quit_at.is_none() {
        *quit_at = Some(Instant::now());
        ui.quitting = true;
        ui.paused = false;
        live.shared.paused.store(false, Ordering::Relaxed);
        live.shared.stop.store(true, Ordering::Relaxed);
    }
}

/// Runs the engine offline to `seconds` and prints the TUI at that moment as
/// plain text (no sound card or terminal needed). Shows both views.
pub fn print_frame(seeds: Seeds, settings: Settings, seconds: f64, width: u16, height: u16, label: &str) -> Result<(), String> {
    use ratatui::backend::TestBackend;
    use shrine0011::Track;
    let mut track = Track::new(seeds, settings);
    let mut ui = Ui::new(track.describe(), "preview".into(), SAMPLE_RATE, None, label.to_string());
    let total = (seconds * SAMPLE_RATE as f64) as u64;
    let block = 512;
    let mut buf = vec![0f32; block * 2];
    // Only the last stretch matters for the display (meters, histories, events).
    let warm = total.saturating_sub(40 * SAMPLE_RATE as u64);
    while track.clock() < total {
        track.render(&mut buf);
        let dt = block as f64 / SAMPLE_RATE as f64;
        if track.clock() >= warm {
            ui.advance(dt);
            ui.played = track.clock();
            ui.absorb(Published { snapshot: track.snapshot(), samples: buf.clone() });
        } else {
            ui.t += dt;
        }
    }
    for line in setup::preview(width, height) {
        println!("{line}");
    }
    for (view, lesson) in [
        (View::Pipeline, 0),
        (View::Engine, 0),
        (View::Rhythm, 0),
        (View::Learn, 0),
        (View::Learn, 8),
        (View::Learn, 9),
        (View::Learn, 13),
        (View::Learn, 14),
        (View::Learn, 15),
        (View::Learn, 16),
    ] {
        ui.lesson = lesson;
        ui.update_spectrum();
        ui.view = view;
        let mut terminal = ratatui::Terminal::new(TestBackend::new(width, height)).map_err(|e| e.to_string())?;
        // Draw twice: the first draw measures the wires the particles travel along.
        terminal.draw(|f| ui.draw(f)).map_err(|e| e.to_string())?;
        terminal.draw(|f| ui.draw(f)).map_err(|e| e.to_string())?;
        let screen = terminal.backend().buffer();
        for y in 0..height {
            let line: String = (0..width).map(|x| screen[(x, y)].symbol().to_string()).collect();
            println!("{}", line.trim_end());
        }
    }
    Ok(())
}
