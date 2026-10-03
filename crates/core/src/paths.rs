//! Where shim keeps its data.
//!
//! On Windows this is `%LOCALAPPDATA%\shim`. Tests and power users can point
//! it elsewhere with the `SHIM_DATA_DIR` environment variable.

use std::path::{Path, PathBuf};

use crate::{Error, Result};

pub const DATA_DIR_ENV: &str = "SHIM_DATA_DIR";
pub const APP_DIR_NAME: &str = "shim";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    root: PathBuf,
}

impl AppPaths {
    /// Resolve the data root from the environment, falling back to the OS
    /// local-data directory.
    pub fn discover() -> Result<Self> {
        if let Some(dir) = std::env::var_os(DATA_DIR_ENV) {
            return Ok(Self::at(dir));
        }
        let base = directories::BaseDirs::new().ok_or(Error::NoDataDir)?;
        Ok(Self::at(base.data_local_dir().join(APP_DIR_NAME)))
    }

    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    pub fn library_file(&self) -> PathBuf {
        self.root.join("library.json")
    }

    pub fn installs_dir(&self) -> PathBuf {
        self.root.join("installs")
    }

    pub fn install_manifest(&self, game_id: &str) -> PathBuf {
        self.installs_dir().join(format!("{game_id}.json"))
    }

    pub fn backups_dir(&self, game_id: &str) -> PathBuf {
        self.root.join("backups").join(game_id)
    }

    pub fn components_dir(&self) -> PathBuf {
        self.root.join("components")
    }

    pub fn covers_dir(&self) -> PathBuf {
        self.root.join("cache").join("covers")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    /// Create every directory the app needs. Safe to call repeatedly.
    pub fn ensure(&self) -> Result<()> {
        for dir in [
            self.root.clone(),
            self.installs_dir(),
            self.components_dir(),
            self.covers_dir(),
            self.logs_dir(),
        ] {
            std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_override_wins() {
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var(DATA_DIR_ENV, tmp.path());
        let paths = AppPaths::discover().unwrap();
        std::env::remove_var(DATA_DIR_ENV);
        assert_eq!(paths.root(), tmp.path());
    }

    #[test]
    fn derived_paths_hang_off_root() {
        let p = AppPaths::at("/data/shim");
        assert_eq!(p.settings_file(), PathBuf::from("/data/shim/settings.json"));
        assert_eq!(
            p.install_manifest("abc"),
            PathBuf::from("/data/shim/installs/abc.json")
        );
        assert_eq!(
            p.backups_dir("abc"),
            PathBuf::from("/data/shim/backups/abc")
        );
    }

    #[test]
    fn ensure_creates_every_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let p = AppPaths::at(tmp.path().join("nested"));
        p.ensure().unwrap();
        assert!(p.installs_dir().is_dir());
        assert!(p.components_dir().is_dir());
        assert!(p.covers_dir().is_dir());
        assert!(p.logs_dir().is_dir());
        p.ensure().unwrap();
    }
}
