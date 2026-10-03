//! Process-wide state handed to commands.

use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

use shim_core::{
    error::ErrorDto, library::Library, paths::AppPaths, platform::Platform, settings::Settings,
};

pub struct AppState {
    pub paths: AppPaths,
    pub platform: Platform,
    pub settings: Mutex<Settings>,
    pub library: Mutex<Library>,
    /// Set while a scan runs; a second scan is refused instead of racing.
    pub scanning: AtomicBool,
    pub startup_warnings: Vec<ErrorDto>,
}

impl AppState {
    /// Load settings and the cached library from an already-prepared data
    /// directory. Damaged files are moved aside and reported, never fatal.
    pub fn boot(paths: AppPaths) -> Self {
        let (settings, settings_err) = Settings::load_or_reset(&paths);
        let (library, library_err) = Library::load_or_reset(&paths);
        let startup_warnings: Vec<ErrorDto> = [settings_err, library_err]
            .into_iter()
            .flatten()
            .inspect(|e| tracing::warn!(detail = %e.detail(), "startup problem"))
            .map(|e| e.to_dto())
            .collect();

        tracing::info!(root = %paths.root().display(), "data directory ready");
        Self {
            paths,
            platform: shim_win::platform(),
            settings: Mutex::new(settings),
            library: Mutex::new(library),
            scanning: AtomicBool::new(false),
            startup_warnings,
        }
    }
}
