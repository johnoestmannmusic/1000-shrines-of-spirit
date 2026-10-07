//! The pipeline view: every chain from the signal-flow diagram drawn large,
//! with the sound itself shown as particles travelling through it.
//!
//! Particles are spawned by real engine events, at the moment they are
//! heard: a drone grain each time a new FFT frame starts sounding, a lettered
//! particle for every glitch hit, dimmer copies leaving the echo at its
//! delay time, a stream out of the reverb as thick as its level, and a
//! pulse from each atmosphere loop every time it goes round.

use super::draw::{kind_letter, kind_color, fmt_hz, note_name, fmt_ratio};
use super::gfx::*;
use super::ticker::wrap;
use super::{Panel, Ui};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType};
use ratatui::Frame;
use shrine0011::glitch::Kind;
use shrine0011::kits::DrumSpace;
use std::cell::RefCell;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Node {
    Seed1,
    Seed2,
    Seed3,
    Seed4,
    Fm,
    Freeze,
    Filter,
    G1,
    Rep1,
    G2,
    Rep2,
    Echo,
    Bass,
    Lp,
    Reverb,
    Out,
    Seed5,
    Brk,
    Smp,
    Chop,
    Bus,
    Seed6,
    Loop1,
}
use Node::*;

const NODES: [Node; 23] = [
    Seed1, Seed2, Seed3, Seed4, Fm, Freeze, Filter, G1, Rep1, G2, Rep2, Echo, Bass, Lp, Reverb, Out, Seed5, Brk, Smp,
    Chop, Bus, Seed6, Loop1,
];

/// (from, to, carries settings rather than sound)
const EDGES: [(Node, Node, bool); 24] = [
    (Seed1, Fm, true),
    (Seed2, G1, true),
    (Seed3, G2, true),
    (Seed4, Bass, true),
    (Fm, Freeze, false),
    (Freeze, Filter, false),
    (Filter, Out, false),
    (Filter, Reverb, false),
    (G1, Rep1, false),
    (Rep1, Echo, false),
    (G2, Rep2, false),
    (Rep2, Echo, false),
    (Echo, Reverb, false),
    (Bass, Lp, false),
    (Lp, Out, false),
    (Reverb, Out, false),
    (Seed5, Brk, true),
    (Brk, Smp, false),
    (Smp, Chop, false),
    (Chop, Bus, false),
    (Bus, Out, false),
    (Seed6, Loop1, true),
    (Loop1, Out, false),
    (Loop1, Reverb, false),
];

fn edge(from: Node, to: Node) -> usize {
    EDGES.iter().position(|e| e.0 == from && e.1 == to).expect("edge exists")
}

fn idx(n: Node) -> usize {
    NODES.iter().position(|m| *m == n).unwrap()
}

/// Where a particle goes after reaching a node, with a brightness factor.
fn onward(n: Node) -> &'static [(Node, Node, f64)] {
    match n {
        Fm => &[(Fm, Freeze, 1.0)],
        Freeze => &[(Freeze, Filter, 1.0)],
        Filter => &[(Filter, Out, 1.0), (Filter, Reverb, 0.45)],
        G1 => &[(G1, Rep1, 1.0)],
        G2 => &[(G2, Rep2, 1.0)],
        Rep1 => &[(Rep1, Echo, 1.0)],
        Rep2 => &[(Rep2, Echo, 1.0)],
        Echo => &[(Echo, Reverb, 1.0)],
        Bass => &[(Bass, Lp, 1.0)],
        Lp => &[(Lp, Out, 1.0)],
        Brk => &[(Brk, Smp, 1.0)],
        Smp => &[(Smp, Chop, 1.0)],
        Chop => &[(Chop, Bus, 1.0)],
        Bus => &[(Bus, Out, 1.0)],
        Loop1 => &[(Loop1, Out, 1.0), (Loop1, Reverb, 0.5)],
        _ => &[],
    }
}

/// Cells per second.
const SPEED: f64 = 42.0;
const ECHO_REPEATS: u32 = 4;

#[derive(Clone)]
pub struct Particle {
    edge: usize,
    pos: f64,
    color: Rgb,
    ch: char,
    bright: f64,
    /// A glitch hit: arriving at the echo schedules its repeats.
    hit: bool,
}

#[derive(Default)]
pub struct Pipeline {
    particles: Vec<Particle>,
    /// (time, particle) spawns waiting to happen (echo repeats).
    scheduled: Vec<(f64, Particle)>,
    flash: [f64; 23],
    /// Path lengths from the last draw, so motion can be advanced between draws.
    lengths: RefCell<Vec<usize>>,
    seed_timer: f64,
    bass_timer: f64,
    reverb_timer: f64,
    fm_timer: f64,
    loop_timer: f64,
    /// Whether the drums exist (kit not Off).
    pub drums_on: bool,
    /// Whether atmosphere loop 1 plays.
    pub loops_on: bool,
}

impl Pipeline {
    pub fn new(drums_on: bool, loops_on: bool) -> Self {
        Pipeline { drums_on, loops_on, ..Default::default() }
    }

    /// An atmosphere loop just went round: a bright pulse leaves it.
    pub fn loop_wrap(&mut self, level: f64) {
        self.flash[idx(Loop1)] = 1.0;
        self.spawn(Loop1, Out, LOOPS, '◌', 0.6 + 0.4 * level, false);
        self.spawn(Loop1, Reverb, LOOPS, '◌', 0.4 + 0.3 * level, false);
    }

    fn spawn(&mut self, from: Node, to: Node, color: Rgb, ch: char, bright: f64, hit: bool) {
        if self.particles.len() < 400 {
            self.particles.push(Particle { edge: edge(from, to), pos: 0.0, color, ch, bright, hit });
        }
        self.flash[idx(from)] = self.flash[idx(from)].max(bright);
    }

    /// A new drone frame (random-phase grain) started sounding.
    pub fn drone_grain(&mut self, level: f64) {
        self.spawn(Freeze, Filter, DRONE, '•', 0.55 + 0.45 * level, false);
    }

    /// A drum hit came out of the chopper.
    pub fn drum_hit(&mut self, v: shrine0011::drums::Voice) {
        self.flash[idx(Chop)] = 1.0;
        self.spawn(Chop, Bus, super::rhythm::voice_color(v), v.letter(), 1.0, false);
    }

    /// A glitch layer (0 or 1) fired an event.
    pub fn glitch_hit(&mut self, layer: usize, kind: Kind) {
        let (g, rep) = if layer == 0 { (G1, Rep1) } else { (G2, Rep2) };
        self.flash[idx(g)] = 1.0;
        self.spawn(rep, Echo, kind_color(kind), kind_letter(kind), 1.0, true);
    }

    /// Moves everything forward by `dt` seconds at animation time `t`.
    pub fn advance(&mut self, ui_levels: &super::Levels, echo_s: f64, feedback: f64, t: f64, dt: f64) {
        let lengths = self.lengths.borrow().clone();
        let len = |e: usize| *lengths.get(e).unwrap_or(&30) as f64;

        // Settings flowing from the seeds into their generators: slow and faint.
        self.seed_timer -= dt;
        if self.seed_timer <= 0.0 {
            self.seed_timer = 1.6;
            let mut seeds = vec![(Seed1, Fm), (Seed2, G1), (Seed3, G2), (Seed4, Bass)];
            if self.drums_on {
                seeds.push((Seed5, Brk));
            }
            if self.loops_on {
                seeds.push((Seed6, Loop1));
            }
            for (s, g) in seeds {
                self.particles.push(Particle { edge: edge(s, g), pos: 0.0, color: DIM, ch: '·', bright: 0.6, hit: false });
            }
        }
        // The FM chord is rendered once at start-up: show it streaming into the freeze briefly.
        if t < 6.0 {
            self.fm_timer -= dt;
            if self.fm_timer <= 0.0 {
                self.fm_timer = 0.12;
                self.spawn(Fm, Freeze, CHORD, '•', 0.9, false);
                // The break is also played and sampled once, at start-up.
                if self.drums_on {
                    self.spawn(Brk, Smp, DRUMS, '•', 0.9, false);
                }
            }
        }
        self.bass_timer -= dt;
        if self.bass_timer <= 0.0 {
            self.bass_timer = 0.3;
            let l = level_frac(ui_levels.bass, 40.0);
            if l > 0.05 {
                self.spawn(Bass, Lp, BASS, '•', 0.4 + 0.6 * l, false);
            }
        }
        // The loop plays continuously: a gentle stream as thick as its level.
        self.loop_timer -= dt;
        if self.loop_timer <= 0.0 {
            self.loop_timer = 0.45;
            let l = level_frac(ui_levels.loop1, 40.0);
            if self.loops_on && l > 0.05 {
                self.spawn(Loop1, Out, LOOPS, '•', 0.3 + 0.5 * l, false);
            }
        }
        // The reverb's output: denser and brighter as its tail grows.
        let rl = level_frac(ui_levels.reverb, 40.0);
        self.reverb_timer -= dt;
        if self.reverb_timer <= 0.0 && rl > 0.05 {
            self.reverb_timer = 0.55 - 0.4 * rl;
            self.spawn(Reverb, Out, REVERB, '·', 0.3 + 0.7 * rl, false);
        }

        // Due echo repeats.
        let mut due = Vec::new();
        self.scheduled.retain(|(at, p)| {
            if *at <= t {
                due.push(p.clone());
                false
            } else {
                true
            }
        });
        for p in due {
            self.flash[idx(Echo)] = self.flash[idx(Echo)].max(p.bright);
            self.particles.push(p);
        }

        // Move; handle arrivals.
        let mut arrived = Vec::new();
        for p in self.particles.iter_mut() {
            p.pos += SPEED * dt;
        }
        self.particles.retain(|p| {
            if p.pos >= len(p.edge) {
                arrived.push(p.clone());
                false
            } else {
                true
            }
        });
        for p in arrived {
            let (_, to, settings) = EDGES[p.edge];
            self.flash[idx(to)] = self.flash[idx(to)].max(p.bright * if settings { 0.4 } else { 1.0 });
            if settings {
                continue;
            }
            if to == Echo && p.hit {
                // The echo returns the hit again and again, quieter each time.
                for k in 1..=ECHO_REPEATS {
                    let bright = p.bright * feedback.powi(k as i32);
                    self.scheduled.push((
                        t + echo_s * k as f64,
                        Particle { edge: edge(Echo, Reverb), pos: 0.0, color: ECHO, ch: '○', bright, hit: false },
                    ));
                }
            }
            for (from, next, f) in onward(to) {
                let bright = p.bright * f;
                if bright > 0.08 {
                    self.particles.push(Particle {
                        edge: edge(*from, *next),
                        pos: 0.0,
                        color: p.color,
                        ch: p.ch,
                        bright,
                        hit: p.hit,
                    });
                }
            }
        }
        let decay = (-dt * 4.0).exp();
        for f in self.flash.iter_mut() {
            *f *= decay;
        }
    }
}

// ---------------------------------------------------------------- geometry

struct Geo {
    boxes: [Rect; 23],
    paths: Vec<Vec<(u16, u16)>>,
}

const BOX_H: u16 = 4;

fn layout(r: Rect) -> Option<Geo> {
    if r.width < 90 || r.height < 29 {
        return None;
    }
    let (seed_w, out_w) = (11u16, 13u16);
    let bw = ((r.width - seed_w - out_w - 5 * 4) / 4).min(24);
    let gap = (r.width - seed_w - out_w - 4 * bw) / 5;
    let cx = [0, seed_w + gap, seed_w + 2 * gap + bw, seed_w + 3 * gap + 2 * bw, seed_w + 4 * gap + 3 * bw];
    let c5 = seed_w + 5 * gap + 4 * bw;
    let gy = ((r.height.min(34) - 6 * BOX_H) / 5).clamp(1, 2);
    let y: Vec<u16> = (0..6).map(|i| i * (BOX_H + gy)).collect();
    let mid12 = (y[1] + y[2]) / 2;
    let rect = |x: u16, y: u16, w: u16| Rect::new(r.x + x, r.y + y, w, BOX_H);
    let mut boxes = [Rect::default(); 23];
    let mut set = |n: Node, rc: Rect| boxes[idx(n)] = rc;
    set(Seed1, rect(cx[0], y[0], seed_w));
    set(Seed2, rect(cx[0], y[1], seed_w));
    set(Seed3, rect(cx[0], y[2], seed_w));
    set(Seed4, rect(cx[0], y[3], seed_w));
    set(Fm, rect(cx[1], y[0], bw));
    set(Freeze, rect(cx[2], y[0], bw));
    set(Filter, rect(cx[3], y[0], bw));
    set(G1, rect(cx[1], y[1], bw));
    set(Rep1, rect(cx[2], y[1], bw));
    set(G2, rect(cx[1], y[2], bw));
    set(Rep2, rect(cx[2], y[2], bw));
    set(Echo, rect(cx[3], mid12, bw));
    set(Reverb, rect(cx[4], mid12, bw));
    set(Bass, rect(cx[1], y[3], bw));
    set(Lp, rect(cx[2], y[3], bw));
    set(Out, rect(c5, (mid12 + y[4]) / 2, out_w));
    set(Seed5, rect(cx[0], y[4], seed_w));
    set(Brk, rect(cx[1], y[4], bw));
    set(Smp, rect(cx[2], y[4], bw));
    set(Chop, rect(cx[3], y[4], bw));
    set(Bus, rect(cx[4], y[4], bw));
    set(Seed6, rect(cx[0], y[5], seed_w));
    set(Loop1, rect(cx[1], y[5], bw));

    // Wires run right from a box's middle, drop or rise on a column two cells
    // before the target (so wires into one box share a bus), then run in.
    let paths = EDGES
        .iter()
        .map(|(a, b, _)| {
            let (ra, rb) = (boxes[idx(*a)], boxes[idx(*b)]);
            let (x1, y1) = (ra.x + ra.width, ra.y + BOX_H / 2 - 1);
            let (x2, y2) = (rb.x.saturating_sub(1), rb.y + BOX_H / 2 - 1);
            let xm = if x2 >= x1 + 2 { x2 - 2 } else { x1 };
            let mut pts = Vec::new();
            for x in x1..=xm {
                pts.push((x, y1));
            }
            if y2 != y1 {
                let step: i32 = if y2 > y1 { 1 } else { -1 };
                let mut yy = y1 as i32 + step;
                while yy != y2 as i32 {
                    pts.push((xm, yy as u16));
                    yy += step;
                }
                pts.push((xm, y2));
            }
            for x in (xm + 1)..=x2 {
                pts.push((x, y2));
            }
            if y2 != y1 && xm == x2 {
                pts.push((x2, y2));
            }
            pts.dedup();
            pts
        })
        .collect();
    Some(Geo { boxes, paths })
}

/// Wire glyphs as sets of directions (left, right, up, down), so crossing
/// wires merge into proper junctions instead of overwriting each other.
const L: u8 = 1;
const R: u8 = 2;
const U: u8 = 4;
const D: u8 = 8;

fn bits(ch: char) -> u8 {
    match ch {
        '─' => L | R,
        '│' => U | D,
        '┐' => L | D,
        '┘' => L | U,
        '└' => U | R,
        '┌' => D | R,
        _ => 0,
    }
}

fn glyph(b: u8) -> char {
    match b {
        x if x == L | R => '─',
        x if x == U | D => '│',
        x if x == L | D => '┐',
        x if x == L | U => '┘',
        x if x == U | R => '└',
        x if x == D | R => '┌',
        x if x == L | R | D => '┬',
        x if x == L | R | U => '┴',
        x if x == U | D | R => '├',
        x if x == U | D | L => '┤',
        _ => '┼',
    }
}

fn wire_char(prev: Option<(u16, u16)>, cur: (u16, u16), next: Option<(u16, u16)>) -> char {
    let d = |a: (u16, u16), b: (u16, u16)| ((b.0 as i32 - a.0 as i32).signum(), (b.1 as i32 - a.1 as i32).signum());
    let din = prev.map(|p| d(p, cur));
    let dout = next.map(|n| d(cur, n));
    match (din, dout) {
        (_, None) => '▶',
        (Some((1, 0)), Some((0, 1))) => '┐',
        (Some((1, 0)), Some((0, -1))) => '┘',
        (Some((0, 1)), Some((1, 0))) => '└',
        (Some((0, -1)), Some((1, 0))) => '┌',
        (_, Some((0, _))) | (Some((0, _)), _) => '│',
        _ => '─',
    }
}

// ---------------------------------------------------------------- drawing

struct BoxText {
    title: String,
    lines: [String; 2],
    color: Rgb,
    level: f64,
    panel: Panel,
    tag: &'static str,
}

fn box_text(ui: &Ui, n: Node) -> BoxText {
    let d = &ui.desc;
    let l = &ui.levels;
    let s = ui.snap.as_ref();
    let lv = |x: f64| level_frac(x, 40.0);
    let seed = |i: usize, v: u64, p: Panel| BoxText {
        title: format!("seed {i}"),
        lines: [v.to_string(), String::new()],
        color: TEXT,
        level: 0.3,
        panel: p,
        tag: "",
    };
    let layer = |i: usize| s.map(|s| &s.layers[i]);
    let rep_line = |i: usize| {
        layer(i).map_or(String::new(), |ly| {
            format!("step {}/{} loop {}/{}", ly.step + 1, ly.steps.len(), (ly.loops + 1).min(ly.loops_until_mutation), ly.loops_until_mutation)
        })
    };
    let audible = |i: usize| {
        layer(i).map_or(String::new(), |ly| {
            let on = ly.steps.iter().flatten().filter(|st| st.threshold < ly.density).count();
            format!("{on}/{} steps audible", ly.steps.len())
        })
    };
    let last_hit = |i: usize| match ui.last_hit[i] {
        Some(k) => format!("last hit: {}", kind_letter(k)),
        None => "waiting".into(),
    };
    match n {
        Seed1 => seed(1, d.seeds.s1, Panel::Chord),
        Seed2 => seed(2, d.seeds.s2, Panel::Glitch),
        Seed3 => seed(3, d.seeds.s3, Panel::Glitch),
        Seed4 => seed(4, d.seeds.s4, Panel::Bass),
        Seed5 => BoxText {
            title: "seed 5".into(),
            lines: [d.seeds.s5.to_string(), if d.drums_on { String::new() } else { "drums off".into() }],
            color: TEXT,
            level: 0.3,
            panel: Panel::Drums,
            tag: "",
        },
        Seed6 => BoxText {
            title: "seed 6".into(),
            lines: [d.seeds.s6.to_string(), if d.loops[0].on() { String::new() } else { "loops off".into() }],
            color: TEXT,
            level: 0.3,
            panel: Panel::Loops,
            tag: "",
        },
        Loop1 if !d.loops[0].on() => BoxText {
            title: "LOOP 1".into(),
            lines: ["off (timbre: Off)".into(), String::new()],
            color: FAINT,
            level: 0.0,
            panel: Panel::Loops,
            tag: "10",
        },
        Loop1 => {
            let lp = &d.loops[0];
            BoxText {
                title: format!("LOOP 1 · {}", lp.timbre.code()),
                lines: [
                    format!("{} beats · {:.1} s", lp.beats, lp.len as f64 / d.sample_rate as f64),
                    s.map_or(String::new(), |s| format!("pass {}", s.clock / lp.len as u64 + 1)),
                ],
                color: LOOPS,
                level: lv(l.loop1),
                panel: Panel::Loops,
                tag: "10",
            }
        }
        Brk | Smp | Chop | Bus if !d.drums_on => BoxText {
            title: match n {
                Brk => "BREAK SYNTH",
                Smp => "SAMPLER",
                Chop => "CHOPPER",
                _ => "DRUM BUS",
            }
            .into(),
            lines: ["off (kit: Off)".into(), String::new()],
            color: FAINT,
            level: 0.0,
            panel: Panel::Drums,
            tag: "",
        },
        Brk => BoxText {
            title: "BREAK SYNTH".into(),
            lines: [
                d.drum_voices.kit.into(),
                format!("{} · {:.0} BPM", d.break_pattern, d.drum_voices.source_bpm),
            ],
            color: DRUMS,
            level: if ui.t < 6.0 { 1.0 } else { 0.3 },
            panel: Panel::Drums,
            tag: "3",
        },
        Smp => BoxText {
            title: "SAMPLER".into(),
            lines: [
                format!("{}-bit · {:.0} kHz", d.drum_voices.bits, d.drum_voices.hold_khz),
                {
                    let st = d.drum_voices.semitones_up;
                    format!("{} {st:+.1} semitones", if st >= 0.0 { "sped up" } else { "slowed" })
                },
            ],
            color: DRUMS,
            level: lv(l.drums) * 0.6,
            panel: Panel::Break,
            tag: "3",
        },
        Chop => {
            let (sec, fill) = s
                .and_then(|s| s.drum_plan.as_ref())
                .map_or(("waiting", false), |p| (p.section.name(), p.fill));
            BoxText {
                title: "CHOPPER".into(),
                lines: [sec.into(), if fill { "fill!".into() } else { "32 slices, re-sequenced".into() }],
                color: DRUMS,
                level: lv(l.drums),
                panel: Panel::Break,
                tag: "3",
            }
        }
        Bus => BoxText {
            title: "DRUM BUS".into(),
            lines: [
                if l.drums > 0.0 { format!("{:.1} dB", 20.0 * l.drums.log10()) } else { "-".into() },
                match d.settings.space {
                    DrumSpace::Centred => "centred · reverb send",
                    DrumSpace::Wide => "wide · reverb send",
                    DrumSpace::Tape => "wide · tape echo",
                }
                .into(),
            ],
            color: DRUMS,
            level: lv(l.drums),
            panel: Panel::Drums,
            tag: "",
        },
        Fm => BoxText {
            title: "FM CHORD".into(),
            lines: [
                format!("{} chord{} · 3-op FM", d.harmony.chords.len(), if d.harmony.chords.len() == 1 { "" } else { "s" }),
                "rendered once".into(),
            ],
            color: CHORD,
            level: if ui.t < 6.0 { 1.0 } else { 0.3 },
            panel: Panel::Chord,
            tag: "3",
        },
        Freeze => BoxText {
            title: "FFT FREEZE".into(),
            lines: [
                s.map_or(String::new(), |s| format!("grain #{}", s.drone_frames)),
                format!("new phases /{:.0}ms", d.hop as f64 / d.sample_rate as f64 * 1000.0),
            ],
            color: DRONE,
            level: lv(l.drone),
            panel: Panel::Spectrum,
            tag: "1 2",
        },
        Filter => BoxText {
            title: "FILTER SWEEP".into(),
            lines: [
                s.map_or(String::new(), |s| format!("low-pass {}", fmt_hz(s.drone_cutoff))),
                s.map_or(String::new(), |s| {
                    if s.duck_gain < 0.995 {
                        format!("drums: ducked {:.1} dB", 20.0 * s.duck_gain.log10())
                    } else {
                        "moved by slow LFOs".into()
                    }
                }),
            ],
            color: DRONE,
            level: lv(l.drone),
            panel: Panel::Cycles,
            tag: "5",
        },
        G1 | G2 => {
            let i = if n == G1 { 0 } else { 1 };
            BoxText {
                title: format!("GLITCH {}", i + 1),
                lines: [last_hit(i), "B T S P C X".into()],
                color: if i == 0 { GLITCH1 } else { GLITCH2 },
                level: lv(if i == 0 { l.glitch1 } else { l.glitch2 }),
                panel: Panel::Glitch,
                tag: "4",
            }
        }
        Rep1 | Rep2 => {
            let i = if n == Rep1 { 0 } else { 1 };
            BoxText {
                title: format!("REPEATS ×{}", d.layers[i].0),
                lines: [rep_line(i), audible(i)],
                color: if i == 0 { GLITCH1 } else { GLITCH2 },
                level: lv(if i == 0 { l.glitch1 } else { l.glitch2 }),
                panel: Panel::Glitch,
                tag: "4",
            }
        }
        Echo => BoxText {
            title: "ECHO".into(),
            lines: [
                format!("ping-pong {:.0} ms", d.echo_delay as f64 / d.sample_rate as f64 * 1000.0),
                format!("each repeat ×{:.2}", d.echo_feedback),
            ],
            color: ECHO,
            level: lv(l.echo),
            panel: Panel::Echo,
            tag: "7",
        },
        Bass => BoxText {
            title: "FM BASS + SUB".into(),
            lines: [
                format!("{} FM + {} sine", note_name(d.bass_root), note_name(d.bass_root - 12.0)),
                s.map_or(String::new(), |s| format!("ratio ×{} idx {:.2}", fmt_ratio(s.bass.0), s.bass.1)),
            ],
            color: BASS,
            level: lv(l.bass),
            panel: Panel::Bass,
            tag: "6",
        },
        Lp => BoxText {
            title: "LOW-PASS".into(),
            lines: [s.map_or(String::new(), |s| format!("{:.0} Hz", s.bass.2)), "skips the reverb".into()],
            color: BASS,
            level: lv(l.bass),
            panel: Panel::Bass,
            tag: "6",
        },
        Reverb => BoxText {
            title: "REVERB".into(),
            lines: ["8 delay lines".into(), format!("tail {:.0} s", d.reverb_decay)],
            color: REVERB,
            level: lv(l.reverb),
            panel: Panel::Reverb,
            tag: "8",
        },
        Out => {
            let peak = l.out_l.max(l.out_r);
            BoxText {
                title: "OUT".into(),
                lines: [
                    if peak > 0.0 { format!("{:.1} dB", 20.0 * peak.log10()) } else { "-".into() },
                    "soft clip".into(),
                ],
                color: OUTPUT,
                level: level_frac(peak, 48.0),
                panel: Panel::Output,
                tag: "9",
            }
        }
    }
}

fn draw_box(buf: &mut Buffer, rc: Rect, bt: &BoxText, flash: f64, explaining: bool) {
    let glow = (0.25 + 0.75 * bt.level).max(flash).min(1.0);
    let border = if explaining { mix(bt.color, WHITE, 0.3) } else { lit(bt.color, 0.25 + 0.6 * glow) };
    let area = Rect::new(rc.x, rc.y, rc.width, rc.height);
    let w = rc.width as usize;
    let top = format!("╭{}╮", "─".repeat(w.saturating_sub(2)));
    let mid = format!("│{}│", " ".repeat(w.saturating_sub(2)));
    let bot = format!("╰{}╯", "─".repeat(w.saturating_sub(2)));
    let b = fg(border);
    put(buf, area, 0, 0, &top, b);
    put(buf, area, 0, 1, &mid, b);
    put(buf, area, 0, 2, &mid, b);
    put(buf, area, 0, 3, &bot, b);
    let title_style = fg(mix(lit(bt.color, 0.6), WHITE, flash * 0.6)).add_modifier(Modifier::BOLD);
    let title = format!(" {} ", bt.title);
    let title_w = title.chars().count() as u16;
    put(buf, Rect::new(rc.x, rc.y, rc.width.saturating_sub(1), 1), 1, 0, &title, title_style);
    if explaining && title_w + 4 <= rc.width {
        put(buf, area, rc.width.saturating_sub(3), 0, "◆", fg(GLITCH2));
    }
    let inner = Rect::new(rc.x + 1, rc.y + 1, rc.width.saturating_sub(2), 2);
    put(buf, inner, 1, 0, &bt.lines[0], fg(mix(DIM, TEXT, glow)));
    put(buf, inner, 1, 1, &bt.lines[1], fg(DIM));
    if !bt.tag.is_empty() {
        let tag = format!(" {} ", bt.tag);
        put(buf, area, rc.width.saturating_sub(tag.chars().count() as u16 + 1), 3, &tag, fg(lit(bt.color, 0.6)));
    }
}

pub fn draw(ui: &Ui, f: &mut Frame, area: Rect, explaining: Panel) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(fg(FAINT))
        .title(Line::from(vec![
            Span::styled(" PIPELINE ", fg(OUTPUT).add_modifier(Modifier::BOLD)),
            Span::styled("· how each sound travels through the program ", fg(DIM)),
        ]))
        .title_bottom(Line::from(Span::styled(
            " numbers on boxes = panels in the engine view · [tab] switch view ",
            fg(DIM),
        )));
    let r = block.inner(area);
    f.render_widget(block, area);
    let buf = f.buffer_mut();

    // The chord progression across the top, then the diagram.
    super::rhythm::chord_strip(ui, buf, Rect::new(r.x + 1, r.y, r.width.saturating_sub(2), 2));
    let diagram_h = 35.min(r.height.saturating_sub(3));
    let diagram = Rect::new(r.x + 1, r.y + 3, r.width.saturating_sub(2), diagram_h);
    let Some(geo) = layout(diagram) else {
        put(buf, r, 1, 3, "Make the terminal at least 94 x 41 for the pipeline view, or press [tab].", fg(TEXT));
        return;
    };
    *ui.pipe.lengths.borrow_mut() = geo.paths.iter().map(|p| p.len()).collect();
    let whole = Rect::new(0, 0, buf.area.width, buf.area.height);

    // Wires: collect every cell's directions first, then draw merged glyphs.
    let levels = &ui.levels;
    let mut cells: std::collections::HashMap<(u16, u16), (u8, char, Rgb)> = std::collections::HashMap::new();
    for (e, path) in geo.paths.iter().enumerate() {
        let (from, _, settings) = EDGES[e];
        let bt = box_text(ui, from);
        let color = if settings {
            mix(FAINT, DIM, 0.5)
        } else {
            lit(bt.color, 0.3 + 0.35 * bt.level.max(level_frac(levels.out_l, 60.0) * 0.2))
        };
        for (i, cell) in path.iter().enumerate() {
            let prev = if i > 0 { Some(path[i - 1]) } else { None };
            let next = path.get(i + 1).copied();
            let ch = wire_char(prev, *cell, next);
            let entry = cells.entry(*cell).or_insert((0, ch, color));
            if ch == '▶' || (settings && ch == '─') {
                entry.1 = if ch == '▶' { '▶' } else { '╌' };
            } else {
                entry.0 |= bits(ch);
                entry.1 = glyph(entry.0);
            }
            entry.2 = color;
        }
    }
    for ((x, y), (_, ch, color)) in cells {
        put_char(buf, whole, x, y, ch, fg(color));
    }

    // Boxes.
    for n in NODES {
        let bt = box_text(ui, n);
        let lit_up = explaining == bt.panel || explaining == Panel::Flow;
        draw_box(buf, geo.boxes[idx(n)], &bt, ui.pipe.flash[idx(n)], lit_up);
    }

    // Particles.
    for p in &ui.pipe.particles {
        if let Some(path) = geo.paths.get(p.edge) {
            if let Some(cell) = path.get((p.pos as usize).min(path.len().saturating_sub(1))) {
                let style = fg(mix(FAINT, p.color, p.bright)).add_modifier(Modifier::BOLD);
                put_char(buf, whole, cell.0, cell.1, p.ch, style);
            }
        }
    }

    // Legend and the live event log.
    let bottom = geo.boxes.iter().map(|b| b.y + b.height).max().unwrap_or(r.y) + 1;
    let below = Rect::new(r.x + 1, bottom, r.width.saturating_sub(2), (r.y + r.height).saturating_sub(bottom));
    if below.height == 0 {
        return;
    }
    let legend = vec![
        Span::styled("•", fg(DRONE)),
        Span::styled(" sound moving through a chain   ", fg(DIM)),
        Span::styled("B T S P C X", fg(kind_color(Kind::Ping))),
        Span::styled(" glitch hits   ", fg(DIM)),
        Span::styled("○", fg(ECHO)),
        Span::styled(" echo repeats, dimmer each time   ", fg(DIM)),
        Span::styled("K S g h O R", fg(DRUMS)),
        Span::styled(" drum hits", fg(DIM)),
    ];
    buf.set_line(below.x, below.y, &Line::from(legend), below.width);
    let second = vec![
        Span::styled("╌·", fg(DIM)),
        Span::styled(" a seed setting up its generator   ", fg(DIM)),
        Span::styled(
            "glitches, echoes and drums also reach the reverb (not drawn); every chain is heard dry too",
            fg(DIM),
        ),
    ];
    buf.set_line(below.x, below.y + 1, &Line::from(second), below.width);
    if below.height < 4 {
        return;
    }
    put(buf, below, 0, 3, "WHAT'S CHANGING", fg(TEXT).add_modifier(Modifier::BOLD));
    let mut y = 4;
    for (time, text, color) in ui.events.iter().rev() {
        if y >= below.height {
            break;
        }
        let stamp = format!("{:>7}  ", super::clock(*time));
        put(buf, below, 0, y, &stamp, fg(DIM));
        for line in wrap(text, (below.width as usize).saturating_sub(10)).iter() {
            if y >= below.height {
                break;
            }
            put(buf, below, 9, y, line, fg(*color));
            y += 1;
        }
    }
    if ui.events.is_empty() {
        put(buf, below, 0, 4, "Changes in the music will be listed here as they happen.", fg(DIM));
    }
}
