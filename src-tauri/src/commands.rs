//! Commands the front end can invoke. Each returns a serialisable value or an
//! `ErrorDto`. Anything that touches disk, scans or downloads runs off the
//! main thread and reports progress through events.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use shim_core::{
    api::{
        AppInfo, ComponentProgress, FileInfo, InstallProgress, ModelStatus, ScanProgress,
        ScanReport,
    },
    components::{self, ComponentRow},
    discovery,
    error::ErrorDto,
    install::{self, Inputs, InstallManifest, Preview},
    library::Library,
    model::{Game, GameStatus},
    scan,
    settings::Settings,
    Error,
};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::AppState;

type CmdResult<T> = Result<T, ErrorDto>;

pub const SCAN_PROGRESS_EVENT: &str = "scan://progress";
pub const COMPONENT_PROGRESS_EVENT: &str = "component://progress";
pub const INSTALL_PROGRESS_EVENT: &str = "install://progress";

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        platform: std::env::consts::OS.to_string(),
        data_dir: state.paths.root().display().to_string(),
        can_scan: shim_win::is_windows(),
        startup_warnings: state.startup_warnings.clone(),
    }
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> CmdResult<Settings> {
    Ok(state.settings.lock().map_err(poisoned)?.clone())
}

/// Validate, persist, then swap in memory while holding the lock, so two
/// concurrent saves cannot leave disk and memory disagreeing.
#[tauri::command]
pub async fn save_settings(state: State<'_, AppState>, settings: Settings) -> CmdResult<Settings> {
    let settings = settings.validated();
    let mut current = state.settings.lock().map_err(poisoned)?;
    settings.save(&state.paths)?;
    let hidden_changed = current.hidden_games != settings.hidden_games;
    *current = settings.clone();
    drop(current);

    if hidden_changed {
        let mut library = state.library.lock().map_err(poisoned)?;
        let updated = library.with_hidden(&settings.hidden_games);
        updated.save(&state.paths)?;
        *library = updated;
    }
    Ok(settings)
}

#[tauri::command]
pub fn get_library(state: State<'_, AppState>) -> CmdResult<Library> {
    Ok(state.library.lock().map_err(poisoned)?.clone())
}

/// What an install would do for one game: the exact plan when components and
/// user files are in place, otherwise a sketch plus the blocker.
#[tauri::command]
pub fn plan_preview(state: State<'_, AppState>, game_id: String) -> CmdResult<Preview> {
    let settings = state.settings.lock().map_err(poisoned)?.clone();
    let game = find_game(&state, &game_id)?;
    Ok(install::preview(&Inputs {
        game: &game,
        store: &state.store,
        components: &state.components,
        settings: &settings,
    }))
}

#[tauri::command]
pub fn get_install(
    state: State<'_, AppState>,
    game_id: String,
) -> CmdResult<Option<InstallManifest>> {
    Ok(InstallManifest::load(&state.paths, &game_id)?)
}

#[tauri::command]
pub fn get_components(state: State<'_, AppState>) -> Vec<ComponentRow> {
    components::rows(&state.components, &state.store)
}

/// Download, verify and unpack one component. Emits `component://progress`.
#[tauri::command]
pub async fn fetch_component(app: AppHandle, id: String) -> CmdResult<ComponentRow> {
    let state = app.state::<AppState>();
    let _guard = Busy::acquire(&state.busy, "Another install or download is running.")?;
    let component = state
        .components
        .get(&id)
        .cloned()
        .ok_or_else(|| Error::ComponentMissing {
            id: id.clone(),
            detail: "not in the component list".into(),
        })?;
    let worker = app.clone();
    blocking(move || {
        let state = worker.state::<AppState>();
        let mut emit = |p: shim_core::components::fetch::Progress| {
            emit_event(
                &worker,
                COMPONENT_PROGRESS_EVENT,
                &ComponentProgress {
                    id: component.id.clone(),
                    received: p.received,
                    expected: p.expected,
                },
            );
        };
        state.store.fetch(&component, &mut emit)?;
        Ok(ComponentRow {
            status: state.store.status(&component),
            component,
        })
    })
    .await
}

/// Size, SHA-256 and Authenticode status of a user-supplied file.
#[tauri::command]
pub async fn inspect_file(app: AppHandle, path: String) -> CmdResult<FileInfo> {
    blocking(move || {
        let state = app.state::<AppState>();
        let path = PathBuf::from(path);
        let meta = std::fs::metadata(&path).map_err(|e| Error::io(&path, e))?;
        if !meta.is_file() {
            return Err(Error::UserFileMissing {
                what: "file".into(),
                detail: format!("{} is not a file", path.display()),
            }
            .into());
        }
        Ok(FileInfo {
            path: path.display().to_string(),
            size: meta.len(),
            sha256: shim_core::components::fetch::sha256_file(&path)?,
            signature: state.platform.signatures.check(&path).into(),
        })
    })
    .await
}

/// The GPU, the chosen model's build, and whether the two suit each other.
#[tauri::command]
pub async fn model_status(app: AppHandle) -> CmdResult<ModelStatus> {
    use shim_core::model_fit;
    blocking(move || {
        let state = app.state::<AppState>();
        let gpu = shim_core::report::detect_gpu(state.platform.registry.as_ref());
        let family = gpu.as_ref().map(|g| model_fit::gpu_family(&g.name));
        let model = state.settings.lock().map_err(poisoned)?.model_path.clone();
        let sha = match model.filter(|p| p.is_file()) {
            Some(path) => Some(cached_sha256(&state, &path)?),
            None => None,
        };
        let build = sha.as_deref().map(model_fit::model_build);
        Ok(ModelStatus {
            fit: model_fit::model_fit(family, build),
            gpu,
            family,
            build,
        })
    })
    .await
}

/// Current hashes of the user's own files. A file that can't be read is left
/// out, which never marks an install as outdated.
fn user_hashes(state: &AppState, settings: &Settings) -> shim_core::install::manifest::UserHashes {
    use shim_core::install::planner::{DLSS_RUNTIME_DLL, RENODX_ADDON};
    let model = settings
        .model_path
        .as_deref()
        .filter(|p| p.is_file())
        .and_then(|p| cached_sha256(state, p).ok());
    let files = [
        (RENODX_ADDON, settings.renodx_addon_path.as_deref()),
        (DLSS_RUNTIME_DLL, settings.dlss_runtime_path.as_deref()),
    ]
    .into_iter()
    .filter_map(|(name, path)| {
        let path = path.filter(|p| p.is_file())?;
        let sha = shim_core::components::fetch::sha256_file(path).ok()?;
        Some((name.to_string(), sha))
    })
    .collect();
    shim_core::install::manifest::UserHashes { model, files }
}

/// What an Update would change for an installed game, in words. Empty when
/// the install is current.
#[tauri::command]
pub async fn update_changes(app: AppHandle, game_id: String) -> CmdResult<Vec<String>> {
    blocking(move || {
        let state = app.state::<AppState>();
        let settings = state.settings.lock().map_err(poisoned)?.clone();
        let Some(manifest) = InstallManifest::load(&state.paths, &game_id)? else {
            return Ok(Vec::new());
        };
        Ok(manifest.changes(&state.components, &user_hashes(&state, &settings)))
    })
    .await
}

/// Bring an installed game up to the current pins and the user's current
/// files. Order matters: missing components are fetched and the new plan is
/// checked *before* the old install is touched, so a failure leaves the game
/// exactly as it was.
#[tauri::command]
pub async fn update_game(app: AppHandle, game_id: String) -> CmdResult<Game> {
    let state = app.state::<AppState>();
    let _guard = Busy::acquire(&state.busy, "Another install or download is running.")?;
    let worker = app.clone();
    blocking(move || {
        let state = worker.state::<AppState>();
        let settings = state.settings.lock().map_err(poisoned)?.clone();
        let game = find_game(&state, &game_id)?;
        let manifest =
            InstallManifest::load(&state.paths, &game_id)?.ok_or(Error::NotInstalled)?;
        let say = |message: String| {
            emit_event(
                &worker,
                INSTALL_PROGRESS_EVENT,
                &InstallProgress {
                    game_id: game_id.clone(),
                    index: 0,
                    total: 0,
                    message,
                },
            );
        };

        // 1. Components the route needs, fetched now if this version isn't here yet.
        let neural = game.neural.unwrap_or_default();
        for id in shim_core::install::planner::components_for(manifest.route, neural) {
            let Some(c) = state.components.get(id) else {
                continue;
            };
            if state.store.status(c) == shim_core::components::ComponentStatus::Verified {
                continue;
            }
            say(format!("Downloading {} {}", c.name, c.version));
            let mut emit = |p: shim_core::components::fetch::Progress| {
                emit_event(
                    &worker,
                    COMPONENT_PROGRESS_EVENT,
                    &ComponentProgress {
                        id: c.id.clone(),
                        received: p.received,
                        expected: p.expected,
                    },
                );
            };
            state.store.fetch(c, &mut emit)?;
        }

        // 2. Would the new install succeed once the old one is gone?
        let target = Game {
            status: GameStatus::Ready {
                route: manifest.route,
                reason: String::new(),
            },
            ..game.clone()
        };
        let inputs = Inputs {
            game: &target,
            store: &state.store,
            components: &state.components,
            settings: &settings,
        };
        let owned: Vec<_> = manifest.files.iter().map(|f| f.target.clone()).collect();
        shim_core::install::planner::plan_over(&inputs, &owned)?;

        // 3. Remove the old install, then install with the current pins.
        say("Removing the old install".into());
        install::uninstall(&state.paths, &manifest)?;
        // From here on the old install is gone. Should the new one still fail
        // (the check above makes that rare), say so plainly and leave the
        // library showing the game as not installed.
        let after_removal = |e: Error| -> ErrorDto {
            tracing::error!(detail = %e.detail(), "update failed after removing the old install");
            let fresh = scan::analyse_game(&game, &state.paths, &state.components);
            if let Err(e) = update_game_record(&state, fresh) {
                tracing::warn!(detail = %e.detail, "library not updated after a failed update");
            }
            ErrorDto {
                code: e.code().to_string(),
                message: format!(
                    "The old install was removed and the folder restored, but the new one could not be written: {} Install again once that is fixed.",
                    e.user_message()
                ),
                detail: e.detail(),
            }
        };
        let plan = install::plan(&inputs).map_err(after_removal)?;
        let total = plan.ops.len();
        let mut emit = |s: install::Step| {
            emit_event(
                &worker,
                INSTALL_PROGRESS_EVENT,
                &InstallProgress {
                    game_id: game_id.clone(),
                    index: s.index,
                    total,
                    message: s.message,
                },
            );
        };
        let fresh = install::Journal::run(
            &state.paths,
            &game_id,
            &plan,
            manifest.anti_cheat_override,
            &mut emit,
        )
        .map_err(after_removal)?;
        tracing::info!(title = %game.title, route = ?fresh.route, files = fresh.files.len(), "updated");
        update_game_record(
            &state,
            Game {
                status: GameStatus::Installed { route: fresh.route },
                ..game
            },
        )
    })
    .await
}

fn cached_sha256(state: &AppState, path: &std::path::Path) -> CmdResult<String> {
    let meta = std::fs::metadata(path).map_err(|e| Error::io(path, e))?;
    let (size, modified) = (meta.len(), meta.modified().ok());
    let mut cache = state.model_hash.lock().map_err(poisoned)?;
    if let Some(c) = cache.as_ref() {
        if c.path == path && c.size == size && c.modified == modified {
            return Ok(c.sha256.clone());
        }
    }
    let sha256 = shim_core::components::fetch::sha256_file(path)?;
    *cache = Some(crate::state::HashCache {
        path: path.to_path_buf(),
        size,
        modified,
        sha256: sha256.clone(),
    });
    Ok(sha256)
}

/// Plan and run the install for one game. `confirm_anti_cheat` must be true
/// for a game with anti-cheat markers; the override is recorded.
#[tauri::command]
pub async fn install_game(
    app: AppHandle,
    game_id: String,
    confirm_anti_cheat: bool,
) -> CmdResult<Game> {
    let state = app.state::<AppState>();
    let _guard = Busy::acquire(&state.busy, "Another install or download is running.")?;
    let worker = app.clone();
    blocking(move || {
        let state = worker.state::<AppState>();
        let settings = state.settings.lock().map_err(poisoned)?.clone();
        let mut game = find_game(&state, &game_id)?;
        if InstallManifest::load(&state.paths, &game_id)?.is_some() {
            return Err(Error::AlreadyInstalled.into());
        }
        let anti_cheat = game.analysis.as_ref().and_then(|a| a.anti_cheat);
        if let Some(which) = anti_cheat {
            if !confirm_anti_cheat {
                return Err(Error::AntiCheatBlocked {
                    which: format!("{which:?}"),
                }
                .into());
            }
            // The override routes the game as if it had no anti-cheat.
            let mut a = game.analysis.clone().expect("anti_cheat implies analysis");
            a.anti_cheat = None;
            game.status = shim_core::routing::decide(&a, game.mode);
            game.analysis = Some(a);
        }
        let plan = install::plan(&Inputs {
            game: &game,
            store: &state.store,
            components: &state.components,
            settings: &settings,
        })?;
        let total = plan.ops.len();
        let mut emit = |s: install::Step| {
            emit_event(
                &worker,
                INSTALL_PROGRESS_EVENT,
                &InstallProgress {
                    game_id: game_id.clone(),
                    index: s.index,
                    total,
                    message: s.message,
                },
            );
        };
        let manifest = install::Journal::run(
            &state.paths,
            &game_id,
            &plan,
            anti_cheat.is_some(),
            &mut emit,
        )?;
        tracing::info!(title = %game.title, route = ?manifest.route, files = manifest.files.len(), "installed");
        let updated = Game {
            status: GameStatus::Installed {
                route: manifest.route,
            },
            ..game
        };
        update_game_record(&state, updated)
    })
    .await
}

/// Restore every backed-up file and delete every file we added.
#[tauri::command]
pub async fn remove_game(app: AppHandle, game_id: String) -> CmdResult<Game> {
    let state = app.state::<AppState>();
    let _guard = Busy::acquire(&state.busy, "Another install or download is running.")?;
    let worker = app.clone();
    blocking(move || {
        let state = worker.state::<AppState>();
        let game = find_game(&state, &game_id)?;
        let manifest = InstallManifest::load(&state.paths, &game_id)?.ok_or(Error::NotInstalled)?;
        emit_event(
            &worker,
            INSTALL_PROGRESS_EVENT,
            &InstallProgress {
                game_id: game_id.clone(),
                index: 0,
                total: manifest.files.len(),
                message: "restoring backups and removing added files".into(),
            },
        );
        install::uninstall(&state.paths, &manifest)?;
        tracing::info!(title = %game.title, "removed");
        // Re-derive the status from the folder as it is now.
        let fresh = scan::analyse_game(
            &Game {
                analysis: None,
                ..game
            },
            &state.paths,
            &state.components,
        );
        update_game_record(&state, fresh)
    })
    .await
}

/// Choose the route for a game that ships DLSS. Refused while installed:
/// remove first, then the new route applies to the next install.
#[tauri::command]
pub fn set_game_mode(
    state: State<'_, AppState>,
    game_id: String,
    mode: Option<shim_core::model::InstallMode>,
) -> CmdResult<Game> {
    let game = find_game(&state, &game_id)?;
    if matches!(
        game.status,
        GameStatus::Installed { .. } | GameStatus::UpdateAvailable { .. }
    ) {
        return Err(Error::AlreadyInstalled.into());
    }
    let status = match &game.analysis {
        Some(a) => shim_core::routing::decide(a, mode),
        None => game.status.clone(),
    };
    update_game_record(
        &state,
        Game {
            mode,
            status,
            ..game
        },
    )
}

/// Choose the neural-pass placement for the OptiScaler + DLSS 5 mode. Refused
/// while installed, because the placement decides which fork is installed.
#[tauri::command]
pub fn set_neural_options(
    state: State<'_, AppState>,
    game_id: String,
    options: Option<shim_core::model::NeuralOptions>,
) -> CmdResult<Game> {
    let game = find_game(&state, &game_id)?;
    if matches!(
        game.status,
        GameStatus::Installed { .. } | GameStatus::UpdateAvailable { .. }
    ) {
        return Err(Error::AlreadyInstalled.into());
    }
    update_game_record(
        &state,
        Game {
            neural: options,
            ..game
        },
    )
}

/// The decisive lines of the component log beside the exe after the game's
/// last run, so a failed pass can be read without opening the file.
#[tauri::command]
pub fn last_run(
    state: State<'_, AppState>,
    game_id: String,
) -> CmdResult<Option<shim_core::api::LastRun>> {
    let game = find_game(&state, &game_id)?;
    let Some(exe_dir) = game
        .analysis
        .as_ref()
        .and_then(|a| a.exe.parent().map(Path::to_path_buf))
    else {
        return Ok(None);
    };
    Ok(shim_core::lastrun::read(&exe_dir))
}

/// The exact report that "Share this result" would send. `outcome` None =
/// the app's suggestion from the last run's log.
#[tauri::command]
pub async fn report_preview(
    app: AppHandle,
    game_id: String,
    outcome: Option<shim_core::report::Outcome>,
) -> CmdResult<shim_core::report::Report> {
    blocking(move || build_report(&app.state::<AppState>(), &game_id, outcome)).await
}

/// Build the same report again and send it. Only ever called from the
/// share form, after the user has seen the preview.
#[tauri::command]
pub async fn submit_report(
    app: AppHandle,
    game_id: String,
    outcome: shim_core::report::Outcome,
) -> CmdResult<()> {
    blocking(move || {
        let report = build_report(&app.state::<AppState>(), &game_id, Some(outcome))?;
        shim_core::report::send(&report)?;
        tracing::info!(title = %report.title, outcome = ?report.outcome, "result report sent");
        Ok(())
    })
    .await
}

fn build_report(
    state: &AppState,
    game_id: &str,
    outcome: Option<shim_core::report::Outcome>,
) -> CmdResult<shim_core::report::Report> {
    use shim_core::platform::Signature;
    use shim_core::report;
    let game = find_game(state, game_id)?;
    let manifest = InstallManifest::load(&state.paths, game_id)?.ok_or(Error::NotInstalled)?;
    let last_run = manifest.exe.parent().and_then(shim_core::lastrun::read);
    let model = state.settings.lock().map_err(poisoned)?.model_path.clone();
    let model_signed =
        model
            .filter(|p| p.is_file())
            .and_then(|p| match state.platform.signatures.check(&p) {
                Signature::Valid { .. } => Some(true),
                Signature::HashMismatch { .. } | Signature::Unsigned => Some(false),
                Signature::Unknown(_) => None,
            });
    Ok(report::build(&report::Inputs {
        game: &game,
        manifest: &manifest,
        gpu: report::detect_gpu(state.platform.registry.as_ref()),
        model_signed,
        last_run: last_run.as_ref(),
        outcome: outcome.unwrap_or_else(|| report::suggested_outcome(last_run.as_ref())),
        app_version: env!("CARGO_PKG_VERSION"),
    }))
}

/// Re-analyse one game from scratch (ignores the size+mtime cache).
#[tauri::command]
pub async fn rescan_game(app: AppHandle, game_id: String) -> CmdResult<Game> {
    blocking(move || {
        let state = app.state::<AppState>();
        let game = find_game(&state, &game_id)?;
        let settings = state.settings.lock().map_err(poisoned)?.clone();
        let fresh = scan::analyse_game_with(
            &Game {
                analysis: None,
                ..game
            },
            &state.paths,
            &state.components,
            &user_hashes(&state, &settings),
        );
        update_game_record(&state, fresh)
    })
    .await
}

/// Show the game's executable in Explorer. The path comes from the library,
/// never from the front end, so no path scope is needed.
#[tauri::command]
pub fn open_folder(state: State<'_, AppState>, game_id: String) -> CmdResult<()> {
    let game = find_game(&state, &game_id)?;
    let target = game
        .analysis
        .as_ref()
        .map(|a| a.exe.clone())
        .unwrap_or_else(|| game.install_dir.clone());
    let mut cmd = std::process::Command::new("explorer.exe");
    if target.is_file() {
        cmd.arg(format!("/select,{}", target.display()));
    } else {
        cmd.arg(target.as_os_str());
    }
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| Error::io(&target, e).to_dto())
}

/// Fetch missing covers for every visible game, one at a time, emitting
/// `cover://ready` as each lands. Returns the library with cover paths set.
#[tauri::command]
pub async fn fetch_covers(app: AppHandle) -> CmdResult<Library> {
    blocking(move || {
        let state = app.state::<AppState>();
        let games = state.library.lock().map_err(poisoned)?.games.clone();
        let covers_dir = state.paths.covers_dir();
        let steam_root = shim_core::discovery::steam::steam_path(&state.platform);
        let mut updated = Vec::with_capacity(games.len());
        for game in games {
            let game = if game.cover.as_ref().is_some_and(|c| c.is_file()) {
                game
            } else {
                match shim_core::artwork::fetch_cover(&covers_dir, &game, steam_root.as_deref()) {
                    Ok(Some(path)) => {
                        emit_event(
                            &app,
                            COVER_READY_EVENT,
                            &shim_core::api::CoverReady {
                                game_id: game.id.clone(),
                                path: path.display().to_string(),
                            },
                        );
                        Game {
                            cover: Some(path),
                            ..game
                        }
                    }
                    Ok(None) => game,
                    Err(e) => {
                        tracing::debug!(title = %game.title, detail = %e.detail(), "no cover");
                        game
                    }
                }
            };
            updated.push(game);
        }
        let mut library = state.library.lock().map_err(poisoned)?;
        // Keep any status changes that happened meanwhile; only covers come from us.
        let games = library
            .games
            .iter()
            .map(|g| Game {
                cover: updated
                    .iter()
                    .find(|u| u.id == g.id)
                    .and_then(|u| u.cover.clone())
                    .or_else(|| g.cover.clone()),
                ..g.clone()
            })
            .collect();
        let merged = Library {
            games,
            ..library.clone()
        };
        merged.save(&state.paths)?;
        *library = merged.clone();
        Ok(merged)
    })
    .await
}

/// Ask GitHub whether a newer release exists. `None` when up to date.
#[tauri::command]
pub async fn check_update() -> CmdResult<Option<shim_core::update::UpdateInfo>> {
    blocking(|| Ok(shim_core::update::check(env!("CARGO_PKG_VERSION"))?)).await
}

/// Relaunch shim as administrator and quit this instance once UAC agreed.
#[tauri::command]
pub fn relaunch_elevated(app: AppHandle) -> CmdResult<()> {
    shim_win::relaunch_elevated().map_err(|detail| ErrorDto {
        code: "elevation_declined".into(),
        message: "shim was not restarted as administrator.".into(),
        detail,
    })?;
    app.exit(0);
    Ok(())
}

pub const COVER_READY_EVENT: &str = "cover://ready";

/// Discover, analyse and route every game, then persist. Runs on a blocking
/// thread and emits `scan://progress`. A second scan while one runs is refused.
#[tauri::command]
pub async fn scan_library(app: AppHandle) -> CmdResult<ScanReport> {
    let state = app.state::<AppState>();
    let _guard = Busy::acquire(&state.scanning, "A scan is already running.")?;
    let worker = app.clone();
    blocking(move || run_scan(&worker)).await
}

fn run_scan(app: &AppHandle) -> CmdResult<ScanReport> {
    let state = app.state::<AppState>();
    let settings = state.settings.lock().map_err(poisoned)?.clone();
    let before = state.library.lock().map_err(poisoned)?.clone();
    let adapters = discovery::default_adapters(&settings.scan);

    let mut emit = |p: ScanProgress| emit_event(app, SCAN_PROGRESS_EVENT, &p);
    let user = user_hashes(&state, &settings);
    let ctx = scan::Context {
        settings: &settings,
        platform: &state.platform,
        adapters: &adapters,
        paths: &state.paths,
        components: &state.components,
        user: &user,
    };
    let result = scan::run(&before, &ctx, now(), &mut emit);

    // The hidden list may have changed during the scan; apply the latest.
    let hidden = state
        .settings
        .lock()
        .map_err(poisoned)?
        .hidden_games
        .clone();
    let mut library = state.library.lock().map_err(poisoned)?;
    let merged = result.library.with_hidden(&hidden);
    merged.save(&state.paths)?;
    *library = merged.clone();

    tracing::info!(
        games = merged.games.len(),
        failures = result.outcome.failures.len(),
        unavailable = result.outcome.unavailable.len(),
        "scan done"
    );
    Ok(ScanReport {
        library: merged,
        outcome: result.outcome,
    })
}

fn find_game(state: &AppState, game_id: &str) -> CmdResult<Game> {
    state
        .library
        .lock()
        .map_err(poisoned)?
        .games
        .iter()
        .find(|g| g.id == game_id)
        .cloned()
        .ok_or_else(|| ErrorDto {
            code: "not_found".into(),
            message: "This game is no longer in the library.".into(),
            detail: format!("no game with id {game_id}"),
        })
}

/// Replace one game in the library, persist, return it.
fn update_game_record(state: &AppState, game: Game) -> CmdResult<Game> {
    let mut library = state.library.lock().map_err(poisoned)?;
    let games = library
        .games
        .iter()
        .map(|g| {
            if g.id == game.id {
                game.clone()
            } else {
                g.clone()
            }
        })
        .collect();
    let updated = Library {
        games,
        ..library.clone()
    };
    updated.save(&state.paths)?;
    *library = updated;
    Ok(game)
}

/// Run `f` on a blocking thread; a panic becomes an `ErrorDto`.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> CmdResult<T> + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .unwrap_or_else(|e| {
            Err(ErrorDto {
                code: "internal".into(),
                message: "The operation stopped unexpectedly. Try again.".into(),
                detail: format!("worker thread panicked: {e}"),
            })
        })
}

fn emit_event<T: serde::Serialize + Clone>(app: &AppHandle, event: &str, payload: &T) {
    if let Err(e) = app.emit(event, payload) {
        tracing::debug!(%event, %e, "event not delivered");
    }
}

/// Clears a busy flag when dropped, so an early `?` cannot leave it set.
struct Busy<'a>(&'a AtomicBool);

impl<'a> Busy<'a> {
    fn acquire(flag: &'a AtomicBool, message: &str) -> CmdResult<Self> {
        if flag.swap(true, Ordering::AcqRel) {
            return Err(ErrorDto {
                code: "busy".into(),
                message: message.into(),
                detail: "a previous operation has not finished".into(),
            });
        }
        Ok(Self(flag))
    }
}

impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn poisoned<T>(_: std::sync::PoisonError<T>) -> ErrorDto {
    ErrorDto {
        code: "internal".into(),
        message: "The app hit an internal error. Restart it and try again.".into(),
        detail: "a state mutex was poisoned by an earlier panic".into(),
    }
}

#[allow(dead_code)]
fn _assert_path_unused(_: &Path) {}
