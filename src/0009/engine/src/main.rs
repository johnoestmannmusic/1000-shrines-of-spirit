//! `render-0009`: the dependency-free way to render 0009 to a 24-bit stereo WAV.
//! (The `shrine-0009` app in the workspace root adds live playback and a TUI.)
//!
//! Run with no arguments to be asked for the seeds, file name and length.
//! For scripting, pass any of these flags instead (no questions asked):
//!
//!     render-0009 [--seconds N] [--out FILE] [--seed1 N] [--seed2 N] [--seed3 N] [--seed4 N]
//!
//! The piece itself is endless; the length decides how much of it to capture,
//! and the end of the file fades out.

use shrine0009::cli::{self, DEFAULT_FILE, DEFAULT_SECONDS};
use shrine0009::{Seeds, DEFAULT_SEEDS};
use std::io;
use std::process::exit;

fn usage(msg: &str) -> ! {
    eprintln!("{msg}");
    eprintln!(
        "usage: render-0009 [--seconds N] [--out FILE] [--seed1 N] [--seed2 N] [--seed3 N] [--seed4 N]"
    );
    eprintln!("       render-0009            (asks for everything)");
    exit(2);
}

fn from_flags(args: Vec<String>) -> (Seeds, String, f64) {
    let mut seconds = DEFAULT_SECONDS;
    let mut out_path = String::from(DEFAULT_FILE);
    let mut seeds = DEFAULT_SEEDS;
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
            _ => usage(&format!("unknown option: {flag}")),
        }
    }
    (seeds, out_path, seconds)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (seeds, path, seconds) = if args.is_empty() {
        let stdin = io::stdin();
        let mut input = stdin.lock();
        println!("0009 - render to WAV");
        println!("Press Enter to keep the value in [brackets].\n");
        let seeds = cli::ask_seeds(&mut input);
        let (path, seconds) = cli::ask_output(&mut input);
        (seeds, path, seconds)
    } else {
        from_flags(args)
    };
    if let Err(e) = cli::render_to_wav(seeds, &path, seconds) {
        eprintln!("{path}: {e}");
        exit(1);
    }
}
