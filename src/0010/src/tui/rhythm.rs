//! View 3, "Rhythm & Harmony": the chord progression and scale, the drum
//! arrangement, the jungle break being chopped, and how each drum voice is
//! synthesised.

use super::draw::{frame, note_name};
use super::gfx::*;
use super::{Panel, Ui};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType};
use ratatui::Frame;
use shrine0010::drums::{self, Op, Section, Voice, PHRASE_BARS, STEPS};
use shrine0010::harmony::key_name;

pub const VOICES: [Voice; 6] = [Voice::Kick, Voice::Snare, Voice::Ghost, Voice::Hat, Voice::OpenHat, Voice::Ride];

pub fn voice_index(v: Voice) -> usize {
    VOICES.iter().position(|x| *x == v).unwrap_or(0)
}

pub fn voice_color(v: Voice) -> Rgb {
    match v {
        Voice::Kick => (255, 110, 70),
        Voice::Snare => (255, 200, 90),
        Voice::Ghost => (200, 160, 90),
        Voice::Hat => (200, 230, 255),
        Voice::OpenHat => (150, 210, 255),
        Voice::Ride => (170, 180, 255),
    }
}

fn roman(degree: usize, scale_len: usize) -> String {
    const R: [&str; 7] = ["I", "II", "III", "IV", "V", "VI", "VII"];
    if scale_len == 7 {
        R[degree % 7].to_string()
    } else {
        format!("{}", degree + 1)
    }
}

fn op_glyph(op: Op) -> char {
    match op {
        Op::Play => '→',
        Op::Reverse => '←',
        Op::Roll => '≡',
        Op::PitchDown => '↓',
        Op::Half => '½',
    }
}

fn section_glyph(s: Section) -> char {
    match s {
        Section::Out => '·',
        Section::HatsOnly => '▂',
        Section::HalfTime => '▄',
        Section::Full => '▆',
        Section::Rolls => '█',
    }
}

/// The progression as proportional blocks with a playhead (2 rows).
pub fn chord_strip(ui: &Ui, buf: &mut Buffer, r: Rect) {
    let h = &ui.desc.harmony;
    let Some(s) = &ui.snap else { return };
    if r.width < 10 || r.height < 2 {
        return;
    }
    let w = r.width as usize;
    let total: u32 = h.chords.iter().map(|c| c.bars).sum();
    let pos = (s.clock % h.cycle) as f64 / h.cycle as f64;
    let n = h.chords.len();
    let mut x0 = 0usize;
    for (i, c) in h.chords.iter().enumerate() {
        let x1 = if i + 1 == n { w } else { x0 + (c.bars as usize * w) / total as usize };
        let current = i == s.harmony.chord;
        let label = format!(
            " {} {}{} · {} bars",
            i + 1,
            note_name(c.bass).trim_end_matches(char::is_numeric),
            if h.scale.intervals().len() == 7 { format!(" ({})", roman(c.degree, 7)) } else { String::new() },
            c.bars
        );
        let style = if current { fg(HARMONY).add_modifier(Modifier::BOLD) } else { fg(DIM) };
        put(buf, Rect::new(r.x + x0 as u16, r.y, (x1 - x0) as u16, 1), 0, 0, &label, style);
        for x in x0..x1 {
            // The last bar of each chord is its morph into the next.
            let bar_w = ((x1 - x0) as f64 / c.bars as f64).max(1.0);
            let morphing = (x as f64) >= x1 as f64 - bar_w;
            let ch = if x == x0 { '▕' } else if morphing { '▒' } else { '█' };
            let c = if current { lit(HARMONY, 0.75) } else { lit(HARMONY, 0.3) };
            put_char(buf, r, x as u16, 1, ch, fg(c));
        }
        x0 = x1;
    }
    let px = ((pos * w as f64) as u16).min(r.width - 1);
    put_char(buf, r, px, 1, '┃', fg(WHITE).add_modifier(Modifier::BOLD));
}

pub fn draw(ui: &Ui, f: &mut Frame, area: Rect, explaining: Panel) {
    let [top, mid, bottom] =
        Layout::vertical([Constraint::Percentage(35), Constraint::Percentage(36), Constraint::Percentage(29)]).areas(area);
    let [harm, arr] = Layout::horizontal([Constraint::Percentage(56), Constraint::Percentage(44)]).areas(top);

    let r = frame(ui, f, Panel::Harmony, harm, explaining == Panel::Harmony);
    harmony(ui, f.buffer_mut(), r);

    let lit_arr = explaining == Panel::Drums;
    let block = Block::bordered()
        .border_type(if lit_arr { BorderType::Thick } else { BorderType::Rounded })
        .border_style(if lit_arr { fg(DRUMS) } else { fg(mix(FAINT, DRUMS, 0.25)) })
        .title(Span::styled(" ARRANGEMENT · 4-bar phrases ", fg(mix(DIM, DRUMS, 0.6))))
        .title_bottom(Line::from(Span::styled(" ·  out  ▂ hats  ▄ half-time  ▆ full  █ rolls ", fg(DIM))));
    let r = block.inner(arr);
    f.render_widget(block, arr);
    arrangement(ui, f.buffer_mut(), r);

    let r = frame(ui, f, Panel::Break, mid, explaining == Panel::Break);
    chopper(ui, f.buffer_mut(), r);

    let r = frame(ui, f, Panel::Drums, bottom, explaining == Panel::Drums);
    voices(ui, f.buffer_mut(), r);
}

fn harmony(ui: &Ui, buf: &mut Buffer, r: Rect) {
    let h = &ui.desc.harmony;
    let Some(s) = &ui.snap else { return };
    let chord = &h.chords[s.harmony.chord];
    put(
        buf,
        r,
        0,
        0,
        &format!("{} {} · {}", key_name(h.key), h.scale.name(), h.scale.mood()),
        fg(HARMONY).add_modifier(Modifier::BOLD),
    );
    // Two octaves of semitones from the key: in-scale dim, chord tones lit, root marked.
    let iv = h.scale.intervals();
    let chord_pcs: Vec<u8> = chord.notes.iter().map(|n| (*n as i64).rem_euclid(12) as u8).collect();
    let cw = (r.width as usize / 24).clamp(1, 3) as u16;
    for i in 0..24u16 {
        let pc = ((h.key as u16 + i) % 12) as u8;
        let rel = ((i % 12) as u8) % 12;
        let in_scale = iv.contains(&rel);
        let in_chord = chord_pcs.contains(&pc);
        let x = i * cw;
        let name = key_name(pc);
        if cw >= 2 || !name.contains('#') {
            put(buf, r, x, 2, &name.chars().take(cw as usize).collect::<String>(), fg(if in_scale { TEXT } else { FAINT }));
        }
        let (ch, c) = if in_chord {
            ('█', HARMONY)
        } else if in_scale {
            ('▒', lit(HARMONY, 0.35))
        } else {
            ('·', FAINT)
        };
        put(buf, r, x, 3, &ch.to_string().repeat(cw as usize), fg(c));
        if rel == 0 {
            put(buf, r, x, 4, "▲", fg(HARMONY));
        }
    }
    let voicing: Vec<String> = chord.notes.iter().map(|n| note_name(*n)).collect();
    if r.height > 6 {
        put(buf, r, 0, 5, &format!("chord {}: {}", s.harmony.chord + 1, voicing.join(" ")), fg(TEXT));
    }
    if r.height > 8 {
        chord_strip(ui, buf, Rect::new(r.x, r.y + r.height - 2, r.width, 2));
    }
}

fn arrangement(ui: &Ui, buf: &mut Buffer, r: Rect) {
    if !ui.desc.drums_on {
        put(buf, r, 0, 0, "Drums are off (seed 5 = 0).", fg(TEXT));
        put(buf, r, 0, 1, "Any other seed 5 brings in the jungle break.", fg(DIM));
        return;
    }
    let Some(s) = &ui.snap else { return };
    let Some(plan) = &s.drum_plan else {
        put(buf, r, 0, 0, "waiting for the first bar…", fg(DIM));
        return;
    };
    let phrase = plan.bar / PHRASE_BARS;
    let bar = plan.bar % PHRASE_BARS + 1;
    let fill = if plan.fill { " · FILL" } else { "" };
    put(
        buf,
        r,
        0,
        0,
        &format!("phrase {} · bar {bar}/{PHRASE_BARS} · {}{fill}", phrase + 1, plan.section.name()),
        fg(DRUMS).add_modifier(Modifier::BOLD),
    );
    // Recent phrases, newest at the right.
    let cells = (r.width / 2) as usize;
    let recent: Vec<&(u64, Section)> = ui.sections.iter().rev().take(cells).collect();
    for (k, (p, sec)) in recent.iter().rev().enumerate() {
        let current = *p == phrase;
        let c = if current { DRUMS } else { lit(DRUMS, 0.45) };
        put(buf, r, (k * 2) as u16, 2, &section_glyph(*sec).to_string(), fg(c).add_modifier(Modifier::BOLD));
    }
    put(buf, r, 0, 3, "↑ recent phrases (newest right)", fg(FAINT));
    let edits = plan
        .steps
        .iter()
        .enumerate()
        .filter(|(i, st)| {
            let straight = (plan.bar % 2) as usize * STEPS + i;
            !(st.op == Op::Play && st.slice as usize == straight) && st.op != Op::Half
        })
        .count();
    if r.height > 5 {
        put(buf, r, 0, 5, &format!("edits this bar: {edits} of {STEPS} steps"), fg(TEXT));
    }
    if r.height > 6 {
        let meter = r.width.saturating_sub(8) as usize;
        put(buf, r, 0, 6, "level ", fg(DIM));
        put(buf, r, 6, 6, &hbar(meter, level_frac(ui.levels.drums, 40.0)), fg(DRUMS));
    }
    if r.height > 8 {
        let v = &ui.desc.drum_voices;
        put(buf, r, 0, 8, &format!("{} · {} at {:.0} BPM, played at 168", v.kit, ui.desc.break_pattern, v.source_bpm), fg(DIM));
    }
}

fn chopper(ui: &Ui, buf: &mut Buffer, r: Rect) {
    if !ui.desc.drums_on {
        put(buf, r, 0, 0, "Drums are off (seed 5 = 0): nothing to chop.", fg(DIM));
        return;
    }
    if r.height < 5 || r.width < 40 {
        return;
    }
    let Some(s) = &ui.snap else { return };
    let w = r.width as usize;
    let slice_w = (w / drums::SLICES).max(1);
    let grid_w = slice_w * drums::SLICES;
    let wave_rows = r.height.saturating_sub(6).clamp(2, 8);
    let playing = s.drum_plan.as_ref().map(|p| p.steps[s.drum_step.min(STEPS - 1)].slice as usize);

    // The sampled break: waveform with the 32-slice grid.
    put(buf, r, 0, 0, "the sampled break (2 bars, 32 slices):", fg(DIM));
    let mut canvas = Braille::new(grid_w as u16, wave_rows);
    let (dots, hgt) = (canvas.width(), canvas.height() as f64);
    let ov = &ui.desc.break_overview;
    if !ov.is_empty() {
        let peak = ov.iter().cloned().fold(1e-9f32, f32::max) as f64;
        for x in 0..dots {
            let v = ov[x * ov.len() / dots] as f64 / peak;
            let half = v * (hgt - 1.0) / 2.0;
            let mid = (hgt - 1.0) / 2.0;
            canvas.vline(x as i64, (mid - half).round() as i64, (mid + half).round() as i64);
        }
    }
    canvas.draw(buf, r, 0, 1, fg(lit(DRUMS, 0.55)));
    // Highlight the slice being played right now.
    if let Some(sl) = playing {
        for y in 1..1 + wave_rows {
            for x in sl * slice_w..(sl + 1) * slice_w {
                if let Some(cell) = buf.cell_mut((r.x + x as u16, r.y + y)) {
                    cell.set_fg(rgb(WHITE));
                }
            }
        }
    }
    // Each slice's main hit.
    let ly = 1 + wave_rows;
    for (i, v) in ui.desc.slice_voices.iter().enumerate() {
        let x = (i * slice_w) as u16;
        let (ch, c) = match v {
            Some(v) => (v.letter(), voice_color(*v)),
            None => ('·', FAINT),
        };
        let mut style = fg(c);
        if Some(i) == playing {
            style = style.bg(rgb(lit(DRUMS, 0.5)));
        }
        put(buf, r, x, ly, &ch.to_string(), style);
    }

    // This bar's plan: which slice each step plays, and how.
    let Some(plan) = &s.drum_plan else { return };
    let py = ly + 2;
    if py >= r.height {
        return;
    }
    put(buf, r, 0, py - 1, "this bar plays:", fg(DIM));
    let cell_w = (r.width as usize / STEPS).clamp(3, 6);
    for (i, st) in plan.steps.iter().enumerate() {
        let straight = (plan.bar % 2) as usize * STEPS + i;
        let edited = !(st.op == Op::Play && st.slice as usize == straight) && st.op != Op::Half;
        let current = i == s.drum_step;
        let text = format!("{}{:02}", op_glyph(st.op), st.slice + 1);
        let mut style = if plan.section == Section::Out {
            fg(FAINT)
        } else if edited {
            fg(DRUMS).add_modifier(Modifier::BOLD)
        } else {
            fg(DIM)
        };
        if current && plan.section != Section::Out {
            style = Style::new().fg(rgb((20, 20, 20))).bg(rgb(DRUMS));
        }
        put(buf, r, (i * cell_w) as u16, py, &text, style);
    }
}

fn voices(ui: &Ui, buf: &mut Buffer, r: Rect) {
    if !ui.desc.drums_on {
        put(buf, r, 0, 0, "Drums are off (seed 5 = 0).", fg(DIM));
        return;
    }
    if r.height < 4 || r.width < 60 {
        return;
    }
    let info = &ui.desc.drum_voices;
    let col_w = r.width / 4;
    let cols: Vec<Rect> = (0..4).map(|i| Rect::new(r.x + i * col_w, r.y, col_w.saturating_sub(1), r.height)).collect();
    let flash = |v: Voice| ui.drum_flash[voice_index(v)];
    let title = |buf: &mut Buffer, c: Rect, text: &str, vs: &[Voice]| {
        let f = vs.iter().map(|v| flash(*v)).fold(0.0, f64::max);
        put(buf, c, 0, 0, text, fg(mix(lit(DRUMS, 0.7), WHITE, f)).add_modifier(Modifier::BOLD));
        let mut x = text.chars().count() as u16 + 1;
        for v in vs {
            let c2 = mix(lit(voice_color(*v), 0.3), voice_color(*v), flash(*v));
            put(buf, c, x, 0, &v.letter().to_string(), fg(c2).add_modifier(Modifier::BOLD));
            x += 2;
        }
    };
    let plot_rows = r.height.saturating_sub(3).max(1);
    let fx = |c: Rect, hz: f64| {
        ((hz / 30.0).ln() / (16_000.0f64 / 30.0).ln() * (c.width as f64 - 1.0)).clamp(0.0, c.width as f64 - 1.0) as u16
    };

    // Kick: its pitch envelope over the first 150 ms.
    let c = cols[0];
    title(buf, c, "KICK", &[Voice::Kick]);
    let mut k = Braille::new(c.width, plot_rows);
    let (kw, kh) = (k.width(), k.height() as f64);
    let top = info.kick_from_hz.max(1.0);
    let ys: Vec<f64> = (0..kw)
        .map(|x| {
            let t = x as f64 / kw as f64 * 0.15;
            let f = info.kick_to_hz + (info.kick_from_hz - info.kick_to_hz) * (-t / (info.kick_drop_ms / 1000.0)).exp();
            (1.0 - (f - info.kick_to_hz * 0.8) / (top - info.kick_to_hz * 0.8)) * (kh - 1.0)
        })
        .collect();
    k.trace(&ys);
    k.draw(buf, c, 0, 1, fg(mix(lit(voice_color(Voice::Kick), 0.6), voice_color(Voice::Kick), flash(Voice::Kick))));
    put(buf, c, 0, r.height - 1, &info.lines[0], fg(DIM));

    // Snare: its tones on a log frequency axis, with the noise band shaded.
    let c = cols[1];
    title(buf, c, "SNARE", &[Voice::Snare, Voice::Ghost]);
    let fl = flash(Voice::Snare).max(flash(Voice::Ghost));
    for y in 1..1 + plot_rows {
        for x in fx(c, 1200.0)..=fx(c, 3600.0) {
            put_char(buf, c, x, y, '░', fg(mix(lit(DRUMS, 0.35), voice_color(Voice::Snare), fl * 0.8)));
        }
        for hz in info.snare_hz.iter().filter(|h| **h > 0.0) {
            put_char(buf, c, fx(c, *hz), y, '█', fg(mix(lit(voice_color(Voice::Snare), 0.6), WHITE, fl * 0.5)));
        }
    }
    put(buf, c, 0, r.height - 1, &info.lines[1], fg(DIM));

    // Cymbals: their oscillators / partials / modes, animated.
    let c = cols[2];
    title(buf, c, "HATS · RIDE", &[Voice::Hat, Voice::OpenHat, Voice::Ride]);
    let fl = flash(Voice::Hat).max(flash(Voice::OpenHat)).max(flash(Voice::Ride));
    for (i, hz) in info.cymbal_hz.iter().enumerate() {
        for y in 1..1 + plot_rows {
            let phase = (ui.t * hz / 400.0 + y as f64 * 0.3 + i as f64).sin();
            let ch = if phase > 0.0 { '▀' } else { '▄' };
            put_char(buf, c, fx(c, hz * 8.0), y, ch, fg(mix(lit(voice_color(Voice::Ride), 0.5), WHITE, fl * 0.7)));
        }
    }
    put(buf, c, 0, r.height - 1, &info.lines[2], fg(DIM));

    // The kit and its sampler.
    let c = cols[3];
    title(buf, c, "KIT · SAMPLER", &[]);
    let lines = [
        info.kit.to_string(),
        format!("{:.0} → {:.0} BPM (+{:.1} st)", info.source_bpm, info.play_bpm, info.semitones_up),
        format!("{}-bit · {:.0} kHz hold", info.bits, info.hold_khz),
        "punch: comp 4:1 + soft clip".to_string(),
        "cymbals choke · small room".to_string(),
    ];
    for (i, l) in lines.iter().enumerate() {
        if (i as u16 + 1) < r.height {
            let st = if i == 0 { fg(DRUMS).add_modifier(Modifier::BOLD) } else { fg(TEXT) };
            put(buf, c, 0, i as u16 + 1, l, st);
        }
    }
}
