//! The explorer's map: every spot is a fixed shrine, neighbours differ only a
//! little, and the home stays the home.

use shrine_explorer::explore::{at, changed_groups, resolve, MAP_CAMERA_SPEED};
use shrine_explorer::framebuffer::Framebuffer;
use shrine_explorer::randomize::Group;
use shrine_explorer::recipe;
use shrine_explorer::scene::Quality;
use shrine_explorer::sim::{Sim, SimConfig};

fn home() -> SimConfig {
    let mut c = SimConfig::default();
    c.settings.master.enabled = true;
    c.settings.master.tape = true;
    c
}

#[test]
fn every_spot_is_always_the_same_shrine() {
    let h = home();
    for (x, y) in [(1, 0), (-7, 3), (40, -12), (-1_000_000, 999)] {
        assert_eq!(recipe::encode(&at(&h, x, y)), recipe::encode(&at(&h, x, y)), "{x},{y}");
    }
}

#[test]
fn home_is_the_home() {
    let h = home();
    assert_eq!(recipe::encode(&at(&h, 0, 0)), recipe::encode(&h));
    assert_eq!(recipe::encode(&resolve(h.clone())), recipe::encode(&h));
}

#[test]
fn neighbours_differ_a_little_and_always_in_architecture() {
    let h = home();
    let mut total = 0usize;
    let mut steps = 0usize;
    for x in -10..10 {
        for y in -10..10 {
            let a = at(&h, x, y);
            for (dx, dy) in [(1, 0), (0, 1)] {
                let b = at(&h, x + dx, y + dy);
                let changed = changed_groups(&a, &b);
                assert!(changed.contains(&Group::Seed), "{x},{y}: the same architecture next door");
                let others = changed.iter().filter(|g| **g != Group::Seed).count();
                assert!(others <= 8, "{x},{y}: {others} groups changed in one step");
                total += others;
                steps += 1;
            }
        }
    }
    let mean = total as f64 / steps as f64;
    assert!((1.0..=3.0).contains(&mean), "a step changes {mean:.2} groups on average");
}

#[test]
fn master_fx_and_camera_never_wander() {
    let h = home();
    for (x, y) in [(1, 0), (5, 5), (-30, 17)] {
        let c = at(&h, x, y);
        assert_eq!(c.settings.master, h.settings.master);
        assert_eq!(c.settings.camera_speed, MAP_CAMERA_SPEED);
        assert_eq!(c.settings.start_s, 0);
        assert_eq!(c.settings.map, (0, 0), "a resolved shrine stands on its own");
    }
}

#[test]
fn the_home_region_keeps_the_home_values() {
    // The biome regions are twelve cells: right next to home, the biome is
    // (almost always) the home's, while far away it varies.
    let h = home();
    let near = [(1, 0), (0, 1), (-1, 0), (0, -1)].iter().filter(|(x, y)| at(&h, *x, *y).biome == h.biome).count();
    assert!(near >= 2, "only {near} of the four neighbours kept the home's biome");
    let far: std::collections::HashSet<_> = (0..40).map(|k| format!("{:?}", at(&h, k * 23, -k * 17).biome)).collect();
    assert!(far.len() >= 3, "the map never leaves the home's biome");
}

#[test]
fn a_spot_is_reached_by_its_coordinates_and_plays() {
    // A config whose map position is set plays the shrine at that spot.
    let h = home();
    let mut there = h.clone();
    there.settings.map = (3, -2);
    let frame = |c: SimConfig| {
        let sim = Sim::new_visual(c);
        let mut fb = Framebuffer::logical();
        sim.render_visual(&mut fb, 1.0, Quality::High);
        fb.hash()
    };
    assert_eq!(frame(there.clone()), frame(at(&h, 3, -2)));
    assert_ne!(frame(there), frame(h.clone()));
    // And through the recipe, as the page and the seed files carry it.
    let mut words_cfg = h;
    words_cfg.settings.map = (3, -2);
    let back = recipe::decode(&recipe::encode(&words_cfg));
    assert_eq!(back.settings.map, (3, -2));
}
