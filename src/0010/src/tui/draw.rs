//! Panel drawing. Every panel is both a live view of its part of the engine
//! and a picture of the algorithm itself.

use super::gfx::*;
use super::{Panel, Ui};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType};
use ratatui::Frame;
use shrine0010::cli::format_length;
use shrine0010::drone::Algo;
use shrine0010::glitch::Kind;
use shrine0010::pattern::Mutation;
use shrine0010::telemetry::Snapshot;
use std::f64::consts::TAU;

const CYCLES: Rgb = (228, 214, 150);

pub fn note_name(midi: f64) -> String {
    const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    let n = midi.round() as i64;
    format!("{}{}", NAMES[n.rem_euclid(12) as usize], n.div_euclid(12) - 1)
}

fn midi_hz(midi: f64) -> f64 {
    440.0 * 2f64.powf((midi - 69.0) / 12.0)
}

pub fn fmt_ratio(r: f64) -> String {
    let s = format!("{r:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub fn fmt_hz(hz: f64) -> String {
    if hz >= 1000.0 {
        format!("{:.1} kHz", hz / 1000.0)
    } else {
        format!("{hz:.0} Hz")
    }
}

pub fn kind_letter(k: Kind) -> char {
    match k {
        Kind::Blip => 'B',
        Kind::Tick => 'T',
        Kind::Stutter => 'S',
        Kind::Ping => 'P',
        Kind::Click => 'C',
        Kind::Crush => 'X',
    }
}

pub fn kind_name(k: Kind) -> &'static str {
    match k {
        Kind::Blip => "blip",
        Kind::Tick => "tick",
        Kind::Stutter => "stutter",
        Kind::Ping => "ping",
        Kind::Click => "click",
        Kind::Crush => "crush",
    }
}

pub fn kind_color(k: Kind) -> Rgb {
    match k {
        Kind::Blip => (250, 222, 95),
        Kind::Tick => (215, 213, 208),
        Kind::Stutter => (95, 212, 232),
        Kind::Ping => (255, 145, 185),
        Kind::Click => (255, 95, 85),
        Kind::Crush => (255, 162, 72),
    }
}

/// Cheap integer hash for visual sparkle (not used for sound).
fn hash(a: u64, b: u64) -> u64 {
    let mut z = a.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ b.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z ^= z >> 29;
    z.wrapping_mul(0x94D0_49BB_1331_11EB) >> 32
}

// ---------------------------------------------------------------- chrome

pub fn too_small(buf: &mut Buffer, r: Rect) {
    let msg = format!("Enlarge the terminal to at least 80 x 22 (now {} x {}).", r.width, r.height + 1);
    let x = r.width.saturating_sub(msg.chars().count() as u16) / 2;
    put(buf, r, x, r.height / 2, &msg, fg(TEXT));
    put(buf, r, x, r.height / 2 + 1, "The music plays on. [space] pause [q] quit", fg(DIM));
}

/// Shortens `s` to at most `max` characters, keeping its end ("…name.wav").
fn shorten(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        s.to_string()
    } else {
        std::iter::once('…').chain(s.chars().skip(n - max + 1)).collect()
    }
}

pub fn header(ui: &Ui, buf: &mut Buffer, r: Rect) {
    // The key hints sit at the right edge and are drawn last, so nothing
    // (a long file or device name) can push them off-screen.
    let tab = |name: &str, on: bool| {
        if on {
            Span::styled(format!(" {name} "), Style::new().fg(rgb((20, 20, 20))).bg(rgb(TEXT)))
        } else {
            Span::styled(format!(" {name} "), fg(DIM))
        }
    };
    let keys = Line::from(vec![
        Span::styled(" [tab]", fg(DIM)),
        tab("1 pipeline", ui.view == super::View::Pipeline),
        tab("2 engine", ui.view == super::View::Engine),
        tab("3 rhythm", ui.view == super::View::Rhythm),
        tab("4 learn", ui.view == super::View::Learn),
        Span::styled(" [space] pause [q] quit ", fg(DIM)),
    ]);
    let keys_w = (keys.width() as u16).min(r.width);

    let seeds = ui.desc.seeds;
    let (state, color) = if ui.quitting {
        ("■ fading out", GLITCH2)
    } else if ui.paused {
        ("❚❚ paused", GLITCH2)
    } else {
        ("▶ playing", BASS)
    };
    // Most important first: whatever doesn't fit is clipped from the right.
    let mut spans = vec![
        Span::styled(format!(" {} ", ui.label), Style::new().fg(rgb((20, 20, 20))).bg(rgb(OUTPUT)).add_modifier(Modifier::BOLD)),
        Span::styled(format!(" {state} {} ", format_length(ui.elapsed())), fg(color)),
    ];
    if let Some((path, secs)) = &ui.recording {
        // Just the file's name, shortened if long; the full path is printed when you leave.
        let name = std::path::Path::new(path).file_name().map_or(path.clone(), |n| n.to_string_lossy().into_owned());
        spans.push(Span::styled(
            format!(" ● REC {} {}/{} ", shorten(&name, 24), format_length(ui.elapsed().min(*secs)), format_length(*secs)),
            fg((255, 95, 85)),
        ));
    }
    if ui.underruns > 0 {
        spans.push(Span::styled(format!(" underruns {} ", ui.underruns), fg((255, 95, 85))));
    }
    spans.push(Span::styled(format!(" {} ", shrine0010::cli::recipe(seeds, ui.desc.settings)), fg(TEXT)));
    spans.push(Span::styled(format!(" {} ", super::setup::VERSION), fg(DIM)));
    let rate = if ui.device_rate == 48_000 { String::new() } else { format!(" (resampled from {} Hz)", ui.device_rate) };
    spans.push(Span::styled(format!(" {}{rate} ", shorten(&ui.device, 28)), fg(DIM)));

    let left_w = r.width.saturating_sub(keys_w + 1);
    buf.set_line(r.x, r.y, &Line::from(spans), left_w);
    buf.set_line(r.x + r.width - keys_w, r.y, &keys, keys_w);
}

struct Meta {
    title: &'static str,
    color: Rgb,
}

fn meta(p: Panel) -> Meta {
    let (title, color) = match p {
        Panel::Spectrum => ("1 · FFT FREEZE · the frozen spectrum, swept by a filter", DRONE),
        Panel::Ola => ("2 · OVERLAP-ADD", DRONE),
        Panel::Chord => ("3 · FM CHORD · seed 1", CHORD),
        Panel::Glitch => ("4 · GLITCH LAYERS · cyclic repeats · seeds 2 + 3", GLITCH1),
        Panel::Cycles => ("5 · SLOW CYCLES", CYCLES),
        Panel::Bass => ("6 · FM BASS + SUB · seed 4", BASS),
        Panel::Echo => ("7 · PING-PONG ECHO", ECHO),
        Panel::Reverb => ("8 · REVERB · 8-line FDN", REVERB),
        Panel::Output => ("9 · OUTPUT", OUTPUT),
        Panel::Flow => ("SIGNAL FLOW", OUTPUT),
        Panel::Harmony => ("HARMONY · scale, key and progression · seed 1", HARMONY),
        Panel::Break => ("BREAK CHOPPER · the sampled break, re-sequenced · seed 5", DRUMS),
        Panel::Drums => ("DRUM VOICES · how each drum is synthesised", DRUMS),
        Panel::General => ("", TEXT),
    };
    Meta { title, color }
}

fn caption(ui: &Ui, p: Panel) -> String {
    let d = &ui.desc;
    let sr = d.sample_rate as f64;
    match p {
        Panel::Spectrum => format!(
            "{}-pt FFT · random phases every {:.0} ms",
            d.fft_size,
            d.hop as f64 / sr * 1000.0
        ),
        Panel::Ola => "75% overlap · Σw² always 1.5".into(),
        Panel::Chord => "rendered once, then frozen".into(),
        Panel::Glitch => "B blip T tick S stutter P ping C click X crush · dim = muted".into(),
        Panel::Cycles => "repeats every ≈9.4 million yrs".into(),
        Panel::Bass => "2-op FM + sine sub → low-pass".into(),
        Panel::Echo => format!(
            "bounce {:.0} ms · feedback {:.0}%",
            d.echo_delay as f64 / sr * 1000.0,
            d.echo_feedback * 100.0
        ),
        Panel::Reverb => format!("Hadamard mix · T60 {:.0} s", d.reverb_decay),
        Panel::Output => "scope · stereo field · soft clip".into(),
        Panel::Harmony => "lit keys = this chord · dim keys = the scale".into(),
        Panel::Break => "→ play  ← reverse  ≡ roll  ↓ pitch down  ½ half-time".into(),
        Panel::Drums => "flashes show each voice as it is hit".into(),
        _ => String::new(),
    }
}

/// Draws a panel's frame and returns its inner area.
pub(super) fn frame(ui: &Ui, f: &mut Frame, p: Panel, area: Rect, explaining: bool) -> Rect {
    let m = meta(p);
    let border = if explaining { fg(m.color) } else { fg(mix(FAINT, m.color, 0.25)) };
    let title_style = if explaining { fg(m.color).add_modifier(Modifier::BOLD) } else { fg(mix(DIM, m.color, 0.6)) };
    let marker = if explaining { " ◆ explaining" } else { "" };
    let block = Block::bordered()
        .border_type(if explaining { BorderType::Thick } else { BorderType::Rounded })
        .border_style(border)
        .title(Line::from(vec![Span::styled(format!(" {} ", m.title), title_style), Span::styled(marker, fg(GLITCH2))]))
        .title_bottom(Line::from(Span::styled(format!(" {} ", caption(ui, p)), fg(DIM))));
    let inner = block.inner(area);
    f.render_widget(block, area);
    inner
}

pub fn panel(ui: &Ui, f: &mut Frame, p: Panel, area: Rect, explaining: bool) {
    let r = frame(ui, f, p, area, explaining);
    let Some(s) = &ui.snap else {
        put(f.buffer_mut(), r, 1, 0, "starting…", fg(DIM));
        return;
    };
    let buf = f.buffer_mut();
    match p {
        Panel::Spectrum => spectrum(ui, s, buf, r),
        Panel::Ola => ola(ui, s, buf, r),
        Panel::Chord => chord(ui, buf, r),
        Panel::Glitch => glitch(ui, s, buf, r),
        Panel::Cycles => cycles(s, buf, r),
        Panel::Bass => bass(ui, s, buf, r),
        Panel::Echo => echo(ui, buf, r),
        Panel::Reverb => reverb(ui, buf, r),
        Panel::Output => output(ui, buf, r),
        _ => {}
    }
}

// ---------------------------------------------------------------- 1 spectrum

fn spectrum(ui: &Ui, s: &Snapshot, buf: &mut Buffer, r: Rect) {
    if r.height < 4 || r.width < 16 {
        return;
    }
    let d = &ui.desc;
    let w = r.width as usize;
    let bars = r.height - 2;
    let (fmin, fmax) = (60.0f64, 16_000.0f64);
    let bin_hz = d.sample_rate as f64 / d.fft_size as f64;
    // The current chord's frozen spectrum, blended into the next during a morph.
    let pos = s.harmony;
    let (ma, mb) = (&d.magnitudes[pos.chord], &d.magnitudes[pos.next]);
    let x = pos.morph;
    let mags: Vec<f64> = if x == 0.0 {
        ma.clone()
    } else {
        ma.iter().zip(mb).map(|(a, b)| ((1.0 - x) * a * a + x * b * b).sqrt()).collect()
    };
    let peak = mags.iter().cloned().fold(1e-12, f64::max);
    let col_freq = |c: f64| fmin * (fmax / fmin).powf(c / w as f64);
    let cutoff = s.drone_cutoff.max(1.0);
    let q = s.drone_q.max(0.1);
    let sparkle = (1.0 - ui.frame_age / 0.17).max(0.0);
    let level = level_frac(ui.levels.drone, 40.0);

    for c in 0..w {
        let (f0, f1) = (col_freq(c as f64), col_freq(c as f64 + 1.0));
        let b0 = ((f0 / bin_hz) as usize).min(mags.len() - 1);
        let b1 = ((f1 / bin_hz) as usize).clamp(b0 + 1, mags.len());
        let m = mags[b0..b1].iter().cloned().fold(0.0, f64::max);
        let frac = ((20.0 * (m / peak).max(1e-9).log10() + 60.0) / 60.0).clamp(0.0, 1.0);
        // The drone filter's response at this column: what the sweep lets through.
        let x = (f0 * f1).sqrt() / cutoff;
        let h = 1.0 / ((1.0 - x * x).powi(2) + (x / q).powi(2)).sqrt();
        let mut color = mix(lit(DRONE, 0.22), DRONE, h.min(1.0).sqrt() * (0.6 + 0.4 * level));
        if h > 1.05 {
            color = mix(color, WHITE, ((h - 1.0) / 2.5).min(0.6));
        }
        vbar(buf, r, c as u16, bars, bars, frac, fg(color));
        // Each new frame re-rolls every bin's phase: a brief glint on the bar tops.
        if sparkle > 0.0 && frac > 0.1 && hash(c as u64, s.drone_frames) % 3 == 0 {
            let cells = ((frac * bars as f64).ceil() as u16).clamp(1, bars);
            if let Some(cell) = buf.cell_mut((r.x + c as u16, r.y + bars + 1 - cells)) {
                cell.set_fg(rgb(mix(color, WHITE, sparkle)));
            }
        }
    }

    let cx = (((cutoff / fmin).ln() / (fmax / fmin).ln()) * w as f64).clamp(0.0, w as f64 - 1.0) as u16;
    put_char(buf, r, cx, 0, '▼', fg(WHITE));
    let label = format!(" low-pass {} · Q {:.1} ", fmt_hz(cutoff), q);
    let lw = label.chars().count() as u16;
    let lx = if cx + 1 + lw <= r.width { cx + 1 } else { cx.saturating_sub(lw) };
    put(buf, r, lx, 0, &label, fg(DRONE));

    let axis = r.height - 1;
    for (hz, text) in [(100.0, "100"), (1000.0, "1k"), (10_000.0, "10k")] {
        let x = (((hz / fmin) as f64).ln() / (fmax / fmin).ln() * w as f64) as u16;
        put(buf, r, x, axis, &format!("┴{text}"), fg(DIM));
    }
    put(buf, r, 0, axis, "Hz", fg(DIM));
}

// ---------------------------------------------------------------- 2 overlap-add

fn ola(ui: &Ui, s: &Snapshot, buf: &mut Buffer, r: Rect) {
    if r.height < 3 || r.width < 12 {
        return;
    }
    let d = &ui.desc;
    let (hop, n) = (d.hop as f64, d.fft_size as f64);
    // Frame-number labels only when there is room for them.
    let label_w = if r.width >= 26 { 7u16 } else { 0 };
    let w = (r.width - label_w) as usize;
    // Time axis: 5 hops of the past to 3 hops of the future, "now" in between.
    let col_t = |c: usize| -5.0 * hop + (c as f64 + 0.5) * 8.0 * hop / w as f64;
    let now_col = (w as f64 * 5.0 / 8.0) as u16;
    let hann = |u: f64| 0.5 - 0.5 * (TAU * u).cos();
    let offset = s.drone_offset as f64;
    let frames = s.drone_frames as i64;
    let frame_rows = 5.min(r.height) as i64;
    const SHADES: [char; 5] = [' ', '░', '▒', '▓', '█'];

    let mut now_w = Vec::new();
    for k in (frames - 3)..=(frames + 1) {
        if k < 0 {
            continue;
        }
        let age = frames - k; // 3 (oldest) .. 0 (newest), -1 = being built
        let start = -(age as f64) * hop - offset;
        let row = (k % 5) as i64;
        if row >= frame_rows {
            continue;
        }
        let row = row as u16;
        let (label, color) = if age < 0 {
            ("build ".to_string(), DIM)
        } else {
            (format!("#{:<5}", k % 100_000), mix(lit(DRONE, 0.45), DRONE, 1.0 - age as f64 / 4.0))
        };
        if label_w > 0 {
            put(buf, r, 0, row, &label, fg(color));
        }
        for c in 0..w {
            let u = (col_t(c) - start) / n;
            if (0.0..1.0).contains(&u) {
                let wv = hann(u);
                let ch = SHADES[(wv * 4.999) as usize];
                put_char(buf, r, label_w + c as u16, row, ch, fg(color));
            }
        }
        if age >= 0 {
            now_w.push(hann((age as f64 * hop + offset) / n));
        }
    }
    for row in 0..frame_rows as u16 {
        let x = label_w + now_col;
        if let Some(cell) = buf.cell_mut((r.x + x, r.y + row)) {
            if cell.symbol() == " " {
                cell.set_char('│').set_fg(rgb(DIM));
            }
        }
    }

    if r.height > 5 {
        // The four window values under the playhead: their squares always sum to 1.5.
        let sum: f64 = now_w.iter().map(|w| w * w).sum();
        let ws: Vec<String> =
            now_w.iter().rev().map(|w| format!("{w:.2}").trim_start_matches('0').to_string()).collect();
        put(buf, r, 0, 5, &format!("now Σw²={sum:.2} ({})", ws.join(" ")), fg(TEXT));
    }
    if r.height > 7 && !ui.frame_overview.is_empty() {
        let rows = (r.height - 6).min(3);
        if label_w > 0 {
            put(buf, r, 0, 6, "grain", fg(DIM));
        }
        let mut canvas = Braille::new(w as u16, rows);
        let (cw, ch) = (canvas.width(), canvas.height() as f64);
        let peak = ui.frame_overview.iter().cloned().fold(1e-9, f32::max) as f64;
        for x in 0..cw {
            let v = ui.frame_overview[x * ui.frame_overview.len() / cw] as f64 / peak;
            let half = v * (ch - 1.0) / 2.0;
            let mid = (ch - 1.0) / 2.0;
            canvas.vline(x as i64, (mid - half).round() as i64, (mid + half).round() as i64);
        }
        canvas.draw(buf, r, label_w, 6, fg(lit(DRONE, 0.7)));
    }
}

// ---------------------------------------------------------------- 3 chord

/// Greedy word wrap to `width` columns.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = vec![String::new()];
    for word in text.split(' ') {
        let cur = lines.last_mut().unwrap();
        if !cur.is_empty() && cur.chars().count() + 1 + word.chars().count() > width {
            lines.push(word.to_string());
        } else {
            if !cur.is_empty() {
                cur.push(' ');
            }
            cur.push_str(word);
        }
    }
    lines
}

fn chord(ui: &Ui, buf: &mut Buffer, r: Rect) {
    let d = &ui.desc;
    let pos = ui.snap.as_ref().map(|s| s.harmony).unwrap_or_default();
    let h = &d.harmony;
    let chord = &h.chords[pos.chord];
    let head = format!(
        "chord {}/{} · degree {} · bar {}/{}{}",
        pos.chord + 1,
        h.chords.len(),
        chord.degree + 1,
        pos.bar + 1,
        chord.bars,
        if pos.morph > 0.0 { " · morphing" } else { "" }
    );
    put(buf, r, 0, 0, &head, fg(CHORD).add_modifier(Modifier::BOLD));
    let r = Rect::new(r.x, r.y + 1, r.width, r.height.saturating_sub(1));
    let notes = &chord.notes;
    // Spare rows explain why these waves are never heard directly.
    let spare = r.height.saturating_sub(notes.len() as u16 + 1);
    if spare > 0 {
        let note = "Never heard directly: only their frozen spectrum (panel 1).";
        let lines = wrap(note, r.width as usize);
        for (i, line) in lines.iter().take(spare as usize).enumerate() {
            put(buf, r, 0, notes.len() as u16 + 1 + i as u16, line, fg(DIM));
        }
    }
    let show_ratios = r.width >= 44;
    let level = level_frac(ui.levels.drone, 40.0);
    for (i, (note, (algo, [r2, r3, i2, i3], _fb))) in notes.iter().zip(&d.recipes[pos.chord]).enumerate() {
        let y = i as u16;
        if y >= r.height {
            break;
        }
        let glyph = match algo {
            Algo::Stack => "3→2→1",
            Algo::Pair => "2→1←3",
        };
        let mut text = format!("{:<3}{:>5.0}Hz {glyph} ", note_name(*note), midi_hz(*note));
        if show_ratios {
            text += &format!("×{:<5} ×{:<5} ", fmt_ratio(*r2), fmt_ratio(*r3));
        }
        let tw = text.chars().count() as u16;
        put(buf, r, 0, y, &text, fg(mix(DIM, CHORD, 0.75)));
        if r.width > tw + 4 {
            let cols = r.width - tw;
            let mut canvas = Braille::new(cols, 1);
            let dots = canvas.width();
            let drift = ui.t * 0.2 * (1.0 + i as f64 * 0.13);
            let ys: Vec<f64> = (0..dots)
                .map(|x| {
                    let p = x as f64 / dots as f64 * 2.0 + drift;
                    let out = match algo {
                        Algo::Stack => (TAU * p + i2 * (TAU * r2 * p + i3 * (TAU * r3 * p).sin()).sin()).sin(),
                        Algo::Pair => (TAU * p + i2 * (TAU * r2 * p).sin() + i3 * (TAU * r3 * p).sin()).sin(),
                    };
                    (1.0 - out) / 2.0 * 3.0
                })
                .collect();
            canvas.trace(&ys);
            canvas.draw(buf, r, tw, y, fg(mix(lit(CHORD, 0.5), CHORD, level)));
        }
    }
}

// ---------------------------------------------------------------- 4 glitch

fn glitch(ui: &Ui, s: &Snapshot, buf: &mut Buffer, r: Rect) {
    const COLORS: [Rgb; 2] = [GLITCH1, GLITCH2];
    const UNITS: [&str; 2] = ["1/16", "1/8"];
    let half = r.height / 2;
    let sr = ui.desc.sample_rate as f64;
    for (i, layer) in s.layers.iter().enumerate() {
        let y0 = i as u16 * half;
        if half == 0 {
            break;
        }
        let c = COLORS[i];
        let dens = layer.density;
        let loop_no = (layer.loops + 1).min(layer.loops_until_mutation);
        let head = if r.width >= 58 {
            format!(
                "LAYER {} · {} steps × {} · loop {} of {} · density ",
                i + 1,
                layer.steps.len(),
                UNITS[i],
                loop_no,
                layer.loops_until_mutation
            )
        } else {
            format!("L{} {}×{} loop {}/{} ", i + 1, layer.steps.len(), UNITS[i], loop_no, layer.loops_until_mutation)
        };
        put(buf, r, 0, y0, &head, fg(c).add_modifier(Modifier::BOLD));
        let hw = head.chars().count() as u16;
        put(buf, r, hw, y0, &format!("{} {dens:.2}", hbar(6, dens)), fg(c));

        if half > 1 {
            let n = layer.steps.len() as u16;
            let cw = (r.width / n).clamp(3, 5);
            for (j, step) in layer.steps.iter().enumerate() {
                let x = j as u16 * cw;
                let flash = ui.flashes[i].get(j).copied().unwrap_or(0.0);
                let current = j == layer.step;
                let (label, mut color) = match step {
                    Some(st) => {
                        let mut l = kind_letter(st.kind).to_string();
                        if st.ratchet > 1 {
                            l += &st.ratchet.to_string();
                        }
                        let muted = st.threshold >= dens;
                        (l, if muted { mix(FAINT, DIM, 0.6) } else { kind_color(st.kind) })
                    }
                    None => ("·".to_string(), FAINT),
                };
                let mut style = Style::new();
                if current {
                    style = style.bg(rgb(mix(lit(c, 0.3), c, flash)));
                    color = mix(color, WHITE, 0.5);
                } else if flash > 0.05 {
                    style = style.bg(rgb(mix(TICKER_BG, c, flash * 0.6)));
                }
                let cell = format!(" {:<w$}", label, w = (cw - 1) as usize);
                put(buf, r, x, y0 + 1, &cell, style.fg(rgb(color)).add_modifier(Modifier::BOLD));
            }
        }
        if half > 2 {
            put(buf, r, 0, y0 + 2, "sounding ", fg(DIM));
            let mut x = 9;
            for v in &layer.voices {
                let seg = format!("{}{} ", kind_letter(v.kind), hbar(3, 1.0 - v.progress));
                put(buf, r, x, y0 + 2, &seg, fg(kind_color(v.kind)));
                x += seg.chars().count() as u16;
            }
        }
        if half > 3 {
            let text = match layer.last_mutation {
                Some((clock, step, what)) => {
                    let ago = format_length(s.clock.saturating_sub(clock) as f64 / sr);
                    let change = match what {
                        Mutation::Replace(k) => format!("step {} became a new {}", step + 1, kind_name(k)),
                        Mutation::Clear => format!("step {} was cleared", step + 1),
                        Mutation::Rotate => "the loop rotated one step".to_string(),
                        Mutation::Nudge => format!("step {} was re-pitched", step + 1),
                    };
                    format!("last mutation {ago} ago: {change}")
                }
                None => "no mutation yet: the first loops repeat unchanged".to_string(),
            };
            put(buf, r, 0, y0 + 3, &text, fg(DIM));
        }
    }
}

// ---------------------------------------------------------------- 5 slow cycles

fn cycles(s: &Snapshot, buf: &mut Buffer, r: Rect) {
    let mut lfos = s.lfos;
    lfos.sort_by_key(|l| l.0);
    let track = r.width.saturating_sub(11) as usize;
    for (i, (period, v)) in lfos.iter().enumerate() {
        let y = i as u16;
        if y >= r.height || track < 4 {
            break;
        }
        put(buf, r, 0, y, &format!("{period:>3}s "), fg(DIM));
        put(buf, r, 5, y, &"─".repeat(track), fg(FAINT));
        let pos = ((v + 1.0) / 2.0 * (track - 1) as f64).round() as u16;
        put_char(buf, r, 5 + pos, y, '●', fg(mix(DIM, CYCLES, 0.5 + 0.5 * v.abs())));
        put(buf, r, 5 + track as u16, y, &format!(" {v:+.2}"), fg(DIM));
    }
    let m = &s.mods;
    let gauges = [
        ("filter open", m.drone_open, DRONE),
        ("filter reso", m.drone_reso, DRONE),
        ("glitch 1", m.density1, GLITCH1),
        ("glitch 2", m.density2, GLITCH2),
        ("bass bright", m.bass_bright, BASS),
        ("bass swell", m.bass_swell, BASS),
    ];
    let bar = r.width.saturating_sub(17) as usize;
    for (j, (name, v, c)) in gauges.iter().enumerate() {
        let y = 8 + j as u16;
        if y >= r.height || bar < 3 {
            break;
        }
        put(buf, r, 0, y, &format!("{name:<12}"), fg(DIM));
        put(buf, r, 12, y, &hbar(bar, *v), fg(*c));
        put(buf, r, 12 + bar as u16, y, &format!(" {v:.2}"), fg(DIM));
    }
}

// ---------------------------------------------------------------- 6 bass

fn bass(ui: &Ui, s: &Snapshot, buf: &mut Buffer, r: Rect) {
    let (ratio, index, cutoff) = s.bass;
    let root = ui.desc.bass_root;
    put(
        buf,
        r,
        0,
        0,
        &format!("FM  {} {:.1} Hz ×{} index {index:.2}", note_name(root), midi_hz(root), fmt_ratio(ratio)),
        fg(BASS),
    );
    put(
        buf,
        r,
        0,
        1,
        &format!("sub {} {:.1} Hz · low-pass {cutoff:.0} Hz", note_name(root - 12.0), midi_hz(root - 12.0)),
        fg(mix(DIM, BASS, 0.6)),
    );
    if r.height < 5 {
        return;
    }
    let rows = r.height - 3;
    let level = level_frac(ui.levels.bass, 40.0);
    let drift = ui.t * 0.05;
    let mut sub = Braille::new(r.width, rows);
    let mut fm = Braille::new(r.width, rows);
    let (dots, h) = (fm.width(), fm.height() as f64);
    let to_y = |v: f64| (1.0 - v) / 2.0 * (h - 1.0);
    let sub_ys: Vec<f64> = (0..dots).map(|x| to_y(0.9 * (TAU * (x as f64 / dots as f64 + drift)).sin())).collect();
    let fm_ys: Vec<f64> = (0..dots)
        .map(|x| {
            let p = x as f64 / dots as f64 * 2.0 + 2.0 * drift;
            to_y(0.6 * (TAU * p + index * (TAU * ratio * p).sin()).sin())
        })
        .collect();
    sub.trace(&sub_ys);
    fm.trace(&fm_ys);
    sub.draw(buf, r, 0, 2, fg(lit(BASS, 0.35)));
    fm.draw(buf, r, 0, 2, fg(mix(lit(BASS, 0.6), BASS, level)));
    let meter = r.width.saturating_sub(7) as usize;
    put(buf, r, 0, r.height - 1, "level ", fg(DIM));
    put(buf, r, 6, r.height - 1, &hbar(meter, level), fg(BASS));
}

// ---------------------------------------------------------------- 7 echo

fn echo(ui: &Ui, buf: &mut Buffer, r: Rect) {
    if r.height < 3 || r.width < 8 {
        return;
    }
    let cols = r.width - 2;
    let mut canvas = Braille::new(cols, r.height);
    let (dots, h) = (canvas.width(), canvas.height() as i64);
    let mid = h / 2;
    let hist = &ui.echo_history;
    // Newest at the right; each dot column covers a few blocks so ~4 s fit.
    let per = hist.len().div_ceil(dots).max(1);
    let groups = hist.len() / per;
    for g in 0..groups {
        let slice = hist.range(g * per..(g + 1) * per);
        let (l, rr) = slice.fold((0.0f64, 0.0f64), |a, v| (a.0.max(v.0), a.1.max(v.1)));
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
    canvas.draw(buf, r, 2, 0, fg(ECHO));
    put(buf, r, 0, 0, "L", fg(DIM));
    put(buf, r, 0, r.height - 1, "R", fg(DIM));
}

// ---------------------------------------------------------------- 8 reverb

fn reverb(ui: &Ui, buf: &mut Buffer, r: Rect) {
    let d = &ui.desc;
    let longest = *d.reverb_lines.iter().max().unwrap_or(&1) as f64;
    let show_matrix = r.width >= 36;
    let bar_w = r.width.saturating_sub(7 + if show_matrix { 17 } else { 0 }) as f64;
    let levels: Vec<f64> = ui.reverb_levels.iter().map(|l| level_frac(*l, 40.0)).collect();
    for (i, len) in d.reverb_lines.iter().enumerate() {
        let y = i as u16;
        if y >= r.height {
            break;
        }
        let ms = *len as f64 / d.sample_rate as f64 * 1000.0;
        put(buf, r, 0, y, &format!("{ms:>4.0}ms "), fg(DIM));
        let n = (*len as f64 / longest * bar_w).round() as usize;
        put(buf, r, 7, y, &"━".repeat(n), fg(mix(lit(REVERB, 0.3), REVERB, levels[i])));
        if show_matrix {
            // Row i of the Hadamard matrix: how line i is mixed from every line.
            let x0 = r.width - 16;
            for j in 0..8 {
                let plus = (i & j).count_ones() % 2 == 0;
                let c = mix(lit(REVERB, 0.25), if plus { REVERB } else { GLITCH1 }, levels[j]);
                put(buf, r, x0 + 2 * j as u16, y, if plus { "+" } else { "−" }, fg(c));
            }
        }
    }
}

// ---------------------------------------------------------------- 9 output

fn output(ui: &Ui, buf: &mut Buffer, r: Rect) {
    if r.height < 4 || r.width < 20 {
        return;
    }
    let plot_h = r.height - 2;
    let gonio_w = (plot_h * 2).min(r.width / 3);
    let scope_w = r.width - gonio_w - 1;
    let frames: Vec<(f32, f32)> = ui.scope.iter().rev().take(1536).rev().copied().collect();

    // Oscilloscope: left on top, right below.
    let half = (plot_h / 2).max(1);
    for (ch, y0, color) in [(0usize, 0u16, OUTPUT), (1, half, DRONE)] {
        let mut canvas = Braille::new(scope_w, half);
        let (dots, h) = (canvas.width(), canvas.height() as f64);
        if frames.len() >= dots {
            let ys: Vec<f64> = (0..dots)
                .map(|x| {
                    let f = frames[x * frames.len() / dots];
                    let v = if ch == 0 { f.0 } else { f.1 } as f64;
                    ((1.0 - (v * 1.6).clamp(-1.0, 1.0)) / 2.0) * (h - 1.0)
                })
                .collect();
            canvas.trace(&ys);
        }
        canvas.draw(buf, r, 0, y0, fg(color));
    }
    put(buf, r, 0, 0, "L", fg(DIM));
    put(buf, r, 0, half, "R", fg(DIM));

    // Goniometer: mono content is vertical, wide stereo spreads sideways.
    let mut g = Braille::new(gonio_w, plot_h);
    let (gw, gh) = (g.width() as f64, g.height() as f64);
    for (l, rr) in frames.iter().rev().take(1024) {
        let (l, rr) = (*l as f64, *rr as f64);
        let x = (rr - l) * 0.707 * 1.4;
        let y = (l + rr) * 0.707 * 1.4;
        g.set(((0.5 + x / 2.0) * (gw - 1.0)).round() as i64, ((0.5 - y / 2.0) * (gh - 1.0)).round() as i64);
    }
    g.draw(buf, r, scope_w + 1, 0, fg(mix(DIM, OUTPUT, 0.8)));

    let meter = r.width.saturating_sub(14) as usize;
    for (i, (name, v)) in [("L", ui.levels.out_l), ("R", ui.levels.out_r)].iter().enumerate() {
        let y = plot_h + i as u16;
        let db = if *v > 0.0 { 20.0 * v.log10() } else { -99.0 };
        let c = if *v > 0.89 { GLITCH2 } else { OUTPUT };
        put(buf, r, 0, y, &format!("{name} "), fg(DIM));
        put(buf, r, 2, y, &hbar(meter, level_frac(*v, 48.0)), fg(c));
        put(buf, r, 2 + meter as u16, y, &format!(" {db:>6.1} dBFS"), fg(DIM));
    }
}
