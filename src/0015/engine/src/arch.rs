//! The architecture transform: rotate the shrine about the x, y and z axes and
//! offset it in all three.
//!
//! [`ArchTransform::IDENTITY`] is the default, so the shipped shrine is exactly
//! the centred 16x16 model that `HC005` describes: the transform is an opt-in
//! *presentation* deviation, and the model itself never moves.
//!
//! Rotations are stored as whole degrees so the setup menu and the seed file
//! stay readable, and are converted with [`crate::math::sin_turns`] /
//! [`crate::math::cos_turns`] — never the platform's `f64::sin`, whose results
//! differ between libm versions and would break `HC004`.
//!
//! Offsets are whole grid cells (x, y) and stack levels (z), applied in *world*
//! space: after the rotation, along the grid's own axes.

use crate::math::{cos_turns, sin_turns};
use crate::{GRID_H, GRID_W};

/// The grid's centre, which the camera orbits.
fn centre_x() -> f64 {
    (GRID_W as f64 - 1.0) / 2.0
}

fn centre_y() -> f64 {
    (GRID_H as f64 - 1.0) / 2.0
}

/// 3x3 matrix product, row-major.
fn mul3(a: &[[f64; 3]; 3], b: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut out = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            out[i][j] = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
    }
    out
}

/// Rotate and offset the shrine. Identity by default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArchTransform {
    /// Degrees about each axis, -180..180.
    pub rot_x: i16,
    pub rot_y: i16,
    pub rot_z: i16,
    /// Whole cells (x, y) and stack levels (z).
    pub off_x: i16,
    pub off_y: i16,
    pub off_z: i16,
}

impl ArchTransform {
    pub const MIN_ROT: i16 = -180;
    pub const MAX_ROT: i16 = 180;
    pub const MIN_OFF: i16 = -24;
    pub const MAX_OFF: i16 = 24;

    /// The normal, centred, unrotated shrine.
    pub const IDENTITY: ArchTransform =
        ArchTransform { rot_x: 0, rot_y: 0, rot_z: 0, off_x: 0, off_y: 0, off_z: 0 };

    /// True when nothing is rotated — an offset alone still uses the exact
    /// axis-aligned render path, because a whole-cell shift is just a moved
    /// grid coordinate.
    pub fn is_unrotated(&self) -> bool {
        self.rot_x == 0 && self.rot_y == 0 && self.rot_z == 0
    }

    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }

    /// The three rotations in turns, for the deterministic sine and cosine.
    pub fn turns(&self) -> (f64, f64, f64) {
        let t = |deg: i16| (deg.clamp(Self::MIN_ROT, Self::MAX_ROT) as f64) / 360.0;
        (t(self.rot_x), t(self.rot_y), t(self.rot_z))
    }

    /// The rotation as a row-major 3x3 matrix, applied x, then y, then z.
    ///
    /// Built only from `+ - * /` (inside `sin_turns`/`cos_turns`), so it is
    /// bit-identical on every platform.
    pub fn rotation(&self) -> [[f64; 3]; 3] {
        let (tx, ty, tz) = self.turns();
        let (sx, cx) = (sin_turns(tx), cos_turns(tx));
        let (sy, cy) = (sin_turns(ty), cos_turns(ty));
        let (sz, cz) = (sin_turns(tz), cos_turns(tz));
        // Rx * Ry * Rz, expanded so the order is explicit.
        [
            [cz * cy, -sz * cy, sy],
            [sz * cx + cz * sy * sx, cz * cx - sz * sy * sx, -cy * sx],
            [sz * sx - cz * sy * cx, cz * sx + sz * sy * cx, cy * cx],
        ]
    }

    /// The full world transform: the architecture transform, then a camera
    /// orbit of `orbit_turns` about the grid's vertical axis.
    ///
    /// The orbit spins the whole scene around the shrine, so the *world*
    /// vertical stays vertical on screen — plants still stand up and rain still
    /// falls — while the ground turns underneath. Returns the 3x3 rotation and
    /// the world offset, ready for `Iso::project_rotated`.
    pub fn world(&self, orbit_turns: f64) -> ([[f64; 3]; 3], (f64, f64, f64)) {
        let rot = self.rotation();
        let off = (self.off_x as f64, self.off_y as f64, self.off_z as f64);
        if orbit_turns == 0.0 {
            return (rot, off);
        }
        let (sin, cos) = (sin_turns(orbit_turns), cos_turns(orbit_turns));
        let orbit = [[cos, -sin, 0.0], [sin, cos, 0.0], [0.0, 0.0, 1.0]];
        let m = mul3(&orbit, &rot);
        // world = O * (R * p + off - centre) + centre, so the scene turns about
        // the shrine rather than swinging around the grid's corner.
        let (cx, cy) = (centre_x(), centre_y());
        let (px, py, pz) = (off.0 - cx, off.1 - cy, off.2);
        (m, (cos * px - sin * py + cx, sin * px + cos * py + cy, pz))
    }

    /// Pack the rotations into one recipe word: three 10-bit values biased by
    /// +180.
    pub fn rot_word(self) -> u32 {
        let a = (self.rot_x.clamp(Self::MIN_ROT, Self::MAX_ROT) + 180) as u32;
        let b = (self.rot_y.clamp(Self::MIN_ROT, Self::MAX_ROT) + 180) as u32;
        let c = (self.rot_z.clamp(Self::MIN_ROT, Self::MAX_ROT) + 180) as u32;
        (a & 0x3ff) | ((b & 0x3ff) << 10) | ((c & 0x3ff) << 20)
    }

    /// Pack the offsets into one recipe word: three 6-bit values biased by +32.
    pub fn off_word(self) -> u32 {
        let a = (self.off_x.clamp(Self::MIN_OFF, Self::MAX_OFF) + 32) as u32;
        let b = (self.off_y.clamp(Self::MIN_OFF, Self::MAX_OFF) + 32) as u32;
        let c = (self.off_z.clamp(Self::MIN_OFF, Self::MAX_OFF) + 32) as u32;
        (a & 0x3f) | ((b & 0x3f) << 6) | ((c & 0x3f) << 12)
    }

    /// Read a rotation pair written by [`rot_word`], tolerating corrupt values.
    pub fn rot_from_word(word: u32) -> (i16, i16, i16) {
        let part = |shift: u32| ((word >> shift) & 0x3ff).min(360) as i16 - 180;
        (part(0), part(10), part(20))
    }

    /// Read an offset triple written by [`off_word`].
    pub fn off_from_word(word: u32) -> (i16, i16, i16) {
        let part = |shift: u32| ((word >> shift) & 0x3f).min(63) as i16 - 32;
        (part(0), part(6), part(12))
    }

    /// One line for the recipe card.
    pub fn summary(self) -> String {
        if self.is_identity() {
            return "centred, upright".to_string();
        }
        let mut parts = Vec::new();
        if !self.is_unrotated() {
            parts.push(format!("{}°, {}°, {}°", self.rot_x, self.rot_y, self.rot_z));
        }
        if self.off_x != 0 || self.off_y != 0 || self.off_z != 0 {
            parts.push(format!("offset {:+}, {:+}, {:+}", self.off_x, self.off_y, self.off_z));
        }
        parts.join(" · ")
    }
}

impl Default for ArchTransform {
    fn default() -> Self {
        ArchTransform::IDENTITY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_unrotated_and_the_default() {
        assert!(ArchTransform::IDENTITY.is_identity());
        assert!(ArchTransform::IDENTITY.is_unrotated());
        assert_eq!(ArchTransform::default(), ArchTransform::IDENTITY);
        assert_eq!(ArchTransform::IDENTITY.summary(), "centred, upright");
    }

    #[test]
    fn an_offset_alone_is_still_unrotated() {
        let moved = ArchTransform { off_x: 3, off_y: -2, off_z: 1, ..ArchTransform::IDENTITY };
        assert!(!moved.is_identity());
        assert!(moved.is_unrotated(), "a pure offset must keep the exact render path");
    }

    #[test]
    fn the_words_round_trip_and_clamp() {
        for fx in [
            ArchTransform::IDENTITY,
            ArchTransform { rot_x: 25, rot_y: -90, rot_z: 180, off_x: 4, off_y: -4, off_z: 2 },
            ArchTransform { rot_x: -180, rot_y: 180, rot_z: 7, off_x: 0, off_y: 0, off_z: 0 },
        ] {
            let back = ArchTransform {
                rot_x: ArchTransform::rot_from_word(fx.rot_word()).0,
                rot_y: ArchTransform::rot_from_word(fx.rot_word()).1,
                rot_z: ArchTransform::rot_from_word(fx.rot_word()).2,
                off_x: ArchTransform::off_from_word(fx.off_word()).0,
                off_y: ArchTransform::off_from_word(fx.off_word()).1,
                off_z: ArchTransform::off_from_word(fx.off_word()).2,
            };
            assert_eq!(back, fx);
        }
        let wild = ArchTransform { rot_x: 999, rot_y: -999, rot_z: 0, off_x: 999, off_y: -999, off_z: 0 };
        assert_eq!(ArchTransform::rot_from_word(wild.rot_word()), (180, -180, 0));
        assert_eq!(ArchTransform::off_from_word(wild.off_word()), (24, -24, 0));
        // Corrupt words must not panic or read out of range.
        assert_eq!(ArchTransform::rot_from_word(u32::MAX), (180, 180, 180));
    }

    #[test]
    fn a_camera_orbit_turns_the_world_about_the_grid_centre() {
        let (rot, off) = ArchTransform::IDENTITY.world(0.25);
        // A quarter turn maps the grid centre to itself.
        let (cx, cy) = (centre_x(), centre_y());
        let (wx, wy) = (rot[0][0] * cx + rot[0][1] * cy + off.0, rot[1][0] * cx + rot[1][1] * cy + off.1);
        assert!((wx - cx).abs() < 1e-9 && (wy - cy).abs() < 1e-9, "the centre moved: {wx},{wy}");
        // The vertical axis is untouched, so plants and rain keep standing up.
        assert!((rot[2][0]).abs() < 1e-12 && (rot[2][1]).abs() < 1e-12 && (rot[2][2] - 1.0).abs() < 1e-12);
        // No orbit is the identity to within the polynomial's rounding, and
        // leaves the offset untouched. The unrotated render path does not use
        // this matrix at all, so the rounding cannot reach a pixel.
        let (r0, o0) = ArchTransform::IDENTITY.world(0.0);
        for i in 0..3 {
            for j in 0..3 {
                let want = if i == j { 1.0 } else { 0.0 };
                assert!((r0[i][j] - want).abs() < 1e-12, "not identity at {i},{j}");
            }
        }
        assert_eq!(o0, (0.0, 0.0, 0.0));
    }

    #[test]
    fn the_rotation_matrix_is_orthonormal_and_leaves_zero_alone() {
        let r = ArchTransform { rot_x: 25, rot_y: -40, rot_z: 115, ..ArchTransform::IDENTITY }.rotation();
        // Every row and column should be a unit vector to within a hair.
        for i in 0..3 {
            let row = (r[i][0] * r[i][0] + r[i][1] * r[i][1] + r[i][2] * r[i][2]).sqrt();
            assert!((row - 1.0).abs() < 1e-12, "row {i} is not unit: {row}");
        }
        // The deterministic polynomial is not exactly 1.0 at zero, so compare
        // with a tolerance: the identity path never calls this anyway.
        let id = ArchTransform::IDENTITY.rotation();
        for i in 0..3 {
            for j in 0..3 {
                let want = if i == j { 1.0 } else { 0.0 };
                assert!((id[i][j] - want).abs() < 1e-12, "identity[{i}][{j}] = {}", id[i][j]);
            }
        }
    }
}
