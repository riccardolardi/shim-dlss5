//! Apply a plan with a journal: back up before every overwrite, record after
//! every write, roll back on the first failure. Uninstall is the same walk
//! backwards from the saved manifest.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::{
    components::fetch::sha256_file,
    install::{
        manifest::{FileRecord, InstallManifest, INSTALL_SCHEMA},
        planner::{FileOp, Plan},
    },
    paths::AppPaths,
    persist::{read_json, write_json},
    Error, Result,
};

/// What the UI shows while an install runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub index: usize,
    pub total: usize,
    pub message: String,
}

/// On-disk journal written after every op, so a crash mid-install leaves
/// enough behind to finish the rollback on the next start.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct JournalFile {
    game_id: String,
    files: Vec<FileRecord>,
}

pub struct Journal<'a> {
    paths: &'a AppPaths,
    game_id: String,
    records: Vec<FileRecord>,
}

impl<'a> Journal<'a> {
    fn journal_path(paths: &AppPaths, game_id: &str) -> PathBuf {
        paths.installs_dir().join(format!("{game_id}.journal.json"))
    }

    /// Run every op of `plan`. On failure every change is undone and the
    /// original error is returned; if the undo itself fails, `RollbackFailed`
    /// lists what is left.
    pub fn run(
        paths: &'a AppPaths,
        game_id: &str,
        plan: &Plan,
        anti_cheat_override: bool,
        progress: &mut dyn FnMut(Step),
    ) -> Result<InstallManifest> {
        check_writable(plan.exe_dir())?;
        let mut journal = Journal {
            paths,
            game_id: game_id.to_string(),
            records: Vec::new(),
        };
        let total = plan.ops.len();
        for (index, op) in plan.ops.iter().enumerate() {
            progress(Step {
                index,
                total,
                message: op.note().to_string(),
            });
            if let Err(e) = journal.apply(op) {
                tracing::warn!(detail = %e.detail(), "install failed, rolling back");
                return match rollback(&journal.records) {
                    Ok(()) => {
                        journal.clear_journal();
                        Err(e)
                    }
                    Err(rb) => Err(rb),
                };
            }
        }
        let manifest = InstallManifest {
            schema: INSTALL_SCHEMA,
            game_id: game_id.to_string(),
            exe: plan.exe.clone(),
            route: plan.route,
            installed_at: now(),
            components: plan.components.clone(),
            model_sha256: plan.model_sha256.clone(),
            user_files: plan.user_files.clone(),
            anti_cheat_override,
            files: journal.records.clone(),
        };
        manifest.save(paths)?;
        journal.clear_journal();
        Ok(manifest)
    }

    fn apply(&mut self, op: &FileOp) -> Result<()> {
        let dst = op.dst();
        let (backup, sha256_before) = if dst.exists() {
            let before = sha256_file(dst)?;
            let backup = self.backup_path(dst, &before);
            copy(dst, &backup)?;
            (Some(backup), Some(before))
        } else {
            (None, None)
        };
        match op {
            FileOp::Copy { src, .. } => copy(src, dst)?,
            FileOp::WriteText { text, .. } => {
                if let Some(parent) = dst.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
                }
                std::fs::write(dst, text).map_err(|e| Error::io(dst, e))?;
            }
        }
        self.records.push(FileRecord {
            target: dst.to_path_buf(),
            backup,
            sha256_before,
            sha256_after: sha256_file(dst)?,
        });
        self.persist_journal()
    }

    fn backup_path(&self, target: &Path, sha256: &str) -> PathBuf {
        let name = target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.paths
            .backups_dir(&self.game_id)
            .join(format!("{}_{name}", &sha256[..10]))
    }

    fn persist_journal(&self) -> Result<()> {
        write_json(
            &Self::journal_path(self.paths, &self.game_id),
            &JournalFile {
                game_id: self.game_id.clone(),
                files: self.records.clone(),
            },
            "install journal",
        )
    }

    fn clear_journal(&self) {
        let _ = std::fs::remove_file(Self::journal_path(self.paths, &self.game_id));
    }
}

/// Undo an install from its manifest, newest file first, then forget it.
pub fn uninstall(paths: &AppPaths, manifest: &InstallManifest) -> Result<()> {
    rollback(&manifest.files)?;
    manifest.delete(paths)?;
    let _ = std::fs::remove_dir_all(paths.backups_dir(&manifest.game_id));
    Ok(())
}

/// Finish a rollback left behind by a crash, if a journal exists.
pub fn recover(paths: &AppPaths, game_id: &str) -> Result<bool> {
    let path = Journal::journal_path(paths, game_id);
    let Some(journal): Option<JournalFile> = read_json(&path, "install journal")? else {
        return Ok(false);
    };
    rollback(&journal.files)?;
    let _ = std::fs::remove_file(&path);
    Ok(true)
}

/// Restore every backup and delete every file we added, in reverse order.
/// Keeps going after a failure so as much as possible is undone, then
/// reports what is left.
pub fn rollback(records: &[FileRecord]) -> Result<()> {
    let mut leftovers = Vec::new();
    for r in records.iter().rev() {
        let result = match &r.backup {
            Some(backup) => copy(backup, &r.target),
            None => match std::fs::remove_file(&r.target) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(Error::io(&r.target, e)),
            },
        };
        if let Err(e) = result {
            let fix = match &r.backup {
                Some(b) => format!("copy {} back over it", b.display()),
                None => "delete it".to_string(),
            };
            tracing::error!(target = %r.target.display(), detail = %e.detail(), "rollback step failed");
            leftovers.push((r.target.clone(), fix));
        }
    }
    remove_empty_dirs(records);
    if leftovers.is_empty() {
        Ok(())
    } else {
        Err(Error::RollbackFailed { leftovers })
    }
}

/// Folders we created for added files (e.g. `reshade-shaders\Shaders`) go
/// away once empty; anything with other content stays.
fn remove_empty_dirs(records: &[FileRecord]) {
    let mut dirs: Vec<&Path> = records
        .iter()
        .filter(|r| r.backup.is_none())
        .filter_map(|r| r.target.parent())
        .collect();
    dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
    dirs.dedup();
    for dir in dirs {
        let mut current = Some(dir);
        while let Some(d) = current {
            if std::fs::remove_dir(d).is_err() {
                break;
            }
            current = d.parent();
        }
    }
}

fn copy(src: &Path, dst: &Path) -> Result<()> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    std::fs::copy(src, dst).map(|_| ()).map_err(|e| {
        Error::io(
            if e.kind() == std::io::ErrorKind::NotFound {
                src
            } else {
                dst
            },
            e,
        )
    })
}

fn check_writable(dir: &Path) -> Result<()> {
    let probe = dir.join(format!(".shim-write-test-{}", std::process::id()));
    let result = std::fs::write(&probe, b"").and_then(|()| std::fs::remove_file(&probe));
    result.map_err(|source| Error::GameFolderNotWritable {
        path: dir.to_path_buf(),
        source,
    })
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::dlss::MODEL_DLL;
    use crate::install::planner::{self, testing::*, Inputs};
    use crate::model::{GraphicsApi, Route};
    use std::collections::BTreeMap;

    /// Every file under `dir` with its bytes, for byte-identical comparisons.
    fn snapshot(dir: &Path) -> BTreeMap<String, Vec<u8>> {
        let mut out = BTreeMap::new();
        fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
            for e in std::fs::read_dir(dir).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    walk(root, &p, out);
                } else {
                    let rel = p
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/");
                    out.insert(rel, std::fs::read(&p).unwrap());
                }
            }
        }
        walk(dir, dir, &mut out);
        out
    }

    struct Fixture {
        _tmp: tempfile::TempDir,
        paths: AppPaths,
        game_dir: PathBuf,
        plan: Plan,
    }

    fn fixture(route: Route) -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(tmp.path().join("data"));
        paths.ensure().unwrap();
        let (store, components) = fake_store(&tmp.path().join("store"));
        let game_dir = tmp.path().join("game");
        std::fs::create_dir_all(&game_dir).unwrap();
        std::fs::write(game_dir.join("G.exe"), b"game").unwrap();
        // A pre-existing OptiScaler.ini / model stand in for files we overwrite.
        std::fs::write(game_dir.join("OptiScaler.ini"), b"old ini").unwrap();
        let settings = settings_with_user_files(tmp.path());
        let (apis, ships) = match route {
            Route::OptiScaler => (vec![GraphicsApi::Dx12], true),
            _ => (vec![GraphicsApi::Dx11], false),
        };
        let game = game(&game_dir, &apis, route, ships);
        let plan = planner::plan(&Inputs {
            game: &game,
            store: &store,
            components: &components,
            settings: &settings,
        })
        .unwrap();
        Fixture {
            _tmp: tmp,
            paths,
            game_dir,
            plan,
        }
    }

    #[test]
    fn install_then_uninstall_is_byte_identical() {
        for route in [Route::OptiScaler, Route::ReShadeRenoDx] {
            let f = fixture(route);
            let before = snapshot(&f.game_dir);
            let mut steps = Vec::new();
            let manifest =
                Journal::run(&f.paths, "g1", &f.plan, false, &mut |s| steps.push(s)).unwrap();

            assert_eq!(steps.len(), f.plan.ops.len());
            assert_eq!(manifest.files.len(), f.plan.ops.len());
            assert!(f.game_dir.join(MODEL_DLL).is_file());
            assert!(f.game_dir.join("shim.json").is_file());
            assert!(InstallManifest::load(&f.paths, "g1").unwrap().is_some());
            if route == Route::OptiScaler {
                let ini = manifest
                    .files
                    .iter()
                    .find(|r| r.target.ends_with("OptiScaler.ini"))
                    .unwrap();
                assert!(
                    ini.backup.as_ref().unwrap().is_file(),
                    "overwritten file was backed up"
                );
                assert_eq!(
                    ini.sha256_before.as_deref(),
                    Some(sha256_file(ini.backup.as_ref().unwrap()).unwrap().as_str())
                );
            }
            assert!(!Journal::journal_path(&f.paths, "g1").exists());

            uninstall(&f.paths, &manifest).unwrap();
            assert_eq!(snapshot(&f.game_dir), before, "{route:?}");
            assert!(InstallManifest::load(&f.paths, "g1").unwrap().is_none());
            assert!(!f.paths.backups_dir("g1").exists());
        }
    }

    #[test]
    fn failure_at_every_op_rolls_back_completely() {
        let total = fixture(Route::ReShadeRenoDx).plan.ops.len();
        for fail_at in 0..total {
            let f = fixture(Route::ReShadeRenoDx);
            let before = snapshot(&f.game_dir);
            let mut broken = f.plan.clone();
            broken.ops[fail_at] = match &broken.ops[fail_at] {
                FileOp::Copy { dst, note, .. } => FileOp::Copy {
                    src: PathBuf::from("Z:/does/not/exist"),
                    dst: dst.clone(),
                    note: note.clone(),
                },
                FileOp::WriteText { dst, note, .. } => FileOp::Copy {
                    src: PathBuf::from("Z:/does/not/exist"),
                    dst: dst.clone(),
                    note: note.clone(),
                },
            };
            let err = Journal::run(&f.paths, "g1", &broken, false, &mut |_| {}).unwrap_err();
            assert_eq!(err.code(), "io", "op {fail_at}");
            assert_eq!(
                snapshot(&f.game_dir),
                before,
                "op {fail_at} left changes behind"
            );
            assert!(InstallManifest::load(&f.paths, "g1").unwrap().is_none());
            assert!(!Journal::journal_path(&f.paths, "g1").exists());
        }
    }

    #[test]
    fn recover_finishes_an_interrupted_install() {
        let f = fixture(Route::OptiScaler);
        let before = snapshot(&f.game_dir);
        let mut journal = Journal {
            paths: &f.paths,
            game_id: "g1".into(),
            records: Vec::new(),
        };
        journal.apply(&f.plan.ops[0]).unwrap();
        journal.apply(&f.plan.ops[1]).unwrap();
        assert_ne!(snapshot(&f.game_dir), before);
        drop(journal); // "crash": the journal file stays on disk
        assert!(recover(&f.paths, "g1").unwrap());
        assert_eq!(snapshot(&f.game_dir), before);
        assert!(!recover(&f.paths, "g1").unwrap());
    }

    #[test]
    fn rollback_reports_leftovers_but_keeps_going() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.txt");
        std::fs::write(&a, b"added").unwrap();
        let records = vec![
            FileRecord {
                target: a.clone(),
                backup: None,
                sha256_before: None,
                sha256_after: String::new(),
            },
            FileRecord {
                target: tmp.path().join("b.txt"),
                backup: Some(PathBuf::from("Z:/missing/backup")),
                sha256_before: None,
                sha256_after: String::new(),
            },
        ];
        let err = rollback(&records).unwrap_err();
        assert!(!a.exists(), "the undoable step was still undone");
        match err {
            Error::RollbackFailed { leftovers } => {
                assert_eq!(leftovers.len(), 1);
                assert!(leftovers[0].0.ends_with("b.txt"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn unwritable_folder_is_reported_before_any_change() {
        let f = fixture(Route::OptiScaler);
        let mut plan = f.plan.clone();
        plan.exe = PathBuf::from("Z:/nope/G.exe");
        let err = Journal::run(&f.paths, "g1", &plan, false, &mut |_| {}).unwrap_err();
        assert_eq!(err.code(), "game_folder_not_writable");
    }
}
