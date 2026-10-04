//! The persisted game library, the stable game id, and scan merging.

use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::{
    discovery::DiscoveryOutcome,
    model::{Game, GameStatus, Launcher},
    paths::AppPaths,
    persist::{read_json, read_json_or_quarantine, write_json},
    Error, Result,
};

pub const LIBRARY_SCHEMA: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct Library {
    pub schema: u32,
    /// Unix seconds of the last completed scan, if any.
    #[ts(type = "number | null")]
    pub scanned_at: Option<u64>,
    pub games: Vec<Game>,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            schema: LIBRARY_SCHEMA,
            scanned_at: None,
            games: Vec::new(),
        }
    }
}

impl Library {
    pub fn load(paths: &AppPaths) -> Result<Self> {
        Self::load_from(&paths.library_file())
    }

    pub fn load_from(file: &Path) -> Result<Self> {
        Ok(read_json(file, "library")?.unwrap_or_default())
    }

    /// For app start: the library is a cache, so a damaged file is moved
    /// aside and an empty library is used.
    pub fn load_or_reset(paths: &AppPaths) -> (Self, Option<Error>) {
        read_json_or_quarantine(&paths.library_file(), "library")
    }

    pub fn save(&self, paths: &AppPaths) -> Result<()> {
        self.save_to(&paths.library_file())
    }

    pub fn save_to(&self, file: &Path) -> Result<()> {
        write_json(file, self, "library")
    }

    pub fn with_games(&self, games: Vec<Game>, scanned_at: u64) -> Self {
        Self {
            schema: self.schema,
            scanned_at: Some(scanned_at),
            games,
        }
    }

    /// Fold a scan into this library.
    ///
    /// Games from launchers that scanned cleanly are replaced by what the scan
    /// found, keeping analysis, cover and status for ids we already knew.
    /// Games from launchers that failed or had no adapter are carried over
    /// untouched, so one broken adapter never empties the library.
    pub fn merge_scan(
        &self,
        outcome: &DiscoveryOutcome,
        hidden_games: &[String],
        scanned_at: u64,
    ) -> Self {
        let untrusted: HashSet<Launcher> = outcome
            .failures
            .iter()
            .map(|f| f.launcher)
            .chain(outcome.unavailable.iter().copied())
            .collect();

        let carried = self
            .games
            .iter()
            .filter(|g| untrusted.contains(&g.launcher))
            .cloned();

        let fresh = outcome.games.iter().map(|d| {
            let id = game_id(&d.install_dir);
            let known = self.games.iter().find(|g| g.id == id);
            Game {
                hidden: hidden_games.contains(&id),
                mode: known.and_then(|g| g.mode),
                neural: known.and_then(|g| g.neural),
                id,
                launcher: d.launcher,
                title: d.title.clone(),
                install_dir: d.install_dir.clone(),
                launcher_id: d.launcher_id.clone(),
                analysis: known.and_then(|g| g.analysis.clone()),
                status: known
                    .map(|g| g.status.clone())
                    .unwrap_or(GameStatus::Pending),
                cover: known.and_then(|g| g.cover.clone()),
            }
        });

        self.with_games(carried.chain(fresh).collect(), scanned_at)
    }

    /// Re-derive the `hidden` flag from a settings list.
    pub fn with_hidden(&self, hidden_games: &[String]) -> Self {
        let games = self
            .games
            .iter()
            .map(|g| Game {
                hidden: hidden_games.contains(&g.id),
                ..g.clone()
            })
            .collect();
        Self {
            games,
            ..self.clone()
        }
    }
}

/// Stable identity for a game: a hash of its normalised install path.
///
/// Normalisation lowercases, converts `\` to `/`, and strips a trailing slash,
/// so the same folder gives the same id however it was discovered. Lowercasing
/// matches Windows' case-insensitive file system, which is the only target.
pub fn game_id(install_dir: &Path) -> String {
    let raw = install_dir.to_string_lossy().replace('\\', "/");
    let trimmed = raw.trim_end_matches('/').to_lowercase();
    hex(&Sha256::digest(trimmed.as_bytes()))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::AdapterFailure;
    use crate::model::{DiscoveredGame, Route};
    use std::path::PathBuf;

    #[test]
    fn sha256_hex_matches_known_vector() {
        assert_eq!(
            hex(&Sha256::digest(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn game_id_is_stable_across_case_slashes_and_trailing_separator() {
        let a = game_id(&PathBuf::from(
            r"D:\SteamLibrary\steamapps\common\Cyberpunk 2077",
        ));
        let b = game_id(&PathBuf::from(
            "d:/steamlibrary/steamapps/common/cyberpunk 2077/",
        ));
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn different_folders_get_different_ids() {
        assert_ne!(
            game_id(Path::new("C:/Games/A")),
            game_id(Path::new("C:/Games/B"))
        );
    }

    #[test]
    fn library_round_trips_and_missing_file_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("library.json");
        let empty = Library::load_from(&file).unwrap();
        assert!(empty.games.is_empty());
        let lib = empty.with_games(Vec::new(), 1_700_000_000);
        lib.save_to(&file).unwrap();
        assert_eq!(Library::load_from(&file).unwrap(), lib);
    }

    fn discovered(launcher: Launcher, dir: &str) -> DiscoveredGame {
        DiscoveredGame {
            launcher,
            title: dir.to_string(),
            install_dir: PathBuf::from(dir),
            declared_exe: None,
            launcher_id: None,
        }
    }

    fn known(launcher: Launcher, dir: &str, status: GameStatus) -> Game {
        Game {
            id: game_id(Path::new(dir)),
            launcher,
            title: dir.to_string(),
            install_dir: PathBuf::from(dir),
            launcher_id: None,
            analysis: None,
            status,
            cover: Some(PathBuf::from("cover.jpg")),
            hidden: false,
            mode: None,
            neural: None,
        }
    }

    #[test]
    fn merge_keeps_known_state_and_adds_new_games() {
        let installed = GameStatus::Installed {
            route: Route::OptiScaler,
        };
        let lib = Library::default()
            .with_games(vec![known(Launcher::Steam, "C:/g/a", installed.clone())], 1);
        let outcome = DiscoveryOutcome {
            games: vec![
                discovered(Launcher::Steam, "C:/g/a"),
                discovered(Launcher::Steam, "C:/g/b"),
            ],
            ..Default::default()
        };
        let merged = lib.merge_scan(&outcome, &[], 2);
        assert_eq!(merged.games.len(), 2);
        let a = merged
            .games
            .iter()
            .find(|g| g.install_dir.ends_with("a"))
            .unwrap();
        assert_eq!(a.status, installed);
        assert_eq!(a.cover, Some(PathBuf::from("cover.jpg")));
        let b = merged
            .games
            .iter()
            .find(|g| g.install_dir.ends_with("b"))
            .unwrap();
        assert_eq!(b.status, GameStatus::Pending);
        assert_eq!(merged.scanned_at, Some(2));
    }

    #[test]
    fn merge_drops_games_a_clean_adapter_no_longer_reports() {
        let lib = Library::default().with_games(
            vec![known(Launcher::Gog, "C:/g/old", GameStatus::Pending)],
            1,
        );
        let outcome = DiscoveryOutcome::default();
        assert!(lib.merge_scan(&outcome, &[], 2).games.is_empty());
    }

    #[test]
    fn merge_carries_over_games_from_failed_or_unavailable_launchers() {
        let lib = Library::default().with_games(
            vec![
                known(Launcher::Steam, "C:/g/s", GameStatus::Pending),
                known(Launcher::Epic, "C:/g/e", GameStatus::Pending),
                known(Launcher::Gog, "C:/g/g", GameStatus::Pending),
            ],
            1,
        );
        let outcome = DiscoveryOutcome {
            games: vec![],
            failures: vec![AdapterFailure {
                launcher: Launcher::Steam,
                error: Error::unsupported("x").to_dto(),
            }],
            unavailable: vec![Launcher::Epic],
        };
        let merged = lib.merge_scan(&outcome, &[], 2);
        let launchers: Vec<_> = merged.games.iter().map(|g| g.launcher).collect();
        assert!(launchers.contains(&Launcher::Steam));
        assert!(launchers.contains(&Launcher::Epic));
        assert!(!launchers.contains(&Launcher::Gog));
    }

    #[test]
    fn merge_and_with_hidden_apply_the_hidden_list() {
        let outcome = DiscoveryOutcome {
            games: vec![discovered(Launcher::Steam, "C:/g/h")],
            ..Default::default()
        };
        let id = game_id(Path::new("C:/g/h"));
        let merged = Library::default().merge_scan(&outcome, std::slice::from_ref(&id), 2);
        assert!(merged.games[0].hidden);
        let shown = merged.with_hidden(&[]);
        assert!(!shown.games[0].hidden);
        assert!(merged.games[0].hidden, "with_hidden must not mutate");
    }
}
