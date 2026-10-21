//! The authored architecture.
//!
//! The important property is the first one: an empty model must leave the
//! generated shrine — and so every existing render and recording — untouched.

use shrine0016::framebuffer::Framebuffer;
use shrine0016::scene::Quality;
use shrine0016::shrine::{kind, Shrine};
use shrine0016::shrine_model::ShrineModel;
use shrine0016::sim::{Sim, SimConfig};

fn frame(config: SimConfig) -> u64 {
    let sim = Sim::new_visual(config);
    let mut fb = Framebuffer::logical();
    sim.render_visual(&mut fb, 1.0, Quality::High);
    fb.hash()
}

#[test]
fn an_empty_model_leaves_the_generated_shrine_alone() {
    let mut config = SimConfig::default();
    config.settings.model = ShrineModel::EMPTY;
    assert_eq!(frame(config), frame(SimConfig::default()));
}

#[test]
fn authoring_the_generated_shrine_changes_nothing() {
    // Reading the generator into a model and rendering from it must land on the
    // very same pixels: that is what makes the editor a no-op until you edit.
    let base = SimConfig::default();
    let model = ShrineModel::from_shrine(&Shrine::generate(base.seed));
    let mut config = SimConfig::default();
    config.settings.model = model;
    assert_eq!(frame(config), frame(SimConfig::default()));
}

#[test]
fn an_edited_model_changes_the_frame_deterministically() {
    let base = SimConfig::default();
    let mut model = ShrineModel::from_shrine(&Shrine::generate(base.seed));
    model.set(8, 8, 6, kind::PILLAR, 0);
    model.set(2, 2, 1, kind::FLOOR, 1);

    let mut config = SimConfig::default();
    config.settings.model = model;
    let a = frame(config.clone());
    assert_ne!(a, frame(SimConfig::default()), "an edit must show");
    assert_eq!(a, frame(config.clone()), "and must be deterministic");

    // The model travels through the recipe intact.
    let words = shrine0016::recipe::encode(&config);
    let back = shrine0016::recipe::decode(&words);
    assert_eq!(back.settings.model, model);
}

#[test]
fn clearing_a_cell_removes_its_column() {
    let base = SimConfig::default();
    let mut model = ShrineModel::from_shrine(&Shrine::generate(base.seed));
    // Height 0 leaves the ground alone: the column that was there is gone.
    model.set(8, 8, 0, kind::FLOOR, 0);
    let mut config = SimConfig::default();
    config.settings.model = model;
    let holed = frame(config.clone());
    assert_ne!(holed, frame(SimConfig::default()), "removing a column must show");
    assert_eq!(holed, frame(config), "and must be deterministic");

    // Blanking every square is a much emptier picture again.
    let mut blank = ShrineModel::EMPTY;
    blank.blank();
    let mut config = SimConfig::default();
    config.settings.model = blank;
    assert_ne!(frame(config), frame(SimConfig::default()), "a blank plan must show");
}

#[test]
fn blocks_below_the_ground_are_obscured_by_the_ground_plane() {
    // The ground plane is at z = 0, so anything below it is hidden behind it.
    // Digging a flat square therefore changes nothing you can see: its cap sits
    // at ground level in the same tile, and the descending sides are behind the
    // plane everywhere they would otherwise spill.
    let base = SimConfig::default();
    let shrine = Shrine::generate(base.seed);
    let (lx, ly) = (0..16)
        .flat_map(|x| (0..16).map(move |y| (x, y)))
        .find(|(x, y)| shrine.height_at(*x, *y) == 1)
        .expect("the generator always leaves some floor");

    let mut dug = ShrineModel::from_shrine(&shrine);
    dug.set(lx as usize, ly as usize, -3, kind::FLOOR, 0);
    let mut config = SimConfig::default();
    config.settings.model = dug;
    assert_eq!(frame(config.clone()), frame(SimConfig::default()), "the plane must hide the descent");
    assert_eq!(frame(config), frame(SimConfig::default()), "and must stay deterministic");

    // The control: a raised column on the same square does show, which proves
    // the model is reaching the renderer (so this test is not passing for the
    // wrong reason).
    let mut up = ShrineModel::from_shrine(&shrine);
    up.set(lx as usize, ly as usize, 5, kind::FLOOR, 0);
    let mut config = SimConfig::default();
    config.settings.model = up;
    assert_ne!(frame(config), frame(SimConfig::default()), "a raised column must show");
}

#[test]
fn a_descent_follows_the_architecture_without_leaving_a_hole() {
    // A descending square's cap sits flush with the ground and its sides are
    // under the plateau, so it must look exactly like a flat square — wherever
    // the architecture transform puts it. Before this held, the ground kept a
    // hole at the untransformed square, and the page's sky showed through it.
    use shrine0016::arch::ArchTransform;
    let base = SimConfig::default();
    let shrine = Shrine::generate(base.seed);
    // An interior square, well inside the plateau, so its sides stay hidden.
    let (lx, ly) = (3..10)
        .flat_map(|x| (3..10).map(move |y| (x, y)))
        .find(|(x, y)| shrine.height_at(*x, *y) == 1)
        .expect("the generator always leaves some floor");
    for arch in [
        // The axis-aligned path, moved.
        ArchTransform { off_x: 2, ..ArchTransform::IDENTITY },
        // The general (rotated) path.
        ArchTransform { rot_z: 10, ..ArchTransform::IDENTITY },
    ] {
        let mut flat = ShrineModel::from_shrine(&shrine);
        flat.set(lx as usize, ly as usize, 1, kind::FLOOR, 0);
        let mut dug = ShrineModel::from_shrine(&shrine);
        dug.set(lx as usize, ly as usize, -3, kind::FLOOR, 0);
        let mut a = SimConfig::default();
        a.settings.arch = arch;
        a.settings.model = flat;
        let mut b = a.clone();
        b.settings.model = dug;
        assert_eq!(frame(b), frame(a), "{arch:?}: a dug square must read as a flat one");
    }
}

