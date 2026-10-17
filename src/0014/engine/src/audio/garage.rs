//! The 2-step garage mode's own settings: how hard it swings, whether the
//! bass moves in rhythm or holds as a drone, how often the chord stabs come
//! and how bright they are. They only matter when the mode is 2-step garage;
//! the other modes ignore them, so their sound never moves.

/// Swing limits, in percent: the first sixteenth's share of each pair.
pub const MIN_SWING: u8 = 50;
pub const MAX_SWING: u8 = 66;

/// How the bass plays in 2-step garage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BassMode {
    /// The sustained FM bass the other modes use.
    Drone,
    /// A syncopated bassline on the chord roots, octaves and fifths.
    Rhythmic,
}

pub const BASS_MODES: [BassMode; 2] = [BassMode::Drone, BassMode::Rhythmic];

impl BassMode {
    pub fn name(self) -> &'static str {
        match self {
            BassMode::Drone => "drone",
            BassMode::Rhythmic => "rhythmic",
        }
    }
}

/// How often the chord stabs play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stabs {
    Off,
    /// A stab or two a bar, on the pushes.
    Sparse,
    /// Answering phrases, most bars.
    Busy,
}

pub const STABS: [Stabs; 3] = [Stabs::Off, Stabs::Sparse, Stabs::Busy];

impl Stabs {
    pub fn name(self) -> &'static str {
        match self {
            Stabs::Off => "off",
            Stabs::Sparse => "sparse",
            Stabs::Busy => "busy",
        }
    }
}

/// The colour of the stabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StabTone {
    /// A soft, round electric-piano tone.
    Dark,
    /// A bright, glassy bell-organ tone.
    Bright,
}

pub const STAB_TONES: [StabTone; 2] = [StabTone::Dark, StabTone::Bright];

impl StabTone {
    pub fn name(self) -> &'static str {
        match self {
            StabTone::Dark => "dark",
            StabTone::Bright => "bright",
        }
    }
}

/// The key lift: the whole progression steps up a whole tone for a while,
/// then comes home — the classic game-menu lift.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyLift {
    Off,
    /// Four passes at home, four lifted.
    Every4,
    /// Eight passes at home, eight lifted.
    Every8,
}

pub const KEY_LIFTS: [KeyLift; 3] = [KeyLift::Off, KeyLift::Every4, KeyLift::Every8];

impl KeyLift {
    pub fn name(self) -> &'static str {
        match self {
            KeyLift::Off => "off",
            KeyLift::Every4 => "every 4 loops",
            KeyLift::Every8 => "every 8 loops",
        }
    }

    /// Passes through the progression before each step (0 = never).
    pub fn passes(self) -> usize {
        match self {
            KeyLift::Off => 0,
            KeyLift::Every4 => 4,
            KeyLift::Every8 => 8,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GarageSetup {
    /// Swing, percent (50 = straight).
    pub swing: u8,
    pub bass: BassMode,
    pub stabs: Stabs,
    pub tone: StabTone,
    /// 0 = the seed picks; otherwise a 1-based index into `harmony::PROGRESSIONS`.
    pub progression: u8,
    pub lift: KeyLift,
}

impl GarageSetup {
    pub const DEFAULT: GarageSetup =
        GarageSetup { swing: 58, bass: BassMode::Rhythmic, stabs: Stabs::Sparse, tone: StabTone::Bright, progression: 0, lift: KeyLift::Off };

    /// The progression's name, or "seed picks".
    pub fn progression_name(self) -> &'static str {
        match self.progression as usize {
            0 => "seed picks",
            i => crate::harmony::PROGRESSIONS.get(i - 1).map_or("seed picks", |p| p.name),
        }
    }

    /// The swing as a fraction (0.5 .. 0.66).
    pub fn swing_fraction(self) -> f64 {
        self.swing.clamp(MIN_SWING, MAX_SWING) as f64 / 100.0
    }

    /// One recipe word: swing in the low byte, then the bass, the stabs and
    /// their tone.
    pub fn word(self) -> u32 {
        fn idx<T: PartialEq>(all: &[T], v: T) -> u32 {
            all.iter().position(|x| *x == v).unwrap_or(0) as u32
        }
        self.swing as u32
            | idx(&BASS_MODES, self.bass) << 8
            | idx(&STABS, self.stabs) << 10
            | idx(&STAB_TONES, self.tone) << 12
            | (self.progression as u32 & 0x1f) << 13
            | idx(&KEY_LIFTS, self.lift) << 18
    }

    pub fn from_word(w: u32) -> Self {
        GarageSetup {
            swing: ((w & 0xff) as u8).clamp(MIN_SWING, MAX_SWING),
            bass: BASS_MODES[(w >> 8 & 3) as usize % BASS_MODES.len()],
            stabs: STABS[(w >> 10 & 3) as usize % STABS.len()],
            tone: STAB_TONES[(w >> 12 & 1) as usize],
            progression: {
                let p = (w >> 13 & 0x1f) as usize;
                if p > crate::harmony::PROGRESSIONS.len() { 0 } else { p as u8 }
            },
            lift: KEY_LIFTS[(w >> 18 & 3) as usize % KEY_LIFTS.len()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_setup_round_trips_through_its_word() {
        for swing in MIN_SWING..=MAX_SWING {
            for bass in BASS_MODES {
                for stabs in STABS {
                    for tone in STAB_TONES {
                        for progression in 0..=crate::harmony::PROGRESSIONS.len() as u8 {
                            for lift in KEY_LIFTS {
                                let g = GarageSetup { swing, bass, stabs, tone, progression, lift };
                                assert_eq!(GarageSetup::from_word(g.word()), g);
                            }
                        }
                    }
                }
            }
        }
    }
}
