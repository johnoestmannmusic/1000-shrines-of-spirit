//! The code-overlay setting: the exported artifact source scrolling, waterfall
//! style, over the shrine in the browser.
//!
//! Purely presentational — it never affects audio or the frame — but it is part
//! of the recipe so a built shrine carries its own transparency and speed.

/// The scroll overlay's settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CodeOverlay {
    pub enabled: bool,
    /// Overlay opacity, percent (0-100).
    pub opacity: u8,
    /// Scroll speed, 0-100 (higher is faster).
    pub speed: u8,
    /// Text scale, percent (100 = normal, 200 = double).
    pub scale: u8,
    /// Tilt of the source drift, in degrees. Positive leans to the left and the
    /// text travels along that tilted axis.
    pub angle: i16,
    /// Vertical shift of the whole drift, percent of the canvas height
    /// (negative lifts it). Useful for re-centring a tilted block.
    pub offset: i16,
}

impl CodeOverlay {
    /// Smallest and largest text scale the overlay accepts, in percent.
    pub const MIN_SCALE: u8 = 50;
    pub const MAX_SCALE: u8 = 250;
    /// Smallest and largest tilt, in degrees.
    pub const MIN_ANGLE: i16 = -180;
    pub const MAX_ANGLE: i16 = 180;
    /// Smallest and largest vertical shift, in percent of the canvas height.
    pub const MIN_OFFSET: i16 = -100;
    pub const MAX_OFFSET: i16 = 100;
    pub const OFF: CodeOverlay =
        CodeOverlay { enabled: false, opacity: 0, speed: 50, scale: 200, angle: 0, offset: 0 };
    /// The default: on, very transparent, moderate speed, double size, level.
    pub const DEFAULT: CodeOverlay =
        CodeOverlay { enabled: true, opacity: 14, speed: 45, scale: 200, angle: 0, offset: 0 };

    /// Pack for the recipe and the C ABI: bit 0 enabled, then opacity, speed,
    /// and the text scale in the top byte.
    pub fn word(self) -> u32 {
        (self.enabled as u32)
            | ((self.opacity.min(100) as u32) << 8)
            | ((self.speed.min(100) as u32) << 16)
            | ((self.scale.clamp(Self::MIN_SCALE, Self::MAX_SCALE) as u32) << 24)
    }

    pub fn from_word(word: u32) -> Self {
        // Recipes written before the scale existed leave the top byte zero.
        let raw = ((word >> 24) & 0xff) as u8;
        let scale = if raw == 0 { Self::DEFAULT.scale } else { raw.clamp(Self::MIN_SCALE, Self::MAX_SCALE) };
        CodeOverlay {
            enabled: word & 1 != 0,
            opacity: ((word >> 8) & 0xff).min(100) as u8,
            speed: ((word >> 16) & 0xff).min(100) as u8,
            scale,
            angle: 0,
            offset: 0,
        }
    }

    /// The layout as its own recipe word: the tilt in bits 0-8 (`0..360`,
    /// offset from `-180`) and the vertical shift in bits 9-17.
    pub fn angle_word(self) -> u32 {
        let angle = (self.angle.clamp(Self::MIN_ANGLE, Self::MAX_ANGLE) + 180) as u32;
        let offset = (self.offset.clamp(Self::MIN_OFFSET, Self::MAX_OFFSET) + 100) as u32;
        (angle & 0x1ff) | ((offset & 0x1ff) << 9)
    }

    /// Read the tilt from a [`angle_word`], tolerating corrupt values.
    pub fn angle_from_word(word: u32) -> i16 {
        (word & 0x1ff).min(360) as i16 - 180
    }

    /// Read the vertical shift from a [`angle_word`].
    pub fn offset_from_word(word: u32) -> i16 {
        ((word >> 9) & 0x1ff).min(200) as i16 - 100
    }

    pub fn summary(self) -> String {
        if self.enabled {
            let tilt = if self.angle == 0 { String::new() } else { format!(" · {angle}°", angle = self.angle) };
            let shift = if self.offset == 0 { String::new() } else { format!(" · {off}%", off = self.offset) };
            format!("on · {}% · speed {} · text {}%{tilt}{shift}", self.opacity, self.speed, self.scale)
        } else {
            "off".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_round_trips() {
        // The tilt lives in its own word, so it is not part of this one.
        for fx in [
            CodeOverlay::DEFAULT,
            CodeOverlay::OFF,
            CodeOverlay { enabled: true, opacity: 100, speed: 1, scale: 50, angle: 0, offset: 0 },
        ] {
            assert_eq!(CodeOverlay::from_word(fx.word()), fx);
        }
    }

    #[test]
    fn the_tilt_round_trips_and_is_clamped() {
        for angle in [0, 25, -25, 180, -180, 7] {
            let fx = CodeOverlay { angle, ..CodeOverlay::DEFAULT };
            assert_eq!(CodeOverlay::angle_from_word(fx.angle_word()), angle);
        }
        let wild = CodeOverlay { angle: 900, ..CodeOverlay::DEFAULT };
        assert_eq!(CodeOverlay::angle_from_word(wild.angle_word()), CodeOverlay::MAX_ANGLE);
        assert_eq!(CodeOverlay::angle_from_word(u32::MAX), CodeOverlay::MAX_ANGLE);
    }

    #[test]
    fn the_vertical_shift_round_trips_and_is_clamped() {
        for offset in [0, 25, -25, 100, -100, 5] {
            let fx = CodeOverlay { offset, ..CodeOverlay::DEFAULT };
            assert_eq!(CodeOverlay::offset_from_word(fx.angle_word()), offset);
        }
        let wild = CodeOverlay { offset: 900, ..CodeOverlay::DEFAULT };
        assert_eq!(CodeOverlay::offset_from_word(wild.angle_word()), CodeOverlay::MAX_OFFSET);
        // A version-7 word carries only the tilt; the shift must read as zero.
        let v7 = CodeOverlay { angle: -25, offset: 0, ..CodeOverlay::DEFAULT };
        assert_eq!(CodeOverlay::offset_from_word(v7.angle_word()), 0);
    }

    #[test]
    fn a_recipe_without_a_scale_gets_the_default() {
        // Version 4 words left the top byte zero; they must not read as 0%.
        let old = CodeOverlay { enabled: true, opacity: 20, speed: 30, scale: 100, angle: 0, offset: 0 }.word() & 0x00ff_ffff;
        assert_eq!(CodeOverlay::from_word(old).scale, CodeOverlay::DEFAULT.scale);
    }

    #[test]
    fn extremes_are_clamped() {
        let high = CodeOverlay { enabled: true, opacity: 255, speed: 255, scale: 255, angle: 0, offset: 0 };
        assert_eq!(
            CodeOverlay::from_word(high.word()),
            CodeOverlay { enabled: true, opacity: 100, speed: 100, scale: CodeOverlay::MAX_SCALE, angle: 0, offset: 0 }
        );
        let tiny = CodeOverlay { enabled: true, opacity: 0, speed: 0, scale: 1, angle: 0, offset: 0 };
        assert_eq!(CodeOverlay::from_word(tiny.word()).scale, CodeOverlay::MIN_SCALE);
    }
}
