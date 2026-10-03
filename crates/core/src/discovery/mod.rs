//! Launcher discovery.
//!
//! Each launcher is an adapter. A scan runs every enabled adapter, keeps the
//! games it finds, and reports each adapter's failure separately instead of
//! aborting the whole scan.

pub mod custom;
pub mod ea;
pub mod epic;
pub mod gog;
pub mod steam;
pub mod ubisoft;
pub mod vdf;
pub mod xbox;

use serde::Serialize;
use ts_rs::TS;

use crate::{
    error::ErrorDto,
    model::{DiscoveredGame, Launcher},
    platform::Platform,
    settings::ScanSources,
    Result,
};

pub trait LauncherAdapter: Send + Sync {
    fn launcher(&self) -> Launcher;
    fn discover(&self, platform: &Platform) -> Result<Vec<DiscoveredGame>>;
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct AdapterFailure {
    pub launcher: Launcher,
    pub error: ErrorDto,
}

#[derive(Debug, Clone, Default, Serialize, TS)]
#[ts(export)]
pub struct DiscoveryOutcome {
    pub games: Vec<DiscoveredGame>,
    pub failures: Vec<AdapterFailure>,
    /// Launchers that were enabled but have no adapter on this platform.
    pub unavailable: Vec<Launcher>,
}

/// Run every adapter that the settings enable.
pub fn discover_all(
    adapters: &[Box<dyn LauncherAdapter>],
    sources: &ScanSources,
    platform: &Platform,
) -> DiscoveryOutcome {
    let enabled = enabled_launchers(sources);
    let mut outcome = DiscoveryOutcome::default();

    for launcher in &enabled {
        match adapters.iter().find(|a| a.launcher() == *launcher) {
            None => outcome.unavailable.push(*launcher),
            Some(adapter) => match adapter.discover(platform) {
                Ok(found) => outcome.games.extend(found),
                Err(e) => outcome.failures.push(AdapterFailure {
                    launcher: *launcher,
                    error: e.to_dto(),
                }),
            },
        }
    }

    outcome.games = dedupe_by_install_dir(outcome.games);
    outcome
}

fn enabled_launchers(s: &ScanSources) -> Vec<Launcher> {
    let flags = [
        (s.steam, Launcher::Steam),
        (s.epic, Launcher::Epic),
        (s.gog, Launcher::Gog),
        (s.xbox, Launcher::Xbox),
        (s.ubisoft, Launcher::Ubisoft),
        (s.ea, Launcher::Ea),
        (!s.custom_folders.is_empty(), Launcher::Custom),
    ];
    flags
        .into_iter()
        .filter(|(on, _)| *on)
        .map(|(_, l)| l)
        .collect()
}

/// `C:\...`, `C:/...` or `\\server\...`. `Path::is_absolute` says no to these
/// on macOS/Linux, where the core is also tested.
pub(crate) fn is_windows_absolute(s: &str) -> bool {
    let b = s.as_bytes();
    (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/'))
        || s.starts_with("\\\\")
}

/// Last path segment, splitting on either separator, so a Windows path read
/// from the registry gives the same answer on every OS.
pub(crate) fn last_segment(path: &str) -> String {
    path.trim_end_matches(['\\', '/'])
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(path)
        .to_string()
}

/// `base\rel` with backslashes, for paths that are Windows paths by nature
/// (they came from a launcher's own records) whatever the host OS.
pub(crate) fn win_join(base: &str, rel: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(format!(
        "{}\\{}",
        base.trim_end_matches(['\\', '/']),
        rel.replace('/', "\\").trim_start_matches('\\')
    ))
}

/// First adapter wins when two launchers report the same folder.
fn dedupe_by_install_dir(games: Vec<DiscoveredGame>) -> Vec<DiscoveredGame> {
    let mut seen = std::collections::HashSet::new();
    games
        .into_iter()
        .filter(|g| seen.insert(crate::library::game_id(&g.install_dir)))
        .collect()
}

/// The adapters available on this build. They read the registry only through
/// the `Registry` trait, so the same list serves every OS; off Windows the
/// registry is empty and each adapter reports no games.
pub fn default_adapters(sources: &ScanSources) -> Vec<Box<dyn LauncherAdapter>> {
    vec![
        Box::new(steam::Steam),
        Box::new(epic::Epic::default_location()),
        Box::new(gog::Gog),
        Box::new(xbox::Xbox::default_location()),
        Box::new(ubisoft::Ubisoft),
        Box::new(ea::Ea),
        Box::new(custom::Custom::new(sources.custom_folders.clone())),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;
    use std::path::PathBuf;

    struct Fake {
        launcher: Launcher,
        result: std::result::Result<Vec<DiscoveredGame>, String>,
    }

    impl LauncherAdapter for Fake {
        fn launcher(&self) -> Launcher {
            self.launcher
        }
        fn discover(&self, _: &Platform) -> Result<Vec<DiscoveredGame>> {
            match &self.result {
                Ok(g) => Ok(g.clone()),
                Err(detail) => Err(Error::Adapter {
                    adapter: self.launcher.label().to_string(),
                    detail: detail.clone(),
                }),
            }
        }
    }

    fn game(launcher: Launcher, dir: &str) -> DiscoveredGame {
        DiscoveredGame {
            launcher,
            title: dir.to_string(),
            install_dir: PathBuf::from(dir),
            declared_exe: None,
            launcher_id: None,
        }
    }

    #[test]
    fn windows_path_helpers_work_on_every_host() {
        assert!(is_windows_absolute(r"D:\Games\X"));
        assert!(is_windows_absolute("c:/games/x"));
        assert!(is_windows_absolute(r"\\nas\games"));
        assert!(!is_windows_absolute("relative/x.exe"));
        assert_eq!(last_segment(r"D:\Ubisoft\Far Cry 6\"), "Far Cry 6");
        assert_eq!(last_segment("D:/Ubisoft/Far Cry 6"), "Far Cry 6");
        assert_eq!(
            win_join(r"D:\Epic Games\Control\", "Binaries/Win64/Control_DX12.exe"),
            PathBuf::from(r"D:\Epic Games\Control\Binaries\Win64\Control_DX12.exe")
        );
    }

    #[test]
    fn one_failing_adapter_does_not_stop_the_others() {
        let adapters: Vec<Box<dyn LauncherAdapter>> = vec![
            Box::new(Fake {
                launcher: Launcher::Steam,
                result: Err("boom".into()),
            }),
            Box::new(Fake {
                launcher: Launcher::Gog,
                result: Ok(vec![game(Launcher::Gog, "C:/g")]),
            }),
        ];
        let out = discover_all(&adapters, &ScanSources::default(), &Platform::unavailable());
        assert_eq!(out.games.len(), 1);
        assert_eq!(out.failures.len(), 1);
        assert_eq!(out.failures[0].launcher, Launcher::Steam);
        assert_eq!(out.failures[0].error.code, "adapter");
    }

    #[test]
    fn disabled_sources_are_skipped_and_missing_adapters_reported() {
        let adapters: Vec<Box<dyn LauncherAdapter>> = vec![Box::new(Fake {
            launcher: Launcher::Steam,
            result: Ok(vec![]),
        })];
        let sources = ScanSources {
            epic: false,
            ..ScanSources::default()
        };
        let out = discover_all(&adapters, &sources, &Platform::unavailable());
        assert!(out.failures.is_empty());
        assert!(!out.unavailable.contains(&Launcher::Epic));
        assert!(out.unavailable.contains(&Launcher::Gog));
        assert!(!out.unavailable.contains(&Launcher::Custom));
    }

    #[test]
    fn same_folder_from_two_launchers_is_kept_once() {
        let adapters: Vec<Box<dyn LauncherAdapter>> = vec![
            Box::new(Fake {
                launcher: Launcher::Steam,
                result: Ok(vec![game(Launcher::Steam, "C:/Games/X")]),
            }),
            Box::new(Fake {
                launcher: Launcher::Gog,
                result: Ok(vec![game(Launcher::Gog, "c:/games/x/")]),
            }),
        ];
        let out = discover_all(&adapters, &ScanSources::default(), &Platform::unavailable());
        assert_eq!(out.games.len(), 1);
        assert_eq!(out.games[0].launcher, Launcher::Steam);
    }
}
