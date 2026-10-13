//! The logical framebuffer: exactly 300x300 RGBA8, row-major, top-left origin.
//!
//! The buffer stores bytes in the order `R, G, B, A`, which is directly the
//! layout the browser's `ImageData` expects, so the WASM export can hand back a
//! `*const u8` and the page can wrap it without conversion or endianness
//! assumptions. The canvas nearest-upscales this buffer to 1200x1200 (`HC003`);
//! nothing here ever draws at screen scale.

use crate::LOGICAL_H;
use crate::LOGICAL_W;

/// One colour, in the exact byte order the browser wants.
pub type Rgba = [u8; 4];

pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Rgba {
    [r, g, b, a]
}

pub const fn rgb(r: u8, g: u8, b: u8) -> Rgba {
    [r, g, b, 255]
}

/// Scale a colour by `num / den`, keeping its alpha. Used for the lit top /
/// shaded side faces of a column. Integer-only, so it is deterministic.
pub fn shade(c: Rgba, num: u32, den: u32) -> Rgba {
    let f = |v: u8| ((v as u32 * num) / den).min(255) as u8;
    [f(c[0]), f(c[1]), f(c[2]), c[3]]
}

/// Mix `a` toward `b` by `num / den` (the weight of `b`). Integer-only.
pub fn blend(a: Rgba, b: Rgba, num: u32, den: u32) -> Rgba {
    let den = den.max(1);
    let num = num.min(den);
    let f = |x: u8, y: u8| ((x as u32 * (den - num) + y as u32 * num) / den) as u8;
    [f(a[0], b[0]), f(a[1], b[1]), f(a[2], b[2]), 255]
}

/// A 300x300 RGBA8 image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Framebuffer {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Framebuffer {
    /// A new buffer, cleared to transparent black.
    pub fn new(width: u32, height: u32) -> Self {
        Framebuffer {
            width,
            height,
            pixels: vec![0; width as usize * height as usize * 4],
        }
    }

    /// The canonical logical buffer (`HC003`).
    pub fn logical() -> Self {
        Framebuffer::new(LOGICAL_W, LOGICAL_H)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn clear(&mut self, colour: Rgba) {
        for chunk in self.pixels.chunks_exact_mut(4) {
            chunk.copy_from_slice(&colour);
        }
    }

    #[inline]
    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as u32) < self.width && (y as u32) < self.height
    }

    #[inline]
    fn offset(&self, x: i32, y: i32) -> usize {
        ((y as u32 * self.width + x as u32) * 4) as usize
    }

    /// Write one pixel. Out-of-bounds writes are ignored (clipping).
    #[inline]
    pub fn put(&mut self, x: i32, y: i32, colour: Rgba) {
        if self.in_bounds(x, y) {
            let i = self.offset(x, y);
            self.pixels[i..i + 4].copy_from_slice(&colour);
        }
    }

    /// Read one pixel; transparent black outside the buffer.
    pub fn get(&self, x: i32, y: i32) -> Rgba {
        if !self.in_bounds(x, y) {
            return [0, 0, 0, 0];
        }
        let i = self.offset(x, y);
        [self.pixels[i], self.pixels[i + 1], self.pixels[i + 2], self.pixels[i + 3]]
    }

    /// Fill a horizontal run `x0..=x1` on row `y`, clipped to the buffer.
    pub fn fill_span(&mut self, y: i32, x0: i32, x1: i32, colour: Rgba) {
        if y < 0 || y as u32 >= self.height {
            return;
        }
        let x0 = x0.max(0);
        let x1 = x1.min(self.width as i32 - 1);
        if x0 > x1 {
            return;
        }
        let start = self.offset(x0, y);
        let end = self.offset(x1, y) + 4;
        for chunk in self.pixels[start..end].chunks_exact_mut(4) {
            chunk.copy_from_slice(&colour);
        }
    }

    /// Fill a vertical run `y0..=y1` in column `x`, clipped to the buffer.
    pub fn fill_vertical(&mut self, x: i32, y0: i32, y1: i32, colour: Rgba) {
        if x < 0 || x as u32 >= self.width {
            return;
        }
        let y0 = y0.max(0);
        let y1 = y1.min(self.height as i32 - 1);
        if y0 > y1 {
            return;
        }
        for y in y0..=y1 {
            let i = self.offset(x, y);
            self.pixels[i..i + 4].copy_from_slice(&colour);
        }
    }

    /// Fill an axis-aligned rectangle `x0..=x1`, `y0..=y1`, clipped.
    pub fn fill_rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, colour: Rgba) {
        for y in y0..=y1 {
            self.fill_span(y, x0, x1, colour);
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.pixels
    }

    pub fn as_mut_bytes(&mut self) -> &mut [u8] {
        &mut self.pixels
    }

    /// Pointer for the WASM ABI. Valid until the buffer is mutated or moved.
    pub fn as_ptr(&self) -> *const u8 {
        self.pixels.as_ptr()
    }

    /// FNV-1a over the raw bytes: the golden-hash primitive for frame tests.
    pub fn hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for &b in &self.pixels {
            h ^= b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        h
    }
}
