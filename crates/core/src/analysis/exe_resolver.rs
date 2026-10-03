//! Which `.exe` in the folder is the game. Deterministic scoring over the
//! collected tree: title similarity, engine layout bonuses, launcher-word
//! penalties, size. Ties break on path so the answer never flips between runs.

use std::path::{Path, PathBuf};

use crate::analysis::walk::{Entry, Tree};
use crate::model::Engine;

const LAUNCHER_WORDS: &[&str] = &[
    "launcher",
    "setup",
    "unins",
    "uninstall",
    "crash",
    "report",
    "redist",
    "install",
    "update",
    "helper",
    "server",
    "dedicated",
    "benchmark",
    "config",
    "settings",
    "editor",
    "tool",
    "cleanup",
    "dxsetup",
    "vc_redist",
    "vcredist",
    "eac",
    "battleye",
    "anticheat",
    "bootstrap",
    "diagnos",
    "activation",
    "ue4prereq",
    "ueprereq",
    "unitycrashhandler",
    "dotnet",
    "_be",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub path: PathBuf,
    pub score: i32,
}

/// The best executable, if there is one. `declared` (from the launcher) wins
/// outright when it exists on disk.
pub fn resolve(tree: &Tree, title: &str, declared: Option<&Path>) -> Option<PathBuf> {
    if let Some(d) = declared.filter(|d| d.is_file()) {
        return Some(d.to_path_buf());
    }
    rank(tree, title).into_iter().next().map(|c| c.path)
}

pub fn rank(tree: &Tree, title: &str) -> Vec<Candidate> {
    let title_norm = normalise(title);
    let title_tokens = tokens(title);
    let mut out: Vec<Candidate> = tree
        .files
        .iter()
        .filter(|f| f.name.ends_with(".exe"))
        .map(|f| Candidate {
            path: f.path.clone(),
            score: score(f, tree, &title_norm, &title_tokens),
        })
        .collect();
    out.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
    out
}

fn score(f: &Entry, tree: &Tree, title_norm: &str, title_tokens: &[String]) -> i32 {
    let stem = f.name.trim_end_matches(".exe");
    let stem_norm = normalise(stem);
    let mut s = 0;

    if !stem_norm.is_empty() && stem_norm == title_norm {
        s += 50;
    } else if !stem_norm.is_empty()
        && !title_norm.is_empty()
        && (stem_norm.contains(title_norm) || title_norm.contains(stem_norm.as_str()))
    {
        s += 30;
    }
    let stem_tokens = tokens(stem);
    let overlap = stem_tokens
        .iter()
        .filter(|t| title_tokens.contains(t))
        .count();
    if !title_tokens.is_empty() {
        s += (overlap * 20 / title_tokens.len()) as i32;
    }

    if LAUNCHER_WORDS.iter().any(|w| f.name.contains(w)) {
        s -= 40;
    }
    if f.rel.contains("binaries/win64/") && f.name.ends_with("-win64-shipping.exe") {
        s += 40;
    } else if f.rel.contains("binaries/wingdk/") && f.name.ends_with("-wingdk-shipping.exe") {
        s += 35;
    }
    let data_dir = format!("{}_data", stem);
    if tree.dirs.iter().any(|d| {
        d.name == data_dir
            && Path::new(&d.rel)
                .parent()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                == Path::new(&f.rel)
                    .parent()
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
    }) {
        s += 40;
    }
    if f.rel.starts_with("bin/x64") {
        s += 25;
    }
    let dir = f.path.parent().unwrap_or(Path::new(""));
    if tree
        .files
        .iter()
        .any(|o| o.name == "steam_api64.dll" && o.path.parent() == Some(dir))
    {
        s += 10;
    }
    s += ((f.size / 1_000_000).min(30) / 2) as i32;
    s -= 2 * f.depth as i32;
    s
}

/// Lowercase letters and digits only: "Cyberpunk 2077" and "Cyberpunk2077.exe" meet.
fn normalise(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn tokens(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| t.len() > 1)
        .map(|t| t.to_lowercase())
        .filter(|t| !matches!(t.as_str(), "the" | "of" | "and" | "exe" | "game"))
        .collect()
}

/// Informational engine guess from the same tree.
pub fn engine(tree: &Tree, exe: &Path) -> Engine {
    let exe_name = exe
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if exe_name.ends_with("-shipping.exe") || tree.has_dir("engine") && tree.has_dir("binaries") {
        return Engine::Unreal;
    }
    // Only the exe's own `<Name>_Data` folder counts: any `*_data` folder is
    // too loose (MSFS 2024 has one deep in its content tree).
    let data_dir = format!("{}_data", exe_name.trim_end_matches(".exe"));
    if tree.has_file("unityplayer.dll") || tree.dirs.iter().any(|d| d.name == data_dir) {
        return Engine::Unity;
    }
    if tree.files.iter().any(|f| f.rel.starts_with("bin/x64")) && tree.has_dir("archive") {
        return Engine::RedEngine;
    }
    Engine::Other
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::walk::testing::make_tree;

    fn fixture(files: &[(&str, usize)]) -> (tempfile::TempDir, Tree) {
        let tmp = tempfile::tempdir().unwrap();
        let blobs: Vec<Vec<u8>> = files.iter().map(|(_, n)| vec![0u8; *n]).collect();
        let entries: Vec<(&str, &[u8])> = files
            .iter()
            .zip(&blobs)
            .map(|((p, _), b)| (*p, b.as_slice()))
            .collect();
        make_tree(tmp.path(), &entries);
        let t = Tree::collect(tmp.path(), 5);
        (tmp, t)
    }

    fn best(tree: &Tree, title: &str) -> String {
        resolve(tree, title, None)
            .unwrap()
            .strip_prefix(&tree.root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/")
    }

    #[test]
    fn unreal_shipping_exe_beats_the_bootstrap_launcher() {
        let (_t, tree) = fixture(&[
            ("Palworld.exe", 100),
            ("Pal/Binaries/Win64/Palworld-Win64-Shipping.exe", 100),
            ("Engine/Binaries/Win64/CrashReportClient.exe", 100),
        ]);
        assert_eq!(
            best(&tree, "Palworld"),
            "Pal/Binaries/Win64/Palworld-Win64-Shipping.exe"
        );
        assert_eq!(
            engine(&tree, &resolve(&tree, "Palworld", None).unwrap()),
            Engine::Unreal
        );
    }

    #[test]
    fn unity_exe_with_data_folder_wins_over_crash_handler() {
        let (_t, tree) = fixture(&[
            ("Hollow Knight.exe", 100),
            ("hollow_knight_Data/globalgamemanagers", 1),
            ("UnityCrashHandler64.exe", 100),
            ("UnityPlayer.dll", 1),
        ]);
        assert_eq!(best(&tree, "Hollow Knight"), "Hollow Knight.exe");
        assert_eq!(engine(&tree, Path::new("Hollow Knight.exe")), Engine::Unity);
    }

    #[test]
    fn red_engine_layout_and_title_match() {
        let (_t, tree) = fixture(&[
            ("REDprelauncher.exe", 100),
            ("bin/x64/Cyberpunk2077.exe", 100),
            ("bin/x64/steam_api64.dll", 1),
            ("archive/pc/content/basegame.archive", 1),
        ]);
        assert_eq!(best(&tree, "Cyberpunk 2077"), "bin/x64/Cyberpunk2077.exe");
        assert_eq!(
            engine(&tree, Path::new("bin/x64/Cyberpunk2077.exe")),
            Engine::RedEngine
        );
    }

    #[test]
    fn launcher_words_lose_and_size_breaks_ties() {
        let (_t, tree) = fixture(&[
            ("Launcher.exe", 10),
            ("Big.exe", 20_000_000),
            ("Small.exe", 10),
        ]);
        assert_eq!(best(&tree, "Something Else"), "Big.exe");
    }

    #[test]
    fn declared_exe_wins_when_present_and_is_ignored_when_missing() {
        let (_t, tree) = fixture(&[("A.exe", 10), ("B.exe", 10)]);
        let declared = tree.root.join("B.exe");
        assert_eq!(resolve(&tree, "A", Some(&declared)), Some(declared));
        let missing = tree.root.join("C.exe");
        assert_eq!(
            resolve(&tree, "A", Some(&missing)),
            Some(tree.root.join("A.exe"))
        );
    }

    #[test]
    fn no_executables_means_none_and_ranking_is_stable() {
        let (_t, tree) = fixture(&[("readme.txt", 1)]);
        assert_eq!(resolve(&tree, "x", None), None);
        let (_t, tree) = fixture(&[("b.exe", 10), ("a.exe", 10)]);
        let ranked = rank(&tree, "zzz");
        assert!(ranked[0].path.ends_with("a.exe"));
    }
}
