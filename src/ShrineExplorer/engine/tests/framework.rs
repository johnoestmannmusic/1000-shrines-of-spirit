//! The isometric and grid framework must stay exact, reusable and
//! shrine-agnostic (`SC002`).

use shrine_explorer::grid::{Layers, Tile, CELLS, MAX_LEVELS};
use shrine_explorer::iso::{to_px, to_units, Iso, FP};
use shrine_explorer::{GRID_H, GRID_W, LOGICAL_H, LOGICAL_W};

#[test]
fn projection_spans_the_logical_width_exactly() {
    let iso = Iso::grid_24();
    let (mut min_x, mut max_x) = (i32::MAX, i32::MIN);
    let (mut min_y, mut max_y) = (i32::MAX, i32::MIN);
    for gy in 0..GRID_H as i32 {
        for gx in 0..GRID_W as i32 {
            for corner in iso.corners(gx, gy, 0) {
                min_x = min_x.min(corner.x);
                max_x = max_x.max(corner.x);
                min_y = min_y.min(corner.y);
                max_y = max_y.max(corner.y);
            }
        }
    }
    assert_eq!(min_x, 0, "grid should touch the left edge of the buffer");
    assert_eq!(max_x, (LOGICAL_W as i32) * FP, "grid should touch the right edge exactly");
    assert!(min_y >= 0 && max_y <= (LOGICAL_H as i32) * FP, "grid must stay inside the buffer vertically");
}

#[test]
fn unproject_round_trips_every_cell() {
    let iso = Iso::grid_24();
    for gz in 0..8 {
        for gy in 0..GRID_H as i32 {
            for gx in 0..GRID_W as i32 {
                let c = iso.project(gx, gy, gz);
                assert_eq!(iso.unproject(c.x, c.y, gz), (gx, gy), "cell ({gx},{gy},{gz})");
            }
        }
    }
}

#[test]
fn one_level_lifts_by_exactly_level_h() {
    let iso = Iso::grid_24();
    let base = iso.project(7, 11, 0);
    let up = iso.project(7, 11, 1);
    assert_eq!(base.x, up.x);
    assert_eq!(base.y - up.y, iso.level_h);
}

#[test]
fn neighbours_step_by_half_a_tile() {
    let iso = Iso::grid_24();
    let a = iso.project(5, 5, 0);
    let east = iso.project(6, 5, 0);
    let south = iso.project(5, 6, 0);
    assert_eq!(east.x - a.x, iso.half_w());
    assert_eq!(east.y - a.y, iso.half_h());
    assert_eq!(south.x - a.x, -iso.half_w());
    assert_eq!(south.y - a.y, iso.half_h());
}

#[test]
fn depth_orders_columns_then_height() {
    let iso = Iso::grid_24();
    assert!(iso.depth_key(0, 0, 0) < iso.depth_key(1, 0, 0));
    assert!(iso.depth_key(0, 0, 0) < iso.depth_key(0, 1, 0));
    assert!(iso.depth_key(3, 3, 0) < iso.depth_key(4, 3, 0));
    assert!(iso.depth_key(3, 3, 0) < iso.depth_key(3, 3, 1));
}

#[test]
fn a_camera_nudge_shears_the_projection() {
    let mut iso = Iso::grid_24();
    let base = iso.project(6, 4, 0);
    iso.cam_yaw = 1.0;
    assert_ne!(base.x, iso.project(6, 4, 0).x, "yaw should shear horizontally");
    iso.cam_yaw = 0.0;
    iso.cam_pitch = 1.0;
    let plain = Iso::grid_24().project(10, 10, 0);
    assert_ne!(plain.y, iso.project(10, 10, 0).y, "pitch should squeeze vertically");
    let flat = Iso::grid_24();
    assert_eq!((flat.cam_yaw, flat.cam_pitch), (0.0, 0.0));
}

#[test]
fn fixed_point_conversions_are_stable() {
    assert_eq!(to_units(12), 12 * FP);
    assert_eq!(to_px(to_units(12) + FP - 1), 12);
    assert_eq!(to_px(-1), -1, "floor, not truncate");
}

#[test]
fn grid_stores_and_reads_tiles() {
    let mut layers = Layers::new(6);
    assert_eq!(layers.levels(), 6);
    assert_eq!(layers.count_non_empty(), 0);
    assert!(layers.set(3, 4, 2, Tile::new(7, 1)));
    assert_eq!(layers.get(3, 4, 2), Tile::new(7, 1));
    assert_eq!(layers.count_non_empty(), 1);
    // Out of bounds never panics and never writes.
    assert!(!layers.set(-1, 0, 0, Tile::new(1, 0)));
    assert!(!layers.set(GRID_W as i32, 0, 0, Tile::new(1, 0)));
    assert!(!layers.set(0, 0, 6, Tile::new(1, 0)));
    assert_eq!(layers.get(99, 99, 99), Tile::EMPTY);
}

#[test]
fn grid_levels_iterate_row_major_and_fill() {
    let mut layers = Layers::new(2);
    layers.fill_level(1, Tile::new(9, 0));
    assert_eq!(layers.count_non_empty(), CELLS);
    let first = layers.iter_level(1).next().unwrap();
    assert_eq!(first, (0, 0, Tile::new(9, 0)));
    let nth = layers.iter_level(1).nth(GRID_W as usize + 2).unwrap();
    assert_eq!(nth, (2, 1, Tile::new(9, 0)));
}

#[test]
fn levels_are_clamped_to_the_model_limit() {
    let layers = Layers::new(MAX_LEVELS + 10);
    assert_eq!(layers.levels(), MAX_LEVELS);
}
