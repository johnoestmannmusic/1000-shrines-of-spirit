//! Shared pieces of the native front-ends (`render-0010` and the `shrine-0010`
//! app): terminal prompts, the end-of-file fade, and the WAV writer.
//! Standard library only, like the rest of the engine.

use crate::math::cos_turns;
use crate::rng::Rng;
use crate::harmony::{bar_weights, Pace, Scale, SCALES};
use crate::kits::{Kit, KITS};
use crate::{Seeds, Settings, Track, DEFAULT_SEEDS, DEFAULT_SETTINGS, SAMPLE_RATE};
use std::fs::File;
use std::io::{self, BufRead, BufWriter, Seek, SeekFrom, Write};
use std::path::Path;

pub const DEFAULT_SECONDS: f64 = 600.0;
pub const DEFAULT_FILE: &str = "0010.wav";
/// Fade-out length; renders shorter than four times this fade over their last quarter.
pub const FADE_OUT_SECONDS: f64 = 40.0;
/// A WAV's size fields are 32-bit: 24-bit stereo at 48 kHz fits just over 4 hours.
pub const MAX_SECONDS: f64 = 4.0 * 3600.0;

pub fn parse_u64(s: &str) -> Option<u64> {
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}

/// "90", "90.5", "1:30" or "1:02:30" → seconds.
pub fn parse_length(s: &str) -> Option<f64> {
    let mut total = 0.0;
    for part in s.split(':') {
        let v: f64 = part.trim().parse().ok()?;
        if !(v >= 0.0) {
            return None;
        }
        total = total * 60.0 + v;
    }
    (total > 0.0 && total <= MAX_SECONDS).then_some(total)
}

pub fn format_length(seconds: f64) -> String {
    let s = seconds.max(0.0).round() as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// Shows `question [default]: ` and returns the trimmed answer, or `default` on
/// an empty answer. Exits cleanly if input ends (Ctrl-D).
pub fn ask(input: &mut impl BufRead, question: &str, default: &str) -> String {
    print!("{question} [{default}]: ");
    io::stdout().flush().ok();
    let mut line = String::new();
    if input.read_line(&mut line).unwrap_or(0) == 0 {
        println!();
        std::process::exit(130);
    }
    let answer = line.trim();
    if answer.is_empty() {
        default.to_string()
    } else {
        answer.to_string()
    }
}

/// Keeps asking until `parse` accepts the answer.
pub fn ask_until<T>(
    input: &mut impl BufRead,
    question: &str,
    default: &str,
    hint: &str,
    parse: impl Fn(&str) -> Option<T>,
) -> T {
    loop {
        let answer = ask(input, question, default);
        match parse(&answer) {
            Some(v) => return v,
            None => println!("  {hint}"),
        }
    }
}

/// A scale by number (1-12, in `SCALES` order) or by (the start of) its name.
pub fn parse_scale(s: &str) -> Option<Scale> {
    let s = s.trim().to_ascii_lowercase();
    if let Ok(n) = s.parse::<usize>() {
        return (1..=SCALES.len()).contains(&n).then(|| SCALES[n - 1]);
    }
    let compact = |x: &str| x.to_ascii_lowercase().replace([' ', '-', '(', ')'], "");
    let want = compact(&s);
    (!want.is_empty()).then_some(())?;
    SCALES.iter().copied().find(|sc| compact(sc.name()).starts_with(&want))
}

/// The track a recipe belongs to.
pub const TRACK: &str = "0010";

/// A recipe: everything needed to regenerate a version of the track,
/// e.g. `0010-LYD-3H-ACO-1000.9.1009.2026.168` (track, scale, chord count +
/// pace (H half-time / J jungle), drum kit, seeds 1-5).
pub fn recipe(seeds: Seeds, settings: Settings) -> String {
    format!(
        "{TRACK}-{}-{}{}-{}-{}.{}.{}.{}.{}",
        settings.scale.code(),
        settings.chords,
        if settings.pace == Pace::Jungle { "J" } else { "H" },
        settings.kit.code(),
        seeds.s1,
        seeds.s2,
        seeds.s3,
        seeds.s4,
        seeds.s5
    )
}

/// Parses a recipe (case and surrounding spaces don't matter).
pub fn parse_recipe(code: &str) -> Option<(Seeds, Settings)> {
    let code = code.trim();
    let mut parts = code.split('-');
    if !parts.next()?.eq_ignore_ascii_case(TRACK) {
        return None;
    }
    let scale = Scale::from_code(parts.next()?)?;
    let cp = parts.next()?;
    let (count, pace) = cp.split_at(cp.len().checked_sub(1)?);
    let chords = parse_chords(count)?;
    let pace = match pace.to_ascii_uppercase().as_str() {
        "H" => Pace::HalfTime,
        "J" => Pace::Jungle,
        _ => return None,
    };
    // The kit code; older recipes have none (seed 5 used to pick the kit).
    let mut next = parts.next()?;
    let kit = match Kit::from_code(next) {
        Some(k) => {
            next = parts.next()?;
            Some(k)
        }
        None => None,
    };
    let seeds: Vec<u64> = next.split('.').map(|s| s.parse().ok()).collect::<Option<_>>()?;
    if parts.next().is_some() || seeds.len() != 5 {
        return None;
    }
    let seeds = Seeds { s1: seeds[0], s2: seeds[1], s3: seeds[2], s4: seeds[3], s5: seeds[4] };
    let kit = kit.unwrap_or_else(|| crate::kits::legacy_kit_for(seeds.s5));
    Some((seeds, Settings { scale, chords, pace, kit }))
}

/// A kit by number (1-6), code (e.g. FMM) or (the start of) its name.
pub fn parse_kit(s: &str) -> Option<Kit> {
    let s = s.trim();
    if let Ok(n) = s.parse::<usize>() {
        return (1..=KITS.len()).contains(&n).then(|| KITS[n - 1]);
    }
    let want = s.to_ascii_lowercase();
    (!want.is_empty()).then_some(())?;
    Kit::from_code(s).or_else(|| KITS.iter().copied().find(|k| k.name().to_ascii_lowercase().starts_with(&want)))
}

pub fn parse_chords(s: &str) -> Option<u8> {
    s.trim().parse::<u8>().ok().filter(|n| (1..=4).contains(n))
}

/// "half" / "1" → half-time; "jungle" / "2" → jungle pace.
pub fn parse_pace(s: &str) -> Option<Pace> {
    match s.trim().to_ascii_lowercase().as_str() {
        "1" | "half" | "halftime" | "half-time" | "h" => Some(Pace::HalfTime),
        "2" | "jungle" | "full" | "j" => Some(Pace::Jungle),
        _ => None,
    }
}

/// "16 / 12 / 4 bars" for a chord count.
pub fn bars_text(chords: u8) -> String {
    let w: Vec<String> = bar_weights(chords as usize).iter().map(|b| b.to_string()).collect();
    format!("{} bars", w.join(" / "))
}

pub fn ask_settings(input: &mut impl BufRead) -> Settings {
    println!("Scales:");
    for (i, s) in SCALES.iter().enumerate() {
        println!("  {:>2}) {:<24} {}", i + 1, s.name(), s.mood());
    }
    let scale = ask_until(input, "Scale", "1", "Enter 1-12 or a scale name.", parse_scale);
    println!("Chords: 1 = {}, 2 = {}, 3 = {}, 4 = {}", bars_text(1), bars_text(2), bars_text(3), bars_text(4));
    let chords = ask_until(input, "How many chords (1-4)", &DEFAULT_SETTINGS.chords.to_string(), "Enter 1 to 4.", parse_chords);
    println!("Chord pace: 1 = half-time (bars ≈2.9 s), 2 = jungle (bars ≈1.4 s)");
    let pace = ask_until(input, "Chord pace", "1", "Enter 1 or 2.", parse_pace);
    println!("Drum kits:");
    for (i, k) in KITS.iter().enumerate() {
        println!("  {}) {:<16} {}", i + 1, k.name(), k.blurb());
    }
    let kit = ask_until(input, "Drum kit (seed 5 shapes it)", "1", "Enter 1-6 or a kit name.", parse_kit);
    println!();
    Settings { scale, chords, pace, kit }
}

pub fn ask_seeds(input: &mut impl BufRead) -> Seeds {
    println!("Seeds (the defaults are the canonical version of the track; decimal or 0x hex):");
    let hint = "Enter a whole number from 0 to 18446744073709551615, or 0x hex.";
    let mut seed = |n: u32, default: u64| {
        ask_until(input, &format!("  seed {n}"), &default.to_string(), hint, parse_u64)
    };
    let seeds = Seeds {
        s1: seed(1, DEFAULT_SEEDS.s1),
        s2: seed(2, DEFAULT_SEEDS.s2),
        s3: seed(3, DEFAULT_SEEDS.s3),
        s4: seed(4, DEFAULT_SEEDS.s4),
        s5: {
            println!("  (seed 5 shapes the drums; 0 = no drums)");
            seed(5, DEFAULT_SEEDS.s5)
        },
    };
    println!();
    seeds
}

/// Asks for the output file (adds `.wav`, confirms overwrites) and the length.
pub fn ask_output(input: &mut impl BufRead) -> (String, f64) {
    let path = loop {
        let mut path = ask(input, "Output file", DEFAULT_FILE);
        if !path.to_ascii_lowercase().ends_with(".wav") {
            path.push_str(".wav");
        }
        if Path::new(&path).exists() {
            let yes = ask(input, &format!("  {path} exists. Overwrite? (y/n)"), "n");
            if !yes.eq_ignore_ascii_case("y") && !yes.eq_ignore_ascii_case("yes") {
                continue;
            }
        }
        break path;
    };
    let seconds = ask_until(
        input,
        "Length (seconds, m:ss or h:mm:ss)",
        &format_length(DEFAULT_SECONDS),
        "Enter a length above 0 and up to 4:00:00, e.g. 600, 10:00 or 1:00:00.",
        parse_length,
    );
    println!();
    (path, seconds)
}

/// The fade at the end of a fixed-length render: raised-cosine, so it eases
/// out of full level and settles gently into silence.
#[derive(Clone, Copy, Debug)]
pub struct Fade {
    total: u64,
    len: u64,
}

impl Fade {
    /// Fade for a render of `total` frames.
    pub fn new(total: u64) -> Self {
        let len = ((FADE_OUT_SECONDS * SAMPLE_RATE as f64) as u64).min(total / 4).max(1);
        Fade { total, len }
    }

    /// A short fade starting at frame `from`, for stopping early.
    pub fn early(from: u64, seconds: f64) -> Self {
        let len = ((seconds * SAMPLE_RATE as f64) as u64).max(1);
        Fade { total: from + len, len }
    }

    pub fn total(&self) -> u64 {
        self.total
    }

    pub fn seconds(&self) -> f64 {
        self.len as f64 / SAMPLE_RATE as f64
    }

    /// Gain for frame `pos` (0 at and after the end).
    pub fn gain(&self, pos: u64) -> f64 {
        let remaining = self.total.saturating_sub(pos);
        if remaining >= self.len {
            1.0
        } else {
            0.5 - 0.5 * cos_turns(0.5 * remaining as f64 / self.len as f64)
        }
    }
}

/// Text tags written into a WAV file's LIST/INFO chunk (shown by most
/// players and by tools like ffprobe as title, genre, encoder and comment).
#[derive(Clone, Debug)]
pub struct WavInfo {
    pub title: String,
    pub software: String,
    pub comment: String,
}

/// The tags for a render: the recipe and everything it stands for,
/// so any file explains how to regenerate itself. Nothing time-dependent is
/// included, so identical renders stay byte-identical.
pub fn wav_info(title: &str, software: &str, seeds: Seeds, settings: Settings) -> WavInfo {
    let code = recipe(seeds, settings);
    let key = crate::harmony::key_name(crate::harmony::key_for(seeds.s1));
    let pace = if settings.pace == Pace::Jungle { "jungle" } else { "half-time" };
    let drums = if seeds.s5 == 0 { "off".to_string() } else { format!("{} kit", settings.kit.name()) };
    WavInfo {
        title: title.to_string(),
        software: software.to_string(),
        comment: format!(
            "GlitchAmbiToolkit track {TRACK}. Recipe {code}. {key} {}, {} chord(s) ({}), {pace} pace. \
             Seeds {} {} {} {} {} (drums {drums}). Regenerate with: --recipe {code}",
            settings.scale.name(),
            settings.chords,
            bars_text(settings.chords),
            seeds.s1,
            seeds.s2,
            seeds.s3,
            seeds.s4,
            seeds.s5,
        ),
    }
}

fn info_chunk(info: &WavInfo) -> Vec<u8> {
    let mut body = b"INFO".to_vec();
    for (id, text) in [
        (b"INAM", info.title.as_str()),
        (b"IGNR", "Glitch Ambient"),
        (b"ISFT", info.software.as_str()),
        (b"ICMT", info.comment.as_str()),
    ] {
        let mut data = text.as_bytes().to_vec();
        data.push(0); // INFO strings are zero-terminated
        body.extend_from_slice(id);
        body.extend_from_slice(&(data.len() as u32).to_le_bytes());
        body.extend_from_slice(&data);
        if data.len() % 2 == 1 {
            body.push(0); // chunks are word-aligned
        }
    }
    let mut chunk = b"LIST".to_vec();
    chunk.extend_from_slice(&(body.len() as u32).to_le_bytes());
    chunk.extend_from_slice(&body);
    chunk
}

/// Streams 24-bit stereo PCM to a WAV file; the header's sizes are patched on `finish`.
pub struct WavWriter {
    w: BufWriter<File>,
    dither: Rng,
    frames: u64,
    info: Vec<u8>,
}

impl WavWriter {
    pub fn create(path: &str, info: &WavInfo) -> io::Result<Self> {
        let mut w = BufWriter::new(File::create(path)?);
        let info = info_chunk(info);
        write_header(&mut w, 0, &info)?;
        Ok(WavWriter { w, dither: Rng::new(0xD17E), frames: 0, info })
    }

    /// Appends interleaved stereo samples, multiplied by `gain(frame index)`.
    pub fn write(&mut self, samples: &[f32], gain: impl Fn(u64) -> f64) -> io::Result<()> {
        let mut bytes = Vec::with_capacity(samples.len() * 3);
        for (i, s) in samples.iter().enumerate() {
            let g = gain(self.frames + (i / 2) as u64);
            // TPDF dither of ±1 LSB.
            let d = self.dither.unit() - self.dither.unit();
            let v = (*s as f64 * g * 8_388_607.0 + d).round().clamp(-8_388_608.0, 8_388_607.0);
            bytes.extend_from_slice(&(v as i32).to_le_bytes()[..3]);
        }
        self.frames += (samples.len() / 2) as u64;
        self.w.write_all(&bytes)
    }

    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Writes the final sizes into the header and closes the file.
    pub fn finish(mut self) -> io::Result<u64> {
        let header = 12 + 24 + self.info.len() as u64 + 8;
        self.w.seek(SeekFrom::Start(0))?;
        write_header(&mut self.w, self.frames * 6, &self.info)?;
        self.w.seek(SeekFrom::Start(header + self.frames * 6))?;
        self.w.flush()?;
        Ok(self.frames)
    }
}

fn write_header(w: &mut impl Write, data_bytes: u64, info: &[u8]) -> io::Result<()> {
    w.write_all(b"RIFF")?;
    w.write_all(&((4 + 24 + info.len() as u64 + 8 + data_bytes) as u32).to_le_bytes())?;
    w.write_all(b"WAVEfmt ")?;
    w.write_all(&16u32.to_le_bytes())?;
    w.write_all(&1u16.to_le_bytes())?; // PCM
    w.write_all(&2u16.to_le_bytes())?; // stereo
    w.write_all(&SAMPLE_RATE.to_le_bytes())?;
    w.write_all(&(SAMPLE_RATE * 2 * 3).to_le_bytes())?;
    w.write_all(&6u16.to_le_bytes())?;
    w.write_all(&24u16.to_le_bytes())?;
    w.write_all(info)?;
    w.write_all(b"data")?;
    w.write_all(&(data_bytes as u32).to_le_bytes())
}

/// Renders `seconds` of the track (with the end fade) to `path`, as fast as
/// possible, printing progress every minute of audio.
pub fn render_to_wav(seeds: Seeds, settings: Settings, path: &str, seconds: f64, info: &WavInfo) -> io::Result<()> {
    let total = (seconds * SAMPLE_RATE as f64) as u64;
    let fade = Fade::new(total);
    eprintln!(
        "0010: rendering {} in {}, {} chord(s), seeds {} {} {} {} {}, fading out over the last {:.0} s -> {path}",
        format_length(seconds),
        settings.scale.name(),
        settings.chords,
        seeds.s1,
        seeds.s2,
        seeds.s3,
        seeds.s4,
        seeds.s5,
        fade.seconds(),
    );
    let minute = SAMPLE_RATE as u64 * 60;
    let mut last = 0;
    render_with_progress(seeds, settings, path, seconds, info, |done, _| {
        if done / minute > last / minute {
            let sr = SAMPLE_RATE as f64;
            eprintln!("  {} / {}", format_length(done as f64 / sr), format_length(seconds));
        }
        last = done;
        true
    })?;
    eprintln!("done. Recipe: {}", recipe(seeds, settings));
    Ok(())
}

/// Renders like `render_to_wav`, silently, calling `progress(frames done,
/// frames total)` after every chunk. If `progress` returns false the render
/// stops early, leaving a valid but shorter file; returns whether it completed.
pub fn render_with_progress(
    seeds: Seeds,
    settings: Settings,
    path: &str,
    seconds: f64,
    info: &WavInfo,
    mut progress: impl FnMut(u64, u64) -> bool,
) -> io::Result<bool> {
    const CHUNK_FRAMES: usize = 4096;
    let total = (seconds * SAMPLE_RATE as f64) as u64;
    let fade = Fade::new(total);
    let mut wav = WavWriter::create(path, info)?;
    let mut track = Track::new(seeds, settings);
    let mut buf = vec![0f32; CHUNK_FRAMES * 2];
    while wav.frames() < total {
        let frames = ((total - wav.frames()) as usize).min(CHUNK_FRAMES);
        let chunk = &mut buf[..frames * 2];
        track.render(chunk);
        wav.write(chunk, |pos| fade.gain(pos))?;
        if !progress(wav.frames(), total) {
            wav.finish()?;
            return Ok(false);
        }
    }
    wav.finish()?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harmony::SCALES;

    #[test]
    fn recipes_round_trip() {
        assert_eq!(recipe(DEFAULT_SEEDS, DEFAULT_SETTINGS), "0010-LYD-3H-ACO-1000.9.1009.2026.168");
        // Older recipes without a kit code still read, with the kit seed 5 used to pick.
        let (seeds, settings) = parse_recipe("0010-LYD-3H-1000.9.1009.2026.168").unwrap();
        assert_eq!((seeds, settings), (DEFAULT_SEEDS, DEFAULT_SETTINGS));
        for (i, scale) in SCALES.iter().enumerate() {
            let settings = Settings {
                scale: *scale,
                chords: (i % 4) as u8 + 1,
                pace: if i % 2 == 0 { Pace::Jungle } else { Pace::HalfTime },
                kit: KITS[i % KITS.len()],
            };
            let seeds = Seeds { s1: i as u64, s2: u64::MAX, s3: 0, s4: 42, s5: 7 };
            let code = recipe(seeds, settings);
            assert_eq!(parse_recipe(&code), Some((seeds, settings)), "{code}");
            assert_eq!(parse_recipe(&code.to_ascii_lowercase()), Some((seeds, settings)));
        }
        for bad in ["", "0009-LYD-3H-1.2.3.4.5", "0010-XXX-3H-1.2.3.4.5", "0010-LYD-5H-1.2.3.4.5", "0010-LYD-3Q-1.2.3.4.5", "0010-LYD-3H-1.2.3.4", "0010-LYD-3H-1.2.3.4.x", "0010-LYD-3H-XYZ-1.2.3.4.5"] {
            assert_eq!(parse_recipe(bad), None, "{bad}");
        }
    }
}
