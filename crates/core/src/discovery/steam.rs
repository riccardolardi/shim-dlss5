//! Steam: `HKCU\Software\Valve\Steam\SteamPath` → `steamapps\libraryfolders.vdf`
//! → every `appmanifest_*.acf` in every library.
//!
//! Only fully installed apps (`StateFlags` has bit 4) are reported, and the
//! usual non-game apps (redistributables, Proton, runtimes, SDKs) are skipped.

use std::path::{Path, PathBuf};

use crate::{
    discovery::{vdf, LauncherAdapter},
    model::{DiscoveredGame, Launcher},
    platform::{Hive, Platform},
    Error, Result,
};

pub struct Steam;

const STATE_FULLY_INSTALLED: u32 = 4;

/// App ids Steam lists like games but which are tools or runtimes.
const SKIP_APPIDS: &[&str] = &[
    "228980",  // Steamworks Common Redistributables
    "1070560", // Steam Linux Runtime
    "1391110", // Steam Linux Runtime - Soldier
    "1628350", // Steam Linux Runtime - Sniper
    "1493710", // Proton Experimental
    "2180100", // Proton Hotfix
    "1826330", // Proton EasyAntiCheat Runtime
    "1161040", // Proton BattlEye Runtime
    "250820",  // SteamVR
    "1007",    // Steamworks SDK Redist
];

const SKIP_NAME_WORDS: &[&str] = &[
    "redistributable",
    "proton",
    "steam linux runtime",
    "dedicated server",
    "soundtrack",
    "sdk",
    "benchmark tool",
];

impl LauncherAdapter for Steam {
    fn launcher(&self) -> Launcher {
        Launcher::Steam
    }

    fn discover(&self, platform: &Platform) -> Result<Vec<DiscoveredGame>> {
        let Some(root) = steam_path(platform) else {
            return Ok(Vec::new());
        };
        discover_from(&root)
    }
}

fn steam_path(platform: &Platform) -> Option<PathBuf> {
    platform
        .registry
        .read_string(Hive::CurrentUser, r"Software\Valve\Steam", "SteamPath")
        .map(|s| PathBuf::from(s.replace('/', "\\")))
        .filter(|p| p.is_dir())
}

/// Discover from a known Steam install folder (the one holding `steam.exe`).
pub fn discover_from(steam_root: &Path) -> Result<Vec<DiscoveredGame>> {
    let folders_file = steam_root.join("steamapps").join("libraryfolders.vdf");
    let text = std::fs::read_to_string(&folders_file).map_err(|e| Error::io(&folders_file, e))?;
    let libraries = library_folders(&text).map_err(|e| Error::Adapter {
        adapter: "Steam".into(),
        detail: format!("{}: {e}", folders_file.display()),
    })?;

    let mut games = Vec::new();
    for library in libraries {
        let steamapps = library.join("steamapps");
        for acf in manifest_files(&steamapps) {
            match std::fs::read_to_string(&acf) {
                Ok(text) => match app_manifest(&text, &steamapps) {
                    Ok(Some(game)) => games.push(game),
                    Ok(None) => {}
                    Err(e) => tracing::warn!(file = %acf.display(), %e, "unreadable appmanifest"),
                },
                Err(e) => tracing::warn!(file = %acf.display(), %e, "unreadable appmanifest"),
            }
        }
    }
    Ok(games)
}

/// Library roots listed in `libraryfolders.vdf`, in file order.
pub fn library_folders(text: &str) -> std::result::Result<Vec<PathBuf>, vdf::ParseError> {
    let doc = vdf::parse(text)?;
    let root = doc
        .obj("libraryfolders")
        .cloned()
        .unwrap_or_else(|| doc.clone());
    let mut out = Vec::new();
    for (_, entry) in root.objects() {
        if let Some(path) = entry.str("path") {
            out.push(PathBuf::from(path.replace("\\\\", "\\")));
        }
    }
    // Old-format files list "1" "D:\\Games" as plain strings.
    for (key, path) in root.strings() {
        if key.chars().all(|c| c.is_ascii_digit()) {
            out.push(PathBuf::from(path.replace("\\\\", "\\")));
        }
    }
    Ok(out)
}

fn manifest_files(steamapps: &Path) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(steamapps) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = read
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            name.starts_with("appmanifest_") && name.ends_with(".acf")
        })
        .collect();
    files.sort();
    files
}

/// One `appmanifest_*.acf`. `None` when it is not an installed game.
pub fn app_manifest(
    text: &str,
    steamapps: &Path,
) -> std::result::Result<Option<DiscoveredGame>, vdf::ParseError> {
    let doc = vdf::parse(text)?;
    let Some(app) = doc.obj("AppState") else {
        return Ok(None);
    };
    let (Some(appid), Some(name), Some(installdir)) =
        (app.str("appid"), app.str("name"), app.str("installdir"))
    else {
        return Ok(None);
    };
    let flags: u32 = app
        .str("StateFlags")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    if flags & STATE_FULLY_INSTALLED == 0 || is_tool(appid, name) {
        return Ok(None);
    }
    Ok(Some(DiscoveredGame {
        launcher: Launcher::Steam,
        title: name.to_string(),
        install_dir: steamapps.join("common").join(installdir),
        declared_exe: None,
        launcher_id: Some(appid.to_string()),
    }))
}

fn is_tool(appid: &str, name: &str) -> bool {
    let lower = name.to_lowercase();
    SKIP_APPIDS.contains(&appid) || SKIP_NAME_WORDS.iter().any(|w| lower.contains(w))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/steam")
            .join(name);
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    #[test]
    fn library_folders_new_and_old_format() {
        let new = r#"
            "libraryfolders"
            {
                "0" { "path" "C:\\Program Files (x86)\\Steam" "label" "" }
                "1" { "path" "D:\\SteamLibrary" }
            }"#;
        assert_eq!(
            library_folders(new).unwrap(),
            vec![
                PathBuf::from(r"C:\Program Files (x86)\Steam"),
                PathBuf::from(r"D:\SteamLibrary")
            ]
        );
        let old = r#""LibraryFolders" { "TimeNextStatsReport" "1" "1" "E:\\Games" }"#;
        assert_eq!(
            library_folders(old).unwrap(),
            vec![PathBuf::from(r"E:\Games")]
        );
    }

    #[test]
    fn installed_game_is_reported_with_appid_and_common_dir() {
        let steamapps = Path::new("D:/SteamLibrary/steamapps");
        let game = app_manifest(&fixture("appmanifest_1091500.acf"), steamapps)
            .unwrap()
            .unwrap();
        assert_eq!(game.title, "Cyberpunk 2077");
        assert_eq!(game.launcher_id.as_deref(), Some("1091500"));
        assert_eq!(
            game.install_dir,
            steamapps.join("common").join("Cyberpunk 2077")
        );
        assert_eq!(game.launcher, Launcher::Steam);
    }

    #[test]
    fn redistributables_and_partial_installs_are_skipped() {
        let steamapps = Path::new("C:/Steam/steamapps");
        assert!(app_manifest(&fixture("appmanifest_228980.acf"), steamapps)
            .unwrap()
            .is_none());
        assert!(
            app_manifest(&fixture("appmanifest_downloading.acf"), steamapps)
                .unwrap()
                .is_none()
        );
        assert!(is_tool("1", "Proton 9.0"));
        assert!(is_tool("1", "Game Soundtrack"));
        assert!(!is_tool("1", "Half-Life 2"));
    }

    #[test]
    fn discover_from_walks_every_library() {
        let tmp = tempfile::tempdir().unwrap();
        let steam = tmp.path().join("Steam");
        let other = tmp.path().join("Other");
        for lib in [&steam, &other] {
            std::fs::create_dir_all(lib.join("steamapps")).unwrap();
        }
        let vdf = format!(
            "\"libraryfolders\" {{ \"0\" {{ \"path\" \"{}\" }} \"1\" {{ \"path\" \"{}\" }} }}",
            steam.display().to_string().replace('\\', "\\\\"),
            other.display().to_string().replace('\\', "\\\\"),
        );
        std::fs::write(steam.join("steamapps/libraryfolders.vdf"), vdf).unwrap();
        std::fs::write(
            steam.join("steamapps/appmanifest_228980.acf"),
            fixture("appmanifest_228980.acf"),
        )
        .unwrap();
        std::fs::write(
            other.join("steamapps/appmanifest_1091500.acf"),
            fixture("appmanifest_1091500.acf"),
        )
        .unwrap();
        std::fs::write(other.join("steamapps/appmanifest_broken.acf"), "{{{").unwrap();

        let games = discover_from(&steam).unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].title, "Cyberpunk 2077");
        assert!(games[0].install_dir.starts_with(&other));
    }

    #[test]
    fn missing_libraryfolders_is_an_io_error() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(discover_from(tmp.path()).unwrap_err().code(), "io");
    }

    #[test]
    fn no_registry_entry_means_no_games_not_an_error() {
        assert!(Steam.discover(&Platform::unavailable()).unwrap().is_empty());
    }
}
