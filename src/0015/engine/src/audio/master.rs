//! End-of-chain master effects: a slight digital-glitch character over the whole
//! mix. Each effect is a toggle, and any combination may run.
//!
//! These sit *after* the mix and the monitor, so soloing still hears a layer
//! clean. With every effect off, the chain is an exact pass-through — which is
//! what keeps the golden hashes unchanged.
//!
//! All of it is per-sample, deterministic and bounded: same seed, same glitches.

use crate::filter::{DcBlock, OnePole, Svf};
use crate::math::{exp2, sanitize, sin_turns, tanh};
use crate::rng::Rng;
use crate::SAMPLE_RATE;

const SR: f64 = SAMPLE_RATE as f64;
/// Delay-line length, seconds.
const DELAY_SECONDS: f64 = 0.02;

/// Which master effects are enabled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MasterFx {
    /// The master switch; the individual effects are remembered regardless.
    pub enabled: bool,
    /// Wow and flutter, saturation, hiss.
    pub tape: bool,
    /// Sample-rate/bit reduction with occasional skips.
    pub cd: bool,
    /// Slow pitch drift with occasional drops.
    pub pitch: bool,
    /// Band-limited, crackling radio.
    pub radio: bool,
    /// Coarse bit/sample crush.
    pub crush: bool,
    /// Rare, short signal dropouts.
    pub dropout: bool,
}

impl MasterFx {
    pub const OFF: MasterFx = MasterFx {
        enabled: false,
        tape: false,
        cd: false,
        pitch: false,
        radio: false,
        crush: false,
        dropout: false,
    };

    /// Are any individual effects selected?
    pub fn any(self) -> bool {
        self.tape || self.cd || self.pitch || self.radio || self.crush || self.dropout
    }

    /// Does the chain actually run?
    pub fn active(self) -> bool {
        self.enabled && self.any()
    }

    /// A stable name, for the menu and the recipe card.
    pub fn labels(self) -> [(&'static str, bool); 6] {
        [
            ("Tape", self.tape),
            ("CD artifacts", self.cd),
            ("Pitch drift", self.pitch),
            ("Radio", self.radio),
            ("Crush", self.crush),
            ("Dropouts", self.dropout),
        ]
    }

    pub fn bits(self) -> u32 {
        (self.enabled as u32)
            | (self.tape as u32) << 1
            | (self.cd as u32) << 2
            | (self.pitch as u32) << 3
            | (self.radio as u32) << 4
            | (self.crush as u32) << 5
            | (self.dropout as u32) << 6
    }

    pub fn from_bits(bits: u32) -> Self {
        MasterFx {
            enabled: bits & 1 != 0,
            tape: bits & (1 << 1) != 0,
            cd: bits & (1 << 2) != 0,
            pitch: bits & (1 << 3) != 0,
            radio: bits & (1 << 4) != 0,
            crush: bits & (1 << 5) != 0,
            dropout: bits & (1 << 6) != 0,
        }
    }
}

/// The running chain: one instance of each effect's state.
pub struct MasterChain {
    fx: MasterFx,
    // Tape.
    tape_l: Vec<f64>,
    tape_r: Vec<f64>,
    tape_write: usize,
    wow: f64,
    flutter: f64,
    tape_lp: [OnePole; 2],
    hiss: Rng,
    // Pitch drift.
    pitch_l: Vec<f64>,
    pitch_r: Vec<f64>,
    pitch_write: usize,
    pitch_offset: f64,
    pitch_vel: f64,
    pitch_rng: Rng,
    // CD artifacts.
    cd_hold: [f64; 2],
    cd_count: u32,
    cd_rng: Rng,
    // Radio.
    radio_bp: [Svf; 2],
    radio_lp: [OnePole; 2],
    radio_am: f64,
    radio_rng: Rng,
    // Crush.
    crush_hold: [f64; 2],
    crush_count: u32,
    // Dropouts.
    drop_count: u32,
    drop_gain: f64,
    drop_rng: Rng,
    dc: [DcBlock; 2],
}

impl MasterChain {
    pub fn new(fx: MasterFx, seed: u64) -> Self {
        let n = (DELAY_SECONDS * SR) as usize;
        MasterChain {
            fx,
            tape_l: vec![0.0; n],
            tape_r: vec![0.0; n],
            tape_write: 0,
            wow: 0.0,
            flutter: 0.0,
            tape_lp: [OnePole::new(9000.0, SR), OnePole::new(9000.0, SR)],
            hiss: Rng::stream(seed, 0x5441_5045), // "TAPE"
            pitch_l: vec![0.0; n],
            pitch_r: vec![0.0; n],
            pitch_write: 0,
            pitch_offset: 0.0,
            pitch_vel: 0.0,
            pitch_rng: Rng::stream(seed, 0x5049_5443), // "PITC"
            cd_hold: [0.0; 2],
            cd_count: 0,
            cd_rng: Rng::stream(seed, 0x4344_0000), // "CD"
            radio_bp: [Svf::new(1200.0, 0.7, SR), Svf::new(1200.0, 0.7, SR)],
            radio_lp: [OnePole::new(3200.0, SR), OnePole::new(3200.0, SR)],
            radio_am: 0.0,
            radio_rng: Rng::stream(seed, 0x5241_4449), // "RADI"
            crush_hold: [0.0; 2],
            crush_count: 0,
            drop_count: 0,
            drop_gain: 1.0,
            drop_rng: Rng::stream(seed, 0x4452_4f50), // "DROP"
            dc: [DcBlock::default(), DcBlock::default()],
        }
    }

    pub fn set_fx(&mut self, fx: MasterFx) {
        self.fx = fx;
    }

    /// One stereo sample through the chain. A pass-through when nothing is on.
    pub fn process(&mut self, l: f64, r: f64) -> (f64, f64) {
        if !self.fx.active() {
            return (l, r);
        }
        let (mut l, mut r) = (l, r);
        if self.fx.tape {
            (l, r) = self.tape(l, r);
        }
        if self.fx.pitch {
            (l, r) = self.pitch(l, r);
        }
        if self.fx.cd {
            (l, r) = self.cd(l, r);
        }
        if self.fx.radio {
            (l, r) = self.radio(l, r);
        }
        if self.fx.crush {
            (l, r) = self.crush(l, r);
        }
        if self.fx.dropout {
            (l, r) = self.dropout(l, r);
        }
        (sanitize(self.dc[0].process(l)), sanitize(self.dc[1].process(r)))
    }

    fn tape(&mut self, l: f64, r: f64) -> (f64, f64) {
        self.wow = (self.wow + 0.6 / SR).rem_euclid(1.0);
        self.flutter = (self.flutter + 6.5 / SR).rem_euclid(1.0);
        let modulate = 0.00035 * sin_turns(self.wow) + 0.00004 * sin_turns(self.flutter);
        let (wl, wr) = read_delay(&self.tape_l, &self.tape_r, self.tape_write, (0.006 + modulate) * SR);
        self.tape_l[self.tape_write] = l;
        self.tape_r[self.tape_write] = r;
        self.tape_write = (self.tape_write + 1) % self.tape_l.len();

        let drive = 1.4;
        let sat_l = self.tape_lp[0].process(tanh(wl * drive) / drive);
        let sat_r = self.tape_lp[1].process(tanh(wr * drive) / drive);
        let hiss = self.hiss.bipolar() * 0.0012;
        (l * 0.7 + (sat_l + hiss) * 0.3, r * 0.7 + (sat_r + hiss) * 0.3)
    }

    fn pitch(&mut self, l: f64, r: f64) -> (f64, f64) {
        // A drifting read pointer: its rate is the pitch shift.
        self.pitch_vel = (self.pitch_vel + self.pitch_rng.bipolar() * 0.0006) * 0.999;
        self.pitch_offset += self.pitch_vel;
        if self.pitch_offset.abs() > 160.0 {
            // A jump, heard as a momentary drop/spin-up.
            self.pitch_offset = 0.0;
            self.pitch_vel = 0.0;
        }
        let (wl, wr) = read_delay(&self.pitch_l, &self.pitch_r, self.pitch_write, 60.0 + self.pitch_offset);
        self.pitch_l[self.pitch_write] = l;
        self.pitch_r[self.pitch_write] = r;
        self.pitch_write = (self.pitch_write + 1) % self.pitch_l.len();

        // Mostly wet: a pitch shift has to replace the dry signal.
        (l * 0.35 + wl * 0.65, r * 0.35 + wr * 0.65)
    }

    fn cd(&mut self, l: f64, r: f64) -> (f64, f64) {
        if self.cd_count == 0 {
            self.cd_hold = [l, r];
            let base = 1.0 + self.cd_rng.range(0.0, 6.0);
            self.cd_count = base as u32;
            if self.cd_rng.chance(0.02) {
                self.cd_count += self.cd_rng.range(20.0, 160.0) as u32;
            }
        } else {
            self.cd_count -= 1;
        }
        let q = exp2(12.0);
        ((self.cd_hold[0] * q).round() / q, (self.cd_hold[1] * q).round() / q)
    }

    fn radio(&mut self, l: f64, r: f64) -> (f64, f64) {
        let (bl, br) = (self.radio_bp[0].process(l).band, self.radio_bp[1].process(r).band);
        let (ll, lr) = (self.radio_lp[0].process(bl), self.radio_lp[1].process(br));
        self.radio_am = (self.radio_am + 31.0 / SR).rem_euclid(1.0);
        let am = 1.0 - 0.18 * (0.5 + 0.5 * sin_turns(self.radio_am));
        let crackle = if self.radio_rng.chance(0.0012) {
            self.radio_rng.bipolar() * 0.35
        } else {
            0.0
        };
        let wet_l = tanh(ll * am * 1.6) + crackle;
        let wet_r = tanh(lr * am * 1.6) + crackle * 0.7;
        (l * 0.45 + wet_l * 0.55, r * 0.45 + wet_r * 0.55)
    }

    fn crush(&mut self, l: f64, r: f64) -> (f64, f64) {
        if self.crush_count == 0 {
            self.crush_hold = [l, r];
            self.crush_count = 4;
        } else {
            self.crush_count -= 1;
        }
        let q = exp2(9.0);
        ((self.crush_hold[0] * q).round() / q, (self.crush_hold[1] * q).round() / q)
    }

    fn dropout(&mut self, l: f64, r: f64) -> (f64, f64) {
        if self.drop_count == 0 {
            if self.drop_rng.chance(0.00006) {
                self.drop_count = self.drop_rng.range(60.0, 500.0) as u32;
                self.drop_gain = self.drop_rng.range(0.0, 0.25);
            }
        } else {
            self.drop_count -= 1;
        }
        let gain = if self.drop_count > 0 { self.drop_gain } else { 1.0 };
        (l * gain, r * gain)
    }
}

fn read_delay(left: &[f64], right: &[f64], write: usize, delay: f64) -> (f64, f64) {
    let n = left.len();
    if n == 0 {
        return (0.0, 0.0);
    }
    let read = (write as f64 - delay).rem_euclid(n as f64);
    let i0 = read.floor() as usize % n;
    let i1 = (i0 + 1) % n;
    let f = read - read.floor();
    (left[i0] + (left[i1] - left[i0]) * f, right[i0] + (right[i1] - right[i0]) * f)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain(fx: MasterFx) -> MasterChain {
        MasterChain::new(fx, 1000)
    }

    #[test]
    fn off_is_an_exact_pass_through() {
        let mut c = chain(MasterFx::OFF);
        for i in 0..1000 {
            let x = (i as f64 * 0.01).sin();
            assert_eq!(c.process(x, -x), (x, -x));
        }
    }

    #[test]
    fn enabled_without_effects_is_still_a_pass_through() {
        let mut c = chain(MasterFx { enabled: true, ..MasterFx::OFF });
        assert_eq!(c.process(0.5, -0.5), (0.5, -0.5));
    }

    #[test]
    fn each_effect_changes_the_signal_and_stays_bounded() {
        let effects = [
            MasterFx { enabled: true, tape: true, ..MasterFx::OFF },
            MasterFx { enabled: true, cd: true, ..MasterFx::OFF },
            MasterFx { enabled: true, pitch: true, ..MasterFx::OFF },
            MasterFx { enabled: true, radio: true, ..MasterFx::OFF },
            MasterFx { enabled: true, crush: true, ..MasterFx::OFF },
            MasterFx { enabled: true, dropout: true, ..MasterFx::OFF },
        ];
        for fx in effects {
            let mut c = chain(fx);
            let mut changed = false;
            for i in 0..20_000 {
                let x = (i as f64 * 0.02).sin() * 0.6;
                let (l, r) = c.process(x, x);
                assert!(l.is_finite() && r.is_finite(), "{fx:?} produced non-finite");
                assert!(l.abs() < 4.0 && r.abs() < 4.0, "{fx:?} ran away");
                if (l - x).abs() > 1e-9 {
                    changed = true;
                }
            }
            assert!(changed, "{fx:?} did nothing");
        }
    }

    #[test]
    fn bits_round_trip() {
        let fx = MasterFx { enabled: true, tape: true, cd: true, pitch: false, radio: true, crush: false, dropout: true };
        assert_eq!(MasterFx::from_bits(fx.bits()), fx);
    }
}
