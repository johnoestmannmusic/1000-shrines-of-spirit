//! A glitch layer allowed only one retrigger per step must still play (it
//! used to ask for a ratchet between 2 and 1 and crash).

use shrine0016::glitch_setup::GlitchSetup;
use shrine0016::{Track, DEFAULT_SEEDS, DEFAULT_SETTINGS};

#[test]
fn one_retrigger_per_step_plays() {
    let mut s = DEFAULT_SETTINGS;
    s.glitch = [GlitchSetup { ratchet: 1, fill: 100, ..GlitchSetup::DEFAULT[0] }, GlitchSetup { ratchet: 1, fill: 100, ..GlitchSetup::DEFAULT[1] }];
    for seed in 0..8u64 {
        let seeds = shrine_seeds(seed);
        let mut t = Track::new(seeds, s);
        let mut buf = vec![0f32; 4800 * 2];
        t.render(&mut buf);
        assert!(buf.iter().all(|x| x.is_finite()));
    }
}

fn shrine_seeds(k: u64) -> shrine0016::Seeds {
    shrine0016::Seeds { s2: DEFAULT_SEEDS.s2 + k, s3: DEFAULT_SEEDS.s3 + k, ..DEFAULT_SEEDS }
}
