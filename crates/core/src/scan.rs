//! One full scan: discover, analyse, route, merge, then layer install state
//! from the per-game manifests. Pure over its inputs so the Tauri command only
//! has to lock, call, emit, save.

use crate::{
    analysis,
    api::ScanProgress,
    components::ComponentManifest,
    discovery::{self, DiscoveryOutcome, LauncherAdapter},
    install::InstallManifest,
    library::Library,
    model::{Game, GameStatus},
    paths::AppPaths,
    platform::Platform,
    routing,
    settings::Settings,
};

pub struct ScanResult {
    pub library: Library,
    pub outcome: DiscoveryOutcome,
}

pub struct Context<'a> {
    pub settings: &'a Settings,
    pub platform: &'a Platform,
    pub adapters: &'a [Box<dyn LauncherAdapter>],
    pub paths: &'a AppPaths,
    pub components: &'a ComponentManifest,
}

/// Run every enabled adapter, then analyse each game whose executable is new
/// or changed. `progress` is called before each game is analysed.
pub fn run(
    library: &Library,
    ctx: &Context<'_>,
    now: u64,
    progress: &mut dyn FnMut(ScanProgress),
) -> ScanResult {
    progress(ScanProgress {
        done: 0,
        total: 0,
        title: None,
    });
    let outcome = discovery::discover_all(ctx.adapters, &ctx.settings.scan, ctx.platform);
    let merged = library.merge_scan(&outcome, &ctx.settings.hidden_games, now);

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
            analyse_game(game, ctx.paths, ctx.components)
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
/// and derive its status, with install state from the manifest on top.
pub fn analyse_game(game: &Game, paths: &AppPaths, components: &ComponentManifest) -> Game {
    let discovered = crate::model::DiscoveredGame {
        launcher: game.launcher,
        title: game.title.clone(),
        install_dir: game.install_dir.clone(),
        declared_exe: None,
        launcher_id: game.launcher_id.clone(),
    };
    let analysed = match analysis::analyse_cached(&discovered, game.analysis.as_ref()) {
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
    };
    with_install_state(analysed, paths, components)
}

/// A game with our manifest is Installed (or UpdateAvailable), whatever the
/// folder analysis says: our own proxy DLL must not read as "foreign".
pub fn with_install_state(game: Game, paths: &AppPaths, components: &ComponentManifest) -> Game {
    match InstallManifest::load(paths, &game.id) {
        Ok(Some(m)) => {
            let status = if m.is_outdated(components) {
                GameStatus::UpdateAvailable { route: m.route }
            } else {
                GameStatus::Installed { route: m.route }
            };
            Game { status, ..game }
        }
        Ok(None) => game,
        Err(e) => {
            tracing::warn!(title = %game.title, detail = %e.detail(), "install record unreadable");
            game
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::pe::testing::{write_exe, X64};
    use crate::install::manifest::{ComponentPin, INSTALL_SCHEMA};
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

    fn steam_only() -> Settings {
        Settings {
            scan: crate::settings::ScanSources {
                epic: false,
                gog: false,
                xbox: false,
                ubisoft: false,
                ea: false,
                ..Default::default()
            },
            ..Settings::default()
        }
    }

    #[test]
    fn scan_discovers_analyses_routes_and_reports_progress() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(tmp.path().join("data"));
        let dx12 = tmp.path().join("Dx12Game");
        write_exe(&dx12.join("Dx12Game.exe"), X64, &["d3d12.dll"]);
        let empty = tmp.path().join("Empty");
        std::fs::create_dir_all(&empty).unwrap();

        let adapters: Vec<Box<dyn LauncherAdapter>> = vec![Box::new(Fixed(vec![
            discovered(dx12.clone(), "Dx12Game"),
            discovered(empty, "Empty"),
        ]))];
        let settings = steam_only();
        let components = ComponentManifest::embedded();
        let ctx = Context {
            settings: &settings,
            platform: &Platform::unavailable(),
            adapters: &adapters,
            paths: &paths,
            components: &components,
        };
        let mut seen = Vec::new();
        let result = run(&Library::default(), &ctx, 42, &mut |p| seen.push(p));

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
        let e = result
            .library
            .games
            .iter()
            .find(|g| g.title == "Empty")
            .unwrap();
        assert!(
            matches!(&e.status, GameStatus::Unsupported { reason } if reason.contains("executable"))
        );
        assert_eq!(seen.len(), 4);
        assert_eq!(seen[1].total, 2);
    }

    #[test]
    fn install_manifest_overrides_folder_analysis() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(tmp.path().join("data"));
        let dir = tmp.path().join("G");
        write_exe(&dir.join("G.exe"), X64, &["d3d12.dll"]);
        // A ReShade-looking dxgi.dll beside the exe would normally read as foreign.
        write_exe(&dir.join("dxgi.dll"), X64, &["ReShade by crosire"]);
        let components = ComponentManifest::embedded();
        let game = Game {
            id: "g1".into(),
            launcher: Launcher::Steam,
            title: "G".into(),
            install_dir: dir.clone(),
            launcher_id: None,
            analysis: None,
            status: GameStatus::Pending,
            cover: None,
            hidden: false,
        };
        let plain = analyse_game(&game, &paths, &components);
        assert!(matches!(plain.status, GameStatus::Unsupported { .. }));

        let reshade = components.get("reshade").unwrap();
        let mut manifest = InstallManifest {
            schema: INSTALL_SCHEMA,
            game_id: "g1".into(),
            exe: dir.join("G.exe"),
            route: Route::ReShadeRenoDx,
            installed_at: 1,
            components: vec![ComponentPin {
                id: "reshade".into(),
                version: reshade.version.clone(),
                sha256: reshade.sha256.clone(),
            }],
            model_sha256: None,
            user_files: vec![],
            anti_cheat_override: false,
            files: vec![],
        };
        manifest.save(&paths).unwrap();
        let installed = analyse_game(&game, &paths, &components);
        assert_eq!(
            installed.status,
            GameStatus::Installed {
                route: Route::ReShadeRenoDx
            }
        );

        manifest.components[0].version = "0.0.0".into();
        manifest.save(&paths).unwrap();
        let outdated = analyse_game(&game, &paths, &components);
        assert_eq!(
            outdated.status,
            GameStatus::UpdateAvailable {
                route: Route::ReShadeRenoDx
            }
        );
    }
}
