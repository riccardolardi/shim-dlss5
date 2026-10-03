//! Ubisoft Connect: one key per installed game under
//! `HKLM\SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs\<id>` holding
//! `InstallDir`. The display name lives in the matching Windows uninstall entry
//! `Uplay Install <id>`; the folder name is the fallback.

use std::path::{Path, PathBuf};

use crate::{
    discovery::LauncherAdapter,
    model::{DiscoveredGame, Launcher},
    platform::{Hive, Platform, Registry},
    Result,
};

pub struct Ubisoft;

const INSTALLS_KEY: &str = r"SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs";
const UNINSTALL_KEY: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";

impl LauncherAdapter for Ubisoft {
    fn launcher(&self) -> Launcher {
        Launcher::Ubisoft
    }

    fn discover(&self, platform: &Platform) -> Result<Vec<DiscoveredGame>> {
        Ok(discover_from(platform.registry.as_ref()))
    }
}

pub fn discover_from(registry: &dyn Registry) -> Vec<DiscoveredGame> {
    let mut ids = registry.subkeys(Hive::LocalMachine, INSTALLS_KEY);
    ids.sort();
    ids.iter()
        .filter_map(|id| {
            let dir = registry
                .read_string(
                    Hive::LocalMachine,
                    &format!(r"{INSTALLS_KEY}\{id}"),
                    "InstallDir",
                )
                .filter(|s| !s.trim().is_empty())?;
            let install_dir = PathBuf::from(dir.replace('/', "\\").trim_end_matches('\\'));
            let title = registry
                .read_string(
                    Hive::LocalMachine,
                    &format!(r"{UNINSTALL_KEY}\Uplay Install {id}"),
                    "DisplayName",
                )
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| folder_name(&install_dir));
            Some(DiscoveredGame {
                launcher: Launcher::Ubisoft,
                title,
                install_dir,
                declared_exe: None,
                launcher_id: Some(id.clone()),
            })
        })
        .collect()
}

pub(crate) fn folder_name(dir: &Path) -> String {
    crate::discovery::last_segment(&dir.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::testing::FakeRegistry;

    #[test]
    fn reads_install_dir_and_display_name_with_folder_fallback() {
        let mut reg = FakeRegistry::default();
        reg.add(
            Hive::LocalMachine,
            &format!(r"{INSTALLS_KEY}\5059"),
            &[("InstallDir", "D:/Ubisoft/Assassin's Creed Valhalla/")],
        );
        reg.add(
            Hive::LocalMachine,
            &format!(r"{UNINSTALL_KEY}\Uplay Install 5059"),
            &[("DisplayName", "Assassin's Creed Valhalla")],
        );
        reg.add(
            Hive::LocalMachine,
            &format!(r"{INSTALLS_KEY}\720"),
            &[("InstallDir", r"D:\Ubisoft\Far Cry 6")],
        );
        reg.add(
            Hive::LocalMachine,
            &format!(r"{INSTALLS_KEY}\999"),
            &[("InstallDir", "   ")],
        );
        let games = discover_from(&reg);
        assert_eq!(games.len(), 2);
        assert_eq!(games[0].title, "Assassin's Creed Valhalla");
        assert_eq!(
            games[0].install_dir,
            PathBuf::from(r"D:\Ubisoft\Assassin's Creed Valhalla")
        );
        assert_eq!(games[0].launcher_id.as_deref(), Some("5059"));
        assert_eq!(games[1].title, "Far Cry 6");
    }

    #[test]
    fn no_key_means_no_games() {
        assert!(Ubisoft
            .discover(&Platform::unavailable())
            .unwrap()
            .is_empty());
    }
}
