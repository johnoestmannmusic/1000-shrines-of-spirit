//! The Insights ticker along the bottom. A terminal can only move text a
//! whole character at a time, which makes continuous scrolling judder and
//! hard to read. Instead, each insight types on quickly, then holds still
//! long enough to read before the next one replaces it. ←/→ step manually.
//!
//! The order is shuffled (every insight once, then a fresh shuffle), so even
//! short sessions see a different selection each time. The shuffle is seeded
//! from the wall clock: it only affects the display, never the sound.

use super::explain::{fill, Vars, EXPLAINERS};
use shrine0011::kits::{DrumSpace, Kit};
use shrine0011::{DroneArc, LoopDesign};
use super::gfx::*;
use super::Panel;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use std::cell::Cell;

/// Characters per second while a new explainer types on.
const TYPE_ON: f64 = 260.0;
/// Hold time: a base plus comfortable reading speed.
const HOLD_BASE: f64 = 2.5;
const READ_CHARS_PER_SECOND: f64 = 15.0;
const LABEL_W: u16 = 18;

pub struct Ticker {
    /// The insights that are true for this version (indices into EXPLAINERS),
    /// and this version's values for their placeholders.
    pool: Vec<usize>,
    vars: Vars,
    /// Position in `order`.
    at: usize,
    /// A shuffled permutation of the explainers.
    order: Vec<usize>,
    shuffle: u64,
    index: usize,
    page: usize,
    shown_at: f64,
    /// Text width at the last draw (decides how explainers wrap into pages).
    width: Cell<usize>,
    rows: Cell<usize>,
}

/// Greedy word wrap to `width` columns.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = vec![String::new()];
    for word in text.split(' ') {
        let cur = lines.last_mut().unwrap();
        if !cur.is_empty() && cur.chars().count() + 1 + word.chars().count() > width {
            lines.push(word.to_string());
        } else {
            if !cur.is_empty() {
                cur.push(' ');
            }
            cur.push_str(word);
        }
    }
    lines
}

/// Fisher–Yates with a tiny xorshift (display only).
fn shuffled(n: usize, seed: &mut u64) -> Vec<usize> {
    let mut v: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 7;
        *seed ^= *seed << 17;
        v.swap(i, (*seed % (i as u64 + 1)) as usize);
    }
    v
}

impl Ticker {
    pub fn new(kit: Kit, space: DrumSpace, loops: [LoopDesign; 4], drone: DroneArc, vars: Vars) -> Self {
        let pool: Vec<usize> = EXPLAINERS
            .iter()
            .enumerate()
            .filter(|(_, (_, when, _))| when.applies(kit, space, loops, drone))
            .map(|(i, _)| i)
            .collect();
        let mut shuffle = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9)
            | 1;
        let order: Vec<usize> = shuffled(pool.len(), &mut shuffle).into_iter().map(|i| pool[i]).collect();
        let index = order[0];
        Ticker { pool, vars, at: 0, order, shuffle, index, page: 0, shown_at: 0.0, width: Cell::new(100), rows: Cell::new(2) }
    }

    fn pages(&self) -> Vec<Vec<String>> {
        // Two columns spare for the "…" that marks a continued explainer.
        let text = fill(EXPLAINERS[self.index].2, &self.vars);
        let lines = wrap(&text, self.width.get().saturating_sub(2).max(20));
        lines.chunks(self.rows.get().max(1)).map(|c| c.to_vec()).collect()
    }

    fn duration(page: &[String]) -> f64 {
        let chars: usize = page.iter().map(|l| l.chars().count()).sum();
        chars as f64 / TYPE_ON + HOLD_BASE + chars as f64 / READ_CHARS_PER_SECOND
    }

    pub fn advance(&mut self, t: f64) {
        let pages = self.pages();
        let page = &pages[self.page.min(pages.len() - 1)];
        if t - self.shown_at > Self::duration(page) {
            if self.page + 1 < pages.len() {
                self.page += 1;
                self.shown_at = t;
            } else {
                self.next(t);
            }
        }
    }

    pub fn next(&mut self, t: f64) {
        self.at += 1;
        if self.at >= self.order.len() {
            // Everything has been shown once: reshuffle, avoiding an immediate repeat.
            let last = self.index;
            self.order = shuffled(self.pool.len(), &mut self.shuffle).into_iter().map(|i| self.pool[i]).collect();
            if self.order[0] == last && self.order.len() > 1 {
                self.order.swap(0, 1);
            }
            self.at = 0;
        }
        self.index = self.order[self.at];
        self.page = 0;
        self.shown_at = t;
    }

    pub fn prev(&mut self, t: f64) {
        self.at = self.at.saturating_sub(1);
        self.index = self.order[self.at];
        self.page = 0;
        self.shown_at = t;
    }

    /// The panel the current explainer is about.
    pub fn panel(&self) -> Panel {
        EXPLAINERS[self.index].0
    }

    pub fn draw(&self, buf: &mut Buffer, r: Rect, t: f64) {
        let text_w = r.width.saturating_sub(LABEL_W + 2) as usize;
        self.width.set(text_w);
        self.rows.set(r.height as usize);
        let bg = Style::new().bg(rgb(TICKER_BG)).fg(rgb(TEXT));
        for y in 0..r.height {
            put(buf, r, 0, y, &" ".repeat(r.width as usize), bg);
        }

        // Label block: title, then position and time left on this explainer.
        let strong = Style::new().bg(rgb(GLITCH2)).fg(rgb((20, 20, 20))).add_modifier(Modifier::BOLD);
        let soft = Style::new().bg(rgb(mix(GLITCH2, TICKER_BG, 0.55))).fg(rgb((20, 20, 20)));
        put(buf, r, 0, 0, &format!("{:<w$}", " INSIGHTS", w = LABEL_W as usize), strong);
        let pages = self.pages();
        let page_no = self.page.min(pages.len() - 1);
        let page = &pages[page_no];
        let left = (1.0 - (t - self.shown_at) / Self::duration(page)).clamp(0.0, 1.0);
        let counter = format!(" {:>2}/{} {} ←→", self.at + 1, self.order.len(), hbar(5, left));
        let row = if r.height > 1 { 1 } else { 0 };
        if r.height > 1 {
            put(buf, r, 0, row, &format!("{counter:<w$}", w = LABEL_W as usize), soft);
        }

        // The explainer, typed on and then held.
        let mut budget = ((t - self.shown_at) * TYPE_ON) as usize;
        for (i, line) in page.iter().enumerate() {
            let shown: String = line.chars().take(budget).collect();
            budget = budget.saturating_sub(line.chars().count());
            let more = page_no + 1 < pages.len() && i + 1 == page.len();
            let text = if more { format!("{shown} …") } else { shown };
            put(buf, r, LABEL_W + 1, i as u16, &text, bg.add_modifier(Modifier::BOLD));
        }
    }
}
