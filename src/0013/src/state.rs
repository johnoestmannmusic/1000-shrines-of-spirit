//! Remembers the last setup answers between runs.
//!
//! Stored as the shrine's own recipe words (`recipe.rs`) in the user's config
//! folder, so the file is portable, inspectable and forward-compatible: a
//! missing or corrupt file simply falls back to the defaults.

use shrine0012::recipe;
use shrine0012::sim::SimConfig;
use std::path::PathBuf;

/// `$XDG_CONFIG_HOME/shrine0012/0012.cfg` (or the platform equivalent).
pub fn file() -> Option<PathBuf> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let dir = if cfg!(windows) {
        env("APPDATA")?.join("shrine0012")
    } else if cfg!(target_os = "macos") {
        env("HOME")?.join("Library/Application Support/shrine0012")
    } else if let Some(x) = env("XDG_CONFIG_HOME") {
        x.join("shrine0012")
    } else {
        env("HOME")?.join(".config/shrine0012")
    };
    Some(dir.join("0012.cfg"))
}

pub fn load() -> SimConfig {
    let Some(path) = file() else {
        return SimConfig::default();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return SimConfig::default();
    };
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("recipe=") {
            let words: Vec<u32> = rest.split(',').filter_map(|w| w.trim().parse().ok()).collect();
            if !words.is_empty() {
                return recipe::decode(&words);
            }
        }
    }
    SimConfig::default()
}

pub fn save(config: &SimConfig) {
    // Never touch the user's real config from a test run.
    if cfg!(test) {
        return;
    }
    let Some(path) = file() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let words = recipe::encode(config);
    let list = words.iter().map(|w| w.to_string()).collect::<Vec<_>>().join(",");
    let text = format!("# ShrineGen last answers, as recipe words. Safe to edit or delete.\nrecipe={list}\n");
    let _ = std::fs::write(path, text);
}
