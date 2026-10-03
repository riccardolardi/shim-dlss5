//! Shapes that cross the Tauri boundary but are not domain models.
//! Kept in the core so one `cargo test -p shim-core` regenerates every
//! TypeScript binding.

use serde::Serialize;
use ts_rs::TS;

use crate::{discovery::DiscoveryOutcome, error::ErrorDto, library::Library};

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct AppInfo {
    pub version: String,
    pub platform: String,
    pub data_dir: String,
    /// False on macOS/Linux dev hosts: scanning and installing are disabled.
    pub can_scan: bool,
    /// Problems found while starting, e.g. a damaged settings file that was
    /// moved aside. Shown once by the UI.
    pub startup_warnings: Vec<ErrorDto>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ScanReport {
    pub library: Library,
    pub outcome: DiscoveryOutcome,
}

/// Emitted as the `scan://progress` event while a scan runs. `total` is 0
/// until discovery has finished; `title` is the game being analysed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ScanProgress {
    pub done: usize,
    pub total: usize,
    pub title: Option<String>,
}
