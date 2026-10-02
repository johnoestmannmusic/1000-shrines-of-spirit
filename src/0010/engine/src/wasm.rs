//! Plain C-ABI exports for the browser player (no wasm-bindgen, no JS glue
//! generation), so the .wasm stays loadable with nothing but the standard
//! WebAssembly API.
//!
//!   const p = sos_new(s1hi, s1lo, … s5hi, s5lo, scale, chords, pace, kit);
//!   (scale: index into `harmony::SCALES`; chords: 1-4; pace: 0 half-time, 1 jungle;
//!    kit: index into `kits::KITS`)
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
    scale: u32,
    chords: u32,
    pace: u32,
    kit: u32,
) -> *mut Player {
    let seeds = Seeds {
        s1: join(s1hi, s1lo),
        s2: join(s2hi, s2lo),
        s3: join(s3hi, s3lo),
        s4: join(s4hi, s4lo),
        s5: join(s5hi, s5lo),
    };
    let settings = Settings {
        scale: Scale::from_index(scale as usize),
        chords: chords.clamp(1, 4) as u8,
        pace: if pace == 1 { Pace::Jungle } else { Pace::HalfTime },
        kit: crate::kits::KITS[kit as usize % crate::kits::KITS.len()],
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
