//! Commands the front end can invoke. Each returns a serialisable value or an
//! `ErrorDto`. Anything that touches disk, scans or downloads runs off the
//! main thread and reports progress through events.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use shim_core::{
    api::{AppInfo, ComponentProgress, FileInfo, InstallProgress, ScanProgress, ScanReport},
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
        update_game(&state, updated)
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
        update_game(&state, fresh)
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
    update_game(
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
    update_game(
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

/// Re-analyse one game from scratch (ignores the size+mtime cache).
#[tauri::command]
pub async fn rescan_game(app: AppHandle, game_id: String) -> CmdResult<Game> {
    blocking(move || {
        let state = app.state::<AppState>();
        let game = find_game(&state, &game_id)?;
        let fresh = scan::analyse_game(
            &Game {
                analysis: None,
                ..game
            },
            &state.paths,
            &state.components,
        );
        update_game(&state, fresh)
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
    let ctx = scan::Context {
        settings: &settings,
        platform: &state.platform,
        adapters: &adapters,
        paths: &state.paths,
        components: &state.components,
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
fn update_game(state: &AppState, game: Game) -> CmdResult<Game> {
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
