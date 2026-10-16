//! Golden hashes for the *coupled* artifact (music + weather + visuals), and
//! the reference for the browser parity check (`web/verify.mjs`).
//!
//! `audio_golden.rs` locks the ported 0011 engine alone. This locks what 0012
//! adds on top: the weather bed in the audio, and the rendered frames.

use shrine0014::framebuffer::Framebuffer;
use shrine0014::scene::Quality;
use shrine0014::sim::{Sim, SimConfig};
use shrine0014::SAMPLE_RATE;

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
// Re-locked for the weather rework (FEAT-40): the default config is Rain,
// so its layered rain, splashes, sheen and grade change every frame, and the
// weather bed's new wash, splash voices and gusting wind change the audio.
const AUDIO_GOLDEN: u64 = 0x31c6_0f66_e239_2f4d;
/// The drone rhythm switched on: locked so the LFO cannot drift silently
/// (`HC004`). Off, the coupled audio above is unchanged.
const DRONE_RHYTHM_GOLDEN: u64 = 0xc214_9db9_b67b_caa1;
/// The same rhythm also shaping the atmosphere loops.
const LOOP_RHYTHM_GOLDEN: u64 = 0x9a1a_fd56_42f7_4247;
/// A deliberately turned architecture, locked so a change to the transform
/// maths cannot pass silently (`HC004`).
// Re-locked for the textured, lit architecture (FEAT-44), its paler
// palette with the white altar (FEAT-45), and the depth test on the turned
// path (BUG-11).
const ARCH_ROTATED_GOLDEN: [u64; 3] = [5606981425953137516, 190012420838077787, 17860756258834833913];
const VISUAL_GOLDEN: [u64; 5] = [
    // Re-locked when `raster::fill_face` was fixed (it drew side faces with x
    // and y swapped), for the weather rework (FEAT-40), for the textured, lit
    // architecture (FEAT-44), and for its paler palette (FEAT-45).
    13428862637609170673,
    16029147735766487710,
    436013470198669323,
    17141771079268836140,
    7056076257641306088,
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
fn drone_rhythm_golden() {
    let mut config = SimConfig::default();
    config.settings.drone_rhythm = true;
    let mut sim = Sim::new(config);
    let frames = AUDIO_SECONDS * SAMPLE_RATE as usize;
    let mut out = vec![0f32; frames * 2];
    for chunk in out.chunks_mut(4096 * 2) {
        sim.render_audio(chunk);
    }
    let h = hash_f32(&out);
    println!("DRONE_RHYTHM_GOLDEN = {h:#018x}");
    assert_ne!(h, AUDIO_GOLDEN, "the rhythm must actually reach the drone");
    assert_eq!(h, DRONE_RHYTHM_GOLDEN, "the drone rhythm changed");
}

#[test]
fn loop_rhythm_golden() {
    let mut config = SimConfig::default();
    config.settings.loop_rhythm = true;
    let mut sim = Sim::new(config);
    let frames = AUDIO_SECONDS * SAMPLE_RATE as usize;
    let mut out = vec![0f32; frames * 2];
    for chunk in out.chunks_mut(4096 * 2) {
        sim.render_audio(chunk);
    }
    let h = hash_f32(&out);
    println!("LOOP_RHYTHM_GOLDEN = {h:#018x}");
    assert_ne!(h, AUDIO_GOLDEN, "the rhythm must reach the loops");
    assert_ne!(h, DRONE_RHYTHM_GOLDEN, "and must differ from the drone-only case");
    assert_eq!(h, LOOP_RHYTHM_GOLDEN, "the loop rhythm changed");
}

#[test]
fn rotated_architecture_golden() {
    let mut config = SimConfig::default();
    config.settings.arch =
        shrine0014::arch::ArchTransform { rot_x: 12, rot_y: 0, rot_z: 35, off_x: 0, off_y: 0, off_z: 0 };
    let sim = Sim::new_visual(config);
    let mut hashes = [0u64; 3];
    for (i, t) in [0.0, 1.0, 2.0].into_iter().enumerate() {
        let mut fb = Framebuffer::logical();
        sim.render_visual(&mut fb, t, Quality::High);
        hashes[i] = hash_bytes(fb.as_bytes());
    }
    println!("ARCH_ROTATED_GOLDEN = {hashes:?}");
    assert_eq!(hashes, ARCH_ROTATED_GOLDEN, "the rotated architecture changed");
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
