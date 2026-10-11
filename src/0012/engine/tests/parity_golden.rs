//! Golden hashes for the *coupled* artifact (music + weather + visuals), and
//! the reference for the browser parity check (`web/verify.mjs`).
//!
//! `audio_golden.rs` locks the ported 0011 engine alone. This locks what 0012
//! adds on top: the weather bed in the audio, and the rendered frames.

use shrine0012::framebuffer::Framebuffer;
use shrine0012::scene::Quality;
use shrine0012::sim::{Sim, SimConfig};
use shrine0012::SAMPLE_RATE;

fn hash_f32(samples: &[f32]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for s in samples {
        for b in s.to_bits().to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

const AUDIO_SECONDS: usize = 5;
const AUDIO_GOLDEN: u64 = 0xddb4_7280_5251_6eb7;
const VISUAL_GOLDEN: [u64; 5] = [
    16372517786461568741,
    9300958290172192542,
    3436581339193562279,
    3770431886702531045,
    7701800233315402307,
];

#[test]
fn coupled_audio_golden() {
    let mut sim = Sim::new(SimConfig::default());
    let frames = AUDIO_SECONDS * SAMPLE_RATE as usize;
    let mut out = vec![0f32; frames * 2];
    for chunk in out.chunks_mut(4096 * 2) {
        sim.render_audio(chunk);
    }
    let h = hash_f32(&out);
    println!("COUPLED_AUDIO_GOLDEN = {h:#018x}");
    assert_eq!(h, AUDIO_GOLDEN, "the coupled audio changed (got {h:#018x})");
}

#[test]
fn visual_frames_golden() {
    let sim = Sim::new_visual(SimConfig::default());
    let mut hashes = [0u64; 5];
    for (i, t) in [0.0, 1.0, 2.0, 3.0, 4.0].into_iter().enumerate() {
        let mut fb = Framebuffer::logical();
        sim.render_visual(&mut fb, t, Quality::High);
        hashes[i] = hash_bytes(fb.as_bytes());
    }
    println!("VISUAL_GOLDEN = {hashes:?}");
    assert_eq!(hashes, VISUAL_GOLDEN, "the rendered frames changed");
}
