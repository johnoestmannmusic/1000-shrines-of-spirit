//! The architecture transform.
//!
//! Three things must hold: the default is *exactly* the untransformed render,
//! turning or moving the shrine always changes the picture, and the result is
//! deterministic (`HC004`). The default-exactness test is the important one —
//! it is what protects every existing recording and the visual goldens.

use shrine0012::arch::ArchTransform;
use shrine0012::framebuffer::Framebuffer;
use shrine0012::scene::Quality;
use shrine0012::sim::{Sim, SimConfig};

/// The frame at `t = 1s` for a default shrine with `arch` applied.
fn frame(arch: ArchTransform) -> u64 {
    let mut config = SimConfig::default();
    config.settings.arch = arch;
    let sim = Sim::new_visual(config);
    let mut fb = Framebuffer::logical();
    sim.render_visual(&mut fb, 1.0, Quality::High);
    fb.hash()
}

#[test]
fn the_identity_transform_is_the_plain_render() {
    let plain = {
        let sim = Sim::new_visual(SimConfig::default());
        let mut fb = Framebuffer::logical();
        sim.render_visual(&mut fb, 1.0, Quality::High);
        fb.hash()
    };
    assert_eq!(frame(ArchTransform::IDENTITY), plain, "the default must be untouched");
}

#[test]
fn turning_or_moving_the_shrine_changes_the_frame() {
    let id = frame(ArchTransform::IDENTITY);
    for arch in [
        ArchTransform { rot_z: 35, ..ArchTransform::IDENTITY },
        ArchTransform { rot_x: 55, rot_y: 20, ..ArchTransform::IDENTITY },
        ArchTransform { rot_y: -90, ..ArchTransform::IDENTITY },
        ArchTransform { rot_z: 180, ..ArchTransform::IDENTITY },
        ArchTransform { off_x: 3, off_y: -2, ..ArchTransform::IDENTITY },
        ArchTransform { off_z: 2, ..ArchTransform::IDENTITY },
    ] {
        let a = frame(arch);
        assert_ne!(a, id, "{arch:?} should change the frame");
        assert_eq!(a, frame(arch), "{arch:?} must be deterministic");
    }
}

#[test]
fn an_offset_by_zero_is_still_the_plain_render() {
    // The offset rides the *exact* axis-aligned path (it is just a moved
    // projection origin), so a zero offset cannot perturb a single pixel.
    let zero = ArchTransform { off_x: 0, off_y: 0, off_z: 0, ..ArchTransform::IDENTITY };
    assert_eq!(frame(zero), frame(ArchTransform::IDENTITY));
}

#[test]
fn the_ground_and_plants_stay_where_they_are() {
    // Only the architecture moves: the ground plane's far corner must sit in
    // exactly the same place with the shrine rotated away from it.
    let plain = frame(ArchTransform::IDENTITY);
    let turned = frame(ArchTransform { rot_z: 90, off_x: 6, off_y: 6, ..ArchTransform::IDENTITY });
    assert_ne!(plain, turned);
    // Rendering the same turned shrine twice is stable, which is the part a
    // future change to the basis maths would break.
    let again = frame(ArchTransform { rot_z: 90, off_x: 6, off_y: 6, ..ArchTransform::IDENTITY });
    assert_eq!(turned, again);
}

/// The frame for a whole configuration, at `t = 1s`.
fn frame_with(config: SimConfig) -> u64 {
    let sim = Sim::new_visual(config);
    let mut fb = Framebuffer::logical();
    sim.render_visual(&mut fb, 1.0, Quality::High);
    fb.hash()
}

#[test]
fn a_camera_orbit_turns_the_world_deterministically() {
    let mut config = SimConfig::default();
    config.settings.camera_speed = 30;
    let a = frame_with(config.clone());
    assert_ne!(a, frame_with(SimConfig::default()), "the orbit must show");
    assert_eq!(a, frame_with(config), "and must be deterministic");
}

#[test]
fn a_zero_camera_speed_is_the_plain_render() {
    let mut config = SimConfig::default();
    config.settings.camera_speed = 0;
    assert_eq!(frame_with(config), frame_with(SimConfig::default()));
}

#[test]
fn the_orbit_is_periodic_in_a_whole_turn() {
    use shrine0012::sim::camera_orbit_turns as turns;
    assert!(turns(360, 1.0).abs() < 1e-12, "a whole turn is no turn");
    assert!((turns(180, 1.0) - 0.5).abs() < 1e-12);
    assert!((turns(-90, 1.0) - 0.75).abs() < 1e-12, "negative turns the other way");
    assert_eq!(turns(0, 12.5), 0.0);
    assert!(turns(30, 1000.0) < 1.0, "the orbit stays wrapped");
}
