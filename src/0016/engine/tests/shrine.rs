//! The shrine must always be a valid, centred 16x16 column field, and the
//! generator must be perfectly deterministic.

use shrine0016::shrine::{kind, Shrine, MAX_HEIGHT};
use shrine0016::{GRID_H, GRID_W, SHRINE_H, SHRINE_W};

#[test]
fn footprint_is_exactly_16_by_16_and_grounded() {
    for seed in [0u64, 1, 1000, 42, 0xDEAD_BEEF] {
        let s = Shrine::generate(seed);
        let (ox, oy) = s.origin();
        for ly in 0..SHRINE_H as i32 {
            for lx in 0..SHRINE_W as i32 {
                let t = s.layers.get(ox + lx, oy + ly, 0);
                assert!(!t.is_empty(), "seed {seed}: no ground at ({lx},{ly})");
            }
        }
    }
}

#[test]
fn shrine_is_centred_in_the_grid() {
    let s = Shrine::generate(7);
    let (ox, oy) = s.origin();
    assert_eq!((ox, oy), ((GRID_W - SHRINE_W) as i32 / 2, (GRID_H - SHRINE_H) as i32 / 2));
}

#[test]
fn there_are_no_floating_tiles() {
    for seed in 0..48u64 {
        let s = Shrine::generate(seed);
        for gz in 1..MAX_HEIGHT as i32 {
            for gy in 0..GRID_H as i32 {
                for gx in 0..GRID_W as i32 {
                    if !s.layers.get(gx, gy, gz).is_empty() {
                        assert!(
                            !s.layers.get(gx, gy, gz - 1).is_empty(),
                            "seed {seed}: tile ({gx},{gy},{gz}) floats"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn heights_stay_within_the_model() {
    for seed in 0..48u64 {
        let s = Shrine::generate(seed);
        let (ox, oy) = s.origin();
        for ly in 0..SHRINE_H as i32 {
            for lx in 0..SHRINE_W as i32 {
                let h = s.height_at(lx, ly);
                assert!((1..=MAX_HEIGHT).contains(&h), "seed {seed}: height {h} at ({lx},{ly})");
                assert!(!s.layers.get(ox + lx, oy + ly, h as i32 - 1).is_empty(), "top tile missing");
                assert!(s.layers.get(ox + lx, oy + ly, h as i32).is_empty(), "tile above the top");
            }
        }
    }
}

#[test]
fn the_centre_is_an_altar() {
    for seed in [0u64, 3, 9, 11] {
        let s = Shrine::generate(seed);
        let (ox, oy) = s.origin();
        let c = s.layers.get(ox + SHRINE_W as i32 / 2, oy + SHRINE_H as i32 / 2, s.height_at(SHRINE_W as i32 / 2, SHRINE_H as i32 / 2) as i32 - 1);
        assert_eq!(c.kind, kind::ALTAR, "seed {seed}: centre is not the altar");
    }
}

#[test]
fn generation_is_deterministic_and_seed_sensitive() {
    for seed in 0..16u64 {
        let a = Shrine::generate(seed);
        let b = Shrine::generate(seed);
        assert_eq!(a.hash(), b.hash(), "seed {seed} not reproducible");
        if seed > 0 {
            assert_ne!(a.hash(), Shrine::generate(seed - 1).hash(), "seeds {seed} and {} collide", seed - 1);
        }
    }
    assert!(Shrine::generate(1).hash() != Shrine::generate(2).hash());
}

#[test]
fn walls_rise_above_the_floor() {
    let s = Shrine::generate(1000);
    let (ox, oy) = s.origin();
    // The centre of each perimeter edge should be a wall, unless a doorway fell there.
    let wall_like = |lx: i32, ly: i32| {
        let h = s.height_at(lx, ly);
        let top = s.layers.get(ox + lx, oy + ly, h as i32 - 1);
        h > 1 && top.kind == kind::WALL
    };
    let count = (0..SHRINE_W as i32).filter(|&lx| wall_like(lx, 0)).count();
    assert!(count >= SHRINE_W as usize / 2, "perimeter mostly open: {count} walls");
}
