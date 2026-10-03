//! Xbox app (PC Game Pass / Microsoft Store games installed the modern way):
//! `<drive>:\XboxGames\<Name>\Content\MicrosoftGame.config`. The config names
//! the display title and the executable. Older `WindowsApps` installs are
//! read-only to users and are not scanned.

use std::path::{Path, PathBuf};

use crate::{
    discovery::{ubisoft::folder_name, LauncherAdapter},
    model::{DiscoveredGame, Launcher},
    platform::Platform,
    Result,
};

pub const CONFIG_FILE: &str = "MicrosoftGame.config";

pub struct Xbox {
    roots: Vec<PathBuf>,
}

impl Xbox {
    pub fn new(roots: Vec<PathBuf>) -> Self {
        Self { roots }
    }

    /// `X:\XboxGames` on every drive letter that has one.
    pub fn default_location() -> Self {
        let roots = (b'A'..=b'Z')
            .map(|d| PathBuf::from(format!(r"{}:\XboxGames", d as char)))
            .filter(|p| p.is_dir())
            .collect();
        Self::new(roots)
    }
}

impl LauncherAdapter for Xbox {
    fn launcher(&self) -> Launcher {
        Launcher::Xbox
    }

    fn discover(&self, _: &Platform) -> Result<Vec<DiscoveredGame>> {
        Ok(self.roots.iter().flat_map(|r| discover_from(r)).collect())
    }
}

pub fn discover_from(root: &Path) -> Vec<DiscoveredGame> {
    let Ok(read) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = read
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs.iter()
        .filter_map(|dir| {
            let content = dir.join("Content");
            let text = std::fs::read_to_string(content.join(CONFIG_FILE)).ok()?;
            let cfg = parse_config(&text);
            Some(DiscoveredGame {
                launcher: Launcher::Xbox,
                title: cfg.display_name.unwrap_or_else(|| folder_name(dir)),
                install_dir: content.clone(),
                declared_exe: cfg.executable.map(|e| content.join(e.replace('/', "\\"))),
                launcher_id: cfg.identity,
            })
        })
        .collect()
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct GameConfig {
    pub display_name: Option<String>,
    pub executable: Option<String>,
    pub identity: Option<String>,
}

/// The three attributes we need, pulled out without an XML dependency. The
/// file is small and machine-written, so attribute scanning is enough.
pub fn parse_config(xml: &str) -> GameConfig {
    // Trailing space so `<Executable ` does not match `<ExecutableList>`.
    GameConfig {
        display_name: attr_in(xml, "<ShellVisuals ", "DefaultDisplayName")
            .filter(|s| !s.starts_with("ms-resource:")),
        executable: attr_in(xml, "<Executable ", "Name"),
        identity: attr_in(xml, "<Identity ", "Name"),
    }
}

/// Value of `attr="…"` inside the first element starting with `open`.
fn attr_in(xml: &str, open: &str, attr: &str) -> Option<String> {
    let start = xml.find(open)?;
    let rest = &xml[start..];
    let end = rest.find('>')?;
    let element = &rest[..end];
    let needle = format!("{attr}=\"");
    let at = element.find(&needle)? + needle.len();
    let value = &element[at..];
    let close = value.find('"')?;
    let v = unescape(&value[..close]);
    (!v.trim().is_empty()).then_some(v)
}

fn unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> String {
        std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/xbox/MicrosoftGame.config"),
        )
        .unwrap()
    }

    #[test]
    fn parses_title_exe_and_identity() {
        let cfg = parse_config(&fixture());
        assert_eq!(cfg.display_name.as_deref(), Some("Forza Horizon 5"));
        assert_eq!(cfg.executable.as_deref(), Some("ForzaHorizon5.exe"));
        assert_eq!(
            cfg.identity.as_deref(),
            Some("Microsoft.624F8B84B80_8wekyb3d8bbwe")
        );
    }

    #[test]
    fn resource_names_are_ignored_and_entities_unescaped() {
        let cfg = parse_config(
            r#"<Game><ShellVisuals DefaultDisplayName="ms-resource:AppName"/><ExecutableList><Executable Name="Bin/Game &amp; Co.exe" Id="Game"/></ExecutableList></Game>"#,
        );
        assert_eq!(cfg.display_name, None);
        assert_eq!(cfg.executable.as_deref(), Some("Bin/Game & Co.exe"));
        assert_eq!(parse_config("not xml"), GameConfig::default());
    }

    #[test]
    fn discover_from_reads_xboxgames_layout() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("XboxGames");
        let content = root.join("Forza Horizon 5").join("Content");
        std::fs::create_dir_all(&content).unwrap();
        std::fs::write(content.join(CONFIG_FILE), fixture()).unwrap();
        std::fs::create_dir_all(root.join("Unfinished").join("Content")).unwrap();
        std::fs::create_dir_all(root.join("NoName").join("Content")).unwrap();
        std::fs::write(
            root.join("NoName").join("Content").join(CONFIG_FILE),
            r#"<Game><ExecutableList><Executable Name="x.exe"/></ExecutableList></Game>"#,
        )
        .unwrap();

        let games = Xbox::new(vec![root.clone(), tmp.path().join("nope")])
            .discover(&Platform::unavailable())
            .unwrap();
        assert_eq!(games.len(), 2);
        assert_eq!(games[0].title, "Forza Horizon 5");
        assert_eq!(games[0].install_dir, content);
        assert_eq!(
            games[0].declared_exe,
            Some(content.join("ForzaHorizon5.exe"))
        );
        assert_eq!(games[1].title, "NoName", "folder name fallback");
    }
}
