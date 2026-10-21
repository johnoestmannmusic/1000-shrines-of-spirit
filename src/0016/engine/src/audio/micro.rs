//! The microsound mode's own settings. Microsound works at the smallest time
//! scale of music: clicks a few samples long, grains of a few milliseconds,
//! short sine pips, and silence, placed exactly. This mode keeps a faint
//! pulse under them (the "clicks and cuts" way): a sub-sine kick on the beat,
//! a sub pulse on the off-beats, ticks and grains on the sixteenths, digital
//! dropouts as edits, the bed scattered into a grain cloud, and the drone in
//! short fragments that keep time with the bar, its chords changing (and its
//! fragments transposing) on a regular count. The settings only
//! matter in microsound; the other modes ignore them, so their sound never
//! moves.

/// What carries the beat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pulse {
    /// No beat: only the ticks, grains and pips.
    Off,
    /// A sub-sine kick on each beat and a sub pulse on the off-beats.
    Sub,
    /// The same, with a click on every kick and a few on the off-beats.
    SubClick,
}

pub const PULSES: [Pulse; 3] = [Pulse::Off, Pulse::Sub, Pulse::SubClick];

impl Pulse {
    /// A one-line sketch, for the setup screen.
    pub fn blurb(self) -> &'static str {
        match self {
            Pulse::Off => "no beat: only the ticks, grains and pips",
            Pulse::Sub => "a sub-sine kick on each beat, a sub pulse between",
            Pulse::SubClick => "the same, with a click on each kick",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Pulse::Off => "off",
            Pulse::Sub => "sub",
            Pulse::SubClick => "sub + click",
        }
    }
}

/// How thick the grains are, in the drums and in the cloud the bed becomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grains {
    Sparse,
    Medium,
    Dense,
}

pub const GRAINS: [Grains; 3] = [Grains::Sparse, Grains::Medium, Grains::Dense];

impl Grains {
    /// A one-line sketch, for the setup screen.
    pub fn blurb(self) -> &'static str {
        match self {
            Grains::Sparse => "a few ticks and grains; the bed mostly itself",
            Grains::Medium => "ticks on most sixteenths; half the bed a grain cloud",
            Grains::Dense => "ticks everywhere; the bed almost all grains",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Grains::Sparse => "sparse",
            Grains::Medium => "medium",
            Grains::Dense => "dense",
        }
    }

    /// 0 (sparse) .. 1 (dense).
    pub fn amount(self) -> f64 {
        match self {
            Grains::Sparse => 0.0,
            Grains::Medium => 0.5,
            Grains::Dense => 1.0,
        }
    }
}

/// How often the high sine pips play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pips {
    Off,
    Sparse,
    Busy,
}

pub const PIPS: [Pips; 3] = [Pips::Off, Pips::Sparse, Pips::Busy];

impl Pips {
    /// A one-line sketch, for the setup screen.
    pub fn blurb(self) -> &'static str {
        match self {
            Pips::Off => "no sine pips",
            Pips::Sparse => "a few high sine pips a bar",
            Pips::Busy => "a scatter of pips, some in pairs",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Pips::Off => "off",
            Pips::Sparse => "sparse",
            Pips::Busy => "busy",
        }
    }
}

/// How often the beat fails: gaps, skips and tiny repeats, like a skipping
/// disc or a dropping buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dropouts {
    Off,
    Some,
    Many,
}

pub const DROPOUTS: [Dropouts; 3] = [Dropouts::Off, Dropouts::Some, Dropouts::Many];

impl Dropouts {
    /// A one-line sketch, for the setup screen.
    pub fn blurb(self) -> &'static str {
        match self {
            Dropouts::Off => "the pulse never fails",
            Dropouts::Some => "now and then a step drops, skips or ratchets",
            Dropouts::Many => "the pulse keeps failing, like a skipping disc",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Dropouts::Off => "off",
            Dropouts::Some => "some",
            Dropouts::Many => "many",
        }
    }

    /// The chance a step is edited.
    pub fn chance(self) -> f64 {
        match self {
            Dropouts::Off => 0.0,
            Dropouts::Some => 0.07,
            Dropouts::Many => 0.18,
        }
    }
}

/// How often the chords change (all of them: the drone, the pips, the sub
/// pulse and the loops follow).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChordRate {
    /// The slow, weighted cycle the ambient modes use.
    Slow,
    Every4,
    Every2,
    EveryBar,
}

pub const CHORD_RATES: [ChordRate; 4] = [ChordRate::Slow, ChordRate::Every4, ChordRate::Every2, ChordRate::EveryBar];

impl ChordRate {
    /// A one-line sketch, for the setup screen.
    pub fn blurb(self) -> &'static str {
        match self {
            ChordRate::Slow => "the slow, weighted cycle of the ambient modes",
            ChordRate::Every4 => "a new chord every four bars",
            ChordRate::Every2 => "a new chord every two bars",
            ChordRate::EveryBar => "a new chord every bar",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ChordRate::Slow => "slow",
            ChordRate::Every4 => "every 4 bars",
            ChordRate::Every2 => "every 2 bars",
            ChordRate::EveryBar => "every bar",
        }
    }

    /// Drum bars per chord (0 = the slow cycle).
    pub fn bars(self) -> u64 {
        match self {
            ChordRate::Slow => 0,
            ChordRate::Every4 => 4,
            ChordRate::Every2 => 2,
            ChordRate::EveryBar => 1,
        }
    }
}

/// How the drone's fragments are transposed: a repeating figure of four,
/// one transposition per fragment, so the movement keeps the groove.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DroneShift {
    Off,
    /// Up or down an octave.
    Octaves,
    /// Fifths and fourths (and the octave).
    Fifths,
    /// Steps of the scale.
    Scale,
}

pub const DRONE_SHIFTS: [DroneShift; 4] = [DroneShift::Off, DroneShift::Octaves, DroneShift::Fifths, DroneShift::Scale];

impl DroneShift {
    /// A one-line sketch, for the setup screen.
    pub fn blurb(self) -> &'static str {
        match self {
            DroneShift::Off => "every drone fragment at home pitch",
            DroneShift::Octaves => "fragments jump an octave up or down",
            DroneShift::Fifths => "fragments move by fifths, fourths and octaves",
            DroneShift::Scale => "fragments step through the scale",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            DroneShift::Off => "off",
            DroneShift::Octaves => "octaves",
            DroneShift::Fifths => "fifths",
            DroneShift::Scale => "scale steps",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MicroSetup {
    pub pulse: Pulse,
    pub grains: Grains,
    pub pips: Pips,
    pub dropouts: Dropouts,
    pub chords: ChordRate,
    pub shift: DroneShift,
}

impl MicroSetup {
    pub const DEFAULT: MicroSetup =
        MicroSetup {
            pulse: Pulse::SubClick,
            grains: Grains::Medium,
            pips: Pips::Sparse,
            dropouts: Dropouts::Some,
            chords: ChordRate::Every2,
            shift: DroneShift::Fifths,
        };

    /// One recipe word: two bits each.
    pub fn word(self) -> u32 {
        fn idx<T: PartialEq>(all: &[T], v: T) -> u32 {
            all.iter().position(|x| *x == v).unwrap_or(0) as u32
        }
        idx(&PULSES, self.pulse)
            | idx(&GRAINS, self.grains) << 2
            | idx(&PIPS, self.pips) << 4
            | idx(&DROPOUTS, self.dropouts) << 6
            | idx(&CHORD_RATES, self.chords) << 8
            | idx(&DRONE_SHIFTS, self.shift) << 10
    }

    pub fn from_word(w: u32) -> Self {
        fn at<T: Copy>(all: &[T], i: u32) -> T {
            all[(i & 3) as usize % all.len()]
        }
        MicroSetup {
            pulse: at(&PULSES, w),
            grains: at(&GRAINS, w >> 2),
            pips: at(&PIPS, w >> 4),
            dropouts: at(&DROPOUTS, w >> 6),
            chords: at(&CHORD_RATES, w >> 8),
            shift: at(&DRONE_SHIFTS, w >> 10),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_setup_round_trips_through_its_word() {
        for pulse in PULSES {
            for grains in GRAINS {
                for pips in PIPS {
                    for dropouts in DROPOUTS {
                        for chords in CHORD_RATES {
                            for shift in DRONE_SHIFTS {
                                let m = MicroSetup { pulse, grains, pips, dropouts, chords, shift };
                                assert_eq!(MicroSetup::from_word(m.word()), m);
                            }
                        }
                    }
                }
            }
        }
    }
}
