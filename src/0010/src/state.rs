//! Remembers the last answers given at setup, so they become the next
//! session's defaults. Stored as plain `key=value` lines in the user's
//! config folder; anything missing or unreadable falls back to the
//! canonical defaults, so the file can be deleted or edited freely.

use shrine0010::cli::{self, DEFAULT_FILE, DEFAULT_SECONDS};
use shrine0010::{Seeds, Settings, DEFAULT_SEEDS, DEFAULT_SETTINGS};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Remembered {
    pub label: String,
    pub seeds: Seeds,
    pub settings: Settings,
    pub path: String,
    pub seconds: f64,
}

impl Default for Remembered {
    fn default() -> Self {
        Remembered {
            label: crate::DEFAULT_LABEL.to_string(),
            seeds: DEFAULT_SEEDS,
            settings: DEFAULT_SETTINGS,
            path: DEFAULT_FILE.to_string(),
            seconds: DEFAULT_SECONDS,
        }
    }
}

/// `$XDG_CONFIG_HOME/glitchambitoolkit/0010.txt`, `~/.config/…` on Linux,
/// `~/Library/Application Support/GlitchAmbiToolkit/0010.txt` on macOS,
/// `%APPDATA%\GlitchAmbiToolkit\0010.txt` on Windows.
pub fn file() -> Option<PathBuf> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let dir = if cfg!(windows) {
        env("APPDATA")?.join("GlitchAmbiToolkit")
    } else if cfg!(target_os = "macos") {
        env("HOME")?.join("Library/Application Support/GlitchAmbiToolkit")
    } else if let Some(x) = env("XDG_CONFIG_HOME") {
        x.join("glitchambitoolkit")
    } else {
        env("HOME")?.join(".config/glitchambitoolkit")
    };
    Some(dir.join(format!("{}.txt", cli::TRACK)))
}

pub fn load() -> Remembered {
    let mut r = Remembered::default();
    let Some(text) = file().and_then(|f| std::fs::read_to_string(f).ok()) else {
        return r;
    };
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else { continue };
        match k.trim() {
            // "code" is the old name, still read.
            "recipe" | "code" => {
                if let Some((seeds, settings)) = cli::parse_recipe(v) {
                    r.seeds = seeds;
                    r.settings = settings;
                }
            }
            "label" => {
                if let Some(l) = crate::parse_label(v) {
                    r.label = l;
                }
            }
            "file" if !v.trim().is_empty() => r.path = v.trim().to_string(),
            "length" => {
                if let Some(s) = cli::parse_length(v) {
                    r.seconds = s;
                }
            }
            _ => {}
        }
    }
    r
}

/// Best effort: failing to remember is never worth interrupting the music for.
pub fn save(r: &Remembered) {
    let Some(f) = file() else { return };
    if let Some(dir) = f.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let text = format!(
        "# GlitchAmbiToolkit {}: last answers, used as the next defaults. Safe to edit or delete.\n\
         recipe={}\nlabel={}\nfile={}\nlength={}\n",
        cli::TRACK,
        cli::recipe(r.seeds, r.settings),
        r.label,
        r.path,
        cli::format_length(r.seconds)
    );
    let _ = std::fs::write(f, text);
}
