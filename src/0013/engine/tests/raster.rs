//! Framebuffer and raster primitives: the pixel contract and the first
//! drawable shapes.

use shrine0012::framebuffer::{rgb, rgba, Framebuffer, Rgba};
use shrine0012::iso::{to_units, Iso};
use shrine0012::raster::{ceil_div, fill_cell, fill_diamond_at, fill_face, floor_div, line};
use shrine0012::{ART_PX, CANVAS_W, LOGICAL_H, LOGICAL_W};

const BG: Rgba = [0, 0, 0, 255];
const FG: Rgba = [255, 255, 255, 255];
const RED: Rgba = rgb(255, 0, 0);

fn fresh(c: Rgba) -> Framebuffer {
    let mut fb = Framebuffer::logical();
    fb.clear(c);
    fb
}

#[test]
fn a_vertical_face_drops_down_its_own_columns() {
    // Regression: `fill_face` called the *horizontal* `fill_span`, so every side
    // face was painted as a sideways band — rows taken from an x coordinate and
    // columns from a y bound — which made the whole architecture look skewed.
    let mut fb = fresh(BG);
    fill_face(&mut fb, to_units(100), to_units(140), to_units(50), to_units(50), to_units(30), FG);

    assert_eq!(fb.get(120, 65), FG, "the face is not filled where it should be");
    // The transposed call painted rows 100..140 across columns 50..80.
    assert_eq!(fb.get(60, 120), BG, "a sideways band was drawn");
    assert_eq!(fb.get(150, 65), BG, "filled past the face's right edge");
    assert_eq!(fb.get(120, 40), BG, "filled above the face");
    assert_eq!(fb.get(120, 85), BG, "filled below the face");
    // Every column across the face carries the full drop.
    for x in 101..140 {
        assert_eq!(fb.get(x, 55), FG, "column {x} is short at the top");
        assert_eq!(fb.get(x, 79), FG, "column {x} is short at the bottom");
    }
}

#[test]
fn logical_buffer_matches_the_contract() {
    let fb = Framebuffer::logical();
    assert_eq!((fb.width(), fb.height()), (300, 300));
    assert_eq!(CANVAS_W, fb.width() * ART_PX);
    assert_eq!(fb.as_bytes().len(), (LOGICAL_W * LOGICAL_H * 4) as usize);
}

#[test]
fn pixels_round_trip_in_rgba_order() {
    let mut fb = Framebuffer::logical();
    fb.clear(rgba(1, 2, 3, 4));
    assert_eq!(&fb.as_bytes()[0..4], &[1, 2, 3, 4]);
    fb.put(5, 6, RED);
    assert_eq!(fb.get(5, 6), RED);
    let i = ((6 * 300 + 5) * 4) as usize;
    assert_eq!(&fb.as_bytes()[i..i + 4], &[255, 0, 0, 255]);
}

#[test]
fn clipping_never_panics_and_only_touches_valid_pixels() {
    let mut fb = fresh(BG);
    fb.put(-1, -1, RED);
    fb.put(300, 300, RED);
    fb.fill_rect(-50, -50, -1, -1, RED); // fully off-screen
    fb.fill_span(999, 0, 299, RED); // row below the buffer
    fb.fill_span(-1, 0, 299, RED); // row above the buffer
    assert_eq!(fb.hash(), fresh(BG).hash());
    // A span that starts off-screen but reaches in fills only the visible part.
    fb.fill_span(0, -100, 1, RED);
    assert_eq!(fb.get(0, 0), RED);
    assert_eq!(fb.get(1, 0), RED);
    assert_eq!(fb.get(2, 0), BG);
}

#[test]
fn a_diamond_covers_a_sensible_area() {
    let mut fb = fresh(BG);
    // hw=100, hh=50 fixed units = 12.5 x 6.25 logical px.
    fill_diamond_at(&mut fb, 2400, 2400, 100, 50, FG);
    let filled = fb
        .as_bytes()
        .chunks_exact(4)
        .filter(|p| **p == FG)
        .count();
    // Ideal area = 2*hw*hh/FP^2 = 39.1 px; edge expansion adds a few per row.
    assert!((30..=60).contains(&filled), "filled {filled} pixels");
}

#[test]
fn adjacent_diamonds_have_no_seam() {
    let iso = Iso::grid_24();
    let mut fb = fresh(BG);
    for gy in 0..24 {
        for gx in 0..24 {
            fill_cell(&mut fb, &iso, gx, gy, 0, FG);
        }
    }
    let mut checked_rows = 0;
    for py in 80..220 {
        let filled: Vec<i32> = (0..300)
            .filter(|&x| fb.get(x, py) == FG)
            .collect();
        if let (Some(&first), Some(&last)) = (filled.first(), filled.last()) {
            for x in first..=last {
                assert_eq!(fb.get(x, py), FG, "seam at ({x}, {py}) between {first}..{last}");
            }
            checked_rows += 1;
        }
    }
    assert!(checked_rows > 50, "expected the grid to cover many rows, got {checked_rows}");
}

#[test]
fn hashing_is_stable_and_content_sensitive() {
    assert_eq!(fresh(BG).hash(), fresh(BG).hash());
    let mut changed = fresh(BG);
    changed.put(10, 10, RED);
    assert_ne!(fresh(BG).hash(), changed.hash());
}

#[test]
fn lines_draw_to_the_endpoints() {
    let mut fb = fresh(BG);
    line(&mut fb, 10, 10, 10, 20, FG);
    assert_eq!(fb.get(10, 10), FG);
    assert_eq!(fb.get(10, 20), FG);
    assert_eq!(fb.get(10, 15), FG);
}

#[test]
fn division_helpers_are_exact_at_edges() {
    assert_eq!(floor_div(15, 16), 0);
    assert_eq!(floor_div(-1, 16), -1);
    assert_eq!(ceil_div(16, 16), 1);
    assert_eq!(ceil_div(17, 16), 2);
    assert_eq!(ceil_div(-16, 16), -1);
}
