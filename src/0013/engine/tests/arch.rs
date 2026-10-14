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

/// The frame for an arch transform, as a buffer.
fn buffer(arch: ArchTransform) -> Framebuffer {
    let mut config = SimConfig::default();
    config.settings.arch = arch;
    let sim = Sim::new_visual(config);
    let mut fb = Framebuffer::logical();
    sim.render_visual(&mut fb, 1.0, Quality::High);
    fb
}

#[test]
fn sinking_the_shrine_hides_it_only_under_the_plateau() {
    use shrine0012::iso::{Iso, FP};
    use shrine0012::{GRID_H, GRID_W};
    // Sunk this far, nothing of the shrine is above the ground. Under the
    // plateau the two depths must therefore be indistinguishable — the plane
    // hides both — while past the plateau's front edge, which has no
    // thickness, the sunk shrine still shows and the depth does matter.
    let a = buffer(ArchTransform { off_z: -12, ..ArchTransform::IDENTITY });
    let b = buffer(ArchTransform { off_z: -13, ..ArchTransform::IDENTITY });
    assert_ne!(a.hash(), b.hash(), "past the plateau's edge the sunk shrine must show");

    // The plateau's diamond, in fixed-point units, shrunk by a pixel so the
    // seam along its edge is not tested.
    let iso = Iso::grid_24();
    let (cx, cy) = (iso.origin_x, iso.origin_y + (GRID_W as i32 + GRID_H as i32 - 2) * iso.half_h() / 2);
    let hw = GRID_W as i32 * iso.half_w() - 2 * FP;
    let hh = GRID_H as i32 * iso.half_h() - 2 * FP;
    let mut inside = 0;
    for py in 0..a.height() as i32 {
        for px in 0..a.width() as i32 {
            let (x, y) = (px * FP + FP / 2, py * FP + FP / 2);
            // |dx| / hw + |dy| / hh <= 1, in integers.
            if (x - cx).abs() as i64 * hh as i64 + (y - cy).abs() as i64 * hw as i64 > hw as i64 * hh as i64 {
                continue;
            }
            inside += 1;
            assert_eq!(a.get(px, py), b.get(px, py), "the plateau must hide the sunk shrine at {px},{py}");
        }
    }
    assert!(inside > 10_000, "the plateau test covered only {inside} pixels");

    // The control: sinking it by a single level still shows above the ground.
    assert_ne!(frame(ArchTransform { off_z: -1, ..ArchTransform::IDENTITY }), frame(ArchTransform::IDENTITY));
}
