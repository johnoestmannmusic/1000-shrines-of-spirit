//! The output contract is a promise; these lock it in.

use shrine0016::{ART_PX, CANVAS_H, CANVAS_W, GRID_H, GRID_W, LOGICAL_H, LOGICAL_W, SHRINE_H, SHRINE_W};

#[test]
fn logical_buffer_is_300_square() {
    assert_eq!((LOGICAL_W, LOGICAL_H), (300, 300));
}

#[test]
fn canvas_is_1200_square_and_upscale_is_integral() {
    assert_eq!((CANVAS_W, CANVAS_H), (1200, 1200));
    assert_eq!(ART_PX, 4);
    assert_eq!(CANVAS_W, LOGICAL_W * ART_PX);
    assert_eq!(CANVAS_H, LOGICAL_H * ART_PX);
}

#[test]
fn grid_is_24_square_and_shrine_is_16_square() {
    assert_eq!((GRID_W, GRID_H), (24, 24));
    assert_eq!((SHRINE_W, SHRINE_H), (16, 16));
    assert!(SHRINE_W < GRID_W && SHRINE_H < GRID_H);
}

#[test]
fn abi_reports_the_contract() {
    assert_eq!(shrine0016::abi::shrine_abi_version(), 1);
    let packed = shrine0016::abi::shrine_logical_size();
    assert_eq!(packed >> 16, LOGICAL_W);
    assert_eq!(packed & 0xffff, LOGICAL_H);
}
