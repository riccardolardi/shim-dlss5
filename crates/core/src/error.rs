//! One error type for the whole core.
//!
//! Every variant carries enough context to produce a short message for the
//! user and a longer detail string for the log drawer. Nothing in the core is
//! allowed to swallow an error silently.

use std::path::PathBuf;

use serde::Serialize;
use ts_rs::TS;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not read or write {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("could not parse {what}: {source}")]
    Parse {
        what: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("no data directory is available on this system")]
    NoDataDir,

    #[error("{feature} is not available on this platform")]
    UnsupportedPlatform { feature: String },

    #[error("launcher adapter {adapter} failed: {detail}")]
    Adapter { adapter: String, detail: String },

    #[error("no executable found under {install_dir}")]
    NoExecutable { install_dir: PathBuf },

    #[error("{path} is not a Windows executable: {detail}")]
    NotAnExecutable { path: PathBuf, detail: String },

    #[error("components manifest is invalid: {detail}")]
    ManifestInvalid { detail: String },

    #[error("components manifest signature check failed: {detail}")]
    ManifestSignature { detail: String },

    #[error("download of {url} failed: {detail}")]
    Download { url: String, detail: String },

    #[error("{what}: SHA-256 is {actual}, expected {expected}")]
    HashMismatch {
        what: String,
        expected: String,
        actual: String,
    },

    #[error("archive problem: {detail}")]
    Archive { detail: String },

    #[error("component {id} is not available: {detail}")]
    ComponentMissing { id: String, detail: String },

    #[error("{what} has not been chosen or is missing: {detail}")]
    UserFileMissing { what: String, detail: String },

    #[error("{path} is occupied by a DLL we do not recognise")]
    ForeignProxyPresent { path: PathBuf },

    #[error("{which} anti-cheat was detected and the override was not confirmed")]
    AntiCheatBlocked { which: String },

    #[error("game folder {path} is not writable: {source}")]
    GameFolderNotWritable {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("this game is already installed")]
    AlreadyInstalled,

    #[error("nothing from shim is installed in this game")]
    NotInstalled,

    #[error("rollback left {} file(s) behind", leftovers.len())]
    RollbackFailed {
        /// `(path, what to do by hand)`.
        leftovers: Vec<(PathBuf, String)>,
    },
}

impl Error {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub fn parse(what: impl Into<String>, source: serde_json::Error) -> Self {
        Self::Parse {
            what: what.into(),
            source,
        }
    }

    pub fn unsupported(feature: impl Into<String>) -> Self {
        Self::UnsupportedPlatform {
            feature: feature.into(),
        }
    }

    /// Stable machine-readable code, used by the front end to pick icons and
    /// recovery hints without string matching.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io { .. } => "io",
            Self::Parse { .. } => "parse",
            Self::NoDataDir => "no_data_dir",
            Self::UnsupportedPlatform { .. } => "unsupported_platform",
            Self::Adapter { .. } => "adapter",
            Self::NoExecutable { .. } => "no_executable",
            Self::NotAnExecutable { .. } => "not_an_executable",
            Self::ManifestInvalid { .. } => "manifest_invalid",
            Self::ManifestSignature { .. } => "manifest_signature",
            Self::Download { .. } => "download",
            Self::HashMismatch { .. } => "hash_mismatch",
            Self::Archive { .. } => "archive",
            Self::ComponentMissing { .. } => "component_missing",
            Self::UserFileMissing { .. } => "user_file_missing",
            Self::ForeignProxyPresent { .. } => "foreign_proxy_present",
            Self::AntiCheatBlocked { .. } => "anti_cheat_blocked",
            Self::GameFolderNotWritable { .. } => "game_folder_not_writable",
            Self::AlreadyInstalled => "already_installed",
            Self::NotInstalled => "not_installed",
            Self::RollbackFailed { .. } => "rollback_failed",
        }
    }

    /// One sentence the user can act on.
    pub fn user_message(&self) -> String {
        match self {
            Self::Io { path, .. } => {
                format!("Could not access {}.", path.display())
            }
            Self::Parse { what, .. } => {
                format!("The {what} file is damaged. Delete it to start fresh.")
            }
            Self::NoDataDir => "No place to store app data was found on this system.".to_string(),
            Self::UnsupportedPlatform { feature } => {
                format!("{feature} only works on Windows.")
            }
            Self::Adapter { adapter, .. } => {
                format!("Scanning {adapter} failed. Other sources were still scanned.")
            }
            Self::NoExecutable { install_dir } => {
                format!("No game executable was found in {}.", install_dir.display())
            }
            Self::NotAnExecutable { path, .. } => {
                format!("{} is not a Windows executable.", path.display())
            }
            Self::ManifestInvalid { .. } => {
                "The component list is damaged. Reinstall shim.".to_string()
            }
            Self::ManifestSignature { .. } => {
                "The downloaded component list is not signed by shim. It was ignored.".to_string()
            }
            Self::Download { url, .. } => {
                format!("Could not download {url}. Check your connection and try again.")
            }
            Self::HashMismatch { what, .. } => {
                format!("{what} does not match the pinned checksum. Nothing was installed.")
            }
            Self::Archive { .. } => "A downloaded archive could not be unpacked.".to_string(),
            Self::ComponentMissing { id, .. } => {
                format!("Component {id} is not ready. Fetch it on the Components screen.")
            }
            Self::UserFileMissing { what, .. } => {
                format!("Choose your {what} on the Components screen first.")
            }
            Self::ForeignProxyPresent { path } => format!(
                "{} already exists and is not ours. Remove it yourself if you want shim to use that slot.",
                path.display()
            ),
            Self::AntiCheatBlocked { which } => {
                format!("{which} protects this game. Installing can get the account banned.")
            }
            Self::GameFolderNotWritable { path, .. } => {
                format!("shim cannot write to {}. Try running it as administrator.", path.display())
            }
            Self::AlreadyInstalled => "shim is already installed in this game.".to_string(),
            Self::NotInstalled => "Nothing from shim is installed in this game.".to_string(),
            Self::RollbackFailed { leftovers } => format!(
                "Undo could not finish. {} file(s) need attention; see the details.",
                leftovers.len()
            ),
        }
    }

    /// Full technical detail for the log drawer.
    pub fn detail(&self) -> String {
        let mut out = self.to_string();
        let mut source = std::error::Error::source(self);
        while let Some(s) = source {
            out.push_str("\n  caused by: ");
            out.push_str(&s.to_string());
            source = s.source();
        }
        out
    }

    pub fn to_dto(&self) -> ErrorDto {
        ErrorDto {
            code: self.code().to_string(),
            message: self.user_message(),
            detail: self.detail(),
        }
    }
}

/// The shape every Tauri command returns on failure.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ErrorDto {
    pub code: String,
    pub message: String,
    pub detail: String,
}

impl From<Error> for ErrorDto {
    fn from(e: Error) -> Self {
        e.to_dto()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_error_has_user_message_and_detail_chain() {
        let inner = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let err = Error::io("C:/Games/x", inner);
        assert_eq!(err.code(), "io");
        assert!(err.user_message().contains("C:/Games/x"));
        assert!(err.detail().contains("caused by: denied"));
    }

    #[test]
    fn unsupported_platform_names_the_feature() {
        let err = Error::unsupported("Library scanning");
        let dto = err.to_dto();
        assert_eq!(dto.code, "unsupported_platform");
        assert_eq!(dto.message, "Library scanning only works on Windows.");
    }
}
