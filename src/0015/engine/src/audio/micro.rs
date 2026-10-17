//! The microsound mode's own settings. Microsound works at the smallest time
//! scale of music: clicks a few samples long, grains of a few milliseconds,
//! short sine pips, and silence, placed exactly. This mode keeps a faint
//! pulse under them (the "clicks and cuts" way): a sub-sine kick on the beat,
//! a sub pulse on the off-beats, ticks and grains on the sixteenths, digital
//! dropouts as edits, the bed scattered into a grain cloud. The settings only
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MicroSetup {
    pub pulse: Pulse,
    pub grains: Grains,
    pub pips: Pips,
    pub dropouts: Dropouts,
}

impl MicroSetup {
    pub const DEFAULT: MicroSetup =
        MicroSetup { pulse: Pulse::SubClick, grains: Grains::Medium, pips: Pips::Sparse, dropouts: Dropouts::Some };

    /// One recipe word: two bits each.
    pub fn word(self) -> u32 {
        fn idx<T: PartialEq>(all: &[T], v: T) -> u32 {
            all.iter().position(|x| *x == v).unwrap_or(0) as u32
        }
        idx(&PULSES, self.pulse) | idx(&GRAINS, self.grains) << 2 | idx(&PIPS, self.pips) << 4 | idx(&DROPOUTS, self.dropouts) << 6
    }

    pub fn from_word(w: u32) -> Self {
        fn at<T: Copy>(all: &[T], i: u32) -> T {
            all[(i & 3) as usize % all.len()]
        }
        MicroSetup { pulse: at(&PULSES, w), grains: at(&GRAINS, w >> 2), pips: at(&PIPS, w >> 4), dropouts: at(&DROPOUTS, w >> 6) }
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
                        let m = MicroSetup { pulse, grains, pips, dropouts };
                        assert_eq!(MicroSetup::from_word(m.word()), m);
                    }
                }
            }
        }
    }
}
