//! Custom folders from Settings: every direct subfolder that holds an
//! executable (a few levels down) is a game, titled after the folder. The
//! folder itself counts as one game when it has an exe but no such subfolders,
//! or when it is plainly one game's folder (engine files beside an exe), so
//! mod tools in its subfolders are not listed as games.

use std::path::{Path, PathBuf};

use crate::{
    analysis::walk::Tree,
    discovery::{ubisoft::folder_name, LauncherAdapter},
    model::{DiscoveredGame, Launcher},
    platform::Platform,
    Result,
};

const EXE_DEPTH: usize = 3;

pub struct Custom {
    folders: Vec<PathBuf>,
}

impl Custom {
    pub fn new(folders: Vec<PathBuf>) -> Self {
        Self { folders }
    }
}

impl LauncherAdapter for Custom {
    fn launcher(&self) -> Launcher {
        Launcher::Custom
    }

    fn discover(&self, _: &Platform) -> Result<Vec<DiscoveredGame>> {
        Ok(self.folders.iter().flat_map(|f| discover_from(f)).collect())
    }
}

pub fn discover_from(folder: &Path) -> Vec<DiscoveredGame> {
    if is_game_folder(folder) {
        return vec![game(folder)];
    }
    let Ok(read) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = read
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    let games: Vec<DiscoveredGame> = dirs
        .iter()
        .filter(|d| has_exe(d))
        .map(|d| game(d))
        .collect();
    if games.is_empty() && has_exe(folder) {
        return vec![game(folder)];
    }
    games
}

/// An exe at the top level next to Unity's player DLL or its `<Name>_Data`
/// folder, or next to Unreal's `Engine` folder.
fn is_game_folder(dir: &Path) -> bool {
    let Ok(read) = std::fs::read_dir(dir) else {
        return false;
    };
    let entries: Vec<(String, bool)> = read
        .filter_map(|e| e.ok())
        .map(|e| {
            (
                e.file_name().to_string_lossy().to_lowercase(),
                e.path().is_dir(),
            )
        })
        .collect();
    let has = |name: &str, dir: bool| entries.iter().any(|(n, d)| n == name && *d == dir);
    entries
        .iter()
        .filter_map(|(n, d)| (!d).then(|| n.strip_suffix(".exe")).flatten())
        .any(|stem| {
            has("unityplayer.dll", false)
                || has(&format!("{stem}_data"), true)
                || has("engine", true)
        })
}

fn has_exe(dir: &Path) -> bool {
    Tree::collect(dir, EXE_DEPTH)
        .files
        .iter()
        .any(|f| f.name.ends_with(".exe"))
}

fn game(dir: &Path) -> DiscoveredGame {
    DiscoveredGame {
        launcher: Launcher::Custom,
        title: folder_name(dir),
        install_dir: dir.to_path_buf(),
        declared_exe: None,
        launcher_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::walk::testing::make_tree;

    #[test]
    fn subfolders_with_executables_are_games() {
        let tmp = tempfile::tempdir().unwrap();
        make_tree(
            tmp.path(),
            &[
                ("Games/Alpha/alpha.exe", b""),
                ("Games/Beta/bin/x64/beta.exe", b""),
                ("Games/Docs/readme.txt", b""),
                ("Games/loose.exe", b""),
            ],
        );
        let games = discover_from(&tmp.path().join("Games"));
        let titles: Vec<&str> = games.iter().map(|g| g.title.as_str()).collect();
        assert_eq!(titles, vec!["Alpha", "Beta"]);
        assert_eq!(games[0].launcher, Launcher::Custom);
    }

    #[test]
    fn a_game_folder_with_tool_subfolders_is_one_game() {
        let tmp = tempfile::tempdir().unwrap();
        make_tree(
            tmp.path(),
            &[
                ("Sim/Sim.exe", b""),
                ("Sim/UnityPlayer.dll", b""),
                ("Sim/Sim_Data/level0", b""),
                ("Sim/Tools/Editor/editor.exe", b""),
                ("Sim/Manager/manager.exe", b""),
                ("Lib/Unreal/Game.exe", b""),
                ("Lib/Unreal/Engine/x.ini", b""),
            ],
        );
        let games = discover_from(&tmp.path().join("Sim"));
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].title, "Sim");
        assert_eq!(discover_from(&tmp.path().join("Lib/Unreal")).len(), 1);
        assert_eq!(discover_from(&tmp.path().join("Lib"))[0].title, "Unreal");
    }

    #[test]
    fn a_folder_that_is_itself_a_game_counts_once() {
        let tmp = tempfile::tempdir().unwrap();
        make_tree(
            tmp.path(),
            &[("Solo/solo.exe", b""), ("Solo/data/level.pak", b"")],
        );
        let games = discover_from(&tmp.path().join("Solo"));
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].title, "Solo");
        assert!(discover_from(&tmp.path().join("missing")).is_empty());
    }
}
