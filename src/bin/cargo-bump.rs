//! Thin wrapper that dispatches `cargo bump` to `scripts/release-tag.sh`.
//!
//! Not distribution-safe: `script_path` is baked at build time from
//! `CARGO_MANIFEST_DIR`. The binary runs correctly from a clone (the only
//! path `cargo bump` is exercised from today, via `.cargo/config.toml`),
//! but `cargo install`-style distribution would resolve the script path
//! against the builder's filesystem, not the runtime filesystem, and fail
//! with a file-not-found error. Do not publish to crates.io without
//! rearchitecting to embed the script or locate it at runtime.

use std::env;
use std::process::Command;

fn main() {
    let script_path = format!("{}/scripts/release-tag.sh", env!("CARGO_MANIFEST_DIR"));
    let arguments: Vec<String> = env::args().skip(1).collect();

    let status = match Command::new(&script_path).args(&arguments).status() {
        Ok(status) => status,
        Err(error) => {
            eprintln!("Failed to execute {}: {}", script_path, error);
            std::process::exit(1);
        }
    };

    match status.code() {
        Some(code) => std::process::exit(code),
        None => std::process::exit(1),
    }
}
