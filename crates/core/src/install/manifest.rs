//! The per-game install record: every file we wrote, where its backup is,
//! and what was pinned. Stored twice: `installs\<game_id>.json` in our data
//! dir and `shim.json` beside the executable, so either side can recover.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    model::Route,
    paths::AppPaths,
    persist::{read_json, write_json},
    Error, Result,
};

pub const INSTALL_SCHEMA: u32 = 1;
pub const SIDECAR: &str = "shim.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FileRecord {
    pub target: PathBuf,
    /// Where the original was copied before we overwrote it. `None` when the
    /// target did not exist, in which case uninstall deletes it.
    pub backup: Option<PathBuf>,
    pub sha256_before: Option<String>,
    pub sha256_after: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ComponentPin {
    pub id: String,
    pub version: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct InstallManifest {
    pub schema: u32,
    pub game_id: String,
    pub exe: PathBuf,
    pub route: Route,
    #[ts(type = "number")]
    pub installed_at: u64,
    pub components: Vec<ComponentPin>,
    /// SHA-256 of the user's `nvngx_dlssnr.dll` as installed.
    pub model_sha256: Option<String>,
    /// Other user-supplied files, `(what, sha256)`.
    pub user_files: Vec<(String, String)>,
    pub anti_cheat_override: bool,
    /// In the order they were written; uninstall walks it backwards.
    pub files: Vec<FileRecord>,
}

impl InstallManifest {
    pub fn load(paths: &AppPaths, game_id: &str) -> Result<Option<Self>> {
        read_json(&paths.install_manifest(game_id), "install record")
    }

    /// Write the data-dir copy and the sidecar. The data-dir copy is the
    /// source of truth; a sidecar that cannot be written is logged, not fatal.
    pub fn save(&self, paths: &AppPaths) -> Result<()> {
        write_json(
            &paths.install_manifest(&self.game_id),
            self,
            "install record",
        )?;
        let sidecar = self.sidecar_path();
        if let Err(e) = write_json(&sidecar, self, "install record") {
            tracing::warn!(path = %sidecar.display(), detail = %e.detail(), "sidecar not written");
        }
        Ok(())
    }

    pub fn delete(&self, paths: &AppPaths) -> Result<()> {
        for path in [paths.install_manifest(&self.game_id), self.sidecar_path()] {
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(Error::io(path, e)),
            }
        }
        Ok(())
    }

    pub fn sidecar_path(&self) -> PathBuf {
        self.exe.parent().unwrap_or(Path::new("")).join(SIDECAR)
    }

    /// True when any pinned component differs from what is installed.
    pub fn is_outdated(&self, current: &crate::components::ComponentManifest) -> bool {
        self.components.iter().any(|pin| {
            current.get(&pin.id).is_none_or(|c| {
                c.version != pin.version || !c.sha256.eq_ignore_ascii_case(&pin.sha256)
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::ComponentManifest;

    fn manifest(dir: &Path) -> InstallManifest {
        InstallManifest {
            schema: INSTALL_SCHEMA,
            game_id: "g1".into(),
            exe: dir.join("Game.exe"),
            route: Route::OptiScaler,
            installed_at: 1,
            components: vec![ComponentPin {
                id: "optiscaler".into(),
                version: "0.9.4".into(),
                sha256: ComponentManifest::embedded()
                    .get("optiscaler")
                    .unwrap()
                    .sha256
                    .clone(),
            }],
            model_sha256: None,
            user_files: vec![],
            anti_cheat_override: false,
            files: vec![],
        }
    }

    #[test]
    fn save_load_delete_round_trip_with_sidecar() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(tmp.path().join("data"));
        let game_dir = tmp.path().join("game");
        std::fs::create_dir_all(&game_dir).unwrap();
        let m = manifest(&game_dir);
        assert_eq!(InstallManifest::load(&paths, "g1").unwrap(), None);
        m.save(&paths).unwrap();
        assert_eq!(
            InstallManifest::load(&paths, "g1").unwrap(),
            Some(m.clone())
        );
        assert!(game_dir.join(SIDECAR).is_file());
        m.delete(&paths).unwrap();
        assert_eq!(InstallManifest::load(&paths, "g1").unwrap(), None);
        assert!(!game_dir.join(SIDECAR).exists());
        m.delete(&paths).unwrap();
    }

    #[test]
    fn outdated_when_pin_differs_or_component_vanished() {
        let tmp = tempfile::tempdir().unwrap();
        let current = ComponentManifest::embedded();
        let mut m = manifest(tmp.path());
        assert!(!m.is_outdated(&current));
        m.components[0].version = "0.0.1".into();
        assert!(m.is_outdated(&current));
        m.components[0].id = "gone".into();
        assert!(m.is_outdated(&current));
    }
}
