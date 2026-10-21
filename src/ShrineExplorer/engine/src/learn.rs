//! What the page's Learn mode is shown: everything about *this* shrine that
//! explains how it is made, as one JSON document, and a small array of the
//! engine's moving parts for its live views.
//!
//! Display only: nothing here feeds the sound or the picture. The JSON is
//! written by hand (no dependencies, `HC001`).

use crate::drums::{Section, Voice};
use crate::sim::{Sim, SimConfig};
use crate::track::{LoopDesign, Track};

/// Every drum section, in the order the live state's section index uses.
pub const SECTIONS: [Section; 11] = [
    Section::Out,
    Section::HatsOnly,
    Section::HalfTime,
    Section::Full,
    Section::Rolls,
    Section::Breakdown,
    Section::TwoStep,
    Section::Shuffle,
    Section::Clicks,
    Section::Pulse,
    Section::Fracture,
];

/// The live state's length, in `f64`s. Layout (see [`state`]):
/// 0 clock · 1 chord · 2 next chord · 3 morph · 4 bar in the chord ·
/// 5 drum step · 6 drum section (index into [`SECTIONS`], −1 none) ·
/// 7 drum bar · 8 hits on the latest step (bit mask: kick 1, snare 2,
/// ghost 4, hat 8, open hat 16, ride 32) · 9 drone cutoff Hz · 10 bass note
/// (MIDI) · 11 drone duck gain · 12 drone arc gain · 13–16 loop arc gains ·
/// 17–18 glitch densities · 19–33 peak meters (drone, bass, glitch 1,
/// glitch 2, echo L, echo R, reverb, drums, tape, loops 1–4, out L, out R) ·
/// 34–40 the seven slow cycles (−1..1) · 41–42 glitch steps · 43 solo
/// (index into [`SOLOS`] + 1, 0 none) · 44 solo fade · 45 drum hits so far.
pub const STATE_LEN: usize = 48;

/// The layers that can be soloed, in the order `shrine_set_solo` numbers
/// them (from 1; 0 is the full mix).
pub const SOLOS: [crate::track::Solo; 10] = {
    use crate::track::Solo::*;
    [Drone, Glitch1, Glitch2, Bass, Drums, Loop1, Loop2, Loop3, Loop4, Space]
};

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn num(x: f64) -> String {
    if !x.is_finite() {
        return "0".to_string();
    }
    let r = (x * 1000.0).round() / 1000.0;
    if r == r.trunc() && r.abs() < 1e15 {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    }
}

fn arr<T>(items: impl IntoIterator<Item = T>, f: impl Fn(T) -> String) -> String {
    let v: Vec<String> = items.into_iter().map(f).collect();
    format!("[{}]", v.join(","))
}

fn nums(items: impl IntoIterator<Item = f64>) -> String {
    arr(items, num)
}

fn rgb(c: [u8; 4]) -> String {
    esc(&format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]))
}

/// A sustained or event layer, for the page.
fn design(d: &LoopDesign) -> String {
    format!("{{\"name\":{},\"blurb\":{},\"on\":{}}}", esc(&d.name()), esc(&d.blurb()), d.is_on())
}

/// The whole description, as JSON. Builds the music engine once (without
/// playing it), so it can be asked of a visuals-only instance too.
pub fn json(config: &SimConfig) -> String {
    let track = Track::new(config.seeds, config.settings);
    let visual = Sim::new_visual(config.clone());
    describe(config, &track, &visual)
}

fn describe(config: &SimConfig, track: &Track, visual: &Sim) -> String {
    let d = track.describe();
    let s = &config.settings;
    let h = &d.harmony;
    let sr = crate::SAMPLE_RATE as f64;
    let mut f: Vec<String> = Vec::new();
    let mut put = |k: &str, v: String| f.push(format!("{}:{}", esc(k), v));

    // The piece.
    put("name", esc(s.name.as_str()));
    put("seed", esc(&config.seed.to_string()));
    let sd = config.seeds;
    put("seeds", arr([sd.s1, sd.s2, sd.s3, sd.s4, sd.s5, sd.s6, sd.s7], |x| esc(&x.to_string())));
    put("recipe", arr(crate::recipe::encode(config), |w| w.to_string()));
    put("start_s", s.start_s.to_string());

    // Mode, tempo, harmony.
    put("mode", format!("{{\"name\":{},\"blurb\":{},\"index\":{}}}", esc(s.pace.name()), esc(s.pace.blurb()), s.pace.index()));
    put("bpm", s.bpm.to_string());
    put("beat_s", num(d.tempo.beat() as f64 / sr));
    put("drum16_s", num(d.tempo.drum16 as f64 / sr));
    put(
        "scale",
        format!(
            "{{\"name\":{},\"mood\":{},\"intervals\":{}}}",
            esc(h.scale.name()),
            esc(h.scale.mood()),
            arr(h.scale.intervals().iter(), |i| i.to_string())
        ),
    );
    put("key", format!("{{\"pc\":{},\"name\":{}}}", h.key, esc(crate::harmony::key_name(h.key))));
    put(
        "chords",
        arr(h.chords.iter(), |c| {
            format!(
                "{{\"degree\":{},\"notes\":{},\"bass\":{},\"bars\":{},\"tones\":{}}}",
                c.degree,
                nums(c.notes.iter().copied()),
                num(c.bass),
                c.bars,
                arr(c.tones.iter(), |t| t.to_string())
            )
        }),
    );
    put("chord_s", num(h.bar_samples as f64 / sr));
    put("cycle_s", num(h.cycle as f64 / sr));

    // The drone: its FM recipes and frozen spectra (96 log-spaced bands,
    // 40 Hz – 16 kHz, scaled to the loudest).
    put(
        "recipes",
        arr(d.recipes.iter(), |chord| {
            arr(chord.iter(), |(algo, [r2, r3, i2, i3], fb)| {
                format!(
                    "{{\"stack\":{},\"r2\":{},\"r3\":{},\"i2\":{},\"i3\":{},\"fb\":{}}}",
                    *algo == crate::drone::Algo::Stack,
                    num(*r2),
                    num(*r3),
                    num(*i2),
                    num(*i3),
                    num(*fb)
                )
            })
        }),
    );
    let bands = 96;
    let band = |m: &[f64], k: usize| -> f64 {
        // 40 Hz × 400^(k / bands), with the engine's own exp2 (log2 400 = 8.6439).
        let edge = |k: usize| 40.0 * crate::math::exp2(8.643_856_189_774_724 * k as f64 / bands as f64);
        let bin = |hz: f64| ((hz * d.fft_size as f64 / sr) as usize).min(m.len().saturating_sub(1));
        let (a, b) = (bin(edge(k)), bin(edge(k + 1)).max(bin(edge(k)) + 1));
        m[a..b.min(m.len())].iter().fold(0.0f64, |x, y| x.max(*y))
    };
    let spectra: Vec<Vec<f64>> = d.magnitudes.iter().map(|m| (0..bands).map(|k| band(m, k)).collect()).collect();
    let peak = spectra.iter().flatten().fold(1e-12f64, |a, b| a.max(*b));
    put("spectra", arr(spectra.iter(), |v| nums(v.iter().map(|x| (x / peak).sqrt()))));
    put("fft_size", d.fft_size.to_string());
    put("hop_s", num(d.hop as f64 / sr));
    put("drone_arc", format!("{{\"cycle_s\":{},\"hold_s\":{}}}", s.drone.cycle_s, s.drone.hold_s));
    put("drone_tone", s.drone_tone.to_string());
    put("drone_cutoff", format!("{{\"min\":{},\"octaves\":{}}}", num(crate::track::DRONE_CUTOFF_MIN), num(crate::track::DRONE_CUTOFF_OCTAVES)));
    put("drone_rhythm", s.drone_rhythm.to_string());

    // Drums.
    let v = &d.drum_voices;
    put(
        "drums",
        format!(
            "{{\"on\":{},\"kit\":{},\"blurb\":{},\"lines\":{},\"pattern\":{},\"space\":{},\"space_blurb\":{},\"overview\":{},\"slices\":{},\"source_bpm\":{},\"semitones_up\":{},\"bits\":{},\"sections\":{}}}",
            d.drums_on,
            esc(s.kit.name()),
            esc(s.kit.blurb()),
            arr(v.lines.iter(), |l| esc(l)),
            esc(d.break_pattern),
            esc(s.space.name()),
            esc(s.space.blurb()),
            nums(d.break_overview.iter().map(|x| *x as f64)),
            arr(d.slice_voices.iter(), |v| esc(&v.map_or(String::new(), |v: Voice| v.letter().to_string()))),
            num(v.source_bpm),
            num(v.semitones_up),
            v.bits,
            arr(d.sections.iter().take(64), |sec| SECTIONS.iter().position(|x| x == sec).unwrap_or(0).to_string())
        ),
    );
    put("section_names", arr(SECTIONS.iter(), |x| esc(x.name())));
    put("phrase_s", num((crate::drums::PHRASE_BARS * d.tempo.drum_bar()) as f64 / sr));

    // Glitches, echo and reverb.
    put(
        "glitch",
        arr(0..2, |i| {
            format!(
                "{{\"steps\":{},\"step_s\":{},\"summary\":{},\"on\":{}}}",
                d.layers[i].0,
                num(d.layers[i].1 as f64 / sr),
                esc(&s.glitch[i].summary()),
                s.glitch[i].enabled
            )
        }),
    );
    put("echo", format!("{{\"delay_s\":{},\"feedback\":{}}}", num(d.echo_delay as f64 / sr), num(d.echo_feedback)));
    put(
        "reverb",
        format!(
            "{{\"lines\":{},\"decay_s\":{}}}",
            arr(d.reverb_lines.iter(), |l| l.to_string()),
            num(if s.pace == crate::harmony::Pace::Micro { 1.2 } else { d.reverb_decay })
        ),
    );

    // The atmosphere loops.
    put(
        "loops",
        arr(d.loops.iter().enumerate(), |(i, l)| {
            format!(
                "{{\"design\":{},\"on\":{},\"beats\":{},\"len_s\":{},\"notes\":{},\"arc\":{{\"cycle_s\":{},\"hold_s\":{}}}}}",
                design(&s.loops[i]),
                l.on(),
                l.beats,
                num(l.len as f64 / sr),
                arr(l.notes.iter(), |n| nums(n.iter().copied())),
                s.loop_arcs[i].cycle_s,
                s.loop_arcs[i].hold_s
            )
        }),
    );
    put("transpose", s.loop_transpose.to_string());

    // The modes' own parts.
    let g = s.garage;
    put(
        "garage",
        format!(
            "{{\"swing\":{},\"progression\":{},\"lift\":{},\"bass\":{},\"stabs\":{},\"tone\":{}}}",
            g.swing,
            esc(g.progression_name()),
            esc(g.lift.name()),
            esc(g.bass.name()),
            esc(g.stabs.name()),
            esc(g.tone.name())
        ),
    );
    let m = s.micro;
    let pattern = |p: (u64, u64)| format!("[{},{}]", p.0, p.1);
    put(
        "micro",
        format!(
            "{{\"pulse\":{},\"grains\":{},\"pips\":{},\"dropouts\":{},\"chords\":{},\"shift\":{},\"drone_fragments\":{},\"loop_fragments\":{},\"shift_figure\":{}}}",
            esc(m.pulse.name()),
            esc(m.grains.name()),
            esc(m.pips.name()),
            esc(m.dropouts.name()),
            esc(m.chords.name()),
            esc(m.shift.name()),
            track.drone_gate.as_ref().map_or("null".to_string(), |g| pattern(g.pattern())),
            track.loop_gates.as_ref().map_or("null".to_string(), |gs| arr(gs.iter(), |g| pattern(g.pattern()))),
            track.drone_shift.as_ref().map_or("null".to_string(), |(_, f)| nums(f.iter().copied()))
        ),
    );
    put("bass_root", num(d.bass_root));
    put("master", arr(s.master.labels().iter().filter(|(_, on)| *on && s.master.enabled), |(n, _)| esc(n)));

    // The picture.
    let scene = &visual.scene;
    put("biome", format!("{{\"name\":{},\"blurb\":{},\"roofed\":{}}}", esc(config.biome.name()), esc(config.biome.blurb()), config.biome.roofed()));
    put("weather", format!("{{\"name\":{},\"blurb\":{}}}", esc(config.weather.name()), esc(config.weather.blurb())));
    let p = &scene.palette;
    put(
        "palette",
        format!(
            "{{\"sky\":[{},{}],\"ground\":[{},{}],\"stone\":[{},{},{}],\"foliage\":[{},{},{}],\"water\":{},\"accent\":{},\"fog\":{}}}",
            rgb(p.sky_top),
            rgb(p.sky_bottom),
            rgb(p.ground),
            rgb(p.ground_dark),
            rgb(p.stone_light),
            rgb(p.stone),
            rgb(p.stone_dark),
            rgb(p.foliage_light),
            rgb(p.foliage),
            rgb(p.foliage_dark),
            rgb(p.water),
            rgb(p.accent),
            rgb(p.fog)
        ),
    );
    let (ox, oy) = scene.shrine.origin();
    let w = crate::SHRINE_W as i32;
    put(
        "shrine",
        format!(
            "{{\"origin\":[{ox},{oy}],\"size\":{w},\"grid\":{},\"heights\":{},\"authored\":{}}}",
            crate::GRID_W,
            arr(0..w * w, |i| scene.shrine.height_at(i % w, i / w).to_string()),
            s.model.is_authored()
        ),
    );
    put("coupling", arr(crate::sim::COUPLING.iter(), |(a, b)| format!("[{},{}]", esc(a), esc(b))));
    put("camera_speed", s.camera_speed.to_string());
    put("code", format!("{{\"on\":{},\"summary\":{}}}", s.code.enabled, esc(&s.code.summary())));
    put("arch", esc(&s.arch.summary()));
    put(
        "wildlife",
        if s.wildlife { esc(&crate::wildlife::Wildlife::new(config.seed, config.biome).summary()) } else { "null".to_string() },
    );
    put("solos", arr(SOLOS.iter(), |x| esc(&format!("{x:?}"))));
    format!("{{{}}}", f.join(","))
}

/// The engine's moving parts right now, into `out` (`STATE_LEN` long). Also
/// resets the peak meters, so each call reads the peaks since the last.
pub fn state(track: &mut Track, out: &mut [f64]) {
    let snap = track.snapshot();
    out.iter_mut().for_each(|x| *x = 0.0);
    let mut set = |i: usize, v: f64| {
        if let Some(x) = out.get_mut(i) {
            *x = if v.is_finite() { v } else { 0.0 };
        }
    };
    set(0, snap.clock as f64);
    set(1, snap.harmony.chord as f64);
    set(2, snap.harmony.next as f64);
    set(3, snap.harmony.morph);
    set(4, snap.harmony.bar as f64);
    set(5, snap.drum_step as f64);
    match &snap.drum_plan {
        Some(p) => {
            set(6, SECTIONS.iter().position(|x| *x == p.section).map_or(-1.0, |i| i as f64));
            set(7, p.bar as f64);
        }
        None => set(6, -1.0),
    }
    let mask = snap.drum_hits.iter().fold(0u32, |m, v| {
        m | match v {
            Voice::Kick => 1,
            Voice::Snare => 2,
            Voice::Ghost => 4,
            Voice::Hat => 8,
            Voice::OpenHat => 16,
            Voice::Ride => 32,
        }
    });
    set(8, mask as f64);
    set(9, snap.drone_cutoff);
    set(10, snap.bass_root);
    set(11, snap.duck_gain);
    set(12, snap.drone_window);
    for k in 0..4 {
        set(13 + k, snap.loop_windows[k]);
    }
    set(17, snap.layers[0].density);
    set(18, snap.layers[1].density);
    let mt = &snap.meters;
    let meters = [
        mt.drone, mt.bass, mt.glitch1, mt.glitch2, mt.echo_l, mt.echo_r, mt.reverb, mt.drums, mt.tape, mt.loop1, mt.loop2, mt.loop3,
        mt.loop4, mt.out_l, mt.out_r,
    ];
    for (k, v) in meters.into_iter().enumerate() {
        set(19 + k, v);
    }
    for (k, (_, v)) in snap.lfos.iter().enumerate() {
        set(34 + k, *v);
    }
    set(41, snap.layers[0].step as f64);
    set(42, snap.layers[1].step as f64);
    set(43, snap.solo.and_then(|s| SOLOS.iter().position(|x| *x == s)).map_or(0.0, |i| i as f64 + 1.0));
    set(44, snap.solo_amount);
    set(45, snap.drum_hit_count as f64);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_description_is_balanced_json_for_every_mode() {
        for pace in crate::harmony::PACES {
            let mut c = SimConfig::default();
            c.settings.set_mode(pace);
            c.settings.name = crate::name::Name::from_str("A \"quoted\" name");
            let j = json(&c);
            // Brackets balance outside strings, and nothing is left unescaped.
            let (mut depth, mut in_str, mut escaped) = (0i32, false, false);
            for ch in j.chars() {
                if in_str {
                    match (escaped, ch) {
                        (true, _) => escaped = false,
                        (false, '\\') => escaped = true,
                        (false, '"') => in_str = false,
                        _ => {}
                    }
                    continue;
                }
                match ch {
                    '"' => in_str = true,
                    '{' | '[' => depth += 1,
                    '}' | ']' => depth -= 1,
                    _ => {}
                }
                assert!(depth >= 0, "{pace:?}: closes too early");
            }
            assert_eq!(depth, 0, "{pace:?}: unbalanced");
            assert!(!in_str);
            assert!(j.contains("\"spectra\"") && j.contains("\"shrine\"") && j.contains("\"chords\""));
            assert!(!j.contains("NaN") && !j.contains("inf"));
        }
    }

    #[test]
    fn the_live_state_reads_the_running_track() {
        let mut t = Track::new(crate::DEFAULT_SEEDS, crate::DEFAULT_SETTINGS);
        let mut buf = vec![0f32; 48_000 * 2];
        t.render(&mut buf);
        let mut out = vec![0.0; STATE_LEN];
        state(&mut t, &mut out);
        assert_eq!(out[0], 48_000.0);
        assert!(out[19] > 0.0, "the drone meter reads nothing");
    }
}
