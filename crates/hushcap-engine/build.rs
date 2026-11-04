//! Embeds build metadata (build number, git commit, build date) so every
//! binary can say exactly which build it is.

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=HUSHCAP_BUILD_NUMBER");
    println!("cargo:rerun-if-env-changed=HUSHCAP_GIT_SHA");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    println!("cargo:rerun-if-changed=../../.git/HEAD");

    let build_number = std::env::var("HUSHCAP_BUILD_NUMBER")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "0".to_string());

    let git_sha = std::env::var("HUSHCAP_GIT_SHA")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| {
            Command::new("git")
                .args(["rev-parse", "--short=10", "HEAD"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        })
        .map(|s| s.chars().take(10).collect::<String>())
        .unwrap_or_else(|| "unknown".to_string());

    let epoch = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        });

    println!("cargo:rustc-env=HUSHCAP_BUILD_NUMBER={build_number}");
    println!("cargo:rustc-env=HUSHCAP_GIT_SHA={git_sha}");
    println!("cargo:rustc-env=HUSHCAP_BUILD_DATE={}", civil_date(epoch));
}

/// Format a Unix timestamp as YYYY-MM-DD (UTC) without extra dependencies.
fn civil_date(epoch_secs: u64) -> String {
    // Howard Hinnant's days-from-civil inverse.
    let z = (epoch_secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}
