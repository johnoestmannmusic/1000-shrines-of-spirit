//! Renders 0009 to a 24-bit stereo WAV file.
//!
//!     render-0009 [--seconds N] [--out FILE] [--seed1 N] [--seed2 N] [--seed3 N] [--seed4 N]
//!
//! The piece itself is endless; `--seconds` decides how much of it to capture
//! (a fade-out is applied to the end of the file only).

use shrine0009::rng::Rng;
use shrine0009::{Track, DEFAULT_SEEDS, SAMPLE_RATE};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::process::exit;

const FADE_OUT_SECONDS: f64 = 12.0;
const CHUNK_FRAMES: usize = 4096;

fn parse_u64(s: &str) -> Option<u64> {
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}

fn usage(msg: &str) -> ! {
    eprintln!("{msg}");
    eprintln!(
        "usage: render-0009 [--seconds N] [--out FILE] [--seed1 N] [--seed2 N] [--seed3 N] [--seed4 N]"
    );
    exit(2);
}

fn main() {
    let mut seconds = 600.0;
    let mut out_path = String::from("0009.wav");
    let mut seeds = DEFAULT_SEEDS;
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let value = args.next().unwrap_or_else(|| usage(&format!("missing value for {flag}")));
        let seed = || parse_u64(&value).unwrap_or_else(|| usage(&format!("bad seed: {value}")));
        match flag.as_str() {
            "--seconds" => {
                seconds = value.parse().unwrap_or_else(|_| usage(&format!("bad seconds: {value}")))
            }
            "--out" => out_path = value.clone(),
            "--seed1" => seeds.s1 = seed(),
            "--seed2" => seeds.s2 = seed(),
            "--seed3" => seeds.s3 = seed(),
            "--seed4" => seeds.s4 = seed(),
            _ => usage(&format!("unknown option: {flag}")),
        }
    }
    if !(seconds > 0.0) {
        usage("--seconds must be positive");
    }

    let total = (seconds * SAMPLE_RATE as f64) as u64;
    let fade = ((FADE_OUT_SECONDS * SAMPLE_RATE as f64) as u64).min(total / 4).max(1);
    eprintln!(
        "0009: rendering {seconds} s with seeds {} {} {} {} → {out_path}",
        seeds.s1, seeds.s2, seeds.s3, seeds.s4
    );

    let file = File::create(&out_path).unwrap_or_else(|e| usage(&format!("{out_path}: {e}")));
    let mut w = BufWriter::new(file);
    if let Err(e) = write_wav(&mut w, seeds, total, fade) {
        eprintln!("write failed: {e}");
        exit(1);
    }
}

fn write_wav(w: &mut impl Write, seeds: shrine0009::Seeds, total: u64, fade: u64) -> std::io::Result<()> {
    let data_bytes = total * 2 * 3;
    let header = |w: &mut dyn Write| -> std::io::Result<()> {
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
    };
    header(w)?;

    let mut track = Track::new(seeds);
    let mut dither = Rng::new(0xD17E);
    let mut buf = vec![0f32; CHUNK_FRAMES * 2];
    let mut written = 0u64;
    let mut bytes = Vec::with_capacity(CHUNK_FRAMES * 6);
    while written < total {
        let frames = ((total - written) as usize).min(CHUNK_FRAMES);
        let chunk = &mut buf[..frames * 2];
        track.render(chunk);
        bytes.clear();
        for (i, s) in chunk.iter().enumerate() {
            let pos = written + (i / 2) as u64;
            let remaining = total - pos;
            let gain = if remaining < fade { remaining as f64 / fade as f64 } else { 1.0 };
            // TPDF dither of ±1 LSB.
            let d = dither.unit() - dither.unit();
            let v = (*s as f64 * gain * 8_388_607.0 + d).round().clamp(-8_388_608.0, 8_388_607.0);
            bytes.extend_from_slice(&(v as i32).to_le_bytes()[..3]);
        }
        w.write_all(&bytes)?;
        written += frames as u64;
        if written % (SAMPLE_RATE as u64 * 60) < frames as u64 {
            eprintln!("  {} min", written / (SAMPLE_RATE as u64 * 60));
        }
    }
    w.flush()
}
