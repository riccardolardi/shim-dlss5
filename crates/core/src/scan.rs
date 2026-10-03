//! One full scan: discover, analyse, route, merge. Pure over its inputs so the
//! Tauri command only has to lock, call, emit, save.

use crate::{
    analysis,
    api::ScanProgress,
    discovery::{self, DiscoveryOutcome, LauncherAdapter},
    library::Library,
    model::{Game, GameStatus},
    platform::Platform,
    routing,
    settings::Settings,
};

pub struct ScanResult {
    pub library: Library,
    pub outcome: DiscoveryOutcome,
}

/// Run every enabled adapter, then analyse each game whose executable is new
/// or changed. `progress` is called before each game is analysed.
pub fn run(
    library: &Library,
    settings: &Settings,
    platform: &Platform,
    adapters: &[Box<dyn LauncherAdapter>],
    now: u64,
    progress: &mut dyn FnMut(ScanProgress),
) -> ScanResult {
    progress(ScanProgress {
        done: 0,
        total: 0,
        title: None,
    });
    let outcome = discovery::discover_all(adapters, &settings.scan, platform);
    let merged = library.merge_scan(&outcome, &settings.hidden_games, now);

    let total = merged.games.len();
    let games: Vec<Game> = merged
        .games
        .iter()
        .enumerate()
        .map(|(i, game)| {
            progress(ScanProgress {
                done: i,
                total,
                title: Some(game.title.clone()),
            });
            analyse_game(game)
        })
        .collect();
    progress(ScanProgress {
        done: total,
        total,
        title: None,
    });

    ScanResult {
        library: merged.with_games(games, now),
        outcome,
    }
}

/// Analyse one game (reusing its cached analysis when the exe is unchanged)
/// and derive its status. Install state is layered on in Phase 2.
pub fn analyse_game(game: &Game) -> Game {
    let discovered = crate::model::DiscoveredGame {
        launcher: game.launcher,
        title: game.title.clone(),
        install_dir: game.install_dir.clone(),
        declared_exe: None,
        launcher_id: game.launcher_id.clone(),
    };
    match analysis::analyse_cached(&discovered, game.analysis.as_ref()) {
        Ok(a) => Game {
            status: routing::decide(&a),
            analysis: Some(a),
            ..game.clone()
        },
        Err(e) => {
            tracing::warn!(title = %game.title, detail = %e.detail(), "analysis failed");
            Game {
                status: GameStatus::Unsupported {
                    reason: e.user_message(),
                },
                analysis: None,
                ..game.clone()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::pe::testing::{write_exe, X64};
    use crate::model::{DiscoveredGame, Launcher, Route};
    use crate::Result;
    use std::path::PathBuf;

    struct Fixed(Vec<DiscoveredGame>);

    impl LauncherAdapter for Fixed {
        fn launcher(&self) -> Launcher {
            Launcher::Steam
        }
        fn discover(&self, _: &Platform) -> Result<Vec<DiscoveredGame>> {
            Ok(self.0.clone())
        }
    }

    fn discovered(dir: PathBuf, title: &str) -> DiscoveredGame {
        DiscoveredGame {
            launcher: Launcher::Steam,
            title: title.into(),
            install_dir: dir,
            declared_exe: None,
            launcher_id: None,
        }
    }

    #[test]
    fn scan_discovers_analyses_routes_and_reports_progress() {
        let tmp = tempfile::tempdir().unwrap();
        let dx12 = tmp.path().join("Dx12Game");
        write_exe(&dx12.join("Dx12Game.exe"), X64, &["d3d12.dll"]);
        let empty = tmp.path().join("Empty");
        std::fs::create_dir_all(&empty).unwrap();

        let adapters: Vec<Box<dyn LauncherAdapter>> = vec![Box::new(Fixed(vec![
            discovered(dx12.clone(), "Dx12Game"),
            discovered(empty, "Empty"),
        ]))];
        let settings = Settings {
            scan: crate::settings::ScanSources {
                epic: false,
                gog: false,
                xbox: false,
                ubisoft: false,
                ea: false,
                ..Default::default()
            },
            ..Settings::default()
        };
        let mut seen = Vec::new();
        let result = run(
            &Library::default(),
            &settings,
            &Platform::unavailable(),
            &adapters,
            42,
            &mut |p| seen.push(p),
        );

        assert_eq!(result.library.scanned_at, Some(42));
        assert_eq!(result.library.games.len(), 2);
        let g = result
            .library
            .games
            .iter()
            .find(|g| g.title == "Dx12Game")
            .unwrap();
        assert!(matches!(
            &g.status,
            GameStatus::Ready {
                route: Route::ReShadeRenoDx,
                ..
            }
        ));
        assert!(g.analysis.as_ref().unwrap().exe.ends_with("Dx12Game.exe"));
        let e = result
            .library
            .games
            .iter()
            .find(|g| g.title == "Empty")
            .unwrap();
        assert!(
            matches!(&e.status, GameStatus::Unsupported { reason } if reason.contains("executable"))
        );

        // start, one per game, finish
        assert_eq!(seen.len(), 4);
        assert_eq!(seen[1].total, 2);
        assert_eq!(seen[3].done, 2);
        assert!(result.outcome.failures.is_empty());
    }
}
