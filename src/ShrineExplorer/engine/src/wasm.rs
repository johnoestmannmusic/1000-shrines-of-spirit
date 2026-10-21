//! Plain C-ABI exports for the browser — no binding generator, no generated JS
//! glue (`HC006`). The page loads this with the standard WebAssembly API.
//!
//! The development page passes a recipe; the emitted artifact bakes its own and
//! ignores these words (`src/emit.rs`).
//!
//! ```text
//!   const words = shrine_alloc(recipeLenBytes);   // or pass 0, 0
//!   write recipe u32s into memory at `words`
//!   const audio  = shrine_new(words, count, 1);   // mode 1 = audio
//!   const visual = shrine_new(words, count, 2);   // mode 2 = visuals
//!   const pcm = shrine_render_audio(audio, frames);   // *const f32, interleaved
//!   const px  = shrine_render_visual(visual, clock);  // *const u8, 300x300 RGBA
//!   shrine_free(audio); shrine_free(visual);
//! ```

use crate::player::Player;
use crate::recipe;
use crate::sim::{Sim, SimConfig};

/// Reserve `len` bytes in the module's memory for the caller to fill.
#[no_mangle]
pub extern "C" fn shrine_alloc(len: u32) -> *mut u8 {
    let mut bytes = Vec::<u8>::with_capacity(len as usize);
    let ptr = bytes.as_mut_ptr();
    core::mem::forget(bytes);
    ptr
}

/// Free memory previously returned by [`shrine_alloc`].
///
/// # Safety
/// `ptr`/`len` must be exactly a pair returned by `shrine_alloc`.
#[no_mangle]
pub unsafe extern "C" fn shrine_dealloc(ptr: *mut u8, len: u32) {
    if !ptr.is_null() {
        unsafe { drop(Vec::from_raw_parts(ptr, 0, len as usize)) };
    }
}

/// Build a player from `words_len` recipe words at `words`.
///
/// `mode`: 0 = audio + visuals, 1 = audio only, 2 = visuals only. A missing or
/// empty recipe falls back to the canonical defaults.
///
/// # Safety
/// `words` must point to `words_len` readable `u32`s (or be null).
#[no_mangle]
pub unsafe extern "C" fn shrine_new(words: *const u32, words_len: u32, mode: u32) -> *mut Player {
    let config = if words.is_null() || words_len == 0 {
        SimConfig::default()
    } else {
        let slice = unsafe { core::slice::from_raw_parts(words, words_len as usize) };
        recipe::decode(slice)
    };
    build(config, mode)
}

fn build(config: SimConfig, mode: u32) -> *mut Player {
    let sim = if mode == 2 { Sim::new_visual(config) } else { Sim::new(config) };
    Box::into_raw(Box::new(Player::new(sim)))
}

/// Render `frames` stereo samples; returns a pointer valid until the next call.
///
/// # Safety
/// `player` must come from [`shrine_new`].
#[no_mangle]
pub unsafe extern "C" fn shrine_render_audio(player: *mut Player, frames: u32) -> *const f32 {
    let p = unsafe { &mut *player };
    p.render_audio(frames as usize).as_ptr()
}

/// Render one 300x300 RGBA frame at `sample_clock` samples; returns a pointer
/// valid until the next call.
///
/// # Safety
/// `player` must come from [`shrine_new`].
#[no_mangle]
pub unsafe extern "C" fn shrine_render_visual(player: *mut Player, sample_clock: u64) -> *const u8 {
    let p = unsafe { &mut *player };
    p.render_visual(sample_clock).as_ptr()
}

/// Nudge the camera from the page's pointer, in `[-1, 1]` per axis.
///
/// # Safety
/// `player` must come from [`shrine_new`].
#[no_mangle]
pub unsafe extern "C" fn shrine_set_camera(player: *mut Player, yaw: f64, pitch: f64) {
    let p = unsafe { &mut *player };
    p.set_camera(yaw, pitch);
}

/// Leave the sky unfilled so a page can draw its own sky (and put the source
/// waterfall in it, behind the shrine).
///
/// # Safety
/// `player` must come from [`shrine_new`].
#[no_mangle]
pub unsafe extern "C" fn shrine_set_sky_transparent(player: *mut Player, on: u32) {
    let p = unsafe { &mut *player };
    p.set_sky_transparent(on != 0);
}

/// The palette's sky gradient endpoints, packed top in the low 24 bits.
///
/// # Safety
/// `player` must come from [`shrine_new`].
#[no_mangle]
pub unsafe extern "C" fn shrine_sky_colors(player: *mut Player) -> u64 {
    let p = unsafe { &*player };
    p.sky_colors()
}

/// The source-code overlay word (default), so the dev page can read it too.
#[no_mangle]
pub extern "C" fn shrine_code_overlay() -> u32 {
    crate::overlay::CodeOverlay::DEFAULT.word()
}

/// The overlay's tilt in degrees, for the dev page.
#[no_mangle]
pub extern "C" fn shrine_code_angle() -> i32 {
    crate::overlay::CodeOverlay::DEFAULT.angle as i32
}

/// The camera rotation speed, for the dev page (always still).
#[no_mangle]
pub extern "C" fn shrine_camera_speed() -> i32 {
    0
}

/// The overlay's vertical shift, for the dev page.
#[no_mangle]
pub extern "C" fn shrine_code_offset() -> i32 {
    crate::overlay::CodeOverlay::DEFAULT.offset as i32
}

/// The dev page's track name: empty; a real artifact bakes its own.
#[no_mangle]
pub extern "C" fn shrine_name_ptr() -> *const u8 {
    b"".as_ptr()
}

/// Length of the dev page's track name (always 0).
#[no_mangle]
pub extern "C" fn shrine_name_len() -> u32 {
    0
}

/// The Learn page's description of this shrine: UTF-8 JSON, `shrine_learn_len` bytes.
///
/// # Safety
/// `player` must come from [`shrine_new`].
#[no_mangle]
pub unsafe extern "C" fn shrine_learn_ptr(player: *mut Player) -> *const u8 {
    let p = unsafe { &mut *player };
    p.learn_json().as_ptr()
}

/// Length of the description in bytes.
///
/// # Safety
/// `player` must come from [`shrine_new`].
#[no_mangle]
pub unsafe extern "C" fn shrine_learn_len(player: *mut Player) -> u32 {
    let p = unsafe { &mut *player };
    p.learn_json().len() as u32
}

/// The music's moving parts now: `shrine_learn_state_len` f64s (layout in
/// `learn::STATE_LEN`). Display only.
///
/// # Safety
/// `player` must come from [`shrine_new`].
#[no_mangle]
pub unsafe extern "C" fn shrine_learn_state(player: *mut Player) -> *const f64 {
    let p = unsafe { &mut *player };
    p.learn_state().as_ptr()
}

#[no_mangle]
pub extern "C" fn shrine_learn_state_len() -> u32 {
    crate::learn::STATE_LEN as u32
}

/// The weather at `sample_clock`: 6 f64s (wind, gust, rain, snow, fog, light).
///
/// # Safety
/// `player` must come from [`shrine_new`].
#[no_mangle]
pub unsafe extern "C" fn shrine_learn_weather(player: *mut Player, sample_clock: u64) -> *const f64 {
    let p = unsafe { &mut *player };
    p.weather_now(sample_clock).as_ptr()
}

/// Where grid cell `(gx, gy)`'s top surface is on screen at `sample_clock`:
/// 3 f64s (x, y in logical pixels, its level); x = −1 off the grid.
///
/// # Safety
/// `player` must come from [`shrine_new`].
#[no_mangle]
pub unsafe extern "C" fn shrine_surface_point(player: *mut Player, gx: i32, gy: i32, sample_clock: u64) -> *const f64 {
    let p = unsafe { &mut *player };
    p.surface_point(gx, gy, sample_clock).as_ptr()
}

/// Where side `side` (0 +x, 1 −x, 2 +y, 3 −y) of the shrine is on screen at
/// `sample_clock`: 4 f64s (x, y in logical pixels, then the outward direction).
///
/// # Safety
/// `player` must come from [`shrine_new`].
#[no_mangle]
pub unsafe extern "C" fn shrine_side_point(player: *mut Player, side: u32, sample_clock: u64) -> *const f64 {
    let p = unsafe { &mut *player };
    p.side_point(side, sample_clock).as_ptr()
}

/// A player for the shrine at `(x, y)` on the map of this module's home (the
/// default recipe here; an artifact bakes its own).
#[no_mangle]
pub extern "C" fn shrine_new_at(x: i32, y: i32, mode: u32) -> *mut Player {
    let mut config = SimConfig::default();
    config.settings.map = (x, y);
    build(config, mode)
}

/// Where on the map this module starts (always home here).
#[no_mangle]
pub extern "C" fn shrine_map_x() -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn shrine_map_y() -> i32 {
    0
}

/// Hear one layer alone (1 drone, 2–3 glitch layers, 4 bass, 5 drums, 6–9
/// atmosphere loops, 10 the echo and reverb), or 0 for the full mix.
///
/// # Safety
/// `player` must come from [`shrine_new`].
#[no_mangle]
pub unsafe extern "C" fn shrine_set_solo(player: *mut Player, layer: u32) {
    let p = unsafe { &mut *player };
    p.set_solo(layer);
}

/// Release a player.
///
/// # Safety
/// `player` must come from [`shrine_new`] and not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn shrine_free(player: *mut Player) {
    if !player.is_null() {
        unsafe { drop(Box::from_raw(player)) };
    }
}
