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

/// A user-supplied file as shown on the Components screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct FileInfo {
    pub path: String,
    #[ts(type = "number")]
    pub size: u64,
    pub sha256: String,
    pub signature: SignatureDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum SignatureDto {
    Valid { signer: String },
    HashMismatch { signer: String },
    Unsigned,
    Unknown { detail: String },
}

impl From<crate::platform::Signature> for SignatureDto {
    fn from(s: crate::platform::Signature) -> Self {
        use crate::platform::Signature as S;
        match s {
            S::Valid { signer } => Self::Valid { signer },
            S::HashMismatch { signer } => Self::HashMismatch { signer },
            S::Unsigned => Self::Unsigned,
            S::Unknown(detail) => Self::Unknown { detail },
        }
    }
}

/// Emitted as `component://progress` while a component downloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ComponentProgress {
    pub id: String,
    #[ts(type = "number")]
    pub received: u64,
    #[ts(type = "number")]
    pub expected: u64,
}

/// Emitted as `install://progress` while an install or removal runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct InstallProgress {
    pub game_id: String,
    pub index: usize,
    pub total: usize,
    pub message: String,
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
