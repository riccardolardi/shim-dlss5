//! GOG Galaxy: one key per installed product under
//! `HKLM\SOFTWARE\WOW6432Node\GOG.com\Games\<productId>`.
//!
//! Values used: `gameName`, `path`, `exe` (full path), `dependsOn` (set on
//! DLC, which we skip). Everything comes through the `Registry` trait, so the
//! adapter is tested with a fake registry on any OS.

use std::path::PathBuf;

use crate::{
    discovery::LauncherAdapter,
    model::{DiscoveredGame, Launcher},
    platform::{Hive, Platform, Registry},
    Result,
};

pub struct Gog;

const GAMES_KEY: &str = r"SOFTWARE\WOW6432Node\GOG.com\Games";

impl LauncherAdapter for Gog {
    fn launcher(&self) -> Launcher {
        Launcher::Gog
    }

    fn discover(&self, platform: &Platform) -> Result<Vec<DiscoveredGame>> {
        Ok(discover_from(platform.registry.as_ref()))
    }
}

pub fn discover_from(registry: &dyn Registry) -> Vec<DiscoveredGame> {
    let mut ids = registry.subkeys(Hive::LocalMachine, GAMES_KEY);
    ids.sort();
    ids.iter()
        .filter_map(|id| read_product(registry, id))
        .collect()
}

fn read_product(registry: &dyn Registry, id: &str) -> Option<DiscoveredGame> {
    let key = format!(r"{GAMES_KEY}\{id}");
    let read = |value: &str| {
        registry
            .read_string(Hive::LocalMachine, &key, value)
            .filter(|s| !s.trim().is_empty())
    };
    if read("dependsOn").is_some() {
        return None;
    }
    let title = read("gameName")?;
    let install_dir = PathBuf::from(read("path")?);
    let declared_exe = read("exe")
        .filter(|s| crate::discovery::is_windows_absolute(s))
        .map(PathBuf::from);
    Some(DiscoveredGame {
        launcher: Launcher::Gog,
        title,
        install_dir,
        declared_exe,
        launcher_id: Some(id.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::testing::FakeRegistry;

    #[test]
    fn games_are_read_and_dlc_is_skipped() {
        let mut reg = FakeRegistry::default();
        reg.add(
            Hive::LocalMachine,
            &format!(r"{GAMES_KEY}\1207658924"),
            &[
                ("gameName", "The Witcher 3: Wild Hunt"),
                ("path", r"D:\GOG Games\The Witcher 3"),
                ("exe", r"D:\GOG Games\The Witcher 3\bin\x64\witcher3.exe"),
            ],
        );
        reg.add(
            Hive::LocalMachine,
            &format!(r"{GAMES_KEY}\1640424747"),
            &[
                ("gameName", "The Witcher 3: Hearts of Stone"),
                ("path", r"D:\GOG Games\The Witcher 3"),
                ("dependsOn", "1207658924"),
            ],
        );
        reg.add(
            Hive::LocalMachine,
            &format!(r"{GAMES_KEY}\broken"),
            &[("gameName", "No path")],
        );
        let games = discover_from(&reg);
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].title, "The Witcher 3: Wild Hunt");
        assert_eq!(games[0].launcher_id.as_deref(), Some("1207658924"));
        assert_eq!(
            games[0].declared_exe,
            Some(PathBuf::from(
                r"D:\GOG Games\The Witcher 3\bin\x64\witcher3.exe"
            ))
        );
    }

    #[test]
    fn no_gog_key_means_no_games() {
        assert!(Gog.discover(&Platform::unavailable()).unwrap().is_empty());
    }
}
