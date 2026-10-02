//! The explainer ticker along the bottom. A terminal can only move text a
//! whole character at a time, which makes continuous scrolling judder and
//! hard to read. Instead, each explainer types on quickly, then holds still
//! long enough to read before the next one replaces it. ←/→ step manually.

use super::explain::EXPLAINERS;
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

impl Ticker {
    pub fn new() -> Self {
        Ticker { index: 0, page: 0, shown_at: 0.0, width: Cell::new(100), rows: Cell::new(2) }
    }

    fn pages(&self) -> Vec<Vec<String>> {
        // Two columns spare for the "…" that marks a continued explainer.
        let lines = wrap(EXPLAINERS[self.index].1, self.width.get().saturating_sub(2).max(20));
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
        self.index = (self.index + 1) % EXPLAINERS.len();
        self.page = 0;
        self.shown_at = t;
    }

    pub fn prev(&mut self, t: f64) {
        self.index = (self.index + EXPLAINERS.len() - 1) % EXPLAINERS.len();
        self.page = 0;
        self.shown_at = t;
    }

    /// The panel the current explainer is about.
    pub fn panel(&self) -> Panel {
        EXPLAINERS[self.index].0
    }

    pub fn draw(&self, buf: &mut Buffer, r: Rect, t: f64, label: &str) {
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
        put(buf, r, 0, 0, &format!("{:<w$}", format!(" {label} EXPLAINED"), w = LABEL_W as usize), strong);
        let pages = self.pages();
        let page_no = self.page.min(pages.len() - 1);
        let page = &pages[page_no];
        let left = (1.0 - (t - self.shown_at) / Self::duration(page)).clamp(0.0, 1.0);
        let counter = format!(" {:>2}/{} {} ←→", self.index + 1, EXPLAINERS.len(), hbar(5, left));
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
