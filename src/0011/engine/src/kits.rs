//! Drum kit families. The kit is chosen at setup; seed 5 then sets every
//! voice's numbers (and the break) within that family's ranges. Every kit keeps the roles of a
//! breakbeat (kick, snare, ghost, hats, open hat, ride) so it always feels
//! like drums, but how each role is synthesised differs widely.

use crate::filter::{OnePole, Svf};
use crate::harmony::Harmony;
use crate::math::{decay_coef, exp, midi_hz, sin_turns, tanh};
use crate::rng::Rng;
use crate::SAMPLE_RATE;

const SR: f64 = SAMPLE_RATE as f64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kit {
    /// Synthesised acoustic funk kit, like a sampled break.
    Acoustic,
    /// Everything made by frequency modulation: rubbery kicks, clangy snares, bell hats.
    FmMetal,
    /// Resonating modes tuned to the key and scale: drums that belong to the harmony.
    Modal,
    /// Bit-crushed, sample-starved, 1-bit cymbals.
    Crush,
    /// Filtered noise only: resonant sweeps and pings.
    NoiseSculpt,
    /// Deep short sine kicks and tiny clicks: sparse and airy.
    SubClicks,
    /// Glitch blips: tiny sine thumps, crushed FM blips, noise ticks.
    MicroBlips,
    /// Bell pings tuned to the scale, soft round kicks.
    GlassPings,
    /// Vinyl-dust crackle and single-sample clicks.
    DataDust,
    /// Square beeps and dropout buzzes, like a modem.
    PulseCode,
    /// No drums at all: pure ambient.
    Off,
}

/// Where the drums sit in the stereo field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrumSpace {
    /// Mono, dead centre (how every recipe without a space letter sounds).
    Centred,
    /// Each hit panned a little: kick centred, snares 45–65% out, hats 50–75%.
    Wide,
    /// Wide, plus a ping-pong tape echo on the snare-type hits.
    Tape,
}

pub const SPACES: [DrumSpace; 3] = [DrumSpace::Centred, DrumSpace::Wide, DrumSpace::Tape];

impl DrumSpace {
    /// One letter in recipes.
    pub fn code(self) -> &'static str {
        match self {
            DrumSpace::Centred => "C",
            DrumSpace::Wide => "W",
            DrumSpace::Tape => "T",
        }
    }

    pub fn from_code(code: &str) -> Option<DrumSpace> {
        SPACES.iter().copied().find(|s| s.code().eq_ignore_ascii_case(code))
    }

    pub fn name(self) -> &'static str {
        match self {
            DrumSpace::Centred => "Centred",
            DrumSpace::Wide => "Wide",
            DrumSpace::Tape => "Wide + tape echo",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            DrumSpace::Centred => "mono, every hit dead centre",
            DrumSpace::Wide => "kick centred, snares 45–65% out, hats 50–75%",
            DrumSpace::Tape => "wide, plus a wobbly ping-pong tape echo on the snares",
        }
    }

    pub fn index(self) -> usize {
        SPACES.iter().position(|s| *s == self).unwrap_or(0)
    }
}

/// In setup order: the light, minimal kits first.
pub const KITS: [Kit; 11] = [
    Kit::SubClicks,
    Kit::MicroBlips,
    Kit::GlassPings,
    Kit::DataDust,
    Kit::PulseCode,
    Kit::Acoustic,
    Kit::FmMetal,
    Kit::Modal,
    Kit::Crush,
    Kit::NoiseSculpt,
    Kit::Off,
];

/// The original order, frozen: old recipes without a kit code map seed 5 through it.
const LEGACY_KITS: [Kit; 6] = [Kit::Acoustic, Kit::FmMetal, Kit::Modal, Kit::Crush, Kit::NoiseSculpt, Kit::SubClicks];

impl Kit {
    /// Three-letter code used in recipes.
    pub fn code(self) -> &'static str {
        match self {
            Kit::Acoustic => "ACO",
            Kit::FmMetal => "FMM",
            Kit::Modal => "MOD",
            Kit::Crush => "CRU",
            Kit::NoiseSculpt => "NOI",
            Kit::SubClicks => "SUB",
            Kit::MicroBlips => "MIC",
            Kit::GlassPings => "GLS",
            Kit::DataDust => "DST",
            Kit::PulseCode => "PCM",
            Kit::Off => "OFF",
        }
    }

    /// The light, minimal kits (gentle processing, short sounds).
    pub fn is_light(self) -> bool {
        matches!(self, Kit::SubClicks | Kit::MicroBlips | Kit::GlassPings | Kit::DataDust | Kit::PulseCode)
    }

    pub fn from_code(code: &str) -> Option<Kit> {
        KITS.iter().copied().find(|k| k.code().eq_ignore_ascii_case(code))
    }

    pub fn index(self) -> usize {
        KITS.iter().position(|k| *k == self).unwrap_or(0)
    }

    /// A one-line character sketch, for the setup screen.
    pub fn blurb(self) -> &'static str {
        match self {
            Kit::Acoustic => "a synthesised funk kit, like a sampled break",
            Kit::FmMetal => "rubbery FM kicks, clangy snares, bell hats",
            Kit::Modal => "resonant drums tuned to the key and scale",
            Kit::Crush => "crushed to a few bits, 1-bit pulse hats",
            Kit::NoiseSculpt => "resonant noise sweeps and pings",
            Kit::SubClicks => "deep short sine kicks and tiny clicks",
            Kit::MicroBlips => "tiny thumps, crushed FM blips, noise ticks",
            Kit::GlassPings => "bell pings tuned to the scale, soft kicks",
            Kit::DataDust => "vinyl-dust crackle and single-sample clicks",
            Kit::PulseCode => "square beeps and dropout buzzes",
            Kit::Off => "no drums: pure ambient",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kit::Acoustic => "Acoustic break",
            Kit::FmMetal => "FM metal",
            Kit::Modal => "Modal (tuned)",
            Kit::Crush => "Digital crush",
            Kit::NoiseSculpt => "Noise sculpture",
            Kit::SubClicks => "Sub & clicks",
            Kit::MicroBlips => "Micro blips",
            Kit::GlassPings => "Glass pings",
            Kit::DataDust => "Data dust",
            Kit::PulseCode => "Pulse code",
            Kit::Off => "Off",
        }
    }
}

/// Every number a kit's voices are built from.
#[derive(Clone, Debug)]
pub struct KitParams {
    pub kit: Kit,
    /// The break's original tempo (it is sped up to 168 BPM).
    pub source_bpm: f64,
    /// Sampler colour.
    pub bits: u32,
    pub hold: usize,
    pub lp_hz: f64,
    // Kick: pitch from → to, drop time constant (s), decay (s), drive.
    pub kick_from: f64,
    pub kick_to: f64,
    pub kick_drop: f64,
    pub kick_decay: f64,
    pub kick_drive: f64,
    pub kick_fm: (f64, f64),
    // Snare: two tones, wire/noise centre, decay, drive.
    pub snare_tones: [f64; 2],
    pub snare_wire: f64,
    pub snare_decay: f64,
    pub snare_drive: f64,
    pub snare_fm: (f64, f64),
    // Cymbals: oscillator/partial frequencies, filter centre, decays, noise share.
    pub metal: [f64; 6],
    pub cymbal_centre: f64,
    pub hat_decay: f64,
    pub ride_decay: f64,
    pub noise_amt: f64,
    /// Modal kit: scale notes (Hz) the bars are tuned to.
    pub tuned: Vec<f64>,
    /// 8ths on the ride (else closed hats), and how many off-beat hats.
    pub ride_on_8ths: bool,
    pub hat_density: f64,
    /// How much of the punch chain (transient boost, drive, bus compression)
    /// to use: light kits keep only a touch of it.
    pub punch: f64,
}

/// The classic 808-style metallic oscillator bank (Hz).
const METAL: [f64; 6] = [205.3, 304.4, 369.6, 522.7, 540.0, 800.0];

/// How seed 5 picked the kit before kits were chosen at setup. Only used to
/// read older recipes (without a kit code) so they still sound the same.
pub fn legacy_kit_for(seed: u64) -> Kit {
    let n = LEGACY_KITS.len() as i64;
    let pick = |s: u64| (Rng::stream(s, 0x4B17).next_u64() % n as u64) as i64;
    LEGACY_KITS[(pick(seed) - pick(crate::DEFAULT_SEEDS.s5)).rem_euclid(n) as usize]
}

impl KitParams {
    pub fn new(seed: u64, kit: Kit, harmony: &Harmony) -> KitParams {
        let mut r = Rng::stream(seed, 0x4B175);
        let metal = {
            let scale = r.range(0.9, 1.15);
            METAL.map(|f| f * scale)
        };
        let root = midi_hz(36.0 + harmony.key as f64); // C2..B2
        let tuned: Vec<f64> = harmony.scale_notes(84.0, 100.0).iter().map(|m| midi_hz(*m)).collect();
        let mut p = KitParams {
            kit,
            source_bpm: r.range(132.0, 142.0),
            bits: r.int(11, 13) as u32,
            hold: 2,
            lp_hz: r.range(9000.0, 12_000.0),
            kick_from: r.range(150.0, 190.0),
            kick_to: r.range(45.0, 55.0),
            kick_drop: r.range(0.022, 0.035),
            kick_decay: r.range(0.25, 0.4),
            kick_drive: r.range(2.0, 2.6),
            kick_fm: (1.0, 0.0),
            snare_tones: [r.range(170.0, 200.0), r.range(300.0, 360.0)],
            snare_wire: r.range(1800.0, 2600.0),
            snare_decay: r.range(0.15, 0.22),
            snare_drive: r.range(1.4, 1.8),
            snare_fm: (1.0, 0.0),
            metal,
            cymbal_centre: r.range(6500.0, 9000.0),
            hat_decay: r.range(0.035, 0.06),
            ride_decay: r.range(0.35, 0.55),
            noise_amt: r.range(0.28, 0.36),
            tuned,
            ride_on_8ths: r.chance(0.7),
            hat_density: r.range(0.25, 0.45),
            punch: 1.0,
        };
        // Shared by the light kits: a clean sampler and gentle drive.
        let light = |p: &mut KitParams, r: &mut Rng| {
            p.source_bpm = r.range(125.0, 140.0);
            p.bits = 14;
            p.hold = 1;
            p.lp_hz = r.range(14_000.0, 16_000.0);
            p.kick_drive = r.range(1.0, 1.3);
            p.snare_drive = r.range(1.0, 1.2);
            p.ride_on_8ths = r.chance(0.2);
            p.hat_density = r.range(0.2, 0.45);
            p.punch = r.range(0.2, 0.3);
        };
        match kit {
            Kit::Acoustic => {}
            Kit::FmMetal => {
                p.source_bpm = r.range(128.0, 150.0);
                p.bits = r.int(12, 14) as u32;
                p.hold = r.int(1, 2) as usize;
                p.kick_from = r.range(160.0, 220.0);
                p.kick_to = r.range(42.0, 55.0);
                p.kick_fm = (if r.chance(0.5) { 1.0 } else { 0.5 }, r.range(6.0, 10.0));
                p.snare_tones = [r.range(180.0, 260.0), 0.0];
                p.snare_fm = (r.range(1.41, 1.73), r.range(8.0, 14.0));
                p.snare_decay = r.range(0.12, 0.2);
                p.cymbal_centre = r.range(3000.0, 5000.0); // FM bell carrier
                p.metal[0] = r.range(3.7, 4.3); // bell ratio
                p.metal[1] = r.range(4.0, 8.0); // bell index
                p.ride_decay = r.range(0.4, 0.7);
            }
            Kit::Modal => {
                p.source_bpm = r.range(126.0, 140.0);
                p.bits = r.int(13, 14) as u32;
                p.hold = 1;
                p.kick_to = root;
                p.kick_from = root * 1.3;
                p.kick_decay = r.range(0.35, 0.55);
                p.kick_drive = r.range(1.2, 1.6);
                let mid = harmony.scale_notes(55.0, 66.0);
                p.snare_tones = [midi_hz(r.pick(&mid)), 0.0];
                p.snare_decay = r.range(0.18, 0.3);
                p.hat_decay = r.range(0.08, 0.16);
                p.ride_decay = r.range(0.5, 0.9);
                p.ride_on_8ths = r.chance(0.5);
            }
            Kit::Crush => {
                p.source_bpm = r.range(130.0, 150.0);
                p.bits = r.int(8, 10) as u32;
                p.hold = r.int(2, 4) as usize;
                p.lp_hz = r.range(7000.0, 10_000.0);
                p.kick_from = r.range(110.0, 150.0);
                p.kick_drive = r.range(2.5, 3.5);
                p.cymbal_centre = r.range(3000.0, 7000.0); // pulse pitch
                p.hat_decay = r.range(0.015, 0.04);
                p.ride_decay = r.range(0.2, 0.4);
            }
            Kit::NoiseSculpt => {
                p.bits = r.int(12, 14) as u32;
                p.hold = 1;
                p.kick_from = r.range(300.0, 500.0);
                p.kick_to = r.range(45.0, 60.0);
                p.kick_drop = r.range(0.03, 0.06);
                p.snare_wire = r.range(2500.0, 3500.0);
                p.cymbal_centre = r.range(6000.0, 11_000.0);
                // Resonant noise washes easily: keep the ride short and often off the 8ths.
                p.ride_decay = r.range(0.15, 0.3);
                p.ride_on_8ths = r.chance(0.4);
            }
            Kit::SubClicks => {
                light(&mut p, &mut r);
                p.kick_from = r.range(70.0, 90.0);
                p.kick_to = r.range(38.0, 45.0);
                p.kick_drop = r.range(0.04, 0.07);
                // Short enough that the drone's duck lets go quickly.
                p.kick_decay = r.range(0.28, 0.45);
                p.kick_drive = r.range(1.1, 1.4);
                p.snare_decay = r.range(0.05, 0.08);
                p.ride_on_8ths = false;
                p.hat_density = r.range(0.1, 0.25);
            }
            Kit::MicroBlips => {
                light(&mut p, &mut r);
                p.kick_from = r.range(110.0, 140.0);
                p.kick_to = r.range(55.0, 70.0);
                p.kick_drop = r.range(0.008, 0.015);
                p.kick_decay = r.range(0.05, 0.08);
                p.snare_tones = [r.range(600.0, 1400.0), 0.0]; // blip pitch
                p.snare_fm = (r.int(3, 7) as f64, r.int(2, 6) as f64); // (levels, hold)
                p.snare_decay = r.range(0.03, 0.05);
                p.hat_decay = r.range(0.002, 0.006);
                p.ride_decay = r.range(0.03, 0.06);
                p.cymbal_centre = r.range(5000.0, 9000.0);
            }
            Kit::GlassPings => {
                light(&mut p, &mut r);
                p.kick_from = r.range(70.0, 95.0);
                p.kick_to = r.range(55.0, 65.0);
                p.kick_drop = r.range(0.015, 0.025);
                p.kick_decay = r.range(0.08, 0.15);
                let bells = harmony.scale_notes(72.0, 84.0);
                p.snare_tones = [midi_hz(r.pick(&bells)), 0.0];
                p.snare_decay = r.range(0.06, 0.1);
                p.hat_decay = r.range(0.02, 0.04);
                p.ride_decay = r.range(0.08, 0.15);
            }
            Kit::DataDust => {
                light(&mut p, &mut r);
                p.kick_from = r.range(90.0, 120.0);
                p.kick_to = r.range(50.0, 60.0);
                p.kick_drop = r.range(0.01, 0.02);
                p.kick_decay = r.range(0.04, 0.07);
                p.snare_wire = r.range(1500.0, 3500.0);
                p.snare_decay = r.range(0.06, 0.12);
                p.snare_fm = (r.range(0.015, 0.04), 0.0); // crackle density
                p.hat_decay = r.range(0.004, 0.01);
                p.ride_decay = r.range(0.05, 0.1);
                p.cymbal_centre = r.range(3000.0, 7000.0);
            }
            Kit::Off => {}
            Kit::PulseCode => {
                light(&mut p, &mut r);
                p.kick_from = r.range(60.0, 90.0);
                p.kick_to = p.kick_from * 0.9;
                p.kick_drop = 0.02;
                p.kick_decay = r.range(0.04, 0.07);
                let t = r.range(400.0, 900.0);
                p.snare_tones = [t, t * r.pick(&[1.25, 1.5, 2.0])];
                p.snare_decay = r.range(0.015, 0.03);
                p.hat_decay = r.range(0.0005, 0.0015);
                p.ride_decay = r.range(0.015, 0.03);
                p.cymbal_centre = r.range(5000.0, 9000.0);
            }
        }
        p
    }

    /// One line per voice, for the TUI.
    pub fn describe(&self) -> [String; 3] {
        match self.kit {
            Kit::Acoustic => [
                format!("sine dive {:.0}→{:.0} Hz + click", self.kick_from, self.kick_to),
                format!("body {:.0}/{:.0} Hz + wires ~{:.1} kHz", self.snare_tones[0], self.snare_tones[1], self.snare_wire / 1000.0),
                "6 ring-modded squares + coloured noise".to_string(),
            ],
            Kit::FmMetal => [
                format!("FM ratio {} index {:.0}→0", self.kick_fm.0, self.kick_fm.1),
                format!("FM {:.0} Hz ×{:.2} index {:.0}", self.snare_tones[0], self.snare_fm.0, self.snare_fm.1),
                format!("FM bells ×{:.2}, index {:.0}", self.metal[0], self.metal[1]),
            ],
            Kit::Modal => [
                format!("membrane modes on {:.0} Hz (the key)", self.kick_to),
                format!("modes on {:.0} Hz, noise-struck", self.snare_tones[0]),
                "free-bar modes on scale notes".to_string(),
            ],
            Kit::Crush => [
                format!("crushed dive {:.0}→{:.0} Hz, 5 bits", self.kick_from, self.kick_to),
                "3-bit noise + square body".to_string(),
                format!("1-bit pulses ~{:.1} kHz", self.cymbal_centre / 1000.0),
            ],
            Kit::NoiseSculpt => [
                format!("noise, resonant LP {:.0}→{:.0} Hz", self.kick_from, self.kick_to),
                format!("noise, band sweep from {:.1} kHz", self.snare_wire / 1000.0),
                format!("resonant noise pings ~{:.1} kHz", self.cymbal_centre / 1000.0),
            ],
            Kit::SubClicks => [
                format!("short sine {:.0}→{:.0} Hz", self.kick_from, self.kick_to),
                "a click + a breath of noise".to_string(),
                "micro clicks".to_string(),
            ],
            Kit::MicroBlips => [
                format!("{:.0} ms thump {:.0}→{:.0} Hz", self.kick_decay * 1000.0, self.kick_from, self.kick_to),
                format!("crushed FM blip {:.0} Hz, {} levels", self.snare_tones[0], self.snare_fm.0),
                "noise ticks · high blips".to_string(),
            ],
            Kit::GlassPings => [
                format!("soft bump {:.0}→{:.0} Hz", self.kick_from, self.kick_to),
                format!("bell ping {:.0} Hz (1 : 2.756)", self.snare_tones[0]),
                "pings on scale notes".to_string(),
            ],
            Kit::DataDust => [
                format!("tock {:.0}→{:.0} Hz + impulse", self.kick_from, self.kick_to),
                format!("crackle burst ~{:.1} kHz", self.snare_wire / 1000.0),
                "single-sample dust".to_string(),
            ],
            Kit::Off => ["no drums".to_string(), String::new(), String::new()],
            Kit::PulseCode => [
                format!("square beep {:.0} Hz", self.kick_from),
                format!("buzz {:.0}/{:.0} Hz", self.snare_tones[0], self.snare_tones[1]),
                format!("clicks · beeps ~{:.1} kHz", self.cymbal_centre / 1000.0),
            ],
        }
    }

    /// Frequencies plotted in the TUI's cymbal panel.
    pub fn cymbal_freqs(&self) -> Vec<f64> {
        match self.kit {
            Kit::Acoustic => self.metal.to_vec(),
            Kit::FmMetal => (1..=5).map(|k| self.cymbal_centre * (1.0 + k as f64 * self.metal[0] / 4.0) / 6.0).collect(),
            Kit::Modal | Kit::GlassPings => self.tuned.iter().take(6).map(|f| f / 8.0).collect(),
            _ => vec![self.cymbal_centre / 10.0],
        }
    }
}

// ---------------------------------------------------------------- shaping

/// Punch: boost the first `ms` milliseconds, then drive (with optional
/// asymmetry, which adds even harmonics for body).
fn shape(v: &mut [f64], boost: f64, ms: f64, drive: f64, asym: f64) {
    let n = (ms / 1000.0 * SR) as usize;
    let norm = tanh(drive * (1.0 + asym)) .max(1e-9);
    for (i, x) in v.iter_mut().enumerate() {
        if i < n {
            *x *= 1.0 + boost * (1.0 - i as f64 / n as f64);
        }
        *x = (tanh(drive * (*x + asym)) - tanh(drive * asym)) / norm;
    }
}

/// Feed-forward compressor over the whole break (3 ms attack, 80 ms release,
/// 4:1 above −12 dB of the peak), then an asymmetric soft clip, blended with
/// the dry break by `amount` (light kits keep only a touch). Stereo-linked:
/// one envelope and one gain for both channels, so the image never shifts
/// (with identical channels this is exactly the mono computation).
pub fn punch_bus(left: &mut [f64], right: &mut [f64], amount: f64) {
    let joint = |l: &[f64], r: &[f64]| l.iter().zip(r).fold(1e-9f64, |a, (x, y)| a.max(x.abs()).max(y.abs()));
    let dry_peak = joint(left, right);
    let dry_l: Vec<f64> = left.iter().map(|x| x / dry_peak).collect();
    let dry_r: Vec<f64> = right.iter().map(|x| x / dry_peak).collect();
    let thr = dry_peak * 0.25; // −12 dB
    let att = 1.0 - exp(-1.0 / (0.003 * SR));
    let rel = 1.0 - exp(-1.0 / (0.08 * SR));
    let mut env = 0.0f64;
    for (l, r) in left.iter_mut().zip(right.iter_mut()) {
        let a = l.abs().max(r.abs());
        env += (if a > env { att } else { rel }) * (a - env);
        if env > thr {
            // 4:1: gain = (thr · (env/thr)^(1/4)) / env, with x^(1/4) = √√x.
            let over = env / thr;
            let g = thr * over.sqrt().sqrt() / env;
            *l *= g;
            *r *= g;
        }
    }
    let peak = joint(left, right);
    for (x, d) in left.iter_mut().zip(dry_l).chain(right.iter_mut().zip(dry_r)) {
        let v = *x / peak * 1.4;
        let wet = tanh(v + 0.06) - tanh(0.06);
        *x = d + (wet - d) * amount;
    }
}

fn crush(v: f64, levels: f64) -> f64 {
    (v * levels).round() / levels
}

// ---------------------------------------------------------------- voices

/// A falling pitch: `to + (from − to)·e^(−t/drop)`, sampled per sample.
struct Dive {
    sweep: f64,
    k: f64,
    from: f64,
    to: f64,
}

impl Dive {
    fn new(from: f64, to: f64, drop: f64) -> Self {
        Dive { sweep: 1.0, k: exp(-1.0 / (drop * SR)), from, to }
    }

    fn next(&mut self) -> f64 {
        let f = self.to + (self.from - self.to) * self.sweep;
        self.sweep *= self.k;
        f
    }
}

pub fn kick(p: &KitParams, vel: f64, rng: &mut Rng) -> Vec<f64> {
    let len = ((p.kick_decay * 1.4).min(1.5) * SR) as usize;
    let amp_k = decay_coef(p.kick_decay, SR);
    let mut dive = Dive::new(p.kick_from, p.kick_to, p.kick_drop);
    let (mut amp, mut ph, mut mph) = (1.0, 0.0, 0.0);
    let mut lp = Svf::new(p.kick_from * 2.0, 6.0, SR);
    let mut held = 0.0;
    let mut v: Vec<f64> = (0..len)
        .map(|i| {
            let f = dive.next();
            let t = i as f64 / SR;
            let attack = (i as f64 / 48.0).min(1.0);
            let s = match p.kit {
                Kit::FmMetal => {
                    let index = p.kick_fm.1 * exp(-t / 0.03);
                    let m = sin_turns(mph) * index / std::f64::consts::TAU;
                    mph += f * p.kick_fm.0 / SR;
                    mph -= mph.floor();
                    sin_turns(ph + m)
                }
                Kit::Modal => {
                    let mut s = 0.0;
                    for (k, (ratio, dec)) in [(1.0, 1.0), (1.59, 0.6), (2.14, 0.4), (2.3, 0.3)].iter().enumerate() {
                        s += sin_turns(ph * ratio + k as f64 * 0.13) * exp(-t / (p.kick_decay * dec * 0.3)) / (k + 1) as f64;
                    }
                    s
                }
                Kit::NoiseSculpt => {
                    if i % 32 == 0 {
                        lp.set(f, 8.0, SR);
                    }
                    lp.process(rng.bipolar()).low * 3.0
                }
                Kit::Crush => {
                    if i % 6 == 0 {
                        let sq = if sin_turns(ph) > 0.0 { 0.6 } else { -0.6 };
                        held = crush(0.6 * sin_turns(ph) + 0.4 * sq, 16.0);
                    }
                    held
                }
                Kit::PulseCode => {
                    if i % 3 == 0 {
                        held = crush(if sin_turns(ph) > 0.0 { 0.7 } else { -0.7 }, 8.0);
                    }
                    held
                }
                _ => sin_turns(ph),
            };
            ph += f / SR;
            ph -= ph.floor();
            let out = s * amp * attack;
            amp *= amp_k;
            let click = match p.kit {
                Kit::SubClicks if i < 48 => (1.0 - i as f64 / 48.0) * 0.5,
                Kit::MicroBlips if i < 24 => rng.bipolar() * (1.0 - i as f64 / 24.0) * 0.25,
                Kit::DataDust if i < 3 => 0.8,
                Kit::NoiseSculpt | Kit::GlassPings | Kit::PulseCode | Kit::SubClicks | Kit::MicroBlips | Kit::DataDust => 0.0,
                _ if i < 96 => rng.bipolar() * (1.0 - i as f64 / 96.0) * 0.4,
                _ => 0.0,
            };
            (out + click) * vel
        })
        .collect();
    shape(&mut v, 0.6 * p.punch, 7.0, p.kick_drive, 0.0);
    v
}

pub fn snare(p: &KitParams, vel: f64, ghost: bool, rng: &mut Rng) -> Vec<f64> {
    let decay = if ghost { p.snare_decay * 0.4 } else { p.snare_decay };
    let len = ((decay * 1.6 + 0.05) * SR) as usize;
    let mut bp = Svf::new(p.snare_wire, 0.75, SR);
    let mut sweep = Svf::new(p.snare_wire, 3.0, SR);
    let noise_k = decay_coef(decay, SR);
    let body_k = decay_coef(decay * 0.5, SR);
    let (mut ne, mut be) = (1.0, 1.0);
    let (mut ph, mut ph2, mut mph) = (0.0, 0.25, 0.0);
    let mut held = 0.0;
    let mut v: Vec<f64> = (0..len)
        .map(|i| {
            let t = i as f64 / SR;
            let n = rng.bipolar();
            let attack = (i as f64 / 24.0).min(1.0);
            let s = match p.kit {
                Kit::FmMetal => {
                    let index = p.snare_fm.1 * exp(-t / 0.05);
                    let m = sin_turns(mph) * index / std::f64::consts::TAU;
                    mph += p.snare_tones[0] * p.snare_fm.0 / SR;
                    mph -= mph.floor();
                    let tone = sin_turns(ph + m) * be;
                    ph += p.snare_tones[0] / SR;
                    0.8 * tone + 0.35 * bp.process(n).band * ne
                }
                Kit::Modal => {
                    let mut s = 0.0;
                    for (k, ratio) in [1.0, 1.47, 1.97, 2.43].iter().enumerate() {
                        s += sin_turns(ph * ratio + k as f64 * 0.21) * exp(-t / (decay * 0.35 / (1.0 + k as f64 * 0.4)));
                    }
                    ph += p.snare_tones[0] / SR;
                    0.5 * s + 0.4 * bp.process(n).band * exp(-t / 0.02)
                }
                Kit::Crush => {
                    if i % 4 == 0 {
                        let sq = if ph < 0.5 { 0.5 } else { -0.5 };
                        held = crush(n * ne + sq * be, 3.0);
                    }
                    ph += 190.0 / SR;
                    held
                }
                Kit::NoiseSculpt => {
                    if i % 32 == 0 {
                        sweep.set(p.snare_wire * (0.3 + 0.7 * exp(-t / 0.04)), 3.0, SR);
                    }
                    sweep.process(n).band * 2.2 * ne
                }
                Kit::SubClicks => {
                    let click = if i < 72 { 1.0 - i as f64 / 72.0 } else { 0.0 };
                    click * 0.9 + bp.process(n).band * 0.8 * ne
                }
                Kit::MicroBlips => {
                    // A glitch blip: FM tone, sample-held and crushed to a few levels.
                    let (levels, hold) = (p.snare_fm.0.max(2.0), p.snare_fm.1.max(1.0) as usize);
                    if i % hold == 0 {
                        held = crush(sin_turns(ph + 0.35 * sin_turns(ph2)), levels);
                    }
                    ph += p.snare_tones[0] / SR;
                    ph2 += p.snare_tones[0] * 3.0 / SR;
                    let tick = if i < 96 { n * (1.0 - i as f64 / 96.0) * 0.35 } else { 0.0 };
                    held * ne + tick
                }
                Kit::GlassPings => {
                    let s = 0.7 * sin_turns(ph) + 0.3 * sin_turns(ph2);
                    ph += p.snare_tones[0] / SR;
                    ph2 += p.snare_tones[0] * 2.756 / SR;
                    s * ne + bp.process(n).band * 0.15 * exp(-t / 0.01)
                }
                Kit::DataDust => {
                    // Crackle: sparse random impulses, rung through a band-pass.
                    let imp = if rng.chance(p.snare_fm.0.max(0.005)) { rng.bipolar() * 3.0 } else { 0.0 };
                    bp.process(imp).band * 1.4 * ne + imp * 0.25 * ne
                }
                Kit::PulseCode => {
                    // A dropout buzz: two square tones swapping every few milliseconds.
                    let f = if (i / 240) % 2 == 0 { p.snare_tones[0] } else { p.snare_tones[1] };
                    ph += f / SR;
                    (if ph.fract() < 0.5 { 0.55 } else { -0.55 }) * ne
                }
                Kit::Acoustic | Kit::Off => {
                    let mut body = 0.0;
                    for (k, f) in p.snare_tones.iter().enumerate() {
                        let phase = if k == 0 { &mut ph } else { &mut ph2 };
                        body += sin_turns(*phase) * be;
                        *phase += f * (1.0 + 0.15 * be) / SR;
                    }
                    let wires = (bp.process(n).band * 1.6 + n * 0.3) * ne;
                    let crack = if i < 144 { n * (1.0 - i as f64 / 144.0) } else { 0.0 };
                    0.55 * body + 0.9 * wires + 0.8 * crack
                }
            };
            ph -= ph.floor();
            ph2 -= ph2.floor();
            ne *= noise_k;
            be *= body_k;
            s * attack * vel
        })
        .collect();
    shape(&mut v, 0.5 * p.punch, 6.0, p.snare_drive, 0.12 * p.punch);
    v
}

/// Hats, open hat and ride. `max_len` chokes the cymbal at the next cymbal hit.
pub fn cymbal(p: &KitParams, voice: crate::drums::Voice, vel: f64, max_len: usize, rng: &mut Rng) -> Vec<f64> {
    use crate::drums::Voice;
    let decay = match voice {
        Voice::Hat => p.hat_decay,
        Voice::OpenHat => p.hat_decay * 5.0,
        _ => p.ride_decay,
    };
    let len = ((decay * 1.3 + 0.01) * SR).min(max_len as f64).max(96.0) as usize;
    let env_k = decay_coef(decay, SR);
    let bright = 0.85 + 0.3 * vel; // harder hits are brighter
    let ride = voice == Voice::Ride;
    // Per-hit variation: detuned oscillators, random noise colours, a random tuned note.
    let detune: [f64; 6] = core::array::from_fn(|_| 1.0 + rng.range(-0.03, 0.03));
    let centre = p.cymbal_centre * bright * if ride { 0.7 } else { 1.0 };
    let mut bp = Svf::new(centre, 1.1, SR);
    let mut n1 = Svf::new(centre * rng.range(0.7, 1.0), 0.9, SR);
    let mut n2 = Svf::new(centre * rng.range(1.0, 1.35), 0.9, SR);
    let mut hp = OnePole::new(if ride { 3000.0 } else { 5000.0 }, SR);
    let mut ping = Svf::new(centre, if ride { 10.0 } else { 14.0 }, SR);
    let note = if p.tuned.is_empty() { 1000.0 } else { rng.pick(&p.tuned) * if ride { 0.5 } else { 1.0 } };
    let mut phases: [f64; 6] = core::array::from_fn(|_| rng.unit());
    let (mut env, mut ph, mut mph) = (1.0, 0.0, 0.0);
    let mut held = 0.0;
    let mut v: Vec<f64> = (0..len)
        .map(|i| {
            let t = i as f64 / SR;
            let n = rng.bipolar();
            let s = match p.kit {
                Kit::FmMetal => {
                    let index = p.metal[1] * exp(-t / (decay * 0.5));
                    let m = sin_turns(mph) * index / std::f64::consts::TAU;
                    mph += centre * p.metal[0] / SR;
                    mph -= mph.floor();
                    let s = sin_turns(ph + m);
                    ph += centre / SR;
                    s * 0.8
                }
                Kit::Modal => {
                    let mut s = 0.0;
                    for (k, ratio) in [1.0, 2.756, 5.404, 8.933].iter().enumerate() {
                        s += sin_turns(ph * ratio + k as f64 * 0.17) * exp(-t / (decay / (1.0 + k as f64))) / (k + 1) as f64;
                    }
                    ph += note / SR;
                    s * 0.9
                }
                Kit::Crush => {
                    let hold = if ride { 7 } else { 3 };
                    if i % hold == 0 {
                        held = if ride {
                            if n > 0.0 { 0.5 } else { -0.5 }
                        } else if sin_turns(ph) > 0.0 {
                            0.6
                        } else {
                            -0.6
                        };
                    }
                    ph += centre / SR;
                    held
                }
                Kit::NoiseSculpt => ping.process(n).band * 2.4,
                Kit::MicroBlips => {
                    if ride {
                        // A short high blip on a scale note.
                        let s = crush(sin_turns(ph), 5.0);
                        ph += note / SR;
                        s * 0.6
                    } else {
                        // A tick: differentiated noise, only the crackly top.
                        let tick = n - held;
                        held = n;
                        tick * 0.5
                    }
                }
                Kit::GlassPings => {
                    let s = 0.7 * sin_turns(ph) + 0.3 * sin_turns(ph * 2.756);
                    ph += note / SR;
                    s * if ride { 0.45 } else { 0.6 }
                }
                Kit::DataDust => {
                    let density = if ride { 0.05 } else { 0.15 };
                    if rng.chance(density) { rng.bipolar() } else { 0.0 }
                }
                Kit::PulseCode => {
                    if ride {
                        let s = sin_turns(ph) * 0.5;
                        ph += centre / SR;
                        s
                    } else if i < 2 {
                        if i == 0 { 0.8 } else { -0.8 }
                    } else {
                        0.0
                    }
                }
                Kit::SubClicks => {
                    let click = if i < 30 { n * (1.0 - i as f64 / 30.0) } else { 0.0 };
                    let tick = if ride { sin_turns(ph) * 0.3 } else { 0.0 };
                    ph += 5200.0 / SR;
                    click + tick
                }
                Kit::Acoustic | Kit::Off => {
                    // Squares, plus ring-modulated pairs for denser, less regular partials.
                    let mut sq = [0.0; 6];
                    for (k, (p_, f)) in phases.iter_mut().zip(p.metal).enumerate() {
                        sq[k] = if *p_ < 0.5 { 1.0 } else { -1.0 };
                        *p_ += f * detune[k] * if ride { 1.37 } else { 1.0 } / SR;
                        *p_ -= p_.floor();
                    }
                    let squares: f64 = sq.iter().sum::<f64>() / 6.0;
                    let rings = (sq[0] * sq[3] + sq[1] * sq[4] + sq[2] * sq[5]) / 3.0;
                    let metal = bp.process(0.6 * squares + 0.4 * rings).band;
                    let wash = n1.process(n).band + n2.process(n).band;
                    let mut s = metal * (1.0 - p.noise_amt) * 1.6 + wash * p.noise_amt * 0.9;
                    if ride {
                        s += 0.1 * sin_turns(ph) * exp(-t / 0.25);
                        ph += 2930.0 * detune[0] / SR;
                    }
                    s
                }
            };
            ph -= ph.floor();
            let high = s - hp.process(s);
            let out = if matches!(p.kit, Kit::Acoustic | Kit::Crush | Kit::NoiseSculpt) { high * 1.8 } else { s };
            let o = out * env * vel;
            env *= env_k;
            let attack = (i as f64 / 8.0).min(1.0);
            // Choke / end: a 2 ms fade.
            let fade = ((len - i) as f64 / 96.0).min(1.0);
            o * attack * fade
        })
        .collect();
    shape(&mut v, 0.3 * p.punch, 3.0, 1.0 + 0.2 * p.punch, 0.0);
    v
}
