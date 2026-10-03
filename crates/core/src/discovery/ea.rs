//! EA app: there is no single install list. Games register two ways, and we
//! read both, deduping by folder:
//!
//! 1. `HKLM\SOFTWARE\WOW6432Node\{EA Games, Electronic Arts, EA Sports}\<Name>`
//!    with an `Install Dir` value (the subkey name is the title).
//! 2. Windows uninstall entries (both registry views) whose `Publisher` is
//!    Electronic Arts and which have an `InstallLocation`.
//!
//! The EA app itself and Origin are skipped by name.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::{
    discovery::LauncherAdapter,
    library::game_id,
    model::{DiscoveredGame, Launcher},
    platform::{Hive, Platform, Registry},
    Result,
};

pub struct Ea;

const VENDOR_KEYS: &[&str] = &[
    r"SOFTWARE\WOW6432Node\EA Games",
    r"SOFTWARE\WOW6432Node\Electronic Arts",
    r"SOFTWARE\WOW6432Node\EA Sports",
    r"SOFTWARE\EA Games",
    r"SOFTWARE\Electronic Arts",
];

const UNINSTALL_KEYS: &[&str] = &[
    r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
    r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
];

const NOT_GAMES: &[&str] = &[
    "ea app",
    "ea desktop",
    "origin",
    "ea anticheat",
    "eaanticheat",
];

impl LauncherAdapter for Ea {
    fn launcher(&self) -> Launcher {
        Launcher::Ea
    }

    fn discover(&self, platform: &Platform) -> Result<Vec<DiscoveredGame>> {
        Ok(discover_from(platform.registry.as_ref()))
    }
}

pub fn discover_from(registry: &dyn Registry) -> Vec<DiscoveredGame> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let mut push = |title: String, dir: String, id: Option<String>| {
        let install_dir = PathBuf::from(dir.replace('/', "\\").trim_end_matches('\\'));
        if is_not_a_game(&title) || !seen.insert(game_id(&install_dir)) {
            return;
        }
        out.push(DiscoveredGame {
            launcher: Launcher::Ea,
            title,
            install_dir,
            declared_exe: None,
            launcher_id: id,
        });
    };

    for key in VENDOR_KEYS {
        let mut names = registry.subkeys(Hive::LocalMachine, key);
        names.sort();
        for name in names {
            let sub = format!(r"{key}\{name}");
            if let Some(dir) = registry
                .read_string(Hive::LocalMachine, &sub, "Install Dir")
                .filter(|s| !s.trim().is_empty())
            {
                push(name.clone(), dir, None);
            }
        }
    }

    for key in UNINSTALL_KEYS {
        let mut entries = registry.subkeys(Hive::LocalMachine, key);
        entries.sort();
        for entry in entries {
            let sub = format!(r"{key}\{entry}");
            let publisher = registry
                .read_string(Hive::LocalMachine, &sub, "Publisher")
                .unwrap_or_default()
                .to_lowercase();
            if !publisher.contains("electronic arts") {
                continue;
            }
            let (Some(title), Some(dir)) = (
                registry.read_string(Hive::LocalMachine, &sub, "DisplayName"),
                registry
                    .read_string(Hive::LocalMachine, &sub, "InstallLocation")
                    .filter(|s| !s.trim().is_empty()),
            ) else {
                continue;
            };
            push(title, dir, Some(entry.clone()));
        }
    }
    out
}

fn is_not_a_game(title: &str) -> bool {
    let lower = title.to_lowercase();
    NOT_GAMES
        .iter()
        .any(|n| lower == *n || lower.starts_with(&format!("{n} ")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::testing::FakeRegistry;

    #[test]
    fn vendor_keys_and_uninstall_entries_are_merged_and_deduped() {
        let mut reg = FakeRegistry::default();
        reg.add(
            Hive::LocalMachine,
            r"SOFTWARE\WOW6432Node\EA Games\Battlefield V",
            &[("Install Dir", r"D:\EA Games\Battlefield V\")],
        );
        reg.add(
            Hive::LocalMachine,
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\{BF-GUID}",
            &[
                ("DisplayName", "Battlefield™ V"),
                ("Publisher", "Electronic Arts"),
                ("InstallLocation", r"D:\EA Games\Battlefield V"),
            ],
        );
        reg.add(
            Hive::LocalMachine,
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\{JEDI}",
            &[
                ("DisplayName", "STAR WARS Jedi: Survivor"),
                ("Publisher", "Electronic Arts, Inc."),
                ("InstallLocation", r"D:\EA Games\Jedi Survivor"),
            ],
        );
        reg.add(
            Hive::LocalMachine,
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\{EAAPP}",
            &[
                ("DisplayName", "EA app"),
                ("Publisher", "Electronic Arts"),
                (
                    "InstallLocation",
                    r"C:\Program Files\Electronic Arts\EA Desktop",
                ),
            ],
        );
        reg.add(
            Hive::LocalMachine,
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\{OTHER}",
            &[
                ("DisplayName", "Some Tool"),
                ("Publisher", "Someone Else"),
                ("InstallLocation", r"C:\Tools"),
            ],
        );
        let games = discover_from(&reg);
        let titles: Vec<&str> = games.iter().map(|g| g.title.as_str()).collect();
        assert_eq!(titles, vec!["Battlefield V", "STAR WARS Jedi: Survivor"]);
        assert_eq!(
            games[0].install_dir,
            PathBuf::from(r"D:\EA Games\Battlefield V")
        );
        assert_eq!(games[1].launcher_id.as_deref(), Some("{JEDI}"));
    }

    #[test]
    fn nothing_registered_means_no_games() {
        assert!(Ea.discover(&Platform::unavailable()).unwrap().is_empty());
    }
}
