//! Shared pieces of the native front-ends (`render-0009` and the `shrine-0009`
//! app): terminal prompts, the end-of-file fade, and the WAV writer.
//! Standard library only, like the rest of the engine.

use crate::math::cos_turns;
use crate::rng::Rng;
use crate::{Seeds, Track, DEFAULT_SEEDS, SAMPLE_RATE};
use std::fs::File;
use std::io::{self, BufRead, BufWriter, Seek, SeekFrom, Write};
use std::path::Path;

pub const DEFAULT_SECONDS: f64 = 600.0;
pub const DEFAULT_FILE: &str = "0009.wav";
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

/// Streams 24-bit stereo PCM to a WAV file; the header's sizes are patched on `finish`.
pub struct WavWriter {
    w: BufWriter<File>,
    dither: Rng,
    frames: u64,
}

const HEADER_BYTES: u64 = 44;

impl WavWriter {
    pub fn create(path: &str) -> io::Result<Self> {
        let mut w = BufWriter::new(File::create(path)?);
        write_header(&mut w, 0)?;
        Ok(WavWriter { w, dither: Rng::new(0xD17E), frames: 0 })
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
        self.w.seek(SeekFrom::Start(0))?;
        write_header(&mut self.w, self.frames * 6)?;
        self.w.seek(SeekFrom::Start(HEADER_BYTES + self.frames * 6))?;
        self.w.flush()?;
        Ok(self.frames)
    }
}

fn write_header(w: &mut impl Write, data_bytes: u64) -> io::Result<()> {
    w.write_all(b"RIFF")?;
    w.write_all(&((36 + data_bytes) as u32).to_le_bytes())?;
    w.write_all(b"WAVEfmt ")?;
    w.write_all(&16u32.to_le_bytes())?;
    w.write_all(&1u16.to_le_bytes())?; // PCM
    w.write_all(&2u16.to_le_bytes())?; // stereo
    w.write_all(&SAMPLE_RATE.to_le_bytes())?;
    w.write_all(&(SAMPLE_RATE * 2 * 3).to_le_bytes())?;
    w.write_all(&6u16.to_le_bytes())?;
    w.write_all(&24u16.to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&(data_bytes as u32).to_le_bytes())
}

/// Renders `seconds` of the track (with the end fade) to `path`, as fast as
/// possible, printing progress every minute of audio.
pub fn render_to_wav(seeds: Seeds, path: &str, seconds: f64) -> io::Result<()> {
    let total = (seconds * SAMPLE_RATE as f64) as u64;
    let fade = Fade::new(total);
    eprintln!(
        "0009: rendering {} with seeds {} {} {} {}, fading out over the last {:.0} s -> {path}",
        format_length(seconds),
        seeds.s1,
        seeds.s2,
        seeds.s3,
        seeds.s4,
        fade.seconds(),
    );
    let minute = SAMPLE_RATE as u64 * 60;
    let mut last = 0;
    render_with_progress(seeds, path, seconds, |done, _| {
        if done / minute > last / minute {
            let sr = SAMPLE_RATE as f64;
            eprintln!("  {} / {}", format_length(done as f64 / sr), format_length(seconds));
        }
        last = done;
        true
    })?;
    eprintln!("done.");
    Ok(())
}

/// Renders like `render_to_wav`, silently, calling `progress(frames done,
/// frames total)` after every chunk. If `progress` returns false the render
/// stops early, leaving a valid but shorter file; returns whether it completed.
pub fn render_with_progress(
    seeds: Seeds,
    path: &str,
    seconds: f64,
    mut progress: impl FnMut(u64, u64) -> bool,
) -> io::Result<bool> {
    const CHUNK_FRAMES: usize = 4096;
    let total = (seconds * SAMPLE_RATE as f64) as u64;
    let fade = Fade::new(total);
    let mut wav = WavWriter::create(path)?;
    let mut track = Track::new(seeds);
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
