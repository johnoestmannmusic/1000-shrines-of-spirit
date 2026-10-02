//! Small drawing helpers on top of ratatui's buffer: clipped text, a braille
//! dot canvas, partial-block bars and colour mixing.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

pub type Rgb = (u8, u8, u8);

pub const DRONE: Rgb = (80, 205, 225);
pub const CHORD: Rgb = (125, 170, 255);
pub const GLITCH1: Rgb = (235, 95, 205);
pub const GLITCH2: Rgb = (245, 185, 65);
pub const BASS: Rgb = (125, 225, 120);
pub const ECHO: Rgb = (110, 145, 255);
pub const REVERB: Rgb = (175, 135, 255);
pub const OUTPUT: Rgb = (232, 232, 228);
pub const TEXT: Rgb = (205, 203, 198);
pub const DIM: Rgb = (110, 108, 104);
pub const FAINT: Rgb = (58, 57, 55);
pub const WHITE: Rgb = (255, 255, 255);
pub const TICKER_BG: Rgb = (28, 27, 26);

pub fn rgb(c: Rgb) -> Color {
    Color::Rgb(c.0, c.1, c.2)
}

pub fn fg(c: Rgb) -> Style {
    Style::new().fg(rgb(c))
}

pub fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * t).round() as u8;
    (m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}

/// A colour faded from near-black (level 0) to full `c` (level 1).
pub fn lit(c: Rgb, level: f64) -> Rgb {
    mix(FAINT, c, level)
}

/// Writes `s` at (x, y) inside `area`, clipped to it.
pub fn put(buf: &mut Buffer, area: Rect, x: u16, y: u16, s: &str, style: Style) {
    if x >= area.width || y >= area.height {
        return;
    }
    buf.set_stringn(area.x + x, area.y + y, s, (area.width - x) as usize, style);
}

pub fn put_char(buf: &mut Buffer, area: Rect, x: u16, y: u16, ch: char, style: Style) {
    if x < area.width && y < area.height {
        if let Some(cell) = buf.cell_mut((area.x + x, area.y + y)) {
            cell.set_char(ch).set_style(style);
        }
    }
}

const VERTICAL: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
const HORIZONTAL: [char; 9] = [' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'];

/// Draws a bar `frac` (0..1) of `height` rows tall, growing up from row `bottom`.
pub fn vbar(buf: &mut Buffer, area: Rect, x: u16, bottom: u16, height: u16, frac: f64, style: Style) {
    let eighths = (frac.clamp(0.0, 1.0) * height as f64 * 8.0).round() as u16;
    for i in 0..height {
        let fill = eighths.saturating_sub(i * 8).min(8);
        if fill > 0 && bottom >= i {
            put_char(buf, area, x, bottom - i, VERTICAL[fill as usize], style);
        }
    }
}

/// A horizontal bar `width` cells wide, filled to `frac`.
pub fn hbar(width: usize, frac: f64) -> String {
    let eighths = (frac.clamp(0.0, 1.0) * width as f64 * 8.0).round() as usize;
    (0..width).map(|i| HORIZONTAL[eighths.saturating_sub(i * 8).min(8)]).collect()
}

/// Level (peak amplitude) to a 0..1 display fraction over `range_db` of dynamic range.
pub fn level_frac(level: f64, range_db: f64) -> f64 {
    if level <= 0.0 {
        return 0.0;
    }
    ((20.0 * level.log10() + range_db) / range_db).clamp(0.0, 1.0)
}

/// A canvas of braille dots: each cell holds 2×4 dots.
pub struct Braille {
    w: usize,
    h: usize,
    cells: Vec<u8>,
}

impl Braille {
    pub fn new(cols: u16, rows: u16) -> Self {
        Braille { w: cols as usize, h: rows as usize, cells: vec![0; cols as usize * rows as usize] }
    }

    pub fn width(&self) -> usize {
        self.w * 2
    }

    pub fn height(&self) -> usize {
        self.h * 4
    }

    pub fn set(&mut self, x: i64, y: i64) {
        if x < 0 || y < 0 || x as usize >= self.width() || y as usize >= self.height() {
            return;
        }
        let (x, y) = (x as usize, y as usize);
        const BITS: [[u8; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];
        self.cells[(y / 4) * self.w + x / 2] |= BITS[x % 2][y % 4];
    }

    /// Vertical run of dots between y0 and y1 at column x.
    pub fn vline(&mut self, x: i64, y0: i64, y1: i64) {
        for y in y0.min(y1)..=y0.max(y1) {
            self.set(x, y);
        }
    }

    /// A connected trace through one y value per dot column.
    pub fn trace(&mut self, ys: &[f64]) {
        let mut last: Option<i64> = None;
        for (x, y) in ys.iter().enumerate() {
            let y = y.round() as i64;
            match last {
                Some(l) => self.vline(x as i64, l + (y - l).signum() * ((y - l).abs() > 1) as i64, y),
                None => self.set(x as i64, y),
            }
            last = Some(y);
        }
    }

    pub fn draw(&self, buf: &mut Buffer, area: Rect, x: u16, y: u16, style: Style) {
        for row in 0..self.h {
            for col in 0..self.w {
                let bits = self.cells[row * self.w + col];
                if bits != 0 {
                    let ch = char::from_u32(0x2800 + bits as u32).unwrap_or(' ');
                    put_char(buf, area, x + col as u16, y + row as u16, ch, style);
                }
            }
        }
    }
}
