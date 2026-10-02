//! Stamps the app with its build date, shown as the version `vYYYYMMDD`.
//! Honours SOURCE_DATE_EPOCH for reproducible builds; otherwise uses the
//! local date from `date`, falling back to UTC where that is unavailable.
//! (The engine has no build script: the sound never depends on any of this.)

use std::time::{SystemTime, UNIX_EPOCH};

/// Days since 1970-01-01 → (year, month, day), proleptic Gregorian.
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + (m <= 2) as i64, m, d)
}

fn utc_date(secs: i64) -> String {
    let (y, m, d) = civil(secs.div_euclid(86_400));
    format!("{y:04}{m:02}{d:02}")
}

fn main() {
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    let date = if let Some(secs) = std::env::var("SOURCE_DATE_EPOCH").ok().and_then(|s| s.parse::<i64>().ok()) {
        utc_date(secs)
    } else {
        std::process::Command::new("date")
            .arg("+%Y%m%d")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| s.len() == 8 && s.bytes().all(|b| b.is_ascii_digit()))
            .unwrap_or_else(|| {
                let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
                utc_date(now)
            })
    };
    println!("cargo:rustc-env=BUILD_DATE={date}");
    // Re-stamp whenever the app's sources change.
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=build.rs");
}
