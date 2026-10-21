//! Determinism and quality of the seeded building blocks.

use shrine0016::palette::{hsl, wrap01, Palette};
use shrine0016::rng::Rng;

#[test]
fn same_seed_is_bit_identical() {
    assert_eq!(Palette::from_seed(12345), Palette::from_seed(12345));
}

#[test]
fn different_seeds_give_different_palettes() {
    let base = Palette::from_seed(1000);
    for seed in [1u64, 2, 3, 999, 0xDEAD_BEEF, u64::MAX] {
        assert_ne!(base, Palette::from_seed(seed), "seed {seed} repeated the palette");
    }
}

#[test]
fn every_role_is_opaque_and_in_range() {
    for seed in 0..64u64 {
        let p = Palette::from_seed(seed);
        for c in [p.sky_top, p.sky_bottom, p.fog, p.ground, p.ground_dark, p.stone, p.stone_light, p.stone_dark, p.foliage_dark, p.foliage, p.foliage_light, p.water, p.accent] {
            assert_eq!(c[3], 255, "seed {seed} produced a translucent role");
        }
    }
}

#[test]
fn foliage_reads_against_the_sky() {
    // The whole composition depends on growth being visible. Keep a real
    // luminance gap between the lightest foliage and the lower sky.
    for seed in 0..64u64 {
        let p = Palette::from_seed(seed);
        let gap = (Palette::luminance(p.foliage_light) - Palette::luminance(p.sky_bottom)).abs();
        assert!(gap > 12.0, "seed {seed}: foliage/sky contrast only {gap:.1}");
    }
}

#[test]
fn hsl_hits_the_expected_corners() {
    assert_eq!(hsl(0.0, 1.0, 0.5), [255, 0, 0, 255]);
    assert_eq!(hsl(1.0 / 3.0, 1.0, 0.5), [0, 255, 0, 255]);
    assert_eq!(hsl(2.0 / 3.0, 1.0, 0.5), [0, 0, 255, 255]);
    assert_eq!(hsl(0.0, 0.0, 0.0), [0, 0, 0, 255]);
    assert_eq!(hsl(0.0, 0.0, 1.0), [255, 255, 255, 255]);
}

#[test]
fn wrap_stays_in_the_unit_interval() {
    for i in -50..50 {
        let x = i as f64 * 0.37;
        let w = wrap01(x);
        assert!((0.0..1.0).contains(&w), "{x} -> {w}");
    }
}

#[test]
fn rng_standard_helpers_behave() {
    let mut r = Rng::stream(7, 7);
    for _ in 0..1000 {
        let u = r.unit();
        assert!((0.0..1.0).contains(&u));
        let i = r.int(3, 7);
        assert!((3..=7).contains(&i));
        assert!(r.below(5) < 5);
    }
}
