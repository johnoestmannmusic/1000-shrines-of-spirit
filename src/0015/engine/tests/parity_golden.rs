//! Golden hashes for the *coupled* artifact (music + weather + visuals), and
//! the reference for the browser parity check (`web/verify.mjs`).
//!
//! `audio_golden.rs` locks the ported 0011 engine alone. This locks what 0012
//! adds on top: the weather bed in the audio, and the rendered frames.

use shrine0015::framebuffer::Framebuffer;
use shrine0015::scene::Quality;
use shrine0015::sim::{Sim, SimConfig};
use shrine0015::SAMPLE_RATE;

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
// Re-locked in 0014 when the weather bed became block-independent: each
// splash, drip and crackle now draws its own randomness and starts on its
// exact sample, so the browser's 128-frame blocks and the exporters' 4096
// give the same samples (and a start offset is exact). The music is unchanged.
const AUDIO_GOLDEN: u64 = 0x4431_b256_53cb_da5c;
/// The drone rhythm switched on: locked so the LFO cannot drift silently
/// (`HC004`). Off, the coupled audio above is unchanged.
const DRONE_RHYTHM_GOLDEN: u64 = 0x1dd1_af04_2c30_7b07;
/// The same rhythm also shaping the atmosphere loops.
const LOOP_RHYTHM_GOLDEN: u64 = 0xf885_744b_4745_15b7;
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
        shrine0015::arch::ArchTransform { rot_x: 12, rot_y: 0, rot_z: 35, off_x: 0, off_y: 0, off_z: 0 };
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

// ---------------------------------------------------------------- 0014

/// 0014's additions, locked in their own goldens: each built future in a
/// weather, playing 2-step garage. The same recipes and hashes are written to
/// `web/parity-futures.json`, so `web/verify.mjs` proves the browser matches.
/// Regenerate the file with `SHRINE_WRITE_FIXTURE=1 cargo test --test parity_golden`.
const FUTURE_AUDIO_SECONDS: usize = 36;
const FUTURE_FRAMES: [f64; 2] = [0.0, 3.0];
// Re-locked when garage's layers stopped clashing: an open-fifth drone,
// whole-number overtones, separate registers and clean chord changes.
const FUTURE_GOLDEN: [(u64, [u64; 2]); 4] = [
    // Re-locked when garage was tightened: the loops change on the downbeat
    // with the bass, every echo keeps its hit's place in the swing, the loops'
    // grains and the glitch ratchets swing with the drums, and the bass glides
    // faster.
    (0x49f2_ebac_73cd_214c, [6579003113373532395, 13247657374849648781]),
    (0x8644_1f47_3bc4_2566, [13912267995583014935, 17051752384514558235]),
    (0xb37b_73ba_d941_6b80, [3398551927623433174, 7995802616033956358]),
    (0xec32_597d_0672_f60a, [15606822369550803988, 16286219466886564680]),
];

fn future_configs() -> Vec<(&'static str, SimConfig)> {
    use shrine0015::plants::Biome;
    use shrine0015::weather::WeatherKind;
    let settings = shrine0015::Settings {
        pace: shrine0015::harmony::Pace::Garage,
        scale: shrine0015::harmony::Scale::Ionian,
        chords: 4,
        bpm: 132,
        ..shrine0015::DEFAULT_SETTINGS
    };
    [
        ("near-future clear", Biome::NearFuture, WeatherKind::Clear),
        ("research-lab storm", Biome::ResearchLab, WeatherKind::Storm),
        ("ecofuture snow", Biome::Ecofuture, WeatherKind::Snow),
        ("greenhouse rain", Biome::Greenhouse, WeatherKind::Rain),
    ]
    .into_iter()
    .map(|(name, biome, weather)| (name, SimConfig { biome, weather, settings, ..SimConfig::default() }))
    .collect()
}

fn future_hashes(config: &SimConfig) -> (u64, [u64; 2]) {
    let mut sim = Sim::new(config.clone());
    let frames = FUTURE_AUDIO_SECONDS * SAMPLE_RATE as usize;
    let mut out = vec![0f32; frames * 2];
    for chunk in out.chunks_mut(4096 * 2) {
        sim.render_audio(chunk);
    }
    let visual = Sim::new_visual(config.clone());
    let mut frames_h = [0u64; 2];
    for (i, t) in FUTURE_FRAMES.into_iter().enumerate() {
        let mut fb = Framebuffer::logical();
        visual.render_visual(&mut fb, t, Quality::High);
        frames_h[i] = hash_bytes(fb.as_bytes());
    }
    (hash_f32(&out), frames_h)
}

#[test]
fn the_futures_and_garage_golden() {
    let mut json = String::from("{\n  \"audio_seconds\": ");
    json.push_str(&format!("{FUTURE_AUDIO_SECONDS},\n  \"frames\": [{}],\n  \"configs\": [\n", FUTURE_FRAMES.iter().map(|t| format!("{t}")).collect::<Vec<_>>().join(", ")));
    let mut got = Vec::new();
    for (i, (name, config)) in future_configs().iter().enumerate() {
        let (audio, frames) = future_hashes(config);
        println!("{name}: ({audio:#018x}, {frames:?})");
        got.push((audio, frames));
        let words = shrine0015::recipe::encode(config).iter().map(|w| w.to_string()).collect::<Vec<_>>().join(",");
        json.push_str(&format!(
            "    {{ \"name\": \"{name}\", \"words\": [{words}], \"audio\": \"{audio:016x}\", \"visual\": [{}] }}{}\n",
            frames.iter().map(|h| format!("\"{h:016x}\"")).collect::<Vec<_>>().join(", "),
            if i + 1 < 4 { "," } else { "" }
        ));
    }
    json.push_str("  ]\n}\n");
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../web/parity-futures.json");
    if std::env::var("SHRINE_WRITE_FIXTURE").is_ok() {
        std::fs::write(path, &json).unwrap();
    }
    assert_eq!(got, FUTURE_GOLDEN.to_vec(), "a future or the garage mode changed");
    assert_eq!(std::fs::read_to_string(path).ok().as_deref(), Some(json.as_str()), "web/parity-futures.json is stale");
}
