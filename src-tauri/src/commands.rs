//! Commands the front end can invoke. Each returns a serialisable value or an
//! `ErrorDto`. Anything that touches disk or scans runs off the main thread.

use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

use shim_core::{
    api::{AppInfo, ScanReport},
    discovery,
    error::ErrorDto,
    library::Library,
    settings::Settings,
};
use tauri::State;

use crate::state::AppState;

type CmdResult<T> = Result<T, ErrorDto>;

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

/// Run every enabled launcher adapter, fold the result into the library,
/// persist. Analysis (exe, API, anti-cheat) is Phase 1; new games start as
/// `Pending`. A second scan while one runs is refused.
#[tauri::command]
pub async fn scan_library(state: State<'_, AppState>) -> CmdResult<ScanReport> {
    if state.scanning.swap(true, Ordering::AcqRel) {
        return Err(ErrorDto {
            code: "busy".into(),
            message: "A scan is already running.".into(),
            detail: "scan_library called while scanning was true".into(),
        });
    }
    let result = run_scan(&state);
    state.scanning.store(false, Ordering::Release);
    result
}

fn run_scan(state: &AppState) -> CmdResult<ScanReport> {
    let scan_sources = state.settings.lock().map_err(poisoned)?.scan.clone();
    let adapters = discovery::default_adapters();
    let outcome = discovery::discover_all(&adapters, &scan_sources, &state.platform);

    // Lock settings again (hidden list may have changed during the scan) and
    // the library together, so the merge sees one consistent snapshot.
    let hidden = state
        .settings
        .lock()
        .map_err(poisoned)?
        .hidden_games
        .clone();
    let mut library = state.library.lock().map_err(poisoned)?;
    let merged = library.merge_scan(&outcome, &hidden, now());
    merged.save(&state.paths)?;
    *library = merged.clone();

    tracing::info!(
        games = merged.games.len(),
        failures = outcome.failures.len(),
        unavailable = outcome.unavailable.len(),
        "scan done"
    );
    Ok(ScanReport {
        library: merged,
        outcome,
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
