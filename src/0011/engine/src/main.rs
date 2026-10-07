//! `render-0011`: the dependency-free way to render 0011 to a 24-bit stereo WAV.
//! (The `shrine-0011` app in the workspace root adds live playback and a TUI.)
//!
//! Run with no arguments to be asked for the seeds, file name and length.
//! For scripting, pass any of these flags instead (no questions asked):
//!
//!     render-0011 [--recipe R] [--seconds N] [--out FILE] [--scale S] [--chords 1-4] [--bpm 70-180] [--pace half|jungle] [--kit K] [--space centred|wide|tape] [--loop1 T] [--loop2 T] [--loop3 T] [--loop4 T] [--drone-cycle S] [--drone-hold S] [--drone-tone HZ] [--loopN-cycle S] [--loopN-hold S] [--key C|C#|…|B|seed] [--transpose N] [--seed1 N] … [--seed6 N]
//!
//! The piece itself is endless; the length decides how much of it to capture,
//! and the end of the file fades out.

use shrine0011::cli::{self, DEFAULT_FILE, DEFAULT_SECONDS};
use shrine0011::{Seeds, Settings, DEFAULT_SEEDS, DEFAULT_SETTINGS};
use std::io;
use std::process::exit;

fn usage(msg: &str) -> ! {
    eprintln!("{msg}");
    eprintln!(
        "usage: render-0011 [--recipe R] [--seconds N] [--out FILE] [--scale S] [--chords 1-4] [--bpm 70-180] [--pace half|jungle] [--kit K] [--space centred|wide|tape] [--loop1 T] [--loop2 T] [--loop3 T] [--loop4 T] [--drone-cycle S] [--drone-hold S] [--drone-tone HZ] [--loopN-cycle S] [--loopN-hold S] [--key C|C#|…|B|seed] [--transpose N] [--seed1 N] … [--seed6 N]"
    );
    eprintln!("       render-0011            (asks for everything)");
    exit(2);
}

fn from_flags(args: Vec<String>) -> (Seeds, Settings, String, f64) {
    let mut seconds = DEFAULT_SECONDS;
    let mut out_path = String::from(DEFAULT_FILE);
    let mut seeds = DEFAULT_SEEDS;
    let mut settings = DEFAULT_SETTINGS;
    let (mut drone_cycle, mut drone_hold) = (settings.drone.cycle_s, settings.drone.hold_s);
    let mut args = args.into_iter();
    while let Some(flag) = args.next() {
        let value = args.next().unwrap_or_else(|| usage(&format!("missing value for {flag}")));
        let seed = || cli::parse_u64(&value).unwrap_or_else(|| usage(&format!("bad seed: {value}")));
        match flag.as_str() {
            "--seconds" => {
                seconds = cli::parse_length(&value).unwrap_or_else(|| {
                    usage(&format!("bad length: {value} (must be above 0 and at most 4 hours)"))
                })
            }
            "--out" => out_path = value.clone(),
            "--seed1" => seeds.s1 = seed(),
            "--seed2" => seeds.s2 = seed(),
            "--seed3" => seeds.s3 = seed(),
            "--seed4" => seeds.s4 = seed(),
            "--seed5" => seeds.s5 = seed(),
            "--seed6" => seeds.s6 = seed(),
            "--loop1" => settings.loops[0] = cli::parse_loop_design(&value).unwrap_or_else(|| usage(&format!("bad loop design: {value}"))),
            "--loop2" => settings.loops[1] = cli::parse_loop_design(&value).unwrap_or_else(|| usage(&format!("bad loop design: {value}"))),
            "--loop3" => settings.loops[2] = cli::parse_loop_design(&value).unwrap_or_else(|| usage(&format!("bad loop design: {value}"))),
            "--loop4" => settings.loops[3] = cli::parse_loop_design(&value).unwrap_or_else(|| usage(&format!("bad loop design: {value}"))),
            "--drone-tone" => settings.drone_tone = cli::parse_drone_tone(&value).unwrap_or_else(|| usage(&format!("bad drone tone: {value} (0 or 100-16000 Hz)"))),
            "--loop1-cycle" => settings.loop_arcs[0].cycle_s = cli::parse_drone_cycle(&value).unwrap_or_else(|| usage(&format!("bad loop cycle: {value}"))),
            "--loop1-hold" => settings.loop_arcs[0].hold_s = value.trim().parse::<u16>().unwrap_or_else(|_| usage(&format!("bad loop hold: {value}"))),
            "--loop2-cycle" => settings.loop_arcs[1].cycle_s = cli::parse_drone_cycle(&value).unwrap_or_else(|| usage(&format!("bad loop cycle: {value}"))),
            "--loop2-hold" => settings.loop_arcs[1].hold_s = value.trim().parse::<u16>().unwrap_or_else(|_| usage(&format!("bad loop hold: {value}"))),
            "--loop3-cycle" => settings.loop_arcs[2].cycle_s = cli::parse_drone_cycle(&value).unwrap_or_else(|| usage(&format!("bad loop cycle: {value}"))),
            "--loop3-hold" => settings.loop_arcs[2].hold_s = value.trim().parse::<u16>().unwrap_or_else(|_| usage(&format!("bad loop hold: {value}"))),
            "--loop4-cycle" => settings.loop_arcs[3].cycle_s = cli::parse_drone_cycle(&value).unwrap_or_else(|| usage(&format!("bad loop cycle: {value}"))),
            "--loop4-hold" => settings.loop_arcs[3].hold_s = value.trim().parse::<u16>().unwrap_or_else(|_| usage(&format!("bad loop hold: {value}"))),
            "--drone-cycle" => drone_cycle = cli::parse_drone_cycle(&value).unwrap_or_else(|| usage(&format!("bad drone cycle: {value} (0-3600 seconds)"))),
            "--drone-hold" => drone_hold = value.trim().parse::<u16>().unwrap_or_else(|_| usage(&format!("bad drone hold: {value} (seconds)"))),
            "--key" => settings.key = cli::parse_key_choice(&value).unwrap_or_else(|| usage(&format!("bad key: {value} (seed, C..B)"))),
            "--transpose" => settings.loop_transpose = cli::parse_transpose(&value).unwrap_or_else(|| usage(&format!("bad transpose: {value} (-12 to 12)"))),
            "--recipe" | "--code" => {
                (seeds, settings) =
                    cli::parse_recipe(&value).unwrap_or_else(|| usage(&format!("bad recipe: {value}")));
                (drone_cycle, drone_hold) = (settings.drone.cycle_s, settings.drone.hold_s);
            }
            "--scale" => settings.scale = cli::parse_scale(&value).unwrap_or_else(|| usage(&format!("bad scale: {value}"))),
            "--chords" => settings.chords = cli::parse_chords(&value).unwrap_or_else(|| usage(&format!("bad chords: {value} (1-4)"))),
            "--space" => settings.space = cli::parse_space(&value).unwrap_or_else(|| usage(&format!("bad space: {value} (centred, wide or tape)"))),
            "--kit" => settings.kit = cli::parse_kit(&value).unwrap_or_else(|| usage(&format!("bad kit: {value}"))),
            "--bpm" => settings.bpm = cli::parse_bpm(&value).unwrap_or_else(|| usage(&format!("bad bpm: {value} (70-180)"))),
            "--pace" => settings.pace = cli::parse_pace(&value).unwrap_or_else(|| usage(&format!("bad pace: {value} (half or jungle)"))),
            _ => usage(&format!("unknown option: {flag}")),
        }
    }
    settings.drone = shrine0011::DroneArc {
        cycle_s: drone_cycle,
        hold_s: if drone_cycle == 0 { 0 } else { drone_hold.min(drone_cycle) },
    };
    for a in settings.loop_arcs.iter_mut() {
        a.hold_s = a.hold_s.min(a.cycle_s);
    }
    (seeds, settings, out_path, seconds)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (seeds, settings, path, seconds) = if args.is_empty() {
        let stdin = io::stdin();
        let mut input = stdin.lock();
        println!("0011 - render to WAV");
        println!("Press Enter to keep the value in [brackets].\n");
        let settings = cli::ask_settings(&mut input);
        let seeds = cli::ask_seeds(&mut input);
        let (path, seconds) = cli::ask_output(&mut input);
        (seeds, settings, path, seconds)
    } else {
        from_flags(args)
    };
    let info = cli::wav_info(cli::TRACK, "render-0011 (GlitchAmbiToolkit engine)", seeds, settings);
    if let Err(e) = cli::render_to_wav(seeds, settings, &path, seconds, &info) {
        eprintln!("{path}: {e}");
        exit(1);
    }
}
