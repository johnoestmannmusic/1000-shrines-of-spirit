//! Preview renderer (host only).
//!
//! Draws a [`Scene`] at a chosen seed/biome/weather into the logical buffer and
//! writes it as a PNG plus a small HTML viewer, so we can watch the art emerge
//! without the browser. A development tool: it never ships inside the artifact.

use shrine0016::framebuffer::Framebuffer;
use shrine0016::plants::Biome;
use shrine0016::scene::{Quality, Scene};
use shrine0016::weather::WeatherKind;
use std::fs;
use std::io;
use std::path::Path;

/// Render and write a preview pair from a seed/biome/weather.
pub fn write(seed: u64, biome: Biome, weather: WeatherKind, t: f64, out: &str) -> io::Result<()> {
    let logical = render(seed, biome, weather, t);
    let caption = format!(
        "seed {seed} · {} · {} · 300×300 logical → 1200×1200 nearest",
        biome.name(),
        weather.name()
    );
    write_frame(&logical, &caption, out)
}

/// Write an already-rendered logical frame as a PNG plus an HTML viewer.
pub fn write_frame(fb: &Framebuffer, caption: &str, out: &str) -> io::Result<()> {
    let png = crate::png::encode_rgba(fb.width(), fb.height(), fb.as_bytes());
    let path = Path::new(out);
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            fs::create_dir_all(dir)?;
        }
    }
    fs::write(path, png)?;

    let png_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("shrine.png");
    let title = path.file_stem().and_then(|s| s.to_str()).unwrap_or("shrine");
    let html = format!(
        "<!doctype html>\n<meta charset=\"utf-8\">\n<title>ShrineGen — {title}</title>\n\
         <style>\n\
         html,body{{margin:0;min-height:100%;background:#0d0c10;color:#c9c4d0;\
         font:14px ui-monospace,SFMono-Regular,Menlo,monospace;display:grid;place-items:center}}\n\
         figure{{margin:0;display:grid;justify-items:center;gap:10px;padding:24px}}\n\
         img{{width:min(1200px,94vmin);height:auto;image-rendering:pixelated;\
         image-rendering:crisp-edges;border:1px solid #2a2732}}\n\
         figcaption{{color:#8a8596}}\n\
         </style>\n\
         <figure>\n  <img src=\"{png_name}\" width=\"1200\" height=\"1200\" alt=\"shrine\">\n\
         <figcaption>{caption}</figcaption>\n</figure>\n"
    );
    fs::write(path.with_extension("html"), html)?;
    Ok(())
}

/// The logical 300x300 image a scene produces at time `t`.
pub fn render(seed: u64, biome: Biome, weather: WeatherKind, t: f64) -> Framebuffer {
    let scene = Scene::new(seed, biome, weather);
    let mut fb = Framebuffer::logical();
    scene.render(&mut fb, t, Quality::High);
    fb
}

/// Nearest-neighbour upscale of a logical frame to `size` x `size`, for a cover
/// image. (The runtime buffer stays 300x300; this is an exported still.)
pub fn upscale(fb: &Framebuffer, size: u32) -> Framebuffer {
    let mut out = Framebuffer::new(size, size);
    let (sw, sh) = (fb.width(), fb.height());
    for dy in 0..size {
        let sy = (dy * sh / size) as i32;
        for dx in 0..size {
            let sx = (dx * sw / size) as i32;
            out.put(dx as i32, dy as i32, fb.get(sx, sy));
        }
    }
    out
}

/// The cover's picture: the piece where it starts, seen straight on (the
/// camera's orbit at 0°, whatever its speed), as the WAV export embeds it.
pub fn cover_frame(config: &shrine0016::sim::SimConfig) -> Framebuffer {
    let sim = shrine0016::sim::Sim::new_visual(config.clone());
    let mut state = sim.state(sim.start_offset());
    state.orbit_turns = 0.0;
    let mut fb = Framebuffer::logical();
    sim.scene.render_state(&mut fb, &state, Quality::High);
    fb
}

/// Encode the current frame as a square PNG cover of `size` x `size`.
pub fn cover_png(fb: &Framebuffer, size: u32) -> Vec<u8> {
    let big = upscale(fb, size);
    crate::png::encode_rgba(big.width(), big.height(), big.as_bytes())
}

/// Write the current frame as a square PNG cover of `size` x `size`.
pub fn write_cover(fb: &Framebuffer, out: &str, size: u32) -> io::Result<()> {
    let png = cover_png(fb, size);
    let path = Path::new(out);
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            fs::create_dir_all(dir)?;
        }
    }
    fs::write(path, png)
}

#[cfg(test)]
mod cover_tests {
    use super::*;
    use shrine0016::sim::SimConfig;

    #[test]
    fn the_cover_is_unturned_whatever_the_camera_speed() {
        let mut still = SimConfig::default();
        still.settings.camera_speed = 0;
        let mut turning = still.clone();
        turning.settings.camera_speed = -37;
        turning.settings.start_s = 90;
        still.settings.start_s = 90;
        assert_eq!(cover_frame(&turning).hash(), cover_frame(&still).hash());
    }
}
