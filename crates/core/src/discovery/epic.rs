//! Epic Games Launcher: one `*.item` JSON per installed app in
//! `%ProgramData%\Epic\EpicGamesLauncher\Data\Manifests`.
//!
//! DLC and add-ons share the base game's `InstallLocation`; they are skipped
//! by category and by `MainGameAppName != AppName`, and `discover_all` would
//! dedupe the folder anyway.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{
    discovery::LauncherAdapter,
    model::{DiscoveredGame, Launcher},
    platform::Platform,
    Result,
};

pub struct Epic {
    manifests_dir: PathBuf,
}

impl Epic {
    pub fn new(manifests_dir: PathBuf) -> Self {
        Self { manifests_dir }
    }

    /// The standard location under `%ProgramData%`.
    pub fn default_location() -> Self {
        let program_data = std::env::var_os("ProgramData")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
        Self::new(program_data.join(r"Epic\EpicGamesLauncher\Data\Manifests"))
    }
}

impl LauncherAdapter for Epic {
    fn launcher(&self) -> Launcher {
        Launcher::Epic
    }

    fn discover(&self, _: &Platform) -> Result<Vec<DiscoveredGame>> {
        Ok(discover_from(&self.manifests_dir))
    }
}

/// An absent manifests folder means Epic is not installed: no games, no error.
pub fn discover_from(manifests_dir: &Path) -> Vec<DiscoveredGame> {
    let Ok(read) = std::fs::read_dir(manifests_dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = read
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("item"))
        })
        .collect();
    files.sort();

    files
        .iter()
        .filter_map(|file| match std::fs::read_to_string(file) {
            Ok(text) => parse_item(&text).unwrap_or_else(|e| {
                tracing::warn!(file = %file.display(), %e, "unreadable Epic manifest");
                None
            }),
            Err(e) => {
                tracing::warn!(file = %file.display(), %e, "unreadable Epic manifest");
                None
            }
        })
        .collect()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Item {
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    install_location: String,
    #[serde(default)]
    launch_executable: String,
    #[serde(default)]
    app_name: String,
    #[serde(default)]
    main_game_app_name: String,
    #[serde(default)]
    app_categories: Vec<String>,
    #[serde(rename = "bIsIncompleteInstall", default)]
    is_incomplete_install: bool,
    #[serde(rename = "bIsApplication", default = "default_true")]
    is_application: bool,
}

fn default_true() -> bool {
    true
}

/// One `.item` file. `None` for DLC, add-ons and unfinished installs.
pub fn parse_item(text: &str) -> std::result::Result<Option<DiscoveredGame>, serde_json::Error> {
    let item: Item = serde_json::from_str(text)?;
    let is_addon = item
        .app_categories
        .iter()
        .any(|c| c.eq_ignore_ascii_case("addons") || c.eq_ignore_ascii_case("dlc"));
    let is_dlc = !item.main_game_app_name.is_empty() && item.main_game_app_name != item.app_name;
    if item.is_incomplete_install
        || !item.is_application
        || is_addon
        || is_dlc
        || item.install_location.is_empty()
        || item.display_name.is_empty()
    {
        return Ok(None);
    }
    let install_dir = PathBuf::from(&item.install_location);
    let declared_exe = (!item.launch_executable.is_empty())
        .then(|| install_dir.join(item.launch_executable.replace('/', "\\")));
    Ok(Some(DiscoveredGame {
        launcher: Launcher::Epic,
        title: item.display_name,
        install_dir,
        declared_exe,
        launcher_id: (!item.app_name.is_empty()).then_some(item.app_name),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixtures() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/epic")
    }

    #[test]
    fn installed_game_has_title_dir_exe_and_app_name() {
        let text = std::fs::read_to_string(fixtures().join("installed.item")).unwrap();
        let game = parse_item(&text).unwrap().unwrap();
        assert_eq!(game.title, "Control");
        assert_eq!(game.install_dir, PathBuf::from(r"D:\Epic Games\Control"));
        assert_eq!(
            game.declared_exe,
            Some(PathBuf::from(
                r"D:\Epic Games\Control\Binaries\Win64\Control_DX12.exe"
            ))
        );
        assert_eq!(game.launcher_id.as_deref(), Some("Calluna"));
    }

    #[test]
    fn dlc_and_incomplete_installs_are_skipped() {
        for name in ["dlc.item", "incomplete.item"] {
            let text = std::fs::read_to_string(fixtures().join(name)).unwrap();
            assert!(parse_item(&text).unwrap().is_none(), "{name}");
        }
    }

    #[test]
    fn discover_from_reads_the_folder_and_survives_junk() {
        let tmp = tempfile::tempdir().unwrap();
        for name in ["installed.item", "dlc.item", "incomplete.item"] {
            std::fs::copy(fixtures().join(name), tmp.path().join(name)).unwrap();
        }
        std::fs::write(tmp.path().join("broken.item"), "{nope").unwrap();
        std::fs::write(tmp.path().join("notes.txt"), "{}").unwrap();
        let games = discover_from(tmp.path());
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].title, "Control");
    }

    #[test]
    fn missing_folder_means_no_games() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(discover_from(&tmp.path().join("nope")).is_empty());
        let epic = Epic::new(tmp.path().join("nope"));
        assert!(epic.discover(&Platform::unavailable()).unwrap().is_empty());
    }
}
