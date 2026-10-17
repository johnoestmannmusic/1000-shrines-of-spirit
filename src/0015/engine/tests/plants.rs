//! The plant library must be deterministic, stay inside the grid, sit on the
//! right surface, and produce every family.

use shrine0015::plants::{kind, Biome, Plants, MAX_PLANTS};
use shrine0015::shrine::Shrine;
use shrine0015::{GRID_H, GRID_W, SHRINE_H, SHRINE_W};

#[test]
fn generation_is_deterministic_and_seed_sensitive() {
    let shrine = Shrine::generate(1000);
    for seed in 0..12u64 {
        let a = Plants::generate(seed, &shrine, Biome::Moss);
        let b = Plants::generate(seed, &shrine, Biome::Moss);
        assert_eq!(a.hash(), b.hash(), "seed {seed} not reproducible");
    }
    let a = Plants::generate(1, &shrine, Biome::Moss);
    let b = Plants::generate(2, &shrine, Biome::Moss);
    assert_ne!(a.hash(), b.hash());
}

#[test]
fn every_plant_is_inside_the_grid_and_has_substance() {
    for seed in 0..24u64 {
        let shrine = Shrine::generate(seed);
        let plants = Plants::generate(seed, &shrine, Biome::Verdant);
        assert!(!plants.items.is_empty(), "seed {seed} grew nothing");
        assert!(plants.items.len() <= MAX_PLANTS);
        for p in &plants.items {
            assert!((0..GRID_W as i32).contains(&p.gx));
            assert!((0..GRID_H as i32).contains(&p.gy));
            assert!(p.z >= 0);
            assert!(p.height > 0);
            assert!(!p.strokes.is_empty());
            assert_eq!(p.strokes[0].points[0], (0, 0), "strokes should start at the anchor");
            assert!((0.0..1.0).contains(&p.sway.phase));
            assert!(p.sway.amplitude > 0.0);
        }
    }
}

#[test]
fn plants_on_the_shrine_sit_on_its_top() {
    let shrine = Shrine::generate(42);
    let (ox, oy) = shrine.origin();
    let plants = Plants::generate(42, &shrine, Biome::Moss);
    let mut on_shrine = 0;
    for p in &plants.items {
        let inside = p.gx >= ox && p.gx < ox + SHRINE_W as i32 && p.gy >= oy && p.gy < oy + SHRINE_H as i32;
        if inside {
            let expected = shrine.height_at(p.gx - ox, p.gy - oy) as i32 - 1;
            assert_eq!(p.z, expected, "plant at ({},{}) floats or sinks", p.gx, p.gy);
            on_shrine += 1;
        } else {
            assert_eq!(p.z, 0);
        }
    }
    assert!(on_shrine > 0, "no plant grew on the shrine");
}

#[test]
fn every_family_appears_and_sways_apart() {
    for biome in Biome::ALL {
        let shrine = Shrine::generate(5);
        let mut seen = [false; 4];
        let mut phases = Vec::new();
        for seed in 0..16u64 {
            let plants = Plants::generate(seed, &shrine, biome);
            seen[kind::GRASS as usize] |= plants.count_of(kind::GRASS) > 0;
            seen[kind::FERN as usize] |= plants.count_of(kind::FERN) > 0;
            seen[kind::VINE as usize] |= plants.count_of(kind::VINE) > 0;
            seen[kind::FUNGUS as usize] |= plants.count_of(kind::FUNGUS) > 0;
            for p in &plants.items {
                phases.push(p.sway.phase);
            }
        }
        assert!(seen.iter().all(|&s| s), "{biome:?} never grew every family");
        let first = phases.first().copied().unwrap();
        assert!(phases.iter().any(|&p| p != first), "sway phases are all identical");
    }
}

#[test]
fn a_denser_biome_grows_more_than_a_sparse_one() {
    let shrine = Shrine::generate(9);
    let count = |b: Biome| Plants::generate(9, &shrine, b).items.len();
    assert!(count(Biome::Verdant) > count(Biome::Fungal));
}
