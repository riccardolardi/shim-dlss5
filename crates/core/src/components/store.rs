//! Verified, extracted components on disk:
//! `components\<id>\<version>\` plus a `.shim-component.json` record that
//! lists every extracted file with its hash, so the store can prove it is
//! intact without re-downloading.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    components::{
        extract,
        fetch::{self, Progress},
        manifest::Component,
    },
    paths::AppPaths,
    persist::{read_json, write_json},
    Error, Result,
};

pub const RECORD_FILE: &str = ".shim-component.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreRecord {
    pub id: String,
    pub version: String,
    pub asset_sha256: String,
    /// `(relative path, sha256)` of every extracted file.
    pub files: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum ComponentStatus {
    /// Extracted and every file hashes as recorded.
    Verified,
    /// Not downloaded yet.
    Missing,
    /// Present but a file is gone or changed; re-fetch to repair.
    Mismatch { detail: String },
}

pub struct ComponentStore {
    root: PathBuf,
}

impl ComponentStore {
    pub fn new(paths: &AppPaths) -> Self {
        Self {
            root: paths.components_dir(),
        }
    }

    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn dir(&self, c: &Component) -> PathBuf {
        self.root.join(&c.id).join(&c.version)
    }

    /// Full path of one extracted file, if the component is verified.
    pub fn file(&self, c: &Component, rel: &str) -> Result<PathBuf> {
        match self.status(c) {
            ComponentStatus::Verified => {
                let path = self.dir(c).join(rel);
                if path.is_file() {
                    Ok(path)
                } else {
                    Err(Error::ComponentMissing {
                        id: c.id.clone(),
                        detail: format!("{rel} is not part of the component"),
                    })
                }
            }
            ComponentStatus::Missing => Err(Error::ComponentMissing {
                id: c.id.clone(),
                detail: "not downloaded".into(),
            }),
            ComponentStatus::Mismatch { detail } => Err(Error::ComponentMissing {
                id: c.id.clone(),
                detail,
            }),
        }
    }

    pub fn status(&self, c: &Component) -> ComponentStatus {
        let dir = self.dir(c);
        let record: Option<StoreRecord> = match read_json(&dir.join(RECORD_FILE), "component") {
            Ok(r) => r,
            Err(e) => return ComponentStatus::Mismatch { detail: e.detail() },
        };
        let Some(record) = record else {
            return ComponentStatus::Missing;
        };
        if record.asset_sha256 != c.sha256 {
            return ComponentStatus::Mismatch {
                detail: "extracted from a different asset than the manifest pins".into(),
            };
        }
        for (rel, want) in &record.files {
            let path = dir.join(rel);
            match fetch::sha256_file(&path) {
                Ok(got) if &got == want => {}
                Ok(_) => {
                    return ComponentStatus::Mismatch {
                        detail: format!("{rel} has changed"),
                    }
                }
                Err(_) => {
                    return ComponentStatus::Mismatch {
                        detail: format!("{rel} is missing"),
                    }
                }
            }
        }
        ComponentStatus::Verified
    }

    /// Download, verify and extract. Replaces whatever was in the folder.
    pub fn fetch(&self, c: &Component, progress: &mut dyn FnMut(Progress)) -> Result<()> {
        let downloads = self.root.join(".downloads");
        let asset_name = c.asset.rsplit('/').next().unwrap_or("asset");
        let asset = downloads.join(format!("{}-{}-{asset_name}", c.id, c.version));
        fetch::download(&c.asset, &c.sha256, c.size, &asset, progress)?;
        let result = self.install_from(c, &asset);
        let _ = std::fs::remove_file(&asset);
        result
    }

    /// Extract an already-downloaded asset (hash is checked again here).
    pub fn install_from(&self, c: &Component, asset: &Path) -> Result<()> {
        let actual = fetch::sha256_file(asset)?;
        if !actual.eq_ignore_ascii_case(&c.sha256) {
            return Err(Error::HashMismatch {
                what: asset.display().to_string(),
                expected: c.sha256.clone(),
                actual,
            });
        }
        let dir = self.dir(c);
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
        }
        let written = extract::extract(asset, c.archive, &c.extract, &dir)?;
        if written.is_empty() {
            return Err(Error::Archive {
                detail: format!("{}: nothing matched the extract rules", c.id),
            });
        }
        let mut files = Vec::with_capacity(written.len());
        for rel in written {
            let hash = fetch::sha256_file(&dir.join(&rel))?;
            files.push((rel, hash));
        }
        let record = StoreRecord {
            id: c.id.clone(),
            version: c.version.clone(),
            asset_sha256: c.sha256.to_lowercase(),
            files,
        };
        write_json(&dir.join(RECORD_FILE), &record, "component")
    }

    /// Every extracted file of a verified component, relative paths sorted.
    pub fn files(&self, c: &Component) -> Result<Vec<String>> {
        let record: StoreRecord = read_json(&self.dir(c).join(RECORD_FILE), "component")?
            .ok_or_else(|| Error::ComponentMissing {
                id: c.id.clone(),
                detail: "not downloaded".into(),
            })?;
        Ok(record.files.into_iter().map(|(rel, _)| rel).collect())
    }
}

#[cfg(test)]
pub mod testing {
    use super::*;
    use crate::components::{extract::testing::write_zip, manifest::ArchiveKind};

    /// A component whose asset is a local zip we build on the spot, already
    /// installed into `store`.
    pub fn fake_component(store: &ComponentStore, id: &str, files: &[(&str, &[u8])]) -> Component {
        // Inside the store root (a per-test tempdir), so parallel tests never share a path.
        std::fs::create_dir_all(&store.root).unwrap();
        let tmp = store.root.join(format!(".fake-{id}.zip"));
        write_zip(&tmp, files);
        let sha = fetch::sha256_file(&tmp).unwrap();
        let size = std::fs::metadata(&tmp).unwrap().len();
        let c = Component {
            id: id.into(),
            name: id.into(),
            license: "MIT".into(),
            publisher: "https://example.com".into(),
            version: "1.0".into(),
            asset: format!("https://example.com/{id}.zip"),
            sha256: sha,
            size,
            archive: ArchiveKind::Zip,
            extract: Default::default(),
            summary: "fake".into(),
        };
        store.install_from(&c, &tmp).unwrap();
        let _ = std::fs::remove_file(&tmp);
        c
    }
}

#[cfg(test)]
mod tests {
    use super::testing::fake_component;
    use super::*;

    #[test]
    fn install_verify_and_detect_tampering() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ComponentStore::at(tmp.path());
        let c = fake_component(&store, "thing", &[("a.dll", b"aaa"), ("sub/b.txt", b"b")]);
        assert_eq!(store.status(&c), ComponentStatus::Verified);
        assert_eq!(store.files(&c).unwrap(), vec!["a.dll", "sub/b.txt"]);
        assert!(store.file(&c, "a.dll").unwrap().is_file());
        assert_eq!(
            store.file(&c, "nope").unwrap_err().code(),
            "component_missing"
        );

        std::fs::write(store.dir(&c).join("a.dll"), b"changed").unwrap();
        assert!(
            matches!(store.status(&c), ComponentStatus::Mismatch { detail } if detail.contains("a.dll"))
        );

        std::fs::remove_file(store.dir(&c).join("sub/b.txt")).unwrap();
        assert!(matches!(store.status(&c), ComponentStatus::Mismatch { .. }));
        assert_eq!(
            store.file(&c, "a.dll").unwrap_err().code(),
            "component_missing"
        );
    }

    #[test]
    fn missing_and_wrong_asset_are_distinguished() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ComponentStore::at(tmp.path());
        let mut c = fake_component(&store, "thing", &[("a.dll", b"aaa")]);
        let mut other = c.clone();
        other.version = "2.0".into();
        assert_eq!(store.status(&other), ComponentStatus::Missing);

        c.sha256 = "0".repeat(64);
        assert!(matches!(store.status(&c), ComponentStatus::Mismatch { .. }));
    }

    #[test]
    fn install_from_refuses_a_bad_hash() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ComponentStore::at(tmp.path());
        let mut c = fake_component(&store, "thing", &[("a.dll", b"aaa")]);
        let zip = tmp.path().join("z.zip");
        crate::components::extract::testing::write_zip(&zip, &[("a.dll", b"other")]);
        c.version = "9".into();
        assert_eq!(
            store.install_from(&c, &zip).unwrap_err().code(),
            "hash_mismatch"
        );
        assert!(!store.dir(&c).exists());
    }
}
