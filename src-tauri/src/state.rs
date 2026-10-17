//! Process-wide state handed to commands.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use std::time::SystemTime;

use shim_core::{
    components::{ComponentManifest, ComponentStore},
    error::ErrorDto,
    library::Library,
    paths::AppPaths,
    platform::Platform,
    settings::Settings,
};

pub struct AppState {
    pub paths: AppPaths,
    pub platform: Platform,
    pub settings: Mutex<Settings>,
    pub library: Mutex<Library>,
    /// Pinned component list (embedded copy for now).
    pub components: ComponentManifest,
    pub store: ComponentStore,
    /// Set while a scan runs; a second scan is refused instead of racing.
    pub scanning: AtomicBool,
    /// Set while an install, removal or component fetch runs.
    pub busy: AtomicBool,
    pub startup_warnings: Vec<ErrorDto>,
    /// SHA-256 of the user's model, so the 165 MB file is hashed once per
    /// change rather than on every screen visit.
    pub model_hash: Mutex<Option<HashCache>>,
}

/// A file's hash, valid while its path, size and modification time match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HashCache {
    pub path: PathBuf,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub sha256: String,
}

impl AppState {
    /// Load settings and the cached library from an already-prepared data
    /// directory. Damaged files are moved aside and reported, never fatal.
    pub fn boot(paths: AppPaths) -> Self {
        let (settings, settings_err) = Settings::load_or_reset(&paths);
        let (library, library_err) = Library::load_or_reset(&paths);
        let mut startup_warnings: Vec<ErrorDto> = [settings_err, library_err]
            .into_iter()
            .flatten()
            .inspect(|e| tracing::warn!(detail = %e.detail(), "startup problem"))
            .map(|e| e.to_dto())
            .collect();

        // An install interrupted by a crash leaves a journal; finish its undo,
        // whether or not the game is still in the library.
        let (rolled_back, errors) = shim_core::install::recover_all(&paths);
        for id in &rolled_back {
            tracing::warn!(game_id = %id, "rolled back an interrupted install");
        }
        for e in errors {
            tracing::error!(detail = %e.detail(), "recovery failed");
            startup_warnings.push(e.to_dto());
        }

        tracing::info!(root = %paths.root().display(), "data directory ready");
        Self {
            store: ComponentStore::new(&paths),
            paths,
            platform: shim_win::platform(),
            settings: Mutex::new(settings),
            library: Mutex::new(library),
            components: ComponentManifest::embedded(),
            scanning: AtomicBool::new(false),
            busy: AtomicBool::new(false),
            startup_warnings,
            model_hash: Mutex::new(None),
        }
    }
}
