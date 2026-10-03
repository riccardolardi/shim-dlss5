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
