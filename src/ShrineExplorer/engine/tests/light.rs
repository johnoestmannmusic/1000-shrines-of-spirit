//! Materials and light: textures fixed to the blocks, shadows that fall away
//! from the sun (and barely at all in fog), the altar's glow, the dither.

use shrine_explorer::framebuffer::Framebuffer;
use shrine_explorer::light::{dither, Rig, Setup};
use shrine_explorer::material::{Albedo, Block, Exposure, Materials};
use shrine_explorer::palette::Palette;
use shrine_explorer::plants::Biome;
use shrine_explorer::raster::{Face, Surface};
use shrine_explorer::scene::Quality;
use shrine_explorer::sim::{Sim, SimConfig};
use shrine_explorer::weather::WeatherKind;

fn rig(kind: WeatherKind, tall: Option<(usize, usize, f64)>, glow: f64) -> Rig {
    let pal = Palette::from_seed(1000);
    let mut tops = vec![0.0; 24 * 24];
    if let Some((x, y, h)) = tall {
        tops[y * 24 + x] = h;
    }
    Rig::new(Setup {
        kind,
        biome: Biome::Moss,
        tops,
        cast: true,
        orbit: 0.0,
        glow_at: Some([12.0, 12.0, 1.0]),
        glow,
        accent: pal.accent,
        fog: pal.fog,
        fog_level: 0.5,
        flash: 0.0,
        t: 0.0,
        wind: 0.0,
    })
}

const GREY: Albedo = Albedo { colour: [140, 140, 140, 255], emissive: 0.0, spec: 0.0 };

/// Mean brightness of the ground over a small patch around `(x, y)`, so the
/// dither averages out.
fn ground(r: &Rig, x: f64, y: f64) -> f64 {
    let mut sum = 0.0;
    for j in 0..8 {
        for i in 0..8 {
            let (sx, sy) = (x - 0.2 + i as f64 * 0.05, y - 0.2 + j as f64 * 0.05);
            let s = Surface { face: Face::Top, x: sx, y: sy, z: 0.0, px: i, py: j, depth: 0.0 };
            let c = r.shade(&s, [sx, sy, 0.0], [0.0, 0.0, 1.0], &GREY);
            sum += c[0] + c[1] + c[2];
        }
    }
    sum / 64.0
}

#[test]
fn a_column_casts_its_shadow_away_from_the_sun() {
    // Clear's sun stands toward (-0.6, 0.8), so the shadow falls toward
    // (+0.6, -0.8): beside the column on that side, not on the other.
    let r = rig(WeatherKind::Clear, Some((10, 10, 4.0)), 0.0);
    let shadowed = ground(&r, 10.6 + 0.3, 10.0 - 0.8);
    let sunny = ground(&r, 10.0 - 0.9, 10.0 + 0.9);
    assert!(shadowed < sunny * 0.85, "shadowed {shadowed:.1} vs sunny {sunny:.1}");
}

#[test]
fn fog_casts_almost_no_shadow() {
    let r = rig(WeatherKind::Fog, Some((10, 10, 4.0)), 0.0);
    let shadowed = ground(&r, 10.9, 9.2);
    let sunny = ground(&r, 9.1, 10.9);
    assert!(shadowed > sunny * 0.9, "fog shadow too dark: {shadowed:.1} vs {sunny:.1}");
}

#[test]
fn dusk_shadows_reach_further_than_clear_ones() {
    // Dusk's low sun stands toward (0.85, -0.5): its shadow runs the other
    // way and much further.
    let far = (10.0 - 0.85 * 3.5, 10.0 + 0.5 * 3.5);
    let dusk = rig(WeatherKind::Dusk, Some((10, 10, 4.0)), 0.0);
    let open = rig(WeatherKind::Dusk, None, 0.0);
    assert!(ground(&dusk, far.0, far.1) < ground(&open, far.0, far.1) * 0.85, "no long dusk shadow");
}

#[test]
fn the_altar_glows_with_the_drone() {
    let dim = ground(&rig(WeatherKind::Fog, None, 0.0), 12.6, 12.4);
    let lit = ground(&rig(WeatherKind::Fog, None, 1.0), 12.6, 12.4);
    assert!(lit > dim * 1.05, "glow {lit:.1} vs none {dim:.1}");
}

#[test]
fn light_falls_off_in_dithered_steps() {
    for t in [0.03, 0.5, 0.97] {
        for i in 0..100 {
            let q = dither(i as f64 / 100.0, 4.0, t) * 4.0;
            assert_eq!(q, q.round(), "{q} is not a whole step");
        }
    }
}

#[test]
fn textures_are_fixed_to_the_blocks_not_the_screen() {
    // The same point of a face has the same colour wherever it lands on
    // screen, so a texture cannot crawl as the camera turns.
    let m = Materials::new(1000, &Palette::from_seed(1000), Biome::Moss);
    let ex = Exposure { cover: 0.0, rain: 0.3 };
    for kind in 0..=7u8 {
        for face in [Face::Top, Face::PosX, Face::PosY] {
            let b = Block { kind, variant: 2, gx: 9, gy: 11, top: 3.0 };
            let at = |px, py| Surface { face, x: 9.2, y: 10.7, z: 1.4, px, py, depth: 0.0 };
            assert_eq!(m.albedo(&b, &at(3, 4), ex), m.albedo(&b, &at(250, 17), ex), "kind {kind} {face:?}");
        }
    }
    // And blocks differ from one another.
    let a = m.albedo(&Block { kind: 2, variant: 0, gx: 5, gy: 5, top: 3.0 }, &Surface { face: Face::PosY, x: 5.1, y: 5.5, z: 1.5, px: 0, py: 0, depth: 0.0 }, ex);
    let differs = (0..40).any(|k| {
        let b = Block { kind: 2, variant: 0, gx: 6 + k, gy: 5, top: 3.0 };
        m.albedo(&b, &Surface { face: Face::PosY, x: 6.1 + k as f64, y: 5.5, z: 1.5, px: 0, py: 0, depth: 0.0 }, ex) != a
    });
    assert!(differs);
}

#[test]
fn every_weather_renders_deterministically_however_the_shrine_is_turned() {
    use shrine_explorer::arch::ArchTransform;
    for kind in WeatherKind::ALL {
        for (orbit, arch) in [(0, ArchTransform::IDENTITY), (20, ArchTransform::IDENTITY), (0, ArchTransform { rot_x: 15, rot_z: 40, ..ArchTransform::IDENTITY })] {
            let mut c = SimConfig::default();
            c.weather = kind;
            c.settings.camera_speed = orbit;
            c.settings.arch = arch;
            let sim = Sim::new_visual(c);
            let (mut a, mut b) = (Framebuffer::logical(), Framebuffer::logical());
            sim.render_visual(&mut a, 6.0, Quality::High);
            sim.render_visual(&mut b, 6.0, Quality::High);
            assert_eq!(a, b, "{kind:?} orbit {orbit} {arch:?}");
        }
    }
}
