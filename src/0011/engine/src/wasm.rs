//! Plain C-ABI exports for the browser player (no wasm-bindgen, no JS glue
//! generation), so the .wasm stays loadable with nothing but the standard
//! WebAssembly API.
//!
//!   const p = sos_new(s1hi, s1lo, … s6hi, s6lo, scale, chords, pace, kit, space, bpm,
//!                     key, transpose, drone_cycle, drone_hold, drone_tone,
//!                     l0_kind, l0_w0..l0_w3, l0_cycle, l0_hold, … l3_…);
//!   (scale: index into `harmony::SCALES`; chords: 1-4; pace: 0 half-time, 1 jungle;
//!    kit: index into `kits::KITS`; space: 0 centred, 1 wide, 2 wide + tape echo;
//!    bpm: the drum tempo, 70-180; key: 0 = seed picks the tonic, 1..12 = C..B;
//!    transpose: atmosphere semitones -12..12; drone_cycle/hold: the drone arc in
//!    seconds (cycle 0 = always on); drone_tone: drone low-pass Hz (0 = off);
//!    each layer is kind 0 = sustained timbre index in w0, or kind 1 = event grid
//!    with its 128 bits in w0..w3; lN_cycle/lN_hold: that layer's own arc)
//!   const ptr = sos_render(p, frames);  // → `frames` interleaved stereo f32s
//!
//! The returned pointer is valid until the next call; re-create any
//! Float32Array view after each call in case memory has grown.

use crate::harmony::{Pace, Scale};
use crate::texture::Grid;
use crate::{DroneArc, KeyChoice, LoopDesign, Seeds, Settings, Track};

pub struct Player {
    track: Track,
    buf: Vec<f32>,
}

fn join(hi: u32, lo: u32) -> u64 {
    ((hi as u64) << 32) | lo as u64
}

fn design(kind: u32, w: [u32; 4]) -> LoopDesign {
    if kind == 1 {
        LoopDesign::Events(Grid::from_words(w))
    } else {
        LoopDesign::Sustained(crate::atmos::LOOP_TIMBRES[w[0] as usize % crate::atmos::LOOP_TIMBRES.len()])
    }
}

fn arc(cycle: u32, hold: u32) -> DroneArc {
    let cycle = cycle.min(3600) as u16;
    DroneArc { cycle_s: cycle, hold_s: if cycle == 0 { 0 } else { (hold.min(cycle as u32)) as u16 } }
}

#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn sos_new(
    s1hi: u32,
    s1lo: u32,
    s2hi: u32,
    s2lo: u32,
    s3hi: u32,
    s3lo: u32,
    s4hi: u32,
    s4lo: u32,
    s5hi: u32,
    s5lo: u32,
    s6hi: u32,
    s6lo: u32,
    scale: u32,
    chords: u32,
    pace: u32,
    kit: u32,
    space: u32,
    bpm: u32,
    key: u32,
    loop_transpose: i32,
    drone_cycle: u32,
    drone_hold: u32,
    drone_tone: u32,
    l0_kind: u32,
    l0_w0: u32,
    l0_w1: u32,
    l0_w2: u32,
    l0_w3: u32,
    l0_cycle: u32,
    l0_hold: u32,
    l1_kind: u32,
    l1_w0: u32,
    l1_w1: u32,
    l1_w2: u32,
    l1_w3: u32,
    l1_cycle: u32,
    l1_hold: u32,
    l2_kind: u32,
    l2_w0: u32,
    l2_w1: u32,
    l2_w2: u32,
    l2_w3: u32,
    l2_cycle: u32,
    l2_hold: u32,
    l3_kind: u32,
    l3_w0: u32,
    l3_w1: u32,
    l3_w2: u32,
    l3_w3: u32,
    l3_cycle: u32,
    l3_hold: u32,
) -> *mut Player {
    let seeds = Seeds {
        s1: join(s1hi, s1lo),
        s2: join(s2hi, s2lo),
        s3: join(s3hi, s3lo),
        s4: join(s4hi, s4lo),
        s5: join(s5hi, s5lo),
        s6: join(s6hi, s6lo),
    };
    let settings = Settings {
        scale: Scale::from_index(scale as usize),
        chords: chords.clamp(1, 4) as u8,
        pace: if pace == 1 { Pace::Jungle } else { Pace::HalfTime },
        kit: crate::kits::KITS[kit as usize % crate::kits::KITS.len()],
        space: crate::kits::SPACES[space as usize % crate::kits::SPACES.len()],
        bpm: bpm.min(u16::MAX as u32) as u16,
        loops: [
            design(l0_kind, [l0_w0, l0_w1, l0_w2, l0_w3]),
            design(l1_kind, [l1_w0, l1_w1, l1_w2, l1_w3]),
            design(l2_kind, [l2_w0, l2_w1, l2_w2, l2_w3]),
            design(l3_kind, [l3_w0, l3_w1, l3_w2, l3_w3]),
        ],
        loop_arcs: [arc(l0_cycle, l0_hold), arc(l1_cycle, l1_hold), arc(l2_cycle, l2_hold), arc(l3_cycle, l3_hold)],
        loop_transpose: loop_transpose.clamp(-12, 12) as i8,
        key: if key == 0 { KeyChoice::Seed } else { KeyChoice::Note(((key - 1) % 12) as u8) },
        drone: arc(drone_cycle, drone_hold),
        drone_tone: if drone_tone == 0 { 0 } else { drone_tone.clamp(100, 16_000) as u16 },
    };
    Box::into_raw(Box::new(Player { track: Track::new(seeds, settings), buf: Vec::new() }))
}

/// # Safety
/// `player` must come from `sos_new`.
#[no_mangle]
pub unsafe extern "C" fn sos_render(player: *mut Player, frames: u32) -> *const f32 {
    let p = &mut *player;
    p.buf.resize(frames as usize * 2, 0.0);
    p.track.render(&mut p.buf);
    p.buf.as_ptr()
}

/// # Safety
/// `player` must come from `sos_new` and not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn sos_free(player: *mut Player) {
    drop(Box::from_raw(player));
}
