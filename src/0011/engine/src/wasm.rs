//! Plain C-ABI exports for the browser player (no wasm-bindgen, no JS glue
//! generation), so the .wasm stays loadable with nothing but the standard
//! WebAssembly API.
//!
//!   const p = sos_new(s1hi, s1lo, … s6hi, s6lo, scale, chords, pace, kit, space, bpm,
//!                     loop1_kind, loop1_lo, loop1_hi, loop2_kind, loop2_lo, loop2_hi,
//!                     drone_cycle, drone_hold, key, loop_transpose);
//!   (scale: index into `harmony::SCALES`; chords: 1-4; pace: 0 half-time, 1 jungle;
//!    kit: index into `kits::KITS`; space: 0 centred, 1 wide, 2 wide + tape echo;
//!    bpm: the drum tempo, 70-180; each loop is kind 0 = sustained timbre index in
//!    `lo`, or kind 1 = event grid with the 64 bits split lo/hi;
//!    drone_cycle, drone_hold: the drone's repeating arc in seconds, cycle 0 = always on;
//!    key: 0 = seed picks the tonic, 1..12 = C..B; loop_transpose: atmosphere semitones -12..12)
//!   const ptr = sos_render(p, frames);  // → `frames` interleaved stereo f32s
//!
//! The returned pointer is valid until the next call; re-create any
//! Float32Array view after each call in case memory has grown.

use crate::harmony::{Pace, Scale};
use crate::{Seeds, Settings, Track};

pub struct Player {
    track: Track,
    buf: Vec<f32>,
}

fn join(hi: u32, lo: u32) -> u64 {
    ((hi as u64) << 32) | lo as u64
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
    loop1_kind: u32,
    loop1_lo: u32,
    loop1_hi: u32,
    loop2_kind: u32,
    loop2_lo: u32,
    loop2_hi: u32,
    drone_cycle: u32,
    drone_hold: u32,
    key: u32,
    loop_transpose: i32,
) -> *mut Player {
    let seeds = Seeds {
        s1: join(s1hi, s1lo),
        s2: join(s2hi, s2lo),
        s3: join(s3hi, s3lo),
        s4: join(s4hi, s4lo),
        s5: join(s5hi, s5lo),
        s6: join(s6hi, s6lo),
    };
    let design = |kind: u32, lo: u32, hi: u32| -> crate::LoopDesign {
        if kind == 1 {
            crate::LoopDesign::Events(crate::texture::Grid::from_bits(join(hi, lo)))
        } else {
            crate::LoopDesign::Sustained(crate::atmos::LOOP_TIMBRES[lo as usize % crate::atmos::LOOP_TIMBRES.len()])
        }
    };
    let settings = Settings {
        scale: Scale::from_index(scale as usize),
        chords: chords.clamp(1, 4) as u8,
        pace: if pace == 1 { Pace::Jungle } else { Pace::HalfTime },
        kit: crate::kits::KITS[kit as usize % crate::kits::KITS.len()],
        space: crate::kits::SPACES[space as usize % crate::kits::SPACES.len()],
        bpm: bpm.min(u16::MAX as u32) as u16,
        loops: [design(loop1_kind, loop1_lo, loop1_hi), design(loop2_kind, loop2_lo, loop2_hi)],
        drone: {
            let cycle = drone_cycle.min(3600) as u16;
            let hold = if cycle == 0 { 0 } else { (drone_hold.min(cycle as u32)) as u16 };
            crate::DroneArc { cycle_s: cycle, hold_s: hold }
        },
        key: if key == 0 { crate::KeyChoice::Seed } else { crate::KeyChoice::Note(((key - 1) % 12) as u8) },
        loop_transpose: loop_transpose.clamp(-12, 12) as i8,
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
