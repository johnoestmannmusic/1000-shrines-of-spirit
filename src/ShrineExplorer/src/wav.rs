//! WAV writing, with metadata and embedded cover art.
//!
//! Host-only. Two writers:
//! * [`write_stereo_16`] — a plain 16-bit PCM file (script renders, quick saves).
//! * [`write_export`] — the same audio plus a RIFF `LIST/INFO` block and an
//!   ID3v2.3 tag carrying the track/artist/album, the recipe in the comment,
//!   and a PNG cover in an `APIC` frame.

use std::fs;
use std::io;
use std::path::Path;

/// Metadata baked into an exported WAV.
#[derive(Clone, Debug, Default)]
pub struct WavMeta {
    pub sample_rate: u32,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Free text: the recipe card, ShrineGen version included.
    pub comment: String,
    pub software: String,
    /// A PNG cover image (e.g. 1600x1600).
    pub cover_png: Vec<u8>,
}

/// The options chosen on the export screen.
#[derive(Clone, Debug)]
pub struct WavOptions {
    pub length_s: f64,
    pub fade_s: f64,
    pub filename: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub normalize: bool,
}

/// Write interleaved stereo `samples` (in `[-1, 1]`) as a plain 16-bit PCM WAV.
pub fn write_stereo_16(path: &str, sample_rate: u32, samples: &[f32]) -> io::Result<()> {
    let fmt = fmt_chunk(sample_rate);
    let data = pcm16(samples);
    let mut chunks = Vec::new();
    chunks.extend_from_slice(b"WAVE");
    riff_chunk(&mut chunks, b"fmt ", &fmt);
    riff_chunk(&mut chunks, b"data", &data);
    write_file(path, &wrap_riff(chunks))
}

/// Write an exported WAV: audio plus `LIST/INFO` metadata and an ID3v2.3 tag
/// with the cover art.
pub fn write_export(path: &str, samples: &[f32], meta: &WavMeta) -> io::Result<()> {
    let mut chunks = Vec::new();
    chunks.extend_from_slice(b"WAVE");
    riff_chunk(&mut chunks, b"fmt ", &fmt_chunk(meta.sample_rate));
    riff_chunk(&mut chunks, b"LIST", &info_block(meta));
    riff_chunk(&mut chunks, b"data", &pcm16(samples));
    let tag = id3_tag(meta);
    if !tag.is_empty() {
        riff_chunk(&mut chunks, b"id3 ", &tag);
    }
    write_file(path, &wrap_riff(chunks))
}

fn fmt_chunk(sample_rate: u32) -> Vec<u8> {
    let channels: u16 = 2;
    let bits: u16 = 16;
    let mut fmt = Vec::with_capacity(16);
    fmt.extend_from_slice(&1u16.to_le_bytes()); // PCM
    fmt.extend_from_slice(&channels.to_le_bytes());
    fmt.extend_from_slice(&sample_rate.to_le_bytes());
    fmt.extend_from_slice(&(sample_rate * channels as u32 * (bits as u32 / 8)).to_le_bytes());
    fmt.extend_from_slice(&(channels * (bits / 8)).to_le_bytes());
    fmt.extend_from_slice(&bits.to_le_bytes());
    fmt
}

fn pcm16(samples: &[f32]) -> Vec<u8> {
    let mut data = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        data.extend_from_slice(&v.to_le_bytes());
    }
    data
}

fn wrap_riff(chunks: Vec<u8>) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + chunks.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(chunks.len() as u32).to_le_bytes());
    out.extend_from_slice(&chunks);
    out
}

fn riff_chunk(out: &mut Vec<u8>, id: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(id);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    if data.len() % 2 == 1 {
        out.push(0);
    }
}

fn info_text(body: &mut Vec<u8>, id: &[u8; 4], text: &str) {
    let mut data = text.as_bytes().to_vec();
    data.push(0);
    riff_chunk(body, id, &data);
}

fn info_block(meta: &WavMeta) -> Vec<u8> {
    let mut body = b"INFO".to_vec();
    info_text(&mut body, b"INAM", &meta.title);
    info_text(&mut body, b"IART", &meta.artist);
    info_text(&mut body, b"IPRD", &meta.album);
    info_text(&mut body, b"ICMT", &meta.comment);
    info_text(&mut body, b"ISFT", &meta.software);
    body
}

fn write_file(path: &str, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = Path::new(path).parent() {
        if !dir.as_os_str().is_empty() {
            fs::create_dir_all(dir)?;
        }
    }
    fs::write(path, bytes)
}

// ---- ID3v2.3 -----------------------------------------------------------------

fn text_encoding(text: &str) -> (u8, Vec<u8>) {
    if text.chars().all(|c| (c as u32) <= 0xFF) {
        (0x00, text.chars().map(|c| c as u8).collect())
    } else {
        let mut bytes = vec![0xFF, 0xFE]; // UTF-16LE BOM
        for unit in text.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        (0x01, bytes)
    }
}

fn frame(out: &mut Vec<u8>, id: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(id);
    out.extend_from_slice(&(data.len() as u32).to_be_bytes()); // v2.3: plain size
    out.extend_from_slice(&[0, 0]); // flags
    out.extend_from_slice(data);
}

fn text_frame(out: &mut Vec<u8>, id: &[u8; 4], text: &str) {
    let (encoding, mut data) = text_encoding(text);
    let mut body = vec![encoding];
    body.append(&mut data);
    match encoding {
        0x00 => body.push(0),
        _ => body.extend_from_slice(&[0, 0]),
    }
    frame(out, id, &body);
}

fn comment_frame(out: &mut Vec<u8>, text: &str) {
    let (encoding, mut data) = text_encoding(text);
    let mut body = vec![encoding];
    body.extend_from_slice(b"eng");
    match encoding {
        0x00 => body.push(0),
        _ => body.extend_from_slice(&[0, 0]),
    }
    body.append(&mut data);
    match encoding {
        0x00 => body.push(0),
        _ => body.extend_from_slice(&[0, 0]),
    }
    frame(out, b"COMM", &body);
}

fn apic_frame(out: &mut Vec<u8>, png: &[u8]) {
    let mut body = vec![0x00];
    body.extend_from_slice(b"image/png");
    body.push(0);
    body.push(0x03); // front cover
    body.push(0); // empty description
    body.extend_from_slice(png);
    frame(out, b"APIC", &body);
}

fn syncsafe(value: u32) -> [u8; 4] {
    [(value >> 21) as u8 & 0x7F, (value >> 14) as u8 & 0x7F, (value >> 7) as u8 & 0x7F, value as u8 & 0x7F]
}

fn id3_tag(meta: &WavMeta) -> Vec<u8> {
    let mut frames = Vec::new();
    text_frame(&mut frames, b"TIT2", &meta.title);
    text_frame(&mut frames, b"TPE1", &meta.artist);
    text_frame(&mut frames, b"TALB", &meta.album);
    comment_frame(&mut frames, &meta.comment);
    if !meta.cover_png.is_empty() {
        apic_frame(&mut frames, &meta.cover_png);
    }
    if frames.is_empty() {
        return Vec::new();
    }
    let mut tag = Vec::with_capacity(10 + frames.len());
    tag.extend_from_slice(b"ID3");
    tag.extend_from_slice(&[3, 0, 0]); // v2.3.0, no flags
    tag.extend_from_slice(&syncsafe(frames.len() as u32));
    tag.extend_from_slice(&frames);
    tag
}

// ---- shaping -----------------------------------------------------------------

/// Fade the first `seconds` in, so a render never starts with a click.
pub fn fade_in(samples: &mut [f32], sample_rate: u32, seconds: f64) {
    let frames = samples.len() / 2;
    let n = ((seconds * sample_rate as f64) as usize).min(frames);
    for i in 0..n {
        let g = i as f32 / n as f32;
        samples[i * 2] *= g;
        samples[i * 2 + 1] *= g;
    }
}

/// Fade out over the last `seconds`, reaching silence exactly at the end.
pub fn fade_out(samples: &mut [f32], sample_rate: u32, seconds: f64) {
    let frames = samples.len() / 2;
    let n = ((seconds * sample_rate as f64) as usize).min(frames);
    if n == 0 {
        return;
    }
    for i in 0..n {
        let g = 1.0 - (i + 1) as f32 / n as f32;
        let at = frames - n + i;
        samples[at * 2] *= g;
        samples[at * 2 + 1] *= g;
    }
}

/// Fade the first and last `seconds` of a buffer (the simple CLI path).
pub fn fade_edges(samples: &mut [f32], sample_rate: u32, seconds: f64) {
    fade_in(samples, sample_rate, seconds);
    fade_out(samples, sample_rate, seconds);
}

/// Scale so the loudest sample sits at `target` (0..1). No-op on silence.
pub fn normalize(samples: &mut [f32], target: f32) {
    let peak = samples.iter().fold(0f32, |a, s| a.max(s.abs()));
    if peak > 0.0 {
        let gain = target / peak;
        for s in samples.iter_mut() {
            *s *= gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> WavMeta {
        WavMeta {
            sample_rate: 48_000,
            title: "Isometric Shrine".into(),
            artist: "ShrineGen".into(),
            album: "ShrineExplorer".into(),
            comment: "ShrineGen v20261011\nseed 1000".into(),
            software: "ShrineGen v20261011".into(),
            cover_png: vec![0x89, b'P', b'N', b'G', 1, 2, 3, 4],
        }
    }

    #[test]
    fn plain_wav_header_is_well_formed() {
        let path = std::env::temp_dir().join("shrines-test-plain.wav");
        write_stereo_16(path.to_str().unwrap(), 48_000, &[0.0, 0.0, 0.5, -0.5]).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[12..16], b"fmt ");
        assert!(bytes.windows(4).any(|w| w == b"data"));
    }

    #[test]
    fn export_carries_info_and_id3_with_cover() {
        let path = std::env::temp_dir().join("shrines-test-export.wav");
        let samples = vec![0.0f32; 2000];
        write_export(path.to_str().unwrap(), &samples, &meta()).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert!(bytes.windows(4).any(|w| w == b"INFO"));
        assert!(bytes.windows(4).any(|w| w == b"INAM"));
        assert!(bytes.windows(4).any(|w| w == b"id3 "));
        assert!(bytes.windows(3).any(|w| w == b"ID3"));
        assert!(bytes.windows(4).any(|w| w == b"APIC"));
    }

    #[test]
    fn fade_out_reaches_silence_at_the_end() {
        let mut s = vec![1.0f32; 48_000 * 4]; // 2 seconds
        fade_out(&mut s, 48_000, 1.0);
        assert!((s[0] - 1.0).abs() < 1e-6, "the start is untouched");
        assert!(s[s.len() - 2].abs() < 1e-6, "the end is silent");
        assert!(s[s.len() - 1].abs() < 1e-6);
    }

    #[test]
    fn normalize_hits_the_target_peak() {
        let mut s = vec![0.1f32, -0.2, 0.05];
        normalize(&mut s, 0.99);
        let peak = s.iter().fold(0f32, |a, x| a.max(x.abs()));
        assert!((peak - 0.99).abs() < 1e-5);
    }
}
