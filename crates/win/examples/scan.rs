//! Smoke test against the real launchers on this PC, without the UI:
//!
//! ```powershell
//! cargo run -p shim-win --example scan
//! ```
//!
//! Prints one line per game with its status and the facts behind it. Writes
//! nothing: no library file, no settings, no game folder.

use shim_core::{discovery, library::Library, model::GameStatus, scan, settings::Settings};

fn main() {
    let platform = shim_win::platform();
    let settings = Settings::default();
    let adapters = discovery::default_adapters(&settings.scan);
    let started = std::time::Instant::now();

    let paths = shim_core::paths::AppPaths::at(std::env::temp_dir().join("shim-scan-example"));
    let components = shim_core::components::ComponentManifest::embedded();
    let ctx = scan::Context {
        settings: &settings,
        platform: &platform,
        adapters: &adapters,
        paths: &paths,
        components: &components,
    };
    let result = scan::run(&Library::default(), &ctx, 0, &mut |p| {
        if let Some(title) = &p.title {
            eprintln!("[{}/{}] {title}", p.done + 1, p.total);
        }
    });

    for f in &result.outcome.failures {
        println!("FAILED {:?}: {}", f.launcher, f.error.detail);
    }
    for l in &result.outcome.unavailable {
        println!("NO ADAPTER {l:?}");
    }
    println!();

    let mut games = result.library.games.clone();
    games.sort_by(|a, b| a.title.cmp(&b.title));
    for g in &games {
        let status = match &g.status {
            GameStatus::Ready { route, reason } => format!("READY {route:?} — {reason}"),
            GameStatus::AntiCheat { which } => format!("ANTI-CHEAT {which:?}"),
            GameStatus::Unsupported { reason } => format!("UNSUPPORTED — {reason}"),
            other => format!("{other:?}"),
        };
        println!("{:<40} [{:?}] {status}", truncate(&g.title, 40), g.launcher);
        if let Some(a) = &g.analysis {
            println!(
                "{:<40}   {} | {:?} {:?} {:?} | dlss={} {} | model={} | foreign reshade={} optiscaler={}",
                "",
                a.exe.display(),
                a.bitness,
                a.apis,
                a.engine,
                a.ships_dlss,
                a.dlss_version.as_deref().unwrap_or("-"),
                a.has_dlss5_model,
                a.foreign_reshade,
                a.foreign_optiscaler,
            );
        }
    }
    println!(
        "\n{} games in {:.1}s",
        games.len(),
        started.elapsed().as_secs_f32()
    );
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let cut: String = s.chars().take(n - 1).collect();
        format!("{cut}…")
    }
}
