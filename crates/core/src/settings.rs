//! User settings, stored as JSON. Missing fields take defaults so the file
//! survives upgrades; unknown fields are dropped on the next save.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    paths::AppPaths,
    persist::{read_json, read_json_or_quarantine, write_json},
    Error, Result,
};

pub const SETTINGS_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct ScanSources {
    pub steam: bool,
    pub epic: bool,
    pub gog: bool,
    pub xbox: bool,
    pub ubisoft: bool,
    pub ea: bool,
    pub custom_folders: Vec<PathBuf>,
}

impl Default for ScanSources {
    fn default() -> Self {
        Self {
            steam: true,
            epic: true,
            gog: true,
            xbox: true,
            ubisoft: true,
            ea: true,
            custom_folders: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct Settings {
    pub schema: u32,
    pub theme: Theme,
    pub language: String,
    pub check_updates: bool,
    pub scan: ScanSources,
    pub hidden_games: Vec<String>,
    /// The user's own `nvngx_dlssnr.dll`. Never downloaded by us.
    pub model_path: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema: SETTINGS_SCHEMA,
            theme: Theme::System,
            language: "en".to_string(),
            check_updates: true,
            scan: ScanSources::default(),
            hidden_games: Vec::new(),
            model_path: None,
        }
    }
}

impl Settings {
    /// Load from disk. A missing file yields defaults; a damaged file is an error.
    pub fn load(paths: &AppPaths) -> Result<Self> {
        Self::load_from(&paths.settings_file())
    }

    pub fn load_from(file: &Path) -> Result<Self> {
        Ok(read_json(file, "settings")?.unwrap_or_default())
    }

    /// Load for app start: a damaged file is moved aside and defaults are
    /// used, with the error returned so the UI can say so.
    pub fn load_or_reset(paths: &AppPaths) -> (Self, Option<Error>) {
        read_json_or_quarantine(&paths.settings_file(), "settings")
    }

    pub fn save(&self, paths: &AppPaths) -> Result<()> {
        self.save_to(&paths.settings_file())
    }

    pub fn save_to(&self, file: &Path) -> Result<()> {
        write_json(file, self, "settings")
    }

    /// Normalise a value that came from outside (the front end): pin the
    /// schema, drop relative custom folders, drop a model path that does not
    /// point at a file, and dedupe hidden ids.
    pub fn validated(&self) -> Self {
        let mut hidden = self.hidden_games.clone();
        hidden.sort();
        hidden.dedup();
        Self {
            schema: SETTINGS_SCHEMA,
            scan: ScanSources {
                custom_folders: self
                    .scan
                    .custom_folders
                    .iter()
                    .filter(|p| p.is_absolute())
                    .cloned()
                    .collect(),
                ..self.scan.clone()
            },
            hidden_games: hidden,
            model_path: self.model_path.clone().filter(|p| p.is_file()),
            ..self.clone()
        }
    }

    /// Return a copy with one field changed. Settings are values, not state.
    pub fn with_theme(&self, theme: Theme) -> Self {
        Self {
            theme,
            ..self.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let s = Settings::load_from(&tmp.path().join("settings.json")).unwrap();
        assert_eq!(s, Settings::default());
        assert!(s.scan.steam);
        assert_eq!(s.language, "en");
    }

    #[test]
    fn round_trips_through_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("deep").join("settings.json");
        let s = Settings::default().with_theme(Theme::Dark);
        s.save_to(&file).unwrap();
        assert_eq!(Settings::load_from(&file).unwrap(), s);
    }

    #[test]
    fn old_file_with_missing_fields_still_loads() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("settings.json");
        std::fs::write(&file, br#"{"schema":1,"theme":"light"}"#).unwrap();
        let s = Settings::load_from(&file).unwrap();
        assert_eq!(s.theme, Theme::Light);
        assert!(s.check_updates);
    }

    #[test]
    fn damaged_file_is_an_error_not_a_reset() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("settings.json");
        std::fs::write(&file, b"{not json").unwrap();
        let err = Settings::load_from(&file).unwrap_err();
        assert_eq!(err.code(), "parse");
    }

    #[test]
    fn load_or_reset_quarantines_and_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(tmp.path());
        std::fs::write(paths.settings_file(), b"{not json").unwrap();
        let (s, err) = Settings::load_or_reset(&paths);
        assert_eq!(s, Settings::default());
        assert!(err.is_some());
        assert!(tmp.path().join("settings.json.bad").exists());
    }

    #[test]
    fn validated_pins_schema_and_drops_bad_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("nvngx_dlssnr.dll");
        std::fs::write(&real, b"x").unwrap();
        let s = Settings {
            schema: 99,
            scan: ScanSources {
                custom_folders: vec![PathBuf::from("relative"), tmp.path().to_path_buf()],
                ..ScanSources::default()
            },
            hidden_games: vec!["b".into(), "a".into(), "b".into()],
            model_path: Some(real.clone()),
            ..Settings::default()
        };
        let v = s.validated();
        assert_eq!(v.schema, SETTINGS_SCHEMA);
        assert_eq!(v.scan.custom_folders, vec![tmp.path().to_path_buf()]);
        assert_eq!(v.hidden_games, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(v.model_path, Some(real));

        let gone = Settings {
            model_path: Some(tmp.path().join("missing.dll")),
            ..s
        };
        assert_eq!(gone.validated().model_path, None);
    }

    #[test]
    fn with_theme_does_not_mutate_original() {
        let a = Settings::default();
        let b = a.with_theme(Theme::Dark);
        assert_eq!(a.theme, Theme::System);
        assert_eq!(b.theme, Theme::Dark);
    }
}
