//! `shrine-0011`: play 0011 live in a TUI that visualises and explains the
//! engine, render it to WAV, or both at once.
//!
//! Run with no arguments to be asked everything. For scripting:
//!
//!     shrine-0011 [--mode play|render|both] [--label TEXT] [--recipe R] [--scale S] [--chords 1-4] [--bpm 70-180] [--pace half|jungle] [--kit K] [--space centred|wide|tape]
//!                 [--seconds N] [--out FILE] [--loop1 T] [--loop2 T] [--drone-cycle S] [--drone-hold S] [--key C|C#|…|B|seed] [--transpose N] [--seed1 N] … [--seed6 N]
//!
//! (`--mode` defaults to `play`.) The sound comes entirely from the
//! dependency-free `engine/` crate; this app only adds the sound card and the
//! terminal.

mod audio;
mod audition;
mod state;
mod tui;

use audio::Recording;
use shrine0011::cli::{self, DEFAULT_FILE, DEFAULT_SECONDS};
use shrine0011::{Seeds, Settings, DEFAULT_SEEDS, DEFAULT_SETTINGS};
use std::io::{self, IsTerminal};
use std::process::exit;

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Mode {
    Play,
    Render,
    Both,
}

fn usage(msg: &str) -> ! {
    eprintln!("{msg}");
    eprintln!(
        "usage: shrine-0011 [--mode play|render|both] [--label TEXT] [--recipe R] [--scale S] [--chords 1-4] [--bpm 70-180] [--pace half|jungle] [--kit K] [--space centred|wide|tape] [--seconds N] [--out FILE] [--loop1 T] [--loop2 T] [--drone-cycle S] [--drone-hold S] [--key C|C#|…|B|seed] [--transpose N] [--seed1 N] … [--seed6 N]"
    );
    eprintln!("       shrine-0011            (asks for everything)");
    exit(2);
}

pub(crate) fn parse_mode(s: &str) -> Option<Mode> {
    match s.trim().to_ascii_lowercase().as_str() {
        "1" | "play" | "p" => Some(Mode::Play),
        "2" | "render" | "r" => Some(Mode::Render),
        "3" | "both" | "b" => Some(Mode::Both),
        _ => None,
    }
}

/// The title shown in the TUI: 1-4 printable characters.
pub(crate) const DEFAULT_LABEL: &str = "0011";

pub(crate) fn parse_label(s: &str) -> Option<String> {
    let s = s.trim();
    let n = s.chars().count();
    ((1..=4).contains(&n) && s.chars().all(|c| !c.is_control())).then(|| s.to_string())
}

fn interactive() -> (Mode, Seeds, Settings, String, f64, String) {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    println!("0011");
    println!("Press Enter to keep the value in [brackets].\n");
    println!("What would you like to do?");
    println!("  1) Play live (endless, with a visual tour of the engine)");
    println!("  2) Render to a WAV file");
    println!("  3) Play live and record to a WAV file at the same time");
    let mode = cli::ask_until(&mut input, "Choose", "1", "Enter 1, 2 or 3.", parse_mode);
    println!();
    let label = match mode {
        Mode::Render => DEFAULT_LABEL.to_string(),
        _ => {
            let l = cli::ask_until(
                &mut input,
                "Title shown in the player (up to 4 characters)",
                DEFAULT_LABEL,
                "Enter 1 to 4 characters.",
                parse_label,
            );
            println!();
            l
        }
    };
    let settings = cli::ask_settings(&mut input);
    let seeds = cli::ask_seeds(&mut input);
    let (path, seconds) = match mode {
        Mode::Play => (String::new(), 0.0),
        _ => cli::ask_output(&mut input),
    };
    (mode, seeds, settings, path, seconds, label)
}

/// The setup menu, then playback or rendering in the same terminal, then
/// back to the menu (showing what happened) until the user chooses Exit.
fn interactive_tui() -> Result<(), String> {
    let mut terminal = ratatui::init();
    let mut last: Option<tui::setup::Notice> = None;
    let outcome = (|| -> Result<(), String> {
        loop {
            let Some(c) = tui::setup::run(&mut terminal, last.take())? else {
                return Ok(());
            };
            let (notice, exit) = match c.mode {
                Mode::Render => tui::setup::render(&mut terminal, &c)?,
                Mode::Play | Mode::Both => {
                    tui::setup::starting(&mut terminal, &c.label)?;
                    let recording = (c.mode == Mode::Both).then(|| Recording {
                        path: c.path.clone(),
                        seconds: c.seconds,
                        info: wav_info(&c.label, c.seeds, c.settings),
                    });
                    tui::play(&mut terminal, c.seeds, c.settings, recording, c.label)?
                }
            };
            last = Some(notice);
            if exit {
                return Ok(());
            }
        }
    })();
    ratatui::restore();
    // Leave the last outcome (e.g. where a file was saved) in the shell.
    if let Some(n) = last {
        if n.error {
            eprintln!("{}", n.text);
        } else {
            println!("{}", n.text);
        }
    }
    outcome
}

/// WAV metadata for this app's renders and recordings.
fn wav_info(label: &str, seeds: Seeds, settings: Settings) -> cli::WavInfo {
    cli::wav_info(label, &format!("GlitchAmbiToolkit {}", tui::setup::VERSION), seeds, settings)
}

fn from_flags(args: Vec<String>) -> (Mode, Seeds, Settings, String, f64, String) {
    let mut mode = Mode::Play;
    let mut settings = DEFAULT_SETTINGS;
    let mut label = DEFAULT_LABEL.to_string();
    let mut seconds = DEFAULT_SECONDS;
    let mut path = String::from(DEFAULT_FILE);
    let mut seeds = DEFAULT_SEEDS;
    let (mut drone_cycle, mut drone_hold) = (settings.drone.cycle_s, settings.drone.hold_s);
    let mut args = args.into_iter();
    while let Some(flag) = args.next() {
        let value = args.next().unwrap_or_else(|| usage(&format!("missing value for {flag}")));
        let seed = || cli::parse_u64(&value).unwrap_or_else(|| usage(&format!("bad seed: {value}")));
        match flag.as_str() {
            "--mode" => mode = parse_mode(&value).unwrap_or_else(|| usage(&format!("bad mode: {value}"))),
            "--seconds" => {
                seconds = cli::parse_length(&value).unwrap_or_else(|| {
                    usage(&format!("bad length: {value} (must be above 0 and at most 4 hours)"))
                })
            }
            "--out" => path = value.clone(),
            "--label" => label = parse_label(&value).unwrap_or_else(|| usage(&format!("bad label: {value} (1-4 characters)"))),
            "--seed1" => seeds.s1 = seed(),
            "--seed2" => seeds.s2 = seed(),
            "--seed3" => seeds.s3 = seed(),
            "--seed4" => seeds.s4 = seed(),
            "--seed5" => seeds.s5 = seed(),
            "--seed6" => seeds.s6 = seed(),
            "--loop1" => settings.loops[0] = cli::parse_loop_design(&value).unwrap_or_else(|| usage(&format!("bad loop design: {value}"))),
            "--loop2" => settings.loops[1] = cli::parse_loop_design(&value).unwrap_or_else(|| usage(&format!("bad loop design: {value}"))),
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
            "--kit" => settings.kit = cli::parse_kit(&value).unwrap_or_else(|| usage(&format!("bad kit: {value}"))),
            "--space" => settings.space = cli::parse_space(&value).unwrap_or_else(|| usage(&format!("bad drum space: {value}"))),
            "--bpm" => settings.bpm = cli::parse_bpm(&value).unwrap_or_else(|| usage(&format!("bad bpm: {value} (70-180)"))),
            "--pace" => settings.pace = cli::parse_pace(&value).unwrap_or_else(|| usage(&format!("bad pace: {value} (half or jungle)"))),
            _ => usage(&format!("unknown option: {flag}")),
        }
    }
    settings.drone = shrine0011::DroneArc {
        cycle_s: drone_cycle,
        hold_s: if drone_cycle == 0 { 0 } else { drone_hold.min(drone_cycle) },
    };
    (mode, seeds, settings, path, seconds, label)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Undocumented helper: `--audio-check` reports how the sound card requests audio.
    if args.first().map(String::as_str) == Some("--audio-check") {
        match audio::check() {
            Ok(report) => println!("{report}"),
            Err(e) => eprintln!("{e}"),
        }
        return;
    }
    // Undocumented helper: `--frame SECONDS [WIDTH HEIGHT]` prints the TUI as text.
    if args.first().map(String::as_str) == Some("--frame") {
        let num = |i: usize, d: f64| args.get(i).and_then(|v| v.parse().ok()).unwrap_or(d);
        let (t, w, h) = (num(1, 60.0), num(2, 140.0) as u16, num(3, 44.0) as u16);
        if let Err(e) = tui::print_frame(DEFAULT_SEEDS, DEFAULT_SETTINGS, t, w, h, args.get(4).map_or(DEFAULT_LABEL, |s| s.as_str())) {
            eprintln!("{e}");
            exit(1);
        }
        return;
    }
    // Interactive in a real terminal: the full-screen setup, then play or render there.
    if args.is_empty() && io::stdin().is_terminal() && io::stdout().is_terminal() {
        if let Err(e) = interactive_tui() {
            eprintln!("{e}");
            exit(1);
        }
        return;
    }
    let (mode, seeds, settings, path, seconds, label) = if args.is_empty() { interactive() } else { from_flags(args) };
    let info = wav_info(&label, seeds, settings);
    let result = match mode {
        Mode::Render => cli::render_to_wav(seeds, settings, &path, seconds, &info).map_err(|e| format!("{path}: {e}")),
        Mode::Play => tui::run(seeds, settings, None, label),
        Mode::Both => tui::run(seeds, settings, Some(Recording { path, seconds, info }), label),
    };
    if let Err(e) = result {
        eprintln!("{e}");
        exit(1);
    }
}
