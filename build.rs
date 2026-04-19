use std::process::Command;

fn main() {
    // Re-run if the skip flag is toggled so `cargo clean` isn't needed to take effect.
    println!("cargo:rerun-if-env-changed=SPACEBOT_SKIP_FRONTEND_BUILD");
    println!("cargo:rerun-if-env-changed=SPACEBOT_REQUIRE_FRONTEND_BUILD");
    println!("cargo:rerun-if-env-changed=SPACEBOT_DEV_FRONTEND");

    if std::env::var("SPACEBOT_SKIP_FRONTEND_BUILD").is_ok() {
        // Still need an empty dist so rust-embed has something to bake in.
        ensure_dist_dir();
        return;
    }

    // Narrow watch set. Previously this included `interface/src/` and
    // `spaceui/packages/` (recursive-looking but actually top-level mtime only).
    // `interface/src/` edits fired the build.rs cascade into rust-embed on
    // every TypeScript save. bun's Turbo cache is the right source of truth
    // for whether a rebuild is needed — we just need to invoke bun when the
    // install or build surface changes.
    println!("cargo:rerun-if-changed=interface/package.json");
    println!("cargo:rerun-if-changed=interface/bun.lock");
    println!("cargo:rerun-if-changed=interface/index.html");
    println!("cargo:rerun-if-changed=interface/vite.config.ts");
    println!("cargo:rerun-if-changed=interface/tailwind.config.ts");

    let interface_dir = std::path::Path::new("interface");

    // Skip if bun isn't installed or node_modules is missing (CI without frontend deps).
    if !interface_dir.join("node_modules").exists() {
        let msg = "interface/node_modules not found, skipping frontend build. Run `bun install` in interface/";
        if std::env::var("SPACEBOT_REQUIRE_FRONTEND_BUILD").is_ok() {
            panic!("{msg}");
        }
        eprintln!("cargo:warning={msg}");
        ensure_dist_dir();
        return;
    }

    // Dev frontend mode: faster bun build with sourcemaps off. Opt-in via env
    // because production release builds must keep sourcemaps for debugging.
    let bun_script = if std::env::var("SPACEBOT_DEV_FRONTEND").is_ok() {
        "build:dev"
    } else {
        "build"
    };

    let status = Command::new("bun")
        .args(["run", bun_script])
        .current_dir(interface_dir)
        .status();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            let msg = format!(
                "frontend build exited with {s}, the binary will serve a stale or empty UI"
            );
            if std::env::var("SPACEBOT_REQUIRE_FRONTEND_BUILD").is_ok() {
                panic!("{msg}");
            }
            eprintln!("cargo:warning={msg}");
        }
        Err(e) => {
            let msg = format!(
                "failed to run `bun run {bun_script}`: {e}. Install bun to build the frontend."
            );
            if std::env::var("SPACEBOT_REQUIRE_FRONTEND_BUILD").is_ok() {
                panic!("{msg}");
            }
            eprintln!("cargo:warning={msg}");
            ensure_dist_dir();
        }
    }
}

/// rust-embed requires the folder to exist even if empty.
fn ensure_dist_dir() {
    let dist = std::path::Path::new("interface/dist");
    if !dist.exists() {
        std::fs::create_dir_all(dist).ok();
    }
}
