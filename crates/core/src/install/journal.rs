//! Apply a plan with a journal: back up before every overwrite, record before
//! and after every write, roll back on the first failure. Uninstall is the
//! same walk backwards from the saved manifest.
//!
//! The journal file is written *before* each file is touched, so a crash in
//! the middle of a copy still leaves a record saying "this target was ours;
//! restore its backup or delete the partial file".

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

/// On-disk journal, rewritten before and after every op.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct JournalFile {
    game_id: String,
    /// The game's executable folder: empty-folder cleanup never climbs past it.
    root: PathBuf,
    files: Vec<FileRecord>,
}

const JOURNAL_SUFFIX: &str = ".journal.json";

pub struct Journal<'a> {
    paths: &'a AppPaths,
    game_id: String,
    root: PathBuf,
    records: Vec<FileRecord>,
}

impl<'a> Journal<'a> {
    fn journal_path(paths: &AppPaths, game_id: &str) -> PathBuf {
        paths
            .installs_dir()
            .join(format!("{game_id}{JOURNAL_SUFFIX}"))
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
            root: plan.exe_dir().to_path_buf(),
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
                return Err(journal.undo_after(e));
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
            side_effects: plan.side_effects.clone(),
        };
        // No record means no way to undo later, so an unsaved manifest is
        // treated like a failed op.
        if let Err(e) = manifest.save(paths) {
            return Err(journal.undo_after(e));
        }
        journal.clear_journal();
        Ok(manifest)
    }

    /// Roll back everything recorded so far; the original error wins unless
    /// the rollback itself fails.
    fn undo_after(&self, original: Error) -> Error {
        match rollback(&self.records, &self.root) {
            Ok(()) => {
                self.clear_journal();
                original
            }
            Err(rb) => rb,
        }
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
        // Provisional record first: if the write below is cut short, the
        // journal already says what to restore or delete.
        self.records.push(FileRecord {
            target: dst.to_path_buf(),
            backup,
            sha256_before,
            sha256_after: String::new(),
        });
        self.persist_journal()?;

        make_writable(dst);
        match op {
            FileOp::Copy { src, .. } => copy(src, dst)?,
            FileOp::WriteText { text, .. } => {
                if let Some(parent) = dst.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
                }
                std::fs::write(dst, text).map_err(|e| Error::io(dst, e))?;
            }
        }
        let after = sha256_file(dst)?;
        if let Some(last) = self.records.last_mut() {
            last.sha256_after = after;
        }
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
                root: self.root.clone(),
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
/// Run-time artefacts of our components (logs, captures) that did not exist
/// before the install go too; a failure there is logged, never fatal.
pub fn uninstall(paths: &AppPaths, manifest: &InstallManifest) -> Result<()> {
    let root = manifest.exe.parent().unwrap_or(Path::new(""));
    rollback(&manifest.files, root)?;
    for artefact in &manifest.side_effects {
        if !artefact.starts_with(root) {
            continue;
        }
        let result = if artefact.is_dir() {
            std::fs::remove_dir_all(artefact)
        } else if artefact.is_file() {
            make_writable(artefact);
            std::fs::remove_file(artefact)
        } else {
            Ok(())
        };
        if let Err(e) = result {
            tracing::warn!(path = %artefact.display(), %e, "run-time artefact not removed");
        }
    }
    manifest.delete(paths)?;
    let _ = std::fs::remove_dir_all(paths.backups_dir(&manifest.game_id));
    Ok(())
}

/// Finish a rollback left behind by a crash, if a journal exists for `game_id`.
pub fn recover(paths: &AppPaths, game_id: &str) -> Result<bool> {
    recover_file(&Journal::journal_path(paths, game_id))
}

/// Finish every interrupted install, whatever game it belonged to. Returns
/// the ids that were rolled back; errors are collected, not short-circuited.
pub fn recover_all(paths: &AppPaths) -> (Vec<String>, Vec<Error>) {
    let mut done = Vec::new();
    let mut errors = Vec::new();
    let Ok(read) = std::fs::read_dir(paths.installs_dir()) else {
        return (done, errors);
    };
    for entry in read.filter_map(|e| e.ok()) {
        let path = entry.path();
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
        let Some(game_id) = name
            .as_deref()
            .and_then(|n| n.strip_suffix(JOURNAL_SUFFIX))
            .map(str::to_string)
        else {
            continue;
        };
        match recover_file(&path) {
            Ok(true) => done.push(game_id),
            Ok(false) => {}
            Err(e) => errors.push(e),
        }
    }
    (done, errors)
}

fn recover_file(path: &Path) -> Result<bool> {
    let Some(journal): Option<JournalFile> = read_json(path, "install journal")? else {
        return Ok(false);
    };
    rollback(&journal.files, &journal.root)?;
    let _ = std::fs::remove_file(path);
    Ok(true)
}

/// Restore every backup and delete every file we added, in reverse order.
/// Keeps going after a failure so as much as possible is undone, then
/// reports what is left. `root` (the game's folder) is never removed.
pub fn rollback(records: &[FileRecord], root: &Path) -> Result<()> {
    let mut leftovers = Vec::new();
    for r in records.iter().rev() {
        make_writable(&r.target);
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
    remove_empty_dirs(records, root);
    if leftovers.is_empty() {
        Ok(())
    } else {
        Err(Error::RollbackFailed { leftovers })
    }
}

/// Folders we created for added files (e.g. `reshade-shaders\Shaders`) go
/// away once empty. The climb stops below `root`, which is never removed.
fn remove_empty_dirs(records: &[FileRecord], root: &Path) {
    let inside = |d: &Path| d != root && d.starts_with(root);
    let mut dirs: Vec<&Path> = records
        .iter()
        .filter(|r| r.backup.is_none())
        .filter_map(|r| r.target.parent())
        .filter(|d| inside(d))
        .collect();
    dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
    dirs.dedup();
    for dir in dirs {
        let mut current = Some(dir);
        while let Some(d) = current.filter(|d| inside(d)) {
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
    std::fs::copy(src, dst).map_err(|e| {
        Error::io(
            if e.kind() == std::io::ErrorKind::NotFound {
                src
            } else {
                dst
            },
            e,
        )
    })?;
    // `fs::copy` carries the read-only attribute over; our files must stay
    // replaceable and deletable.
    make_writable(dst);
    Ok(())
}

/// Clear a read-only attribute so the file can be overwritten or removed.
/// Best effort: a failure here surfaces as the real error one step later.
fn make_writable(path: &Path) {
    if let Ok(meta) = std::fs::metadata(path) {
        let mut perms = meta.permissions();
        if perms.readonly() {
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            let _ = std::fs::set_permissions(path, perms);
        }
    }
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
        // A pre-existing nvngx_dlssnr.dll / model stand in for files we overwrite.
        std::fs::write(game_dir.join("nvngx_dlssnr.dll"), b"old model").unwrap();
        let settings = settings_with_user_files(tmp.path());
        let (apis, ships) = match route {
            Route::ReShadeRenoDx => (vec![GraphicsApi::Dx12], true),
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

    fn set_readonly(path: &Path, on: bool) {
        let mut perms = std::fs::metadata(path).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(on);
        std::fs::set_permissions(path, perms).unwrap();
    }

    fn record(target: PathBuf, backup: Option<PathBuf>) -> FileRecord {
        FileRecord {
            target,
            backup,
            sha256_before: None,
            sha256_after: String::new(),
        }
    }

    #[test]
    fn install_then_uninstall_is_byte_identical() {
        for route in [Route::ReShadeRenoDx, Route::ReShadeFeeder] {
            let f = fixture(route);
            let before = snapshot(&f.game_dir);
            let mut steps = Vec::new();
            let manifest =
                Journal::run(&f.paths, "g1", &f.plan, false, &mut |s| steps.push(s)).unwrap();

            assert_eq!(steps.len(), f.plan.ops.len());
            assert_eq!(manifest.files.len(), f.plan.ops.len());
            assert!(manifest.files.iter().all(|r| !r.sha256_after.is_empty()));
            assert!(f.game_dir.join(MODEL_DLL).is_file());
            assert!(f.game_dir.join("shim.json").is_file());
            assert!(InstallManifest::load(&f.paths, "g1").unwrap().is_some());
            if route == Route::ReShadeRenoDx {
                let ini = manifest
                    .files
                    .iter()
                    .find(|r| r.target.ends_with("nvngx_dlssnr.dll"))
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
            assert!(
                f.game_dir.is_dir(),
                "the game folder itself is never removed"
            );
        }
    }

    #[test]
    fn failure_at_every_op_rolls_back_completely() {
        for route in [Route::ReShadeRenoDx, Route::ReShadeFeeder] {
            let total = fixture(route).plan.ops.len();
            for fail_at in 0..total {
                let f = fixture(route);
                let before = snapshot(&f.game_dir);
                let mut broken = f.plan.clone();
                let (dst, note) = (
                    broken.ops[fail_at].dst().to_path_buf(),
                    broken.ops[fail_at].note().to_string(),
                );
                broken.ops[fail_at] = FileOp::Copy {
                    src: PathBuf::from("Z:/does/not/exist"),
                    dst,
                    note,
                };
                let err = Journal::run(&f.paths, "g1", &broken, false, &mut |_| {}).unwrap_err();
                assert_eq!(err.code(), "io", "{route:?} op {fail_at}");
                assert_eq!(
                    snapshot(&f.game_dir),
                    before,
                    "{route:?} op {fail_at} left changes behind"
                );
                assert!(InstallManifest::load(&f.paths, "g1").unwrap().is_none());
                assert!(!Journal::journal_path(&f.paths, "g1").exists());
            }
        }
    }

    #[test]
    fn run_time_artefacts_of_our_components_go_on_uninstall_but_not_pre_existing_ones() {
        let f = fixture(Route::ReShadeRenoDx);
        // A log from an earlier, foreign ReShade run: not ours, must survive.
        let old_log = f.game_dir.join("ReShade.log");
        std::fs::write(&old_log, b"someone else's log").unwrap();
        // Re-plan so the pre-existing log is excluded from side effects.
        let plan = {
            let (store, components) = fake_store(&f._tmp.path().join("store2"));
            let settings = settings_with_user_files(f._tmp.path());
            let game = game(
                &f.game_dir,
                &[GraphicsApi::Dx12],
                Route::ReShadeRenoDx,
                true,
            );
            planner::plan(&Inputs {
                game: &game,
                store: &store,
                components: &components,
                settings: &settings,
            })
            .unwrap()
        };
        assert!(!plan.side_effects.iter().any(|p| p.ends_with("ReShade.log")));
        assert!(plan
            .side_effects
            .iter()
            .any(|p| p.ends_with("ReShadePreset.ini")));
        let before = snapshot(&f.game_dir);

        let manifest = Journal::run(&f.paths, "g1", &plan, false, &mut |_| {}).unwrap();
        // ReShade "runs" and leaves a preset and the feeder's cfg behind.
        std::fs::write(f.game_dir.join("ReShadePreset.ini"), b"written by reshade").unwrap();
        std::fs::write(f.game_dir.join("dlss5-feed.cfg"), b"written by feeder").unwrap();
        uninstall(&f.paths, &manifest).unwrap();
        assert_eq!(
            snapshot(&f.game_dir),
            before,
            "artefacts gone, foreign log kept"
        );
        assert_eq!(std::fs::read(&old_log).unwrap(), b"someone else's log");
    }

    #[test]
    fn read_only_game_files_are_replaced_and_restored() {
        let f = fixture(Route::ReShadeRenoDx);
        set_readonly(&f.game_dir.join("nvngx_dlssnr.dll"), true);
        let before = snapshot(&f.game_dir);
        let manifest = Journal::run(&f.paths, "g1", &f.plan, false, &mut |_| {}).unwrap();
        assert_ne!(
            std::fs::read(f.game_dir.join("nvngx_dlssnr.dll")).unwrap(),
            b"old model"
        );
        // A file someone marked read-only after our install must still go.
        set_readonly(&f.game_dir.join("dxgi.dll"), true);
        uninstall(&f.paths, &manifest).unwrap();
        assert_eq!(snapshot(&f.game_dir), before);
    }

    #[test]
    fn recover_finishes_an_interrupted_install_even_mid_write() {
        let f = fixture(Route::ReShadeRenoDx);
        let before = snapshot(&f.game_dir);
        let mut journal = Journal {
            paths: &f.paths,
            game_id: "g1".into(),
            root: f.game_dir.clone(),
            records: Vec::new(),
        };
        journal.apply(&f.plan.ops[0]).unwrap();
        journal.apply(&f.plan.ops[1]).unwrap();
        // Simulate a crash in the middle of the third write: the provisional
        // record is on disk, the target holds garbage, no sha256_after yet.
        let third = &f.plan.ops[2];
        journal
            .records
            .push(record(third.dst().to_path_buf(), None));
        journal.persist_journal().unwrap();
        std::fs::write(third.dst(), b"half written").unwrap();
        drop(journal);

        assert_ne!(snapshot(&f.game_dir), before);
        let (done, errors) = recover_all(&f.paths);
        assert_eq!(done, vec!["g1".to_string()]);
        assert!(errors.is_empty());
        assert_eq!(snapshot(&f.game_dir), before);
        assert!(!recover(&f.paths, "g1").unwrap());
        assert!(recover_all(&f.paths).0.is_empty());
    }

    #[test]
    fn rollback_reports_leftovers_but_keeps_going() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.txt");
        std::fs::write(&a, b"added").unwrap();
        let records = vec![
            record(a.clone(), None),
            record(
                tmp.path().join("b.txt"),
                Some(PathBuf::from("Z:/missing/backup")),
            ),
        ];
        let err = rollback(&records, tmp.path()).unwrap_err();
        assert!(!a.exists(), "the undoable step was still undone");
        match err {
            Error::RollbackFailed { leftovers } => {
                assert_eq!(leftovers.len(), 1);
                assert!(leftovers[0].0.ends_with("b.txt"));
            }
            other => panic!("{other:?}"),
        }
        assert!(tmp.path().is_dir(), "the root is never removed");
    }

    #[test]
    fn empty_dir_cleanup_never_climbs_above_the_root() {
        let tmp = tempfile::tempdir().unwrap();
        let game = tmp.path().join("only").join("game");
        let deep = game.join("a").join("b").join("c.txt");
        std::fs::create_dir_all(deep.parent().unwrap()).unwrap();
        std::fs::write(&deep, b"x").unwrap();
        rollback(&[record(deep.clone(), None)], &game).unwrap();
        assert!(!game.join("a").exists(), "our empty folders go");
        assert!(game.is_dir(), "the root stays even when empty");

        // A target outside the root is deleted but its folders are left alone.
        let outside = tmp.path().join("elsewhere").join("x.txt");
        std::fs::create_dir_all(outside.parent().unwrap()).unwrap();
        std::fs::write(&outside, b"x").unwrap();
        rollback(&[record(outside.clone(), None)], &game).unwrap();
        assert!(!outside.exists());
        assert!(outside.parent().unwrap().is_dir());
    }

    #[test]
    fn unwritable_folder_is_reported_before_any_change() {
        let f = fixture(Route::ReShadeRenoDx);
        let mut plan = f.plan.clone();
        plan.exe = PathBuf::from("Z:/nope/G.exe");
        let err = Journal::run(&f.paths, "g1", &plan, false, &mut |_| {}).unwrap_err();
        assert_eq!(err.code(), "game_folder_not_writable");
    }
}
