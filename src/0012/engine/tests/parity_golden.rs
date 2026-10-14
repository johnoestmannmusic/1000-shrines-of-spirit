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
/// The drone rhythm switched on: locked so the LFO cannot drift silently
/// (`HC004`). Off, the coupled audio above is unchanged.
const DRONE_RHYTHM_GOLDEN: u64 = 0xcbc3_d7a7_dda6_25e1;
/// The same rhythm also shaping the atmosphere loops.
const LOOP_RHYTHM_GOLDEN: u64 = 0x2f7f_7134_86c0_ac44;
/// A deliberately turned architecture, locked so a change to the transform
/// maths cannot pass silently (`HC004`).
const ARCH_ROTATED_GOLDEN: [u64; 3] = [15091846030116639487, 12992143712555696799, 12703277274131942356];
const VISUAL_GOLDEN: [u64; 5] = [
    // Re-locked when `raster::fill_face` was fixed: it had called the horizontal
    // `fill_span`, drawing every column's side faces with x and y swapped.
    8079878040143304953,
    3435403895262894872,
    13197474975392812766,
    412205298506576578,
    4611702382427217164,
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
        shrine0012::arch::ArchTransform { rot_x: 12, rot_y: 0, rot_z: 35, off_x: 0, off_y: 0, off_z: 0 };
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
