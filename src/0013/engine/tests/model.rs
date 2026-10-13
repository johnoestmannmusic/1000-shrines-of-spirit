//! The authored architecture.
//!
//! The important property is the first one: an empty model must leave the
//! generated shrine — and so every existing render and recording — untouched.

use shrine0012::framebuffer::Framebuffer;
use shrine0012::scene::Quality;
use shrine0012::shrine::{kind, Shrine};
use shrine0012::shrine_model::ShrineModel;
use shrine0012::sim::{Sim, SimConfig};

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
    let words = shrine0012::recipe::encode(&config);
    let back = shrine0012::recipe::decode(&words);
    assert_eq!(back.settings.model, model);
}

#[test]
fn removing_a_base_opens_a_hole() {
    let base = SimConfig::default();
    let mut model = ShrineModel::from_shrine(&Shrine::generate(base.seed));
    // Height 0 removes the column entirely, so the ground shows through.
    model.set(8, 8, 0, kind::FLOOR, 0);
    let mut config = SimConfig::default();
    config.settings.model = model;
    let holed = frame(config.clone());
    assert_ne!(holed, frame(SimConfig::default()), "a hole must show");
    assert_eq!(holed, frame(config), "and must be deterministic");

    // Blanking every square is a much emptier picture again.
    let mut blank = ShrineModel::EMPTY;
    blank.blank();
    let mut config = SimConfig::default();
    config.settings.model = blank;
    assert_ne!(frame(config), frame(SimConfig::default()), "a blank plan must show");
}
