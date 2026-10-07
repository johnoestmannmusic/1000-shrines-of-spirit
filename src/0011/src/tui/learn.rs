//! View 4, "Learn": how this piece is made, as a series of lessons. Each
//! lesson has a live visualisation of *this* version's real data, a short
//! "how it's made here", and a "recreate it" checklist with this version's
//! actual settings, in tool-agnostic terms, so a listener could rebuild the
//! idea in their own studio.

use super::draw::{fmt_ratio, note_name};
use super::gfx::*;
use super::rhythm::{op_glyph, section_glyph, voice_color};
use super::ticker::wrap;
use super::{Panel, Ui};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType};
use ratatui::Frame;
use shrine0011::drone::Algo;
use shrine0011::drums::{PHRASE_BARS, STEPS};
use shrine0011::harmony::{key_name, Pace};
use shrine0011::kits::DrumSpace;
use shrine0011::{Solo, DRONE_CUTOFF_MIN, DRONE_CUTOFF_OCTAVES, SAMPLE_RATE};
use std::f64::consts::TAU;


/// (title, the panel its Insights are tagged with). The first 8 explain how
/// the piece is made; the last 6 each solo one instrument.
pub const LESSONS: [(&str, Panel); 14] = [
    ("The score", Panel::Cycles),
    ("The chord", Panel::Chord),
    ("Freezing", Panel::Spectrum),
    ("Slow motion", Panel::Harmony),
    ("Glitch loops", Panel::Glitch),
    ("Space", Panel::Reverb),
    ("The break", Panel::Break),
    ("Making room", Panel::Drums),
    ("Drone", Panel::Spectrum),
    ("Glitch layer 1", Panel::Glitch),
    ("Glitch layer 2", Panel::Glitch),
    ("Bass", Panel::Bass),
    ("Drums", Panel::Drums),
    ("Space (echo+reverb)", Panel::Reverb),
];

/// Lessons from here on are instrument lessons.
const FIRST_INSTRUMENT: usize = 8;

pub fn is_instrument(lesson: usize) -> bool {
    lesson >= FIRST_INSTRUMENT
}

/// The layer an instrument lesson solos.
pub fn solo_for(lesson: usize) -> Option<Solo> {
    if !is_instrument(lesson) {
        return None;
    }
    crate::audio::SOLOS.get(lesson - FIRST_INSTRUMENT).copied()
}

const SR: f64 = SAMPLE_RATE as f64;
const ACCENT: Rgb = (255, 214, 120);

pub fn draw(ui: &Ui, f: &mut Frame, area: Rect, explaining: Panel) {
    let [list, main] = Layout::horizontal([Constraint::Length(26), Constraint::Min(40)]).areas(area);

    // The lesson list.
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(fg(mix(FAINT, ACCENT, 0.4)))
        .title(Span::styled(" LEARN · how it's made ", fg(ACCENT).add_modifier(Modifier::BOLD)))
        .title_bottom(Line::from(Span::styled(" ↑↓ choose · s solo ", fg(DIM))));
    let r = block.inner(list);
    f.render_widget(block, list);
    let buf = f.buffer_mut();
    put(buf, r, 1, 0, "HOW IT'S MADE", fg(DIM).add_modifier(Modifier::BOLD));
    put(buf, r, 1, FIRST_INSTRUMENT as u16 + 2, "INSTRUMENTS · solo", fg(DIM).add_modifier(Modifier::BOLD));
    for (i, (title, panel)) in LESSONS.iter().enumerate() {
        let y = i as u16 + 1 + if is_instrument(i) { 2 } else { 0 };
        if y >= r.height {
            break;
        }
        let on = i == ui.lesson;
        let style = if on {
            Style::new().fg(rgb((20, 20, 20))).bg(rgb(ACCENT)).add_modifier(Modifier::BOLD)
        } else {
            fg(TEXT)
        };
        put(buf, r, 0, y, &format!(" {:>2} {:<20}", i + 1, title), style);
        if *panel == explaining {
            put(buf, r, r.width.saturating_sub(2), y, "◆", fg(GLITCH2));
        }
    }

    // The lesson itself.
    let (title, _) = LESSONS[ui.lesson];
    let instrument = is_instrument(ui.lesson);
    let soloing = ui.wanted_solo().is_some();
    let heading = if instrument {
        format!(" INSTRUMENT · {} ", title.to_uppercase())
    } else {
        format!(" LESSON {} · {} ", ui.lesson + 1, title.to_uppercase())
    };
    let status = if !instrument {
        String::new()
    } else if soloing {
        " ● SOLO: you hear only this layer · s = full mix ".to_string()
    } else if solo_for(ui.lesson) == Some(Solo::Drums) && !ui.desc.drums_on {
        " no drums in this version ".to_string()
    } else {
        " ○ full mix · s = solo this layer ".to_string()
    };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(fg(mix(FAINT, ACCENT, 0.6)))
        .title(Line::from(vec![
            Span::styled(heading, fg(ACCENT).add_modifier(Modifier::BOLD)),
            Span::styled(status, if soloing { Style::new().fg(rgb((20, 20, 20))).bg(rgb(ACCENT)) } else { fg(DIM) }),
        ]));
    let r = block.inner(main);
    f.render_widget(block, main);
    let Some(_) = &ui.snap else { return };
    let (h1, how, h2, recreate) = if instrument {
        let (a, b) = instrument_text(ui);
        ("ON ITS OWN", a, "HOW IT FITS", b)
    } else {
        let (a, b) = text(ui);
        ("HOW IT'S MADE HERE", a, "RECREATE IT", b)
    };
    let width = r.width.saturating_sub(4) as usize;
    let how_lines = wrap(&how, width);
    let mut recreate_lines = Vec::new();
    for item in &recreate {
        for (j, l) in wrap(item, width.saturating_sub(2)).into_iter().enumerate() {
            recreate_lines.push((j == 0, l));
        }
    }
    let text_h = (how_lines.len() + recreate_lines.len() + 4) as u16;
    let vis_h = r.height.saturating_sub(text_h).max(6);
    let vis = Rect::new(r.x + 1, r.y, r.width.saturating_sub(2), vis_h.min(r.height));
    let buf = f.buffer_mut();
    match ui.lesson {
        0 => score(ui, buf, vis),
        1 => chord(ui, buf, vis),
        2 => freeze(ui, buf, vis),
        3 => motion(ui, buf, vis),
        4 => loops(ui, buf, vis),
        5 => space(ui, buf, vis),
        6 => brk(ui, buf, vis),
        7 => room(ui, buf, vis),
        _ => instrument_view(ui, buf, vis),
    }
    let mut y = vis_h + 1;
    let tr = Rect::new(r.x + 2, r.y, r.width.saturating_sub(3), r.height);
    put(buf, tr, 0, y, h1, fg(ACCENT).add_modifier(Modifier::BOLD));
    y += 1;
    for l in how_lines {
        put(buf, tr, 0, y, &l, fg(TEXT));
        y += 1;
    }
    y += 1;
    put(buf, tr, 0, y, h2, fg(ACCENT).add_modifier(Modifier::BOLD));
    y += 1;
    for (first, l) in recreate_lines {
        if first {
            put(buf, tr, 0, y, "▸", fg(ACCENT));
        }
        put(buf, tr, 2, y, &l, fg(TEXT));
        y += 1;
    }
}

// ---------------------------------------------------------------- words

/// How the break reaches the track's tempo: faster (pitch up) or slower.
fn speed_word(semitones_up: f64) -> &'static str {
    if semitones_up >= 0.0 {
        "sped up"
    } else {
        "slowed"
    }
}

fn text(ui: &Ui) -> (String, Vec<String>) {
    let d = &ui.desc;
    let h = &d.harmony;
    let key = key_name(h.key);
    let scale = h.scale.name();
    let t = d.tempo;
    let (bpm, half) = (t.bpm, t.half_bpm());
    let pace = if d.settings.pace == Pace::HalfTime {
        format!("half-time ({half:.0} BPM)")
    } else {
        format!("jungle ({bpm} BPM)")
    };
    let bars: Vec<String> = h.chords.iter().map(|c| c.bars.to_string()).collect();
    let pos = ui.snap.as_ref().map(|s| s.harmony).unwrap_or_default();
    let chord = &h.chords[pos.chord];
    let notes: Vec<String> = chord.notes.iter().map(|n| note_name(*n)).collect();
    let v = &d.drum_voices;
    let drums = d.drums_on;
    match ui.lesson {
        0 => (
            format!(
                "Nothing here is arranged by hand. Seven slow cycles (41 to 307 seconds, all prime) decide when the filter opens, when the glitch loops thin out and when the drums play; the chords change on a fixed {}-bar plan ({} bars). The strip above is the next few minutes, worked out in advance.",
                32,
                bars.join(" / ")
            ),
            vec![
                "Write the form as slow automation lanes rather than sections: one per layer, each a slow LFO of a different, unrelated length.".to_string(),
                "Let one lane decide how busy the glitches are, another whether the drums play, a third how bright the drone is.".to_string(),
                format!("Change chords rarely: here {} chord{} over 32 bars at {} pace.", h.chords.len(), if h.chords.len() == 1 { "" } else { "s" }, pace),
            ],
        ),
        1 => {
            let (algo, [r2, r3, i2, i3], _) = d.recipes[pos.chord][0];
            (
                format!(
                    "Each note of the chord is a 3-operator FM voice. This chord ({} of {}) is built in {} {} from scale degree {}: {}.",
                    pos.chord + 1,
                    h.chords.len(),
                    key,
                    scale,
                    chord.degree + 1,
                    notes.join(" ")
                ),
                vec![
                    format!(
                        "In any FM synth with 3 operators: carrier ratio 1, modulators ×{} and ×{}, index about {:.1} and {:.1}, wired as {}.",
                        fmt_ratio(r2),
                        fmt_ratio(r3),
                        i2,
                        i3,
                        if algo == Algo::Stack { "a stack (3 → 2 → 1)" } else { "a pair (2 and 3 both into 1)" }
                    ),
                    format!("Voice the chord wide, about three octaves: {}.", notes.join(" ")),
                    "Sway the modulation depth slowly (0.1 to 0.3 Hz) so the tone breathes, then record about 8 seconds.".to_string(),
                ],
            )
        }
        2 => (
            "The recorded chord is never played back directly. Its spectrum is measured and averaged, then rebuilt every 171 ms with the same strengths but brand-new random phases: the sound keeps its colour but loses all motion, an endless still cloud.".to_string(),
            vec![
                "Freeze the 8-second chord with a spectral freeze or an extreme time-stretch (50× or more) using long windows (about 0.7 s).".to_string(),
                "Do it twice, pan one left and one right: independent randomness gives a wide drone that never cancels in mono.".to_string(),
                "Cut everything below ~110 Hz from the freeze; leave the low end to the bass.".to_string(),
            ],
        ),
        3 => (
            format!(
                "The frozen drone runs through a low-pass filter between {:.0} Hz and {:.1} kHz, moved by two slow cycles added together, so it never quite repeats. At each chord change the frozen spectra cross-fade over one bar.",
                DRONE_CUTOFF_MIN,
                DRONE_CUTOFF_MIN * 2f64.powf(DRONE_CUTOFF_OCTAVES) / 1000.0
            ),
            vec![
                format!(
                    "Low-pass the drone, sweeping roughly {:.0} Hz to {:.1} kHz.",
                    DRONE_CUTOFF_MIN,
                    DRONE_CUTOFF_MIN * 2f64.powf(DRONE_CUTOFF_OCTAVES) / 1000.0
                ),
                "Drive the cutoff from two LFOs of unrelated lengths (here 61 s and 233 s); add a gentle resonance LFO too.".to_string(),
                "Move between chords with a slow cross-fade between their freezes, about one bar long.".to_string(),
            ],
        ),
        4 => (
            "Two step sequencers of different lengths, 7 sixteenths and 11 eighths, run side by side, so their patterns drift against each other and only line up every 154 sixteenths (27.5 s). Every few loops, one step changes.".to_string(),
            vec![
                format!("Make two loops of co-prime lengths (7 and 11 steps), one in 16ths and one in 8ths, at {half:.0} BPM."),
                "After 3 to 9 repeats, change one step: a new sound, a rest, a new pitch, or rotate the loop.".to_string(),
                "Use tiny sounds: crushed FM blips, noise ticks, two-sine bell pings, short square buzzes, 1-bit noise bursts, short grains cut from the drone.".to_string(),
                "Gate each step against a slow 'density' control so the loops thin out and fill back in.".to_string(),
            ],
        ),
        5 => {
            let ms = d.echo_delay as f64 / SR * 1000.0;
            (
                format!(
                    "Glitches go through a ping-pong echo ({:.0} ms, three 16ths) whose repeats darken as they fade, then everything shares one long reverb with a {:.0}-second tail.",
                    ms, d.reverb_decay
                ),
                vec![
                    format!("Ping-pong delay at 3/16 ({half:.0} BPM, {:.0} ms), {:.0}% feedback, a low-pass inside the feedback.", ms, d.echo_feedback * 100.0),
                    format!("A long reverb on a send (about {:.0} s), high-passed at 180 Hz so the tail never gets muddy.", d.reverb_decay),
                    "Send the drone and glitches generously; the drums only a little, so they stay close.".to_string(),
                ],
            )
        }
        6 if !drums => (
            "This version has no drums (kit: Off). Choose a kit at setup to hear a break built, sampled and chopped live.".to_string(),
            vec!["Sometimes the strongest choice is to leave the drums out entirely.".to_string()],
        ),
        6 => (
            format!(
                "The {} kit plays a two-bar break once at {:.0} BPM; it is then sampled ({}-bit, {:.0} kHz), {} to {bpm} BPM ({:+.1} semitones), cut into 32 slices and re-sequenced bar by bar. Every 4-bar phrase picks a section from a slow cycle.",
                v.kit, v.source_bpm, v.bits, v.hold_khz, speed_word(v.semitones_up), v.semitones_up
            ),
            vec![
                format!("Program a 2-bar break at about {:.0} BPM: kick, snare, quiet ghost notes, hats, with a little swing.", v.source_bpm),
                format!("Kit sounds here: kick {}; snare {}; hats {}.", v.lines[0], v.lines[1], v.lines[2]),
                format!(
                    "Resample it ({}-bit) and play it at {bpm} BPM so it {} in pitch.",
                    v.bits,
                    if v.semitones_up >= 0.0 { "rises" } else { "drops" }
                ),
                "Chop it into 16ths. In some bars, reverse, roll or swap a few slices; add a fill on every 4th bar.".to_string(),
                "Arrange in 4-bar phrases: out, hats only, half-time, full, full with rolls.".to_string(),
                space_tip(d.settings.space).to_string(),
            ],
        ),
        _ if !drums => (
            "With no drums in this version, nothing needs to duck: the drone stays at full level and full brightness.".to_string(),
            vec!["When you add drums to a dense drone, plan to carve space for them (see this lesson with a kit chosen).".to_string()],
        ),
        _ => (
            "While the drums play, the drone steps back: about 5 dB quieter and 6 dB darker above 2.5 kHz, with a tiny quick dip on each hit. When the drums drop out, the drone swells back.".to_string(),
            vec![
                "Sidechain-compress the drone from the drum bus with a slow release (about 1.5 s) for around −5 dB while drums play.".to_string(),
                "Add a fast, shallow duck on each hit (about 60 ms release, −1 dB) so the kick has room.".to_string(),
                "Shelve the drone's top end down about 6 dB above 2.5 kHz while the drums are in.".to_string(),
            ],
        ),
    }
}

// ---------------------------------------------------------------- pictures

/// Lesson 1: the next few minutes as lanes.
fn score(ui: &Ui, buf: &mut Buffer, r: Rect) {
    let d = &ui.desc;
    let Some(s) = &ui.snap else { return };
    if r.width < 30 || r.height < 6 {
        return;
    }
    let label_w = 10u16;
    let w = (r.width - label_w) as usize;
    let span = 480.0; // seconds shown
    let start = (ui.elapsed() - 60.0).max(0.0);
    let t_at = |c: usize| start + c as f64 / w as f64 * span;
    let clock_at = |c: usize| (t_at(c) * SR) as u64;
    let now_col = (((ui.elapsed() - start) / span) * w as f64) as u16;
    const VBARS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let lanes: [(&str, Rgb); 5] =
        [("chords", HARMONY), ("drums", DRUMS), ("filter", DRONE), ("glitch 1", GLITCH1), ("glitch 2", GLITCH2)];
    // Each lane gets an equal share of the height.
    let lane_h = ((r.height.saturating_sub(4)) / 5).clamp(1, 5);
    for (i, (name, c)) in lanes.iter().enumerate() {
        let y = 1 + i as u16 * lane_h;
        if y + lane_h > r.height {
            break;
        }
        let fill_h = lane_h.saturating_sub(1).max(1); // a one-row gutter between lanes
        put(buf, r, 0, y + (fill_h - 1) / 2, name, fg(DIM));
        for col in 0..w {
            let clock = clock_at(col);
            let (ch, color) = match i {
                0 => {
                    let p = d.harmony.at(clock);
                    let shade = [1.0, 0.65, 0.45, 0.3][p.chord % 4];
                    (if p.morph > 0.0 { '▒' } else { '█' }, lit(*c, shade))
                }
                1 => {
                    let phrase = (clock / (PHRASE_BARS * d.tempo.drum_bar())) as usize;
                    let sec = d.sections.get(phrase).copied().unwrap_or(shrine0011::drums::Section::Out);
                    (section_glyph(sec), *c)
                }
                _ => {
                    let m = ui.mods.at(clock);
                    let v = match i {
                        2 => m.drone_open,
                        3 => m.density1,
                        _ => m.density2,
                    };
                    (VBARS[(v * 8.0).round().clamp(0.0, 8.0) as usize], lit(*c, 0.4 + 0.6 * v))
                }
            };
            let past = (col as u16) < now_col;
            let style = fg(if past { mix(color, FAINT, 0.6) } else { color });
            if i >= 2 {
                // Curves: a bar as tall as the value, across the lane's rows.
                let v = VBARS.iter().position(|x| *x == ch).unwrap_or(0) as f64 / 8.0;
                vbar(buf, r, label_w + col as u16, y + fill_h - 1, fill_h, v, style);
            } else {
                for k in 0..fill_h {
                    put_char(buf, r, label_w + col as u16, y + k, ch, style);
                }
            }
        }
    }
    // Time axis and the "now" line.
    for m in 0..=8 {
        let x = ((m as f64 * 60.0) / span * w as f64) as u16;
        let label = format!("{}m", ((start / 60.0) as u64) + m);
        put(buf, r, label_w + x, 0, &label, fg(FAINT));
    }
    let lanes_end = (1 + 5 * lane_h).min(r.height);
    for y in 1..lanes_end {
        if let Some(cell) = buf.cell_mut((r.x + label_w + now_col, r.y + y)) {
            cell.set_bg(rgb(lit(ACCENT, 0.5)));
        }
    }
    if r.height > lanes_end + 1 {
        put(buf, r, label_w, lanes_end + 1, &format!("now ▲  {} · drums by phrase: · out ▂ hats ▄ half ▆ full █ rolls", s.drum_plan.as_ref().map_or("", |p| p.section.name())), fg(DIM));
    }
}

/// Lesson 2: the chord as a piano roll, and the FM wiring.
fn chord(ui: &Ui, buf: &mut Buffer, r: Rect) {
    let d = &ui.desc;
    let pos = ui.snap.as_ref().map(|s| s.harmony).unwrap_or_default();
    let chord = &d.harmony.chords[pos.chord];
    let (lo, hi) = (chord.notes.first().copied().unwrap_or(48.0), chord.notes.last().copied().unwrap_or(84.0));
    let rows = r.height.saturating_sub(1).min(chord.notes.len() as u16 * 2);
    // Piano roll: one row per note, highest first, bar length = pitch.
    for (i, n) in chord.notes.iter().rev().enumerate() {
        let y = i as u16 * (rows / chord.notes.len() as u16).max(1);
        if y >= r.height {
            break;
        }
        let frac = (n - lo + 6.0) / (hi - lo + 6.0);
        put(buf, r, 0, y, &format!("{:<4}", note_name(*n)), fg(TEXT));
        put(buf, r, 5, y, &hbar(18, frac), fg(lit(CHORD, 0.5 + 0.5 * frac)));
    }
    // The FM wiring of this chord's first voice, with a live waveform.
    let (algo, [r2, r3, i2, i3], _) = d.recipes[pos.chord][0];
    let x0 = 28u16;
    if r.width < x0 + 30 {
        return;
    }
    let wiring = if algo == Algo::Stack {
        format!("[op3 ×{}]↺ ──▶ [op2 ×{}] ──▶ [op1 ×1] ──▶ out", fmt_ratio(r3), fmt_ratio(r2))
    } else {
        format!("[op2 ×{}] ──▶ [op1 ×1] ◀── [op3 ×{}]↺   op1 ──▶ out", fmt_ratio(r2), fmt_ratio(r3))
    };
    put(buf, r, x0, 0, &wiring, fg(CHORD).add_modifier(Modifier::BOLD));
    put(buf, r, x0, 1, &format!("index {i2:.1} (op2), {i3:.1} (op3) · feedback on op3"), fg(DIM));
    let rows = r.height.saturating_sub(3).max(1);
    let mut canvas = Braille::new(r.width - x0, rows);
    let dots = canvas.width();
    let hh = canvas.height() as f64;
    let drift = ui.t * 0.15;
    let ys: Vec<f64> = (0..dots)
        .map(|x| {
            let p = x as f64 / dots as f64 * 3.0 + drift;
            let out = match algo {
                Algo::Stack => (TAU * p + i2 * (TAU * r2 * p + i3 * (TAU * r3 * p).sin()).sin()).sin(),
                Algo::Pair => (TAU * p + i2 * (TAU * r2 * p).sin() + i3 * (TAU * r3 * p).sin()).sin(),
            };
            (1.0 - out) / 2.0 * (hh - 1.0)
        })
        .collect();
    canvas.trace(&ys);
    canvas.draw(buf, r, x0, 3, fg(CHORD));
}

/// Lesson 3: a moment of sound → the frozen spectrum.
fn freeze(ui: &Ui, buf: &mut Buffer, r: Rect) {
    if r.width < 40 || r.height < 4 {
        return;
    }
    let half = r.width / 2 - 2;
    put(buf, r, 0, 0, "a moment of the sound", fg(DIM));
    put(buf, r, half + 4, 0, "its frozen spectrum (new random phases every 171 ms)", fg(DIM));
    put(buf, r, half + 1, r.height / 2, "▶", fg(ACCENT));
    let rows = r.height - 1;
    let mut canvas = Braille::new(half, rows);
    let (dots, hh) = (canvas.width(), canvas.height() as f64);
    let frames: Vec<(f32, f32)> = ui.scope.iter().rev().take(1024).rev().copied().collect();
    if frames.len() >= dots {
        let ys: Vec<f64> = (0..dots)
            .map(|x| (1.0 - (frames[x * frames.len() / dots].0 as f64 * 1.6).clamp(-1.0, 1.0)) / 2.0 * (hh - 1.0))
            .collect();
        canvas.trace(&ys);
    }
    canvas.draw(buf, r, 0, 1, fg(OUTPUT));
    // Spectrum bars (current chord), glinting as phases re-roll.
    let d = &ui.desc;
    let pos = ui.snap.as_ref().map(|s| s.harmony).unwrap_or_default();
    let mags = &d.magnitudes[pos.chord];
    let peak = mags.iter().cloned().fold(1e-12, f64::max);
    let w = (r.width - half - 4) as usize;
    let bin_hz = d.sample_rate as f64 / d.fft_size as f64;
    let glint = (1.0 - ui.frame_age / 0.17).max(0.0);
    for c in 0..w {
        let f0 = 80.0 * (8000.0f64 / 80.0).powf(c as f64 / w as f64);
        let f1 = 80.0 * (8000.0f64 / 80.0).powf((c + 1) as f64 / w as f64);
        let b0 = ((f0 / bin_hz) as usize).min(mags.len() - 1);
        let b1 = ((f1 / bin_hz) as usize).clamp(b0 + 1, mags.len());
        let m = mags[b0..b1].iter().cloned().fold(0.0, f64::max);
        let frac = ((20.0 * (m / peak).max(1e-9).log10() + 50.0) / 50.0).clamp(0.0, 1.0);
        let lift = if (c as u64 * 7 + ui.snap.as_ref().map_or(0, |s| s.drone_frames)) % 5 == 0 { glint } else { 0.0 };
        vbar(buf, r, half + 4 + c as u16, rows, rows - 1, frac, fg(mix(DRONE, WHITE, lift)));
    }
}

/// Lesson 4: the filter's future, with chord changes marked.
fn motion(ui: &Ui, buf: &mut Buffer, r: Rect) {
    if r.width < 30 || r.height < 4 {
        return;
    }
    let d = &ui.desc;
    let span = 360.0;
    let start = ui.elapsed();
    let rows = r.height - 1;
    let mut canvas = Braille::new(r.width, rows);
    let (dots, hh) = (canvas.width(), canvas.height() as f64);
    let ys: Vec<f64> = (0..dots)
        .map(|x| {
            let clock = ((start + x as f64 / dots as f64 * span) * SR) as u64;
            (1.0 - ui.mods.at(clock).drone_open) * (hh - 1.0)
        })
        .collect();
    canvas.trace(&ys);
    canvas.draw(buf, r, 0, 1, fg(DRONE));
    // Chord changes.
    let mut last = d.harmony.at((start * SR) as u64).chord;
    for col in 0..r.width {
        let clock = ((start + col as f64 / r.width as f64 * span) * SR) as u64;
        let c = d.harmony.at(clock).chord;
        if c != last {
            put_char(buf, r, col, 0, '┊', fg(HARMONY));
            put(buf, r, col + 1, 0, &format!("chord {}", c + 1), fg(HARMONY));
            last = c;
        }
    }
    put(buf, r, 0, 0, "filter cutoff, next 6 minutes (top = open)", fg(DIM));
}

/// Lesson 5: two loops of 7 and 11 steps, as rings.
fn loops(ui: &Ui, buf: &mut Buffer, r: Rect) {
    let Some(s) = &ui.snap else { return };
    if r.width < 40 || r.height < 7 {
        return;
    }
    let rad_y = ((r.height - 2) / 2).clamp(2, 9) as f64;
    let rad_x = rad_y * 2.2;
    // Ring outlines and "clock hands" in braille, then the step letters on top.
    let mut canvas = Braille::new(r.width, r.height);
    for (k, layer) in s.layers.iter().enumerate() {
        let cx = r.width as f64 * if k == 0 { 0.25 } else { 0.62 };
        let cy = (r.height / 2) as f64;
        let (dx, dy) = (cx * 2.0, cy * 4.0 + 2.0);
        for a in 0..360 {
            let a = a as f64 / 360.0 * TAU;
            canvas.set((dx + a.cos() * rad_x * 2.0).round() as i64, (dy + a.sin() * rad_y * 4.0).round() as i64);
        }
        let n = layer.steps.len() as f64;
        let a = (layer.step as f64 + layer.step_phase) / n * TAU - TAU / 4.0;
        for k in 0..40 {
            let f = k as f64 / 40.0 * 0.8;
            canvas.set((dx + a.cos() * rad_x * 2.0 * f).round() as i64, (dy + a.sin() * rad_y * 4.0 * f).round() as i64);
        }
    }
    canvas.draw(buf, r, 0, 0, fg(lit(ACCENT, 0.35)));
    for (k, layer) in s.layers.iter().enumerate() {
        let cx = r.width as f64 * if k == 0 { 0.25 } else { 0.62 };
        let cy = (r.height / 2) as f64;
        let n = layer.steps.len();
        let color = if k == 0 { GLITCH1 } else { GLITCH2 };
        for (i, st) in layer.steps.iter().enumerate() {
            let a = i as f64 / n as f64 * TAU - TAU / 4.0;
            let x = (cx + a.cos() * rad_x).round() as u16;
            let y = (cy + a.sin() * rad_y).round() as u16;
            let label = match st {
                Some(st) => super::draw::kind_letter(st.kind),
                None => '·',
            };
            let current = i == layer.step;
            let style = if current {
                Style::new().fg(rgb((20, 20, 20))).bg(rgb(color)).add_modifier(Modifier::BOLD)
            } else if st.as_ref().is_some_and(|st| st.threshold < layer.density) {
                fg(color)
            } else {
                fg(FAINT)
            };
            put_char(buf, r, x, y, label, style);
        }
        put(
            buf,
            r,
            (cx - 3.0).max(0.0) as u16,
            (cy + rad_y + 1.0) as u16,
            &format!("{} steps", n),
            fg(lit(color, 0.8)).add_modifier(Modifier::BOLD),
        );
    }
    // When do they line up again? 7 × 1/16 and 11 × 1/8 realign every 154 sixteenths.
    let sixteenth = ui.desc.layers[0].1;
    let pos = (s.clock / sixteenth) % 154;
    let left = 154 - pos;
    put(
        buf,
        r,
        (r.width as f64 * 0.82) as u16,
        r.height / 2 - 1,
        "line up again in",
        fg(DIM),
    );
    put(
        buf,
        r,
        (r.width as f64 * 0.82) as u16,
        r.height / 2,
        &format!("{left} sixteenths"),
        fg(ACCENT).add_modifier(Modifier::BOLD),
    );
}

/// Lesson 6: echo bounces and the reverb's decay.
fn space(ui: &Ui, buf: &mut Buffer, r: Rect) {
    if r.width < 40 || r.height < 5 {
        return;
    }
    let half = r.width / 2;
    put(buf, r, 0, 0, "echo: left above, right below (recent seconds)", fg(DIM));
    let rows = r.height - 1;
    let mut canvas = Braille::new(half - 2, rows);
    let (dots, h) = (canvas.width(), canvas.height() as i64);
    let mid = h / 2;
    let hist = &ui.echo_history;
    let per = hist.len().div_ceil(dots).max(1);
    let groups = hist.len() / per;
    for g in 0..groups {
        let (l, rr) = hist.range(g * per..(g + 1) * per).fold((0.0f64, 0.0f64), |a, v| (a.0.max(v.0), a.1.max(v.1)));
        let x = (dots - groups + g) as i64;
        let lh = (level_frac(l, 42.0) * (mid - 1) as f64).round() as i64;
        let rh = (level_frac(rr, 42.0) * (h - mid - 1) as f64).round() as i64;
        if lh > 0 {
            canvas.vline(x, mid - 1, mid - lh);
        }
        if rh > 0 {
            canvas.vline(x, mid, mid + rh - 1);
        }
    }
    canvas.draw(buf, r, 0, 1, fg(ECHO));
    // Reverb decay: 60 dB over the tail.
    put(buf, r, half + 1, 0, &format!("reverb tail: −60 dB in {:.0} s", ui.desc.reverb_decay), fg(DIM));
    let mut rc = Braille::new(r.width - half - 1, rows);
    let (rd, rh) = (rc.width(), rc.height() as f64);
    let ys: Vec<f64> = (0..rd)
        .map(|x| {
            let t = x as f64 / rd as f64 * ui.desc.reverb_decay * 1.1;
            let db = -60.0 * t / ui.desc.reverb_decay;
            (-db / 66.0).clamp(0.0, 1.0) * (rh - 1.0)
        })
        .collect();
    rc.trace(&ys);
    rc.draw(buf, r, half + 1, 1, fg(REVERB));
}

/// Lesson 7: pattern → sampler → this bar's chop → what's coming.
fn brk(ui: &Ui, buf: &mut Buffer, r: Rect) {
    let d = &ui.desc;
    if !d.drums_on {
        put(buf, r, 0, 1, "No drums in this version (kit: Off).", fg(TEXT));
        return;
    }
    let Some(s) = &ui.snap else { return };
    put(buf, r, 0, 0, "1  the break as played (2 bars, 32 sixteenths):", fg(DIM));
    let cw = (r.width / 32).max(1);
    for (i, v) in d.slice_voices.iter().enumerate() {
        let (ch, c) = v.map_or(('·', FAINT), |v| (v.letter(), voice_color(v)));
        put(buf, r, i as u16 * cw, 1, &ch.to_string(), fg(c).add_modifier(Modifier::BOLD));
    }
    let v = &d.drum_voices;
    put(
        buf,
        r,
        0,
        3,
        &format!(
            "2  sampled: {}-bit, {:.0} kHz · {:.0} → {:.0} BPM ({:+.1} semitones) · kit: {}",
            v.bits, v.hold_khz, v.source_bpm, v.play_bpm, v.semitones_up, v.kit
        ),
        fg(DRUMS),
    );
    put(buf, r, 0, 5, "3  this bar plays (slice numbers · → straight ← reversed ≡ rolled ↓ pitched down ½ half-time):", fg(DIM));
    if let Some(plan) = &s.drum_plan {
        let cell = (r.width / STEPS as u16).clamp(3, 6);
        for (i, st) in plan.steps.iter().enumerate() {
            let current = i == s.drum_step;
            let style = if current { Style::new().fg(rgb((20, 20, 20))).bg(rgb(DRUMS)) } else { fg(TEXT) };
            put(buf, r, i as u16 * cell, 6, &format!("{}{:02}", op_glyph(st.op), st.slice + 1), style);
        }
        put(buf, r, 0, 8, "4  the next phrases (4 bars each):", fg(DIM));
        let phrase = (plan.bar / PHRASE_BARS) as usize;
        let mut x = 0u16;
        for (k, sec) in d.sections.iter().enumerate().skip(phrase).take((r.width / 2) as usize) {
            let st = if k == phrase { fg(DRUMS).add_modifier(Modifier::BOLD | Modifier::UNDERLINED) } else { fg(lit(DRUMS, 0.7)) };
            put(buf, r, x, 9, &section_glyph(*sec).to_string(), st);
            x += 2;
        }
    }
}

/// Lesson 8: drum level against the drone's gain.
fn room(ui: &Ui, buf: &mut Buffer, r: Rect) {
    if !ui.desc.drums_on {
        put(buf, r, 0, 1, "No drums, so the drone is never ducked.", fg(TEXT));
        return;
    }
    if r.width < 30 || r.height < 5 {
        return;
    }
    put(buf, r, 0, 0, "drums (orange, from the bottom) · drone level (blue line) · recent seconds", fg(DIM));
    let rows = r.height - 1;
    let mut bars = Braille::new(r.width, rows);
    let mut line = Braille::new(r.width, rows);
    let (dots, h) = (bars.width(), bars.height() as f64);
    let hist = &ui.duck_history;
    let per = hist.len().div_ceil(dots).max(1);
    let groups = hist.len() / per;
    let mut ys = vec![0.0; groups];
    for g in 0..groups {
        let (lvl, gain) = hist.range(g * per..(g + 1) * per).fold((0.0f64, 1.0f64), |a, v| (a.0.max(v.0), a.1.min(v.1)));
        let x = (dots - groups + g) as i64;
        let bh = (level_frac(lvl, 40.0) * (h - 1.0)).round() as i64;
        if bh > 0 {
            bars.vline(x, h as i64 - 1, h as i64 - bh);
        }
        // Gain in dB: 0 dB at the top, −8 dB at the bottom.
        let db = 20.0 * gain.max(1e-6).log10();
        ys[g] = (-db / 8.0).clamp(0.0, 1.0) * (h - 1.0);
    }
    let offset = dots - groups;
    let full: Vec<f64> = (0..offset).map(|_| 0.0).chain(ys).collect();
    line.trace(&full);
    bars.draw(buf, r, 0, 1, fg(lit(DRUMS, 0.7)));
    line.draw(buf, r, 0, 1, fg(DRONE));
    let gain = ui.snap.as_ref().map_or(1.0, |s| s.duck_gain);
    put(buf, r, r.width.saturating_sub(20), 0, &format!("drone now {:+.1} dB", 20.0 * gain.log10()), fg(DRONE));
}

// ---------------------------------------------------------------- instruments

/// "On its own" and "how it fits" for the instrument lessons, with live values.
fn instrument_text(ui: &Ui) -> (String, Vec<String>) {
    let d = &ui.desc;
    let h = &d.harmony;
    let Some(s) = &ui.snap else { return (String::new(), Vec::new()) };
    let key = key_name(h.key);
    let scale = h.scale.name();
    let duck_db = 20.0 * s.duck_gain.max(1e-6).log10();
    let root = note_name(s.bass_root);
    match solo_for(ui.lesson) {
        Some(Solo::Drone) => {
            let chord = &h.chords[s.harmony.chord];
            let notes: Vec<String> = chord.notes.iter().map(|n| note_name(*n)).collect();
            (
                format!(
                    "A frozen FM chord ({}), resynthesised with random phases every 171 ms and swept by a low-pass, right now at {:.0} Hz. On its own it is a slow, wide, motionless cloud.",
                    notes.join(" "),
                    s.drone_cutoff
                ),
                vec![
                    "It is the bed everything else sits on, and the loudest layer when the drums are out.".to_string(),
                    if d.drums_on {
                        format!("While the drums play it steps back: right now {duck_db:+.1} dB, and darker above 2.5 kHz.")
                    } else {
                        "With no drums it is never ducked: it always fills the room.".to_string()
                    },
                    format!("The bass plays this chord's root ({root}) underneath; the glitch stutters are grains cut from this very drone."),
                ],
            )
        }
        Some(Solo::Glitch1) => {
            let l = &s.layers[0];
            (
                format!(
                    "7 sixteenth-note steps at {:.0} BPM, mostly ticks, clicks and crushed noise, now on step {} of loop {} of {}. Density {:.2}: only steps whose threshold is below it sound.",
                    d.tempo.half_bpm(),
                    l.step + 1,
                    (l.loops + 1).min(l.loops_until_mutation),
                    l.loops_until_mutation,
                    l.density
                ),
                vec![
                    "It is the fast, percussive texture: the closest thing to hi-hats in the ambient sections.".to_string(),
                    "Its stutters copy grains of the drone, and a third of it goes to the ping-pong echo, so hits bounce left and right.".to_string(),
                    format!(
                        "It thins out when its slow density cycle dips, leaving space; the drums at {} BPM run exactly twice its tempo.",
                        d.tempo.bpm
                    ),
                ],
            )
        }
        Some(Solo::Glitch2) => {
            let l = &s.layers[1];
            (
                format!(
                    "11 eighth-note steps: pitched FM blips and bell pings on notes of {key} {scale}. Now on step {} of 11, density {:.2}.",
                    l.step + 1,
                    l.density
                ),
                vec![
                    "It is the melodic sparkle: tiny tuned events that hint at the harmony without playing a tune.".to_string(),
                    "Against layer 1's 7 steps it drifts, and the two only line up every 154 sixteenths (27.5 s).".to_string(),
                    "It sends the most to the echo, so its pings trail off in the stereo field.".to_string(),
                ],
            )
        }
        Some(Solo::Bass) => {
            let (ratio, index, cutoff) = s.bass;
            (
                format!(
                    "A 2-operator FM tone on {root} (ratio ×{}, index {:.2}) with a pure sine an octave below, through a low-pass at {:.0} Hz.",
                    fmt_ratio(ratio),
                    index,
                    cutoff
                ),
                vec![
                    "It holds the floor: the drone leaves everything below 110 Hz empty, and the bass fills exactly that.".to_string(),
                    "It glides to each new chord's root during the morph bar, so the low end moves with the harmony.".to_string(),
                    "It never goes to the reverb, so the low end stays clean and centred.".to_string(),
                ],
            )
        }
        Some(Solo::Drums) if !d.drums_on => (
            "This version has no drums (kit: Off), so there is nothing to solo here.".to_string(),
            vec!["Pick a kit at setup to hear the drums on their own.".to_string()],
        ),
        Some(Solo::Drums) => {
            let v = &d.drum_voices;
            let sec = s.drum_plan.as_ref().map_or("waiting", |p| p.section.name());
            (
                format!(
                    "The {} kit: a 2-bar break played at {:.0} BPM, sampled, {} to {} BPM and chopped live. Right now: {}.",
                    v.kit, v.source_bpm, speed_word(v.semitones_up), d.tempo.bpm, sec
                ),
                vec![
                    format!("While it plays, the drone ducks to make room (now {duck_db:+.1} dB)."),
                    format!("Its {} BPM is exactly twice the glitch tempo, so drums and glitches lock together.", d.tempo.bpm),
                    "Whole phrases drop out ('out' sections) so the ambience can breathe between the beats.".to_string(),
                    space_tip(d.settings.space).to_string(),
                ],
            )
        }
        _ => {
            let ms = d.echo_delay as f64 / SR * 1000.0;
            (
                format!(
                    "Only the effect returns: the ping-pong echo ({ms:.0} ms) and the {:.0}-second reverb, with the instruments themselves taken out. This is the room everything sits in.",
                    d.reverb_decay
                ),
                vec![
                    "Press s to switch to the full mix and back: the difference is how much of the piece is space.".to_string(),
                    "The drone and glitches send generously; the drums only a little; the bass not at all.".to_string(),
                    "The reverb is high-passed at 180 Hz, so even 18 seconds of tail never muddies the bass.".to_string(),
                ],
            )
        }
    }
}

/// How the drums use the stereo field, as a recreate-it tip.
fn space_tip(space: DrumSpace) -> &'static str {
    match space {
        DrumSpace::Centred => "Every hit sits dead centre (Drum space: Centred). Try Wide at setup to spread snares and hats.",
        DrumSpace::Wide => "Each hit gets its own pan, fixed in the break: the kick centred, snares 45–65% to one side, hats 50–75%.",
        DrumSpace::Tape => "Hits are panned (kick centred, snares 45–65%, hats 50–75%) and the snares feed a ping-pong tape echo, half-width: 268 ms, a slow wobble, darker and thinner each repeat.",
    }
}

/// What you hear (waveform + spectrum), the layer's share of the mix, and its mechanics.
fn instrument_view(ui: &Ui, buf: &mut Buffer, r: Rect) {
    if r.width < 50 || r.height < 6 {
        return;
    }
    let solo = solo_for(ui.lesson);
    let color = match solo {
        Some(Solo::Drone) => DRONE,
        Some(Solo::Glitch1) => GLITCH1,
        Some(Solo::Glitch2) => GLITCH2,
        Some(Solo::Bass) => BASS,
        Some(Solo::Drums) => DRUMS,
        _ => REVERB,
    };
    let top = (r.height * 3 / 5).max(3);
    let half = r.width / 2;
    let heard = if ui.wanted_solo().is_some() { "this layer alone" } else { "the full mix" };

    // What you hear: waveform…
    put(buf, r, 0, 0, &format!("waveform · {heard}"), fg(DIM));
    let mut canvas = Braille::new(half - 1, top - 1);
    let (dots, hh) = (canvas.width(), canvas.height() as f64);
    let frames: Vec<(f32, f32)> = ui.scope.iter().rev().take(2048).rev().copied().collect();
    if frames.len() >= dots {
        let peak = frames.iter().fold(0.05f32, |a, f| a.max(f.0.abs()).max(f.1.abs())) as f64;
        let ys: Vec<f64> = (0..dots)
            .map(|x| {
                let f = frames[x * frames.len() / dots];
                let v = 0.5 * (f.0 as f64 + f.1 as f64) / peak;
                (1.0 - v.clamp(-1.0, 1.0)) / 2.0 * (hh - 1.0)
            })
            .collect();
        canvas.trace(&ys);
    }
    canvas.draw(buf, r, 0, 1, fg(color));

    // …and spectrum (log frequency, 40 Hz – 16 kHz).
    put(buf, r, half + 1, 0, &format!("spectrum · {heard}"), fg(DIM));
    let spec = &ui.heard_spectrum;
    let w = (r.width - half - 1) as usize;
    if !spec.is_empty() {
        let bin_hz = SR / (spec.len() * 2) as f64;
        let peak = spec.iter().cloned().fold(1e-9, f64::max);
        for c in 0..w {
            let f0 = 40.0 * (16_000.0f64 / 40.0).powf(c as f64 / w as f64);
            let f1 = 40.0 * (16_000.0f64 / 40.0).powf((c + 1) as f64 / w as f64);
            let b0 = ((f0 / bin_hz) as usize).min(spec.len() - 1);
            let b1 = ((f1 / bin_hz) as usize).clamp(b0 + 1, spec.len());
            let m = spec[b0..b1].iter().cloned().fold(0.0, f64::max);
            let frac = ((20.0 * (m / peak).max(1e-9).log10() + 60.0) / 60.0).clamp(0.0, 1.0);
            vbar(buf, r, half + 1 + c as u16, top - 1, top - 1, frac, fg(lit(color, 0.4 + 0.6 * frac)));
        }
        for (hz, text) in [(100.0, "100"), (1000.0, "1k"), (10_000.0, "10k")] {
            let x = ((hz / 40.0f64).ln() / (16_000.0f64 / 40.0).ln() * w as f64) as u16;
            put(buf, r, half + 1 + x, top, text, fg(FAINT));
        }
    }

    // Its share of the mix: every layer's level, this one highlighted.
    let l = &ui.levels;
    let layers: [(&str, f64, Option<Solo>, Rgb); 6] = [
        ("drone", l.drone, Some(Solo::Drone), DRONE),
        ("glitch 1", l.glitch1, Some(Solo::Glitch1), GLITCH1),
        ("glitch 2", l.glitch2, Some(Solo::Glitch2), GLITCH2),
        ("bass", l.bass, Some(Solo::Bass), BASS),
        ("drums", l.drums, Some(Solo::Drums), DRUMS),
        ("echo+reverb", l.reverb.max(l.echo), Some(Solo::Space), REVERB),
    ];
    let y0 = top + 1;
    put(buf, r, 0, y0, "share of the mix (level)", fg(DIM));
    let bar_w = (half as usize).saturating_sub(14);
    for (i, (name, lvl, s, c)) in layers.iter().enumerate() {
        let y = y0 + 1 + i as u16;
        if y >= r.height {
            break;
        }
        let me = *s == solo;
        let st = if me { fg(*c).add_modifier(Modifier::BOLD) } else { fg(mix(DIM, *c, 0.3)) };
        put(buf, r, 0, y, &format!("{}{:<11}", if me { "▶" } else { " " }, name), st);
        put(buf, r, 12, y, &hbar(bar_w, level_frac(*lvl, 48.0)), fg(if me { *c } else { lit(*c, 0.35) }));
    }

    // The layer's own mechanics, live.
    let Some(s) = &ui.snap else { return };
    let x0 = half + 1;
    let mech = |buf: &mut Buffer, y: u16, text: &str, st: Style| put(buf, r, x0, y0 + y, text, st);
    match solo {
        Some(Solo::Drone) => {
            mech(buf, 0, "the drone right now", fg(DIM));
            mech(buf, 1, &format!("chord {} of {}", s.harmony.chord + 1, ui.desc.harmony.chords.len()), fg(TEXT));
            mech(buf, 2, &format!("low-pass {:.0} Hz · Q {:.1}", s.drone_cutoff, s.drone_q), fg(TEXT));
            mech(buf, 3, &format!("ducked {:+.1} dB for drums", 20.0 * s.duck_gain.max(1e-6).log10()), fg(TEXT));
            mech(buf, 4, &format!("frame #{} (new phases every 171 ms)", s.drone_frames), fg(TEXT));
        }
        Some(Solo::Glitch1) | Some(Solo::Glitch2) => {
            let i = if solo == Some(Solo::Glitch1) { 0 } else { 1 };
            let layer = &s.layers[i];
            mech(buf, 0, &format!("{} steps, now on step {}", layer.steps.len(), layer.step + 1), fg(DIM));
            let mut x = 0u16;
            for (j, st) in layer.steps.iter().enumerate() {
                let (ch, live) = match st {
                    Some(st) => (super::draw::kind_letter(st.kind), st.threshold < layer.density),
                    None => ('·', false),
                };
                let style = if j == layer.step {
                    Style::new().fg(rgb((20, 20, 20))).bg(rgb(color))
                } else if live {
                    fg(color)
                } else {
                    fg(FAINT)
                };
                put(buf, r, x0 + x, y0 + 1, &format!(" {ch} "), style);
                x += 3;
            }
            mech(buf, 2, &format!("density {:.2} · hits so far {}", layer.density, layer.triggers), fg(TEXT));
            mech(buf, 3, "B blip T tick S stutter P ping C click X crush", fg(DIM));
        }
        Some(Solo::Bass) => {
            let (ratio, index, cutoff) = s.bass;
            mech(buf, 0, "the bass right now", fg(DIM));
            mech(buf, 1, &format!("root {} ({:.1} Hz)", note_name(s.bass_root), 440.0 * 2f64.powf((s.bass_root - 69.0) / 12.0)), fg(TEXT));
            mech(buf, 2, &format!("FM ratio ×{} · index {index:.2}", fmt_ratio(ratio)), fg(TEXT));
            mech(buf, 3, &format!("low-pass {cutoff:.0} Hz · sine sub an octave below"), fg(TEXT));
        }
        Some(Solo::Drums) if ui.desc.drums_on => {
            mech(buf, 0, &format!("{} kit · this bar:", ui.desc.drum_voices.kit), fg(DIM));
            if let Some(plan) = &s.drum_plan {
                let mut x = 0u16;
                for (i, st) in plan.steps.iter().enumerate().take(((r.width - x0) / 4) as usize) {
                    let style = if i == s.drum_step { Style::new().fg(rgb((20, 20, 20))).bg(rgb(DRUMS)) } else { fg(TEXT) };
                    put(buf, r, x0 + x, y0 + 1, &format!("{}{:02}", op_glyph(st.op), st.slice + 1), style);
                    x += 4;
                }
                mech(buf, 2, &format!("section: {}{}", plan.section.name(), if plan.fill { " · fill" } else { "" }), fg(TEXT));
            }
            let hits: String = s.drum_hits.iter().map(|v| v.letter()).collect();
            mech(buf, 3, &format!("last hit: {}", if hits.is_empty() { "-".to_string() } else { hits }), fg(TEXT));
        }
        Some(Solo::Drums) => mech(buf, 0, "no drums in this version (kit: Off)", fg(DIM)),
        _ => {
            mech(buf, 0, "the room", fg(DIM));
            mech(buf, 1, &format!("echo {:.0} ms · feedback {:.0}%", ui.desc.echo_delay as f64 / SR * 1000.0, ui.desc.echo_feedback * 100.0), fg(TEXT));
            mech(buf, 2, &format!("reverb: 8 lines, tail {:.0} s", ui.desc.reverb_decay), fg(TEXT));
            let lines: String = ui.reverb_levels.iter().map(|l| ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'][(level_frac(*l, 40.0) * 7.0) as usize]).collect();
            put(buf, r, x0, y0 + 3, &format!("line levels {lines}"), fg(REVERB));
        }
    }
}
