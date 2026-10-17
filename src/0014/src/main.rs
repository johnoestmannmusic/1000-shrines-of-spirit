//! `ShrineGen` — the host.
//!
//! The editor/setup half of the project. None of it ships inside the artifact.
//!
//! ```text
//! cargo run                                             # interactive menu + live preview
//! cargo run -- --preview [SEED] [--biome NAME] [--weather NAME] [--out FILE]
//! cargo run -- --wav [SEED] [--seconds N] [--biome NAME] [--weather NAME] [--out FILE]
//! ```

mod audition;
mod emit;
mod png;
mod preview;
mod seed;
mod serve;
mod state;
mod tui;
mod wav;

use shrine0014::plants::Biome;
use shrine0014::sim::{Sim, SimConfig};
use shrine0014::weather::WeatherKind;
use shrine0014::{DEFAULT_SEEDS, DEFAULT_SETTINGS, SAMPLE_RATE};
use std::io::IsTerminal;

fn usage() {
    println!("ShrineGen host");
    println!("  (no arguments)   interactive menu and live preview");
    println!("  --preview [SEED] [--biome NAME] [--weather NAME] [--out FILE]");
    println!("  --cover   [SEED] [--biome NAME] [--weather NAME] [--out FILE]   a 1600×1600 PNG");
    println!("  --wav     [SEED] [SECONDS] [--biome NAME] [--weather NAME] [--mode NAME] [--bpm N] [--out FILE]");
    println!("  --serve   [PORT]   serve web/ locally (default 8080)");
    println!("  --emit    [--seed N] [--biome NAME] [--weather NAME] [--out FILE]   write the self-contained artifact");
    println!("  --export  [FILE]   write the stored settings as a .shrineseed file");
    println!("  --import  FILE     load settings from a .shrineseed file");
    println!(
        "    biomes:  {}",
        Biome::ALL.iter().map(|b| b.name()).collect::<Vec<_>>().join(", ")
    );
    println!(
        "    weather: {}",
        WeatherKind::ALL.iter().map(|w| w.name()).collect::<Vec<_>>().join(", ")
    );
    println!(
        "    modes:   {}",
        shrine0014::harmony::PACES.iter().map(|p| p.name()).collect::<Vec<_>>().join(", ")
    );
}

/// Parse the shared `[SEED] [--biome] [--weather]` arguments.
struct Args {
    seed: u64,
    biome: Biome,
    weather: WeatherKind,
    seconds: f64,
    out: Option<String>,
    /// The music's mode and tempo, if given (else the defaults).
    mode: Option<shrine0014::harmony::Pace>,
    bpm: Option<u16>,
}

fn parse(args: &[String]) -> Args {
    let mut seed: u64 = 1000;
    let mut biome: Option<Biome> = None;
    let mut weather: Option<WeatherKind> = None;
    let mut out: Option<String> = None;
    let mut seconds: f64 = 20.0;
    let mut mode = None;
    let mut bpm = None;

    let mut it = args.iter().skip(1);
    let mut positional = 0;
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--biome" => {
                biome = it.next().and_then(|n| Biome::from_name(n));
            }
            "--weather" => {
                weather = it
                    .next()
                    .and_then(|name| WeatherKind::ALL.iter().copied().find(|w| w.name().eq_ignore_ascii_case(name)));
            }
            "--out" => out = it.next().cloned(),
            "--mode" => mode = it.next().and_then(|n| shrine0014::harmony::Pace::from_name(n)),
            "--bpm" => bpm = it.next().and_then(|n| n.parse::<u16>().ok()),
            "--seconds" => {
                if let Some(n) = it.next().and_then(|s| s.parse::<f64>().ok()) {
                    seconds = n.clamp(0.5, 3600.0);
                }
            }
            other => {
                // Positional: first number is the seed, second is the length in seconds.
                if let Ok(n) = other.parse::<u64>() {
                    if positional == 0 {
                        seed = n;
                    } else if positional == 1 {
                        seconds = (n as f64).clamp(0.5, 3600.0);
                    }
                    positional += 1;
                }
            }
        }
    }

    Args {
        seed,
        // Only the three wild biomes are picked by seed, so a seed keeps the
        // biome it always had; the futures are chosen by name.
        biome: biome.unwrap_or(Biome::ALL[(seed % 3) as usize]),
        // Only the first five kinds, so a seed keeps the weather it always had.
        weather: weather.unwrap_or(WeatherKind::ALL[(seed / 3 % 5) as usize]),
        seconds,
        out,
        mode,
        bpm,
    }
}

fn sim_for(a: &Args) -> Sim {
    Sim::new(SimConfig {
        seed: a.seed,
        seeds: DEFAULT_SEEDS,
        settings: shrine0014::Settings {
            pace: a.mode.unwrap_or(DEFAULT_SETTINGS.pace),
            // Garage sits around 132 BPM unless told otherwise.
            bpm: a.bpm.unwrap_or(if a.mode == Some(shrine0014::harmony::Pace::Garage) { 132 } else { DEFAULT_SETTINGS.bpm }),
            ..DEFAULT_SETTINGS
        },
        biome: a.biome,
        weather: a.weather,
    })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str);

    if args.is_empty() || command == Some("--tui") {
        if !std::io::stdout().is_terminal() {
            usage();
            return;
        }
        if let Err(e) = tui::run() {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return;
    }

    match command {
        Some("--preview") => {
            let a = parse(&args);
            let out = a.out.unwrap_or_else(|| format!("previews/shrine-{}.png", a.seed));
            match preview::write(a.seed, a.biome, a.weather, 0.0, &out) {
                Ok(()) => println!("wrote {out} (seed {}, {}, {})", a.seed, a.biome.name(), a.weather.name()),
                Err(e) => fail(&e.to_string()),
            }
        }
        Some("--cover") => {
            let a = parse(&args);
            let out = a.out.unwrap_or_else(|| format!("previews/ShrineGen-{}-cover.png", a.seed));
            let fb = preview::render(a.seed, a.biome, a.weather, 0.0);
            match preview::write_cover(&fb, &out, 1600) {
                Ok(()) => println!("wrote {out} (1600×1600, seed {}, {}, {})", a.seed, a.biome.name(), a.weather.name()),
                Err(e) => fail(&e.to_string()),
            }
        }
        Some("--wav") => {
            let a = parse(&args);
            let mut sim = sim_for(&a);
            let frames = (a.seconds * SAMPLE_RATE as f64) as usize;
            let mut buf = vec![0f32; frames * 2];
            for chunk in buf.chunks_mut(4096 * 2) {
                sim.render_audio(chunk);
            }
            wav::fade_edges(&mut buf, SAMPLE_RATE, 0.03);
            let out = a.out.unwrap_or_else(|| format!("previews/shrine-{}.wav", a.seed));
            match wav::write_stereo_16(&out, SAMPLE_RATE, &buf) {
                Ok(()) => println!(
                    "wrote {out} ({:.0}s, seed {}, {}, {})",
                    a.seconds,
                    a.seed,
                    a.biome.name(),
                    a.weather.name()
                ),
                Err(e) => fail(&e.to_string()),
            }
        }
        Some("--serve") => {
            let port = args.get(1).and_then(|s| s.parse::<u16>().ok()).unwrap_or(8080);
            if let Err(e) = serve::run(port) {
                fail(&e.to_string());
            }
        }
        Some("--emit") => {
            let mut config = state::load();
            let mut out: Option<String> = None;
            let mut it = args.iter().skip(1);
            while let Some(arg) = it.next() {
                match arg.as_str() {
                    "--seed" => {
                        if let Some(n) = it.next().and_then(|s| s.parse().ok()) {
                            config.seed = n;
                        }
                    }
                    "--biome" => {
                        if let Some(b) = it.next().and_then(|n| Biome::from_name(n)) {
                            config.biome = b;
                        }
                    }
                    "--weather" => {
                        if let Some(w) = it
                            .next()
                            .and_then(|name| WeatherKind::ALL.iter().copied().find(|w| w.name().eq_ignore_ascii_case(name)))
                        {
                            config.weather = w;
                        }
                    }
                    "--out" => out = it.next().cloned(),
                    _ => {}
                }
            }
            let out = out.unwrap_or_else(|| format!("artifacts/ShrineGen-{}.rs", config.seed));
            match emit::emit(&config, &out) {
                Ok(()) => {
                    // Keep the source beside the page so the code overlay can load it.
                    let _ = emit::publish_source(&out, "web/shrine.rs");
                    // Keep the folder self-contained: hosting web/ needs no server
                    // to discover the bundled render.
                    let _ = serve::write_wav_manifest(std::path::Path::new("web"));
                    println!("wrote {out} (seed {}, {}, {})", config.seed, config.biome.name(), config.weather.name());
                }
                Err(e) => fail(&e.to_string()),
            }
        }
        Some("--export") => {
            let config = state::load();
            let out = args.get(1).cloned().unwrap_or_else(|| format!("artifacts/ShrineGen-{}.shrineseed", config.seed));
            match seed::write(&config, &out) {
                Ok(()) => println!("wrote {out}"),
                Err(e) => fail(&e.to_string()),
            }
        }
        Some("--import") => {
            let Some(path) = args.get(1) else {
                fail("usage: --import FILE.shrineseed")
            };
            match seed::read(path) {
                Ok(config) => {
                    state::save(&config);
                    println!("imported {path} — now the stored default");
                }
                Err(e) => fail(&e.to_string()),
            }
        }
        _ => usage(),
    }
}

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(1);
}
