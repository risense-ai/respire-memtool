//! Read-only diagnostic for the rsrs CLI install/update detection.
//! Run: cargo run --example rsrs_install_probe
//!
//! Prints only install metadata (version/paths/registry), never tokens or memory content.

use memtools_lib::{config::AppConfig, rsrs_install};

fn main() {
    let config = AppConfig::load();
    let status = rsrs_install::status(&config.rsrs_cli);
    let latest = rsrs_install::check_latest(&config.rsrs_cli);
    println!(
        "{}",
        serde_json::json!({
            "status": status,
            "latest": latest.ok(),
        })
    );
}
