//! Commands the front end can invoke. Each returns a serialisable value or an
//! `ErrorDto`. Anything that touches disk or scans runs off the main thread.

use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

use shim_core::{
    api::{AppInfo, ScanProgress, ScanReport},
    discovery,
    error::ErrorDto,
    install::PlannedChange,
    library::Library,
    scan,
    settings::Settings,
};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::AppState;

type CmdResult<T> = Result<T, ErrorDto>;

/// Event carrying a `ScanProgress` while `scan_library` runs.
pub const SCAN_PROGRESS_EVENT: &str = "scan://progress";

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

/// The files an install would add, back up or edit for one game. Empty when
/// the game has no route.
#[tauri::command]
pub fn plan_preview(state: State<'_, AppState>, game_id: String) -> CmdResult<Vec<PlannedChange>> {
    let library = state.library.lock().map_err(poisoned)?;
    Ok(library
        .games
        .iter()
        .find(|g| g.id == game_id)
        .map(shim_core::install::preview)
        .unwrap_or_default())
}

/// Discover, analyse and route every game, then persist. Runs on a blocking
/// thread and emits `scan://progress`. A second scan while one runs is refused.
#[tauri::command]
pub async fn scan_library(app: AppHandle) -> CmdResult<ScanReport> {
    let state = app.state::<AppState>();
    if state.scanning.swap(true, Ordering::AcqRel) {
        return Err(ErrorDto {
            code: "busy".into(),
            message: "A scan is already running.".into(),
            detail: "scan_library called while scanning was true".into(),
        });
    }
    let worker = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || run_scan(&worker))
        .await
        .unwrap_or_else(|e| {
            Err(ErrorDto {
                code: "internal".into(),
                message: "The scan stopped unexpectedly. Try again.".into(),
                detail: format!("scan thread panicked: {e}"),
            })
        });
    state.scanning.store(false, Ordering::Release);
    result
}

fn run_scan(app: &AppHandle) -> CmdResult<ScanReport> {
    let state = app.state::<AppState>();
    let settings = state.settings.lock().map_err(poisoned)?.clone();
    let before = state.library.lock().map_err(poisoned)?.clone();
    let adapters = discovery::default_adapters();

    let mut emit = |p: ScanProgress| {
        if let Err(e) = app.emit(SCAN_PROGRESS_EVENT, &p) {
            tracing::debug!(%e, "progress event not delivered");
        }
    };
    let result = scan::run(
        &before,
        &settings,
        &state.platform,
        &adapters,
        now(),
        &mut emit,
    );

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
